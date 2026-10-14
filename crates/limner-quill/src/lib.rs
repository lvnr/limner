//! One-way importer for Quill project folders (`Quill.json`, `Quill.qbin`, `State.json`).
//!
//! The format knowledge here comes from user-owned project files and from openly
//! licensed readers and the IMM player source; nothing is derived from Quill's binaries.
//!
//! `docs/quill-import.md` says what is imported per layer kind and Quill version, lists
//! every warning, and separates what was checked on real files from what rests on code.

mod convert;
mod error;
mod json;
mod mesh;
mod qbin;
mod sound;
mod state;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use error::ImportError;
pub use state::{QuillState, Taper, ToolSettings};

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

/// Finds `name` in `dir`, ignoring letter case: projects that passed through other
/// tools or file systems do not always keep Quill's capitalisation.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
}

/// Resolves what the user pointed at (the folder, or one of the files inside it) to the project folder.
pub fn project_dir(path: &Path) -> Option<PathBuf> {
    if path.is_dir() {
        return find_file(path, "Quill.json").map(|_| path.to_path_buf());
    }
    let name = path.file_name()?.to_str()?;
    let known = ["quill.json", "quill.qbin", "state.json"];
    if known.contains(&name.to_ascii_lowercase().as_str()) {
        let dir = match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent,
            _ => Path::new("."),
        };
        return find_file(dir, "Quill.json").map(|_| dir.to_path_buf());
    }
    None
}

pub fn is_quill_project(path: &Path) -> bool {
    project_dir(path).is_some()
}

/// Parses JSON that may start with a UTF-8 byte-order mark, which some editors add.
fn parse_json(bytes: &[u8]) -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes))
}

/// Imports the Quill project at `path` (the project folder or a file inside it).
pub fn import_project(path: &Path) -> Result<Imported, ImportError> {
    let not_found =
        || ImportError::NotAProject(format!("no Quill.json found at {}", path.display()));
    let dir = project_dir(path).ok_or_else(not_found)?;
    let json_path = find_file(&dir, "Quill.json").ok_or_else(not_found)?;
    let text = fs::read(&json_path).map_err(|e| ImportError::io(&json_path, e))?;
    let root = parse_json(&text).map_err(|source| ImportError::Json {
        path: json_path.clone(),
        source,
    })?;

    let mut warnings = Vec::new();
    let qbin_path = find_file(&dir, "Quill.qbin").unwrap_or_else(|| dir.join("Quill.qbin"));
    let mapped = match fs::File::open(&qbin_path) {
        Ok(file) => {
            let len = file
                .metadata()
                .map_err(|e| ImportError::io(&qbin_path, e))?
                .len();
            if len == 0 {
                // An empty file cannot be mapped on every platform.
                Some(None)
            } else {
                // SAFETY: the file is opened read-only and only read while mapped. If another
                // process truncates it during the import the read could fault; that is the
                // same failure a concurrent writer would cause for any reader.
                let map = unsafe { memmap2::Mmap::map(&file) }
                    .map_err(|e| ImportError::io(&qbin_path, e))?;
                Some(Some(map))
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            warnings.push("Quill.qbin is missing; all drawings and media are empty".to_owned());
            None
        }
        Err(e) => return Err(ImportError::io(&qbin_path, e)),
    };
    let qbin: Option<&[u8]> = mapped.as_ref().map(|map| map.as_deref().unwrap_or(&[]));

    let converted = convert::convert(&root, qbin)?;
    let mut document = converted.document;
    warnings.extend(converted.warnings);

    let state = match find_file(&dir, "State.json").map(fs::read) {
        Some(Ok(bytes)) => match parse_json(&bytes) {
            Ok(value) => state::parse(&value),
            Err(_) => {
                warnings
                    .push("State.json could not be read; editor state was not imported".to_owned());
                QuillState::default()
            }
        },
        _ => QuillState::default(),
    };
    if state.palette.len() == PALETTE_SIZE {
        document.settings.palette = state.palette.clone();
    }
    if document.settings.title.is_empty() {
        // "." has no name of its own; ask the file system what the folder is called.
        let resolved = dir.canonicalize().ok();
        let name = dir
            .file_name()
            .or_else(|| resolved.as_deref().and_then(Path::file_name))
            .and_then(|n| n.to_str());
        if let Some(name) = name {
            document.settings.title = name.to_owned();
        }
    }
    Ok(Imported {
        document,
        state,
        warnings,
    })
}
