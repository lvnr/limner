//! The tools that change existing strokes inside a sphere around the hand:
//! Colorize, Eraser, Thicken/Thin, Nudge, Optimize and Grab.
//!
//! None of them adds points. They work on the strokes of the selection when there is
//! one, and otherwise on the frame the paint layer shows.

use super::settings::ToolKind;
use super::space::PaintTarget;
use crate::eval::RenderFrame;
use crate::model::{Drawing, DrawingId, Point, Stroke};
use crate::ops::color::{blend, BlendMode};
use crate::ops::simplify::simplify_stroke;
use crate::session::input::InputFrame;
use crate::session::Session;
use glam::{DAffine3, DVec3, Vec3};
use std::collections::{BTreeSet, HashMap, HashSet};

/// How fast Thicken changes width at full strength: a factor of e per this many seconds.
const THICKEN_SECONDS: f32 = 0.4;
/// The part of the Eraser's sphere that erases completely.
const ERASER_CORE: f32 = 0.7;
/// How fast the stick pushes or pulls grabbed strokes: a factor of e per this many seconds.
const GRAB_SCALE_SECONDS: f64 = 0.7;

/// One drawing a tool may change, and which of its strokes.
pub(crate) struct EditTarget {
    pub target: PaintTarget,
    /// The ids of the strokes that may change; `None` means all of them.
    pub only: Option<BTreeSet<u32>>,
}

impl EditTarget {
    fn allows(&self, stroke: &Stroke) -> bool {
        self.only
            .as_ref()
            .is_none_or(|ids| ids.contains(&stroke.id))
    }
}

impl Session {
    /// What a modifying tool works on: the selected strokes if there are any,
    /// otherwise the frame the paint layer shows.
    pub(crate) fn edit_targets(&self, frame: &RenderFrame) -> Result<Vec<EditTarget>, String> {
        if self.selection.is_empty() {
            let layer = self
                .paint_layer
                .ok_or("There is no paint layer to work on")?;
            return Ok(vec![EditTarget {
                target: self.editable_drawing(frame, layer)?,
                only: None,
            }]);
        }
        let mut targets = Vec::new();
        for (drawing, ids) in self.selection.drawings() {
            let Some(layer) = self.doc.layer_of_drawing(drawing) else {
                continue;
            };
            let editable = self.doc.layer(layer).is_some_and(|l| !l.locked)
                && frame.get(layer).is_some_and(|s| s.visible);
            if editable {
                targets.push(EditTarget {
                    target: PaintTarget { layer, drawing },
                    only: Some(ids.clone()),
                });
            }
        }
        if targets.is_empty() {
            return Err("The selected strokes are on locked or hidden layers".into());
        }
        Ok(targets)
    }

    /// Changes one drawing. The closure says whether it changed anything, so that a
    /// tool waved through empty space does not count as an edit.
    pub(crate) fn edit_drawing(&mut self, id: DrawingId, edit: impl FnOnce(&mut Drawing) -> bool) {
        let Some(drawing) = self.doc.drawing_mut(id) else {
            return;
        };
        if edit(drawing) {
            self.edits += 1;
        }
    }
}

/// How strongly a tool acts at `distance` from its centre: fully inside `inner`,
/// fading smoothly to nothing at `radius`.
pub fn falloff(distance: f32, radius: f32, inner: f32) -> f32 {
    if distance >= radius {
        0.0
    } else if distance <= inner {
        1.0
    } else {
        let t = (radius - distance) / (radius - inner);
        t * t * (3.0 - 2.0 * t)
    }
}

/// Distance from `center` to the surface of the stroke at `point`, so that a thick
/// stroke is touched as soon as the sphere reaches its skin.
pub(crate) fn reach(point: &Point, center: Vec3) -> f32 {
    (point.position().distance(center) - point.width).max(0.0)
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// A stroke as it was when a gesture first touched it, and how strongly each of its
/// points has been touched since. Applying the tool to the original at the strongest
/// touch so far means that going over the same spot twice in one gesture does not
/// double the effect, while a second gesture does.
struct Touched {
    original: Stroke,
    weights: Vec<f32>,
}

/// Colorize, Eraser, Thicken/Thin, Nudge or Optimize at work.
pub(crate) struct SphereGesture {
    tool: ToolKind,
    touched: HashMap<(DrawingId, u32), Touched>,
    /// Strokes Optimize has already simplified in this gesture.
    simplified: HashSet<(DrawingId, u32)>,
    drawings: BTreeSet<DrawingId>,
    /// Where the hand was last frame, in viewer space.
    last_hand: DVec3,
}

impl SphereGesture {
    pub(crate) fn begin(
        session: &mut Session,
        input: &InputFrame,
        tool: ToolKind,
    ) -> Result<Self, String> {
        let frame = session.evaluate();
        session.edit_targets(&frame)?;
        let mut gesture = Self {
            tool,
            touched: HashMap::new(),
            simplified: HashSet::new(),
            drawings: BTreeSet::new(),
            last_hand: input.main.position,
        };
        gesture.update(session, input);
        Ok(gesture)
    }

    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        let frame = session.evaluate();
        let Ok(targets) = session.edit_targets(&frame) else {
            return true;
        };
        let pressure = input.main.trigger.clamp(0.0, 1.0);
        let tools = session.tools.clone();
        let thin = input.alt() > 0.5;
        let moved = input.main.position - self.last_hand;
        self.last_hand = input.main.position;

        for target in &targets {
            let Some(space) = session.layer_space(&frame, target.target.layer) else {
                continue;
            };
            let drawing_id = target.target.drawing;
            let center = space.point(input.main.position).as_vec3();
            let radius = space.length(tools.size() as f64) as f32;
            // With mirrored working on, the tool is also at work at the hand's
            // reflection: a point feels whichever of the two is nearer.
            let twin = session
                .mirror_in(target.target.layer)
                .map(|mirror| (mirror, mirror.transform_point(center.as_dvec3()).as_vec3()));
            let near = |p: &Point| -> f32 {
                let distance = reach(p, center);
                twin.map_or(distance, |(_, other)| distance.min(reach(p, other)))
            };
            let touched = &mut self.touched;
            let simplified = &mut self.simplified;
            let tool = self.tool;
            let mut changed = false;
            session.edit_drawing(drawing_id, |drawing| {
                let count = drawing.edit_strokes(
                    |bounds| {
                        bounds.intersects_sphere(center, radius)
                            || twin
                                .is_some_and(|(_, other)| bounds.intersects_sphere(other, radius))
                    },
                    |stroke| {
                        if !target.allows(stroke) {
                            return None;
                        }
                        match tool {
                            ToolKind::Colorize => {
                                let settings = tools.colorize;
                                let inner = radius * settings.inner;
                                let opacity_mode = settings.mode == BlendMode::Opacity;
                                // In Opacity mode the strength is the opacity aimed for, not how hard to push.
                                let push = if opacity_mode {
                                    pressure
                                } else {
                                    settings.strength * pressure
                                };
                                strengthen(touched, drawing_id, stroke, |p| {
                                    push * falloff(near(p), radius, inner)
                                })
                                .map(|entry| {
                                    rebuild(entry, stroke, |original, weight| {
                                        let mut p = *original;
                                        if opacity_mode {
                                            p.opacity = original.opacity
                                                + (settings.strength - original.opacity) * weight;
                                        } else {
                                            p.color = mix3(
                                                original.color,
                                                blend(settings.mode, original.color, tools.color),
                                                weight,
                                            );
                                        }
                                        p
                                    })
                                })
                            }
                            ToolKind::Eraser => {
                                let strength = tools.eraser.strength * pressure;
                                strengthen(touched, drawing_id, stroke, |p| {
                                    strength * falloff(near(p), radius, radius * ERASER_CORE)
                                })
                                .map(|entry| {
                                    rebuild(entry, stroke, |original, weight| Point {
                                        opacity: original.opacity * (1.0 - weight),
                                        ..*original
                                    })
                                })
                            }
                            ToolKind::Thicken => {
                                let rate = tools.thicken.strength * pressure * input.dt as f32
                                    / THICKEN_SECONDS
                                    * if thin { -1.0 } else { 1.0 };
                                reshape(stroke, |p| {
                                    let weight = falloff(near(p), radius, 0.0);
                                    (weight > 0.0 && p.width > 0.0).then(|| Point {
                                        width: p.width * (rate * weight).exp(),
                                        ..*p
                                    })
                                })
                            }
                            ToolKind::Nudge => {
                                let push = space.vector(moved).as_vec3()
                                    * (tools.nudge.strength * pressure);
                                if push == Vec3::ZERO {
                                    return None;
                                }
                                // On the far side of the mirror the push is mirrored too.
                                let twin_push = twin.map(|(mirror, other)| {
                                    (other, mirror.transform_vector(push.as_dvec3()).as_vec3())
                                });
                                reshape(stroke, |p| {
                                    let here = reach(p, center);
                                    let (distance, push) = match twin_push {
                                        Some((other, mirrored)) if reach(p, other) < here => {
                                            (reach(p, other), mirrored)
                                        }
                                        _ => (here, push),
                                    };
                                    let weight = falloff(distance, radius, 0.0);
                                    (weight > 0.0).then(|| Point {
                                        pos: (p.position() + push * weight).to_array(),
                                        ..*p
                                    })
                                })
                            }
                            ToolKind::Optimize => {
                                if simplified.contains(&(drawing_id, stroke.id))
                                    || !stroke.points().iter().any(|p| near(p) < radius)
                                {
                                    return None;
                                }
                                simplified.insert((drawing_id, stroke.id));
                                simplify_stroke(stroke, tools.optimize.threshold)
                            }
                            _ => None,
                        }
                    },
                );
                changed = count > 0;
                changed
            });
            if changed {
                self.drawings.insert(drawing_id);
            }
        }
        true
    }

    pub(crate) fn end(self, session: &mut Session) {
        if self.tool != ToolKind::Eraser {
            return;
        }
        // A stroke erased from end to end is gone, not merely invisible.
        for drawing in self.drawings {
            session.edit_drawing(drawing, |drawing| {
                drawing.retain(|stroke| !stroke.is_fully_erased()) > 0
            });
        }
    }
}

/// Raises the recorded touch of each point of `stroke` to `weight(point)` where that
/// is stronger. Returns the record when anything rose.
fn strengthen<'a>(
    touched: &'a mut HashMap<(DrawingId, u32), Touched>,
    drawing: DrawingId,
    stroke: &Stroke,
    weight: impl Fn(&Point) -> f32,
) -> Option<&'a Touched> {
    let fresh: Vec<f32> = stroke.points().iter().map(weight).collect();
    if fresh.iter().all(|w| *w <= 0.0) {
        return None;
    }
    let entry = touched
        .entry((drawing, stroke.id))
        .or_insert_with(|| Touched {
            original: stroke.clone(),
            weights: vec![0.0; stroke.len()],
        });
    if entry.weights.len() != fresh.len() {
        return None;
    }
    let mut rose = false;
    for (recorded, now) in entry.weights.iter_mut().zip(fresh) {
        if now > *recorded {
            *recorded = now.min(1.0);
            rose = true;
        }
    }
    rose.then_some(&*entry)
}

/// The stroke with `apply(original point, weight)` at every touched point.
fn rebuild(entry: &Touched, stroke: &Stroke, apply: impl Fn(&Point, f32) -> Point) -> Stroke {
    let points = entry
        .original
        .points()
        .iter()
        .zip(&entry.weights)
        .map(|(original, weight)| {
            if *weight > 0.0 {
                apply(original, *weight)
            } else {
                *original
            }
        })
        .collect();
    Stroke::new(stroke.id, stroke.brush, stroke.directional, points)
}

/// The stroke with every point `change` returns a replacement for, or `None` when it returns none.
fn reshape(stroke: &Stroke, change: impl Fn(&Point) -> Option<Point>) -> Option<Stroke> {
    let mut points: Option<Vec<Point>> = None;
    for (i, point) in stroke.points().iter().enumerate() {
        if let Some(replacement) = change(point) {
            points.get_or_insert_with(|| stroke.points().to_vec())[i] = replacement;
        }
    }
    points.map(|points| Stroke::new(stroke.id, stroke.brush, stroke.directional, points))
}

/// The pose of a hand as a transform of viewer space.
fn pose(input: &InputFrame) -> DAffine3 {
    DAffine3::from_rotation_translation(input.main.rotation, input.main.position)
}

/// The Grab tool at work: strokes near the hand when the trigger was pressed follow
/// the hand, rigidly inside the inner sphere and less and less toward the outer one.
pub(crate) struct GrabGesture {
    /// For each drawing, the strokes taken hold of, by id.
    held: HashMap<DrawingId, HashMap<u32, Touched>>,
    targets: Vec<PaintTarget>,
    start: DAffine3,
    /// How far the stick has pushed the strokes away from (above one) or pulled them toward the hand.
    scale: f64,
}

impl GrabGesture {
    pub(crate) fn begin(session: &mut Session, input: &InputFrame) -> Result<Self, String> {
        let frame = session.evaluate();
        let targets = session.edit_targets(&frame)?;
        let settings = session.tools.grab;
        let mut held: HashMap<DrawingId, HashMap<u32, Touched>> = HashMap::new();
        for target in &targets {
            let (Some(space), Some(drawing)) = (
                session.layer_space(&frame, target.target.layer),
                session.doc.drawing(target.target.drawing),
            ) else {
                continue;
            };
            let center = space.point(input.main.position).as_vec3();
            let radius = space.length(settings.size as f64) as f32;
            let inner = radius * settings.inner;
            for stroke in drawing.strokes() {
                if !target.allows(stroke) || !stroke.bounds().intersects_sphere(center, radius) {
                    continue;
                }
                let weights: Vec<f32> = stroke
                    .points()
                    .iter()
                    .map(|p| settings.strength * falloff(reach(p, center), radius, inner))
                    .collect();
                if weights.iter().any(|w| *w > 0.0) {
                    held.entry(target.target.drawing).or_default().insert(
                        stroke.id,
                        Touched {
                            original: (**stroke).clone(),
                            weights,
                        },
                    );
                }
            }
        }
        Ok(Self {
            held,
            targets: targets.iter().map(|t| t.target).collect(),
            start: pose(input),
            scale: 1.0,
        })
    }

    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        self.scale = (self.scale
            * (input.main.stick[1] as f64 * input.dt / GRAB_SCALE_SECONDS).exp())
        .clamp(0.01, 100.0);
        let frame = session.evaluate();
        let motion =
            pose(input) * DAffine3::from_scale(DVec3::splat(self.scale)) * self.start.inverse();
        for target in &self.targets {
            let (Some(space), Some(held)) = (
                session.layer_space(&frame, target.layer),
                self.held.get(&target.drawing),
            ) else {
                continue;
            };
            let in_layer = space.layer_from_viewer * motion * space.layer_from_viewer.inverse();
            let scale = self.scale as f32;
            session.edit_drawing(target.drawing, |drawing| {
                drawing.edit_strokes(
                    |_| true,
                    |stroke| {
                        held.get(&stroke.id)
                            .map(|entry| carried(entry, stroke, &in_layer, scale))
                    },
                ) > 0
            });
        }
        true
    }

    pub(crate) fn end(self, _session: &mut Session) {}
}

/// The held stroke with each point moved along `motion` by its weight.
fn carried(entry: &Touched, stroke: &Stroke, motion: &DAffine3, scale: f32) -> Stroke {
    let points = entry
        .original
        .points()
        .iter()
        .zip(&entry.weights)
        .map(|(original, weight)| {
            if *weight <= 0.0 {
                return *original;
            }
            let w = *weight as f64;
            let position = original.position().as_dvec3();
            let normal = Vec3::from_array(original.normal).as_dvec3();
            let turned = motion.transform_vector3(normal).normalize_or(normal);
            Point {
                pos: position
                    .lerp(motion.transform_point3(position), w)
                    .as_vec3()
                    .to_array(),
                normal: normal
                    .lerp(turned, w)
                    .normalize_or(normal)
                    .as_vec3()
                    .to_array(),
                width: original.width * (1.0 + (scale - 1.0) * weight),
                ..*original
            }
        })
        .collect();
    Stroke::new(stroke.id, stroke.brush, stroke.directional, points)
}
