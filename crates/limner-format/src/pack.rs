//! Packed projects: a `.limnz` file is a zip archive of a project folder.

use crate::error::FormatError;
use crate::hash::ContentHash;
use crate::load::media_hash;
use crate::manifest;
use crate::save::{finish_archive, stored_entry};
use crate::store::{
    self, blob_path, join, media_path, tmp_path, write_atomic, MANIFEST_FILE, STATE_FILE,
    THUMBNAIL_FILE,
};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;
use zip::{ZipArchive, ZipWriter};

/// Packs the project folder `dir` into the single file `file`.
///
/// The archive holds `limn.json`, the optional `state.json` and `thumbnail.png`,
/// and exactly the drawing and media files the manifest refers to, as stored
/// (uncompressed) entries in a fixed order with a fixed timestamp: packing the same
/// project twice gives the same bytes. Leftovers in the folder are not packed.
pub fn pack(dir: &Path, file: &Path) -> Result<(), FormatError> {
    let manifest_path = dir.join(MANIFEST_FILE);
    let manifest_bytes = match fs::read(&manifest_path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(FormatError::NotAProject(dir.to_path_buf()))
        }
        Err(e) => return Err(FormatError::io(&manifest_path, e)),
    };
    let manifest = manifest::parse(&manifest_bytes, &manifest_path)?;

    let mut entries = vec![MANIFEST_FILE.to_owned()];
    for optional in [STATE_FILE, THUMBNAIL_FILE] {
        if dir.join(optional).is_file() {
            entries.push(optional.to_owned());
        }
    }
    // The same checks as loading: a name that is not a hash (and an extension)
    // could point anywhere, `..\..\x` and `C:\x` on Windows included.
    let not_ours = |name: &str| {
        FormatError::manifest(
            &manifest_path,
            format!("it refers to \"{name}\", which is not a file of the project"),
        )
    };
    let mut content: BTreeSet<String> = BTreeSet::new();
    for drawing in &manifest.drawings {
        if ContentHash::from_hex(&drawing.blob).is_none() {
            return Err(not_ours(&drawing.blob));
        }
        content.insert(blob_path(&drawing.blob));
    }
    for media in &manifest.media {
        if media_hash(&media.file).is_none() {
            return Err(not_ours(&media.file));
        }
        content.insert(media_path(&media.file));
    }
    entries.extend(content);

    if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
        store::create_dir_all(parent)?;
    }
    let tmp = tmp_path(file);
    let handle = File::create(&tmp).map_err(|e| FormatError::io(&tmp, e))?;
    let mut writer = ZipWriter::new(BufWriter::new(handle));
    let result = (|| {
        for relative in &entries {
            let path = join(dir, relative);
            let bytes = if relative == MANIFEST_FILE {
                manifest_bytes.clone()
            } else {
                match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {
                        return Err(FormatError::MissingFile { path })
                    }
                    Err(e) => return Err(FormatError::io(&path, e)),
                }
            };
            writer
                .start_file(relative.as_str(), stored_entry(bytes.len()))
                .map_err(|e| FormatError::zip(&tmp, e))?;
            writer
                .write_all(&bytes)
                .map_err(|e| FormatError::io(&tmp, e))?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => finish_archive(writer, &tmp, file, true),
        Err(e) => {
            drop(writer);
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Unpacks the packed project `file` into the folder `dir`, which must not exist
/// yet or be empty. `limn.json` is written last, so a folder left behind by an
/// interrupted unpack is not mistaken for a project.
pub fn unpack(file: &Path, dir: &Path) -> Result<(), FormatError> {
    let handle = File::open(file).map_err(|e| FormatError::io(file, e))?;
    let mut archive =
        ZipArchive::new(BufReader::new(handle)).map_err(|e| FormatError::zip(file, e))?;
    if archive.index_for_name(MANIFEST_FILE).is_none() {
        return Err(FormatError::NotAProject(file.to_path_buf()));
    }
    if fs::read_dir(dir).is_ok_and(|mut entries| entries.next().is_some()) {
        return Err(FormatError::AlreadyExists(dir.to_path_buf()));
    }
    store::create_dir_all(dir)?;

    let mut manifest = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| FormatError::zip(file, e))?;
        if entry.is_dir() {
            continue;
        }
        let name = entry
            .name()
            .map_err(|e| FormatError::zip(file, e))?
            .into_owned();
        let relative = entry.enclosed_name().ok_or_else(|| {
            FormatError::corrupt(
                file,
                format!("it holds an entry with the unsafe name \"{name}\""),
            )
        })?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| {
            FormatError::corrupt(file, format!("its entry \"{name}\" cannot be read ({e})"))
        })?;
        if name == MANIFEST_FILE {
            manifest = bytes;
            continue;
        }
        let target = dir.join(relative);
        if let Some(parent) = target.parent() {
            store::create_dir_all(parent)?;
        }
        write_atomic(&target, &bytes, true)?;
    }
    write_atomic(&dir.join(MANIFEST_FILE), &manifest, true)?;
    store::sync_dir(dir);
    Ok(())
}
