//! Binary FBX: container, settings, geometry, materials, transforms, animation,
//! lights and cameras, all read back with the independent reader in `support::fbx`.

mod support;

use limner_core::eval::{evaluate, Clock, Content};
use limner_core::glam::{DAffine3, DVec3, Vec3};
use limner_core::math::Flip;
use limner_core::model::{Brush, Document, Key, LayerKind, Paint, Shading, Stroke, Track};
use limner_core::tess::ring_positions;
use limner_core::time::FrameRate;
use limner_export::{
    build_scene, export_fbx, export_fbx_with_report, scene_to_fbx, write_fbx, ExportError,
    ExportOptions, ExportScene, FbxColorSpace, FbxOptions, FbxUnit,
};
use support::fbx::{euler_to_quat, Curve, Fbx, Object, KTIME_FRAME, KTIME_PER_SECOND};
use support::stage::{arm_strokes, chain, stage, CLIP_SHOWN, FLIP_CASES, STAGE_LAST};
use support::{group, paint, straight, straight_colored, FRAME, RATE};

fn still(doc: &Document) -> Fbx {
    still_with(doc, &FbxOptions::default())
}

fn still_with(doc: &Document, extra: &FbxOptions) -> Fbx {
    Fbx::checked(&export_fbx(doc, &ExportOptions::still(0), extra).unwrap())
}

fn animated(doc: &Document, last_frame: i64) -> Fbx {
    let options = ExportOptions::animated(0, last_frame * FRAME, RATE);
    Fbx::checked(&export_fbx(doc, &options, &FbxOptions::default()).unwrap())
}

fn rings(stroke: &Stroke) -> Vec<DVec3> {
    let mut out: Vec<Vec3> = Vec::new();
    for i in 0..stroke.len() {
        ring_positions(stroke.points(), i, stroke.brush, &mut out);
    }
    out.into_iter().map(|p| p.as_dvec3()).collect()
}

/// The sRGB transfer curve, written out here so the writer's is checked against it.
fn srgb(linear: f64) -> f64 {
    if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    }
}

fn close(a: &DAffine3, b: &DAffine3, tolerance: f64) -> bool {
    a.abs_diff_eq(*b, tolerance)
}

#[test]
fn an_empty_document_is_still_a_valid_file() {
    let fbx = still(&Document::empty());
    assert_eq!(fbx.version, 7400);
    let models = fbx.objects_of("Model");
    assert_eq!(models.len(), 1, "the root layer");
    assert_eq!(models[0].name, "Root");
    assert_eq!(models[0].subtype, "Null");
    assert!(fbx.parent_of(models[0].id).is_none());
    assert_eq!(fbx.objects().len(), 1);
    assert_eq!(fbx.section("Takes").value("Current").as_str(), "");
}

#[test]
fn a_scene_without_any_node_is_still_a_valid_file() {
    let scene = ExportScene {
        name: String::new(),
        rate: RATE,
        times: vec![0],
        baked: true,
        background: [0.0; 3],
        roots: Vec::new(),
        nodes: Vec::new(),
        meshes: Vec::new(),
    };
    let (bytes, report) = scene_to_fbx(&scene, &FbxOptions::default()).unwrap();
    let fbx = Fbx::checked(&bytes);
    assert!(fbx.objects().is_empty());
    assert!(fbx.connections().is_empty());
    assert_eq!(report.nodes, 0);
    let title = fbx
        .section("FBXHeaderExtension")
        .child("SceneInfo")
        .child("MetaData")
        .value("Title");
    assert_eq!(title.as_str(), "Limner scene");
}

#[test]
fn the_file_declares_axes_unit_and_time() {
    let stage = stage();
    let fbx = animated(&stage.doc, STAGE_LAST);
    let settings = fbx.section("GlobalSettings");
    // Y up, Z to the front, X to the right: right-handed, as Limner.
    for (name, value) in [
        ("UpAxis", 1),
        ("UpAxisSign", 1),
        ("FrontAxis", 2),
        ("FrontAxisSign", 1),
        ("CoordAxis", 0),
        ("CoordAxisSign", 1),
    ] {
        assert_eq!(settings.integer(name), value, "{name}");
    }
    // One unit is 100 centimetres: the numbers in the file are metres.
    assert_eq!(settings.number("UnitScaleFactor"), 100.0);
    assert_eq!(settings.number("OriginalUnitScaleFactor"), 100.0);
    assert_eq!(settings.integer("TimeMode"), 11, "24 frames per second");
    assert_eq!(settings.number("CustomFrameRate"), 24.0);
    assert_eq!(settings.integer("TimeSpanStart"), 0);
    assert_eq!(settings.integer("TimeSpanStop"), STAGE_LAST * KTIME_FRAME);

    let header = fbx.section("FBXHeaderExtension");
    assert!(header.value("Creator").as_str().contains("limner-export"));
    assert_eq!(
        header
            .child("SceneInfo")
            .child("MetaData")
            .value("Title")
            .as_str(),
        "Stage"
    );
    // One stack named after the document, spanning the range.
    let stacks = fbx.objects_of("AnimationStack");
    assert_eq!(stacks[0].name, "Stage");
    assert_eq!(
        stacks[0].record.integer("LocalStop"),
        STAGE_LAST * KTIME_FRAME
    );
    let take = fbx.section("Takes").child("Take");
    assert_eq!(take.values[0].as_str(), "Stage");
    assert_eq!(
        take.child("LocalTime").values[1].as_i64(),
        STAGE_LAST * KTIME_FRAME
    );

    // A rate FBX has no code for is a custom rate, and key times stay exact.
    let rate = FrameRate::whole(12);
    let options = ExportOptions::animated(0, rate.frame_start(6), rate);
    let fbx = Fbx::checked(&export_fbx(&stage.doc, &options, &FbxOptions::default()).unwrap());
    let settings = fbx.section("GlobalSettings");
    assert_eq!(settings.integer("TimeMode"), 14);
    assert_eq!(settings.number("CustomFrameRate"), 12.0);
    assert_eq!(settings.integer("TimeSpanStop"), 6 * KTIME_PER_SECOND / 12);
}

#[test]
fn models_mirror_the_layer_tree() {
    let stage = stage();
    let fbx = animated(&stage.doc, STAGE_LAST);
    let names = |parent: &Object| -> Vec<String> {
        fbx.child_models(parent.id)
            .iter()
            .map(|m| m.name.to_owned())
            .collect()
    };
    let root = fbx.model("Root");
    assert!(fbx.parent_of(root.id).is_none());
    assert_eq!(
        names(&root),
        ["Rig", "Loop", "Clip", "Ghost", "Bulb", "Torch", "Sun", "Shot"]
    );
    assert_eq!(names(&fbx.model("Rig")), ["Left arm"]);
    assert_eq!(names(&fbx.model("Loop")), ["Flicker"]);

    let kinds: Vec<(&str, &str)> = ["Root", "Rig", "Left arm", "Flicker", "Clip", "Bulb", "Shot"]
        .iter()
        .map(|name| {
            let model = fbx.model(name);
            (model.subtype, model.record.text("limnerKind"))
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("Null", "Sequence"),
            ("Null", "Group"),
            ("Mesh", "Paint"),
            // A frame-by-frame layer is an empty parent of its drawings.
            ("Null", "Paint"),
            ("Mesh", "Paint"),
            ("Light", "Light"),
            ("Camera", "Camera"),
        ]
    );
    let arm = fbx.model("Left arm");
    assert_eq!(arm.record.text("limnerLayerId"), stage.arm.0.to_string());
    assert_eq!(arm.record.property_flags("limnerLayerId"), "U");
    assert_eq!(arm.record.integer("Visibility Inheritance"), 1);
}

#[test]
fn a_straight_tube_round_trips_ring_for_ring() {
    for (brush, ring) in [(Brush::Cylinder, 7), (Brush::Ellipse, 7), (Brush::Cube, 4)] {
        let stroke = straight(0, brush, 5, [1.0, 2.0, -1.0]);
        let mut doc = Document::empty();
        let root = doc.root();
        paint(&mut doc, root, "Tube", vec![stroke.clone()]);
        let fbx = still(&doc);
        let model = fbx.model("Tube");
        let g = fbx.geometry_of(&model);
        assert_eq!(g.positions.len(), 5 * ring, "{brush:?}: one ring per point");
        assert_eq!(g.triangles.len(), 4 * ring * 2, "{brush:?}");
        assert_eq!(
            g.positions,
            rings(&stroke),
            "{brush:?}: the renderer's rings"
        );
        let scene = build_scene(&doc, &ExportOptions::still(0));
        let expected: Vec<[u32; 3]> = scene.meshes[0].indices.as_chunks::<3>().0.to_vec();
        assert_eq!(g.triangles, expected, "{brush:?}: the scene's triangles");

        // The stroke runs along X at y = 2, z = -1.
        let outward = |v: DVec3| DVec3::new(0.0, v.y - 2.0, v.z + 1.0);
        for t in &g.triangles {
            let [a, b, c] = t.map(|i| g.positions[i as usize]);
            assert!(
                (b - a).cross(c - a).dot(outward((a + b + c) / 3.0)) > 0.0,
                "{brush:?}: counter-clockwise seen from outside"
            );
        }
        for (position, normal) in g.positions.iter().zip(&g.normals) {
            assert!(normal.dot(outward(*position)) > 0.0, "{brush:?}");
        }
        // Colours are sRGB-encoded by default; alpha is not a colour and stays linear.
        for color in &g.colors {
            for (channel, linear) in [1.0, 0.5, 0.25].into_iter().enumerate() {
                assert!((color[channel] - srgb(linear)).abs() < 1e-6, "{brush:?}");
            }
            assert_eq!(color[3], 1.0);
        }
        assert_eq!(fbx.material_names(&model), ["unlit"]);
        assert!(g.slots.iter().all(|s| *s == 0));
    }
}

#[test]
fn colours_can_stay_linear_and_normals_can_be_left_out() {
    let mut doc = Document::empty();
    let root = doc.root();
    let color = [1.5f32, 0.5, 0.1];
    paint(
        &mut doc,
        root,
        "P",
        vec![straight_colored(0, Brush::Ribbon, 3, [0.0; 3], color, 0.75)],
    );
    let linear = FbxOptions {
        color_space: FbxColorSpace::Linear,
        ..FbxOptions::default()
    };
    let g = still_with(&doc, &linear).geometry_of(&still_with(&doc, &linear).model("P"));
    // Exactly the stored values, brighter than 1 included.
    let expected = [
        f64::from(color[0]),
        f64::from(color[1]),
        f64::from(color[2]),
        0.75,
    ];
    assert!(g.colors.iter().all(|c| *c == expected));
    // The default clamps: sRGB encoding is only defined up to 1.
    let fbx = still(&doc);
    let g = fbx.geometry_of(&fbx.model("P"));
    assert!((g.colors[0][0] - 1.0).abs() < 1e-6);
    assert!((g.colors[0][2] - srgb(f64::from(0.1f32))).abs() < 1e-6);
    assert_eq!(g.colors[0][3], 0.75);
    assert_eq!(g.normals.len(), g.positions.len());

    let bare = FbxOptions {
        normals: false,
        ..FbxOptions::default()
    };
    let with = export_fbx(&doc, &ExportOptions::still(0), &FbxOptions::default()).unwrap();
    let without = export_fbx(&doc, &ExportOptions::still(0), &bare).unwrap();
    assert!(without.len() < with.len());
    let fbx = Fbx::checked(&without);
    assert!(fbx.geometry_of(&fbx.model("P")).normals.is_empty());
}

#[test]
fn a_drawing_is_one_geometry_with_a_slot_per_kind_of_surface() {
    let mut doc = Document::empty();
    let root = doc.root();
    let color = [1.0, 0.5, 0.25];
    let strokes = vec![
        straight_colored(0, Brush::Cylinder, 3, [0.0, 0.0, 0.0], color, 1.0),
        straight_colored(1, Brush::Cylinder, 4, [0.0, 1.0, 0.0], color, 0.4),
        straight_colored(2, Brush::Ribbon, 5, [0.0, 2.0, 0.0], color, 1.0),
        straight_colored(3, Brush::Ribbon, 6, [0.0, 3.0, 0.0], color, 0.4),
        // A second opaque tube joins the first one's surface.
        straight_colored(4, Brush::Cube, 2, [0.0, 4.0, 0.0], color, 1.0),
    ];
    paint(&mut doc, root, "Mixed", strokes);
    let (bytes, report) =
        export_fbx_with_report(&doc, &ExportOptions::still(0), &FbxOptions::default()).unwrap();
    let fbx = Fbx::checked(&bytes);
    let model = fbx.model("Mixed");
    assert_eq!(fbx.objects_of("Geometry").len(), 1);
    // Opaque first, then see-through; single-sided before double-sided.
    assert_eq!(
        fbx.material_names(&model),
        [
            "unlit",
            "unlit double-sided",
            "unlit see-through",
            "unlit double-sided see-through"
        ]
    );
    let g = fbx.geometry_of(&model);
    let vertices = [3 * 7 + 2 * 4, 5 * 2, 4 * 7, 6 * 2];
    assert_eq!(g.positions.len(), vertices.iter().sum::<usize>());
    // Each triangle is in the slot of the surface its vertices came from.
    let mut first_vertex = 0;
    let mut counted = [0usize; 4];
    for (slot, count) in vertices.iter().enumerate() {
        let range = first_vertex..first_vertex + count;
        for (triangle, triangle_slot) in g.triangles.iter().zip(&g.slots) {
            if range.contains(&(triangle[0] as usize)) {
                assert!(triangle.iter().all(|i| range.contains(&(*i as usize))));
                assert_eq!(*triangle_slot, slot);
                counted[slot] += 1;
            }
        }
        let alpha = if slot >= 2 { f64::from(0.4f32) } else { 1.0 };
        assert!(g.colors[range].iter().all(|c| c[3] == alpha));
        first_vertex += count;
    }
    assert_eq!(counted, [2 * 14 + 8, 8, 3 * 14, 10]);

    assert_eq!(report.nodes, 2);
    assert_eq!(report.meshes, 4, "four surfaces, in one geometry");
    assert_eq!(report.vertices, g.positions.len());
    assert_eq!(report.triangles, g.triangles.len());
    assert_eq!((report.animation_frames, report.animation_channels), (0, 0));
    assert_eq!(report.bytes, bytes.len() as u64);

    // Materials say what they are for, since FBX cannot.
    let materials = fbx.objects_of("Material");
    assert_eq!(materials.len(), 4);
    let last = materials[3].record;
    assert_eq!(last.text("limnerShading"), "unlit");
    assert_eq!(last.text("limnerDoubleSided"), "true");
    assert_eq!(last.text("limnerSeeThrough"), "true");
    assert_eq!(last.vector("DiffuseColor"), DVec3::ONE);
    assert_eq!(last.value("ShadingModel").as_str(), "lambert");
}

#[test]
fn layer_opacity_is_multiplied_into_the_vertex_alpha() {
    let mut doc = Document::empty();
    let root = doc.root();
    let faded = group(&mut doc, root, "Faded");
    doc.layer_mut(faded).unwrap().opacity = 0.5;
    let child = paint(
        &mut doc,
        faded,
        "Child",
        vec![
            straight(0, Brush::Cylinder, 3, [0.0; 3]),
            straight_colored(1, Brush::Cylinder, 3, [0.0, 1.0, 0.0], [1.0; 3], 0.5),
        ],
    );
    {
        let layer = doc.layer_mut(child).unwrap();
        layer.opacity = 0.5;
        layer.as_paint_mut().unwrap().shading = Shading::Lit;
    }
    let fbx = still(&doc);
    let model = fbx.model("Child");
    let g = fbx.geometry_of(&model);
    // Opacity multiplies down the tree: 0.5 * 0.5, and again with the stroke's own.
    assert!(g.colors[..21].iter().all(|c| c[3] == 0.25));
    assert!(g.colors[21..].iter().all(|c| c[3] == 0.125));
    // Both surfaces now need blending and look the same, so they share one slot.
    assert_eq!(fbx.material_names(&model), ["lit see-through"]);
    assert!(g.slots.iter().all(|s| *s == 0));
}

#[test]
fn layers_that_show_the_same_drawing_share_one_geometry() {
    let mut doc = Document::empty();
    let root = doc.root();
    let first = paint(
        &mut doc,
        root,
        "First",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    let shared = doc.layer(first).unwrap().as_paint().unwrap().clone();
    let second = doc.new_layer(
        "Second",
        LayerKind::Paint(Paint {
            shading: Shading::Lit,
            ..shared
        }),
        0,
    );
    doc.insert_layer(second, root, usize::MAX).unwrap();
    let fbx = still(&doc);
    assert_eq!(fbx.objects_of("Geometry").len(), 1);
    let geometry = fbx.objects_of("Geometry")[0].id;
    let users: Vec<i64> = fbx
        .connections()
        .iter()
        .filter(|c| c.source == geometry)
        .map(|c| c.target)
        .collect();
    assert_eq!(users, [fbx.model("First").id, fbx.model("Second").id]);
    // The look still belongs to the layer.
    assert_eq!(fbx.material_names(&fbx.model("First")), ["unlit"]);
    assert_eq!(fbx.material_names(&fbx.model("Second")), ["lit"]);
}

#[test]
fn the_model_chain_reproduces_world_placement() {
    let (plain, _, _) = chain([Flip::None; 3]);
    let plain = still(&plain);
    let plain_triangles = plain.geometry_of(&plain.model("P")).triangles;

    for flips in FLIP_CASES {
        let (doc, layer, strokes) = chain(flips);
        let fbx = still(&doc);
        let model = fbx.model("P");
        let world = fbx.world_matrix(&model);
        let expected = doc.world_from_layer(layer);
        assert!(close(&world, &expected, 1e-9), "{flips:?}");
        let g = fbx.geometry_of(&model);
        // Vertices stay in layer space, exactly as tessellated.
        let all: Vec<DVec3> = strokes.iter().flat_map(rings).collect();
        assert_eq!(g.positions, all);
        // A mirror is a negative scale on the flip axis of that layer's own model.
        for (name, flip, scale) in [
            ("A", flips[0], 2.0),
            ("B", flips[1], 0.5),
            ("P", flips[2], 1.5),
        ] {
            assert_eq!(
                fbx.model(name).record.vector("Lcl Scaling"),
                flip.signs() * scale,
                "{flips:?} {name}"
            );
        }
        // With the hierarchy kept the winding is left for the consumer to handle.
        assert_eq!(g.triangles, plain_triangles, "{flips:?}");
    }
}

#[test]
fn baked_meshes_are_in_world_space_and_face_outward() {
    for flips in FLIP_CASES {
        let (doc, layer, strokes) = chain(flips);
        let options = ExportOptions {
            bake_transforms: true,
            ..ExportOptions::still(0)
        };
        let fbx = Fbx::checked(&export_fbx(&doc, &options, &FbxOptions::default()).unwrap());
        // The hierarchy is gone: one model at the root, with no transform of its own.
        let models = fbx.objects_of("Model");
        assert_eq!(models.len(), 1, "{flips:?}");
        let model = models[0];
        assert!(fbx.parent_of(model.id).is_none());
        assert!(close(&fbx.local_matrix(&model), &DAffine3::IDENTITY, 0.0));

        let world = doc.world_from_layer(layer);
        let g = fbx.geometry_of(&model);
        let expected: Vec<DVec3> = strokes
            .iter()
            .flat_map(rings)
            .map(|p| world.transform_point3(p))
            .collect();
        assert_eq!(g.positions.len(), expected.len());
        for (exported, expected) in g.positions.iter().zip(&expected) {
            assert!((*exported - *expected).length() < 1e-5, "{flips:?}");
        }
        // Winding agrees with the vertex normals, mirrored or not.
        for t in &g.triangles {
            let [a, b, c] = t.map(|i| g.positions[i as usize]);
            let normal: DVec3 = t.iter().map(|i| g.normals[*i as usize]).sum();
            assert!((b - a).cross(c - a).dot(normal) > 0.0, "{flips:?}");
        }
    }
}

#[test]
fn centimetres_scale_every_length_and_nothing_else() {
    let stage = stage();
    let centimetres = FbxOptions {
        unit: FbxUnit::Centimetres,
        ..FbxOptions::default()
    };
    let options = ExportOptions::animated(0, STAGE_LAST * FRAME, RATE);
    let metres = Fbx::checked(&export_fbx(&stage.doc, &options, &FbxOptions::default()).unwrap());
    let fbx = Fbx::checked(&export_fbx(&stage.doc, &options, &centimetres).unwrap());
    let settings = fbx.section("GlobalSettings");
    // FBX's own unit, so readers that ignore the declaration are right too.
    assert_eq!(settings.number("UnitScaleFactor"), 1.0);

    for name in ["Rig", "Left arm", "Clip", "Torch", "Shot"] {
        let (m, c) = (metres.model(name), fbx.model(name));
        assert!(
            (c.record.vector("Lcl Translation") - m.record.vector("Lcl Translation") * 100.0)
                .length()
                < 1e-9,
            "{name}"
        );
        assert_eq!(
            c.record.vector("Lcl Rotation"),
            m.record.vector("Lcl Rotation")
        );
        assert_eq!(
            c.record.vector("Lcl Scaling"),
            m.record.vector("Lcl Scaling")
        );
    }
    let (m, c) = (metres.model("Left arm"), fbx.model("Left arm"));
    let (gm, gc) = (metres.geometry_of(&m), fbx.geometry_of(&c));
    for (a, b) in gm.positions.iter().zip(&gc.positions) {
        assert!((*a * 100.0 - *b).length() < 1e-9);
    }
    assert_eq!(gm.normals, gc.normals);
    // Animated translations are lengths too.
    let time = 6 * KTIME_FRAME;
    assert!(
        (fbx.vector_at(&c, "Lcl Translation", time)
            - metres.vector_at(&m, "Lcl Translation", time) * 100.0)
            .length()
            < 1e-3
    );
    assert_eq!(
        fbx.vector_at(&c, "Lcl Scaling", time),
        metres.vector_at(&m, "Lcl Scaling", time)
    );
    // And so are a light's reach and a camera's clip planes.
    let reach = |file: &Fbx| {
        file.attribute_of(&file.model("Torch"))
            .record
            .number("FarAttenuationEnd")
    };
    assert_eq!((reach(&metres), reach(&fbx)), (6.0, 600.0));
    let near = |file: &Fbx| {
        file.attribute_of(&file.model("Shot"))
            .record
            .number("NearPlane")
    };
    assert!((near(&fbx) - near(&metres) * 100.0).abs() < 1e-6);
}

#[test]
fn keyed_transforms_become_sampled_curves() {
    let stage = stage();
    let (bytes, report) = export_fbx_with_report(
        &stage.doc,
        &ExportOptions::animated(0, STAGE_LAST * FRAME, RATE),
        &FbxOptions::default(),
    )
    .unwrap();
    let fbx = Fbx::checked(&bytes);
    let arm = fbx.model("Left arm");
    for k in 0..=STAGE_LAST {
        let frame = evaluate(&stage.doc, Clock::at(k * FRAME));
        let expected = frame.get(stage.arm).unwrap().local.to_affine();
        let got = fbx.local_matrix_at(&arm, k * KTIME_FRAME);
        assert!(
            close(&got, &expected, 1e-5),
            "frame {k}: {got:?} vs {expected:?}"
        );
    }
    for property in ["Lcl Translation", "Lcl Rotation", "Lcl Scaling"] {
        assert_eq!(arm.record.property_flags(property), "A+", "{property}");
        let curves = fbx.curves(arm.id, property).unwrap();
        for curve in curves.values() {
            assert!(curve.constant.iter().all(|c| !c), "{property}: linear keys");
            assert!(
                curve.times.iter().all(|t| t % KTIME_FRAME == 0),
                "{property}: keys sit exactly on frames"
            );
        }
    }
    // Runs where nothing changes are merged: before the first key (frames 0..=2) and
    // after the last (frames 10..=12) only the ends of the hold are written.
    let translation = fbx.curves(arm.id, "Lcl Translation").unwrap();
    let frames: Vec<i64> = translation["d|X"]
        .times
        .iter()
        .map(|t| t / KTIME_FRAME)
        .collect();
    let expected: Vec<i64> = [0, 2].into_iter().chain(3..=10).chain([12]).collect();
    assert_eq!(frames, expected);

    // The arm turns from 0.5 to 5 radians about Z, through half a turn. Unrolled, the
    // Z angle climbs to about 286 degrees instead of wrapping to -74.
    let z = &fbx.curves(arm.id, "Lcl Rotation").unwrap()["d|Z"];
    for pair in z.values.windows(2) {
        assert!((pair[1] - pair[0]).abs() < 60.0, "no jump: {pair:?}");
    }
    let last = *z.values.last().unwrap();
    assert!(
        (f64::from(last) - 5.0f64.to_degrees()).abs() < 1e-3,
        "ends at 5 radians, got {last} degrees"
    );
    // The rest pose is the first frame.
    let first = evaluate(&stage.doc, Clock::at(0))
        .get(stage.arm)
        .unwrap()
        .local
        .to_affine();
    assert!(close(&fbx.local_matrix(&arm), &first, 1e-9));

    // Nothing else in the scene moves.
    for name in ["Root", "Rig", "Loop", "Flicker", "Ghost", "Bulb", "Shot"] {
        let model = fbx.model(name);
        for property in [
            "Lcl Translation",
            "Lcl Rotation",
            "Lcl Scaling",
            "Visibility",
        ] {
            assert!(
                fbx.curves(model.id, property).is_none(),
                "{name} {property}"
            );
            assert_eq!(model.record.property_flags(property), "A");
        }
    }
    // Arm: T, R, S. Clip: S, Visibility. Three drawings: S, Visibility each.
    assert_eq!(report.animation_channels, 3 + 2 + 3 * 2);
    assert_eq!(fbx.objects_of("AnimationCurveNode").len(), 11);
    assert_eq!(fbx.objects_of("AnimationCurve").len(), 3 * 3 + 4 + 3 * 4);
    assert_eq!(report.animation_frames, 13);
}

/// The curve as a reader that ignores interpolation flags would play it.
fn as_linear(curve: &Curve) -> Curve {
    Curve {
        times: curve.times.clone(),
        values: curve.values.clone(),
        constant: vec![false; curve.times.len()],
    }
}

#[test]
fn a_clip_that_starts_mid_range_is_visibility_and_a_zero_scale() {
    let stage = stage();
    let fbx = animated(&stage.doc, STAGE_LAST);
    let clip = fbx.model("Clip");
    // Hidden on the first frame, which is also what a reader without animation sees.
    assert_eq!(clip.record.number("Visibility"), 0.0);
    assert_eq!(clip.record.vector("Lcl Scaling"), DVec3::ZERO);
    assert_eq!(clip.record.property_flags("Visibility"), "A+");
    // The layer's own rotation and position are untouched by hiding.
    let layer = stage.doc.layer(stage.clip).unwrap().transform;
    assert_eq!(clip.record.vector("Lcl Translation"), layer.translation);
    assert!(
        euler_to_quat(clip.record.vector("Lcl Rotation"))
            .dot(layer.rotation)
            .abs()
            > 1.0 - 1e-9
    );

    let visibility = &fbx.curves(clip.id, "Visibility").unwrap()["d|Visibility"];
    let scale = fbx.curves(clip.id, "Lcl Scaling").unwrap();
    // Both ends of every hold are keys: 0 0 | 1 1 | 0 0.
    let frames: Vec<i64> = visibility.times.iter().map(|t| t / KTIME_FRAME).collect();
    assert_eq!(
        frames,
        [
            0,
            CLIP_SHOWN.0 - 1,
            CLIP_SHOWN.0,
            CLIP_SHOWN.1,
            CLIP_SHOWN.1 + 1,
            STAGE_LAST
        ]
    );
    assert_eq!(visibility.values, [0.0, 0.0, 1.0, 1.0, 0.0, 0.0]);
    assert!(visibility.constant.iter().all(|c| *c), "stepped");
    for axis in ["d|X", "d|Y", "d|Z"] {
        assert_eq!(scale[axis].times, visibility.times);
        assert_eq!(scale[axis].values, [0.0, 0.0, 2.0, 2.0, 0.0, 0.0]);
        assert!(scale[axis].constant.iter().all(|c| *c), "stepped");
    }
    for k in 0..=STAGE_LAST {
        let shown = evaluate(&stage.doc, Clock::at(k * FRAME))
            .get(stage.clip)
            .unwrap()
            .in_clip;
        let time = k * KTIME_FRAME;
        assert_eq!(fbx.visibility_at(&clip, time) == 1.0, shown, "frame {k}");
        let expected = if shown { 2.0 } else { 0.0 };
        assert_eq!(scale["d|X"].sample(time), expected, "frame {k}");
        // A reader that interpolates linearly whatever the keys say gets the same
        // answer on every frame, because both ends of each hold are keys.
        assert_eq!(as_linear(&scale["d|X"]).sample(time), expected, "frame {k}");
        assert_eq!(
            as_linear(visibility).sample(time) == 1.0,
            shown,
            "frame {k}"
        );
    }
    // Between frames a stepped key holds; nothing shrinks or grows.
    assert_eq!(scale["d|X"].sample(3 * KTIME_FRAME + KTIME_FRAME / 2), 0.0);
    assert!(fbx.curves(clip.id, "Lcl Rotation").is_none());
}

#[test]
fn frame_by_frame_drawings_are_children_switched_on_and_off() {
    let stage = stage();
    let fbx = animated(&stage.doc, STAGE_LAST);
    let drawings = stage
        .doc
        .layer(stage.flicker)
        .unwrap()
        .as_paint()
        .unwrap()
        .drawings
        .clone();
    let parent = fbx.model("Flicker");
    let children = fbx.child_models(parent.id);
    let names: Vec<&str> = children.iter().map(|c| c.name).collect();
    assert_eq!(names, ["Flicker.000", "Flicker.001", "Flicker.002"]);
    for (ordinal, child) in children.iter().enumerate() {
        assert_eq!(child.subtype, "Mesh");
        assert_eq!(child.record.text("limnerDrawing"), ordinal.to_string());
        assert_eq!(
            child.record.text("limnerDrawingId"),
            drawings[ordinal].0.to_string()
        );
        assert_eq!(
            child.record.text("limnerLayerId"),
            stage.flicker.0.to_string()
        );
        // Each drawing kept its own geometry.
        assert_eq!(fbx.geometry_of(child).positions.len(), (2 + ordinal) * 7);
        for property in ["Lcl Translation", "Lcl Rotation"] {
            assert!(fbx.curves(child.id, property).is_none());
        }
    }
    let mut seen = Vec::new();
    for k in 0..=STAGE_LAST {
        let frame = evaluate(&stage.doc, Clock::at(k * FRAME));
        let Content::Paint { drawing, .. } = frame.get(stage.flicker).unwrap().content else {
            panic!("a paint layer");
        };
        let expected = drawings.iter().position(|d| *d == drawing).unwrap();
        seen.push(expected);
        for (ordinal, child) in children.iter().enumerate() {
            let on = if ordinal == expected { 1.0 } else { 0.0 };
            let time = k * KTIME_FRAME;
            assert_eq!(fbx.visibility_at(child, time), on, "frame {k} #{ordinal}");
            assert_eq!(
                fbx.vector_at(child, "Lcl Scaling", time),
                DVec3::splat(on),
                "frame {k} #{ordinal}"
            );
        }
    }
    assert_eq!(seen, [0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0]);
    // Rest state: the drawing of the first frame is the only one shown.
    assert_eq!(children[0].record.vector("Lcl Scaling"), DVec3::ONE);
    assert_eq!(children[1].record.vector("Lcl Scaling"), DVec3::ZERO);
    assert_eq!(children[1].record.number("Visibility"), 0.0);

    // In a still, a frame-by-frame layer is just the drawing it shows.
    let fbx = still(&stage.doc);
    let model = fbx.model("Flicker");
    assert_eq!(model.subtype, "Mesh");
    assert!(fbx.child_models(model.id).is_empty());
    assert!(fbx.objects_of("AnimationStack").is_empty());
    assert!(
        fbx.find_model("Clip").is_none(),
        "nothing to show at frame 0"
    );
}

#[test]
fn a_hidden_group_in_a_still_is_scaled_to_zero() {
    let mut doc = Document::empty();
    let root = doc.root();
    let later = group(&mut doc, root, "Later");
    {
        let layer = doc.layer_mut(later).unwrap();
        layer.anim.visibility = Track::from_keys([Key::step(4 * FRAME, true)]);
        layer.anim.offset.clear();
    }
    let fbx = still(&doc);
    let model = fbx.model("Later");
    assert_eq!(model.record.number("Visibility"), 0.0);
    assert_eq!(model.record.vector("Lcl Scaling"), DVec3::ZERO);
    assert_eq!(model.record.property_flags("Visibility"), "A");
}

#[test]
fn lights_point_the_way_fbx_expects() {
    let stage = stage();
    let fbx = still(&stage.doc);
    let property = |name: &str, key: &str| fbx.attribute_of(&fbx.model(name)).record.number(key);
    let integer = |name: &str, key: &str| fbx.attribute_of(&fbx.model(name)).record.integer(key);

    assert_eq!(integer("Bulb", "LightType"), 0, "point");
    assert_eq!(integer("Torch", "LightType"), 2, "spot");
    assert_eq!(integer("Sun", "LightType"), 1, "directional");
    let bulb = fbx.attribute_of(&fbx.model("Bulb"));
    assert_eq!(bulb.subtype, "Light");
    assert_eq!(bulb.record.value("TypeFlags").as_str(), "Light");
    assert!((bulb.record.vector("Color") - DVec3::new(1.0, 0.5, 0.25)).length() < 1e-7);
    // FBX intensity is a percentage.
    assert_eq!(property("Bulb", "Intensity"), 300.0);
    assert_eq!(property("Sun", "Intensity"), 75.0);
    // The reach is in world units: 4 under the layer's scale of 2.
    assert_eq!(property("Bulb", "FarAttenuationEnd"), 8.0);
    assert!(fbx
        .attribute_of(&fbx.model("Sun"))
        .record
        .property("FarAttenuationEnd")
        .is_none());
    assert_eq!(integer("Bulb", "CastShadows"), 0);
    assert_eq!(integer("Torch", "CastShadows"), 1);
    // Colours above 1 are normalised and the excess moves into the intensity.
    let torch = fbx.attribute_of(&fbx.model("Torch"));
    assert!((torch.record.vector("Color") - DVec3::new(1.0, 0.5, 0.25)).length() < 1e-7);
    assert_eq!(property("Torch", "Intensity"), 300.0);
    // FBX measures the whole cone; Limner's 30 degrees are half of it.
    assert_eq!(property("Torch", "OuterAngle"), 60.0);
    assert_eq!(property("Torch", "InnerAngle"), 45.0);
    assert_eq!(fbx.model("Torch").record.text("limnerEnergy"), "1.5");
    assert_eq!(fbx.model("Torch").record.text("limnerRange"), "6");

    // An FBX light shines along its local -Y; a Limner light along its local -Z.
    for (name, layer) in [
        ("Torch", stage.torch),
        ("Sun", stage.sun),
        ("Bulb", stage.bulb),
    ] {
        let world = fbx.world_matrix(&fbx.model(name));
        let expected = stage.doc.world_from_layer(layer);
        let shines = world.transform_vector3(DVec3::NEG_Y).normalize();
        let wanted = expected.transform_vector3(DVec3::NEG_Z).normalize();
        assert!((shines - wanted).length() < 1e-9, "{name}");
        assert!(
            (world.translation - expected.translation).length() < 1e-9,
            "{name}"
        );
    }
    assert_eq!(
        fbx.model("Bulb").record.vector("Lcl Scaling"),
        DVec3::splat(2.0)
    );
}

#[test]
fn cameras_carry_the_field_of_view_and_look_the_way_fbx_expects() {
    let stage = stage();
    let fbx = still(&stage.doc);
    let model = fbx.model("Shot");
    let camera = fbx.attribute_of(&model);
    assert_eq!(camera.subtype, "Camera");
    assert_eq!(camera.record.value("TypeFlags").as_str(), "Camera");
    let r = camera.record;
    // The lens: focal length in millimetres over a film back in inches.
    assert_eq!(r.integer("ApertureMode"), 3, "by focal length");
    let film_height = r.number("FilmHeight") * 25.4;
    let fov = 2.0
        * (film_height / (2.0 * r.number("FocalLength")))
            .atan()
            .to_degrees();
    assert!(
        (fov - 50.0).abs() < 1e-9,
        "vertical field of view, got {fov}"
    );
    assert!((r.number("FilmWidth") / r.number("FilmHeight") - 16.0 / 9.0).abs() < 1e-6);
    assert!((r.number("FilmAspectRatio") - 16.0 / 9.0).abs() < 1e-6);
    assert!((r.number("FieldOfViewY") - 50.0).abs() < 1e-9);
    let horizontal = 2.0
        * ((25.0f64.to_radians()).tan() * 16.0 / 9.0)
            .atan()
            .to_degrees();
    assert!((r.number("FieldOfView") - horizontal).abs() < 1e-5);
    assert!((r.number("FieldOfViewX") - horizontal).abs() < 1e-5);
    assert_eq!(
        (r.number("AspectWidth"), r.number("AspectHeight")),
        (1920.0, 1080.0)
    );
    assert!((r.number("NearPlane") - 0.01).abs() < 1e-8);
    assert_eq!(r.number("FarPlane"), 1000.0);

    // An FBX camera looks along its local +X with +Y up; Limner's along -Z.
    let world = fbx.world_matrix(&model);
    let expected = stage.doc.world_from_layer(stage.shot);
    let looks = world.transform_vector3(DVec3::X);
    let up = world.transform_vector3(DVec3::Y);
    assert!((looks - expected.transform_vector3(DVec3::NEG_Z)).length() < 1e-9);
    assert!((up - expected.transform_vector3(DVec3::Y)).length() < 1e-9);
    assert_eq!(
        model.record.vector("Lcl Translation"),
        DVec3::new(0.0, 1.6, 4.0)
    );
}

#[test]
fn lights_and_cameras_can_be_left_out() {
    let stage = stage();
    let options = ExportOptions {
        lights: false,
        cameras: false,
        ..ExportOptions::still(0)
    };
    let fbx = Fbx::checked(&export_fbx(&stage.doc, &options, &FbxOptions::default()).unwrap());
    assert!(fbx.objects_of("NodeAttribute").is_empty());
    for name in ["Bulb", "Torch", "Sun", "Shot"] {
        assert!(fbx.find_model(name).is_none());
    }
    assert!(fbx.find_model("Rig").is_some());
}

#[test]
fn the_stage_arm_keeps_its_strokes() {
    let stage = stage();
    let fbx = animated(&stage.doc, STAGE_LAST);
    let model = fbx.model("Left arm");
    let g = fbx.geometry_of(&model);
    let strokes = arm_strokes();
    let all: Vec<DVec3> = strokes.iter().flat_map(rings).collect();
    assert_eq!(g.positions, all);
    assert_eq!(
        fbx.material_names(&model),
        ["unlit", "unlit double-sided see-through"]
    );
    // The half-faded lit layer blends.
    assert_eq!(fbx.material_names(&fbx.model("Ghost")), ["lit see-through"]);
    assert!(fbx
        .geometry_of(&fbx.model("Ghost"))
        .colors
        .iter()
        .all(|c| c[3] == 0.5));
}

#[test]
fn the_reader_rejects_damaged_files() {
    let stage = stage();
    let good = export_fbx(
        &stage.doc,
        &ExportOptions::animated(0, STAGE_LAST * FRAME, RATE),
        &FbxOptions::default(),
    )
    .unwrap();
    Fbx::checked(&good);
    let find = |needle: &[u8]| {
        good.windows(needle.len())
            .position(|w| w == needle)
            .unwrap_or_else(|| panic!("{needle:?} is in the file"))
    };
    let breaks = |what: &str, change: &dyn Fn(&mut Vec<u8>)| {
        let mut bad = good.clone();
        change(&mut bad);
        let result = std::panic::catch_unwind(|| {
            Fbx::checked(&bad);
        });
        assert!(result.is_err(), "the reader accepted: {what}");
    };
    breaks("a wrong end offset", &|b| b[27] = b[27].wrapping_add(1));
    breaks("a wrong value count", &|b| b[31] = b[31].wrapping_add(1));
    breaks("a different version in the footer", &|b| {
        let at = b.len() - 16 - 120 - 4;
        b[at] ^= 1;
    });
    breaks("a damaged footer magic", &|b| {
        let last = b.len() - 1;
        b[last] ^= 1;
    });
    breaks("a truncated file", &|b| b.truncate(b.len() - 1));
    breaks("bytes after the footer", &|b| b.push(0));
    // The array header follows the record name: type, count, encoding, byte length.
    let vertices = find(b"Vertices") + 8;
    assert_eq!(good[vertices], b'd');
    breaks("a compressed array", &|b| b[vertices + 5] = 1);
    breaks("an array count that disagrees with its bytes", &|b| {
        b[vertices + 1] = b[vertices + 1].wrapping_add(1)
    });
    // The third index of the first triangle is the complemented one.
    let polygons = find(b"PolygonVertexIndex") + 18;
    assert_eq!(good[polygons], b'i');
    breaks("a polygon without its end marker", &|b| {
        let at = polygons + 13 + 8;
        let value = i32::from_le_bytes(b[at..at + 4].try_into().unwrap());
        b[at..at + 4].copy_from_slice(&(!value).to_le_bytes());
    });
    // A key time that goes backwards.
    let keys = find(b"KeyTime") + 7;
    assert_eq!(good[keys], b'l');
    breaks("key times that do not increase", &|b| {
        let at = keys + 13 + 8;
        b[at..at + 8].copy_from_slice(&(-1i64).to_le_bytes());
    });
    // A connection to an object that does not exist.
    let connection = find(b"\x01CS\x02\x00\x00\x00OOL") + 10;
    breaks("a dangling connection", &|b| b[connection + 7] ^= 0x40);
}

#[test]
fn writing_a_file_and_reporting_errors() {
    let stage = stage();
    let dir = std::env::temp_dir().join(format!("limner-fbx-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stage.fbx");
    let options = ExportOptions::animated(0, STAGE_LAST * FRAME, RATE);
    let report = write_fbx(&stage.doc, &path, &options, &FbxOptions::default()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(report.bytes, bytes.len() as u64);
    Fbx::checked(&bytes);
    // The same input always gives the same bytes: the creation date is fixed.
    assert_eq!(
        export_fbx(&stage.doc, &options, &FbxOptions::default()).unwrap(),
        bytes
    );
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(matches!(
        write_fbx(
            &stage.doc,
            &dir.join("nowhere").join("stage.fbx"),
            &options,
            &FbxOptions::default()
        ),
        Err(ExportError::Io { .. })
    ));
    assert!(matches!(
        export_fbx(
            &stage.doc,
            &ExportOptions::animated(10 * FRAME, 0, RATE),
            &FbxOptions::default()
        ),
        Err(ExportError::InvalidRange { .. })
    ));
    assert!(matches!(
        export_fbx(
            &stage.doc,
            &ExportOptions::animated(0, FRAME, FrameRate::new(0, 1)),
            &FbxOptions::default()
        ),
        Err(ExportError::InvalidRate { .. })
    ));
}
