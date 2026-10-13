//! Timeline evaluation: what every layer looks like at a given moment.
//!
//! Evaluation is a pure function of the document and a clock, so scrubbing,
//! playback and rendering all agree. The rules follow the open-source IMM player,
//! which shares its timeline engine with Quill:
//!
//! * a layer's keys are read at the time of its nearest enclosing sequence;
//! * a clip is the span after a `true` visibility key and before the next `false` one;
//! * plain groups only gate visibility, while sequences and paint layers measure
//!   their own time from the start of their clip.

use crate::math::Xform;
use crate::model::{
    Document, DrawingId, Interp, Layer, LayerAnim, LayerId, LayerKind, Sequence, Track,
};
use crate::time::Flicks;
use glam::DAffine3;
use std::collections::HashMap;

/// Where playback is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Clock {
    /// Position of the root playhead.
    pub time: Flicks,
    /// Real time that has passed while the playhead was held at a Stop. The root
    /// timeline is frozen during it, but sequences keep running.
    pub held: Flicks,
}

impl Clock {
    pub fn at(time: Flicks) -> Self {
        Self { time, held: 0 }
    }
}

/// What a layer contributes at the evaluated moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Content {
    None,
    /// The frame a paint layer shows and the drawing behind it.
    Paint {
        frame: u32,
        drawing: DrawingId,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayerState {
    pub id: LayerId,
    /// Shown at this moment: its own eye toggle, an active clip, and the same for every ancestor.
    pub visible: bool,
    /// The layer is inside one of its clips, regardless of eye toggles and ancestors.
    pub in_clip: bool,
    /// Opacity including ancestors.
    pub opacity: f32,
    /// The layer's own opacity at this moment, from its keys or its static value.
    pub own_opacity: f32,
    /// The layer's own transform at this moment, from its keys or its static value.
    pub local: Xform,
    /// Placement in world space.
    pub world: DAffine3,
    /// The time this layer's keys were read at: the time of its enclosing sequence.
    pub timeline_time: Flicks,
    /// Time since the layer's clip began, including its trim. What a paint layer's
    /// frames, a sound's playback and a sequence's children run on.
    pub local_time: Flicks,
    pub content: Content,
}

/// The evaluated state of every layer, parents before children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderFrame {
    pub clock: Clock,
    pub layers: Vec<LayerState>,
    index: HashMap<LayerId, usize>,
}

impl RenderFrame {
    pub fn get(&self, id: LayerId) -> Option<&LayerState> {
        self.index.get(&id).map(|i| &self.layers[*i])
    }
}

/// The state of a layer's clips at one moment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClipState {
    pub active: bool,
    /// When the active clip began, in the enclosing sequence's time.
    pub start: Flicks,
    /// Time since the clip began plus its trim.
    pub local_time: Flicks,
}

/// Finds the clip that covers `time`. A layer with no visibility keys is always present.
pub fn clip_at(anim: &LayerAnim, time: Flicks) -> ClipState {
    let offset = anim.offset.key_at_or_before(time).map_or(0, |k| k.value);
    if anim.visibility.is_empty() {
        return ClipState {
            active: true,
            start: 0,
            local_time: time + offset,
        };
    }
    match anim.visibility.key_at_or_before(time) {
        Some(key) if key.value => ClipState {
            active: true,
            start: key.time,
            local_time: time - key.time + offset,
        },
        _ => ClipState {
            active: false,
            start: 0,
            local_time: 0,
        },
    }
}

/// The time a sequence's children see, given the time since the sequence's clip began.
pub fn sequence_time(sequence: &Sequence, local: Flicks) -> Flicks {
    if sequence.loop_duration <= 0 {
        return local;
    }
    let plays = local.div_euclid(sequence.loop_duration);
    if sequence.max_repeat == 0 || plays < sequence.max_repeat as i64 {
        local.rem_euclid(sequence.loop_duration)
    } else {
        sequence.loop_duration
    }
}

/// Evaluates a keyed value. Before the first key the first value holds; after the
/// last, the last. Between two keys the earlier key's interpolation decides.
pub fn eval_track<T: Clone>(
    track: &Track<T>,
    time: Flicks,
    mix: impl Fn(&T, &T, f64) -> T,
) -> Option<T> {
    let keys = track.keys();
    let first = keys.first()?;
    let Some(i) = track.index_at_or_before(time) else {
        return Some(first.value.clone());
    };
    let last = &keys[i];
    let Some(next) = keys.get(i + 1) else {
        return Some(last.value.clone());
    };
    if last.interp == Interp::Step || next.time <= last.time {
        return Some(last.value.clone());
    }
    let t = (time - last.time) as f64 / (next.time - last.time) as f64;
    Some(mix(&last.value, &next.value, last.interp.ease(t)))
}

/// A layer's own transform at `time` (the time of its enclosing sequence).
pub fn layer_transform_at(layer: &Layer, time: Flicks) -> Xform {
    eval_track(&layer.anim.transform, time, |a, b, t| {
        Xform::mix_about_pivot(a, b, &layer.pivot, t)
    })
    .unwrap_or(layer.transform)
}

/// A layer's own opacity at `time` (the time of its enclosing sequence).
pub fn layer_opacity_at(layer: &Layer, time: Flicks) -> f32 {
    eval_track(&layer.anim.opacity, time, |a, b, t| a + (b - a) * t as f32).unwrap_or(layer.opacity)
}

struct Context {
    /// Time of the enclosing sequence.
    timeline_time: Flicks,
    /// Held time still to be added when a nested sequence starts its own clock.
    held: Flicks,
    visible: bool,
    opacity: f32,
    world: DAffine3,
}

/// Evaluates every layer of the document at `clock`.
pub fn evaluate(doc: &Document, clock: Clock) -> RenderFrame {
    let mut frame = RenderFrame {
        clock,
        layers: Vec::with_capacity(doc.layer_count()),
        index: HashMap::with_capacity(doc.layer_count()),
    };
    let context = Context {
        timeline_time: clock.time,
        held: clock.held,
        visible: true,
        opacity: 1.0,
        world: DAffine3::IDENTITY,
    };
    visit(doc, doc.root(), &context, true, &mut frame);
    frame
}

fn visit(doc: &Document, id: LayerId, context: &Context, is_root: bool, frame: &mut RenderFrame) {
    let Some(layer) = doc.layer(id) else {
        return;
    };
    let time = context.timeline_time;
    let clip = clip_at(&layer.anim, time);
    let visible = context.visible && layer.visible && clip.active;
    let own_opacity = layer_opacity_at(layer, time).clamp(0.0, 1.0);
    let opacity = context.opacity * own_opacity;
    let local = layer_transform_at(layer, time);
    let world = context.world * local.to_affine();

    // The root is the main clock and is frozen while held. Any other sequence keeps
    // running, so it absorbs the held time when it starts its own clock.
    let mut local_time = clip.local_time;
    let mut child_time = time;
    let mut child_held = context.held;
    if let LayerKind::Group(group) = &layer.kind {
        if let Some(sequence) = &group.sequence {
            if !is_root {
                local_time += context.held;
                child_held = 0;
            }
            child_time = sequence_time(sequence, local_time);
        }
    }

    let content = match &layer.kind {
        LayerKind::Paint(paint) => {
            let index = paint.frame_at(local_time);
            match paint.drawing_at_frame(index) {
                Some(drawing) => Content::Paint {
                    frame: index as u32,
                    drawing,
                },
                None => Content::None,
            }
        }
        _ => Content::None,
    };

    frame.index.insert(id, frame.layers.len());
    frame.layers.push(LayerState {
        id,
        visible,
        in_clip: clip.active,
        opacity,
        own_opacity,
        local,
        world,
        timeline_time: time,
        local_time,
        content,
    });

    let child_context = Context {
        timeline_time: child_time,
        held: child_held,
        visible,
        opacity,
        world,
    };
    for child in layer.children() {
        visit(doc, *child, &child_context, false, frame);
    }
}

#[cfg(test)]
mod tests;
