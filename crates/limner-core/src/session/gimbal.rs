//! The gimbal: handles around the pivot of whatever is being transformed, for moving
//! it along one axis or in one plane, and for turning it about one axis.
//!
//! A grip on a handle moves the thing, and so does the trigger, except that on a ring
//! the trigger turns it in fixed steps. With the modifier held, the trigger on an
//! arrow, a square or the middle moves the pivot instead, and on a ring it turns a
//! copy.

use super::grab::{Carried, Subject};
use super::input::{HandInput, InputFrame};
use super::view::about;
use super::Session;
use crate::math::Xform;
use crate::model::LayerId;
use glam::{DAffine3, DQuat, DVec3};
use serde::{Deserialize, Serialize};
use std::f64::consts::TAU;

/// Radius of the rings as a fraction of the distance from the eyes, so that the
/// gimbal looks the same size wherever it is.
pub const GIMBAL_SIZE: f64 = 0.2;
/// Where the parts sit, as fractions of the rings' radius. The arrows end short of
/// the rings so that a hand is never at both.
pub const ARROW_START: f64 = 0.28;
pub const ARROW_END: f64 = 0.75;
pub const PLANE_NEAR: f64 = 0.3;
pub const PLANE_FAR: f64 = 0.5;
pub const CENTER_RADIUS: f64 = 0.12;
/// How close to a handle a hand has to be, and a pointer from a screen.
const HAND_REACH: f64 = 0.1;
const POINTER_REACH: f64 = 0.06;
/// Straight pieces a ring is cut into for finding where a pointer passes it.
const RING_SEGMENTS: usize = 48;

/// One part of the gimbal. The axes are X, Y and Z of the gimbal's own frame; a plane
/// or a ring is named after the axis it is square to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GimbalHandle {
    Center,
    AxisX,
    AxisY,
    AxisZ,
    PlaneX,
    PlaneY,
    PlaneZ,
    RingX,
    RingY,
    RingZ,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Center,
    Axis(usize),
    Plane(usize),
    Ring(usize),
}

impl GimbalHandle {
    pub const ALL: [GimbalHandle; 10] = [
        GimbalHandle::Center,
        GimbalHandle::AxisX,
        GimbalHandle::AxisY,
        GimbalHandle::AxisZ,
        GimbalHandle::PlaneX,
        GimbalHandle::PlaneY,
        GimbalHandle::PlaneZ,
        GimbalHandle::RingX,
        GimbalHandle::RingY,
        GimbalHandle::RingZ,
    ];

    fn part(self) -> Part {
        match self {
            GimbalHandle::Center => Part::Center,
            GimbalHandle::AxisX => Part::Axis(0),
            GimbalHandle::AxisY => Part::Axis(1),
            GimbalHandle::AxisZ => Part::Axis(2),
            GimbalHandle::PlaneX => Part::Plane(0),
            GimbalHandle::PlaneY => Part::Plane(1),
            GimbalHandle::PlaneZ => Part::Plane(2),
            GimbalHandle::RingX => Part::Ring(0),
            GimbalHandle::RingY => Part::Ring(1),
            GimbalHandle::RingZ => Part::Ring(2),
        }
    }
}

/// The gimbal as it should be drawn, in viewer space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gimbal {
    pub subject: Subject,
    pub position: DVec3,
    pub rotation: DQuat,
    /// Radius of the rings, in viewer-space metres.
    pub size: f64,
    /// The handle at each hand: the drawing hand, then the other.
    pub hovered: [Option<GimbalHandle>; 2],
    /// The handle being dragged.
    pub active: Option<GimbalHandle>,
}

/// The nearest points of a ray and a straight piece from `a` to `b`: how far apart
/// they are, and the point on the piece.
fn ray_to_segment(origin: DVec3, direction: DVec3, a: DVec3, b: DVec3) -> (f64, DVec3) {
    let along = b - a;
    let from = origin - a;
    let (across, length) = (direction.dot(along), along.dot(along));
    let (d_from, a_from) = (direction.dot(from), along.dot(from));
    let spread = length - across * across;
    let mut on_piece = if spread > 1e-12 {
        ((a_from - d_from * across) / spread).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mut on_ray = on_piece * across - d_from;
    if on_ray < 0.0 {
        on_ray = 0.0;
        on_piece = if length > 0.0 {
            (a_from / length).clamp(0.0, 1.0)
        } else {
            0.0
        };
    }
    let point = a + along * on_piece;
    (point.distance(origin + direction * on_ray), point)
}

/// Where a ray crosses the plane through `point` square to `normal`.
fn ray_on_plane(origin: DVec3, direction: DVec3, point: DVec3, normal: DVec3) -> Option<DVec3> {
    let facing = direction.dot(normal);
    if facing.abs() < 1e-4 {
        return None;
    }
    let distance = (point - origin).dot(normal) / facing;
    (distance > 0.0).then(|| origin + direction * distance)
}

fn outside(value: f64, low: f64, high: f64) -> f64 {
    (low - value).max(value - high).max(0.0)
}

impl Gimbal {
    fn axes(&self) -> [DVec3; 3] {
        [
            self.rotation * DVec3::X,
            self.rotation * DVec3::Y,
            self.rotation * DVec3::Z,
        ]
    }

    /// How far a point is from a part.
    fn point_distance(&self, part: Part, point: DVec3) -> f64 {
        let (axes, size) = (self.axes(), self.size);
        let from = point - self.position;
        match part {
            Part::Center => (from.length() - CENTER_RADIUS * size).max(0.0),
            Part::Axis(i) => {
                let along = from
                    .dot(axes[i])
                    .clamp(ARROW_START * size, ARROW_END * size);
                (from - axes[i] * along).length()
            }
            Part::Plane(i) => {
                let (j, k) = ((i + 1) % 3, (i + 2) % 3);
                let (low, high) = (PLANE_NEAR * size, PLANE_FAR * size);
                DVec3::new(
                    outside(from.dot(axes[j]), low, high),
                    outside(from.dot(axes[k]), low, high),
                    from.dot(axes[i]),
                )
                .length()
            }
            Part::Ring(i) => {
                let height = from.dot(axes[i]);
                let radial = (from - axes[i] * height).length();
                (radial - size).hypot(height)
            }
        }
    }

    /// How far a ray passes from a part.
    fn ray_distance(&self, part: Part, origin: DVec3, direction: DVec3) -> f64 {
        let (axes, size) = (self.axes(), self.size);
        match part {
            Part::Center => {
                let (distance, _) = ray_to_segment(origin, direction, self.position, self.position);
                (distance - CENTER_RADIUS * size).max(0.0)
            }
            Part::Axis(i) => {
                ray_to_segment(
                    origin,
                    direction,
                    self.position + axes[i] * (ARROW_START * size),
                    self.position + axes[i] * (ARROW_END * size),
                )
                .0
            }
            Part::Plane(i) => match ray_on_plane(origin, direction, self.position, axes[i]) {
                Some(hit) => self.point_distance(part, hit),
                None => f64::INFINITY,
            },
            Part::Ring(i) => {
                let (u, v) = (axes[(i + 1) % 3], axes[(i + 2) % 3]);
                let at = |n: usize| {
                    let angle = TAU * n as f64 / RING_SEGMENTS as f64;
                    self.position + (u * angle.cos() + v * angle.sin()) * size
                };
                (0..RING_SEGMENTS)
                    .map(|n| ray_to_segment(origin, direction, at(n), at(n + 1)).0)
                    .fold(f64::INFINITY, f64::min)
            }
        }
    }

    /// The handle a hand is at: the nearest one within reach.
    pub fn handle_at(&self, hand: &HandInput) -> Option<GimbalHandle> {
        let mut best: Option<(f64, GimbalHandle)> = None;
        for handle in GimbalHandle::ALL {
            let (distance, reach) = match hand.ray {
                Some((origin, direction)) => (
                    self.ray_distance(handle.part(), origin, direction),
                    POINTER_REACH * self.size,
                ),
                None => (
                    self.point_distance(handle.part(), hand.position),
                    HAND_REACH * self.size,
                ),
            };
            if distance < reach && best.is_none_or(|(nearest, _)| distance < nearest) {
                best = Some((distance, handle));
            }
        }
        best.map(|(_, handle)| handle)
    }
}

/// A handle being dragged: the gimbal as it was when the drag began, and how far the
/// drag has come.
pub(crate) struct HandleDrag {
    pub(crate) handle: GimbalHandle,
    origin: DVec3,
    axes: [DVec3; 3],
    /// Where the hand took hold.
    start: DVec3,
    /// Where it was last.
    last: DVec3,
    /// For a ring: how far it has been turned, which may be more than once round.
    angle: f64,
    /// The plane a free drag moves in when it is steered from a screen.
    facing: DVec3,
}

impl HandleDrag {
    pub(crate) fn begin(
        gimbal: &Gimbal,
        handle: GimbalHandle,
        hand: &HandInput,
        head: DVec3,
    ) -> Self {
        let mut drag = Self {
            handle,
            origin: gimbal.position,
            axes: gimbal.axes(),
            start: gimbal.position,
            last: gimbal.position,
            angle: 0.0,
            facing: (head - gimbal.position).try_normalize().unwrap_or(DVec3::Z),
        };
        if let Some(point) = drag.point(hand) {
            drag.start = point;
            drag.last = point;
        }
        drag
    }

    /// Where the hand is for the purposes of this handle. A real hand is where it is;
    /// a pointer from a screen is where its ray meets the handle's own line or plane.
    fn point(&self, hand: &HandInput) -> Option<DVec3> {
        let Some((origin, direction)) = hand.ray else {
            return Some(hand.position);
        };
        match self.handle.part() {
            Part::Center => ray_on_plane(origin, direction, self.origin, self.facing),
            Part::Plane(i) | Part::Ring(i) => {
                ray_on_plane(origin, direction, self.origin, self.axes[i])
            }
            Part::Axis(i) => {
                let axis = self.axes[i];
                let from = origin - self.origin;
                let across = direction.dot(axis);
                let spread = 1.0 - across * across;
                (spread > 1e-6).then(|| {
                    self.origin + axis * ((axis.dot(from) - direction.dot(from) * across) / spread)
                })
            }
        }
    }

    /// How far the drag has slid the handle, along its axis or in its plane.
    pub(crate) fn slide(&mut self, hand: &HandInput) -> DVec3 {
        if let Some(point) = self.point(hand) {
            self.last = point;
        }
        let moved = self.last - self.start;
        match self.handle.part() {
            Part::Center => moved,
            Part::Axis(i) => self.axes[i] * moved.dot(self.axes[i]),
            Part::Plane(i) | Part::Ring(i) => moved - self.axes[i] * moved.dot(self.axes[i]),
        }
    }

    /// The motion of viewer space the drag asks for so far. With `steps`, a ring
    /// turns in that many equal steps to the full turn.
    pub(crate) fn motion(&mut self, hand: &HandInput, steps: Option<u32>) -> Xform {
        let Part::Ring(i) = self.handle.part() else {
            return Xform::from_translation(self.slide(hand));
        };
        let axis = self.axes[i];
        if let Some(point) = self.point(hand) {
            // The turn since last time is added up, so the ring can go round and round.
            let flat = |p: DVec3| {
                let from = p - self.origin;
                from - axis * from.dot(axis)
            };
            let (before, now) = (flat(self.last), flat(point));
            if before.length() > 1e-9 && now.length() > 1e-9 {
                self.angle += axis.dot(before.cross(now)).atan2(before.dot(now));
            }
            self.last = point;
        }
        let angle = match steps {
            Some(steps) => {
                let step = TAU / steps.max(1) as f64;
                (self.angle / step).round() * step
            }
            None => self.angle,
        };
        about(
            self.origin,
            &Xform::from_rotation(DQuat::from_axis_angle(axis, angle)),
        )
    }
}

/// Where a pivot was when the trigger, with the modifier, began to move it.
pub(crate) enum PivotStart {
    Layer {
        id: LayerId,
        pivot: Xform,
        /// Maps viewer space into the layer's own.
        layer_from_viewer: DAffine3,
    },
    Selection {
        world: DVec3,
    },
}

/// The trigger at work on a handle.
pub(crate) enum HandleGesture {
    /// The thing moved along an arrow, in a plane or freely by the middle, as the
    /// grip moves it, or turned by a ring in `steps` to the full turn.
    Carry {
        carried: Carried,
        drag: HandleDrag,
        steps: Option<u32>,
        motion: Xform,
    },
    /// With the modifier held: the pivot moved along an arrow, in a plane, or freely
    /// by the middle.
    Pivot { drag: HandleDrag, start: PivotStart },
}

impl HandleGesture {
    /// The name of the undo step the trigger on `handle` makes, with the modifier
    /// held or not.
    pub(crate) fn label(subject: Subject, handle: GimbalHandle, modifier: bool) -> &'static str {
        let selection = subject == Subject::Selection;
        match (handle.part(), modifier, selection) {
            (Part::Ring(_), false, true) => "Turn strokes",
            (Part::Ring(_), true, true) => "Duplicate strokes",
            (Part::Ring(_), false, false) => "Turn layer",
            (Part::Ring(_), true, false) => "Duplicate layer",
            (_, false, true) => "Move strokes",
            (_, false, false) => "Move layer",
            (_, true, _) => "Move pivot",
        }
    }

    pub(crate) fn begin(
        session: &mut Session,
        input: &InputFrame,
        gimbal: &Gimbal,
        handle: GimbalHandle,
    ) -> Result<Self, String> {
        let drag = HandleDrag::begin(gimbal, handle, &input.main, input.head);
        session.pin_gimbal_pivot(gimbal.subject);
        let modifier = input.alt() > 0.5;
        let ring = matches!(handle.part(), Part::Ring(_));
        if ring || !modifier {
            // On a ring the modifier turns a copy; elsewhere it is for the pivot.
            let carried = Carried::begin(session, gimbal.subject, ring && modifier)?;
            return Ok(HandleGesture::Carry {
                carried,
                drag,
                steps: ring.then_some(session.options.rotation_snap_steps),
                motion: Xform::IDENTITY,
            });
        }
        let start = match gimbal.subject {
            Subject::Layer(id) => {
                let layer = session.doc.layer(id).ok_or("That layer no longer exists")?;
                if layer.locked {
                    return Err(format!("Layer \"{}\" is locked", layer.name));
                }
                let frame = session.evaluate();
                let space = session
                    .layer_space(&frame, id)
                    .ok_or("That layer cannot be reached")?;
                PivotStart::Layer {
                    id,
                    pivot: layer.pivot,
                    layer_from_viewer: space.layer_from_viewer,
                }
            }
            Subject::Selection => PivotStart::Selection {
                world: session.selection_pivot().ok_or("Nothing is selected")?,
            },
            Subject::View => return Err("The view has no pivot".into()),
        };
        Ok(HandleGesture::Pivot { drag, start })
    }

    /// What the trigger moves, unless it is moving the pivot.
    pub(crate) fn carried(&self) -> Option<&Carried> {
        match self {
            HandleGesture::Carry { carried, .. } => Some(carried),
            HandleGesture::Pivot { .. } => None,
        }
    }

    pub(crate) fn handle(&self) -> GimbalHandle {
        match self {
            HandleGesture::Carry { drag, .. } | HandleGesture::Pivot { drag, .. } => drag.handle,
        }
    }

    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        match self {
            HandleGesture::Carry {
                carried,
                drag,
                steps,
                motion,
            } => {
                *motion = drag.motion(&input.main, *steps);
                carried.apply(session, motion);
            }
            HandleGesture::Pivot { drag, start } => {
                let slid = drag.slide(&input.main);
                match start {
                    PivotStart::Layer {
                        id,
                        pivot,
                        layer_from_viewer,
                    } => {
                        let mut moved = *pivot;
                        moved.translation += layer_from_viewer.transform_vector3(slid);
                        if let Some(layer) = session.strokes_mut().layer_mut(*id) {
                            layer.pivot = moved;
                        }
                    }
                    PivotStart::Selection { world } => {
                        let slid = session.view.world_from_viewer.transform_vector(slid);
                        session.set_selection_pivot(*world + slid);
                    }
                }
            }
        }
        true
    }

    pub(crate) fn end(self, session: &mut Session) {
        if let HandleGesture::Carry { carried, .. } = self {
            carried.finish(session);
            session.pending_repeat = carried.repeat(session);
        }
    }
}

impl Session {
    /// Where the gimbal of a layer sits, in the layer's own space: the layer's pivot,
    /// or, while that has not been set (it is the layer's origin, unturned), the
    /// middle of the layer's content, as "Reset pivot" would put it. A layer with no
    /// content of its own keeps its origin.
    pub(crate) fn gimbal_pivot(&self, id: LayerId) -> Option<Xform> {
        let layer = self.doc.layer(id)?;
        if !layer.pivot.is_identity() {
            return Some(layer.pivot);
        }
        Some(
            crate::ops::layers::layer_bounds(&self.doc, id).map_or(Xform::IDENTITY, |(lo, hi)| {
                Xform::from_translation((lo + hi) * 0.5)
            }),
        )
    }

    /// As the gimbal starts to move, turn or re-pivot a layer whose pivot has not been
    /// set: sets the pivot where the gimbal shows it, as part of the same edit. From
    /// then on the pivot stays where it is put, and the layer's animation turns about
    /// the point the gimbal turned it about.
    pub(crate) fn pin_gimbal_pivot(&mut self, subject: Subject) {
        let Subject::Layer(id) = subject else {
            return;
        };
        if self
            .doc
            .layer(id)
            .is_none_or(|l| l.locked || !l.pivot.is_identity())
        {
            return;
        }
        let Some(pivot) = self.gimbal_pivot(id).filter(|p| !p.is_identity()) else {
            return;
        };
        if let Some(layer) = self.doc_mut().layer_mut(id) {
            layer.pivot = pivot;
        }
    }

    /// The gimbal to draw and to take hold of, when there is one.
    pub fn gimbal(&self) -> Option<Gimbal> {
        let subject = self.gimbal_subject()?;
        let viewer_from_world = self.view.viewer_from_world();
        let (position, rotation) = match subject {
            Subject::Layer(id) => {
                let frame = self.evaluate();
                let pivot = viewer_from_world
                    .mul(&frame.world_xform(&self.doc, id))
                    .mul(&self.gimbal_pivot(id)?);
                (pivot.translation, pivot.rotation)
            }
            // A selection's gimbal is always square to the world.
            Subject::Selection => (
                viewer_from_world.transform_point(self.selection_pivot()?),
                viewer_from_world.rotation,
            ),
            Subject::View => return None,
        };
        if !position.is_finite() {
            return None;
        }
        Some(Gimbal {
            subject,
            position,
            rotation: rotation.normalize(),
            size: (GIMBAL_SIZE * position.distance(self.last_input.head)).max(0.02),
            hovered: self.hover,
            active: self.active_handle(),
        })
    }
}
