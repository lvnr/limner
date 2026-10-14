//! Gizmos: the marks that show where spawn areas, cameras, lights and sounds are, and
//! the box around a layer whose bounds are shown.
//!
//! A gizmo is drawn in viewer space at a fixed size, whatever the scale of the layer
//! or the view, so it can always be found and never fills the room. Only the parts
//! that measure something (a sound's reach, a spawn area's eye height) follow the scale.

use super::nodes::{release, Placement};
use crate::convert::{color_from_linear, transform3d};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::{ArrayMesh, Material, MeshInstance3D};
use godot::prelude::*;
use limner::core::glam::{DAffine3, DVec3};
use limner::core::model::{AttenuationMode, Camera, LayerId, Light, LightKind, Sound, Viewpoint};
use std::f32::consts::TAU;

/// How large a gizmo is, in metres of viewer space.
pub(super) const GIZMO_SIZE: f64 = 0.08;
/// The eye height of a standing viewer, in metres at the viewer's own scale.
const EYE_HEIGHT: f64 = 1.6;
/// The render layer gizmos are on (the twentieth), so that a camera rendering the
/// finished piece can leave them out with its cull mask.
pub(super) const GIZMO_RENDER_LAYER: u32 = 1 << 19;

const CAMERA_COLOR: [f32; 3] = [0.9, 0.45, 0.05];
const SOUND_COLOR: [f32; 3] = [0.1, 0.55, 0.2];
const BOUNDS_COLOR: [f32; 3] = [0.15, 0.4, 0.9];

/// The line drawings gizmos are put together from, each in its own unit measure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Shape {
    /// A ring of radius one on the ground with an arrow pointing ahead (-Z).
    Ring,
    /// A smaller double ring that marks the spawn area a viewer starts at.
    DefaultMark,
    /// A line from the origin one unit up.
    Pole,
    /// A pyramid from the origin to a square of half-side one, one unit ahead, with
    /// a small triangle marking its top.
    Frustum,
    /// A ball with rays.
    Sun,
    /// A cone from the origin to a circle of radius one, one unit ahead.
    Cone,
    /// An arrow pointing ahead from a small disc.
    Arrow,
    /// A horn opening ahead.
    Speaker,
    /// Three great circles of radius one.
    Sphere,
    /// The edges of a cube of side one centred on the origin.
    Box,
}

const SHAPE_COUNT: usize = 10;

fn circle(out: &mut Vec<[f32; 3]>, center: Vec3, u: Vec3, v: Vec3, radius: f32, segments: u32) {
    let at = |i: u32| {
        let angle = i as f32 / segments as f32 * TAU;
        (center + (u * angle.cos() + v * angle.sin()) * radius).to_array()
    };
    for i in 0..segments {
        out.extend([at(i), at(i + 1)]);
    }
}

type Vec3 = limner::core::glam::Vec3;

impl Shape {
    /// The shape as a list of line segments: two points each.
    pub(super) fn lines(self) -> Vec<[f32; 3]> {
        let mut out: Vec<[f32; 3]> = Vec::new();
        let (x, y, z) = (Vec3::X, Vec3::Y, Vec3::Z);
        match self {
            Shape::Ring => {
                circle(&mut out, Vec3::ZERO, x, z, 1.0, 40);
                out.extend([[0.0, 0.0, -1.0], [0.0, 0.0, -1.7]]);
                out.extend([[0.0, 0.0, -1.7], [0.25, 0.0, -1.35]]);
                out.extend([[0.0, 0.0, -1.7], [-0.25, 0.0, -1.35]]);
            }
            Shape::DefaultMark => {
                circle(&mut out, Vec3::ZERO, x, z, 0.7, 32);
                circle(&mut out, Vec3::ZERO, x, z, 0.4, 24);
            }
            Shape::Pole => out.extend([[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            Shape::Frustum => {
                let corners = [
                    [-1.0, -1.0, -1.0],
                    [1.0, -1.0, -1.0],
                    [1.0, 1.0, -1.0],
                    [-1.0, 1.0, -1.0],
                ];
                for (i, corner) in corners.iter().enumerate() {
                    out.extend([[0.0; 3], *corner]);
                    out.extend([*corner, corners[(i + 1) % 4]]);
                }
                let mark = [[-0.35, 1.1, -1.0], [0.35, 1.1, -1.0], [0.0, 1.55, -1.0]];
                for i in 0..3 {
                    out.extend([mark[i], mark[(i + 1) % 3]]);
                }
            }
            Shape::Sun => {
                circle(&mut out, Vec3::ZERO, x, y, 0.5, 20);
                circle(&mut out, Vec3::ZERO, y, z, 0.5, 20);
                circle(&mut out, Vec3::ZERO, x, z, 0.5, 20);
                for axis in [x, y, z] {
                    for sign in [-1.0, 1.0] {
                        out.extend([(axis * 0.7 * sign).to_array(), (axis * sign).to_array()]);
                    }
                }
            }
            Shape::Cone => {
                let base = Vec3::new(0.0, 0.0, -1.0);
                circle(&mut out, base, x, y, 1.0, 32);
                for corner in [x, y, -x, -y] {
                    out.extend([[0.0; 3], (base + corner).to_array()]);
                }
            }
            Shape::Arrow => {
                circle(&mut out, Vec3::new(0.0, 0.0, 0.5), x, y, 0.3, 16);
                out.extend([[0.0, 0.0, 0.5], [0.0, 0.0, -1.0]]);
                for side in [x, y, -x, -y] {
                    out.extend([[0.0, 0.0, -1.0], (side * 0.22 - z * 0.6).to_array()]);
                }
            }
            Shape::Speaker => {
                let square = |half: f32, depth: f32| {
                    [
                        [-half, -half, depth],
                        [half, -half, depth],
                        [half, half, depth],
                        [-half, half, depth],
                    ]
                };
                let (back, throat, mouth) = (square(0.3, 0.7), square(0.3, 0.2), square(0.8, -0.5));
                for i in 0..4 {
                    let next = (i + 1) % 4;
                    out.extend([back[i], back[next]]);
                    out.extend([throat[i], throat[next]]);
                    out.extend([mouth[i], mouth[next]]);
                    out.extend([back[i], throat[i]]);
                    out.extend([throat[i], mouth[i]]);
                }
            }
            Shape::Sphere => {
                circle(&mut out, Vec3::ZERO, x, y, 1.0, 48);
                circle(&mut out, Vec3::ZERO, y, z, 1.0, 48);
                circle(&mut out, Vec3::ZERO, x, z, 1.0, 48);
            }
            Shape::Box => {
                let corner = |i: usize| {
                    [
                        if i & 1 == 0 { -0.5 } else { 0.5 },
                        if i & 2 == 0 { -0.5 } else { 0.5 },
                        if i & 4 == 0 { -0.5 } else { 0.5 },
                    ]
                };
                for i in 0..8 {
                    for bit in [1, 2, 4] {
                        if i & bit == 0 {
                            out.extend([corner(i), corner(i | bit)]);
                        }
                    }
                }
            }
        }
        out
    }
}

/// The meshes of the shapes, each built the first time it is drawn.
#[derive(Default)]
pub(super) struct Shapes {
    meshes: [Option<Gd<ArrayMesh>>; SHAPE_COUNT],
}

impl Shapes {
    fn mesh(&mut self, shape: Shape) -> &Gd<ArrayMesh> {
        self.meshes[shape as usize].get_or_insert_with(|| {
            let points: PackedVector3Array = shape
                .lines()
                .iter()
                .map(|p| Vector3::new(p[0], p[1], p[2]))
                .collect();
            let mut arrays = VarArray::new();
            arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
            arrays.set(ArrayType::VERTEX.ord() as usize, &points.to_variant());
            let mut mesh = ArrayMesh::new_gd();
            mesh.add_surface_from_arrays(PrimitiveType::LINES, &arrays);
            mesh
        })
    }
}

/// One line drawing to show: which, where and in what colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Part {
    pub shape: Shape,
    pub transform: Transform3D,
    pub color: Color,
}

struct PartNode {
    node: Gd<MeshInstance3D>,
    shown: Option<Part>,
    visible: bool,
}

/// The nodes that draw one layer's gizmo.
#[derive(Default)]
pub(super) struct Parts {
    nodes: Vec<PartNode>,
}

impl Parts {
    /// Shows exactly `wanted`, reusing the nodes there are.
    pub(super) fn sync(
        &mut self,
        wanted: &[Part],
        shapes: &mut Shapes,
        material: Option<&Gd<Material>>,
        holder: &mut Gd<Node3D>,
        id: LayerId,
    ) {
        // Without their material lines would be drawn lit and white; better not at all.
        let wanted = if material.is_some() { wanted } else { &[] };
        while self.nodes.len() < wanted.len() {
            let mut node = MeshInstance3D::new_alloc();
            node.set_name(&format!("Gizmo_{}_{}", id.0, self.nodes.len()));
            node.set_visible(false);
            node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            node.set_layer_mask(GIZMO_RENDER_LAYER);
            if let Some(material) = material {
                node.set_material_override(material);
            }
            holder.add_child(&node);
            self.nodes.push(PartNode {
                node,
                shown: None,
                visible: false,
            });
        }
        for (slot, part) in self.nodes.iter_mut().zip(wanted) {
            let before = slot.shown;
            if before.map(|b| b.shape) != Some(part.shape) {
                slot.node.set_mesh(shapes.mesh(part.shape));
            }
            if before.map(|b| b.transform) != Some(part.transform) {
                slot.node.set_transform(part.transform);
            }
            if before.map(|b| b.color) != Some(part.color) {
                slot.node
                    .set_instance_shader_parameter("line_color", &part.color.to_variant());
            }
            slot.shown = Some(*part);
            if !slot.visible {
                slot.node.set_visible(true);
                slot.visible = true;
            }
        }
        for slot in self.nodes.iter_mut().skip(wanted.len()) {
            if slot.visible {
                slot.node.set_visible(false);
                slot.visible = false;
            }
        }
    }

    pub(super) fn free(self) {
        for slot in self.nodes {
            release(slot.node.upcast());
        }
    }
}

/// A shape at a layer's place, turned as the layer is, `size` metres to the unit.
fn sized(place: &Placement, shape: Shape, size: DVec3, color: Color) -> Part {
    Part {
        shape,
        transform: transform3d(&(place.rigid * DAffine3::from_scale(size))),
        color,
    }
}

/// The largest size that keeps a shape whose unit extents are `extents` within the gizmo size.
fn fitted(extents: DVec3) -> DVec3 {
    extents * (GIZMO_SIZE / extents.max_element().max(1e-9))
}

/// A spawn area: the ring a viewer stands in, with an arrow the way they face. One
/// that maps to the floor also gets a pole as tall as the viewer's eyes, at the
/// scale the viewer would be. The spawn area a viewer starts at is double-ringed.
pub(super) fn viewpoint(
    out: &mut Vec<Part>,
    place: &Placement,
    viewpoint: &Viewpoint,
    is_default: bool,
) {
    let color = color_from_linear(viewpoint.color);
    let size = DVec3::splat(GIZMO_SIZE);
    out.push(sized(place, Shape::Ring, size, color));
    if is_default {
        out.push(sized(place, Shape::DefaultMark, size, color));
    }
    if viewpoint.floor_level {
        let height = EYE_HEIGHT * place.scale;
        out.push(sized(
            place,
            Shape::Pole,
            DVec3::new(1.0, height, 1.0),
            color,
        ));
        let head = place.rigid * DAffine3::from_translation(DVec3::Y * height);
        out.push(Part {
            shape: Shape::Sphere,
            transform: transform3d(&(head * DAffine3::from_scale(DVec3::splat(GIZMO_SIZE * 0.3)))),
            color,
        });
    } else {
        // At eye level the layer's own origin is where the eyes are.
        out.push(sized(
            place,
            Shape::Sphere,
            DVec3::splat(GIZMO_SIZE * 0.3),
            color,
        ));
    }
    if viewpoint.show_volume {
        let [x, y, z, radius] = viewpoint.volume.map(f64::from);
        let center = place.rigid * DAffine3::from_translation(DVec3::new(x, y, z) * place.scale);
        let radius = (radius * place.scale).max(1e-9);
        out.push(Part {
            shape: Shape::Sphere,
            transform: transform3d(&(center * DAffine3::from_scale(DVec3::splat(radius)))),
            color,
        });
    }
}

/// A camera: the pyramid of what it sees, as wide and as tall as its picture.
pub(super) fn camera(out: &mut Vec<Part>, place: &Placement, camera: &Camera, aspect: f64) {
    let half_height = (f64::from(camera.fov).to_radians() * 0.5).tan();
    let extents = DVec3::new(half_height * aspect, half_height, 1.0);
    out.push(sized(
        place,
        Shape::Frustum,
        fitted(extents),
        color_from_linear(CAMERA_COLOR),
    ));
}

/// A light, in its own colour: a sun for a point light, a cone as wide as a spot
/// light's, an arrow for a directional light.
pub(super) fn light(out: &mut Vec<Part>, place: &Placement, light: &Light) {
    let color = color_from_linear(light.color);
    let (shape, size) = match light.kind {
        LightKind::Point => (Shape::Sun, DVec3::splat(GIZMO_SIZE * 0.5)),
        LightKind::Spot => {
            let radius = f64::from(light.spot_angle).to_radians().tan();
            (Shape::Cone, fitted(DVec3::new(radius, radius, 1.0)))
        }
        LightKind::Directional => (Shape::Arrow, DVec3::splat(GIZMO_SIZE)),
    };
    out.push(sized(place, shape, size, color));
}

/// A sound: a horn pointing the way the sound faces, and, when the sound fades with
/// distance, the sphere it is at full volume in and the sphere it is silent beyond.
pub(super) fn sound(out: &mut Vec<Part>, place: &Placement, sound: &Sound) {
    let color = color_from_linear(SOUND_COLOR);
    out.push(sized(
        place,
        Shape::Speaker,
        DVec3::splat(GIZMO_SIZE * 0.5),
        color,
    ));
    if sound.attenuation.mode != AttenuationMode::None {
        for radius in [sound.attenuation.min, sound.attenuation.max] {
            let radius = (f64::from(radius) * place.scale).max(1e-9);
            out.push(sized(place, Shape::Sphere, DVec3::splat(radius), color));
        }
    }
}

/// The box around a layer's content, given in the layer's own space.
pub(super) fn bounds(out: &mut Vec<Part>, layer_matrix: &DAffine3, min: DVec3, max: DVec3) {
    let size = max - min;
    // A flat thing still gets a box with some thickness, so the transform stays usable.
    let size = size.max(DVec3::splat(size.max_element() * 1e-4).max(DVec3::splat(1e-12)));
    let matrix =
        *layer_matrix * DAffine3::from_translation((min + max) * 0.5) * DAffine3::from_scale(size);
    out.push(Part {
        shape: Shape::Box,
        transform: transform3d(&matrix),
        color: color_from_linear(BOUNDS_COLOR),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Shape; SHAPE_COUNT] = [
        Shape::Ring,
        Shape::DefaultMark,
        Shape::Pole,
        Shape::Frustum,
        Shape::Sun,
        Shape::Cone,
        Shape::Arrow,
        Shape::Speaker,
        Shape::Sphere,
        Shape::Box,
    ];

    #[test]
    fn every_shape_is_a_list_of_whole_segments_of_finite_points() {
        for (index, shape) in ALL.iter().enumerate() {
            assert_eq!(*shape as usize, index);
            let lines = shape.lines();
            assert!(!lines.is_empty() && lines.len() % 2 == 0, "{shape:?}");
            assert!(lines.iter().flatten().all(|v| v.is_finite()), "{shape:?}");
        }
        assert_eq!(Shape::Box.lines().len(), 24, "a cube has twelve edges");
        assert_eq!(Shape::Pole.lines(), [[0.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    }

    #[test]
    fn shapes_that_point_somewhere_point_ahead() {
        for shape in [Shape::Ring, Shape::Frustum, Shape::Cone, Shape::Arrow] {
            let lines = shape.lines();
            let front = lines.iter().map(|p| p[2]).fold(f32::INFINITY, f32::min);
            let back = lines.iter().map(|p| p[2]).fold(f32::NEG_INFINITY, f32::max);
            assert!(-front > back, "{shape:?} reaches further along -Z");
        }
    }

    #[test]
    fn a_fitted_shape_is_never_larger_than_a_gizmo() {
        for extents in [
            DVec3::new(0.2, 0.1, 1.0),
            DVec3::new(20.0, 11.0, 1.0),
            DVec3::new(1.0, 1.0, 1.0),
        ] {
            let size = fitted(extents);
            assert!((size.max_element() - GIZMO_SIZE).abs() < 1e-12);
            assert!((size.x / size.y - extents.x / extents.y).abs() < 1e-9);
        }
    }
}
