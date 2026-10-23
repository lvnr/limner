//! `LimnerScene`: the node that owns an editing session and shows it.
//!
//! Each frame it evaluates the document and reconciles its child mesh instances with
//! the result. It holds no product logic: what is visible, where, and what an edit
//! does are all decided in the core.

mod export;
mod gizmos;
mod import;
mod lights;
mod models;
mod nodes;
mod overlays;
mod pictures;
mod sounds;
mod strokes;

use crate::convert::{
    color_from_linear, dictionary_from_json, dvec3, json_from_variant, transform3d,
    variant_from_json,
};
use crate::meshes::MeshCache;
use godot::classes::image::Format;
use godot::classes::{Image, Material};
use godot::prelude::*;
use limner::core::dither;
use limner::core::eval::Content;
use limner::core::glam::{DQuat, DVec3};
use limner::core::model::{Document, LayerId, Shading};
use limner::core::session::{
    Feedback, GimbalHandle, HandInput, InputFrame, PlaybackEvent, Session, Subject, ARROW_END,
    ARROW_START, CENTER_RADIUS, DEFAULT_ZOOM_LIMITS, PLANE_FAR, PLANE_NEAR,
};
use limner::core::tess::SurfaceKind;
use limner::core::time;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use strokes::{container, set_visible, sync_layer, LayerInstances, Stats};

#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct LimnerScene {
    base: Base<Node3D>,
    session: Session,
    /// Revisions the interface was last told about.
    announced: (u64, u64),
    /// Controller input gathered for the next `input_commit`.
    input: InputFrame,
    source_path: String,
    source_kind: String,
    warnings: Vec<String>,
    cache: MeshCache,
    layers: HashMap<LayerId, LayerInstances>,
    container: Option<Gd<Node3D>>,
    /// Nodes for the layers that are not paint.
    nodes: nodes::LayerNodes,
    /// Onion skins and the highlight on selected strokes.
    overlays: overlays::Overlays,
    /// Indexed by `SurfaceKind`.
    materials: [Option<Gd<Material>>; 4],
    /// The same four for paint layers that scene lights shade.
    lit_materials: [Option<Gd<Material>>; 4],
    stats: Stats,
}

#[godot_api]
impl INode3D for LimnerScene {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            base,
            session: Session::new(Document::new()),
            announced: (0, 0),
            input: InputFrame::default(),
            source_path: String::new(),
            source_kind: String::new(),
            warnings: Vec::new(),
            cache: MeshCache::default(),
            layers: HashMap::new(),
            container: None,
            nodes: nodes::LayerNodes::default(),
            overlays: overlays::Overlays::default(),
            materials: [None, None, None, None],
            lit_materials: [None, None, None, None],
            stats: Stats::default(),
        }
    }

    fn process(&mut self, delta: f64) {
        if let Some(event) = self.session.advance(delta) {
            let name = match event {
                PlaybackEvent::ReachedStop => "stop",
                PlaybackEvent::Looped => "loop",
                PlaybackEvent::ReachedEnd => "end",
            };
            self.signals().playback_event().emit(name);
        }
        self.sync();
        self.announce_changes();
    }
}

#[godot_api]
impl LimnerScene {
    /// The document changed: layers, strokes, keys or settings.
    #[signal]
    fn document_changed();

    /// Something else an interface shows changed: the tool, the selection, playback.
    #[signal]
    fn state_changed();

    /// Another document was opened or a new one started.
    #[signal]
    fn document_replaced();

    /// Playback reached a "stop", wrapped around ("loop") or reached the "end" of the range.
    #[signal]
    fn playback_event(event: GString);

    // ----- documents -----

    /// Starts a new, empty document.
    #[func]
    fn new_document(&mut self) {
        self.new_document_with_palette(VarArray::new());
    }

    /// Starts a new, empty document whose swatches are `palette`: linear `[r, g, b]`
    /// lists, as many as there are (the rest keep their built-in colour).
    #[func]
    fn new_document_with_palette(&mut self, palette: VarArray) {
        self.clear_instances();
        self.source_path.clear();
        self.source_kind.clear();
        self.warnings.clear();
        let mut document = Document::new();
        let colors = json_from_variant(&palette.to_variant());
        if let Ok(colors) = serde_json::from_value::<Vec<[f32; 3]>>(colors) {
            for (swatch, color) in document.settings.palette.iter_mut().zip(colors) {
                if color.iter().all(|v| v.is_finite() && *v >= 0.0) {
                    *swatch = color;
                }
            }
        }
        self.session.replace_document(document);
        self.signals().document_replaced().emit();
    }

    /// Brings a picture (`.png`, `.jpg`), a sound (`.wav`, `.ogg`, `.mp3`) or a model
    /// (`.obj`, `.glb`, `.gltf`, `.fbx`) into the document as a new layer, or another
    /// project (a `.limn` or Quill folder, or a `.limnz` file) as a folded group of
    /// everything in it, as one undo step. Returns `{ok, error, layer, warnings}`; the warnings say what the
    /// file had that was not brought in.
    #[func]
    fn import_file(&mut self, path: GString) -> VarDictionary {
        match import::import(&mut self.session, &path.to_string()) {
            Ok((layer, warnings)) => {
                let warnings: PackedStringArray = warnings.iter().map(GString::from).collect();
                vdict! { "ok" => true, "error" => "", "layer" => layer.0 as i64, "warnings" => &warnings }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error, "layer" => -1, "warnings" => &PackedStringArray::new() }
            }
        }
    }

    // ----- commands and queries -----

    /// Runs a command, given as a dictionary with a `cmd` entry. Returns
    /// `{ok, error, result}`. This is how every edit reaches the document.
    #[func]
    fn exec(&mut self, command: VarDictionary) -> VarDictionary {
        match self
            .session
            .exec_json(&json_from_variant(&command.to_variant()))
        {
            Ok(result) => {
                vdict! { "ok" => true, "error" => "", "result" => &variant_from_json(&result) }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error.to_string(), "result" => &Variant::nil() }
            }
        }
    }

    /// Starts an edit that spans several commands, such as dragging a slider.
    /// Everything up to `commit_edit` becomes one undo step.
    #[func]
    fn begin_edit(&mut self, label: GString) {
        self.session.begin_transaction(label.to_string());
    }

    #[func]
    fn commit_edit(&mut self) {
        self.session.commit_transaction();
    }

    #[func]
    fn cancel_edit(&mut self) {
        self.session.cancel_transaction();
    }

    /// Revisions, undo state, playhead and current tool. Cheap enough for every frame.
    #[func]
    fn status(&self) -> VarDictionary {
        dictionary_from_json(&self.session.status())
    }

    /// One row per layer in list order.
    #[func]
    fn layer_rows(&self) -> VarArray {
        variant_from_json(&self.session.layer_rows())
            .try_to()
            .unwrap_or_default()
    }

    #[func]
    fn layer_details(&self, layer: i64) -> VarDictionary {
        dictionary_from_json(&self.session.layer_details(LayerId(layer.max(0) as u64)))
    }

    /// One row per layer in list order, with what the timeline draws: clips, keys,
    /// frames and where the layer's own time starts.
    #[func]
    fn timeline_rows(&self) -> VarArray {
        variant_from_json(&self.session.timeline_rows())
            .try_to()
            .unwrap_or_default()
    }

    /// The part of `timeline_rows` that moves with the playhead: for every layer in
    /// list order its `id`, `origin`, `time`, `local_time` and the `frame` a paint
    /// layer shows. Cheap enough to ask for every frame.
    #[func]
    fn timeline_clock(&self) -> VarArray {
        variant_from_json(&self.session.timeline_clock())
            .try_to()
            .unwrap_or_default()
    }

    #[func]
    fn document_info(&self) -> VarDictionary {
        dictionary_from_json(&self.session.document_info())
    }

    #[func]
    fn tool_settings(&self) -> VarDictionary {
        dictionary_from_json(&serde_json::to_value(&self.session.tools).unwrap_or_default())
    }

    #[func]
    fn options(&self) -> VarDictionary {
        dictionary_from_json(&serde_json::to_value(&self.session.options).unwrap_or_default())
    }

    /// Sets the four stroke materials: tubes and flat strokes, each opaque and see-through.
    #[func]
    fn set_stroke_materials(
        &mut self,
        tube_opaque: Gd<Material>,
        tube_see_through: Gd<Material>,
        flat_opaque: Gd<Material>,
        flat_see_through: Gd<Material>,
    ) {
        self.materials[SurfaceKind::TubeOpaque as usize] = Some(tube_opaque);
        self.materials[SurfaceKind::TubeSeeThrough as usize] = Some(tube_see_through);
        self.materials[SurfaceKind::FlatOpaque as usize] = Some(flat_opaque);
        self.materials[SurfaceKind::FlatSeeThrough as usize] = Some(flat_see_through);
        // Force materials to be applied again.
        for layer in self.layers.values_mut() {
            for chunk in &mut layer.chunks {
                chunk.faded = None;
            }
        }
    }

    /// Sets the four materials of paint layers that scene lights shade, in the order
    /// of `set_stroke_materials`.
    #[func]
    fn set_lit_stroke_materials(
        &mut self,
        tube_opaque: Gd<Material>,
        tube_see_through: Gd<Material>,
        flat_opaque: Gd<Material>,
        flat_see_through: Gd<Material>,
    ) {
        self.lit_materials[SurfaceKind::TubeOpaque as usize] = Some(tube_opaque);
        self.lit_materials[SurfaceKind::TubeSeeThrough as usize] = Some(tube_see_through);
        self.lit_materials[SurfaceKind::FlatOpaque as usize] = Some(flat_opaque);
        self.lit_materials[SurfaceKind::FlatSeeThrough as usize] = Some(flat_see_through);
        for layer in self.layers.values_mut() {
            if strokes::is_lit(layer) {
                strokes::set_lit(layer, Some(&self.lit_materials));
            }
        }
    }

    /// Sets the materials of everything that is not a stroke, by name: the picture
    /// materials (`picture_flat_opaque`, `picture_flat_see_through`,
    /// `picture_sphere_opaque`, `picture_sphere_see_through`), which are copied once
    /// per picture; the model materials (`model_opaque`, `model_see_through`,
    /// `model_opaque_lit`, `model_see_through_lit`); `line` for gizmos and bounding
    /// boxes; and `wire` for the edges drawn over a model.
    #[func]
    fn set_layer_materials(&mut self, materials: VarDictionary) {
        self.nodes.set_materials(&materials);
    }

    /// A tileable blue-noise threshold map as a single-channel image.
    #[func]
    fn blue_noise_image(size: i32) -> Option<Gd<Image>> {
        let size = (size.max(8) as usize).next_power_of_two();
        let data = PackedByteArray::from(dither::blue_noise(size, 1).as_slice());
        Image::create_from_data(size as i32, size as i32, false, Format::R8, &data)
    }

    /// Opens the document at `path`: a native project or a Quill project folder.
    /// Returns `{ok, error, warnings, kind}`, where `kind` is "Limner project" or
    /// "Quill project".
    #[func]
    fn open(&mut self, path: GString) -> VarDictionary {
        let path_string = path.to_string();
        match limner::open(Path::new(&path_string)) {
            Ok(opened) => {
                self.clear_instances();
                self.source_kind = opened.kind.label().to_owned();
                self.warnings = opened.warnings;
                self.session.replace_document(opened.document);
                match opened.kind {
                    limner::SourceKind::Limn => {
                        if let Some(state) = limner::read_state(Path::new(&path_string)) {
                            self.session.restore_editor_state(&state);
                        }
                    }
                    limner::SourceKind::Quill => {
                        if let Some(state) = &opened.quill_state {
                            self.adopt_quill_state(state);
                        }
                    }
                }
                self.source_path = path_string;
                self.signals().document_replaced().emit();
                vdict! { "ok" => true, "error" => "", "kind" => self.source_kind.clone(), "warnings" => &self.get_warnings() }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error.to_string(), "kind" => "", "warnings" => &PackedStringArray::new() }
            }
        }
    }

    /// Saves the document at `path` in the native format: a project folder, or one
    /// packed file when the path ends in `.limnz`. The editor's working state is
    /// saved with it, and `thumbnail` (PNG data, may be empty) as the project's
    /// picture. Returns `{ok, error, bytes_written}`.
    #[func]
    fn save(&mut self, path: GString, thumbnail: PackedByteArray) -> VarDictionary {
        let path_string = path.to_string();
        let state = self.session.editor_state();
        let thumbnail = (!thumbnail.is_empty()).then(|| thumbnail.as_slice());
        match limner::save_with_state(
            self.session.doc(),
            Path::new(&path_string),
            &state,
            thumbnail,
        ) {
            Ok(report) => {
                self.source_path = path_string;
                self.source_kind = limner::SourceKind::Limn.label().to_owned();
                self.session.mark_saved();
                vdict! { "ok" => true, "error" => "", "bytes_written" => report.bytes_written as i64 }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error.to_string(), "bytes_written" => 0 }
            }
        }
    }

    /// Counts the document as having unsaved changes, as after recovering work that
    /// was never saved.
    #[func]
    fn mark_edited(&mut self) {
        self.session.mark_unsaved();
    }

    #[func]
    fn get_warnings(&self) -> PackedStringArray {
        self.warnings.iter().map(GString::from).collect()
    }

    /// Title, source and size of the open document.
    #[func]
    fn info(&self) -> VarDictionary {
        let doc = self.session.doc();
        vdict! {
            "title" => doc.settings.title.clone(),
            "path" => self.source_path.clone(),
            "kind" => self.source_kind.clone(),
            "layers" => doc.layer_count() as i64,
            "drawings" => doc.drawing_count() as i64,
            "strokes" => doc.stroke_count() as i64,
            "points" => doc.point_count() as i64,
            "fps" => doc.settings.rate.as_f64(),
            "range_start" => time::to_seconds(doc.settings.range_start),
            "range_end" => time::to_seconds(doc.settings.range_end),
        }
    }

    #[func]
    fn background_color(&self) -> Color {
        color_from_linear(self.session.doc().settings.background)
    }

    /// What the last frame drew, and how many sound layers are playing.
    #[func]
    fn stats(&self) -> VarDictionary {
        vdict! {
            "visible_layers" => self.stats.visible_layers as i64,
            "chunks" => self.stats.chunks as i64,
            "vertices" => self.stats.vertices as i64,
            "triangles" => self.stats.triangles as i64,
            "meshes_built" => self.stats.meshes_built as i64,
            "cached_meshes" => self.cache.len() as i64,
            "sounds_playing" => self.nodes.sounds_playing() as i64,
        }
    }

    // ----- controller input -----

    /// Describes one hand for the next `input_commit`. `which` is 0 for the drawing
    /// hand and 1 for the other. Position and rotation are in viewer space; the
    /// hand's -Z axis points forward. `buttons` has bit 0 for the lower face button,
    /// bit 1 for the upper one and bit 2 for a click of the stick.
    #[func]
    #[allow(clippy::too_many_arguments)]
    fn input_hand(
        &mut self,
        which: i32,
        position: Vector3,
        rotation: Quaternion,
        trigger: f32,
        grip: f32,
        stick: Vector2,
        buttons: i32,
    ) {
        let hand = HandInput {
            tracked: true,
            position: dvec3(position),
            rotation: DQuat::from_xyzw(
                rotation.x as f64,
                rotation.y as f64,
                rotation.z as f64,
                rotation.w as f64,
            )
            .normalize(),
            trigger: trigger.clamp(0.0, 1.0),
            grip: grip.clamp(0.0, 1.0),
            stick: [stick.x.clamp(-1.0, 1.0), stick.y.clamp(-1.0, 1.0)],
            lower_button: buttons & 1 != 0,
            upper_button: buttons & 2 != 0,
            stick_click: buttons & 4 != 0,
            ray: None,
        };
        if which == 0 {
            self.input.main = hand;
        } else {
            self.input.off = hand;
        }
    }

    /// Says that a hand is not there: not tracked, or the pointer has left the view.
    #[func]
    fn input_hand_absent(&mut self, which: i32) {
        if which == 0 {
            self.input.main = HandInput::default();
        } else {
            self.input.off = HandInput::default();
        }
    }

    /// For a hand steered from a screen: the line of sight through the pointer, in
    /// viewer space. Handles of the gimbal are then picked along it. Call it after
    /// `input_hand` for the same hand, every frame the pointer is over the view.
    #[func]
    fn input_hand_ray(&mut self, which: i32, origin: Vector3, direction: Vector3) {
        let direction = dvec3(direction).normalize_or_zero();
        let ray = (direction != DVec3::ZERO).then(|| (dvec3(origin), direction));
        if which == 0 {
            self.input.main.ray = ray;
        } else {
            self.input.off.ray = ray;
        }
    }

    /// Abandons the tool at work and anything held by the grips, undoing what they
    /// did so far: what Escape does.
    #[func]
    fn input_cancel(&mut self) {
        self.session.cancel_gesture();
    }

    /// True while the grip of that hand (0 the drawing hand, 1 the other) holds something.
    #[func]
    fn is_grabbing(&self, which: i32) -> bool {
        self.session.is_grabbing(which.clamp(0, 1) as usize)
    }

    /// The gimbal to draw, in viewer space, or an empty dictionary when there is
    /// none: `{subject ("layer" or "selection"), layer, position, rotation, size,
    /// hovered ([handle or "", handle or ""] for the two hands), active (handle or
    /// ""), arrow_start, arrow_end, plane_near, plane_far, center_radius}`. `size` is
    /// the radius of the rings in metres and the last five are fractions of it.
    /// Handles are named "center", "axis_x", "plane_x", "ring_x" and so on.
    #[func]
    fn gimbal(&self) -> VarDictionary {
        let Some(gimbal) = self.session.gimbal() else {
            return VarDictionary::new();
        };
        let name = |handle: Option<GimbalHandle>| -> GString {
            handle
                .and_then(|h| serde_json::to_value(h).ok())
                .and_then(|v| v.as_str().map(GString::from))
                .unwrap_or_default()
        };
        let (subject, layer) = match gimbal.subject {
            Subject::Layer(id) => ("layer", id.0 as i64),
            Subject::Selection => ("selection", -1),
            Subject::View => ("view", -1),
        };
        let hovered = varray![&name(gimbal.hovered[0]), &name(gimbal.hovered[1])];
        vdict! {
            "subject" => subject,
            "layer" => layer,
            "position" => crate::convert::vector3(gimbal.position),
            "rotation" => Quaternion::new(
                gimbal.rotation.x as f32,
                gimbal.rotation.y as f32,
                gimbal.rotation.z as f32,
                gimbal.rotation.w as f32,
            ),
            "size" => gimbal.size,
            "hovered" => &hovered,
            "active" => &name(gimbal.active),
            "arrow_start" => ARROW_START,
            "arrow_end" => ARROW_END,
            "plane_near" => PLANE_NEAR,
            "plane_far" => PLANE_FAR,
            "center_radius" => CENTER_RADIUS,
        }
    }

    /// The mirror of mirrored working, in viewer space, or an empty dictionary when
    /// it is off: `{position, rotation, size}`. The mirror's face looks along the Z
    /// axis of `rotation`; `size` is half the side of a square of it worth drawing.
    #[func]
    fn mirror_plane(&self) -> VarDictionary {
        let Some((position, turn)) = self.session.mirror_plane() else {
            return VarDictionary::new();
        };
        let size = (position.distance(self.input.head) * 0.8).max(0.5);
        vdict! {
            "position" => crate::convert::vector3(position),
            "rotation" => Quaternion::new(turn.x as f32, turn.y as f32, turn.z as f32, turn.w as f32),
            "size" => size,
        }
    }

    /// Sets the material drawn over selected strokes.
    #[func]
    fn set_highlight_material(&mut self, material: Gd<Material>) {
        self.overlays.set_highlight_material(material);
    }

    /// Sets the material drawn over the stroke the lower button would delete.
    #[func]
    fn set_at_hand_material(&mut self, material: Gd<Material>) {
        self.overlays.set_at_hand_material(material);
    }

    /// The stroke the lower button would delete now, as `{layer, drawing, stroke}`,
    /// or an empty dictionary when the hand is at none.
    #[func]
    fn stroke_at_hand(&self) -> VarDictionary {
        match self.session.stroke_at_hand() {
            Some(at) => vdict! {
                "layer" => at.layer.0 as i64,
                "drawing" => at.drawing.0 as i64,
                "stroke" => at.stroke as i64,
            },
            None => VarDictionary::new(),
        }
    }

    /// Where the eyes are, in viewer space.
    #[func]
    fn input_head(&mut self, position: Vector3) {
        self.input.head = dvec3(position);
    }

    /// Which way the eyes look, in viewer space. Kept until it is given again.
    #[func]
    fn input_head_facing(&mut self, forward: Vector3) {
        if let Some(forward) = dvec3(forward).try_normalize() {
            self.input.facing = forward;
        }
    }

    /// Applies the input described since the last commit: runs the tool, grabs, and
    /// button actions. Returns what the device should do in response, as dictionaries
    /// with a `type` of "haptic" (`main`, `strength`, `seconds`), "sound" (`name`),
    /// "message" (`text`) or "menu" (show or hide the hand-held palette).
    #[func]
    fn input_commit(&mut self, delta: f64) -> VarArray {
        self.input.dt = delta.clamp(0.0, 0.25);
        let mut out = VarArray::new();
        for feedback in self.session.update(&self.input) {
            let item = match feedback {
                Feedback::Haptic {
                    main,
                    strength,
                    seconds,
                } => {
                    vdict! { "type" => "haptic", "main" => main, "strength" => strength, "seconds" => seconds }
                }
                Feedback::Sound(name) => vdict! { "type" => "sound", "name" => name },
                Feedback::Message(text) => vdict! { "type" => "message", "text" => text },
                Feedback::Menu => vdict! { "type" => "menu" },
            };
            out.push(&item.to_variant());
        }
        out
    }

    /// What to draw at the drawing hand: `{tracked, position, rotation, tool, radius,
    /// inner (or null), color, opacity, active, painting, blocked, grid_snap, alt,
    /// at_handle}`, in viewer space. `at_handle` is true when the hand is at a handle
    /// of the gimbal, where the trigger works the handle instead of the tool.
    /// `painting` is true while Paint or Line lays down a stroke. `blocked` is the
    /// message the trigger would give because the current layer is not one the tool
    /// can work on, or an empty string when it would work. `grid_snap` is true when
    /// the Line tool would snap to the brush grid.
    #[func]
    fn tool_cursor(&self) -> VarDictionary {
        let cursor = self.session.cursor();
        let rotation = Quaternion::new(
            cursor.rotation.x as f32,
            cursor.rotation.y as f32,
            cursor.rotation.z as f32,
            cursor.rotation.w as f32,
        );
        let inner = cursor.inner.map_or(Variant::nil(), |r| r.to_variant());
        // With mirrored working on, the tools act at the hand's mirror image too.
        let mirror = if cursor.tracked {
            self.session.mirror_plane().map(|(on_plane, turn)| {
                let normal = turn * DVec3::Z;
                cursor.position - normal * 2.0 * (cursor.position - on_plane).dot(normal)
            })
        } else {
            None
        };
        let mirror = mirror.map_or(Variant::nil(), |p| crate::convert::vector3(p).to_variant());
        vdict! {
            "tracked" => cursor.tracked,
            "position" => crate::convert::vector3(cursor.position),
            "mirror_position" => &mirror,
            "rotation" => rotation,
            "tool" => &variant_from_json(&serde_json::to_value(cursor.tool).unwrap_or_default()),
            "radius" => cursor.radius,
            "inner" => &inner,
            "color" => color_from_linear(cursor.color),
            "opacity" => cursor.opacity,
            "active" => cursor.active,
            "painting" => cursor.painting,
            "blocked" => cursor.blocked.as_deref().unwrap_or(""),
            "grid_snap" => cursor.grid_snap,
            "alt" => cursor.alt,
            "at_handle" => cursor.at_handle,
        }
    }

    // ----- time -----

    #[func]
    fn set_time(&mut self, seconds: f64) {
        self.session
            .playback
            .seek(time::from_seconds(seconds.max(0.0)));
    }

    #[func]
    fn get_time(&self) -> f64 {
        time::to_seconds(self.session.playback.time)
    }

    // ----- view -----

    /// Slides the world by `delta` metres of viewer space.
    #[func]
    fn view_pan(&mut self, delta: Vector3) {
        self.session.view.pan(dvec3(delta));
    }

    /// Turns the world about a viewer-space point.
    #[func]
    fn view_rotate(&mut self, pivot: Vector3, axis: Vector3, angle: f32) {
        let axis = dvec3(axis).normalize_or_zero();
        if axis != DVec3::ZERO {
            self.session
                .view
                .rotate_about(dvec3(pivot), DQuat::from_axis_angle(axis, angle as f64));
        }
    }

    /// Scales the world about a viewer-space point; a factor above one enlarges it.
    #[func]
    fn view_zoom(&mut self, pivot: Vector3, factor: f32) {
        self.session
            .view
            .zoom_about(dvec3(pivot), factor as f64, DEFAULT_ZOOM_LIMITS);
    }

    /// Fits everything visible into a viewer-space sphere. Returns false when nothing is visible.
    #[func]
    fn view_frame_all(&mut self, target: Vector3, radius: f32) -> bool {
        let Some((center, world_radius)) = self.session.visible_bounds() else {
            return false;
        };
        self.session
            .view
            .frame_sphere(center, world_radius, dvec3(target), radius as f64);
        true
    }

    #[func]
    fn view_go_to_default_viewpoint(&mut self) {
        self.session.go_to_default_viewpoint();
    }

    /// Stands the viewer on a spawn area, or at the origin of any other layer.
    #[func]
    fn view_go_to_viewpoint(&mut self, layer: i64) {
        self.session.go_to_viewpoint(LayerId(layer.max(0) as u64));
    }

    /// Where the world sits in viewer space: the transform for anything drawn in world coordinates.
    #[func]
    fn viewer_from_world(&self) -> Transform3D {
        transform3d(&self.session.view.viewer_from_world_affine())
    }

    /// Where a layer is in viewer space at the playhead: the transform for anything
    /// drawn at the layer, such as a gizmo. Its scale includes the layer's and the view's.
    #[func]
    fn layer_transform(&self, layer: i64) -> Transform3D {
        let frame = self.session.evaluate();
        match frame.get(LayerId(layer.max(0) as u64)) {
            Some(state) => {
                transform3d(&(self.session.view.viewer_from_world_affine() * state.world))
            }
            None => Transform3D::IDENTITY,
        }
    }

    /// World units per metre of viewer space.
    #[func]
    fn view_scale(&self) -> f64 {
        self.session.view.scale()
    }

    /// The size of a cell of the brush grid at this zoom, in world units: the lattice
    /// the brush grid shows and the Line tool snaps to with the modifier.
    #[func]
    fn brush_grid_spacing(&self) -> f64 {
        self.session.brush_grid_spacing()
    }

    // ----- rendering -----

    /// Brings the child mesh instances in line with the document at the current time.
    /// Runs every frame; call it directly to update without waiting for one.
    #[func]
    fn sync(&mut self) {
        let session = &self.session;
        let frame = session.evaluate();
        let viewer_from_world = session.view.viewer_from_world_affine();
        let doc = session.doc();
        // Selected strokes being carried are shown moved, not made again.
        let carried = session.selection_motion();

        self.cache.begin_frame();
        let mut stats = Stats::default();
        let mut shown: HashSet<LayerId> = HashSet::new();
        let container = container(&mut self.container, &mut self.base);

        for state in &frame.layers {
            let Content::Paint {
                drawing: drawing_id,
                ..
            } = state.content
            else {
                continue;
            };
            if !state.visible || state.opacity <= 0.0 {
                continue;
            }
            let Some(drawing) = doc.drawing(drawing_id) else {
                continue;
            };
            shown.insert(state.id);
            stats.visible_layers += 1;
            let instances = self.layers.entry(state.id).or_default();
            // Lights shade the paint layers that ask for it; the rest are drawn as painted.
            let lit = doc
                .layer(state.id)
                .and_then(|l| l.as_paint())
                .is_some_and(|p| p.shading == Shading::Lit);
            strokes::set_lit(instances, lit.then_some(&self.lit_materials));
            sync_layer(
                instances,
                state,
                strokes::Shown {
                    id: drawing_id,
                    drawing,
                    carried: carried.as_ref(),
                    edits: session.edit_count(),
                },
                &viewer_from_world,
                &mut self.cache,
                &self.materials,
                container,
                &mut stats,
            );
        }

        // Hide layers that are not shown now; free the ones that no longer exist.
        self.layers.retain(|id, instances| {
            if shown.contains(id) {
                return true;
            }
            if doc.layer(*id).is_none() {
                for chunk in instances.chunks.drain(..) {
                    chunk.node.clone().queue_free();
                }
                return false;
            }
            for chunk in &mut instances.chunks {
                set_visible(chunk, false);
            }
            true
        });

        self.nodes
            .sync(container, session, &frame, &viewer_from_world);
        self.overlays.sync(
            container,
            session,
            &frame,
            &viewer_from_world,
            &mut self.cache,
            &self.materials,
        );

        stats.meshes_built = self.cache.built_this_frame;
        self.cache.end_frame();
        self.stats = stats;
    }
}

impl LimnerScene {
    /// Carries over what a Quill project remembers about how it was being worked on.
    fn adopt_quill_state(&mut self, state: &limner::quill::QuillState) {
        if let Some(layer) = state
            .paint_layer
            .as_deref()
            .and_then(|path| self.session.doc().find_by_path(path))
        {
            self.session.set_current_layer(layer);
        }
        if let Some(show_grid) = state.show_grid {
            self.session.options.show_grid = show_grid;
        }
    }

    /// Tells the interface about changes since it was last told.
    fn announce_changes(&mut self) {
        let now = (self.session.doc_revision(), self.session.state_revision());
        let before = std::mem::replace(&mut self.announced, now);
        if now.0 != before.0 {
            self.signals().document_changed().emit();
        }
        if now.1 != before.1 {
            self.signals().state_changed().emit();
        }
    }

    fn clear_instances(&mut self) {
        self.nodes.clear();
        self.overlays.clear();
        for (_, instances) in self.layers.drain() {
            for chunk in instances.chunks {
                chunk.node.clone().queue_free();
            }
        }
        self.cache.clear();
        self.stats = Stats::default();
    }
}
