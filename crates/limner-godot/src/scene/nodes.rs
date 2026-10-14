//! Layers that are not paint: pictures, models, sounds, spawn areas, cameras and lights.
//!
//! Each frame the nodes are brought in line with the evaluated document: made when a
//! layer appears, changed when its settings change, hidden while it is not shown and
//! freed when it is gone. Textures, meshes and audio streams are made once per media
//! item and found again by the item's identity, as stroke meshes are.

use super::gizmos::{self, Part, Parts, Shapes};
use super::lights::LightNode;
use super::models::{self, BuiltModel, ModelNode};
use super::pictures::{self, PictureMeshes, PictureNode, Texture};
use super::sounds::{self, Hearing, SoundNode, Stream};
use crate::convert::dvec3;
use godot::classes::{Material, ShaderMaterial};
use godot::prelude::*;
use limner::core::eval::RenderFrame;
use limner::core::glam::{DAffine3, DMat3, DVec3};
use limner::core::model::{Document, LayerId, LayerKind, Media, MediaId, SoundKind};
use limner::core::ops::layers::layer_bounds;
use limner::core::session::Session;
use std::collections::HashMap;
use std::sync::Arc;

/// A layer's place in viewer space with its size taken out. Gizmos, lights and
/// sounds sit where a layer is and face the way it faces, but are not scaled by it.
pub(super) struct Placement {
    /// Position and facing, without scale or mirroring.
    pub rigid: DAffine3,
    /// Metres of viewer space per layer unit.
    pub scale: f64,
}

pub(super) fn placement(matrix: &DAffine3) -> Placement {
    let m = &matrix.matrix3;
    // The layer's -Z is the way lights shine and cameras look, so Z is kept exactly.
    let z = m.z_axis.try_normalize().unwrap_or(DVec3::Z);
    let y = (m.y_axis - z * m.y_axis.dot(z))
        .try_normalize()
        .unwrap_or_else(|| z.any_orthonormal_vector());
    let x = y.cross(z);
    let scale = m.determinant().abs().cbrt();
    Placement {
        rigid: DAffine3::from_mat3_translation(DMat3::from_cols(x, y, z), matrix.translation),
        scale: if scale.is_finite() { scale } else { 1.0 },
    }
}

/// Takes a node out of the scene now and frees it once the frame is done. Leaving at
/// once matters: a layer of the next document may be given the same id, and with it
/// the same node name, before the frame ends.
pub(super) fn release(mut node: Gd<Node>) {
    if let Some(mut parent) = node.get_parent() {
        parent.remove_child(&node);
    }
    node.queue_free();
}

/// The materials the interface made for these layers.
#[derive(Default)]
pub(super) struct Materials {
    /// Templates, copied once per picture: flat and sphere, each opaque and see-through.
    pub picture: [Option<Gd<ShaderMaterial>>; 4],
    /// Unlit and lit, each for full and faded layers.
    pub model: [Option<Gd<Material>>; 4],
    /// Gizmos and bounding boxes.
    pub line: Option<Gd<Material>>,
    /// The edges drawn over a model.
    pub wire: Option<Gd<Material>>,
    /// Counts each time the materials are set, so nodes take the new ones.
    pub version: u32,
    /// Where the moving dither pattern is this frame.
    pub dither_frame: i32,
}

struct CacheEntry<T> {
    /// Keeps the media item alive so its address cannot be reused while it is a key.
    media: Arc<Media>,
    /// `None` when the item cannot be shown or played.
    built: Option<T>,
    used: u64,
}

/// What was built from media items (textures, meshes, audio streams), keyed by the
/// identity of the item. Versions of a document share their media, so an item is
/// decoded once and stays for as long as any version still holds it.
pub(super) struct MediaCache<T> {
    entries: HashMap<usize, CacheEntry<T>>,
    frame: u64,
}

impl<T> Default for MediaCache<T> {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            frame: 0,
        }
    }
}

impl<T> MediaCache<T> {
    fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// What `build` makes of `media`, made on first use.
    pub(super) fn get(
        &mut self,
        media: &Arc<Media>,
        build: impl FnOnce(&Media) -> Option<T>,
    ) -> Option<&mut T> {
        let frame = self.frame;
        let entry = self
            .entries
            .entry(Arc::as_ptr(media) as usize)
            .or_insert_with(|| CacheEntry {
                media: media.clone(),
                built: build(media),
                used: frame,
            });
        entry.used = frame;
        entry.built.as_mut()
    }

    /// Drops what was built from items that nothing else holds any more.
    fn end_frame(&mut self) {
        let frame = self.frame;
        self.entries
            .retain(|_, e| e.used == frame || Arc::strong_count(&e.media) > 1);
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

/// What shows or plays a layer's content.
enum Content {
    /// The layer is only marked: a spawn area, a camera, or bounds around something else.
    None,
    Picture(PictureNode),
    Model(ModelNode),
    Sound(SoundNode),
    Light(LightNode),
}

impl Content {
    fn free(self) {
        match self {
            Content::None => {}
            Content::Picture(node) => node.free(),
            Content::Model(node) => node.free(),
            Content::Sound(node) => node.free(),
            Content::Light(node) => node.free(),
        }
    }
}

struct Entry {
    content: Content,
    gizmo: Parts,
    seen: u64,
}

impl Entry {
    /// Takes what shows the layer's content away, to free it before its successor is
    /// made: the two would otherwise be in the scene under one name for a moment.
    fn take_content(&mut self) -> Content {
        std::mem::replace(&mut self.content, Content::None)
    }

    fn free(self) {
        self.content.free();
        self.gizmo.free();
    }
}

/// The nodes that show the non-paint layers of the document.
#[derive(Default)]
pub(super) struct LayerNodes {
    /// The parent of every node made here, created on first use.
    holder: Option<Gd<Node3D>>,
    materials: Materials,
    entries: HashMap<LayerId, Entry>,
    textures: MediaCache<Texture>,
    models: MediaCache<BuiltModel>,
    streams: MediaCache<Stream>,
    picture_meshes: PictureMeshes,
    shapes: Shapes,
    /// The gizmo parts of the layer at hand; kept to reuse its memory.
    parts: Vec<Part>,
    frame: u64,
    warned_about_ambisonic: bool,
}

/// True when every layer above `id` is inside one of its clips.
fn ancestors_in_clip(doc: &Document, frame: &RenderFrame, id: LayerId) -> bool {
    doc.ancestors(id)
        .all(|a| frame.get(a).is_none_or(|s| s.in_clip))
}

impl LayerNodes {
    /// Takes the materials the interface made, by name: `picture_flat_opaque`,
    /// `picture_flat_see_through`, `picture_sphere_opaque`, `picture_sphere_see_through`,
    /// `model_opaque`, `model_see_through`, `model_opaque_lit`, `model_see_through_lit`,
    /// `line` and `wire`.
    pub(super) fn set_materials(&mut self, materials: &VarDictionary) {
        let named = |name: &str| -> Option<Gd<Material>> { materials.get(name)?.try_to().ok() };
        let shader = |name: &str| named(name)?.try_cast::<ShaderMaterial>().ok();
        self.materials = Materials {
            picture: [
                shader("picture_flat_opaque"),
                shader("picture_flat_see_through"),
                shader("picture_sphere_opaque"),
                shader("picture_sphere_see_through"),
            ],
            model: [
                named("model_opaque"),
                named("model_see_through"),
                named("model_opaque_lit"),
                named("model_see_through_lit"),
            ],
            line: named("line"),
            wire: named("wire"),
            version: self.materials.version + 1,
            dither_frame: 0,
        };
    }

    /// Brings the nodes in line with the document at the evaluated moment.
    pub(super) fn sync(
        &mut self,
        parent: &mut Gd<Node3D>,
        session: &Session,
        frame: &RenderFrame,
        viewer_from_world: &DAffine3,
    ) {
        self.frame += 1;
        self.textures.begin_frame();
        self.models.begin_frame();
        self.streams.begin_frame();
        let doc = session.doc();
        let show_extras = session.options.show_extras;
        // The see-through picture template carries the dither pattern's place this frame.
        self.materials.dither_frame = self.materials.picture[1]
            .as_ref()
            .and_then(|m| m.get_shader_parameter("frame_index").try_to().ok())
            .unwrap_or(0);
        let hearing = Hearing {
            playing: session.playback.playing,
            mute: session.options.mute,
            listener: parent
                .get_viewport()
                .and_then(|viewport| viewport.get_camera_3d())
                .map_or(DVec3::ZERO, |camera| dvec3(camera.get_global_position())),
        };
        let resolution = doc.settings.camera_resolution;
        let camera_aspect = f64::from(resolution[0].max(1)) / f64::from(resolution[1].max(1));

        for state in &frame.layers {
            let Some(layer) = doc.layer(state.id) else {
                continue;
            };
            let has_content = !matches!(&layer.kind, LayerKind::Paint(_) | LayerKind::Group(_));
            if !has_content && !layer.show_bounds {
                if let Some(entry) = self.entries.remove(&state.id) {
                    entry.free();
                }
                continue;
            }
            let holder = self.holder.get_or_insert_with(|| {
                let mut node = Node3D::new_alloc();
                node.set_name("Layers");
                parent.add_child(&node);
                node
            });
            let entry = self.entries.entry(state.id).or_insert_with(|| Entry {
                content: Content::None,
                gizmo: Parts::default(),
                seen: 0,
            });
            entry.seen = self.frame;
            let matrix = *viewer_from_world * state.world;
            let place = placement(&matrix);
            let media = |id: Option<MediaId>| id.and_then(|id| doc.media(id));
            self.parts.clear();
            let marked = show_extras && state.visible;
            let boxed = marked && layer.show_bounds;
            let mut bounds = None;
            // Where the layer's content is drawn: for all but one kind, at the layer.
            let mut content_matrix = matrix;

            match &layer.kind {
                LayerKind::Picture(picture) => {
                    if !matches!(entry.content, Content::Picture(_)) {
                        entry.take_content().free();
                        entry.content = Content::Picture(PictureNode::new(holder, state));
                    }
                    let Content::Picture(node) = &mut entry.content else {
                        unreachable!("made above")
                    };
                    let media = media(picture.media);
                    node.sync(
                        picture,
                        media,
                        state,
                        viewer_from_world,
                        &mut self.textures,
                        &mut self.picture_meshes,
                        &self.materials,
                    );
                    if picture.viewer_locked {
                        content_matrix = state.world;
                    }
                    if boxed {
                        bounds = media
                            .and_then(|m| self.textures.get(m, pictures::build_texture))
                            .map(|texture| pictures::bounds(picture, texture));
                    }
                }
                LayerKind::Model(model) => {
                    if !matches!(entry.content, Content::Model(_)) {
                        entry.take_content().free();
                        entry.content = Content::Model(ModelNode::new(holder, state));
                    }
                    let Content::Model(node) = &mut entry.content else {
                        unreachable!("made above")
                    };
                    let media = media(model.media);
                    node.sync(
                        model,
                        media,
                        state,
                        viewer_from_world,
                        &mut self.models,
                        &self.materials,
                    );
                    if boxed {
                        bounds = media
                            .and_then(|m| self.models.get(m, models::build))
                            .map(|built| built.bounds());
                    }
                }
                LayerKind::Sound(sound) => {
                    if sound.kind == SoundKind::Ambisonic && !self.warned_about_ambisonic {
                        godot_warn!(
                            "Ambisonic sound is not supported yet; \"{}\" plays the same everywhere.",
                            layer.name
                        );
                        self.warned_about_ambisonic = true;
                    }
                    let positional = sounds::is_positional(sound.kind);
                    if !matches!(&entry.content, Content::Sound(node) if node.is_positional() == positional)
                    {
                        entry.take_content().free();
                        entry.content = Content::Sound(SoundNode::new(holder, state, positional));
                    }
                    let Content::Sound(node) = &mut entry.content else {
                        unreachable!("made above")
                    };
                    let audible = state.in_clip && ancestors_in_clip(doc, frame, state.id);
                    node.sync(
                        sound,
                        media(sound.media),
                        state,
                        audible,
                        &place,
                        &hearing,
                        &mut self.streams,
                    );
                    if marked {
                        gizmos::sound(&mut self.parts, &place, sound);
                    }
                }
                LayerKind::Light(light) => {
                    if !matches!(&entry.content, Content::Light(node) if node.kind() == light.kind)
                    {
                        entry.take_content().free();
                        entry.content = Content::Light(LightNode::new(holder, state, light.kind));
                    }
                    let Content::Light(node) = &mut entry.content else {
                        unreachable!("made above")
                    };
                    node.sync(light, state, &place);
                    if marked {
                        gizmos::light(&mut self.parts, &place, light);
                    }
                }
                LayerKind::Viewpoint(viewpoint) => {
                    if marked {
                        let is_default = doc.settings.default_viewpoint == Some(state.id);
                        gizmos::viewpoint(&mut self.parts, &place, viewpoint, is_default);
                    }
                }
                LayerKind::Camera(camera) => {
                    if marked {
                        gizmos::camera(&mut self.parts, &place, camera, camera_aspect);
                    }
                }
                LayerKind::Paint(_) | LayerKind::Group(_) => {
                    if boxed {
                        bounds = layer_bounds(doc, state.id);
                    }
                }
            }
            if let Some((min, max)) = bounds {
                gizmos::bounds(&mut self.parts, &content_matrix, min, max);
            }
            entry.gizmo.sync(
                &self.parts,
                &mut self.shapes,
                self.materials.line.as_ref(),
                holder,
                state.id,
            );
        }

        // A layer that is no longer in the document takes its nodes with it.
        let now = self.frame;
        let gone: Vec<LayerId> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.seen != now)
            .map(|(id, _)| *id)
            .collect();
        for id in gone {
            if let Some(entry) = self.entries.remove(&id) {
                entry.free();
            }
        }
        self.textures.end_frame();
        self.models.end_frame();
        self.streams.end_frame();
    }

    /// How many sound layers are sounding now.
    pub(super) fn sounds_playing(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| matches!(&entry.content, Content::Sound(node) if node.is_playing()))
            .count()
    }

    /// Frees every node, as when another document is opened.
    pub(super) fn clear(&mut self) {
        for (_, entry) in self.entries.drain() {
            entry.free();
        }
        self.textures.clear();
        self.models.clear();
        self.streams.clear();
        self.warned_about_ambisonic = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner::core::glam::DQuat;

    #[test]
    fn a_placement_keeps_where_a_layer_is_and_faces_and_takes_out_its_size() {
        let rotation = DQuat::from_euler(limner::core::glam::EulerRot::YXZ, 0.7, -0.3, 0.2);
        let matrix = DAffine3::from_scale_rotation_translation(
            DVec3::splat(250.0),
            rotation,
            DVec3::new(1.0, 2.0, 3.0),
        );
        let place = placement(&matrix);
        assert!((place.scale - 250.0).abs() < 1e-9);
        assert_eq!(place.rigid.translation, DVec3::new(1.0, 2.0, 3.0));
        let expected = DMat3::from_quat(rotation);
        assert!(place.rigid.matrix3.abs_diff_eq(expected, 1e-12));
    }

    #[test]
    fn a_mirrored_or_collapsed_layer_still_has_a_usable_placement() {
        let mirrored = DAffine3::from_scale(DVec3::new(-2.0, 2.0, 2.0));
        let place = placement(&mirrored);
        assert!((place.scale - 2.0).abs() < 1e-12);
        assert!((place.rigid.matrix3.determinant() - 1.0).abs() < 1e-12);
        assert_eq!(
            place.rigid.matrix3.z_axis,
            DVec3::Z,
            "it faces the same way"
        );

        let collapsed = placement(&DAffine3::from_scale(DVec3::ZERO));
        assert_eq!(collapsed.scale, 0.0);
        assert!((collapsed.rigid.matrix3.determinant() - 1.0).abs() < 1e-12);

        let tiny = placement(&DAffine3::from_scale(DVec3::splat(1e-6)));
        assert!((tiny.scale - 1e-6).abs() < 1e-18);
        assert!(tiny.rigid.matrix3.abs_diff_eq(DMat3::IDENTITY, 1e-12));
    }
}
