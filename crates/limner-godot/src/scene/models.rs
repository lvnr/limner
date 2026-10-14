//! Model layers on screen: a mesh, and its edges when the wireframe is asked for.

use super::nodes::{release, Materials, MediaCache};
use super::strokes::layer_seed;
use crate::convert::transform3d;
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, MeshInstance3D};
use godot::prelude::*;
use limner::core::eval::LayerState;
use limner::core::glam::{DAffine3, DVec3, Vec3};
use limner::core::model::{Media, MediaData, MeshData, Model, Shading};
use std::collections::HashSet;
use std::sync::Arc;

/// The colour of a model that brings none of its own, in linear light.
const DEFAULT_COLOR: [f32; 3] = [0.5, 0.5, 0.5];

/// A model's mesh in the form the renderer takes.
#[derive(Debug, PartialEq)]
pub(super) struct Geometry {
    /// The centre of the mesh in layer space. Positions are relative to it, so they
    /// stay small numbers wherever the model sits.
    pub origin: DVec3,
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// sRGB-encoded, as the stroke shaders expect; always opaque.
    pub colors: Vec<[u8; 4]>,
    /// Triangles wound clockwise, which is the renderer's front.
    pub indices: Vec<i32>,
    /// The mesh's corners in layer space.
    pub min: DVec3,
    pub max: DVec3,
}

fn srgb8(linear: f32) -> u8 {
    let c = linear.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

fn encode(color: [f32; 3]) -> [u8; 4] {
    [srgb8(color[0]), srgb8(color[1]), srgb8(color[2]), 255]
}

/// Prepares a model for drawing. A model without normals gets one per face, which
/// means no two faces share a vertex any more.
pub(super) fn geometry(mesh: &MeshData) -> Option<Geometry> {
    let count = mesh.positions.len();
    let usable = |i: &u32| {
        mesh.positions
            .get(*i as usize)
            .is_some_and(|p| p.iter().all(|v| v.is_finite()))
    };
    let triangles: Vec<[u32; 3]> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| t.iter().all(usable))
        .copied()
        .collect();
    if triangles.is_empty() {
        return None;
    }
    let mut min = Vec3::INFINITY;
    let mut max = Vec3::NEG_INFINITY;
    for index in triangles.iter().flatten() {
        let p = Vec3::from_array(mesh.positions[*index as usize]);
        min = min.min(p);
        max = max.max(p);
    }
    let center = (min + max) * 0.5;
    // A vertex no triangle can use is parked at the centre, where it does no harm.
    let position = |i: u32| {
        let p = Vec3::from_array(mesh.positions[i as usize]);
        (if p.is_finite() {
            p - center
        } else {
            Vec3::ZERO
        })
        .to_array()
    };
    let color = |i: u32| {
        encode(
            mesh.colors
                .get(i as usize)
                .copied()
                .unwrap_or(DEFAULT_COLOR),
        )
    };

    let mut out = Geometry {
        origin: center.as_dvec3(),
        positions: Vec::new(),
        normals: Vec::new(),
        colors: Vec::new(),
        indices: Vec::new(),
        min: min.as_dvec3(),
        max: max.as_dvec3(),
    };
    if mesh.normals.len() == count {
        out.positions = (0..count as u32).map(position).collect();
        out.colors = (0..count as u32).map(color).collect();
        out.normals = mesh
            .normals
            .iter()
            .map(|n| Vec3::from_array(*n).normalize_or(Vec3::Y).to_array())
            .collect();
        // The document winds triangles counter-clockwise.
        out.indices = triangles
            .iter()
            .flat_map(|t| [t[0] as i32, t[2] as i32, t[1] as i32])
            .collect();
    } else {
        for triangle in &triangles {
            let [a, b, c] = triangle.map(|i| Vec3::from_array(mesh.positions[i as usize]));
            let normal = (b - a).cross(c - a).normalize_or(Vec3::Y).to_array();
            let first = out.positions.len() as i32;
            for index in triangle {
                out.positions.push(position(*index));
                out.colors.push(color(*index));
                out.normals.push(normal);
            }
            out.indices.extend([first, first + 2, first + 1]);
        }
    }
    Some(out)
}

/// The edges of a triangle list, each once, as pairs of vertex indices.
pub(super) fn edges(indices: &[i32]) -> Vec<i32> {
    let mut seen: HashSet<(i32, i32)> = HashSet::with_capacity(indices.len());
    let mut out = Vec::new();
    for triangle in indices.as_chunks::<3>().0 {
        for (a, b) in [(0, 1), (1, 2), (2, 0)] {
            let edge = (triangle[a].min(triangle[b]), triangle[a].max(triangle[b]));
            if edge.0 != edge.1 && seen.insert(edge) {
                out.extend([edge.0, edge.1]);
            }
        }
    }
    out
}

/// A model as the renderer holds it.
pub(super) struct BuiltModel {
    mesh: Gd<ArrayMesh>,
    /// The edges as lines, made the first time the wireframe is shown.
    wire: Option<Gd<ArrayMesh>>,
    geometry: Geometry,
}

impl BuiltModel {
    pub(super) fn bounds(&self) -> (DVec3, DVec3) {
        (self.geometry.min, self.geometry.max)
    }

    fn arrays(&self) -> (VarArray, PackedVector3Array, PackedColorArray) {
        let positions: PackedVector3Array = self
            .geometry
            .positions
            .iter()
            .map(|p| Vector3::new(p[0], p[1], p[2]))
            .collect();
        let colors: PackedColorArray = self
            .geometry
            .colors
            .iter()
            .map(|c| Color::from_rgba8(c[0], c[1], c[2], c[3]))
            .collect();
        let mut arrays = VarArray::new();
        arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
        (arrays, positions, colors)
    }

    fn wire(&mut self) -> &Gd<ArrayMesh> {
        if self.wire.is_none() {
            let (mut arrays, positions, colors) = self.arrays();
            let lines: PackedInt32Array = edges(&self.geometry.indices).into_iter().collect();
            arrays.set(ArrayType::VERTEX.ord() as usize, &positions.to_variant());
            arrays.set(ArrayType::COLOR.ord() as usize, &colors.to_variant());
            arrays.set(ArrayType::INDEX.ord() as usize, &lines.to_variant());
            let mut mesh = ArrayMesh::new_gd();
            mesh.add_surface_from_arrays(PrimitiveType::LINES, &arrays);
            self.wire = Some(mesh);
        }
        self.wire.as_ref().expect("made above")
    }
}

pub(super) fn build(media: &Media) -> Option<BuiltModel> {
    let MediaData::Mesh(data) = &media.data else {
        return None;
    };
    let geometry = geometry(data)?;
    let mut built = BuiltModel {
        mesh: ArrayMesh::new_gd(),
        wire: None,
        geometry,
    };
    let (mut arrays, positions, colors) = built.arrays();
    let normals: PackedVector3Array = built
        .geometry
        .normals
        .iter()
        .map(|n| Vector3::new(n[0], n[1], n[2]))
        .collect();
    // Models are drawn by the stroke shaders, which read a stroke's draw direction
    // and dither seed from here; a model has neither.
    let mut extra = PackedByteArray::new();
    extra.resize(built.geometry.positions.len() * 4);
    let indices: PackedInt32Array = built.geometry.indices.iter().copied().collect();
    arrays.set(ArrayType::VERTEX.ord() as usize, &positions.to_variant());
    arrays.set(ArrayType::NORMAL.ord() as usize, &normals.to_variant());
    arrays.set(ArrayType::COLOR.ord() as usize, &colors.to_variant());
    arrays.set(ArrayType::CUSTOM0.ord() as usize, &extra.to_variant());
    arrays.set(ArrayType::INDEX.ord() as usize, &indices.to_variant());
    built
        .mesh
        .add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
    Some(built)
}

/// What a model's material was chosen for; it is chosen again when any of it changes.
#[derive(Clone, Copy, PartialEq)]
struct Look {
    lit: bool,
    faded: bool,
    materials: u32,
}

pub(super) struct ModelNode {
    node: Gd<MeshInstance3D>,
    /// The edges, drawn over the surface. A child of `node`, so it moves with it.
    wire: Gd<MeshInstance3D>,
    media: usize,
    look: Option<Look>,
    opacity: f32,
    transform: Option<Transform3D>,
    visible: bool,
    wire_visible: bool,
    wire_built: bool,
}

impl ModelNode {
    pub(super) fn new(holder: &mut Gd<Node3D>, state: &LayerState) -> Self {
        let mut node = MeshInstance3D::new_alloc();
        node.set_name(&format!("Model_{}", state.id.0));
        node.set_visible(false);
        node.set_instance_shader_parameter("layer_seed", &layer_seed(state.id).to_variant());
        let mut wire = MeshInstance3D::new_alloc();
        wire.set_name("Wire");
        wire.set_visible(false);
        wire.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        node.add_child(&wire);
        holder.add_child(&node);
        Self {
            node,
            wire,
            media: 0,
            look: None,
            opacity: -1.0,
            transform: None,
            visible: false,
            wire_visible: false,
            wire_built: false,
        }
    }

    pub(super) fn sync(
        &mut self,
        model: &Model,
        media: Option<&Arc<Media>>,
        state: &LayerState,
        viewer_from_world: &DAffine3,
        models: &mut MediaCache<BuiltModel>,
        materials: &Materials,
    ) {
        let built = media
            .filter(|_| state.visible && state.opacity > 0.0)
            .and_then(|media| Some((media, models.get(media, build)?)));
        let Some((media, built)) = built else {
            self.set_visible(false);
            return;
        };
        let key = Arc::as_ptr(media) as usize;
        if self.media != key {
            self.node.set_mesh(&built.mesh);
            self.media = key;
            self.wire_built = false;
            self.transform = None;
        }
        let look = Look {
            lit: model.shading == Shading::Lit,
            // As with strokes, a faded layer is dithered, never blended.
            faded: state.opacity < 0.999,
            materials: materials.version,
        };
        if self.look != Some(look) {
            let material = &materials.model[usize::from(look.lit) * 2 + usize::from(look.faded)];
            match material {
                Some(material) => self.node.set_material_override(material),
                None => self.node.set_material_override(Gd::null_arg()),
            }
            match &materials.wire {
                Some(material) => self.wire.set_material_override(material),
                None => self.wire.set_material_override(Gd::null_arg()),
            }
            self.look = Some(look);
        }
        if self.opacity != state.opacity {
            let opacity = state.opacity.to_variant();
            self.node
                .set_instance_shader_parameter("layer_opacity", &opacity);
            self.wire.set_instance_shader_parameter("fade", &opacity);
            self.opacity = state.opacity;
        }
        let wire_visible = model.wireframe && materials.wire.is_some();
        if wire_visible && !self.wire_built {
            self.wire.set_mesh(built.wire());
            self.wire_built = true;
        }
        if self.wire_visible != wire_visible {
            self.wire.set_visible(wire_visible);
            self.wire_visible = wire_visible;
        }
        let place = *viewer_from_world * state.world;
        let transform = transform3d(&(place * DAffine3::from_translation(built.geometry.origin)));
        if self.transform != Some(transform) {
            self.node.set_transform(transform);
            self.transform = Some(transform);
        }
        self.set_visible(true);
    }

    fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.node.set_visible(visible);
            self.visible = visible;
        }
    }

    pub(super) fn free(self) {
        release(self.node.upcast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(z: f32) -> MeshData {
        MeshData {
            positions: vec![
                [10.0, 0.0, z],
                [12.0, 0.0, z],
                [12.0, 2.0, z],
                [10.0, 2.0, z],
            ],
            normals: Vec::new(),
            colors: Vec::new(),
            indices: vec![0, 1, 2, 0, 2, 3],
        }
    }

    #[test]
    fn a_model_without_normals_gets_one_per_face_and_mid_grey() {
        let geometry = geometry(&square(5.0)).unwrap();
        assert_eq!(
            geometry.positions.len(),
            6,
            "faces no longer share vertices"
        );
        assert_eq!(geometry.indices, [0, 2, 1, 3, 5, 4]);
        assert!(geometry.normals.iter().all(|n| *n == [0.0, 0.0, 1.0]));
        assert!(geometry.colors.iter().all(|c| *c == [188, 188, 188, 255]));
        assert_eq!(geometry.origin, DVec3::new(11.0, 1.0, 5.0));
        assert_eq!(geometry.positions[0], [-1.0, -1.0, 0.0]);
        assert_eq!(geometry.min, DVec3::new(10.0, 0.0, 5.0));
        assert_eq!(geometry.max, DVec3::new(12.0, 2.0, 5.0));
    }

    #[test]
    fn a_model_with_normals_and_colours_keeps_its_vertices() {
        let mut mesh = square(0.0);
        mesh.normals = vec![[0.0, 0.0, 2.0]; 4];
        mesh.colors = vec![[1.0, 0.0, 0.214]; 4];
        let geometry = geometry(&mesh).unwrap();
        assert_eq!(geometry.positions.len(), 4);
        assert_eq!(geometry.indices, [0, 2, 1, 0, 3, 2]);
        assert_eq!(geometry.normals[0], [0.0, 0.0, 1.0]);
        assert_eq!(geometry.colors[0], [255, 0, 127, 255]);
    }

    #[test]
    fn unusable_meshes_draw_nothing() {
        assert!(geometry(&MeshData::default()).is_none());
        let mut mesh = square(0.0);
        mesh.indices = vec![0, 1, 9];
        assert!(
            geometry(&mesh).is_none(),
            "the only triangle points nowhere"
        );
    }

    #[test]
    fn a_vertex_that_is_not_a_number_costs_only_the_faces_that_use_it() {
        let mut mesh = square(0.0);
        mesh.positions[1][0] = f32::NAN;
        mesh.normals = vec![[0.0, 0.0, 1.0]; 4];
        let geometry = geometry(&mesh).unwrap();
        assert_eq!(geometry.indices, [0, 3, 2]);
        assert!(geometry.positions.iter().flatten().all(|v| v.is_finite()));
        assert_eq!(geometry.min, DVec3::new(10.0, 0.0, 0.0));
    }

    #[test]
    fn shared_edges_are_drawn_once() {
        assert_eq!(edges(&[0, 2, 1, 0, 3, 2]).len(), 5 * 2);
        assert_eq!(
            edges(&[0, 1, 1]),
            [0, 1],
            "a collapsed triangle is one edge"
        );
    }
}
