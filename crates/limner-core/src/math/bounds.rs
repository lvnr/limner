//! Axis-aligned bounding boxes in single precision (layer space).

use glam::{DAffine3, DVec3, Vec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl Aabb {
    /// A box that contains nothing; extending it with a point yields that point.
    pub const EMPTY: Self = Self {
        min: Vec3::INFINITY,
        max: Vec3::NEG_INFINITY,
    };

    pub fn new(min: Vec3, max: Vec3) -> Self {
        Self { min, max }
    }

    pub fn is_empty(&self) -> bool {
        self.min.x > self.max.x || self.min.y > self.max.y || self.min.z > self.max.z
    }

    pub fn extend(&mut self, p: Vec3) {
        self.min = self.min.min(p);
        self.max = self.max.max(p);
    }

    /// Grows the box to contain a sphere.
    pub fn extend_sphere(&mut self, center: Vec3, radius: f32) {
        self.min = self.min.min(center - Vec3::splat(radius));
        self.max = self.max.max(center + Vec3::splat(radius));
    }

    pub fn union(&self, other: &Aabb) -> Aabb {
        if other.is_empty() {
            return *self;
        }
        if self.is_empty() {
            return *other;
        }
        Aabb {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }

    pub fn center(&self) -> Vec3 {
        (self.min + self.max) * 0.5
    }

    pub fn size(&self) -> Vec3 {
        if self.is_empty() {
            Vec3::ZERO
        } else {
            self.max - self.min
        }
    }

    pub fn contains(&self, p: Vec3) -> bool {
        p.cmpge(self.min).all() && p.cmple(self.max).all()
    }

    pub fn intersects_sphere(&self, center: Vec3, radius: f32) -> bool {
        if self.is_empty() {
            return false;
        }
        let nearest = center.clamp(self.min, self.max);
        nearest.distance_squared(center) <= radius * radius
    }

    pub fn corners(&self) -> [Vec3; 8] {
        let (a, b) = (self.min, self.max);
        [
            Vec3::new(a.x, a.y, a.z),
            Vec3::new(b.x, a.y, a.z),
            Vec3::new(a.x, b.y, a.z),
            Vec3::new(b.x, b.y, a.z),
            Vec3::new(a.x, a.y, b.z),
            Vec3::new(b.x, a.y, b.z),
            Vec3::new(a.x, b.y, b.z),
            Vec3::new(b.x, b.y, b.z),
        ]
    }

    /// The box that contains this box after a transform, as double-precision min and max.
    pub fn transformed(&self, m: &DAffine3) -> Option<(DVec3, DVec3)> {
        if self.is_empty() {
            return None;
        }
        let mut min = DVec3::INFINITY;
        let mut max = DVec3::NEG_INFINITY;
        for corner in self.corners() {
            let p = m.transform_point3(corner.as_dvec3());
            min = min.min(p);
            max = max.max(p);
        }
        Some((min, max))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_box_behaves() {
        let mut b = Aabb::EMPTY;
        assert!(b.is_empty());
        assert!(!b.intersects_sphere(Vec3::ZERO, 10.0));
        b.extend(Vec3::new(1.0, 2.0, 3.0));
        assert!(!b.is_empty());
        assert_eq!(b.min, b.max);
        assert_eq!(Aabb::EMPTY.union(&b), b);
        assert_eq!(b.union(&Aabb::EMPTY), b);
    }

    #[test]
    fn sphere_tests() {
        let b = Aabb::new(Vec3::ZERO, Vec3::ONE);
        assert!(b.intersects_sphere(Vec3::new(1.5, 0.5, 0.5), 0.6));
        assert!(!b.intersects_sphere(Vec3::new(1.5, 0.5, 0.5), 0.4));
        assert!(b.intersects_sphere(Vec3::splat(0.5), 0.01));
        let mut c = Aabb::EMPTY;
        c.extend_sphere(Vec3::ZERO, 2.0);
        assert_eq!(c.size(), Vec3::splat(4.0));
    }
}
