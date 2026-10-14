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

/// A Godot colour (sRGB-encoded components) as a linear RGB triple.
#[allow(dead_code)]
pub fn linear_from_color(color: Color) -> [f32; 3] {
    let decode = |c: f32| {
        let c = c.max(0.0);
        if c <= 0.040_45 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [decode(color.r), decode(color.g), decode(color.b)]
}

/// Converts a value from GDScript into JSON for the core.
///
/// Whole-number floats become integers, because GDScript arithmetic readily turns an
/// id or a time into a float and the core's commands expect integers for those.
/// Vectors, quaternions and colours become arrays of their components.
pub fn json_from_variant(value: &Variant) -> serde_json::Value {
    use serde_json::Value;
    let number = |f: f64| {
        if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
            Value::from(f as i64)
        } else {
            serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
        }
    };
    let list = |items: &[f64]| {
        Value::Array(
            items
                .iter()
                .map(|f| serde_json::Number::from_f64(*f).map_or(Value::Null, Value::Number))
                .collect(),
        )
    };
    match value.get_type() {
        VariantType::NIL => Value::Null,
        VariantType::BOOL => Value::Bool(value.to::<bool>()),
        VariantType::INT => Value::from(value.to::<i64>()),
        VariantType::FLOAT => number(value.to::<f64>()),
        VariantType::STRING | VariantType::STRING_NAME | VariantType::NODE_PATH => {
            Value::String(value.to_string())
        }
        VariantType::VECTOR2 => {
            let v = value.to::<Vector2>();
            list(&[v.x as f64, v.y as f64])
        }
        VariantType::VECTOR3 => {
            let v = value.to::<Vector3>();
            list(&[v.x as f64, v.y as f64, v.z as f64])
        }
        VariantType::QUATERNION => {
            let q = value.to::<Quaternion>();
            list(&[q.x as f64, q.y as f64, q.z as f64, q.w as f64])
        }
        VariantType::COLOR => {
            let c = value.to::<Color>();
            list(&[c.r as f64, c.g as f64, c.b as f64, c.a as f64])
        }
        VariantType::DICTIONARY => {
            let dictionary = value.to::<VarDictionary>();
            Value::Object(
                dictionary
                    .iter_shared()
                    .map(|(k, v)| (k.to_string(), json_from_variant(&v)))
                    .collect(),
            )
        }
        VariantType::ARRAY => match value.try_to::<VarArray>() {
            Ok(array) => Value::Array(array.iter_shared().map(|v| json_from_variant(&v)).collect()),
            // An array declared with an element type (`Array[Dictionary]`) is not the
            // plain kind; read it through the calls every array answers.
            Err(_) => {
                let count = value.call("size", &[]).try_to::<i64>().unwrap_or(0);
                Value::Array(
                    (0..count)
                        .map(|i| json_from_variant(&value.call("get", &[i.to_variant()])))
                        .collect(),
                )
            }
        },
        VariantType::PACKED_INT32_ARRAY => Value::Array(
            value
                .to::<PackedInt32Array>()
                .as_slice()
                .iter()
                .map(|i| Value::from(*i))
                .collect(),
        ),
        VariantType::PACKED_INT64_ARRAY => Value::Array(
            value
                .to::<PackedInt64Array>()
                .as_slice()
                .iter()
                .map(|i| Value::from(*i))
                .collect(),
        ),
        VariantType::PACKED_FLOAT32_ARRAY => Value::Array(
            value
                .to::<PackedFloat32Array>()
                .as_slice()
                .iter()
                .map(|f| number(*f as f64))
                .collect(),
        ),
        VariantType::PACKED_FLOAT64_ARRAY => Value::Array(
            value
                .to::<PackedFloat64Array>()
                .as_slice()
                .iter()
                .map(|f| number(*f))
                .collect(),
        ),
        VariantType::PACKED_STRING_ARRAY => Value::Array(
            value
                .to::<PackedStringArray>()
                .as_slice()
                .iter()
                .map(|s| Value::String(s.to_string()))
                .collect(),
        ),
        _ => Value::Null,
    }
}

/// Converts JSON from the core into a value GDScript can use directly.
pub fn variant_from_json(value: &serde_json::Value) -> Variant {
    use serde_json::Value;
    match value {
        Value::Null => Variant::nil(),
        Value::Bool(b) => b.to_variant(),
        Value::Number(n) => match n.as_i64() {
            Some(i) => i.to_variant(),
            None => n.as_f64().unwrap_or(0.0).to_variant(),
        },
        Value::String(s) => GString::from(s.as_str()).to_variant(),
        Value::Array(items) => {
            let mut array = VarArray::new();
            for item in items {
                array.push(&variant_from_json(item));
            }
            array.to_variant()
        }
        Value::Object(fields) => {
            let mut dictionary = VarDictionary::new();
            for (key, item) in fields {
                dictionary.set(
                    &GString::from(key.as_str()).to_variant(),
                    &variant_from_json(item),
                );
            }
            dictionary.to_variant()
        }
    }
}

/// A JSON object from the core as a dictionary; anything else gives an empty one.
pub fn dictionary_from_json(value: &serde_json::Value) -> VarDictionary {
    variant_from_json(value).try_to().unwrap_or_default()
}
