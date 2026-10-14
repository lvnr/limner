//! The moving frame along a stroke, and the cross-section rings built on it.
//!
//! One ring per stored point, with no resampling or smoothing and nothing carried
//! from one point to the next: the twist of a ribbon is exactly what the artist's
//! wrist did. This matches how Quill builds its geometry.

use crate::model::{Brush, Point};
use glam::Vec3;

const EPSILON: f32 = 1e-7;

/// Orthonormal frame at a stroke point: tangent, and the two cross-section axes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub t: Vec3,
    pub u: Vec3,
    pub v: Vec3,
}

fn tangent(points: &[Point], i: usize) -> Vec3 {
    tangent_between(points, i, i + 1, i)
}

/// The tangent at point `i`, looking for neighbours from `after` onwards and before
/// `before`. Every point in between must be exactly where point `i` is, so skipping
/// them changes nothing.
fn tangent_between(points: &[Point], i: usize, after: usize, before: usize) -> Vec3 {
    let here = points[i].position();
    // First neighbour on each side that is not on top of this point.
    let mut forward = Vec3::ZERO;
    for other in &points[after..] {
        let d = other.position() - here;
        let len = d.length();
        if len >= EPSILON {
            forward = d / len;
            break;
        }
    }
    let mut backward = Vec3::ZERO;
    for other in points[..before].iter().rev() {
        let d = here - other.position();
        let len = d.length();
        if len >= EPSILON {
            backward = d / len;
            break;
        }
    }
    let sum = forward + backward;
    let len = sum.length();
    if len > EPSILON {
        return sum / len;
    }
    // Every neighbour coincides with this point, or the stroke doubles back exactly:
    // fall back to the overall direction, nudged so it is never zero.
    let overall =
        points[points.len() - 1].position() - points[0].position() + Vec3::new(1e-6, 2e-6, 3e-6);
    overall.normalize()
}

/// The frame at point `i`: `t` along the stroke, `u` across it, `v` along the stored normal.
///
/// Finding the tangent scans past points on top of this one, so calling this for
/// every point of a stroke that stays in one place is quadratic; use
/// [`stroke_frames`] for whole strokes.
pub fn point_frame(points: &[Point], i: usize) -> Frame {
    frame_with_tangent(points, i, tangent(points, i))
}

/// The frame at every point of a stroke, the same as [`point_frame`] gives, in
/// time proportional to the number of points: runs of points at exactly the same
/// place are stepped over in one go.
pub fn stroke_frames(points: &[Point]) -> Vec<Frame> {
    let n = points.len();
    // For each point, where the run of points at exactly its place ends and begins.
    let mut run_end = vec![n; n];
    for i in (0..n.saturating_sub(1)).rev() {
        run_end[i] = if points[i + 1].position() == points[i].position() {
            run_end[i + 1]
        } else {
            i + 1
        };
    }
    let mut run_start = vec![0; n];
    for i in 1..n {
        run_start[i] = if points[i - 1].position() == points[i].position() {
            run_start[i - 1]
        } else {
            i
        };
    }
    (0..n)
        .map(|i| {
            frame_with_tangent(
                points,
                i,
                tangent_between(points, i, run_end[i], run_start[i]),
            )
        })
        .collect()
}

fn frame_with_tangent(points: &[Point], i: usize, t: Vec3) -> Frame {
    let normal = Vec3::from_array(points[i].normal);
    let mut u = normal.cross(t);
    let len = u.length();
    if len >= EPSILON {
        u /= len;
    } else {
        // The stored normal is parallel to the stroke: any perpendicular will do.
        u = t.any_orthonormal_vector();
    }
    let v = t.cross(u).normalize();
    Frame { t, u, v }
}

/// Ellipse brushes are this much thinner along the normal than across it.
pub const ELLIPSE_RATIO: f32 = 0.3;

/// For each vertex of a brush's cross-section: its offset from the centre line as
/// multiples of the half-width along `u` and `v`, and its outward normal in the same basis.
pub fn ring_profile(brush: Brush) -> &'static [RingVertex] {
    use std::sync::OnceLock;
    static CYLINDER: OnceLock<Vec<RingVertex>> = OnceLock::new();
    static ELLIPSE: OnceLock<Vec<RingVertex>> = OnceLock::new();
    const RIBBON: [RingVertex; 2] = [
        RingVertex {
            offset: [1.0, 0.0],
            normal: [0.0, 1.0],
        },
        RingVertex {
            offset: [-1.0, 0.0],
            normal: [0.0, 1.0],
        },
    ];
    const D: f32 = std::f32::consts::FRAC_1_SQRT_2;
    const CUBE: [RingVertex; 4] = [
        RingVertex {
            offset: [-1.0, 1.0],
            normal: [-D, D],
        },
        RingVertex {
            offset: [-1.0, -1.0],
            normal: [-D, -D],
        },
        RingVertex {
            offset: [1.0, -1.0],
            normal: [D, -D],
        },
        RingVertex {
            offset: [1.0, 1.0],
            normal: [D, D],
        },
    ];
    let heptagon = |squash: f32| -> Vec<RingVertex> {
        (0..7)
            .map(|k| {
                let a = k as f32 / 7.0 * std::f32::consts::TAU;
                let (sin, cos) = a.sin_cos();
                let normal = glam::Vec2::new(cos, sin / squash).normalize();
                RingVertex {
                    offset: [cos, sin * squash],
                    normal: normal.to_array(),
                }
            })
            .collect()
    };
    match brush {
        Brush::Line | Brush::Ribbon => &RIBBON,
        Brush::Cylinder => CYLINDER.get_or_init(|| heptagon(1.0)),
        Brush::Ellipse => ELLIPSE.get_or_init(|| heptagon(ELLIPSE_RATIO)),
        Brush::Cube => &CUBE,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingVertex {
    pub offset: [f32; 2],
    pub normal: [f32; 2],
}

/// Layer-space positions of the ring at point `i`.
pub fn ring_positions(points: &[Point], i: usize, brush: Brush, out: &mut Vec<Vec3>) {
    let frame = point_frame(points, i);
    let p = points[i].position();
    let w = points[i].width;
    for vertex in ring_profile(brush) {
        out.push(p + frame.u * (vertex.offset[0] * w) + frame.v * (vertex.offset[1] * w));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(pos: [f32; 3], normal: [f32; 3], width: f32) -> Point {
        Point {
            pos,
            normal,
            dir: [0.0, 0.0, -1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width,
        }
    }

    fn line_along_x(n: usize) -> Vec<Point> {
        (0..n)
            .map(|i| pt([i as f32 * 0.1, 0.0, 0.0], [0.0, 1.0, 0.0], 0.05))
            .collect()
    }

    #[test]
    fn frame_of_a_straight_stroke() {
        let points = line_along_x(4);
        for i in 0..4 {
            let f = point_frame(&points, i);
            assert!((f.t - Vec3::X).length() < 1e-6);
            // u = normal x tangent = Y x X = -Z; v = t x u = X x -Z = Y.
            assert!((f.u - Vec3::NEG_Z).length() < 1e-6);
            assert!(
                (f.v - Vec3::Y).length() < 1e-6,
                "v follows the stored normal"
            );
        }
    }

    #[test]
    fn tangent_averages_the_neighbouring_directions() {
        let points = vec![
            pt([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.1),
            pt([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 0.1),
            pt([1.0, 1.0, 0.0], [0.0, 0.0, 1.0], 0.1),
        ];
        let corner = point_frame(&points, 1);
        assert!((corner.t - Vec3::new(1.0, 1.0, 0.0).normalize()).length() < 1e-6);
        assert!((point_frame(&points, 0).t - Vec3::X).length() < 1e-6);
        assert!((point_frame(&points, 2).t - Vec3::Y).length() < 1e-6);
    }

    #[test]
    fn coincident_points_are_skipped_when_finding_the_tangent() {
        let mut points = line_along_x(3);
        points.insert(1, points[0]);
        points.insert(1, points[0]);
        for i in 0..points.len() {
            let f = point_frame(&points, i);
            assert!((f.t - Vec3::X).length() < 1e-5, "point {i}: {:?}", f.t);
        }
    }

    #[test]
    fn whole_stroke_frames_match_point_frames() {
        let mut points = line_along_x(5);
        // Runs of coincident points at the start, in the middle and at the end, and
        // a point that is only nearly on top of its neighbour.
        for at in [0, 3, 3, 3, 6] {
            points.insert(at, points[at]);
        }
        let len = points.len();
        points.push(points[len - 1]);
        let mut nearly = points[2];
        nearly.pos[1] += 1e-9;
        points.insert(2, nearly);
        let tilted: Vec<Point> = points
            .iter()
            .enumerate()
            .map(|(i, p)| pt([p.pos[0], (i as f32 * 0.7).sin(), p.pos[2]], p.normal, 0.05))
            .collect();
        let same = vec![pt([1.0, 1.0, 1.0], [0.0, 1.0, 0.0], 0.1); 4];
        for stroke in [&points, &tilted, &same, &line_along_x(1), &Vec::new()] {
            let whole = stroke_frames(stroke);
            assert_eq!(whole.len(), stroke.len());
            for (i, frame) in whole.iter().enumerate() {
                assert_eq!(*frame, point_frame(stroke, i), "point {i}");
            }
        }
    }

    #[test]
    fn many_coincident_points_take_linear_time() {
        // Quadratic work over this many points would take minutes in a debug build.
        let points = vec![pt([1.0, 1.0, 1.0], [0.0, 1.0, 0.0], 0.1); 200_000];
        let frames = stroke_frames(&points);
        assert_eq!(frames.len(), points.len());
        assert!(frames.iter().all(|f| f.t.is_finite()));
    }

    #[test]
    fn degenerate_strokes_still_get_an_orthonormal_frame() {
        let same = vec![pt([1.0, 1.0, 1.0], [0.0, 1.0, 0.0], 0.1); 3];
        let parallel = vec![
            pt([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1),
            pt([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1),
        ];
        for points in [&same, &parallel] {
            for i in 0..points.len() {
                let f = point_frame(points, i);
                for axis in [f.t, f.u, f.v] {
                    assert!(axis.is_finite() && (axis.length() - 1.0).abs() < 1e-4);
                }
                assert!(
                    f.t.dot(f.u).abs() < 1e-4
                        && f.u.dot(f.v).abs() < 1e-4
                        && f.v.dot(f.t).abs() < 1e-4
                );
            }
        }
    }

    #[test]
    fn ring_shapes() {
        let points = line_along_x(3);
        let centre = points[1].position();
        let w = points[1].width;
        let ring = |brush| {
            let mut out = Vec::new();
            ring_positions(&points, 1, brush, &mut out);
            out
        };

        let ribbon = ring(Brush::Ribbon);
        assert_eq!(ribbon.len(), 2);
        assert!(
            (ribbon[0] - (centre + Vec3::NEG_Z * w)).length() < 1e-6,
            "first vertex is at +u"
        );
        assert!((ribbon[1] - (centre - Vec3::NEG_Z * w)).length() < 1e-6);

        let cylinder = ring(Brush::Cylinder);
        assert_eq!(cylinder.len(), 7);
        assert!(cylinder
            .iter()
            .all(|v| ((*v - centre).length() - w).abs() < 1e-6));
        assert!(
            (cylinder[0] - (centre + Vec3::NEG_Z * w)).length() < 1e-6,
            "the ring starts on u"
        );

        let ellipse = ring(Brush::Ellipse);
        let along_v = ellipse
            .iter()
            .map(|v| (*v - centre).dot(Vec3::Y).abs())
            .fold(0.0f32, f32::max);
        let along_u = ellipse
            .iter()
            .map(|v| (*v - centre).dot(Vec3::Z).abs())
            .fold(0.0f32, f32::max);
        assert!((along_u - w).abs() < 1e-6, "full width across the stroke");
        assert!(
            along_v <= w * ELLIPSE_RATIO + 1e-6 && along_v > w * ELLIPSE_RATIO * 0.9,
            "thin along the normal"
        );

        let cube = ring(Brush::Cube);
        assert_eq!(cube.len(), 4);
        assert!(cube
            .iter()
            .all(|v| ((*v - centre).length() - w * 2f32.sqrt()).abs() < 1e-6));
        assert_eq!(ring(Brush::Line).len(), 2);
    }

    #[test]
    fn ring_normals_are_unit_and_point_outward() {
        for brush in [Brush::Cylinder, Brush::Ellipse, Brush::Cube] {
            for vertex in ring_profile(brush) {
                let n = glam::Vec2::from_array(vertex.normal);
                assert!((n.length() - 1.0).abs() < 1e-5);
                assert!(n.dot(glam::Vec2::from_array(vertex.offset)) > 0.0);
            }
        }
    }
}
