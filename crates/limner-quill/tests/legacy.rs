//! Old schemas, version quirks and files that passed through other tools.

use limner_core::glam::DVec3;
use limner_core::model::{Brush, PALETTE_SIZE};
use limner_core::time::FrameRate;
use limner_quill::testing::*;
use limner_quill::{import_project, Taper};
use serde_json::{json, Value};

/// A project in the January 2017 schema: three paint layers, one inside a group, with
/// stroke ids that count up across the whole file.
fn january_2017() -> (Value, QbinBuilder) {
    let mut qbin = QbinBuilder::with_stroke_counter(42);
    let mut ribbon = StrokeSpec::new(7, 1, vec![point(0.0, 0.0, 0.0, 0.01); 5]);
    ribbon.visibility = 0;
    // No zero-width end points: strokes of that era start and stop at full width.
    let first = qbin.drawing(&[
        StrokeSpec::new(3, 3, vec![point(0.0, 1.0, 0.0, 0.02); 6]),
        ribbon,
    ]);
    let second = qbin.drawing(&[
        StrokeSpec::new(9, 2, tube(1.0, 4)),
        StrokeSpec::new(41, 4, tube(2.0, 4)),
    ]);
    let empty = qbin.drawing(&[]);

    let mut hidden = paint_2017("Sketch", matrix(1.0, [0.0; 3]), &empty);
    hidden["Visible"] = json!(false);
    hidden["Opacity"] = json!(0.25);
    let folder = layer_2017(
        "Folder",
        "Group",
        matrix(0.5, [1.0, 2.0, 3.0]),
        json!({"Children": [paint_2017("Inner", matrix(1.0, [0.0; 3]), &second)]}),
    );
    let quill = project_2017(vec![
        paint_2017("Paint", matrix(2.0, [-0.5, 0.25, 4.0]), &first),
        folder,
        hidden,
    ]);
    (quill, qbin)
}

#[test]
fn imports_the_january_2017_schema() {
    let (quill, qbin) = january_2017();
    assert_eq!(&qbin.bytes[..8], &[42, 0, 0, 0, 0, 0, 0, 0]);
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("happy_new_year");
    write_project(&project_dir, &quill, &qbin.bytes, Some(&state_2017()));

    let imported = import_project(&project_dir).unwrap();
    assert_eq!(imported.warnings, Vec::<String>::new());
    let doc = &imported.document;
    doc.validate().unwrap();

    let paths: Vec<String> = doc.walk().iter().map(|id| doc.path_of(*id)).collect();
    assert_eq!(
        paths,
        [
            "Root",
            "Root/Paint",
            "Root/Folder",
            "Root/Folder/Inner",
            "Root/Sketch"
        ]
    );

    // Settings the old schema does not have keep their defaults.
    assert_eq!(doc.settings.title, "happy_new_year");
    assert_eq!(doc.settings.background, [0.1, 0.2, 0.3]);
    assert_eq!(doc.settings.rate, FrameRate::whole(24));
    assert_eq!(doc.settings.range_start, 0);
    assert_eq!(doc.settings.default_viewpoint, None);
    assert_eq!(doc.settings.palette, vec![[0.82f32; 3]; PALETTE_SIZE]);

    // Matrix transforms; no pivot; no clips, so every layer is present from the start.
    let paint = doc.layer(doc.find_by_path("Root/Paint").unwrap()).unwrap();
    assert_eq!(paint.transform.scale, 2.0);
    assert_eq!(paint.transform.translation, DVec3::new(-0.5, 0.25, 4.0));
    assert!(paint.pivot.is_identity());
    let clip: Vec<_> = paint
        .anim
        .visibility
        .keys()
        .iter()
        .map(|k| (k.time, k.value))
        .collect();
    assert_eq!(clip, [(0, true)]);
    assert!(!paint.anim.has_motion_keys());
    let folder = doc.layer(doc.find_by_path("Root/Folder").unwrap()).unwrap();
    assert_eq!(folder.transform.scale, 0.5);
    assert_eq!(folder.transform.translation, DVec3::new(1.0, 2.0, 3.0));
    assert!(!folder.is_sequence());
    let sketch = doc.layer(doc.find_by_path("Root/Sketch").unwrap()).unwrap();
    assert!(!sketch.visible);
    assert_eq!(sketch.opacity, 0.25);

    // One drawing per layer, one frame, at the document's rate.
    let layer = paint.as_paint().unwrap();
    assert_eq!(
        (layer.drawings.len(), layer.frames.as_slice()),
        (1, &[0u32][..])
    );
    assert_eq!(layer.rate, FrameRate::whole(24));
    // Stroke ids are global in these files and are kept as they are.
    let strokes: Vec<_> = doc.drawing(layer.drawings[0]).unwrap().strokes().collect();
    assert_eq!(
        strokes
            .iter()
            .map(|s| (s.id, s.brush, s.directional, s.len()))
            .collect::<Vec<_>>(),
        [(3, Brush::Ellipse, false, 6), (7, Brush::Ribbon, true, 5)]
    );
    assert_eq!(
        strokes[0].points(),
        vec![point(0.0, 1.0, 0.0, 0.02); 6].as_slice()
    );
    let inner = doc
        .layer(doc.find_by_path("Root/Folder/Inner").unwrap())
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    let ids: Vec<u32> = doc
        .drawing(inner.drawings[0])
        .unwrap()
        .strokes()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, [9, 41]);
    assert!(doc
        .drawing(sketch.as_paint().unwrap().drawings[0])
        .unwrap()
        .is_empty());
    assert_eq!(
        (doc.drawing_count(), doc.stroke_count(), doc.point_count()),
        (3, 4, 19)
    );

    // Editor state: a tool number instead of a name, and boolean tapers.
    let state = &imported.state;
    assert_eq!(state.tool_id, Some(0));
    assert_eq!(state.tool_name.as_deref(), Some("Paint"));
    assert_eq!(state.show_grid, Some(false));
    assert_eq!(state.direct_manipulation, Some(false));
    assert_eq!(state.paint_layer.as_deref(), Some("Root/Paint"));
    assert_eq!(state.settings.width_taper, Some(Taper::Pressure));
    assert_eq!(state.settings.color, Some([0.8, 0.5, 0.1]));
}

#[test]
fn what_the_2017_schema_cannot_carry_over_is_reported() {
    let (mut quill, qbin) = january_2017();
    // Moved and turned a quarter about Y.
    quill["Sequence"]["HomePosition"] =
        json!([0.0, 0.0, 1.0, 2.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 1.0]);
    quill["Sequence"]["TrackingOrigin"] = json!(0);
    let children = &mut quill["Sequence"]["RootLayer"]["Implementation"]["Children"];
    children[0]["AnimOffset"] = json!(1.5);
    // Twice as wide as it is tall.
    children[1]["Transform"] =
        json!([2.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 1.0, 3.0, 0.0, 0.0, 0.0, 1.0]);
    children[2]["Transform"] = json!([1.0, 0.0, 0.0]);

    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), &quill, &qbin.bytes, None);
    let imported = import_project(dir.path()).unwrap();
    imported.document.validate().unwrap();
    let doc = &imported.document;
    let folder = doc.layer(doc.find_by_path("Root/Folder").unwrap()).unwrap();
    assert_eq!(folder.transform.translation, DVec3::new(1.0, 2.0, 3.0));
    assert!((folder.transform.scale - 4.0 / 3.0).abs() < 1e-12);

    assert_eq!(
        imported.warnings,
        [
            "the home position of this 2017 file is not imported; viewers start at the origin",
            "the tracking origin of this 2017 file (0) is not imported; floor level is assumed",
            "Root/Paint: the animation offset of this 2017 layer (1.5) is not imported",
            "Root/Folder: the transform matrix has uneven scale, shear or perspective; the nearest uniform transform is used",
            "Root/Sketch: unreadable transform; identity used",
        ]
    );
}

#[test]
fn quill_1x_clip_blocks_are_reported_not_guessed() {
    let mut qbin = QbinBuilder::default();
    let drawing = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 4))]);
    let mut old = paint("Old", &[&drawing], &[0, 0]);
    old["Animation"] = json!({"Spans": [{"Start": 0, "End": 1000}], "Frames": [0, 0]});
    let mut bare = group("Bare", vec![]);
    bare["Animation"] = json!({"Duration": 25200, "Timeline": true});
    let mut quill = project(vec![old, bare]);
    quill["Sequence"]["DefaultViewpoint"] = Value::Null;

    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), &quill, &qbin.bytes, None);
    let imported = import_project(dir.path()).unwrap();
    let doc = &imported.document;
    assert_eq!(
        imported.warnings,
        ["Root/Old: the clips of this Quill 1.x layer are in a layout this importer does not know; the layer is shown for the whole timeline"]
    );
    let old = doc.layer(doc.find_by_path("Root/Old").unwrap()).unwrap();
    assert_eq!(old.anim.visibility.keys().len(), 1);
    assert_eq!(old.as_paint().unwrap().frames, [0, 0]);
    // An animation block with no keys still says whether the group runs its own clock.
    let bare = doc.layer(doc.find_by_path("Root/Bare").unwrap()).unwrap();
    assert!(bare.is_sequence());
    assert_eq!(bare.anim.visibility.keys().len(), 1);
}

#[test]
fn oculus_era_extras_are_dropped_with_one_warning_each() {
    let mut qbin = QbinBuilder::default();
    let poster = qbin.picture_rgb(1, 1, &[1, 2, 3]);
    let mut wiggly = group("Wiggly", vec![]);
    wiggly["KeepAlive"] = json!({"Type": "Wiggle", "Frequency": 2.0});
    let mut quill = project(vec![wiggly]);
    quill["Version"] = json!(2);
    let sequence = &mut quill["Sequence"];
    sequence["DefaultViewpoint"] = json!("");
    sequence["Framerate"] = json!(0.0001);
    sequence["Metadata"] = json!({"Title": "Untitled", "Description": "Shared from Quill", "ThumbnailCropPosition": 0.5});
    sequence["Gallery"] = json!({
        "Thumbnails": {"Picture_2D_Type1": 1},
        "Pictures": [{"Type": "2D", "DataFileOffset": poster, "Metadata": {}},
                     {"Type": "2D", "DataFileOffset": poster, "Metadata": {}}]
    });

    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("lighthouse");
    write_project(&project_dir, &quill, &qbin.bytes, None);
    let imported = import_project(&project_dir).unwrap();
    let doc = &imported.document;
    doc.validate().unwrap();
    // The sharing dialog's placeholders are not a title or a description.
    assert_eq!(doc.settings.title, "lighthouse");
    assert_eq!(doc.settings.description, "");
    assert_eq!(
        doc.settings.rate,
        FrameRate::whole(24),
        "an unusable rate is ignored"
    );
    assert_eq!(doc.media_items().count(), 0);
    assert_eq!(
        imported.warnings,
        [
            "Quill.json declares format version 2; this importer was written for version 1",
            "2 gallery poster(s) saved for Quill's sharing feature were not imported",
            "Root/Wiggly: the keep-alive behaviour \"Wiggle\" is not imported",
        ]
    );

    // A real title and description are kept.
    quill["Sequence"]["Metadata"] = json!({"Title": "Lighthouse at dusk", "Description": "Study"});
    write_project(&project_dir, &quill, &qbin.bytes, None);
    let doc = import_project(&project_dir).unwrap().document;
    assert_eq!(doc.settings.title, "Lighthouse at dusk");
    assert_eq!(doc.settings.description, "Study");
}

#[test]
fn files_renamed_or_re_saved_by_other_tools_still_open() {
    let mut qbin = QbinBuilder::default();
    let drawing = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 4))]);
    let mut quill = project(vec![paint("P", &[&drawing], &[0])]);
    quill["Sequence"]["DefaultViewpoint"] = json!("");
    let state = json!({"Quill": {"ToolName": "Select", "ColorPalette": vec![0.5; 48]}});

    // Lower-case names and a byte-order mark in front of both JSON files.
    let dir = tempfile::tempdir().unwrap();
    let project_dir = dir.path().join("renamed");
    std::fs::create_dir_all(&project_dir).unwrap();
    let with_bom = |value: &Value| {
        let mut bytes = b"\xef\xbb\xbf".to_vec();
        bytes.extend_from_slice(&serde_json::to_vec(value).unwrap());
        bytes
    };
    std::fs::write(project_dir.join("quill.json"), with_bom(&quill)).unwrap();
    std::fs::write(project_dir.join("QUILL.QBIN"), &qbin.bytes).unwrap();
    std::fs::write(project_dir.join("state.json"), with_bom(&state)).unwrap();

    assert!(limner_quill::is_quill_project(&project_dir));
    for path in [
        project_dir.clone(),
        project_dir.join("quill.json"),
        project_dir.join("QUILL.QBIN"),
    ] {
        let imported = import_project(&path).unwrap();
        assert_eq!(
            imported.warnings,
            Vec::<String>::new(),
            "{}",
            path.display()
        );
        assert_eq!(imported.document.stroke_count(), 1);
        assert_eq!(imported.state.tool_name.as_deref(), Some("Select"));
        assert_eq!(imported.document.settings.palette[0], [0.5; 3]);
    }

    // A broken State.json costs the editor state, never the artwork.
    std::fs::write(project_dir.join("state.json"), b"{ nope").unwrap();
    let imported = import_project(&project_dir).unwrap();
    assert_eq!(
        imported.warnings,
        ["State.json could not be read; editor state was not imported"]
    );
    assert_eq!(imported.document.stroke_count(), 1);
}
