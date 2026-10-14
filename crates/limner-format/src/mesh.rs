//! Meshes (`.lmsh`): the triangle data of a model layer.
//!
//! Canonical form, all values little-endian:
//!
//! ```text
//! 0   8   "LIMNMSH1"
//! 8   4   flags (0 in canonical form; bit 0 = body is a zstd frame)
//! 12  4   position count
//! 16  4   normal count
//! 20  4   colour count
//! 24  4   index count
//! 28  4   reserved, zero
//! 32  ..  positions (3 f32 each), normals (3 f32 each), colours (3 f32 each), indices (u32 each)
//! ```

use crate::container::{self, u32_at, Canonical};
use crate::hash::ContentHash;
use limner_core::model::MeshData;

pub(crate) const MAGIC: &[u8; 8] = b"LIMNMSH1";
pub(crate) const HEADER_LEN: usize = 32;

pub(crate) fn encode(mesh: &MeshData) -> Result<Canonical, String> {
    let count = |n: usize| {
        u32::try_from(n).map_err(|_| "a mesh has more than 4,294,967,295 elements".to_owned())
    };
    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&0u32.to_le_bytes());
    for n in [
        mesh.positions.len(),
        mesh.normals.len(),
        mesh.colors.len(),
        mesh.indices.len(),
    ] {
        header.extend_from_slice(&count(n)?.to_le_bytes());
    }
    header.extend_from_slice(&0u32.to_le_bytes());

    let vectors = mesh.positions.len() + mesh.normals.len() + mesh.colors.len();
    let mut payload = Vec::with_capacity(vectors * 12 + mesh.indices.len() * 4);
    for list in [&mesh.positions, &mesh.normals, &mesh.colors] {
        for vector in list {
            for value in vector {
                payload.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    for index in &mesh.indices {
        payload.extend_from_slice(&index.to_le_bytes());
    }
    Ok(Canonical::new(header, payload))
}

pub(crate) fn decode(bytes: &[u8], expected: Option<&ContentHash>) -> Result<MeshData, String> {
    let canonical = container::open(bytes, MAGIC, HEADER_LEN, expected, |header| {
        if u32_at(header, 28) != 0 {
            return Err("its reserved header field is not zero".to_owned());
        }
        let vectors =
            u32_at(header, 12) as u64 + u32_at(header, 16) as u64 + u32_at(header, 20) as u64;
        Ok(vectors * 12 + u32_at(header, 24) as u64 * 4)
    })?;
    let mut at = 0usize;
    let payload = &canonical.payload;
    let mut vectors = |count: u32| -> Vec<[f32; 3]> {
        let (records, _) = payload[at..at + count as usize * 12].as_chunks::<12>();
        at += count as usize * 12;
        records
            .iter()
            .map(|v| {
                [
                    f32::from_le_bytes([v[0], v[1], v[2], v[3]]),
                    f32::from_le_bytes([v[4], v[5], v[6], v[7]]),
                    f32::from_le_bytes([v[8], v[9], v[10], v[11]]),
                ]
            })
            .collect()
    };
    let positions = vectors(u32_at(&canonical.header, 12));
    let normals = vectors(u32_at(&canonical.header, 16));
    let colors = vectors(u32_at(&canonical.header, 20));
    let (indices, _) = payload[at..].as_chunks::<4>();
    let indices = indices.iter().map(|v| u32::from_le_bytes(*v)).collect();
    Ok(MeshData {
        positions,
        normals,
        colors,
        indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meshes_round_trip_even_when_their_arrays_disagree() {
        let mesh = MeshData {
            positions: vec![
                [0.0, 1.0, 2.0],
                [3.0, 4.0, 5.0],
                [-0.0, f32::MIN_POSITIVE, 7.5],
            ],
            normals: vec![[0.0, 1.0, 0.0]],
            colors: Vec::new(),
            indices: vec![0, 1, 2, 2, 1, u32::MAX],
        };
        let canonical = encode(&mesh).unwrap();
        assert_eq!(canonical.header.len(), HEADER_LEN);
        assert_eq!(u32_at(&canonical.header, 12), 3);
        assert_eq!(u32_at(&canonical.header, 16), 1);
        assert_eq!(u32_at(&canonical.header, 20), 0);
        assert_eq!(u32_at(&canonical.header, 24), 6);
        assert_eq!(canonical.payload.len(), 4 * 12 + 6 * 4);
        let back = decode(&canonical.file_bytes(3), Some(&canonical.hash)).unwrap();
        assert_eq!(back, mesh);
        assert_eq!(back.positions[2][0].to_bits(), (-0.0f32).to_bits());

        let empty = encode(&MeshData::default()).unwrap();
        assert_eq!(
            decode(&empty.file_bytes(3), Some(&empty.hash)).unwrap(),
            MeshData::default()
        );
    }

    #[test]
    fn a_truncated_mesh_is_rejected() {
        let mesh = MeshData {
            positions: vec![[1.0, 2.0, 3.0]; 100],
            normals: Vec::new(),
            colors: Vec::new(),
            indices: (0..300).collect(),
        };
        let canonical = encode(&mesh).unwrap();
        let stored = canonical.file_bytes(3);
        assert!(decode(&stored[..stored.len() / 2], Some(&canonical.hash)).is_err());
        assert!(decode(&stored[..20], Some(&canonical.hash)).is_err());
    }
}
