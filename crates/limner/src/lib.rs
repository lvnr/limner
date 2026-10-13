//! Facade over the Limner crates: opening and saving documents by path, shared by
//! the command-line tool and the Godot bridge.

pub mod files;

pub use limner_core as core;
pub use limner_quill as quill;

pub use files::{open, OpenError, Opened, SourceKind};
