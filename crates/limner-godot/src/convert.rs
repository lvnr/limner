//! Conversions between core (glam, double precision) and Godot (single precision) types.

use godot::prelude::*;
use limner::core::glam::{DAffine3, DVec3};

pub fn vector3(v: DVec3) -> Vector3 {
    Vector3::new(v.x as f32, v.y as f32, v.z as f32)
}

pub fn dvec3(v: Vector3) -> DVec3 {
    DVec3::new(v.x as f64, v.y as f64, v.z as f64)
}

/// The final cast from double to single precision. Callers compose everything in
/// doubles first and convert only the transform that goes to the renderer.
pub fn transform3d(m: &DAffine3) -> Transform3D {
    let c = &m.matrix3;
    Transform3D::new(
        Basis::from_cols(vector3(c.x_axis), vector3(c.y_axis), vector3(c.z_axis)),
        vector3(m.translation),
    )
}

/// A linear RGB triple as a Godot colour (which stores sRGB-encoded components).
pub fn color_from_linear(rgb: [f32; 3]) -> Color {
    let encode = |c: f32| {
        let c = c.clamp(0.0, 1.0);
        if c <= 0.003_130_8 {
            c * 12.92
        } else {
            1.055 * c.powf(1.0 / 2.4) - 0.055
        }
    };
    Color::from_rgb(encode(rgb[0]), encode(rgb[1]), encode(rgb[2]))
}
