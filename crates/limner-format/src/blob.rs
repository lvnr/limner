//! Stroke blobs (`.lstk`): the strokes of one drawing.
//!
//! Canonical form, all integers little-endian:
//!
//! ```text
//! 0   8   "LIMNSTK1"
//! 8   4   flags (0 in canonical form; bit 0 = body is a zstd frame)
//! 12  4   stroke count S
//! 16  8   point count P
//! 24  12S stroke table: { id u32, point count u32, brush u8, flags u8, reserved u16 }
//! ..  56P point data: 56 byte planes of P bytes each
//! ```
//!
//! A point is 14 `f32` components (position xyz, normal xyz, direction xyz, colour
//! rgb, opacity, width). Plane `4c + b` holds byte `b` (0 = least significant) of
//! component `c` for every point of the drawing in stroke order. Keeping each byte of
//! each component together is what lets zstd find the redundancy in float data.

use crate::container::{self, u32_at, u64_at, Canonical};
use crate::hash::ContentHash;
use limner_core::model::{Brush, Drawing, Point, Stroke};

pub(crate) const MAGIC: &[u8; 8] = b"LIMNSTK1";
pub(crate) const HEADER_LEN: usize = 24;
const ENTRY_LEN: usize = 12;
const COMPONENTS: usize = 14;
const POINT_LEN: usize = COMPONENTS * 4;
const STROKE_DIRECTIONAL: u8 = 1;

fn components(p: &Point) -> [f32; COMPONENTS] {
    [
        p.pos[0],
        p.pos[1],
        p.pos[2],
        p.normal[0],
        p.normal[1],
        p.normal[2],
        p.dir[0],
        p.dir[1],
        p.dir[2],
        p.color[0],
        p.color[1],
        p.color[2],
        p.opacity,
        p.width,
    ]
}

fn from_components(c: [f32; COMPONENTS]) -> Point {
    Point {
        pos: [c[0], c[1], c[2]],
        normal: [c[3], c[4], c[5]],
        dir: [c[6], c[7], c[8]],
        color: [c[9], c[10], c[11]],
        opacity: c[12],
        width: c[13],
    }
}

/// Encodes a drawing in canonical form. Chunk boundaries are not part of the
/// content: two drawings with the same strokes in the same order encode identically.
pub(crate) fn encode(drawing: &Drawing) -> Result<Canonical, String> {
    let stroke_count = u32::try_from(drawing.stroke_count())
        .map_err(|_| "a drawing has more than 4,294,967,295 strokes".to_owned())?;
    let point_count = drawing.point_count();

    let mut header = Vec::with_capacity(HEADER_LEN);
    header.extend_from_slice(MAGIC);
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&stroke_count.to_le_bytes());
    header.extend_from_slice(&(point_count as u64).to_le_bytes());

    let table_len = stroke_count as usize * ENTRY_LEN;
    crate::container::check_payload_len(
        table_len as u64 + point_count as u64 * POINT_LEN as u64,
        "a drawing",
    )?;
    let mut payload = vec![0u8; table_len + point_count * POINT_LEN];
    let (table, planes) = payload.split_at_mut(table_len);
    let mut index = 0usize;
    for (entry, stroke) in table
        .as_chunks_mut::<ENTRY_LEN>()
        .0
        .iter_mut()
        .zip(drawing.strokes())
    {
        let len = u32::try_from(stroke.len())
            .map_err(|_| "a stroke has more than 4,294,967,295 points".to_owned())?;
        entry[0..4].copy_from_slice(&stroke.id.to_le_bytes());
        entry[4..8].copy_from_slice(&len.to_le_bytes());
        entry[8] = stroke.brush.code();
        entry[9] = if stroke.directional {
            STROKE_DIRECTIONAL
        } else {
            0
        };
        for point in stroke.points() {
            for (c, value) in components(point).iter().enumerate() {
                for (b, byte) in value.to_le_bytes().iter().enumerate() {
                    planes[(c * 4 + b) * point_count + index] = *byte;
                }
            }
            index += 1;
        }
    }
    Ok(Canonical::new(header, payload))
}

/// Decodes a stored blob, verifying it against `expected` when a hash is given.
pub(crate) fn decode(bytes: &[u8], expected: Option<&ContentHash>) -> Result<Drawing, String> {
    let canonical = container::open(bytes, MAGIC, HEADER_LEN, expected, |header| {
        let strokes = u32_at(header, 12) as u64;
        let points = u64_at(header, 16);
        points
            .checked_mul(POINT_LEN as u64)
            .and_then(|p| p.checked_add(strokes * ENTRY_LEN as u64))
            .ok_or_else(|| "its header describes an impossible number of points".to_owned())
    })?;
    let stroke_count = u32_at(&canonical.header, 12) as usize;
    let point_count = u64_at(&canonical.header, 16) as usize;
    let (table, planes) = canonical.payload.split_at(stroke_count * ENTRY_LEN);

    let mut drawing = Drawing::new();
    let mut index = 0usize;
    for (number, entry) in table.as_chunks::<ENTRY_LEN>().0.iter().enumerate() {
        let id = u32_at(entry, 0);
        let len = u32_at(entry, 4) as usize;
        let brush = Brush::from_code(entry[8] as i32)
            .ok_or_else(|| format!("stroke {number} uses unknown brush code {}", entry[8]))?;
        if entry[9] & !STROKE_DIRECTIONAL != 0 || entry[10] != 0 || entry[11] != 0 {
            return Err(format!(
                "stroke {number} sets flags this version does not know"
            ));
        }
        if len > point_count - index {
            return Err(format!(
                "stroke {number} claims {len} points but only {} remain",
                point_count - index
            ));
        }
        let mut points = Vec::with_capacity(len);
        for i in index..index + len {
            let mut values = [0f32; COMPONENTS];
            for (c, value) in values.iter_mut().enumerate() {
                let mut raw = [0u8; 4];
                for (b, byte) in raw.iter_mut().enumerate() {
                    *byte = planes[(c * 4 + b) * point_count + i];
                }
                *value = f32::from_le_bytes(raw);
            }
            points.push(from_components(values));
        }
        index += len;
        drawing.push_stroke(Stroke::new(
            id,
            brush,
            entry[9] & STROKE_DIRECTIONAL != 0,
            points,
        ));
    }
    if index != point_count {
        return Err(format!(
            "its strokes hold {index} points but its header says {point_count}"
        ));
    }
    Ok(drawing)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(seed: f32) -> Point {
        Point {
            pos: [seed, seed + 0.25, -seed],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, -0.0, -1.0],
            color: [0.1, 0.2, 0.3],
            opacity: 0.5,
            width: 0.01 * seed,
        }
    }

    fn sample() -> Drawing {
        Drawing::from_strokes([
            Stroke::new(
                7,
                Brush::Cylinder,
                false,
                (0..5).map(|i| point(i as f32)).collect(),
            ),
            Stroke::new(8, Brush::Ribbon, true, Vec::new()),
            Stroke::new(u32::MAX, Brush::Cube, true, vec![point(9.0)]),
        ])
    }

    #[test]
    fn layout_matches_the_specification() {
        let canonical = encode(&sample()).unwrap();
        assert_eq!(&canonical.header[..8], b"LIMNSTK1");
        assert_eq!(u32_at(&canonical.header, 8), 0);
        assert_eq!(u32_at(&canonical.header, 12), 3);
        assert_eq!(u64_at(&canonical.header, 16), 6);
        assert_eq!(canonical.payload.len(), 3 * 12 + 6 * 56);
        // Second table entry: id 8, no points, ribbon, directional.
        assert_eq!(
            &canonical.payload[12..24],
            &[8, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0]
        );
        // Plane 0 is the low byte of pos.x of all six points; plane 3 the high byte.
        let planes = &canonical.payload[36..];
        let xs = [0.0f32, 1.0, 2.0, 3.0, 4.0, 9.0];
        for (i, x) in xs.iter().enumerate() {
            let bytes = x.to_le_bytes();
            assert_eq!(planes[i], bytes[0]);
            assert_eq!(planes[3 * 6 + i], bytes[3]);
        }
        // Component 12 is opacity: plane 4 * 12 + 3 is its high byte.
        assert_eq!(planes[(4 * 12 + 3) * 6], 0.5f32.to_le_bytes()[3]);
        assert_eq!(
            canonical.hash,
            ContentHash::of(&[&canonical.header, &canonical.payload])
        );
    }

    #[test]
    fn stored_blobs_decode_whether_or_not_they_are_compressed() {
        let drawing = sample();
        let canonical = encode(&drawing).unwrap();
        let mut raw = canonical.header.clone();
        raw.extend_from_slice(&canonical.payload);
        assert_eq!(decode(&raw, Some(&canonical.hash)).unwrap(), drawing);

        let stored = canonical.file_bytes(3);
        assert_eq!(u32_at(&stored, 8), container::FLAG_ZSTD);
        assert!(stored.len() < raw.len());
        assert_eq!(decode(&stored, Some(&canonical.hash)).unwrap(), drawing);
    }

    #[test]
    fn an_empty_drawing_is_stored_uncompressed() {
        let canonical = encode(&Drawing::new()).unwrap();
        let stored = canonical.file_bytes(3);
        assert_eq!(stored.len(), HEADER_LEN);
        assert_eq!(u32_at(&stored, 8), 0);
        assert!(decode(&stored, Some(&canonical.hash)).unwrap().is_empty());
    }

    #[test]
    fn chunking_does_not_change_the_content() {
        let strokes =
            || (0..4).map(|i| Stroke::new(i, Brush::Ribbon, false, vec![point(i as f32); 3]));
        let one_chunk = Drawing::from_strokes(strokes());
        let mut many = Drawing::new();
        for stroke in strokes() {
            many.push_stroke_in_new_chunk(stroke);
        }
        assert_ne!(one_chunk.chunks().len(), many.chunks().len());
        assert_eq!(
            encode(&one_chunk).unwrap().hash,
            encode(&many).unwrap().hash
        );
    }

    #[test]
    fn damage_is_reported() {
        let canonical = encode(&sample()).unwrap();
        let stored = canonical.file_bytes(3);
        let hash = Some(&canonical.hash);

        let err = decode(&stored[..10], hash).unwrap_err();
        assert!(err.contains("shorter than"), "{err}");
        let err = decode(&stored[..stored.len() - 5], hash).unwrap_err();
        assert!(err.contains("compressed body"), "{err}");

        let mut wrong_magic = stored.clone();
        wrong_magic[0] = b'X';
        assert!(decode(&wrong_magic, hash)
            .unwrap_err()
            .contains("signature"));

        let mut unknown_flag = stored.clone();
        unknown_flag[9] = 1;
        assert!(decode(&unknown_flag, hash).unwrap_err().contains("flags"));

        let mut wrong_count = stored.clone();
        wrong_count[16] += 1;
        assert!(decode(&wrong_count, hash).is_err());

        // A flipped bit in an uncompressed blob is caught by the hash alone.
        let mut raw = canonical.header.clone();
        raw.extend_from_slice(&canonical.payload);
        let last = raw.len() - 1;
        raw[last] ^= 0x10;
        let err = decode(&raw, hash).unwrap_err();
        assert!(err.contains("hashes to"), "{err}");
        // Without a hash to check against, the same bytes still decode.
        assert!(decode(&raw, None).is_ok());

        let other = ContentHash::of(&[b"something else"]);
        assert!(decode(&stored, Some(&other))
            .unwrap_err()
            .contains("hashes to"));
    }
}
