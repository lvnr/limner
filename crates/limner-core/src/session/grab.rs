//! Taking hold with the grips.
//!
//! A grip moves whatever is the transform object: the view by default, a layer when
//! one was chosen, the selected strokes when the Selection tool reaches them, or one
//! handle of the gimbal. A second grip adds scale. Holding the modifier when the
//! grab starts moves a copy instead, and that move can then be repeated.

use super::gesture::Gesture;
use super::gimbal::{GimbalHandle, HandleDrag};
use super::input::{Edge, Feedback, HandInput, InputFrame, REST};
use super::options::AutoSelect;
use super::tools::sphere::reach;
use super::tools::{PaintTarget, ToolKind};
use super::view::DEFAULT_ZOOM_LIMITS;
use super::{CommandError, Session};
use crate::eval::{Content, RenderFrame};
use crate::math::{Aabb, Flip, Xform};
use crate::model::{DrawingId, LayerId, Stroke};
use crate::ops::frames::{self, NewFrame};
use crate::ops::layers;
use crate::ops::strokes::transformed_stroke;
use glam::{DQuat, DVec3};
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};

/// A grip takes hold past this and lets go below [`GRIP_RELEASE`].
pub const GRIP_PRESS: f32 = 0.35;
pub const GRIP_RELEASE: f32 = 0.2;
/// A grip pressed more lightly than this steadies the motion: the lighter, the steadier.
const FIRM_GRIP: f32 = 0.9;
const STEADY_SECONDS: f64 = 0.2;
/// How fast the stick zooms while a grip is held: a factor of e per this many seconds.
const ZOOM_SECONDS: f64 = 0.45;
/// A hand this close to a stroke takes hold of its layer, when grabbing strokes moves
/// layers. Viewer-space metres.
const TOUCH: f64 = 0.03;

/// What a grab moves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    /// The viewer's placement in the world: navigation.
    View,
    Layer(LayerId),
    /// The selected strokes.
    Selection,
}

/// A duplicate-and-move that can be done again.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Repeat {
    /// Selected strokes were copied and the copies moved by this much, in world space.
    Strokes { motion: Xform },
    /// A layer was copied and the copy moved by this much, in its parent's space.
    Layer { layer: LayerId, motion: Xform },
}

/// The custom place of the selection's pivot, valid for one state of the selection.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SelectionPivot {
    revision: u64,
    world: DVec3,
}

#[derive(Clone, Copy, Debug)]
struct Pose {
    position: DVec3,
    rotation: DQuat,
}

impl Pose {
    fn of(hand: &HandInput) -> Self {
        Self {
            position: hand.position,
            rotation: hand.rotation,
        }
    }

    fn toward(self, target: Pose, amount: f64) -> Pose {
        Pose {
            position: self.position.lerp(target.position, amount),
            rotation: self.rotation.slerp(target.rotation, amount).normalize(),
        }
    }
}

/// How much of the way to the hand's true pose a held pose moves in `dt` seconds.
fn steadiness(grip: f32, dt: f64) -> f64 {
    let firm = ((grip - GRIP_PRESS) / (FIRM_GRIP - GRIP_PRESS)).clamp(0.0, 1.0) as f64;
    let seconds = STEADY_SECONDS * (1.0 - firm) * (1.0 - firm);
    if seconds <= 1e-6 {
        1.0
    } else {
        1.0 - (-dt / seconds).exp()
    }
}

/// The part of a rotation that turns about `axis`.
fn twist(rotation: DQuat, axis: DVec3) -> DQuat {
    let along = axis * DVec3::new(rotation.x, rotation.y, rotation.z).dot(axis);
    let twist = DQuat::from_xyzw(along.x, along.y, along.z, rotation.w);
    if twist.length_squared() < 1e-18 {
        DQuat::IDENTITY
    } else {
        twist.normalize()
    }
}

/// Rotation and scale about one point, which itself moves to `to`.
fn about_moving(from: DVec3, to: DVec3, rotation: DQuat, scale: f64) -> Xform {
    Xform {
        rotation,
        scale,
        translation: to - rotation * (scale * from),
        flip: Flip::None,
    }
}

/// How a subject was when it was taken hold of, so that it can be put anywhere from
/// there without many small steps adding up their errors.
enum Start {
    View {
        world_from_viewer: Xform,
    },
    Layer {
        id: LayerId,
        /// The layer's placement in viewer space.
        in_viewer: Xform,
        /// Its own transform.
        local: Xform,
    },
    Selection {
        strokes: HashMap<DrawingId, HashMap<u32, Stroke>>,
        targets: Vec<PaintTarget>,
        pivot_world: DVec3,
    },
}

/// A subject taken hold of.
pub(crate) struct Carried {
    pub(crate) subject: Subject,
    /// Where the subject's pivot was in viewer space.
    pub(crate) pivot: DVec3,
    start: Start,
    /// A copy was made, and it is the copy that moves.
    duplicate: bool,
}

impl Carried {
    /// The name of the undo step moving `subject` makes; `None` for the view.
    pub(crate) fn label(subject: Subject, duplicate: bool) -> Option<&'static str> {
        match (subject, duplicate) {
            (Subject::View, _) => None,
            (Subject::Layer(_), false) => Some("Move layer"),
            (Subject::Layer(_), true) => Some("Duplicate layer"),
            (Subject::Selection, false) => Some("Move strokes"),
            (Subject::Selection, true) => Some("Duplicate strokes"),
        }
    }

    pub(crate) fn begin(
        session: &mut Session,
        subject: Subject,
        duplicate: bool,
    ) -> Result<Self, String> {
        match subject {
            Subject::View => Ok(Self {
                subject,
                pivot: DVec3::ZERO,
                start: Start::View {
                    world_from_viewer: session.view.world_from_viewer,
                },
                duplicate: false,
            }),
            Subject::Layer(id) => {
                let layer = session.doc.layer(id).ok_or("That layer no longer exists")?;
                if layer.locked {
                    return Err(format!("Layer \"{}\" is locked", layer.name));
                }
                if id == session.doc.root() {
                    return Err("The root layer stays where it is".into());
                }
                let id = if duplicate {
                    let copy = layers::duplicate_layer(session.doc_mut(), id)
                        .map_err(|e| e.to_string())?;
                    if session.transform_target == Some(id) {
                        session.transform_target = Some(copy);
                    }
                    session.set_current_layer(copy);
                    copy
                } else {
                    id
                };
                let frame = session.evaluate();
                let local = frame
                    .get(id)
                    .map(|s| s.local)
                    .ok_or("That layer no longer exists")?;
                let pivot = session.doc.layer(id).map_or(Xform::IDENTITY, |l| l.pivot);
                let in_viewer = session
                    .view
                    .viewer_from_world()
                    .mul(&frame.world_xform(&session.doc, id));
                Ok(Self {
                    subject: Subject::Layer(id),
                    pivot: in_viewer.transform_point(pivot.translation),
                    start: Start::Layer {
                        id,
                        in_viewer,
                        local,
                    },
                    duplicate,
                })
            }
            Subject::Selection => {
                if session.selection.is_empty() {
                    return Err("Nothing is selected".into());
                }
                let frame = session.evaluate();
                let targets = session.edit_targets(&frame)?;
                let pivot_world = session.selection_pivot().unwrap_or(DVec3::ZERO);
                if duplicate {
                    session.duplicate_selected(&targets);
                }
                let mut strokes: HashMap<DrawingId, HashMap<u32, Stroke>> = HashMap::new();
                let mut held = Vec::new();
                for target in &targets {
                    let (Some(drawing), Some(ids)) = (
                        session.doc.drawing(target.target.drawing),
                        session.selection.in_drawing(target.target.drawing),
                    ) else {
                        continue;
                    };
                    let originals: HashMap<u32, Stroke> = drawing
                        .strokes()
                        .filter(|s| ids.contains(&s.id))
                        .map(|s| (s.id, (**s).clone()))
                        .collect();
                    if !originals.is_empty() {
                        strokes.insert(target.target.drawing, originals);
                        held.push(target.target);
                    }
                }
                session.set_selection_pivot(pivot_world);
                Ok(Self {
                    subject,
                    pivot: session
                        .view
                        .viewer_from_world()
                        .transform_point(pivot_world),
                    start: Start::Selection {
                        strokes,
                        targets: held,
                        pivot_world,
                    },
                    duplicate,
                })
            }
        }
    }

    /// Puts the subject where `motion` takes it from where it started. `motion` maps
    /// viewer space as it was when the grab began to how it should be now.
    pub(crate) fn apply(&self, session: &mut Session, motion: &Xform) {
        if !(motion.is_finite() && motion.scale > 0.0) {
            return;
        }
        match &self.start {
            Start::View { world_from_viewer } => {
                let mut next = world_from_viewer.mul(&motion.inverse());
                next.rotation = next.rotation.normalize();
                let (smallest, largest) = DEFAULT_ZOOM_LIMITS;
                if next.is_finite() && next.scale >= smallest && next.scale <= largest {
                    session.view.world_from_viewer = next;
                }
            }
            Start::Layer { id, in_viewer, .. } => {
                let Some(parent) = session.doc.parent_of(*id) else {
                    return;
                };
                let frame = session.evaluate();
                let parent_in_viewer = session
                    .view
                    .viewer_from_world()
                    .mul(&frame.world_xform(&session.doc, parent));
                let local = parent_in_viewer.inverse().mul(motion).mul(in_viewer);
                let _ = session.write_transform_as(*id, local, true);
            }
            Start::Selection {
                strokes,
                targets,
                pivot_world,
            } => {
                let frame = session.evaluate();
                let viewer_from_world = session.view.viewer_from_world();
                for target in targets {
                    let Some(held) = strokes.get(&target.drawing) else {
                        continue;
                    };
                    let layer_in_viewer =
                        viewer_from_world.mul(&frame.world_xform(&session.doc, target.layer));
                    let in_layer = layer_in_viewer.inverse().mul(motion).mul(&layer_in_viewer);
                    session.edit_drawing(target.drawing, |drawing| {
                        drawing.edit_strokes(
                            |_| true,
                            |stroke| {
                                held.get(&stroke.id)
                                    .map(|original| transformed_stroke(original, &in_layer))
                            },
                        ) > 0
                    });
                }
                let in_world = session
                    .view
                    .world_from_viewer
                    .mul(motion)
                    .mul(&viewer_from_world);
                session.set_selection_pivot(in_world.transform_point(*pivot_world));
            }
        }
    }

    /// What doing this again would be, when it moved a copy.
    pub(crate) fn repeat(&self, session: &Session, motion: &Xform) -> Option<Repeat> {
        if !self.duplicate {
            return None;
        }
        match &self.start {
            Start::View { .. } => None,
            Start::Layer { id, local, .. } => {
                let now = session.evaluate().get(*id)?.local;
                Some(Repeat::Layer {
                    layer: *id,
                    motion: now.mul(&local.inverse()),
                })
            }
            Start::Selection { .. } => Some(Repeat::Strokes {
                motion: session
                    .view
                    .world_from_viewer
                    .mul(motion)
                    .mul(&session.view.viewer_from_world()),
            }),
        }
    }
}

/// A grab by one hand or both, anywhere.
struct Free {
    holding: [bool; 2],
    /// Where the hands are, steadied.
    poses: [Pose; 2],
    /// Where they were when this stretch of the grab began: since a hand last joined
    /// or let go.
    anchor: [Pose; 2],
    /// The motion from the stretches before.
    base: Xform,
    /// What the stick has added in this stretch.
    zoom: f64,
    /// Where the grab first took hold, in viewer space.
    origin: DVec3,
}

impl Free {
    fn stretch(&self) -> Xform {
        match self.holding {
            [true, true] => {
                let (a0, b0) = (self.anchor[0].position, self.anchor[1].position);
                let (a, b) = (self.poses[0].position, self.poses[1].position);
                let (before, after) = (b0 - a0, b - a);
                let (rotation, scale) = match (before.try_normalize(), after.try_normalize()) {
                    (Some(u), Some(v)) if before.length() > 1e-6 => (
                        DQuat::from_rotation_arc(u, v),
                        after.length() / before.length(),
                    ),
                    _ => (DQuat::IDENTITY, 1.0),
                };
                about_moving((a0 + b0) * 0.5, (a + b) * 0.5, rotation, scale)
            }
            [true, false] | [false, true] => {
                let i = usize::from(!self.holding[0]);
                let rotation =
                    (self.poses[i].rotation * self.anchor[i].rotation.inverse()).normalize();
                about_moving(
                    self.anchor[i].position,
                    self.poses[i].position,
                    rotation,
                    self.zoom,
                )
            }
            [false, false] => Xform::IDENTITY,
        }
    }

    fn total(&self) -> Xform {
        self.stretch().mul(&self.base)
    }

    /// Starts a new stretch from where things are now.
    fn rebase(&mut self) {
        self.base = self.total();
        self.anchor = self.poses;
        self.zoom = 1.0;
    }
}

enum How {
    Free(Free),
    /// By a handle of the gimbal, with one hand.
    Handle {
        hand: usize,
        drag: HandleDrag,
    },
}

/// Something held by the grips.
pub(crate) struct Grab {
    carried: Carried,
    how: How,
    /// Whether it changes the document, and so runs inside a transaction.
    edits: bool,
    /// The motion applied last.
    motion: Xform,
}

impl Grab {
    pub(crate) fn handle(&self) -> Option<GimbalHandle> {
        match &self.how {
            How::Handle { drag, .. } => Some(drag.handle),
            How::Free(_) => None,
        }
    }

    pub(crate) fn subject(&self) -> Subject {
        self.carried.subject
    }

    fn holds(&self, hand: usize) -> bool {
        match &self.how {
            How::Free(free) => free.holding[hand],
            How::Handle { hand: held, .. } => *held == hand,
        }
    }
}

impl Session {
    /// True while the grip of that hand (0 the drawing hand, 1 the other) holds something.
    pub fn is_grabbing(&self, hand: usize) -> bool {
        self.grab.as_ref().is_some_and(|g| g.holds(hand))
    }

    /// What the grips hold now.
    pub fn grabbed(&self) -> Option<Subject> {
        self.grab.as_ref().map(Grab::subject)
    }

    // ---- the selection's pivot ----

    /// The middle of the selected strokes, in world space. Strokes on frames that are
    /// not showing count only when none of the selection is showing.
    fn selection_center(&self, frame: &RenderFrame) -> Option<DVec3> {
        let mut shown = (DVec3::INFINITY, DVec3::NEG_INFINITY);
        let mut all = shown;
        for (drawing_id, ids) in self.selection.drawings() {
            let (Some(layer), Some(drawing)) = (
                self.doc.layer_of_drawing(drawing_id),
                self.doc.drawing(drawing_id),
            ) else {
                continue;
            };
            let Some(state) = frame.get(layer) else {
                continue;
            };
            let bounds = drawing
                .strokes()
                .filter(|s| ids.contains(&s.id))
                .fold(Aabb::EMPTY, |b, s| b.union(&s.bounds()));
            let Some((lo, hi)) = bounds.transformed(&state.world) else {
                continue;
            };
            all = (all.0.min(lo), all.1.max(hi));
            if matches!(state.content, Content::Paint { drawing, .. } if drawing == drawing_id) {
                shown = (shown.0.min(lo), shown.1.max(hi));
            }
        }
        let (lo, hi) = if shown.0.x <= shown.1.x { shown } else { all };
        (lo.x <= hi.x).then(|| (lo + hi) * 0.5)
    }

    /// The point the selection turns and scales about, in world space: where it was
    /// put, or else the middle of the selected strokes.
    pub fn selection_pivot(&self) -> Option<DVec3> {
        if self.selection.is_empty() {
            return None;
        }
        match self.selection_pivot {
            Some(pivot) if pivot.revision == self.selection.revision() => Some(pivot.world),
            _ => self.selection_center(&self.evaluate()),
        }
    }

    pub(crate) fn set_selection_pivot(&mut self, world: DVec3) {
        if world.is_finite() {
            self.selection_pivot = Some(SelectionPivot {
                revision: self.selection.revision(),
                world,
            });
        }
    }

    // ---- what a grip takes hold of ----

    /// True when the sphere of `radius` viewer-space metres around `hand` reaches a
    /// selected stroke that is showing.
    fn touches_selection(&self, frame: &RenderFrame, hand: DVec3, radius: f64) -> bool {
        self.selection.drawings().any(|(drawing_id, ids)| {
            let Some(layer) = self.doc.layer_of_drawing(drawing_id) else {
                return false;
            };
            let showing = frame.get(layer).is_some_and(|s| {
                s.visible
                    && matches!(s.content, Content::Paint { drawing, .. } if drawing == drawing_id)
            });
            let (true, Some(space), Some(drawing)) = (
                showing,
                self.layer_space(frame, layer),
                self.doc.drawing(drawing_id),
            ) else {
                return false;
            };
            let center = space.point(hand).as_vec3();
            let radius = space.length(radius) as f32;
            drawing.strokes().any(|stroke| {
                ids.contains(&stroke.id)
                    && stroke.bounds().intersects_sphere(center, radius)
                    && stroke.points().iter().any(|p| reach(p, center) < radius)
            })
        })
    }

    /// What a grip pressed at `hand` takes hold of.
    fn grab_subject(&self, hand: &HandInput) -> Subject {
        let frame = self.evaluate();
        if self.tools.tool == ToolKind::Select
            && !self.selection.is_empty()
            && self.touches_selection(&frame, hand.position, self.tools.select.size as f64)
        {
            return Subject::Selection;
        }
        let root = self.doc.root();
        match self.options.auto_select {
            AutoSelect::None => self
                .transform_target
                .filter(|id| *id != root)
                .map_or(Subject::View, Subject::Layer),
            mode => {
                let layers: Vec<LayerId> = self
                    .visible_paint_layers(&frame)
                    .into_iter()
                    .filter(|id| self.doc.layer(*id).is_some_and(|l| !l.locked))
                    .collect();
                match self.nearest_stroke(&frame, &layers, hand.position, TOUCH) {
                    Some(nearest) => {
                        let layer = nearest.target.layer;
                        let group = self.doc.parent_of(layer).filter(|parent| *parent != root);
                        Subject::Layer(match (mode, group) {
                            (AutoSelect::Group, Some(group)) => group,
                            _ => layer,
                        })
                    }
                    None => Subject::View,
                }
            }
        }
    }

    /// Copies the selected strokes of `targets` in place and selects the copies.
    fn duplicate_selected(&mut self, targets: &[super::tools::sphere::EditTarget]) {
        for target in targets {
            let drawing_id = target.target.drawing;
            let Some(ids) = self.selection.in_drawing(drawing_id).cloned() else {
                continue;
            };
            let mut copies = BTreeSet::new();
            self.edit_drawing(drawing_id, |drawing| {
                let originals: Vec<Stroke> = drawing
                    .strokes()
                    .filter(|s| ids.contains(&s.id))
                    .map(|s| (**s).clone())
                    .collect();
                for mut copy in originals {
                    copy.id = drawing.next_stroke_id();
                    copies.insert(copy.id);
                    drawing.push_stroke(copy);
                }
                !copies.is_empty()
            });
            self.selection.set_drawing(drawing_id, copies);
        }
        self.touch_state();
    }

    // ---- the grips ----

    /// Reads the grips: takes hold, carries along, lets go.
    pub(crate) fn update_grabs(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        let hands = [input.main, input.off];
        let mut grips = self.grips;
        let mut edges = [Edge::None; 2];
        for (i, hand) in hands.iter().enumerate() {
            // The drawing hand's grip belongs to a line being drawn: it moves where
            // the line starts.
            let taken = i == 0 && matches!(self.gesture, Some(Gesture::Line(_)));
            let value = if hand.tracked && !taken {
                hand.grip
            } else {
                0.0
            };
            edges[i] = grips[i].update(value, GRIP_PRESS, GRIP_RELEASE);
        }
        self.grips = grips;
        self.refresh_hover(input);
        for (i, edge) in edges.iter().enumerate() {
            match edge {
                Edge::Pressed => self.grip_pressed(i, input, feedback),
                Edge::Released => self.grip_released(i),
                Edge::Held | Edge::None => {}
            }
        }
        self.carry(input);
    }

    fn grip_pressed(&mut self, hand: usize, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        let hands = [input.main, input.off];
        if let Some(grab) = &mut self.grab {
            // A second hand joins in; a handle is held by one hand alone.
            if let How::Free(free) = &mut grab.how {
                free.rebase();
                free.holding[hand] = true;
                free.poses[hand] = Pose::of(&hands[hand]);
                free.anchor = free.poses;
            }
            return;
        }
        let handle = self.hover[hand].zip(self.gimbal());
        let subject = match &handle {
            Some((_, gimbal)) => gimbal.subject,
            None => self.grab_subject(&hands[hand]),
        };
        let duplicate = input.alt() > 0.5 && subject != Subject::View;
        let label = Carried::label(subject, duplicate);
        if let Some(label) = label {
            self.begin_transaction(label);
        }
        let carried = match Carried::begin(self, subject, duplicate) {
            Ok(carried) => carried,
            Err(message) => {
                if label.is_some() {
                    self.cancel_transaction();
                }
                feedback.push(Feedback::Message(message));
                return;
            }
        };
        let how = match handle {
            // The middle of the gimbal, held in the hand, is an ordinary grab of the
            // thing; pointed at from a screen it slides it across the view.
            Some((handle, gimbal))
                if handle != GimbalHandle::Center || hands[hand].ray.is_some() =>
            {
                How::Handle {
                    hand,
                    drag: HandleDrag::begin(&gimbal, handle, &hands[hand], input.head),
                }
            }
            _ => {
                let pose = Pose::of(&hands[hand]);
                let mut holding = [false; 2];
                holding[hand] = true;
                How::Free(Free {
                    holding,
                    poses: [pose; 2],
                    anchor: [pose; 2],
                    base: Xform::IDENTITY,
                    zoom: 1.0,
                    origin: pose.position,
                })
            }
        };
        self.grab = Some(Grab {
            carried,
            how,
            edits: label.is_some(),
            motion: Xform::IDENTITY,
        });
        self.touch_state();
        feedback.push(Feedback::Haptic {
            main: hand == 0,
            strength: 0.2,
            seconds: 0.02,
        });
    }

    fn grip_released(&mut self, hand: usize) {
        let Some(grab) = &mut self.grab else {
            return;
        };
        let done = match &mut grab.how {
            How::Free(free) => {
                if !free.holding[hand] {
                    return;
                }
                free.rebase();
                free.holding[hand] = false;
                free.holding == [false, false]
            }
            How::Handle { hand: held, .. } => *held == hand,
        };
        if done {
            self.end_grab();
        }
    }

    /// Moves what is held to where the hands are now.
    fn carry(&mut self, input: &InputFrame) {
        let Some(mut grab) = self.grab.take() else {
            return;
        };
        let hands = [input.main, input.off];
        let motion = match &mut grab.how {
            How::Free(free) => {
                for (i, hand) in hands.iter().enumerate() {
                    if free.holding[i] && hand.tracked {
                        free.poses[i] =
                            free.poses[i].toward(Pose::of(hand), steadiness(hand.grip, input.dt));
                    }
                }
                // With one hand, its stick zooms about the hand.
                if free.holding[0] != free.holding[1] {
                    let push = hands[usize::from(!free.holding[0])].stick[1];
                    if push.abs() > REST {
                        free.zoom = (free.zoom * (push as f64 * input.dt / ZOOM_SECONDS).exp())
                            .clamp(1e-9, 1e9);
                    }
                }
                self.constrained(&grab.carried, free)
            }
            How::Handle { hand, drag } => drag.motion(&hands[*hand], None),
        };
        grab.carried.apply(self, &motion);
        grab.motion = motion;
        self.grab = Some(grab);
    }

    /// The motion of a free grab, limited by what the options allow.
    fn constrained(&self, carried: &Carried, free: &Free) -> Xform {
        let motion = free.total();
        let options = &self.options;
        let held = motion.transform_point(free.origin);
        match carried.subject {
            Subject::View => {
                if !options.level_horizon {
                    return motion;
                }
                // Only turning about the vertical; the point taken hold of stays in the hand.
                about_moving(
                    free.origin,
                    held,
                    twist(motion.rotation, DVec3::Y),
                    motion.scale,
                )
            }
            Subject::Layer(_) | Subject::Selection => {
                if options.allow_translate && options.allow_rotate && options.allow_scale {
                    return motion;
                }
                let rotation = if options.allow_rotate {
                    motion.rotation
                } else {
                    DQuat::IDENTITY
                };
                let scale = if options.allow_scale {
                    motion.scale
                } else {
                    1.0
                };
                // Without translation the thing turns and grows about its own pivot.
                let pivot = carried.pivot;
                let moved = if options.allow_translate {
                    pivot + (held - free.origin)
                } else {
                    pivot
                };
                about_moving(pivot, moved, rotation, scale)
            }
        }
    }

    /// Lets go of what the grips hold, making the move one undo step.
    pub(crate) fn end_grab(&mut self) {
        let Some(grab) = self.grab.take() else {
            return;
        };
        let repeat = grab.carried.repeat(self, &grab.motion);
        if grab.edits {
            self.commit_transaction();
        }
        self.remember_repeat(repeat);
        self.touch_state();
    }

    /// Lets go and puts back what was being moved.
    pub(crate) fn cancel_grab(&mut self) {
        if let Some(grab) = self.grab.take() {
            if grab.edits {
                self.cancel_transaction();
            }
            self.touch_state();
        }
    }

    // ---- doing a duplicate again ----

    pub(crate) fn remember_repeat(&mut self, repeat: Option<Repeat>) {
        if let Some(repeat) = repeat {
            self.repeat = Some((repeat, self.edits));
        }
    }

    /// True when the last thing done was to move a copy, so that it can be done again.
    pub fn can_transform_again(&self) -> bool {
        match self.repeat {
            Some((repeat, at)) if at == self.edits => match repeat {
                Repeat::Strokes { .. } => !self.selection.is_empty(),
                Repeat::Layer { layer, .. } => self.doc.layer(layer).is_some(),
            },
            _ => false,
        }
    }

    /// Makes another copy, moved from the last one as that was from the one before.
    pub(crate) fn transform_again(&mut self) -> Result<Value, CommandError> {
        let Some((repeat, _)) = self.repeat.filter(|_| self.can_transform_again()) else {
            return Err(CommandError::invalid(
                "There is no duplicate to repeat: hold the modifier while moving something first",
            ));
        };
        let (next, result) = match repeat {
            Repeat::Layer { layer, motion } => {
                self.require_unlocked(layer)?;
                let copy = layers::duplicate_layer(self.doc_mut(), layer)?;
                layers::rebase_transform(self.doc_mut(), copy, &motion)?;
                if self.transform_target == Some(layer) {
                    self.transform_target = Some(copy);
                }
                self.set_current_layer(copy);
                (
                    Repeat::Layer {
                        layer: copy,
                        motion,
                    },
                    json!({ "layer": copy }),
                )
            }
            Repeat::Strokes { motion } => {
                let frame = self.evaluate();
                let targets = self.edit_targets(&frame).map_err(CommandError::Invalid)?;
                let pivot = self.selection_pivot();
                let animate = self.options.animate_duplicates;
                let mut selected: Vec<(DrawingId, BTreeSet<u32>)> = Vec::new();
                let mut copied = 0;
                for target in &targets {
                    let PaintTarget { layer, drawing } = target.target;
                    let (Some(source), Some(ids)) = (
                        self.doc.drawing(drawing),
                        self.selection.in_drawing(drawing),
                    ) else {
                        continue;
                    };
                    let in_world = frame.world_xform(&self.doc, layer);
                    let in_layer = in_world.inverse().mul(&motion).mul(&in_world);
                    let copies: Vec<Stroke> = source
                        .strokes()
                        .filter(|s| ids.contains(&s.id))
                        .map(|s| transformed_stroke(s, &in_layer))
                        .collect();
                    // Each repeat can go on a frame of its own, to animate the move.
                    let showing = matches!(
                        frame.get(layer).map(|s| s.content),
                        Some(Content::Paint { drawing: shown, .. }) if shown == drawing
                    );
                    let destination = if animate && showing {
                        let index = match frame.get(layer).map(|s| s.content) {
                            Some(Content::Paint { frame, .. }) => frame as usize,
                            _ => 0,
                        };
                        let added =
                            frames::add_frame(self.doc_mut(), layer, index, NewFrame::Blank)?;
                        self.doc
                            .layer(layer)
                            .and_then(|l| l.as_paint())
                            .and_then(|p| p.drawing_at_frame(added))
                            .unwrap_or(drawing)
                    } else {
                        drawing
                    };
                    let mut ids = BTreeSet::new();
                    if let Some(drawing) = self.doc_mut().drawing_mut(destination) {
                        for mut copy in copies {
                            copy.id = drawing.next_stroke_id();
                            ids.insert(copy.id);
                            drawing.push_stroke(copy);
                        }
                    }
                    copied += ids.len();
                    selected.push((destination, ids));
                }
                self.selection.clear();
                for (drawing, ids) in selected {
                    self.selection.set_drawing(drawing, ids);
                }
                if let Some(pivot) = pivot {
                    self.set_selection_pivot(motion.transform_point(pivot));
                }
                if animate {
                    let step = self.doc.settings.rate.frame_duration();
                    self.playback.seek(self.playback.time + step);
                }
                (Repeat::Strokes { motion }, json!({ "copied": copied }))
            }
        };
        self.repeat = Some((next, self.edits));
        Ok(result)
    }

    /// What the gimbal would move: the selection while the Selection tool is in hand,
    /// or else the layer chosen as the transform object.
    pub(crate) fn gimbal_subject(&self) -> Option<Subject> {
        if !self.options.show_gimbal {
            return None;
        }
        if self.tools.tool == ToolKind::Select && !self.selection.is_empty() {
            return Some(Subject::Selection);
        }
        self.transform_target
            .filter(|id| *id != self.doc.root() && self.doc.layer(*id).is_some())
            .map(Subject::Layer)
    }

    /// Notes which handle of the gimbal each hand is at.
    fn refresh_hover(&mut self, input: &InputFrame) {
        self.hover = [None, None];
        if self.grab.is_none() && self.gesture.is_none() {
            if let Some(gimbal) = self.gimbal() {
                for (i, hand) in [input.main, input.off].iter().enumerate() {
                    if hand.tracked {
                        self.hover[i] = gimbal.handle_at(hand);
                    }
                }
            }
        }
    }

    /// The handle of the gimbal being dragged, by a grip or by the trigger.
    pub(crate) fn active_handle(&self) -> Option<GimbalHandle> {
        self.grab
            .as_ref()
            .and_then(Grab::handle)
            .or(match &self.gesture {
                Some(Gesture::Handle(gesture)) => Some(gesture.handle()),
                _ => None,
            })
    }
}
