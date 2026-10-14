//! `LimnerScene`: the node that owns an editing session and shows it.
//!
//! Each frame it evaluates the document and reconciles its child mesh instances with
//! the result. It holds no product logic: what is visible, where, and what an edit
//! does are all decided in the core.

mod nodes;
mod overlays;
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
use limner::core::model::{Document, LayerId};
use limner::core::session::{
    Feedback, HandInput, InputFrame, PlaybackEvent, Session, DEFAULT_ZOOM_LIMITS,
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
    /// Indexed by `SurfaceKind`.
    materials: [Option<Gd<Material>>; 4],
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
            materials: [None, None, None, None],
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
        self.clear_instances();
        self.source_path.clear();
        self.source_kind.clear();
        self.warnings.clear();
        self.session.replace_document(Document::new());
        self.signals().document_replaced().emit();
    }

    /// Brings a picture, a sound or a model into the document as a new layer.
    /// Returns `{ok, error, layer}`.
    #[func]
    fn import_file(&mut self, path: GString) -> VarDictionary {
        let _ = path;
        vdict! { "ok" => false, "error" => "importing files is not available in this build yet", "layer" => -1 }
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

    /// What the last frame drew.
    #[func]
    fn stats(&self) -> VarDictionary {
        vdict! {
            "visible_layers" => self.stats.visible_layers as i64,
            "chunks" => self.stats.chunks as i64,
            "vertices" => self.stats.vertices as i64,
            "triangles" => self.stats.triangles as i64,
            "meshes_built" => self.stats.meshes_built as i64,
            "cached_meshes" => self.cache.len() as i64,
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

    /// Where the eyes are, in viewer space.
    #[func]
    fn input_head(&mut self, position: Vector3) {
        self.input.head = dvec3(position);
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
    /// inner (or null), color, opacity, active, alt}`, in viewer space.
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
        vdict! {
            "tracked" => cursor.tracked,
            "position" => crate::convert::vector3(cursor.position),
            "rotation" => rotation,
            "tool" => &variant_from_json(&serde_json::to_value(cursor.tool).unwrap_or_default()),
            "radius" => cursor.radius,
            "inner" => &inner,
            "color" => color_from_linear(cursor.color),
            "opacity" => cursor.opacity,
            "active" => cursor.active,
            "alt" => cursor.alt,
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

    // ----- rendering -----

    /// Brings the child mesh instances in line with the document at the current time.
    /// Runs every frame; call it directly to update without waiting for one.
    #[func]
    fn sync(&mut self) {
        let session = &self.session;
        let frame = session.evaluate();
        let viewer_from_world = session.view.viewer_from_world_affine();
        let doc = session.doc();

        self.cache.begin_frame();
        let mut stats = Stats::default();
        let mut shown: HashSet<LayerId> = HashSet::new();
        let container = container(&mut self.container, &mut self.base);

        for state in &frame.layers {
            let Content::Paint { drawing, .. } = state.content else {
                continue;
            };
            if !state.visible || state.opacity <= 0.0 {
                continue;
            }
            let Some(drawing) = doc.drawing(drawing) else {
                continue;
            };
            shown.insert(state.id);
            stats.visible_layers += 1;
            let instances = self.layers.entry(state.id).or_default();
            sync_layer(
                instances,
                state,
                drawing,
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
        for (_, instances) in self.layers.drain() {
            for chunk in instances.chunks {
                chunk.node.clone().queue_free();
            }
        }
        self.cache.clear();
        self.stats = Stats::default();
    }
}
