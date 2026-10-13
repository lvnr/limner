//! Geometry types shared by the model, evaluation and tools.

mod bounds;
mod xform;

pub use bounds::Aabb;
pub use xform::{Flip, Xform};
