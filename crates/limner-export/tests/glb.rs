//! glTF export of stills: container, geometry, materials, transforms, lights, cameras.

mod support;

use limner_core::glam::{DVec3, Vec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Brush, Camera, Document, LayerId, LayerKind, Light, LightKind, Shading, Stroke,
};
use limner_core::tess::ring_positions;
use limner_export::{
    export_glb, export_glb_with_report, write_glb, ExportError, ExportOptions, GltfOptions,
};
use support::stage::{chain, FLIP_CASES};
use support::{group, paint, straight, straight_colored, Glb, FRAME, RATE};

fn still(doc: &Document) -> Glb {
    let bytes = export_glb(doc, &ExportOptions::still(0), &GltfOptions::default()).unwrap();
    Glb::checked(&bytes)
}

fn rings(stroke: &Stroke) -> Vec<Vec3> {
    let mut out = Vec::new();
    for i in 0..stroke.len() {
        ring_positions(stroke.points(), i, stroke.brush, &mut out);
    }
    out
}

#[test]
fn an_empty_document_is_still_a_valid_file() {
    let glb = still(&Document::empty());
    assert_eq!(glb.list("nodes").len(), 1, "the root layer");
    assert!(glb.bin.is_empty());
    assert!(glb.json.get("buffers").is_none());
    assert!(glb.json.get("meshes").is_none());
    assert!(glb.json.get("animations").is_none());
    assert!(glb.json.get("extensionsUsed").is_none());
}

#[test]
fn asset_scene_and_node_metadata() {
    let mut doc = Document::empty();
    doc.settings.title = "Lantern".to_owned();
    let root = doc.root();
    let body = group(&mut doc, root, "Body");
    let arm = paint(
        &mut doc,
        body,
        "Arm",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    let glb = still(&doc);
    assert_eq!(glb.json["scenes"][0]["name"], "Lantern");
    assert!(glb.json["asset"]["generator"]
        .as_str()
        .unwrap()
        .contains("limner-export"));
    let node = glb.node(glb.node_named("Arm"));
    assert_eq!(node["extras"]["limner"]["layerId"], arm.0);
    assert_eq!(node["extras"]["limner"]["kind"], "Paint");
    assert_eq!(
        glb.node(glb.node_named("Root"))["extras"]["limner"]["kind"],
        "Sequence"
    );
    assert_eq!(
        glb.parent_of(glb.node_named("Arm")),
        Some(glb.node_named("Body"))
    );
    assert_eq!(
        glb.parent_of(glb.node_named("Body")),
        Some(glb.node_named("Root"))
    );
    assert_eq!(glb.json["scenes"][0]["nodes"][0], glb.node_named("Root"));
    // An untitled document still gets a scene name.
    doc.settings.title.clear();
    assert_eq!(still(&doc).json["scenes"][0]["name"], "Limner scene");
}

#[test]
fn a_straight_tube_exports_ring_for_ring() {
    for (brush, ring) in [(Brush::Cylinder, 7), (Brush::Ellipse, 7), (Brush::Cube, 4)] {
        let stroke = straight(0, brush, 5, [1.0, 2.0, -1.0]);
        let mut doc = Document::empty();
        let root = doc.root();
        paint(&mut doc, root, "Tube", vec![stroke.clone()]);
        let glb = still(&doc);
        let primitives = glb.primitives(glb.node_named("Tube"));
        assert_eq!(primitives.len(), 1);
        let p = &primitives[0];
        assert_eq!(p.positions.len(), 5 * ring, "{brush:?}: one ring per point");
        assert_eq!(
            p.indices.len() / 3,
            4 * ring * 2,
            "{brush:?}: two triangles per side"
        );
        assert_eq!(
            p.positions,
            rings(&stroke),
            "{brush:?}: the renderer's rings"
        );

        // The stroke runs along X at y = 2, z = -1.
        let outward = |v: Vec3| Vec3::new(0.0, v.y - 2.0, v.z + 1.0);
        for [a, b, c] in p.triangles() {
            let face = (b - a).cross(c - a);
            assert!(
                face.dot(outward((a + b + c) / 3.0)) > 0.0,
                "{brush:?}: counter-clockwise seen from outside"
            );
        }
        for (position, normal) in p.positions.iter().zip(&p.normals) {
            assert!(
                normal.dot(outward(*position)) > 0.0,
                "{brush:?}: normals point out"
            );
        }
        // Tubes are single-sided, opaque and unlit.
        assert!(p.material.get("doubleSided").is_none());
        assert!(p.material.get("alphaMode").is_none());
        assert!(p.material["extensions"]
            .get("KHR_materials_unlit")
            .is_some());
        assert_eq!(glb.extensions_used(), vec!["KHR_materials_unlit"]);
    }
}

#[test]
fn a_ribbon_exports_as_a_double_sided_strip() {
    for brush in [Brush::Ribbon, Brush::Line] {
        let stroke = straight(0, brush, 5, [1.0, 2.0, -1.0]);
        let mut doc = Document::empty();
        let root = doc.root();
        paint(&mut doc, root, "Ribbon", vec![stroke.clone()]);
        let glb = still(&doc);
        let primitives = glb.primitives(glb.node_named("Ribbon"));
        assert_eq!(primitives.len(), 1);
        let p = &primitives[0];
        assert_eq!(p.positions.len(), 10);
        assert_eq!(p.indices.len() / 3, 8);
        assert_eq!(p.positions, rings(&stroke));
        for [a, b, c] in p.triangles() {
            assert!(
                (b - a).cross(c - a).normalize().dot(Vec3::Y) > 0.999,
                "faces the stored normal"
            );
        }
        assert!(p.normals.iter().all(|n| (*n - Vec3::Y).length() < 1e-5));
        assert_eq!(p.material["doubleSided"], true);
        assert!(p.material.get("alphaMode").is_none());
    }
}

#[test]
fn strokes_are_split_by_sidedness_and_transparency() {
    let mut doc = Document::empty();
    let root = doc.root();
    let color = [1.0, 0.5, 0.25];
    paint(
        &mut doc,
        root,
        "Mixed",
        vec![
            straight_colored(0, Brush::Cylinder, 3, [0.0, 0.0, 0.0], color, 1.0),
            straight_colored(1, Brush::Cylinder, 4, [0.0, 1.0, 0.0], color, 0.4),
            straight_colored(2, Brush::Ribbon, 5, [0.0, 2.0, 0.0], color, 1.0),
            straight_colored(3, Brush::Ribbon, 6, [0.0, 3.0, 0.0], color, 0.4),
            // A second opaque tube joins the first one's mesh.
            straight_colored(4, Brush::Cube, 2, [0.0, 4.0, 0.0], color, 1.0),
        ],
    );
    let glb = still(&doc);
    let primitives = glb.primitives(glb.node_named("Mixed"));
    // Opaque first, then see-through; single-sided before double-sided.
    let summary: Vec<(usize, bool, bool)> = primitives
        .iter()
        .map(|p| {
            (
                p.positions.len(),
                p.material.get("doubleSided").is_some(),
                p.material.get("alphaMode").is_some(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            (3 * 7 + 2 * 4, false, false),
            (5 * 2, true, false),
            (4 * 7, false, true),
            (6 * 2, true, true),
        ]
    );
    for p in &primitives {
        let see_through = p.material.get("alphaMode").is_some();
        if see_through {
            assert_eq!(p.material["alphaMode"], "BLEND");
        }
        for c in &p.colors {
            // Colour is linear, not sRGB-encoded: 0.5 stays 0.5.
            for (channel, expected) in color.iter().enumerate() {
                assert!((c[channel] - expected).abs() < 1e-4);
            }
            let alpha = if see_through { 0.4 } else { 1.0 };
            assert!((c[3] - alpha).abs() < 1e-4);
        }
    }
    assert_eq!(glb.list("materials").len(), 4);
}

#[test]
fn shading_picks_the_material_model() {
    let mut doc = Document::empty();
    let root = doc.root();
    let lit = paint(
        &mut doc,
        root,
        "Lit",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    doc.layer_mut(lit).unwrap().as_paint_mut().unwrap().shading = Shading::Lit;
    let glb = still(&doc);
    let material = &glb.primitives(glb.node_named("Lit"))[0].material;
    assert!(material.get("extensions").is_none(), "plain PBR");
    assert_eq!(material["pbrMetallicRoughness"]["metallicFactor"], 0.0);
    assert!(glb.extensions_used().is_empty());

    paint(
        &mut doc,
        root,
        "Unlit",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    let glb = still(&doc);
    assert_eq!(glb.list("materials").len(), 2);
    assert_eq!(glb.extensions_used(), vec!["KHR_materials_unlit"]);
    // Two layers with the same look share one material.
    paint(
        &mut doc,
        root,
        "Unlit 2",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    assert_eq!(still(&doc).list("materials").len(), 2);
}

#[test]
fn layer_opacity_becomes_material_alpha() {
    let mut doc = Document::empty();
    let root = doc.root();
    let faded = group(&mut doc, root, "Faded");
    doc.layer_mut(faded).unwrap().opacity = 0.5;
    let child = paint(
        &mut doc,
        faded,
        "Child",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    doc.layer_mut(child).unwrap().opacity = 0.5;
    paint(
        &mut doc,
        root,
        "Solid",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    let glb = still(&doc);
    let material = &glb.primitives(glb.node_named("Child"))[0].material;
    assert_eq!(
        material["pbrMetallicRoughness"]["baseColorFactor"],
        serde_json::json!([1.0, 1.0, 1.0, 0.25]),
        "opacity multiplies down the tree"
    );
    assert_eq!(material["alphaMode"], "BLEND");
    let solid = &glb.primitives(glb.node_named("Solid"))[0].material;
    assert!(solid["pbrMetallicRoughness"]
        .get("baseColorFactor")
        .is_none());
    assert!(solid.get("alphaMode").is_none());
}

#[test]
fn index_width_follows_the_vertex_count() {
    let index_type = |points: usize| {
        let mut doc = Document::empty();
        let root = doc.root();
        paint(
            &mut doc,
            root,
            "P",
            vec![straight(0, Brush::Cylinder, points, [0.0; 3])],
        );
        let glb = still(&doc);
        let mesh = &glb.list("meshes")[0]["primitives"][0];
        let indices = mesh["indices"].as_u64().unwrap() as usize;
        glb.list("accessors")[indices]["componentType"]
            .as_u64()
            .unwrap()
    };
    assert_eq!(index_type(9_362), 5123, "65,534 vertices fit 16 bits");
    assert_eq!(index_type(9_363), 5125, "65,541 vertices need 32 bits");
}

#[test]
fn normals_can_be_left_out() {
    let mut doc = Document::empty();
    let root = doc.root();
    paint(
        &mut doc,
        root,
        "P",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    let extra = GltfOptions {
        normals: false,
        ..GltfOptions::default()
    };
    let with = export_glb(&doc, &ExportOptions::still(0), &GltfOptions::default()).unwrap();
    let without = export_glb(&doc, &ExportOptions::still(0), &extra).unwrap();
    let glb = Glb::checked(&without);
    assert!(glb.primitives(glb.node_named("P"))[0].normals.is_empty());
    assert!(without.len() < with.len());
}

#[test]
fn the_node_chain_reproduces_world_placement() {
    let (plain, _, _) = chain([Flip::None; 3]);
    let plain = still(&plain);
    let plain_indices: Vec<Vec<u32>> = plain
        .primitives(plain.node_named("P"))
        .into_iter()
        .map(|p| p.indices)
        .collect();

    for flips in FLIP_CASES {
        let (doc, layer, strokes) = chain(flips);
        let glb = still(&doc);
        let node = glb.node_named("P");
        let world = glb.world_matrix(node);
        let expected = doc.world_from_layer(layer);
        let primitives = glb.primitives(node);
        assert_eq!(primitives.len(), 2);
        for (primitive, stroke) in primitives.iter().zip(&strokes) {
            // Vertices stay in layer space, exactly as tessellated.
            assert_eq!(primitive.positions, rings(stroke));
            for position in &primitive.positions {
                let via_nodes = world.transform_point3(position.as_dvec3());
                let via_document = expected.transform_point3(position.as_dvec3());
                assert!(
                    (via_nodes - via_document).length() < 1e-5,
                    "{flips:?}: {via_nodes:?} vs {via_document:?}"
                );
            }
        }
        // A mirror is a negative scale on the flip axis of that layer's own node.
        for (name, flip, scale) in [
            ("A", flips[0], 2.0),
            ("B", flips[1], 0.5),
            ("P", flips[2], 1.5),
        ] {
            let expected = (flip.signs() * scale).to_array();
            assert_eq!(
                glb.scale(glb.node_named(name)),
                expected,
                "{flips:?} {name}"
            );
        }
        // With the hierarchy kept the winding is left for the consumer to handle.
        let indices: Vec<Vec<u32>> = primitives.into_iter().map(|p| p.indices).collect();
        assert_eq!(indices, plain_indices, "{flips:?}");
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
        let glb = Glb::checked(&export_glb(&doc, &options, &GltfOptions::default()).unwrap());
        // The hierarchy is gone: one root node, with no transform of its own.
        assert_eq!(glb.list("nodes").len(), 1, "{flips:?}");
        let node = glb.node_named("P");
        assert!(glb.parent_of(node).is_none());
        for key in ["translation", "rotation", "scale"] {
            assert!(glb.node(node).get(key).is_none(), "{flips:?}: no {key}");
        }

        let world = doc.world_from_layer(layer);
        let primitives = glb.primitives(node);
        for (primitive, stroke) in primitives.iter().zip(&strokes) {
            let expected: Vec<Vec3> = rings(stroke)
                .iter()
                .map(|p| world.transform_point3(p.as_dvec3()).as_vec3())
                .collect();
            assert_eq!(primitive.positions.len(), expected.len());
            for (exported, expected) in primitive.positions.iter().zip(&expected) {
                assert!(
                    (*exported - *expected).length() < 1e-5,
                    "{flips:?}: {exported:?} vs {expected:?}"
                );
            }
            // Winding agrees with the vertex normals, mirrored or not.
            for triangle in primitive.indices.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|i| primitive.positions[triangle[i] as usize]);
                let normal: Vec3 = triangle
                    .iter()
                    .map(|i| primitive.normals[*i as usize])
                    .sum();
                assert!(
                    (b - a).cross(c - a).dot(normal) > 0.0,
                    "{flips:?}: winding matches the normals"
                );
            }
        }

        // And for the tube, both really point away from the centre line.
        let tube = &primitives[0];
        let points = strokes[0].points();
        let start = world
            .transform_point3(points[0].position().as_dvec3())
            .as_vec3();
        let end = world
            .transform_point3(points[points.len() - 1].position().as_dvec3())
            .as_vec3();
        let axis = (end - start).normalize();
        let outward = |v: Vec3| (v - start) - axis * (v - start).dot(axis);
        for [a, b, c] in tube.triangles() {
            assert!(
                (b - a).cross(c - a).dot(outward((a + b + c) / 3.0)) > 0.0,
                "{flips:?}: counter-clockwise seen from outside after baking"
            );
        }
        for (position, normal) in tube.positions.iter().zip(&tube.normals) {
            assert!(normal.dot(outward(*position)) > 0.0, "{flips:?}");
        }
    }
}

fn light_layer(doc: &mut Document, parent: LayerId, name: &str, light: Light) -> LayerId {
    let layer = doc.new_layer(name, LayerKind::Light(light), 0);
    doc.insert_layer(layer, parent, usize::MAX).unwrap()
}

fn lit_scene() -> Document {
    let mut doc = Document::empty();
    doc.settings.camera_resolution = [1920, 1080];
    let root = doc.root();
    let rig = group(&mut doc, root, "Rig");
    doc.layer_mut(rig).unwrap().transform = Xform::from_scale(2.0);
    light_layer(
        &mut doc,
        rig,
        "Bulb",
        Light {
            kind: LightKind::Point,
            color: [1.0, 0.5, 0.25],
            energy: 3.0,
            range: 4.0,
            ..Light::default()
        },
    );
    light_layer(
        &mut doc,
        root,
        "Torch",
        Light {
            kind: LightKind::Spot,
            color: [2.0, 1.0, 0.5],
            energy: 1.5,
            range: 6.0,
            spot_angle: 30.0,
            spot_blend: 0.25,
            shadows: true,
        },
    );
    light_layer(
        &mut doc,
        root,
        "Sun",
        Light {
            kind: LightKind::Directional,
            energy: 0.75,
            ..Light::default()
        },
    );
    let mut camera = doc.new_layer("Shot", LayerKind::Camera(Camera { fov: 60.0 }), 0);
    camera.transform = Xform::from_translation(DVec3::new(0.0, 1.6, 4.0));
    doc.insert_layer(camera, root, usize::MAX).unwrap();
    doc
}

#[test]
fn lights_carry_their_parameters() {
    let glb = still(&lit_scene());
    assert_eq!(glb.extensions_used(), vec!["KHR_lights_punctual"]);
    let lights = &glb.json["extensions"]["KHR_lights_punctual"]["lights"];
    let light_of = |name: &str| {
        let node = glb.node(glb.node_named(name));
        let index = node["extensions"]["KHR_lights_punctual"]["light"]
            .as_u64()
            .unwrap() as usize;
        lights[index].clone()
    };
    let close = |value: &serde_json::Value, expected: f64| {
        (value.as_f64().unwrap() - expected).abs() < 1e-6
    };

    let bulb = light_of("Bulb");
    assert_eq!(bulb["type"], "point");
    assert_eq!(bulb["color"], serde_json::json!([1.0, 0.5, 0.25]));
    assert_eq!(bulb["intensity"], 3.0);
    assert_eq!(
        bulb["range"], 8.0,
        "range is in world metres: 4 under a scale of 2"
    );
    assert!(bulb.get("spot").is_none());
    assert_eq!(bulb["extras"]["limner"]["range"], 4.0);

    let torch = light_of("Torch");
    assert_eq!(torch["type"], "spot");
    // Colours above 1 are normalised and the excess moves into the intensity.
    assert_eq!(torch["color"], serde_json::json!([1.0, 0.5, 0.25]));
    assert_eq!(torch["intensity"], 3.0);
    assert_eq!(torch["range"], 6.0);
    assert!(close(&torch["spot"]["outerConeAngle"], 30f64.to_radians()));
    assert!(close(
        &torch["spot"]["innerConeAngle"],
        22.5f64.to_radians()
    ));
    assert_eq!(torch["extras"]["limner"]["shadows"], true);
    assert_eq!(torch["extras"]["limner"]["energy"], 1.5);
    assert_eq!(torch["extras"]["limner"]["spotAngle"], 30.0);
    assert_eq!(torch["extras"]["limner"]["spotBlend"], 0.25);

    let sun = light_of("Sun");
    assert_eq!(sun["type"], "directional");
    assert_eq!(sun["intensity"], 0.75);
    assert!(
        sun.get("range").is_none(),
        "directional lights have no range"
    );
}

#[test]
fn a_hard_edged_spot_still_has_a_valid_cone() {
    let mut doc = Document::empty();
    let root = doc.root();
    light_layer(
        &mut doc,
        root,
        "Hard",
        Light {
            kind: LightKind::Spot,
            spot_angle: 120.0,
            spot_blend: 0.0,
            ..Light::default()
        },
    );
    // The validator checks inner < outer <= 90 degrees.
    still(&doc);
}

#[test]
fn cameras_carry_field_of_view_and_aspect() {
    let glb = still(&lit_scene());
    let node = glb.node_named("Shot");
    let camera = &glb.list("cameras")[glb.node(node)["camera"].as_u64().unwrap() as usize];
    assert_eq!(camera["type"], "perspective");
    let p = &camera["perspective"];
    assert!((p["yfov"].as_f64().unwrap() - 60f64.to_radians()).abs() < 1e-6);
    assert!((p["aspectRatio"].as_f64().unwrap() - 16.0 / 9.0).abs() < 1e-6);
    assert_eq!(glb.translation(node), [0.0, 1.6, 4.0]);
}

#[test]
fn lights_and_cameras_can_be_left_out() {
    let options = ExportOptions {
        lights: false,
        cameras: false,
        ..ExportOptions::still(0)
    };
    let glb = Glb::checked(&export_glb(&lit_scene(), &options, &GltfOptions::default()).unwrap());
    for name in ["Bulb", "Torch", "Sun", "Shot"] {
        assert!(!glb.has_node(name));
    }
    assert!(glb.has_node("Rig"), "the group stays");
    assert!(glb.json.get("cameras").is_none());
    assert!(glb.json.get("extensions").is_none());
    assert!(glb.extensions_used().is_empty());
}

#[test]
fn baked_lights_and_cameras_keep_their_world_placement() {
    let options = ExportOptions {
        bake_transforms: true,
        ..ExportOptions::still(0)
    };
    let doc = lit_scene();
    let glb = Glb::checked(&export_glb(&doc, &options, &GltfOptions::default()).unwrap());
    assert!(!glb.has_node("Rig"), "groups are flattened away");
    let bulb = glb.node_named("Bulb");
    assert!(glb.parent_of(bulb).is_none());
    assert_eq!(glb.scale(bulb), [2.0; 3]);
    assert_eq!(glb.translation(glb.node_named("Shot")), [0.0, 1.6, 4.0]);
}

#[test]
fn the_report_counts_what_was_written() {
    let mut doc = Document::empty();
    let root = doc.root();
    paint(
        &mut doc,
        root,
        "A",
        vec![
            straight(0, Brush::Cylinder, 5, [0.0; 3]),
            straight(1, Brush::Ribbon, 3, [0.0, 1.0, 0.0]),
        ],
    );
    let output =
        export_glb_with_report(&doc, &ExportOptions::still(0), &GltfOptions::default()).unwrap();
    let report = output.report;
    assert_eq!(report.nodes, 2);
    assert_eq!(report.meshes, 2);
    assert_eq!(report.vertices, 35 + 6);
    assert_eq!(report.triangles, 56 + 4);
    assert_eq!(report.animation_frames, 0);
    assert_eq!(report.animation_channels, 0);
    assert_eq!(report.bytes, output.bytes.len() as u64);
}

#[test]
fn writing_a_file_and_reporting_errors() {
    let mut doc = Document::empty();
    let root = doc.root();
    paint(
        &mut doc,
        root,
        "A",
        vec![straight(0, Brush::Cylinder, 5, [0.0; 3])],
    );
    let dir = std::env::temp_dir().join(format!("limner-export-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("still.glb");
    let report = write_glb(
        &doc,
        &path,
        &ExportOptions::still(0),
        &GltfOptions::default(),
    )
    .unwrap();
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(report.bytes, bytes.len() as u64);
    Glb::checked(&bytes);
    std::fs::remove_dir_all(&dir).unwrap();

    let missing = dir.join("nowhere").join("still.glb");
    assert!(matches!(
        write_glb(
            &doc,
            &missing,
            &ExportOptions::still(0),
            &GltfOptions::default()
        ),
        Err(ExportError::Io { .. })
    ));
    assert!(matches!(
        export_glb(
            &doc,
            &ExportOptions::animated(10 * FRAME, 0, RATE),
            &GltfOptions::default()
        ),
        Err(ExportError::InvalidRange { .. })
    ));
    assert!(matches!(
        export_glb(
            &doc,
            &ExportOptions::animated(0, FRAME, limner_core::time::FrameRate::new(0, 1)),
            &GltfOptions::default()
        ),
        Err(ExportError::InvalidRate { .. })
    ));
}
