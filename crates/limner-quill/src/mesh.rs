//! Model records: Quill's mesh serializer (`piMesh` in the IMM sources).
//!
//! ```text
//! 8 bytes   zero in every file seen (meaning unknown)
//! u32       version = 0
//! f32[6]    bounding box: minX, maxX, minY, maxY, minZ, maxZ
//! u32       vertex stream count (at most 8)
//!   per stream: u32 vertex count, u32 stride in bytes, u32 divisor, u32 element count (at most 8)
//!               per element: u32 type (1 = f32), u32 components, u32 normalize, u32 byte offset
//!               vertex count * stride bytes
//! u32       face type (0 = polygons, 1 = points)
//! u32       index depth (1 = 32-bit)
//! u32       index array count (at most 48)
//!   per array:  u32 face count, then per face { i32 corners (3 or 4), u32 index[4] }
//! ```
//!
//! The elements of a model's stream are position, colour and normal, in the order
//! Quill's model shader declares them. A triangle keeps a meaningless fourth index.

use crate::qbin::{past_the_end, Qbin, Unreadable};
use limner_core::model::MeshData;

const MAX_STREAMS: u32 = 8;
const MAX_ELEMENTS: u32 = 8;
const MAX_INDEX_ARRAYS: u32 = 48;
const ELEMENT_F32: u32 = 1;
const FACE_POLYGONS: u32 = 0;
const FACE_POINTS: u32 = 1;
const INDEX_DEPTH_32: u32 = 1;
/// Bytes of one stored polygon: a corner count and four indices.
const FACE_LEN: usize = 20;

/// What was repaired or left out while reading a mesh that could still be used.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MeshNotes {
    /// Faces that were not triangles or quads, or that used a missing or damaged vertex.
    pub skipped_faces: usize,
    /// Vertices with non-finite coordinates; the faces using them were skipped.
    pub damaged_vertices: usize,
    /// Non-finite normals and colours, replaced by an up vector and white.
    pub repaired_attributes: usize,
    /// The vertex attributes were not the position, colour, normal triple Quill writes.
    pub unexpected_layout: bool,
    /// More than one vertex stream; only the first was read.
    pub extra_streams: bool,
    /// The record holds points instead of polygons.
    pub points_only: bool,
}

#[cfg(test)]
impl MeshNotes {
    fn is_clean(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy)]
struct Element {
    kind: u32,
    components: u32,
    offset: u32,
}

struct Stream<'a> {
    count: usize,
    stride: usize,
    elements: Vec<Element>,
    data: &'a [u8],
}

impl Stream<'_> {
    /// True when `element` can be read as three floats from every vertex.
    fn holds_vec3(&self, element: &Element) -> bool {
        element.kind == ELEMENT_F32
            && element.components >= 3
            && (element.offset as usize)
                .checked_add(12)
                .is_some_and(|end| end <= self.stride)
    }

    fn vec3s(&self, element: &Element) -> Vec<[f32; 3]> {
        let offset = element.offset as usize;
        (0..self.count)
            .map(|i| {
                let at = i * self.stride + offset;
                let value: [[u8; 4]; 3] = bytemuck::pod_read_unaligned(&self.data[at..at + 12]);
                value.map(f32::from_le_bytes)
            })
            .collect()
    }
}

impl Qbin<'_> {
    /// Reads the mesh record at `offset` into triangles.
    pub(crate) fn read_mesh(&self, offset: u64) -> Result<(MeshData, MeshNotes), Unreadable> {
        let mut c = self.cursor(offset).ok_or_else(past_the_end)?;
        c.take(8)?;
        let version = c.u32()?;
        if version != 0 {
            return Err(format!("mesh version {version} is not supported"));
        }
        c.take(24)?;

        let stream_count = c.u32()?;
        if stream_count == 0 || stream_count > MAX_STREAMS {
            return Err(format!("the mesh claims {stream_count} vertex streams"));
        }
        let mut streams = Vec::with_capacity(stream_count as usize);
        for _ in 0..stream_count {
            let count = c.u32()? as usize;
            let stride = c.u32()? as usize;
            let _divisor = c.u32()?;
            let element_count = c.u32()?;
            if element_count > MAX_ELEMENTS {
                return Err(format!("a vertex stream claims {element_count} attributes"));
            }
            let mut elements = Vec::with_capacity(element_count as usize);
            for _ in 0..element_count {
                let kind = c.u32()?;
                let components = c.u32()?;
                let _normalize = c.u32()?;
                let offset = c.u32()?;
                elements.push(Element {
                    kind,
                    components,
                    offset,
                });
            }
            let data = c.take(count.checked_mul(stride).ok_or_else(past_the_end)?)?;
            streams.push(Stream {
                count,
                stride,
                elements,
                data,
            });
        }

        let face_type = c.u32()?;
        if face_type != FACE_POLYGONS && face_type != FACE_POINTS {
            return Err(format!("face type {face_type} is not supported"));
        }
        let depth = c.u32()?;
        if depth != INDEX_DEPTH_32 {
            return Err(format!("index depth {depth} is not supported"));
        }
        let array_count = c.u32()?;
        if array_count > MAX_INDEX_ARRAYS {
            return Err(format!("the mesh claims {array_count} index arrays"));
        }
        let entry_len = if face_type == FACE_POLYGONS {
            FACE_LEN
        } else {
            4
        };
        let mut index_arrays = Vec::with_capacity(array_count as usize);
        for _ in 0..array_count {
            let count = c.u32()? as usize;
            index_arrays.push(c.take(count.checked_mul(entry_len).ok_or_else(past_the_end)?)?);
        }

        let mut notes = MeshNotes {
            extra_streams: streams.iter().skip(1).any(|s| s.count > 0),
            points_only: face_type == FACE_POINTS,
            ..MeshNotes::default()
        };
        let stream = &streams[0];
        let Some(position) = stream.elements.first().filter(|e| stream.holds_vec3(e)) else {
            return Err("the mesh has no floating-point positions".to_owned());
        };
        let mut mesh = MeshData {
            positions: stream.vec3s(position),
            ..MeshData::default()
        };

        // Quill writes position, colour, normal. Anything else is matched as well as
        // the data allows and reported.
        let rest = &stream.elements[1..];
        let vectors: Vec<&Element> = rest.iter().filter(|e| stream.holds_vec3(e)).collect();
        notes.unexpected_layout = vectors.len() != rest.len();
        match vectors.as_slice() {
            [] => {}
            [color, normal] if normal.components == 3 => {
                mesh.colors = stream.vec3s(color);
                mesh.normals = stream.vec3s(normal);
            }
            [single, ..] => {
                notes.unexpected_layout = true;
                let values = stream.vec3s(single);
                if single.components == 3 && values.iter().all(is_unit_or_zero) {
                    mesh.normals = values;
                } else {
                    mesh.colors = values;
                }
            }
        }

        let mut damaged: Vec<bool> = Vec::new();
        for (index, p) in mesh.positions.iter_mut().enumerate() {
            if !p.iter().all(|v| v.is_finite()) {
                if damaged.is_empty() {
                    damaged = vec![false; stream.count];
                }
                damaged[index] = true;
                notes.damaged_vertices += 1;
                *p = [0.0; 3];
            }
        }
        for n in &mut mesh.normals {
            if !n.iter().all(|v| v.is_finite()) {
                notes.repaired_attributes += 1;
                *n = [0.0, 1.0, 0.0];
            }
        }
        for color in &mut mesh.colors {
            if !color.iter().all(|v| v.is_finite()) {
                notes.repaired_attributes += 1;
                *color = [1.0; 3];
            }
        }

        if face_type == FACE_POLYGONS {
            let usable = |i: u32| {
                (i as usize) < stream.count && !damaged.get(i as usize).copied().unwrap_or(false)
            };
            let face_total: usize = index_arrays.iter().map(|a| a.len() / FACE_LEN).sum();
            mesh.indices.reserve(face_total.saturating_mul(3));
            for face in index_arrays
                .iter()
                .flat_map(|a| a.as_chunks::<FACE_LEN>().0)
            {
                let words: [[u8; 4]; 5] = bytemuck::cast(*face);
                let corners = i32::from_le_bytes(words[0]);
                let [a, b, c, d] = [words[1], words[2], words[3], words[4]].map(u32::from_le_bytes);
                match corners {
                    3 if [a, b, c].into_iter().all(usable) => mesh.indices.extend([a, b, c]),
                    4 if [a, b, c, d].into_iter().all(usable) => {
                        mesh.indices.extend([a, b, c, a, c, d]);
                    }
                    _ => notes.skipped_faces += 1,
                }
            }
        }
        Ok((mesh, notes))
    }
}

fn is_unit_or_zero(v: &[f32; 3]) -> bool {
    let length_squared: f32 = v.iter().map(|c| c * c).sum();
    length_squared == 0.0 || (length_squared - 1.0).abs() < 1e-2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MeshSpec, QbinBuilder};

    fn read(spec: &MeshSpec) -> Result<(MeshData, MeshNotes), Unreadable> {
        let mut qbin = QbinBuilder::default();
        let offset = qbin.mesh(spec);
        Qbin::new(&qbin.bytes).read_mesh(u64::from_str_radix(&offset, 16).unwrap())
    }

    fn quad() -> MeshSpec {
        MeshSpec {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [2.0, 0.0, 0.0],
            ],
            colors: Some(vec![
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [0.5, 0.5, 0.5],
                [0.25, 0.5, 0.75],
            ]),
            normals: Some(vec![[0.0, 0.0, 1.0]; 5]),
            faces: vec![vec![0, 1, 2, 3], vec![1, 4, 2]],
        }
    }

    #[test]
    fn reads_positions_colours_normals_and_triangulates_quads() {
        let spec = quad();
        let (mesh, notes) = read(&spec).unwrap();
        assert!(notes.is_clean(), "{notes:?}");
        assert_eq!(mesh.positions, spec.positions);
        assert_eq!(Some(&mesh.colors), spec.colors.as_ref());
        assert_eq!(Some(&mesh.normals), spec.normals.as_ref());
        // The quad splits along its first diagonal; the triangle ignores its spare index.
        assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 3, 1, 4, 2]);
    }

    #[test]
    fn positions_alone_are_enough() {
        let spec = MeshSpec {
            colors: None,
            normals: None,
            ..quad()
        };
        let (mesh, notes) = read(&spec).unwrap();
        assert!(notes.is_clean());
        assert!(mesh.colors.is_empty() && mesh.normals.is_empty());
        assert_eq!(mesh.indices.len(), 9);
    }

    #[test]
    fn a_lone_extra_attribute_is_classified_by_its_values() {
        let normals_only = MeshSpec {
            colors: None,
            ..quad()
        };
        let (mesh, notes) = read(&normals_only).unwrap();
        assert!(notes.unexpected_layout);
        assert_eq!(mesh.normals.len(), 5);
        assert!(mesh.colors.is_empty());

        let colors_only = MeshSpec {
            normals: None,
            ..quad()
        };
        let (mesh, notes) = read(&colors_only).unwrap();
        assert!(notes.unexpected_layout);
        assert_eq!(mesh.colors.len(), 5);
        assert!(mesh.normals.is_empty());
    }

    #[test]
    fn bad_faces_and_vertices_are_dropped_and_counted() {
        let mut spec = quad();
        spec.positions[4] = [f32::NAN, 0.0, 0.0];
        spec.normals.as_mut().unwrap()[0] = [f32::INFINITY, 0.0, 0.0];
        spec.faces.push(vec![0, 1, 99]);
        spec.faces.push(vec![0, 1, 2, 3, 4]);
        spec.faces.push(vec![0, 1]);
        let (mesh, notes) = read(&spec).unwrap();
        assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 3]);
        assert_eq!(notes.damaged_vertices, 1);
        assert_eq!(notes.repaired_attributes, 1);
        // The triangle on the damaged vertex, the out-of-range one, the pentagon and the edge.
        assert_eq!(notes.skipped_faces, 4);
        assert_eq!(mesh.positions[4], [0.0; 3]);
        assert_eq!(mesh.normals[0], [0.0, 1.0, 0.0]);
    }

    #[test]
    fn truncated_or_foreign_records_are_errors_not_panics() {
        let mut qbin = QbinBuilder::default();
        let offset = u64::from_str_radix(&qbin.mesh(&quad()), 16).unwrap();
        let full = qbin.bytes.clone();
        assert!(Qbin::new(&full).read_mesh(offset).is_ok());
        for len in 0..full.len() {
            assert!(
                Qbin::new(&full[..len]).read_mesh(offset).is_err(),
                "a record cut at {len} bytes must not read"
            );
        }
        // Version, stream count, index depth.
        for (at, value) in [(8usize, 1u32), (36, 9), (36, 0)] {
            let mut bytes = full.clone();
            let at = offset as usize + at;
            bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
            assert!(Qbin::new(&bytes).read_mesh(offset).is_err());
        }
        assert!(Qbin::new(&full).read_mesh(u64::MAX).is_err());
    }

    #[test]
    fn point_clouds_keep_their_vertices() {
        let mut qbin = QbinBuilder::default();
        let offset = u64::from_str_radix(&qbin.mesh(&quad()), 16).unwrap();
        // Rewrite the face block as a point list: face type 1 and two 4-byte entries.
        let faces_at = qbin.bytes.len() - (12 + 4 + 2 * FACE_LEN);
        qbin.bytes.truncate(faces_at);
        for word in [1u32, 1, 1, 2, 0, 1] {
            qbin.bytes.extend_from_slice(&word.to_le_bytes());
        }
        let (mesh, notes) = Qbin::new(&qbin.bytes).read_mesh(offset).unwrap();
        assert!(notes.points_only);
        assert_eq!(mesh.positions.len(), 5);
        assert!(mesh.indices.is_empty());
    }
}
