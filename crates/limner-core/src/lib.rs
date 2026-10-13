//! Engine-independent core of Limner: the document model, timeline evaluation,
//! stroke tessellation, tools and editing. No I/O and no engine types live here.

pub mod math;
pub mod model;
pub mod tess;
pub mod time;

pub use glam;
