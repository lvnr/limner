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

    /// Appends an RGB picture record and returns its offset.
    pub fn picture_rgb(&mut self, width: u32, height: u32, pixels: &[u8]) -> String {
        assert_eq!(pixels.len(), (width * height * 3) as usize);
        let offset = self.bytes.len();
        self.bytes.extend_from_slice(&[0, 0, 0, 1, 2, 6, 0, 0]);
        self.bytes.extend_from_slice(&width.to_le_bytes());
        self.bytes.extend_from_slice(&height.to_le_bytes());
        self.bytes.extend_from_slice(&1u32.to_le_bytes());
        self.bytes.extend_from_slice(pixels);
        format!("{offset:016x}")
    }
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
