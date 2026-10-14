//! Turns the path of the hand into the points of a stroke.
//!
//! The hand is sampled as it moves; nothing is smoothed or resampled along the way, so
//! a stroke is exactly what the hand did. What this module adds is what the brush
//! adds: the width from pressure, the tapers, and the ends. Ends are ordinary points:
//! a flat end is a zero-width point just beyond the last full-width one, and a round
//! end is a run of points whose width swells along a semicircle.

use super::settings::{Cap, Taper};
use crate::model::{Brush, Point};
use glam::DVec3;

/// The most points one stroke may have. A longer gesture continues in a new stroke.
pub const MAX_STROKE_POINTS: usize = 8192;

/// The hand at one moment of a stroke, in the layer's space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    pub pos: DVec3,
    /// The side a flat stroke faces: the axis of the hand, pointing back at the user.
    pub normal: DVec3,
    /// The hand's up axis. Gives a single click its orientation.
    pub up: DVec3,
    /// From the eye to this point when it was drawn.
    pub view_dir: DVec3,
    /// Trigger or pen pressure, 0 to 1.
    pub pressure: f32,
    /// Half-width at full pressure, in layer units.
    pub size: f32,
    pub color: [f32; 3],
    pub opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrokeStyle {
    pub brush: Brush,
    pub cap: Cap,
    pub width_taper: Taper,
    pub opacity_taper: Taper,
    /// How far beyond the end the closing point of a flat cap sits, in layer units.
    pub cap_gap: f32,
}

/// How far along the end of a stroke the automatic taper reaches, in half-widths.
const AUTO_TAPER_WIDTHS: f32 = 20.0;
/// Round ends are drawn with at least this many points per half-width of length.
const CAP_STEPS: f32 = 6.0;

fn lerp_sample(a: &Sample, b: &Sample, t: f32) -> Sample {
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let mix_dir = |x: DVec3, y: DVec3| x.lerp(y, t as f64).try_normalize().unwrap_or(x);
    Sample {
        pos: a.pos.lerp(b.pos, t as f64),
        normal: mix_dir(a.normal, b.normal),
        up: mix_dir(a.up, b.up),
        view_dir: mix_dir(a.view_dir, b.view_dir),
        pressure: mix(a.pressure, b.pressure),
        size: mix(a.size, b.size),
        color: [
            mix(a.color[0], b.color[0]),
            mix(a.color[1], b.color[1]),
            mix(a.color[2], b.color[2]),
        ],
        opacity: mix(a.opacity, b.opacity),
    }
}

/// The semicircular profile of a round end: zero at the tip, full after one half-width.
fn round_cap(distance_from_tip: f32, radius: f32) -> f32 {
    if radius <= 0.0 || distance_from_tip >= radius {
        return 1.0;
    }
    let u = 1.0 - (distance_from_tip / radius).max(0.0);
    (1.0 - u * u).max(0.0).sqrt()
}

/// The automatic taper: eases in from the tip over `reach`.
fn auto_taper(distance_from_tip: f32, reach: f32) -> f32 {
    if reach <= 0.0 || distance_from_tip >= reach {
        return 1.0;
    }
    ((distance_from_tip / reach).max(0.0) * std::f32::consts::FRAC_PI_2).sin()
}

/// A single click has no path. It becomes a short stroke along the hand's up axis, one
/// brush width long, so that the brush leaves its own shape: a disc, a ball, a square, a cube.
fn dot(sample: &Sample) -> Vec<Sample> {
    let axis = sample.up.try_normalize().unwrap_or(DVec3::Y) * sample.size as f64;
    let at = |offset: f64| Sample {
        pos: sample.pos + axis * offset,
        pressure: 1.0,
        ..*sample
    };
    vec![at(-1.0), at(0.0), at(1.0)]
}

/// Adds points where a round end needs them to look round.
fn refine_round_ends(samples: &[Sample], lengths: &[f32]) -> (Vec<Sample>, Vec<f32>) {
    let total = *lengths.last().unwrap_or(&0.0);
    let mut out = Vec::with_capacity(samples.len() + 2 * CAP_STEPS as usize);
    let mut out_lengths = Vec::with_capacity(out.capacity());
    out.push(samples[0]);
    out_lengths.push(0.0);
    for i in 1..samples.len() {
        let (a, b) = (&samples[i - 1], &samples[i]);
        let (s0, s1) = (lengths[i - 1], lengths[i]);
        let radius = a.size.max(b.size);
        let step = radius / CAP_STEPS;
        // Only the parts of the segment within one half-width of either tip need refining.
        let near_tip = s0 < radius || total - s1 < radius;
        if near_tip && step > 0.0 && s1 - s0 > step * 1.01 {
            let pieces = (((s1 - s0) / step).ceil() as usize).min(64);
            for k in 1..pieces {
                let t = k as f32 / pieces as f32;
                let s = s0 + (s1 - s0) * t;
                if s < radius || total - s < radius {
                    out.push(lerp_sample(a, b, t));
                    out_lengths.push(s);
                }
            }
        }
        out.push(*b);
        out_lengths.push(s1);
    }
    (out, out_lengths)
}

/// The points of a stroke drawn through `samples`.
pub fn build_points(samples: &[Sample], style: &StrokeStyle) -> Vec<Point> {
    let Some(first) = samples.first() else {
        return Vec::new();
    };
    let path_length: f64 = samples
        .windows(2)
        .map(|w| w[0].pos.distance(w[1].pos))
        .sum();
    // Shorter than a tenth of the brush: a click, not a stroke.
    let is_dot = path_length < first.size as f64 * 0.1;
    let dot_samples;
    let samples = if is_dot {
        dot_samples = dot(first);
        &dot_samples[..]
    } else {
        samples
    };

    let mut lengths = Vec::with_capacity(samples.len());
    let mut total = 0.0f32;
    lengths.push(0.0);
    for pair in samples.windows(2) {
        total += pair[0].pos.distance(pair[1].pos) as f32;
        lengths.push(total);
    }
    let (samples, lengths) = if style.cap == Cap::Round {
        refine_round_ends(samples, &lengths)
    } else {
        (samples.to_vec(), lengths)
    };

    let mut points: Vec<Point> = Vec::with_capacity(samples.len() + 2);
    for (sample, s) in samples.iter().zip(&lengths) {
        let from_tip = s.min(total - s);
        let pressure = if is_dot {
            1.0
        } else {
            sample.pressure.clamp(0.0, 1.0)
        };
        let taper = auto_taper(from_tip, (total * 0.5).min(sample.size * AUTO_TAPER_WIDTHS));
        let mut width = sample.size;
        if style.width_taper.uses_pressure() {
            width *= pressure;
        }
        if style.width_taper.is_automatic() && !is_dot {
            width *= taper;
        }
        if style.cap == Cap::Round {
            width *= round_cap(from_tip, sample.size);
        }
        let mut opacity = sample.opacity;
        if style.opacity_taper.uses_pressure() {
            opacity *= pressure;
        }
        if style.opacity_taper.is_automatic() && !is_dot {
            opacity *= taper;
        }
        points.push(Point {
            pos: sample.pos.as_vec3().to_array(),
            normal: sample.normal.as_vec3().to_array(),
            dir: sample.view_dir.as_vec3().to_array(),
            color: sample.color,
            opacity: opacity.clamp(0.0, 1.0),
            width: width.max(0.0),
        });
    }

    if style.cap == Cap::Flat && points.len() >= 2 {
        let closing = |end: usize, neighbour: usize| {
            let (at, toward) = (samples[end].pos, samples[neighbour].pos);
            let outward = (at - toward).try_normalize().unwrap_or(DVec3::X);
            Point {
                pos: (at + outward * style.cap_gap as f64).as_vec3().to_array(),
                width: 0.0,
                ..points[end]
            }
        };
        let (start, end) = (closing(0, 1), closing(points.len() - 1, points.len() - 2));
        points.insert(0, start);
        points.push(end);
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(x: f64, pressure: f32) -> Sample {
        Sample {
            pos: DVec3::new(x, 0.0, 0.0),
            normal: DVec3::Z,
            up: DVec3::Y,
            view_dir: DVec3::NEG_Z,
            pressure,
            size: 0.01,
            color: [0.2, 0.4, 0.6],
            opacity: 0.8,
        }
    }

    fn style(brush: Brush, cap: Cap) -> StrokeStyle {
        StrokeStyle {
            brush,
            cap,
            width_taper: Taper::None,
            opacity_taper: Taper::None,
            cap_gap: 1e-4,
        }
    }

    fn line(count: usize, spacing: f64) -> Vec<Sample> {
        (0..count)
            .map(|i| sample(i as f64 * spacing, 1.0))
            .collect()
    }

    #[test]
    fn an_open_ribbon_is_exactly_the_samples() {
        let points = build_points(&line(5, 0.02), &style(Brush::Ribbon, Cap::None));
        assert_eq!(points.len(), 5);
        assert!(points
            .iter()
            .all(|p| p.width == 0.01 && p.opacity == 0.8 && p.color == [0.2, 0.4, 0.6]));
        assert_eq!(points[3].pos, [0.06, 0.0, 0.0]);
        assert_eq!(points[0].normal, [0.0, 0.0, 1.0]);
        assert_eq!(points[0].dir, [0.0, 0.0, -1.0]);
    }

    #[test]
    fn a_flat_end_is_a_zero_width_point_just_beyond_the_stroke() {
        let points = build_points(&line(4, 0.02), &style(Brush::Cube, Cap::Flat));
        assert_eq!(points.len(), 6);
        assert_eq!(points[0].width, 0.0);
        assert!(
            (points[0].pos[0] + 1e-4).abs() < 1e-7,
            "{:?}",
            points[0].pos
        );
        assert_eq!(points[1].width, 0.01);
        assert_eq!(points[5].width, 0.0);
        assert!((points[5].pos[0] - 0.0601).abs() < 1e-6);
    }

    #[test]
    fn a_round_end_swells_along_a_semicircle() {
        let points = build_points(&line(11, 0.01), &style(Brush::Cylinder, Cap::Round));
        assert_eq!(points[0].width, 0.0);
        assert_eq!(points.last().unwrap().width, 0.0);
        // Enough points inside the first half-width to draw the curve.
        let in_cap: Vec<&Point> = points.iter().filter(|p| p.pos[0] < 0.01 - 1e-6).collect();
        assert!(in_cap.len() >= 6, "{} points in the cap", in_cap.len());
        for p in &in_cap {
            let s = p.pos[0];
            let expected = (0.01f32.powi(2) - (0.01 - s).powi(2)).max(0.0).sqrt();
            assert!(
                (p.width - expected).abs() < 1e-5,
                "at {s}: {} vs {expected}",
                p.width
            );
        }
        // The middle of the stroke is untouched: one point per sample, full width.
        let middle: Vec<&Point> = points
            .iter()
            .filter(|p| p.pos[0] > 0.0095 && p.pos[0] < 0.0905)
            .collect();
        assert_eq!(middle.len(), 9);
        assert!(middle.iter().all(|p| p.width == 0.01));
    }

    #[test]
    fn a_click_leaves_the_shape_of_the_brush() {
        // Round ribbon: a disc. Zero width at both tips, a full circle profile between.
        let disc = build_points(&[sample(0.0, 0.3)], &style(Brush::Ribbon, Cap::Round));
        assert_eq!(disc[0].width, 0.0);
        assert_eq!(disc.last().unwrap().width, 0.0);
        assert!(disc.len() >= 13);
        for p in &disc {
            let y = p.pos[1];
            assert!(
                (y * y + p.width * p.width - 1e-4).abs() < 2e-6,
                "{y} {}",
                p.width
            );
            assert_eq!(p.pos[0], 0.0);
        }
        // Cube: a block one brush width long with flat ends, whatever the pressure was.
        let cube = build_points(&[sample(0.0, 0.3)], &style(Brush::Cube, Cap::Flat));
        assert_eq!(cube.len(), 5);
        assert_eq!(cube[1].width, 0.01);
        assert!((cube[3].pos[1] - cube[1].pos[1] - 0.02).abs() < 1e-7);
        // Plain ribbon: a square.
        let square = build_points(&[sample(0.0, 1.0)], &style(Brush::Ribbon, Cap::None));
        assert_eq!(square.len(), 3);
        assert!((square[2].pos[1] - square[0].pos[1] - 0.02).abs() < 1e-7);
    }

    #[test]
    fn pressure_drives_width_or_opacity_when_asked() {
        let samples = vec![sample(0.0, 0.25), sample(0.05, 0.5), sample(0.1, 1.0)];
        let by_width = build_points(
            &samples,
            &StrokeStyle {
                width_taper: Taper::Pressure,
                ..style(Brush::Ribbon, Cap::None)
            },
        );
        assert_eq!(
            by_width.iter().map(|p| p.width).collect::<Vec<_>>(),
            [0.0025, 0.005, 0.01]
        );
        assert!(by_width.iter().all(|p| p.opacity == 0.8));
        let by_opacity = build_points(
            &samples,
            &StrokeStyle {
                opacity_taper: Taper::Pressure,
                ..style(Brush::Ribbon, Cap::None)
            },
        );
        assert_eq!(
            by_opacity.iter().map(|p| p.opacity).collect::<Vec<_>>(),
            [0.2, 0.4, 0.8]
        );
        assert!(by_opacity.iter().all(|p| p.width == 0.01));
    }

    #[test]
    fn the_automatic_taper_thins_both_ends() {
        let points = build_points(
            &line(41, 0.01),
            &StrokeStyle {
                width_taper: Taper::Auto,
                ..style(Brush::Ribbon, Cap::None)
            },
        );
        assert_eq!(points[0].width, 0.0);
        assert_eq!(points[40].width, 0.0);
        assert!(
            (points[20].width - 0.01).abs() < 1e-6,
            "full width in the middle"
        );
        assert!(
            points.windows(2).take(20).all(|w| w[1].width >= w[0].width),
            "widening from the start"
        );
        assert!(points[10].width > 0.006 && points[10].width < 0.0085);
    }

    #[test]
    fn nothing_in_gives_nothing_out() {
        assert!(build_points(&[], &style(Brush::Cube, Cap::Flat)).is_empty());
    }
}
