//! `LimnerScene`: the node that owns an editing session and shows it.
//!
//! Each frame it evaluates the document and reconciles its child mesh instances with
//! the result. It holds no product logic: what is visible, where, and what an edit
//! does are all decided in the core.

use crate::convert::{
    color_from_linear, dictionary_from_json, dvec3, json_from_variant, transform3d,
    variant_from_json,
};
use crate::meshes::{chunk_key, MeshCache};
use godot::classes::image::Format;
use godot::classes::{Image, Material, MeshInstance3D};
use godot::prelude::*;
use limner::core::dither;
use limner::core::eval::{Content, LayerState};
use limner::core::glam::{DAffine3, DQuat, DVec3};
use limner::core::model::{Document, Drawing, LayerId};
use limner::core::session::{
    Feedback, HandInput, InputFrame, PlaybackEvent, Session, DEFAULT_ZOOM_LIMITS,
};
use limner::core::tess::SurfaceKind;
use limner::core::time;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// One chunk of a paint layer on screen.
struct ChunkInstance {
    node: Gd<MeshInstance3D>,
    /// Identity of the chunk whose mesh the node shows; zero when it shows none.
    key: usize,
    kinds: Vec<SurfaceKind>,
    origin: DVec3,
    /// Whether the materials currently applied are the faded-layer set.
    faded: Option<bool>,
    opacity: f32,
    transform: Option<Transform3D>,
    visible: bool,
}

#[derive(Default)]
struct LayerInstances {
    chunks: Vec<ChunkInstance>,
}

#[derive(Default, Clone, Copy)]
struct Stats {
    visible_layers: usize,
    chunks: usize,
    vertices: usize,
    triangles: usize,
    meshes_built: usize,
}

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

    /// Opens the document at `path`. Returns `{ok, error, warnings, kind}`.
    #[func]
    fn open(&mut self, path: GString) -> VarDictionary {
        let path_string = path.to_string();
        match limner::open(Path::new(&path_string)) {
            Ok(opened) => {
                self.clear_instances();
                self.source_path = path_string;
                self.source_kind = opened.kind.label().to_owned();
                self.warnings = opened.warnings;
                self.session.replace_document(opened.document);
                self.signals().document_replaced().emit();
                vdict! { "ok" => true, "error" => "", "kind" => self.source_kind.clone(), "warnings" => &self.get_warnings() }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error.to_string(), "kind" => "", "warnings" => &PackedStringArray::new() }
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

        stats.meshes_built = self.cache.built_this_frame;
        self.cache.end_frame();
        self.stats = stats;
    }
}

impl LimnerScene {
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
        for (_, instances) in self.layers.drain() {
            for chunk in instances.chunks {
                chunk.node.clone().queue_free();
            }
        }
        self.cache.clear();
        self.stats = Stats::default();
    }
}

/// The node that parents every stroke mesh, created on first use.
fn container<'a>(slot: &'a mut Option<Gd<Node3D>>, base: &mut Base<Node3D>) -> &'a mut Gd<Node3D> {
    slot.get_or_insert_with(|| {
        let mut node = Node3D::new_alloc();
        node.set_name("Strokes");
        base.to_init_gd().add_child(&node);
        node
    })
}

fn set_visible(chunk: &mut ChunkInstance, visible: bool) {
    if chunk.visible != visible {
        chunk.node.set_visible(visible);
        chunk.visible = visible;
    }
}

/// A small per-layer number that keeps the dither patterns of different layers apart.
fn layer_seed(id: LayerId) -> f32 {
    (id.0.wrapping_mul(2_654_435_761) % 251) as f32
}

#[allow(clippy::too_many_arguments)]
fn sync_layer(
    instances: &mut LayerInstances,
    state: &LayerState,
    drawing: &Drawing,
    viewer_from_world: &DAffine3,
    cache: &mut MeshCache,
    materials: &[Option<Gd<Material>>; 4],
    container: &mut Gd<Node3D>,
    stats: &mut Stats,
) {
    let chunks = drawing.chunks();
    while instances.chunks.len() < chunks.len() {
        let mut node = MeshInstance3D::new_alloc();
        node.set_visible(false);
        node.set_instance_shader_parameter("layer_seed", &layer_seed(state.id).to_variant());
        container.add_child(&node);
        instances.chunks.push(ChunkInstance {
            node,
            key: 0,
            kinds: Vec::new(),
            origin: DVec3::ZERO,
            faded: None,
            opacity: -1.0,
            transform: None,
            visible: false,
        });
    }

    let layer_matrix = *viewer_from_world * state.world;
    let faded = state.opacity < 0.999;

    for (instance, chunk) in instances.chunks.iter_mut().zip(chunks) {
        let key = chunk_key(chunk);
        let built = cache.get(chunk);
        let Some(built) = built else {
            instance.key = key;
            instance.kinds.clear();
            set_visible(instance, false);
            continue;
        };
        if instance.key != key || instance.kinds.is_empty() {
            instance.node.set_mesh(&built.mesh);
            instance.kinds.clone_from(&built.kinds);
            instance.origin = built.origin;
            instance.key = key;
            instance.faded = None;
            instance.transform = None;
        }
        if instance.faded != Some(faded) {
            for (surface, kind) in instance.kinds.iter().enumerate() {
                // On a faded layer even opaque strokes are dithered.
                let material_kind = match (faded, kind) {
                    (true, SurfaceKind::TubeOpaque) => SurfaceKind::TubeSeeThrough,
                    (true, SurfaceKind::FlatOpaque) => SurfaceKind::FlatSeeThrough,
                    _ => *kind,
                };
                if let Some(material) = &materials[material_kind as usize] {
                    instance
                        .node
                        .set_surface_override_material(surface as i32, material);
                }
            }
            instance.faded = Some(faded);
        }
        if instance.opacity != state.opacity {
            instance
                .node
                .set_instance_shader_parameter("layer_opacity", &state.opacity.to_variant());
            instance.opacity = state.opacity;
        }
        // The chunk centre is added here, in double precision, so vertices stay small
        // numbers however far the chunk is from the layer origin.
        let transform = transform3d(&(layer_matrix * DAffine3::from_translation(instance.origin)));
        if instance.transform != Some(transform) {
            instance.node.set_transform(transform);
            instance.transform = Some(transform);
        }
        set_visible(instance, true);
        stats.chunks += 1;
        stats.vertices += built.vertices;
        stats.triangles += built.triangles;
    }
    for instance in instances.chunks.iter_mut().skip(chunks.len()) {
        set_visible(instance, false);
    }
}
