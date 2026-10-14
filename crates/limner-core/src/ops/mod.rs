//! Operations on a document, shared by commands and tools.
//!
//! Each operation takes the document as a value to change. The session snapshots the
//! document before running one and restores it on failure, so an operation that fails
//! half-way never leaves a half-edited document behind.

pub mod clips;
pub mod color;
pub mod frames;
pub mod keys;
pub mod layers;
pub mod lighting;
pub mod occlusion;
pub mod simplify;
pub mod strokes;

use crate::model::{DocumentError, LayerId};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum OpError {
    #[error("layer {0} does not exist")]
    NoSuchLayer(LayerId),
    #[error("layer {0} is not a group")]
    NotGroup(LayerId),
    #[error("layer {0} is not a paint layer")]
    NotPaint(LayerId),
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Document(#[from] DocumentError),
}
