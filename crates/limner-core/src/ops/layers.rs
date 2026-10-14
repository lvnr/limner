//! Layer operations: creating, copying, restructuring and merging layers.

use super::strokes::{append_transformed, transformed_drawing};
use super::OpError;
use crate::math::{Aabb, Flip, Xform};
use crate::model::{
    Camera, Document, Drawing, DrawingId, Group, Layer, LayerId, LayerKind, Light, MediaId, Paint,
    Sequence, Viewpoint,
};
use crate::time::Flicks;
use glam::DVec3;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// The kinds of layer that can be created empty. Pictures, models and sounds are
/// created by importing a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewLayerKind {
    Group,
    Sequence,
    Paint,
    Viewpoint,
    Camera,
    Light,
}

impl NewLayerKind {
    fn default_name(self) -> &'static str {
        match self {
            NewLayerKind::Group => "Group",
            NewLayerKind::Sequence => "Sequence",
            NewLayerKind::Paint => "Paint",
            NewLayerKind::Viewpoint => "Spawn Area",
            NewLayerKind::Camera => "Camera",
            NewLayerKind::Light => "Light",
        }
    }
}

/// Layer names are used in slash-separated paths, so they cannot contain a slash or be empty.
pub fn clean_name(name: &str) -> String {
    let cleaned: String = name
        .trim()
        .chars()
        .map(|c| if c == '/' || c.is_control() { '-' } else { c })
        .collect();
    if cleaned.is_empty() {
        "Layer".to_owned()
    } else {
        cleaned
    }
}

/// `base`, or `base 2`, `base 3`, … whichever is not yet used by a sibling under `parent`.
pub fn unique_name(doc: &Document, parent: LayerId, base: &str) -> String {
    let taken = |name: &str| {
        doc.children(parent)
            .iter()
            .any(|c| doc.layer(*c).is_some_and(|l| l.name == name))
    };
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|candidate| !taken(candidate))
        .expect("an unused number exists")
}

/// Creates an empty layer under `parent` at `index`, with a clip starting at `clip_start`.
pub fn add_layer(
    doc: &mut Document,
    kind: NewLayerKind,
    parent: LayerId,
    index: usize,
    name: Option<&str>,
    clip_start: Flicks,
) -> Result<LayerId, OpError> {
    if doc
        .layer(parent)
        .ok_or(OpError::NoSuchLayer(parent))?
        .as_group()
        .is_none()
    {
        return Err(OpError::NotGroup(parent));
    }
    let name = unique_name(
        doc,
        parent,
        &clean_name(name.unwrap_or(kind.default_name())),
    );
    let layer_kind = match kind {
        NewLayerKind::Group => LayerKind::Group(Group::default()),
        NewLayerKind::Sequence => LayerKind::Group(Group {
            children: Vec::new(),
            sequence: Some(Sequence::default()),
        }),
        NewLayerKind::Paint => {
            let drawing = doc.add_drawing(Drawing::new());
            LayerKind::Paint(Paint::single(drawing, doc.settings.rate))
        }
        NewLayerKind::Viewpoint => LayerKind::Viewpoint(Viewpoint::default()),
        NewLayerKind::Camera => LayerKind::Camera(Camera::default()),
        NewLayerKind::Light => LayerKind::Light(Light::default()),
    };
    let layer = doc.new_layer(name, layer_kind, clip_start);
    Ok(doc.insert_layer(layer, parent, index)?)
}

/// Deletes a layer and everything under it.
pub fn delete_layer(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    doc.remove_layer(id)?;
    doc.collect_garbage();
    Ok(())
}

/// Copies a layer and its subtree, placing the copy just after the original.
/// Drawings are shared until either copy is edited.
pub fn duplicate_layer(doc: &mut Document, id: LayerId) -> Result<LayerId, OpError> {
    if id == doc.root() {
        return Err(OpError::Invalid(
            "the root layer cannot be duplicated".into(),
        ));
    }
    let parent = doc.parent_of(id).ok_or(OpError::NoSuchLayer(id))?;
    let index = doc.index_in_parent(id).unwrap_or(usize::MAX);
    let mut drawing_map: HashMap<DrawingId, DrawingId> = HashMap::new();
    let copy = copy_subtree(doc, id, parent, index.saturating_add(1), &mut drawing_map)?;
    let name = doc
        .layer(id)
        .map(|l| format!("{} copy", l.name))
        .unwrap_or_default();
    let name = unique_name(doc, parent, &name);
    doc.layer_mut(copy).expect("just inserted").name = name;
    Ok(copy)
}

fn copy_subtree(
    doc: &mut Document,
    source: LayerId,
    parent: LayerId,
    index: usize,
    drawing_map: &mut HashMap<DrawingId, DrawingId>,
) -> Result<LayerId, OpError> {
    let original: Arc<Layer> = doc
        .layer(source)
        .ok_or(OpError::NoSuchLayer(source))?
        .clone();
    let mut layer = (*original).clone();
    layer.id = doc.alloc_layer_id();
    layer.parent = None;
    let children: Vec<LayerId> = match &mut layer.kind {
        LayerKind::Group(group) => std::mem::take(&mut group.children),
        LayerKind::Paint(paint) => {
            for drawing in &mut paint.drawings {
                let mapped = match drawing_map.get(drawing) {
                    Some(mapped) => *mapped,
                    None => {
                        let shared = doc.drawing(*drawing).cloned().unwrap_or_default();
                        let new_id = doc.add_shared_drawing(shared);
                        drawing_map.insert(*drawing, new_id);
                        new_id
                    }
                };
                *drawing = mapped;
            }
            Vec::new()
        }
        _ => Vec::new(),
    };
    let new_id = doc.insert_layer(layer, parent, index)?;
    for child in children {
        copy_subtree(doc, child, new_id, usize::MAX, drawing_map)?;
    }
    Ok(new_id)
}

/// Brings everything in `other` into `doc` as one new group named `name` under
/// `parent`: its layers with their animation, their drawings and their media. The
/// other document's own settings, marks and initial spawn area stay behind. Returns
/// the new group.
pub fn graft_document(
    doc: &mut Document,
    other: &Document,
    parent: LayerId,
    index: usize,
    name: &str,
) -> Result<LayerId, OpError> {
    let name = unique_name(doc, parent, &clean_name(name));
    let group = doc.new_layer(name, LayerKind::Group(Group::default()), 0);
    let group = doc.insert_layer(group, parent, index)?;
    let mut drawings: HashMap<DrawingId, DrawingId> = HashMap::new();
    let mut media: HashMap<MediaId, MediaId> = HashMap::new();
    for child in other.children(other.root()) {
        graft_layer(doc, other, *child, group, &mut drawings, &mut media)?;
    }
    Ok(group)
}

fn graft_layer(
    doc: &mut Document,
    other: &Document,
    source: LayerId,
    parent: LayerId,
    drawings: &mut HashMap<DrawingId, DrawingId>,
    media: &mut HashMap<MediaId, MediaId>,
) -> Result<(), OpError> {
    let Some(original) = other.layer(source) else {
        return Ok(());
    };
    let mut layer = (**original).clone();
    layer.id = doc.alloc_layer_id();
    layer.parent = None;
    // What a layer shows lives in tables of the document it came from; bring that
    // along under names of this one. Shared drawings and media stay shared.
    let mut bring = |id: &mut Option<MediaId>, doc: &mut Document| {
        *id = id.and_then(|old| match media.get(&old) {
            Some(new) => Some(*new),
            None => {
                let item = other.media(old)?;
                let new = doc.add_media(item.name.clone(), item.kind, item.data.clone());
                media.insert(old, new);
                Some(new)
            }
        });
    };
    let children: Vec<LayerId> = match &mut layer.kind {
        LayerKind::Group(group) => std::mem::take(&mut group.children),
        LayerKind::Paint(paint) => {
            for drawing in &mut paint.drawings {
                *drawing = match drawings.get(drawing) {
                    Some(new) => *new,
                    None => {
                        let shared = other.drawing(*drawing).cloned().unwrap_or_default();
                        let new = doc.add_shared_drawing(shared);
                        drawings.insert(*drawing, new);
                        new
                    }
                };
            }
            Vec::new()
        }
        LayerKind::Picture(picture) => {
            bring(&mut picture.media, doc);
            Vec::new()
        }
        LayerKind::Model(model) => {
            bring(&mut model.media, doc);
            Vec::new()
        }
        LayerKind::Sound(sound) => {
            bring(&mut sound.media, doc);
            Vec::new()
        }
        _ => Vec::new(),
    };
    let id = doc.insert_layer(layer, parent, usize::MAX)?;
    for child in children {
        graft_layer(doc, other, child, id, drawings, media)?;
    }
    Ok(())
}

/// Moves a layer one step up the list (towards the first child). At the top of its
/// group it leaves the group; next to an open group it enters it.
pub fn move_layer_up(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    step_layer(doc, id, true)
}

/// Moves a layer one step down the list. See [`move_layer_up`].
pub fn move_layer_down(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    step_layer(doc, id, false)
}

fn step_layer(doc: &mut Document, id: LayerId, up: bool) -> Result<(), OpError> {
    let parent = doc.parent_of(id).ok_or(OpError::NoSuchLayer(id))?;
    let index = doc.index_in_parent(id).ok_or(OpError::NoSuchLayer(id))?;
    let siblings = doc.children(parent).to_vec();
    let neighbour = if up {
        index.checked_sub(1).map(|i| siblings[i])
    } else {
        siblings.get(index + 1).copied()
    };
    match neighbour {
        Some(next) => {
            let enters = doc
                .layer(next)
                .is_some_and(|l| l.as_group().is_some() && !l.collapsed);
            if enters {
                // Going up enters at the bottom of the group above; going down, at the top of the group below.
                let position = if up { usize::MAX } else { 0 };
                doc.move_layer(id, next, position)?;
            } else {
                doc.move_layer(id, parent, if up { index - 1 } else { index + 1 })?;
            }
        }
        None => {
            // At the edge of the group: step out, landing just before or after the group.
            let Some(grandparent) = doc.parent_of(parent) else {
                return Ok(());
            };
            let parent_index = doc.index_in_parent(parent).unwrap_or(0);
            doc.move_layer(
                id,
                grandparent,
                if up { parent_index } else { parent_index + 1 },
            )?;
        }
    }
    Ok(())
}

/// Turns a group into a sequence (a group with its own clock) or back.
pub fn set_group_is_sequence(
    doc: &mut Document,
    id: LayerId,
    sequence: bool,
) -> Result<(), OpError> {
    if id == doc.root() {
        return Err(OpError::Invalid("the root is always a sequence".into()));
    }
    let group = doc
        .layer_mut(id)
        .ok_or(OpError::NoSuchLayer(id))?
        .as_group_mut()
        .ok_or(OpError::NotGroup(id))?;
    match (sequence, group.sequence) {
        (true, None) => group.sequence = Some(Sequence::default()),
        (false, Some(_)) => group.sequence = None,
        _ => {}
    }
    Ok(())
}

/// Mirrors a layer on one of its axes, through its pivot. Keyed transforms are mirrored too.
pub fn flip_layer(doc: &mut Document, id: LayerId, axis: Flip) -> Result<(), OpError> {
    let layer = doc.layer_mut(id).ok_or(OpError::NoSuchLayer(id))?;
    let about_pivot = layer
        .pivot
        .mul(&Xform::from_flip(axis))
        .mul(&layer.pivot.inverse());
    layer.transform = layer.transform.mul(&about_pivot);
    for key in layer.anim.transform.clone().keys() {
        layer
            .anim
            .transform
            .set(key.time, key.value.mul(&about_pivot), key.interp);
    }
    Ok(())
}

/// Re-expresses a layer's transform, and its transform keys, in another parent space:
/// `delta` maps the old parent's space into the new one.
pub fn rebase_transform(doc: &mut Document, id: LayerId, delta: &Xform) -> Result<(), OpError> {
    let layer = doc.layer_mut(id).ok_or(OpError::NoSuchLayer(id))?;
    layer.transform = delta.mul(&layer.transform);
    for key in layer.anim.transform.clone().keys() {
        layer
            .anim
            .transform
            .set(key.time, delta.mul(&key.value), key.interp);
    }
    Ok(())
}

/// Bounds of a layer's content in its own space, at its first frame. Groups include their children.
pub fn layer_bounds(doc: &Document, id: LayerId) -> Option<(DVec3, DVec3)> {
    let layer = doc.layer(id)?;
    match &layer.kind {
        LayerKind::Paint(paint) => {
            let bounds = paint
                .drawings
                .iter()
                .filter_map(|d| doc.drawing(*d))
                .fold(Aabb::EMPTY, |b, d| b.union(&d.bounds()));
            (!bounds.is_empty()).then(|| (bounds.min.as_dvec3(), bounds.max.as_dvec3()))
        }
        LayerKind::Group(group) => {
            let mut min = DVec3::INFINITY;
            let mut max = DVec3::NEG_INFINITY;
            for child in &group.children {
                let Some((lo, hi)) = layer_bounds(doc, *child) else {
                    continue;
                };
                let m = doc.layer(*child)?.transform.to_affine();
                for corner in 0..8 {
                    let p = DVec3::new(
                        if corner & 1 == 0 { lo.x } else { hi.x },
                        if corner & 2 == 0 { lo.y } else { hi.y },
                        if corner & 4 == 0 { lo.z } else { hi.z },
                    );
                    let q = m.transform_point3(p);
                    min = min.min(q);
                    max = max.max(q);
                }
            }
            (min.x <= max.x).then_some((min, max))
        }
        _ => None,
    }
}

/// Moves the pivot to the centre of the layer's content.
pub fn reset_pivot(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    let center = layer_bounds(doc, id).map_or(DVec3::ZERO, |(lo, hi)| (lo + hi) * 0.5);
    doc.layer_mut(id).ok_or(OpError::NoSuchLayer(id))?.pivot = Xform::from_translation(center);
    Ok(())
}

/// Makes the pivot the layer's origin without moving anything: the pivot is folded
/// into the layer's transform and taken back out of its content.
pub fn make_pivot_origin(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    let layer = doc.layer(id).ok_or(OpError::NoSuchLayer(id))?.clone();
    let pivot = layer.pivot;
    if pivot.is_identity() {
        return Ok(());
    }
    let undo_pivot = pivot.inverse();
    match &layer.kind {
        LayerKind::Paint(paint) => {
            for drawing in &paint.drawings {
                if let Some(original) = doc.drawing(*drawing).cloned() {
                    *doc.drawing_mut(*drawing).expect("exists") =
                        transformed_drawing(&original, &undo_pivot);
                }
            }
        }
        LayerKind::Group(group) => {
            for child in &group.children {
                if let Some(child_layer) = doc.layer_mut(*child) {
                    child_layer.transform = undo_pivot.mul(&child_layer.transform);
                    for key in child_layer.anim.transform.clone().keys() {
                        child_layer.anim.transform.set(
                            key.time,
                            undo_pivot.mul(&key.value),
                            key.interp,
                        );
                    }
                }
            }
        }
        _ => {}
    }
    let layer = doc.layer_mut(id).expect("checked above");
    layer.transform = layer.transform.mul(&pivot);
    for key in layer.anim.transform.clone().keys() {
        layer
            .anim
            .transform
            .set(key.time, key.value.mul(&pivot), key.interp);
    }
    layer.pivot = Xform::IDENTITY;
    Ok(())
}

/// Merges a paint layer into the paint layer directly below it in the list. The
/// lower layer keeps its place and properties and gains the upper layer's strokes.
pub fn merge_down(doc: &mut Document, id: LayerId) -> Result<LayerId, OpError> {
    let parent = doc.parent_of(id).ok_or(OpError::NoSuchLayer(id))?;
    let index = doc.index_in_parent(id).ok_or(OpError::NoSuchLayer(id))?;
    let below = *doc
        .children(parent)
        .get(index + 1)
        .ok_or_else(|| OpError::Invalid("there is no layer below to merge into".into()))?;
    merge_paint_layers(doc, id, below)?;
    doc.remove_layer(id)?;
    doc.collect_garbage();
    Ok(below)
}

/// Adds the strokes of paint layer `source` to paint layer `target`. Both must be
/// siblings. Their frame-by-frame animations must match in length, unless one of them
/// has a single frame, which is then shown on every frame of the other.
fn merge_paint_layers(doc: &mut Document, source: LayerId, target: LayerId) -> Result<(), OpError> {
    let source_layer = doc
        .layer(source)
        .ok_or(OpError::NoSuchLayer(source))?
        .clone();
    let target_layer = doc
        .layer(target)
        .ok_or(OpError::NoSuchLayer(target))?
        .clone();
    let source_paint = source_layer.as_paint().ok_or(OpError::NotPaint(source))?;
    let target_paint = target_layer.as_paint().ok_or(OpError::NotPaint(target))?;
    if target_layer.locked {
        return Err(OpError::Invalid(format!(
            "layer \"{}\" is locked",
            target_layer.name
        )));
    }
    let (source_frames, target_frames) = (source_paint.frames.len(), target_paint.frames.len());
    if source_frames > 1
        && target_frames > 1
        && (source_frames != target_frames || source_paint.rate != target_paint.rate)
    {
        return Err(OpError::Invalid(
            "layers with different frame counts or frame rates cannot be merged".into(),
        ));
    }
    // Strokes move from the source layer's space into the target's.
    let into_target = target_layer
        .transform
        .inverse()
        .mul(&source_layer.transform);

    // Build one merged drawing per distinct (target drawing, source drawing) pair.
    let frame_count = source_frames.max(target_frames);
    let mut merged: HashMap<(DrawingId, DrawingId), u32> = HashMap::new();
    let mut drawings: Vec<DrawingId> = Vec::new();
    let mut frames: Vec<u32> = Vec::with_capacity(frame_count);
    for frame in 0..frame_count {
        let target_drawing = target_paint
            .drawing_at_frame(frame % target_frames)
            .ok_or_else(|| OpError::Invalid("target layer has a broken frame".into()))?;
        let source_drawing = source_paint
            .drawing_at_frame(frame % source_frames)
            .ok_or_else(|| OpError::Invalid("source layer has a broken frame".into()))?;
        let index = match merged.get(&(target_drawing, source_drawing)) {
            Some(index) => *index,
            None => {
                let mut combined: Drawing = doc
                    .drawing(target_drawing)
                    .map(|d| (**d).clone())
                    .unwrap_or_default();
                if let Some(extra) = doc.drawing(source_drawing).cloned() {
                    append_transformed(&mut combined, &extra, &into_target);
                }
                let id = doc.add_drawing(combined);
                drawings.push(id);
                let index = (drawings.len() - 1) as u32;
                merged.insert((target_drawing, source_drawing), index);
                index
            }
        };
        frames.push(index);
    }

    let animated_source = source_frames > 1 && target_frames <= 1;
    let paint = doc
        .layer_mut(target)
        .expect("checked above")
        .as_paint_mut()
        .expect("checked above");
    paint.drawings = drawings;
    paint.frames = frames;
    if animated_source {
        paint.rate = source_paint.rate;
        paint.max_repeat = source_paint.max_repeat;
    }
    Ok(())
}

/// Replaces a group by a single paint layer holding all the strokes of the paint
/// layers inside it. Only groups whose paint layers have a single frame can be flattened.
pub fn flatten_group(doc: &mut Document, id: LayerId) -> Result<LayerId, OpError> {
    if id == doc.root() {
        return Err(OpError::Invalid(
            "the root layer cannot be flattened".into(),
        ));
    }
    let group_layer = doc.layer(id).ok_or(OpError::NoSuchLayer(id))?.clone();
    group_layer.as_group().ok_or(OpError::NotGroup(id))?;
    let parent = doc.parent_of(id).ok_or(OpError::NoSuchLayer(id))?;
    let index = doc.index_in_parent(id).unwrap_or(0);

    let mut combined = Drawing::new();
    for descendant in doc.subtree(id).into_iter().skip(1) {
        let layer = doc
            .layer(descendant)
            .expect("subtree yields existing layers")
            .clone();
        let Some(paint) = layer.as_paint() else {
            if layer.as_group().is_some() {
                continue;
            }
            return Err(OpError::Invalid(format!(
                "\"{}\" is not a paint layer or group, so the group cannot be flattened",
                layer.name
            )));
        };
        if paint.is_animated() {
            return Err(OpError::Invalid(format!(
                "\"{}\" is animated, so the group cannot be flattened",
                layer.name
            )));
        }
        // Transform from the paint layer's space up to the group's own space.
        let mut to_group = layer.transform;
        for ancestor in doc.ancestors(descendant).take_while(|a| *a != id) {
            to_group = doc
                .layer(ancestor)
                .expect("ancestors exist")
                .transform
                .mul(&to_group);
        }
        if let Some(drawing) = paint
            .drawing_at_frame(0)
            .and_then(|d| doc.drawing(d))
            .cloned()
        {
            append_transformed(&mut combined, &drawing, &to_group);
        }
    }

    let drawing = doc.add_drawing(combined);
    let mut flat = doc.new_layer(
        group_layer.name.clone(),
        LayerKind::Paint(Paint::single(drawing, doc.settings.rate)),
        0,
    );
    flat.visible = group_layer.visible;
    flat.locked = group_layer.locked;
    flat.opacity = group_layer.opacity;
    flat.transform = group_layer.transform;
    flat.pivot = group_layer.pivot;
    flat.anim = group_layer.anim.clone();
    doc.remove_layer(id)?;
    let flat_id = doc.insert_layer(flat, parent, index)?;
    doc.collect_garbage();
    Ok(flat_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Brush, Point, Stroke};
    use glam::DQuat;

    fn line(id: u32, x: f32) -> Stroke {
        let p = |y: f32| Point {
            pos: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            dir: [0.0, 0.0, -1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.01,
        };
        Stroke::new(id, Brush::Ribbon, false, vec![p(0.0), p(1.0)])
    }

    fn paint_with(
        doc: &mut Document,
        parent: LayerId,
        name: &str,
        strokes: Vec<Stroke>,
    ) -> LayerId {
        let id = add_layer(doc, NewLayerKind::Paint, parent, usize::MAX, Some(name), 0).unwrap();
        let drawing = doc.layer(id).unwrap().as_paint().unwrap().drawings[0];
        *doc.drawing_mut(drawing).unwrap() = Drawing::from_strokes(strokes);
        id
    }

    fn names(doc: &Document, parent: LayerId) -> Vec<String> {
        doc.children(parent)
            .iter()
            .map(|c| doc.layer(*c).unwrap().name.clone())
            .collect()
    }

    fn first_points(doc: &Document, layer: LayerId) -> Vec<[f32; 3]> {
        let paint = doc.layer(layer).unwrap().as_paint().unwrap();
        doc.drawing(paint.drawings[paint.frames[0] as usize])
            .unwrap()
            .strokes()
            .map(|s| s.points()[0].pos)
            .collect()
    }

    #[test]
    fn new_layers_get_unique_clean_names() {
        let mut doc = Document::empty();
        let root = doc.root();
        add_layer(&mut doc, NewLayerKind::Paint, root, 0, None, 0).unwrap();
        add_layer(&mut doc, NewLayerKind::Paint, root, 0, None, 0).unwrap();
        add_layer(&mut doc, NewLayerKind::Paint, root, 0, Some("a/b"), 0).unwrap();
        add_layer(
            &mut doc,
            NewLayerKind::Group,
            root,
            usize::MAX,
            Some("   "),
            0,
        )
        .unwrap();
        assert_eq!(names(&doc, root), ["a-b", "Paint 2", "Paint", "Layer"]);
        doc.validate().unwrap();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        assert_eq!(
            add_layer(&mut doc, NewLayerKind::Camera, paint, 0, None, 0),
            Err(OpError::NotGroup(paint))
        );
    }

    #[test]
    fn new_layers_start_their_clip_where_asked() {
        let mut doc = Document::empty();
        let root = doc.root();
        let id = add_layer(&mut doc, NewLayerKind::Light, root, 0, None, 4_200).unwrap();
        let layer = doc.layer(id).unwrap();
        assert_eq!(layer.anim.visibility.keys()[0].time, 4_200);
        assert!(matches!(layer.kind, LayerKind::Light(_)));
    }

    #[test]
    fn duplicates_are_independent_but_share_strokes_until_edited() {
        let mut doc = Document::empty();
        let root = doc.root();
        let group = add_layer(&mut doc, NewLayerKind::Group, root, 0, Some("Arm"), 0).unwrap();
        let hand = paint_with(&mut doc, group, "Hand", vec![line(0, 1.0)]);
        let copy = duplicate_layer(&mut doc, group).unwrap();
        doc.validate().unwrap();
        assert_eq!(names(&doc, root), ["Arm", "Arm copy"]);
        let copy_hand = doc.children(copy)[0];
        assert_ne!(copy_hand, hand);

        let original_drawing = doc.layer(hand).unwrap().as_paint().unwrap().drawings[0];
        let copied_drawing = doc.layer(copy_hand).unwrap().as_paint().unwrap().drawings[0];
        assert_ne!(
            original_drawing, copied_drawing,
            "the copy has its own drawing id"
        );
        assert!(
            Arc::ptr_eq(
                doc.drawing(original_drawing).unwrap(),
                doc.drawing(copied_drawing).unwrap()
            ),
            "but the strokes are shared"
        );

        doc.drawing_mut(copied_drawing)
            .unwrap()
            .push_stroke(line(1, 2.0));
        assert_eq!(doc.drawing(original_drawing).unwrap().stroke_count(), 1);
        assert_eq!(doc.drawing(copied_drawing).unwrap().stroke_count(), 2);
        assert!(duplicate_layer(&mut doc, root).is_err());
    }

    #[test]
    fn stepping_a_layer_swaps_enters_and_leaves_groups() {
        let mut doc = Document::empty();
        let root = doc.root();
        let a = add_layer(
            &mut doc,
            NewLayerKind::Paint,
            root,
            usize::MAX,
            Some("A"),
            0,
        )
        .unwrap();
        let group = add_layer(
            &mut doc,
            NewLayerKind::Group,
            root,
            usize::MAX,
            Some("G"),
            0,
        )
        .unwrap();
        add_layer(
            &mut doc,
            NewLayerKind::Paint,
            group,
            usize::MAX,
            Some("Inside"),
            0,
        )
        .unwrap();
        add_layer(
            &mut doc,
            NewLayerKind::Paint,
            root,
            usize::MAX,
            Some("B"),
            0,
        )
        .unwrap();

        // A steps down into the open group, landing at its top.
        move_layer_down(&mut doc, a).unwrap();
        assert_eq!(names(&doc, root), ["G", "B"]);
        assert_eq!(names(&doc, group), ["A", "Inside"]);
        // Down again swaps with its sibling; down once more leaves the group below it.
        move_layer_down(&mut doc, a).unwrap();
        assert_eq!(names(&doc, group), ["Inside", "A"]);
        move_layer_down(&mut doc, a).unwrap();
        assert_eq!(names(&doc, root), ["G", "A", "B"]);
        // Up enters the group again, at its bottom.
        move_layer_up(&mut doc, a).unwrap();
        assert_eq!(names(&doc, group), ["Inside", "A"]);
        // A collapsed group is stepped over, not entered.
        move_layer_up(&mut doc, a).unwrap();
        move_layer_up(&mut doc, a).unwrap();
        assert_eq!(names(&doc, root), ["A", "G", "B"]);
        doc.layer_mut(group).unwrap().collapsed = true;
        move_layer_down(&mut doc, a).unwrap();
        assert_eq!(names(&doc, root), ["G", "A", "B"]);
        // At the very top nothing happens.
        let g = group;
        move_layer_up(&mut doc, g).unwrap();
        assert_eq!(names(&doc, root), ["G", "A", "B"]);
        doc.validate().unwrap();
    }

    #[test]
    fn flipping_mirrors_through_the_pivot_and_twice_restores() {
        let mut doc = Document::empty();
        let root = doc.root();
        let id = paint_with(&mut doc, root, "P", vec![line(0, 0.0)]);
        {
            let layer = doc.layer_mut(id).unwrap();
            layer.transform = Xform {
                rotation: DQuat::from_rotation_y(0.4),
                scale: 2.0,
                translation: DVec3::new(1.0, 2.0, 3.0),
                flip: Flip::None,
            };
            layer.pivot = Xform::from_translation(DVec3::new(0.5, 0.0, 0.0));
        }
        let before = doc.layer(id).unwrap().transform;
        flip_layer(&mut doc, id, Flip::X).unwrap();
        let flipped = doc.layer(id).unwrap().transform;
        assert!(flipped.flip.is_mirrored());
        // The pivot itself does not move; a point left of it lands the same distance to its right.
        let pivot = DVec3::new(0.5, 0.0, 0.0);
        assert!((flipped.transform_point(pivot) - before.transform_point(pivot)).length() < 1e-12);
        assert!(
            (flipped.transform_point(DVec3::new(0.2, 0.0, 0.0))
                - before.transform_point(DVec3::new(0.8, 0.0, 0.0)))
            .length()
                < 1e-12
        );
        flip_layer(&mut doc, id, Flip::X).unwrap();
        let restored = doc.layer(id).unwrap().transform;
        let p = DVec3::new(0.3, -0.7, 0.1);
        assert!((restored.transform_point(p) - before.transform_point(p)).length() < 1e-12);
        assert!(!restored.flip.is_mirrored());
    }

    #[test]
    fn pivot_can_be_centred_and_made_the_origin_without_moving_content() {
        let mut doc = Document::empty();
        let root = doc.root();
        let id = paint_with(&mut doc, root, "P", vec![line(0, 2.0), line(1, 4.0)]);
        doc.layer_mut(id).unwrap().transform = Xform {
            rotation: DQuat::from_rotation_z(0.3),
            scale: 1.5,
            translation: DVec3::new(-1.0, 0.0, 2.0),
            flip: Flip::None,
        };
        reset_pivot(&mut doc, id).unwrap();
        let pivot = doc.layer(id).unwrap().pivot.translation;
        assert!(
            (pivot - DVec3::new(3.0, 0.5, 0.0)).length() < 1e-6,
            "{pivot:?}"
        );

        let world_before: Vec<DVec3> = first_points(&doc, id)
            .iter()
            .map(|p| {
                doc.world_from_layer(id)
                    .transform_point3(DVec3::from_array(p.map(f64::from)))
            })
            .collect();
        make_pivot_origin(&mut doc, id).unwrap();
        assert!(doc.layer(id).unwrap().pivot.is_identity());
        let world_after: Vec<DVec3> = first_points(&doc, id)
            .iter()
            .map(|p| {
                doc.world_from_layer(id)
                    .transform_point3(DVec3::from_array(p.map(f64::from)))
            })
            .collect();
        for (a, b) in world_before.iter().zip(&world_after) {
            assert!((*a - *b).length() < 1e-5, "content moved: {a:?} -> {b:?}");
        }
        // The content is now centred on the layer origin.
        let (lo, hi) = layer_bounds(&doc, id).unwrap();
        assert!(((lo + hi) * 0.5).length() < 1e-5);
    }

    #[test]
    fn merge_down_bakes_the_difference_in_transforms() {
        let mut doc = Document::empty();
        let root = doc.root();
        let upper = paint_with(&mut doc, root, "Upper", vec![line(0, 1.0)]);
        let lower = paint_with(&mut doc, root, "Lower", vec![line(0, 5.0)]);
        doc.layer_mut(upper).unwrap().transform =
            Xform::from_translation(DVec3::new(0.0, 0.0, 10.0));
        doc.layer_mut(lower).unwrap().transform = Xform {
            scale: 2.0,
            flip: Flip::X,
            ..Xform::IDENTITY
        };
        let upper_world = doc
            .world_from_layer(upper)
            .transform_point3(DVec3::new(1.0, 0.0, 0.0));

        let merged = merge_down(&mut doc, upper).unwrap();
        assert_eq!(merged, lower);
        assert!(doc.layer(upper).is_none());
        doc.validate().unwrap();
        let points = first_points(&doc, lower);
        assert_eq!(points.len(), 2);
        assert_eq!(
            points[0],
            [5.0, 0.0, 0.0],
            "the lower layer's own strokes are untouched"
        );
        let world = doc
            .world_from_layer(lower)
            .transform_point3(DVec3::from_array(points[1].map(f64::from)));
        assert!(
            (world - upper_world).length() < 1e-6,
            "the merged stroke stays where it was in the world"
        );
        // Ids stay unique inside the merged drawing.
        let paint = doc.layer(lower).unwrap().as_paint().unwrap();
        let ids: Vec<u32> = doc
            .drawing(paint.drawings[0])
            .unwrap()
            .strokes()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, [0, 1]);
        assert_eq!(doc.drawing_count(), 1, "the old drawings were collected");
        assert!(
            merge_down(&mut doc, lower).is_err(),
            "nothing below the last layer"
        );
    }

    #[test]
    fn merging_an_animated_layer_with_a_still_one_shows_the_still_on_every_frame() {
        let mut doc = Document::empty();
        let root = doc.root();
        let still = paint_with(&mut doc, root, "Still", vec![line(0, 9.0)]);
        let animated = paint_with(&mut doc, root, "Animated", vec![line(0, 0.0)]);
        {
            let second = doc.add_drawing(Drawing::from_strokes([line(0, 1.0)]));
            let paint = doc.layer_mut(animated).unwrap().as_paint_mut().unwrap();
            paint.drawings.push(second);
            paint.frames = vec![0, 0, 1];
        }
        merge_down(&mut doc, still).unwrap();
        doc.validate().unwrap();
        let paint = doc.layer(animated).unwrap().as_paint().unwrap().clone();
        assert_eq!(paint.frames, [0, 0, 1], "held frames still share a drawing");
        assert_eq!(paint.drawings.len(), 2);
        for drawing in &paint.drawings {
            assert_eq!(doc.drawing(*drawing).unwrap().stroke_count(), 2);
        }
    }

    #[test]
    fn layers_with_different_frame_counts_do_not_merge() {
        let mut doc = Document::empty();
        let root = doc.root();
        let a = paint_with(&mut doc, root, "A", vec![line(0, 0.0)]);
        let b = paint_with(&mut doc, root, "B", vec![line(0, 0.0)]);
        doc.layer_mut(a).unwrap().as_paint_mut().unwrap().frames = vec![0, 0];
        doc.layer_mut(b).unwrap().as_paint_mut().unwrap().frames = vec![0, 0, 0];
        assert!(matches!(merge_down(&mut doc, a), Err(OpError::Invalid(_))));
        assert!(doc.layer(a).is_some(), "a failed merge changes nothing");
    }

    #[test]
    fn flattening_a_group_keeps_strokes_in_place() {
        let mut doc = Document::empty();
        let root = doc.root();
        let group = add_layer(&mut doc, NewLayerKind::Group, root, 0, Some("Body"), 0).unwrap();
        let inner = add_layer(&mut doc, NewLayerKind::Group, group, 0, Some("Arm"), 0).unwrap();
        let hand = paint_with(&mut doc, inner, "Hand", vec![line(0, 1.0)]);
        let torso = paint_with(&mut doc, group, "Torso", vec![line(0, 2.0)]);
        doc.layer_mut(group).unwrap().transform =
            Xform::from_translation(DVec3::new(0.0, 5.0, 0.0));
        doc.layer_mut(inner).unwrap().transform = Xform {
            scale: 3.0,
            ..Xform::IDENTITY
        };
        doc.layer_mut(hand).unwrap().transform =
            Xform::from_rotation(DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2));
        let expected: Vec<DVec3> = [(hand, 1.0), (torso, 2.0)]
            .iter()
            .map(|(id, x)| {
                doc.world_from_layer(*id)
                    .transform_point3(DVec3::new(*x, 0.0, 0.0))
            })
            .collect();

        let flat = flatten_group(&mut doc, group).unwrap();
        doc.validate().unwrap();
        assert_eq!(names(&doc, root), ["Body"]);
        assert!(doc.layer(flat).unwrap().as_paint().is_some());
        let world: Vec<DVec3> = first_points(&doc, flat)
            .iter()
            .map(|p| {
                doc.world_from_layer(flat)
                    .transform_point3(DVec3::from_array(p.map(f64::from)))
            })
            .collect();
        for (a, b) in expected.iter().zip(&world) {
            assert!((*a - *b).length() < 1e-5);
        }
        assert_eq!(doc.layer_count(), 2);
    }

    #[test]
    fn groups_convert_to_sequences_and_back() {
        let mut doc = Document::empty();
        let root = doc.root();
        let group = add_layer(&mut doc, NewLayerKind::Group, root, 0, None, 0).unwrap();
        set_group_is_sequence(&mut doc, group, true).unwrap();
        assert!(doc.layer(group).unwrap().is_sequence());
        set_group_is_sequence(&mut doc, group, false).unwrap();
        assert!(!doc.layer(group).unwrap().is_sequence());
        assert!(set_group_is_sequence(&mut doc, root, false).is_err());
    }
}
