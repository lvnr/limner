//! File access: reading a project from a folder or a packed file, and writing
//! files so that a crash never leaves a half-written one under its final name.

use crate::error::FormatError;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

pub(crate) const MANIFEST_FILE: &str = "limn.json";
pub(crate) const STATE_FILE: &str = "state.json";
pub(crate) const THUMBNAIL_FILE: &str = "thumbnail.png";
pub(crate) const DRAWINGS_DIR: &str = "drawings";
pub(crate) const MEDIA_DIR: &str = "media";
pub(crate) const BLOB_EXTENSION: &str = "lstk";
pub(crate) const TMP_SUFFIX: &str = ".tmp";

/// `drawings/ab/abcdef….lstk` for a blob hash in hexadecimal.
pub(crate) fn blob_path(hex: &str) -> String {
    format!("{DRAWINGS_DIR}/{}/{hex}.{BLOB_EXTENSION}", &hex[..2])
}

pub(crate) fn media_path(file: &str) -> String {
    format!("{MEDIA_DIR}/{file}")
}

/// Joins a `/`-separated project-relative path onto a folder.
pub(crate) fn join(dir: &Path, relative: &str) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.extend(relative.split('/'));
    path
}

fn starts_with_zip_signature(path: &Path) -> bool {
    let mut signature = [0u8; 4];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut signature))
        .is_ok()
        && &signature == b"PK\x03\x04"
}

/// A project that can be read: a folder, or a packed file read in place.
pub(crate) enum Source {
    Dir(PathBuf),
    Zip {
        path: PathBuf,
        archive: Box<ZipArchive<BufReader<File>>>,
    },
}

impl Source {
    /// Opens the project at `path`: a project folder, the `limn.json` inside one,
    /// or a packed project file.
    pub fn open(path: &Path) -> Result<Source, FormatError> {
        let metadata = fs::metadata(path).map_err(|e| FormatError::io(path, e))?;
        if metadata.is_dir() {
            return if path.join(MANIFEST_FILE).is_file() {
                Ok(Source::Dir(path.to_path_buf()))
            } else {
                Err(FormatError::NotAProject(path.to_path_buf()))
            };
        }
        if path.file_name().is_some_and(|n| n == MANIFEST_FILE) {
            let dir = path.parent().unwrap_or(Path::new(""));
            return Ok(Source::Dir(dir.to_path_buf()));
        }
        if !starts_with_zip_signature(path) {
            return Err(FormatError::NotAProject(path.to_path_buf()));
        }
        let file = File::open(path).map_err(|e| FormatError::io(path, e))?;
        let archive =
            ZipArchive::new(BufReader::new(file)).map_err(|e| FormatError::zip(path, e))?;
        if archive.index_for_name(MANIFEST_FILE).is_none() {
            return Err(FormatError::NotAProject(path.to_path_buf()));
        }
        Ok(Source::Zip {
            path: path.to_path_buf(),
            archive: Box::new(archive),
        })
    }

    /// Whether `path` could be opened as a project, without reading its content.
    pub fn recognises(path: &Path) -> bool {
        if path.is_dir() {
            return path.join(MANIFEST_FILE).is_file();
        }
        if !path.is_file() {
            return false;
        }
        if path.file_name().is_some_and(|n| n == MANIFEST_FILE) {
            return true;
        }
        starts_with_zip_signature(path)
            && File::open(path)
                .ok()
                .and_then(|file| ZipArchive::new(BufReader::new(file)).ok())
                .is_some_and(|archive| archive.index_for_name(MANIFEST_FILE).is_some())
    }

    /// The folder of an unpacked project.
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Source::Dir(dir) => Some(dir),
            Source::Zip { .. } => None,
        }
    }

    /// Where `relative` lives, for messages.
    pub fn locate(&self, relative: &str) -> PathBuf {
        match self {
            Source::Dir(dir) => join(dir, relative),
            Source::Zip { path, .. } => join(path, relative),
        }
    }

    /// Reads a whole file, or `None` when the project does not have it.
    pub fn read(&mut self, relative: &str) -> Result<Option<Vec<u8>>, FormatError> {
        let location = self.locate(relative);
        match self {
            Source::Dir(_) => match fs::read(&location) {
                Ok(bytes) => Ok(Some(bytes)),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
                Err(e) => Err(FormatError::io(&location, e)),
            },
            Source::Zip { archive, .. } => {
                let mut entry = match archive.by_name(relative) {
                    Ok(entry) => entry,
                    Err(zip::result::ZipError::FileNotFound) => return Ok(None),
                    Err(e) => return Err(FormatError::zip(&location, e)),
                };
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(usize::try_from(entry.size()).unwrap_or(usize::MAX))
                    .map_err(|_| FormatError::corrupt(&location, "it claims an impossible size"))?;
                entry.read_to_end(&mut bytes).map_err(|e| {
                    FormatError::corrupt(
                        &location,
                        format!("it cannot be read from the archive ({e})"),
                    )
                })?;
                Ok(Some(bytes))
            }
        }
    }

    /// Reads a file the manifest refers to; its absence is an error.
    pub fn read_required(&mut self, relative: &str) -> Result<Vec<u8>, FormatError> {
        self.read(relative)?
            .ok_or_else(|| FormatError::MissingFile {
                path: self.locate(relative),
            })
    }
}

pub(crate) fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(TMP_SUFFIX);
    path.with_file_name(name)
}

/// Makes a completed rename in `dir` survive a power cut. A no-op where folders
/// cannot be synchronised (Windows).
pub(crate) fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    {
        if let Ok(handle) = File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    #[cfg(not(unix))]
    {
        let _ = dir;
    }
}

/// Writes `bytes` to `path` by way of `path.tmp`: the final name only ever holds a
/// complete file.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8], sync: bool) -> Result<(), FormatError> {
    let tmp = tmp_path(path);
    let write = || -> io::Result<()> {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        if sync {
            file.sync_all()?;
        }
        Ok(())
    };
    write().map_err(|e| FormatError::io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| FormatError::io(path, e))
}

pub(crate) fn create_dir_all(dir: &Path) -> Result<(), FormatError> {
    fs::create_dir_all(dir).map_err(|e| FormatError::io(dir, e))
}
