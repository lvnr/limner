//! Operations on strokes that are shared by several tools and commands.

use crate::math::Xform;
use crate::model::{Drawing, Point, Stroke};
use glam::DVec3;

/// Applies a layer transform to a point's stored vectors, in double precision.
pub fn transform_point(p: &Point, x: &Xform) -> Point {
    let position = x.transform_point(DVec3::from_array(p.pos.map(f64::from)));
    let direction = |v: [f32; 3]| {
        x.transform_vector(DVec3::from_array(v.map(f64::from)))
            .normalize_or_zero()
            .as_vec3()
            .to_array()
    };
    Point {
        pos: position.as_vec3().to_array(),
        // The normal goes through the same linear map as positions, mirror included.
        // That keeps tubes facing outward after a mirrored transform is baked in: the
        // cross-section frame flips handedness together with the geometry.
        normal: direction(p.normal),
        dir: direction(p.dir),
        color: p.color,
        opacity: p.opacity,
        width: (p.width as f64 * x.scale.abs()) as f32,
    }
}

/// A copy of `stroke` with `x` baked into its points.
pub fn transformed_stroke(stroke: &Stroke, x: &Xform) -> Stroke {
    Stroke::new(
        stroke.id,
        stroke.brush,
        stroke.directional,
        stroke
            .points()
            .iter()
            .map(|p| transform_point(p, x))
            .collect(),
    )
}

/// A copy of `drawing` with `x` baked into every stroke.
pub fn transformed_drawing(drawing: &Drawing, x: &Xform) -> Drawing {
    if x.is_identity() {
        return drawing.clone();
    }
    Drawing::from_strokes(drawing.strokes().map(|s| transformed_stroke(s, x)))
}

/// Appends the strokes of `source` to `target`, baking `x` into them and giving them
/// ids that do not clash with strokes already in `target`.
pub fn append_transformed(target: &mut Drawing, source: &Drawing, x: &Xform) {
    let mut next_id = target.next_stroke_id();
    for stroke in source.strokes() {
        let mut copy = if x.is_identity() {
            (**stroke).clone()
        } else {
            transformed_stroke(stroke, x)
        };
        copy.id = next_id;
        next_id = next_id.wrapping_add(1);
        target.push_stroke(copy);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Flip;
    use crate::model::Brush;
    use crate::tess::{tessellate_chunk, SurfaceKind};
    use glam::{DQuat, Vec3};

    fn tube() -> Stroke {
        let p = |x: f32| Point {
            pos: [x, 0.2, 0.0],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, -1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.05,
        };
        Stroke::new(
            4,
            Brush::Cylinder,
            false,
            (0..5).map(|i| p(i as f32 * 0.1)).collect(),
        )
    }

    #[test]
    fn points_follow_the_transform() {
        let x = Xform {
            rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
            scale: 2.0,
            translation: DVec3::new(1.0, 0.0, 0.0),
            flip: Flip::None,
        };
        let moved = transformed_stroke(&tube(), &x);
        let first = moved.points()[0];
        // (0, 0.2, 0) scaled by two and turned a quarter about Z is (-0.4, 0, 0), then shifted by one.
        assert!((Vec3::from_array(first.pos) - Vec3::new(0.6, 0.0, 0.0)).length() < 1e-6);
        assert!((Vec3::from_array(first.normal) - Vec3::new(-1.0, 0.0, 0.0)).length() < 1e-6);
        assert!((first.width - 0.1).abs() < 1e-7);
        assert_eq!(moved.id, 4);
    }

    #[test]
    fn a_baked_mirror_keeps_tubes_facing_outward() {
        for flip in [Flip::X, Flip::Y, Flip::Z] {
            let x = Xform {
                rotation: DQuat::from_rotation_y(0.7),
                scale: 1.5,
                translation: DVec3::new(0.3, -1.0, 2.0),
                flip,
            };
            let source = tube();
            let mirrored = transformed_stroke(&source, &x);
            let drawing = Drawing::from_strokes([mirrored.clone()]);
            let mesh = tessellate_chunk(&drawing.chunks()[0]);
            let surface = mesh.surface(SurfaceKind::TubeOpaque);
            let origin = Vec3::from_array(mesh.origin);
            // The centre line of the mirrored tube.
            let a = Vec3::from_array(mirrored.points()[0].pos);
            let b = Vec3::from_array(mirrored.points()[4].pos);
            let axis = (b - a).normalize();
            for tri in surface.indices.as_chunks::<3>().0 {
                let [p, q, r] = [0, 1, 2]
                    .map(|i| Vec3::from_array(surface.positions[tri[i] as usize]) + origin);
                let centroid = (p + q + r) / 3.0;
                let outward = (centroid - a) - axis * (centroid - a).dot(axis);
                assert!((q - p).cross(r - p).dot(outward) > 0.0, "{flip:?}");
            }
        }
    }

    #[test]
    fn appended_strokes_get_fresh_ids() {
        let mut target = Drawing::from_strokes([tube()]);
        let source = Drawing::from_strokes([tube(), tube()]);
        append_transformed(&mut target, &source, &Xform::from_translation(DVec3::Y));
        let ids: Vec<u32> = target.strokes().map(|s| s.id).collect();
        assert_eq!(ids, vec![4, 5, 6]);
        assert!((target.strokes().nth(1).unwrap().points()[0].pos[1] - 1.2).abs() < 1e-6);
    }
}
