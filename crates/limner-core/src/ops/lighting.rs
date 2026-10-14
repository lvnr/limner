//! Light worked out in the core, as the lit shaders show it, for painting it into
//! strokes for good.
//!
//! A stroke carries one colour per point, so what is baked is the light a point gets
//! on the whole: a flat stroke takes the light of the side that faces it, and a tube
//! the mean of the light around it.
//!
//! A light that throws shadows lights a point only if no stroke stands between them.
//! The point is tested from the side of its own stroke that faces the light, so a
//! stroke never shades itself unless it curls round into the light's way.

use super::occlusion::{Blockers, Capsule, Ray, SegmentKey};
use super::OpError;
use crate::eval::{Content, RenderFrame};
use crate::model::{
    Document, Drawing, DrawingId, LayerId, LayerKind, LightKind, Point, Shading, Stroke,
};
use glam::{DAffine3, DVec3};
use std::collections::HashMap;

/// How far past the edge of its shadow a light still reaches a surface: the wrap of
/// the lit shaders, which keeps thin strokes from falling into black.
pub const LIGHT_WRAP: f64 = 0.5;
/// A point or spot light of energy 1 lights a surface facing it fully at this share
/// of its range. Nearer is brighter, and at the range it has faded to nothing.
pub const FULL_AT: f64 = 0.25;
/// Directions around a tube the light is sampled from.
const AROUND: usize = 12;
/// How far past its own surface, as a share of its width, a point looks for shadows
/// from: just enough to clear rounding errors.
const LIFT: f64 = 0.01;
/// Along its own stroke, a point ignores segments this many widths away on either side:
/// the stroke's own body, whose small swellings would otherwise shade it in light that
/// rakes along it.
const OWN_REACH: f64 = 2.0;

/// A light layer at one moment, in world space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SceneLight {
    pub kind: LightKind,
    pub position: DVec3,
    /// The way it shines: the layer's -Z axis.
    pub forward: DVec3,
    /// Linear RGB times energy times the layer's opacity.
    pub color: DVec3,
    /// Reach in world units.
    pub range: f64,
    /// Cosine of the half-angle of a spot's cone.
    pub cone: f64,
    /// How sharply a spot fades toward the edge of its cone.
    pub cone_exponent: f64,
    /// Strokes in its way keep its light off what is behind them.
    pub shadows: bool,
}

/// How strongly a light of this softness fades toward the edge of its cone: the
/// exponent of the fall-off. A blend of 1 fades from the middle, 0 only at the edge.
pub fn cone_exponent(blend: f32) -> f32 {
    (1.0 / blend.max(1e-3)).clamp(1.0, 64.0)
}

/// How much of a point or spot light's energy arrives `distance` away, for a light
/// that reaches `range`. Does not depend on the units both are measured in.
pub fn reach_falloff(distance: f64, range: f64) -> f64 {
    if range.is_nan() || range <= 0.0 || distance >= range {
        return 0.0;
    }
    let part = distance / range;
    let window = (1.0 - part.powi(4)).max(0.0).powi(2);
    window * FULL_AT / part.max(1e-4)
}

/// The share of a light a surface shows when the light arrives at an angle whose
/// cosine with the surface's normal is `facing`.
pub fn wrapped(facing: f64) -> f64 {
    ((facing + LIGHT_WRAP) / ((1.0 + LIGHT_WRAP) * (1.0 + LIGHT_WRAP))).max(0.0)
}

/// The lights that are on in an evaluated frame.
pub fn scene_lights(doc: &Document, frame: &RenderFrame) -> Vec<SceneLight> {
    let mut lights = Vec::new();
    for state in frame.layers.iter().filter(|s| s.visible && s.opacity > 0.0) {
        let Some(LayerKind::Light(light)) = doc.layer(state.id).map(|l| &l.kind) else {
            continue;
        };
        if light.energy.is_nan() || light.energy <= 0.0 {
            continue;
        }
        let scale = state.world.matrix3.determinant().abs().cbrt();
        let Some(forward) = state.world.transform_vector3(DVec3::NEG_Z).try_normalize() else {
            continue;
        };
        lights.push(SceneLight {
            kind: light.kind,
            position: state.world.translation,
            forward,
            color: DVec3::from_array(light.color.map(f64::from))
                * f64::from(light.energy * state.opacity),
            range: f64::from(light.range) * scale,
            cone: f64::from(light.spot_angle.clamp(0.0, 89.9))
                .to_radians()
                .cos(),
            cone_exponent: f64::from(cone_exponent(light.spot_blend)),
            shadows: light.shadows,
        });
    }
    lights
}

/// What kind of surface a point of a stroke is, for the light falling on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Surface {
    /// Flat and seen from both sides: lit by whichever side faces the light.
    Flat { normal: DVec3 },
    /// Round, running along `tangent`: lit by the mean of the light around it.
    Round { tangent: DVec3 },
}

/// The light arriving on a surface at `position`, as a factor on its own colour on
/// each channel: the ambient light plus what every light adds.
pub fn light_on(lights: &[SceneLight], ambient: DVec3, position: DVec3, surface: Surface) -> DVec3 {
    light_on_through(lights, ambient, position, surface, |_, _| 1.0)
}

/// [`light_on`] with each light's contribution scaled by `through`: the share of it,
/// 0 to 1, that gets to the surface past whatever is in the way. It is given the light
/// and the direction toward it, and asked only about lights that reach the surface.
pub fn light_on_through(
    lights: &[SceneLight],
    ambient: DVec3,
    position: DVec3,
    surface: Surface,
    mut through: impl FnMut(&SceneLight, DVec3) -> f64,
) -> DVec3 {
    let mut total = ambient;
    for light in lights {
        let (toward, strength) = match light.kind {
            LightKind::Directional => (-light.forward, 1.0),
            LightKind::Point | LightKind::Spot => {
                let to_light = light.position - position;
                let distance = to_light.length();
                let mut strength = reach_falloff(distance, light.range);
                let toward = to_light.try_normalize().unwrap_or(-light.forward);
                if light.kind == LightKind::Spot && strength > 0.0 {
                    let along = (-toward).dot(light.forward).max(light.cone);
                    let rim = ((1.0 - along) / (1.0 - light.cone).max(1e-9)).max(1e-4);
                    strength *= 1.0 - rim.powf(light.cone_exponent);
                }
                (toward, strength)
            }
        };
        if strength <= 0.0 {
            continue;
        }
        let shown = match surface {
            Surface::Flat { normal } => wrapped(normal.dot(toward).abs()),
            Surface::Round { tangent } => {
                // Around a tube the normal makes every angle with the light that its
                // lean along the tube allows.
                let across = (1.0 - toward.dot(tangent).powi(2)).max(0.0).sqrt();
                (0..AROUND)
                    .map(|i| {
                        let angle = std::f64::consts::TAU * (i as f64 + 0.5) / AROUND as f64;
                        wrapped(across * angle.cos())
                    })
                    .sum::<f64>()
                    / AROUND as f64
            }
        };
        if shown <= 0.0 {
            continue;
        }
        let share = through(light, toward);
        total += light.color * (strength * shown * share);
    }
    total
}

/// How much a layer's world placement scales lengths.
fn length_scale(world: &DAffine3) -> f64 {
    world.matrix3.determinant().abs().cbrt()
}

/// The segments of a stroke as capsules in world space, from drawing `source`.
fn capsules<'a>(
    stroke: &'a Stroke,
    world: &'a DAffine3,
    opacity: f64,
    source: u32,
    index: u32,
) -> impl Iterator<Item = Capsule> + 'a {
    let points = stroke.points();
    let scale = length_scale(world);
    let place = move |p: &Point| world.transform_point3(p.position().as_dvec3());
    // A stroke of one point is a ball.
    let pairs = points
        .windows(2)
        .map(|w| (w[0], w[1]))
        .chain((points.len() == 1).then(|| (points[0], points[0])));
    pairs.enumerate().map(move |(i, (a, b))| Capsule {
        a: place(&a),
        b: place(&b),
        radius: f64::from(a.width.max(b.width)) * scale,
        opacity: f64::from(a.opacity.max(b.opacity)).clamp(0.0, 1.0) * opacity,
        key: SegmentKey {
            source,
            stroke: index,
            index: i as u32,
        },
    })
}

fn drawing_capsules<'a>(
    drawing: &'a Drawing,
    world: &'a DAffine3,
    opacity: f64,
    source: u32,
) -> impl Iterator<Item = Capsule> + 'a {
    drawing
        .strokes()
        .enumerate()
        .flat_map(move |(i, stroke)| capsules(stroke, world, opacity, source, i as u32))
}

/// What a drawing's own blockers call it, apart from the scene's drawings.
const OWN_DRAWING: u32 = u32::MAX;

/// Every stroke shown at `frame`, as blockers, and for each paint layer shown the
/// number its drawing goes by among them and which drawing that is. Pictures throw no
/// shadow in the live view either; models are left out, as the core does not hold
/// their shapes.
fn scene_blockers(
    doc: &Document,
    frame: &RenderFrame,
) -> (Blockers, HashMap<LayerId, (u32, DrawingId)>) {
    let mut shown = HashMap::new();
    let mut all = Vec::new();
    for state in frame.layers.iter().filter(|s| s.visible && s.opacity > 0.0) {
        let Content::Paint { drawing: id, .. } = state.content else {
            continue;
        };
        let Some(drawing) = doc.drawing(id) else {
            continue;
        };
        let source = shown.len() as u32;
        shown.insert(state.id, (source, id));
        all.extend(drawing_capsules(
            drawing,
            &state.world,
            f64::from(state.opacity.min(1.0)),
            source,
        ));
    }
    (Blockers::new(all), shown)
}

/// The strokes that throw shadows on the strokes of one drawing being baked.
struct Shade<'a> {
    /// Every stroke shown in the scene at the baked moment.
    scene: &'a Blockers,
    /// The drawing's own strokes, when the scene does not show them.
    own: Option<Blockers>,
    /// A drawing of the scene to leave out: the one the baked layer shows instead.
    instead: Option<u32>,
    /// What the drawing is called in whichever of the two holds its strokes.
    source: u32,
}

impl Shade<'_> {
    /// The share of light that gets along `ray` to a point of the drawing's stroke
    /// `stroke`, ignoring that stroke's segments in `near` (its own body around the point).
    fn light_through(&self, ray: &Ray, stroke: u32, near: std::ops::Range<u32>) -> f64 {
        let own = |k: &SegmentKey| k.source == self.source && k.stroke == stroke;
        // A point buried in another stroke is not shaded by it: that stroke's own
        // surface is what shows there.
        let mut inside = Vec::new();
        self.scene.strokes_containing(ray.origin, &mut inside);
        if let Some(blockers) = &self.own {
            blockers.strokes_containing(ray.origin, &mut inside);
        }
        let skip = |k: &SegmentKey| {
            if own(k) {
                return near.contains(&k.index);
            }
            Some(k.source) == self.instead || inside.iter().any(|i| i.same_stroke(k))
        };
        let mut through = self.scene.light_through(ray, skip);
        if let Some(blockers) = &self.own {
            if through > 0.0 {
                through *= blockers.light_through(ray, skip);
            }
        }
        through
    }
}

/// The stroke with the light on it painted into its colours. `index` is its place in
/// its drawing, for `shade`.
fn lit_stroke(
    stroke: &Stroke,
    index: u32,
    world: &DAffine3,
    lights: &[SceneLight],
    ambient: DVec3,
    shade: Option<&Shade>,
) -> Stroke {
    let points = stroke.points();
    let place = |p: &Point| world.transform_point3(p.position().as_dvec3());
    let flat = stroke.brush.is_flat();
    let scale = length_scale(world);
    let placed: Vec<DVec3> = points.iter().map(place).collect();
    // Distance along the stroke to each point.
    let along: Vec<f64> = placed
        .iter()
        .scan((0.0, placed.first().copied()), |(total, last), p| {
            *total += last.map_or(0.0, |l| l.distance(*p));
            *last = Some(*p);
            Some(*total)
        })
        .collect();
    let lit: Vec<Point> = points
        .iter()
        .enumerate()
        .map(|(i, point)| {
            let position = placed[i];
            let before = placed[i.saturating_sub(1)];
            let after = placed[(i + 1).min(points.len() - 1)];
            let tangent = (after - before).try_normalize().unwrap_or(DVec3::X);
            let surface = if flat {
                let normal = world
                    .transform_vector3(DVec3::from_array(point.normal.map(f64::from)))
                    .try_normalize()
                    .unwrap_or(DVec3::Z);
                Surface::Flat { normal }
            } else {
                Surface::Round { tangent }
            };
            let width = f64::from(point.width) * scale;
            let near = {
                let reach = OWN_REACH * width;
                let first = along.partition_point(|s| *s < along[i] - reach);
                let end = along.partition_point(|s| *s <= along[i] + reach);
                first.saturating_sub(1) as u32..end as u32
            };
            let through = |light: &SceneLight, toward: DVec3| {
                let Some(shade) = shade.filter(|_| light.shadows) else {
                    return 1.0;
                };
                // Start on the side of the stroke that faces the light, just outside it.
                // Shining straight along the stroke, any side will do; what is left
                // of the direction then is only rounding error.
                let across = toward - tangent * toward.dot(tangent);
                let side = if across.length_squared() > 1e-12 {
                    across.normalize()
                } else {
                    tangent.any_orthonormal_vector()
                };
                let origin =
                    position + side * (width * (1.0 + LIFT) + 1e-9 * position.abs().max_element());
                let ray = match light.kind {
                    LightKind::Directional => Ray {
                        origin,
                        dir: toward,
                        reach: f64::INFINITY,
                    },
                    LightKind::Point | LightKind::Spot => {
                        let to_light = light.position - origin;
                        Ray {
                            origin,
                            dir: to_light.try_normalize().unwrap_or(toward),
                            reach: to_light.length(),
                        }
                    }
                };
                shade.light_through(&ray, index, near.clone())
            };
            let light = light_on_through(lights, ambient, position, surface, through);
            Point {
                color: [
                    (f64::from(point.color[0]) * light.x) as f32,
                    (f64::from(point.color[1]) * light.y) as f32,
                    (f64::from(point.color[2]) * light.z) as f32,
                ],
                ..*point
            }
        })
        .collect();
    Stroke::new(stroke.id, stroke.brush, stroke.directional, lit)
}

/// Paints the light that falls on the lit paint layers in and under `id` into their
/// strokes, on every frame, as the lights stand in `frame`, and makes those layers
/// unlit: they then look the same without the lights. Returns how many layers and
/// points were painted.
pub fn bake_lighting(
    doc: &mut Document,
    frame: &RenderFrame,
    id: LayerId,
) -> Result<(usize, usize), OpError> {
    if doc.layer(id).is_none() {
        return Err(OpError::NoSuchLayer(id));
    }
    let lights = scene_lights(doc, frame);
    let ambient = DVec3::from_array(doc.settings.ambient_light.map(f64::from));
    let layers: Vec<LayerId> = doc
        .subtree(id)
        .into_iter()
        .filter(|l| {
            doc.layer(*l)
                .and_then(|layer| layer.as_paint())
                .is_some_and(|paint| paint.shading == Shading::Lit)
        })
        .collect();
    if layers.is_empty() {
        return Err(OpError::Invalid(
            "there is no lit paint layer here to bake light into".into(),
        ));
    }
    if let Some(locked) = layers
        .iter()
        .filter_map(|l| doc.layer(*l))
        .find(|l| l.locked)
    {
        return Err(OpError::Invalid(format!(
            "layer \"{}\" is locked",
            locked.name
        )));
    }
    // Built once, before anything is painted; baking changes colours, not shapes.
    let shadows = lights.iter().any(|l| l.shadows);
    let (scene, shown) = if shadows {
        scene_blockers(doc, frame)
    } else {
        (Blockers::default(), HashMap::new())
    };
    let mut painted = 0;
    for layer in &layers {
        let Some(state) = frame.get(*layer) else {
            continue;
        };
        let world = state.world;
        let drawings = doc
            .layer(*layer)
            .and_then(|l| l.as_paint())
            .map(|p| p.drawings.clone())
            .unwrap_or_default();
        for drawing_id in drawings {
            let Some(drawing) = doc.drawing(drawing_id) else {
                continue;
            };
            painted += drawing.point_count();
            let shade = shadows.then(|| match shown.get(layer) {
                Some(&(source, showing)) if showing == drawing_id => Shade {
                    scene: &scene,
                    own: None,
                    instead: None,
                    source,
                },
                // A frame not showing at this moment is shaded by its own strokes
                // and the rest of the scene, not by the frame the layer shows.
                other => Shade {
                    scene: &scene,
                    own: Some(Blockers::new(drawing_capsules(
                        drawing,
                        &world,
                        f64::from(state.opacity.clamp(0.0, 1.0)),
                        OWN_DRAWING,
                    ))),
                    instead: other.map(|&(source, _)| source),
                    source: OWN_DRAWING,
                },
            });
            let lit = Drawing::from_strokes(drawing.strokes().enumerate().map(|(i, stroke)| {
                lit_stroke(stroke, i as u32, &world, &lights, ambient, shade.as_ref())
            }));
            if let Some(target) = doc.drawing_mut(drawing_id) {
                *target = lit;
            }
        }
        if let Some(paint) = doc.layer_mut(*layer).and_then(|l| l.as_paint_mut()) {
            paint.shading = Shading::Unlit;
        }
    }
    Ok((layers.len(), painted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::{evaluate, Clock};
    use crate::math::Xform;
    use crate::model::{Brush, Light};
    use crate::ops::layers::{add_layer, NewLayerKind};
    use glam::DQuat;

    fn light(kind: LightKind) -> SceneLight {
        SceneLight {
            kind,
            position: DVec3::new(0.0, 2.0, 0.0),
            forward: DVec3::NEG_Y,
            color: DVec3::ONE,
            range: 4.0,
            cone: 30f64.to_radians().cos(),
            cone_exponent: 4.0,
            shadows: false,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn a_flat_surface_takes_the_light_of_the_side_that_faces_it() {
        let sun = [light(LightKind::Directional)];
        let on =
            |normal: DVec3| light_on(&sun, DVec3::ZERO, DVec3::ZERO, Surface::Flat { normal }).x;
        // Facing the light it shows the most a wrapped surface can; from behind, the same.
        assert!(close(on(DVec3::Y), 1.0 / 1.5));
        assert!(close(on(DVec3::NEG_Y), 1.0 / 1.5));
        // Edge on, the wrap still lets some round.
        assert!(close(on(DVec3::X), 0.5 / 2.25));
        // Ambient light is there whatever the lights do.
        let dim = light_on(
            &[],
            DVec3::splat(0.35),
            DVec3::ZERO,
            Surface::Flat { normal: DVec3::Y },
        );
        assert_eq!(dim, DVec3::splat(0.35));
    }

    #[test]
    fn a_tube_takes_the_mean_of_the_light_around_it() {
        let sun = [light(LightKind::Directional)];
        let on =
            |tangent: DVec3| light_on(&sun, DVec3::ZERO, DVec3::ZERO, Surface::Round { tangent }).x;
        let across = on(DVec3::X);
        let along = on(DVec3::Y);
        // Lying across the light one side is bright and the other dark; pointing at
        // the light every side is edge on.
        assert!(across > 0.2 && across < 1.0 / 1.5, "{across}");
        assert!(close(along, 0.5 / 2.25), "{along}");
        assert!(across > along);
    }

    #[test]
    fn a_point_light_fades_with_distance_whatever_the_units() {
        assert!(close(
            reach_falloff(1.0, 4.0),
            (1.0 - 0.25f64.powi(4)).powi(2)
        ));
        assert!(reach_falloff(0.5, 4.0) > 1.9, "nearer is brighter");
        assert!(reach_falloff(3.0, 4.0) < 0.2);
        assert_eq!(reach_falloff(4.0, 4.0), 0.0);
        assert_eq!(reach_falloff(9.0, 4.0), 0.0);
        // The same scene measured in millimetres.
        assert!(close(
            reach_falloff(1000.0, 4000.0),
            reach_falloff(1.0, 4.0)
        ));

        let lamp = [light(LightKind::Point)];
        let below = light_on(
            &lamp,
            DVec3::ZERO,
            DVec3::new(0.0, 1.0, 0.0),
            Surface::Flat { normal: DVec3::Y },
        );
        assert!(close(below.x, reach_falloff(1.0, 4.0) / 1.5));
        let far = light_on(
            &lamp,
            DVec3::ZERO,
            DVec3::new(0.0, -3.0, 0.0),
            Surface::Flat { normal: DVec3::Y },
        );
        assert_eq!(far, DVec3::ZERO);
    }

    #[test]
    fn a_spot_lights_only_what_is_in_its_cone() {
        let spot = [light(LightKind::Spot)];
        let at = |x: f64| {
            light_on(
                &spot,
                DVec3::ZERO,
                DVec3::new(x, 1.0, 0.0),
                Surface::Flat { normal: DVec3::Y },
            )
            .x
        };
        assert!(at(0.0) > 0.5, "straight below it");
        assert!(at(0.3) < at(0.0) && at(0.3) > 0.0, "fading toward the edge");
        // Thirty degrees off the axis is a metre down and 0.577 to the side.
        assert_eq!(at(0.6), 0.0);
        assert_eq!(at(3.0), 0.0);
    }

    #[test]
    fn baking_paints_the_light_into_the_strokes_and_makes_the_layer_unlit() {
        let mut doc = Document::new();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        let root = doc.root();
        let lamp = add_layer(&mut doc, NewLayerKind::Light, root, usize::MAX, None, 0).unwrap();
        {
            let layer = doc.layer_mut(lamp).unwrap();
            // A sun shining straight down.
            layer.transform =
                Xform::from_rotation(DQuat::from_rotation_x(-std::f64::consts::FRAC_PI_2));
            layer.kind = LayerKind::Light(Light {
                kind: LightKind::Directional,
                color: [1.0, 0.5, 0.0],
                energy: 1.5,
                ..Light::default()
            });
        }
        doc.settings.ambient_light = [0.1, 0.1, 0.1];
        let point = |x: f32, normal: [f32; 3]| Point {
            pos: [x, 0.0, 0.0],
            normal,
            dir: [0.0, 0.0, -1.0],
            color: [0.8, 0.8, 0.8],
            opacity: 1.0,
            width: 0.01,
        };
        let drawing = doc.layer(paint).unwrap().as_paint().unwrap().drawings[0];
        *doc.drawing_mut(drawing).unwrap() = Drawing::from_strokes([
            Stroke::new(
                0,
                Brush::Ribbon,
                false,
                vec![point(0.0, [0.0, 1.0, 0.0]), point(1.0, [0.0, 1.0, 0.0])],
            ),
            Stroke::new(
                1,
                Brush::Ribbon,
                false,
                vec![point(0.0, [1.0, 0.0, 0.0]), point(1.0, [1.0, 0.0, 0.0])],
            ),
        ]);
        let frame = evaluate(&doc, Clock::at(0));

        let refused = bake_lighting(&mut doc.clone(), &frame, paint);
        assert!(refused.is_err(), "an unlit layer has no light to bake");
        doc.layer_mut(paint)
            .unwrap()
            .as_paint_mut()
            .unwrap()
            .shading = Shading::Lit;
        assert_eq!(bake_lighting(&mut doc, &frame, root).unwrap(), (1, 4));

        let baked = doc.drawing(drawing).unwrap();
        let colors: Vec<[f32; 3]> = baked.strokes().map(|s| s.points()[0].color).collect();
        // Facing up: ambient plus the sun at full; its red at 1.5, green at 0.75, no blue.
        let facing = 1.0 / 1.5;
        let expected = [
            0.8 * (0.1 + 1.5 * facing),
            0.8 * (0.1 + 0.75 * facing),
            0.8 * 0.1,
        ];
        for (got, want) in colors[0].iter().zip(expected) {
            assert!((f64::from(*got) - want).abs() < 1e-6, "{colors:?}");
        }
        assert!(
            colors[1][0] < colors[0][0],
            "edge on to the sun it is dimmer"
        );
        assert_eq!(
            doc.layer(paint).unwrap().as_paint().unwrap().shading,
            Shading::Unlit,
            "what was light is now paint"
        );
        doc.validate().unwrap();
    }

    /// A stroke of `brush` from `from` to `to` through `count` points, all of half-width `width`.
    fn line(id: u32, brush: Brush, from: DVec3, to: DVec3, width: f32, count: usize) -> Stroke {
        let points = (0..count)
            .map(|i| {
                let at = from
                    .lerp(to, i as f64 / (count - 1).max(1) as f64)
                    .as_vec3();
                Point {
                    pos: at.to_array(),
                    normal: [0.0, 1.0, 0.0],
                    dir: [0.0, 0.0, -1.0],
                    color: [0.8, 0.8, 0.8],
                    opacity: 1.0,
                    width,
                }
            })
            .collect();
        Stroke::new(id, brush, false, points)
    }

    fn paint_layer(
        doc: &mut Document,
        name: &str,
        strokes: Vec<Stroke>,
        shading: Shading,
    ) -> LayerId {
        let root = doc.root();
        let layer = add_layer(doc, NewLayerKind::Paint, root, usize::MAX, Some(name), 0).unwrap();
        let paint = doc.layer_mut(layer).unwrap().as_paint_mut().unwrap();
        paint.shading = shading;
        let drawing = paint.drawings[0];
        *doc.drawing_mut(drawing).unwrap() = Drawing::from_strokes(strokes);
        layer
    }

    fn light_layer(doc: &mut Document, light: Light, place: Xform) -> LayerId {
        let root = doc.root();
        let layer = add_layer(doc, NewLayerKind::Light, root, usize::MAX, None, 0).unwrap();
        let layer_ref = doc.layer_mut(layer).unwrap();
        layer_ref.transform = place;
        layer_ref.kind = LayerKind::Light(light);
        layer
    }

    /// A sun shining down, leaning `lean` radians toward +X.
    fn sun_place(lean: f64) -> Xform {
        Xform::from_rotation(
            DQuat::from_rotation_z(lean) * DQuat::from_rotation_x(-std::f64::consts::FRAC_PI_2),
        )
    }

    /// The colours of every point of a layer's first drawing, stroke by stroke.
    fn colors(doc: &Document, layer: LayerId) -> Vec<Vec<[f32; 3]>> {
        let drawing = doc.layer(layer).unwrap().as_paint().unwrap().drawings[0];
        doc.drawing(drawing)
            .unwrap()
            .strokes()
            .map(|s| s.points().iter().map(|p| p.color).collect())
            .collect()
    }

    fn baked(doc: &Document, layer: LayerId) -> Vec<Vec<[f32; 3]>> {
        let mut doc = doc.clone();
        let frame = evaluate(&doc, Clock::at(0));
        bake_lighting(&mut doc, &frame, layer).unwrap();
        colors(&doc, layer)
    }

    fn without_shadows(doc: &Document) -> Document {
        let mut doc = doc.clone();
        for id in doc.subtree(doc.root()) {
            if let Some(LayerKind::Light(light)) = doc.layer_mut(id).map(|l| &mut l.kind) {
                light.shadows = false;
            }
        }
        doc
    }

    /// A short tube at the origin, a wide stroke a metre above it on another layer, and
    /// a red lamp that throws shadows and a green one that does not, both above that.
    fn shaded_scene() -> (Document, LayerId, LayerId) {
        let mut doc = Document::new();
        doc.settings.ambient_light = [0.1, 0.1, 0.1];
        let target = paint_layer(
            &mut doc,
            "Target",
            vec![
                line(
                    0,
                    Brush::Cylinder,
                    DVec3::new(-0.1, 0.0, 0.0),
                    DVec3::new(0.1, 0.0, 0.0),
                    0.01,
                    3,
                ),
                // Out in the open, well to the side of the blocker.
                line(
                    1,
                    Brush::Cylinder,
                    DVec3::new(-0.1, 0.0, 3.0),
                    DVec3::new(0.1, 0.0, 3.0),
                    0.01,
                    3,
                ),
            ],
            Shading::Lit,
        );
        let blocker = paint_layer(
            &mut doc,
            "Blocker",
            vec![line(
                0,
                Brush::Ribbon,
                DVec3::new(-1.0, 1.0, 0.0),
                DVec3::new(1.0, 1.0, 0.0),
                0.5,
                9,
            )],
            Shading::Unlit,
        );
        for (color, shadows) in [([1.0, 0.0, 0.0], true), ([0.0, 1.0, 0.0], false)] {
            light_layer(
                &mut doc,
                Light {
                    kind: LightKind::Point,
                    color,
                    range: 6.0,
                    shadows,
                    ..Light::default()
                },
                Xform::from_translation(DVec3::new(0.0, 2.0, 0.0)),
            );
        }
        (doc, target, blocker)
    }

    #[test]
    fn a_point_behind_a_stroke_is_dark_to_a_light_that_throws_shadows() {
        let (doc, target, _) = shaded_scene();
        let shaded = baked(&doc, target);
        let open = baked(&without_shadows(&doc), target);
        // Under the blocker the red lamp is gone and only the ambient light is left.
        for (got, unshaded) in shaded[0].iter().zip(&open[0]) {
            assert!((got[0] - 0.8 * 0.1).abs() < 1e-6, "{shaded:?}");
            assert!(unshaded[0] > 0.2, "{open:?}");
            // The green lamp throws no shadow, so its light is all there.
            assert_eq!(got[1], unshaded[1]);
            assert!(got[1] > 0.2);
        }
    }

    #[test]
    fn a_point_in_the_open_is_lit_as_before() {
        let (doc, target, _) = shaded_scene();
        let shaded = baked(&doc, target);
        let open = baked(&without_shadows(&doc), target);
        assert_eq!(shaded[1], open[1]);
        assert!(shaded[1][0][0] > 0.1, "{shaded:?}");
    }

    #[test]
    fn hidden_strokes_and_strokes_beyond_the_lamp_throw_no_shadow() {
        let (doc, target, blocker) = shaded_scene();
        let open = baked(&without_shadows(&doc), target);
        let mut hidden = doc.clone();
        hidden.layer_mut(blocker).unwrap().visible = false;
        assert_eq!(baked(&hidden, target), open);

        // Lamps brought down between the point and the stroke.
        let mut below = doc.clone();
        let lamps: Vec<LayerId> = below
            .subtree(below.root())
            .into_iter()
            .filter(|id| matches!(below.layer(*id).unwrap().kind, LayerKind::Light(_)))
            .collect();
        for lamp in lamps {
            below.layer_mut(lamp).unwrap().transform =
                Xform::from_translation(DVec3::new(0.0, 0.3, 0.0));
        }
        assert_eq!(
            baked(&below, target),
            baked(&without_shadows(&below), target)
        );
    }

    #[test]
    fn a_lone_stroke_does_not_shadow_itself() {
        let wobbly = Stroke::new(
            2,
            Brush::Cylinder,
            false,
            (0..40)
                .map(|i| Point {
                    pos: [i as f32 * 0.005, 0.0, 0.0],
                    normal: [0.0, 1.0, 0.0],
                    dir: [0.0, 0.0, -1.0],
                    color: [0.8, 0.8, 0.8],
                    opacity: 1.0,
                    // Pressure that wavers by a tenth from point to point.
                    width: 0.01 * (1.0 + 0.1 * (i as f32 * 1.7).sin()),
                })
                .collect(),
        );
        // Suns from every side, grazing ones included, at least ten degrees off the
        // strokes' line.
        let mut turns = Vec::new();
        for pitch in [-80.0f64, -45.0, -15.0, 0.0, 15.0, 45.0, 80.0] {
            for yaw in (0..12).map(|i| f64::from(i) * 30.0 + 10.0) {
                turns.push(
                    DQuat::from_rotation_y(yaw.to_radians())
                        * DQuat::from_rotation_x(pitch.to_radians()),
                );
            }
        }
        // Straight along the line, which an even stroke also survives.
        let along = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
        let strokes = [
            (
                line(
                    0,
                    Brush::Cylinder,
                    DVec3::ZERO,
                    DVec3::new(0.3, 0.0, 0.0),
                    0.02,
                    30,
                ),
                true,
            ),
            (
                line(
                    1,
                    Brush::Ribbon,
                    DVec3::ZERO,
                    DVec3::new(0.3, 0.0, 0.0),
                    0.05,
                    30,
                ),
                true,
            ),
            (wobbly, false),
        ];
        for (stroke, even) in strokes {
            let mut doc = Document::new();
            let layer = paint_layer(&mut doc, "Lone", vec![stroke], Shading::Lit);
            let sun = light_layer(
                &mut doc,
                Light {
                    kind: LightKind::Directional,
                    shadows: true,
                    ..Light::default()
                },
                Xform::IDENTITY,
            );
            for turn in turns.iter().chain(even.then_some(&along)) {
                doc.layer_mut(sun).unwrap().transform = Xform::from_rotation(*turn);
                assert_eq!(
                    baked(&doc, layer),
                    baked(&without_shadows(&doc), layer),
                    "{turn:?}"
                );
            }
        }
    }

    #[test]
    fn a_sun_throws_shadows_along_the_way_it_shines() {
        let (mut doc, target, _) = shaded_scene();
        let lamps: Vec<LayerId> = doc
            .subtree(doc.root())
            .into_iter()
            .filter(|id| matches!(doc.layer(*id).unwrap().kind, LayerKind::Light(_)))
            .collect();
        doc.layer_mut(lamps[1]).unwrap().visible = false;
        let sun = lamps[0];
        doc.layer_mut(sun).unwrap().kind = LayerKind::Light(Light {
            kind: LightKind::Directional,
            color: [1.0, 0.0, 0.0],
            shadows: true,
            ..Light::default()
        });
        // Straight down: the blocker is overhead. A sun has no place, so how far away
        // its layer sits does not matter.
        doc.layer_mut(sun).unwrap().transform = sun_place(0.0);
        let overhead = baked(&doc, target);
        assert!((overhead[0][1][0] - 0.08).abs() < 1e-6, "{overhead:?}");
        assert!(overhead[1][1][0] > 0.2, "the far tube is in the open");
        // Low from the side, the light passes beside the blocker's end.
        doc.layer_mut(sun).unwrap().transform = sun_place(80f64.to_radians());
        let low = baked(&doc, target);
        assert_eq!(low[0], baked(&without_shadows(&doc), target)[0]);
        assert!(low[0][1][0] > 0.09, "{low:?}");
        // Leaning a little the other way the blocker is still between the tube and the sun.
        doc.layer_mut(sun).unwrap().transform = sun_place(-20f64.to_radians());
        let slight = baked(&doc, target);
        assert!((slight[0][1][0] - 0.08).abs() < 1e-6, "{slight:?}");
    }

    #[test]
    fn each_frame_is_shaded_by_its_own_strokes() {
        use crate::ops::frames::{add_frame, NewFrame};
        let mut doc = Document::new();
        doc.settings.ambient_light = [0.1, 0.1, 0.1];
        let blocker_at = |x: f64| {
            line(
                9,
                Brush::Ribbon,
                DVec3::new(x - 1.0, 1.0, 0.0),
                DVec3::new(x + 1.0, 1.0, 0.0),
                0.5,
                5,
            )
        };
        let target_at = |id: u32, x: f64| {
            line(
                id,
                Brush::Cylinder,
                DVec3::new(x - 0.1, 0.0, 0.0),
                DVec3::new(x + 0.1, 0.0, 0.0),
                0.01,
                3,
            )
        };
        // The first frame, which is showing, has only a blocker over x = 0.
        let layer = paint_layer(&mut doc, "Flipbook", vec![blocker_at(0.0)], Shading::Lit);
        let second = add_frame(&mut doc, layer, 0, NewFrame::Blank).unwrap();
        let paint = doc.layer(layer).unwrap().as_paint().unwrap();
        let drawing = paint.drawing_at_frame(second).unwrap();
        // The second has a tube at x = 0 and one at x = 5 with its own blocker over it.
        *doc.drawing_mut(drawing).unwrap() =
            Drawing::from_strokes([target_at(0, 0.0), target_at(1, 5.0), blocker_at(5.0)]);
        light_layer(
            &mut doc,
            Light {
                kind: LightKind::Directional,
                shadows: true,
                ..Light::default()
            },
            sun_place(0.0),
        );
        let frame = evaluate(&doc, Clock::at(0));
        bake_lighting(&mut doc, &frame, layer).unwrap();
        let strokes: Vec<[f32; 3]> = doc
            .drawing(drawing)
            .unwrap()
            .strokes()
            .map(|s| s.points()[1].color)
            .collect();
        assert!(
            strokes[0][0] > 0.2,
            "not shaded by another frame: {strokes:?}"
        );
        assert!(
            (strokes[1][0] - 0.08).abs() < 1e-6,
            "shaded by its own: {strokes:?}"
        );
    }
}
