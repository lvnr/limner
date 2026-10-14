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

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// Width and height from a PNG file's header.
pub fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    const SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(SIGNATURE) || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be_u32(bytes, 16)?, be_u32(bytes, 20)?))
}

/// Width and height from the frame header of a JPEG file.
pub fn jpeg_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut at = 2;
    loop {
        // Segments start with 0xFF, possibly padded with more of them.
        while *bytes.get(at)? == 0xFF {
            at += 1;
        }
        let marker = *bytes.get(at)?;
        at += 1;
        match marker {
            // Markers that stand alone.
            0x01 | 0xD0..=0xD8 => continue,
            // The end of the image, or the start of the scan, before any frame header.
            0xD9 | 0xDA => return None,
            _ => {}
        }
        let length = u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?) as usize;
        let is_frame_header =
            (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_frame_header {
            let height = u16::from_be_bytes(bytes.get(at + 3..at + 5)?.try_into().ok()?);
            let width = u16::from_be_bytes(bytes.get(at + 5..at + 7)?.try_into().ok()?);
            return Some((width as u32, height as u32));
        }
        at += length.max(2);
    }
}

impl MediaData {
    /// Width and height of a picture in pixels, read from its header when it is
    /// still as it was in the file.
    pub fn image_size(&self) -> Option<(u32, u32)> {
        match self {
            MediaData::RawImage { width, height, .. } => Some((*width, *height)),
            MediaData::Encoded { format, bytes } => match format.as_str() {
                "png" => png_size(bytes),
                "jpg" => jpeg_size(bytes),
                _ => None,
            },
            _ => None,
        }
    }
}
