//! What stands between a point and a light: the strokes of a scene as solid capsules,
//! kept in a tree of boxes so that a ray only looks at the few strokes near its path.
//!
//! Every segment of a stroke, from one point to the next, is a capsule: a tube as wide
//! as the stroke with a round cap at each end. Flat strokes are treated the same way,
//! which makes a ribbon throw a slightly rounder shadow than it should.

use glam::DVec3;

/// Which stroke a segment comes from, and where in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentKey {
    /// The drawing the stroke belongs to, numbered by whoever builds the blockers.
    pub source: u32,
    /// The stroke's place in its drawing.
    pub stroke: u32,
    /// The segment's place in its stroke: it runs from this point to the next.
    pub index: u32,
}

impl SegmentKey {
    pub fn same_stroke(&self, other: &SegmentKey) -> bool {
        self.source == other.source && self.stroke == other.stroke
    }
}

/// One segment of a stroke in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Capsule {
    pub a: DVec3,
    pub b: DVec3,
    pub radius: f64,
    /// How much light it stops, 0 to 1.
    pub opacity: f64,
    pub key: SegmentKey,
}

impl Capsule {
    fn contains(&self, p: DVec3) -> bool {
        let ab = self.b - self.a;
        let length2 = ab.length_squared();
        let along = if length2 > 0.0 {
            ((p - self.a).dot(ab) / length2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (self.a + ab * along).distance_squared(p) <= self.radius * self.radius
    }

    /// How far along the ray it first enters the capsule, for a ray that starts outside.
    fn entry(&self, origin: DVec3, dir: DVec3) -> Option<f64> {
        let r2 = self.radius * self.radius;
        let mut best: Option<f64> = None;
        let mut offer = |t: f64| {
            if t >= 0.0 && best.is_none_or(|b| t < b) {
                best = Some(t);
            }
        };
        for end in [self.a, self.b] {
            let oc = origin - end;
            let b = dir.dot(oc);
            let h = b * b - (oc.length_squared() - r2);
            if h >= 0.0 {
                offer(-b - h.sqrt());
            }
        }
        // The side of the tube between the caps.
        let ba = self.b - self.a;
        let oa = origin - self.a;
        let baba = ba.length_squared();
        let bard = ba.dot(dir);
        let baoa = ba.dot(oa);
        let a = baba - bard * bard;
        if a > 1e-12 * baba {
            let b = baba * dir.dot(oa) - baoa * bard;
            let c = baba * oa.length_squared() - baoa * baoa - r2 * baba;
            let h = b * b - a * c;
            if h >= 0.0 {
                let t = (-b - h.sqrt()) / a;
                let y = baoa + t * bard;
                if (0.0..=baba).contains(&y) {
                    offer(t);
                }
            }
        }
        best
    }

    fn min(&self) -> DVec3 {
        self.a.min(self.b) - DVec3::splat(self.radius)
    }

    fn max(&self) -> DVec3 {
        self.a.max(self.b) + DVec3::splat(self.radius)
    }

    fn center(&self) -> DVec3 {
        (self.a + self.b) * 0.5
    }
}

/// A ray toward a light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: DVec3,
    /// Unit length.
    pub dir: DVec3,
    /// How far the light is; infinite for a sun.
    pub reach: f64,
}

#[derive(Clone, Copy, Debug)]
struct Node {
    min: DVec3,
    max: DVec3,
    /// A leaf's first capsule, or an inner node's first child (the second follows it).
    first: u32,
    /// Capsules in a leaf; 0 for an inner node.
    count: u32,
}

/// Capsules a leaf of the tree holds at most.
const LEAF: usize = 4;
/// Below this share of light getting through, a ray counts as fully blocked.
const OPAQUE: f64 = 1e-4;

/// Strokes that throw shadows, ready to be asked what blocks a ray.
#[derive(Clone, Debug, Default)]
pub struct Blockers {
    capsules: Vec<Capsule>,
    nodes: Vec<Node>,
}

impl Blockers {
    /// Builds the tree once; every ray afterwards reuses it. Capsules that stop no
    /// light are left out.
    pub fn new(capsules: impl IntoIterator<Item = Capsule>) -> Self {
        let mut capsules: Vec<Capsule> = capsules
            .into_iter()
            .filter(|c| c.opacity > 0.0 && c.radius > 0.0 && c.a.is_finite() && c.b.is_finite())
            .collect();
        let mut nodes = Vec::with_capacity(2 * capsules.len() / LEAF + 1);
        if !capsules.is_empty() {
            nodes.push(Node {
                min: DVec3::ZERO,
                max: DVec3::ZERO,
                first: 0,
                count: 0,
            });
            let len = capsules.len();
            split(&mut capsules, &mut nodes, 0, 0, len);
        }
        Self { capsules, nodes }
    }

    pub fn len(&self) -> usize {
        self.capsules.len()
    }

    pub fn is_empty(&self) -> bool {
        self.capsules.is_empty()
    }

    /// Adds to `found` the strokes that have a segment containing `p`, once each.
    pub fn strokes_containing(&self, p: DVec3, found: &mut Vec<SegmentKey>) {
        self.visit(
            |min, max| p.cmpge(min).all() && p.cmple(max).all(),
            |capsule| {
                if capsule.contains(p) && !found.iter().any(|k| k.same_stroke(&capsule.key)) {
                    found.push(capsule.key);
                }
                true
            },
        );
    }

    /// The share of light that gets along `ray` past the strokes, 0 to 1. Capsules the
    /// ray starts inside do not block it, and nor do those `skip` rules out. A stroke
    /// the ray passes through more than once (where its segments overlap, or where it
    /// curls) stops its share of light only once.
    pub fn light_through(&self, ray: &Ray, skip: impl Fn(&SegmentKey) -> bool) -> f64 {
        let inverse = ray.dir.recip();
        let mut through = 1.0;
        let mut crossed: Vec<SegmentKey> = Vec::new();
        self.visit(
            |min, max| slab(ray.origin, inverse, min, max, ray.reach),
            |capsule| {
                if skip(&capsule.key)
                    || crossed.iter().any(|k| k.same_stroke(&capsule.key))
                    || capsule.contains(ray.origin)
                {
                    return true;
                }
                if capsule
                    .entry(ray.origin, ray.dir)
                    .is_some_and(|t| t < ray.reach)
                {
                    crossed.push(capsule.key);
                    through *= 1.0 - capsule.opacity.min(1.0);
                }
                through > OPAQUE
            },
        );
        if through > OPAQUE {
            through
        } else {
            0.0
        }
    }

    /// Walks the tree into every box `enter` accepts and offers each capsule in it to
    /// `each`, until `each` returns false.
    fn visit(&self, enter: impl Fn(DVec3, DVec3) -> bool, mut each: impl FnMut(&Capsule) -> bool) {
        if self.nodes.is_empty() {
            return;
        }
        // The tree halves its capsules at every level, so its depth is about the
        // logarithm of their number and this stack never fills.
        let mut stack = [0u32; 128];
        let mut depth = 1;
        while depth > 0 {
            depth -= 1;
            let node = &self.nodes[stack[depth] as usize];
            if !enter(node.min, node.max) {
                continue;
            }
            if node.count == 0 {
                stack[depth] = node.first;
                stack[depth + 1] = node.first + 1;
                depth += 2;
                continue;
            }
            let first = node.first as usize;
            for capsule in &self.capsules[first..first + node.count as usize] {
                if !each(capsule) {
                    return;
                }
            }
        }
    }
}

/// Fills in node `at` for capsules `start..end`, splitting them in half along the
/// longest side of the box their middles span until they are few enough for a leaf.
fn split(capsules: &mut [Capsule], nodes: &mut Vec<Node>, at: usize, start: usize, end: usize) {
    let part = &mut capsules[start..end];
    let (mut min, mut max) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    let (mut low, mut high) = (DVec3::INFINITY, DVec3::NEG_INFINITY);
    for c in part.iter() {
        min = min.min(c.min());
        max = max.max(c.max());
        low = low.min(c.center());
        high = high.max(c.center());
    }
    nodes[at].min = min;
    nodes[at].max = max;
    let span = high - low;
    if part.len() <= LEAF || span.max_element() <= 0.0 {
        nodes[at].first = start as u32;
        nodes[at].count = part.len() as u32;
        return;
    }
    let axis = if span.x >= span.y && span.x >= span.z {
        0
    } else if span.y >= span.z {
        1
    } else {
        2
    };
    let half = part.len() / 2;
    part.select_nth_unstable_by(half, |p, q| p.center()[axis].total_cmp(&q.center()[axis]));
    let left = nodes.len();
    let empty = Node {
        min: DVec3::ZERO,
        max: DVec3::ZERO,
        first: 0,
        count: 0,
    };
    nodes.push(empty);
    nodes.push(empty);
    nodes[at].first = left as u32;
    nodes[at].count = 0;
    split(capsules, nodes, left, start, start + half);
    split(capsules, nodes, left + 1, start + half, end);
}

/// Whether a ray from `origin` within `reach` passes through the box.
fn slab(origin: DVec3, inverse: DVec3, min: DVec3, max: DVec3, reach: f64) -> bool {
    let t1 = (min - origin) * inverse;
    let t2 = (max - origin) * inverse;
    // A ray parallel to a pair of sides gives infinities, which compare as they should.
    // One that also starts exactly on such a side gives 0 times infinity, which is not
    // a number and which `max` and `min` pass over; such a ray could only graze what is
    // inside, as every capsule in a box lies within it.
    let enter =
        t1.x.min(t2.x)
            .max(t1.y.min(t2.y))
            .max(t1.z.min(t2.z))
            .max(0.0);
    let leave =
        t1.x.max(t2.x)
            .min(t1.y.max(t2.y))
            .min(t1.z.max(t2.z))
            .min(reach);
    enter <= leave
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(stroke: u32, index: u32) -> SegmentKey {
        SegmentKey {
            source: 0,
            stroke,
            index,
        }
    }

    fn capsule(a: DVec3, b: DVec3, radius: f64, stroke: u32) -> Capsule {
        Capsule {
            a,
            b,
            radius,
            opacity: 1.0,
            key: key(stroke, 0),
        }
    }

    fn up_from(origin: DVec3) -> Ray {
        Ray {
            origin,
            dir: DVec3::Y,
            reach: f64::INFINITY,
        }
    }

    #[test]
    fn a_ray_enters_a_capsule_through_its_side_or_its_caps() {
        let c = capsule(
            DVec3::new(-1.0, 1.0, 0.0),
            DVec3::new(1.0, 1.0, 0.0),
            0.1,
            0,
        );
        let side = c.entry(DVec3::ZERO, DVec3::Y).unwrap();
        assert!((side - 0.9).abs() < 1e-9, "{side}");
        let cap = c.entry(DVec3::new(1.05, 0.0, 0.0), DVec3::Y).unwrap();
        assert!(cap > 0.9 && cap < 1.0, "{cap}");
        assert_eq!(c.entry(DVec3::new(1.2, 0.0, 0.0), DVec3::Y), None);
        // Along its axis, from beyond one end.
        let along = c.entry(DVec3::new(-3.0, 1.0, 0.0), DVec3::X).unwrap();
        assert!((along - 1.9).abs() < 1e-9, "{along}");
        // Behind the ray is not in its way.
        assert_eq!(c.entry(DVec3::new(0.0, 2.0, 0.0), DVec3::Y), None);
        // A single point is a ball.
        let ball = capsule(DVec3::Y, DVec3::Y, 0.25, 1);
        assert!((ball.entry(DVec3::ZERO, DVec3::Y).unwrap() - 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_stroke_in_the_way_stops_the_light_and_one_beside_it_does_not() {
        let blockers = Blockers::new([
            capsule(
                DVec3::new(-1.0, 1.0, 0.0),
                DVec3::new(1.0, 1.0, 0.0),
                0.1,
                0,
            ),
            capsule(DVec3::new(5.0, 1.0, 0.0), DVec3::new(6.0, 1.0, 0.0), 0.1, 1),
        ]);
        assert_eq!(
            blockers.light_through(&up_from(DVec3::ZERO), |_| false),
            0.0
        );
        assert_eq!(
            blockers.light_through(&up_from(DVec3::new(3.0, 0.0, 0.0)), |_| false),
            1.0
        );
        // A lamp nearer than the stroke is not blocked by it.
        let short = Ray {
            reach: 0.5,
            ..up_from(DVec3::ZERO)
        };
        assert_eq!(blockers.light_through(&short, |_| false), 1.0);
        // Ruled out by the caller.
        assert_eq!(
            blockers.light_through(&up_from(DVec3::ZERO), |k| k.stroke == 0),
            1.0
        );
    }

    #[test]
    fn a_ray_that_starts_inside_a_capsule_is_not_blocked_by_it() {
        let blockers = Blockers::new([capsule(
            DVec3::new(-1.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            0.1,
            0,
        )]);
        assert_eq!(
            blockers.light_through(&up_from(DVec3::ZERO), |_| false),
            1.0
        );
        let mut inside = Vec::new();
        blockers.strokes_containing(DVec3::new(0.5, 0.05, 0.0), &mut inside);
        assert_eq!(inside, vec![key(0, 0)]);
        inside.clear();
        blockers.strokes_containing(DVec3::new(0.5, 0.2, 0.0), &mut inside);
        assert!(inside.is_empty());
    }

    #[test]
    fn see_through_strokes_let_some_light_by_once_per_stroke() {
        let half = |y: f64, stroke: u32, index: u32| Capsule {
            a: DVec3::new(-1.0, y, 0.0),
            b: DVec3::new(1.0, y, 0.0),
            radius: 0.1,
            opacity: 0.5,
            key: key(stroke, index),
        };
        // Two overlapping segments of one stroke, and another stroke above them.
        let blockers = Blockers::new([half(1.0, 0, 0), half(1.05, 0, 1), half(2.0, 1, 0)]);
        let through = blockers.light_through(&up_from(DVec3::ZERO), |_| false);
        assert!((through - 0.25).abs() < 1e-9, "{through}");
    }

    #[test]
    fn the_tree_finds_what_looking_at_every_capsule_finds() {
        // A tangle of short segments, and rays in many directions through it.
        let mut capsules = Vec::new();
        let mut seed = 7u64;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) as f64) / f64::from(1u32 << 31)
        };
        for stroke in 0..300 {
            let a = DVec3::new(random(), random(), random()) * 4.0 - 2.0;
            let b = a + (DVec3::new(random(), random(), random()) - 0.5) * 0.4;
            capsules.push(Capsule {
                key: key(stroke, 0),
                ..capsule(a, b, 0.02 + random() * 0.05, stroke)
            });
        }
        let blockers = Blockers::new(capsules.clone());
        for _ in 0..500 {
            let origin = DVec3::new(random(), random(), random()) * 5.0 - 2.5;
            let dir = (DVec3::new(random(), random(), random()) - 0.5).normalize();
            let reach = if random() < 0.5 {
                f64::INFINITY
            } else {
                random() * 3.0
            };
            let ray = Ray { origin, dir, reach };
            let every = if capsules
                .iter()
                .any(|c| !c.contains(origin) && c.entry(origin, dir).is_some_and(|t| t < reach))
            {
                0.0
            } else {
                1.0
            };
            assert_eq!(blockers.light_through(&ray, |_| false), every, "{ray:?}");
        }
    }

    #[test]
    fn a_ray_along_an_axis_still_finds_what_is_ahead() {
        // Zero in two of the direction's parts, so dividing by them gives infinities.
        let blockers = Blockers::new([capsule(
            DVec3::new(0.0, 0.0, 5.0),
            DVec3::new(0.0, 0.0, 6.0),
            0.5,
            0,
        )]);
        let ray = |x: f64| Ray {
            origin: DVec3::new(x, 0.0, 0.0),
            dir: DVec3::Z,
            reach: f64::INFINITY,
        };
        assert_eq!(blockers.light_through(&ray(0.2), |_| false), 0.0);
        assert_eq!(blockers.light_through(&ray(0.7), |_| false), 1.0);
        assert_eq!(blockers.light_through(&ray(-0.7), |_| false), 1.0);
    }
}
