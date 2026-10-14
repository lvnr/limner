//! Editor state and the thumbnail: optional files beside the manifest.
//!
//! `state.json` belongs to the application (palette, last tool, view). This crate
//! stores and returns it without interpreting it, and a save of the document never
//! touches it.

use crate::error::FormatError;
use crate::store::{write_atomic, Source, STATE_FILE, THUMBNAIL_FILE};
use std::path::Path;

/// Reads the editor state of the project at `path` (a folder or a packed file).
/// `None` when the project has none.
pub fn read_state(path: &Path) -> Result<Option<serde_json::Value>, FormatError> {
    let mut source = Source::open(path)?;
    let Some(bytes) = source.read(STATE_FILE)? else {
        return Ok(None);
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| FormatError::Json {
            path: source.locate(STATE_FILE),
            source: e,
        })
}

/// Writes the editor state of the project folder `dir`, replacing it atomically.
pub fn write_state(dir: &Path, state: &serde_json::Value) -> Result<(), FormatError> {
    write_atomic(&dir.join(STATE_FILE), &state_bytes(state)?, true)
}

/// The bytes of `state.json`: indented JSON with members in sorted order.
pub(crate) fn state_bytes(state: &serde_json::Value) -> Result<Vec<u8>, FormatError> {
    let mut text = serde_json::to_string_pretty(state)
        .map_err(|e| FormatError::Unsaveable(format!("the editor state cannot be written: {e}")))?;
    text.push('\n');
    Ok(text.into_bytes())
}

/// Reads the project's thumbnail, a PNG file, if it has one.
pub fn read_thumbnail(path: &Path) -> Result<Option<Vec<u8>>, FormatError> {
    Source::open(path)?.read(THUMBNAIL_FILE)
}

/// Writes the thumbnail of the project folder `dir`. `png` must be a PNG file.
pub fn write_thumbnail(dir: &Path, png: &[u8]) -> Result<(), FormatError> {
    write_atomic(&dir.join(THUMBNAIL_FILE), png, true)
}
