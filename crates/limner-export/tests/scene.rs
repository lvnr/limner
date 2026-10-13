//! The export scene: what `build_scene` keeps, drops, shares and bakes.

mod support;

use limner_core::glam::{DQuat, DVec3, Vec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Brush, Document, Interp, Key, LayerKind, MediaData, MediaKind, Model, Paint, Picture,
    Projection, Shading, Stroke, Track,
};
use limner_core::tess::ring_positions;
use limner_export::{
    build_scene, tessellate_drawing, ExportOptions, NodeContent, NodeKind, Sampled,
};
use support::{frames, group, paint, point, straight, straight_colored, FRAME, RATE};

fn tube() -> Vec<Stroke> {
    vec![straight(0, Brush::Cylinder, 3, [0.0; 3])]
}

fn names(scene: &limner_export::ExportScene) -> Vec<&str> {
    scene.nodes.iter().map(|n| n.name.as_str()).collect()
}

#[test]
fn the_node_tree_mirrors_the_layer_tree() {
    let mut doc = Document::new(); // Root, Spawn Area, Paint (empty)
    let root = doc.root();
    let body = group(&mut doc, root, "Body");
    let arm = paint(&mut doc, body, "Arm", tube());
    let scene = build_scene(&doc, &ExportOptions::still(0));
    // The empty default paint layer shows nothing and is left out.
    assert_eq!(names(&scene), vec!["Root", "Spawn Area", "Body", "Arm"]);
    assert_eq!(scene.roots, vec![0]);
    assert_eq!(scene.nodes[0].kind, NodeKind::Sequence);
    assert_eq!(scene.nodes[1].kind, NodeKind::Viewpoint);
    assert_eq!(scene.nodes[2].kind, NodeKind::Group);
    assert_eq!(scene.nodes[3].kind, NodeKind::Paint);
    assert_eq!(scene.nodes[0].children, vec![1, 2]);
    assert_eq!(scene.nodes[2].children, vec![3]);
    assert_eq!(scene.nodes[3].parent, Some(2));
    assert_eq!(scene.find_layer(arm), Some(3));
    assert_eq!(scene.frame_count(), 1);
    assert!(!scene.is_animated());
    assert!(!scene.baked);
}

#[test]
fn hidden_layers_are_left_out_unless_asked_for() {
    let mut doc = Document::empty();
    let root = doc.root();
    let hidden = group(&mut doc, root, "Hidden");
    doc.layer_mut(hidden).unwrap().visible = false;
    paint(&mut doc, hidden, "Inside", tube());
    paint(&mut doc, root, "Shown", tube());

    let scene = build_scene(&doc, &ExportOptions::still(0));
    assert_eq!(names(&scene), vec!["Root", "Shown"]);
    assert_eq!(scene.meshes.len(), 1);

    let options = ExportOptions {
        include_hidden: true,
        ..ExportOptions::still(0)
    };
    let scene = build_scene(&doc, &options);
    assert_eq!(names(&scene), vec!["Root", "Hidden", "Inside", "Shown"]);
    assert!(scene.is_visible(2, 0), "the eye toggle is ignored");
    // Both layers show the same strokes but not the same drawing, so two meshes.
    assert_eq!(scene.meshes.len(), 2);
}

#[test]
fn layers_outside_their_clip_export_nothing() {
    let mut doc = Document::empty();
    let root = doc.root();
    let late = paint(&mut doc, root, "Late", tube());
    doc.layer_mut(late).unwrap().anim.visibility = Track::from_keys([Key::step(100 * FRAME, true)]);

    // Hidden at the instant of a still, and throughout a range that ends before the
    // clip: include_hidden does not override clips.
    for options in [
        ExportOptions::still(0),
        ExportOptions::animated(0, 10 * FRAME, RATE),
        ExportOptions {
            include_hidden: true,
            ..ExportOptions::still(0)
        },
    ] {
        let scene = build_scene(&doc, &options);
        assert_eq!(names(&scene), vec!["Root"]);
        assert!(scene.meshes.is_empty());
    }
    let scene = build_scene(&doc, &ExportOptions::still(100 * FRAME));
    assert_eq!(names(&scene), vec!["Root", "Late"]);
}

#[test]
fn visibility_opacity_and_transform_are_sampled_per_frame() {
    let mut doc = Document::empty();
    let root = doc.root();
    let outer = group(&mut doc, root, "Outer");
    doc.layer_mut(outer).unwrap().opacity = 0.5;
    doc.layer_mut(outer).unwrap().anim.visibility =
        Track::from_keys([Key::step(0, true), Key::step(2 * FRAME, false)]);
    let leaf = paint(&mut doc, outer, "Leaf", tube());
    {
        let layer = doc.layer_mut(leaf).unwrap();
        layer.anim.opacity = Track::from_keys([
            Key::new(0, 1.0, Interp::Linear),
            Key::new(4 * FRAME, 0.0, Interp::Linear),
        ]);
        layer.anim.transform = Track::from_keys([
            Key::new(0, Xform::IDENTITY, Interp::Linear),
            Key::new(
                4 * FRAME,
                Xform::from_translation(DVec3::new(4.0, 0.0, 0.0)),
                Interp::Linear,
            ),
        ]);
    }
    let scene = build_scene(&doc, &ExportOptions::animated(0, 4 * FRAME, RATE));
    assert_eq!(scene.frame_count(), 5);
    assert_eq!(scene.seconds(3), 3.0 / 24.0);

    let outer_node = &scene.nodes[scene.find_layer(outer).unwrap()];
    assert_eq!(
        outer_node.visible,
        Sampled::PerFrame(vec![true, true, false, false, false])
    );
    assert_eq!(outer_node.opacity, Sampled::Constant(0.5));
    assert_eq!(outer_node.transform, Sampled::Constant(Xform::IDENTITY));

    let leaf_index = scene.find_layer(leaf).unwrap();
    let leaf_node = &scene.nodes[leaf_index];
    // With the hierarchy kept, a node's visibility is its own: the group's clip is not repeated.
    assert_eq!(leaf_node.visible, Sampled::Constant(true));
    assert!(scene.is_visible(leaf_index, 1));
    assert!(!scene.is_visible(leaf_index, 2));
    // Opacity is resolved down the tree, since no format inherits it.
    assert_eq!(
        leaf_node.opacity,
        Sampled::PerFrame(vec![0.5, 0.375, 0.25, 0.125, 0.0])
    );
    let Sampled::PerFrame(transforms) = &leaf_node.transform else {
        panic!("the transform is keyed");
    };
    assert_eq!(transforms[3].translation, DVec3::new(3.0, 0.0, 0.0));
    // The drawing is shown whenever the layer is: hiding the group adds no change to it.
    let NodeContent::Paint(content) = &leaf_node.content else {
        panic!("a paint node");
    };
    assert_eq!(content.drawings.len(), 1);
    assert_eq!(content.drawings[0].shown, Sampled::Constant(true));
}

#[test]
fn a_drawing_shared_by_two_layers_is_tessellated_once() {
    let mut doc = Document::empty();
    let root = doc.root();
    let first = paint(&mut doc, root, "First", tube());
    let drawing = doc.layer(first).unwrap().as_paint().unwrap().drawings[0];
    let mut second = doc.new_layer("Second", LayerKind::Paint(Paint::single(drawing, RATE)), 0);
    second.transform = Xform::from_translation(DVec3::new(0.0, 2.0, 0.0));
    doc.insert_layer(second, root, usize::MAX).unwrap();

    let meshes_of = |scene: &limner_export::ExportScene, name: &str| {
        let node = scene.nodes.iter().find(|n| n.name == name).unwrap();
        let NodeContent::Paint(content) = &node.content else {
            panic!("a paint node");
        };
        content.drawings[0].meshes.clone()
    };
    let scene = build_scene(&doc, &ExportOptions::still(0));
    assert_eq!(scene.meshes.len(), 1);
    assert_eq!(meshes_of(&scene, "First"), meshes_of(&scene, "Second"));

    // Baked, each layer needs its own world-space copy.
    let options = ExportOptions {
        bake_transforms: true,
        ..ExportOptions::still(0)
    };
    let scene = build_scene(&doc, &options);
    assert!(scene.baked);
    assert_eq!(scene.meshes.len(), 2);
    assert_ne!(meshes_of(&scene, "First"), meshes_of(&scene, "Second"));
    let second = &scene.meshes[meshes_of(&scene, "Second")[0]];
    assert!(second.positions.iter().all(|p| p[1] > 1.9));
}

#[test]
fn baking_flattens_the_tree_and_keeps_only_content() {
    let mut doc = Document::new();
    let root = doc.root();
    let body = group(&mut doc, root, "Body");
    doc.layer_mut(body).unwrap().transform = Xform::new(
        DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
        2.0,
        DVec3::new(10.0, 0.0, 0.0),
        Flip::None,
    );
    let stroke = straight(0, Brush::Cylinder, 3, [0.0; 3]);
    paint(&mut doc, body, "Arm", vec![stroke.clone()]);
    let options = ExportOptions {
        bake_transforms: true,
        ..ExportOptions::still(0)
    };
    let scene = build_scene(&doc, &options);
    assert_eq!(
        names(&scene),
        vec!["Arm"],
        "groups and spawn areas are dropped"
    );
    assert_eq!(scene.roots, vec![0]);
    assert_eq!(scene.nodes[0].parent, None);
    assert_eq!(scene.nodes[0].transform, Sampled::Constant(Xform::IDENTITY));

    // A quarter turn about Z, doubled, moved ten metres along X: (x, y, z) -> (10 - 2y, 2x, 2z).
    let mut layer_space = Vec::new();
    for i in 0..stroke.len() {
        ring_positions(stroke.points(), i, stroke.brush, &mut layer_space);
    }
    for (baked, original) in scene.meshes[0].positions.iter().zip(&layer_space) {
        let expected = Vec3::new(10.0 - 2.0 * original.y, 2.0 * original.x, 2.0 * original.z);
        assert!((Vec3::from_array(*baked) - expected).length() < 1e-5);
    }
    assert!(scene.meshes[0]
        .normals
        .iter()
        .all(|n| (Vec3::from_array(*n).length() - 1.0).abs() < 1e-5));
}

#[test]
fn tessellation_for_export_follows_the_renderer_with_documented_differences() {
    let color = [0.2, 0.4, 3.0];
    let mut erased = straight(1, Brush::Cylinder, 3, [0.0, 1.0, 0.0]);
    for p in erased.points_mut() {
        p.opacity = 0.0;
    }
    let mut broken = straight(2, Brush::Cylinder, 3, [0.0, 2.0, 0.0]);
    broken.points_mut()[1].pos[0] = f32::NAN;
    let directional = Stroke::new(
        3,
        Brush::Ribbon,
        true,
        straight(3, Brush::Ribbon, 3, [0.0, 3.0, 0.0]).into_points(),
    );
    let single = Stroke::new(
        4,
        Brush::Cube,
        false,
        vec![point([0.0; 3], 0.1, color, 1.0)],
    );
    let drawing = limner_core::model::Drawing::from_strokes([
        straight_colored(0, Brush::Cylinder, 4, [0.0; 3], color, 1.0),
        erased,
        broken,
        directional,
        single,
    ]);
    let meshes = tessellate_drawing(&drawing);
    assert_eq!(
        meshes.len(),
        2,
        "one tube mesh and one flat mesh, both opaque"
    );
    let tube = &meshes[0];
    assert!(!tube.double_sided && !tube.has_transparency);
    assert_eq!(
        tube.vertex_count(),
        28,
        "erased and broken strokes add nothing"
    );
    assert_eq!(tube.triangle_count(), 3 * 7 * 2);
    // Colours stay linear and unclamped; writers decide how to encode them.
    assert_eq!(tube.colors[0], [0.2, 0.4, 3.0, 1.0]);
    assert_eq!(tube.positions.len(), tube.normals.len());
    assert_eq!(tube.positions.len(), tube.colors.len());

    // An opaque directional stroke is exported opaque: its fade depends on the viewer.
    let flat = &meshes[1];
    assert!(flat.double_sided && !flat.has_transparency);
    assert_eq!(flat.vertex_count(), 6);

    assert!(tessellate_drawing(&limner_core::model::Drawing::new()).is_empty());
}

#[test]
fn pictures_and_models_are_exported_by_reference() {
    let mut doc = Document::empty();
    let root = doc.root();
    let media = doc.add_media(
        "poster.png",
        MediaKind::Image,
        MediaData::Encoded {
            format: "png".into(),
            bytes: vec![1, 2, 3].into(),
        },
    );
    let picture = doc.new_layer(
        "Poster",
        LayerKind::Picture(Picture {
            media: Some(media),
            projection: Projection::Flat,
            viewer_locked: false,
            source_path: "C:/art/poster.png".into(),
        }),
        0,
    );
    doc.insert_layer(picture, root, usize::MAX).unwrap();
    let model = doc.new_layer(
        "Prop",
        LayerKind::Model(Model {
            media: None,
            shading: Shading::Unlit,
            wireframe: false,
            source_path: String::new(),
        }),
        0,
    );
    doc.insert_layer(model, root, usize::MAX).unwrap();

    let scene = build_scene(&doc, &ExportOptions::still(0));
    let NodeContent::Media(poster) = &scene.nodes[1].content else {
        panic!("a media node");
    };
    assert_eq!(poster.media, Some(media));
    assert_eq!(poster.name, "poster.png");
    assert_eq!(poster.source_path, "C:/art/poster.png");
    let NodeContent::Media(prop) = &scene.nodes[2].content else {
        panic!("a media node");
    };
    assert_eq!(prop.media, None);

    // The glTF node records the reference in its extras.
    let bytes = limner_export::export_glb(
        &doc,
        &ExportOptions::still(0),
        &limner_export::GltfOptions::default(),
    )
    .unwrap();
    let glb = support::Glb::checked(&bytes);
    let extras = &glb.node(glb.node_named("Poster"))["extras"]["limner"];
    assert_eq!(extras["kind"], "Picture");
    assert_eq!(extras["media"], "poster.png");
    assert_eq!(extras["mediaId"], media.0);
}

#[test]
fn frame_by_frame_schedules_are_per_drawing() {
    let mut doc = Document::empty();
    let root = doc.root();
    let layer = frames(
        &mut doc,
        root,
        "Flip",
        vec![
            vec![straight(0, Brush::Cylinder, 2, [0.0; 3])],
            // An empty drawing is a blank frame: nothing to export, but it still hides the others.
            vec![],
            vec![straight(0, Brush::Cylinder, 2, [0.0, 1.0, 0.0])],
        ],
        0,
    );
    let scene = build_scene(&doc, &ExportOptions::animated(0, 5 * FRAME, RATE));
    let node = &scene.nodes[scene.find_layer(layer).unwrap()];
    let NodeContent::Paint(content) = &node.content else {
        panic!("a paint node");
    };
    let ordinals: Vec<u32> = content.drawings.iter().map(|d| d.ordinal).collect();
    assert_eq!(ordinals, vec![0, 2]);
    assert_eq!(
        content.drawings[0].shown,
        Sampled::PerFrame(vec![true, false, false, true, false, false])
    );
    assert_eq!(
        content.drawings[1].shown,
        Sampled::PerFrame(vec![false, false, true, false, false, true])
    );
    assert_eq!(scene.meshes.len(), 2);
    assert_eq!(scene.vertex_count(), 28);
    assert_eq!(scene.triangle_count(), 28);
}
