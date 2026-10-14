//! Light worked out in the core, as the lit shaders show it, for painting it into
//! strokes for good.
//!
//! A stroke carries one colour per point, so what is baked is the light a point gets
//! on the whole: a flat stroke takes the light of the side that faces it, and a tube
//! the mean of the light around it. Shadows are not worked out.

use super::OpError;
use crate::eval::RenderFrame;
use crate::model::{Document, Drawing, LayerId, LayerKind, LightKind, Point, Shading, Stroke};
use glam::{DAffine3, DVec3};

/// How far past the edge of its shadow a light still reaches a surface: the wrap of
/// the lit shaders, which keeps thin strokes from falling into black.
pub const LIGHT_WRAP: f64 = 0.5;
/// A point or spot light of energy 1 lights a surface facing it fully at this share
/// of its range. Nearer is brighter, and at the range it has faded to nothing.
pub const FULL_AT: f64 = 0.25;
/// Directions around a tube the light is sampled from.
const AROUND: usize = 12;

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
        total += light.color * (strength * shown);
    }
    total
}

/// The stroke with the light on it painted into its colours.
fn lit_stroke(stroke: &Stroke, world: &DAffine3, lights: &[SceneLight], ambient: DVec3) -> Stroke {
    let points = stroke.points();
    let place = |p: &Point| world.transform_point3(p.position().as_dvec3());
    let flat = stroke.brush.is_flat();
    let lit: Vec<Point> = points
        .iter()
        .enumerate()
        .map(|(i, point)| {
            let position = place(point);
            let surface = if flat {
                let normal = world
                    .transform_vector3(DVec3::from_array(point.normal.map(f64::from)))
                    .try_normalize()
                    .unwrap_or(DVec3::Z);
                Surface::Flat { normal }
            } else {
                let before = place(&points[i.saturating_sub(1)]);
                let after = place(&points[(i + 1).min(points.len() - 1)]);
                Surface::Round {
                    tangent: (after - before).try_normalize().unwrap_or(DVec3::X),
                }
            };
            let light = light_on(lights, ambient, position, surface);
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
    let mut painted = 0;
    for layer in &layers {
        let Some(world) = frame.get(*layer).map(|s| s.world) else {
            continue;
        };
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
            let lit = Drawing::from_strokes(
                drawing
                    .strokes()
                    .map(|stroke| lit_stroke(stroke, &world, &lights, ambient)),
            );
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
}
