//! glTF 2.0 binary (`.glb`) writer.
//!
//! Limner's world is metres, Y up, right-handed, which is exactly glTF's, so
//! nothing is converted on the way out. The mapping of each Limner concept is
//! described in `docs/export.md`; the short version:
//!
//! * one node per layer with translation, rotation and scale; a mirrored layer has a
//!   negative scale on its flip axis;
//! * one mesh per drawing with `POSITION`, `NORMAL` and linear `COLOR_0`;
//! * one animation: sampled transforms, and visibility as a stepped scale of 0 or 1.

use crate::scene::{ExportLight, ExportNode, ExportPaint, ExportScene, NodeContent, Sampled};
use crate::{ExportError, ExportReport};
use limner_core::math::Xform;
use limner_core::model::{LightKind, Shading, OPAQUE_THRESHOLD};
use serde_json::{json, Map, Value};
use std::collections::{BTreeSet, HashMap};

const GLB_MAGIC: u32 = 0x4654_6C67; // "glTF"
const GLB_VERSION: u32 = 2;
const CHUNK_JSON: u32 = 0x4E4F_534A; // "JSON"
const CHUNK_BIN: u32 = 0x004E_4942; // "BIN\0"

const UNSIGNED_BYTE: u32 = 5121;
const UNSIGNED_SHORT: u32 = 5123;
const UNSIGNED_INT: u32 = 5125;
const FLOAT: u32 = 5126;
const ARRAY_BUFFER: u32 = 34962;
const ELEMENT_ARRAY_BUFFER: u32 = 34963;

const UNLIT: &str = "KHR_materials_unlit";
const LIGHTS: &str = "KHR_lights_punctual";
const VISIBILITY: &str = "KHR_node_visibility";
const POINTER: &str = "KHR_animation_pointer";

/// Near and far planes written for cameras. Limner's camera layers carry only a
/// field of view; these suit artwork measured in metres.
pub const CAMERA_ZNEAR: f32 = 0.01;
pub const CAMERA_ZFAR: f32 = 1000.0;

/// Choices that only concern glTF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GltfOptions {
    /// Also write `KHR_node_visibility` on nodes whose visibility changes, animated
    /// through `KHR_animation_pointer`. The stepped scale is written either way, so
    /// viewers that know neither extension still show the right frames.
    pub node_visibility: bool,
    /// Animate layer opacity through `KHR_animation_pointer` on the material's base
    /// colour alpha. Off, a layer keeps the opacity it has on the first frame.
    pub animate_opacity: bool,
    /// Write `NORMAL`. Unlit materials do not need normals, so turning this off
    /// makes files about a third smaller at the cost of relighting downstream.
    pub normals: bool,
}

impl Default for GltfOptions {
    fn default() -> Self {
        Self {
            node_visibility: false,
            animate_opacity: false,
            normals: true,
        }
    }
}

/// A finished `.glb` and what went into it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlbOutput {
    pub bytes: Vec<u8>,
    pub report: ExportReport,
}

/// The shortest decimal that reads back as the same `f32`.
fn num(x: f32) -> Value {
    let x = if x.is_finite() { x } else { 0.0 };
    json!(format!("{x}").parse::<f64>().unwrap_or(f64::from(x)))
}

fn nums<const N: usize>(values: [f32; N]) -> Value {
    Value::Array(values.into_iter().map(num).collect())
}

/// The exact value of an `f32`, for accessor bounds that validators compare bit for bit.
fn exact<const N: usize>(values: [f32; N]) -> Value {
    Value::Array(values.into_iter().map(|v| json!(f64::from(v))).collect())
}

/// Translation, rotation (x, y, z, w) and scale of a layer transform. A hidden node
/// collapses to scale zero.
fn trs(x: &Xform, visible: bool) -> ([f32; 3], [f32; 4], [f32; 3]) {
    // A transform with a non-finite number in it cannot be written; fall back to none.
    let x = if x.is_finite() { x } else { &Xform::IDENTITY };
    let q = x.rotation.normalize();
    let q = if q.is_finite() {
        q
    } else {
        limner_core::glam::DQuat::IDENTITY
    };
    let scale = if visible {
        (x.flip.signs() * x.scale).as_vec3().to_array()
    } else {
        [0.0; 3]
    };
    (
        x.translation.as_vec3().to_array(),
        [q.x as f32, q.y as f32, q.z as f32, q.w as f32],
        scale,
    )
}

/// Frames worth keeping from per-frame samples. For linear interpolation both ends of
/// a run of equal values stay, so a hold is not interpolated across; for stepped
/// values only changes stay. The first and last frames always stay, so every sampler
/// spans the whole animation and viewers agree on its length.
fn kept_frames<T: PartialEq>(values: &[T], step: bool) -> Vec<u32> {
    let last = values.len() - 1;
    (0..values.len())
        .filter(|&k| {
            k == 0
                || k == last
                || values[k] != values[k - 1]
                || (!step && values[k] != values[k + 1])
        })
        .map(|k| k as u32)
        .collect()
}

#[derive(Clone, Copy)]
struct Primitive {
    position: usize,
    normal: Option<usize>,
    color: usize,
    indices: usize,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct MaterialKey {
    lit: bool,
    double_sided: bool,
    blend: bool,
    /// Bits of the base colour alpha.
    alpha: u32,
    /// Set for a material whose alpha is animated: it belongs to one node.
    owner: Option<usize>,
}

enum Target {
    Node { node: usize, path: &'static str },
    Pointer(String),
}

impl Target {
    fn json(&self) -> Value {
        match self {
            Target::Node { node, path } => json!({ "node": node, "path": path }),
            Target::Pointer(pointer) => json!({
                "path": "pointer",
                "extensions": { POINTER: { "pointer": pointer } },
            }),
        }
    }
}

struct Writer<'a> {
    scene: &'a ExportScene,
    extra: &'a GltfOptions,
    bin: Vec<u8>,
    views: Vec<Value>,
    accessors: Vec<Value>,
    materials: Vec<Value>,
    material_keys: HashMap<MaterialKey, usize>,
    meshes: Vec<Value>,
    mesh_keys: HashMap<Vec<(usize, usize)>, usize>,
    primitives: HashMap<usize, Primitive>,
    nodes: Vec<Value>,
    cameras: Vec<Value>,
    lights: Vec<Value>,
    samplers: Vec<Value>,
    channels: Vec<Value>,
    time_accessors: HashMap<Vec<u32>, usize>,
    used: BTreeSet<&'static str>,
}

impl<'a> Writer<'a> {
    fn new(scene: &'a ExportScene, extra: &'a GltfOptions) -> Self {
        Self {
            scene,
            extra,
            bin: Vec::new(),
            views: Vec::new(),
            accessors: Vec::new(),
            materials: Vec::new(),
            material_keys: HashMap::new(),
            meshes: Vec::new(),
            mesh_keys: HashMap::new(),
            primitives: HashMap::new(),
            nodes: Vec::new(),
            cameras: Vec::new(),
            lights: Vec::new(),
            samplers: Vec::new(),
            channels: Vec::new(),
            time_accessors: HashMap::new(),
            used: BTreeSet::new(),
        }
    }

    // ----- buffer, views, accessors -----

    /// Starts a buffer view at the current end of the buffer, which is always 4-byte aligned.
    fn begin_view(&self) -> usize {
        debug_assert_eq!(self.bin.len() % 4, 0);
        self.bin.len()
    }

    /// Closes the view started at `offset` and pads the buffer back to 4 bytes.
    fn end_view(&mut self, offset: usize, target: Option<u32>) -> usize {
        let mut view = json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": self.bin.len() - offset,
        });
        if let Some(target) = target {
            view["target"] = json!(target);
        }
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
        self.views.push(view);
        self.views.len() - 1
    }

    fn push_accessor(&mut self, accessor: Value) -> usize {
        self.accessors.push(accessor);
        self.accessors.len() - 1
    }

    fn float_accessor<const N: usize>(
        &mut self,
        values: impl Iterator<Item = [f32; N]>,
        target: Option<u32>,
        with_bounds: bool,
    ) -> usize {
        let offset = self.begin_view();
        let mut count = 0usize;
        let mut min = [f32::INFINITY; N];
        let mut max = [f32::NEG_INFINITY; N];
        for value in values {
            for (i, component) in value.into_iter().enumerate() {
                min[i] = min[i].min(component);
                max[i] = max[i].max(component);
                self.bin.extend_from_slice(&component.to_le_bytes());
            }
            count += 1;
        }
        let view = self.end_view(offset, target);
        let kind = match N {
            1 => "SCALAR",
            3 => "VEC3",
            _ => "VEC4",
        };
        let mut accessor = json!({
            "bufferView": view,
            "componentType": FLOAT,
            "count": count,
            "type": kind,
        });
        if with_bounds && count > 0 {
            accessor["min"] = exact(min);
            accessor["max"] = exact(max);
        }
        self.push_accessor(accessor)
    }

    /// Accessors of one scene mesh, written to the buffer the first time it is used.
    fn primitive(&mut self, mesh_index: usize) -> Primitive {
        if let Some(primitive) = self.primitives.get(&mesh_index) {
            return *primitive;
        }
        let scene = self.scene;
        let mesh = &scene.meshes[mesh_index];

        let position =
            self.float_accessor(mesh.positions.iter().copied(), Some(ARRAY_BUFFER), true);
        let normal = self
            .extra
            .normals
            .then(|| self.float_accessor(mesh.normals.iter().copied(), Some(ARRAY_BUFFER), false));

        // Colour is linear RGBA in normalised 16-bit integers: half the size of floats,
        // 256 times finer than the 8-bit steps that band in dark linear values, and one
        // of the three encodings every glTF reader must accept for COLOR_0.
        let offset = self.begin_view();
        for color in &mesh.colors {
            for component in color {
                let q = (component.clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
                self.bin.extend_from_slice(&q.to_le_bytes());
            }
        }
        let view = self.end_view(offset, Some(ARRAY_BUFFER));
        let color = self.push_accessor(json!({
            "bufferView": view,
            "componentType": UNSIGNED_SHORT,
            "normalized": true,
            "count": mesh.colors.len(),
            "type": "VEC4",
        }));

        // 65535 is the primitive-restart value of 16-bit indices, so it is not a usable index.
        let short = mesh.positions.len() <= 65_535;
        let offset = self.begin_view();
        for index in &mesh.indices {
            if short {
                self.bin.extend_from_slice(&(*index as u16).to_le_bytes());
            } else {
                self.bin.extend_from_slice(&index.to_le_bytes());
            }
        }
        let view = self.end_view(offset, Some(ELEMENT_ARRAY_BUFFER));
        let indices = self.push_accessor(json!({
            "bufferView": view,
            "componentType": if short { UNSIGNED_SHORT } else { UNSIGNED_INT },
            "count": mesh.indices.len(),
            "type": "SCALAR",
        }));

        let primitive = Primitive {
            position,
            normal,
            color,
            indices,
        };
        self.primitives.insert(mesh_index, primitive);
        primitive
    }

    // ----- materials and meshes -----

    fn material(&mut self, key: MaterialKey, owner_name: &str) -> usize {
        if let Some(index) = self.material_keys.get(&key) {
            return *index;
        }
        let mut name = String::new();
        if key.owner.is_some() {
            name.push_str(owner_name);
            name.push(' ');
        }
        name.push_str(if key.lit { "lit" } else { "unlit" });
        if key.double_sided {
            name.push_str(" double-sided");
        }
        if key.blend {
            name.push_str(" see-through");
        }
        let alpha = f32::from_bits(key.alpha);
        let mut pbr = Map::new();
        // Paint is matte: without this a reader that ignores the unlit extension, or a
        // lit layer, would show the default fully metallic surface.
        pbr.insert("metallicFactor".into(), json!(0.0));
        // An animated material always spells the factor out: the animation points at it.
        if alpha != 1.0 || key.owner.is_some() {
            pbr.insert("baseColorFactor".into(), nums([1.0, 1.0, 1.0, alpha]));
        }
        let mut material = json!({ "name": name, "pbrMetallicRoughness": pbr });
        if key.double_sided {
            material["doubleSided"] = json!(true);
        }
        if key.blend {
            material["alphaMode"] = json!("BLEND");
        }
        if !key.lit {
            material["extensions"] = json!({ UNLIT: {} });
            self.used.insert(UNLIT);
        }
        let index = self.materials.len();
        self.materials.push(material);
        self.material_keys.insert(key, index);
        index
    }

    /// The glTF mesh for some scene meshes drawn by `node`.
    fn mesh(
        &mut self,
        node_index: usize,
        shading: Shading,
        scene_meshes: &[usize],
        name: &str,
    ) -> usize {
        let scene = self.scene;
        let node = &scene.nodes[node_index];
        let animated = self.extra.animate_opacity && node.opacity.is_animated();
        let opacity = node.opacity.first().clamp(0.0, 1.0);

        let mut key = Vec::with_capacity(scene_meshes.len());
        for mesh_index in scene_meshes {
            let mesh = &scene.meshes[*mesh_index];
            let material_key = MaterialKey {
                lit: shading == Shading::Lit,
                double_sided: mesh.double_sided,
                // Blending is only for what is really see-through: opaque paint keeps
                // depth writes and needs no sorting.
                blend: mesh.has_transparency || animated || opacity < OPAQUE_THRESHOLD,
                alpha: opacity.to_bits(),
                owner: animated.then_some(node_index),
            };
            let is_new = !self.material_keys.contains_key(&material_key);
            let material = self.material(material_key, &node.name);
            if animated && is_new {
                if let Sampled::PerFrame(samples) = &node.opacity {
                    let values: Vec<[f32; 4]> = samples
                        .iter()
                        .map(|o| [1.0, 1.0, 1.0, o.clamp(0.0, 1.0)])
                        .collect();
                    let pointer =
                        format!("/materials/{material}/pbrMetallicRoughness/baseColorFactor");
                    if self.add_channel(Target::Pointer(pointer), &values, false) {
                        self.used.insert(POINTER);
                    }
                }
            }
            key.push((*mesh_index, material));
        }
        if let Some(index) = self.mesh_keys.get(&key) {
            return *index;
        }

        let mut primitives = Vec::with_capacity(key.len());
        for (mesh_index, material) in &key {
            let primitive = self.primitive(*mesh_index);
            let mut attributes = Map::new();
            attributes.insert("POSITION".into(), json!(primitive.position));
            if let Some(normal) = primitive.normal {
                attributes.insert("NORMAL".into(), json!(normal));
            }
            attributes.insert("COLOR_0".into(), json!(primitive.color));
            primitives.push(json!({
                "attributes": attributes,
                "indices": primitive.indices,
                "material": material,
                "mode": 4,
            }));
        }
        let index = self.meshes.len();
        self.meshes
            .push(json!({ "name": name, "primitives": primitives }));
        self.mesh_keys.insert(key, index);
        index
    }

    // ----- animation -----

    fn time_accessor(&mut self, frames: &[u32]) -> usize {
        if let Some(index) = self.time_accessors.get(frames) {
            return *index;
        }
        let scene = self.scene;
        let index = self.float_accessor(
            frames.iter().map(|k| [scene.seconds(*k as usize) as f32]),
            None,
            true,
        );
        self.time_accessors.insert(frames.to_vec(), index);
        index
    }

    fn push_channel(&mut self, target: Target, input: usize, output: usize, step: bool) {
        let sampler = self.samplers.len();
        self.samplers.push(json!({
            "input": input,
            "output": output,
            "interpolation": if step { "STEP" } else { "LINEAR" },
        }));
        self.channels
            .push(json!({ "sampler": sampler, "target": target.json() }));
    }

    /// Writes per-frame values as a channel. Returns false, writing nothing, when the
    /// values never change.
    fn add_channel<const N: usize>(
        &mut self,
        target: Target,
        values: &[[f32; N]],
        step: bool,
    ) -> bool {
        if values.iter().all(|v| *v == values[0]) {
            return false;
        }
        self.write_channel(target, values, step);
        true
    }

    /// Writes per-frame values as a channel even when they never change.
    fn write_channel<const N: usize>(&mut self, target: Target, values: &[[f32; N]], step: bool) {
        let frames = kept_frames(values, step);
        let input = self.time_accessor(&frames);
        let output = self.float_accessor(frames.iter().map(|k| values[*k as usize]), None, false);
        self.push_channel(target, input, output, step);
    }

    /// Writes a boolean property through `KHR_animation_pointer`: one unsigned byte per
    /// key, zero for false, stepped.
    fn add_bool_channel(&mut self, pointer: String, values: &[bool]) -> bool {
        if values.len() < 2 || values.iter().all(|v| *v == values[0]) {
            return false;
        }
        let frames = kept_frames(values, true);
        let input = self.time_accessor(&frames);
        let offset = self.begin_view();
        for frame in &frames {
            self.bin.push(u8::from(values[*frame as usize]));
        }
        let view = self.end_view(offset, None);
        let output = self.push_accessor(json!({
            "bufferView": view,
            "componentType": UNSIGNED_BYTE,
            "count": frames.len(),
            "type": "SCALAR",
        }));
        self.push_channel(Target::Pointer(pointer), input, output, true);
        self.used.insert(POINTER);
        true
    }

    /// Writes `KHR_node_visibility` for a glTF node when the option asks for it: the
    /// state on the first frame, and a pointer channel when it changes. Nodes that are
    /// always visible get nothing, which is the extension's default.
    fn node_visibility(&mut self, gltf_node: usize, visible: &Sampled<bool>) {
        if !self.extra.node_visibility {
            return;
        }
        let animated = match visible {
            Sampled::PerFrame(samples) => {
                let pointer = format!("/nodes/{gltf_node}/extensions/{VISIBILITY}/visible");
                self.add_bool_channel(pointer, samples)
            }
            Sampled::Constant(_) => false,
        };
        if animated || !*visible.first() {
            self.nodes[gltf_node]["extensions"][VISIBILITY] =
                json!({ "visible": *visible.first() });
            self.used.insert(VISIBILITY);
        }
    }

    /// Transform and visibility channels of a scene node.
    fn animate_node(&mut self, node: &ExportNode, gltf_node: usize) {
        let frames = self.scene.frame_count();
        if frames < 2 || !(node.transform.is_animated() || node.visible.is_animated()) {
            return;
        }
        let mut translations = Vec::with_capacity(frames);
        let mut rotations: Vec<[f32; 4]> = Vec::with_capacity(frames);
        let mut scales = Vec::with_capacity(frames);
        for frame in 0..frames {
            let (t, mut r, s) = trs(node.transform.at(frame), *node.visible.at(frame));
            // q and -q are the same rotation, but interpolating between them goes the
            // long way round: stay on the previous sample's hemisphere.
            if let Some(previous) = rotations.last() {
                let dot: f32 = previous.iter().zip(&r).map(|(a, b)| a * b).sum();
                if dot < 0.0 {
                    r = r.map(|c| -c);
                }
            }
            translations.push(t);
            rotations.push(r);
            scales.push(s);
        }
        let target = |path| Target::Node {
            node: gltf_node,
            path,
        };
        self.add_channel(target("translation"), &translations, false);
        // A node that is hidden by a zero scale has a singular matrix. Importers that
        // keep matrices and rebuild rotation from them (Godot's does) come back with no
        // rotation once the node is shown again, unless the animation states it. So a
        // rotated node that is ever hidden keeps its rotation channel even when constant.
        let hidden_while_rotated =
            node.visible.is_animated() && rotations[0] != [0.0, 0.0, 0.0, 1.0];
        if !self.add_channel(target("rotation"), &rotations, false) && hidden_while_rotated {
            self.write_channel(target("rotation"), &rotations, false);
        }
        // Showing and hiding must not be interpolated, so a scale channel that carries
        // visibility is stepped as a whole.
        self.add_channel(target("scale"), &scales, node.visible.is_animated());
    }

    // ----- nodes -----

    fn light(&mut self, name: &str, light: &ExportLight) -> usize {
        // glTF wants colour components within 0..=1; brighter colours move into the intensity.
        let color = light
            .color
            .map(|c| if c.is_finite() { c.max(0.0) } else { 0.0 });
        let peak = color.iter().copied().fold(1.0f32, f32::max);
        let mut value = json!({
            "name": name,
            "type": match light.kind {
                LightKind::Point => "point",
                LightKind::Spot => "spot",
                LightKind::Directional => "directional",
            },
            "color": nums(color.map(|c| c / peak)),
            "intensity": num((light.energy * peak).max(0.0)),
            "extras": { "limner": {
                "energy": num(light.energy),
                "range": num(light.range),
                "spotAngle": num(light.spot_angle),
                "spotBlend": num(light.spot_blend),
                "shadows": light.shadows,
            } },
        });
        if light.kind != LightKind::Directional && light.world_range > 0.0 {
            value["range"] = num(light.world_range);
        }
        if light.kind == LightKind::Spot {
            let limit = std::f32::consts::FRAC_PI_2;
            let outer = light.spot_angle.to_radians().clamp(1e-3, limit);
            // The inner cone must be strictly inside the outer one.
            let inner = (outer * (1.0 - light.spot_blend.clamp(0.0, 1.0))).min(outer - 1e-4);
            value["spot"] = json!({
                "innerConeAngle": num(inner.max(0.0)),
                "outerConeAngle": num(outer),
            });
        }
        self.lights.push(value);
        self.used.insert(LIGHTS);
        self.lights.len() - 1
    }

    fn paint(&mut self, node_index: usize, gltf_node: usize, paint: &ExportPaint) {
        let scene = self.scene;
        let node = &scene.nodes[node_index];
        // The usual case, a layer showing one drawing throughout, needs no extra node.
        if let [only] = paint.drawings.as_slice() {
            if only.shown == Sampled::Constant(true) {
                let mesh = self.mesh(node_index, paint.shading, &only.meshes, &node.name);
                self.nodes[gltf_node]["mesh"] = json!(mesh);
                return;
            }
        }
        // Frame-by-frame: one child per drawing, switched by a stepped scale.
        for drawing in &paint.drawings {
            let name = format!("{}.{:03}", node.name, drawing.ordinal);
            let mesh = self.mesh(node_index, paint.shading, &drawing.meshes, &name);
            let child = self.nodes.len();
            let mut value = json!({
                "name": name,
                "mesh": mesh,
                "extras": { "limner": {
                    "layerId": node.layer.0,
                    "drawingId": drawing.drawing.0,
                    "drawing": drawing.ordinal,
                } },
            });
            if !*drawing.shown.first() {
                value["scale"] = nums([0.0; 3]);
            }
            self.nodes.push(value);
            push_child(&mut self.nodes[gltf_node], child);
            if let Sampled::PerFrame(shown) = &drawing.shown {
                let scales: Vec<[f32; 3]> = shown
                    .iter()
                    .map(|s| if *s { [1.0; 3] } else { [0.0; 3] })
                    .collect();
                let target = Target::Node {
                    node: child,
                    path: "scale",
                };
                self.add_channel(target, &scales, true);
            }
            self.node_visibility(child, &drawing.shown);
        }
    }

    fn build_nodes(&mut self) {
        let scene = self.scene;
        // Scene node `i` is glTF node `i`; the per-drawing children come after them.
        for node in &scene.nodes {
            let (t, r, s) = trs(node.transform.first(), *node.visible.first());
            let mut value = json!({
                "name": node.name,
                "extras": { "limner": { "layerId": node.layer.0, "kind": node.kind.as_str() } },
            });
            if t != [0.0; 3] {
                value["translation"] = nums(t);
            }
            if r != [0.0, 0.0, 0.0, 1.0] {
                value["rotation"] = nums(r);
            }
            if s != [1.0; 3] {
                value["scale"] = nums(s);
            }
            if !node.children.is_empty() {
                value["children"] = json!(node.children);
            }
            self.nodes.push(value);
        }
        for (index, node) in scene.nodes.iter().enumerate() {
            match &node.content {
                NodeContent::None => {}
                NodeContent::Paint(paint) => self.paint(index, index, paint),
                NodeContent::Light(light) => {
                    let light = self.light(&node.name, light);
                    self.nodes[index]["extensions"][LIGHTS] = json!({ "light": light });
                }
                NodeContent::Camera(camera) => {
                    let [width, height] = camera.resolution;
                    let aspect = if width > 0 && height > 0 {
                        width as f32 / height as f32
                    } else {
                        16.0 / 9.0
                    };
                    self.cameras.push(json!({
                        "name": node.name,
                        "type": "perspective",
                        "perspective": {
                            "yfov": num(camera.fov.clamp(1.0, 179.0).to_radians()),
                            "aspectRatio": num(aspect),
                            "znear": num(CAMERA_ZNEAR),
                            "zfar": num(CAMERA_ZFAR),
                        },
                    }));
                    self.nodes[index]["camera"] = json!(self.cameras.len() - 1);
                }
                NodeContent::Media(media) => {
                    let limner = &mut self.nodes[index]["extras"]["limner"];
                    limner["media"] = json!(media.name);
                    limner["sourcePath"] = json!(media.source_path);
                    if let Some(id) = media.media {
                        limner["mediaId"] = json!(id.0);
                    }
                }
            }
            self.animate_node(node, index);
            self.node_visibility(index, &node.visible);
        }
    }

    fn finish(mut self) -> Result<GlbOutput, ExportError> {
        self.build_nodes();
        let scene = self.scene;

        let title = if scene.name.is_empty() {
            "Limner scene"
        } else {
            scene.name.as_str()
        };
        let mut root = Map::new();
        root.insert(
            "asset".into(),
            json!({
                "version": "2.0",
                "generator": concat!("Limner ", env!("CARGO_PKG_VERSION"), " (limner-export)"),
            }),
        );
        let mut scene_value = json!({
            "name": title,
            "extras": { "limner": {
                "frameRate": [scene.rate.num, scene.rate.den],
                "startSeconds": limner_core::time::to_seconds(scene.times[0]),
                "frames": scene.frame_count(),
                "baked": scene.baked,
                "background": nums(scene.background),
            } },
        });
        if !scene.roots.is_empty() {
            scene_value["nodes"] = json!(scene.roots);
        }
        root.insert("scene".into(), json!(0));
        root.insert("scenes".into(), json!([scene_value]));

        let animation_channels = self.channels.len();
        if !self.channels.is_empty() {
            root.insert(
                "animations".into(),
                json!([{
                    "name": title,
                    "samplers": self.samplers,
                    "channels": self.channels,
                }]),
            );
        }
        if !self.lights.is_empty() {
            root.insert(
                "extensions".into(),
                json!({ LIGHTS: { "lights": self.lights } }),
            );
        }
        let lists = [
            ("nodes", self.nodes),
            ("meshes", self.meshes),
            ("materials", self.materials),
            ("cameras", self.cameras),
            ("accessors", self.accessors),
            ("bufferViews", self.views),
        ];
        let node_count = lists[0].1.len();
        for (name, list) in lists {
            // glTF forbids empty arrays.
            if !list.is_empty() {
                root.insert(name.into(), Value::Array(list));
            }
        }
        if !self.bin.is_empty() {
            root.insert("buffers".into(), json!([{ "byteLength": self.bin.len() }]));
        }
        if !self.used.is_empty() {
            root.insert("extensionsUsed".into(), json!(self.used));
        }

        let mut text = serde_json::to_vec(&Value::Object(root)).map_err(ExportError::Json)?;
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let bin_chunk = if self.bin.is_empty() {
            0
        } else {
            8 + self.bin.len()
        };
        let total = 12 + 8 + text.len() + bin_chunk;
        if total > u32::MAX as usize {
            return Err(ExportError::TooLarge { bytes: total });
        }
        let mut bytes = Vec::with_capacity(total);
        bytes.extend_from_slice(&GLB_MAGIC.to_le_bytes());
        bytes.extend_from_slice(&GLB_VERSION.to_le_bytes());
        bytes.extend_from_slice(&(total as u32).to_le_bytes());
        bytes.extend_from_slice(&(text.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&CHUNK_JSON.to_le_bytes());
        bytes.extend_from_slice(&text);
        if !self.bin.is_empty() {
            bytes.extend_from_slice(&(self.bin.len() as u32).to_le_bytes());
            bytes.extend_from_slice(&CHUNK_BIN.to_le_bytes());
            bytes.extend_from_slice(&self.bin);
        }

        let written = self.primitives.keys().map(|index| &scene.meshes[*index]);
        let report = ExportReport {
            nodes: node_count,
            meshes: self.primitives.len(),
            vertices: written.clone().map(|m| m.vertex_count()).sum(),
            triangles: written.map(|m| m.triangle_count()).sum(),
            animation_frames: if scene.is_animated() {
                scene.frame_count()
            } else {
                0
            },
            animation_channels,
            bytes: bytes.len() as u64,
        };
        Ok(GlbOutput { bytes, report })
    }
}

fn push_child(node: &mut Value, child: usize) {
    match node.get_mut("children").and_then(Value::as_array_mut) {
        Some(children) => children.push(json!(child)),
        None => node["children"] = json!([child]),
    }
}

/// Writes an export scene as a binary glTF file.
pub fn scene_to_glb(scene: &ExportScene, extra: &GltfOptions) -> Result<GlbOutput, ExportError> {
    Writer::new(scene, extra).finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_of_equal_samples_are_merged() {
        let values = [1, 1, 1, 2, 2, 2, 2, 3];
        assert_eq!(kept_frames(&values, true), vec![0, 3, 7]);
        // Linear keeps both ends of each hold so the hold is not interpolated across.
        assert_eq!(kept_frames(&values, false), vec![0, 2, 3, 6, 7]);
        // Stepped samplers still end on the last frame, so the animation keeps its length.
        assert_eq!(kept_frames(&[5, 6, 6, 6], true), vec![0, 1, 3]);
    }

    #[test]
    fn numbers_are_written_as_the_shortest_round_tripping_decimal() {
        assert_eq!(num(0.1).to_string(), "0.1");
        assert_eq!(num(1.0).to_string(), "1.0");
        assert_eq!(num(f32::NAN).to_string(), "0.0");
        assert_eq!(exact([0.1f32]).to_string(), "[0.10000000149011612]");
    }

    #[test]
    fn a_mirrored_transform_becomes_a_negative_scale_on_its_flip_axis() {
        use limner_core::glam::{DQuat, DVec3};
        use limner_core::math::Flip;
        let x = Xform::new(DQuat::IDENTITY, 2.0, DVec3::new(1.0, 2.0, 3.0), Flip::Y);
        assert_eq!(
            trs(&x, true),
            ([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0], [2.0, -2.0, 2.0])
        );
        assert_eq!(trs(&x, false).2, [0.0; 3]);
    }
}
