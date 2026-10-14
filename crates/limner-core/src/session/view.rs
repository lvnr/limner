//! The viewer's placement in the world.
//!
//! "Viewer space" is the room the user stands in: metres, Y up, the same on the
//! desktop and in a headset. Navigation never moves the user; it changes where the
//! room sits in the world, and at what scale.

use crate::math::Xform;
use glam::{DAffine3, DQuat, DVec3};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// Maps viewer space to world space. Its scale is the size of the viewer in the world.
    pub world_from_viewer: Xform,
}

impl Default for View {
    fn default() -> Self {
        Self {
            world_from_viewer: Xform::IDENTITY,
        }
    }
}

impl View {
    pub fn viewer_from_world(&self) -> Xform {
        self.world_from_viewer.inverse()
    }

    pub fn viewer_from_world_affine(&self) -> DAffine3 {
        self.viewer_from_world().to_affine()
    }

    /// How many world units one viewer-space metre covers.
    pub fn scale(&self) -> f64 {
        self.world_from_viewer.scale
    }

    /// Moves the world as seen by the viewer: `motion` maps where things were in
    /// viewer space to where they should be now.
    pub fn apply_motion(&mut self, motion: &Xform) {
        let mut next = self.world_from_viewer.mul(&motion.inverse());
        next.rotation = next.rotation.normalize();
        if next.is_finite() && next.scale > 0.0 {
            self.world_from_viewer = next;
        }
    }

    /// Slides the world by `delta` viewer-space metres.
    pub fn pan(&mut self, delta: DVec3) {
        self.apply_motion(&Xform::from_translation(delta));
    }

    /// Turns the world about a viewer-space point.
    pub fn rotate_about(&mut self, pivot: DVec3, rotation: DQuat) {
        self.apply_motion(&about(pivot, &Xform::from_rotation(rotation)));
    }

    /// Scales the world about a viewer-space point. A factor above one makes the world
    /// look bigger. The resulting viewer scale is kept within `limits` (world units per metre).
    pub fn zoom_about(&mut self, pivot: DVec3, factor: f64, limits: (f64, f64)) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let current = self.world_from_viewer.scale;
        let target = (current / factor).clamp(limits.0, limits.1);
        let factor = current / target;
        self.apply_motion(&about(pivot, &Xform::from_scale(factor)));
    }

    /// Turns the world about a viewer-space point until its up is the viewer's up.
    pub fn level(&mut self, pivot: DVec3) {
        let up = self.viewer_from_world().rotation * DVec3::Y;
        if let Some(up) = up.try_normalize() {
            self.rotate_about(pivot, DQuat::from_rotation_arc(up, DVec3::Y));
        }
    }

    /// Places the viewer so the world-space sphere (`center`, `radius`) appears as the
    /// viewer-space sphere (`target`, `target_radius`), keeping the current orientation.
    pub fn frame_sphere(&mut self, center: DVec3, radius: f64, target: DVec3, target_radius: f64) {
        if !(radius.is_finite() && radius > 0.0 && target_radius > 0.0) {
            return;
        }
        let mut viewer_from_world = self.viewer_from_world();
        viewer_from_world.scale = target_radius / radius;
        viewer_from_world.translation = DVec3::ZERO;
        viewer_from_world.translation = target - viewer_from_world.transform_point(center);
        self.world_from_viewer = viewer_from_world.inverse();
    }
}

/// The transform `inner` applied about `pivot` instead of the origin.
pub(crate) fn about(pivot: DVec3, inner: &Xform) -> Xform {
    Xform::from_translation(pivot)
        .mul(inner)
        .mul(&Xform::from_translation(-pivot))
}

/// The limits on viewer scale: from a millionth of a metre per metre to a million.
pub const DEFAULT_ZOOM_LIMITS: (f64, f64) = (1e-9, 1e9);

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: DVec3, b: DVec3) -> bool {
        (a - b).length() < 1e-9 * (1.0 + a.length().max(b.length()))
    }

    #[test]
    fn pan_moves_the_world_in_viewer_space() {
        let mut view = View::default();
        let world_point = DVec3::new(1.0, 2.0, 3.0);
        view.pan(DVec3::new(0.5, 0.0, 0.0));
        assert!(close(
            view.viewer_from_world().transform_point(world_point),
            DVec3::new(1.5, 2.0, 3.0)
        ));
    }

    #[test]
    fn zoom_keeps_the_pivot_fixed_and_scales_everything_else() {
        let mut view = View {
            world_from_viewer: Xform {
                rotation: DQuat::from_rotation_y(0.4),
                scale: 2.0,
                translation: DVec3::new(5.0, 0.0, -3.0),
                flip: crate::math::Flip::None,
            },
        };
        let pivot = DVec3::new(0.0, 1.5, -1.0);
        let pivot_world = view.world_from_viewer.transform_point(pivot);
        let other_world = view
            .world_from_viewer
            .transform_point(DVec3::new(0.3, 1.5, -1.0));
        view.zoom_about(pivot, 4.0, DEFAULT_ZOOM_LIMITS);
        assert!(close(
            view.viewer_from_world().transform_point(pivot_world),
            pivot
        ));
        // A point that was 0.3 m from the pivot is now 1.2 m from it.
        assert!(close(
            view.viewer_from_world().transform_point(other_world),
            DVec3::new(1.2, 1.5, -1.0)
        ));
        assert!((view.scale() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn zoom_respects_its_limits() {
        let mut view = View::default();
        view.zoom_about(DVec3::ZERO, 1e30, (0.01, 100.0));
        assert!((view.scale() - 0.01).abs() < 1e-12);
        view.zoom_about(DVec3::ZERO, 1e-30, (0.01, 100.0));
        assert!((view.scale() - 100.0).abs() < 1e-9);
        view.zoom_about(DVec3::ZERO, f64::NAN, (0.01, 100.0));
        assert!(view.world_from_viewer.is_finite());
    }

    #[test]
    fn rotation_about_a_pivot_keeps_it_fixed() {
        let mut view = View::default();
        let pivot = DVec3::new(0.0, 1.5, -1.0);
        view.rotate_about(pivot, DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2));
        assert!(close(
            view.viewer_from_world().transform_point(pivot),
            pivot
        ));
        // A world point one metre to the right of the pivot swings to one metre in front of it.
        let moved = view.viewer_from_world().transform_point(pivot + DVec3::X);
        assert!(close(moved, pivot + DVec3::NEG_Z), "{moved:?}");
    }

    #[test]
    fn framing_maps_one_sphere_onto_another() {
        let mut view = View {
            world_from_viewer: Xform::from_rotation(DQuat::from_rotation_y(1.0)),
        };
        let center = DVec3::new(100.0, -20.0, 7.0);
        view.frame_sphere(center, 50.0, DVec3::new(0.0, 1.4, -1.2), 0.5);
        let v = view.viewer_from_world();
        assert!(close(v.transform_point(center), DVec3::new(0.0, 1.4, -1.2)));
        assert!((v.transform_vector(DVec3::X).length() - 0.01).abs() < 1e-12);
        assert!(
            (view
                .world_from_viewer
                .rotation
                .dot(DQuat::from_rotation_y(1.0))
                .abs()
                - 1.0)
                .abs()
                < 1e-12,
            "orientation is kept"
        );
    }
}
