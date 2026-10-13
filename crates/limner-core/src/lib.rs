//! Engine-independent core of Limner: the document model, timeline evaluation,
//! stroke tessellation, tools and editing. No I/O and no engine types live here.

pub mod dither;
pub mod eval;
pub mod math;
pub mod model;
pub mod session;
pub mod tess;
pub mod time;

pub use glam;
