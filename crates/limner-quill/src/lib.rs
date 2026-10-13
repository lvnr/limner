//! One-way importer for Quill project folders (`Quill.json`, `Quill.qbin`, `State.json`).
//!
//! The format knowledge here comes from user-owned project files and from openly
//! licensed readers and the IMM player source; nothing is derived from Quill's binaries.

mod convert;
mod error;
mod json;
mod qbin;
mod state;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use error::ImportError;
pub use state::QuillState;

use limner_core::model::{Document, PALETTE_SIZE};
use std::fs;
use std::path::{Path, PathBuf};

/// The result of importing a Quill project.
#[derive(Debug)]
pub struct Imported {
    pub document: Document,
    pub state: QuillState,
    /// Things that were repaired, approximated or left out. Empty for a clean import.
    pub warnings: Vec<String>,
}

/// Resolves what the user pointed at (the folder, or one of the files inside it) to the project folder.
pub fn project_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return path
            .join("Quill.json")
            .is_file()
            .then(|| path.to_path_buf());
    }
    let name = path.file_name()?.to_str()?;
    let known = ["quill.json", "quill.qbin", "state.json"];
    if known.contains(&name.to_ascii_lowercase().as_str()) {
        let dir = path.parent()?;
        return dir.join("Quill.json").is_file().then(|| dir.to_path_buf());
    }
    None
}

pub fn is_quill_project(path: &Path) -> bool {
    project_dir(path).is_some()
}

/// Imports the Quill project at `path` (the project folder or a file inside it).
pub fn import_project(path: &Path) -> Result<Imported, ImportError> {
    let dir = project_dir(path).ok_or_else(|| {
        ImportError::NotAProject(format!("no Quill.json found at {}", path.display()))
    })?;

    let json_path = dir.join("Quill.json");
    let text = fs::read(&json_path).map_err(|e| ImportError::io(&json_path, e))?;
    let root: serde_json::Value =
        serde_json::from_slice(&text).map_err(|source| ImportError::Json {
            path: json_path.clone(),
            source,
        })?;

    let mut warnings = Vec::new();
    let qbin_path = dir.join("Quill.qbin");
    let mapped = match fs::File::open(&qbin_path) {
        Ok(file) => {
            // SAFETY: the file is opened read-only and only read while mapped. If another
            // process truncates it during the import the read could fault; that is the
            // same failure a concurrent writer would cause for any reader.
            Some(unsafe { memmap2::Mmap::map(&file) }.map_err(|e| ImportError::io(&qbin_path, e))?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            warnings.push("Quill.qbin is missing; all drawings and media are empty".to_owned());
            None
        }
        Err(e) => return Err(ImportError::io(&qbin_path, e)),
    };
    let qbin: &[u8] = mapped.as_deref().unwrap_or(&[]);

    let converted = convert::convert(&root, qbin)?;
    let mut document = converted.document;
    warnings.extend(converted.warnings);

    let state_path = dir.join("State.json");
    let state = match fs::read(&state_path) {
        Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => state::parse(&value),
            Err(_) => {
                warnings
                    .push("State.json could not be read; editor state was not imported".to_owned());
                QuillState::default()
            }
        },
        Err(_) => QuillState::default(),
    };
    if state.palette.len() == PALETTE_SIZE {
        document.settings.palette = state.palette.clone();
    }
    if document.settings.title.is_empty() {
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            document.settings.title = name.to_owned();
        }
    }
    Ok(Imported {
        document,
        state,
        warnings,
    })
}
