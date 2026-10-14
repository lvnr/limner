//! Builds small Quill projects on disk for tests, byte for byte in Quill's layout.
//!
//! Available to other crates' tests through the `testing` feature.

use limner_core::model::Point;
use serde_json::{json, Value};
use std::path::Path;

pub fn point(x: f32, y: f32, z: f32, width: f32) -> Point {
    Point {
        pos: [x, y, z],
        normal: [0.0, 1.0, 0.0],
        dir: [0.0, 0.0, -1.0],
        color: [0.25, 0.5, 0.75],
        opacity: 1.0,
        width,
    }
}

/// A straight tube with zero-width ends, like the ones Quill writes.
pub fn tube(start_x: f32, points: usize) -> Vec<Point> {
    (0..points)
        .map(|i| {
            point(
                start_x + i as f32 * 0.01,
                1.0,
                0.0,
                if i == 0 || i + 1 == points { 0.0 } else { 0.02 },
            )
        })
        .collect()
}

pub struct StrokeSpec {
    pub id: u32,
    pub brush: i16,
    /// 0 = directional, 1 = always visible.
    pub visibility: u8,
    pub points: Vec<Point>,
}

impl StrokeSpec {
    pub fn new(id: u32, brush: i16, points: Vec<Point>) -> Self {
        Self {
            id,
            brush,
            visibility: 1,
            points,
        }
    }
}

/// A model as Quill's mesh serializer stores it: one vertex stream holding positions,
/// then colours and normals when given, and a list of polygons.
#[derive(Clone, Debug, Default)]
pub struct MeshSpec {
    pub positions: Vec<[f32; 3]>,
    pub colors: Option<Vec<[f32; 3]>>,
    pub normals: Option<Vec<[f32; 3]>>,
    /// Vertex indices of each face. Quill writes triangles and quads.
    pub faces: Vec<Vec<u32>>,
}

pub struct QbinBuilder {
    pub bytes: Vec<u8>,
}

impl Default for QbinBuilder {
    fn default() -> Self {
        Self {
            bytes: vec![0u8; 8],
        }
    }
}

impl QbinBuilder {
    /// Appends a drawing record and returns its offset as Quill writes it (16 hex digits).
    pub fn drawing(&mut self, strokes: &[StrokeSpec]) -> String {
        let offset = self.bytes.len();
        self.bytes
            .extend_from_slice(&(strokes.len() as u32).to_le_bytes());
        for s in strokes {
            self.bytes.extend_from_slice(&s.id.to_le_bytes());
            self.bytes.extend_from_slice(&0i32.to_le_bytes());
            self.bytes.extend_from_slice(&[0u8; 24]);
            self.bytes.extend_from_slice(&s.brush.to_le_bytes());
            self.bytes.push(s.visibility);
            self.bytes.push(0);
            self.bytes
                .extend_from_slice(&(s.points.len() as i32).to_le_bytes());
            self.bytes
                .extend_from_slice(bytemuck::cast_slice(&s.points));
        }
        format!("{offset:016x}")
    }

    /// A file from January 2017: the header holds a stroke-id counter instead of zeros.
    pub fn with_stroke_counter(next_id: u32) -> Self {
        let mut bytes = next_id.to_le_bytes().to_vec();
        bytes.extend_from_slice(&[0u8; 4]);
        Self { bytes }
    }

    /// Appends an RGB picture record and returns its offset.
    pub fn picture_rgb(&mut self, width: u32, height: u32, pixels: &[u8]) -> String {
        assert_eq!(pixels.len(), (width * height * 3) as usize);
        self.picture(6, width, height, pixels)
    }

    /// Appends an RGBA picture record and returns its offset.
    pub fn picture_rgba(&mut self, width: u32, height: u32, pixels: &[u8]) -> String {
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        self.picture(7, width, height, pixels)
    }

    /// Appends a picture record with the given pixel format byte (6 RGB, 7 RGBA).
    pub fn picture(&mut self, format: u8, width: u32, height: u32, pixels: &[u8]) -> String {
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(&[0, 0, 0, 1, 2, format, 0, 0]);
        self.bytes.extend_from_slice(&width.to_le_bytes());
        self.bytes.extend_from_slice(&height.to_le_bytes());
        self.bytes.extend_from_slice(&1u32.to_le_bytes());
        self.bytes.extend_from_slice(pixels);
        format!("{offset:016x}")
    }

    /// Appends a model record in the layout of Quill's mesh serializer and returns its offset.
    pub fn mesh(&mut self, spec: &MeshSpec) -> String {
        let offset = self.bytes.len();
        let attributes: Vec<&Vec<[f32; 3]>> = [spec.colors.as_ref(), spec.normals.as_ref()]
            .into_iter()
            .flatten()
            .collect();
        for attribute in &attributes {
            assert_eq!(attribute.len(), spec.positions.len());
        }
        let element_count = 1 + attributes.len() as u32;
        let stride = 12 * element_count;

        let mut bounds = [f32::MAX, f32::MIN, f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for p in spec
            .positions
            .iter()
            .filter(|p| p.iter().all(|v| v.is_finite()))
        {
            for axis in 0..3 {
                bounds[axis * 2] = bounds[axis * 2].min(p[axis]);
                bounds[axis * 2 + 1] = bounds[axis * 2 + 1].max(p[axis]);
            }
        }

        let out = &mut self.bytes;
        let word = |out: &mut Vec<u8>, v: u32| out.extend_from_slice(&v.to_le_bytes());
        out.extend_from_slice(&[0u8; 8]);
        word(out, 0); // version
        out.extend_from_slice(bytemuck::cast_slice(&bounds));
        word(out, 1); // vertex streams
        word(out, spec.positions.len() as u32);
        word(out, stride);
        word(out, 0); // divisor
        word(out, element_count);
        for element in 0..element_count {
            word(out, 1); // 32-bit float
            word(out, 3);
            word(out, 0);
            word(out, element * 12);
        }
        for (index, position) in spec.positions.iter().enumerate() {
            out.extend_from_slice(bytemuck::cast_slice(position));
            for attribute in &attributes {
                out.extend_from_slice(bytemuck::cast_slice(&attribute[index]));
            }
        }
        word(out, 0); // polygons
        word(out, 1); // 32-bit indices
        word(out, 1); // index arrays
        word(out, spec.faces.len() as u32);
        for face in &spec.faces {
            out.extend_from_slice(&(face.len() as i32).to_le_bytes());
            for corner in 0..4 {
                // Quill leaves whatever was in memory in the unused slot of a triangle.
                word(out, face.get(corner).copied().unwrap_or(0xcdcd_cdcd));
            }
        }
        format!("{offset:016x}")
    }

    /// Appends a sound record in the layout of Quill's wave serializer and returns its offset.
    pub fn sound_wave(&mut self, channels: u8, bits: u8, sample_rate: u32, pcm: &[u8]) -> String {
        let frame_len = channels as usize * bits as usize / 8;
        assert_eq!(pcm.len() % frame_len, 0);
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(&0u32.to_le_bytes());
        self.bytes.extend_from_slice(&0u16.to_le_bytes());
        self.bytes.push(channels);
        self.bytes.push(bits);
        self.bytes.extend_from_slice(&sample_rate.to_le_bytes());
        self.bytes
            .extend_from_slice(&((pcm.len() / frame_len) as u64).to_le_bytes());
        self.bytes.extend_from_slice(pcm);
        format!("{offset:016x}")
    }

    /// Appends a sound record in the other candidate layout, a length followed by a
    /// whole audio file, and returns its offset.
    pub fn sound_file(&mut self, file: &[u8]) -> String {
        let offset = self.bytes.len();
        self.bytes
            .extend_from_slice(&(file.len() as i32).to_le_bytes());
        self.bytes.extend_from_slice(file);
        format!("{offset:016x}")
    }
}

/// A minimal PCM WAV file.
pub fn wav_file(channels: u16, bits: u16, sample_rate: u32, pcm: &[u8]) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(36 + pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&(sample_rate * block as u32).to_le_bytes());
    out.extend_from_slice(&block.to_le_bytes());
    out.extend_from_slice(&bits.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(pcm.len() as u32).to_le_bytes());
    out.extend_from_slice(pcm);
    out
}

pub fn identity() -> Value {
    json!({"Rotation": [0.0, 0.0, 0.0, 1.0], "Scale": 1.0, "Flip": "N", "Translation": [0.0, 0.0, 0.0]})
}

pub fn xform(rotation: [f64; 4], scale: f64, flip: &str, translation: [f64; 3]) -> Value {
    json!({"Rotation": rotation, "Scale": scale, "Flip": flip, "Translation": translation})
}

pub fn visible_from_zero() -> Value {
    json!({
        "Visibility": [{"Time": "0", "Value": true, "Interpolation": "None"}],
        "Offset": [{"Time": "0", "Value": 0, "Interpolation": "None"}]
    })
}

/// A layer in the shape Quill 2.x writes, with string-typed times.
pub fn layer(name: &str, type_name: &str, implementation: Value) -> Value {
    json!({
        "Name": name, "Visible": true, "Locked": false, "Collapsed": false, "BBoxVisible": false,
        "Opacity": 1.0, "Type": type_name, "IsModelTopLayer": false, "KeepAlive": {"Type": "None"},
        "Transform": identity(), "Pivot": identity(),
        "Animation": {"Duration": "0", "Timeline": false, "StartOffset": "0", "MaxRepeatCount": "0", "Keys": visible_from_zero()},
        "Implementation": implementation
    })
}

pub fn group(name: &str, children: Vec<Value>) -> Value {
    layer(name, "Group", json!({"Children": children}))
}

pub fn paint(name: &str, drawings: &[&str], frames: &[u32]) -> Value {
    let drawings: Vec<Value> = drawings
        .iter()
        .map(|o| json!({"BoundingBox": [0.0, 1.0, 0.0, 1.0, 0.0, 1.0], "DataFileOffset": o}))
        .collect();
    let frames: Vec<String> = frames.iter().map(|f| f.to_string()).collect();
    layer(
        name,
        "Paint",
        json!({"Framerate": 24, "MaxRepeatCount": 0, "Drawings": drawings, "Frames": frames}),
    )
}

/// A picture layer. `type_name` is "2D", "360 Equirectangular Mono" and so on.
pub fn picture(name: &str, type_name: &str, offset: &str, import_path: &str) -> Value {
    layer(
        name,
        "Picture",
        json!({"Type": type_name, "ViewerLocked": false, "DataFileOffset": offset, "ImportFilePath": import_path}),
    )
}

/// A model layer as Quill writes it for an imported OBJ.
pub fn model(name: &str, offset: &str, import_path: &str) -> Value {
    let mut layer = layer(
        name,
        "Model",
        json!({"DataFileOffset": offset, "ImportFilePath": import_path, "ShadingModel": "Unlit", "RenderWireFrame": false}),
    );
    layer["IsModelTopLayer"] = json!(true);
    layer
}

/// A sound layer with the fields the community parsers know.
pub fn sound(name: &str, offset: &str, import_path: &str) -> Value {
    layer(
        name,
        "Sound",
        json!({
            "DataFileOffset": offset, "ImportFilePath": import_path, "Type": "Flat", "Gain": 1.0, "Loop": false,
            "Attenuation": {"Mode": "None", "Minimum": 1.0, "Maximum": 10.0},
            "Modifiers": [{"Type": "None"}]
        }),
    )
}

/// A row-major 4x4 matrix as files from before Quill 1.7 store transforms: uniform
/// scale and a translation in the fourth column.
pub fn matrix(scale: f64, translation: [f64; 3]) -> Value {
    let [x, y, z] = translation;
    json!([scale, 0.0, 0.0, x, 0.0, scale, 0.0, y, 0.0, 0.0, scale, z, 0.0, 0.0, 0.0, 1.0])
}

/// A layer in the January 2017 schema: a matrix transform and `AnimOffset`, with no
/// pivot, animation block or keep-alive.
pub fn layer_2017(name: &str, type_name: &str, transform: Value, implementation: Value) -> Value {
    json!({
        "Name": name, "Visible": true, "Locked": false, "Collapsed": false, "BBoxVisible": false,
        "Opacity": 1.0, "Type": type_name, "Transform": transform, "AnimOffset": 0.0,
        "Implementation": implementation
    })
}

/// A January 2017 paint layer: one drawing stored on the layer, with an unpadded offset.
pub fn paint_2017(name: &str, transform: Value, offset: &str) -> Value {
    let bare = offset.trim_start_matches('0');
    layer_2017(
        name,
        "Paint",
        transform,
        json!({
            "BoundingBox": [0.0, 1.0, 0.0, 1.0, 0.0, 1.0], "AnimSpeed": 125.0, "PlaybackReduce": 1,
            "DataFileOffset": if bare.is_empty() { "0" } else { bare }
        }),
    )
}

/// A whole January 2017 `Quill.json`.
pub fn project_2017(root_children: Vec<Value>) -> Value {
    let root = layer_2017(
        "Root",
        "Group",
        matrix(1.0, [0.0; 3]),
        json!({"Children": root_children}),
    );
    json!({"Version": 1, "Sequence": {
        "BackgroundColor": [0.1, 0.2, 0.3], "HomePosition": matrix(1.0, [0.0; 3]), "TrackingOrigin": 1,
        "RootLayer": root
    }})
}

/// A January 2017 `State.json`: a numeric tool id and boolean tapers.
pub fn state_2017() -> Value {
    json!({"Quill": {
        "ShowGrid": false, "MoveLayer": "Root/Paint", "PaintLayer": "Root/Paint", "DirectManipulation": false,
        "ToolID": 0,
        "Tool": {"BrushID": 3, "Color": [0.8, 0.5, 0.1], "Opacity": 1.0, "Size": 0.01,
                 "TransparentTaper": true, "WidthTaper": true, "DirectionalStroke": false},
        "ColorPalette": vec![0.82; 48]
    }})
}

pub fn project(root_children: Vec<Value>) -> Value {
    let mut root = group("Root", root_children);
    root["Animation"]["Timeline"] = json!(true);
    root["Animation"]["Duration"] = json!("45360000");
    json!({"Version": 1, "Sequence": {
        "BackgroundColor": [0.1, 0.2, 0.3], "DefaultViewpoint": "Root/InitialSpawnArea", "Framerate": 24,
        "ExportStart": 0, "ExportEnd": 126000, "CameraResolution": [1920, 1080], "RootLayer": root
    }})
}

pub fn write_project(dir: &Path, quill_json: &Value, qbin: &[u8], state: Option<&Value>) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("Quill.json"),
        serde_json::to_vec_pretty(quill_json).unwrap(),
    )
    .unwrap();
    std::fs::write(dir.join("Quill.qbin"), qbin).unwrap();
    if let Some(state) = state {
        std::fs::write(dir.join("State.json"), serde_json::to_vec(state).unwrap()).unwrap();
    }
}
