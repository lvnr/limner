//! Layers at opacity zero: not drawn by Limner, so not exported as solid geometry;
//! and layers that fade in keep their fullest opacity in formats without animation.

mod support;

use limner_core::model::{Brush, Document, Interp, Key, LayerKind, Light, SoundKind, Track};
use limner_export::{
    build_scene, export_fbx, export_glb, export_obj, export_usda, ExportOptions, FbxOptions,
    GltfOptions, UsdOptions,
};
use support::fbx::Fbx;
use support::usda::{numbers, Usda};
use support::{audio, group, paint, sound, straight, wav, Glb, FRAME, RATE};

fn range() -> ExportOptions {
    ExportOptions::animated(0, 12 * FRAME, RATE)
}

/// "Ghost" at opacity zero; "Veil", a group at opacity zero holding a paint layer and
/// a light; "Curtain", a group at opacity zero holding only paint; "Mute", a sound at
/// opacity zero; "Solid", an ordinary layer.
fn transparent_layers() -> Document {
    let mut doc = Document::empty();
    let root = doc.root();
    let ghost = paint(
        &mut doc,
        root,
        "Ghost",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    doc.layer_mut(ghost).unwrap().opacity = 0.0;

    let veil = group(&mut doc, root, "Veil");
    doc.layer_mut(veil).unwrap().opacity = 0.0;
    paint(
        &mut doc,
        veil,
        "Behind veil",
        vec![straight(0, Brush::Ribbon, 3, [0.0; 3])],
    );
    let lamp = doc.new_layer("Lamp", LayerKind::Light(Light::default()), 0);
    doc.insert_layer(lamp, veil, usize::MAX).unwrap();

    let curtain = group(&mut doc, root, "Curtain");
    doc.layer_mut(curtain).unwrap().opacity = 0.0;
    let inner = group(&mut doc, curtain, "Inner");
    paint(
        &mut doc,
        inner,
        "Behind curtain",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );

    let media = audio(&mut doc, "hum.wav", "wav", wav(800, 8000));
    let mute = sound(&mut doc, root, "Mute", Some(media), SoundKind::Flat, true);
    doc.layer_mut(mute).unwrap().opacity = 0.0;

    paint(
        &mut doc,
        root,
        "Solid",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    doc
}

/// "Fade in": invisible on the first frame, fully opaque from frame 6.
fn fade_in() -> Document {
    let mut doc = Document::empty();
    let root = doc.root();
    let layer = paint(
        &mut doc,
        root,
        "Fade in",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    doc.layer_mut(layer).unwrap().anim.opacity = Track::from_keys([
        Key::new(0, 0.0, Interp::Step),
        Key::new(6 * FRAME, 1.0, Interp::Step),
    ]);
    doc
}

#[test]
fn layers_at_opacity_zero_are_not_exported() {
    let doc = transparent_layers();
    for options in [ExportOptions::still(0), range()] {
        let scene = build_scene(&doc, &options);
        let names: Vec<&str> = scene.nodes.iter().map(|n| n.name.as_str()).collect();
        // The light ignores opacity, so it stays, and so does the group that places it.
        assert_eq!(names, ["Root", "Veil", "Lamp", "Solid"], "{options:?}");
    }
    let obj = export_obj(&doc, 0, false).unwrap();
    let objects: Vec<&str> = obj.lines().filter(|l| l.starts_with("o ")).collect();
    assert_eq!(objects, ["o Solid"]);

    let glb = Glb::checked(&export_glb(&doc, &range(), &GltfOptions::default()).unwrap());
    assert_eq!(glb.json["meshes"].as_array().unwrap().len(), 1);
    let fbx = Fbx::checked(&export_fbx(&doc, &range(), &FbxOptions::default()).unwrap());
    assert_eq!(fbx.objects_of("Geometry").len(), 1);
}

#[test]
fn a_frame_at_opacity_zero_is_not_on_screen() {
    // Shown for the first five frames only, and transparent on all of them.
    let mut doc = fade_in();
    let layer = doc.layer_mut(doc.root()).unwrap().children()[0];
    doc.layer_mut(layer).unwrap().anim.visibility =
        Track::from_keys([Key::step(0, true), Key::step(5 * FRAME, false)]);
    let scene = build_scene(&doc, &range());
    assert!(scene.find_layer(layer).is_none());
}

#[test]
fn a_layer_that_fades_in_keeps_its_fullest_opacity() {
    let doc = fade_in();
    let scene = build_scene(&doc, &range());
    assert_eq!(*scene.nodes[1].opacity.first(), 0.0);

    let glb = Glb::checked(&export_glb(&doc, &range(), &GltfOptions::default()).unwrap());
    let material = &glb.primitives(glb.node_named("Fade in"))[0].material;
    assert!(
        material["pbrMetallicRoughness"]
            .get("baseColorFactor")
            .is_none(),
        "fully opaque at its fullest: {material}"
    );
    assert!(material.get("alphaMode").is_none());

    // Animated opacity starts where the layer starts: invisible.
    let extra = GltfOptions {
        animate_opacity: true,
        ..GltfOptions::default()
    };
    let glb = Glb::checked(&export_glb(&doc, &range(), &extra).unwrap());
    let material = &glb.primitives(glb.node_named("Fade in"))[0].material;
    assert_eq!(
        material["pbrMetallicRoughness"]["baseColorFactor"],
        serde_json::json!([1.0, 1.0, 1.0, 0.0])
    );

    let usda = Usda::parse(&export_usda(&doc, &range(), &UsdOptions::default()).unwrap());
    let tubes = usda.prim("/Limner_scene/Root/Fade_in/Tubes");
    let opacities = numbers(tubes.attr("primvars:displayOpacity").unwrap());
    assert!(opacities.iter().all(|o| *o == 1.0), "{opacities:?}");

    let fbx = Fbx::checked(&export_fbx(&doc, &range(), &FbxOptions::default()).unwrap());
    let geometry = &fbx.objects_of("Geometry")[0];
    let colors = geometry
        .record
        .child("LayerElementColor")
        .value("Colors")
        .as_f64s();
    assert!(colors.chunks(4).all(|c| c[3] == 1.0));
}

#[test]
fn layers_that_are_never_transparent_are_unchanged() {
    // A layer fading out keeps its first frame's opacity, which is also its fullest.
    let mut doc = fade_in();
    let layer = doc.layer_mut(doc.root()).unwrap().children()[0];
    doc.layer_mut(layer).unwrap().anim.opacity = Track::from_keys([
        Key::new(0, 0.5, Interp::Linear),
        Key::new(12 * FRAME, 0.2, Interp::Linear),
    ]);
    let glb = Glb::checked(&export_glb(&doc, &range(), &GltfOptions::default()).unwrap());
    let material = &glb.primitives(glb.node_named("Fade in"))[0].material;
    assert_eq!(
        material["pbrMetallicRoughness"]["baseColorFactor"],
        serde_json::json!([1.0, 1.0, 1.0, 0.5])
    );
}
