//! Opening documents from disk, whatever format they are in.

use limner_core::model::Document;
use limner_quill::QuillState;
use std::path::Path;

/// Where a document came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A Quill project folder. Imported, never written back.
    Quill,
}

impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Quill => "Quill project",
        }
    }
}

#[derive(Debug)]
pub struct Opened {
    pub document: Document,
    pub kind: SourceKind,
    /// Editor state carried by a Quill project, when that is the source.
    pub quill_state: Option<QuillState>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{0} does not exist")]
    NotFound(String),
    #[error("{0} is not a document Limner can open")]
    Unrecognised(String),
    #[error(transparent)]
    Quill(#[from] limner_quill::ImportError),
}

/// Opens the document at `path`, detecting its format.
pub fn open(path: &Path) -> Result<Opened, OpenError> {
    if !path.exists() {
        return Err(OpenError::NotFound(path.display().to_string()));
    }
    if limner_quill::is_quill_project(path) {
        let imported = limner_quill::import_project(path)?;
        return Ok(Opened {
            document: imported.document,
            kind: SourceKind::Quill,
            quill_state: Some(imported.state),
            warnings: imported.warnings,
        });
    }
    Err(OpenError::Unrecognised(path.display().to_string()))
}
