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
    Brush, Camera, Document, Interp, Key, LayerKind, Light, LightKind, MediaData, MediaKind,
    Shading, SoundKind, Track,
};
use limner_export::{
    write_fbx, write_glb, write_obj, write_usda, write_usdz, ExportOptions, FbxOptions, FbxUnit,
    GltfOptions, UsdOptions,
};
use std::path::PathBuf;
use support::fbx::Fbx;
use support::usda::Usda;
use support::zip::read_usdz;
use support::{
    frames, group, paint, sequence, sound, straight, straight_colored, Glb, FRAME, RATE,
};

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

/// A sine tone as 16-bit mono samples, fading in and out so it does not click.
fn tone(seconds: f64, hz: f64, rate: u32) -> Vec<u8> {
    let count = (seconds * f64::from(rate)) as usize;
    (0..count)
        .flat_map(|i| {
            let t = i as f64 / f64::from(rate);
            let fade = (t / 0.02).min((seconds - t) / 0.02).clamp(0.0, 1.0);
            let value = (t * hz * std::f64::consts::TAU).sin() * fade * 0.3;
            ((value * 32767.0) as i16).to_le_bytes()
        })
        .collect()
}

/// The fixture scene with two sounds: "Chime", a positional half-second tone at the
/// bulb's place, heard in a clip from frame 4 to frame 16; and "Drone", a flat tone
/// that loops for the whole range. Both are held as samples, so they export as WAV.
pub fn with_sound() -> Document {
    let mut doc = everything();
    doc.settings.title = "Limner sound fixture".to_owned();
    let root = doc.root();
    let samples = |seconds, hz| MediaData::RawAudio {
        channels: 1,
        sample_rate: 22_050,
        bits_per_sample: 16,
        float: false,
        samples: tone(seconds, hz, 22_050).into(),
    };
    let chime_audio = doc.add_media("chime.wav", MediaKind::Audio, samples(0.5, 880.0));
    let chime = sound(
        &mut doc,
        root,
        "Chime",
        Some(chime_audio),
        SoundKind::Positional,
        false,
    );
    {
        let layer = doc.layer_mut(chime).unwrap();
        layer.transform = Xform::from_translation(DVec3::new(0.0, 2.0, 0.0));
        layer.anim.visibility =
            Track::from_keys([Key::step(4 * FRAME, true), Key::step(16 * FRAME, false)]);
    }
    let drone_audio = doc.add_media("drone.wav", MediaKind::Audio, samples(0.25, 220.0));
    sound(
        &mut doc,
        root,
        "Drone",
        Some(drone_audio),
        SoundKind::Flat,
        true,
    );
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
        // The other formats' readers check structure as they read.
        let usd = UsdOptions::default();
        let text = limner_export::export_usda(&doc, &options, &usd).unwrap();
        Usda::parse(&text).all();
        let package = limner_export::export_usdz(&doc, &options, &usd).unwrap();
        assert_eq!(read_usdz(&package)[0].data, text.as_bytes());
        for unit in [FbxUnit::Metres, FbxUnit::Centimetres] {
            let extra = FbxOptions {
                unit,
                ..FbxOptions::default()
            };
            Fbx::checked(&limner_export::export_fbx(&doc, &options, &extra).unwrap());
        }
    }
}

#[test]
fn the_sound_fixture_passes_validation() {
    let doc = with_sound();
    doc.validate().unwrap();
    let range = ExportOptions::animated(0, 24 * FRAME, RATE);
    for options in [
        ExportOptions::still(0),
        range,
        ExportOptions {
            bake_transforms: true,
            ..range
        },
    ] {
        let usd = UsdOptions::default();
        let (text, report) = limner_export::export_usda_with_report(&doc, &options, &usd).unwrap();
        Usda::parse(&text).all();
        // The still is taken before the chime's clip, so only the drone is heard.
        let heard = if options.animation { 2 } else { 1 };
        assert_eq!(report.sounds, heard, "{options:?}");
        let package = limner_export::export_usdz(&doc, &options, &usd).unwrap();
        assert_eq!(read_usdz(&package).len(), 1 + heard);
        Glb::checked(&limner_export::export_glb(&doc, &options, &GltfOptions::default()).unwrap());
        Fbx::checked(&limner_export::export_fbx(&doc, &options, &FbxOptions::default()).unwrap());
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

    let usd = UsdOptions::default();
    for (name, options) in [
        ("fixture-still.usda", ExportOptions::still(0)),
        ("fixture-animated.usda", range),
        ("fixture-animated-baked.usda", baked),
    ] {
        let path = out.join(name);
        let report = write_usda(&doc, &path, &options, &usd).unwrap();
        println!("{}: {report:?}", path.display());
    }
    let path = out.join("fixture-animated.usdz");
    let report = write_usdz(&doc, &path, &range, &usd).unwrap();
    println!("{}: {report:?}", path.display());

    // Sounds: the .usda's audio goes to local/out/export/audio/.
    let sounding = with_sound();
    let path = out.join("fixture-sound.usda");
    let report = write_usda(&sounding, &path, &range, &usd).unwrap();
    println!("{}: {report:?}", path.display());
    let path = out.join("fixture-sound.usdz");
    let report = write_usdz(&sounding, &path, &range, &usd).unwrap();
    println!("{}: {report:?}", path.display());
    let path = out.join("fixture-sound.glb");
    let report = write_glb(&sounding, &path, &range, &GltfOptions::default()).unwrap();
    println!("{}: {report:?}", path.display());

    let centimetres = FbxOptions {
        unit: FbxUnit::Centimetres,
        ..FbxOptions::default()
    };
    for (name, options, extra) in [
        (
            "fixture-still.fbx",
            ExportOptions::still(0),
            FbxOptions::default(),
        ),
        ("fixture-animated.fbx", range, FbxOptions::default()),
        ("fixture-animated-baked.fbx", baked, FbxOptions::default()),
        ("fixture-animated-centimetres.fbx", range, centimetres),
    ] {
        let path = out.join(name);
        let report = write_fbx(&doc, &path, &options, &extra).unwrap();
        println!("{}: {report:?}", path.display());
    }
}
