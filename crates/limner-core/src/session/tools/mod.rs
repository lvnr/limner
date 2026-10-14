//! The painting and editing tools.

pub(crate) mod paint;
pub(crate) mod select;
mod settings;
pub(crate) mod space;
pub(crate) mod sphere;
pub mod stroke_builder;

pub use select::Nearest;
pub use settings::{
    BlendMode, BrushStyle, Cap, ColorizeSettings, LineSettings, OptimizeSettings, PaintSettings,
    SelectSettings, SphereSettings, Taper, ToolKind, ToolSettings, MAX_TOOL_SIZE, MIN_TOOL_SIZE,
};
pub use space::{LayerSpace, PaintTarget};
pub use sphere::falloff;

#[cfg(test)]
pub(crate) mod tests;
