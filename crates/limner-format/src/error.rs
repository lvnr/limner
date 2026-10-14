//! Errors of the native format.

use limner_core::model::DocumentError;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} is not a Limner project (no limn.json found)", .0.display())]
    NotAProject(PathBuf),
    #[error("{}: not a readable Limner manifest: {source}", path.display())]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "{}: written in format version {found}, but this build of Limner reads up to version \
         {supported}; a newer Limner is needed to open it",
        path.display()
    )]
    UnsupportedVersion {
        path: PathBuf,
        found: u64,
        supported: u32,
    },
    /// A file exists but its bytes are not what the format requires: truncated,
    /// altered, or not the content its name promises.
    #[error("{} is damaged: {reason}", path.display())]
    Corrupt { path: PathBuf, reason: String },
    #[error("{} is missing, but limn.json refers to it", path.display())]
    MissingFile { path: PathBuf },
    /// The manifest is well-formed JSON of a supported version but describes
    /// something that cannot be a document.
    #[error("{}: {reason}", path.display())]
    Manifest { path: PathBuf, reason: String },
    #[error("{}: the project does not describe a consistent document: {source}", path.display())]
    Invalid {
        path: PathBuf,
        #[source]
        source: DocumentError,
    },
    /// The document in memory cannot be written; nothing on disk was changed.
    #[error("the document cannot be saved: {0}")]
    Unsaveable(String),
    #[error("{}: {source}", path.display())]
    Zip {
        path: PathBuf,
        #[source]
        source: zip::result::ZipError,
    },
    #[error("{} already exists and is not empty", .0.display())]
    AlreadyExists(PathBuf),
}

impl FormatError {
    pub(crate) fn io(path: &Path, source: io::Error) -> Self {
        FormatError::Io {
            path: path.to_path_buf(),
            source,
        }
    }

    pub(crate) fn corrupt(path: &Path, reason: impl Into<String>) -> Self {
        FormatError::Corrupt {
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }

    pub(crate) fn manifest(path: &Path, reason: impl Into<String>) -> Self {
        FormatError::Manifest {
            path: path.to_path_buf(),
            reason: reason.into(),
        }
    }

    pub(crate) fn zip(path: &Path, source: zip::result::ZipError) -> Self {
        match source {
            zip::result::ZipError::Io(source) => FormatError::io(path, source),
            source => FormatError::Zip {
                path: path.to_path_buf(),
                source,
            },
        }
    }
}
