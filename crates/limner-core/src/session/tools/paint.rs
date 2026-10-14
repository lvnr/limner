//! The Paint and Line tools: the two that lay down new strokes.

use super::settings::BrushStyle;
use super::space::{LayerSpace, PaintTarget};
use super::stroke_builder::{build_points, Sample, StrokeStyle, MAX_STROKE_POINTS};
use super::Taper;
use crate::model::{Stroke, StrokeAddr};
use crate::session::input::InputFrame;
use crate::session::Session;
use glam::DVec3;

/// The hand has to move this far, in viewer-space metres, before a new point is laid down.
const SAMPLE_SPACING: f64 = 0.001;
/// How far beyond a stroke's end the closing point of a flat cap sits, in viewer-space metres.
const CAP_GAP: f64 = 0.0001;
/// The Line tool lays one point per this much length, in viewer-space metres. Drawing
/// "big" (zoomed in) therefore gives a line more points to bend later.
const LINE_SEGMENT: f64 = 0.005;
const MAX_LINE_SEGMENTS: usize = 2048;

fn style(
    brush: BrushStyle,
    width_taper: Taper,
    opacity_taper: Taper,
    space: &LayerSpace,
) -> StrokeStyle {
    StrokeStyle {
        brush: brush.brush(),
        cap: brush.cap(),
        width_taper,
        opacity_taper,
        cap_gap: space.length(CAP_GAP) as f32,
    }
}

/// A freehand stroke being drawn.
pub(crate) struct PaintGesture {
    target: PaintTarget,
    addr: StrokeAddr,
    /// Points laid down so far.
    samples: Vec<Sample>,
    /// Where the last of them was laid, in viewer space.
    last_laid: DVec3,
}

impl PaintGesture {
    fn sample(session: &Session, space: &LayerSpace, input: &InputFrame) -> Sample {
        let hand = &input.main;
        let pos = space.point(hand.position);
        let settings = &session.tools.paint;
        Sample {
            pos,
            normal: space.direction(hand.backward()),
            up: space.direction(hand.up()),
            view_dir: (pos - space.point(input.head)).normalize_or_zero(),
            pressure: hand.trigger.clamp(0.0, 1.0),
            size: space.length(settings.size as f64) as f32,
            color: session.tools.color,
            opacity: settings.opacity,
        }
    }

    pub(crate) fn begin(session: &mut Session, input: &InputFrame) -> Result<Self, String> {
        let target = session.paint_target()?;
        let frame = session.evaluate();
        let space = session
            .layer_space(&frame, target.layer)
            .ok_or("That layer cannot be drawn on here")?;
        let first = Self::sample(session, &space, input);
        let settings = session.tools.paint;
        let points = build_points(
            &[first],
            &style(
                settings.brush,
                settings.width_taper,
                settings.opacity_taper,
                &space,
            ),
        );
        let drawing = session
            .strokes_mut()
            .drawing_mut(target.drawing)
            .ok_or("That frame no longer exists")?;
        let stroke = Stroke::new(
            drawing.next_stroke_id(),
            settings.brush.brush(),
            settings.directional,
            points,
        );
        let addr = drawing.push_stroke_in_new_chunk(stroke);
        Ok(Self {
            target,
            addr,
            samples: vec![first],
            last_laid: input.main.position,
        })
    }

    /// Extends the stroke to where the hand is now. Returns false when the stroke
    /// cannot go on (its layer is gone, or it is full) and should be ended.
    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        let frame = session.evaluate();
        let Some(space) = session.layer_space(&frame, self.target.layer) else {
            return false;
        };
        let now = Self::sample(session, &space, input);
        let moved = input.main.position.distance(self.last_laid);
        if moved >= SAMPLE_SPACING {
            self.samples.push(now);
            self.last_laid = input.main.position;
        }
        // Between laid points the stroke still reaches the hand, so it never lags behind it.
        let reaching = moved < SAMPLE_SPACING && moved > SAMPLE_SPACING * 0.05;
        if reaching {
            self.samples.push(now);
        }
        let settings = session.tools.paint;
        let points = build_points(
            &self.samples,
            &style(
                settings.brush,
                settings.width_taper,
                settings.opacity_taper,
                &space,
            ),
        );
        if reaching {
            self.samples.pop();
        }
        let full = points.len() >= MAX_STROKE_POINTS;
        let addr = self.addr;
        let Some(stroke) = session
            .strokes_mut()
            .drawing_mut(self.target.drawing)
            .and_then(|d| d.stroke_mut(addr))
        else {
            return false;
        };
        *stroke.points_mut() = points;
        !full
    }

    pub(crate) fn end(self, session: &mut Session) {
        if let Some(drawing) = session.strokes_mut().drawing_mut(self.target.drawing) {
            drawing.merge_tail();
        }
    }
}

/// A straight line being pulled out from where the trigger was pressed.
pub(crate) struct LineGesture {
    target: PaintTarget,
    addr: StrokeAddr,
    /// Where the line starts, in world space, so it stays put if the view is moved meanwhile.
    origin: DVec3,
    /// The brush size when the line was started, in viewer-space metres. Changing the
    /// size before letting go tapers the line.
    origin_size: f32,
    /// Where the hand was last frame, in world space, for moving the origin with a grab.
    last_hand: DVec3,
}

impl LineGesture {
    pub(crate) fn begin(session: &mut Session, input: &InputFrame) -> Result<Self, String> {
        let target = session.paint_target()?;
        let hand_world = session
            .view
            .world_from_viewer
            .transform_point(input.main.position);
        let settings = session.tools.line;
        let drawing = session
            .strokes_mut()
            .drawing_mut(target.drawing)
            .ok_or("That frame no longer exists")?;
        let stroke = Stroke::new(
            drawing.next_stroke_id(),
            settings.brush.brush(),
            settings.directional,
            Vec::new(),
        );
        let addr = drawing.push_stroke_in_new_chunk(stroke);
        let mut gesture = Self {
            target,
            addr,
            origin: hand_world,
            origin_size: settings.size,
            last_hand: hand_world,
        };
        gesture.update(session, input);
        Ok(gesture)
    }

    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        let frame = session.evaluate();
        let Some(space) = session.layer_space(&frame, self.target.layer) else {
            return false;
        };
        let world_from_viewer = session.view.world_from_viewer;
        let hand_world = world_from_viewer.transform_point(input.main.position);
        // Grabbing with the drawing hand moves the start of the line along with the hand.
        if input.main.grip > 0.5 {
            self.origin += hand_world - self.last_hand;
        }
        self.last_hand = hand_world;

        let mut end_world = hand_world;
        if input.alt() > 0.5 {
            // Snap to the world axis the line runs along most.
            let delta = hand_world - self.origin;
            let abs = delta.abs();
            let along = if abs.x >= abs.y && abs.x >= abs.z {
                DVec3::X
            } else if abs.y >= abs.z {
                DVec3::Y
            } else {
                DVec3::Z
            };
            end_world = self.origin + along * delta.dot(along);
        }

        let viewer_from_world = world_from_viewer.inverse();
        let (start, end) = (
            viewer_from_world.transform_point(self.origin),
            viewer_from_world.transform_point(end_world),
        );
        let length = start.distance(end);
        let segments = ((length / LINE_SEGMENT).ceil() as usize).clamp(1, MAX_LINE_SEGMENTS);
        let settings = session.tools.line;
        let hand = &input.main;
        let samples: Vec<Sample> = (0..=segments)
            .map(|i| {
                let t = i as f64 / segments as f64;
                let pos = space.point(start.lerp(end, t));
                let size =
                    self.origin_size as f64 + (settings.size as f64 - self.origin_size as f64) * t;
                Sample {
                    pos,
                    normal: space.direction(hand.backward()),
                    up: space.direction(hand.up()),
                    view_dir: (pos - space.point(input.head)).normalize_or_zero(),
                    pressure: 1.0,
                    size: space.length(size) as f32,
                    color: session.tools.color,
                    opacity: settings.opacity,
                }
            })
            .collect();
        let points = build_points(
            &samples,
            &style(settings.brush, Taper::None, Taper::None, &space),
        );
        let addr = self.addr;
        let Some(stroke) = session
            .strokes_mut()
            .drawing_mut(self.target.drawing)
            .and_then(|d| d.stroke_mut(addr))
        else {
            return false;
        };
        *stroke.points_mut() = points;
        true
    }

    pub(crate) fn end(self, session: &mut Session) {
        if let Some(drawing) = session.strokes_mut().drawing_mut(self.target.drawing) {
            drawing.merge_tail();
        }
    }
}
