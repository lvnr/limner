//! Shared by the integration tests: small documents built by hand, and a structural
//! validator for `.glb` files that also reads them back.
#![allow(dead_code)]

use limner_core::glam::{DMat4, DQuat, DVec3, Vec3};
use limner_core::model::{
    Brush, Document, Drawing, Group, LayerId, LayerKind, Paint, Point, Sequence, Shading, Stroke,
};
use limner_core::time::{Flicks, FrameRate};
use serde_json::Value;
use std::collections::BTreeSet;

/// One frame at 24 frames per second.
pub const FRAME: Flicks = 29_400_000;
pub const RATE: FrameRate = FrameRate::whole(24);

// ----- documents -----

pub fn point(pos: [f32; 3], width: f32, color: [f32; 3], opacity: f32) -> Point {
    Point {
        pos,
        normal: [0.0, 1.0, 0.0],
        dir: [0.0, 0.0, -1.0],
        color,
        opacity,
        width,
    }
}

/// A straight stroke of `n` points along +X from `origin`, 0.1 m apart, half-width 0.05 m.
pub fn straight(id: u32, brush: Brush, n: usize, origin: [f32; 3]) -> Stroke {
    straight_colored(id, brush, n, origin, [1.0, 0.5, 0.25], 1.0)
}

pub fn straight_colored(
    id: u32,
    brush: Brush,
    n: usize,
    origin: [f32; 3],
    color: [f32; 3],
    opacity: f32,
) -> Stroke {
    let points = (0..n)
        .map(|i| {
            point(
                [origin[0] + i as f32 * 0.1, origin[1], origin[2]],
                0.05,
                color,
                opacity,
            )
        })
        .collect();
    Stroke::new(id, brush, false, points)
}

pub fn group(doc: &mut Document, parent: LayerId, name: &str) -> LayerId {
    let layer = doc.new_layer(name, LayerKind::Group(Group::default()), 0);
    doc.insert_layer(layer, parent, usize::MAX).unwrap()
}

pub fn sequence(
    doc: &mut Document,
    parent: LayerId,
    name: &str,
    loop_duration: Flicks,
    max_repeat: u32,
) -> LayerId {
    let kind = LayerKind::Group(Group {
        children: Vec::new(),
        sequence: Some(Sequence {
            loop_duration,
            max_repeat,
        }),
    });
    let layer = doc.new_layer(name, kind, 0);
    doc.insert_layer(layer, parent, usize::MAX).unwrap()
}

/// A paint layer showing one drawing made of `strokes`.
pub fn paint(doc: &mut Document, parent: LayerId, name: &str, strokes: Vec<Stroke>) -> LayerId {
    frames(doc, parent, name, vec![strokes], 1)
}

/// A frame-by-frame paint layer at 24 fps with one drawing per frame.
pub fn frames(
    doc: &mut Document,
    parent: LayerId,
    name: &str,
    drawings: Vec<Vec<Stroke>>,
    max_repeat: u32,
) -> LayerId {
    let ids: Vec<_> = drawings
        .into_iter()
        .map(|strokes| doc.add_drawing(Drawing::from_strokes(strokes)))
        .collect();
    let paint = Paint {
        rate: RATE,
        frames: (0..ids.len() as u32).collect(),
        drawings: ids,
        max_repeat,
        shading: Shading::Unlit,
    };
    let layer = doc.new_layer(name, LayerKind::Paint(paint), 0);
    doc.insert_layer(layer, parent, usize::MAX).unwrap()
}

// ----- reading a .glb back -----

const GLB_MAGIC: u32 = 0x4654_6C67;
const CHUNK_JSON: u32 = 0x4E4F_534A;
const CHUNK_BIN: u32 = 0x004E_4942;
const UNSIGNED_BYTE: u64 = 5121;
const UNSIGNED_SHORT: u64 = 5123;
const UNSIGNED_INT: u64 = 5125;
const FLOAT: u64 = 5126;

pub struct Glb {
    pub json: Value,
    pub bin: Vec<u8>,
}

pub struct Primitive {
    pub positions: Vec<Vec3>,
    pub normals: Vec<Vec3>,
    /// Linear RGBA.
    pub colors: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
    pub material: Value,
}

impl Primitive {
    pub fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| [0, 1, 2].map(|i| self.positions[t[i] as usize]))
    }
}

pub struct Channel {
    pub times: Vec<f32>,
    pub values: Vec<Vec<f32>>,
    pub step: bool,
}

impl Channel {
    /// The channel's value at `t`, by the sampler's interpolation (component-wise for
    /// linear, which is exact at key times and good enough between close keys).
    pub fn sample(&self, t: f32) -> Vec<f32> {
        let after = self.times.partition_point(|k| *k <= t);
        if after == 0 {
            return self.values[0].clone();
        }
        let i = after - 1;
        if self.step || i + 1 >= self.times.len() || self.times[i] == t {
            return self.values[i].clone();
        }
        let u = (t - self.times[i]) / (self.times[i + 1] - self.times[i]);
        self.values[i]
            .iter()
            .zip(&self.values[i + 1])
            .map(|(a, b)| a + (b - a) * u)
            .collect()
    }
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn index_of(value: &Value) -> usize {
    value.as_u64().expect("an index") as usize
}

fn array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

fn component_size(component_type: u64) -> usize {
    match component_type {
        UNSIGNED_BYTE | 5120 => 1,
        UNSIGNED_SHORT | 5122 => 2,
        UNSIGNED_INT | FLOAT => 4,
        other => panic!("unknown componentType {other}"),
    }
}

fn component_count(kind: &str) -> usize {
    match kind {
        "SCALAR" => 1,
        "VEC2" => 2,
        "VEC3" => 3,
        "VEC4" => 4,
        "MAT4" => 16,
        other => panic!("unknown accessor type {other}"),
    }
}

/// Every key that appears inside an `extensions` object anywhere in the document.
fn collect_extensions(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if key == "extensions" {
                    if let Some(extensions) = child.as_object() {
                        out.extend(extensions.keys().cloned());
                    }
                }
                collect_extensions(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_extensions(item, out)),
        _ => {}
    }
}

impl Glb {
    /// Splits the container, checking the header and chunk layout.
    pub fn parse(bytes: &[u8]) -> Glb {
        assert!(bytes.len() >= 20, "too short for a header and a chunk");
        assert_eq!(u32_at(bytes, 0), GLB_MAGIC, "magic");
        assert_eq!(u32_at(bytes, 4), 2, "container version");
        assert_eq!(
            u32_at(bytes, 8) as usize,
            bytes.len(),
            "the header's length is the file's length"
        );
        assert_eq!(bytes.len() % 4, 0, "the file is 4-byte aligned");

        let json_length = u32_at(bytes, 12) as usize;
        assert_eq!(u32_at(bytes, 16), CHUNK_JSON, "the first chunk is JSON");
        assert_eq!(json_length % 4, 0, "the JSON chunk is padded to 4 bytes");
        let json_end = 20 + json_length;
        assert!(json_end <= bytes.len(), "the JSON chunk fits in the file");
        let text = &bytes[20..json_end];
        let trimmed = text.len() - text.iter().rev().take_while(|b| **b == b' ').count();
        assert!(
            text.len() - trimmed < 4,
            "the JSON chunk is padded with at most three spaces"
        );
        let json: Value = serde_json::from_slice(text).expect("the JSON chunk parses");

        let bin = if json_end == bytes.len() {
            Vec::new()
        } else {
            assert!(
                json_end + 8 <= bytes.len(),
                "room for a second chunk header"
            );
            let bin_length = u32_at(bytes, json_end) as usize;
            assert_eq!(
                u32_at(bytes, json_end + 4),
                CHUNK_BIN,
                "the second chunk is BIN"
            );
            assert_eq!(bin_length % 4, 0, "the BIN chunk is padded to 4 bytes");
            assert_eq!(
                json_end + 8 + bin_length,
                bytes.len(),
                "nothing follows the BIN chunk"
            );
            bytes[json_end + 8..].to_vec()
        };
        Glb { json, bin }
    }

    /// Parses and runs every structural check.
    pub fn checked(bytes: &[u8]) -> Glb {
        let glb = Glb::parse(bytes);
        glb.validate();
        glb
    }

    pub fn list(&self, key: &str) -> &[Value] {
        array(&self.json, key)
    }

    // ----- accessors -----

    /// Start and length in the buffer of an accessor's data, after checking it fits.
    fn accessor_span(&self, index: usize) -> (usize, usize, u64, usize) {
        let accessor = &self.list("accessors")[index];
        let component_type = accessor["componentType"].as_u64().expect("componentType");
        let components = component_count(accessor["type"].as_str().expect("type"));
        let count = index_of(&accessor["count"]);
        assert!(count >= 1, "accessor {index} is empty");
        let view = &self.list("bufferViews")[index_of(&accessor["bufferView"])];
        assert!(
            view.get("byteStride").is_none(),
            "views are not interleaved"
        );
        let view_offset = view.get("byteOffset").map_or(0, index_of);
        let view_length = index_of(&view["byteLength"]);
        let offset = accessor.get("byteOffset").map_or(0, index_of);
        let size = component_size(component_type);
        let length = count * components * size;
        assert_eq!(offset % size, 0, "accessor {index} offset alignment");
        assert_eq!(
            (view_offset + offset) % size,
            0,
            "accessor {index} is aligned to its component size"
        );
        assert!(
            offset + length <= view_length,
            "accessor {index} fits in its buffer view"
        );
        (view_offset + offset, components, component_type, count)
    }

    /// An accessor's elements as floats. Normalised integers are mapped to `0..=1`.
    pub fn floats(&self, index: usize) -> Vec<Vec<f32>> {
        let (start, components, component_type, count) = self.accessor_span(index);
        let normalized = self.list("accessors")[index]
            .get("normalized")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let size = component_size(component_type);
        (0..count)
            .map(|element| {
                (0..components)
                    .map(|c| {
                        let at = start + (element * components + c) * size;
                        let bytes = &self.bin[at..at + size];
                        match component_type {
                            FLOAT => f32::from_le_bytes(bytes.try_into().unwrap()),
                            UNSIGNED_SHORT => {
                                let raw = u16::from_le_bytes(bytes.try_into().unwrap());
                                if normalized {
                                    raw as f32 / 65535.0
                                } else {
                                    raw as f32
                                }
                            }
                            UNSIGNED_BYTE => {
                                if normalized {
                                    bytes[0] as f32 / 255.0
                                } else {
                                    bytes[0] as f32
                                }
                            }
                            other => panic!("accessor {index}: unexpected componentType {other}"),
                        }
                    })
                    .collect()
            })
            .collect()
    }

    pub fn vec3s(&self, index: usize) -> Vec<Vec3> {
        self.floats(index)
            .iter()
            .map(|v| Vec3::new(v[0], v[1], v[2]))
            .collect()
    }

    pub fn integers(&self, index: usize) -> Vec<u32> {
        let (start, components, component_type, count) = self.accessor_span(index);
        assert_eq!(components, 1);
        let size = component_size(component_type);
        (0..count)
            .map(|element| {
                let at = start + element * size;
                match component_type {
                    UNSIGNED_BYTE => self.bin[at] as u32,
                    UNSIGNED_SHORT => {
                        u16::from_le_bytes(self.bin[at..at + 2].try_into().unwrap()) as u32
                    }
                    UNSIGNED_INT => u32_at(&self.bin, at),
                    other => panic!("accessor {index}: unexpected index type {other}"),
                }
            })
            .collect()
    }

    // ----- nodes -----

    pub fn node_named(&self, name: &str) -> usize {
        let matches: Vec<usize> = self
            .list("nodes")
            .iter()
            .enumerate()
            .filter(|(_, n)| n["name"] == name)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(matches.len(), 1, "exactly one node named {name}");
        matches[0]
    }

    pub fn has_node(&self, name: &str) -> bool {
        self.list("nodes").iter().any(|n| n["name"] == name)
    }

    pub fn node(&self, index: usize) -> &Value {
        &self.list("nodes")[index]
    }

    pub fn parent_of(&self, node: usize) -> Option<usize> {
        self.list("nodes").iter().position(|n| {
            array(n, "children")
                .iter()
                .any(|child| index_of(child) == node)
        })
    }

    fn vector<const N: usize>(node: &Value, key: &str, default: [f64; N]) -> [f64; N] {
        match node.get(key).and_then(Value::as_array) {
            Some(items) => {
                assert_eq!(items.len(), N, "{key} has {N} components");
                let mut out = [0.0; N];
                for (o, item) in out.iter_mut().zip(items) {
                    *o = item.as_f64().expect("a number");
                }
                out
            }
            None => default,
        }
    }

    pub fn translation(&self, node: usize) -> [f64; 3] {
        Self::vector(self.node(node), "translation", [0.0; 3])
    }

    pub fn rotation(&self, node: usize) -> [f64; 4] {
        Self::vector(self.node(node), "rotation", [0.0, 0.0, 0.0, 1.0])
    }

    pub fn scale(&self, node: usize) -> [f64; 3] {
        Self::vector(self.node(node), "scale", [1.0; 3])
    }

    /// The node's own matrix, `T * R * S` as glTF defines it.
    pub fn local_matrix(&self, node: usize) -> DMat4 {
        let [x, y, z, w] = self.rotation(node);
        DMat4::from_scale_rotation_translation(
            DVec3::from_array(self.scale(node)),
            DQuat::from_xyzw(x, y, z, w),
            DVec3::from_array(self.translation(node)),
        )
    }

    /// The node's matrix in the scene, composed up its parents.
    pub fn world_matrix(&self, node: usize) -> DMat4 {
        let mut m = self.local_matrix(node);
        let mut current = self.parent_of(node);
        while let Some(parent) = current {
            m = self.local_matrix(parent) * m;
            current = self.parent_of(parent);
        }
        m
    }

    // ----- meshes -----

    pub fn primitives_of_mesh(&self, mesh: usize) -> Vec<Primitive> {
        array(&self.list("meshes")[mesh], "primitives")
            .iter()
            .map(|primitive| {
                let attributes = &primitive["attributes"];
                let normals = attributes
                    .get("NORMAL")
                    .map(|a| self.vec3s(index_of(a)))
                    .unwrap_or_default();
                let colors = self
                    .floats(index_of(&attributes["COLOR_0"]))
                    .iter()
                    .map(|c| [c[0], c[1], c[2], c[3]])
                    .collect();
                Primitive {
                    positions: self.vec3s(index_of(&attributes["POSITION"])),
                    normals,
                    colors,
                    indices: self.integers(index_of(&primitive["indices"])),
                    material: self.list("materials")[index_of(&primitive["material"])].clone(),
                }
            })
            .collect()
    }

    /// The primitives of the mesh attached to `node`.
    pub fn primitives(&self, node: usize) -> Vec<Primitive> {
        let mesh = self.node(node).get("mesh").expect("the node has a mesh");
        self.primitives_of_mesh(index_of(mesh))
    }

    // ----- animation -----

    fn read_channel(&self, animation: &Value, channel: &Value) -> Channel {
        let sampler = &array(animation, "samplers")[index_of(&channel["sampler"])];
        Channel {
            times: self
                .floats(index_of(&sampler["input"]))
                .iter()
                .map(|t| t[0])
                .collect(),
            values: self.floats(index_of(&sampler["output"])),
            step: sampler["interpolation"] == "STEP",
        }
    }

    /// The channel animating `path` ("translation", "rotation" or "scale") of `node`.
    pub fn channel(&self, node: usize, path: &str) -> Option<Channel> {
        self.list("animations").iter().find_map(|animation| {
            array(animation, "channels")
                .iter()
                .find(|c| {
                    c["target"].get("node").map(index_of) == Some(node)
                        && c["target"]["path"] == path
                })
                .map(|c| self.read_channel(animation, c))
        })
    }

    /// The channel animating a JSON pointer through `KHR_animation_pointer`.
    pub fn pointer_channel(&self, pointer: &str) -> Option<Channel> {
        self.list("animations").iter().find_map(|animation| {
            array(animation, "channels")
                .iter()
                .find(|c| c["target"]["extensions"]["KHR_animation_pointer"]["pointer"] == pointer)
                .map(|c| self.read_channel(animation, c))
        })
    }

    pub fn channel_count(&self) -> usize {
        self.list("animations")
            .iter()
            .map(|a| array(a, "channels").len())
            .sum()
    }

    pub fn extensions_used(&self) -> Vec<&str> {
        self.list("extensionsUsed")
            .iter()
            .map(|e| e.as_str().expect("an extension name"))
            .collect()
    }

    // ----- validation -----

    /// Checks every structural rule the exporter relies on. Panics with the rule broken.
    pub fn validate(&self) {
        let json = &self.json;
        assert_eq!(json["asset"]["version"], "2.0");
        assert!(json["asset"]["generator"]
            .as_str()
            .is_some_and(|g| g.starts_with("Limner")));
        for (key, value) in json.as_object().expect("the root is an object") {
            if let Some(items) = value.as_array() {
                assert!(!items.is_empty(), "{key} must not be an empty array");
            }
        }

        // Buffer and views.
        let buffers = self.list("buffers");
        assert!(buffers.len() <= 1, "a .glb has at most one buffer");
        let buffer_length = buffers.first().map_or(0, |b| index_of(&b["byteLength"]));
        assert!(
            buffer_length <= self.bin.len() && self.bin.len() - buffer_length < 4,
            "the buffer is the BIN chunk, give or take padding"
        );
        assert!(
            buffers.iter().all(|b| b.get("uri").is_none()),
            "the buffer is embedded"
        );
        for (index, view) in self.list("bufferViews").iter().enumerate() {
            assert_eq!(view["buffer"], 0, "view {index} uses the only buffer");
            let offset = view.get("byteOffset").map_or(0, index_of);
            let length = index_of(&view["byteLength"]);
            assert!(length >= 1, "view {index} is not empty");
            assert_eq!(offset % 4, 0, "view {index} starts on a 4-byte boundary");
            assert!(
                offset + length <= buffer_length,
                "view {index} fits in the buffer"
            );
            if let Some(target) = view.get("target") {
                assert!(
                    target == 34962 || target == 34963,
                    "view {index} has a valid target"
                );
            }
        }

        // Accessors: bounds, type consistency, finite floats, declared min and max.
        let accessors = self.list("accessors");
        for (index, accessor) in accessors.iter().enumerate() {
            let (_, components, component_type, count) = self.accessor_span(index);
            if component_type == FLOAT {
                let values = self.floats(index);
                assert_eq!(values.len(), count);
                assert!(
                    values.iter().flatten().all(|v| v.is_finite()),
                    "accessor {index} holds only finite numbers"
                );
                for (key, pick) in [
                    ("min", f32::min as fn(f32, f32) -> f32),
                    ("max", f32::max as fn(f32, f32) -> f32),
                ] {
                    let Some(declared) = accessor.get(key).and_then(Value::as_array) else {
                        continue;
                    };
                    assert_eq!(declared.len(), components, "accessor {index} {key} length");
                    for (c, declared) in declared.iter().enumerate() {
                        let actual = values.iter().map(|v| v[c]).reduce(pick).unwrap();
                        assert_eq!(
                            declared.as_f64().unwrap() as f32,
                            actual,
                            "accessor {index} {key}[{c}]"
                        );
                    }
                }
                assert_eq!(
                    accessor.get("min").is_some(),
                    accessor.get("max").is_some(),
                    "accessor {index} declares both bounds or neither"
                );
            } else {
                assert!(
                    matches!(
                        component_type,
                        UNSIGNED_BYTE | UNSIGNED_SHORT | UNSIGNED_INT
                    ),
                    "accessor {index} has an expected componentType"
                );
            }
        }

        // Materials.
        let materials = self.list("materials");
        for (index, material) in materials.iter().enumerate() {
            if let Some(mode) = material.get("alphaMode") {
                assert!(
                    mode == "OPAQUE" || mode == "MASK" || mode == "BLEND",
                    "material {index} alphaMode"
                );
            }
            if let Some(factor) = material["pbrMetallicRoughness"].get("baseColorFactor") {
                let factor = factor.as_array().expect("an array");
                assert_eq!(factor.len(), 4);
                assert!(factor
                    .iter()
                    .all(|c| (0.0..=1.0).contains(&c.as_f64().unwrap())));
            }
        }

        // Meshes.
        let views = self.list("bufferViews");
        let view_target = |accessor: usize| {
            views[index_of(&accessors[accessor]["bufferView"])]
                .get("target")
                .and_then(Value::as_u64)
        };
        for (index, mesh) in self.list("meshes").iter().enumerate() {
            let primitives = array(mesh, "primitives");
            assert!(!primitives.is_empty(), "mesh {index} has primitives");
            for primitive in primitives {
                assert_eq!(
                    primitive.get("mode").map_or(4, index_of),
                    4,
                    "triangles only"
                );
                assert!(index_of(&primitive["material"]) < materials.len());
                let attributes = primitive["attributes"].as_object().expect("attributes");
                let position = index_of(&attributes["POSITION"]);
                let p = &accessors[position];
                assert_eq!(p["type"], "VEC3");
                assert_eq!(p["componentType"], FLOAT);
                assert!(
                    p.get("min").is_some() && p.get("max").is_some(),
                    "POSITION declares its bounds"
                );
                let vertices = index_of(&p["count"]);
                for (name, accessor) in attributes {
                    let accessor = index_of(accessor);
                    let a = &accessors[accessor];
                    assert_eq!(
                        index_of(&a["count"]),
                        vertices,
                        "mesh {index}: {name} has one element per vertex"
                    );
                    assert_eq!(view_target(accessor), Some(34962), "{name} is vertex data");
                    match name.as_str() {
                        "POSITION" => {}
                        "NORMAL" => {
                            assert_eq!(a["type"], "VEC3");
                            assert_eq!(a["componentType"], FLOAT);
                            for n in self.vec3s(accessor) {
                                assert!(
                                    (n.length() - 1.0).abs() < 1e-3,
                                    "mesh {index}: normals are unit length"
                                );
                            }
                        }
                        "COLOR_0" => {
                            assert_eq!(a["type"], "VEC4");
                            let component = a["componentType"].as_u64().unwrap();
                            assert!(
                                component == FLOAT
                                    || (matches!(component, UNSIGNED_BYTE | UNSIGNED_SHORT)
                                        && a["normalized"] == true),
                                "COLOR_0 is float or a normalised unsigned integer"
                            );
                        }
                        other => panic!("unexpected attribute {other}"),
                    }
                }
                let indices = index_of(&primitive["indices"]);
                let i = &accessors[indices];
                assert_eq!(i["type"], "SCALAR");
                let component = i["componentType"].as_u64().unwrap();
                assert!(matches!(component, UNSIGNED_SHORT | UNSIGNED_INT));
                assert_eq!(
                    view_target(indices),
                    Some(34963),
                    "indices are element data"
                );
                let values = self.integers(indices);
                assert_eq!(values.len() % 3, 0, "whole triangles");
                assert!(
                    values.iter().all(|v| (*v as usize) < vertices),
                    "mesh {index}: every index is below the vertex count"
                );
                if component == UNSIGNED_SHORT {
                    assert!(
                        values.iter().all(|v| *v != 65_535),
                        "65535 restarts a 16-bit primitive"
                    );
                }
            }
        }

        // Cameras and lights.
        for camera in self.list("cameras") {
            assert_eq!(camera["type"], "perspective");
            let p = &camera["perspective"];
            let yfov = p["yfov"].as_f64().unwrap();
            assert!(yfov > 0.0 && yfov < std::f64::consts::PI);
            assert!(p["aspectRatio"].as_f64().unwrap() > 0.0);
            let near = p["znear"].as_f64().unwrap();
            assert!(near > 0.0);
            if let Some(far) = p.get("zfar") {
                assert!(far.as_f64().unwrap() > near);
            }
        }
        let lights = array(&json["extensions"]["KHR_lights_punctual"], "lights");
        for light in lights {
            let kind = light["type"].as_str().unwrap();
            assert!(["point", "spot", "directional"].contains(&kind));
            assert!(light["intensity"].as_f64().unwrap() >= 0.0);
            assert!(array(light, "color")
                .iter()
                .all(|c| (0.0..=1.0).contains(&c.as_f64().unwrap())));
            assert_eq!(kind == "spot", light.get("spot").is_some());
            if let Some(spot) = light.get("spot") {
                let inner = spot["innerConeAngle"].as_f64().unwrap();
                let outer = spot["outerConeAngle"].as_f64().unwrap();
                assert!(0.0 <= inner && inner < outer);
                assert!(outer <= std::f64::consts::FRAC_PI_2 + 1e-6);
            }
            if let Some(range) = light.get("range") {
                assert!(range.as_f64().unwrap() > 0.0);
                assert_ne!(kind, "directional");
            }
        }

        // Nodes: valid references, one parent each, no cycles, all reachable from the scene.
        let nodes = self.list("nodes");
        let mut parent: Vec<Option<usize>> = vec![None; nodes.len()];
        for (index, node) in nodes.iter().enumerate() {
            assert!(node.get("matrix").is_none(), "nodes use TRS, not matrices");
            for child in array(node, "children") {
                let child = index_of(child);
                assert!(child < nodes.len(), "node {index} lists a missing child");
                assert_ne!(child, index, "node {index} is its own child");
                assert!(
                    parent[child].replace(index).is_none(),
                    "node {child} has two parents"
                );
            }
            if let Some(mesh) = node.get("mesh") {
                assert!(index_of(mesh) < self.list("meshes").len());
            }
            if let Some(camera) = node.get("camera") {
                assert!(index_of(camera) < self.list("cameras").len());
            }
            if let Some(light) = node["extensions"]["KHR_lights_punctual"].get("light") {
                assert!(index_of(light) < lights.len());
            }
            let [x, y, z, w] = Self::vector(node, "rotation", [0.0, 0.0, 0.0, 1.0]);
            let length = (x * x + y * y + z * z + w * w).sqrt();
            assert!((length - 1.0).abs() < 1e-5, "node {index} rotation is unit");
        }
        assert_eq!(json["scene"], 0);
        let scenes = self.list("scenes");
        assert_eq!(scenes.len(), 1);
        let mut reached = vec![false; nodes.len()];
        let mut stack: Vec<usize> = Vec::new();
        for root in array(&scenes[0], "nodes") {
            let root = index_of(root);
            assert!(root < nodes.len());
            assert!(parent[root].is_none(), "scene root {root} has a parent");
            stack.push(root);
        }
        while let Some(index) = stack.pop() {
            assert!(!reached[index], "node {index} is reached twice: a cycle");
            reached[index] = true;
            stack.extend(array(&nodes[index], "children").iter().map(index_of));
        }
        assert!(
            reached.iter().all(|r| *r),
            "every node is reachable from the scene"
        );

        // Animations.
        for animation in self.list("animations") {
            let samplers = array(animation, "samplers");
            let channels = array(animation, "channels");
            assert!(!samplers.is_empty() && !channels.is_empty());
            let mut targets = BTreeSet::new();
            let mut sampler_used = vec![false; samplers.len()];
            for channel in channels {
                let sampler_index = index_of(&channel["sampler"]);
                sampler_used[sampler_index] = true;
                let sampler = &samplers[sampler_index];
                let interpolation = sampler["interpolation"].as_str().unwrap_or("LINEAR");
                assert!(interpolation == "LINEAR" || interpolation == "STEP");

                let input = index_of(&sampler["input"]);
                let a = &accessors[input];
                assert_eq!(a["type"], "SCALAR");
                assert_eq!(a["componentType"], FLOAT);
                assert!(
                    a.get("min").is_some() && a.get("max").is_some(),
                    "animation input declares its bounds"
                );
                let times: Vec<f32> = self.floats(input).iter().map(|t| t[0]).collect();
                assert!(times[0] >= 0.0);
                assert!(
                    times.windows(2).all(|w| w[0] < w[1]),
                    "animation input times are strictly increasing"
                );

                let output = index_of(&sampler["output"]);
                let o = &accessors[output];
                assert_eq!(
                    index_of(&o["count"]),
                    times.len(),
                    "one output element per key"
                );
                let target = &channel["target"];
                let path = target["path"].as_str().expect("a target path");
                match path {
                    "translation" | "scale" => {
                        assert_eq!(o["type"], "VEC3");
                        assert_eq!(o["componentType"], FLOAT);
                    }
                    "rotation" => {
                        assert_eq!(o["type"], "VEC4");
                        assert_eq!(o["componentType"], FLOAT);
                        let keys = self.floats(output);
                        for q in &keys {
                            let length = q.iter().map(|c| c * c).sum::<f32>().sqrt();
                            assert!((length - 1.0).abs() < 1e-4, "rotation keys are unit");
                        }
                        for pair in keys.windows(2) {
                            let dot: f32 = pair[0].iter().zip(&pair[1]).map(|(a, b)| a * b).sum();
                            assert!(dot >= 0.0, "rotation keys stay on one hemisphere");
                        }
                    }
                    "pointer" => {
                        assert!(target.get("node").is_none());
                        let pointer = target["extensions"]["KHR_animation_pointer"]["pointer"]
                            .as_str()
                            .expect("a pointer");
                        let resolved = json.pointer(pointer);
                        assert!(resolved.is_some(), "{pointer} resolves in the document");
                        match resolved.unwrap() {
                            Value::Bool(_) => {
                                assert_eq!(o["type"], "SCALAR");
                                assert_eq!(o["componentType"], UNSIGNED_BYTE);
                                assert_eq!(interpolation, "STEP", "booleans are stepped");
                            }
                            Value::Array(items) => {
                                assert_eq!(
                                    component_count(o["type"].as_str().unwrap()),
                                    items.len()
                                );
                                assert_eq!(o["componentType"], FLOAT);
                            }
                            other => panic!("{pointer} points at {other}"),
                        }
                    }
                    other => panic!("unexpected animation path {other}"),
                }
                if path != "pointer" {
                    let node = index_of(&target["node"]);
                    assert!(node < nodes.len());
                    assert!(
                        targets.insert((node.to_string(), path.to_owned())),
                        "node {node} has two {path} channels"
                    );
                } else {
                    let pointer = target["extensions"]["KHR_animation_pointer"]["pointer"]
                        .as_str()
                        .unwrap();
                    assert!(
                        targets.insert((pointer.to_owned(), path.to_owned())),
                        "{pointer} is animated twice"
                    );
                }
            }
            assert!(sampler_used.iter().all(|u| *u), "no sampler is unused");
        }

        // Extensions: used exactly when listed.
        let mut found = BTreeSet::new();
        collect_extensions(json, &mut found);
        let listed: BTreeSet<String> = self
            .extensions_used()
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(found, listed, "extensionsUsed lists what is used");
        assert!(
            json.get("extensionsRequired").is_none(),
            "nothing is required to open the file"
        );

        // Nothing is written that nothing refers to.
        let mut accessor_used = vec![false; accessors.len()];
        for mesh in self.list("meshes") {
            for primitive in array(mesh, "primitives") {
                for accessor in primitive["attributes"].as_object().unwrap().values() {
                    accessor_used[index_of(accessor)] = true;
                }
                accessor_used[index_of(&primitive["indices"])] = true;
            }
        }
        for animation in self.list("animations") {
            for sampler in array(animation, "samplers") {
                accessor_used[index_of(&sampler["input"])] = true;
                accessor_used[index_of(&sampler["output"])] = true;
            }
        }
        assert!(accessor_used.iter().all(|u| *u), "no accessor is unused");
        let mut view_used = vec![false; views.len()];
        for accessor in accessors {
            view_used[index_of(&accessor["bufferView"])] = true;
        }
        assert!(view_used.iter().all(|u| *u), "no buffer view is unused");
        let mut mesh_used = vec![false; self.list("meshes").len()];
        for node in nodes {
            if let Some(mesh) = node.get("mesh") {
                mesh_used[index_of(mesh)] = true;
            }
        }
        assert!(mesh_used.iter().all(|u| *u), "no mesh is unused");
        let mut material_used = vec![false; materials.len()];
        for mesh in self.list("meshes") {
            for primitive in array(mesh, "primitives") {
                material_used[index_of(&primitive["material"])] = true;
            }
        }
        assert!(material_used.iter().all(|u| *u), "no material is unused");
    }
}

/// Packs a description and a buffer back into a `.glb`, for tests that damage a file
/// on purpose.
pub fn pack(json: &Value, bin: &[u8]) -> Vec<u8> {
    let mut text = serde_json::to_vec(json).unwrap();
    while !text.len().is_multiple_of(4) {
        text.push(b' ');
    }
    let total = 12 + 8 + text.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(text.len() as u32).to_le_bytes());
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&text);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
        out.extend_from_slice(bin);
    }
    out
}
