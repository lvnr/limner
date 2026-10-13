//! `LimnerScene`: the node that owns an editing session and shows it.
//!
//! Each frame it evaluates the document and reconciles its child mesh instances with
//! the result. It holds no product logic: what is visible, where, and what an edit
//! does are all decided in the core.

use crate::convert::{color_from_linear, dvec3, transform3d};
use crate::meshes::{chunk_key, MeshCache};
use godot::classes::image::Format;
use godot::classes::{Image, Material, MeshInstance3D};
use godot::prelude::*;
use limner::core::dither;
use limner::core::eval::{Content, LayerState};
use limner::core::glam::{DAffine3, DQuat, DVec3};
use limner::core::model::{Drawing, LayerId};
use limner::core::session::{Session, DEFAULT_ZOOM_LIMITS};
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
    session: Option<Session>,
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
            session: None,
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

    fn process(&mut self, _delta: f64) {
        self.sync();
    }
}

#[godot_api]
impl LimnerScene {
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
                self.session = Some(Session::new(opened.document));
                vdict! { "ok" => true, "error" => "", "kind" => self.source_kind.clone(), "warnings" => &self.get_warnings() }
            }
            Err(error) => {
                vdict! { "ok" => false, "error" => error.to_string(), "kind" => "", "warnings" => &PackedStringArray::new() }
            }
        }
    }

    #[func]
    fn has_document(&self) -> bool {
        self.session.is_some()
    }

    #[func]
    fn get_warnings(&self) -> PackedStringArray {
        self.warnings.iter().map(GString::from).collect()
    }

    /// Title, source and size of the open document.
    #[func]
    fn info(&self) -> VarDictionary {
        let Some(session) = &self.session else {
            return VarDictionary::new();
        };
        let doc = session.doc();
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
        let rgb = self
            .session
            .as_ref()
            .map_or([0.8, 0.8, 0.8], |s| s.doc().settings.background);
        color_from_linear(rgb)
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

    // ----- time -----

    #[func]
    fn set_time(&mut self, seconds: f64) {
        if let Some(session) = &mut self.session {
            session.clock.time = time::from_seconds(seconds.max(0.0));
        }
    }

    #[func]
    fn get_time(&self) -> f64 {
        self.session
            .as_ref()
            .map_or(0.0, |s| time::to_seconds(s.clock.time))
    }

    // ----- view -----

    /// Slides the world by `delta` metres of viewer space.
    #[func]
    fn view_pan(&mut self, delta: Vector3) {
        if let Some(session) = &mut self.session {
            session.view.pan(dvec3(delta));
        }
    }

    /// Turns the world about a viewer-space point.
    #[func]
    fn view_rotate(&mut self, pivot: Vector3, axis: Vector3, angle: f32) {
        if let Some(session) = &mut self.session {
            let axis = dvec3(axis).normalize_or_zero();
            if axis != DVec3::ZERO {
                session
                    .view
                    .rotate_about(dvec3(pivot), DQuat::from_axis_angle(axis, angle as f64));
            }
        }
    }

    /// Scales the world about a viewer-space point; a factor above one enlarges it.
    #[func]
    fn view_zoom(&mut self, pivot: Vector3, factor: f32) {
        if let Some(session) = &mut self.session {
            session
                .view
                .zoom_about(dvec3(pivot), factor as f64, DEFAULT_ZOOM_LIMITS);
        }
    }

    /// Fits everything visible into a viewer-space sphere. Returns false when nothing is visible.
    #[func]
    fn view_frame_all(&mut self, target: Vector3, radius: f32) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        let Some((center, world_radius)) = session.visible_bounds() else {
            return false;
        };
        session
            .view
            .frame_sphere(center, world_radius, dvec3(target), radius as f64);
        true
    }

    #[func]
    fn view_go_to_default_viewpoint(&mut self) {
        if let Some(session) = &mut self.session {
            session.go_to_default_viewpoint();
        }
    }

    /// World units per metre of viewer space.
    #[func]
    fn view_scale(&self) -> f64 {
        self.session.as_ref().map_or(1.0, |s| s.view.scale())
    }

    // ----- rendering -----

    /// Brings the child mesh instances in line with the document at the current time.
    /// Runs every frame; call it directly to update without waiting for one.
    #[func]
    fn sync(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
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
