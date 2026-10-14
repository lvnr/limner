mod common;

use common::*;
use limner_core::model::{Brush, Drawing, LayerKind, Paint, Stroke};
use limner_core::time::FrameRate;
use limner_format::testing::{assert_documents_equal, exhaustive_document};
use limner_format::{load_project, save_project, SaveOptions};

#[test]
fn an_unchanged_document_writes_nothing() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let before = snapshot(dir.path());
    let modified = |name: &str| {
        std::fs::metadata(dir.path().join(name))
            .unwrap()
            .modified()
            .unwrap()
    };
    let manifest_time = modified("limn.json");

    for _ in 0..2 {
        let report = save_project(&doc, dir.path(), &fast()).unwrap();
        assert!(report.wrote_nothing(), "{report}");
        assert_eq!(report.blobs_written + report.media_written, 0);
        assert_eq!(report.bytes_written, 0);
        assert_eq!(report.files_removed, 0);
    }
    assert_eq!(snapshot(dir.path()), before);
    assert_eq!(modified("limn.json"), manifest_time);
}

#[test]
fn editing_one_drawing_writes_exactly_one_blob() {
    let mut doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let before = snapshot(dir.path());

    let big = drawing_with_strokes(&doc, 30);
    let old_blob = blob_file(dir.path(), &doc, big);
    nudge(&mut doc, big);
    let new_blob = blob_file(dir.path(), &doc, big);
    assert_ne!(old_blob, new_blob);
    assert!(old_blob.is_file() && !new_blob.exists());

    let report = save_project(&doc, dir.path(), &fast()).unwrap();
    assert_eq!((report.blobs_written, report.blobs_reused), (1, 4));
    assert_eq!((report.media_written, report.media_reused), (0, 11));
    assert!(report.manifest_written);
    // The blob of the previous version is no longer referenced.
    assert_eq!(report.files_removed, 1);
    assert!(new_blob.is_file() && !old_blob.exists());

    // Nothing else in the folder was touched.
    let after = snapshot(dir.path());
    let changed: Vec<&String> = after
        .iter()
        .filter(|(name, bytes)| before.get(*name) != Some(*bytes))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(changed.len(), 2, "{changed:?}");
    assert!(changed.iter().any(|name| *name == "limn.json"));
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);
}

#[test]
fn a_change_to_a_layer_rewrites_only_the_manifest() {
    let mut doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let before = std::fs::read_to_string(dir.path().join("limn.json")).unwrap();

    let camera = doc.find_by_path("Root/Camera").unwrap();
    doc.layer_mut(camera).unwrap().opacity = 0.25;
    let report = save_project(&doc, dir.path(), &fast()).unwrap();
    assert_eq!((report.blobs_written, report.media_written), (0, 0));
    assert!(report.manifest_written);

    // And the change is a one-line diff.
    let after = std::fs::read_to_string(dir.path().join("limn.json")).unwrap();
    let differing: Vec<(&str, &str)> = before
        .lines()
        .zip(after.lines())
        .filter(|(a, b)| a != b)
        .collect();
    assert_eq!(before.lines().count(), after.lines().count());
    assert_eq!(differing.len(), 1, "{differing:?}");
    assert_eq!(differing[0].1.trim(), "\"opacity\": 0.25,");
}

#[test]
fn garbage_collection_removes_only_unreferenced_files() {
    let mut doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();

    // Files of the user's, in every folder a save works in.
    let keep = [
        "notes.txt",
        ".gitattributes",
        "state.json",
        "thumbnail.png",
        "drawings/README",
        "drawings/zz/hand-made.lstk",
        "media/reference-sheet.png",
        "exports/shot.png",
    ];
    for name in keep {
        let path = dir.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"mine").unwrap();
    }
    let with_extras = file_names(dir.path());

    // Without the option nothing is deleted, however stale.
    let big = drawing_with_strokes(&doc, 30);
    let stale_blob = blob_file(dir.path(), &doc, big);
    nudge(&mut doc, big);
    // Dropping the orphan media item and an orphan drawing makes their files stale too.
    let stale_drawing = drawing_with_strokes(&doc, 2);
    let stale_orphan = [
        blob_file(dir.path(), &doc, stale_drawing),
        blob_file(dir.path(), &doc, drawing_with_strokes(&doc, 0)),
    ];
    let media_before: Vec<String> = file_names(dir.path())
        .into_iter()
        .filter(|n| n.starts_with("media/"))
        .collect();
    assert_eq!(doc.collect_garbage(), 2);
    let keep_all = SaveOptions {
        collect_garbage: false,
        ..fast()
    };
    let report = save_project(&doc, dir.path(), &keep_all).unwrap();
    assert_eq!(report.files_removed, 0);
    assert!(stale_blob.is_file() && stale_orphan[0].is_file());
    assert_eq!(file_names(dir.path()).len(), with_extras.len() + 1);

    // With it, exactly the stale content files go: two drawings and one media file.
    let report = save_project(&doc, dir.path(), &fast()).unwrap();
    assert!(report.wrote_nothing());
    assert_eq!(report.files_removed, 3);
    assert!(!stale_blob.exists() && !stale_orphan[0].exists());
    // The empty drawing is still used by other layers.
    assert!(stale_orphan[1].is_file());
    let names = file_names(dir.path());
    for name in keep {
        assert!(names.iter().any(|n| n == name), "{name} was removed");
    }
    let media_after: Vec<&String> = names.iter().filter(|n| n.starts_with("media/")).collect();
    assert_eq!(media_after.len(), media_before.len() - 1);
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);

    // Every remaining blob is one the manifest names, plus the hand-made file.
    let manifest = std::fs::read_to_string(dir.path().join("limn.json")).unwrap();
    for blob in blob_files(dir.path()) {
        let stem = blob.rsplit('/').next().unwrap().trim_end_matches(".lstk");
        assert!(manifest.contains(stem) || stem == "hand-made", "{blob}");
    }
}

#[test]
fn a_new_frame_reusing_a_drawing_adds_no_blob() {
    let mut doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    let blobs = blob_files(dir.path());

    // A new layer showing a copy of an existing drawing, and a brand new drawing.
    let root = doc.root();
    let big = drawing_with_strokes(&doc, 30);
    let copy = doc.add_drawing(Drawing::clone(doc.drawing(big).unwrap()));
    let fresh = doc.add_drawing(Drawing::from_strokes([Stroke::new(
        0,
        Brush::Ribbon,
        false,
        limner_quill::testing::tube(0.0, 8),
    )]));
    let layer = doc.new_layer(
        "More",
        LayerKind::Paint(Paint {
            rate: FrameRate::whole(24),
            drawings: vec![copy, fresh],
            frames: vec![0, 1],
            max_repeat: 0,
            shading: Default::default(),
        }),
        0,
    );
    doc.insert_layer(layer, root, usize::MAX).unwrap();

    let report = save_project(&doc, dir.path(), &fast()).unwrap();
    assert_eq!((report.blobs_written, report.blobs_reused), (1, 5));
    assert_eq!(blob_files(dir.path()).len(), blobs.len() + 1);
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);
}
