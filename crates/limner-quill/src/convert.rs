//! Converts a parsed `Quill.json` and its `Quill.qbin` into a Limner document.

use crate::error::ImportError;
use crate::json;
use crate::qbin::{DrawingReport, Qbin};
use limner_core::glam::{DAffine3, DMat3, DQuat, DVec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Action, Attenuation, AttenuationMode, Camera, Document, Drawing, DrawingId, Group, Interp, Key,
    Layer, LayerAnim, LayerId, LayerKind, MediaId, MediaKind, Model, Paint, Picture, Projection,
    Sequence, Shading, Sound, SoundKind, SoundModifier, Viewpoint,
};
use limner_core::time::{self, FrameRate};
use serde_json::Value;
use std::collections::HashMap;

pub(crate) struct Converted {
    pub document: Document,
    pub warnings: Vec<String>,
}

pub(crate) fn convert(root: &Value, qbin: &[u8]) -> Result<Converted, ImportError> {
    let sequence = root
        .get("Sequence")
        .ok_or_else(|| ImportError::NotAProject("Quill.json has no \"Sequence\" object".into()))?;
    let root_layer = sequence
        .get("RootLayer")
        .ok_or_else(|| ImportError::NotAProject("Quill.json has no \"RootLayer\"".into()))?;
    if root_layer
        .get("Implementation")
        .and_then(|i| i.get("Children"))
        .is_none()
    {
        return Err(ImportError::Format("the root layer is not a group".into()));
    }

    let mut c = Converter {
        qbin: Qbin::new(qbin),
        doc: Document::empty(),
        warnings: Vec::new(),
        drawings_by_offset: HashMap::new(),
        pictures_by_offset: HashMap::new(),
        totals: DrawingReport::default(),
        damaged_drawings: 0,
        unreadable_drawings: 0,
    };
    c.read_settings(sequence);
    c.read_root(root_layer);

    if let Some(path) = json::get_str(sequence, "DefaultViewpoint").filter(|p| !p.is_empty()) {
        c.doc.settings.default_viewpoint = c.doc.find_by_path(path);
        if c.doc.settings.default_viewpoint.is_none() {
            c.warn(format!("the default spawn area \"{path}\" was not found"));
        }
    }
    c.summarise_drawing_problems();
    c.doc
        .validate()
        .map_err(|e| ImportError::Format(e.to_string()))?;
    Ok(Converted {
        document: c.doc,
        warnings: c.warnings,
    })
}

struct Converter<'a> {
    qbin: Qbin<'a>,
    doc: Document,
    warnings: Vec<String>,
    /// Several layers can point at the same record; they then share one drawing.
    drawings_by_offset: HashMap<u64, DrawingId>,
    pictures_by_offset: HashMap<u64, MediaId>,
    totals: DrawingReport,
    damaged_drawings: usize,
    unreadable_drawings: usize,
}

impl Converter<'_> {
    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    fn read_settings(&mut self, sequence: &Value) {
        let settings = &mut self.doc.settings;
        if let Some(color) = sequence.get("BackgroundColor").and_then(json::floats::<3>) {
            settings.background = color.map(|v| v as f32);
        }
        if let Some(fps) = json::get_f64(sequence, "Framerate").filter(|f| *f > 0.0) {
            settings.rate = FrameRate::from_f64(fps);
        }
        if let Some(start) = json::get_i64(sequence, "ExportStart") {
            settings.range_start = time::from_quill_ticks(start);
        }
        if let Some(end) = json::get_i64(sequence, "ExportEnd") {
            settings.range_end = time::from_quill_ticks(end);
        }
        if let Some([w, h]) = sequence.get("CameraResolution").and_then(json::floats::<2>) {
            if w >= 1.0 && h >= 1.0 {
                settings.camera_resolution = [w as u32, h as u32];
            }
        }
        if let Some(metadata) = sequence.get("Metadata") {
            settings.title = json::get_str(metadata, "Title")
                .unwrap_or_default()
                .to_owned();
            settings.description = json::get_str(metadata, "Description")
                .unwrap_or_default()
                .to_owned();
        }
    }

    fn read_root(&mut self, v: &Value) {
        let root = self.doc.root();
        let name = json::get_str(v, "Name")
            .filter(|n| !n.is_empty())
            .unwrap_or("Root")
            .to_owned();
        let (anim, sequence) = self.read_animation(v, &name);
        let layer = self.doc.layer_mut(root).expect("root exists");
        layer.name = name.clone();
        read_common(layer, v);
        layer.anim = anim;
        // The root always runs the main clock. Quill stores a one-hour duration on it,
        // which is the length of its timeline, not a loop point.
        let max_repeat = sequence.map_or(0, |s| s.max_repeat);
        layer.as_group_mut().expect("root is a group").sequence = Some(Sequence {
            loop_duration: 0,
            max_repeat,
        });
        self.read_transforms(root, v, &name);
        self.read_children(v, root, &name);
    }

    fn read_children(&mut self, v: &Value, parent: LayerId, parent_path: &str) {
        let children = v
            .get("Implementation")
            .and_then(|i| i.get("Children"))
            .and_then(Value::as_array);
        for child in children.into_iter().flatten() {
            self.read_layer(child, parent, parent_path);
        }
    }

    fn read_layer(&mut self, v: &Value, parent: LayerId, parent_path: &str) {
        let name = json::get_str(v, "Name").unwrap_or("Layer").to_owned();
        let path = format!("{parent_path}/{name}");
        let empty = Value::Null;
        let implementation = v.get("Implementation").unwrap_or(&empty);
        let type_name = json::get_str(v, "Type").unwrap_or("");
        let (anim, sequence) = self.read_animation(v, &path);

        let kind = match type_name {
            "Group" => LayerKind::Group(Group {
                children: Vec::new(),
                sequence,
            }),
            "Paint" => LayerKind::Paint(self.read_paint(implementation, &path)),
            "Viewpoint" => LayerKind::Viewpoint(self.read_viewpoint(implementation)),
            "Camera" => LayerKind::Camera(Camera {
                fov: json::get_f64(implementation, "FOV")
                    .filter(|f| *f > 0.0 && *f < 180.0)
                    .unwrap_or(45.0) as f32,
            }),
            "Picture" => LayerKind::Picture(self.read_picture(implementation, &path)),
            "Model" => {
                self.warn(format!(
                    "{path}: model geometry is not imported yet; the layer is kept empty"
                ));
                LayerKind::Model(Model {
                    media: None,
                    shading: if json::get_str(implementation, "ShadingModel") == Some("Smooth") {
                        Shading::Lit
                    } else {
                        Shading::Unlit
                    },
                    wireframe: json::get_bool(implementation, "RenderWireFrame").unwrap_or(false),
                    source_path: json::get_str(implementation, "ImportFilePath")
                        .unwrap_or_default()
                        .to_owned(),
                })
            }
            "Sound" => {
                self.warn(format!(
                    "{path}: sound data is not imported yet; the layer is kept silent"
                ));
                LayerKind::Sound(read_sound(implementation))
            }
            other if implementation.get("Children").is_some() => {
                self.warn(format!(
                    "{path}: unknown layer type \"{other}\" imported as a group"
                ));
                LayerKind::Group(Group {
                    children: Vec::new(),
                    sequence,
                })
            }
            other => {
                self.warn(format!(
                    "{path}: layer of unknown type \"{other}\" was skipped"
                ));
                return;
            }
        };

        let mut layer = self.doc.new_layer(name, kind, 0);
        read_common(&mut layer, v);
        layer.anim = anim;
        let id = self
            .doc
            .insert_layer(layer, parent, usize::MAX)
            .expect("parent is a group");
        self.read_transforms(id, v, &path);
        if self.doc.layer(id).is_some_and(|l| l.as_group().is_some()) {
            self.read_children(v, id, &path);
        }
    }

    fn read_transforms(&mut self, id: LayerId, v: &Value, path: &str) {
        let transform = self.read_xform(v.get("Transform"), path, "transform");
        let pivot = self.read_xform(v.get("Pivot"), path, "pivot");
        let layer = self.doc.layer_mut(id).expect("layer was just inserted");
        layer.transform = transform;
        layer.pivot = pivot;
    }

    fn read_xform(&mut self, v: Option<&Value>, path: &str, what: &str) -> Xform {
        match v {
            None | Some(Value::Null) => Xform::IDENTITY,
            Some(v) => parse_xform(v).unwrap_or_else(|| {
                self.warn(format!("{path}: unreadable {what}; identity used"));
                Xform::IDENTITY
            }),
        }
    }

    fn read_animation(&mut self, v: &Value, path: &str) -> (LayerAnim, Option<Sequence>) {
        let Some(animation) = v.get("Animation").filter(|a| a.is_object()) else {
            // Files from before Quill 2.0 have no timeline: the layer is simply present.
            return (LayerAnim::starting_at(0), None);
        };
        let mut anim = LayerAnim::default();
        let empty = Value::Null;
        let keys = animation.get("Keys").unwrap_or(&empty);
        let list = |name: &str| {
            keys.get(name)
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[])
        };
        let key_time = |k: &Value| json::get_i64(k, "Time").map(time::from_quill_ticks);

        for k in list("Visibility") {
            if let (Some(t), Some(value)) = (key_time(k), json::get_bool(k, "Value")) {
                anim.visibility.insert(Key::step(t, value));
            }
        }
        for k in list("Offset") {
            if let (Some(t), Some(value)) = (key_time(k), json::get_i64(k, "Value")) {
                anim.offset
                    .insert(Key::step(t, time::from_quill_ticks(value)));
            }
        }
        for k in list("Opacity") {
            if let (Some(t), Some(value)) = (key_time(k), json::get_f64(k, "Value")) {
                let interp = self.read_interp(k, path);
                anim.opacity.insert(Key::new(t, value as f32, interp));
            }
        }
        for k in list("Transform") {
            let value = k.get("Value").and_then(parse_xform);
            match (key_time(k), value) {
                (Some(t), Some(value)) => {
                    let interp = self.read_interp(k, path);
                    anim.transform.insert(Key::new(t, value, interp));
                }
                _ => self.warn(format!("{path}: an unreadable transform key was skipped")),
            }
        }
        for k in list("Action") {
            let action = match k.get("Value") {
                Some(Value::String(s)) => match s.to_ascii_lowercase().as_str() {
                    "stop" => Some(Action::Stop),
                    "play" | "start" => Some(Action::Play),
                    "loop" => Some(Action::Loop),
                    _ => None,
                },
                Some(other) => match json::as_i64(other) {
                    Some(0) => Some(Action::Stop),
                    Some(1) => Some(Action::Play),
                    Some(2) => Some(Action::Loop),
                    _ => None,
                },
                None => None,
            };
            match (key_time(k), action) {
                (Some(t), Some(action)) => anim.actions.insert(Key::step(t, action)),
                _ => self.warn(format!("{path}: an unreadable action key was skipped")),
            }
        }

        // A start offset with no offset key applies to the first clip.
        let start_offset = json::get_i64(animation, "StartOffset").unwrap_or(0);
        if start_offset != 0 && anim.offset.is_empty() {
            let at = anim.visibility.first().map_or(0, |k| k.time);
            anim.offset
                .insert(Key::step(at, time::from_quill_ticks(start_offset)));
        }

        let sequence = json::get_bool(animation, "Timeline")
            .unwrap_or(false)
            .then(|| Sequence {
                loop_duration: time::from_quill_ticks(
                    json::get_i64(animation, "Duration").unwrap_or(0).max(0),
                ),
                max_repeat: json::get_i64(animation, "MaxRepeatCount")
                    .unwrap_or(0)
                    .clamp(0, u32::MAX as i64) as u32,
            });
        (anim, sequence)
    }

    fn read_interp(&mut self, key: &Value, path: &str) -> Interp {
        match json::get_str(key, "Interpolation").unwrap_or("None") {
            "None" => Interp::Step,
            "Linear" => Interp::Linear,
            "EaseIn" => Interp::EaseIn,
            "EaseOut" => Interp::EaseOut,
            "Smoothstep" => Interp::EaseInOut,
            "Spline" | "Auto" => Interp::Smooth,
            other => {
                self.warn(format!(
                    "{path}: unknown interpolation \"{other}\" read as linear"
                ));
                Interp::Linear
            }
        }
    }

    fn read_paint(&mut self, implementation: &Value, path: &str) -> Paint {
        let rate = json::get_f64(implementation, "Framerate")
            .filter(|f| *f > 0.0)
            .map(FrameRate::from_f64)
            .unwrap_or(self.doc.settings.rate);
        let max_repeat = json::get_i64(implementation, "MaxRepeatCount")
            .unwrap_or(0)
            .clamp(0, u32::MAX as i64) as u32;

        let mut drawings: Vec<DrawingId> = Vec::new();
        if let Some(list) = implementation.get("Drawings").and_then(Value::as_array) {
            for entry in list {
                drawings.push(self.drawing_at(entry.get("DataFileOffset"), path));
            }
        } else if implementation.get("DataFileOffset").is_some() {
            // Quill 1.3 and earlier: one drawing stored directly on the layer.
            drawings.push(self.drawing_at(implementation.get("DataFileOffset"), path));
        }
        if drawings.is_empty() {
            drawings.push(self.doc.add_drawing(Drawing::new()));
        }

        let mut frames: Vec<u32> = implementation
            .get("Frames")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(json::as_i64)
                    .map(|f| f.max(0) as u32)
                    .collect()
            })
            .unwrap_or_default();
        if frames.is_empty() {
            frames.push(0);
        }
        let last = (drawings.len() - 1) as u32;
        if frames.iter().any(|f| *f > last) {
            self.warn(format!(
                "{path}: some frames pointed at missing drawings and now show the last one"
            ));
            for f in &mut frames {
                *f = (*f).min(last);
            }
        }
        Paint {
            rate,
            drawings,
            frames,
            max_repeat,
            shading: Shading::Unlit,
        }
    }

    fn drawing_at(&mut self, offset: Option<&Value>, path: &str) -> DrawingId {
        let Some(offset) = offset.and_then(json::file_offset) else {
            self.warn(format!(
                "{path}: a drawing has no readable file offset and is empty"
            ));
            return self.doc.add_drawing(Drawing::new());
        };
        if let Some(id) = self.drawings_by_offset.get(&offset) {
            return *id;
        }
        let drawing = match self.qbin.read_drawing(offset) {
            Some((drawing, report)) => {
                if !report.is_clean() {
                    self.damaged_drawings += 1;
                    self.totals.dropped_strokes += report.dropped_strokes;
                    self.totals.unknown_brushes += report.unknown_brushes;
                    self.totals.truncated |= report.truncated;
                    self.totals.renumbered |= report.renumbered;
                }
                drawing
            }
            None => {
                self.unreadable_drawings += 1;
                Drawing::new()
            }
        };
        let id = self.doc.add_drawing(drawing);
        self.drawings_by_offset.insert(offset, id);
        id
    }

    fn summarise_drawing_problems(&mut self) {
        if self.unreadable_drawings > 0 {
            self.warn(format!(
                "{} drawing(s) point outside Quill.qbin and were imported empty",
                self.unreadable_drawings
            ));
        }
        if self.totals.dropped_strokes > 0 {
            self.warn(format!(
                "{} stroke(s) with no points or non-finite values were skipped",
                self.totals.dropped_strokes
            ));
        }
        if self.totals.truncated {
            self.warn("stroke data ran past the end of Quill.qbin; the affected drawing(s) were cut short".to_owned());
        }
        if self.totals.unknown_brushes > 0 {
            self.warn(format!(
                "{} stroke(s) used an unknown brush type and were read as cylinders",
                self.totals.unknown_brushes
            ));
        }
        if self.totals.renumbered {
            self.warn(
                "some drawings had repeated stroke ids; their strokes were renumbered".to_owned(),
            );
        }
    }

    fn read_viewpoint(&mut self, implementation: &Value) -> Viewpoint {
        let mut viewpoint = Viewpoint {
            // Spawn areas written before the eye/floor choice existed behaved as floor level.
            floor_level: json::get_str(implementation, "TypeStr")
                .is_none_or(|t| t.eq_ignore_ascii_case("FloorLevel")),
            ..Viewpoint::default()
        };
        if let Some(color) = implementation.get("Color").and_then(json::floats::<3>) {
            viewpoint.color = color.map(|v| v as f32);
        }
        if let Some(sphere) = implementation.get("Sphere").and_then(json::floats::<4>) {
            viewpoint.volume = sphere.map(|v| v as f32);
        }
        for (slot, key) in viewpoint.allow_translation.iter_mut().zip([
            "AllowTranslationX",
            "AllowTranslationY",
            "AllowTranslationZ",
        ]) {
            *slot = json::get_bool(implementation, key).unwrap_or(true);
        }
        viewpoint.exporting = json::get_bool(implementation, "Exporting").unwrap_or(true);
        viewpoint.show_volume = json::get_bool(implementation, "ShowingVolume").unwrap_or(false);
        viewpoint.thumbnail = implementation
            .get("DataFileOffset")
            .and_then(json::file_offset)
            .and_then(|offset| self.picture_at(offset, "thumbnail"));
        viewpoint
    }

    fn read_picture(&mut self, implementation: &Value, path: &str) -> Picture {
        let source_path = json::get_str(implementation, "ImportFilePath")
            .unwrap_or_default()
            .to_owned();
        let type_name = json::get_str(implementation, "Type").unwrap_or("2D");
        let lower = type_name.to_ascii_lowercase();
        let projection = if lower.contains("equirect") && lower.contains("stereo") {
            Projection::EquirectStereo
        } else if lower.contains("equirect") {
            Projection::Equirect
        } else if lower.contains("cross") {
            Projection::CubemapCross
        } else if lower.contains("cube") {
            Projection::CubemapStrip
        } else {
            if lower != "2d" {
                self.warn(format!(
                    "{path}: picture type \"{type_name}\" is shown as a flat image"
                ));
            }
            Projection::Flat
        };
        let name = file_name(&source_path).unwrap_or("picture").to_owned();
        let media = implementation
            .get("DataFileOffset")
            .and_then(json::file_offset)
            .and_then(|offset| self.picture_at(offset, &name));
        if media.is_none() {
            self.warn(format!("{path}: the picture's pixels could not be read"));
        }
        Picture {
            media,
            projection,
            viewer_locked: json::get_bool(implementation, "ViewerLocked").unwrap_or(false),
            source_path,
        }
    }

    fn picture_at(&mut self, offset: u64, name: &str) -> Option<MediaId> {
        if let Some(id) = self.pictures_by_offset.get(&offset) {
            return Some(*id);
        }
        let data = self.qbin.read_picture(offset)?;
        let id = self.doc.add_media(name, MediaKind::Image, data);
        self.pictures_by_offset.insert(offset, id);
        Some(id)
    }
}

fn read_common(layer: &mut Layer, v: &Value) {
    layer.visible = json::get_bool(v, "Visible").unwrap_or(true);
    layer.locked = json::get_bool(v, "Locked").unwrap_or(false);
    layer.collapsed = json::get_bool(v, "Collapsed").unwrap_or(false);
    layer.show_bounds = json::get_bool(v, "BBoxVisible").unwrap_or(false);
    layer.opacity = json::get_f64(v, "Opacity").map_or(1.0, |o| (o as f32).clamp(0.0, 1.0));
}

fn read_sound(implementation: &Value) -> Sound {
    let kind = match json::get_str(implementation, "Type").unwrap_or("Flat") {
        "Positional" => SoundKind::Positional,
        "Ambisonic" => SoundKind::Ambisonic,
        _ => SoundKind::Flat,
    };
    let mut attenuation = Attenuation::default();
    if let Some(a) = implementation.get("Attenuation") {
        attenuation.mode = match json::get_str(a, "Mode")
            .unwrap_or("None")
            .to_ascii_lowercase()
            .as_str()
        {
            "linear" => AttenuationMode::Linear,
            "logarithmic" | "logarithm" | "log" => AttenuationMode::Logarithmic,
            _ => AttenuationMode::None,
        };
        attenuation.min = json::get_f64(a, "Minimum").map_or(attenuation.min, |v| v as f32);
        attenuation.max = json::get_f64(a, "Maximum").map_or(attenuation.max, |v| v as f32);
    }
    Sound {
        media: None,
        kind,
        gain: json::get_f64(implementation, "Gain").map_or(1.0, |g| g as f32),
        looped: json::get_bool(implementation, "Loop").unwrap_or(false),
        attenuation,
        modifier: SoundModifier::None,
        source_path: json::get_str(implementation, "ImportFilePath")
            .unwrap_or_default()
            .to_owned(),
    }
}

/// Last component of a Windows or POSIX path.
fn file_name(path: &str) -> Option<&str> {
    path.rsplit(['/', '\\']).next().filter(|s| !s.is_empty())
}

/// Reads a layer transform: an object in Quill 1.7 and later, a row-major 4x4 matrix before.
fn parse_xform(v: &Value) -> Option<Xform> {
    if let Some(m) = json::floats::<16>(v) {
        let linear = DMat3::from_cols(
            DVec3::new(m[0], m[4], m[8]),
            DVec3::new(m[1], m[5], m[9]),
            DVec3::new(m[2], m[6], m[10]),
        );
        let x = Xform::from_affine(&DAffine3::from_mat3_translation(
            linear,
            DVec3::new(m[3], m[7], m[11]),
        ));
        return (x.is_finite() && x.scale > 0.0).then_some(x);
    }
    if !v.is_object() {
        return None;
    }
    let [qx, qy, qz, qw] = v
        .get("Rotation")
        .and_then(json::floats::<4>)
        .unwrap_or([0.0, 0.0, 0.0, 1.0]);
    let rotation = DQuat::from_xyzw(qx, qy, qz, qw);
    let rotation = if rotation.length_squared() > 1e-20 && rotation.is_finite() {
        rotation.normalize()
    } else {
        DQuat::IDENTITY
    };
    let scale = json::get_f64(v, "Scale").unwrap_or(1.0);
    let translation = v
        .get("Translation")
        .and_then(json::floats::<3>)
        .map_or(DVec3::ZERO, DVec3::from_array);
    let flip = match json::get_str(v, "Flip").unwrap_or("N") {
        "X" => Flip::X,
        "Y" => Flip::Y,
        "Z" => Flip::Z,
        _ => Flip::None,
    };
    let x = Xform {
        rotation,
        scale,
        translation,
        flip,
    };
    (x.is_finite() && scale != 0.0).then_some(x)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transform_objects_and_matrices_agree() {
        let object = parse_xform(&json!({"Rotation": [0.0, 0.3826834323650898, 0.0, 0.9238795325112867], "Scale": 2.0, "Flip": "N", "Translation": [1.0, 2.0, 3.0]})).unwrap();
        // The same transform as a row-major matrix: a 45 degree turn about Y, scaled by two.
        let s = 2.0 * std::f64::consts::FRAC_1_SQRT_2;
        let matrix = parse_xform(&json!([
            s, 0.0, s, 1.0, 0.0, 2.0, 0.0, 2.0, -s, 0.0, s, 3.0, 0.0, 0.0, 0.0, 1.0
        ]))
        .unwrap();
        let p = DVec3::new(0.5, -1.0, 2.0);
        assert!((object.transform_point(p) - matrix.transform_point(p)).length() < 1e-9);
    }

    #[test]
    fn mirrored_matrices_keep_their_handedness() {
        let m = parse_xform(&json!([
            -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0
        ]))
        .unwrap();
        assert!(m.flip.is_mirrored());
        assert!(
            (m.transform_point(DVec3::new(1.0, 2.0, 3.0)) - DVec3::new(-1.0, 2.0, 3.0)).length()
                < 1e-12
        );
    }

    #[test]
    fn broken_transforms_are_rejected() {
        assert!(parse_xform(&json!("nope")).is_none());
        assert!(parse_xform(&json!({"Scale": 0.0})).is_none());
        assert!(parse_xform(&json!([1.0, 2.0])).is_none());
        // A zero quaternion falls back to no rotation instead of producing NaNs.
        let x = parse_xform(&json!({"Rotation": [0.0, 0.0, 0.0, 0.0], "Scale": 1.0})).unwrap();
        assert_eq!(x.rotation, DQuat::IDENTITY);
    }

    #[test]
    fn file_names_from_windows_paths() {
        assert_eq!(
            file_name("C:\\Users\\me\\Pictures\\ref image.png"),
            Some("ref image.png")
        );
        assert_eq!(file_name("/home/me/a.jpg"), Some("a.jpg"));
        assert_eq!(file_name(""), None);
    }
}
