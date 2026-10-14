mod common;

use common::*;
use limner_core::model::{LayerKind, Light};
use limner_format::testing::{assert_documents_equal, exhaustive_document};
use limner_format::{load_project, save_project, SaveOptions};
use std::path::Path;

/// The next version of the test document: one drawing edited, one layer added, one
/// media item dropped, so the save has new files to write and old ones to retire.
fn next_version(doc: &limner_core::model::Document) -> limner_core::model::Document {
    let mut next = doc.clone();
    nudge(&mut next, drawing_with_strokes(doc, 30));
    let root = next.root();
    let layer = next.new_layer("Key light", LayerKind::Light(Light::default()), 0);
    next.insert_layer(layer, root, 0).unwrap();
    next.collect_garbage();
    next
}

fn leftovers(dir: &Path) -> Vec<String> {
    file_names(dir)
        .into_iter()
        .filter(|name| name.ends_with(".tmp"))
        .collect()
}

#[test]
fn a_save_interrupted_before_the_manifest_is_replaced_leaves_the_old_project() {
    let old = exhaustive_document();
    let new = next_version(&old);
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("work.limn");
    save_project(&old, &project, &fast()).unwrap();
    let old_files = snapshot(&project);

    // What the complete save of the new version looks like, written elsewhere.
    let reference = dir.path().join("reference.limn");
    save_project(&new, &reference, &fast()).unwrap();
    let new_files = snapshot(&reference);

    // The interrupted save got this far: every new content file is in place under
    // its final name, a further one was cut off mid-write, and the new manifest was
    // being written to its temporary name when the power went.
    let mut new_content = 0;
    for (name, bytes) in &new_files {
        if name != "limn.json" && !old_files.contains_key(name) {
            let path = project.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
            new_content += 1;
        }
    }
    assert_eq!(new_content, 1);
    let new_manifest = &new_files["limn.json"];
    std::fs::write(
        project.join("limn.json.tmp"),
        &new_manifest[..new_manifest.len() / 2],
    )
    .unwrap();
    let shard = project.join("drawings").join("0f");
    std::fs::create_dir_all(&shard).unwrap();
    std::fs::write(
        shard.join(format!("{}.lstk.tmp", "0f".repeat(32))),
        b"LIMNSTK1 half a bl",
    )
    .unwrap();
    std::fs::write(
        project
            .join("media")
            .join(format!("{}.png.tmp", "ab".repeat(32))),
        b"\x89PNG",
    )
    .unwrap();
    std::fs::write(project.join("state.json.tmp"), b"{\"view\":").unwrap();
    assert_eq!(leftovers(&project).len(), 4);

    // The project is still the old one, complete and exact.
    let loaded = load_project(&project).unwrap();
    assert_documents_equal(&old, &loaded.document);
    assert_eq!(loaded.warnings.len(), 1);
    assert!(
        loaded.warnings[0].contains("interrupted"),
        "{:?}",
        loaded.warnings
    );
    assert_eq!(snapshot(&project)["limn.json"], old_files["limn.json"]);

    // Saving again completes the work: the blob that made it is reused, the
    // leftovers and the retired files are removed, and the folder is exactly what
    // an uninterrupted save produces.
    let report = save_project(&new, &project, &fast()).unwrap();
    assert_eq!(report.blobs_written, 0);
    assert_eq!(report.media_written, 0);
    assert!(report.manifest_written);
    // Three temporary files (the fourth, `limn.json.tmp`, was overwritten and renamed
    // by the save itself), the edited drawing's old blob, the orphan drawing's blob
    // and the orphan media file.
    assert_eq!(report.files_removed, 6);
    assert!(leftovers(&project).is_empty());
    assert_eq!(snapshot(&project), new_files);
    let loaded = load_project(&project).unwrap();
    assert_eq!(loaded.warnings, Vec::<String>::new());
    assert_documents_equal(&new, &loaded.document);
}

#[test]
fn a_save_interrupted_at_any_file_boundary_leaves_a_loadable_project() {
    let old = exhaustive_document();
    let mut new = next_version(&old);
    // More new content, so there are several boundaries to stop at.
    nudge(&mut new, drawing_with_strokes(&old, 6));
    let dir = tempfile::tempdir().unwrap();
    let reference = dir.path().join("reference.limn");
    save_project(&old, &reference, &fast()).unwrap();
    let old_files = snapshot(&reference);
    let keep_all = SaveOptions {
        collect_garbage: false,
        ..fast()
    };
    save_project(&new, &reference, &keep_all).unwrap();
    let both = snapshot(&reference);
    let added: Vec<&String> = both
        .keys()
        .filter(|name| !old_files.contains_key(*name))
        .collect();
    assert!(added.len() >= 2, "{added:?}");

    // Stop after 0, 1, 2, … of the new content files: the manifest is still the old one.
    for written in 0..=added.len() {
        let project = dir.path().join(format!("stopped-{written}.limn"));
        save_project(&old, &project, &fast()).unwrap();
        for name in &added[..written] {
            let path = project.join(name.as_str());
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, &both[*name]).unwrap();
        }
        assert_documents_equal(&old, &load_project(&project).unwrap().document);
    }

    // Stop right after the manifest was replaced, before stale files were removed:
    // the project is the new one, with the old files still lying around.
    let project = dir.path().join("stopped-after-manifest.limn");
    save_project(&old, &project, &fast()).unwrap();
    save_project(&new, &project, &keep_all).unwrap();
    assert_eq!(snapshot(&project), both);
    assert_documents_equal(&new, &load_project(&project).unwrap().document);
    let report = save_project(&new, &project, &fast()).unwrap();
    assert!(report.wrote_nothing());
    assert!(report.files_removed >= 2);
    assert_documents_equal(&new, &load_project(&project).unwrap().document);
}

#[test]
fn a_truncated_temporary_manifest_is_never_read() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    std::fs::write(dir.path().join("limn.json.tmp"), b"{ this is not json").unwrap();
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);
    // A temporary file with no manifest beside it is not a project at all.
    let empty = tempfile::tempdir().unwrap();
    std::fs::write(empty.path().join("limn.json.tmp"), b"{}").unwrap();
    assert!(!limner_format::is_limn_project(empty.path()));
    assert!(load_project(empty.path()).is_err());
}

#[test]
fn the_default_options_flush_to_disk_and_clean_up() {
    // Everything above skips the flushes for speed; this runs the real thing once.
    let options = SaveOptions::default();
    assert!(options.sync && options.collect_garbage);
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("limn.json.tmp"), b"junk").unwrap();
    std::fs::write(dir.path().join("state.json.tmp"), b"junk").unwrap();
    let report = save_project(&doc, dir.path(), &options).unwrap();
    assert_eq!(report.files_removed, 1);
    assert!(leftovers(dir.path()).is_empty());
    assert_documents_equal(&doc, &load_project(dir.path()).unwrap().document);
    assert!(save_project(&doc, dir.path(), &options)
        .unwrap()
        .wrote_nothing());
}

#[test]
fn a_save_that_fails_at_the_manifest_has_written_content_and_nothing_else() {
    // Not a simulation: the real save is made to fail at the step that replaces the
    // manifest, by putting a folder where its temporary file has to go.
    let old = exhaustive_document();
    let new = next_version(&old);
    let dir = tempfile::tempdir().unwrap();
    save_project(&old, dir.path(), &fast()).unwrap();
    let before = snapshot(dir.path());
    let obstacle = dir.path().join("limn.json.tmp");
    std::fs::create_dir(&obstacle).unwrap();

    let error = save_project(&new, dir.path(), &fast()).unwrap_err();
    assert!(
        matches!(error, limner_format::FormatError::Io { .. }),
        "{error}"
    );
    assert!(error.to_string().contains("limn.json.tmp"), "{error}");

    // New content was written before the manifest was attempted...
    let new_blob = blob_file(dir.path(), &new, drawing_with_strokes(&new, 30));
    assert!(new_blob.is_file());
    // ...and nothing that existed was changed or removed: no garbage was collected
    // on behalf of a manifest that never arrived.
    let after = snapshot(dir.path());
    assert_eq!(after.len(), before.len() + 1);
    for (name, bytes) in &before {
        assert_eq!(after.get(name), Some(bytes), "{name}");
    }
    assert_documents_equal(&old, &load_project(dir.path()).unwrap().document);

    // With the obstacle gone the same save goes through and reuses what was written.
    std::fs::remove_dir(&obstacle).unwrap();
    let report = save_project(&new, dir.path(), &fast()).unwrap();
    assert_eq!((report.blobs_written, report.manifest_written), (0, true));
    assert_eq!(report.files_removed, 3);
    assert_documents_equal(&new, &load_project(dir.path()).unwrap().document);
}
