//! Things drawn over the document that are not part of it: the frames before and
//! after the one being drawn on (onion skins), the highlight on selected strokes,
//! and the blink on the stroke the lower button would delete.

use crate::convert::transform3d;
use crate::meshes::{build_mesh, chunk_key, MeshCache};
use godot::classes::{Material, MeshInstance3D};
use godot::prelude::*;
use limner::core::eval::{Content, RenderFrame};
use limner::core::glam::{DAffine3, DVec3};
use limner::core::model::{Drawing, DrawingId, LayerId, Stroke};
use limner::core::session::{Session, StrokeAtHand};
use limner::core::tess::{tessellate_chunk, SurfaceKind};

/// Frames before the current one are tinted warm and frames after it cool.
const BEFORE: Color = Color::from_rgb(1.0, 0.32, 0.18);
const AFTER: Color = Color::from_rgb(0.2, 0.55, 1.0);
/// How much of a skin's own colour gives way to the tint.
const TINT: f32 = 0.75;
/// How solid the nearest skin is; each frame further away is fainter still.
const SKIN_OPACITY: f32 = 0.45;

/// The render layer of things that help while working and are not part of the
/// picture; cameras that take pictures do not see it.
const HELPER_LAYERS: u32 = 2;

/// One chunk of an onion skin on screen.
struct Skin {
    node: Gd<MeshInstance3D>,
    key: usize,
    tint: Option<Color>,
    opacity: f32,
    transform: Option<Transform3D>,
    visible: bool,
}

/// One chunk of the selected strokes of one drawing.
struct Highlight {
    node: Gd<MeshInstance3D>,
    layer: LayerId,
    origin: DVec3,
}

/// What highlight meshes were built from: the state of the selection, the state of
/// the document, and the drawings on screen.
type BuiltFrom = (u64, u64, Vec<(LayerId, DrawingId)>);

/// The stroke under the hand, lit up.
struct AtHand {
    node: Gd<MeshInstance3D>,
    origin: DVec3,
}

#[derive(Default)]
pub(super) struct Overlays {
    skins: Vec<Skin>,
    highlights: Vec<Highlight>,
    built_from: Option<BuiltFrom>,
    highlight_material: Option<Gd<Material>>,
    at_hand: Option<AtHand>,
    /// The stroke the blink was built from, and the state of the document then.
    at_hand_from: Option<(StrokeAtHand, u64)>,
    at_hand_material: Option<Gd<Material>>,
}

impl Overlays {
    pub(super) fn set_highlight_material(&mut self, material: Gd<Material>) {
        self.highlight_material = Some(material);
        self.built_from = None;
    }

    pub(super) fn set_at_hand_material(&mut self, material: Gd<Material>) {
        self.at_hand_material = Some(material);
        self.at_hand_from = None;
    }

    pub(super) fn sync(
        &mut self,
        parent: &mut Gd<Node3D>,
        session: &Session,
        frame: &RenderFrame,
        viewer_from_world: &DAffine3,
        cache: &mut MeshCache,
        materials: &[Option<Gd<Material>>; 4],
    ) {
        self.sync_skins(parent, session, frame, viewer_from_world, cache, materials);
        self.sync_highlights(parent, session, frame, viewer_from_world);
        self.sync_at_hand(parent, session, frame, viewer_from_world);
    }

    fn sync_skins(
        &mut self,
        parent: &mut Gd<Node3D>,
        session: &Session,
        frame: &RenderFrame,
        viewer_from_world: &DAffine3,
        cache: &mut MeshCache,
        materials: &[Option<Gd<Material>>; 4],
    ) {
        let mut used = 0;
        for skin in session.onion_skins() {
            let (Some(state), Some(drawing)) =
                (frame.get(skin.layer), session.doc().drawing(skin.drawing))
            else {
                continue;
            };
            if !state.visible {
                continue;
            }
            let matrix = *viewer_from_world * state.world;
            let mut tint = if skin.offset < 0 { BEFORE } else { AFTER };
            tint.a = TINT;
            let opacity = SKIN_OPACITY / skin.offset.unsigned_abs().max(1) as f32 * state.opacity;
            for chunk in drawing.chunks() {
                let key = chunk_key(chunk);
                let Some(built) = cache.get(chunk) else {
                    continue;
                };
                if used == self.skins.len() {
                    let mut node = MeshInstance3D::new_alloc();
                    node.set_visible(false);
                    node.set_name("OnionSkin");
                    node.set_layer_mask(HELPER_LAYERS);
                    parent.add_child(&node);
                    self.skins.push(Skin {
                        node,
                        key: 0,
                        tint: None,
                        opacity: -1.0,
                        transform: None,
                        visible: false,
                    });
                }
                let instance = &mut self.skins[used];
                used += 1;
                if instance.key != key {
                    instance.node.set_mesh(&built.mesh);
                    // A skin is always seen through, whatever its strokes are.
                    for (surface, kind) in built.kinds.iter().enumerate() {
                        let kind = if kind.is_flat() {
                            SurfaceKind::FlatSeeThrough
                        } else {
                            SurfaceKind::TubeSeeThrough
                        };
                        if let Some(material) = &materials[kind as usize] {
                            instance
                                .node
                                .set_surface_override_material(surface as i32, material);
                        }
                    }
                    instance.key = key;
                }
                if instance.tint != Some(tint) {
                    instance
                        .node
                        .set_instance_shader_parameter("layer_tint", &tint.to_variant());
                    instance.tint = Some(tint);
                }
                if instance.opacity != opacity {
                    instance
                        .node
                        .set_instance_shader_parameter("layer_opacity", &opacity.to_variant());
                    instance.opacity = opacity;
                }
                let transform = transform3d(&(matrix * DAffine3::from_translation(built.origin)));
                if instance.transform != Some(transform) {
                    instance.node.set_transform(transform);
                    instance.transform = Some(transform);
                }
                if !instance.visible {
                    instance.node.set_visible(true);
                    instance.visible = true;
                }
            }
        }
        for instance in self.skins.iter_mut().skip(used) {
            if instance.visible {
                instance.node.set_visible(false);
                instance.visible = false;
                // Let go of the mesh so that the cache can drop it.
                instance
                    .node
                    .set_mesh(Gd::<godot::classes::Mesh>::null_arg());
                instance.key = 0;
            }
        }
    }

    fn sync_highlights(
        &mut self,
        parent: &mut Gd<Node3D>,
        session: &Session,
        frame: &RenderFrame,
        viewer_from_world: &DAffine3,
    ) {
        let doc = session.doc();
        let selection = session.selection();
        // Only strokes on the frames that are showing are lit up.
        let showing: Vec<(LayerId, DrawingId)> = selection
            .drawings()
            .filter_map(|(drawing, _)| {
                let layer = doc.layer_of_drawing(drawing)?;
                let state = frame.get(layer)?;
                (state.visible
                    && matches!(state.content, Content::Paint { drawing: shown, .. } if shown == drawing))
                .then_some((layer, drawing))
            })
            .collect();
        let from = (selection.revision(), session.edit_count(), showing);
        if self.built_from.as_ref() != Some(&from) {
            for highlight in self.highlights.drain(..) {
                highlight.node.clone().queue_free();
            }
            for (layer, drawing_id) in &from.2 {
                let (Some(drawing), Some(ids)) =
                    (doc.drawing(*drawing_id), selection.in_drawing(*drawing_id))
                else {
                    continue;
                };
                let selected: Vec<Stroke> = drawing
                    .strokes()
                    .filter(|s| ids.contains(&s.id))
                    .map(|s| (**s).clone())
                    .collect();
                for chunk in Drawing::from_strokes(selected).chunks() {
                    let Some(built) = build_mesh(&tessellate_chunk(chunk)) else {
                        continue;
                    };
                    let mut node = MeshInstance3D::new_alloc();
                    node.set_name("Selected");
                    node.set_layer_mask(HELPER_LAYERS);
                    node.set_mesh(&built.mesh);
                    if let Some(material) = &self.highlight_material {
                        for surface in 0..built.kinds.len() {
                            node.set_surface_override_material(surface as i32, material);
                        }
                    }
                    parent.add_child(&node);
                    self.highlights.push(Highlight {
                        node,
                        layer: *layer,
                        origin: built.origin,
                    });
                }
            }
            self.built_from = Some(from);
        }
        for highlight in &mut self.highlights {
            let Some(state) = frame.get(highlight.layer) else {
                continue;
            };
            let matrix =
                *viewer_from_world * state.world * DAffine3::from_translation(highlight.origin);
            highlight.node.set_transform(transform3d(&matrix));
        }
    }

    /// Shows the stroke the lower button would delete, so that it is seen before
    /// the button is pressed.
    fn sync_at_hand(
        &mut self,
        parent: &mut Gd<Node3D>,
        session: &Session,
        frame: &RenderFrame,
        viewer_from_world: &DAffine3,
    ) {
        let stroke = session.stroke_at_hand();
        let from = stroke.map(|stroke| (stroke, session.edit_count()));
        if self.at_hand_from != from {
            self.at_hand_from = from;
            let built = stroke.and_then(|at| {
                let drawing = session.doc().drawing(at.drawing)?;
                let stroke = drawing.strokes().find(|s| s.id == at.stroke)?;
                let alone = Drawing::from_strokes([(**stroke).clone()]);
                build_mesh(&tessellate_chunk(alone.chunks().first()?))
            });
            match (built, &mut self.at_hand) {
                (Some(built), shown) => {
                    let at_hand = shown.get_or_insert_with(|| {
                        let mut node = MeshInstance3D::new_alloc();
                        node.set_name("AtHand");
                        node.set_layer_mask(HELPER_LAYERS);
                        parent.add_child(&node);
                        AtHand {
                            node,
                            origin: DVec3::ZERO,
                        }
                    });
                    at_hand.node.set_mesh(&built.mesh);
                    if let Some(material) = &self.at_hand_material {
                        for surface in 0..built.kinds.len() {
                            at_hand
                                .node
                                .set_surface_override_material(surface as i32, material);
                        }
                    }
                    at_hand.origin = built.origin;
                    at_hand.node.set_visible(true);
                }
                (None, Some(at_hand)) => at_hand.node.set_visible(false),
                (None, None) => {}
            }
        }
        if let (Some(at_hand), Some((stroke, _))) = (&mut self.at_hand, self.at_hand_from) {
            if let Some(state) = frame.get(stroke.layer) {
                let matrix =
                    *viewer_from_world * state.world * DAffine3::from_translation(at_hand.origin);
                at_hand.node.set_transform(transform3d(&matrix));
            }
        }
    }

    pub(super) fn clear(&mut self) {
        for skin in self.skins.drain(..) {
            skin.node.clone().queue_free();
        }
        for highlight in self.highlights.drain(..) {
            highlight.node.clone().queue_free();
        }
        if let Some(at_hand) = self.at_hand.take() {
            at_hand.node.clone().queue_free();
        }
        self.built_from = None;
        self.at_hand_from = None;
    }
}
