//! glTF animation: sampled transforms, frame-by-frame drawings, clips, opacity.

mod support;

use limner_core::eval::{evaluate, Clock, Content};
use limner_core::glam::{DQuat, DVec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{Brush, Document, Interp, Key, LayerId, Track};
use limner_core::time;
use limner_export::{export_glb, export_glb_with_report, ExportOptions, GltfOptions};
use support::{frames, group, paint, sequence, straight, Channel, Glb, FRAME, RATE};

fn animated(doc: &Document, last_frame: i64, extra: &GltfOptions) -> Glb {
    let options = ExportOptions::animated(0, last_frame * FRAME, RATE);
    Glb::checked(&export_glb(doc, &options, extra).unwrap())
}

fn seconds(frame: i64) -> f32 {
    time::to_seconds(frame * FRAME) as f32
}

fn tube() -> Vec<limner_core::model::Stroke> {
    vec![straight(0, Brush::Cylinder, 3, [0.0; 3])]
}

fn clips(doc: &mut Document, layer: LayerId, keys: &[(i64, bool)]) {
    let layer = doc.layer_mut(layer).unwrap();
    layer.anim.visibility = Track::from_keys(keys.iter().map(|(f, v)| Key::step(f * FRAME, *v)));
    layer.anim.offset.clear();
}

/// A step channel's value on every frame, as "is the scale non-zero".
fn shown_per_frame(channel: &Channel, last_frame: i64) -> Vec<bool> {
    assert!(channel.step, "visibility is stepped");
    (0..=last_frame)
        .map(|k| {
            let value = channel.sample(seconds(k));
            assert!(
                value == [1.0; 3] || value == [0.0; 3],
                "visibility scale is exactly one or zero"
            );
            value[0] == 1.0
        })
        .collect()
}

#[test]
fn a_still_has_no_animation() {
    let mut doc = Document::empty();
    let root = doc.root();
    let layer = paint(&mut doc, root, "P", tube());
    doc.layer_mut(layer).unwrap().anim.transform = Track::from_keys([
        Key::new(0, Xform::IDENTITY, Interp::Linear),
        Key::new(
            10 * FRAME,
            Xform::from_translation(DVec3::X),
            Interp::Linear,
        ),
    ]);
    let bytes = export_glb(
        &doc,
        &ExportOptions::still(5 * FRAME),
        &GltfOptions::default(),
    )
    .unwrap();
    let glb = Glb::checked(&bytes);
    assert!(glb.json.get("animations").is_none());
    // The still shows the state at its instant.
    assert_eq!(glb.translation(glb.node_named("P")), [0.5, 0.0, 0.0]);

    let mut off = ExportOptions::animated(0, 10 * FRAME, RATE);
    off.animation = false;
    let glb = Glb::checked(&export_glb(&doc, &off, &GltfOptions::default()).unwrap());
    assert!(glb.json.get("animations").is_none());
}

#[test]
fn keyed_transforms_are_sampled_from_the_evaluator() {
    let mut doc = Document::empty();
    doc.settings.title = "Swing".to_owned();
    let root = doc.root();
    let rig = group(&mut doc, root, "Rig");
    doc.layer_mut(rig).unwrap().transform = Xform {
        rotation: DQuat::from_rotation_z(0.4),
        scale: 1.5,
        translation: DVec3::new(0.0, 1.0, 0.0),
        flip: Flip::None,
    };
    let arm = paint(&mut doc, rig, "Arm", tube());
    let a = Xform {
        rotation: DQuat::from_rotation_y(0.2),
        scale: 1.0,
        translation: DVec3::new(0.0, 0.0, 0.0),
        flip: Flip::X,
    };
    let b = Xform {
        rotation: DQuat::from_rotation_y(2.9) * DQuat::from_rotation_x(1.1),
        scale: 2.0,
        translation: DVec3::new(1.0, 2.0, -3.0),
        flip: Flip::X,
    };
    let c = Xform {
        // More than half a turn on from `b`, so the stored quaternions change hemisphere.
        rotation: DQuat::from_rotation_y(-2.9) * DQuat::from_rotation_x(1.1),
        scale: 0.5,
        translation: DVec3::new(-1.0, 0.0, 0.5),
        flip: Flip::X,
    };
    {
        let layer = doc.layer_mut(arm).unwrap();
        layer.pivot = Xform::from_translation(DVec3::new(0.5, 0.0, 0.0));
        layer.anim.transform = Track::from_keys([
            Key::new(2 * FRAME, a, Interp::EaseInOut),
            Key::new(12 * FRAME, b, Interp::EaseIn),
            Key::new(20 * FRAME, c, Interp::Step),
        ]);
    }

    let last = 24;
    let glb = animated(&doc, last, &GltfOptions::default());
    let node = glb.node_named("Arm");
    let translation = glb.channel(node, "translation").expect("translation moves");
    let rotation = glb.channel(node, "rotation").expect("rotation moves");
    let scale = glb.channel(node, "scale").expect("scale moves");
    assert!(!translation.step && !rotation.step && !scale.step);
    assert_eq!(glb.channel_count(), 3, "nothing else in the scene moves");
    assert_eq!(glb.json["animations"][0]["name"], "Swing");

    for k in 0..=last {
        let frame = evaluate(&doc, Clock::at(k * FRAME));
        let expected = frame.get(arm).unwrap().local;
        let t = translation.sample(seconds(k));
        let r = rotation.sample(seconds(k));
        let s = scale.sample(seconds(k));
        let got_t = DVec3::new(t[0] as f64, t[1] as f64, t[2] as f64);
        assert!(
            (got_t - expected.translation).length() < 1e-5,
            "frame {k}: translation {got_t:?} vs {:?}",
            expected.translation
        );
        let got_r = DQuat::from_xyzw(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64);
        assert!(
            got_r.normalize().dot(expected.rotation).abs() > 1.0 - 1e-6,
            "frame {k}: rotation"
        );
        let expected_scale = expected.flip.signs() * expected.scale;
        let got_s = DVec3::new(s[0] as f64, s[1] as f64, s[2] as f64);
        assert!(
            (got_s - expected_scale).length() < 1e-5,
            "frame {k}: scale {got_s:?} vs {expected_scale:?}"
        );
    }

    // Runs where nothing changes are merged: before the first key (frames 0..=2) and
    // after the last (frames 20..=24) only the ends of the hold are written.
    let times: Vec<f32> = [0, 2]
        .into_iter()
        .chain(3..=20)
        .chain([24])
        .map(seconds)
        .collect();
    assert_eq!(translation.times, times);
    // The rest pose is the first frame.
    assert_eq!(glb.scale(node), [-1.0, 1.0, 1.0]);
    // The animation spans the whole range even though the last change is at frame 20.
    assert_eq!(*rotation.times.last().unwrap(), seconds(last));
}

#[test]
fn constant_channels_are_omitted() {
    let mut doc = Document::empty();
    let root = doc.root();
    let slide = paint(&mut doc, root, "Slide", tube());
    let rotation = DQuat::from_rotation_x(0.3);
    doc.layer_mut(slide).unwrap().anim.transform = Track::from_keys([
        Key::new(
            0,
            Xform::new(rotation, 2.0, DVec3::ZERO, Flip::None),
            Interp::Linear,
        ),
        Key::new(
            8 * FRAME,
            Xform::new(rotation, 2.0, DVec3::new(0.0, 3.0, 0.0), Flip::None),
            Interp::Linear,
        ),
    ]);
    let held = paint(&mut doc, root, "Held", tube());
    let pose = Xform::new(rotation, 0.5, DVec3::ONE, Flip::Z);
    doc.layer_mut(held).unwrap().anim.transform = Track::from_keys([
        Key::new(0, pose, Interp::EaseOut),
        Key::new(8 * FRAME, pose, Interp::Linear),
    ]);
    paint(&mut doc, root, "Still", tube());

    let glb = animated(&doc, 8, &GltfOptions::default());
    let slide = glb.node_named("Slide");
    assert!(glb.channel(slide, "translation").is_some());
    assert!(glb.channel(slide, "rotation").is_none());
    assert!(glb.channel(slide, "scale").is_none());
    for name in ["Held", "Still", "Root"] {
        let node = glb.node_named(name);
        for path in ["translation", "rotation", "scale"] {
            assert!(glb.channel(node, path).is_none(), "{name} {path}");
        }
    }
    assert_eq!(glb.channel_count(), 1);
    // The keyed-but-constant layer still has its pose as the node transform.
    assert_eq!(glb.scale(glb.node_named("Held")), [0.5, 0.5, -0.5]);
}

#[test]
fn frame_by_frame_drawings_become_stepped_children() {
    let mut doc = Document::empty();
    let root = doc.root();
    // The sequence loops every four frames; the layer inside has three drawings that
    // loop on their own. So the layer shows 0 1 2 0 | 0 1 2 0 | ...
    let looped = sequence(&mut doc, root, "Loop", 4 * FRAME, 0);
    let layer = frames(
        &mut doc,
        looped,
        "Flip",
        vec![
            vec![straight(0, Brush::Cylinder, 2, [0.0, 0.0, 0.0])],
            vec![straight(0, Brush::Cylinder, 3, [0.0, 1.0, 0.0])],
            vec![straight(0, Brush::Ribbon, 4, [0.0, 2.0, 0.0])],
        ],
        0,
    );
    let drawings = doc
        .layer(layer)
        .unwrap()
        .as_paint()
        .unwrap()
        .drawings
        .clone();
    let last = 11;
    let output = export_glb_with_report(
        &doc,
        &ExportOptions::animated(0, last * FRAME, RATE),
        &GltfOptions::default(),
    )
    .unwrap();
    let glb = Glb::checked(&output.bytes);
    assert_eq!(output.report.animation_frames, 12);
    assert_eq!(output.report.animation_channels, 3);
    assert_eq!(output.report.meshes, 3);

    let parent = glb.node_named("Flip");
    assert!(glb.node(parent).get("mesh").is_none());
    let children: Vec<usize> = ["Flip.000", "Flip.001", "Flip.002"]
        .iter()
        .map(|name| glb.node_named(name))
        .collect();
    let schedules: Vec<Vec<bool>> = children
        .iter()
        .enumerate()
        .map(|(ordinal, child)| {
            assert_eq!(glb.parent_of(*child), Some(parent));
            let extras = &glb.node(*child)["extras"]["limner"];
            assert_eq!(extras["layerId"], layer.0);
            assert_eq!(extras["drawing"], ordinal);
            assert_eq!(extras["drawingId"], drawings[ordinal].0);
            for path in ["translation", "rotation"] {
                assert!(glb.channel(*child, path).is_none());
            }
            shown_per_frame(&glb.channel(*child, "scale").expect("a schedule"), last)
        })
        .collect();

    let mut seen = Vec::new();
    for k in 0..=last {
        let frame = evaluate(&doc, Clock::at(k * FRAME));
        let Content::Paint { drawing, .. } = frame.get(layer).unwrap().content else {
            panic!("a paint layer");
        };
        let expected = drawings.iter().position(|d| *d == drawing).unwrap();
        seen.push(expected);
        for (ordinal, schedule) in schedules.iter().enumerate() {
            assert_eq!(
                schedule[k as usize],
                ordinal == expected,
                "frame {k}, drawing {ordinal}"
            );
        }
    }
    assert_eq!(seen, vec![0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0]);

    // Rest state: the drawing of the first frame is the only one at full scale.
    assert_eq!(glb.scale(children[0]), [1.0; 3]);
    assert_eq!(glb.scale(children[1]), [0.0; 3]);
    assert_eq!(glb.scale(children[2]), [0.0; 3]);
    // Each drawing kept its own geometry.
    let vertices: Vec<usize> = children
        .iter()
        .map(|c| glb.primitives(*c)[0].positions.len())
        .collect();
    assert_eq!(vertices, vec![14, 21, 8]);
    // Neither the looping sequence nor the layer needs a channel of its own.
    for name in ["Loop", "Flip"] {
        assert!(glb.channel(glb.node_named(name), "scale").is_none());
    }
}

#[test]
fn a_finite_repeat_holds_its_last_drawing() {
    let mut doc = Document::empty();
    let root = doc.root();
    let layer = frames(
        &mut doc,
        root,
        "Once",
        vec![
            vec![straight(0, Brush::Cylinder, 2, [0.0, 0.0, 0.0])],
            vec![straight(0, Brush::Cylinder, 2, [0.0, 1.0, 0.0])],
        ],
        1,
    );
    let _ = layer;
    let glb = animated(&doc, 6, &GltfOptions::default());
    let first = shown_per_frame(
        &glb.channel(glb.node_named("Once.000"), "scale").unwrap(),
        6,
    );
    let second = shown_per_frame(
        &glb.channel(glb.node_named("Once.001"), "scale").unwrap(),
        6,
    );
    assert_eq!(first, vec![true, false, false, false, false, false, false]);
    assert_eq!(second, vec![false, true, true, true, true, true, true]);
    // The hold is one key, plus the closing key that gives the animation its length.
    assert_eq!(
        glb.channel(glb.node_named("Once.001"), "scale")
            .unwrap()
            .times,
        vec![seconds(0), seconds(1), seconds(6)]
    );
}

#[test]
fn drawings_outside_the_range_are_not_exported() {
    let mut doc = Document::empty();
    let root = doc.root();
    frames(
        &mut doc,
        root,
        "Long",
        (0..6)
            .map(|i| vec![straight(0, Brush::Cylinder, 2, [0.0, i as f32, 0.0])])
            .collect(),
        1,
    );
    // Frames 2 and 3 only.
    let options = ExportOptions::animated(2 * FRAME, 3 * FRAME, RATE);
    let glb = Glb::checked(&export_glb(&doc, &options, &GltfOptions::default()).unwrap());
    assert!(glb.has_node("Long.002") && glb.has_node("Long.003"));
    for missing in ["Long.000", "Long.001", "Long.004", "Long.005"] {
        assert!(!glb.has_node(missing));
    }
    assert_eq!(glb.list("meshes").len(), 2);
    // Time starts at zero at the start of the range.
    let channel = glb.channel(glb.node_named("Long.003"), "scale").unwrap();
    assert_eq!(channel.times, vec![seconds(0), seconds(1)]);
}

#[test]
fn clips_hide_and_show_with_a_stepped_scale() {
    let mut doc = Document::empty();
    let root = doc.root();
    let clip = paint(&mut doc, root, "Clip", tube());
    doc.layer_mut(clip).unwrap().transform =
        Xform::new(DQuat::IDENTITY, 2.0, DVec3::new(1.0, 0.0, 0.0), Flip::Y);
    clips(&mut doc, clip, &[(2, true), (5, false), (8, true)]);
    let gate = group(&mut doc, root, "Gate");
    clips(&mut doc, gate, &[(0, true), (4, false)]);
    paint(&mut doc, gate, "Inside", tube());

    let last = 10;
    let glb = animated(&doc, last, &GltfOptions::default());
    let node = glb.node_named("Clip");
    // One drawing shown throughout its clips: the mesh sits on the layer's own node.
    assert!(glb.node(node).get("mesh").is_some());
    assert_eq!(glb.scale(node), [0.0; 3], "hidden on the first frame");
    let channel = glb.channel(node, "scale").expect("the clip");
    assert!(channel.step);
    let expected = [
        false, false, true, true, true, false, false, false, true, true, true,
    ];
    for (k, visible) in expected.iter().enumerate() {
        let value = channel.sample(seconds(k as i64));
        let scale = if *visible {
            vec![2.0, -2.0, 2.0]
        } else {
            vec![0.0; 3]
        };
        assert_eq!(value, scale, "frame {k}");
        let state = evaluate(&doc, Clock::at(k as i64 * FRAME));
        assert_eq!(state.get(clip).unwrap().visible, *visible, "frame {k}");
    }
    assert_eq!(
        channel.times,
        [0, 2, 5, 8, 10].map(seconds).to_vec(),
        "one key per change, plus the closing key"
    );
    assert!(glb.channel(node, "translation").is_none());

    // A group's clip hides its children through the hierarchy: only the group is animated.
    let gate = glb.node_named("Gate");
    let gate_channel = glb.channel(gate, "scale").unwrap();
    assert_eq!(
        shown_per_frame(&gate_channel, last),
        (0..=last).map(|k| k < 4).collect::<Vec<_>>()
    );
    let inside = glb.node_named("Inside");
    assert!(glb.channel(inside, "scale").is_none());
    assert!(glb.node(inside).get("mesh").is_some());
    assert_eq!(glb.channel_count(), 2);
    assert!(
        !glb.extensions_used().contains(&"KHR_node_visibility"),
        "the visibility extension is opt-in"
    );
}

#[test]
fn a_moving_clip_steps_its_scale_and_interpolates_the_rest() {
    let mut doc = Document::empty();
    let root = doc.root();
    let layer = paint(&mut doc, root, "Mover", tube());
    clips(&mut doc, layer, &[(0, true), (3, false)]);
    doc.layer_mut(layer).unwrap().anim.transform = Track::from_keys([
        Key::new(0, Xform::IDENTITY, Interp::Linear),
        Key::new(
            6 * FRAME,
            Xform::new(DQuat::IDENTITY, 4.0, DVec3::new(6.0, 0.0, 0.0), Flip::None),
            Interp::Linear,
        ),
    ]);
    let glb = animated(&doc, 6, &GltfOptions::default());
    let node = glb.node_named("Mover");
    let translation = glb.channel(node, "translation").unwrap();
    let scale = glb.channel(node, "scale").unwrap();
    assert!(!translation.step);
    assert!(
        scale.step,
        "a scale channel that carries visibility is stepped"
    );
    assert_eq!(scale.sample(seconds(2)), vec![2.0; 3]);
    assert_eq!(scale.sample(seconds(3)), vec![0.0; 3]);
    assert_eq!(translation.sample(seconds(3)), vec![3.0, 0.0, 0.0]);
}

#[test]
fn node_visibility_is_written_on_request() {
    let mut doc = Document::empty();
    let root = doc.root();
    let clip = paint(&mut doc, root, "Clip", tube());
    clips(&mut doc, clip, &[(2, true), (5, false)]);
    frames(
        &mut doc,
        root,
        "Flip",
        vec![
            vec![straight(0, Brush::Cylinder, 2, [0.0, 0.0, 0.0])],
            vec![straight(0, Brush::Cylinder, 2, [0.0, 1.0, 0.0])],
        ],
        0,
    );
    paint(&mut doc, root, "Always", tube());
    let extra = GltfOptions {
        node_visibility: true,
        ..GltfOptions::default()
    };
    let glb = animated(&doc, 6, &extra);
    assert_eq!(
        glb.extensions_used(),
        vec![
            "KHR_animation_pointer",
            "KHR_materials_unlit",
            "KHR_node_visibility"
        ]
    );
    let node = glb.node_named("Clip");
    assert_eq!(
        glb.node(node)["extensions"]["KHR_node_visibility"]["visible"],
        false
    );
    let pointer = format!("/nodes/{node}/extensions/KHR_node_visibility/visible");
    let channel = glb.pointer_channel(&pointer).expect("a pointer channel");
    assert!(channel.step);
    let per_frame: Vec<f32> = (0..=6).map(|k| channel.sample(seconds(k))[0]).collect();
    assert_eq!(per_frame, vec![0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
    // The stepped scale is still there for viewers without the extension.
    assert!(glb.channel(node, "scale").is_some());

    let second = glb.node_named("Flip.001");
    assert_eq!(
        glb.node(second)["extensions"]["KHR_node_visibility"]["visible"],
        false
    );
    assert!(glb
        .pointer_channel(&format!(
            "/nodes/{second}/extensions/KHR_node_visibility/visible"
        ))
        .is_some());
    // Nodes that never hide carry nothing.
    assert!(glb
        .node(glb.node_named("Always"))
        .get("extensions")
        .is_none());
}

#[test]
fn opacity_keys_animate_material_alpha_on_request() {
    let mut doc = Document::empty();
    let root = doc.root();
    let layer = paint(&mut doc, root, "Fade", tube());
    doc.layer_mut(layer).unwrap().anim.opacity = Track::from_keys([
        Key::new(0, 1.0, Interp::Linear),
        Key::new(8 * FRAME, 0.0, Interp::Linear),
    ]);
    paint(&mut doc, root, "Solid", tube());

    // By default the layer keeps the opacity of the first frame.
    let plain = animated(&doc, 8, &GltfOptions::default());
    assert!(plain.json.get("animations").is_none());
    let material = &plain.primitives(plain.node_named("Fade"))[0].material;
    assert!(material.get("alphaMode").is_none());
    assert_eq!(
        plain.list("materials").len(),
        1,
        "both layers share a material"
    );

    let extra = GltfOptions {
        animate_opacity: true,
        ..GltfOptions::default()
    };
    let glb = animated(&doc, 8, &extra);
    assert_eq!(
        glb.extensions_used(),
        vec!["KHR_animation_pointer", "KHR_materials_unlit"]
    );
    let node = glb.node_named("Fade");
    let mesh = glb.node(node)["mesh"].as_u64().unwrap() as usize;
    let material_index = glb.list("meshes")[mesh]["primitives"][0]["material"]
        .as_u64()
        .unwrap();
    let material = &glb.list("materials")[material_index as usize];
    assert_eq!(material["alphaMode"], "BLEND");
    assert_eq!(material["name"], "Fade unlit see-through");
    let pointer = format!("/materials/{material_index}/pbrMetallicRoughness/baseColorFactor");
    let channel = glb.pointer_channel(&pointer).expect("an opacity channel");
    assert!(!channel.step);
    for k in 0..=8 {
        let expected = evaluate(&doc, Clock::at(k * FRAME))
            .get(layer)
            .unwrap()
            .opacity;
        let value = channel.sample(seconds(k));
        assert_eq!(&value[..3], &[1.0, 1.0, 1.0]);
        assert!((value[3] - expected).abs() < 1e-6, "frame {k}");
    }
    // The layer that does not fade keeps the shared opaque material.
    let solid = &glb.primitives(glb.node_named("Solid"))[0].material;
    assert!(solid.get("alphaMode").is_none());
    assert_eq!(glb.list("materials").len(), 2);
}

#[test]
fn baking_keeps_motion_as_a_world_transform() {
    let mut doc = Document::empty();
    let root = doc.root();
    let rig = group(&mut doc, root, "Rig");
    doc.layer_mut(rig).unwrap().transform = Xform::new(
        DQuat::from_rotation_y(0.5),
        2.0,
        DVec3::new(0.0, 1.0, 0.0),
        Flip::X,
    );
    doc.layer_mut(rig).unwrap().anim.transform = Track::from_keys([
        Key::new(
            0,
            Xform::new(DQuat::from_rotation_y(0.5), 2.0, DVec3::Y, Flip::X),
            Interp::Linear,
        ),
        Key::new(
            6 * FRAME,
            Xform::new(
                DQuat::from_rotation_y(1.5),
                2.0,
                DVec3::new(3.0, 1.0, 0.0),
                Flip::X,
            ),
            Interp::Linear,
        ),
    ]);
    let moving = paint(&mut doc, rig, "Moving", tube());
    doc.layer_mut(moving).unwrap().transform = Xform::from_translation(DVec3::new(0.0, 0.0, 1.0));
    let fixed = paint(&mut doc, root, "Fixed", tube());
    doc.layer_mut(fixed).unwrap().transform = Xform::from_translation(DVec3::new(5.0, 0.0, 0.0));

    let options = ExportOptions {
        bake_transforms: true,
        ..ExportOptions::animated(0, 6 * FRAME, RATE)
    };
    let glb = Glb::checked(&export_glb(&doc, &options, &GltfOptions::default()).unwrap());
    assert!(!glb.has_node("Rig"));
    assert_eq!(glb.list("nodes").len(), 2);

    // The layer that does not move is baked: world-space vertices, no transform.
    let fixed_node = glb.node_named("Fixed");
    assert!(glb.node(fixed_node).get("translation").is_none());
    assert!(glb.primitives(fixed_node)[0]
        .positions
        .iter()
        .all(|p| p.x > 4.9));

    // The moving one keeps layer-space vertices under its sampled world transform.
    let node = glb.node_named("Moving");
    assert!(glb.parent_of(node).is_none());
    let translation = glb.channel(node, "translation").unwrap();
    let rotation = glb.channel(node, "rotation").unwrap();
    assert!(
        glb.channel(node, "scale").is_none(),
        "the scale never changes"
    );
    let positions = glb.primitives(node)[0].positions.clone();
    for k in 0..=6 {
        let world = evaluate(&doc, Clock::at(k * FRAME))
            .get(moving)
            .unwrap()
            .world;
        let t = translation.sample(seconds(k));
        let r = rotation.sample(seconds(k));
        let s = glb.scale(node);
        let node_world = limner_core::glam::DMat4::from_scale_rotation_translation(
            DVec3::from_array(s),
            DQuat::from_xyzw(r[0] as f64, r[1] as f64, r[2] as f64, r[3] as f64).normalize(),
            DVec3::new(t[0] as f64, t[1] as f64, t[2] as f64),
        );
        for p in &positions {
            let got = node_world.transform_point3(p.as_dvec3());
            let expected = world.transform_point3(p.as_dvec3());
            assert!((got - expected).length() < 1e-5, "frame {k}");
        }
    }
    assert!(
        glb.scale(node).iter().product::<f64>() < 0.0,
        "the mirror survives"
    );
}

#[test]
fn a_rotated_layer_with_clips_states_its_rotation_in_the_animation() {
    // Hidden means scale zero, and a zero-scale matrix has no rotation left in it.
    // Importers that rebuild transforms from matrices would show the layer unrotated
    // once its clip starts, so the rotation is written as a channel although constant.
    let mut doc = Document::empty();
    let root = doc.root();
    let rotation = DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2);
    let layer = paint(&mut doc, root, "Turned", tube());
    doc.layer_mut(layer).unwrap().transform =
        Xform::new(rotation, 2.0, DVec3::new(0.0, 0.25, 0.0), Flip::None);
    clips(&mut doc, layer, &[(2, true), (5, false)]);
    let glb = animated(&doc, 6, &GltfOptions::default());
    let node = glb.node_named("Turned");
    let channel = glb.channel(node, "rotation").expect("a rotation channel");
    assert_eq!(
        channel.times,
        vec![seconds(0), seconds(6)],
        "two keys suffice"
    );
    for key in &channel.values {
        let q = DQuat::from_xyzw(key[0] as f64, key[1] as f64, key[2] as f64, key[3] as f64);
        assert!(q.dot(rotation).abs() > 1.0 - 1e-6);
    }
    assert!(glb.channel(node, "scale").is_some());
    assert!(
        glb.channel(node, "translation").is_none(),
        "translation survives in a matrix, so it stays out"
    );
}
