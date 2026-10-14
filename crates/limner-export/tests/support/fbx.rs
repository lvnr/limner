//! A reader for binary FBX files, written from the format's layout and sharing no
//! code with the writer. It walks the whole record tree, checks every offset,
//! sentinel and array length on the way, and then offers the scene in the terms the
//! tests ask about: models, geometries, materials, connections and curves.

use limner_core::glam::{DAffine3, DMat3, DQuat, DVec3};
use std::collections::{HashMap, HashSet};

pub const KTIME_PER_SECOND: i64 = 46_186_158_000;
/// One frame at 24 frames per second, in FBX time.
pub const KTIME_FRAME: i64 = KTIME_PER_SECOND / 24;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    I16(i16),
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    Str(Vec<u8>),
    Raw(Vec<u8>),
    Bools(Vec<bool>),
    I32s(Vec<i32>),
    I64s(Vec<i64>),
    F32s(Vec<f32>),
    F64s(Vec<f64>),
}

impl Value {
    pub fn as_i64(&self) -> i64 {
        match self {
            Value::I16(v) => i64::from(*v),
            Value::I32(v) => i64::from(*v),
            Value::I64(v) => *v,
            other => panic!("not an integer: {other:?}"),
        }
    }

    pub fn as_f64(&self) -> f64 {
        match self {
            Value::F32(v) => f64::from(*v),
            Value::F64(v) => *v,
            other => panic!("not a number: {other:?}"),
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Value::Str(bytes) => std::str::from_utf8(bytes).expect("UTF-8"),
            other => panic!("not a string: {other:?}"),
        }
    }

    pub fn as_f64s(&self) -> &[f64] {
        match self {
            Value::F64s(v) => v,
            other => panic!("not a double array: {other:?}"),
        }
    }

    pub fn as_f32s(&self) -> &[f32] {
        match self {
            Value::F32s(v) => v,
            other => panic!("not a float array: {other:?}"),
        }
    }

    pub fn as_i32s(&self) -> &[i32] {
        match self {
            Value::I32s(v) => v,
            other => panic!("not an int array: {other:?}"),
        }
    }

    pub fn as_i64s(&self) -> &[i64] {
        match self {
            Value::I64s(v) => v,
            other => panic!("not a long array: {other:?}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Record {
    pub name: String,
    pub values: Vec<Value>,
    pub children: Vec<Record>,
}

impl Record {
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Record> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn get(&self, name: &str) -> Option<&Record> {
        let mut found = self.children.iter().filter(|c| c.name == name);
        let first = found.next();
        assert!(found.next().is_none(), "{} has one {name}", self.name);
        first
    }

    pub fn child(&self, name: &str) -> &Record {
        self.get(name)
            .unwrap_or_else(|| panic!("{} has no {name}", self.name))
    }

    /// The single value of a child record.
    pub fn value(&self, name: &str) -> &Value {
        let child = self.child(name);
        assert_eq!(child.values.len(), 1, "{name} holds one value");
        &child.values[0]
    }

    /// The `P` record of a property in this record's `Properties70`.
    pub fn property_record(&self, name: &str) -> Option<&Record> {
        self.get("Properties70")?
            .children
            .iter()
            .find(|p| p.values[0].as_str() == name)
    }

    /// The values of a property, after its name, type, label and flags.
    pub fn property(&self, name: &str) -> Option<&[Value]> {
        self.property_record(name).map(|p| &p.values[4..])
    }

    pub fn number(&self, name: &str) -> f64 {
        self.property(name)
            .unwrap_or_else(|| panic!("{} has no property {name}", self.name))[0]
            .as_f64()
    }

    pub fn integer(&self, name: &str) -> i64 {
        self.property(name)
            .unwrap_or_else(|| panic!("{} has no property {name}", self.name))[0]
            .as_i64()
    }

    pub fn text(&self, name: &str) -> &str {
        self.property(name)
            .unwrap_or_else(|| panic!("{} has no property {name}", self.name))[0]
            .as_str()
    }

    pub fn vector(&self, name: &str) -> DVec3 {
        let values = self
            .property(name)
            .unwrap_or_else(|| panic!("{} has no property {name}", self.name));
        assert_eq!(values.len(), 3, "{name} is a triple");
        DVec3::new(values[0].as_f64(), values[1].as_f64(), values[2].as_f64())
    }

    /// The flags of a property: "A" animatable, "A+" animated, "U" user-defined.
    pub fn property_flags(&self, name: &str) -> &str {
        self.property_record(name)
            .unwrap_or_else(|| panic!("{} has no property {name}", self.name))
            .values[3]
            .as_str()
    }
}

/// An entry of the `Objects` section.
#[derive(Clone, Copy)]
pub struct Object<'a> {
    pub id: i64,
    pub name: &'a str,
    /// The class in the name ("Model", "Geometry", "AnimCurveNode", ...).
    pub class: &'a str,
    /// The third value: "Mesh", "Null", "Light", "Camera" or empty.
    pub subtype: &'a str,
    pub record: &'a Record,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Connection {
    /// "OO" object to object, "OP" object to property.
    pub kind: String,
    pub source: i64,
    pub target: i64,
    pub property: Option<String>,
}

pub struct Fbx {
    pub version: u32,
    pub records: Vec<Record>,
}

pub struct Geometry {
    pub positions: Vec<DVec3>,
    /// One per vertex.
    pub normals: Vec<DVec3>,
    /// RGBA per vertex, as stored.
    pub colors: Vec<[f64; 4]>,
    /// Triangles as vertex indices.
    pub triangles: Vec<[u32; 3]>,
    /// The material slot of each triangle.
    pub slots: Vec<usize>,
}

pub struct Curve {
    pub times: Vec<i64>,
    pub values: Vec<f32>,
    /// Per key: true when the value is held until the next key.
    pub constant: Vec<bool>,
}

impl Curve {
    /// The curve's value at an FBX time.
    pub fn sample(&self, time: i64) -> f64 {
        let after = self.times.partition_point(|t| *t <= time);
        if after == 0 {
            return f64::from(self.values[0]);
        }
        let i = after - 1;
        if i + 1 == self.times.len() || self.constant[i] || self.times[i] == time {
            return f64::from(self.values[i]);
        }
        let u = (time - self.times[i]) as f64 / (self.times[i + 1] - self.times[i]) as f64;
        f64::from(self.values[i]) + f64::from(self.values[i + 1] - self.values[i]) * u
    }
}

const MAGIC: &[u8] = b"Kaydara FBX Binary  \x00\x1a\x00";
const FOOTER_ID: [u8; 16] = [
    0xfa, 0xbc, 0xab, 0x09, 0xd0, 0xc8, 0xd4, 0x66, 0xb1, 0x76, 0xfb, 0x83, 0x1c, 0xf7, 0x26, 0x7e,
];
const FOOTER_MAGIC: [u8; 16] = [
    0xf8, 0x5a, 0x8c, 0x6a, 0xde, 0xf5, 0xd9, 0x7e, 0xec, 0xe9, 0x0c, 0xe3, 0x75, 0x8f, 0x29, 0x0b,
];

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> &'a [u8] {
        assert!(
            self.at + n <= self.bytes.len(),
            "the file ends inside a record (at {}, wanted {n})",
            self.at
        );
        let slice = &self.bytes[self.at..self.at + n];
        self.at += n;
        slice
    }

    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take(4).try_into().unwrap())
    }

    fn array<const N: usize, T>(&mut self, convert: impl Fn([u8; N]) -> T) -> Vec<T> {
        let count = self.u32() as usize;
        let encoding = self.u32();
        let stored = self.u32() as usize;
        assert_eq!(encoding, 0, "arrays are stored raw, not compressed");
        assert_eq!(
            stored,
            count * N,
            "an array's byte length matches its count"
        );
        self.take(stored)
            .as_chunks::<N>()
            .0
            .iter()
            .map(|chunk| convert(*chunk))
            .collect()
    }

    fn value(&mut self) -> Value {
        let code = self.u8();
        match code {
            b'C' => {
                let byte = self.u8();
                assert!(byte <= 1, "a boolean is 0 or 1");
                Value::Bool(byte == 1)
            }
            b'Y' => Value::I16(i16::from_le_bytes(self.take(2).try_into().unwrap())),
            b'I' => Value::I32(i32::from_le_bytes(self.take(4).try_into().unwrap())),
            b'L' => Value::I64(i64::from_le_bytes(self.take(8).try_into().unwrap())),
            b'F' => Value::F32(f32::from_le_bytes(self.take(4).try_into().unwrap())),
            b'D' => Value::F64(f64::from_le_bytes(self.take(8).try_into().unwrap())),
            b'S' | b'R' => {
                let length = self.u32() as usize;
                let bytes = self.take(length).to_vec();
                if code == b'S' {
                    Value::Str(bytes)
                } else {
                    Value::Raw(bytes)
                }
            }
            b'b' => Value::Bools(self.array(|b: [u8; 1]| b[0] != 0)),
            b'i' => Value::I32s(self.array(i32::from_le_bytes)),
            b'l' => Value::I64s(self.array(i64::from_le_bytes)),
            b'f' => Value::F32s(self.array(f32::from_le_bytes)),
            b'd' => Value::F64s(self.array(f64::from_le_bytes)),
            other => panic!("unknown value type {:?} at {}", other as char, self.at - 1),
        }
    }

    /// Reads one record, or `None` for the all-zero sentinel that ends a list.
    fn record(&mut self) -> Option<Record> {
        let start = self.at;
        let end = self.u32() as usize;
        let value_count = self.u32() as usize;
        let values_length = self.u32() as usize;
        let name_length = self.u8() as usize;
        if end == 0 {
            assert!(
                value_count == 0 && values_length == 0 && name_length == 0,
                "a sentinel is thirteen zero bytes (at {start})"
            );
            return None;
        }
        let name = std::str::from_utf8(self.take(name_length))
            .expect("record names are ASCII")
            .to_owned();
        assert!(!name.is_empty(), "a record has a name (at {start})");
        assert!(
            end > self.at - 1 && end <= self.bytes.len(),
            "{name}: the end offset lies inside the file and after the header"
        );
        let values_start = self.at;
        let values = (0..value_count).map(|_| self.value()).collect();
        assert_eq!(
            self.at - values_start,
            values_length,
            "{name}: the values take the stated number of bytes"
        );
        let mut children = Vec::new();
        if self.at < end {
            // Nested records, closed by a sentinel that ends exactly at the end offset.
            while let Some(child) = self.record() {
                children.push(child);
                assert!(self.at < end, "{name}: a nested list ends with a sentinel");
            }
        }
        assert_eq!(self.at, end, "{name}: the record ends at its end offset");
        Some(Record {
            name,
            values,
            children,
        })
    }
}

fn split_name(value: &Value) -> (&str, &str) {
    let Value::Str(bytes) = value else {
        panic!("an object name is a string");
    };
    let at = bytes
        .windows(2)
        .position(|w| w == [0, 1])
        .expect("an object name holds a name and a class");
    (
        std::str::from_utf8(&bytes[..at]).expect("UTF-8"),
        std::str::from_utf8(&bytes[at + 2..]).expect("UTF-8"),
    )
}

/// Rotation from FBX Euler angles in degrees, order XYZ: X is applied first.
pub fn euler_to_quat(degrees: DVec3) -> DQuat {
    DQuat::from_rotation_z(degrees.z.to_radians())
        * DQuat::from_rotation_y(degrees.y.to_radians())
        * DQuat::from_rotation_x(degrees.x.to_radians())
}

pub fn trs(translation: DVec3, rotation: DVec3, scale: DVec3) -> DAffine3 {
    DAffine3::from_translation(translation)
        * DAffine3::from_quat(euler_to_quat(rotation))
        * DAffine3::from_mat3(DMat3::from_diagonal(scale))
}

impl Fbx {
    /// Reads the container: header, every record, footer. Panics on anything that is
    /// not exactly as the format lays it out.
    pub fn parse(bytes: &[u8]) -> Fbx {
        assert!(bytes.len() > 27 + 13 + 160, "too short to be an FBX file");
        assert_eq!(&bytes[..23], MAGIC, "the binary FBX magic");
        let mut reader = Reader { bytes, at: 23 };
        let version = reader.u32();
        assert_eq!(version, 7400, "FBX 7.4: 32-bit offsets");
        let mut records = Vec::new();
        while let Some(record) = reader.record() {
            records.push(record);
        }

        // The footer: an id, four zero bytes, zero padding to a multiple of 16 (a full
        // 16 when already aligned), the version again, 120 zero bytes, a fixed magic.
        assert_eq!(reader.take(16), FOOTER_ID, "footer id");
        assert_eq!(reader.take(4), [0; 4]);
        let padding = 16 - reader.at % 16;
        assert!(
            reader.take(padding).iter().all(|b| *b == 0),
            "footer padding"
        );
        assert_eq!(reader.u32(), version, "the footer repeats the version");
        assert!(reader.take(120).iter().all(|b| *b == 0));
        assert_eq!(reader.take(16), FOOTER_MAGIC, "footer magic");
        assert_eq!(reader.at, bytes.len(), "nothing follows the footer");
        Fbx { version, records }
    }

    /// Parses and runs every structural check.
    pub fn checked(bytes: &[u8]) -> Fbx {
        let fbx = Fbx::parse(bytes);
        fbx.validate();
        fbx
    }

    pub fn section(&self, name: &str) -> &Record {
        let mut found = self.records.iter().filter(|r| r.name == name);
        let first = found.next().unwrap_or_else(|| panic!("no {name} section"));
        assert!(found.next().is_none(), "one {name} section");
        first
    }

    pub fn objects(&self) -> Vec<Object<'_>> {
        self.section("Objects")
            .children
            .iter()
            .map(|record| {
                assert_eq!(record.values.len(), 3, "{}: id, name, subtype", record.name);
                let (name, class) = split_name(&record.values[1]);
                Object {
                    id: record.values[0].as_i64(),
                    name,
                    class,
                    subtype: record.values[2].as_str(),
                    record,
                }
            })
            .collect()
    }

    /// Objects of one record type ("Model", "Geometry", "Material", ...).
    pub fn objects_of(&self, kind: &str) -> Vec<Object<'_>> {
        self.objects()
            .into_iter()
            .filter(|o| o.record.name == kind)
            .collect()
    }

    pub fn object(&self, id: i64) -> Object<'_> {
        self.objects()
            .into_iter()
            .find(|o| o.id == id)
            .unwrap_or_else(|| panic!("no object {id}"))
    }

    pub fn find_model(&self, name: &str) -> Option<Object<'_>> {
        let mut found = self
            .objects_of("Model")
            .into_iter()
            .filter(|o| o.name == name);
        let first = found.next();
        assert!(found.next().is_none(), "one model named {name}");
        first
    }

    pub fn model(&self, name: &str) -> Object<'_> {
        self.find_model(name)
            .unwrap_or_else(|| panic!("no model named {name}"))
    }

    pub fn connections(&self) -> Vec<Connection> {
        self.section("Connections")
            .children
            .iter()
            .map(|c| {
                assert_eq!(c.name, "C");
                let kind = c.values[0].as_str().to_owned();
                match kind.as_str() {
                    "OO" => assert_eq!(c.values.len(), 3),
                    "OP" => assert_eq!(c.values.len(), 4),
                    other => panic!("unexpected connection type {other}"),
                }
                Connection {
                    kind,
                    source: c.values[1].as_i64(),
                    target: c.values[2].as_i64(),
                    property: c.values.get(3).map(|p| p.as_str().to_owned()),
                }
            })
            .collect()
    }

    /// Objects connected to `target` object-to-object, in file order, of one record type.
    pub fn sources(&self, target: i64, kind: &str) -> Vec<Object<'_>> {
        let objects: HashMap<i64, Object> = self.objects().into_iter().map(|o| (o.id, o)).collect();
        self.connections()
            .iter()
            .filter(|c| c.kind == "OO" && c.target == target)
            .filter_map(|c| objects.get(&c.source).copied())
            .filter(|o| o.record.name == kind)
            .collect()
    }

    /// The model `id` hangs under; `None` for a model at the scene root.
    pub fn parent_of(&self, id: i64) -> Option<Object<'_>> {
        let parents: Vec<i64> = self
            .connections()
            .iter()
            .filter(|c| c.kind == "OO" && c.source == id)
            .map(|c| c.target)
            .collect();
        assert_eq!(parents.len(), 1, "a model has one parent");
        (parents[0] != 0).then(|| self.object(parents[0]))
    }

    pub fn child_models(&self, id: i64) -> Vec<Object<'_>> {
        self.sources(id, "Model")
    }

    /// The model's placement relative to its parent, from its rest properties.
    pub fn local_matrix(&self, model: &Object) -> DAffine3 {
        trs(
            model.record.vector("Lcl Translation"),
            model.record.vector("Lcl Rotation"),
            model.record.vector("Lcl Scaling"),
        )
    }

    /// The model's placement in the scene: parents apply as plain matrix products,
    /// which is what `InheritType` 1 means.
    pub fn world_matrix(&self, model: &Object) -> DAffine3 {
        let mut m = self.local_matrix(model);
        let mut current = self.parent_of(model.id);
        while let Some(parent) = current {
            m = self.local_matrix(&parent) * m;
            current = self.parent_of(parent.id);
        }
        m
    }

    /// The geometry connected to a model, decoded.
    pub fn geometry_of(&self, model: &Object) -> Geometry {
        let geometries = self.sources(model.id, "Geometry");
        assert_eq!(geometries.len(), 1, "{} has one geometry", model.name);
        decode_geometry(geometries[0].record)
    }

    /// Names of the materials in a model's slots, in slot order.
    pub fn material_names(&self, model: &Object) -> Vec<String> {
        self.sources(model.id, "Material")
            .iter()
            .map(|m| m.name.to_owned())
            .collect()
    }

    pub fn attribute_of(&self, model: &Object) -> Object<'_> {
        let attributes = self.sources(model.id, "NodeAttribute");
        assert_eq!(attributes.len(), 1, "{} has one attribute", model.name);
        attributes[0]
    }

    /// The curves animating `property` of an object, by component ("d|X", ...).
    pub fn curves(&self, target: i64, property: &str) -> Option<HashMap<String, Curve>> {
        let connections = self.connections();
        let nodes: Vec<i64> = connections
            .iter()
            .filter(|c| {
                c.kind == "OP" && c.target == target && c.property.as_deref() == Some(property)
            })
            .map(|c| c.source)
            .collect();
        assert!(nodes.len() <= 1, "one curve node per property");
        let node = *nodes.first()?;
        assert_eq!(self.object(node).record.name, "AnimationCurveNode");
        Some(
            connections
                .iter()
                .filter(|c| c.kind == "OP" && c.target == node)
                .map(|c| {
                    (
                        c.property.clone().expect("a component"),
                        decode_curve(self.object(c.source).record),
                    )
                })
                .collect(),
        )
    }

    /// A three-component property of a model at an FBX time: its curves when it has
    /// them, otherwise the rest value.
    pub fn vector_at(&self, model: &Object, property: &str, time: i64) -> DVec3 {
        match self.curves(model.id, property) {
            Some(curves) => DVec3::new(
                curves["d|X"].sample(time),
                curves["d|Y"].sample(time),
                curves["d|Z"].sample(time),
            ),
            None => model.record.vector(property),
        }
    }

    pub fn local_matrix_at(&self, model: &Object, time: i64) -> DAffine3 {
        trs(
            self.vector_at(model, "Lcl Translation", time),
            self.vector_at(model, "Lcl Rotation", time),
            self.vector_at(model, "Lcl Scaling", time),
        )
    }

    /// The `Visibility` property at an FBX time.
    pub fn visibility_at(&self, model: &Object, time: i64) -> f64 {
        match self.curves(model.id, "Visibility") {
            Some(curves) => curves["d|Visibility"].sample(time),
            None => model.record.number("Visibility"),
        }
    }

    /// Checks everything that must hold in any file the exporter writes, beyond what
    /// [`Fbx::parse`] already checked about the container.
    pub fn validate(&self) {
        let order: Vec<&str> = self.records.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            order,
            [
                "FBXHeaderExtension",
                "FileId",
                "CreationTime",
                "Creator",
                "GlobalSettings",
                "Documents",
                "References",
                "Definitions",
                "Objects",
                "Connections",
                "Takes",
            ],
            "the sections every importer expects, in the usual order"
        );
        let header = self.section("FBXHeaderExtension");
        assert_eq!(header.value("FBXVersion").as_i64(), i64::from(self.version));
        assert_eq!(header.value("FBXHeaderVersion").as_i64(), 1003);
        assert_eq!(header.value("EncryptionType").as_i64(), 0);
        assert!(matches!(&self.section("FileId").values[0], Value::Raw(id) if id.len() == 16));
        let document = self.section("Documents").child("Document");
        assert_eq!(document.value("RootNode").as_i64(), 0);

        // Objects: unique non-zero ids, and counts that match the definitions.
        let objects = self.objects();
        let mut ids = HashSet::new();
        let mut counts: HashMap<&str, i64> = HashMap::from([("GlobalSettings", 1)]);
        for object in &objects {
            assert!(object.id != 0, "id 0 is the scene root");
            assert!(ids.insert(object.id), "ids are unique");
            *counts.entry(object.record.name.as_str()).or_default() += 1;
            let class = match object.record.name.as_str() {
                "AnimationStack" => "AnimStack",
                "AnimationLayer" => "AnimLayer",
                "AnimationCurveNode" => "AnimCurveNode",
                "AnimationCurve" => "AnimCurve",
                other => other,
            };
            assert_eq!(object.class, class, "{}: class in the name", object.name);
        }
        let definitions = self.section("Definitions");
        let mut defined: HashMap<&str, i64> = HashMap::new();
        for kind in definitions.children_named("ObjectType") {
            let count = kind.value("Count").as_i64();
            assert!(count > 0, "only types that occur are defined");
            defined.insert(kind.values[0].as_str(), count);
        }
        assert_eq!(defined, counts, "definitions count the objects per type");
        assert_eq!(
            definitions.value("Count").as_i64(),
            counts.values().sum::<i64>()
        );

        // Connections: both ends exist, no duplicates.
        let connections = self.connections();
        let mut seen = HashSet::new();
        for c in &connections {
            assert!(
                ids.contains(&c.source),
                "connection source {} exists",
                c.source
            );
            assert!(
                c.target == 0 || ids.contains(&c.target),
                "connection target {} exists",
                c.target
            );
            assert!(
                seen.insert((c.kind.clone(), c.source, c.target, c.property.clone())),
                "no connection is written twice"
            );
        }

        // Models: one parent each, all reachable from the root.
        for model in self.objects_of("Model") {
            let mut steps = 0;
            let mut current = self.parent_of(model.id);
            while let Some(parent) = current {
                assert_eq!(parent.record.name, "Model", "models hang under models");
                steps += 1;
                assert!(steps <= objects.len(), "the model tree has no cycle");
                current = self.parent_of(parent.id);
            }
            assert!(["Null", "Mesh", "Light", "Camera"].contains(&model.subtype));
            let geometries = self.sources(model.id, "Geometry").len();
            let attributes = self.sources(model.id, "NodeAttribute").len();
            match model.subtype {
                "Mesh" => assert_eq!((geometries, attributes), (1, 0), "{}", model.name),
                "Light" | "Camera" => {
                    assert_eq!((geometries, attributes), (0, 1), "{}", model.name);
                    assert_eq!(self.attribute_of(&model).subtype, model.subtype);
                }
                _ => assert_eq!((geometries, attributes), (0, 0), "{}", model.name),
            }
            // A plain matrix product down the tree, and Euler order XYZ.
            assert_eq!(model.record.integer("InheritType"), 1);
            assert_eq!(model.record.integer("RotationOrder"), 0);
        }

        // Geometries decode, and their slots exist on every model that uses them.
        for geometry in self.objects_of("Geometry") {
            assert_eq!(geometry.subtype, "Mesh");
            let decoded = decode_geometry(geometry.record);
            let slots = decoded.slots.iter().max().map_or(0, |s| s + 1);
            let users: Vec<i64> = connections
                .iter()
                .filter(|c| c.kind == "OO" && c.source == geometry.id)
                .map(|c| c.target)
                .collect();
            assert!(!users.is_empty(), "{}: a geometry is used", geometry.name);
            for user in users {
                assert_eq!(
                    self.sources(user, "Material").len(),
                    slots,
                    "{}: one material per slot",
                    geometry.name
                );
            }
        }

        // Animation: stack > layer > curve nodes > curves, all wired up.
        let stacks = self.objects_of("AnimationStack");
        let layers = self.objects_of("AnimationLayer");
        let nodes = self.objects_of("AnimationCurveNode");
        let curves = self.objects_of("AnimationCurve");
        if nodes.is_empty() {
            assert!(stacks.is_empty() && layers.is_empty() && curves.is_empty());
            return;
        }
        assert_eq!(
            (stacks.len(), layers.len()),
            (1, 1),
            "one stack with one layer"
        );
        assert_eq!(self.sources(stacks[0].id, "AnimationLayer").len(), 1);
        let stop = stacks[0].record.integer("LocalStop");
        assert_eq!(stacks[0].record.integer("LocalStart"), 0);
        assert!(stop > 0);
        assert_eq!(
            self.sources(layers[0].id, "AnimationCurveNode").len(),
            nodes.len(),
            "every curve node is in the layer"
        );
        let mut curve_users = 0;
        for node in &nodes {
            let drives: Vec<&Connection> = connections
                .iter()
                .filter(|c| c.kind == "OP" && c.source == node.id)
                .collect();
            assert_eq!(drives.len(), 1, "a curve node drives one property");
            let property = drives[0].property.as_deref().unwrap();
            let components: &[&str] = match property {
                "Lcl Translation" | "Lcl Rotation" | "Lcl Scaling" => &["d|X", "d|Y", "d|Z"],
                "Visibility" => &["d|Visibility"],
                other => panic!("unexpected animated property {other}"),
            };
            assert_eq!(self.object(drives[0].target).record.name, "Model");
            assert!(
                self.object(drives[0].target)
                    .record
                    .property_flags(property)
                    .contains('+'),
                "{property}: an animated property is flagged as such"
            );
            let inputs: Vec<&Connection> = connections
                .iter()
                .filter(|c| c.kind == "OP" && c.target == node.id)
                .collect();
            let mut wired: Vec<&str> = inputs
                .iter()
                .map(|c| c.property.as_deref().unwrap())
                .collect();
            wired.sort_unstable();
            assert_eq!(wired, components, "{property}: one curve per component");
            for component in components {
                let input = inputs
                    .iter()
                    .find(|c| c.property.as_deref() == Some(*component))
                    .unwrap();
                let curve = decode_curve(self.object(input.source).record);
                assert_eq!(
                    *curve.times.last().unwrap(),
                    stop,
                    "{property}: every curve spans the whole animation"
                );
                // The curve node's default is the first key.
                let default = node.record.number(component);
                assert!((default - f64::from(curve.values[0])).abs() < 1e-4);
                curve_users += 1;
            }
        }
        assert_eq!(
            curve_users,
            curves.len(),
            "every curve is used exactly once"
        );
    }
}

fn layer_element<'a>(geometry: &'a Record, kind: &str) -> Option<&'a Record> {
    let element = geometry.get(kind)?;
    assert_eq!(element.values[0].as_i64(), 0, "{kind} index");
    let listed = geometry
        .child("Layer")
        .children_named("LayerElement")
        .any(|e| e.value("Type").as_str() == kind && e.value("TypedIndex").as_i64() == 0);
    assert!(listed, "{kind} is listed in the layer");
    Some(element)
}

fn triples(flat: &[f64]) -> Vec<DVec3> {
    assert_eq!(flat.len() % 3, 0, "triples");
    flat.as_chunks::<3>()
        .0
        .iter()
        .map(|c| DVec3::from_array(*c))
        .collect()
}

/// Decodes a `Geometry` record, checking every array against the others.
pub fn decode_geometry(geometry: &Record) -> Geometry {
    let positions = triples(geometry.value("Vertices").as_f64s());
    assert!(!positions.is_empty());
    assert!(positions.iter().all(|p| p.is_finite()));
    let raw = geometry.value("PolygonVertexIndex").as_i32s();
    assert_eq!(raw.len() % 3, 0, "triangles only");
    let triangles: Vec<[u32; 3]> = raw
        .as_chunks::<3>()
        .0
        .iter()
        .map(|t| {
            assert!(
                t[0] >= 0 && t[1] >= 0,
                "only a polygon's last corner is negative"
            );
            assert!(
                t[2] < 0,
                "a polygon's last corner is stored as its complement"
            );
            let triangle = [t[0] as u32, t[1] as u32, !t[2] as u32];
            assert!(triangle.iter().all(|i| (*i as usize) < positions.len()));
            triangle
        })
        .collect();
    let layers: Vec<&Record> = geometry.children_named("Layer").collect();
    assert_eq!(layers.len(), 1, "one layer");
    for element in layers[0].children_named("LayerElement") {
        let kind = element.value("Type").as_str();
        assert!(
            geometry.get(kind).is_some(),
            "the layer lists {kind}, which exists"
        );
    }

    let normals = match layer_element(geometry, "LayerElementNormal") {
        Some(element) => {
            assert_eq!(
                element.value("MappingInformationType").as_str(),
                "ByVertice"
            );
            assert_eq!(element.value("ReferenceInformationType").as_str(), "Direct");
            let normals = triples(element.value("Normals").as_f64s());
            assert_eq!(normals.len(), positions.len(), "one normal per vertex");
            assert!(normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
            normals
        }
        None => Vec::new(),
    };

    let element = layer_element(geometry, "LayerElementColor").expect("vertex colours");
    assert_eq!(
        element.value("MappingInformationType").as_str(),
        "ByPolygonVertex"
    );
    assert_eq!(
        element.value("ReferenceInformationType").as_str(),
        "IndexToDirect"
    );
    let flat = element.value("Colors").as_f64s();
    assert_eq!(flat.len() % 4, 0, "RGBA");
    let palette: Vec<[f64; 4]> = flat.as_chunks::<4>().0.to_vec();
    let index = element.value("ColorIndex").as_i32s();
    assert_eq!(
        index.len(),
        raw.len(),
        "one colour index per polygon corner"
    );
    // Colours are per corner; the exporter stores one per vertex, so every corner of
    // a vertex must agree and the colours can be handed back per vertex.
    let mut colors: Vec<Option<[f64; 4]>> = vec![None; positions.len()];
    for (corner, color_index) in index.iter().enumerate() {
        assert!(*color_index >= 0 && (*color_index as usize) < palette.len());
        let color = palette[*color_index as usize];
        assert!(color.iter().all(|c| c.is_finite() && *c >= 0.0));
        assert!((0.0..=1.0).contains(&color[3]), "alpha is within 0..1");
        let vertex = triangles[corner / 3][corner % 3] as usize;
        assert!(
            colors[vertex].is_none_or(|c| c == color),
            "a vertex has one colour"
        );
        colors[vertex] = Some(color);
    }
    let colors = colors
        .into_iter()
        .map(|c| c.expect("every vertex is used by a triangle"))
        .collect();

    let element = layer_element(geometry, "LayerElementMaterial").expect("material slots");
    assert_eq!(
        element.value("ReferenceInformationType").as_str(),
        "IndexToDirect"
    );
    let materials = element.value("Materials").as_i32s();
    let slots = match element.value("MappingInformationType").as_str() {
        "AllSame" => {
            assert_eq!(materials, [0]);
            vec![0; triangles.len()]
        }
        "ByPolygon" => {
            assert_eq!(materials.len(), triangles.len(), "one slot per polygon");
            assert!(materials.iter().all(|m| *m >= 0));
            assert!(
                materials.iter().any(|m| *m > 0),
                "more than one slot is used"
            );
            materials.iter().map(|m| *m as usize).collect()
        }
        other => panic!("unexpected material mapping {other}"),
    };
    Geometry {
        positions,
        normals,
        colors,
        triangles,
        slots,
    }
}

/// Decodes an `AnimationCurve` record, checking its arrays against each other.
pub fn decode_curve(curve: &Record) -> Curve {
    assert_eq!(curve.value("KeyVer").as_i64(), 4008);
    let times = curve.value("KeyTime").as_i64s().to_vec();
    let values = curve.value("KeyValueFloat").as_f32s().to_vec();
    assert!(times.len() >= 2, "an animated curve has at least two keys");
    assert_eq!(times.len(), values.len(), "one value per key");
    assert!(times.windows(2).all(|w| w[0] < w[1]), "key times increase");
    assert_eq!(times[0], 0, "curves start at time zero");
    assert!(values.iter().all(|v| v.is_finite()));
    assert!((curve.value("Default").as_f64() - f64::from(values[0])).abs() < 1e-4);
    // Attributes are run-length encoded: each flag set applies to a count of keys.
    let flags = curve.value("KeyAttrFlags").as_i32s();
    let counts = curve.value("KeyAttrRefCount").as_i32s();
    let data = curve.value("KeyAttrDataFloat").as_f32s();
    assert_eq!(flags.len(), counts.len());
    assert_eq!(data.len(), 4 * flags.len(), "four floats per attribute set");
    assert_eq!(
        counts.iter().map(|c| *c as usize).sum::<usize>(),
        times.len(),
        "the attribute runs cover every key"
    );
    let mut constant = Vec::with_capacity(times.len());
    for (flag, count) in flags.iter().zip(counts) {
        let held = match flag & 0xE {
            0x2 => {
                // Bit 8 on a constant key means "take the next key's value".
                assert_eq!(flag & 0x100, 0, "constant keys hold their own value");
                true
            }
            0x4 => false,
            other => panic!("keys are constant or linear, not {other:#x}"),
        };
        constant.extend(std::iter::repeat_n(held, *count as usize));
    }
    Curve {
        times,
        values,
        constant,
    }
}
