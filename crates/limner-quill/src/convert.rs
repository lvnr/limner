//! Converts a parsed `Quill.json` and its `Quill.qbin` into a Limner document.

use crate::error::ImportError;
use crate::json;
use crate::qbin::{DrawingReport, Qbin};
use limner_core::glam::{DAffine3, DMat3, DQuat, DVec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Action, Attenuation, AttenuationMode, Camera, Document, Drawing, DrawingId, Group, Interp, Key,
    Layer, LayerAnim, LayerId, LayerKind, MediaData, MediaId, MediaKind, Model, Paint, Picture,
    Projection, Sequence, Shading, Sound, SoundKind, SoundModifier, Viewpoint,
};
use limner_core::time::{self, FrameRate};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// How much decoded drawing and media data an import may hold, as a multiple of the
/// size of `Quill.qbin`, plus [`DECODED_ALLOWANCE`]. Every record is read once, and
/// none decodes to more than about three times its stored size (a grey picture
/// becomes RGB), so real projects stay far below it. A damaged or hostile
/// `Quill.json` that points many layers at overlapping records stops here instead
/// of multiplying the file in memory.
const DECODED_PER_QBIN_BYTE: usize = 4;
const DECODED_ALLOWANCE: usize = 1 << 20;

/// Placeholders Quill's sharing dialog wrote when the artist typed nothing.
const DEFAULT_TITLE: &str = "Untitled";
const DEFAULT_DESCRIPTION: &str = "Shared from Quill";

pub(crate) struct Converted {
    pub document: Document,
    pub warnings: Vec<String>,
}

/// Converts a project. `qbin` is `None` when the project has no `Quill.qbin`; the
/// caller reports that once, and the per-layer consequences are then not repeated.
pub(crate) fn convert(root: &Value, qbin: Option<&[u8]>) -> Result<Converted, ImportError> {
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

    let qbin_len = qbin.map_or(0, <[u8]>::len);
    let mut c = Converter {
        qbin: Qbin::new(qbin.unwrap_or(&[])),
        has_data: qbin.is_some_and(|bytes| !bytes.is_empty()),
        doc: Document::empty(),
        warnings: Vec::new(),
        drawings_by_offset: HashMap::new(),
        media_by_offset: HashMap::new(),
        totals: DrawingReport::default(),
        damaged_drawings: 0,
        unreadable_drawings: 0,
        decoded: 0,
        budget: qbin_len
            .saturating_mul(DECODED_PER_QBIN_BYTE)
            .saturating_add(DECODED_ALLOWANCE),
    };
    if qbin.is_some_and(|bytes| bytes.is_empty()) {
        c.warn("Quill.qbin is empty; all drawings and media are empty".to_owned());
    }
    if let Some(version) = root
        .get("Version")
        .and_then(json::as_i64)
        .filter(|v| *v != 1)
    {
        c.warn(format!(
            "Quill.json declares format version {version}; this importer was written for version 1"
        ));
    }
    c.read_settings(sequence);
    c.read_root(root_layer);

    if let Some(path) = json::get_str(sequence, "DefaultViewpoint").filter(|p| !p.is_empty()) {
        c.doc.settings.default_viewpoint = c.doc.find_by_path(path);
        if c.doc.settings.default_viewpoint.is_none() {
            c.warn(format!("the default spawn area \"{path}\" was not found"));
        }
    }
    if c.over_budget() {
        return Err(ImportError::Format(format!(
            "Quill.json points at overlapping records that decode to more than {} bytes \
             from a {qbin_len}-byte Quill.qbin",
            c.budget
        )));
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
    /// False when there is no binary data at all. That is reported once for the whole
    /// project, so records that cannot be read are then not reported one by one.
    has_data: bool,
    doc: Document,
    warnings: Vec<String>,
    /// Several layers can point at the same record. Each gets a drawing of its own,
    /// since edits change one drawing of one layer, but the strokes are read once and
    /// shared until one of them is edited.
    drawings_by_offset: HashMap<u64, Arc<Drawing>>,
    media_by_offset: HashMap<(MediaKind, u64), MediaId>,
    totals: DrawingReport,
    damaged_drawings: usize,
    unreadable_drawings: usize,
    /// Bytes of drawing and media data decoded so far, and how many may be.
    decoded: usize,
    budget: usize,
}

/// Bytes a decoded media item holds.
fn media_bytes(data: &MediaData) -> usize {
    match data {
        MediaData::Encoded { bytes, .. } => bytes.len(),
        MediaData::RawImage { pixels, .. } => pixels.len(),
        MediaData::RawAudio { samples, .. } => samples.len(),
        MediaData::Mesh(mesh) => mesh_bytes(mesh),
    }
}

fn mesh_bytes(mesh: &limner_core::model::MeshData) -> usize {
    (mesh.positions.len() + mesh.normals.len() + mesh.colors.len()) * 12 + mesh.indices.len() * 4
}

impl Converter<'_> {
    /// True once more has been decoded than the import allows. Nothing more is read
    /// then, and the import fails.
    fn over_budget(&self) -> bool {
        self.decoded > self.budget
    }

    fn spend(&mut self, bytes: usize) {
        self.decoded = self.decoded.saturating_add(bytes);
    }

    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    /// Reports a record that could not be read, unless the whole file is absent.
    fn warn_unreadable(&mut self, message: String) {
        if self.has_data {
            self.warn(message);
        }
    }

    fn read_settings(&mut self, sequence: &Value) {
        let settings = &mut self.doc.settings;
        if let Some(color) = sequence.get("BackgroundColor").and_then(json::floats::<3>) {
            settings.background = color.map(|v| v as f32);
        }
        if let Some(rate) = json::get_f64(sequence, "Framerate").and_then(frame_rate) {
            settings.rate = rate;
        }
        if let Some(start) = json::get_i64(sequence, "ExportStart") {
            settings.range_start = time::from_quill_ticks(start);
        }
        if let Some(end) = json::get_i64(sequence, "ExportEnd") {
            settings.range_end = time::from_quill_ticks(end);
        }
        if settings.range_end < settings.range_start {
            settings.range_end = settings.range_start;
            self.warnings.push(
                "the export range ended before it started; it now ends where it starts".to_owned(),
            );
        }
        if let Some([w, h]) = sequence.get("CameraResolution").and_then(json::floats::<2>) {
            if w >= 1.0 && h >= 1.0 {
                settings.camera_resolution = [w as u32, h as u32];
            }
        }
        if let Some(metadata) = sequence.get("Metadata") {
            settings.title = json::get_str(metadata, "Title")
                .filter(|t| *t != DEFAULT_TITLE)
                .unwrap_or_default()
                .to_owned();
            settings.description = json::get_str(metadata, "Description")
                .filter(|d| *d != DEFAULT_DESCRIPTION)
                .unwrap_or_default()
                .to_owned();
        }

        let posters = sequence
            .get("Gallery")
            .and_then(|g| g.get("Pictures"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        if posters > 0 {
            self.warn(format!(
                "{posters} gallery poster(s) saved for Quill's sharing feature were not imported"
            ));
        }

        // January 2017 files place the viewer with a matrix instead of a spawn area.
        if let Some(home) = sequence.get("HomePosition") {
            if !parse_xform(home).is_some_and(|x| is_nearly_identity(&x)) {
                self.warn(
                    "the home position of this 2017 file is not imported; viewers start at the origin"
                        .to_owned(),
                );
            }
        }
        // 1 is floor level in the Oculus SDK and the only value seen in files.
        if let Some(origin) = json::get_i64(sequence, "TrackingOrigin").filter(|o| *o != 1) {
            self.warn(format!(
                "the tracking origin of this 2017 file ({origin}) is not imported; floor level is assumed"
            ));
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
        self.note_unsupported(v, &name);
        self.read_children(v, root, &name);
    }

    /// Warns about per-layer features that have no counterpart in the document.
    fn note_unsupported(&mut self, v: &Value, path: &str) {
        let keep_alive = v
            .get("KeepAlive")
            .and_then(|k| json::get_str(k, "Type"))
            .filter(|t| !t.is_empty() && !t.eq_ignore_ascii_case("none"));
        if let Some(behaviour) = keep_alive {
            self.warn(format!(
                "{path}: the keep-alive behaviour \"{behaviour}\" is not imported"
            ));
        }
        if let Some(offset) = json::get_f64(v, "AnimOffset").filter(|o| *o != 0.0) {
            self.warn(format!(
                "{path}: the animation offset of this 2017 layer ({offset}) is not imported"
            ));
        }
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

        // A model imported with its hierarchy is a tree of nodes; a node can carry a
        // mesh and children at once, which no single layer here can.
        let has_children = implementation.get("Children").is_some();
        let mut node_mesh = None;
        let kind = match type_name.to_ascii_lowercase().as_str() {
            "group" => LayerKind::Group(Group {
                children: Vec::new(),
                sequence,
            }),
            "paint" => LayerKind::Paint(self.read_paint(implementation, &path)),
            "viewpoint" => LayerKind::Viewpoint(self.read_viewpoint(implementation, &path)),
            "camera" => LayerKind::Camera(Camera {
                fov: json::get_f64(implementation, "FOV")
                    .filter(|f| *f > 0.0 && *f < 180.0)
                    .unwrap_or(45.0) as f32,
            }),
            "picture" => LayerKind::Picture(self.read_picture(implementation, &name, &path)),
            "model" if has_children => {
                if implementation.get("DataFileOffset").is_some() {
                    node_mesh = Some(self.read_model(implementation, &name, &path));
                }
                LayerKind::Group(Group {
                    children: Vec::new(),
                    sequence,
                })
            }
            "model" => LayerKind::Model(self.read_model(implementation, &name, &path)),
            "sound" => LayerKind::Sound(self.read_sound(implementation, &name, &path)),
            _ if has_children => {
                self.warn(format!(
                    "{path}: unknown layer type \"{type_name}\" imported as a group"
                ));
                LayerKind::Group(Group {
                    children: Vec::new(),
                    sequence,
                })
            }
            _ => {
                self.warn(format!(
                    "{path}: layer of unknown type \"{type_name}\" was skipped"
                ));
                return;
            }
        };

        let mut layer = self.doc.new_layer(name.clone(), kind, 0);
        read_common(&mut layer, v);
        layer.anim = anim;
        let id = self
            .doc
            .insert_layer(layer, parent, usize::MAX)
            .expect("parent is a group");
        self.read_transforms(id, v, &path);
        self.note_unsupported(v, &path);
        if let Some(model) = node_mesh {
            let mesh_layer = self.doc.new_layer(name, LayerKind::Model(model), 0);
            self.doc
                .insert_layer(mesh_layer, id, usize::MAX)
                .expect("a model node with children is imported as a group");
        }
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
            Some(v) => match parse_xform(v) {
                Some(x) => {
                    if json::floats::<16>(v).is_some_and(|m| !is_similarity(&m)) {
                        self.warn(format!(
                            "{path}: the {what} matrix has uneven scale, shear or perspective; the nearest uniform transform is used"
                        ));
                    }
                    x
                }
                None => {
                    self.warn(format!("{path}: unreadable {what}; identity used"));
                    Xform::IDENTITY
                }
            },
        }
    }

    fn read_animation(&mut self, v: &Value, path: &str) -> (LayerAnim, Option<Sequence>) {
        let Some(animation) = v.get("Animation").filter(|a| a.is_object()) else {
            // Files from before Quill 2.0 have no timeline: the layer is simply present.
            return (LayerAnim::starting_at(0), None);
        };
        let Some(keys) = animation.get("Keys").filter(|k| k.is_object()) else {
            // Quill 1.4 to 1.8 kept clips as "Spans" (and frames in the animation block).
            // No file or reader documents that layout, so the layer is simply present.
            if animation.get("Spans").is_some() || animation.get("Frames").is_some() {
                self.warn(format!(
                    "{path}: the clips of this Quill 1.x layer are in a layout this importer does not know; the layer is shown for the whole timeline"
                ));
            }
            return (LayerAnim::starting_at(0), read_sequence(animation));
        };
        let mut anim = LayerAnim::default();
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

        (anim, read_sequence(animation))
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
            .and_then(frame_rate)
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
        if let Some(shared) = self.drawings_by_offset.get(&offset) {
            let shared = shared.clone();
            return self.doc.add_shared_drawing(shared);
        }
        if self.over_budget() {
            return self.doc.add_drawing(Drawing::new());
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
        self.spend(
            drawing.point_count() * std::mem::size_of::<limner_core::model::Point>()
                + drawing.stroke_count() * crate::qbin::STROKE_HEADER_LEN,
        );
        let drawing = Arc::new(drawing);
        self.drawings_by_offset.insert(offset, drawing.clone());
        self.doc.add_shared_drawing(drawing)
    }

    fn summarise_drawing_problems(&mut self) {
        if self.unreadable_drawings > 0 && self.has_data {
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

    fn read_viewpoint(&mut self, implementation: &Value, path: &str) -> Viewpoint {
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
            .and_then(|offset| self.picture_at(offset, "thumbnail", path, "spawn area thumbnail"));
        viewpoint
    }

    fn read_picture(&mut self, implementation: &Value, layer_name: &str, path: &str) -> Picture {
        let source_path = json::get_str(implementation, "ImportFilePath")
            .unwrap_or_default()
            .to_owned();
        let name = file_name(&source_path).unwrap_or(layer_name).to_owned();
        let media = match implementation
            .get("DataFileOffset")
            .and_then(json::file_offset)
        {
            Some(offset) => self.picture_at(offset, &name, path, "picture"),
            None => {
                self.warn(format!(
                    "{path}: the picture has no readable file offset and is empty"
                ));
                None
            }
        };
        let size = media
            .and_then(|id| self.doc.media(id))
            .and_then(|m| match &m.data {
                MediaData::RawImage { width, height, .. } => Some((*width, *height)),
                _ => None,
            });
        let type_name = json::get_str(implementation, "Type").unwrap_or("2D");
        let projection = self.picture_projection(type_name, size, path);
        Picture {
            media,
            projection,
            viewer_locked: json::get_bool(implementation, "ViewerLocked").unwrap_or(false),
            source_path,
        }
    }

    /// Maps a picture `Type` string to a projection. "2D", "2DDepth" and the two
    /// equirectangular names come from the community readers; Quill also has two
    /// cubemap layouts whose names are in no reader, so those are matched loosely.
    fn picture_projection(
        &mut self,
        type_name: &str,
        size: Option<(u32, u32)>,
        path: &str,
    ) -> Projection {
        let key: String = type_name
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect();
        match key.as_str() {
            "2d" | "" => return Projection::Flat,
            "360equirectangularmono" => return Projection::Equirect,
            "360equirectangularstereo" => return Projection::EquirectStereo,
            "2ddepth" => {
                self.warn(format!(
                    "{path}: picture type \"{type_name}\" is shown as a flat image; its depth is not imported"
                ));
                return Projection::Flat;
            }
            _ => {}
        }
        let (projection, shown_as) = if key.contains("equirect") && key.contains("stereo") {
            (Projection::EquirectStereo, "a stereo 360 image")
        } else if key.contains("equirect") {
            (Projection::Equirect, "a 360 image")
        } else if key.contains("cube") {
            // A cross is four faces wide and three high; a strip is one face wide.
            let cross = key.contains("cross")
                || (!key.contains("strip") && size.is_some_and(|(w, h)| w > h));
            if cross {
                (Projection::CubemapCross, "a cubemap cross")
            } else {
                (Projection::CubemapStrip, "a cubemap strip")
            }
        } else {
            (Projection::Flat, "a flat image")
        };
        self.warn(format!(
            "{path}: picture type \"{type_name}\" is not a name this importer knows; it is shown as {shown_as}"
        ));
        projection
    }

    /// Reads the picture record at `offset`, once per record.
    fn picture_at(&mut self, offset: u64, name: &str, path: &str, what: &str) -> Option<MediaId> {
        if let Some(id) = self.media_by_offset.get(&(MediaKind::Image, offset)) {
            return Some(*id);
        }
        if self.over_budget() {
            return None;
        }
        match self.qbin.read_picture(offset) {
            Ok((data, notes)) => {
                self.spend(media_bytes(&data));
                if let Some(image_type) = notes.unexpected_type {
                    self.warn(format!(
                        "{path}: the {what} is stored as image type {image_type}, not a flat image; it was read as one grid of pixels"
                    ));
                }
                let id = self.doc.add_media(name, MediaKind::Image, data);
                self.media_by_offset.insert((MediaKind::Image, offset), id);
                Some(id)
            }
            Err(reason) => {
                self.warn_unreadable(format!("{path}: the {what} could not be read ({reason})"));
                None
            }
        }
    }

    fn read_model(&mut self, implementation: &Value, layer_name: &str, path: &str) -> Model {
        let source_path = json::get_str(implementation, "ImportFilePath")
            .unwrap_or_default()
            .to_owned();
        let name = file_name(&source_path).unwrap_or(layer_name).to_owned();
        let media = match implementation
            .get("DataFileOffset")
            .and_then(json::file_offset)
        {
            Some(offset) => self.mesh_at(offset, &name, path),
            None => {
                self.warn(format!(
                    "{path}: the model has no readable file offset; the layer is kept empty"
                ));
                None
            }
        };
        let shading = match json::get_str(implementation, "ShadingModel").unwrap_or("Unlit") {
            s if s.eq_ignore_ascii_case("Smooth") => Shading::Lit,
            s if s.eq_ignore_ascii_case("Unlit") => Shading::Unlit,
            other => {
                self.warn(format!(
                    "{path}: unknown shading model \"{other}\" read as unlit"
                ));
                Shading::Unlit
            }
        };
        Model {
            media,
            shading,
            wireframe: json::get_bool(implementation, "RenderWireFrame").unwrap_or(false),
            source_path,
        }
    }

    /// Reads the mesh record at `offset`, once per record.
    fn mesh_at(&mut self, offset: u64, name: &str, path: &str) -> Option<MediaId> {
        if let Some(id) = self.media_by_offset.get(&(MediaKind::Mesh, offset)) {
            return Some(*id);
        }
        if self.over_budget() {
            return None;
        }
        let (mesh, notes) = match self.qbin.read_mesh(offset) {
            Ok(read) => read,
            Err(reason) => {
                self.warn_unreadable(format!(
                    "{path}: the model's geometry could not be read ({reason}); the layer is kept empty"
                ));
                return None;
            }
        };
        if notes.points_only {
            self.warn(format!(
                "{path}: the model is a point cloud; its {} point(s) are kept but nothing is drawn",
                mesh.positions.len()
            ));
        }
        if notes.damaged_vertices > 0 {
            self.warn(format!(
                "{path}: {} vertex position(s) of the model were not finite; the faces using them were skipped",
                notes.damaged_vertices
            ));
        }
        if notes.skipped_faces > 0 {
            self.warn(format!(
                "{path}: {} face(s) of the model were skipped (not a triangle or quad, or pointing at a missing vertex)",
                notes.skipped_faces
            ));
        }
        if notes.repaired_attributes > 0 {
            self.warn(format!(
                "{path}: {} non-finite normal(s) or colour(s) of the model were replaced",
                notes.repaired_attributes
            ));
        }
        if notes.unexpected_layout {
            self.warn(format!(
                "{path}: the model's vertex attributes are not the position, colour, normal set Quill writes; normals and colours may be missing or swapped"
            ));
        }
        if notes.extra_streams {
            self.warn(format!(
                "{path}: the model has more than one vertex stream; only the first was imported"
            ));
        }
        self.spend(mesh_bytes(&mesh));
        let id = self
            .doc
            .add_media(name, MediaKind::Mesh, MediaData::Mesh(Arc::new(mesh)));
        self.media_by_offset.insert((MediaKind::Mesh, offset), id);
        Some(id)
    }

    fn read_sound(&mut self, implementation: &Value, layer_name: &str, path: &str) -> Sound {
        let source_path = json::get_str(implementation, "ImportFilePath")
            .unwrap_or_default()
            .to_owned();
        let name = file_name(&source_path).unwrap_or(layer_name).to_owned();
        let media = match implementation
            .get("DataFileOffset")
            .and_then(json::file_offset)
        {
            Some(offset) => self.sound_at(offset, &name, path),
            None => {
                self.warn(format!(
                    "{path}: the sound has no readable file offset; the layer is kept silent"
                ));
                None
            }
        };

        let type_name = json::get_str(implementation, "Type").unwrap_or("Flat");
        let kind = match type_name.to_ascii_lowercase().as_str() {
            "flat" => SoundKind::Flat,
            "positional" => SoundKind::Positional,
            "ambisonic" => SoundKind::Ambisonic,
            _ => {
                self.warn(format!(
                    "{path}: unknown sound type \"{type_name}\" read as flat"
                ));
                SoundKind::Flat
            }
        };

        let mut attenuation = Attenuation::default();
        if let Some(a) = implementation.get("Attenuation").filter(|a| a.is_object()) {
            let mode = json::get_str(a, "Mode").unwrap_or("None");
            attenuation.mode = match mode.to_ascii_lowercase().as_str() {
                "none" => AttenuationMode::None,
                "linear" => AttenuationMode::Linear,
                "logarithmic" | "logarithm" | "log" => AttenuationMode::Logarithmic,
                _ => {
                    self.warn(format!(
                        "{path}: unknown attenuation mode \"{mode}\"; the sound does not fade with distance"
                    ));
                    AttenuationMode::None
                }
            };
            let distance = |key: &str, fallback: f32| {
                json::get_f64(a, key)
                    .filter(|v| *v >= 0.0)
                    .map_or(fallback, |v| v as f32)
            };
            attenuation.min = distance("Minimum", attenuation.min);
            attenuation.max = distance("Maximum", attenuation.max);
        }

        let modifier = self.read_sound_modifier(implementation.get("Modifiers"), path);
        Sound {
            media,
            kind,
            gain: json::get_f64(implementation, "Gain")
                .filter(|g| *g >= 0.0)
                .map_or(1.0, |g| g as f32),
            looped: json::get_bool(implementation, "Loop").unwrap_or(false),
            attenuation,
            modifier,
            source_path,
        }
    }

    /// Reads the directional shaping of a positional sound.
    ///
    /// Only the modifier type names are public (`DirectionalCone`, and `DirectionalFrus`
    /// spelled `DirectionFrus` by one reader). The parameter names appear in no reader
    /// and no file, so they are matched by what Quill's runtime calls them (inner angle,
    /// band, attenuation outside) and every match is reported.
    fn read_sound_modifier(&mut self, modifiers: Option<&Value>, path: &str) -> SoundModifier {
        let list: Vec<&Value> = match modifiers {
            Some(Value::Array(items)) => items.iter().collect(),
            Some(single @ Value::Object(_)) => vec![single],
            _ => Vec::new(),
        };
        let active: Vec<(&Value, &str)> = list
            .into_iter()
            .filter_map(|m| {
                let kind = json::get_str(m, "Type")?;
                (!kind.is_empty() && !kind.eq_ignore_ascii_case("none")).then_some((m, kind))
            })
            .collect();
        if active.len() > 1 {
            self.warn(format!(
                "{path}: the sound has {} directional modifiers; only the first is imported",
                active.len()
            ));
        }
        let Some((value, type_name)) = active.into_iter().next() else {
            return SoundModifier::None;
        };
        let kind = type_name.to_ascii_lowercase();

        let numbers = modifier_numbers(value);
        let find = |wanted: &dyn Fn(&str) -> bool| {
            numbers.iter().find(|(key, _)| wanted(key)).map(|(_, v)| *v)
        };
        let band = find(&|k| k.contains("band"));
        let outside = find(&|k| k.contains("atten"))
            .or_else(|| find(&|k| k.contains("out") && !k.contains("angle")));
        let (shape, angles): (&str, Option<Vec<f64>>) = if kind.contains("cone") {
            let inner = find(&|k| k.contains("anglein") || k.contains("inner"));
            ("cone", inner.map(|a| vec![a]))
        } else if kind.contains("frus") {
            let axis = |axis: char, word: &str| {
                find(&|k| {
                    k.contains(word)
                        || ((k.contains("anglein") || k.contains("inner")) && k.ends_with(axis))
                })
            };
            let both = axis('x', "horizontal").zip(axis('y', "vertical"));
            ("frustum", both.map(|(h, v)| vec![h, v]))
        } else {
            self.warn(format!(
                "{path}: unknown sound modifier \"{type_name}\" was not imported"
            ));
            return SoundModifier::None;
        };
        let Some(angles) = angles else {
            self.warn(format!(
                "{path}: the sound's directional {shape} has no parameters this importer recognises; the sound plays in all directions"
            ));
            return SoundModifier::None;
        };

        // Quill's runtime holds these angles in radians and shows degrees; a value past
        // a full turn can only be degrees.
        let band = band.unwrap_or(0.0);
        let degrees = angles
            .iter()
            .chain([&band])
            .any(|a| a.abs() > std::f64::consts::TAU);
        let angle = |a: f64| (if degrees { a.to_radians() } else { a }) as f32;
        let outside = outside.unwrap_or(0.0) as f32;
        self.warn(format!(
            "{path}: the sound's directional {shape} was read from parameter names no Quill file has confirmed (angles taken as {}); check it",
            if degrees { "degrees" } else { "radians" }
        ));
        match angles.as_slice() {
            [inner] => SoundModifier::Cone {
                inner: angle(*inner),
                band: angle(band),
                outside,
            },
            [inner_h, inner_v] => SoundModifier::Frustum {
                inner_h: angle(*inner_h),
                inner_v: angle(*inner_v),
                band: angle(band),
                outside,
            },
            _ => SoundModifier::None,
        }
    }

    /// Reads the sound record at `offset`, once per record.
    fn sound_at(&mut self, offset: u64, name: &str, path: &str) -> Option<MediaId> {
        if let Some(id) = self.media_by_offset.get(&(MediaKind::Audio, offset)) {
            return Some(*id);
        }
        if self.over_budget() {
            return None;
        }
        match self.qbin.read_sound(offset) {
            Ok((data, _layout)) => {
                self.spend(media_bytes(&data));
                let id = self.doc.add_media(name, MediaKind::Audio, data);
                self.media_by_offset.insert((MediaKind::Audio, offset), id);
                Some(id)
            }
            Err(reason) => {
                self.warn_unreadable(format!(
                    "{path}: the sound's audio could not be read ({reason}); the layer is kept silent"
                ));
                None
            }
        }
    }
}

fn read_common(layer: &mut Layer, v: &Value) {
    layer.visible = json::get_bool(v, "Visible").unwrap_or(true);
    layer.locked = json::get_bool(v, "Locked").unwrap_or(false);
    layer.collapsed = json::get_bool(v, "Collapsed").unwrap_or(false);
    layer.show_bounds = json::get_bool(v, "BBoxVisible").unwrap_or(false);
    layer.opacity = json::get_f64(v, "Opacity").map_or(1.0, |o| (o as f32).clamp(0.0, 1.0));
}

/// The numeric fields of a sound modifier, keyed by lower-case name without
/// punctuation. Parameters nested one level down (for example under the modifier's
/// own name) are included.
fn modifier_numbers(modifier: &Value) -> BTreeMap<String, f64> {
    fn collect(value: &Value, depth: usize, out: &mut BTreeMap<String, f64>) {
        let Some(object) = value.as_object() else {
            return;
        };
        for (key, field) in object {
            if field.is_object() && depth == 0 {
                collect(field, 1, out);
            } else if let Some(number) = json::as_f64(field) {
                let key: String = key
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .map(|c| c.to_ascii_lowercase())
                    .collect();
                out.entry(key).or_insert(number);
            }
        }
    }
    let mut out = BTreeMap::new();
    collect(modifier, 0, &mut out);
    out
}

/// A usable frame rate, or `None` for values that cannot time frames.
fn frame_rate(fps: f64) -> Option<FrameRate> {
    (fps > 0.0)
        .then(|| FrameRate::from_f64(fps))
        .filter(|rate| rate.is_valid())
}

/// The nested clock of a group whose animation block has the timeline flag.
fn read_sequence(animation: &Value) -> Option<Sequence> {
    json::get_bool(animation, "Timeline")
        .unwrap_or(false)
        .then(|| Sequence {
            loop_duration: time::from_quill_ticks(
                json::get_i64(animation, "Duration").unwrap_or(0).max(0),
            ),
            max_repeat: json::get_i64(animation, "MaxRepeatCount")
                .unwrap_or(0)
                .clamp(0, u32::MAX as i64) as u32,
        })
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

/// True when a row-major 4x4 matrix is a rotation with one uniform scale, at most one
/// mirror and a translation: everything a layer transform can hold. Old files round
/// to six decimals, hence the loose tolerance.
fn is_similarity(m: &[f64; 16]) -> bool {
    const TOLERANCE: f64 = 1e-3;
    let columns = [
        DVec3::new(m[0], m[4], m[8]),
        DVec3::new(m[1], m[5], m[9]),
        DVec3::new(m[2], m[6], m[10]),
    ];
    let lengths = columns.map(DVec3::length);
    let scale = lengths.iter().sum::<f64>() / 3.0;
    if scale <= 0.0 {
        return false;
    }
    let even = lengths
        .iter()
        .all(|l| (l - scale).abs() <= TOLERANCE * scale);
    let square = [(0, 1), (0, 2), (1, 2)]
        .iter()
        .all(|(a, b)| columns[*a].dot(columns[*b]).abs() <= TOLERANCE * scale * scale);
    let affine = m[12].abs() <= TOLERANCE
        && m[13].abs() <= TOLERANCE
        && m[14].abs() <= TOLERANCE
        && (m[15] - 1.0).abs() <= TOLERANCE;
    even && square && affine
}

fn is_nearly_identity(x: &Xform) -> bool {
    const TOLERANCE: f64 = 1e-6;
    !x.flip.is_mirrored()
        && (x.scale - 1.0).abs() <= TOLERANCE
        && x.translation.length() <= TOLERANCE
        && x.rotation.dot(DQuat::IDENTITY).abs() >= 1.0 - TOLERANCE
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

    /// A project with one record of every kind, in the modern schema.
    fn everything() -> (Value, Vec<u8>) {
        use crate::testing::*;
        let mut qbin = QbinBuilder::default();
        let thumbnail = qbin.picture_rgb(2, 2, &[9; 12]);
        let drawing = qbin.drawing(&[
            StrokeSpec::new(0, 2, tube(0.0, 5)),
            StrokeSpec::new(1, 1, tube(1.0, 3)),
        ]);
        let pixels = qbin.picture_rgba(2, 1, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let mesh = qbin.mesh(&MeshSpec {
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            colors: Some(vec![[1.0; 3]; 4]),
            normals: Some(vec![[0.0, 0.0, 1.0]; 4]),
            faces: vec![vec![0, 1, 2, 3], vec![0, 1, 2]],
        });
        let wave = qbin.sound_wave(2, 16, 44_100, &[0; 16]);
        let file = qbin.sound_file(&wav_file(1, 8, 8_000, &[1, 2, 3]));

        let spawn = layer(
            "InitialSpawnArea",
            "Viewpoint",
            json!({"Version": 1, "Color": [0.4, 0.3, 0.5], "Sphere": [0.0, 1.0, 0.0, 2.0],
                   "DataFileOffset": thumbnail, "TypeStr": "FloorLevel"}),
        );
        let mut positional = sound("Voice", &wave, "voice.wav");
        positional["Implementation"]["Modifiers"] =
            json!([{"Type": "DirectionalCone", "AngleIn": 0.5, "AngleBand": 0.1, "AttenOut": 0.0}]);
        let mut body = group(
            "Body",
            vec![
                paint("Anim", &[&drawing, &drawing], &[0, 1, 1]),
                picture("Ref", "360 Equirectangular Stereo", &pixels, "C:/ref.png"),
                model("Rock", &mesh, "C:/rock.obj"),
                positional,
                sound("Music", &file, "music.wav"),
                layer("Cam", "Camera", json!({"FOV": 40})),
            ],
        );
        body["Animation"]["Keys"]["Transform"] = json!([
            {"Time": "0", "Value": identity(), "Interpolation": "Linear"},
            {"Time": "525", "Value": xform([0.0, 1.0, 0.0, 0.0], 2.0, "X", [1.0, 2.0, 3.0]), "Interpolation": "None"}
        ]);
        body["Animation"]["Keys"]["Opacity"] =
            json!([{"Time": "0", "Value": 0.5, "Interpolation": "EaseOut"}]);
        let mut quill = project(vec![spawn, body]);
        quill["Sequence"]["RootLayer"]["Animation"]["Keys"]["Action"] =
            json!([{"Time": "1050", "Value": "Stop", "Interpolation": "None"}]);
        (quill, qbin.bytes)
    }

    /// A small deterministic generator, so a failure can be reproduced from its seed.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: usize) -> usize {
            (self.next() % n.max(1) as u64) as usize
        }
    }

    #[test]
    fn the_test_project_is_clean_when_whole() {
        let (quill, qbin) = everything();
        let converted = convert(&quill, Some(&qbin)).unwrap();
        // The cone is the one thing in it that cannot be read with certainty.
        assert_eq!(converted.warnings.len(), 1, "{:?}", converted.warnings);
        assert!(converted.warnings[0].contains("directional cone"));
        let doc = &converted.document;
        assert_eq!(doc.layer_count(), 9);
        assert_eq!(doc.media_items().count(), 5);
        // The layer lists its one record twice: two drawings sharing two strokes.
        assert_eq!(doc.drawing_count(), 2);
        assert_eq!(doc.stroke_count(), 4);
    }

    #[test]
    fn a_binary_file_cut_anywhere_still_imports() {
        let (quill, qbin) = everything();
        for len in 0..=qbin.len() {
            let converted = convert(&quill, Some(&qbin[..len]))
                .unwrap_or_else(|e| panic!("cut at {len} bytes: {e}"));
            converted
                .document
                .validate()
                .unwrap_or_else(|e| panic!("cut at {len} bytes: {e}"));
            assert_eq!(converted.document.layer_count(), 9, "cut at {len} bytes");
            assert_eq!(
                converted.warnings.len() > 1,
                len < qbin.len(),
                "a cut at {len} of {} bytes must be reported: {:?}",
                qbin.len(),
                converted.warnings
            );
        }
    }

    #[test]
    fn corrupted_binary_data_never_panics() {
        let (quill, qbin) = everything();
        let mut rng = Rng(0x2545_f491_4f6c_dd1d);
        for round in 0..4000 {
            let mut bytes = qbin.clone();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(bytes.len());
                bytes[at] = match rng.below(4) {
                    0 => 0,
                    1 => 0xff,
                    2 => 0x7f,
                    _ => rng.next() as u8,
                };
            }
            let converted =
                convert(&quill, Some(&bytes)).unwrap_or_else(|e| panic!("round {round}: {e}"));
            converted
                .document
                .validate()
                .unwrap_or_else(|e| panic!("round {round}: {e}"));
        }
    }

    /// Replaces one value somewhere in the tree with something of another shape.
    fn mutate(value: &mut Value, rng: &mut Rng) {
        let children = match value {
            Value::Object(map) => map.len(),
            Value::Array(items) => items.len(),
            _ => 0,
        };
        if children > 0 && rng.below(8) != 0 {
            let pick = rng.below(children);
            match value {
                Value::Object(map) => mutate(map.values_mut().nth(pick).expect("in range"), rng),
                Value::Array(items) => mutate(&mut items[pick], rng),
                _ => unreachable!(),
            }
            return;
        }
        *value = match rng.below(12) {
            0 => Value::Null,
            1 => json!(true),
            2 => json!(-1),
            3 => json!(1e300),
            4 => json!("nan"),
            5 => json!("ffffffffffffffff"),
            6 => json!(""),
            7 => json!([]),
            8 => json!({}),
            9 => json!([1, "two", null]),
            10 => json!(u64::MAX),
            _ => json!("Model"),
        };
    }

    #[test]
    fn mangled_json_is_an_error_or_an_import_never_a_panic() {
        let (quill, qbin) = everything();
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let mut imported = 0;
        for round in 0..6000 {
            let mut mangled = quill.clone();
            for _ in 0..1 + rng.below(3) {
                mutate(&mut mangled, &mut rng);
            }
            if let Ok(converted) = convert(&mangled, Some(&qbin)) {
                imported += 1;
                converted
                    .document
                    .validate()
                    .unwrap_or_else(|e| panic!("round {round}: {e}"));
            }
        }
        assert!(imported > 3000, "only {imported} mangled projects imported");
    }

    #[test]
    fn similarity_matrices_are_told_from_the_rest() {
        let floats = |v: Value| json::floats::<16>(&v).unwrap();
        // Six-decimal rounding, as old files have it, is not an uneven scale.
        assert!(is_similarity(&floats(json!([
            5.002647, 0.0, 0.0, 1.0, 0.0, 5.002635, 0.0, 2.0, 0.0, 0.0, 5.002636, 3.0, 0.0, 0.0,
            0.0, 1.0
        ]))));
        assert!(is_similarity(&floats(json!([
            -1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0
        ]))));
        for bad in [
            json!([2.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
            json!([1.0, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
            json!([1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.5, 1.0]),
            json!([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
        ] {
            assert!(!is_similarity(&floats(bad)));
        }
        assert!(is_nearly_identity(
            &parse_xform(&json!([
                0.9999999999,
                0.0,
                0.0,
                0.0,
                0.0,
                0.9999999999,
                0.0,
                0.0,
                0.0,
                0.0,
                0.9999999999,
                0.0,
                0.0,
                0.0,
                0.0,
                1.0
            ]))
            .unwrap()
        ));
        assert!(!is_nearly_identity(&Xform::from_translation(DVec3::X)));
    }

    #[test]
    fn modifier_parameters_are_gathered_one_level_deep() {
        let numbers = modifier_numbers(&json!({
            "Type": "DirectionalCone", "Angle In": "0.5", "Enabled": true,
            "Params": {"Angle_Band": 0.25, "Deeper": {"Ignored": 1.0}}
        }));
        assert_eq!(
            numbers.into_iter().collect::<Vec<_>>(),
            [("angleband".to_owned(), 0.25), ("anglein".to_owned(), 0.5)]
        );
        assert!(modifier_numbers(&json!("cone")).is_empty());
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

    #[test]
    fn layers_that_share_a_record_get_drawings_of_their_own() {
        use crate::testing::*;
        let mut qbin = QbinBuilder::default();
        let record = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 5))]);
        let quill = project(vec![
            paint("A", &[&record], &[0]),
            paint("B", &[&record, &record], &[0, 1]),
        ]);
        let mut doc = convert(&quill, Some(&qbin.bytes)).unwrap().document;
        let drawings = |doc: &Document, name: &str| {
            doc.layer(doc.find_by_path(&format!("Root/{name}")).unwrap())
                .unwrap()
                .as_paint()
                .unwrap()
                .drawings
                .clone()
        };
        let (a, b) = (drawings(&doc, "A"), drawings(&doc, "B"));
        let all = [a[0], b[0], b[1]];
        assert_eq!(
            all.iter().collect::<std::collections::BTreeSet<_>>().len(),
            3,
            "every use has its own id"
        );
        for id in &all[1..] {
            assert!(Arc::ptr_eq(
                doc.drawing(all[0]).unwrap(),
                doc.drawing(*id).unwrap()
            ));
        }
        let layer_a = doc.find_by_path("Root/A").unwrap();
        assert_eq!(doc.layer_of_drawing(a[0]), Some(layer_a));

        // Painting on one layer leaves the other as it was.
        doc.drawing_mut(a[0]).unwrap().retain(|_| false);
        assert!(doc.drawing(a[0]).unwrap().is_empty());
        assert_eq!(doc.drawing(b[0]).unwrap().stroke_count(), 1);
    }

    #[test]
    fn records_that_overlap_cannot_multiply_the_file_in_memory() {
        use crate::testing::*;
        // Strokes laid end to end, each ending in a width whose bits read as a stroke
        // count of 300. Four bytes before any stroke therefore parse as a drawing that
        // runs from that stroke to the end, so each offset decodes most of the file.
        const STROKES: usize = 300;
        let mut qbin = QbinBuilder::default();
        let mut points = tube(0.0, 10);
        points[9].width = f32::from_bits(STROKES as u32);
        let strokes: Vec<StrokeSpec> = (0..STROKES)
            .map(|i| StrokeSpec::new(i as u32, 2, points.clone()))
            .collect();
        let first = u64::from_str_radix(&qbin.drawing(&strokes), 16).unwrap() as usize;
        let stroke_len = 40 + 10 * std::mem::size_of::<limner_core::model::Point>();
        let layers: Vec<Value> = (0..STROKES)
            .map(|i| {
                let offset = format!("{:016x}", first + i * stroke_len);
                paint(&format!("P{i}"), &[&offset], &[0])
            })
            .collect();
        let quill = project(layers);
        let error = convert(&quill, Some(&qbin.bytes)).err().expect("refused");
        assert!(error.to_string().contains("overlapping records"), "{error}");

        // The same strokes pointed at once each import normally.
        let one = project(vec![paint("P", &[&format!("{first:016x}")], &[0])]);
        let converted = convert(&one, Some(&qbin.bytes)).unwrap();
        assert_eq!(converted.document.stroke_count(), STROKES);
    }

    #[test]
    fn an_export_range_that_ends_before_it_starts_is_repaired() {
        use crate::testing::*;
        let mut quill = project(vec![]);
        quill["Sequence"]["ExportStart"] = json!(5000);
        quill["Sequence"]["ExportEnd"] = json!(100);
        let converted = convert(&quill, Some(&[0u8; 8][..])).unwrap();
        let settings = &converted.document.settings;
        assert_eq!(settings.range_start, settings.range_end);
        assert_eq!(settings.range_start, time::from_quill_ticks(5000));
        assert!(
            converted
                .warnings
                .iter()
                .any(|w| w.contains("export range")),
            "{:?}",
            converted.warnings
        );
    }
}
