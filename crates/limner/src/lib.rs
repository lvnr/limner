//! Facade over the Limner crates: opening and saving documents by path, shared by
//! the command-line tool and the Godot bridge.

pub mod files;
pub mod media;

pub use limner_core as core;
pub use limner_format as format;
pub use limner_quill as quill;

pub use files::{
    is_packed_path, open, read_state, save, save_with_state, OpenError, Opened, SaveError,
    SourceKind,
};
pub use limner_format::SaveReport;
