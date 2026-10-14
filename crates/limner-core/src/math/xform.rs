//! The layer transform: rotation, uniform scale, an optional mirror on one axis, translation.
//!
//! This is the transform Quill stores on every layer. A point maps as
//! `v' = rotation * (scale * flip * v) + translation`. Composition, inversion and
//! interpolation stay inside this family, so keys can be blended component-wise
//! instead of through matrices.

use glam::{DAffine3, DMat3, DQuat, DVec3};
use serde::{Deserialize, Serialize};

/// Mirror applied before rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Flip {
    #[default]
    None,
    X,
    Y,
    Z,
}

impl Flip {
    /// The mirror as per-axis signs.
    pub fn signs(self) -> DVec3 {
        match self {
            Flip::None => DVec3::ONE,
            Flip::X => DVec3::new(-1.0, 1.0, 1.0),
            Flip::Y => DVec3::new(1.0, -1.0, 1.0),
            Flip::Z => DVec3::new(1.0, 1.0, -1.0),
        }
    }

    pub fn is_mirrored(self) -> bool {
        self != Flip::None
    }

    /// The rotation `F * q * F` for this mirror `F`. Conjugating a rotation by a
    /// mirror gives the rotation about the mirrored axis by the opposite angle.
    fn conjugate(self, q: DQuat) -> DQuat {
        match self {
            Flip::None => q,
            Flip::X => DQuat::from_xyzw(q.x, -q.y, -q.z, q.w),
            Flip::Y => DQuat::from_xyzw(-q.x, q.y, -q.z, q.w),
            Flip::Z => DQuat::from_xyzw(-q.x, -q.y, q.z, q.w),
        }
    }

    /// The product of two mirrors: identity when they are equal, otherwise a half
    /// turn about the remaining axis. Returned as a rotation.
    fn product_rotation(a: Flip, b: Flip) -> DQuat {
        use Flip::*;
        match (a, b) {
            (X, Y) | (Y, X) => DQuat::from_xyzw(0.0, 0.0, 1.0, 0.0),
            (X, Z) | (Z, X) => DQuat::from_xyzw(0.0, 1.0, 0.0, 0.0),
            (Y, Z) | (Z, Y) => DQuat::from_xyzw(1.0, 0.0, 0.0, 0.0),
            _ => DQuat::IDENTITY,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Xform {
    pub rotation: DQuat,
    pub scale: f64,
    pub translation: DVec3,
    #[serde(default)]
    pub flip: Flip,
}

impl Default for Xform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Xform {
    pub const IDENTITY: Self = Self {
        rotation: DQuat::IDENTITY,
        scale: 1.0,
        translation: DVec3::ZERO,
        flip: Flip::None,
    };

    pub fn new(rotation: DQuat, scale: f64, translation: DVec3, flip: Flip) -> Self {
        Self {
            rotation,
            scale,
            translation,
            flip,
        }
    }

    pub fn from_translation(translation: DVec3) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    pub fn from_rotation(rotation: DQuat) -> Self {
        Self {
            rotation,
            ..Self::IDENTITY
        }
    }

    pub fn from_scale(scale: f64) -> Self {
        Self {
            scale,
            ..Self::IDENTITY
        }
    }

    pub fn from_flip(flip: Flip) -> Self {
        Self {
            flip,
            ..Self::IDENTITY
        }
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    pub fn is_finite(&self) -> bool {
        self.rotation.is_finite() && self.scale.is_finite() && self.translation.is_finite()
    }

    pub fn transform_point(&self, p: DVec3) -> DVec3 {
        self.rotation * (self.scale * self.flip.signs() * p) + self.translation
    }

    /// Transforms a direction or offset: rotation, scale and mirror, no translation.
    pub fn transform_vector(&self, v: DVec3) -> DVec3 {
        self.rotation * (self.scale * self.flip.signs() * v)
    }

    pub fn to_affine(&self) -> DAffine3 {
        let linear =
            DMat3::from_quat(self.rotation) * DMat3::from_diagonal(self.flip.signs() * self.scale);
        DAffine3::from_mat3_translation(linear, self.translation)
    }

    /// Recovers a transform from a matrix made of rotation, uniform scale, translation
    /// and at most one mirror. A mirrored matrix comes back with `Flip::X`. Any other
    /// matrix gives the nearest such transform: its scale is the mean of its axes'.
    pub fn from_affine(m: &DAffine3) -> Self {
        let mut linear = m.matrix3;
        let mut flip = Flip::None;
        if linear.determinant() < 0.0 {
            linear *= DMat3::from_diagonal(Flip::X.signs());
            flip = Flip::X;
        }
        let scale =
            (linear.x_axis.length() + linear.y_axis.length() + linear.z_axis.length()) / 3.0;
        // The axes are made orthonormal first: a matrix with uneven scale or shear has
        // no exact transform of this kind, and gets the nearest one.
        let x = linear.x_axis.normalize_or(DVec3::X);
        let y = (linear.y_axis - x * linear.y_axis.dot(x))
            .try_normalize()
            .unwrap_or_else(|| x.any_orthonormal_vector());
        let rotation = if scale > 0.0 && scale.is_finite() {
            DQuat::from_mat3(&DMat3::from_cols(x, y, x.cross(y))).normalize()
        } else {
            DQuat::IDENTITY
        };
        Self {
            rotation,
            scale,
            translation: m.translation,
            flip,
        }
    }

    /// `self` applied after `rhs`: `(self * rhs)(v) == self(rhs(v))`.
    pub fn mul(&self, rhs: &Xform) -> Xform {
        let translation = self.transform_point(rhs.translation);
        let scale = self.scale * rhs.scale;
        let (rotation, flip) = if !self.flip.is_mirrored() {
            (self.rotation * rhs.rotation, rhs.flip)
        } else if !rhs.flip.is_mirrored() {
            (self.rotation * self.flip.conjugate(rhs.rotation), self.flip)
        } else {
            // Two mirrors make a rotation.
            (
                self.rotation
                    * self.flip.conjugate(rhs.rotation)
                    * Flip::product_rotation(self.flip, rhs.flip),
                Flip::None,
            )
        };
        Xform {
            rotation: rotation.normalize(),
            scale,
            translation,
            flip,
        }
    }

    pub fn inverse(&self) -> Xform {
        let inv_rotation = self.rotation.inverse();
        let inv_scale = 1.0 / self.scale;
        let translation = -(self.flip.signs() * inv_scale * (inv_rotation * self.translation));
        Xform {
            rotation: self.flip.conjugate(inv_rotation),
            scale: inv_scale,
            translation,
            flip: self.flip,
        }
    }

    /// Blends two transforms: shortest-arc slerp for rotation, linear for scale and
    /// translation. The mirror is taken from `a`, as it cannot be blended.
    pub fn mix(a: &Xform, b: &Xform, t: f64) -> Xform {
        Xform {
            rotation: slerp_shortest(a.rotation, b.rotation, t),
            scale: a.scale + (b.scale - a.scale) * t,
            translation: a.translation.lerp(b.translation, t),
            flip: a.flip,
        }
    }

    /// Blends two layer transforms so the layer turns about its pivot instead of its origin.
    pub fn mix_about_pivot(a: &Xform, b: &Xform, pivot: &Xform, t: f64) -> Xform {
        Xform::mix(&a.mul(pivot), &b.mul(pivot), t).mul(&pivot.inverse())
    }
}

impl std::ops::Mul for Xform {
    type Output = Xform;
    fn mul(self, rhs: Xform) -> Xform {
        Xform::mul(&self, &rhs)
    }
}

fn slerp_shortest(a: DQuat, b: DQuat, t: f64) -> DQuat {
    let mut cos_half = a.dot(b);
    let mut b = b;
    if cos_half < 0.0 {
        cos_half = -cos_half;
        b = -b;
    }
    let (wa, wb) = if 1.0 - cos_half > 1e-6 {
        let half = cos_half.clamp(-1.0, 1.0).acos();
        let sin_half = half.sin();
        (
            ((1.0 - t) * half).sin() / sin_half,
            (t * half).sin() / sin_half,
        )
    } else {
        (1.0 - t, t)
    };
    (a * wa + b * wb).normalize()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Small deterministic generator so the tests need no extra dependency.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
        fn range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
        fn xform(&mut self, flip: Flip) -> Xform {
            let axis = DVec3::new(
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
                self.range(-1.0, 1.0),
            )
            .normalize_or(DVec3::X);
            Xform {
                rotation: DQuat::from_axis_angle(axis, self.range(-3.1, 3.1)),
                scale: self.range(0.05, 20.0),
                translation: DVec3::new(
                    self.range(-50.0, 50.0),
                    self.range(-50.0, 50.0),
                    self.range(-50.0, 50.0),
                ),
                flip,
            }
        }
        fn point(&mut self) -> DVec3 {
            DVec3::new(
                self.range(-5.0, 5.0),
                self.range(-5.0, 5.0),
                self.range(-5.0, 5.0),
            )
        }
    }

    const FLIPS: [Flip; 4] = [Flip::None, Flip::X, Flip::Y, Flip::Z];

    fn assert_close(a: DVec3, b: DVec3, what: &str) {
        let tolerance = 1e-9 * (1.0 + a.length().max(b.length()));
        assert!((a - b).length() < tolerance, "{what}: {a:?} vs {b:?}");
    }

    #[test]
    fn transform_point_matches_matrix() {
        let mut rng = Rng(1);
        for flip in FLIPS {
            for _ in 0..50 {
                let x = rng.xform(flip);
                let p = rng.point();
                assert_close(
                    x.transform_point(p),
                    x.to_affine().transform_point3(p),
                    "point",
                );
                assert_close(
                    x.transform_vector(p),
                    x.to_affine().transform_vector3(p),
                    "vector",
                );
            }
        }
    }

    #[test]
    fn composition_matches_matrix_composition_for_every_flip_pair() {
        let mut rng = Rng(2);
        for fa in FLIPS {
            for fb in FLIPS {
                for _ in 0..40 {
                    let a = rng.xform(fa);
                    let b = rng.xform(fb);
                    let p = rng.point();
                    let composed = a.mul(&b);
                    assert_close(
                        composed.transform_point(p),
                        (a.to_affine() * b.to_affine()).transform_point3(p),
                        "compose",
                    );
                    assert_close(
                        composed.transform_point(p),
                        a.transform_point(b.transform_point(p)),
                        "nested",
                    );
                    // Two mirrors cancel into a rotation; one mirror survives.
                    let expected_flip = match (fa.is_mirrored(), fb.is_mirrored()) {
                        (false, _) => fb,
                        (true, false) => fa,
                        (true, true) => Flip::None,
                    };
                    assert_eq!(composed.flip, expected_flip);
                }
            }
        }
    }

    #[test]
    fn inverse_undoes_the_transform() {
        let mut rng = Rng(3);
        for flip in FLIPS {
            for _ in 0..50 {
                let x = rng.xform(flip);
                let p = rng.point();
                assert_close(
                    x.inverse().transform_point(x.transform_point(p)),
                    p,
                    "inverse then forward",
                );
                assert_close(x.mul(&x.inverse()).transform_point(p), p, "x * inverse");
                assert_close(x.inverse().mul(&x).transform_point(p), p, "inverse * x");
            }
        }
    }

    #[test]
    fn from_affine_round_trips() {
        let mut rng = Rng(4);
        for flip in FLIPS {
            for _ in 0..50 {
                let x = rng.xform(flip);
                let back = Xform::from_affine(&x.to_affine());
                let p = rng.point();
                assert_close(back.transform_point(p), x.transform_point(p), "from_affine");
                assert!((back.scale - x.scale).abs() < 1e-9 * x.scale);
                assert_eq!(back.flip.is_mirrored(), flip.is_mirrored());
            }
        }
    }

    #[test]
    fn mix_hits_the_end_points_and_takes_the_short_way() {
        let a = Xform::from_rotation(DQuat::from_rotation_y(0.2));
        // The same orientation as a 0.6 rad turn, written with the opposite quaternion sign.
        let b = Xform {
            rotation: -DQuat::from_rotation_y(0.6),
            flip: Flip::Z,
            ..Xform::IDENTITY
        };
        let half = Xform::mix(&a, &b, 0.5);
        let expected = DQuat::from_rotation_y(0.4);
        assert!(half.rotation.dot(expected).abs() > 1.0 - 1e-12);
        assert_eq!(half.flip, Flip::None, "mirror comes from the earlier key");
        let p = DVec3::new(1.0, 2.0, 3.0);
        assert_close(
            Xform::mix(&a, &b, 0.0).transform_point(p),
            a.transform_point(p),
            "t = 0",
        );
        let end = Xform::mix(&a, &b, 1.0);
        assert!(end.rotation.dot(b.rotation).abs() > 1.0 - 1e-12);
    }

    #[test]
    fn mix_about_pivot_keeps_the_pivot_on_a_straight_line() {
        // A quarter turn about a pivot one unit along X. The origin swings on an arc,
        // but the pivot point itself only moves in a straight line between its end positions.
        let pivot = Xform::from_translation(DVec3::X);
        let a = Xform::IDENTITY;
        let b = Xform::from_translation(DVec3::new(0.0, 0.0, 3.0))
            .mul(&Xform::from_rotation(DQuat::from_rotation_y(1.2)));
        let start = a.transform_point(DVec3::X);
        let end = b.transform_point(DVec3::X);
        for i in 0..=10 {
            let t = i as f64 / 10.0;
            let m = Xform::mix_about_pivot(&a, &b, &pivot, t);
            assert_close(
                m.transform_point(DVec3::X),
                start.lerp(end, t),
                "pivot path",
            );
        }
        // Plain mixing does not have this property.
        let plain = Xform::mix(&a, &b, 0.5);
        assert!((plain.transform_point(DVec3::X) - start.lerp(end, 0.5)).length() > 1e-3);
    }

    #[test]
    fn a_matrix_with_uneven_scale_gives_the_nearest_transform() {
        let stretched = DAffine3::from_cols(
            DVec3::new(2.0, 0.0, 0.0),
            DVec3::new(0.3, 1.0, 0.0),
            DVec3::new(0.0, 0.0, 1.0),
            DVec3::new(1.0, 2.0, 3.0),
        );
        let x = Xform::from_affine(&stretched);
        assert!(x.is_finite());
        assert!((x.rotation.length() - 1.0).abs() < 1e-12);
        assert!(
            x.rotation.angle_between(DQuat::IDENTITY) < 1e-9,
            "the axes still point the same way"
        );
        assert_eq!(x.translation, DVec3::new(1.0, 2.0, 3.0));
        assert!(x.scale > 1.0 && x.scale < 2.0);
        let collapsed = Xform::from_affine(&DAffine3::from_cols(
            DVec3::X,
            DVec3::X,
            DVec3::X,
            DVec3::ZERO,
        ));
        assert!(collapsed.is_finite());
    }
}
