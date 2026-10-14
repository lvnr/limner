//! The Paint and Line tools: the two that lay down new strokes.

use super::settings::BrushStyle;
use super::space::{LayerSpace, PaintTarget};
use super::stroke_builder::{build_points, Sample, StrokeStyle, MAX_STROKE_POINTS};
use super::Taper;
use crate::eval::{Content, RenderFrame};
use crate::math::Xform;
use crate::model::{Drawing, DrawingId, Point, Stroke, StrokeAddr};
use crate::ops::strokes::transform_point;
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

/// A piece of a stroke that has been finished: which drawing it is in and the frame it was drawn on.
struct Piece {
    drawing: DrawingId,
    stroke: u32,
    /// Its twin across the mirror, when mirrored working is on.
    twin: Option<u32>,
    frame: usize,
}

/// The twin of a stroke being drawn: the same stroke on the other side of the mirror.
#[derive(Clone, Copy)]
struct Twin {
    addr: StrokeAddr,
    stroke: u32,
    mirror: Xform,
}

impl Twin {
    /// Adds the twin of the stroke just started, in the same chunk as it.
    fn open(drawing: &mut Drawing, original: &Stroke, mirror: Xform) -> Self {
        let stroke = drawing.next_stroke_id();
        let addr = drawing.push_stroke(Stroke::new(
            stroke,
            original.brush,
            original.directional,
            Self::points(original.points(), &mirror),
        ));
        Self {
            addr,
            stroke,
            mirror,
        }
    }

    fn points(points: &[Point], mirror: &Xform) -> Vec<Point> {
        points.iter().map(|p| transform_point(p, mirror)).collect()
    }

    /// Brings the twin in line with the stroke it mirrors.
    fn follow(&self, drawing: &mut Drawing, points: &[Point]) {
        if let Some(stroke) = drawing.stroke_mut(self.addr) {
            *stroke.points_mut() = Self::points(points, &self.mirror);
        }
    }
}

/// A freehand stroke being drawn.
///
/// When the document is playing, the frame under the brush changes as the stroke is
/// drawn. The stroke is then laid down in pieces, each on the frame that was showing
/// when it was drawn: this is how motion is painted straight into an animation.
pub(crate) struct PaintGesture {
    target: PaintTarget,
    /// The frame of the layer the current piece is drawn on.
    frame: usize,
    addr: StrokeAddr,
    stroke: u32,
    twin: Option<Twin>,
    /// Points laid down so far in the current piece.
    samples: Vec<Sample>,
    /// Where the last of them was laid, in viewer space.
    last_laid: DVec3,
    /// Pieces finished on earlier frames.
    pieces: Vec<Piece>,
    /// Whether the document played at any time during the gesture.
    played: bool,
}

/// The frame of a paint layer that is showing in an evaluated frame.
fn frame_showing(frame: &RenderFrame, layer: crate::model::LayerId) -> usize {
    match frame.get(layer).map(|s| s.content) {
        Some(Content::Paint { frame, .. }) => frame as usize,
        _ => 0,
    }
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

    /// Starts a stroke in `target` from `samples`, in a chunk of its own so that only
    /// it is redrawn while it grows.
    fn open(
        session: &mut Session,
        target: PaintTarget,
        space: &LayerSpace,
        samples: &[Sample],
    ) -> Result<(StrokeAddr, u32, Option<Twin>), String> {
        let settings = session.tools.paint;
        let points = build_points(
            samples,
            &style(
                settings.brush,
                settings.width_taper,
                settings.opacity_taper,
                space,
            ),
        );
        let mirror = session.mirror_in(target.layer);
        let drawing = session
            .strokes_mut()
            .drawing_mut(target.drawing)
            .ok_or("That frame no longer exists")?;
        let id = drawing.next_stroke_id();
        let stroke = Stroke::new(id, settings.brush.brush(), settings.directional, points);
        let twin_of = mirror.map(|_| stroke.clone());
        let addr = drawing.push_stroke_in_new_chunk(stroke);
        let twin = mirror
            .zip(twin_of)
            .map(|(mirror, original)| Twin::open(drawing, &original, mirror));
        Ok((addr, id, twin))
    }

    pub(crate) fn begin(session: &mut Session, input: &InputFrame) -> Result<Self, String> {
        let target = session.paint_target()?;
        let frame = session.evaluate();
        let space = session
            .layer_space(&frame, target.layer)
            .ok_or("That layer cannot be drawn on here")?;
        let first = Self::sample(session, &space, input);
        let (addr, stroke, twin) = Self::open(session, target, &space, &[first])?;
        Ok(Self {
            target,
            frame: frame_showing(&frame, target.layer),
            addr,
            stroke,
            twin,
            samples: vec![first],
            last_laid: input.main.position,
            pieces: Vec::new(),
            played: session.playback.playing,
        })
    }

    /// Extends the stroke to where the hand is now. Returns false when the stroke
    /// cannot go on (its layer is gone, or it is full) and should be ended.
    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        let frame = session.evaluate();
        let Some(space) = session.layer_space(&frame, self.target.layer) else {
            return false;
        };
        self.played |= session.playback.playing;
        // Playback has moved on to another frame: finish this piece and carry on there.
        match session.editable_drawing(&frame, self.target.layer) {
            Ok(target) if target.drawing != self.target.drawing => {
                self.close_piece(session);
                let from = *self.samples.last().expect("never empty");
                let Ok((addr, stroke, twin)) = Self::open(session, target, &space, &[from]) else {
                    return false;
                };
                (self.target, self.addr, self.stroke, self.twin) = (target, addr, stroke, twin);
                self.frame = frame_showing(&frame, target.layer);
                self.samples = vec![from];
            }
            Ok(_) => {}
            // Off screen for the moment (between clips): wait for it to come back.
            Err(_) => return true,
        }

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
        let (addr, twin) = (self.addr, self.twin);
        let Some(drawing) = session.strokes_mut().drawing_mut(self.target.drawing) else {
            return false;
        };
        if let Some(twin) = twin {
            twin.follow(drawing, &points);
        }
        let Some(stroke) = drawing.stroke_mut(addr) else {
            return false;
        };
        *stroke.points_mut() = points;
        !full
    }

    fn close_piece(&mut self, session: &mut Session) {
        if let Some(drawing) = session.strokes_mut().drawing_mut(self.target.drawing) {
            drawing.merge_tail();
        }
        self.pieces.push(Piece {
            drawing: self.target.drawing,
            stroke: self.stroke,
            twin: self.twin.map(|t| t.stroke),
            frame: self.frame,
        });
    }

    pub(crate) fn end(mut self, session: &mut Session) {
        self.close_piece(session);
        if self.played {
            self.linger(session);
        }
    }

    /// Painted during playback, each piece of the stroke also shows on the frames
    /// that follow the one it was drawn on, for as long as the brush's "animated
    /// brush duration" says. On a looping layer that wraps around the end.
    fn linger(&self, session: &mut Session) {
        let Some(paint) = session
            .doc()
            .layer(self.target.layer)
            .and_then(|l| l.as_paint())
            .cloned()
        else {
            return;
        };
        let count = paint.frames.len();
        let extra =
            (session.tools.paint.anim_duration as f64 * paint.rate.as_f64()).round() as usize;
        if count < 2 || extra == 0 {
            return;
        }
        for piece in &self.pieces {
            let ids = [Some(piece.stroke), piece.twin];
            let strokes: Vec<_> = session
                .doc()
                .drawing(piece.drawing)
                .map(|d| {
                    d.strokes()
                        .filter(|s| ids.contains(&Some(s.id)))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default();
            if strokes.is_empty() {
                continue;
            }
            let mut reached = vec![piece.drawing];
            for step in 1..=extra.min(count - 1) {
                let at = piece.frame + step;
                let at = if paint.max_repeat == 0 {
                    at % count
                } else if at < count {
                    at
                } else {
                    break;
                };
                let Some(drawing_id) = paint.drawing_at_frame(at) else {
                    continue;
                };
                if reached.contains(&drawing_id) {
                    continue;
                }
                reached.push(drawing_id);
                if let Some(drawing) = session.strokes_mut().drawing_mut(drawing_id) {
                    for stroke in &strokes {
                        let mut copy = (**stroke).clone();
                        copy.id = drawing.next_stroke_id();
                        drawing.push_stroke(copy);
                    }
                }
            }
        }
    }
}

/// A straight line being pulled out from where the trigger was pressed.
pub(crate) struct LineGesture {
    target: PaintTarget,
    addr: StrokeAddr,
    twin: Option<Twin>,
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
        let mirror = session.mirror_in(target.layer);
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
        let twin_of = mirror.map(|_| stroke.clone());
        let addr = drawing.push_stroke_in_new_chunk(stroke);
        let twin = mirror
            .zip(twin_of)
            .map(|(mirror, original)| Twin::open(drawing, &original, mirror));
        let mut gesture = Self {
            target,
            addr,
            twin,
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
        let (addr, twin) = (self.addr, self.twin);
        let Some(drawing) = session.strokes_mut().drawing_mut(self.target.drawing) else {
            return false;
        };
        if let Some(twin) = twin {
            twin.follow(drawing, &points);
        }
        let Some(stroke) = drawing.stroke_mut(addr) else {
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
