//! Controller input: what the hands are doing, independent of the device.
//!
//! A headset supplies two tracked controllers. The desktop supplies a stand-in for the
//! drawing hand, steered by the mouse or a pen. Everything the hands can do is decided
//! from this one description, so tools behave the same in both.

use glam::{DQuat, DVec3};

/// One hand at one moment, in viewer space (metres, Y up).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandInput {
    /// False when the controller is not tracked or not present.
    pub tracked: bool,
    /// The tip: where a stroke is laid down, the centre of a tool's sphere.
    pub position: DVec3,
    /// Orientation of the hand. Its -Z axis points forward, away from the user, and +Y is up.
    pub rotation: DQuat,
    /// Index trigger, 0 to 1. On the drawing hand it applies the tool, with pressure.
    /// On the other hand it is the modifier ("alt").
    pub trigger: f32,
    /// Grip, 0 to 1. Grabs the view, a layer or the selection.
    pub grip: f32,
    /// Thumbstick: x to the right, y forward (up).
    pub stick: [f32; 2],
    /// Lower face button (A on the right controller, X on the left).
    pub lower_button: bool,
    /// Upper face button (B on the right controller, Y on the left).
    pub upper_button: bool,
    pub stick_click: bool,
    /// For a hand steered from a screen by a mouse or a pen: the line of sight through
    /// the pointer, as an origin and a unit direction. Handles are picked along it,
    /// since such a hand cannot be moved in depth to reach them.
    pub ray: Option<(DVec3, DVec3)>,
}

impl Default for HandInput {
    fn default() -> Self {
        Self {
            tracked: false,
            position: DVec3::ZERO,
            rotation: DQuat::IDENTITY,
            trigger: 0.0,
            grip: 0.0,
            stick: [0.0, 0.0],
            lower_button: false,
            upper_button: false,
            stick_click: false,
            ray: None,
        }
    }
}

impl HandInput {
    /// The direction the hand points in.
    pub fn forward(&self) -> DVec3 {
        self.rotation * DVec3::NEG_Z
    }

    /// The direction from the tip back toward the user along the hand.
    pub fn backward(&self) -> DVec3 {
        self.rotation * DVec3::Z
    }

    pub fn up(&self) -> DVec3 {
        self.rotation * DVec3::Y
    }
}

/// Everything the user is doing at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputFrame {
    /// Seconds since the previous frame.
    pub dt: f64,
    /// Where the eyes are, in viewer space.
    pub head: DVec3,
    /// The drawing hand.
    pub main: HandInput,
    /// The other hand.
    pub off: HandInput,
}

impl Default for InputFrame {
    fn default() -> Self {
        Self {
            dt: 1.0 / 90.0,
            head: DVec3::new(0.0, 1.6, 0.0),
            main: HandInput::default(),
            off: HandInput::default(),
        }
    }
}

impl InputFrame {
    /// How far the modifier is pressed, 0 to 1.
    pub fn alt(&self) -> f32 {
        if self.off.tracked {
            self.off.trigger.clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// A trigger counts as pressed above this, and stays pressed until it falls below
/// [`RELEASE`]: a tool is applied until the trigger is let go completely.
pub const PRESS: f32 = 0.06;
pub const RELEASE: f32 = 0.02;
/// A stick counts as flicked in a direction past this deflection.
pub const FLICK: f32 = 0.7;
/// And as back at rest below this.
pub const REST: f32 = 0.3;

/// Something for the device to do in response: a buzz, a click.
#[derive(Clone, Debug, PartialEq)]
pub enum Feedback {
    /// Vibrate a controller. `main` is the drawing hand.
    Haptic {
        main: bool,
        strength: f32,
        seconds: f32,
    },
    /// Play an interface sound by name: "undo", "redo", "pick", "delete", "stop".
    Sound(&'static str),
    /// Tell the user something, such as why a tool did nothing.
    Message(String),
    /// Show or hide the hand-held palette.
    Menu,
}

/// Tracks a two-level control with hysteresis: pressed past one level, released below another.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Latch {
    pub down: bool,
}

/// What a latch did this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    None,
    Pressed,
    Held,
    Released,
}

impl Latch {
    pub fn update(&mut self, value: f32, press: f32, release: f32) -> Edge {
        match (self.down, value) {
            (false, v) if v > press => {
                self.down = true;
                Edge::Pressed
            }
            (true, v) if v < release => {
                self.down = false;
                Edge::Released
            }
            (true, _) => Edge::Held,
            (false, _) => Edge::None,
        }
    }

    pub fn update_bool(&mut self, value: bool) -> Edge {
        self.update(if value { 1.0 } else { 0.0 }, 0.5, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latch_reports_each_edge_once_and_ignores_jitter() {
        let mut latch = Latch::default();
        let edges: Vec<Edge> = [0.0, 0.05, 0.3, 0.04, 0.9, 0.01, 0.0]
            .iter()
            .map(|v| latch.update(*v, PRESS, RELEASE))
            .collect();
        assert_eq!(
            edges,
            [
                Edge::None,
                Edge::None,
                Edge::Pressed,
                Edge::Held,
                Edge::Held,
                Edge::Released,
                Edge::None
            ]
        );
    }

    #[test]
    fn hand_axes_follow_its_rotation() {
        let hand = HandInput {
            rotation: DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2),
            ..HandInput::default()
        };
        assert!((hand.forward() - DVec3::NEG_X).length() < 1e-12);
        assert!((hand.backward() - DVec3::X).length() < 1e-12);
        assert!((hand.up() - DVec3::Y).length() < 1e-12);
    }
}
