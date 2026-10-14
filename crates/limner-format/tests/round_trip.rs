mod common;

use common::*;
use limner_core::glam::DVec3;
use limner_core::model::{Brush, Document, Drawing, Group, LayerId, LayerKind, Paint, Stroke};
use limner_core::time::FrameRate;
use limner_format::testing::{assert_documents_equal, exhaustive_document, Rng};
use limner_format::{load_project, save_project, FormatError, MAX_LAYER_DEPTH};
use std::sync::Arc;

#[test]
fn every_part_of_the_model_round_trips() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("everything.limn");

    let report = save_project(&doc, &project, &fast()).unwrap();
    // Seven drawings: three with the same strokes share one blob.
    assert_eq!(doc.drawing_count(), 7);
    assert_eq!((report.blobs_written, report.blobs_reused), (5, 0));
    assert_eq!(doc.media_items().count(), 11);
    assert_eq!((report.media_written, report.media_reused), (11, 0));
    assert!(report.manifest_written);
    assert_eq!(report.files_removed, 0);
    assert_eq!(
        report.bytes_written,
        snapshot(&project)
            .values()
            .map(|v| v.len() as u64)
            .sum::<u64>()
    );

    let loaded = load_project(&project).unwrap();
    assert_eq!(loaded.warnings, Vec::<String>::new());
    loaded.document.validate().unwrap();
    assert_documents_equal(&doc, &loaded.document);

    // The same result through `limn.json` itself.
    let by_manifest = load_project(&project.join("limn.json")).unwrap();
    assert_documents_equal(&doc, &by_manifest.document);
    assert!(limner_format::is_limn_project(&project));
    assert!(limner_format::is_limn_project(&project.join("limn.json")));
    assert!(!limner_format::is_limn_project(dir.path()));
    assert!(!limner_format::is_limn_project(&project.join("missing")));
}

#[test]
fn loading_restores_sharing_and_chunking() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let loaded = load_project(dir.path()).unwrap().document;

    // Drawings with the same content come back as one allocation.
    let twins: Vec<_> = loaded
        .drawings()
        .filter(|(_, d)| d.stroke_count() == 6)
        .map(|(_, d)| d.clone())
        .collect();
    assert_eq!(twins.len(), 3);
    assert!(Arc::ptr_eq(&twins[0], &twins[1]) && Arc::ptr_eq(&twins[1], &twins[2]));

    // The large drawing is split into chunks again, exactly as the original was.
    let big = drawing_with_strokes(&doc, 30);
    let (original, restored) = (doc.drawing(big).unwrap(), loaded.drawing(big).unwrap());
    assert!(restored.chunks().len() > 1);
    assert_eq!(original, restored);
    assert_eq!(loaded.stroke_count(), doc.stroke_count());
    assert_eq!(loaded.point_count(), doc.point_count());
}

#[test]
fn saving_what_was_loaded_changes_nothing() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("a.limn");
    save_project(&doc, &first, &fast()).unwrap();
    let before = snapshot(&first);

    let loaded = load_project(&first).unwrap().document;
    let report = save_project(&loaded, &first, &fast()).unwrap();
    assert!(report.wrote_nothing(), "{report}");
    assert_eq!(report.bytes_written, 0);
    assert_eq!((report.blobs_reused, report.media_reused), (5, 11));
    assert_eq!(snapshot(&first), before);

    // And into a fresh folder it produces the same files, byte for byte.
    let second = dir.path().join("b.limn");
    save_project(&loaded, &second, &fast()).unwrap();
    assert_eq!(snapshot(&second), before);
}

#[test]
fn random_point_bits_survive_exactly() {
    let mut rng = Rng(0xDEAD_BEEF_0BAD_F00D);
    let mut doc = Document::new();
    let paint = doc.find_by_path("Root/Paint").unwrap();
    let id = doc.layer(paint).unwrap().as_paint().unwrap().drawings[0];
    let brushes = [
        Brush::Line,
        Brush::Ribbon,
        Brush::Cylinder,
        Brush::Ellipse,
        Brush::Cube,
    ];
    let mut expected = Vec::new();
    for s in 0..40u32 {
        let points: Vec<_> = (0..(rng.next_u64() % 400) as usize)
            .map(|_| rng.point())
            .collect();
        expected.extend(points.iter().copied());
        doc.drawing_mut(id).unwrap().push_stroke(Stroke::new(
            rng.next_u64() as u32,
            brushes[s as usize % 5],
            s % 3 == 0,
            points,
        ));
    }
    // Values a careless encoder would normalise.
    let mut special = rng.point();
    special.pos = [-0.0, f32::MIN_POSITIVE, f32::from_bits(1)];
    special.color = [f32::MAX, f32::MIN, 0.0];
    doc.drawing_mut(id)
        .unwrap()
        .push_stroke(Stroke::new(1, Brush::Ribbon, false, vec![special]));
    expected.push(special);

    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let loaded = load_project(dir.path()).unwrap().document;
    assert_documents_equal(&doc, &loaded);
    let restored: Vec<_> = loaded
        .drawing(id)
        .unwrap()
        .strokes()
        .flat_map(|s| s.points().iter().copied())
        .collect();
    assert_eq!(restored.len(), expected.len());
    assert!(expected.len() > 5_000);
    let bits = |p: &limner_core::model::Point| {
        [p.pos, p.normal, p.dir, p.color, [p.opacity, p.width, 0.0]].map(|v| v.map(f32::to_bits))
    };
    for (a, b) in expected.iter().zip(&restored) {
        assert_eq!(bits(a), bits(b));
    }
}

#[test]
fn values_json_cannot_spell_still_round_trip() {
    let mut doc = Document::new();
    let paint = doc.find_by_path("Root/Paint").unwrap();
    let layer = doc.layer_mut(paint).unwrap();
    layer.opacity = f32::NAN;
    layer.transform.scale = f64::INFINITY;
    layer.transform.translation = DVec3::new(f64::NEG_INFINITY, -0.0, 1e-320);
    doc.settings.background = [f32::INFINITY, -0.0, 1.0e-40];

    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let loaded = load_project(dir.path()).unwrap().document;
    assert_documents_equal(&doc, &loaded);
    assert!(loaded.layer(paint).unwrap().opacity.is_nan());
}

fn nested(depth: usize) -> Document {
    let mut doc = Document::empty();
    let mut parent = doc.root();
    for level in 1..=depth {
        let layer = doc.new_layer(
            format!("level {level}"),
            LayerKind::Group(Group::default()),
            0,
        );
        parent = doc.insert_layer(layer, parent, 0).unwrap();
    }
    doc
}

#[test]
fn nesting_is_supported_up_to_the_documented_limit() {
    let dir = tempfile::tempdir().unwrap();
    let deepest = nested(MAX_LAYER_DEPTH);
    save_project(&deepest, dir.path(), &fast()).unwrap();
    assert_documents_equal(&deepest, &load_project(dir.path()).unwrap().document);
    let before = snapshot(dir.path());

    // One level more is refused before anything is written, not saved and then unreadable.
    let error = save_project(&nested(MAX_LAYER_DEPTH + 1), dir.path(), &fast()).unwrap_err();
    assert!(matches!(error, FormatError::Unsaveable(_)), "{error}");
    assert!(error.to_string().contains("nested"), "{error}");
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn an_inconsistent_document_is_refused_and_the_project_is_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let good = exhaustive_document();
    save_project(&good, dir.path(), &fast()).unwrap();
    let before = snapshot(dir.path());

    let mut broken = good.clone();
    let layer = broken.find_by_path("Root/Body/Shared").unwrap();
    broken
        .layer_mut(layer)
        .unwrap()
        .as_paint_mut()
        .unwrap()
        .frames = vec![9];
    let error = save_project(&broken, dir.path(), &fast()).unwrap_err();
    assert!(matches!(error, FormatError::Unsaveable(_)), "{error}");
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn the_manifest_is_readable_stable_json_in_snake_case() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let text = std::fs::read_to_string(dir.path().join("limn.json")).unwrap();

    assert!(text.starts_with(
        "{\n  \"format\": \"limn\",\n  \"version\": 1,\n  \"generator\": \"limner-format "
    ));
    assert!(text.ends_with("}\n"));
    assert!(!text.contains('\r') && !text.contains('\t'));
    // One record per line where it helps a diff.
    assert!(text.contains("\"transform\": {\"rotation\": ["));
    assert!(text.contains("{\"time\": 705600000, \"value\": false}"));
    assert!(text.contains("\"interp\": \"ease_in_out\""));
    assert!(text.contains("\"projection\": \"equirect_stereo\""));
    assert!(text.contains("\"storage\": \"raw_image\""));
    assert!(text.contains("\"background\": [0.8, 0.1, 0.33333334]"));
    let blob_line = text
        .lines()
        .find(|line| line.contains("\"blob\""))
        .unwrap()
        .trim();
    assert!(
        blob_line.starts_with("{\"id\": ") && blob_line.len() < 120,
        "{blob_line}"
    );

    // Every key and every enumeration value is snake_case.
    fn check(value: &serde_json::Value, path: &str) {
        let snake = |s: &str| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        };
        match value {
            serde_json::Value::Object(map) => {
                for (key, child) in map {
                    let inside = format!("{path}/{key}");
                    // Channel names are user data, not schema.
                    if !path.ends_with("/channels") {
                        assert!(snake(key), "key {inside} is not snake_case");
                    }
                    let is_enum = [
                        "type",
                        "kind",
                        "storage",
                        "interp",
                        "flip",
                        "shading",
                        "projection",
                        "mode",
                    ]
                    .contains(&key.as_str());
                    if let (true, Some(text)) = (is_enum, child.as_str()) {
                        assert!(snake(text), "value {text} at {inside} is not snake_case");
                    }
                    check(child, &inside);
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|item| check(item, path)),
            _ => {}
        }
    }
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    check(&value, "");
    assert_eq!(value["next_id"].as_u64(), Some(doc.next_id()));
    assert_eq!(value["root"]["id"].as_u64(), Some(doc.root().0));
    assert_eq!(value["root"]["type"], "group");
}

#[test]
fn unknown_members_are_ignored_and_missing_ones_take_defaults() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let path = dir.path().join("limn.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    value["from_the_future"] = serde_json::json!({"anything": [1, 2, 3]});
    value["settings"]["hdr_background"] = serde_json::json!(true);
    value["root"]["blend_mode"] = serde_json::json!("multiply");
    value["root"]["children"][0]["transform"]["shear"] = serde_json::json!([0, 0, 0]);
    value["root"]["anim"]["actions"][0]["note"] = serde_json::json!("chapter one");
    value["drawings"][0]["bounds"] = serde_json::json!([0, 0, 0, 1, 1, 1]);
    value["media"][0]["license"] = serde_json::json!("CC0");
    std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
    let loaded = load_project(dir.path()).unwrap();
    assert_documents_equal(&doc, &loaded.document);

    // The smallest manifest: everything optional takes the model's defaults.
    let minimal = tempfile::tempdir().unwrap();
    std::fs::write(
        minimal.path().join("limn.json"),
        br#"{"format": "limn", "version": 1, "root": {"id": 1, "type": "group", "children": [
            {"id": 5, "type": "camera", "camera": {}},
            {"id": 3, "type": "light", "light": {"kind": "spot"}}]}}"#,
    )
    .unwrap();
    let loaded = load_project(minimal.path()).unwrap();
    let doc = loaded.document;
    assert_eq!(doc.walk(), vec![LayerId(1), LayerId(5), LayerId(3)]);
    assert_eq!(doc.next_id(), 6);
    assert_eq!(doc.settings, limner_core::model::Settings::default());
    let camera = doc.layer(LayerId(5)).unwrap();
    assert!(camera.visible && camera.opacity == 1.0 && camera.transform.is_identity());
    assert_eq!(camera.kind, LayerKind::Camera(Default::default()));
    let LayerKind::Light(light) = &doc.layer(LayerId(3)).unwrap().kind else {
        panic!("not a light");
    };
    assert_eq!(light.kind, limner_core::model::LightKind::Spot);
    assert_eq!(light.energy, limner_core::model::Light::default().energy);
    // The counter in the file was behind the ids in use, which is worth a warning.
    assert_eq!(loaded.warnings.len(), 1, "{:?}", loaded.warnings);
}

#[test]
fn the_default_new_document_round_trips() {
    let doc = Document::new();
    let dir = tempfile::tempdir().unwrap();
    let report = save_project(&doc, dir.path(), &fast()).unwrap();
    assert_eq!((report.blobs_written, report.media_written), (1, 0));
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);
    // No media: the folder is not created until it is needed.
    assert!(!dir.path().join("media").exists());
}

#[test]
fn identical_drawings_in_separate_allocations_share_one_blob() {
    let mut doc = Document::new();
    let root = doc.root();
    let strokes = || {
        (0..3).map(|i| {
            Stroke::new(
                i,
                Brush::Cylinder,
                false,
                limner_quill::testing::tube(i as f32, 20),
            )
        })
    };
    for name in ["One", "Two", "Three"] {
        let drawing = doc.add_drawing(Drawing::from_strokes(strokes()));
        let layer = doc.new_layer(
            name,
            LayerKind::Paint(Paint::single(drawing, FrameRate::whole(24))),
            0,
        );
        doc.insert_layer(layer, root, usize::MAX).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let report = save_project(&doc, dir.path(), &fast()).unwrap();
    // The three equal drawings, and the empty drawing of the default paint layer.
    assert_eq!(doc.drawing_count(), 4);
    assert_eq!(report.blobs_written, 2);
    assert_eq!(blob_files(dir.path()).len(), 2);
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);
}

/// The worked examples of `docs/format.md`, checked against the implementation.
#[test]
fn the_examples_in_the_specification_hold() {
    // The smallest project: one file.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("limn.json"),
        "{\n  \"format\": \"limn\",\n  \"version\": 1,\n  \"root\": {\"id\": 1, \"type\": \"group\"}\n}\n",
    )
    .unwrap();
    let doc = load_project(dir.path()).unwrap().document;
    assert_eq!(doc.layer_count(), 1);
    assert_eq!(doc.settings, limner_core::model::Settings::default());

    // The empty drawing is 24 bytes under a known name.
    let mut doc = Document::empty();
    let empty = doc.add_drawing(Drawing::new());
    let point = limner_quill::testing::point(1.0, 0.0, 0.0, 0.01);
    let second = limner_quill::testing::point(2.0, 0.0, 0.0, 0.01);
    let ribbon = doc.add_drawing(Drawing::from_strokes([Stroke::new(
        7,
        Brush::Ribbon,
        false,
        vec![point, second],
    )]));
    let root = doc.root();
    let layer = doc.new_layer(
        "L",
        LayerKind::Paint(Paint {
            rate: FrameRate::whole(24),
            drawings: vec![empty, ribbon],
            frames: vec![0, 1],
            max_repeat: 0,
            shading: Default::default(),
        }),
        0,
    );
    doc.insert_layer(layer, root, 0).unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();

    let name = "drawings/bd/bd5cbcc212c3a8c48180d006a35627ac743984463db7a5d3aaa4ea8746c74ac6.lstk";
    let files = snapshot(dir.path());
    let mut expected = b"LIMNSTK1".to_vec();
    expected.extend_from_slice(&[0u8; 16]);
    assert_eq!(files[name], expected);

    // One ribbon of two points: the header, the table record, then 56 planes of 2 bytes.
    let blob = &files[&blob_files(dir.path())
        .into_iter()
        .find(|n| n != name)
        .unwrap()];
    // The body is a zstd frame when the flags say so; the payload is the same either way.
    let payload: Vec<u8> = match blob[8] {
        1 => zstd::decode_all(&blob[24..]).unwrap(),
        _ => blob[24..].to_vec(),
    };
    assert_eq!(&blob[..8], b"LIMNSTK1");
    assert_eq!(&blob[12..24], &[1, 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(payload.len(), 12 + 112);
    assert_eq!(&payload[..12], &[7, 0, 0, 0, 2, 0, 0, 0, 1, 0, 0, 0]);
    assert_eq!(&payload[12..20], &[0, 0, 0, 0, 0x80, 0, 0x3F, 0x40]);
}
