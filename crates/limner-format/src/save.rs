//! Saving: content-addressed files first, the manifest last.

use crate::blob;
use crate::error::FormatError;
use crate::hash::ContentHash;
use crate::manifest::{self, MediaFile};
use crate::media;
use crate::schema::ManifestDto;
use crate::store::{
    self, blob_path, join, media_path, tmp_path, write_atomic, BLOB_EXTENSION, DRAWINGS_DIR,
    MANIFEST_FILE, MEDIA_DIR, STATE_FILE, THUMBNAIL_FILE, TMP_SUFFIX,
};
use limner_core::model::{Document, Drawing};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipWriter};

#[derive(Clone, Debug)]
pub struct SaveOptions {
    /// After the save, delete drawing and media files the project no longer refers
    /// to, and temporary files left by an interrupted save.
    pub collect_garbage: bool,
    /// zstd level for new stroke blobs and meshes. Has no effect on content hashes.
    pub compression_level: i32,
    /// Flush every file and folder to the disk before relying on it. Turning this
    /// off makes saving faster and keeps it safe against a crash of the program, but
    /// not against a power cut.
    pub sync: bool,
}

impl Default for SaveOptions {
    fn default() -> Self {
        Self {
            collect_garbage: true,
            compression_level: 3,
            sync: true,
        }
    }
}

/// What a save did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SaveReport {
    /// Stroke blobs written because no file with their content existed.
    pub blobs_written: usize,
    /// Stroke blobs the document needs that were already on disk.
    pub blobs_reused: usize,
    pub media_written: usize,
    pub media_reused: usize,
    /// False when `limn.json` already had exactly the content this save would write.
    pub manifest_written: bool,
    /// Unreferenced and temporary files deleted by garbage collection.
    pub files_removed: usize,
    /// Bytes of all files written, the manifest included.
    pub bytes_written: u64,
}

impl SaveReport {
    /// True when the save found everything already on disk and changed nothing.
    pub fn wrote_nothing(&self) -> bool {
        self.blobs_written == 0 && self.media_written == 0 && !self.manifest_written
    }
}

impl fmt::Display for SaveReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "drawings: {} written, {} reused; media: {} written, {} reused; manifest {}; \
             {} stale files removed; {} bytes written",
            self.blobs_written,
            self.blobs_reused,
            self.media_written,
            self.media_reused,
            if self.manifest_written {
                "written"
            } else {
                "unchanged"
            },
            self.files_removed,
            self.bytes_written
        )
    }
}

/// Where content files go: a project folder or a packed file being written.
trait Sink {
    /// Whether the file is already there and need not be produced.
    fn contains(&mut self, relative: &str) -> bool;
    fn put(&mut self, relative: &str, bytes: &[u8]) -> Result<(), FormatError>;
}

/// Refuses documents that could be written but not read back.
fn check(doc: &Document) -> Result<(), FormatError> {
    doc.validate()
        .map_err(|e| FormatError::Unsaveable(e.to_string()))
}

/// Writes every drawing and media file the document needs that `sink` lacks, and
/// returns the manifest that refers to them.
fn write_content(
    doc: &Document,
    options: &SaveOptions,
    sink: &mut dyn Sink,
    report: &mut SaveReport,
) -> Result<ManifestDto, FormatError> {
    // Drawings shared between frames or layers are one `Arc`; identical drawings
    // that are separate allocations meet again at their hash.
    let mut by_pointer: HashMap<*const Drawing, ContentHash> = HashMap::new();
    let mut handled: BTreeSet<String> = BTreeSet::new();
    let mut drawings = BTreeMap::new();
    for (id, drawing) in doc.drawings() {
        let hash = match by_pointer.get(&Arc::as_ptr(drawing)) {
            Some(hash) => *hash,
            None => {
                let canonical = blob::encode(drawing).map_err(FormatError::Unsaveable)?;
                let relative = blob_path(&canonical.hash.to_hex());
                if handled.insert(relative.clone()) {
                    if sink.contains(&relative) {
                        report.blobs_reused += 1;
                    } else {
                        let bytes = canonical.file_bytes(options.compression_level);
                        sink.put(&relative, &bytes)?;
                        report.blobs_written += 1;
                        report.bytes_written += bytes.len() as u64;
                    }
                }
                by_pointer.insert(Arc::as_ptr(drawing), canonical.hash);
                canonical.hash
            }
        };
        drawings.insert(id, hash);
    }

    let mut media_files = BTreeMap::new();
    for item in doc.media_items() {
        let prepared = media::prepare(&item.data).map_err(FormatError::Unsaveable)?;
        let file = prepared.file_name();
        let relative = media_path(&file);
        if handled.insert(relative.clone()) {
            if sink.contains(&relative) {
                report.media_reused += 1;
            } else {
                let bytes = prepared
                    .file_bytes(options.compression_level)
                    .map_err(|e| {
                        FormatError::Unsaveable(format!("media \"{}\": {e}", item.name))
                    })?;
                sink.put(&relative, &bytes)?;
                report.media_written += 1;
                report.bytes_written += bytes.len() as u64;
            }
        }
        media_files.insert(
            item.id,
            MediaFile {
                storage: prepared.storage,
                file,
            },
        );
    }
    manifest::from_document(doc, &drawings, &media_files)
}

struct DirSink<'a> {
    dir: &'a Path,
    sync: bool,
    /// Folders that received a file and must be flushed before the manifest is replaced.
    touched: BTreeSet<PathBuf>,
}

impl Sink for DirSink<'_> {
    fn contains(&mut self, relative: &str) -> bool {
        join(self.dir, relative).is_file()
    }

    fn put(&mut self, relative: &str, bytes: &[u8]) -> Result<(), FormatError> {
        let path = join(self.dir, relative);
        if let Some(parent) = path.parent() {
            if !parent.is_dir() {
                store::create_dir_all(parent)?;
                // The new folder's own entry lives in its parent.
                if let Some(grandparent) = parent.parent() {
                    self.touched.insert(grandparent.to_path_buf());
                }
            }
            self.touched.insert(parent.to_path_buf());
        }
        write_atomic(&path, bytes, self.sync)
    }
}

/// Saves `doc` as the project folder `dir`, creating it if needed.
///
/// The order of work is what makes an interrupted save harmless: new drawing and
/// media files are written first, under names nothing refers to yet; then
/// `limn.json` is replaced in one atomic rename; only then are files the new
/// manifest no longer needs deleted. At every instant the folder holds a complete
/// project: the previous one, or the new one.
///
/// Files that already exist are not rewritten, and `limn.json` is left untouched
/// when its content would not change, so saving an unchanged document writes nothing.
pub fn save_project(
    doc: &Document,
    dir: &Path,
    options: &SaveOptions,
) -> Result<SaveReport, FormatError> {
    check(doc)?;
    store::create_dir_all(dir)?;
    let mut report = SaveReport::default();
    let mut sink = DirSink {
        dir,
        sync: options.sync,
        touched: BTreeSet::new(),
    };
    let manifest = write_content(doc, options, &mut sink, &mut report)?;
    if options.sync {
        for folder in &sink.touched {
            store::sync_dir(folder);
        }
    }

    let bytes = manifest::to_bytes(&manifest)?;
    let manifest_path = dir.join(MANIFEST_FILE);
    if fs::read(&manifest_path).is_ok_and(|existing| existing == bytes) {
        report.manifest_written = false;
    } else {
        write_atomic(&manifest_path, &bytes, options.sync)?;
        if options.sync {
            store::sync_dir(dir);
        }
        report.manifest_written = true;
        report.bytes_written += bytes.len() as u64;
    }

    if options.collect_garbage {
        report.files_removed = collect_garbage(dir, &manifest);
    }
    Ok(report)
}

/// `<64 hexadecimal digits>.<anything>`: the shape of every content file's name.
fn is_hash_name(name: &str) -> bool {
    name.split_once('.')
        .is_some_and(|(stem, _)| ContentHash::from_hex(stem).is_some())
}

/// Deletes what the manifest does not refer to: stroke blobs, media files named by a
/// hash, and `*.tmp` files in the places a save writes to. Anything else in the
/// folder is left alone. Failures are ignored: the project is already saved.
fn collect_garbage(dir: &Path, manifest: &ManifestDto) -> usize {
    let blobs: BTreeSet<String> = manifest
        .drawings
        .iter()
        .map(|d| format!("{}.{BLOB_EXTENSION}", d.blob))
        .collect();
    let media: BTreeSet<&str> = manifest.media.iter().map(|m| m.file.as_str()).collect();
    let mut removed = 0;
    let mut remove = |path: &Path| {
        if fs::remove_file(path).is_ok() {
            removed += 1;
        }
    };
    let names = |folder: &Path| -> Vec<(PathBuf, String, bool)> {
        fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name().into_string().ok()?;
                let is_dir = entry.file_type().ok()?.is_dir();
                Some((entry.path(), name, is_dir))
            })
            .collect()
    };

    for file in [MANIFEST_FILE, STATE_FILE, THUMBNAIL_FILE] {
        let tmp = tmp_path(&dir.join(file));
        if tmp.is_file() {
            remove(&tmp);
        }
    }
    for (shard, shard_name, is_dir) in names(&dir.join(DRAWINGS_DIR)) {
        if !is_dir {
            if shard_name.ends_with(TMP_SUFFIX) {
                remove(&shard);
            }
            continue;
        }
        for (path, name, is_dir) in names(&shard) {
            // Anything the manifest names is kept, whatever it looks like.
            let stale = !blobs.contains(&name)
                && (name.ends_with(TMP_SUFFIX)
                    || (name.ends_with(&format!(".{BLOB_EXTENSION}")) && is_hash_name(&name)));
            if !is_dir && stale {
                remove(&path);
            }
        }
        // Succeeds only when the shard is now empty.
        let _ = fs::remove_dir(&shard);
    }
    for (path, name, is_dir) in names(&dir.join(MEDIA_DIR)) {
        let stale =
            !media.contains(name.as_str()) && (name.ends_with(TMP_SUFFIX) || is_hash_name(&name));
        if !is_dir && stale {
            remove(&path);
        }
    }
    removed
}

struct ZipSink<'a> {
    path: &'a Path,
    writer: ZipWriter<BufWriter<File>>,
}

/// Entries are stored, not compressed, and carry a fixed timestamp, so the same
/// content always packs to the same bytes.
pub(crate) fn stored_entry(len: usize) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .last_modified_time(DateTime::default())
        .unix_permissions(0o644)
        .large_file(len as u64 >= u32::MAX as u64)
}

impl Sink for ZipSink<'_> {
    fn contains(&mut self, _relative: &str) -> bool {
        false
    }

    fn put(&mut self, relative: &str, bytes: &[u8]) -> Result<(), FormatError> {
        self.writer
            .start_file(relative, stored_entry(bytes.len()))
            .map_err(|e| FormatError::zip(self.path, e))?;
        self.writer
            .write_all(bytes)
            .map_err(|e| FormatError::io(self.path, e))
    }
}

/// Finishes a packed file written at `tmp` and moves it over `file`.
pub(crate) fn finish_archive(
    writer: ZipWriter<BufWriter<File>>,
    tmp: &Path,
    file: &Path,
    sync: bool,
) -> Result<(), FormatError> {
    let inner = writer.finish().map_err(|e| FormatError::zip(tmp, e))?;
    let handle = inner
        .into_inner()
        .map_err(|e| FormatError::io(tmp, e.into_error()))?;
    if sync {
        handle.sync_all().map_err(|e| FormatError::io(tmp, e))?;
    }
    drop(handle);
    fs::rename(tmp, file).map_err(|e| FormatError::io(file, e))?;
    if sync {
        if let Some(parent) = file.parent() {
            store::sync_dir(parent);
        }
    }
    Ok(())
}

/// Saves `doc` straight into a packed project file (`.limnz`), replacing it atomically.
///
/// Editor state and the thumbnail of a packed project already at `file` are carried
/// over. A packed file has no unchanged parts to reuse, so everything is written.
pub fn save_packed(
    doc: &Document,
    file: &Path,
    options: &SaveOptions,
) -> Result<SaveReport, FormatError> {
    let mut state = None;
    let mut thumbnail = None;
    if file.is_file() {
        if let Ok(mut existing) = store::Source::open(file) {
            state = existing.read(STATE_FILE).ok().flatten();
            thumbnail = existing.read(THUMBNAIL_FILE).ok().flatten();
        }
    }
    write_packed(doc, file, options, state.as_deref(), thumbnail.as_deref())
}

/// Like [`save_packed`], but with the editor state and thumbnail given by the
/// caller instead of carried over from the file being replaced.
pub fn save_packed_with(
    doc: &Document,
    file: &Path,
    options: &SaveOptions,
    state: Option<&serde_json::Value>,
    thumbnail: Option<&[u8]>,
) -> Result<SaveReport, FormatError> {
    let state = state.map(crate::state::state_bytes).transpose()?;
    write_packed(doc, file, options, state.as_deref(), thumbnail)
}

fn write_packed(
    doc: &Document,
    file: &Path,
    options: &SaveOptions,
    state: Option<&[u8]>,
    thumbnail: Option<&[u8]>,
) -> Result<SaveReport, FormatError> {
    check(doc)?;
    if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
        store::create_dir_all(parent)?;
    }
    let tmp = tmp_path(file);
    let handle = File::create(&tmp).map_err(|e| FormatError::io(&tmp, e))?;
    let mut sink = ZipSink {
        path: &tmp,
        writer: ZipWriter::new(BufWriter::new(handle)),
    };
    let mut report = SaveReport::default();
    let result = (|| {
        let manifest = write_content(doc, options, &mut sink, &mut report)?;
        let bytes = manifest::to_bytes(&manifest)?;
        sink.put(MANIFEST_FILE, &bytes)?;
        report.manifest_written = true;
        report.bytes_written += bytes.len() as u64;
        for (name, bytes) in [(STATE_FILE, state), (THUMBNAIL_FILE, thumbnail)] {
            if let Some(bytes) = bytes {
                sink.put(name, bytes)?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => finish_archive(sink.writer, &tmp, file, options.sync)?,
        Err(e) => {
            drop(sink);
            let _ = fs::remove_file(&tmp);
            return Err(e);
        }
    }
    Ok(report)
}
