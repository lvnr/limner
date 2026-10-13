//! Stroke tessellation: from stored points to triangles.

mod chunk;
mod frame;

pub use chunk::{
    chunk_is_opaque, linear_to_srgb8, tessellate_chunk, ChunkMesh, SurfaceKind, SurfaceMesh,
};
pub use frame::{point_frame, ring_positions, ring_profile, Frame, RingVertex, ELLIPSE_RATIO};
