//! Time in integer flicks.
//!
//! A flick is 1/705,600,000 of a second. Every common frame rate, and Quill's
//! 12,600 ticks per second, divide it exactly, so frame boundaries and imported
//! key times are represented without rounding.

use serde::{Deserialize, Serialize};

/// A point in time or a duration, in flicks.
pub type Flicks = i64;

pub const FLICKS_PER_SECOND: Flicks = 705_600_000;

/// Quill stores time in ticks of 1/12,600 of a second.
pub const QUILL_TICKS_PER_SECOND: i64 = 12_600;
pub const FLICKS_PER_QUILL_TICK: Flicks = FLICKS_PER_SECOND / QUILL_TICKS_PER_SECOND;

pub fn from_seconds(seconds: f64) -> Flicks {
    (seconds * FLICKS_PER_SECOND as f64).round() as Flicks
}

pub fn to_seconds(t: Flicks) -> f64 {
    t as f64 / FLICKS_PER_SECOND as f64
}

pub fn from_quill_ticks(ticks: i64) -> Flicks {
    ticks.saturating_mul(FLICKS_PER_QUILL_TICK)
}

/// Frames per second as a rational number, so rates such as 24000/1001 stay exact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrameRate {
    pub num: u32,
    pub den: u32,
}

impl Default for FrameRate {
    fn default() -> Self {
        Self::whole(24)
    }
}

impl FrameRate {
    pub const fn new(num: u32, den: u32) -> Self {
        Self { num, den }
    }

    pub const fn whole(fps: u32) -> Self {
        Self { num: fps, den: 1 }
    }

    /// Converts a decimal rate, snapping to whole and NTSC rates where they match.
    pub fn from_f64(fps: f64) -> Self {
        if !(fps.is_finite() && fps > 0.0) {
            return Self::default();
        }
        let rounded = fps.round();
        if (fps - rounded).abs() < 1e-6 {
            return Self::whole(rounded as u32);
        }
        let ntsc = rounded * 1000.0 / 1001.0;
        if (fps - ntsc).abs() < 5e-3 {
            return Self::new(rounded as u32 * 1000, 1001);
        }
        Self::new((fps * 1000.0).round() as u32, 1000)
    }

    pub fn as_f64(self) -> f64 {
        self.num as f64 / self.den as f64
    }

    pub fn is_valid(self) -> bool {
        self.num > 0 && self.den > 0
    }

    /// Index of the frame that contains time `t` (floor division, so negative times work).
    pub fn frame_at(self, t: Flicks) -> i64 {
        let n = t as i128 * self.num as i128;
        let d = FLICKS_PER_SECOND as i128 * self.den as i128;
        n.div_euclid(d) as i64
    }

    /// The first instant of `frame`: the smallest `t` with `frame_at(t) == frame`.
    pub fn frame_start(self, frame: i64) -> Flicks {
        let n = frame as i128 * FLICKS_PER_SECOND as i128 * self.den as i128;
        let d = self.num as i128;
        // Ceiling division that is correct for negative numerators.
        (n.div_euclid(d) + i128::from(n.rem_euclid(d) != 0)) as Flicks
    }

    /// Length of one frame, exact for rates that divide the flick.
    pub fn frame_duration(self) -> Flicks {
        self.frame_start(1)
    }

    /// The time `t` moved to the start of the frame that contains it.
    pub fn snap_floor(self, t: Flicks) -> Flicks {
        self.frame_start(self.frame_at(t))
    }

    /// The frame boundary nearest to `t`.
    pub fn snap_nearest(self, t: Flicks) -> Flicks {
        let lower = self.snap_floor(t);
        let upper = self.frame_start(self.frame_at(t) + 1);
        if t - lower <= upper - t {
            lower
        } else {
            upper
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quill_ticks_convert_exactly() {
        assert_eq!(FLICKS_PER_QUILL_TICK, 56_000);
        assert_eq!(from_quill_ticks(12_600), FLICKS_PER_SECOND);
        // One 24 fps frame is 525 Quill ticks.
        assert_eq!(from_quill_ticks(525), FrameRate::whole(24).frame_duration());
    }

    #[test]
    fn common_rates_have_exact_frame_durations() {
        for fps in [
            1, 6, 8, 10, 12, 15, 24, 25, 30, 48, 50, 60, 72, 90, 100, 120,
        ] {
            let rate = FrameRate::whole(fps);
            assert_eq!(
                rate.frame_duration() * fps as i64,
                FLICKS_PER_SECOND,
                "{fps} fps"
            );
        }
        // 24000/1001 also divides the flick.
        let ntsc = FrameRate::new(24_000, 1001);
        assert_eq!(ntsc.frame_duration() * 24_000, FLICKS_PER_SECOND * 1001);
    }

    #[test]
    fn frame_at_and_frame_start_are_inverse() {
        for rate in [
            FrameRate::whole(24),
            FrameRate::new(30_000, 1001),
            FrameRate::whole(7),
            FrameRate::new(12_345, 1000),
        ] {
            for frame in [-3i64, -1, 0, 1, 2, 23, 24, 1000, 86_399] {
                let start = rate.frame_start(frame);
                assert_eq!(rate.frame_at(start), frame, "{rate:?} frame {frame}");
                assert_eq!(
                    rate.frame_at(start - 1),
                    frame - 1,
                    "{rate:?} frame {frame}"
                );
            }
        }
    }

    #[test]
    fn decimal_rates_snap_to_rationals() {
        assert_eq!(FrameRate::from_f64(24.0), FrameRate::whole(24));
        assert_eq!(FrameRate::from_f64(23.976), FrameRate::new(24_000, 1001));
        assert_eq!(FrameRate::from_f64(29.97), FrameRate::new(30_000, 1001));
        assert_eq!(FrameRate::from_f64(12.5), FrameRate::new(12_500, 1000));
        assert_eq!(FrameRate::from_f64(f64::NAN), FrameRate::whole(24));
    }

    #[test]
    fn seconds_round_trip() {
        assert_eq!(from_seconds(1.5), 1_058_400_000);
        assert!((to_seconds(from_seconds(0.123456)) - 0.123456).abs() < 1e-9);
    }

    #[test]
    fn snapping() {
        let rate = FrameRate::whole(24);
        let d = rate.frame_duration();
        assert_eq!(rate.snap_floor(d + 5), d);
        assert_eq!(rate.snap_nearest(d + 5), d);
        assert_eq!(rate.snap_nearest(2 * d - 5), 2 * d);
    }
}
