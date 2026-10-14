//! The native Limner project format.
//!
//! A project is a folder, `Name.limn/`:
//!
//! ```text
//! limn.json                    manifest: settings, layer tree, drawing and media tables
//! drawings/ab/abcdef….lstk     stroke data, one file per distinct drawing, named by content hash
//! media/<hash>.<ext>           pictures, sounds and models
//! state.json                   optional editor state, owned by the application
//! thumbnail.png                optional
//! ```
//!
//! or the same files packed into one `Name.limnz` (a zip archive) for sharing.
//!
//! The design goals, in order: an interrupted save can never corrupt a project; a
//! save rewrites only what changed; the folder works well in git and Perforce
//! (readable, stable JSON and immutable binaries); and everything in the core model
//! survives a round trip exactly. `docs/format.md` is the byte-level specification.
//!
//! All file access of the native format lives here; `limner-core` does no I/O.

mod blob;
mod container;
mod error;
mod hash;
mod json;
mod load;
mod manifest;
mod media;
mod mesh;
mod pack;
mod save;
mod schema;
mod state;
mod store;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
mod wav;

pub use error::FormatError;
pub use hash::ContentHash;
pub use load::{load_project, Loaded};
pub use manifest::MAX_LAYER_DEPTH;
pub use pack::{pack, unpack};
pub use save::{save_packed, save_packed_with, save_project, SaveOptions, SaveReport};
pub use state::{read_state, read_thumbnail, write_state, write_thumbnail};

use std::path::Path;

/// The version of the format this crate writes, and the newest it reads.
pub const FORMAT_VERSION: u32 = schema::FORMAT_VERSION;
/// Extension of a project folder.
pub const PROJECT_EXTENSION: &str = "limn";
/// Extension of a packed project file.
pub const PACKED_EXTENSION: &str = "limnz";

/// True when `path` is a project folder, the `limn.json` inside one, or a packed
/// project file. The extension is not consulted; the content is.
pub fn is_limn_project(path: &Path) -> bool {
    store::Source::recognises(path)
}

/// The content hash that names the stroke blob of `drawing`.
pub fn drawing_hash(drawing: &limner_core::model::Drawing) -> Result<ContentHash, FormatError> {
    blob::encode(drawing)
        .map(|canonical| canonical.hash)
        .map_err(FormatError::Unsaveable)
}
