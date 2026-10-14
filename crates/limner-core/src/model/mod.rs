//! The document model.

mod document;
mod drawing;
mod ids;
mod keys;
mod layer;
mod media;
mod settings;
mod stroke;

pub use document::{Document, DocumentError, MAX_LAYER_DEPTH};
pub use drawing::{Drawing, StrokeAddr, StrokeChunk, MAX_CHUNK_VERTICES};
pub use ids::{DrawingId, LayerId, MediaId};
pub use keys::{Interp, Key, Track};
pub use layer::{
    Action, Attenuation, AttenuationMode, Camera, Group, Layer, LayerAnim, LayerKind, Light,
    LightKind, Model, Paint, Picture, Projection, Sequence, Shading, Sound, SoundKind,
    SoundModifier, Viewpoint,
};
pub use media::{jpeg_size, png_size, Media, MediaData, MediaKind, MeshData};
pub use settings::{Settings, PALETTE_SIZE};
pub use stroke::{Brush, Point, Stroke, OPAQUE_THRESHOLD};
