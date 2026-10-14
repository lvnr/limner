//! Simplifying strokes: removing points a stroke can do without.
//!
//! A point is expendable when the stroke looks the same with a straight run between
//! its neighbours: its position, width, colour, opacity and twist are all close to
//! what interpolation gives. Positions are judged against the stroke's own width, so
//! a hair-thin line keeps the detail a thick one may lose.

use crate::model::{Point, Stroke};
use glam::Vec3;

/// How strongly a change in colour or opacity argues for keeping a point, relative
/// to a position error of one half-width.
const COLOR_WEIGHT: f32 = 4.0;
const TWIST_WEIGHT: f32 = 2.0;
/// The threshold setting is a fraction; at 1.0 a point may stray this many half-widths.
const THRESHOLD_SCALE: f32 = 5.0;

/// How badly `point` would be approximated by a straight run from `a` to `b`.
fn error(a: &Point, b: &Point, point: &Point) -> f32 {
    let (pa, pb, p) = (a.position(), b.position(), point.position());
    let run = pb - pa;
    let length_squared = run.length_squared();
    let t = if length_squared > 0.0 {
        ((p - pa).dot(run) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let mix = |x: f32, y: f32| x + (y - x) * t;
    let width = mix(a.width, b.width);
    let reference = point.width.max(width).max(1e-9);
    let position = (p - pa.lerp(pb, t)).length() / reference;
    let thickness = (point.width - width).abs() / reference;
    let color = (0..3)
        .map(|i| (point.color[i] - mix(a.color[i], b.color[i])).abs())
        .fold(0.0, f32::max)
        * COLOR_WEIGHT;
    let opacity = (point.opacity - mix(a.opacity, b.opacity)).abs() * COLOR_WEIGHT;
    let normal = Vec3::from_array(a.normal)
        .lerp(Vec3::from_array(b.normal), t)
        .normalize_or_zero();
    let twist = (1.0 - Vec3::from_array(point.normal).dot(normal)).max(0.0) * TWIST_WEIGHT;
    position.max(thickness).max(color).max(opacity).max(twist)
}

/// The indices of the points worth keeping, in order.
fn kept_indices(points: &[Point], tolerance: f32) -> Vec<usize> {
    let count = points.len();
    let mut keep = vec![false; count];
    keep[0] = true;
    keep[count - 1] = true;
    // A zero-width end point and the full-width point next to it are a cap; both stay.
    if points[0].width <= 0.0 {
        keep[1] = true;
    }
    if points[count - 1].width <= 0.0 {
        keep[count - 2] = true;
    }
    let anchors: Vec<usize> = (0..count).filter(|i| keep[*i]).collect();
    let mut stack: Vec<(usize, usize)> = anchors.windows(2).map(|w| (w[0], w[1])).collect();
    while let Some((first, last)) = stack.pop() {
        if last <= first + 1 {
            continue;
        }
        let (mut worst, mut worst_error) = (first, -1.0f32);
        for i in first + 1..last {
            let e = error(&points[first], &points[last], &points[i]);
            if e > worst_error {
                (worst, worst_error) = (i, e);
            }
        }
        if worst_error > tolerance {
            keep[worst] = true;
            stack.push((first, worst));
            stack.push((worst, last));
        }
    }
    (0..count).filter(|i| keep[*i]).collect()
}

/// A copy of `stroke` with expendable points removed, or `None` when every point is
/// needed. `threshold` is the Optimize tool's setting, 0 to 1.
pub fn simplify_stroke(stroke: &Stroke, threshold: f32) -> Option<Stroke> {
    let points = stroke.points();
    if points.len() <= 2 || threshold.is_nan() || threshold <= 0.0 {
        return None;
    }
    let kept = kept_indices(points, threshold * THRESHOLD_SCALE);
    if kept.len() == points.len() {
        return None;
    }
    Some(Stroke::new(
        stroke.id,
        stroke.brush,
        stroke.directional,
        kept.into_iter().map(|i| points[i]).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Brush;

    fn point(x: f32, y: f32, width: f32) -> Point {
        Point {
            pos: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            dir: [0.0, 0.0, -1.0],
            color: [0.5; 3],
            opacity: 1.0,
            width,
        }
    }

    fn stroke(points: Vec<Point>) -> Stroke {
        Stroke::new(7, Brush::Cylinder, false, points)
    }

    #[test]
    fn a_straight_even_stroke_keeps_only_its_ends() {
        let straight = stroke((0..50).map(|i| point(i as f32 * 0.01, 0.0, 0.01)).collect());
        let simple = simplify_stroke(&straight, 0.02).unwrap();
        assert_eq!(simple.len(), 2);
        assert_eq!(simple.id, 7);
        assert_eq!(simple.points()[1].pos, [49.0 * 0.01, 0.0, 0.0]);
    }

    #[test]
    fn bends_survive_in_proportion_to_the_threshold() {
        let wave = stroke(
            (0..200)
                .map(|i| point(i as f32 * 0.005, (i as f32 * 0.1).sin() * 0.05, 0.01))
                .collect(),
        );
        let gentle = simplify_stroke(&wave, 0.02).unwrap();
        let harsh = simplify_stroke(&wave, 0.5).unwrap();
        assert!(
            gentle.len() < wave.len() && harsh.len() < gentle.len(),
            "{} {} {}",
            wave.len(),
            gentle.len(),
            harsh.len()
        );
        // At the gentle setting no original point is further than a tenth of the
        // half-width from the simplified centre line.
        for p in wave.points() {
            let nearest = gentle.points().windows(2).map(|w| {
                let (a, b) = (w[0].position(), w[1].position());
                let t = ((p.position() - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                (p.position() - a.lerp(b, t)).length()
            });
            assert!(nearest.fold(f32::MAX, f32::min) <= 0.001 + 1e-6);
        }
    }

    #[test]
    fn caps_colour_changes_and_width_changes_are_kept() {
        let mut points: Vec<Point> = (0..40).map(|i| point(i as f32 * 0.01, 0.0, 0.01)).collect();
        points[0].width = 0.0;
        points[39].width = 0.0;
        points[20].color = [1.0, 0.0, 0.0];
        points[30].width = 0.03;
        let simple = simplify_stroke(&stroke(points), 0.1).unwrap();
        let xs: Vec<i32> = simple
            .points()
            .iter()
            .map(|p| (p.pos[0] * 100.0).round() as i32)
            .collect();
        for needed in [0, 1, 19, 20, 21, 29, 30, 31, 38, 39] {
            assert!(xs.contains(&needed), "point {needed} was dropped: {xs:?}");
        }
        assert!(simple.len() <= 12, "{xs:?}");
    }

    #[test]
    fn strokes_that_cannot_lose_points_are_left_alone() {
        assert!(simplify_stroke(
            &stroke(vec![point(0.0, 0.0, 0.01), point(1.0, 0.0, 0.01)]),
            0.5
        )
        .is_none());
        let zigzag = stroke(
            (0..10)
                .map(|i| point(i as f32 * 0.01, if i % 2 == 0 { 0.0 } else { 0.05 }, 0.01))
                .collect(),
        );
        assert!(simplify_stroke(&zigzag, 0.02).is_none());
        assert!(simplify_stroke(&zigzag, 0.0).is_none());
    }
}
