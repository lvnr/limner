//! Commands that animate: frames of a paint layer, clips, keys, chapter marks and loops.

use super::query::kind_name;
use super::{CommandError, Session};
use crate::eval::{evaluate_children, layer_opacity_at, layer_transform_at, Content};
use crate::math::Xform;
use crate::model::{
    Action, Drawing, DrawingId, Interp, Key, Layer, LayerId, LayerKind, Paint, Track,
};
use crate::ops::clips::{self, clips_of};
use crate::ops::frames::{self, NewFrame, MAX_FRAMES};
use crate::ops::keys;
use crate::ops::layers::unique_name;
use crate::ops::strokes::transformed_stroke;
use crate::ops::OpError;
use crate::time::Flicks;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// The keyed tracks a command can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyTrack {
    Transform,
    Opacity,
    /// Chapter marks: stops, plays and loops. Only the root layer has them.
    Action,
}

/// Keys copied from a track, with times relative to the first of them.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum KeyClipboard {
    Transform(Vec<Key<Xform>>),
    Opacity(Vec<Key<f32>>),
    Action(Vec<Key<Action>>),
}

/// One frame of an animated layer drawn faintly around the current one, to draw against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OnionSkin {
    pub layer: LayerId,
    pub drawing: DrawingId,
    /// How many frames before (negative) or after (positive) the current one it is.
    pub offset: i32,
}

fn copied<T: Clone>(track: &Track<T>, times: &[Flicks]) -> Vec<Key<T>> {
    let mut keys: Vec<Key<T>> = times
        .iter()
        .filter_map(|t| track.get(*t).cloned())
        .collect();
    keys.sort_by_key(|k| k.time);
    let first = keys.first().map_or(0, |k| k.time);
    for key in &mut keys {
        key.time -= first;
    }
    keys
}

fn pasted<T: Clone>(track: &mut Track<T>, keys: &[Key<T>], at: Flicks) {
    for key in keys {
        track.insert(Key {
            time: at + key.time,
            value: key.value.clone(),
            interp: key.interp,
        });
    }
}

impl Session {
    fn layer_ref(&self, id: LayerId) -> Result<&Layer, CommandError> {
        Ok(self.doc.layer(id).ok_or(OpError::NoSuchLayer(id))?)
    }

    /// The frame a paint layer shows at the playhead: its index in the frame table,
    /// and how many frames have passed since the layer's clip began.
    fn frame_shown(&self, id: LayerId) -> Result<(usize, i64, Paint), CommandError> {
        let paint = self
            .layer_ref(id)?
            .as_paint()
            .ok_or(OpError::NotPaint(id))?
            .clone();
        let frame = self.evaluate();
        let state = frame.get(id).ok_or(OpError::NoSuchLayer(id))?;
        let index = match state.content {
            Content::Paint { frame, .. } => frame as usize,
            Content::None => 0,
        };
        Ok((index, paint.rate.frame_at(state.local_time), paint))
    }

    /// Moves the playhead by whole frames of the document.
    fn step_playhead(&mut self, frames: i64) {
        let step = self.doc.settings.rate.frame_duration();
        self.playback.seek(self.playback.time + step * frames);
    }

    // ---- frames ----

    pub(crate) fn add_frame(&mut self, id: LayerId, kind: NewFrame) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        let (index, passed, _) = self.frame_shown(id)?;
        let new_index = frames::add_frame(self.doc_mut(), id, index, kind)?;
        // Move on to the new frame, so that the next stroke lands on it.
        if passed == index as i64 {
            self.step_playhead(1);
        }
        Ok(json!({ "frame": new_index }))
    }

    pub(crate) fn add_hold_frame(
        &mut self,
        id: LayerId,
        after: bool,
    ) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        let (index, passed, _) = self.frame_shown(id)?;
        let new_index = frames::add_hold_frame(self.doc_mut(), id, index, after)?;
        if after && passed == index as i64 {
            self.step_playhead(1);
        }
        Ok(json!({ "frame": new_index }))
    }

    pub(crate) fn edit_frames(
        &mut self,
        id: LayerId,
        edit: impl FnOnce(&mut crate::model::Document, LayerId, usize) -> Result<(), OpError>,
    ) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        let (index, _, _) = self.frame_shown(id)?;
        edit(self.doc_mut(), id, index)?;
        Ok(Value::Null)
    }

    /// Copies the selected strokes into the frame the paint layer shows, or into
    /// every frame of it.
    pub(crate) fn copy_selection_to_frames(&mut self, all: bool) -> Result<Value, CommandError> {
        if self.selection.is_empty() {
            return Err(CommandError::invalid("Nothing is selected"));
        }
        let layer = self
            .paint_layer
            .ok_or_else(|| CommandError::invalid("There is no paint layer to copy into"))?;
        self.require_unlocked(layer)?;
        let (index, _, paint) = self.frame_shown(layer)?;
        let mut targets: Vec<DrawingId> = if all {
            paint.drawings.clone()
        } else {
            paint.drawing_at_frame(index).into_iter().collect()
        };
        targets.dedup();
        let frame = self.evaluate();
        let into_layer = frame.world_xform(&self.doc, layer).inverse();
        let mut strokes = Vec::new();
        for (drawing_id, ids) in self.selection.drawings() {
            let (Some(source_layer), Some(drawing)) = (
                self.doc.layer_of_drawing(drawing_id),
                self.doc.drawing(drawing_id),
            ) else {
                continue;
            };
            let to_layer = (source_layer != layer)
                .then(|| into_layer.mul(&frame.world_xform(&self.doc, source_layer)));
            for stroke in drawing.strokes().filter(|s| ids.contains(&s.id)) {
                let stroke = match &to_layer {
                    Some(x) => transformed_stroke(stroke, x),
                    None => (**stroke).clone(),
                };
                strokes.push((drawing_id, stroke));
            }
        }
        let mut copies = 0;
        for target in targets {
            let Some(drawing) = self.doc_mut().drawing_mut(target) else {
                continue;
            };
            // A stroke is not copied onto itself.
            let mut next_id = drawing.next_stroke_id();
            for (_, stroke) in strokes.iter().filter(|(from, _)| *from != target) {
                let mut copy = stroke.clone();
                copy.id = next_id;
                next_id = next_id.wrapping_add(1);
                drawing.push_stroke(copy);
                copies += 1;
            }
        }
        Ok(json!({ "copied": copies }))
    }

    /// Moves the playhead to the key frame before or after the one shown.
    pub(crate) fn go_to_key_frame(
        &mut self,
        id: LayerId,
        forward: bool,
    ) -> Result<(), CommandError> {
        let (index, _, paint) = self.frame_shown(id)?;
        let keys = frames::key_frames(&paint);
        let run_start = frames::hold_run(&paint, index).start;
        // From a hold, back goes to the key frame it holds; from a key frame, to the one before.
        let target = if forward {
            keys.iter().copied().find(|k| *k > index)
        } else if run_start < index {
            Some(run_start)
        } else {
            keys.iter().copied().rev().find(|k| *k < index)
        };
        if let Some(target) = target {
            let step = paint.rate.frame_duration();
            self.playback.pause();
            self.playback.seek(
                self.playback
                    .time
                    .saturating_add(step.saturating_mul(target as i64 - index as i64)),
            );
        }
        Ok(())
    }

    // ---- clips ----

    pub(crate) fn edit_clips(
        &mut self,
        id: LayerId,
        edit: impl FnOnce(&mut crate::model::Document, Flicks) -> Result<(), OpError>,
        time: Option<Flicks>,
    ) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        if id == self.doc.root() {
            return Err(CommandError::invalid("the root is always on the timeline"));
        }
        let time = time.unwrap_or_else(|| self.key_time(id)).max(0);
        edit(self.doc_mut(), time)?;
        Ok(Value::Null)
    }

    // ---- keys ----

    /// Writes a key holding the layer's present value on `track`, at `time` or the playhead.
    pub(crate) fn set_key(
        &mut self,
        id: LayerId,
        track: KeyTrack,
        time: Option<Flicks>,
        interp: Option<Interp>,
        children: bool,
    ) -> Result<Value, CommandError> {
        let targets = if children {
            self.doc.subtree(id)
        } else {
            vec![id]
        };
        let default_interp = self.options.default_interp;
        let mut written = 0;
        for target in targets {
            if self.layer_ref(target)?.locked {
                if target == id {
                    self.require_unlocked(id)?;
                }
                continue;
            }
            let time = time.unwrap_or_else(|| self.key_time(target)).max(0);
            let layer = self.doc_mut().layer_mut(target).expect("checked above");
            match track {
                KeyTrack::Transform => {
                    let value = layer_transform_at(layer, time);
                    let interp = interp
                        .or(layer.anim.transform.get(time).map(|k| k.interp))
                        .unwrap_or(default_interp);
                    layer.anim.transform.set(time, value, interp);
                }
                KeyTrack::Opacity => {
                    let value = layer_opacity_at(layer, time);
                    let interp = interp
                        .or(layer.anim.opacity.get(time).map(|k| k.interp))
                        .unwrap_or(default_interp);
                    layer.anim.opacity.set(time, value, interp);
                }
                KeyTrack::Action => {
                    return Err(CommandError::invalid(
                        "chapter marks are set with set_action",
                    ))
                }
            }
            written += 1;
        }
        Ok(json!({ "keys": written }))
    }

    /// Runs `edit` on one of a layer's tracks, whatever it holds.
    pub(crate) fn edit_track(
        &mut self,
        id: LayerId,
        track: KeyTrack,
        times: Vec<Flicks>,
        edit: TrackEdit,
    ) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        if track == KeyTrack::Action && id != self.doc.root() {
            return Err(CommandError::invalid(
                "only the root layer has chapter marks",
            ));
        }
        let times = if times.is_empty() {
            vec![self.key_time(id)]
        } else {
            times
        };
        let layer = self.doc_mut().layer_mut(id).expect("checked above");
        let count = match track {
            KeyTrack::Transform => edit.apply(&mut layer.anim.transform, &times),
            KeyTrack::Opacity => edit.apply(&mut layer.anim.opacity, &times),
            KeyTrack::Action => edit.apply(&mut layer.anim.actions, &times),
        };
        if count == 0 {
            return Err(CommandError::invalid("there is no key there"));
        }
        Ok(json!({ "keys": count }))
    }

    pub(crate) fn copy_keys(
        &mut self,
        id: LayerId,
        track: KeyTrack,
        times: Vec<Flicks>,
    ) -> Result<Value, CommandError> {
        let anim = &self.layer_ref(id)?.anim;
        let clipboard = match track {
            KeyTrack::Transform => KeyClipboard::Transform(copied(&anim.transform, &times)),
            KeyTrack::Opacity => KeyClipboard::Opacity(copied(&anim.opacity, &times)),
            KeyTrack::Action => KeyClipboard::Action(copied(&anim.actions, &times)),
        };
        let count = match &clipboard {
            KeyClipboard::Transform(keys) => keys.len(),
            KeyClipboard::Opacity(keys) => keys.len(),
            KeyClipboard::Action(keys) => keys.len(),
        };
        if count == 0 {
            return Err(CommandError::invalid("there are no keys there to copy"));
        }
        self.key_clipboard = Some(clipboard);
        Ok(json!({ "keys": count }))
    }

    /// The track the keys waiting to be pasted were copied from.
    pub fn copied_keys(&self) -> Option<KeyTrack> {
        self.key_clipboard
            .as_ref()
            .map(|clipboard| match clipboard {
                KeyClipboard::Transform(_) => KeyTrack::Transform,
                KeyClipboard::Opacity(_) => KeyTrack::Opacity,
                KeyClipboard::Action(_) => KeyTrack::Action,
            })
    }

    /// Pastes the copied keys onto `id`, the first of them at `time` or the playhead.
    pub(crate) fn paste_keys(
        &mut self,
        id: LayerId,
        time: Option<Flicks>,
    ) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        let clipboard = self
            .key_clipboard
            .clone()
            .ok_or_else(|| CommandError::invalid("no keys have been copied"))?;
        if matches!(clipboard, KeyClipboard::Action(_)) && id != self.doc.root() {
            return Err(CommandError::invalid(
                "only the root layer has chapter marks",
            ));
        }
        let at = time.unwrap_or_else(|| self.key_time(id)).max(0);
        let anim = &mut self.doc_mut().layer_mut(id).expect("checked above").anim;
        match &clipboard {
            KeyClipboard::Transform(keys) => pasted(&mut anim.transform, keys, at),
            KeyClipboard::Opacity(keys) => pasted(&mut anim.opacity, keys, at),
            KeyClipboard::Action(keys) => pasted(&mut anim.actions, keys, at),
        }
        Ok(Value::Null)
    }

    /// Moves the playhead to the key before or after it on one of a layer's tracks.
    pub(crate) fn go_to_key(
        &mut self,
        id: LayerId,
        track: KeyTrack,
        forward: bool,
    ) -> Result<(), CommandError> {
        let frame = self.evaluate();
        let now = frame.get(id).ok_or(OpError::NoSuchLayer(id))?.timeline_time;
        let anim = &self.layer_ref(id)?.anim;
        let target = match track {
            KeyTrack::Transform => keys::neighbour(&anim.transform, now, forward),
            KeyTrack::Opacity => keys::neighbour(&anim.opacity, now, forward),
            KeyTrack::Action => keys::neighbour(&anim.actions, now, forward),
        };
        if let Some(target) = target {
            // The layer's keys may be in the time of a sequence; the playhead moves by the same amount.
            self.playback.pause();
            self.playback.seek(self.playback.time + (target - now));
        }
        Ok(())
    }

    /// Puts a chapter mark on the root timeline at `time` or the playhead, or removes the one there.
    pub(crate) fn set_action(
        &mut self,
        time: Option<Flicks>,
        action: Option<Action>,
    ) -> Result<Value, CommandError> {
        let root = self.doc.root();
        let time = time.unwrap_or_else(|| self.key_time(root)).max(0);
        let actions = &mut self
            .doc_mut()
            .layer_mut(root)
            .expect("the root exists")
            .anim
            .actions;
        match action {
            Some(action) => actions.set(time, action, Interp::Step),
            None => {
                if actions.remove(time).is_none() {
                    return Err(CommandError::invalid("there is no chapter mark there"));
                }
            }
        }
        Ok(Value::Null)
    }

    /// Makes a sequence loop at the playhead, or stops it looping.
    pub(crate) fn set_loop_point(
        &mut self,
        id: LayerId,
        enabled: bool,
    ) -> Result<Value, CommandError> {
        self.require_unlocked(id)?;
        let frame = self.evaluate();
        let local = frame.get(id).ok_or(OpError::NoSuchLayer(id))?.local_time;
        let rate = self.doc.settings.rate;
        let sequence = self
            .doc_mut()
            .layer_mut(id)
            .expect("checked above")
            .as_group_mut()
            .and_then(|g| g.sequence.as_mut())
            .ok_or_else(|| CommandError::invalid("only a sequence can loop"))?;
        if enabled {
            let at = rate.snap_nearest(local);
            if at <= 0 {
                return Err(CommandError::invalid(
                    "move the playhead to where the sequence should loop first",
                ));
            }
            sequence.loop_duration = at;
        } else {
            sequence.loop_duration = 0;
        }
        Ok(json!({ "loop_duration": sequence.loop_duration }))
    }

    /// Turns one loop of a looping sequence into a single frame-by-frame paint layer:
    /// every frame holds the strokes of everything visible inside the sequence at that
    /// moment, in place. The sequence is kept, hidden.
    pub(crate) fn bake_sequence(&mut self, id: LayerId) -> Result<Value, CommandError> {
        let source = self.layer_ref(id)?.clone();
        let sequence = source
            .as_group()
            .and_then(|g| g.sequence)
            .filter(|s| s.loop_duration > 0)
            .ok_or_else(|| CommandError::invalid("only a looping sequence can be baked"))?;
        let parent = source
            .parent
            .ok_or_else(|| CommandError::invalid("the root cannot be baked"))?;
        let rate = self.doc.settings.rate;
        let step = rate.frame_duration().max(1);
        // Rounded up, without adding to a loop length that may be near the largest
        // number there is.
        let frames = sequence.loop_duration / step + i64::from(sequence.loop_duration % step != 0);
        let frame_count = usize::try_from(frames.max(1)).unwrap_or(usize::MAX);
        if frame_count > MAX_FRAMES {
            return Err(CommandError::invalid("the loop is too long to bake"));
        }

        // What a frame is made of: each visible drawing, where it sits, and how faded.
        type Parts = Vec<(DrawingId, Xform, f32)>;
        let mut seen: Vec<(Parts, u32)> = Vec::new();
        let mut drawings: Vec<Drawing> = Vec::new();
        let mut table: Vec<u32> = Vec::with_capacity(frame_count);
        for k in 0..frame_count {
            let frame = evaluate_children(&self.doc, id, rate.frame_start(k as i64));
            let parts: Parts = frame
                .layers
                .iter()
                .filter(|s| s.visible && s.opacity > 0.0)
                .filter_map(|s| match s.content {
                    Content::Paint { drawing, .. } => {
                        Some((drawing, frame.world_xform(&self.doc, s.id), s.opacity))
                    }
                    Content::None => None,
                })
                .collect();
            let index = match seen.iter().find(|(known, _)| *known == parts) {
                Some((_, index)) => *index,
                None => {
                    let mut combined = Drawing::new();
                    let mut next_id = 0u32;
                    for (drawing, placement, opacity) in &parts {
                        let Some(strokes) = self.doc.drawing(*drawing) else {
                            continue;
                        };
                        for stroke in strokes.strokes() {
                            let mut baked = transformed_stroke(stroke, placement);
                            baked.id = next_id;
                            next_id = next_id.wrapping_add(1);
                            if *opacity < 1.0 {
                                for point in baked.points_mut() {
                                    point.opacity *= opacity;
                                }
                            }
                            combined.push_stroke(baked);
                        }
                    }
                    drawings.push(combined);
                    let index = (drawings.len() - 1) as u32;
                    seen.push((parts, index));
                    index
                }
            };
            table.push(index);
        }

        let doc = self.doc_mut();
        let drawing_ids: Vec<DrawingId> =
            drawings.into_iter().map(|d| doc.add_drawing(d)).collect();
        let paint = Paint {
            rate,
            drawings: drawing_ids,
            frames: table,
            max_repeat: sequence.max_repeat,
            shading: Default::default(),
        };
        let name = unique_name(doc, parent, &format!("{}_baked", source.name));
        let mut baked = doc.new_layer(name, LayerKind::Paint(paint), 0);
        baked.transform = source.transform;
        baked.pivot = source.pivot;
        baked.opacity = source.opacity;
        baked.anim = source.anim.clone();
        let index = doc.index_in_parent(id).map_or(usize::MAX, |i| i + 1);
        let baked_id = doc.insert_layer(baked, parent, index)?;
        doc.layer_mut(id).expect("checked above").visible = false;
        self.set_current_layer(baked_id);
        Ok(json!({ "layer": baked_id, "frames": frame_count }))
    }

    // ---- what to draw around the current frame ----

    /// The neighbouring frames of the paint layer to show faintly, when onion
    /// skinning is on and playback is stopped.
    pub fn onion_skins(&self) -> Vec<OnionSkin> {
        if !self.options.onion_skin || self.playback.playing {
            return Vec::new();
        }
        let Some(layer) = self.paint_layer else {
            return Vec::new();
        };
        let Ok((index, _, paint)) = self.frame_shown(layer) else {
            return Vec::new();
        };
        let count = paint.frames.len() as i64;
        if count < 2 {
            return Vec::new();
        }
        let loops = paint.max_repeat == 0;
        let reach = if self.options.onion_frames == 0 {
            count - 1
        } else {
            (self.options.onion_frames as i64).min(count - 1)
        };
        let current = paint.drawing_at_frame(index);
        let mut skins: Vec<OnionSkin> = Vec::new();
        for distance in 1..=reach {
            for offset in [-distance, distance] {
                let at = index as i64 + offset;
                // A looping layer shows its first frame after its last, so loops can be closed.
                let at = if loops {
                    at.rem_euclid(count)
                } else if (0..count).contains(&at) {
                    at
                } else {
                    continue;
                };
                let Some(drawing) = paint.drawing_at_frame(at as usize) else {
                    continue;
                };
                if Some(drawing) != current && !skins.iter().any(|s| s.drawing == drawing) {
                    skins.push(OnionSkin {
                        layer,
                        drawing,
                        offset: offset as i32,
                    });
                }
            }
        }
        skins
    }

    /// What the timeline shows for every layer, in list order: clips, keys, frames
    /// and loops, with times in the layer's own time base, and where that time base
    /// sits on the root timeline.
    pub fn timeline_rows(&self) -> Value {
        let frame = self.evaluate();
        let mut origins: std::collections::HashMap<LayerId, Flicks> =
            std::collections::HashMap::new();
        let mut rows = Vec::with_capacity(self.doc.layer_count());
        for id in self.doc.walk() {
            let Some(layer) = self.doc.layer(id) else {
                continue;
            };
            let layer_clips = clips_of(&layer.anim);
            let state = frame.get(id);
            let origin = self.time_origin(layer, id, state, &mut origins);
            let keys_of = |times: Vec<(Flicks, Interp)>| -> Value {
                times
                    .into_iter()
                    .map(|(time, interp)| json!({ "time": time, "interp": interp }))
                    .collect()
            };
            let paint = layer.as_paint().map(|p| {
                // Drawings that hold no strokes, by their place in the frame table's numbering.
                let empty: Vec<usize> = p
                    .drawings
                    .iter()
                    .enumerate()
                    .filter(|(_, drawing)| {
                        self.doc
                            .drawing(**drawing)
                            .is_none_or(|d| d.stroke_count() == 0)
                    })
                    .map(|(index, _)| index)
                    .collect();
                json!({
                    "frames": p.frames,
                    "frame_duration": p.rate.frame_duration(),
                    "max_repeat": p.max_repeat,
                    "frame": shown_frame(state),
                    "empty": empty,
                })
            });
            let sequence = layer
                .as_group()
                .and_then(|g| g.sequence)
                .map(|s| json!({ "loop_duration": s.loop_duration, "max_repeat": s.max_repeat }));
            rows.push(json!({
                "id": id,
                "kind": kind_name(layer),
                "origin": origin,
                "time": state.map_or(0, |s| s.timeline_time),
                "local_time": state.map_or(0, |s| s.local_time),
                "clips": layer_clips,
                "transform_keys": keys_of(layer.anim.transform.keys().iter().map(|k| (k.time, k.interp)).collect()),
                "opacity_keys": layer.anim.opacity.keys().iter().map(|k| json!({ "time": k.time, "value": k.value, "interp": k.interp })).collect::<Vec<_>>(),
                "actions": layer.anim.actions.keys().iter().map(|k| json!({ "time": k.time, "action": k.value })).collect::<Vec<_>>(),
                "paint": paint,
                "sequence": sequence,
            }));
        }
        Value::Array(rows)
    }

    /// The part of [`Session::timeline_rows`] that moves with the playhead, for every
    /// layer in list order: where its time base sits on the root timeline, its own
    /// time, and the frame a paint layer shows. Cheap enough to ask for every frame.
    pub fn timeline_clock(&self) -> Value {
        let frame = self.evaluate();
        let mut origins: std::collections::HashMap<LayerId, Flicks> =
            std::collections::HashMap::new();
        let mut rows = Vec::with_capacity(self.doc.layer_count());
        for id in self.doc.walk() {
            let Some(layer) = self.doc.layer(id) else {
                continue;
            };
            let state = frame.get(id);
            let origin = self.time_origin(layer, id, state, &mut origins);
            rows.push(json!({
                "id": id,
                "origin": origin,
                "time": state.map_or(0, |s| s.timeline_time),
                "local_time": state.map_or(0, |s| s.local_time),
                "frame": shown_frame(state),
            }));
        }
        Value::Array(rows)
    }

    /// Root time at which a layer's own time base reads zero. Remembers in `origins`
    /// where the time base of a group's children sits, so layers must be asked for in
    /// list order.
    fn time_origin(
        &self,
        layer: &Layer,
        id: LayerId,
        state: Option<&crate::eval::LayerState>,
        origins: &mut std::collections::HashMap<LayerId, Flicks>,
    ) -> Flicks {
        let origin = layer
            .parent
            .and_then(|p| origins.get(&p).copied())
            .unwrap_or(0);
        if let Some(group) = layer.as_group() {
            // Children of a sequence count time from the start of its clip.
            let inner = match (&group.sequence, layer.parent) {
                (Some(_), Some(_)) => {
                    let now = state.map_or(0, |s| s.timeline_time);
                    let clips = clips_of(&layer.anim);
                    let clip = clips.iter().find(|c| c.contains(now)).or(clips.first());
                    origin + clip.map_or(0, |c| c.start - c.trim)
                }
                _ => origin,
            };
            origins.insert(id, inner);
        }
        origin
    }
}

/// The frame of its table a paint layer shows, or zero for anything else.
fn shown_frame(state: Option<&crate::eval::LayerState>) -> u32 {
    match state.map(|s| s.content) {
        Some(Content::Paint { frame, .. }) => frame,
        _ => 0,
    }
}

/// An edit to the keys of a track that does not depend on what the track holds.
#[derive(Clone, Copy, Debug)]
pub(crate) enum TrackEdit {
    Delete,
    Move(Flicks),
    SetInterp(Interp),
    Reverse,
}

impl TrackEdit {
    fn apply<T: Clone>(self, track: &mut Track<T>, times: &[Flicks]) -> usize {
        match self {
            TrackEdit::Delete => keys::delete_keys(track, times),
            TrackEdit::Move(delta) => keys::move_keys(track, times, delta),
            TrackEdit::SetInterp(interp) => {
                keys::set_interp(track, times, interp);
                // Keys that were already eased this way still count as found.
                times.iter().filter(|t| track.get(**t).is_some()).count()
            }
            TrackEdit::Reverse => keys::reverse_keys(track, times),
        }
    }
}

/// Runs a clip operation on a layer through [`Session::edit_clips`].
pub(crate) fn clip_op(
    id: LayerId,
    op: impl FnOnce(&mut crate::model::Document, LayerId, Flicks) -> Result<(), OpError>,
) -> impl FnOnce(&mut crate::model::Document, Flicks) -> Result<(), OpError> {
    move |doc, time| op(doc, id, time)
}

pub(crate) use clips::{
    delete_clip, duplicate_clip, move_clip, split_clip, toggle_clip_at, trim_clip,
};
