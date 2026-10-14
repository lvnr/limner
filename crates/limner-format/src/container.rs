//! The binary container shared by stroke blobs and meshes.
//!
//! A file is a fixed header followed by a body. The header's `flags` word (bytes
//! 8..12) says how the body is stored; the *canonical* form of a file is the same
//! header with `flags = 0` followed by the body uncompressed. The content hash is
//! taken over the canonical form, so it does not depend on the compressor.

use crate::hash::ContentHash;

/// Body is one zstd frame that records its content size.
pub(crate) const FLAG_ZSTD: u32 = 1;
/// Offset of the little-endian `flags` word in every container header.
const FLAGS_AT: usize = 8;

/// A piece of content in canonical form.
pub(crate) struct Canonical {
    /// The header with `flags = 0`.
    pub header: Vec<u8>,
    pub payload: Vec<u8>,
    pub hash: ContentHash,
}

impl Canonical {
    pub fn new(header: Vec<u8>, payload: Vec<u8>) -> Self {
        let hash = ContentHash::of(&[&header, &payload]);
        Self {
            header,
            payload,
            hash,
        }
    }

    /// The bytes to store: compressed when that is smaller, canonical otherwise.
    pub fn file_bytes(&self, level: i32) -> Vec<u8> {
        let range = zstd::compression_level_range();
        let level = level.clamp(*range.start(), *range.end());
        let compressed = zstd::bulk::compress(&self.payload, level)
            .ok()
            .filter(|c| c.len() < self.payload.len());
        let (flags, body) = match &compressed {
            Some(c) => (FLAG_ZSTD, c.as_slice()),
            None => (0, self.payload.as_slice()),
        };
        let mut out = Vec::with_capacity(self.header.len() + body.len());
        out.extend_from_slice(&self.header);
        out[FLAGS_AT..FLAGS_AT + 4].copy_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(body);
        out
    }
}

/// Splits a stored file into its canonical header and payload, checking the magic,
/// the flags, the payload length the header implies, and the content hash.
///
/// `payload_len` computes the expected payload length from the canonical header, or
/// explains why the header is not acceptable.
pub(crate) fn open(
    bytes: &[u8],
    magic: &[u8; 8],
    header_len: usize,
    expected: Option<&ContentHash>,
    payload_len: impl FnOnce(&[u8]) -> Result<u64, String>,
) -> Result<Canonical, String> {
    if bytes.len() < header_len {
        return Err(format!(
            "the file is {} bytes long, shorter than its {header_len}-byte header",
            bytes.len()
        ));
    }
    if &bytes[..8] != magic {
        return Err(format!(
            "it does not start with the signature {}",
            String::from_utf8_lossy(magic)
        ));
    }
    let flags = u32_at(bytes, FLAGS_AT);
    if flags & !FLAG_ZSTD != 0 {
        return Err(format!(
            "it uses storage flags {flags:#x} that this version of Limner does not know"
        ));
    }
    let mut header = bytes[..header_len].to_vec();
    header[FLAGS_AT..FLAGS_AT + 4].fill(0);
    let wanted = payload_len(&header)?;
    let wanted = usize::try_from(wanted)
        .map_err(|_| "its header describes more data than fits in memory".to_owned())?;
    let body = &bytes[header_len..];
    let payload = if flags & FLAG_ZSTD != 0 {
        match zstd::zstd_safe::get_frame_content_size(body) {
            Ok(Some(size)) if size == wanted as u64 => {}
            Ok(Some(size)) => {
                return Err(format!(
                "its header describes {wanted} bytes of data but its compressed body holds {size}"
            ))
            }
            _ => return Err("its compressed body is cut short or not a zstd frame".to_owned()),
        }
        let mut payload = Vec::new();
        payload
            .try_reserve_exact(wanted)
            .map_err(|_| "its header describes more data than fits in memory".to_owned())?;
        // Writing into a `Vec` fills its reserved capacity, which bounds the output.
        zstd::bulk::Decompressor::new()
            .and_then(|mut decompressor| decompressor.decompress_to_buffer(body, &mut payload))
            .map_err(|e| format!("its compressed body cannot be read ({e})"))?;
        payload
    } else {
        body.to_vec()
    };
    if payload.len() != wanted {
        return Err(format!(
            "its header describes {wanted} bytes of data but {} are present",
            payload.len()
        ));
    }
    let canonical = Canonical::new(header, payload);
    if let Some(expected) = expected {
        if canonical.hash != *expected {
            return Err(format!(
                "its content hashes to {} but its name says {expected}",
                canonical.hash
            ));
        }
    }
    Ok(canonical)
}

pub(crate) fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

pub(crate) fn u32_at(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

pub(crate) fn u64_at(bytes: &[u8], at: usize) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[at..at + 8]);
    u64::from_le_bytes(raw)
}
