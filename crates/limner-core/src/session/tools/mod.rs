//! The painting and editing tools.

mod settings;

pub use settings::{
    BlendMode, BrushStyle, Cap, ColorizeSettings, LineSettings, OptimizeSettings, PaintSettings,
    SelectSettings, SphereSettings, Taper, ToolKind, ToolSettings, MAX_TOOL_SIZE, MIN_TOOL_SIZE,
};
