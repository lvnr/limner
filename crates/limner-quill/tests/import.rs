use limner_core::glam::DVec3;
use limner_core::math::Flip;
use limner_core::model::{Action, Brush, Interp, LayerKind, MediaData, Projection};
use limner_core::time::{from_quill_ticks, FrameRate};
use limner_quill::testing::*;
use limner_quill::{import_project, is_quill_project, ImportError};
use serde_json::json;

#[test]
fn imports_a_modern_project() {
    let mut qbin = QbinBuilder::default();
    let thumbnail = qbin.picture_rgb(2, 2, &[255; 12]);
    let mut ribbon = StrokeSpec::new(1, 1, vec![point(0.0, 0.0, 0.0, 0.01); 3]);
    ribbon.visibility = 0;
    let drawing_a = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 5)), ribbon]);
    let drawing_b = qbin.drawing(&[StrokeSpec::new(0, 4, tube(1.0, 4))]);
    let reference = qbin.picture_rgb(2, 1, &[1, 2, 3, 4, 5, 6]);

    let mut spawn = layer(
        "InitialSpawnArea",
        "Viewpoint",
        json!({"Version": 1, "Color": [0.4, 0.3, 0.5], "Sphere": [0.0, 1.0, 0.0, 2.0], "AllowTranslationX": true, "AllowTranslationY": false,
               "AllowTranslationZ": true, "DataFileOffset": thumbnail, "Exporting": true, "ShowingVolume": false, "TypeStr": "FloorLevel"}),
    );
    spawn["Visible"] = json!(false);

    let camera = layer("Cam", "Camera", json!({"FOV": 37}));

    let mut sequence = group(
        "Loop",
        vec![paint("Anim", &[&drawing_a, &drawing_b], &[0, 0, 1])],
    );
    sequence["Animation"]["Timeline"] = json!(true);
    sequence["Animation"]["Duration"] = json!("16800");

    let mut body = group("Body", vec![sequence, paint("Shared", &[&drawing_a], &[0])]);
    body["Transform"] = xform([0.0, 0.0, 0.0, 1.0], 2.0, "X", [1.0, 2.0, 3.0]);
    body["Pivot"] = xform([0.0, 0.0, 0.0, 1.0], 1.0, "N", [0.5, 0.0, 0.0]);
    body["Opacity"] = json!(0.5);
    body["Animation"]["Keys"]["Transform"] = json!([
        {"Time": "0", "Value": xform([0.0, 0.0, 0.0, 1.0], 1.0, "N", [0.0, 0.0, 0.0]), "Interpolation": "EaseIn"},
        {"Time": "525", "Value": xform([0.0, 1.0, 0.0, 0.0], 3.0, "N", [0.0, 1.0, 0.0]), "Interpolation": "None"}
    ]);
    body["Animation"]["Keys"]["Opacity"] = json!([
        {"Time": "0", "Value": 0.0, "Interpolation": "Smoothstep"},
        {"Time": "1050", "Value": 1.0, "Interpolation": "Linear"}
    ]);
    body["Animation"]["Keys"]["Visibility"] = json!([
        {"Time": "0", "Value": true, "Interpolation": "None"},
        {"Time": "2100", "Value": false, "Interpolation": "None"}
    ]);
    body["Animation"]["Keys"]["Offset"] =
        json!([{"Time": "0", "Value": 525, "Interpolation": "None"}]);

    let picture = layer(
        "Ref",
        "Picture",
        json!({"Type": "2D", "ViewerLocked": true, "DataFileOffset": reference, "ImportFilePath": "C:\\art\\ref.png"}),
    );

    let mut quill = project(vec![spawn, camera, body, picture]);
    quill["Sequence"]["RootLayer"]["Animation"]["Keys"]["Action"] = json!([
        {"Time": "1050", "Value": "Stop", "Interpolation": "None"},
        {"Time": "2100", "Value": "Play", "Interpolation": "None"}
    ]);
    let palette: Vec<f64> = (0..48).map(|i| i as f64 / 48.0).collect();
    let state = json!({"Quill": {"PaintLayer": "Root/Body/Shared", "ToolName": "Paint", "Tool": {"Size": 0.01}, "ColorPalette": palette}});

    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("piece");
    write_project(&project_dir, &quill, &qbin.bytes, Some(&state));
    assert!(is_quill_project(&project_dir));
    assert!(is_quill_project(&project_dir.join("Quill.json")));

    let imported = import_project(&project_dir).unwrap();
    assert_eq!(imported.warnings, Vec::<String>::new());
    let doc = &imported.document;
    doc.validate().unwrap();

    let paths: Vec<String> = doc.walk().iter().map(|id| doc.path_of(*id)).collect();
    assert_eq!(
        paths,
        [
            "Root",
            "Root/InitialSpawnArea",
            "Root/Cam",
            "Root/Body",
            "Root/Body/Loop",
            "Root/Body/Loop/Anim",
            "Root/Body/Shared",
            "Root/Ref"
        ]
    );

    // Settings.
    assert_eq!(doc.settings.background, [0.1, 0.2, 0.3]);
    assert_eq!(doc.settings.rate, FrameRate::whole(24));
    assert_eq!(doc.settings.range_end, from_quill_ticks(126_000));
    assert_eq!(doc.settings.title, "piece");
    assert_eq!(doc.settings.palette.len(), 16);
    assert_eq!(
        doc.settings.palette[1],
        [3.0 / 48.0, 4.0 / 48.0, 5.0 / 48.0]
    );
    assert_eq!(
        doc.settings.default_viewpoint,
        doc.find_by_path("Root/InitialSpawnArea")
    );
    assert_eq!(
        imported.state.paint_layer.as_deref(),
        Some("Root/Body/Shared")
    );

    // Root: always the main clock, never looping on Quill's one-hour duration.
    let root = doc.layer(doc.root()).unwrap();
    assert_eq!(root.as_group().unwrap().sequence.unwrap().loop_duration, 0);
    let actions: Vec<_> = root
        .anim
        .actions
        .keys()
        .iter()
        .map(|k| (k.time, k.value))
        .collect();
    assert_eq!(
        actions,
        [
            (from_quill_ticks(1050), Action::Stop),
            (from_quill_ticks(2100), Action::Play)
        ]
    );

    // Spawn area and its thumbnail.
    let spawn = doc
        .layer(doc.find_by_path("Root/InitialSpawnArea").unwrap())
        .unwrap();
    assert!(!spawn.visible);
    let LayerKind::Viewpoint(viewpoint) = &spawn.kind else {
        panic!("not a viewpoint")
    };
    assert!(viewpoint.floor_level);
    assert_eq!(viewpoint.allow_translation, [true, false, true]);
    assert!(matches!(
        &doc.media(viewpoint.thumbnail.unwrap()).unwrap().data,
        MediaData::RawImage {
            width: 2,
            height: 2,
            channels: 3,
            ..
        }
    ));

    let LayerKind::Camera(cam) = &doc
        .layer(doc.find_by_path("Root/Cam").unwrap())
        .unwrap()
        .kind
    else {
        panic!("not a camera")
    };
    assert_eq!(cam.fov, 37.0);

    // Group with transform, pivot and keys.
    let body = doc.layer(doc.find_by_path("Root/Body").unwrap()).unwrap();
    assert!(!body.is_sequence());
    assert_eq!(body.opacity, 0.5);
    assert_eq!(body.transform.scale, 2.0);
    assert_eq!(body.transform.flip, Flip::X);
    assert_eq!(body.transform.translation, DVec3::new(1.0, 2.0, 3.0));
    assert_eq!(body.pivot.translation, DVec3::new(0.5, 0.0, 0.0));
    let transform_keys = body.anim.transform.keys();
    assert_eq!(transform_keys.len(), 2);
    assert_eq!(
        (transform_keys[0].time, transform_keys[0].interp),
        (0, Interp::EaseIn)
    );
    assert_eq!(
        (transform_keys[1].time, transform_keys[1].interp),
        (from_quill_ticks(525), Interp::Step)
    );
    assert_eq!(transform_keys[1].value.scale, 3.0);
    let opacity_keys = body.anim.opacity.keys();
    assert_eq!(
        (opacity_keys[0].value, opacity_keys[0].interp),
        (0.0, Interp::EaseInOut)
    );
    assert_eq!(
        (opacity_keys[1].time, opacity_keys[1].interp),
        (from_quill_ticks(1050), Interp::Linear)
    );
    let clip: Vec<_> = body
        .anim
        .visibility
        .keys()
        .iter()
        .map(|k| (k.time, k.value))
        .collect();
    assert_eq!(clip, [(0, true), (from_quill_ticks(2100), false)]);
    assert_eq!(body.anim.offset.keys()[0].value, from_quill_ticks(525));

    // Sequence with a loop point of 16,800 ticks (1.33 s).
    let sequence = doc
        .layer(doc.find_by_path("Root/Body/Loop").unwrap())
        .unwrap();
    assert_eq!(
        sequence.as_group().unwrap().sequence.unwrap().loop_duration,
        from_quill_ticks(16_800)
    );

    // Animated paint layer, and a second layer sharing its first drawing.
    let anim = doc
        .layer(doc.find_by_path("Root/Body/Loop/Anim").unwrap())
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    assert_eq!(anim.frames, [0, 0, 1]);
    assert_eq!(anim.max_repeat, 0);
    let shared = doc
        .layer(doc.find_by_path("Root/Body/Shared").unwrap())
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    assert_eq!(
        shared.drawings[0], anim.drawings[0],
        "one file record, one drawing"
    );
    assert_eq!(doc.drawing_count(), 2);

    let a = doc.drawing(anim.drawings[0]).unwrap();
    let strokes: Vec<_> = a.strokes().collect();
    assert_eq!(strokes.len(), 2);
    assert_eq!(
        (strokes[0].brush, strokes[0].directional, strokes[0].len()),
        (Brush::Cylinder, false, 5)
    );
    assert_eq!(
        strokes[0].points(),
        tube(0.0, 5).as_slice(),
        "points are kept bit for bit"
    );
    assert_eq!(
        (strokes[1].brush, strokes[1].directional),
        (Brush::Ribbon, true)
    );
    let b = doc.drawing(anim.drawings[1]).unwrap();
    assert_eq!(b.strokes().next().unwrap().brush, Brush::Cube);
    assert_eq!(doc.stroke_count(), 3);
    assert_eq!(doc.point_count(), 12);

    // Picture.
    let LayerKind::Picture(picture) = &doc
        .layer(doc.find_by_path("Root/Ref").unwrap())
        .unwrap()
        .kind
    else {
        panic!("not a picture")
    };
    assert_eq!(picture.projection, Projection::Flat);
    assert!(picture.viewer_locked);
    let media = doc.media(picture.media.unwrap()).unwrap();
    assert_eq!(media.name, "ref.png");
    let MediaData::RawImage {
        width,
        height,
        pixels,
        ..
    } = &media.data
    else {
        panic!("not an image")
    };
    assert_eq!(
        (*width, *height, &pixels[..]),
        (2, 1, &[1u8, 2, 3, 4, 5, 6][..])
    );
}

#[test]
fn imports_legacy_layouts() {
    let mut qbin = QbinBuilder::default();
    let offset = qbin.drawing(&[StrokeSpec::new(77, 2, tube(0.0, 6))]);
    let bare_offset = offset.trim_start_matches('0').to_owned();

    // Pre-1.7 layer: matrix transform, no pivot, no animation block, one drawing on the layer itself.
    let old_paint = json!({
        "Name": "Old", "Visible": true, "Locked": false, "Opacity": 1.0, "Type": "Paint",
        "Transform": [2.0, 0.0, 0.0, 5.0,  0.0, 2.0, 0.0, 6.0,  0.0, 0.0, 2.0, 7.0,  0.0, 0.0, 0.0, 1.0],
        "AnimOffset": 0,
        "Implementation": {"BoundingBox": [0.0, 1.0, 0.0, 1.0, 0.0, 1.0], "AnimSpeed": 12, "PlaybackReduce": 1, "DataFileOffset": bare_offset}
    });
    // A third-party writer: numeric times and frames.
    let mut numeric = paint("Numeric", &[&offset], &[0]);
    numeric["Implementation"]["Frames"] = json!([0, 0]);
    numeric["Animation"]["Duration"] = json!(0);
    numeric["Animation"]["Keys"]["Visibility"] =
        json!([{"Time": 1050, "Value": true, "Interpolation": "None"}]);
    numeric["Animation"]["Keys"]["Offset"] = json!([]);
    numeric["Animation"]["StartOffset"] = json!(525);

    let mut quill = project(vec![old_paint, numeric]);
    quill["Sequence"]["DefaultViewpoint"] = json!("");
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), &quill, &qbin.bytes, None);

    let imported = import_project(dir.path()).unwrap();
    assert_eq!(imported.warnings, Vec::<String>::new());
    let doc = &imported.document;

    let old = doc.layer(doc.find_by_path("Root/Old").unwrap()).unwrap();
    assert_eq!(old.transform.scale, 2.0);
    assert_eq!(old.transform.translation, DVec3::new(5.0, 6.0, 7.0));
    assert!(old.pivot.is_identity());
    assert_eq!(
        old.anim.visibility.keys().len(),
        1,
        "a layer without a timeline is simply present"
    );
    let old_paint = old.as_paint().unwrap();
    assert_eq!(old_paint.frames, [0]);
    assert_eq!(
        doc.drawing(old_paint.drawings[0])
            .unwrap()
            .strokes()
            .next()
            .unwrap()
            .id,
        77
    );

    let numeric = doc
        .layer(doc.find_by_path("Root/Numeric").unwrap())
        .unwrap();
    assert_eq!(
        numeric.anim.visibility.keys()[0].time,
        from_quill_ticks(1050)
    );
    assert_eq!(numeric.as_paint().unwrap().frames, [0, 0]);
    // A start offset with no offset key becomes an offset key on the first clip.
    let offsets: Vec<_> = numeric
        .anim
        .offset
        .keys()
        .iter()
        .map(|k| (k.time, k.value))
        .collect();
    assert_eq!(offsets, [(from_quill_ticks(1050), from_quill_ticks(525))]);
    assert_eq!(
        numeric.as_paint().unwrap().drawings,
        old_paint.drawings,
        "padded and bare offsets name the same record"
    );
}

#[test]
fn damaged_projects_import_with_warnings() {
    let mut qbin = QbinBuilder::default();
    let good = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 4))]);

    let mut bad_frames = paint("BadFrames", &[&good], &[0]);
    bad_frames["Implementation"]["Frames"] = json!(["0", "7"]);
    let far = paint("Far", &["00000000ffffffff"], &[0]);
    let unknown = layer("Mystery", "Hologram", json!({}));
    let mut eased = group("Eased", vec![]);
    eased["Animation"]["Keys"]["Opacity"] =
        json!([{"Time": "0", "Value": 1.0, "Interpolation": "Wobble"}]);
    eased["Transform"] = json!({"Scale": 0.0});

    let quill = project(vec![bad_frames, far, unknown, eased]);
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), &quill, &qbin.bytes, None);

    let imported = import_project(dir.path()).unwrap();
    let doc = &imported.document;
    doc.validate().unwrap();
    assert!(doc.find_by_path("Root/Mystery").is_none());
    assert_eq!(
        doc.layer(doc.find_by_path("Root/BadFrames").unwrap())
            .unwrap()
            .as_paint()
            .unwrap()
            .frames,
        [0, 0]
    );
    let far = doc
        .layer(doc.find_by_path("Root/Far").unwrap())
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    assert!(doc.drawing(far.drawings[0]).unwrap().is_empty());
    assert!(doc
        .layer(doc.find_by_path("Root/Eased").unwrap())
        .unwrap()
        .transform
        .is_identity());

    let all = imported.warnings.join("\n");
    for expected in [
        "frames pointed at missing drawings",
        "outside Quill.qbin",
        "unknown type \"Hologram\"",
        "unknown interpolation \"Wobble\"",
        "unreadable transform",
        "default spawn area",
    ] {
        assert!(
            all.contains(expected),
            "missing warning about {expected:?} in:\n{all}"
        );
    }
}

#[test]
fn rejects_things_that_are_not_projects() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        import_project(dir.path()),
        Err(ImportError::NotAProject(_))
    ));
    std::fs::write(dir.path().join("Quill.json"), b"{ not json").unwrap();
    assert!(matches!(
        import_project(dir.path()),
        Err(ImportError::Json { .. })
    ));
    std::fs::write(dir.path().join("Quill.json"), b"{\"Version\": 1}").unwrap();
    assert!(matches!(
        import_project(dir.path()),
        Err(ImportError::NotAProject(_))
    ));
}

#[test]
fn a_missing_qbin_is_a_warning() {
    let quill = project(vec![paint("P", &["0000000000000008"], &[0])]);
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), &quill, &[], None);
    std::fs::remove_file(dir.path().join("Quill.qbin")).unwrap();
    let imported = import_project(dir.path()).unwrap();
    assert!(imported
        .warnings
        .iter()
        .any(|w| w.contains("Quill.qbin is missing")));
    assert_eq!(imported.document.stroke_count(), 0);
}

/// Runs against the project owner's private sample when it is present on this machine.
#[test]
fn imports_the_local_sample_exactly() {
    let dir = std::env::var_os("LIMNER_SAMPLE_DIR")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| {
                std::path::PathBuf::from(home).join("projects/quill-exporter/sample")
            })
        });
    let Some(dir) = dir.filter(|d| d.join("Quill.json").is_file()) else {
        eprintln!("skipped: no local sample project");
        return;
    };
    let imported = import_project(&dir).unwrap();
    let doc = &imported.document;
    doc.validate().unwrap();
    assert_eq!(imported.warnings, Vec::<String>::new());

    let count = |name: &str| doc.layers().filter(|l| l.type_name() == name).count();
    assert_eq!(doc.layer_count(), 94);
    assert_eq!(count("Group") + count("Sequence"), 37);
    assert_eq!(count("Paint"), 55);
    assert_eq!(count("Viewpoint"), 1);
    assert_eq!(count("Camera"), 1);
    assert_eq!(doc.drawing_count(), 70);
    assert_eq!(doc.stroke_count(), 2_198);
    assert_eq!(doc.point_count(), 82_524);
    assert_eq!(
        doc.layers()
            .filter(|l| !l.anim.transform.is_empty())
            .count(),
        18
    );
    assert_eq!(
        doc.settings.default_viewpoint,
        doc.find_by_path("Root/InitialSpawnArea")
    );
}
