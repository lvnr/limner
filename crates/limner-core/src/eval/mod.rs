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
    Document, DrawingId, Interp, Key, Layer, LayerAnim, LayerId, LayerKind, Sequence, Track,
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

    /// Placement of a layer in the world at this moment, composed as a layer
    /// transform so that it can be inverted and re-based without leaving the family.
    pub fn world_xform(&self, doc: &Document, id: LayerId) -> Xform {
        let mut x = self.get(id).map_or(Xform::IDENTITY, |s| s.local);
        for ancestor in doc.ancestors(id) {
            if let Some(state) = self.get(ancestor) {
                x = state.local.mul(&x);
            }
        }
        x
    }

    /// The time the children of `parent` read their keys at.
    pub fn child_time(&self, doc: &Document, parent: LayerId) -> Flicks {
        let Some(state) = self.get(parent) else {
            return self.clock.time;
        };
        let sequence = doc
            .layer(parent)
            .and_then(|l| l.as_group())
            .and_then(|g| g.sequence.as_ref());
        match sequence {
            Some(sequence) => sequence_time(sequence, state.local_time),
            None => state.timeline_time,
        }
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

/// The keys around a moment that lies between two of them, for curves that pass
/// through their neighbours.
pub struct Span<'a, T> {
    pub before: Option<&'a Key<T>>,
    pub from: &'a Key<T>,
    pub to: &'a Key<T>,
    pub after: Option<&'a Key<T>>,
    /// How far from `from` to `to` the moment is, 0 to 1.
    pub t: f64,
}

/// Evaluates a keyed value. Before the first key the first value holds; after the
/// last, the last. Between two keys the earlier key's interpolation decides. A
/// smooth key moves as a linear one here; see [`eval_track_with`].
pub fn eval_track<T: Clone>(
    track: &Track<T>,
    time: Flicks,
    mix: impl Fn(&T, &T, f64) -> T,
) -> Option<T> {
    eval_track_with(track, time, &mix, |span| {
        mix(&span.from.value, &span.to.value, span.t)
    })
}

/// As [`eval_track`], with `curve` deciding how a smooth key moves to the next.
pub fn eval_track_with<T: Clone>(
    track: &Track<T>,
    time: Flicks,
    mix: impl Fn(&T, &T, f64) -> T,
    curve: impl Fn(&Span<T>) -> T,
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
    if last.interp == Interp::Smooth {
        return Some(curve(&Span {
            before: i.checked_sub(1).map(|b| &keys[b]),
            from: last,
            to: next,
            after: keys.get(i + 2),
            t,
        }));
    }
    Some(mix(&last.value, &next.value, last.interp.ease(t)))
}

/// A curve through the values of a span's keys that arrives at each key at its
/// time and passes it without a corner: a cubic whose slope at a key is that of
/// the straight line between the key's neighbours. `value` reads the quantity to
/// curve from a key.
pub fn curve_through<T, V>(span: &Span<T>, value: impl Fn(&T) -> V) -> V
where
    V: Copy
        + std::ops::Add<Output = V>
        + std::ops::Sub<Output = V>
        + std::ops::Mul<f64, Output = V>,
{
    let (p0, p1) = (value(&span.from.value), value(&span.to.value));
    let length = (span.to.time - span.from.time) as f64;
    let straight = p1 - p0;
    // Slopes are per unit of time, and scaled by the length of this stretch below.
    let slope = |a: &Key<T>, b: &Key<T>| {
        let between = (b.time - a.time) as f64;
        if between > 0.0 {
            (value(&b.value) - value(&a.value)) * (length / between)
        } else {
            straight
        }
    };
    let m0 = span
        .before
        .map_or(straight, |before| slope(before, span.to));
    let m1 = span.after.map_or(straight, |after| slope(span.from, after));
    let t = span.t;
    let (t2, t3) = (t * t, t * t * t);
    p0 * (2.0 * t3 - 3.0 * t2 + 1.0)
        + m0 * (t3 - 2.0 * t2 + t)
        + p1 * (-2.0 * t3 + 3.0 * t2)
        + m1 * (t3 - t2)
}

/// A layer's own transform at `time` (the time of its enclosing sequence).
pub fn layer_transform_at(layer: &Layer, time: Flicks) -> Xform {
    let pivot = &layer.pivot;
    eval_track_with(
        &layer.anim.transform,
        time,
        |a, b, t| Xform::mix_about_pivot(a, b, pivot, t),
        |span| {
            // The pivot travels a curve through where the keys put it, and the size
            // changes as smoothly. Turning stays the shortest way round at an even pace.
            let turned = Xform::mix_about_pivot(&span.from.value, &span.to.value, pivot, span.t);
            let mut placed = turned.mul(pivot);
            placed.translation = curve_through(span, |x: &Xform| x.mul(pivot).translation);
            placed.scale = curve_through(span, |x: &Xform| x.mul(pivot).scale)
                .max(span.from.value.scale.min(span.to.value.scale) * 1e-3);
            placed.mul(&pivot.inverse())
        },
    )
    .unwrap_or(layer.transform)
}

/// A layer's own opacity at `time` (the time of its enclosing sequence).
pub fn layer_opacity_at(layer: &Layer, time: Flicks) -> f32 {
    eval_track_with(
        &layer.anim.opacity,
        time,
        |a, b, t| a + (b - a) * t as f32,
        |span| curve_through(span, |v: &f32| *v as f64).clamp(0.0, 1.0) as f32,
    )
    .unwrap_or(layer.opacity)
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

/// Evaluates only the layers under `parent`, as they are when the time inside
/// `parent` is `time`: the frames of a sequence's loop, taken on their own. Placement
/// is relative to `parent`, and its own visibility and opacity are left out.
pub fn evaluate_children(doc: &Document, parent: LayerId, time: Flicks) -> RenderFrame {
    let mut frame = RenderFrame {
        clock: Clock::at(time),
        layers: Vec::new(),
        index: HashMap::new(),
    };
    let context = Context {
        timeline_time: time,
        held: 0,
        visible: true,
        opacity: 1.0,
        world: DAffine3::IDENTITY,
    };
    for child in doc.children(parent) {
        visit(doc, *child, &context, false, &mut frame);
    }
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
