//! Paint layers on screen: one mesh instance per chunk of the drawing a layer shows.

use crate::convert::transform3d;
use crate::meshes::{chunk_key, MeshCache};
use godot::classes::{Material, MeshInstance3D};
use godot::prelude::*;
use limner::core::eval::LayerState;
use limner::core::glam::{DAffine3, DVec3};
use limner::core::model::{Drawing, LayerId};
use limner::core::tess::SurfaceKind;

/// One chunk of a paint layer on screen.
pub(super) struct ChunkInstance {
    pub(super) node: Gd<MeshInstance3D>,
    /// Identity of the chunk whose mesh the node shows; zero when it shows none.
    key: usize,
    kinds: Vec<SurfaceKind>,
    origin: DVec3,
    /// Whether the materials currently applied are the faded-layer set.
    pub(super) faded: Option<bool>,
    opacity: f32,
    transform: Option<Transform3D>,
    visible: bool,
}

#[derive(Default)]
pub(super) struct LayerInstances {
    pub(super) chunks: Vec<ChunkInstance>,
}

/// What a frame put on screen, for the statistics readout.
#[derive(Default, Clone, Copy)]
pub(super) struct Stats {
    pub visible_layers: usize,
    pub chunks: usize,
    pub vertices: usize,
    pub triangles: usize,
    pub meshes_built: usize,
}

/// The node that parents every stroke mesh, created on first use.
pub(super) fn container<'a>(
    slot: &'a mut Option<Gd<Node3D>>,
    base: &mut Base<Node3D>,
) -> &'a mut Gd<Node3D> {
    slot.get_or_insert_with(|| {
        let mut node = Node3D::new_alloc();
        node.set_name("Strokes");
        base.to_init_gd().add_child(&node);
        node
    })
}

pub(super) fn set_visible(chunk: &mut ChunkInstance, visible: bool) {
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
pub(super) fn sync_layer(
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
