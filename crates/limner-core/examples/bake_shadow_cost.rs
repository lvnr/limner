//! Prints how long "Bake lighting into paint" takes on a scene of about 100 000 points
//! with lights that throw shadows, and the same bake with shadows off.
//!
//!   cargo run -p limner-core --example bake_shadow_cost

use glam::{DQuat, DVec3};
use limner_core::eval::{evaluate, Clock};
use limner_core::math::Xform;
use limner_core::model::{
    Brush, Document, Drawing, LayerId, LayerKind, Light, LightKind, Point, Shading, Stroke,
};
use limner_core::ops::layers::{add_layer, NewLayerKind};
use limner_core::ops::lighting::bake_lighting;
use std::time::Instant;

/// A wavy stroke of `count` points starting at `start` and heading along `heading`.
fn wavy(id: u32, brush: Brush, start: DVec3, heading: DVec3, count: usize, width: f32) -> Stroke {
    let points = (0..count)
        .map(|i| {
            let t = i as f64 * 0.01;
            let wave = DVec3::new(0.0, (t * 9.0 + f64::from(id)).sin(), (t * 7.0).cos()) * 0.03;
            Point {
                pos: (start + heading * t + wave).as_vec3().to_array(),
                normal: [0.0, 1.0, 0.0],
                dir: [0.0, 0.0, -1.0],
                color: [0.6, 0.5, 0.4],
                opacity: 1.0,
                width,
            }
        })
        .collect();
    Stroke::new(id, brush, false, points)
}

fn paint_layer(doc: &mut Document, name: &str, strokes: Vec<Stroke>, shading: Shading) -> LayerId {
    let root = doc.root();
    let layer = add_layer(doc, NewLayerKind::Paint, root, usize::MAX, Some(name), 0).unwrap();
    let paint = doc.layer_mut(layer).unwrap().as_paint_mut().unwrap();
    paint.shading = shading;
    let drawing = paint.drawings[0];
    *doc.drawing_mut(drawing).unwrap() = Drawing::from_strokes(strokes);
    layer
}

fn light(doc: &mut Document, light: Light, place: Xform) {
    let root = doc.root();
    let layer = add_layer(doc, NewLayerKind::Light, root, usize::MAX, None, 0).unwrap();
    let layer = doc.layer_mut(layer).unwrap();
    layer.transform = place;
    layer.kind = LayerKind::Light(light);
}

fn main() {
    let mut doc = Document::new();
    let mut seed = 11u64;
    let mut random = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((seed >> 33) as f64) / f64::from(1u32 << 31)
    };
    // A bush of 1000 lit tubes of 100 points in a two-metre box.
    let bush: Vec<Stroke> = (0..1000)
        .map(|i| {
            let start = DVec3::new(random(), random(), random()) * 2.0 - 1.0;
            let heading = (DVec3::new(random(), random(), random()) - 0.5).normalize();
            wavy(i, Brush::Cylinder, start, heading, 100, 0.006)
        })
        .collect();
    let target = paint_layer(&mut doc, "Bush", bush, Shading::Lit);
    // A canopy of 50 wide ribbons of 60 points above it, on a layer of its own.
    let canopy: Vec<Stroke> = (0..50)
        .map(|i| {
            let start = DVec3::new(random() * 2.0 - 1.5, 1.5, random() * 2.0 - 1.0);
            wavy(i, Brush::Ribbon, start, DVec3::X, 60, 0.04)
        })
        .collect();
    paint_layer(&mut doc, "Canopy", canopy, Shading::Unlit);
    light(
        &mut doc,
        Light {
            kind: LightKind::Directional,
            shadows: true,
            ..Light::default()
        },
        Xform::from_rotation(
            DQuat::from_rotation_z(0.3) * DQuat::from_rotation_x(-std::f64::consts::FRAC_PI_2),
        ),
    );
    light(
        &mut doc,
        Light {
            kind: LightKind::Point,
            range: 6.0,
            shadows: true,
            ..Light::default()
        },
        Xform::from_translation(DVec3::new(1.5, 2.0, 0.5)),
    );

    let frame = evaluate(&doc, Clock::at(0));
    for shadows in [false, true] {
        let mut doc = doc.clone();
        for id in doc.subtree(doc.root()) {
            if let Some(LayerKind::Light(light)) = doc.layer_mut(id).map(|l| &mut l.kind) {
                light.shadows = shadows;
            }
        }
        let start = Instant::now();
        let (_, points) = bake_lighting(&mut doc, &frame, target).unwrap();
        let took = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "two lights, shadows {}: {points} points baked in {took:.0} ms",
            if shadows { "on " } else { "off" },
        );
    }
    println!("blockers: the bush's own 99 000 segments and the canopy's 2 950, all in one tree");
}
