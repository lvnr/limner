//! Strokes: a brush cross-section swept along stored points.

use crate::math::Aabb;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// The cross-section a stroke is drawn with. The numeric values are the ones Quill stores.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Brush {
    /// A legacy thin line (removed from Quill's interface in 2.0); drawn as a ribbon.
    Line = 0,
    Ribbon = 1,
    Cylinder = 2,
    Ellipse = 3,
    Cube = 4,
}

impl Brush {
    pub fn from_code(code: i32) -> Option<Self> {
        Some(match code {
            0 => Brush::Line,
            1 => Brush::Ribbon,
            2 => Brush::Cylinder,
            3 => Brush::Ellipse,
            4 => Brush::Cube,
            _ => return None,
        })
    }

    pub fn code(self) -> u8 {
        self as u8
    }

    /// Vertices in the cross-section ring generated at each point.
    pub fn ring_size(self) -> usize {
        match self {
            Brush::Line | Brush::Ribbon => 2,
            Brush::Cylinder | Brush::Ellipse => 7,
            Brush::Cube => 4,
        }
    }

    /// Flat strokes are visible from both sides; tubes only from outside.
    pub fn is_flat(self) -> bool {
        matches!(self, Brush::Line | Brush::Ribbon)
    }
}

/// One stored sample of a stroke. The layout matches Quill's 56-byte vertex, so
/// imported strokes are kept bit for bit.
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
#[repr(C)]
pub struct Point {
    /// Position in layer space, metres.
    pub pos: [f32; 3],
    /// Unit vector that orients the cross-section.
    pub normal: [f32; 3],
    /// Unit view direction at the moment of drawing; used by directional strokes.
    pub dir: [f32; 3],
    /// Linear RGB.
    pub color: [f32; 3],
    pub opacity: f32,
    /// Half-width in layer space.
    pub width: f32,
}

const _: () = assert!(std::mem::size_of::<Point>() == 56);

impl Point {
    pub fn position(&self) -> Vec3 {
        Vec3::from_array(self.pos)
    }
}

/// Opacity at or above this is treated as fully opaque.
pub const OPAQUE_THRESHOLD: f32 = 0.999;

#[derive(Clone, Debug)]
pub struct Stroke {
    /// Unique within its drawing. Seeds the dither pattern of see-through strokes.
    pub id: u32,
    pub brush: Brush,
    /// Fades as the viewing direction departs from the direction it was drawn from.
    pub directional: bool,
    points: Vec<Point>,
    bounds: OnceLock<Aabb>,
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.brush == other.brush
            && self.directional == other.directional
            && self.points == other.points
    }
}

impl Stroke {
    pub fn new(id: u32, brush: Brush, directional: bool, points: Vec<Point>) -> Self {
        Self {
            id,
            brush,
            directional,
            points,
            bounds: OnceLock::new(),
        }
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    /// Mutable access to the points. Cached bounds are discarded.
    pub fn points_mut(&mut self) -> &mut Vec<Point> {
        self.bounds = OnceLock::new();
        &mut self.points
    }

    pub fn into_points(self) -> Vec<Point> {
        self.points
    }

    pub fn len(&self) -> usize {
        self.points.len()
    }

    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Vertices this stroke expands to when tessellated.
    pub fn vertex_count(&self) -> usize {
        self.points.len() * self.brush.ring_size()
    }

    /// Bounds of the stroke including its width.
    pub fn bounds(&self) -> Aabb {
        *self.bounds.get_or_init(|| {
            let mut b = Aabb::EMPTY;
            for p in &self.points {
                b.extend_sphere(p.position(), p.width.abs());
            }
            b
        })
    }

    /// True when every point is fully opaque and the stroke is not directional,
    /// so it can be drawn without the see-through path.
    pub fn is_opaque(&self) -> bool {
        !self.directional && self.points.iter().all(|p| p.opacity >= OPAQUE_THRESHOLD)
    }

    /// True when no point is visible at all (for example after erasing).
    pub fn is_fully_erased(&self) -> bool {
        !self.points.is_empty() && self.points.iter().all(|p| p.opacity <= 0.0)
    }

    /// Length of the centre line.
    pub fn arc_length(&self) -> f32 {
        self.points
            .windows(2)
            .map(|w| w[0].position().distance(w[1].position()))
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn point(x: f32, y: f32, z: f32, width: f32) -> Point {
        Point {
            pos: [x, y, z],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, -1.0],
            color: [1.0, 0.5, 0.25],
            opacity: 1.0,
            width,
        }
    }

    #[test]
    fn point_layout_matches_quill() {
        let p = point(1.0, 2.0, 3.0, 0.5);
        let bytes: &[u8] = bytemuck::bytes_of(&p);
        assert_eq!(bytes.len(), 56);
        assert_eq!(&bytes[0..4], &1.0f32.to_le_bytes());
        assert_eq!(
            &bytes[36..40],
            &1.0f32.to_le_bytes(),
            "colour starts at byte 36"
        );
        assert_eq!(&bytes[48..52], &1.0f32.to_le_bytes(), "opacity at byte 48");
        assert_eq!(&bytes[52..56], &0.5f32.to_le_bytes(), "width at byte 52");
    }

    #[test]
    fn bounds_include_width_and_refresh_after_edits() {
        let mut s = Stroke::new(
            0,
            Brush::Cylinder,
            false,
            vec![point(0.0, 0.0, 0.0, 0.1), point(1.0, 0.0, 0.0, 0.2)],
        );
        let b = s.bounds();
        assert_eq!(b.min, Vec3::new(-0.1, -0.2, -0.2));
        assert_eq!(b.max, Vec3::new(1.2, 0.2, 0.2));
        s.points_mut().push(point(5.0, 0.0, 0.0, 0.0));
        assert_eq!(s.bounds().max.x, 5.0);
        assert_eq!(s.vertex_count(), 21);
    }

    #[test]
    fn opacity_classification() {
        let mut s = Stroke::new(0, Brush::Ribbon, false, vec![point(0.0, 0.0, 0.0, 0.1); 3]);
        assert!(s.is_opaque());
        s.points_mut()[1].opacity = 0.5;
        assert!(!s.is_opaque());
        for p in s.points_mut() {
            p.opacity = 0.0;
        }
        assert!(s.is_fully_erased());
        let directional = Stroke::new(1, Brush::Ribbon, true, vec![point(0.0, 0.0, 0.0, 0.1)]);
        assert!(!directional.is_opaque());
    }
}
