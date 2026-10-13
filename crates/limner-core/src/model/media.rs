//! Embedded media: pictures, sounds and models.

use super::ids::MediaId;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Audio,
    Mesh,
}

/// Triangle or quad mesh data for model layers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    /// Empty, or one per position.
    pub normals: Vec<[f32; 3]>,
    /// Empty, or one linear RGB colour per position.
    pub colors: Vec<[f32; 3]>,
    /// Triangle list.
    pub indices: Vec<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaData {
    /// A file in its original encoding (`png`, `jpg`, `wav`, `ogg`, `mp3`, `glb`, `obj`, `fbx`).
    Encoded {
        format: String,
        bytes: Arc<[u8]>,
    },
    /// Decoded 8-bit pixels, rows top to bottom.
    RawImage {
        width: u32,
        height: u32,
        channels: u8,
        pixels: Arc<[u8]>,
    },
    /// Interleaved PCM samples, little-endian.
    RawAudio {
        channels: u16,
        sample_rate: u32,
        bits_per_sample: u16,
        float: bool,
        samples: Arc<[u8]>,
    },
    Mesh(Arc<MeshData>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Media {
    pub id: MediaId,
    /// Original file name, for display.
    pub name: String,
    pub kind: MediaKind,
    pub data: MediaData,
}

impl Media {
    /// Approximate memory held by this item.
    pub fn byte_size(&self) -> usize {
        match &self.data {
            MediaData::Encoded { bytes, .. } => bytes.len(),
            MediaData::RawImage { pixels, .. } => pixels.len(),
            MediaData::RawAudio { samples, .. } => samples.len(),
            MediaData::Mesh(mesh) => {
                mesh.positions.len() * 12
                    + mesh.normals.len() * 12
                    + mesh.colors.len() * 12
                    + mesh.indices.len() * 4
            }
        }
    }
}
