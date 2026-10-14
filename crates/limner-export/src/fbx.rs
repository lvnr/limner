//! Binary FBX 7.4 writer.
//!
//! FBX has no public specification; this writes the subset every importer agrees on,
//! in the binary container (Blender reads no other kind). Arrays are stored raw, so
//! no compression code is needed. The mapping of each Limner concept is described in
//! `docs/export.md`; the short version:
//!
//! * the file declares Y up, right-handed, like Limner, so no axis is converted;
//! * numbers are metres and the file says so (`UnitScaleFactor` 100: one unit is 100
//!   centimetres), or real centimetres on request ([`FbxUnit`]);
//! * one `Model` per layer with `Lcl Translation`, `Lcl Rotation` (Euler XYZ in
//!   degrees) and `Lcl Scaling`; a mirrored layer has a negative scale on its flip axis;
//! * one `Geometry` per drawing, with normals per vertex and RGBA colours per polygon
//!   corner, and one material slot per kind of surface;
//! * one animation stack of sampled curves; visibility is written twice, as
//!   `Visibility` curves and as a scale of zero, because many importers ignore the first.

use crate::common::{
    camera_aspect, camera_fov, light_color_and_intensity, printable, title, transforms, visibility,
    GENERATOR,
};
use crate::gltf::{kept_frames, CAMERA_ZFAR, CAMERA_ZNEAR};
use crate::obj::linear_to_srgb;
use crate::scene::{
    ExportCamera, ExportLight, ExportNode, ExportPaint, ExportScene, NodeContent, Sampled,
};
use crate::{ExportError, ExportReport};
use limner_core::glam::{DMat3, DQuat};
use limner_core::model::{LightKind, Shading, OPAQUE_THRESHOLD};
use limner_core::time::{Flicks, FLICKS_PER_SECOND};
use std::collections::HashMap;

/// The FBX version written: 7.4, the newest one with 32-bit offsets, which is what
/// Blender, Unity, Unreal and ufbx all read.
pub const FBX_VERSION: u32 = 7400;

/// FBX measures time in these units per second.
pub const KTIME_PER_SECOND: i64 = 46_186_158_000;

/// Height of the camera's film back in millimetres. Only its ratio to the focal
/// length matters; this is a full-frame sensor.
pub const CAMERA_FILM_HEIGHT_MM: f64 = 24.0;

/// The length unit of the numbers in the file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FbxUnit {
    /// Positions stay in metres and the file declares a unit of 100 centimetres.
    /// Blender, Unity and anything built on ufbx honour the declaration.
    #[default]
    Metres,
    /// Positions are multiplied by 100 and the file declares FBX's native
    /// centimetre. For importers that ignore the declared unit, such as Unreal's
    /// classic FBX importer with its default settings.
    Centimetres,
}

impl FbxUnit {
    /// What positions are multiplied by.
    pub fn scale(self) -> f64 {
        match self {
            FbxUnit::Metres => 1.0,
            FbxUnit::Centimetres => 100.0,
        }
    }

    /// The file's `UnitScaleFactor`: centimetres per unit.
    pub fn centimetres_per_unit(self) -> f64 {
        match self {
            FbxUnit::Metres => 100.0,
            FbxUnit::Centimetres => 1.0,
        }
    }
}

/// How vertex colours are encoded. FBX itself does not say what the numbers mean.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FbxColorSpace {
    /// sRGB-encoded, which is what Blender's importer assumes by default and what
    /// survives the 8 bits per channel that game engines store vertex colours in.
    #[default]
    Srgb,
    /// Limner's own linear values, unchanged.
    Linear,
}

/// Choices that only concern FBX.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FbxOptions {
    pub unit: FbxUnit,
    pub color_space: FbxColorSpace,
    /// Write normals. Without them every importer computes its own.
    pub normals: bool,
}

impl Default for FbxOptions {
    fn default() -> Self {
        Self {
            unit: FbxUnit::default(),
            color_space: FbxColorSpace::default(),
            normals: true,
        }
    }
}

/// A time span in FBX's units. Exact whenever the span is a whole number of them,
/// which holds for frames of every common rate.
pub fn ktime(span: Flicks) -> i64 {
    let scaled = i128::from(span) * i128::from(KTIME_PER_SECOND);
    let per_second = i128::from(FLICKS_PER_SECOND);
    (scaled + per_second / 2).div_euclid(per_second) as i64
}

// ----- the container -----

#[derive(Clone, Debug, PartialEq)]
enum Prop {
    Bool(bool),
    I32(i32),
    I64(i64),
    F64(f64),
    Str(Vec<u8>),
    Raw(Vec<u8>),
    I32s(Vec<i32>),
    I64s(Vec<i64>),
    F32s(Vec<f32>),
    F64s(Vec<f64>),
}

impl From<bool> for Prop {
    fn from(value: bool) -> Self {
        Prop::Bool(value)
    }
}
impl From<i32> for Prop {
    fn from(value: i32) -> Self {
        Prop::I32(value)
    }
}
impl From<i64> for Prop {
    fn from(value: i64) -> Self {
        Prop::I64(value)
    }
}
impl From<f64> for Prop {
    fn from(value: f64) -> Self {
        Prop::F64(if value.is_finite() { value } else { 0.0 })
    }
}
impl From<&str> for Prop {
    fn from(value: &str) -> Self {
        Prop::Str(value.as_bytes().to_vec())
    }
}
impl From<Vec<i32>> for Prop {
    fn from(value: Vec<i32>) -> Self {
        Prop::I32s(value)
    }
}
impl From<Vec<i64>> for Prop {
    fn from(value: Vec<i64>) -> Self {
        Prop::I64s(value)
    }
}
impl From<Vec<f32>> for Prop {
    fn from(value: Vec<f32>) -> Self {
        Prop::F32s(value)
    }
}
impl From<Vec<f64>> for Prop {
    fn from(value: Vec<f64>) -> Self {
        Prop::F64s(value)
    }
}

/// One record of the file: a name, values, and nested records.
#[derive(Clone, Debug, PartialEq)]
struct Node {
    name: &'static str,
    props: Vec<Prop>,
    children: Vec<Node>,
}

impl Node {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            props: Vec::new(),
            children: Vec::new(),
        }
    }

    fn with(mut self, prop: impl Into<Prop>) -> Self {
        self.props.push(prop.into());
        self
    }

    fn child(mut self, node: Node) -> Self {
        self.children.push(node);
        self
    }

    fn children(mut self, nodes: impl IntoIterator<Item = Node>) -> Self {
        self.children.extend(nodes);
        self
    }
}

/// A record with one value.
fn leaf(name: &'static str, value: impl Into<Prop>) -> Node {
    Node::new(name).with(value)
}

/// The name of an object: its own name and its class, joined the way FBX stores them.
fn object_name(name: &str, class: &str) -> Prop {
    let mut bytes = printable(name).into_bytes();
    bytes.extend_from_slice(&[0, 1]);
    bytes.extend_from_slice(class.as_bytes());
    Prop::Str(bytes)
}

/// A `P` record of a `Properties70` block: name, type, label, flags, then values.
fn p(name: &str, kind: &str, label: &str, flags: &str) -> Node {
    Node::new("P").with(name).with(kind).with(label).with(flags)
}

fn p_int(name: &str, value: i32) -> Node {
    p(name, "int", "Integer", "").with(value)
}

fn p_enum(name: &str, value: i32) -> Node {
    p(name, "enum", "", "").with(value)
}

fn p_bool(name: &str, value: bool) -> Node {
    p(name, "bool", "", "").with(i32::from(value))
}

fn p_double(name: &str, value: f64) -> Node {
    p(name, "double", "Number", "").with(value)
}

/// An animatable number.
fn p_number(name: &str, value: f64) -> Node {
    p(name, "Number", "", "A").with(value)
}

fn p_color(name: &str, value: [f64; 3]) -> Node {
    p(name, "Color", "", "A")
        .with(value[0])
        .with(value[1])
        .with(value[2])
}

fn p_string(name: &str, value: &str) -> Node {
    p(name, "KString", "", "").with(value)
}

fn p_time(name: &str, value: i64) -> Node {
    p(name, "KTime", "Time", "").with(value)
}

/// A user property: importers that keep custom properties show it on the object.
fn p_user(name: &str, value: &str) -> Node {
    p(name, "KString", "", "U").with(printable(value).as_str())
}

/// `flags` for a property that can be animated, marked when it is.
fn animatable(animated: bool) -> &'static str {
    if animated {
        "A+"
    } else {
        "A"
    }
}

const MAGIC: &[u8; 23] = b"Kaydara FBX Binary  \x00\x1a\x00";
/// Records that close a list of nested records.
const SENTINEL: [u8; 13] = [0; 13];
/// The file id and footer bytes Blender's exporter writes. Real FBX SDK files derive
/// them from the creation time; importers do not check them, and fixed values make
/// the same scene always produce the same bytes.
const FILE_ID: [u8; 16] = [
    0x28, 0xb3, 0x2a, 0xeb, 0xb6, 0x24, 0xcc, 0xc2, 0xbf, 0xc8, 0xb0, 0x2a, 0xa9, 0x2b, 0xfc, 0xf1,
];
const CREATION_TIME: &str = "1970-01-01 10:00:00:000";
const FOOTER_ID: [u8; 16] = [
    0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e,
];
const FOOTER_MAGIC: [u8; 16] = [
    0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b,
];

fn write_array<const N: usize, T: Copy>(
    out: &mut Vec<u8>,
    code: u8,
    values: &[T],
    bytes: impl Fn(T) -> [u8; N],
) {
    out.push(code);
    out.extend_from_slice(&(values.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // encoding: raw
    out.extend_from_slice(&((values.len() * N) as u32).to_le_bytes());
    out.reserve(values.len() * N);
    for value in values {
        out.extend_from_slice(&bytes(*value));
    }
}

fn write_prop(out: &mut Vec<u8>, prop: &Prop) {
    match prop {
        Prop::Bool(value) => out.extend_from_slice(&[b'C', u8::from(*value)]),
        Prop::I32(value) => {
            out.push(b'I');
            out.extend_from_slice(&value.to_le_bytes());
        }
        Prop::I64(value) => {
            out.push(b'L');
            out.extend_from_slice(&value.to_le_bytes());
        }
        Prop::F64(value) => {
            out.push(b'D');
            out.extend_from_slice(&value.to_le_bytes());
        }
        Prop::Str(bytes) | Prop::Raw(bytes) => {
            out.push(if matches!(prop, Prop::Str(_)) {
                b'S'
            } else {
                b'R'
            });
            out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            out.extend_from_slice(bytes);
        }
        Prop::I32s(values) => write_array(out, b'i', values, i32::to_le_bytes),
        Prop::I64s(values) => write_array(out, b'l', values, i64::to_le_bytes),
        Prop::F32s(values) => write_array(out, b'f', values, f32::to_le_bytes),
        Prop::F64s(values) => write_array(out, b'd', values, f64::to_le_bytes),
    }
}

/// Writes one record. Offsets are absolute and 32-bit; the caller checks afterwards
/// that the file stayed below 4 GiB.
///
/// A record with nested records ends with a sentinel. One with neither values nor
/// nested records gets a sentinel too unless it is the last of its list, and so do
/// `AnimationStack` and `AnimationLayer`. These are the FBX SDK's own habits, which
/// Blender's exporter copies; following them keeps the file byte-compatible with what
/// importers are tested against.
fn write_node(out: &mut Vec<u8>, node: &Node, is_last: bool) {
    let start = out.len();
    out.extend_from_slice(&[0; 12]);
    out.push(node.name.len() as u8);
    out.extend_from_slice(node.name.as_bytes());
    let props_start = out.len();
    for prop in &node.props {
        write_prop(out, prop);
    }
    let props_length = out.len() - props_start;
    if !node.children.is_empty() {
        write_nodes(out, &node.children);
    } else if (node.props.is_empty() || matches!(node.name, "AnimationStack" | "AnimationLayer"))
        && !is_last
    {
        out.extend_from_slice(&SENTINEL);
    }
    let end = out.len() as u32;
    out[start..start + 4].copy_from_slice(&end.to_le_bytes());
    out[start + 4..start + 8].copy_from_slice(&(node.props.len() as u32).to_le_bytes());
    out[start + 8..start + 12].copy_from_slice(&(props_length as u32).to_le_bytes());
}

/// Writes a list of records and the sentinel that closes it.
fn write_nodes(out: &mut Vec<u8>, nodes: &[Node]) {
    for (index, node) in nodes.iter().enumerate() {
        write_node(out, node, index + 1 == nodes.len());
    }
    out.extend_from_slice(&SENTINEL);
}

fn write_file(nodes: &[Node]) -> Result<Vec<u8>, ExportError> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&FBX_VERSION.to_le_bytes());
    write_nodes(&mut out, nodes);
    out.extend_from_slice(&FOOTER_ID);
    out.extend_from_slice(&[0; 4]);
    // Pad to 16 bytes; a file that is already aligned gets a full 16.
    let padding = 16 - out.len() % 16;
    out.resize(out.len() + padding, 0);
    out.extend_from_slice(&FBX_VERSION.to_le_bytes());
    out.extend_from_slice(&[0; 120]);
    out.extend_from_slice(&FOOTER_MAGIC);
    if out.len() > u32::MAX as usize {
        return Err(ExportError::FileTooLarge {
            format: "FBX 7.4",
            bytes: out.len(),
        });
    }
    Ok(out)
}

// ----- rotations -----

/// Lights shine along -Y in FBX and along -Z in Limner: a quarter turn about X takes
/// the first to the second.
fn light_fix() -> DQuat {
    DQuat::from_rotation_x(std::f64::consts::FRAC_PI_2)
}

/// Cameras look along +X in FBX and along -Z in Limner, both with Y up: a quarter
/// turn about Y takes the first to the second.
fn camera_fix() -> DQuat {
    DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2)
}

/// Which kind of node a transform belongs to, for the axis FBX expects it to point along.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Aim {
    Plain,
    Light,
    Camera,
}

/// Rotation and scale of a node as FBX wants them. For lights and cameras the
/// rotation gains the quarter turn, and the per-axis scale is carried through it
/// (the turn swaps two axes).
fn aimed(rotation: DQuat, scale: [f64; 3], aim: Aim) -> (DQuat, [f64; 3]) {
    let [x, y, z] = scale;
    match aim {
        Aim::Plain => (rotation, scale),
        Aim::Light => (rotation * light_fix(), [x, z, y]),
        Aim::Camera => (rotation * camera_fix(), [z, y, x]),
    }
}

/// Euler angles in degrees for FBX's default rotation order: about X first, then Y,
/// then Z, each about the parent's axes. `previous` is the triple of the frame before;
/// of the many triples that mean the same rotation, the one nearest to it is returned,
/// so curves never jump by a full or half turn.
fn euler_xyz(rotation: DQuat, previous: Option<[f64; 3]>) -> [f64; 3] {
    let m = DMat3::from_quat(rotation);
    // m = Rz(c) * Ry(b) * Rx(a); element (row, column) is m.col(column)[row].
    let sin_b = (-m.x_axis.z).clamp(-1.0, 1.0);
    let (a, b, c);
    if sin_b.abs() > 1.0 - 1e-12 {
        // Looking straight up or down the Y turn, X and Z turn about the same axis and
        // only their sum (or difference) is defined. Keep Z where it was.
        c = previous.map_or(0.0, |p| p[2].to_radians());
        b = sin_b.asin();
        let around = (-m.z_axis.y).atan2(m.y_axis.y);
        a = around + sin_b.signum() * c;
    } else {
        a = m.y_axis.z.atan2(m.z_axis.z);
        b = sin_b.asin();
        c = m.x_axis.y.atan2(m.x_axis.x);
    }
    let direct = [a.to_degrees(), b.to_degrees(), c.to_degrees()];
    let Some(previous) = previous else {
        return direct;
    };
    // The same rotation the other way round the Y turn.
    let other = [direct[0] + 180.0, 180.0 - direct[1], direct[2] + 180.0];
    let near = |candidate: [f64; 3]| {
        let mut out = [0.0; 3];
        for i in 0..3 {
            let turns = ((previous[i] - candidate[i]) / 360.0).round();
            out[i] = candidate[i] + turns * 360.0;
        }
        out
    };
    let distance = |candidate: &[f64; 3]| -> f64 {
        candidate
            .iter()
            .zip(&previous)
            .map(|(a, b)| (a - b).abs())
            .sum()
    };
    let (direct, other) = (near(direct), near(other));
    if distance(&other) < distance(&direct) {
        other
    } else {
        direct
    }
}

// ----- the scene -----

/// A material slot: the look of one kind of surface.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Look {
    lit: bool,
    double_sided: bool,
    see_through: bool,
}

impl Look {
    fn name(self) -> String {
        let mut name = String::from(if self.lit { "lit" } else { "unlit" });
        if self.double_sided {
            name.push_str(" double-sided");
        }
        if self.see_through {
            name.push_str(" see-through");
        }
        name
    }
}

/// A geometry that was written, and the surface kinds of its material slots
/// (double-sided, see-through), in slot order.
#[derive(Clone)]
struct Geometry {
    id: i64,
    slots: Vec<(bool, bool)>,
}

/// What a model looks like on every sampled frame.
struct Pose {
    translations: Vec<[f64; 3]>,
    /// Euler XYZ in degrees, unrolled.
    rotations: Vec<[f64; 3]>,
    /// Zero on frames where the node is hidden.
    scales: Vec<[f64; 3]>,
    visible: Vec<bool>,
}

impl Pose {
    /// A node that never moves and only switches on and off.
    fn switch(visible: Vec<bool>) -> Self {
        Self {
            translations: vec![[0.0; 3]; visible.len()],
            rotations: vec![[0.0; 3]; visible.len()],
            scales: visible
                .iter()
                .map(|v| if *v { [1.0; 3] } else { [0.0; 3] })
                .collect(),
            visible,
        }
    }
}

fn varies<T: PartialEq>(values: &[T]) -> bool {
    values.iter().any(|v| *v != values[0])
}

struct Builder<'a> {
    scene: &'a ExportScene,
    options: &'a FbxOptions,
    next_id: i64,
    attributes: Vec<Node>,
    geometries: Vec<Node>,
    models: Vec<Node>,
    materials: Vec<Node>,
    curve_nodes: Vec<Node>,
    curves: Vec<Node>,
    connections: Vec<Node>,
    geometry_ids: HashMap<(Vec<usize>, u32), Geometry>,
    material_ids: HashMap<Look, i64>,
    layer_id: i64,
    report: ExportReport,
}

impl<'a> Builder<'a> {
    fn id(&mut self) -> i64 {
        self.next_id += 1;
        self.next_id
    }

    /// Object to object: `child` belongs to `parent`.
    fn connect(&mut self, child: i64, parent: i64) {
        self.connections
            .push(Node::new("C").with("OO").with(child).with(parent));
    }

    /// Object to property: `source` drives `property` of `target`.
    fn connect_property(&mut self, source: i64, target: i64, property: &str) {
        self.connections.push(
            Node::new("C")
                .with("OP")
                .with(source)
                .with(target)
                .with(property),
        );
    }

    // ----- animation -----

    /// One animated property: a curve node and one curve per component. Writes
    /// nothing when the values never change. Returns whether it wrote.
    ///
    /// Both ends of every run of equal values are kept as keys, so a reader that
    /// interpolates linearly whatever the file says still holds the value through the
    /// run. `step` additionally marks the keys as constant, for readers that listen.
    fn channel<const N: usize>(
        &mut self,
        model: i64,
        name: &str,
        property: &str,
        components: [&str; N],
        values: &[[f64; N]],
        step: bool,
    ) -> bool {
        if !varies(values) {
            return false;
        }
        let scene = self.scene;
        let frames = kept_frames(values, false);
        let times: Vec<i64> = frames
            .iter()
            .map(|k| ktime(scene.times[*k as usize] - scene.times[0]))
            .collect();

        let node_id = self.id();
        let mut defaults = Node::new("Properties70");
        for (i, component) in components.iter().enumerate() {
            defaults = defaults.child(p_number(component, values[0][i]));
        }
        self.curve_nodes.push(
            Node::new("AnimationCurveNode")
                .with(node_id)
                .with(object_name(name, "AnimCurveNode"))
                .with("")
                .child(defaults),
        );
        self.connect(node_id, self.layer_id);
        self.connect_property(node_id, model, property);

        // Interpolation of every key: constant (2) or linear (4).
        let flags = if step { 0x2 } else { 0x4 };
        for (i, component) in components.iter().enumerate() {
            let curve_id = self.id();
            let keys: Vec<f32> = frames
                .iter()
                .map(|k| values[*k as usize][i] as f32)
                .collect();
            self.curves.push(
                Node::new("AnimationCurve")
                    .with(curve_id)
                    .with(object_name("", "AnimCurve"))
                    .with("")
                    .child(leaf("Default", values[0][i]))
                    .child(leaf("KeyVer", 4008))
                    .child(leaf("KeyTime", times.clone()))
                    .child(leaf("KeyValueFloat", keys))
                    .child(leaf("KeyAttrFlags", vec![flags]))
                    // Tangent data shared by all keys: flat slopes and the SDK's
                    // default weights, which only cubic keys would use.
                    .child(leaf(
                        "KeyAttrDataFloat",
                        vec![0.0, 0.0, f32::from_bits(218_434_821), 0.0],
                    ))
                    .child(leaf("KeyAttrRefCount", vec![frames.len() as i32])),
            );
            self.connect_property(curve_id, node_id, component);
        }
        self.report.animation_channels += 1;
        true
    }

    // ----- objects -----

    /// Writes a model with its pose on the first frame and its animation. `extra`
    /// holds user properties.
    fn model(&mut self, name: &str, kind: &str, parent: i64, pose: &Pose, extra: Vec<Node>) -> i64 {
        let id = self.id();
        let hides = varies(&pose.visible);
        let moved = self.channel(
            id,
            "T",
            "Lcl Translation",
            ["d|X", "d|Y", "d|Z"],
            &pose.translations,
            false,
        );
        let turned = self.channel(
            id,
            "R",
            "Lcl Rotation",
            ["d|X", "d|Y", "d|Z"],
            &pose.rotations,
            false,
        );
        // Showing and hiding must not be interpolated, so a scale that carries
        // visibility is stepped as a whole.
        let scaled = self.channel(
            id,
            "S",
            "Lcl Scaling",
            ["d|X", "d|Y", "d|Z"],
            &pose.scales,
            hides,
        );
        let shown: Vec<[f64; 1]> = pose.visible.iter().map(|v| [f64::from(*v)]).collect();
        let switched = self.channel(
            id,
            "Visibility",
            "Visibility",
            ["d|Visibility"],
            &shown,
            true,
        );

        let triple = |name: &str, animated: bool, v: [f64; 3]| {
            p(name, name, "", animatable(animated))
                .with(v[0])
                .with(v[1])
                .with(v[2])
        };
        let properties = Node::new("Properties70")
            .child(triple("Lcl Translation", moved, pose.translations[0]))
            .child(triple("Lcl Rotation", turned, pose.rotations[0]))
            .child(triple("Lcl Scaling", scaled, pose.scales[0]))
            // Rotation order XYZ, spelled out because the curves depend on it.
            .child(p_enum("RotationOrder", 0))
            // Parent scale applies to the child's rotated axes, as in a plain matrix
            // product. The FBX default would apply it after the child's own scale,
            // which differs under a mirrored parent.
            .child(p_enum("InheritType", 1))
            .child(p_int("DefaultAttributeIndex", 0))
            .child(
                p("Visibility", "Visibility", "", animatable(switched))
                    .with(f64::from(pose.visible[0])),
            )
            .child(p("Visibility Inheritance", "Visibility Inheritance", "", "").with(1))
            .children(extra);
        self.models.push(
            Node::new("Model")
                .with(id)
                .with(object_name(name, "Model"))
                .with(kind)
                .child(leaf("Version", 232))
                .child(properties)
                .child(leaf("MultiLayer", 0))
                .child(leaf("MultiTake", 0))
                .child(leaf("Shading", true))
                .child(leaf("Culling", "CullingOff")),
        );
        self.connect(id, parent);
        self.report.nodes += 1;
        id
    }

    fn material(&mut self, look: Look) -> i64 {
        if let Some(id) = self.material_ids.get(&look) {
            return *id;
        }
        let id = self.id();
        // An FBX material cannot say "unlit" or "use the vertex colours". It is a
        // plain white Lambert whose name and user properties say what to set up.
        let yes_no = |flag: bool| if flag { "true" } else { "false" };
        self.materials.push(
            Node::new("Material")
                .with(id)
                .with(object_name(&look.name(), "Material"))
                .with("")
                .child(leaf("Version", 102))
                .child(leaf("ShadingModel", "lambert"))
                .child(leaf("MultiLayer", 0))
                .child(
                    Node::new("Properties70")
                        .child(p_color("DiffuseColor", [1.0; 3]))
                        .child(p_number("DiffuseFactor", 1.0))
                        .child(p_user(
                            "limnerShading",
                            if look.lit { "lit" } else { "unlit" },
                        ))
                        .child(p_user("limnerDoubleSided", yes_no(look.double_sided)))
                        .child(p_user("limnerSeeThrough", yes_no(look.see_through))),
                ),
        );
        self.material_ids.insert(look, id);
        id
    }

    /// One drawing as one geometry: its meshes joined, with a material slot per kind
    /// of surface. `opacity` is the layer's, which is multiplied into the vertex alpha.
    fn geometry(&mut self, name: &str, meshes: &[usize], opacity: f32) -> Geometry {
        let key = (meshes.to_vec(), opacity.to_bits());
        if let Some(geometry) = self.geometry_ids.get(&key) {
            return geometry.clone();
        }
        let scene = self.scene;
        let unit = self.options.unit.scale();
        let srgb = self.options.color_space == FbxColorSpace::Srgb;
        let vertex_total: usize = meshes.iter().map(|m| scene.meshes[*m].vertex_count()).sum();
        let corner_total: usize = meshes.iter().map(|m| scene.meshes[*m].indices.len()).sum();

        let mut vertices = Vec::with_capacity(vertex_total * 3);
        let mut normals = Vec::with_capacity(vertex_total * 3);
        let mut colors = Vec::with_capacity(vertex_total * 4);
        let mut polygons = Vec::with_capacity(corner_total);
        let mut corners = Vec::with_capacity(corner_total);
        let mut slots: Vec<(bool, bool)> = Vec::new();
        let mut polygon_slots = Vec::with_capacity(corner_total / 3);
        let mut base = 0i32;
        for mesh_index in meshes {
            let mesh = &scene.meshes[*mesh_index];
            let kind = (
                mesh.double_sided,
                // Blending is only for what is really see-through.
                mesh.has_transparency || opacity < OPAQUE_THRESHOLD,
            );
            let slot = slots.iter().position(|s| *s == kind).unwrap_or_else(|| {
                slots.push(kind);
                slots.len() - 1
            }) as i32;
            for position in &mesh.positions {
                vertices.extend(position.iter().map(|c| f64::from(*c) * unit));
            }
            for normal in &mesh.normals {
                normals.extend(normal.iter().map(|c| f64::from(*c)));
            }
            for color in &mesh.colors {
                for channel in &color[..3] {
                    colors.push(f64::from(if srgb {
                        linear_to_srgb(*channel)
                    } else if channel.is_finite() {
                        channel.max(0.0)
                    } else {
                        0.0
                    }));
                }
                colors.push(f64::from((color[3] * opacity).clamp(0.0, 1.0)));
            }
            for triangle in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = triangle.map(|i| i as i32 + base);
                // The last corner of a polygon is stored as its bitwise complement.
                polygons.extend_from_slice(&[a, b, !c]);
                corners.extend_from_slice(&[a, b, c]);
                polygon_slots.push(slot);
            }
            base += mesh.vertex_count() as i32;
            self.report.meshes += 1;
            self.report.vertices += mesh.vertex_count();
            self.report.triangles += mesh.triangle_count();
        }

        let id = self.id();
        let mut node = Node::new("Geometry")
            .with(id)
            .with(object_name(name, "Geometry"))
            .with("Mesh")
            .child(leaf("GeometryVersion", 124))
            .child(leaf("Vertices", vertices))
            .child(leaf("PolygonVertexIndex", polygons));
        let mut layer = Node::new("Layer").with(0).child(leaf("Version", 100));
        let element = |kind: &str| {
            Node::new("LayerElement")
                .child(leaf("Type", kind))
                .child(leaf("TypedIndex", 0))
        };
        if self.options.normals {
            // One normal per vertex ("control point"), which is what the scene has.
            node = node.child(
                Node::new("LayerElementNormal")
                    .with(0)
                    .child(leaf("Version", 101))
                    .child(leaf("Name", ""))
                    .child(leaf("MappingInformationType", "ByVertice"))
                    .child(leaf("ReferenceInformationType", "Direct"))
                    .child(leaf("Normals", normals)),
            );
            layer = layer.child(element("LayerElementNormal"));
        }
        // Colours per polygon corner through an index, the one layout Blender, Unity
        // and Unreal all read. The index is the vertex index, so each vertex's colour
        // is stored once.
        node = node.child(
            Node::new("LayerElementColor")
                .with(0)
                .child(leaf("Version", 101))
                .child(leaf("Name", "Color"))
                .child(leaf("MappingInformationType", "ByPolygonVertex"))
                .child(leaf("ReferenceInformationType", "IndexToDirect"))
                .child(leaf("Colors", colors))
                .child(leaf("ColorIndex", corners)),
        );
        layer = layer.child(element("LayerElementColor"));
        let one_slot = slots.len() < 2;
        node = node.child(
            Node::new("LayerElementMaterial")
                .with(0)
                .child(leaf("Version", 101))
                .child(leaf("Name", ""))
                .child(leaf(
                    "MappingInformationType",
                    if one_slot { "AllSame" } else { "ByPolygon" },
                ))
                .child(leaf("ReferenceInformationType", "IndexToDirect"))
                .child(leaf(
                    "Materials",
                    if one_slot { vec![0] } else { polygon_slots },
                )),
        );
        layer = layer.child(element("LayerElementMaterial"));
        self.geometries.push(node.child(layer));

        let geometry = Geometry { id, slots };
        self.geometry_ids.insert(key, geometry.clone());
        geometry
    }

    /// Gives `model` the geometry of one drawing and the materials of its slots.
    fn attach_drawing(
        &mut self,
        model: i64,
        name: &str,
        meshes: &[usize],
        node: &ExportNode,
        shading: Shading,
    ) {
        let opacity = node.opacity.first();
        let opacity = if opacity.is_finite() {
            opacity.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let geometry = self.geometry(name, meshes, opacity);
        self.connect(geometry.id, model);
        // The order of these connections is the order of the slots.
        for (double_sided, see_through) in &geometry.slots {
            let material = self.material(Look {
                lit: shading == Shading::Lit,
                double_sided: *double_sided,
                see_through: *see_through,
            });
            self.connect(material, model);
        }
    }

    fn light(&mut self, model: i64, name: &str, light: &ExportLight) {
        let (color, intensity) = light_color_and_intensity(light);
        let unit = self.options.unit.scale();
        let mut properties = Node::new("Properties70")
            .child(p_enum(
                "LightType",
                match light.kind {
                    LightKind::Point => 0,
                    LightKind::Directional => 1,
                    LightKind::Spot => 2,
                },
            ))
            .child(p_color("Color", color.map(f64::from)))
            // FBX intensity is a percentage.
            .child(p_number("Intensity", f64::from(intensity) * 100.0))
            .child(p_bool("CastLight", true))
            .child(p_bool("CastShadows", light.shadows))
            // Inverse-square fall-off, like Limner's lights.
            .child(p_enum("DecayType", 2));
        if light.kind != LightKind::Directional && light.world_range > 0.0 {
            properties = properties
                .child(p_bool("EnableFarAttenuation", true))
                .child(p_double(
                    "FarAttenuationEnd",
                    f64::from(light.world_range) * unit,
                ));
        }
        if light.kind == LightKind::Spot {
            // FBX measures the whole cone, Limner half of it.
            let outer = (f64::from(light.spot_angle) * 2.0).clamp(0.1, 179.0);
            let inner = outer * (1.0 - f64::from(light.spot_blend).clamp(0.0, 1.0));
            properties = properties
                .child(p_number("InnerAngle", inner))
                .child(p_number("OuterAngle", outer));
        }
        let id = self.id();
        self.attributes.push(
            Node::new("NodeAttribute")
                .with(id)
                .with(object_name(name, "NodeAttribute"))
                .with("Light")
                .child(leaf("TypeFlags", "Light"))
                .child(leaf("GeometryVersion", 124))
                .child(properties),
        );
        self.connect(id, model);
    }

    fn camera(&mut self, model: i64, name: &str, camera: &ExportCamera) {
        let unit = self.options.unit.scale();
        let aspect = f64::from(camera_aspect(camera));
        let vertical = f64::from(camera_fov(camera)).to_radians();
        let focal = CAMERA_FILM_HEIGHT_MM * 0.5 / (vertical * 0.5).tan();
        let horizontal = 2.0 * ((vertical * 0.5).tan() * aspect).atan();
        let [width, height] = camera.resolution;
        let (width, height) = if width > 0 && height > 0 {
            (f64::from(width), f64::from(height))
        } else {
            (1920.0, 1080.0)
        };
        // Film sizes are in inches.
        let film_height = CAMERA_FILM_HEIGHT_MM / 25.4;
        let properties = Node::new("Properties70")
            // The picture has a fixed size in pixels.
            .child(p_enum("AspectRatioMode", 2))
            .child(p_double("AspectWidth", width))
            .child(p_double("AspectHeight", height))
            .child(p_double("PixelAspectRatio", 1.0))
            .child(p_double("FilmWidth", film_height * aspect))
            .child(p_double("FilmHeight", film_height))
            .child(p_double("FilmAspectRatio", aspect))
            // The lens is given by its focal length; the angles repeat it for readers
            // that prefer them.
            .child(p_enum("ApertureMode", 3))
            .child(p_enum("GateFit", 0))
            .child(p("FieldOfView", "FieldOfView", "", "A").with(horizontal.to_degrees()))
            .child(p("FieldOfViewX", "FieldOfViewX", "", "A").with(horizontal.to_degrees()))
            .child(p("FieldOfViewY", "FieldOfViewY", "", "A").with(vertical.to_degrees()))
            .child(p_number("FocalLength", focal))
            .child(p_enum("CameraProjectionType", 0))
            .child(p_double("NearPlane", f64::from(CAMERA_ZNEAR) * unit))
            .child(p_double("FarPlane", f64::from(CAMERA_ZFAR) * unit));
        let id = self.id();
        self.attributes.push(
            Node::new("NodeAttribute")
                .with(id)
                .with(object_name(name, "NodeAttribute"))
                .with("Camera")
                .child(leaf("TypeFlags", "Camera"))
                .child(leaf("GeometryVersion", 124))
                .child(properties),
        );
        self.connect(id, model);
    }

    /// The pose of a scene node on every sampled frame.
    fn pose(&self, node: &ExportNode, aim: Aim) -> Pose {
        let unit = self.options.unit.scale();
        let visible = visibility(self.scene, node);
        let samples = transforms(self.scene, node);
        let mut pose = Pose {
            translations: Vec::with_capacity(samples.len()),
            rotations: Vec::with_capacity(samples.len()),
            scales: Vec::with_capacity(samples.len()),
            visible,
        };
        for (frame, x) in samples.iter().enumerate() {
            let (rotation, scale) = aimed(x.rotation, (x.flip.signs() * x.scale).to_array(), aim);
            pose.translations.push((x.translation * unit).to_array());
            pose.rotations
                .push(euler_xyz(rotation, pose.rotations.last().copied()));
            pose.scales
                .push(if pose.visible[frame] { scale } else { [0.0; 3] });
        }
        pose
    }

    fn paint(&mut self, node: &ExportNode, model: i64, paint: &ExportPaint) {
        let frames = self.scene.frame_count();
        // Frame-by-frame: one child per drawing, switched on and off.
        for drawing in &paint.drawings {
            let name = format!("{}.{:03}", node.name, drawing.ordinal);
            let shown: Vec<bool> = (0..frames).map(|k| *drawing.shown.at(k)).collect();
            let extra = vec![
                p_user("limnerLayerId", &node.layer.0.to_string()),
                p_user("limnerDrawing", &drawing.ordinal.to_string()),
                p_user("limnerDrawingId", &drawing.drawing.0.to_string()),
            ];
            let child = self.model(&name, "Mesh", model, &Pose::switch(shown), extra);
            self.attach_drawing(child, &name, &drawing.meshes, node, paint.shading);
        }
    }

    fn node(&mut self, index: usize, parent: i64) {
        let scene = self.scene;
        let node = &scene.nodes[index];
        let mut extra = vec![
            p_user("limnerKind", node.kind.as_str()),
            p_user("limnerLayerId", &node.layer.0.to_string()),
        ];
        // The usual case, a layer showing one drawing throughout, carries its mesh itself.
        let single = match &node.content {
            NodeContent::Paint(paint) => match paint.drawings.as_slice() {
                [only] if only.shown == Sampled::Constant(true) => Some(only),
                _ => None,
            },
            _ => None,
        };
        let (kind, aim) = match &node.content {
            NodeContent::Light(light) => {
                extra.push(p_user("limnerEnergy", &light.energy.to_string()));
                extra.push(p_user("limnerRange", &light.range.to_string()));
                ("Light", Aim::Light)
            }
            NodeContent::Camera(_) => ("Camera", Aim::Camera),
            NodeContent::Media(media) => {
                extra.push(p_user("limnerMedia", &media.name));
                extra.push(p_user("limnerSourcePath", &media.source_path));
                if let Some(id) = media.media {
                    extra.push(p_user("limnerMediaId", &id.0.to_string()));
                }
                ("Null", Aim::Plain)
            }
            NodeContent::Paint(_) if single.is_some() => ("Mesh", Aim::Plain),
            _ => ("Null", Aim::Plain),
        };
        let pose = self.pose(node, aim);
        let model = self.model(&node.name, kind, parent, &pose, extra);
        match &node.content {
            NodeContent::Paint(paint) => match single {
                Some(only) => {
                    self.attach_drawing(model, &node.name, &only.meshes, node, paint.shading)
                }
                None => self.paint(node, model, paint),
            },
            NodeContent::Light(light) => self.light(model, &node.name, light),
            NodeContent::Camera(camera) => self.camera(model, &node.name, camera),
            NodeContent::Media(_) | NodeContent::None => {}
        }
        for child in &node.children {
            self.node(*child, model);
        }
    }

    fn finish(mut self) -> Result<(Vec<u8>, ExportReport), ExportError> {
        let scene = self.scene;
        let name = printable(title(scene));
        let animated = scene.is_animated();
        let stack_id = self.id();
        self.layer_id = self.id();
        for root in &scene.roots {
            self.node(*root, 0);
        }
        let last_frame = scene.frame_count() - 1;
        let stop = ktime(scene.times[last_frame] - scene.times[0]);

        // Animation objects exist only when something moves.
        let mut animation = Vec::new();
        if animated && !self.curve_nodes.is_empty() {
            animation.push(
                Node::new("AnimationStack")
                    .with(stack_id)
                    .with(object_name(&name, "AnimStack"))
                    .with("")
                    .child(
                        Node::new("Properties70")
                            .child(p_time("LocalStart", 0))
                            .child(p_time("LocalStop", stop))
                            .child(p_time("ReferenceStart", 0))
                            .child(p_time("ReferenceStop", stop)),
                    ),
            );
            animation.push(
                Node::new("AnimationLayer")
                    .with(self.layer_id)
                    .with(object_name(&name, "AnimLayer"))
                    .with(""),
            );
            self.connections
                .push(Node::new("C").with("OO").with(self.layer_id).with(stack_id));
        }
        let has_stack = !animation.is_empty();

        let counts = [
            ("GlobalSettings", 1),
            ("NodeAttribute", self.attributes.len()),
            ("Geometry", self.geometries.len()),
            ("Model", self.models.len()),
            ("Material", self.materials.len()),
            ("AnimationStack", usize::from(has_stack)),
            ("AnimationLayer", usize::from(has_stack)),
            ("AnimationCurveNode", self.curve_nodes.len()),
            ("AnimationCurve", self.curves.len()),
        ];
        let total: usize = counts.iter().map(|(_, count)| count).sum();
        let mut definitions = Node::new("Definitions")
            .child(leaf("Version", 100))
            .child(leaf("Count", total as i32));
        for (kind, count) in counts {
            if count > 0 {
                definitions = definitions.child(
                    Node::new("ObjectType")
                        .with(kind)
                        .child(leaf("Count", count as i32)),
                );
            }
        }

        let objects = Node::new("Objects")
            .children(self.attributes)
            .children(self.geometries)
            .children(self.models)
            .children(self.materials)
            .children(animation)
            .children(self.curve_nodes)
            .children(self.curves);

        let mut takes =
            Node::new("Takes").child(leaf("Current", if has_stack { name.as_str() } else { "" }));
        if has_stack {
            takes = takes.child(
                Node::new("Take")
                    .with(name.as_str())
                    .child(leaf("FileName", format!("{name}.tak").as_str()))
                    .child(Node::new("LocalTime").with(0i64).with(stop))
                    .child(Node::new("ReferenceTime").with(0i64).with(stop)),
            );
        }

        let document_id = self.next_id + 1;
        let nodes = [
            header_extension(&name),
            Node::new("FileId").with(Prop::Raw(FILE_ID.to_vec())),
            leaf("CreationTime", CREATION_TIME),
            leaf("Creator", GENERATOR),
            global_settings(scene, self.options, stop),
            Node::new("Documents").child(leaf("Count", 1)).child(
                Node::new("Document")
                    .with(document_id)
                    .with("")
                    .with("Scene")
                    .child(
                        Node::new("Properties70")
                            .child(p("SourceObject", "object", "", ""))
                            .child(p_string(
                                "ActiveAnimStackName",
                                if has_stack { name.as_str() } else { "" },
                            )),
                    )
                    .child(leaf("RootNode", 0i64)),
            ),
            Node::new("References"),
            definitions,
            objects,
            Node::new("Connections").children(self.connections),
            takes,
        ];
        let bytes = write_file(&nodes)?;
        self.report.animation_frames = if animated { scene.frame_count() } else { 0 };
        self.report.bytes = bytes.len() as u64;
        Ok((bytes, self.report))
    }
}

fn header_extension(name: &str) -> Node {
    let application = |prefix: &str| {
        [
            p_string(&format!("{prefix}|ApplicationVendor"), "Limner"),
            p_string(&format!("{prefix}|ApplicationName"), "Limner"),
            p_string(
                &format!("{prefix}|ApplicationVersion"),
                env!("CARGO_PKG_VERSION"),
            ),
        ]
    };
    Node::new("FBXHeaderExtension")
        .child(leaf("FBXHeaderVersion", 1003))
        .child(leaf("FBXVersion", FBX_VERSION as i32))
        .child(leaf("EncryptionType", 0))
        .child(
            // The same fixed instant as `CreationTime`.
            Node::new("CreationTimeStamp")
                .child(leaf("Version", 1000))
                .child(leaf("Year", 1970))
                .child(leaf("Month", 1))
                .child(leaf("Day", 1))
                .child(leaf("Hour", 10))
                .child(leaf("Minute", 0))
                .child(leaf("Second", 0))
                .child(leaf("Millisecond", 0)),
        )
        .child(leaf("Creator", GENERATOR))
        .child(
            Node::new("SceneInfo")
                .with(object_name("GlobalInfo", "SceneInfo"))
                .with("UserData")
                .child(leaf("Type", "UserData"))
                .child(leaf("Version", 100))
                .child(
                    Node::new("MetaData")
                        .child(leaf("Version", 100))
                        .child(leaf("Title", name))
                        .child(leaf("Subject", ""))
                        .child(leaf("Author", ""))
                        .child(leaf("Keywords", ""))
                        .child(leaf("Revision", ""))
                        .child(leaf("Comment", "")),
                )
                .child(
                    Node::new("Properties70")
                        .child(p("Original", "Compound", "", ""))
                        .children(application("Original"))
                        .child(p("LastSaved", "Compound", "", ""))
                        .children(application("LastSaved")),
                ),
        )
}

/// FBX's code for a frame rate, and the rate itself for the "custom" code.
fn time_mode(rate: f64) -> i32 {
    const MODES: [(f64, i32); 14] = [
        (120.0, 1),
        (100.0, 2),
        (60.0, 3),
        (50.0, 4),
        (48.0, 5),
        (30.0, 6),
        (30.0 / 1.001, 9),
        (25.0, 10),
        (24.0, 11),
        (24.0 / 1.001, 13),
        (96.0, 15),
        (72.0, 16),
        (60.0 / 1.001, 17),
        (120.0 / 1.001, 18),
    ];
    MODES
        .iter()
        .find(|(fps, _)| (fps - rate).abs() < 1e-6)
        .map_or(14, |(_, mode)| *mode)
}

fn global_settings(scene: &ExportScene, options: &FbxOptions, stop: i64) -> Node {
    let rate = if scene.rate.is_valid() {
        scene.rate.as_f64()
    } else {
        24.0
    };
    let unit = options.unit.centimetres_per_unit();
    // A still spans one frame, so no reader meets an empty time range.
    let stop = stop.max((KTIME_PER_SECOND as f64 / rate).round() as i64);
    Node::new("GlobalSettings")
        .child(leaf("Version", 1000))
        .child(
            Node::new("Properties70")
                // Y up, Z towards the viewer, X to the right: right-handed, as Limner.
                .child(p_int("UpAxis", 1))
                .child(p_int("UpAxisSign", 1))
                .child(p_int("FrontAxis", 2))
                .child(p_int("FrontAxisSign", 1))
                .child(p_int("CoordAxis", 0))
                .child(p_int("CoordAxisSign", 1))
                .child(p_int("OriginalUpAxis", 1))
                .child(p_int("OriginalUpAxisSign", 1))
                .child(p_double("UnitScaleFactor", unit))
                .child(p_double("OriginalUnitScaleFactor", unit))
                .child(
                    p("AmbientColor", "ColorRGB", "Color", "")
                        .with(0.0)
                        .with(0.0)
                        .with(0.0),
                )
                .child(p_string("DefaultCamera", "Producer Perspective"))
                .child(p_enum("TimeMode", time_mode(rate)))
                .child(p_enum("TimeProtocol", 2))
                .child(p_enum("SnapOnFrameMode", 0))
                .child(p_time("TimeSpanStart", 0))
                .child(p_time("TimeSpanStop", stop))
                .child(p_double("CustomFrameRate", rate))
                .child(p("TimeMarker", "Compound", "", ""))
                .child(p_int("CurrentTimeMarker", -1)),
        )
}

/// Writes an export scene as a binary FBX 7.4 file.
///
/// Time zero is the first sampled frame. The file's creation date is fixed, so the
/// same scene always gives the same bytes.
pub fn scene_to_fbx(
    scene: &ExportScene,
    options: &FbxOptions,
) -> Result<(Vec<u8>, ExportReport), ExportError> {
    Builder {
        scene,
        options,
        // Object ids only need to be unique and non-zero; zero is the scene root.
        next_id: 1_000_000,
        attributes: Vec::new(),
        geometries: Vec::new(),
        models: Vec::new(),
        materials: Vec::new(),
        curve_nodes: Vec::new(),
        curves: Vec::new(),
        connections: Vec::new(),
        geometry_ids: HashMap::new(),
        material_ids: HashMap::new(),
        layer_id: 0,
        report: ExportReport::default(),
    }
    .finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner_core::glam::DVec3;

    fn rebuilt(euler: [f64; 3]) -> DQuat {
        DQuat::from_rotation_z(euler[2].to_radians())
            * DQuat::from_rotation_y(euler[1].to_radians())
            * DQuat::from_rotation_x(euler[0].to_radians())
    }

    #[test]
    fn frame_times_are_exact_in_fbx_units() {
        assert_eq!(ktime(FLICKS_PER_SECOND), KTIME_PER_SECOND);
        assert_eq!(ktime(FLICKS_PER_SECOND / 24), 1_924_423_250);
        assert_eq!(ktime(FLICKS_PER_SECOND / 30), 1_539_538_600);
        assert_eq!(ktime(0), 0);
        // 24000/1001 fps: frame 1001 is at exactly 1001 * 1001 / 24000 seconds.
        let rate = limner_core::time::FrameRate::new(24_000, 1001);
        let t = rate.frame_start(24_000);
        assert_eq!(ktime(t), 1001 * KTIME_PER_SECOND);
    }

    #[test]
    fn euler_angles_rebuild_the_rotation() {
        let axes = [
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.3, -1.0, 0.2).normalize(),
            DVec3::new(-0.5, 0.4, 1.0).normalize(),
        ];
        for axis in axes {
            for step in 0..48 {
                let q = DQuat::from_axis_angle(axis, step as f64 * 0.27 - 6.0);
                let back = rebuilt(euler_xyz(q, None));
                assert!(back.dot(q).abs() > 1.0 - 1e-9, "{axis:?} step {step}");
            }
        }
        // Straight up and down the Y turn, where X and Z coincide.
        for (y, x, z) in [(90.0f64, 20.0f64, 35.0f64), (-90.0, -50.0, 10.0)] {
            let q = rebuilt([x, y, z]);
            let back = rebuilt(euler_xyz(q, None));
            assert!(back.dot(q).abs() > 1.0 - 1e-9, "y = {y}");
            let kept = euler_xyz(q, Some([0.0, y, z]));
            assert!((kept[2] - z).abs() < 1e-6, "Z stays where it was");
            assert!(rebuilt(kept).dot(q).abs() > 1.0 - 1e-9);
        }
    }

    #[test]
    fn euler_angles_are_unrolled() {
        // Two full turns about an oblique axis in small steps: no component may jump.
        let axis = DVec3::new(0.2, 0.3, 1.0).normalize();
        let mut previous: Option<[f64; 3]> = None;
        for step in 0..=200 {
            let angle = step as f64 / 200.0 * 4.0 * std::f64::consts::PI;
            let q = DQuat::from_axis_angle(axis, angle);
            let euler = euler_xyz(q, previous);
            assert!(rebuilt(euler).dot(q).abs() > 1.0 - 1e-9);
            if let Some(p) = previous {
                for i in 0..3 {
                    assert!(
                        (euler[i] - p[i]).abs() < 20.0,
                        "step {step}: {p:?} to {euler:?}"
                    );
                }
            }
            previous = Some(euler);
        }
        let last = previous.unwrap();
        assert!(
            last[2] > 600.0,
            "two turns about (mostly) Z read as about 720 degrees, got {last:?}"
        );
    }

    #[test]
    fn lights_and_cameras_are_turned_to_fbx_axes() {
        // An unrotated Limner light shines along -Z; the FBX node must take FBX's -Y there.
        let (q, s) = aimed(DQuat::IDENTITY, [2.0, 3.0, 4.0], Aim::Light);
        assert!((q * DVec3::NEG_Y - DVec3::NEG_Z).length() < 1e-12);
        assert_eq!(s, [2.0, 4.0, 3.0]);
        // A camera: FBX's +X view direction goes to -Z, and up stays up.
        let (q, s) = aimed(DQuat::IDENTITY, [2.0, 3.0, 4.0], Aim::Camera);
        assert!((q * DVec3::X - DVec3::NEG_Z).length() < 1e-12);
        assert!((q * DVec3::Y - DVec3::Y).length() < 1e-12);
        assert_eq!(s, [4.0, 3.0, 2.0]);
        // The turned rotation and the carried scale make the same matrix as before.
        let rotation = DQuat::from_axis_angle(DVec3::new(0.3, 1.0, -0.2).normalize(), 1.1);
        for (aim, fix) in [(Aim::Light, light_fix()), (Aim::Camera, camera_fix())] {
            let scale = [-1.5, 1.5, 1.5];
            let (q, s) = aimed(rotation, scale, aim);
            let before = DMat3::from_quat(rotation) * DMat3::from_diagonal(DVec3::from(scale));
            let after = DMat3::from_quat(q) * DMat3::from_diagonal(DVec3::from(s));
            let expected = before * DMat3::from_quat(fix);
            assert!((after - expected).abs_diff_eq(DMat3::ZERO, 1e-12));
        }
    }

    #[test]
    fn records_carry_their_end_offset_and_sentinels() {
        let nodes = [
            Node::new("A").with(7).child(leaf("B", "x")),
            Node::new("Empty"),
            Node::new("Last"),
        ];
        let mut out = Vec::new();
        write_nodes(&mut out, &nodes);
        // A: header 13 + name 1 + value 5, child B: 13 + 1 + (1 + 4 + 1), sentinel 13.
        let a_end = 13 + 1 + 5 + (13 + 1 + 6) + 13;
        assert_eq!(u32::from_le_bytes(out[0..4].try_into().unwrap()), a_end);
        assert_eq!(u32::from_le_bytes(out[4..8].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(out[8..12].try_into().unwrap()), 5);
        assert_eq!(out[12], 1);
        assert_eq!(out[13], b'A');
        // An empty record that is not the last one carries a sentinel of its own.
        let empty = a_end as usize;
        let empty_end = empty + 13 + 5 + 13;
        assert_eq!(
            u32::from_le_bytes(out[empty..empty + 4].try_into().unwrap()) as usize,
            empty_end
        );
        // The last one does not; then the list's own sentinel closes it.
        let last_end = empty_end + 13 + 4;
        assert_eq!(
            u32::from_le_bytes(out[empty_end..empty_end + 4].try_into().unwrap()) as usize,
            last_end
        );
        assert_eq!(out.len(), last_end + 13);
        assert!(out[last_end..].iter().all(|b| *b == 0));
    }

    #[test]
    fn frame_rates_map_to_fbx_time_modes() {
        assert_eq!(time_mode(24.0), 11);
        assert_eq!(time_mode(30.0), 6);
        assert_eq!(time_mode(24_000.0 / 1001.0), 13);
        assert_eq!(time_mode(12.0), 14, "anything else is a custom rate");
    }
}
