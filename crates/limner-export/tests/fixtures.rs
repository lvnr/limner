//! Writes a synthetic scene that exercises every exported feature to `local/out/export/`,
//! for checking by hand in external tools (the Khronos validator, Blender, Godot).
//! Nothing here is private artwork. Run with:
//!
//! ```text
//! cargo test -p limner-export --test fixtures -- --ignored --nocapture
//! ```

mod support;

use limner_core::glam::{DQuat, DVec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Brush, Camera, Document, Interp, Key, LayerKind, Light, LightKind, Shading, Track,
};
use limner_export::{write_glb, write_obj, ExportOptions, GltfOptions};
use std::path::PathBuf;
use support::{frames, group, paint, sequence, straight, straight_colored, Glb, FRAME, RATE};

/// Lights, a camera, a mirrored chain, a keyed layer, clips, a looping frame-by-frame
/// layer, see-through strokes, a lit layer and a fading layer.
pub fn everything() -> Document {
    let mut doc = Document::empty();
    doc.settings.title = "Limner export fixture".to_owned();
    let root = doc.root();

    let rig = group(&mut doc, root, "Mirrored rig");
    doc.layer_mut(rig).unwrap().transform = Xform::new(
        DQuat::from_rotation_y(0.6),
        1.5,
        DVec3::new(-1.0, 0.0, 0.0),
        Flip::X,
    );
    let arm = paint(
        &mut doc,
        rig,
        "Arm",
        vec![
            straight(0, Brush::Cylinder, 6, [0.0, 0.5, 0.0]),
            straight_colored(1, Brush::Ribbon, 6, [0.0, 0.8, 0.0], [0.1, 0.6, 0.9], 0.5),
            straight_colored(2, Brush::Cube, 4, [0.0, 1.1, 0.0], [0.9, 0.9, 0.1], 1.0),
            straight_colored(3, Brush::Ellipse, 5, [0.0, 1.4, 0.0], [0.9, 0.1, 0.4], 0.3),
        ],
    );
    {
        let layer = doc.layer_mut(arm).unwrap();
        layer.pivot = Xform::from_translation(DVec3::new(0.0, 0.5, 0.0));
        layer.anim.transform = Track::from_keys([
            Key::new(0, Xform::IDENTITY, Interp::EaseInOut),
            Key::new(
                12 * FRAME,
                Xform::new(
                    DQuat::from_rotation_z(2.5),
                    1.5,
                    DVec3::new(0.0, 0.5, 0.0),
                    Flip::None,
                ),
                Interp::EaseOut,
            ),
            Key::new(24 * FRAME, Xform::IDENTITY, Interp::Step),
        ]);
    }

    let looped = sequence(&mut doc, root, "Loop", 4 * FRAME, 0);
    frames(
        &mut doc,
        looped,
        "Flicker",
        (0..3)
            .map(|i| {
                vec![straight_colored(
                    0,
                    Brush::Cylinder,
                    3 + i,
                    [1.0, 0.3 * i as f32, 0.0],
                    [0.2 + 0.3 * i as f32, 0.8, 0.3],
                    1.0,
                )]
            })
            .collect(),
        0,
    );

    let clip = paint(
        &mut doc,
        root,
        "Clip",
        vec![straight(0, Brush::Ribbon, 4, [0.0, 0.0, 1.0])],
    );
    // Rotated and hidden on the first frame: the case that loses its rotation in
    // importers that rebuild transforms from matrices, unless the writer guards it.
    doc.layer_mut(clip).unwrap().transform = Xform::new(
        DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2),
        2.0,
        DVec3::new(0.0, 0.25, 0.0),
        Flip::None,
    );
    doc.layer_mut(clip).unwrap().anim.visibility = Track::from_keys([
        Key::step(4 * FRAME, true),
        Key::step(10 * FRAME, false),
        Key::step(16 * FRAME, true),
    ]);

    let fade = paint(
        &mut doc,
        root,
        "Fade",
        vec![straight(0, Brush::Cylinder, 4, [0.0, 0.0, 2.0])],
    );
    {
        let layer = doc.layer_mut(fade).unwrap();
        layer.as_paint_mut().unwrap().shading = Shading::Lit;
        layer.anim.opacity = Track::from_keys([
            Key::new(0, 1.0, Interp::Linear),
            Key::new(24 * FRAME, 0.2, Interp::Linear),
        ]);
    }

    for (name, kind, position) in [
        ("Bulb", LightKind::Point, DVec3::new(0.0, 2.0, 0.0)),
        ("Torch", LightKind::Spot, DVec3::new(2.0, 2.0, 2.0)),
        ("Sun", LightKind::Directional, DVec3::new(0.0, 5.0, 0.0)),
    ] {
        let mut layer = doc.new_layer(
            name,
            LayerKind::Light(Light {
                kind,
                color: [1.0, 0.9, 0.7],
                energy: 2.0,
                ..Light::default()
            }),
            0,
        );
        layer.transform = Xform::new(DQuat::from_rotation_x(-1.0), 1.0, position, Flip::None);
        doc.insert_layer(layer, root, usize::MAX).unwrap();
    }
    let mut camera = doc.new_layer("Shot", LayerKind::Camera(Camera { fov: 50.0 }), 0);
    camera.transform = Xform::from_translation(DVec3::new(0.0, 1.0, 5.0));
    doc.insert_layer(camera, root, usize::MAX).unwrap();
    doc
}

#[test]
fn the_fixture_scene_passes_validation_in_every_configuration() {
    let doc = everything();
    doc.validate().unwrap();
    let range = ExportOptions::animated(0, 24 * FRAME, RATE);
    let extensions = GltfOptions {
        node_visibility: true,
        animate_opacity: true,
        ..GltfOptions::default()
    };
    for options in [
        ExportOptions::still(0),
        range,
        ExportOptions {
            bake_transforms: true,
            ..range
        },
    ] {
        for extra in [GltfOptions::default(), extensions] {
            let bytes = limner_export::export_glb(&doc, &options, &extra).unwrap();
            Glb::checked(&bytes);
        }
    }
}

#[test]
#[ignore = "writes files for manual checks in external tools"]
fn write_fixture_files() {
    let doc = everything();
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../local/out/export")
        .components()
        .collect::<PathBuf>();
    std::fs::create_dir_all(&out).unwrap();
    let range = ExportOptions::animated(0, 24 * FRAME, RATE);
    let extensions = GltfOptions {
        node_visibility: true,
        animate_opacity: true,
        ..GltfOptions::default()
    };
    let baked = ExportOptions {
        bake_transforms: true,
        ..range
    };
    for (name, options, extra) in [
        (
            "fixture-still.glb",
            ExportOptions::still(0),
            GltfOptions::default(),
        ),
        ("fixture-animated.glb", range, GltfOptions::default()),
        ("fixture-animated-extensions.glb", range, extensions),
        ("fixture-animated-baked.glb", baked, GltfOptions::default()),
    ] {
        let path = out.join(name);
        let report = write_glb(&doc, &path, &options, &extra).unwrap();
        println!("{}: {report:?}", path.display());
    }
    let path = out.join("fixture-still.obj");
    let report = write_obj(&doc, &path, 0, false).unwrap();
    println!("{}: {report:?}", path.display());
}
