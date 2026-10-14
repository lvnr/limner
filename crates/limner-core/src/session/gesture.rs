//! Turning controller input into tool use and button actions.
//!
//! One trigger press is one gesture and one undo step: it begins when the trigger is
//! pressed, is updated every frame while it is held, and ends when it is let go.
//! Buttons and sticks follow Quill's layout: the drawing hand's stick sizes the tool
//! and steps through undo history, its lower button deletes and its upper button
//! picks a colour; the other hand's stick steps through frames.

use super::command::Command;
use super::gimbal::HandleGesture;
use super::input::{Edge, Feedback, InputFrame, Latch, FLICK, PRESS, RELEASE, REST};
use super::tools::paint::{LineGesture, PaintGesture};
use super::tools::select::SelectGesture;
use super::tools::sphere::{GrabGesture, SphereGesture};
use super::tools::ToolKind;
use super::Session;
use glam::{DQuat, DVec3};

/// The tool at work.
pub(crate) enum Gesture {
    Paint(PaintGesture),
    Line(LineGesture),
    Sphere(SphereGesture),
    Grab(GrabGesture),
    Select(SelectGesture),
    /// The trigger on a handle of the gimbal, in place of the tool.
    Handle(Box<HandleGesture>),
}

/// A stick held past this long in one direction starts repeating its action.
const REPEAT_DELAY: f64 = 0.4;
const REPEAT_EVERY: f64 = 0.09;
/// How fast the stick resizes the tool: a factor of e per this many seconds at full deflection.
const RESIZE_SECONDS: f64 = 0.6;
/// The lower button deletes, and the upper button picks from, a stroke this close to
/// the hand at most, in viewer-space metres beyond the tool's own radius.
const REACH: f64 = 0.01;

/// A stick pushed to one side: reports the push once, then again and again while held.
#[derive(Clone, Copy, Debug, Default)]
struct Flick {
    direction: i8,
    held: f64,
    next_repeat: f64,
}

impl Flick {
    /// Returns -1 or 1 on the frames the action should happen, 0 otherwise.
    fn update(&mut self, value: f32, dt: f64) -> i8 {
        let direction = if value > FLICK {
            1
        } else if value < -FLICK {
            -1
        } else if value.abs() < REST {
            0
        } else {
            self.direction
        };
        if direction != self.direction {
            self.direction = direction;
            self.held = 0.0;
            self.next_repeat = REPEAT_DELAY;
            return direction;
        }
        if direction != 0 {
            self.held += dt;
            if self.held >= self.next_repeat {
                self.next_repeat += REPEAT_EVERY;
                return direction;
            }
        }
        0
    }

    /// As `update`, but a push counts once however long the stick is held.
    fn update_once(&mut self, value: f32) -> i8 {
        let direction = if value > FLICK {
            1
        } else if value < -FLICK {
            -1
        } else if value.abs() < REST {
            0
        } else {
            self.direction
        };
        if direction == self.direction {
            return 0;
        }
        self.direction = direction;
        direction
    }
}

/// The state of every button and stick between frames.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Controls {
    main_lower: Latch,
    main_upper: Latch,
    main_click: Latch,
    off_lower: Latch,
    off_upper: Latch,
    off_click: Latch,
    main_sideways: Flick,
    off_sideways: Flick,
    off_upward: Flick,
}

/// What to draw at the drawing hand: a preview of the tool.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cursor {
    /// False when the drawing hand is not tracked; nothing should be drawn.
    pub tracked: bool,
    /// Viewer space.
    pub position: DVec3,
    pub rotation: DQuat,
    pub tool: ToolKind,
    /// Half-width of the brush for Paint and Line; radius of the sphere for the others. Viewer-space metres.
    pub radius: f32,
    /// Radius of the inner sphere, for the tools that have one.
    pub inner: Option<f32>,
    /// The paint colour, linear RGB.
    pub color: [f32; 3],
    /// Opacity of the brush preview: the tool's opacity or strength.
    pub opacity: f32,
    /// True while the tool is being applied.
    pub active: bool,
    /// How far the modifier is pressed.
    pub alt: f32,
    /// The hand is at a handle of the gimbal: the trigger would move that, not apply the tool.
    pub at_handle: bool,
}

impl Session {
    /// The tool preview for the drawing hand, as of the last input.
    pub fn cursor(&self) -> Cursor {
        let hand = &self.last_input.main;
        let tools = &self.tools;
        let opacity = match tools.tool {
            ToolKind::Paint => tools.paint.opacity,
            ToolKind::Line => tools.line.opacity,
            ToolKind::Colorize => tools.colorize.strength,
            ToolKind::Eraser => tools.eraser.strength,
            ToolKind::Grab => tools.grab.strength,
            ToolKind::Thicken => tools.thicken.strength,
            ToolKind::Nudge => tools.nudge.strength,
            ToolKind::Select | ToolKind::Optimize => 1.0,
        };
        Cursor {
            tracked: hand.tracked,
            position: hand.position,
            rotation: hand.rotation,
            tool: tools.tool,
            radius: tools.size(),
            inner: tools.inner().map(|fraction| fraction * tools.size()),
            color: tools.color,
            opacity,
            active: self.gesture.is_some(),
            alt: self.last_input.alt(),
            at_handle: self.hover[0].is_some() || matches!(self.gesture, Some(Gesture::Handle(_))),
        }
    }

    /// Feeds one frame of controller input to the session. Returns what the device
    /// should do in response.
    pub fn update(&mut self, input: &InputFrame) -> Vec<Feedback> {
        let mut feedback = Vec::new();
        self.update_buttons(input, &mut feedback);
        self.update_grabs(input, &mut feedback);
        self.update_sticks(input, &mut feedback);
        self.update_tool(input, &mut feedback);
        self.last_input = *input;
        feedback
    }

    /// True while a tool is being applied.
    pub fn is_tool_active(&self) -> bool {
        self.gesture.is_some()
    }

    /// Runs a command on behalf of a button, turning a failure into a message.
    fn press(&mut self, command: Command, feedback: &mut Vec<Feedback>) -> bool {
        match self.exec(command) {
            Ok(_) => true,
            Err(error) => {
                feedback.push(Feedback::Message(error.to_string()));
                false
            }
        }
    }

    fn update_buttons(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        let (main, off) = (input.main, input.off);
        let mut controls = self.controls;
        if controls
            .main_lower
            .update_bool(main.tracked && main.lower_button)
            == Edge::Pressed
        {
            self.delete_at_hand(input, feedback);
        }
        if controls
            .main_upper
            .update_bool(main.tracked && main.upper_button)
            == Edge::Pressed
        {
            self.pick_color_at_hand(input, feedback);
        }
        if controls
            .main_click
            .update_bool(main.tracked && main.stick_click)
            == Edge::Pressed
        {
            self.options.show_gimbal = !self.options.show_gimbal;
            self.touch_state();
        }
        if controls
            .off_lower
            .update_bool(off.tracked && off.lower_button)
            == Edge::Pressed
        {
            self.tools.toggle_select_tool();
            self.touch_state();
            feedback.push(Feedback::Haptic {
                main: false,
                strength: 0.3,
                seconds: 0.03,
            });
        }
        if controls
            .off_upper
            .update_bool(off.tracked && off.upper_button)
            == Edge::Pressed
        {
            feedback.push(Feedback::Menu);
        }
        if controls
            .off_click
            .update_bool(off.tracked && off.stick_click)
            == Edge::Pressed
        {
            self.press(Command::TogglePlay, feedback);
        }
        self.controls = controls;
    }

    fn update_sticks(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        let mut controls = self.controls;
        let alt = input.alt() > 0.5;

        // Drawing hand: up and down size the tool, left and right undo and redo. While
        // the Grab tool holds strokes, or the hand grabs something, the stick belongs to that.
        let [x, y] = if input.main.tracked {
            input.main.stick
        } else {
            [0.0, 0.0]
        };
        let stick_is_taken = matches!(self.gesture, Some(Gesture::Grab(_))) || self.is_grabbing(0);
        let sideways = if x.abs() > y.abs() && !stick_is_taken {
            x
        } else {
            0.0
        };
        let stepped = match controls.main_sideways.update(sideways, input.dt) {
            -1 => self.undo().map(|_| "undo"),
            1 => self.redo().map(|_| "redo"),
            _ => None,
        };
        if let Some(sound) = stepped {
            feedback.push(Feedback::Sound(sound));
        }
        if y.abs() >= x.abs() && y.abs() > REST && !stick_is_taken {
            let amount = y as f64 * input.dt / RESIZE_SECONDS;
            match self.tools.inner() {
                // With the modifier, the stick moves the inner sphere alone.
                Some(inner) if alt => self.tools.set_inner(inner + amount as f32),
                _ => self.tools.set_size(self.tools.size() * amount.exp() as f32),
            }
            self.touch_state();
        }

        // Other hand: left and right step through frames; with the modifier they jump
        // between chapter marks. Up and down make and hold frames.
        let [x, y] = if input.off.tracked {
            input.off.stick
        } else {
            [0.0, 0.0]
        };
        let off_grabbing = self.is_grabbing(1);
        let sideways = if x.abs() > y.abs() && !off_grabbing {
            x
        } else {
            0.0
        };
        let upward = if y.abs() >= x.abs() && !off_grabbing {
            y
        } else {
            0.0
        };
        match (controls.off_sideways.update(sideways, input.dt), alt) {
            (0, _) => {}
            (direction, false) => {
                self.press(
                    Command::StepFrames {
                        frames: direction as i64,
                    },
                    feedback,
                );
            }
            (-1, true) => {
                self.press(Command::Rewind, feedback);
            }
            (_, true) => {
                self.press(Command::FastForward, feedback);
            }
        }
        // Up makes a new frame and down holds the one showing a frame longer; with the
        // modifier, up makes a copy of the frame and down deletes it. One push, one frame.
        let pushed = controls.off_upward.update_once(upward);
        if pushed != 0 {
            if let Some(layer) = self.paint_layer {
                let command = match (pushed > 0, alt) {
                    (true, false) => Command::AddFrame {
                        layer,
                        kind: crate::ops::frames::NewFrame::Blank,
                    },
                    (false, false) => Command::AddHoldFrame { layer, after: true },
                    (true, true) => Command::AddFrame {
                        layer,
                        kind: crate::ops::frames::NewFrame::Duplicate,
                    },
                    (false, true) => Command::DeleteFrame { layer },
                };
                if self.press(command, feedback) {
                    feedback.push(Feedback::Haptic {
                        main: false,
                        strength: 0.3,
                        seconds: 0.03,
                    });
                }
            }
        }
        self.controls = controls;
    }

    /// The lower button: deletes the selection, or else the stroke nearest the hand.
    fn delete_at_hand(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        if !self.selection.is_empty() {
            if self.press(Command::DeleteSelection, feedback) {
                feedback.push(Feedback::Sound("delete"));
            }
            return;
        }
        let frame = self.evaluate();
        let Some(layer) = self.paint_layer else {
            return;
        };
        if let Err(message) = self.editable_drawing(&frame, layer) {
            feedback.push(Feedback::Message(message));
            return;
        }
        let limit = self.tools.size() as f64 + REACH;
        let Some(nearest) = self.nearest_stroke(&frame, &[layer], input.main.position, limit)
        else {
            return;
        };
        let deleted = self.edit("Delete stroke", |session| {
            let drawing = session
                .doc_mut()
                .drawing_mut(nearest.target.drawing)
                .ok_or_else(|| super::CommandError::invalid("that frame no longer exists"))?;
            drawing.retain(|stroke| stroke.id != nearest.stroke);
            Ok(())
        });
        if deleted.is_ok() {
            feedback.push(Feedback::Sound("delete"));
            feedback.push(Feedback::Haptic {
                main: true,
                strength: 0.5,
                seconds: 0.04,
            });
        }
    }

    /// The upper button: takes the paint colour from the stroke nearest the hand.
    fn pick_color_at_hand(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        let frame = self.evaluate();
        let layers = self.visible_paint_layers(&frame);
        let limit = self.tools.size() as f64 + REACH;
        if let Some(nearest) = self.nearest_stroke(&frame, &layers, input.main.position, limit) {
            self.tools.color = nearest.point.color;
            self.touch_state();
            feedback.push(Feedback::Sound("pick"));
            feedback.push(Feedback::Haptic {
                main: true,
                strength: 0.3,
                seconds: 0.03,
            });
        }
    }

    fn update_tool(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        let trigger = if input.main.tracked {
            input.main.trigger
        } else {
            0.0
        };
        let mut latch = self.trigger;
        let edge = latch.update(trigger, PRESS, RELEASE);
        self.trigger = latch;
        match edge {
            Edge::Pressed => self.begin_gesture(input, feedback),
            Edge::Held => self.continue_gesture(input),
            Edge::Released => self.end_gesture(),
            Edge::None => {}
        }
    }

    fn begin_gesture(&mut self, input: &InputFrame, feedback: &mut Vec<Feedback>) {
        // An unfinished gesture means input stopped without a release; close it first.
        self.end_gesture();
        // At a handle of the gimbal the trigger works the handle, not the tool.
        if let Some((handle, gimbal)) = self.hover[0].zip(self.gimbal()) {
            self.begin_transaction(HandleGesture::label(
                gimbal.subject,
                handle,
                input.alt() > 0.5,
            ));
            match HandleGesture::begin(self, input, &gimbal, handle) {
                Ok(gesture) => self.gesture = Some(Gesture::Handle(Box::new(gesture))),
                Err(message) => {
                    self.cancel_transaction();
                    feedback.push(Feedback::Message(message));
                }
            }
            return;
        }
        let tool = if self.tools.tool == ToolKind::Paint && input.alt() > 0.5 {
            ToolKind::Line
        } else {
            self.tools.tool
        };
        let label = match tool {
            ToolKind::Paint => "Paint stroke",
            ToolKind::Line => "Straight line",
            ToolKind::Colorize => "Colorize",
            ToolKind::Eraser => "Erase",
            ToolKind::Select => "Select strokes",
            ToolKind::Grab => "Reshape strokes",
            ToolKind::Thicken if input.alt() > 0.5 => "Thin strokes",
            ToolKind::Thicken => "Thicken strokes",
            ToolKind::Nudge => "Nudge strokes",
            ToolKind::Optimize => "Optimize strokes",
        };
        self.begin_transaction(label);
        let gesture = match tool {
            ToolKind::Paint => PaintGesture::begin(self, input).map(Gesture::Paint),
            ToolKind::Line => LineGesture::begin(self, input).map(Gesture::Line),
            ToolKind::Select => Ok(Gesture::Select(SelectGesture::begin(self, input))),
            ToolKind::Grab => GrabGesture::begin(self, input).map(Gesture::Grab),
            ToolKind::Colorize
            | ToolKind::Eraser
            | ToolKind::Thicken
            | ToolKind::Nudge
            | ToolKind::Optimize => SphereGesture::begin(self, input, tool).map(Gesture::Sphere),
        };
        match gesture {
            Ok(gesture) => self.gesture = Some(gesture),
            Err(message) => {
                self.cancel_transaction();
                feedback.push(Feedback::Message(message));
            }
        }
    }

    fn continue_gesture(&mut self, input: &InputFrame) {
        let Some(mut gesture) = self.gesture.take() else {
            return;
        };
        let keep_going = match &mut gesture {
            Gesture::Paint(paint) => paint.update(self, input),
            Gesture::Line(line) => line.update(self, input),
            Gesture::Sphere(sphere) => sphere.update(self, input),
            Gesture::Grab(grab) => grab.update(self, input),
            Gesture::Select(select) => select.update(self, input),
            Gesture::Handle(handle) => handle.update(self, input),
        };
        self.gesture = Some(gesture);
        if !keep_going {
            // The stroke is full: close it and carry on in a new one from the same spot.
            self.end_gesture();
            let mut ignored = Vec::new();
            self.begin_gesture(input, &mut ignored);
        }
    }

    /// Finishes the tool at work, making what it did one undo step.
    pub fn end_gesture(&mut self) {
        let Some(gesture) = self.gesture.take() else {
            return;
        };
        match gesture {
            Gesture::Paint(paint) => paint.end(self),
            Gesture::Line(line) => line.end(self),
            Gesture::Sphere(sphere) => sphere.end(self),
            Gesture::Grab(grab) => grab.end(self),
            Gesture::Select(_) => {}
            Gesture::Handle(handle) => handle.end(self),
        }
        self.commit_transaction();
        let repeat = self.pending_repeat.take();
        self.remember_repeat(repeat);
    }

    /// Abandons the tool at work, and anything held by the grips, and undoes what
    /// they did so far.
    pub fn cancel_gesture(&mut self) {
        self.cancel_grab();
        if self.gesture.take().is_some() {
            self.cancel_transaction();
        }
    }
}
