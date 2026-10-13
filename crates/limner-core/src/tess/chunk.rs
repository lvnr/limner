//! Turns a chunk of strokes into renderable triangle meshes.

use super::frame::{point_frame, ring_profile};
use crate::math::Aabb;
use crate::model::{Stroke, StrokeChunk, OPAQUE_THRESHOLD};
use glam::Vec3;

/// The four surfaces a chunk is split into. Tubes are single-sided and flat strokes
/// double-sided; opaque strokes take the fast path and the rest are dithered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SurfaceKind {
    TubeOpaque = 0,
    TubeSeeThrough = 1,
    FlatOpaque = 2,
    FlatSeeThrough = 3,
}

impl SurfaceKind {
    pub const ALL: [SurfaceKind; 4] = [
        SurfaceKind::TubeOpaque,
        SurfaceKind::TubeSeeThrough,
        SurfaceKind::FlatOpaque,
        SurfaceKind::FlatSeeThrough,
    ];

    pub fn of(stroke: &Stroke) -> Self {
        match (stroke.brush.is_flat(), stroke.is_opaque()) {
            (false, true) => SurfaceKind::TubeOpaque,
            (false, false) => SurfaceKind::TubeSeeThrough,
            (true, true) => SurfaceKind::FlatOpaque,
            (true, false) => SurfaceKind::FlatSeeThrough,
        }
    }

    pub fn is_flat(self) -> bool {
        matches!(self, SurfaceKind::FlatOpaque | SurfaceKind::FlatSeeThrough)
    }

    pub fn is_see_through(self) -> bool {
        matches!(
            self,
            SurfaceKind::TubeSeeThrough | SurfaceKind::FlatSeeThrough
        )
    }
}

/// Vertex and index data for one surface. Triangles are counter-clockwise when
/// seen from outside a tube, or from the side a ribbon's normal points to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SurfaceMesh {
    /// Positions relative to [`ChunkMesh::origin`].
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// sRGB-encoded colour and linear opacity.
    pub colors: Vec<[u8; 4]>,
    /// Draw-time view direction mapped to bytes (all zero when the stroke is not
    /// directional), and the stroke's dither seed.
    pub extra: Vec<[u8; 4]>,
    pub indices: Vec<u32>,
}

impl SurfaceMesh {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChunkMesh {
    /// A point near the middle of the chunk, in layer space. Vertices are stored
    /// relative to it so the renderer can add it in double precision.
    pub origin: [f32; 3],
    /// Bounds in layer space.
    pub bounds: Aabb,
    pub surfaces: [SurfaceMesh; 4],
}

impl ChunkMesh {
    pub fn surface(&self, kind: SurfaceKind) -> &SurfaceMesh {
        &self.surfaces[kind as usize]
    }

    pub fn vertex_count(&self) -> usize {
        self.surfaces.iter().map(SurfaceMesh::vertex_count).sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.surfaces.iter().map(SurfaceMesh::triangle_count).sum()
    }
}

/// Converts a linear colour component to an sRGB-encoded byte.
pub fn linear_to_srgb8(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0 + 0.5) as u8
}

fn unit_to_byte(v: f32) -> u8 {
    ((v.clamp(-1.0, 1.0) * 0.5 + 0.5) * 255.0 + 0.5) as u8
}

fn append_stroke(stroke: &Stroke, origin: Vec3, out: &mut SurfaceMesh) {
    let points = stroke.points();
    if points.len() < 2 {
        return;
    }
    let profile = ring_profile(stroke.brush);
    let ring = profile.len();
    let base = out.positions.len() as u32;
    let seed = (stroke.id & 0xff) as u8;

    for (i, point) in points.iter().enumerate() {
        let frame = point_frame(points, i);
        let centre = point.position() - origin;
        let color = [
            linear_to_srgb8(point.color[0]),
            linear_to_srgb8(point.color[1]),
            linear_to_srgb8(point.color[2]),
            (point.opacity.clamp(0.0, 1.0) * 255.0 + 0.5) as u8,
        ];
        let extra = if stroke.directional {
            [
                unit_to_byte(point.dir[0]),
                unit_to_byte(point.dir[1]),
                unit_to_byte(point.dir[2]),
                seed,
            ]
        } else {
            [0, 0, 0, seed]
        };
        for vertex in profile {
            let position = centre
                + frame.u * (vertex.offset[0] * point.width)
                + frame.v * (vertex.offset[1] * point.width);
            let normal = frame.u * vertex.normal[0] + frame.v * vertex.normal[1];
            out.positions.push(position.to_array());
            out.normals.push(normal.to_array());
            out.colors.push(color);
            out.extra.push(extra);
        }
    }

    // A ribbon has one strip between its two edges; a tube has one per side of the ring.
    let sides = if ring == 2 { 1 } else { ring };
    for i in 0..points.len() - 1 {
        let row = base + (i * ring) as u32;
        for k in 0..sides {
            let a = row + k as u32;
            let b = row + ((k + 1) % ring) as u32;
            let c = a + ring as u32;
            let d = b + ring as u32;
            out.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }
}

/// Tessellates every stroke of a chunk.
pub fn tessellate_chunk(chunk: &StrokeChunk) -> ChunkMesh {
    let bounds = chunk.bounds();
    let origin = if bounds.is_empty() {
        Vec3::ZERO
    } else {
        bounds.center()
    };
    let mut mesh = ChunkMesh {
        origin: origin.to_array(),
        bounds,
        surfaces: Default::default(),
    };
    for stroke in chunk.strokes() {
        append_stroke(
            stroke,
            origin,
            &mut mesh.surfaces[SurfaceKind::of(stroke) as usize],
        );
    }
    mesh
}

/// True when the whole chunk can be drawn without the see-through path.
pub fn chunk_is_opaque(chunk: &StrokeChunk) -> bool {
    chunk
        .strokes()
        .iter()
        .all(|s| s.points().iter().all(|p| p.opacity >= OPAQUE_THRESHOLD) && !s.directional)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Brush, Drawing, Point};

    fn pt(x: f32, width: f32, opacity: f32) -> Point {
        Point {
            pos: [x, 2.0, -1.0],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, -1.0],
            color: [1.0, 0.5, 0.0],
            opacity,
            width,
        }
    }

    fn straight(id: u32, brush: Brush, n: usize) -> Stroke {
        Stroke::new(
            id,
            brush,
            false,
            (0..n)
                .map(|i| pt(10.0 + i as f32 * 0.1, 0.05, 1.0))
                .collect(),
        )
    }

    fn chunk_of(strokes: Vec<Stroke>) -> std::sync::Arc<StrokeChunk> {
        Drawing::from_strokes(strokes).chunks()[0].clone()
    }

    #[test]
    fn vertex_and_triangle_counts_per_brush() {
        for (brush, ring, sides) in [
            (Brush::Ribbon, 2, 1),
            (Brush::Line, 2, 1),
            (Brush::Cylinder, 7, 7),
            (Brush::Ellipse, 7, 7),
            (Brush::Cube, 4, 4),
        ] {
            let mesh = tessellate_chunk(&chunk_of(vec![straight(0, brush, 5)]));
            assert_eq!(mesh.vertex_count(), 5 * ring, "{brush:?}");
            assert_eq!(mesh.triangle_count(), 4 * sides * 2, "{brush:?}");
            let kind = if brush.is_flat() {
                SurfaceKind::FlatOpaque
            } else {
                SurfaceKind::TubeOpaque
            };
            assert_eq!(mesh.surface(kind).vertex_count(), 5 * ring);
        }
    }

    #[test]
    fn positions_are_relative_to_the_chunk_origin() {
        let stroke = straight(0, Brush::Cylinder, 3);
        let mesh = tessellate_chunk(&chunk_of(vec![stroke.clone()]));
        let origin = Vec3::from_array(mesh.origin);
        assert!((origin - stroke.bounds().center()).length() < 1e-6);
        let surface = mesh.surface(SurfaceKind::TubeOpaque);
        // Stored coordinates are small even though the stroke sits ten metres from the layer origin.
        assert!(surface
            .positions
            .iter()
            .all(|p| Vec3::from_array(*p).length() < 0.5));
        // First ring is centred on the first point.
        let centre: Vec3 = surface.positions[..7]
            .iter()
            .map(|p| Vec3::from_array(*p))
            .sum::<Vec3>()
            / 7.0
            + origin;
        assert!((centre - stroke.points()[0].position()).length() < 1e-5);
    }

    #[test]
    fn tube_triangles_face_outward_counter_clockwise() {
        for brush in [Brush::Cylinder, Brush::Ellipse, Brush::Cube] {
            let stroke = straight(0, brush, 4);
            let mesh = tessellate_chunk(&chunk_of(vec![stroke.clone()]));
            let origin = Vec3::from_array(mesh.origin);
            let surface = mesh.surface(SurfaceKind::TubeOpaque);
            for tri in surface.indices.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2]
                    .map(|i| Vec3::from_array(surface.positions[tri[i] as usize]) + origin);
                let face_normal = (b - a).cross(c - a);
                let centroid = (a + b + c) / 3.0;
                // The stroke runs along X at y = 2, z = -1.
                let outward = Vec3::new(0.0, centroid.y - 2.0, centroid.z + 1.0);
                assert!(face_normal.dot(outward) > 0.0, "{brush:?}");
            }
            // Vertex normals point away from the centre line too.
            for (p, n) in surface.positions.iter().zip(&surface.normals) {
                let p = Vec3::from_array(*p) + origin;
                assert!(Vec3::from_array(*n).dot(Vec3::new(0.0, p.y - 2.0, p.z + 1.0)) > 0.0);
            }
        }
    }

    #[test]
    fn ribbon_faces_along_its_stored_normal() {
        let mesh = tessellate_chunk(&chunk_of(vec![straight(0, Brush::Ribbon, 3)]));
        let surface = mesh.surface(SurfaceKind::FlatOpaque);
        for tri in surface.indices.as_chunks::<3>().0 {
            let [a, b, c] = [0, 1, 2].map(|i| Vec3::from_array(surface.positions[tri[i] as usize]));
            assert!((b - a).cross(c - a).normalize().dot(Vec3::Y) > 0.999);
        }
        assert!(surface
            .normals
            .iter()
            .all(|n| (Vec3::from_array(*n) - Vec3::Y).length() < 1e-5));
    }

    #[test]
    fn strokes_are_sorted_into_surfaces() {
        let mut faint = straight(1, Brush::Cylinder, 3);
        faint.points_mut()[1].opacity = 0.4;
        let directional = Stroke::new(
            2,
            Brush::Ribbon,
            true,
            straight(2, Brush::Ribbon, 3).into_points(),
        );
        let chunk = chunk_of(vec![
            straight(0, Brush::Cylinder, 3),
            faint,
            directional,
            straight(3, Brush::Ribbon, 3),
        ]);
        assert!(!chunk_is_opaque(&chunk));
        let mesh = tessellate_chunk(&chunk);
        assert_eq!(mesh.surface(SurfaceKind::TubeOpaque).vertex_count(), 21);
        assert_eq!(mesh.surface(SurfaceKind::TubeSeeThrough).vertex_count(), 21);
        assert_eq!(mesh.surface(SurfaceKind::FlatSeeThrough).vertex_count(), 6);
        assert_eq!(mesh.surface(SurfaceKind::FlatOpaque).vertex_count(), 6);
        // The see-through tube carries its opacity and seed.
        let see_through = mesh.surface(SurfaceKind::TubeSeeThrough);
        assert_eq!(see_through.colors[7][3], 102);
        assert_eq!(see_through.extra[0], [0, 0, 0, 1]);
        // The directional ribbon carries its view direction (0, 0, -1).
        assert_eq!(
            mesh.surface(SurfaceKind::FlatSeeThrough).extra[0],
            [128, 128, 0, 2]
        );
        assert!(chunk_is_opaque(&chunk_of(vec![straight(
            0,
            Brush::Cube,
            3
        )])));
    }

    #[test]
    fn colours_are_srgb_encoded() {
        assert_eq!(linear_to_srgb8(0.0), 0);
        assert_eq!(linear_to_srgb8(1.0), 255);
        assert_eq!(linear_to_srgb8(0.5), 188);
        assert_eq!(linear_to_srgb8(0.214_041_14), 128);
        assert_eq!(linear_to_srgb8(7.0), 255);
        let mesh = tessellate_chunk(&chunk_of(vec![straight(0, Brush::Ribbon, 2)]));
        assert_eq!(
            mesh.surface(SurfaceKind::FlatOpaque).colors[0],
            [255, 188, 0, 255]
        );
    }

    #[test]
    fn zero_width_ends_collapse_to_a_point_and_short_strokes_are_skipped() {
        let mut stroke = straight(0, Brush::Cylinder, 3);
        stroke.points_mut()[0].width = 0.0;
        let mesh = tessellate_chunk(&chunk_of(vec![stroke, straight(1, Brush::Cylinder, 1)]));
        let surface = mesh.surface(SurfaceKind::TubeOpaque);
        assert_eq!(
            surface.vertex_count(),
            21,
            "a one-point stroke makes no geometry"
        );
        let first = surface.positions[0];
        assert!(surface.positions[..7].iter().all(|p| *p == first));
    }
}
