//! Things drawn over the document that are not part of it: the frames before and
//! after the one being drawn on (onion skins), and the highlight on selected strokes.

use crate::convert::transform3d;
use crate::meshes::{build_mesh, chunk_key, MeshCache};
use godot::classes::{Material, MeshInstance3D};
use godot::prelude::*;
use limner::core::eval::{Content, RenderFrame};
use limner::core::glam::{DAffine3, DVec3};
use limner::core::model::{Drawing, DrawingId, LayerId, Stroke};
use limner::core::session::Session;
use limner::core::tess::{tessellate_chunk, SurfaceKind};

/// Frames before the current one are tinted warm and frames after it cool.
const BEFORE: Color = Color::from_rgb(1.0, 0.32, 0.18);
const AFTER: Color = Color::from_rgb(0.2, 0.55, 1.0);
/// How much of a skin's own colour gives way to the tint.
const TINT: f32 = 0.75;
/// How solid the nearest skin is; each frame further away is fainter still.
const SKIN_OPACITY: f32 = 0.45;

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

#[derive(Default)]
pub(super) struct Overlays {
    skins: Vec<Skin>,
    highlights: Vec<Highlight>,
    built_from: Option<BuiltFrom>,
    highlight_material: Option<Gd<Material>>,
}

impl Overlays {
    pub(super) fn set_highlight_material(&mut self, material: Gd<Material>) {
        self.highlight_material = Some(material);
        self.built_from = None;
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

    pub(super) fn clear(&mut self) {
        for skin in self.skins.drain(..) {
            skin.node.clone().queue_free();
        }
        for highlight in self.highlights.drain(..) {
            highlight.node.clone().queue_free();
        }
        self.built_from = None;
    }
}
