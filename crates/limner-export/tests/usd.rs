//! USD text layers: structure, geometry, materials, transforms, animation, lights, cameras.

mod support;

use limner_core::eval::{evaluate, Clock, Content};
use limner_core::glam::{DQuat, DVec3, Vec3};
use limner_core::math::Flip;
use limner_core::model::{Brush, Document, Key, LayerId, Shading, Stroke, Track};
use limner_core::tess::ring_positions;
use limner_core::time::FrameRate;
use limner_export::{
    build_scene, export_usda, export_usda_with_report, scene_to_usda, write_usda, ExportError,
    ExportOptions, ExportScene, UsdOptions,
};
use support::stage::{arm_strokes, chain, stage, CLIP_SHOWN, FLIP_CASES, STAGE_LAST};
use support::usda::{numbers, unquote, vec3s, Prim, Usda};
use support::{group, paint, straight, straight_colored, FRAME, RATE};

fn still(doc: &Document) -> Usda {
    let text = export_usda(doc, &ExportOptions::still(0), &UsdOptions::default()).unwrap();
    Usda::parse(&text)
}

fn animated(doc: &Document, last_frame: i64) -> Usda {
    let options = ExportOptions::animated(0, last_frame * FRAME, RATE);
    Usda::parse(&export_usda(doc, &options, &UsdOptions::default()).unwrap())
}

fn rings(stroke: &Stroke) -> Vec<Vec3> {
    let mut out = Vec::new();
    for i in 0..stroke.len() {
        ring_positions(stroke.points(), i, stroke.brush, &mut out);
    }
    out
}

struct Mesh {
    points: Vec<Vec3>,
    normals: Vec<Vec3>,
    colors: Vec<Vec3>,
    opacities: Vec<f32>,
    indices: Vec<u32>,
}

/// Reads a `Mesh` prim back, checking everything that must hold for any mesh.
fn mesh(prim: Prim) -> Mesh {
    assert_eq!(prim.type_name(), "Mesh");
    let attr = |name: &str| {
        prim.attr(name)
            .unwrap_or_else(|| panic!("{} has no {name}", prim.name()))
    };
    assert_eq!(prim.attr_type("points"), Some("point3f[]"));
    assert_eq!(prim.attr_type("normals"), Some("normal3f[]"));
    assert_eq!(prim.attr_type("faceVertexCounts"), Some("int[]"));
    assert_eq!(prim.attr_type("faceVertexIndices"), Some("int[]"));
    assert_eq!(prim.attr_type("primvars:displayColor"), Some("color3f[]"));
    assert_eq!(prim.attr_type("primvars:displayOpacity"), Some("float[]"));
    for name in [
        "normals",
        "primvars:displayColor",
        "primvars:displayOpacity",
    ] {
        assert_eq!(
            prim.attr_metadata(name),
            Some("interpolation = \"vertex\""),
            "{name} is per vertex"
        );
    }
    assert_eq!(attr("subdivisionScheme"), "\"none\"");
    assert_eq!(prim.attr_type("subdivisionScheme"), Some("uniform token"));
    // Counter-clockwise from outside in a right-handed space is USD's "rightHanded".
    assert_eq!(attr("orientation"), "\"rightHanded\"");
    assert_eq!(prim.attr_type("doubleSided"), Some("uniform bool"));

    let points = vec3s(attr("points"));
    let normals = vec3s(attr("normals"));
    let colors = vec3s(attr("primvars:displayColor"));
    let opacities: Vec<f32> = numbers(attr("primvars:displayOpacity"))
        .into_iter()
        .map(|o| o as f32)
        .collect();
    let counts = numbers(attr("faceVertexCounts"));
    let indices: Vec<u32> = numbers(attr("faceVertexIndices"))
        .into_iter()
        .map(|i| i as u32)
        .collect();
    assert!(!points.is_empty());
    assert_eq!(normals.len(), points.len());
    assert_eq!(colors.len(), points.len());
    assert_eq!(opacities.len(), points.len());
    assert!(counts.iter().all(|c| *c == 3.0), "triangles only");
    assert_eq!(indices.len(), counts.len() * 3);
    assert!(indices.iter().all(|i| (*i as usize) < points.len()));
    assert!(normals.iter().all(|n| (n.length() - 1.0).abs() < 1e-4));
    assert!(opacities.iter().all(|o| (0.0..=1.0).contains(o)));

    let extent = vec3s(attr("extent"));
    let min = points.iter().copied().reduce(Vec3::min).unwrap();
    let max = points.iter().copied().reduce(Vec3::max).unwrap();
    assert_eq!(extent, vec![min, max], "extent is the exact bounding box");
    Mesh {
        points,
        normals,
        colors,
        opacities,
        indices,
    }
}

impl Mesh {
    fn triangles(&self) -> impl Iterator<Item = [Vec3; 3]> + '_ {
        self.indices
            .as_chunks::<3>()
            .0
            .iter()
            .map(|t| [0, 1, 2].map(|i| self.points[t[i] as usize]))
    }
}

fn limner<'a>(prim: &Prim<'a>, key: &str) -> &'a str {
    prim.metadata_value(key)
        .unwrap_or_else(|| panic!("{} has no customData {key}", prim.name()))
}

#[test]
fn the_layer_states_units_axes_and_time() {
    let stage = stage();
    let usda = still(&stage.doc);
    assert_eq!(usda.layer_metadata("upAxis"), Some("\"Y\""));
    assert_eq!(usda.layer_metadata("metersPerUnit"), Some("1"));
    assert_eq!(usda.layer_metadata("defaultPrim"), Some("\"Stage\""));
    assert_eq!(usda.layer_metadata("timeCodesPerSecond"), Some("24"));
    assert_eq!(usda.layer_metadata("framesPerSecond"), Some("24"));
    assert!(usda
        .layer_metadata("doc")
        .unwrap()
        .contains("limner-export"));
    // A still has no time range.
    assert_eq!(usda.layer_metadata("startTimeCode"), None);
    assert_eq!(usda.layer_metadata("endTimeCode"), None);
    assert_eq!(
        usda.roots().len(),
        1,
        "everything is under the default prim"
    );
    assert_eq!(usda.default_prim().type_name(), "Xform");

    let usda = animated(&stage.doc, STAGE_LAST);
    assert_eq!(usda.layer_metadata("startTimeCode"), Some("0"));
    assert_eq!(usda.layer_metadata("endTimeCode"), Some("12"));

    // Time codes are frames of the sampling rate, wherever the range starts.
    let rate = FrameRate::new(24_000, 1001);
    let options = ExportOptions::animated(rate.frame_start(10), rate.frame_start(15), rate);
    let text = export_usda(&stage.doc, &options, &UsdOptions::default()).unwrap();
    let usda = Usda::parse(&text);
    let per_second: f64 = usda
        .layer_metadata("timeCodesPerSecond")
        .unwrap()
        .parse()
        .unwrap();
    assert!((per_second - 24_000.0 / 1001.0).abs() < 1e-9);
    assert_eq!(usda.layer_metadata("startTimeCode"), Some("0"));
    assert_eq!(usda.layer_metadata("endTimeCode"), Some("5"));
}

#[test]
fn an_empty_document_is_still_a_valid_layer() {
    let usda = still(&Document::empty());
    assert_eq!(usda.layer_metadata("defaultPrim"), Some("\"Limner_scene\""));
    let all = usda.all();
    let paths: Vec<&str> = all.iter().map(|(path, _)| path.as_str()).collect();
    assert_eq!(paths, vec!["/Limner_scene", "/Limner_scene/Root"]);
    assert!(usda.find("/Limner_scene/Materials").is_none());
}

#[test]
fn a_scene_without_any_node_is_still_a_valid_layer() {
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
    let (text, report) = scene_to_usda(&scene, &UsdOptions::default()).unwrap();
    let usda = Usda::parse(&text);
    assert_eq!(usda.all().len(), 1, "the default prim alone");
    assert!(usda.default_prim().children().is_empty());
    assert_eq!(report.nodes, 0);
}

#[test]
fn prims_mirror_the_layer_tree_and_keep_the_real_names() {
    let stage = stage();
    let usda = still(&stage.doc);
    let arm = usda.prim("/Stage/Root/Rig/Left_arm");
    assert_eq!(arm.type_name(), "Xform");
    assert_eq!(limner(&arm, "string name"), "\"Left arm\"");
    assert_eq!(limner(&arm, "string kind"), "\"Paint\"");
    assert_eq!(limner(&arm, "int64 layerId"), stage.arm.0.to_string());
    assert_eq!(
        limner(&usda.prim("/Stage/Root"), "string kind"),
        "\"Sequence\""
    );
    assert_eq!(
        limner(&usda.prim("/Stage/Root/Rig"), "string kind"),
        "\"Group\""
    );
    // Children keep the order of the layers.
    let names: Vec<&str> = usda
        .prim("/Stage/Root")
        .children()
        .iter()
        .map(Prim::name)
        .collect();
    // "Clip" shows nothing at frame 0, so a still of that frame leaves it out.
    assert_eq!(
        names,
        vec!["Rig", "Loop", "Ghost", "Bulb", "Torch", "Sun", "Shot"]
    );
    let usda = animated(&stage.doc, STAGE_LAST);
    let names: Vec<&str> = usda
        .prim("/Stage/Root")
        .children()
        .iter()
        .map(Prim::name)
        .collect();
    assert_eq!(
        names,
        vec!["Rig", "Loop", "Clip", "Ghost", "Bulb", "Torch", "Sun", "Shot"]
    );
}

#[test]
fn names_that_usd_cannot_hold_are_cleaned_and_kept_unique() {
    let mut doc = Document::empty();
    doc.settings.title = "9 lives: a \"test\"".to_owned();
    let root = doc.root();
    let first = group(&mut doc, root, "A b");
    let second = group(&mut doc, root, "A_b");
    let third = group(&mut doc, root, "A/b");
    let unnamed = group(&mut doc, root, "");
    let reserved = group(&mut doc, root, "Materials");
    paint(
        &mut doc,
        reserved,
        "Füße \"quoted\" \\ back",
        vec![straight(0, Brush::Cylinder, 2, [0.0; 3])],
    );
    let usda = still(&doc);
    let top = usda.default_prim();
    assert_eq!(top.name(), "_9_lives__a__test_");
    assert!(usda
        .layer_metadata("customLayerData")
        .is_some_and(|v| v == "{"));
    let root_prim = top.child("Root").unwrap();
    let names: Vec<&str> = root_prim.children().iter().map(Prim::name).collect();
    assert_eq!(
        names,
        vec![
            "A_b".to_owned(),
            format!("A_b_{}", second.0),
            format!("A_b_{}", third.0),
            "Group".to_owned(),
            "Materials".to_owned(),
        ]
    );
    let _ = (first, unnamed);
    // A layer called "Materials" at the top would collide with the material scope.
    assert_eq!(top.child("Materials").unwrap().type_name(), "Scope");
    let odd = root_prim.child("Materials").unwrap().children()[0];
    assert_eq!(odd.name(), "F__e__quoted____back");
    assert_eq!(
        limner(&odd, "string name"),
        "\"Füße \\\"quoted\\\" \\\\ back\""
    );
    for (_, prim) in usda.all() {
        let name = prim.name();
        assert!(
            name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit()),
            "{name} is a USD identifier"
        );
    }
}

#[test]
fn a_straight_tube_round_trips_ring_for_ring() {
    for (brush, ring) in [(Brush::Cylinder, 7), (Brush::Ellipse, 7), (Brush::Cube, 4)] {
        let stroke = straight(0, brush, 5, [1.0, 2.0, -1.0]);
        let mut doc = Document::empty();
        let root = doc.root();
        paint(&mut doc, root, "Tube", vec![stroke.clone()]);
        let usda = still(&doc);
        let layer = usda.prim("/Limner_scene/Root/Tube");
        assert_eq!(layer.children().len(), 1);
        let prim = layer.child("Tubes").unwrap();
        let m = mesh(prim);
        assert_eq!(m.points.len(), 5 * ring, "{brush:?}: one ring per point");
        assert_eq!(m.indices.len() / 3, 4 * ring * 2, "{brush:?}");
        assert_eq!(m.points, rings(&stroke), "{brush:?}: the renderer's rings");
        let scene = build_scene(&doc, &ExportOptions::still(0));
        assert_eq!(m.indices, scene.meshes[0].indices, "{brush:?}");

        // The stroke runs along X at y = 2, z = -1.
        let outward = |v: Vec3| Vec3::new(0.0, v.y - 2.0, v.z + 1.0);
        for [a, b, c] in m.triangles() {
            assert!(
                (b - a).cross(c - a).dot(outward((a + b + c) / 3.0)) > 0.0,
                "{brush:?}: counter-clockwise seen from outside"
            );
        }
        for (position, normal) in m.points.iter().zip(&m.normals) {
            assert!(normal.dot(outward(*position)) > 0.0, "{brush:?}");
        }
        assert_eq!(
            prim.attr("doubleSided"),
            Some("0"),
            "tubes are single-sided"
        );
        // Colour is linear, written as it is.
        assert!(m.colors.iter().all(|c| *c == Vec3::new(1.0, 0.5, 0.25)));
        assert!(m.opacities.iter().all(|o| *o == 1.0));
        assert_eq!(
            prim.target("material:binding"),
            Some("/Limner_scene/Materials/unlit")
        );
        assert!(prim
            .metadata()
            .contains(&"prepend apiSchemas = [\"MaterialBindingAPI\"]"));
    }
}

#[test]
fn a_ribbon_is_a_double_sided_strip() {
    for brush in [Brush::Ribbon, Brush::Line] {
        let stroke = straight(0, brush, 5, [1.0, 2.0, -1.0]);
        let mut doc = Document::empty();
        let root = doc.root();
        paint(&mut doc, root, "Ribbon", vec![stroke.clone()]);
        let usda = still(&doc);
        let prim = usda.prim("/Limner_scene/Root/Ribbon/Flat");
        let m = mesh(prim);
        assert_eq!(m.points, rings(&stroke));
        assert_eq!(m.indices.len() / 3, 8);
        for [a, b, c] in m.triangles() {
            assert!((b - a).cross(c - a).normalize().dot(Vec3::Y) > 0.999);
        }
        assert!(m.normals.iter().all(|n| (*n - Vec3::Y).length() < 1e-5));
        assert_eq!(prim.attr("doubleSided"), Some("1"));
    }
}

#[test]
fn strokes_are_split_by_sidedness_and_transparency() {
    let mut doc = Document::empty();
    let root = doc.root();
    let color = [1.5, 0.5, 0.25];
    paint(
        &mut doc,
        root,
        "Mixed",
        vec![
            straight_colored(0, Brush::Cylinder, 3, [0.0, 0.0, 0.0], color, 1.0),
            straight_colored(1, Brush::Cylinder, 4, [0.0, 1.0, 0.0], color, 0.4),
            straight_colored(2, Brush::Ribbon, 5, [0.0, 2.0, 0.0], color, 1.0),
            straight_colored(3, Brush::Ribbon, 6, [0.0, 3.0, 0.0], color, 0.4),
            straight_colored(4, Brush::Cube, 2, [0.0, 4.0, 0.0], color, 1.0),
        ],
    );
    let (text, report) =
        export_usda_with_report(&doc, &ExportOptions::still(0), &UsdOptions::default()).unwrap();
    let usda = Usda::parse(&text);
    let layer = usda.prim("/Limner_scene/Root/Mixed");
    let summary: Vec<(&str, usize, &str, &str, f32)> = layer
        .children()
        .into_iter()
        .map(|prim| {
            let m = mesh(prim);
            // Colours above 1 are kept: USD holds floats.
            assert!(m.colors.iter().all(|c| *c == Vec3::from(color)));
            assert!(m.opacities.iter().all(|o| *o == m.opacities[0]));
            (
                prim.name(),
                m.points.len(),
                prim.attr("doubleSided").unwrap(),
                prim.target("material:binding").unwrap(),
                m.opacities[0],
            )
        })
        .collect();
    let materials = "/Limner_scene/Materials";
    assert_eq!(
        summary,
        vec![
            (
                "Tubes",
                3 * 7 + 2 * 4,
                "0",
                &*format!("{materials}/unlit"),
                1.0
            ),
            ("Flat", 5 * 2, "1", &*format!("{materials}/unlit"), 1.0),
            (
                "TubesSeeThrough",
                4 * 7,
                "0",
                &*format!("{materials}/unlit_see_through"),
                0.4
            ),
            (
                "FlatSeeThrough",
                6 * 2,
                "1",
                &*format!("{materials}/unlit_see_through"),
                0.4
            ),
        ]
    );
    assert_eq!(report.meshes, 4);
    assert_eq!(report.vertices, 29 + 10 + 28 + 12);
    assert_eq!(
        report.nodes, 2,
        "the root and the layer; meshes are not nodes"
    );
    assert_eq!(report.bytes, text.len() as u64);
    assert_eq!((report.animation_frames, report.animation_channels), (0, 0));
}

fn shader_input<'a>(usda: &'a Usda, material: &str, shader: &str, input: &str) -> Option<&'a str> {
    usda.prim(&format!("{material}/{shader}"))
        .attr(&format!("inputs:{input}"))
}

#[test]
fn materials_read_the_vertex_colours() {
    let stage = stage();
    let usda = still(&stage.doc);
    let scope = usda.prim("/Stage/Materials");
    assert_eq!(scope.type_name(), "Scope");
    let names: Vec<&str> = scope.children().iter().map(Prim::name).collect();
    assert_eq!(names, vec!["unlit", "unlit_see_through", "lit_see_through"]);

    for material in scope.children() {
        assert_eq!(material.type_name(), "Material");
        let path = format!("/Stage/Materials/{}", material.name());
        assert_eq!(
            material.target("outputs:surface.connect"),
            Some(&*format!("{path}/Surface.outputs:surface"))
        );
        let surface = material.child("Surface").unwrap();
        assert_eq!(surface.type_name(), "Shader");
        assert_eq!(surface.attr("info:id"), Some("\"UsdPreviewSurface\""));
        assert_eq!(surface.attr_type("outputs:surface"), Some("token"));
        let reader = material.child("Color").unwrap();
        assert_eq!(reader.attr("info:id"), Some("\"UsdPrimvarReader_float3\""));
        assert_eq!(reader.attr("inputs:varname"), Some("\"displayColor\""));
        assert_eq!(reader.attr_type("outputs:result"), Some("float3"));
        let color = format!("{path}/Color.outputs:result");

        let lit = material.name().starts_with("lit");
        if lit {
            assert_eq!(surface.target("inputs:diffuseColor.connect"), Some(&*color));
            assert!(surface.attr("inputs:emissiveColor.connect").is_none());
        } else {
            // Unlit: nothing for lights to act on, the paint is emitted.
            assert_eq!(surface.attr("inputs:diffuseColor"), Some("(0, 0, 0)"));
            assert_eq!(
                surface.target("inputs:emissiveColor.connect"),
                Some(&*color)
            );
            assert_eq!(surface.attr("inputs:useSpecularWorkflow"), Some("1"));
            assert_eq!(surface.attr("inputs:specularColor"), Some("(0, 0, 0)"));
        }
        assert_eq!(surface.attr("inputs:metallic"), Some("0"));
        assert_eq!(surface.attr("inputs:roughness"), Some("1"));

        let see_through = material.name().ends_with("see_through");
        assert_eq!(material.child("Opacity").is_some(), see_through);
        if see_through {
            let opacity = material.child("Opacity").unwrap();
            assert_eq!(opacity.attr("info:id"), Some("\"UsdPrimvarReader_float\""));
            assert_eq!(opacity.attr("inputs:varname"), Some("\"displayOpacity\""));
            assert_eq!(
                surface.target("inputs:opacity.connect"),
                Some(&*format!("{path}/Opacity.outputs:result"))
            );
        } else {
            assert!(surface.attr("inputs:opacity.connect").is_none());
        }
    }
    let _ = shader_input;

    // The arm's see-through ribbon and the half-faded lit layer use the blended looks.
    let arm = usda.prim("/Stage/Root/Rig/Left_arm");
    assert_eq!(
        arm.child("Tubes").unwrap().target("material:binding"),
        Some("/Stage/Materials/unlit")
    );
    assert_eq!(
        arm.child("FlatSeeThrough")
            .unwrap()
            .target("material:binding"),
        Some("/Stage/Materials/unlit_see_through")
    );
}

#[test]
fn layer_opacity_is_multiplied_into_the_vertex_opacity() {
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
    let usda = still(&doc);
    let layer = usda.prim("/Limner_scene/Root/Faded/Child");
    // Opacity multiplies down the tree: 0.5 * 0.5, and again with the stroke's own.
    let opaque = mesh(layer.child("Tubes").unwrap());
    assert!(opaque.opacities.iter().all(|o| *o == 0.25));
    let faint = mesh(layer.child("TubesSeeThrough").unwrap());
    assert!(faint.opacities.iter().all(|o| *o == 0.125));
    // Strokes that were opaque need blending once the layer is faded.
    for name in ["Tubes", "TubesSeeThrough"] {
        assert_eq!(
            layer.child(name).unwrap().target("material:binding"),
            Some("/Limner_scene/Materials/lit_see_through")
        );
    }
}

#[test]
fn normals_and_materials_can_be_left_out() {
    let stage = stage();
    let plain = export_usda(&stage.doc, &ExportOptions::still(0), &UsdOptions::default()).unwrap();
    let bare = UsdOptions {
        normals: false,
        materials: false,
    };
    let text = export_usda(&stage.doc, &ExportOptions::still(0), &bare).unwrap();
    assert!(text.len() < plain.len());
    let usda = Usda::parse(&text);
    assert!(usda.find("/Stage/Materials").is_none());
    let prim = usda.prim("/Stage/Root/Rig/Left_arm/Tubes");
    assert!(prim.attr("normals").is_none());
    assert!(prim.attr("material:binding").is_none());
    assert!(prim.metadata().is_empty());
    assert!(prim.attr("primvars:displayColor").is_some());
    assert!(!text.contains("Material"));
}

#[test]
fn the_prim_chain_reproduces_world_placement() {
    let (plain, _, _) = chain([Flip::None; 3]);
    let plain = still(&plain);
    let path = "/Limner_scene/Root/A/B/P";
    let plain_indices: Vec<Vec<u32>> = ["Tubes", "Flat"]
        .iter()
        .map(|name| mesh(plain.prim(path).child(name).unwrap()).indices)
        .collect();

    for flips in FLIP_CASES {
        let (doc, layer, strokes) = chain(flips);
        let usda = still(&doc);
        let world = usda.world_matrix(path, 0.0);
        let expected = doc.world_from_layer(layer);
        let prim = usda.prim(path);
        let mut indices = Vec::new();
        for (name, stroke) in ["Tubes", "Flat"].iter().zip(&strokes) {
            let m = mesh(prim.child(name).unwrap());
            // Vertices stay in layer space, exactly as tessellated.
            assert_eq!(m.points, rings(stroke));
            for point in &m.points {
                let via_prims = world.transform_point3(point.as_dvec3());
                let via_document = expected.transform_point3(point.as_dvec3());
                assert!(
                    (via_prims - via_document).length() < 1e-5,
                    "{flips:?}: {via_prims:?} vs {via_document:?}"
                );
            }
            indices.push(m.indices);
        }
        // A mirror is a negative scale on the flip axis of that layer's own prim.
        for (prim_path, flip, scale) in [
            ("/Limner_scene/Root/A", flips[0], 2.0),
            ("/Limner_scene/Root/A/B", flips[1], 0.5),
            (path, flips[2], 1.5),
        ] {
            let prim = usda.prim(prim_path);
            assert_eq!(
                numbers(prim.attr("xformOp:scale").unwrap()),
                (flip.signs() * scale).to_array(),
                "{flips:?} {prim_path}"
            );
            assert_eq!(prim.attr_type("xformOp:scale"), Some("float3"));
            assert_eq!(prim.attr_type("xformOp:translate"), Some("double3"));
            assert_eq!(prim.attr_type("xformOp:orient"), Some("quatf"));
            assert_eq!(
                prim.attr("xformOpOrder"),
                Some("[\"xformOp:translate\", \"xformOp:orient\", \"xformOp:scale\"]")
            );
        }
        // The winding is left alone: USD flips the facing under a mirror by itself.
        assert_eq!(indices, plain_indices, "{flips:?}");
    }
    // Layers that do nothing have no ops at all.
    let root = plain.prim("/Limner_scene/Root");
    assert!(root.attr("xformOpOrder").is_none());
    assert!(root.property_names().is_empty());
}

#[test]
fn baked_meshes_are_in_world_space_and_face_outward() {
    for flips in FLIP_CASES {
        let (doc, layer, strokes) = chain(flips);
        let options = ExportOptions {
            bake_transforms: true,
            ..ExportOptions::still(0)
        };
        let usda = Usda::parse(&export_usda(&doc, &options, &UsdOptions::default()).unwrap());
        // The hierarchy is gone: the layer sits under the default prim, without ops.
        let top = usda.default_prim();
        let names: Vec<&str> = top.children().iter().map(Prim::name).collect();
        assert_eq!(names, vec!["P", "Materials"], "{flips:?}");
        let prim = top.child("P").unwrap();
        assert!(prim.property_names().is_empty(), "{flips:?}: no ops");

        let world = doc.world_from_layer(layer);
        for (name, stroke) in ["Tubes", "Flat"].iter().zip(&strokes) {
            let m = mesh(prim.child(name).unwrap());
            let expected: Vec<Vec3> = rings(stroke)
                .iter()
                .map(|p| world.transform_point3(p.as_dvec3()).as_vec3())
                .collect();
            assert_eq!(m.points.len(), expected.len());
            for (exported, expected) in m.points.iter().zip(&expected) {
                assert!((*exported - *expected).length() < 1e-5, "{flips:?}");
            }
            // Winding agrees with the vertex normals, mirrored or not.
            for triangle in m.indices.as_chunks::<3>().0 {
                let [a, b, c] = [0, 1, 2].map(|i| m.points[triangle[i] as usize]);
                let normal: Vec3 = triangle.iter().map(|i| m.normals[*i as usize]).sum();
                assert!((b - a).cross(c - a).dot(normal) > 0.0, "{flips:?}");
            }
        }
    }
}

fn sampled_xform(prim: &Prim, time: f64) -> (DVec3, DQuat, DVec3) {
    let get = |name: &str, fallback: &[f64]| {
        prim.sample_numbers(name, time, false)
            .unwrap_or_else(|| fallback.to_vec())
    };
    let t = get("xformOp:translate", &[0.0; 3]);
    let r = get("xformOp:orient", &[1.0, 0.0, 0.0, 0.0]);
    let s = get("xformOp:scale", &[1.0; 3]);
    (
        DVec3::new(t[0], t[1], t[2]),
        DQuat::from_xyzw(r[1], r[2], r[3], r[0]).normalize(),
        DVec3::new(s[0], s[1], s[2]),
    )
}

#[test]
fn keyed_transforms_become_time_samples() {
    let stage = stage();
    let usda = animated(&stage.doc, STAGE_LAST);
    let prim = usda.prim("/Stage/Root/Rig/Left_arm");
    for k in 0..=STAGE_LAST {
        let frame = evaluate(&stage.doc, Clock::at(k * FRAME));
        let expected = frame.get(stage.arm).unwrap().local;
        let (t, r, s) = sampled_xform(&prim, k as f64);
        assert!(
            (t - expected.translation).length() < 1e-9,
            "frame {k}: translation {t:?} vs {:?}",
            expected.translation
        );
        assert!(r.dot(expected.rotation).abs() > 1.0 - 1e-6, "frame {k}");
        let expected_scale = expected.flip.signs() * expected.scale;
        assert!((s - expected_scale).length() < 1e-6, "frame {k}: scale");
        // And the whole chain, mirrored rig included, lands where the document says.
        let world = usda.world_matrix("/Stage/Root/Rig/Left_arm", k as f64);
        let expected_world = frame.get(stage.arm).unwrap().world;
        let probe = DVec3::new(0.3, 0.5, -0.2);
        assert!(
            (world.transform_point3(probe) - expected_world.transform_point3(probe)).length()
                < 1e-5,
            "frame {k}: world placement"
        );
    }

    // Runs where nothing changes are merged: before the first key (frames 0..=2) and
    // after the last (frames 10..=12) only the ends of the hold are written.
    let times: Vec<f64> = prim
        .time_samples("xformOp:translate")
        .unwrap()
        .iter()
        .map(|(t, _)| *t)
        .collect();
    let expected: Vec<f64> = [0, 2]
        .into_iter()
        .chain(3..=10)
        .chain([12])
        .map(f64::from)
        .collect();
    assert_eq!(times, expected);
    // Consecutive quaternions stay on one hemisphere, so no reader takes the long way.
    let rotations: Vec<Vec<f64>> = prim
        .time_samples("xformOp:orient")
        .unwrap()
        .iter()
        .map(|(_, value)| numbers(value))
        .collect();
    for pair in rotations.windows(2) {
        let dot: f64 = pair[0].iter().zip(&pair[1]).map(|(a, b)| a * b).sum();
        assert!(dot > 0.0);
    }
    // The value outside the samples is the first frame, for readers without animation.
    let first = evaluate(&stage.doc, Clock::at(0))
        .get(stage.arm)
        .unwrap()
        .local;
    assert_eq!(
        numbers(prim.attr("xformOp:translate").unwrap()),
        first.translation.to_array()
    );

    // Nothing else in the scene moves: the rig and the lights have plain values.
    for path in ["/Stage/Root/Rig", "/Stage/Root/Bulb", "/Stage/Root/Clip"] {
        let prim = usda.prim(path);
        for op in ["translate", "orient", "scale"] {
            assert!(
                prim.time_samples(&format!("xformOp:{op}")).is_none(),
                "{path} {op}"
            );
        }
    }
}

#[test]
fn a_still_has_no_time_samples() {
    let stage = stage();
    let text = export_usda(
        &stage.doc,
        &ExportOptions::still(5 * FRAME),
        &UsdOptions::default(),
    )
    .unwrap();
    assert!(!text.contains("timeSamples"));
    let usda = Usda::parse(&text);
    // The still shows the state at its instant: the clip is on at frame 5.
    assert!(usda.prim("/Stage/Root/Clip").attr("visibility").is_none());
    let expected = evaluate(&stage.doc, Clock::at(5 * FRAME))
        .get(stage.arm)
        .unwrap()
        .local;
    let (t, _, _) = sampled_xform(&usda.prim("/Stage/Root/Rig/Left_arm"), 0.0);
    assert!((t - expected.translation).length() < 1e-9);
    // At frame 0 it shows nothing, so a still of that frame leaves the layer out.
    assert!(still(&stage.doc).find("/Stage/Root/Clip").is_none());

    // A group outside its clip stays in the tree, hidden by a plain value.
    let mut doc = Document::empty();
    let root = doc.root();
    let later = group(&mut doc, root, "Later");
    {
        let layer = doc.layer_mut(later).unwrap();
        layer.anim.visibility = Track::from_keys([Key::step(4 * FRAME, true)]);
        layer.anim.offset.clear();
    }
    let usda = still(&doc);
    let prim = usda.prim("/Limner_scene/Root/Later");
    assert_eq!(prim.attr("visibility"), Some("\"invisible\""));
    assert_eq!(prim.property_names(), vec!["visibility"]);
}

#[test]
fn a_clip_that_starts_mid_range_is_animated_visibility() {
    let stage = stage();
    let (text, report) = export_usda_with_report(
        &stage.doc,
        &ExportOptions::animated(0, STAGE_LAST * FRAME, RATE),
        &UsdOptions::default(),
    )
    .unwrap();
    let usda = Usda::parse(&text);
    let prim = usda.prim("/Stage/Root/Clip");
    assert_eq!(prim.attr_type("visibility"), Some("token"));
    // Hidden on the first frame, which is also what a reader without animation sees.
    assert_eq!(prim.attr("visibility"), Some("\"invisible\""));
    let samples = prim.time_samples("visibility").unwrap();
    assert_eq!(
        samples,
        vec![
            (0.0, "\"invisible\""),
            (CLIP_SHOWN.0 as f64, "\"inherited\""),
            ((CLIP_SHOWN.1 + 1) as f64, "\"invisible\""),
            (STAGE_LAST as f64, "\"invisible\""),
        ]
    );
    for k in 0..=STAGE_LAST {
        let frame = evaluate(&stage.doc, Clock::at(k * FRAME));
        assert_eq!(
            usda.is_visible("/Stage/Root/Clip", k as f64),
            frame.get(stage.clip).unwrap().in_clip,
            "frame {k}"
        );
    }
    // USD has real visibility, so the transform is left alone: no zero scale.
    assert_eq!(prim.attr("xformOp:scale"), Some("(2, 2, 2)"));
    assert!(prim.time_samples("xformOp:scale").is_none());
    // Layers that are always shown say nothing about visibility.
    assert!(usda.prim("/Stage/Root/Rig").attr("visibility").is_none());

    assert_eq!(report.animation_frames, 13);
    // Arm: translate, orient, scale. Clip: visibility. Flicker: three drawings.
    assert_eq!(report.animation_channels, 7);
    assert_eq!(text.matches(".timeSamples = {").count(), 7);
}

#[test]
fn frame_by_frame_drawings_are_children_switched_by_visibility() {
    let stage = stage();
    let usda = animated(&stage.doc, STAGE_LAST);
    let drawings = stage
        .doc
        .layer(stage.flicker)
        .unwrap()
        .as_paint()
        .unwrap()
        .drawings
        .clone();
    let layer = usda.prim("/Stage/Root/Loop/Flicker");
    let children = layer.children();
    let names: Vec<&str> = children.iter().map(Prim::name).collect();
    assert_eq!(names, vec!["Flicker_000", "Flicker_001", "Flicker_002"]);
    for (ordinal, child) in children.iter().enumerate() {
        assert_eq!(child.type_name(), "Xform");
        assert_eq!(limner(child, "int drawing"), ordinal.to_string());
        assert_eq!(
            limner(child, "int64 drawingId"),
            drawings[ordinal].0.to_string()
        );
        assert_eq!(limner(child, "int64 layerId"), stage.flicker.0.to_string());
        // Each drawing kept its own geometry.
        let m = mesh(child.child("Tubes").unwrap());
        assert_eq!(m.points.len(), (2 + ordinal) * 7);
        assert!(child.attr("xformOpOrder").is_none());
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
            assert_eq!(
                child.visible_at(k as f64),
                ordinal == expected,
                "frame {k}, drawing {ordinal}"
            );
        }
    }
    assert_eq!(seen, vec![0, 1, 2, 0, 0, 1, 2, 0, 0, 1, 2, 0, 0]);
    // Rest state: only the first frame's drawing is shown.
    assert!(children[0].attr("visibility") == Some("\"inherited\""));
    assert_eq!(children[1].attr("visibility"), Some("\"invisible\""));
    // The layer and the looping sequence need no samples of their own.
    for path in ["/Stage/Root/Loop", "/Stage/Root/Loop/Flicker"] {
        assert!(usda.prim(path).attr("visibility").is_none());
    }

    // In a still, a frame-by-frame layer is just the drawing it shows.
    let usda = still(&stage.doc);
    let layer = usda.prim("/Stage/Root/Loop/Flicker");
    let names: Vec<&str> = layer.children().iter().map(Prim::name).collect();
    assert_eq!(names, vec!["Tubes"]);
}

fn number(prim: &Prim, name: &str) -> f64 {
    numbers(
        prim.attr(name)
            .unwrap_or_else(|| panic!("{} has no {name}", prim.name())),
    )[0]
}

#[test]
fn lights_are_usdlux_prims() {
    let stage = stage();
    let usda = still(&stage.doc);

    let bulb = usda.prim("/Stage/Root/Bulb");
    assert_eq!(bulb.type_name(), "SphereLight");
    assert_eq!(bulb.attr("treatAsPoint"), Some("1"));
    assert_eq!(bulb.attr_type("treatAsPoint"), Some("bool"));
    assert_eq!(bulb.attr("inputs:color"), Some("(1, 0.5, 0.25)"));
    assert_eq!(bulb.attr_type("inputs:color"), Some("color3f"));
    assert_eq!(number(&bulb, "inputs:intensity"), 3.0);
    assert_eq!(bulb.attr("inputs:normalize"), Some("1"));
    assert!(bulb.attr("inputs:shaping:cone:angle").is_none());
    assert_eq!(bulb.attr("inputs:shadow:enable"), Some("0"));
    assert!(bulb
        .metadata()
        .contains(&"prepend apiSchemas = [\"ShadowAPI\"]"));
    assert_eq!(bulb.attr("xformOp:translate"), Some("(0, 2, 0)"));
    // USD lights have no range; the original values stay readable.
    assert_eq!(limner(&bulb, "float range"), "4");
    assert_eq!(limner(&bulb, "float worldRange"), "8");
    assert_eq!(limner(&bulb, "float energy"), "3");
    assert_eq!(limner(&bulb, "string kind"), "\"Light\"");

    let torch = usda.prim("/Stage/Root/Torch");
    assert_eq!(torch.type_name(), "SphereLight");
    assert!(torch
        .metadata()
        .contains(&"prepend apiSchemas = [\"ShapingAPI\", \"ShadowAPI\"]"));
    assert_eq!(torch.attr("treatAsPoint"), Some("1"));
    // Colours above 1 are normalised and the excess moves into the intensity.
    assert_eq!(torch.attr("inputs:color"), Some("(1, 0.5, 0.25)"));
    assert_eq!(number(&torch, "inputs:intensity"), 3.0);
    // The cone is measured off the axis in degrees, like Limner's.
    assert_eq!(number(&torch, "inputs:shaping:cone:angle"), 30.0);
    assert_eq!(number(&torch, "inputs:shaping:cone:softness"), 0.25);
    assert_eq!(torch.attr("inputs:shadow:enable"), Some("1"));

    let sun = usda.prim("/Stage/Root/Sun");
    assert_eq!(sun.type_name(), "DistantLight");
    assert_eq!(number(&sun, "inputs:intensity"), 0.75);
    assert!(sun.attr("treatAsPoint").is_none());
    assert!(sun.attr("inputs:radius").is_none());
    // Lights point along their local -Z in both Limner and UsdLux, so the layer's
    // rotation is the prim's.
    let expected = stage.doc.layer(stage.sun).unwrap().transform.rotation;
    let (_, r, _) = sampled_xform(&sun, 0.0);
    assert!(r.dot(expected).abs() > 1.0 - 1e-6);
}

#[test]
fn cameras_carry_the_field_of_view_as_a_lens() {
    let stage = stage();
    let usda = still(&stage.doc);
    let shot = usda.prim("/Stage/Root/Shot");
    assert_eq!(shot.type_name(), "Camera");
    assert_eq!(shot.attr("projection"), Some("\"perspective\""));
    let focal = number(&shot, "focalLength");
    let vertical = number(&shot, "verticalAperture");
    let horizontal = number(&shot, "horizontalAperture");
    let fov = 2.0 * (vertical / (2.0 * focal)).atan().to_degrees();
    assert!(
        (fov - 50.0).abs() < 1e-4,
        "vertical field of view, got {fov}"
    );
    assert!((horizontal / vertical - 16.0 / 9.0).abs() < 1e-5);
    assert_eq!(shot.attr("clippingRange"), Some("(0.01, 1000)"));
    assert_eq!(shot.attr_type("clippingRange"), Some("float2"));
    assert_eq!(shot.attr("xformOp:translate"), Some("(0, 1.6, 4)"));
    let expected = stage.doc.layer(stage.shot).unwrap().transform.rotation;
    let (_, r, _) = sampled_xform(&shot, 0.0);
    assert!(r.dot(expected).abs() > 1.0 - 1e-6);
}

#[test]
fn lights_and_cameras_can_be_left_out() {
    let stage = stage();
    let options = ExportOptions {
        lights: false,
        cameras: false,
        ..ExportOptions::still(0)
    };
    let text = export_usda(&stage.doc, &options, &UsdOptions::default()).unwrap();
    for word in ["SphereLight", "DistantLight", "Camera", "ShapingAPI"] {
        assert!(!text.contains(word), "{word}");
    }
}

fn find_layer(usda: &Usda, layer: LayerId) -> Option<String> {
    let wanted = layer.0.to_string();
    usda.all().into_iter().find_map(|(path, prim)| {
        (prim.metadata_value("int64 layerId") == Some(wanted.as_str())
            && prim.metadata_value("string kind").is_some())
        .then_some(path)
    })
}

#[test]
fn every_layer_of_the_stage_is_findable_by_its_id() {
    let stage = stage();
    let usda = animated(&stage.doc, STAGE_LAST);
    for (layer, path) in [
        (stage.rig, "/Stage/Root/Rig"),
        (stage.arm, "/Stage/Root/Rig/Left_arm"),
        (stage.clip, "/Stage/Root/Clip"),
        (stage.flicker, "/Stage/Root/Loop/Flicker"),
        (stage.ghost, "/Stage/Root/Ghost"),
        (stage.bulb, "/Stage/Root/Bulb"),
        (stage.torch, "/Stage/Root/Torch"),
        (stage.sun, "/Stage/Root/Sun"),
        (stage.shot, "/Stage/Root/Shot"),
    ] {
        assert_eq!(find_layer(&usda, layer).as_deref(), Some(path));
    }
    // The arm's two strokes came through as they were tessellated.
    let strokes = arm_strokes();
    let arm = usda.prim("/Stage/Root/Rig/Left_arm");
    assert_eq!(mesh(arm.child("Tubes").unwrap()).points, rings(&strokes[0]));
    let ribbon = mesh(arm.child("FlatSeeThrough").unwrap());
    assert_eq!(ribbon.points, rings(&strokes[1]));
    assert!(ribbon.opacities.iter().all(|o| *o == 0.5));
    assert!(ribbon.colors.iter().all(|c| *c == Vec3::new(0.1, 0.6, 0.9)));
    assert_eq!(
        unquote(usda.layer_metadata("defaultPrim").unwrap()),
        "Stage"
    );
}

#[test]
fn writing_a_file_and_reporting_errors() {
    let stage = stage();
    let dir = std::env::temp_dir().join(format!("limner-usd-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("stage.usda");
    let options = ExportOptions::animated(0, STAGE_LAST * FRAME, RATE);
    let report = write_usda(&stage.doc, &path, &options, &UsdOptions::default()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(report.bytes, text.len() as u64);
    Usda::parse(&text);
    std::fs::remove_dir_all(&dir).unwrap();

    let missing = dir.join("nowhere").join("stage.usda");
    assert!(matches!(
        write_usda(&stage.doc, &missing, &options, &UsdOptions::default()),
        Err(ExportError::Io { .. })
    ));
    assert!(matches!(
        export_usda(
            &stage.doc,
            &ExportOptions::animated(10 * FRAME, 0, RATE),
            &UsdOptions::default()
        ),
        Err(ExportError::InvalidRange { .. })
    ));
}

/// Runs Apple's `usdchecker` on the stage when the tool is installed (macOS ships it
/// in `/usr/bin`). Ignored by default because it depends on the machine:
///
/// ```text
/// cargo test -p limner-export --test usd -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs Apple's USD tools in /usr/bin"]
fn usdchecker_accepts_the_stage() {
    let checker = std::path::Path::new("/usr/bin/usdchecker");
    if !checker.is_file() {
        eprintln!("skipped: /usr/bin/usdchecker is not on this machine");
        return;
    }
    let stage = stage();
    let dir = std::env::temp_dir().join(format!("limner-usdchecker-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, options) in [
        ("still.usda", ExportOptions::still(0)),
        (
            "animated.usda",
            ExportOptions::animated(0, STAGE_LAST * FRAME, RATE),
        ),
    ] {
        let path = dir.join(name);
        write_usda(&stage.doc, &path, &options, &UsdOptions::default()).unwrap();
        let output = std::process::Command::new(checker)
            .arg(&path)
            .output()
            .expect("usdchecker runs");
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("{name}: {}", stdout.trim());
        assert!(
            output.status.success() && stdout.contains("Success"),
            "{name}: {stdout}"
        );
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
