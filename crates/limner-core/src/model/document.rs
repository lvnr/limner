//! The document: a tree of layers plus the drawings and media they refer to.
//!
//! A `Document` is a value. Cloning it copies three maps of `Arc`s, and edits copy
//! only what they touch, which is what makes snapshot undo and pointer-based change
//! detection cheap.

use super::drawing::Drawing;
use super::ids::{DrawingId, LayerId, MediaId};
use super::layer::{Group, Layer, LayerAnim, LayerKind, Paint, Sequence, Viewpoint};
use super::media::{Media, MediaData, MediaKind};
use super::settings::Settings;
use crate::math::Xform;
use glam::DAffine3;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub settings: Settings,
    root: LayerId,
    layers: BTreeMap<LayerId, Arc<Layer>>,
    drawings: BTreeMap<DrawingId, Arc<Drawing>>,
    media: BTreeMap<MediaId, Arc<Media>>,
    next_id: u64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum DocumentError {
    #[error("layer {0} does not exist")]
    NoSuchLayer(LayerId),
    #[error("layer {0} is not a group")]
    NotAGroup(LayerId),
    #[error("the root layer cannot be moved or removed")]
    RootIsFixed,
    #[error("a layer cannot be moved into itself or its own descendants")]
    WouldCreateCycle,
    #[error("layers cannot be nested more than {MAX_LAYER_DEPTH} deep")]
    TooDeep,
    #[error("document is inconsistent: {0}")]
    Inconsistent(String),
}

/// The deepest a layer may sit below the root. The file format holds no more (see
/// `docs/format.md`), so the document refuses to grow past it instead of becoming
/// something that cannot be saved.
pub const MAX_LAYER_DEPTH: usize = 48;

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

impl Document {
    /// A document holding only an empty root sequence.
    pub fn empty() -> Self {
        let root = LayerId(1);
        let mut root_layer = Layer::new(
            root,
            "Root",
            LayerKind::Group(Group {
                children: Vec::new(),
                sequence: Some(Sequence::default()),
            }),
        );
        root_layer.anim = LayerAnim::starting_at(0);
        let mut layers = BTreeMap::new();
        layers.insert(root, Arc::new(root_layer));
        Self {
            settings: Settings::default(),
            root,
            layers,
            drawings: BTreeMap::new(),
            media: BTreeMap::new(),
            next_id: 2,
        }
    }

    /// The document a user starts with: a spawn area and one empty paint layer.
    pub fn new() -> Self {
        let mut doc = Self::empty();
        let root = doc.root;
        let spawn = doc.new_layer("Spawn Area", LayerKind::Viewpoint(Viewpoint::default()), 0);
        let spawn_id = doc.insert_layer(spawn, root, 0).expect("root is a group");
        doc.settings.default_viewpoint = Some(spawn_id);
        doc.add_paint_layer(root, usize::MAX, "Paint", 0)
            .expect("root is a group");
        doc
    }

    pub fn root(&self) -> LayerId {
        self.root
    }

    // ----- ids -----

    fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn alloc_layer_id(&mut self) -> LayerId {
        LayerId(self.alloc())
    }

    /// The next id that would be handed out. Persisted so ids are never reused.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    /// Raises the id counter, for loaders that insert items with existing ids.
    pub fn reserve_ids(&mut self, at_least: u64) {
        self.next_id = self.next_id.max(at_least);
    }

    // ----- layers -----

    pub fn layer(&self, id: LayerId) -> Option<&Arc<Layer>> {
        self.layers.get(&id)
    }

    /// Mutable access to a layer, copying it if it is shared with another document version.
    /// Do not change `parent` or a group's `children` through this; use the tree methods.
    pub fn layer_mut(&mut self, id: LayerId) -> Option<&mut Layer> {
        self.layers.get_mut(&id).map(Arc::make_mut)
    }

    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }

    pub fn layers(&self) -> impl Iterator<Item = &Arc<Layer>> + '_ {
        self.layers.values()
    }

    pub fn children(&self, id: LayerId) -> &[LayerId] {
        self.layer(id).map_or(&[], |l| l.children())
    }

    pub fn parent_of(&self, id: LayerId) -> Option<LayerId> {
        self.layer(id)?.parent
    }

    /// Ancestors from the parent up to the root.
    pub fn ancestors(&self, id: LayerId) -> impl Iterator<Item = LayerId> + '_ {
        let mut current = self.parent_of(id);
        std::iter::from_fn(move || {
            let this = current?;
            current = self.parent_of(this);
            Some(this)
        })
    }

    pub fn is_ancestor_or_self(&self, ancestor: LayerId, id: LayerId) -> bool {
        ancestor == id || self.ancestors(id).any(|a| a == ancestor)
    }

    /// Every layer in the subtree of `id`, parents before children, in display order.
    pub fn subtree(&self, id: LayerId) -> Vec<LayerId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if self.layers.contains_key(&current) {
                out.push(current);
                stack.extend(self.children(current).iter().rev().copied());
            }
        }
        out
    }

    /// Every layer, parents before children, in display order.
    pub fn walk(&self) -> Vec<LayerId> {
        self.subtree(self.root)
    }

    pub fn depth(&self, id: LayerId) -> usize {
        self.ancestors(id).count()
    }

    /// Slash-separated names from the root, such as `Root/Body/Arm`.
    pub fn path_of(&self, id: LayerId) -> String {
        let mut names: Vec<&str> = self
            .ancestors(id)
            .filter_map(|a| self.layer(a))
            .map(|l| l.name.as_str())
            .collect();
        names.reverse();
        if let Some(layer) = self.layer(id) {
            names.push(&layer.name);
        }
        names.join("/")
    }

    /// Finds a layer by its slash-separated path. The first matching sibling wins.
    pub fn find_by_path(&self, path: &str) -> Option<LayerId> {
        let mut parts = path.split('/');
        let first = parts.next()?;
        let root = self.layer(self.root)?;
        if root.name != first {
            return None;
        }
        let mut current = self.root;
        for part in parts {
            current = *self
                .children(current)
                .iter()
                .find(|c| self.layer(**c).is_some_and(|l| l.name == part))?;
        }
        Some(current)
    }

    /// Creates a layer value with a fresh id and a clip starting at `clip_start`.
    /// The layer is not part of the tree until it is inserted.
    pub fn new_layer(
        &mut self,
        name: impl Into<String>,
        kind: LayerKind,
        clip_start: crate::time::Flicks,
    ) -> Layer {
        let mut layer = Layer::new(self.alloc_layer_id(), name, kind);
        layer.anim = LayerAnim::starting_at(clip_start);
        layer
    }

    /// Inserts a layer under `parent` at `index` (clamped to the end).
    pub fn insert_layer(
        &mut self,
        mut layer: Layer,
        parent: LayerId,
        index: usize,
    ) -> Result<LayerId, DocumentError> {
        let id = layer.id;
        if self.layers.contains_key(&parent) && self.depth(parent) >= MAX_LAYER_DEPTH {
            return Err(DocumentError::TooDeep);
        }
        let parent_layer = self
            .layers
            .get_mut(&parent)
            .ok_or(DocumentError::NoSuchLayer(parent))?;
        let group = Arc::make_mut(parent_layer)
            .as_group_mut()
            .ok_or(DocumentError::NotAGroup(parent))?;
        let index = index.min(group.children.len());
        group.children.insert(index, id);
        layer.parent = Some(parent);
        self.next_id = self.next_id.max(id.0 + 1);
        self.layers.insert(id, Arc::new(layer));
        Ok(id)
    }

    /// Adds a paint layer with one empty drawing.
    pub fn add_paint_layer(
        &mut self,
        parent: LayerId,
        index: usize,
        name: &str,
        clip_start: crate::time::Flicks,
    ) -> Result<LayerId, DocumentError> {
        if self.layer(parent).and_then(|l| l.as_group()).is_none() {
            return Err(if self.layer(parent).is_some() {
                DocumentError::NotAGroup(parent)
            } else {
                DocumentError::NoSuchLayer(parent)
            });
        }
        let drawing = self.add_drawing(Drawing::new());
        let rate = self.settings.rate;
        let layer = self.new_layer(
            name,
            LayerKind::Paint(Paint::single(drawing, rate)),
            clip_start,
        );
        self.insert_layer(layer, parent, index)
    }

    /// Removes a layer and its subtree, returning the removed layers (parents first).
    pub fn remove_layer(&mut self, id: LayerId) -> Result<Vec<Arc<Layer>>, DocumentError> {
        if id == self.root {
            return Err(DocumentError::RootIsFixed);
        }
        let parent = self.parent_of(id).ok_or(DocumentError::NoSuchLayer(id))?;
        if let Some(group) = self.layer_mut(parent).and_then(|l| l.as_group_mut()) {
            group.children.retain(|c| *c != id);
        }
        let removed = self
            .subtree(id)
            .into_iter()
            .filter_map(|l| self.layers.remove(&l))
            .collect();
        if self
            .settings
            .default_viewpoint
            .is_some_and(|v| !self.layers.contains_key(&v))
        {
            self.settings.default_viewpoint = None;
        }
        Ok(removed)
    }

    /// Moves a layer to `index` under `new_parent`. The index is the position in the
    /// parent's child list after the layer has been taken out of its old place.
    pub fn move_layer(
        &mut self,
        id: LayerId,
        new_parent: LayerId,
        index: usize,
    ) -> Result<(), DocumentError> {
        if id == self.root {
            return Err(DocumentError::RootIsFixed);
        }
        if !self.layers.contains_key(&id) {
            return Err(DocumentError::NoSuchLayer(id));
        }
        let target = self
            .layer(new_parent)
            .ok_or(DocumentError::NoSuchLayer(new_parent))?;
        if target.as_group().is_none() {
            return Err(DocumentError::NotAGroup(new_parent));
        }
        if self.is_ancestor_or_self(id, new_parent) {
            return Err(DocumentError::WouldCreateCycle);
        }
        // The layer brings everything inside it along, however deep that goes.
        let own_depth = self.depth(id);
        let height = self
            .subtree(id)
            .iter()
            .map(|l| self.depth(*l) - own_depth)
            .max()
            .unwrap_or(0);
        if self.depth(new_parent) + 1 + height > MAX_LAYER_DEPTH {
            return Err(DocumentError::TooDeep);
        }
        let old_parent = self.parent_of(id).ok_or(DocumentError::NoSuchLayer(id))?;
        if let Some(group) = self.layer_mut(old_parent).and_then(|l| l.as_group_mut()) {
            group.children.retain(|c| *c != id);
        }
        let group = self
            .layer_mut(new_parent)
            .and_then(|l| l.as_group_mut())
            .expect("checked above");
        let index = index.min(group.children.len());
        group.children.insert(index, id);
        self.layer_mut(id).expect("checked above").parent = Some(new_parent);
        Ok(())
    }

    /// Position of a layer among its siblings.
    pub fn index_in_parent(&self, id: LayerId) -> Option<usize> {
        let parent = self.parent_of(id)?;
        self.children(parent).iter().position(|c| *c == id)
    }

    // ----- transforms -----

    /// The layer's static placement in world space, ignoring animation.
    pub fn world_from_layer(&self, id: LayerId) -> DAffine3 {
        let mut m = self
            .layer(id)
            .map_or(DAffine3::IDENTITY, |l| l.transform.to_affine());
        for ancestor in self.ancestors(id) {
            if let Some(layer) = self.layer(ancestor) {
                m = layer.transform.to_affine() * m;
            }
        }
        m
    }

    /// The same placement as a layer transform, composed without leaving the transform family.
    pub fn world_xform(&self, id: LayerId) -> Xform {
        let mut x = self.layer(id).map_or(Xform::IDENTITY, |l| l.transform);
        for ancestor in self.ancestors(id) {
            if let Some(layer) = self.layer(ancestor) {
                x = layer.transform.mul(&x);
            }
        }
        x
    }

    // ----- drawings -----

    pub fn drawing(&self, id: DrawingId) -> Option<&Arc<Drawing>> {
        self.drawings.get(&id)
    }

    /// Mutable access to a drawing, copying it if it is shared with another document version.
    pub fn drawing_mut(&mut self, id: DrawingId) -> Option<&mut Drawing> {
        self.drawings.get_mut(&id).map(Arc::make_mut)
    }

    pub fn drawings(&self) -> impl Iterator<Item = (DrawingId, &Arc<Drawing>)> + '_ {
        self.drawings.iter().map(|(id, d)| (*id, d))
    }

    pub fn drawing_count(&self) -> usize {
        self.drawings.len()
    }

    pub fn add_drawing(&mut self, drawing: Drawing) -> DrawingId {
        self.add_shared_drawing(Arc::new(drawing))
    }

    pub fn add_shared_drawing(&mut self, drawing: Arc<Drawing>) -> DrawingId {
        let id = DrawingId(self.alloc());
        self.drawings.insert(id, drawing);
        id
    }

    /// Inserts a drawing under a known id (used by loaders).
    pub fn insert_drawing(&mut self, id: DrawingId, drawing: Arc<Drawing>) {
        self.next_id = self.next_id.max(id.0 + 1);
        self.drawings.insert(id, drawing);
    }

    /// Drawings referenced by at least one paint layer.
    /// True when both documents hold the same content. Parts the two share are
    /// recognised without looking inside them, so comparing a document with an edited
    /// copy of itself is fast however large it is.
    pub fn same_as(&self, other: &Document) -> bool {
        fn same<K: Ord, V>(
            a: &BTreeMap<K, Arc<V>>,
            b: &BTreeMap<K, Arc<V>>,
            eq: impl Fn(&V, &V) -> bool,
        ) -> bool {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|((ka, va), (kb, vb))| ka == kb && (Arc::ptr_eq(va, vb) || eq(va, vb)))
        }
        self.root == other.root
            && self.next_id == other.next_id
            && self.settings == other.settings
            && same(&self.layers, &other.layers, |a, b| a == b)
            && same(&self.drawings, &other.drawings, |a, b| a.same_as(b))
            && same(&self.media, &other.media, |a, b| a == b)
    }

    /// The paint layer a drawing belongs to.
    pub fn layer_of_drawing(&self, drawing: DrawingId) -> Option<LayerId> {
        self.layers
            .values()
            .find(|l| l.as_paint().is_some_and(|p| p.drawings.contains(&drawing)))
            .map(|l| l.id)
    }

    pub fn referenced_drawings(&self) -> BTreeSet<DrawingId> {
        self.layers
            .values()
            .filter_map(|l| l.as_paint())
            .flat_map(|p| p.drawings.iter().copied())
            .collect()
    }

    /// Drops drawings and media no layer refers to. Returns how many items were dropped.
    pub fn collect_garbage(&mut self) -> usize {
        let used = self.referenced_drawings();
        let before = self.drawings.len() + self.media.len();
        self.drawings.retain(|id, _| used.contains(id));
        let used_media = self.referenced_media();
        self.media.retain(|id, _| used_media.contains(id));
        before - self.drawings.len() - self.media.len()
    }

    // ----- media -----

    pub fn media(&self, id: MediaId) -> Option<&Arc<Media>> {
        self.media.get(&id)
    }

    pub fn media_items(&self) -> impl Iterator<Item = &Arc<Media>> + '_ {
        self.media.values()
    }

    pub fn add_media(
        &mut self,
        name: impl Into<String>,
        kind: MediaKind,
        data: MediaData,
    ) -> MediaId {
        let id = MediaId(self.alloc());
        self.media.insert(
            id,
            Arc::new(Media {
                id,
                name: name.into(),
                kind,
                data,
            }),
        );
        id
    }

    /// Inserts a media item under its own id (used by loaders).
    pub fn insert_media(&mut self, media: Media) {
        self.next_id = self.next_id.max(media.id.0 + 1);
        self.media.insert(media.id, Arc::new(media));
    }

    pub fn referenced_media(&self) -> BTreeSet<MediaId> {
        self.layers
            .values()
            .filter_map(|l| match &l.kind {
                LayerKind::Picture(p) => p.media,
                LayerKind::Model(m) => m.media,
                LayerKind::Sound(s) => s.media,
                LayerKind::Viewpoint(v) => v.thumbnail,
                _ => None,
            })
            .collect()
    }

    // ----- statistics and checks -----

    pub fn stroke_count(&self) -> usize {
        self.drawings.values().map(|d| d.stroke_count()).sum()
    }

    pub fn point_count(&self) -> usize {
        self.drawings.values().map(|d| d.point_count()).sum()
    }

    /// Checks the invariants the rest of the code relies on.
    pub fn validate(&self) -> Result<(), DocumentError> {
        let bad = |message: String| Err(DocumentError::Inconsistent(message));
        let root = match self.layer(self.root) {
            Some(root) => root,
            None => return bad("root layer is missing".into()),
        };
        if root.parent.is_some() || root.as_group().is_none() {
            return bad("root must be a parentless group".into());
        }
        let reachable = self.walk();
        let unique: BTreeSet<LayerId> = reachable.iter().copied().collect();
        if unique.len() != reachable.len() {
            return bad("a layer appears more than once in the tree".into());
        }
        if unique.len() != self.layers.len() {
            return bad(format!(
                "{} layers are not reachable from the root",
                self.layers.len() - unique.len()
            ));
        }
        if let Some(viewpoint) = self.settings.default_viewpoint {
            if !self.layers.contains_key(&viewpoint) {
                return bad(format!(
                    "the initial spawn area, layer {viewpoint}, does not exist"
                ));
            }
        }
        for id in &reachable {
            let layer = self.layer(*id).expect("walk only yields existing layers");
            if self.depth(*id) > MAX_LAYER_DEPTH {
                return bad(format!("layer {id} is nested too deep"));
            }
            if layer.id != *id {
                return bad(format!("layer stored under {id} carries id {}", layer.id));
            }
            for child in layer.children() {
                match self.layer(*child) {
                    Some(c) if c.parent == Some(*id) => {}
                    Some(_) => return bad(format!("layer {child} has the wrong parent")),
                    None => return bad(format!("layer {id} lists missing child {child}")),
                }
            }
            if let Some(paint) = layer.as_paint() {
                if paint.frames.is_empty() || paint.drawings.is_empty() {
                    return bad(format!("paint layer {id} has no frames or no drawings"));
                }
                if paint
                    .frames
                    .iter()
                    .any(|f| *f as usize >= paint.drawings.len())
                {
                    return bad(format!(
                        "paint layer {id} has a frame pointing past its drawings"
                    ));
                }
                if let Some(missing) = paint
                    .drawings
                    .iter()
                    .find(|d| !self.drawings.contains_key(d))
                {
                    return bad(format!(
                        "paint layer {id} refers to missing drawing {missing}"
                    ));
                }
                if !paint.rate.is_valid() {
                    return bad(format!("paint layer {id} has an invalid frame rate"));
                }
            }
            if layer.id.0 >= self.next_id {
                return bad(format!("layer id {id} is not below the id counter"));
            }
        }
        if let Some(missing) = self
            .referenced_media()
            .into_iter()
            .find(|m| !self.media.contains_key(m))
        {
            return bad(format!("a layer refers to missing media {missing}"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stroke::{Brush, Point, Stroke};
    use glam::{DQuat, DVec3};

    fn group(doc: &mut Document, parent: LayerId, name: &str) -> LayerId {
        let layer = doc.new_layer(name, LayerKind::Group(Group::default()), 0);
        doc.insert_layer(layer, parent, usize::MAX).unwrap()
    }

    #[test]
    fn new_document_is_valid_and_has_the_default_layers() {
        let doc = Document::new();
        doc.validate().unwrap();
        let names: Vec<String> = doc.walk().iter().map(|id| doc.path_of(*id)).collect();
        assert_eq!(names, vec!["Root", "Root/Spawn Area", "Root/Paint"]);
        assert_eq!(
            doc.settings.default_viewpoint,
            doc.find_by_path("Root/Spawn Area")
        );
        assert_eq!(doc.drawing_count(), 1);
        assert!(doc.layer(doc.root()).unwrap().is_sequence());
    }

    #[test]
    fn tree_edits_keep_the_document_valid() {
        let mut doc = Document::new();
        let root = doc.root();
        let body = group(&mut doc, root, "Body");
        let arm = group(&mut doc, body, "Arm");
        let hand = doc.add_paint_layer(arm, 0, "Hand", 0).unwrap();
        doc.validate().unwrap();
        assert_eq!(doc.path_of(hand), "Root/Body/Arm/Hand");
        assert_eq!(doc.depth(hand), 3);
        assert_eq!(doc.find_by_path("Root/Body/Arm/Hand"), Some(hand));

        assert_eq!(
            doc.move_layer(body, arm, 0),
            Err(DocumentError::WouldCreateCycle)
        );
        assert_eq!(
            doc.move_layer(root, body, 0),
            Err(DocumentError::RootIsFixed)
        );
        assert_eq!(
            doc.move_layer(arm, hand, 0),
            Err(DocumentError::NotAGroup(hand))
        );

        doc.move_layer(hand, root, 0).unwrap();
        doc.validate().unwrap();
        assert_eq!(doc.children(root)[0], hand);
        assert_eq!(doc.index_in_parent(hand), Some(0));

        let removed = doc.remove_layer(body).unwrap();
        assert_eq!(removed.len(), 2);
        doc.validate().unwrap();
        assert!(doc.layer(arm).is_none());
        assert_eq!(doc.remove_layer(root), Err(DocumentError::RootIsFixed));
    }

    #[test]
    fn clones_share_layers_until_edited() {
        let mut doc = Document::new();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        let snapshot = doc.clone();
        doc.layer_mut(paint).unwrap().opacity = 0.5;
        assert_eq!(snapshot.layer(paint).unwrap().opacity, 1.0);
        let spawn = doc.find_by_path("Root/Spawn Area").unwrap();
        assert!(Arc::ptr_eq(
            doc.layer(spawn).unwrap(),
            snapshot.layer(spawn).unwrap()
        ));
        assert!(!Arc::ptr_eq(
            doc.layer(paint).unwrap(),
            snapshot.layer(paint).unwrap()
        ));
    }

    #[test]
    fn world_transform_composes_down_the_tree() {
        let mut doc = Document::new();
        let root = doc.root();
        let a = group(&mut doc, root, "A");
        let b = doc.add_paint_layer(a, 0, "B", 0).unwrap();
        doc.layer_mut(a).unwrap().transform = Xform {
            rotation: DQuat::from_rotation_y(0.7),
            scale: 2.0,
            translation: DVec3::new(1.0, 2.0, 3.0),
            flip: crate::math::Flip::X,
        };
        doc.layer_mut(b).unwrap().transform = Xform {
            rotation: DQuat::from_rotation_x(-0.3),
            scale: 0.5,
            translation: DVec3::new(-4.0, 0.0, 1.0),
            flip: crate::math::Flip::Z,
        };
        let p = DVec3::new(0.3, -0.2, 0.9);
        let via_matrix = doc.world_from_layer(b).transform_point3(p);
        let by_hand = doc
            .layer(a)
            .unwrap()
            .transform
            .transform_point(doc.layer(b).unwrap().transform.transform_point(p));
        assert!((via_matrix - by_hand).length() < 1e-12);
        assert!((doc.world_xform(b).transform_point(p) - by_hand).length() < 1e-12);
    }

    #[test]
    fn garbage_collection_drops_unreferenced_drawings_only() {
        let mut doc = Document::new();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        let p = Point {
            pos: [0.0; 3],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, 1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.01,
        };
        let orphan = doc.add_drawing(Drawing::from_strokes([Stroke::new(
            0,
            Brush::Ribbon,
            false,
            vec![p; 4],
        )]));
        assert_eq!(doc.stroke_count(), 1);
        assert_eq!(doc.collect_garbage(), 1);
        assert!(doc.drawing(orphan).is_none());
        let kept = doc.layer(paint).unwrap().as_paint().unwrap().drawings[0];
        assert!(doc.drawing(kept).is_some());
        doc.validate().unwrap();
    }

    #[test]
    fn validation_catches_broken_frames() {
        let mut doc = Document::new();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        doc.layer_mut(paint).unwrap().as_paint_mut().unwrap().frames = vec![3];
        assert!(matches!(
            doc.validate(),
            Err(DocumentError::Inconsistent(_))
        ));
    }
}
