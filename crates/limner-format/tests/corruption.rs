mod common;

use common::*;
use limner_format::testing::exhaustive_document;
use limner_format::{load_project, save_project, FormatError};
use std::path::{Path, PathBuf};

fn saved() -> (tempfile::TempDir, limner_core::model::Document) {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    (dir, doc)
}

/// Loads, expecting a damaged-file error, and returns the file it names and its message.
fn damaged(dir: &Path) -> (PathBuf, String) {
    let error = load_project(dir).unwrap_err();
    let message = error.to_string();
    match error {
        FormatError::Corrupt { path, .. } => (path, message),
        other => panic!("expected a damaged-file error, got: {other}"),
    }
}

#[test]
fn a_truncated_blob_is_named_in_the_error() {
    let (dir, doc) = saved();
    let blob = blob_file(dir.path(), &doc, drawing_with_strokes(&doc, 30));
    let bytes = std::fs::read(&blob).unwrap();
    for keep in [bytes.len() - 1, bytes.len() / 2, 30, 24, 10, 0] {
        std::fs::write(&blob, &bytes[..keep]).unwrap();
        let (path, message) = damaged(dir.path());
        assert_eq!(path, blob, "cut to {keep} bytes");
        assert!(
            message.contains(blob.file_name().unwrap().to_str().unwrap()),
            "{message}"
        );
        assert!(message.contains("damaged"), "{message}");
    }
}

#[test]
fn an_altered_blob_is_named_in_the_error() {
    let (dir, doc) = saved();
    // A compressed blob (the large drawing) and one stored as is (the empty drawing
    // is too small to compress): alter one byte anywhere and the load fails.
    for strokes in [30, 6, 0] {
        let blob = blob_file(dir.path(), &doc, drawing_with_strokes(&doc, strokes));
        let original = std::fs::read(&blob).unwrap();
        let step = (original.len() / 23).max(1);
        for at in (0..original.len()).step_by(step) {
            let mut bytes = original.clone();
            bytes[at] ^= 0x04;
            std::fs::write(&blob, &bytes).unwrap();
            let (path, message) = damaged(dir.path());
            assert_eq!(path, blob, "byte {at} of a {strokes}-stroke drawing");
            assert!(message.contains(".lstk"), "{message}");
        }
        // Appended bytes are damage too.
        let mut longer = original.clone();
        longer.extend_from_slice(b"extra");
        std::fs::write(&blob, &longer).unwrap();
        assert_eq!(damaged(dir.path()).0, blob);
        std::fs::write(&blob, &original).unwrap();
        load_project(dir.path()).unwrap();
    }
}

#[test]
fn a_blob_under_the_wrong_name_is_rejected() {
    let (dir, doc) = saved();
    let big = blob_file(dir.path(), &doc, drawing_with_strokes(&doc, 30));
    let small = blob_file(dir.path(), &doc, drawing_with_strokes(&doc, 6));
    std::fs::copy(&small, &big).unwrap();
    let (path, message) = damaged(dir.path());
    assert_eq!(path, big);
    assert!(message.contains("hashes to"), "{message}");
}

#[test]
fn a_missing_blob_or_media_file_is_named_in_the_error() {
    let (dir, doc) = saved();
    let blob = blob_file(dir.path(), &doc, drawing_with_strokes(&doc, 30));
    let bytes = std::fs::read(&blob).unwrap();
    std::fs::remove_file(&blob).unwrap();
    match load_project(dir.path()).unwrap_err() {
        FormatError::MissingFile { path } => assert_eq!(path, blob),
        other => panic!("{other}"),
    }
    std::fs::write(&blob, bytes).unwrap();

    let media = file_names(dir.path())
        .into_iter()
        .find(|name| name.ends_with(".wav"))
        .unwrap();
    std::fs::remove_file(dir.path().join(&media)).unwrap();
    let error = load_project(dir.path()).unwrap_err();
    assert!(matches!(error, FormatError::MissingFile { .. }), "{error}");
    assert!(error.to_string().contains(".wav"), "{error}");
}

#[test]
fn altered_media_is_named_in_the_error() {
    for extension in [".png", ".wav", ".lmsh", ".limg", ".laud", ".jpg"] {
        let (dir, _) = saved();
        let media = file_names(dir.path())
            .into_iter()
            .find(|name| name.starts_with("media/") && name.ends_with(extension))
            .unwrap_or_else(|| panic!("no {extension} file"));
        let path = dir.path().join(&media);
        let mut bytes = std::fs::read(&path).unwrap();
        // The middle of the file is content in every one of these formats.
        let middle = bytes.len() / 2;
        bytes[middle] ^= 0xFF;
        std::fs::write(&path, &bytes).unwrap();
        let (named, message) = damaged(dir.path());
        assert_eq!(named, path);
        assert!(message.contains(extension), "{message}");

        bytes.truncate(bytes.len() / 2);
        std::fs::write(&path, &bytes).unwrap();
        assert_eq!(damaged(dir.path()).0, path);
    }
}

fn with_manifest(edit: impl FnOnce(&mut serde_json::Value)) -> FormatError {
    let (dir, _) = saved();
    let path = dir.path().join("limn.json");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    edit(&mut value);
    std::fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
    load_project(dir.path()).unwrap_err()
}

#[test]
fn a_newer_format_version_is_rejected_clearly() {
    let error = with_manifest(|m| m["version"] = serde_json::json!(999));
    match &error {
        FormatError::UnsupportedVersion {
            found,
            supported,
            path,
        } => {
            assert_eq!((*found, *supported), (999, limner_format::FORMAT_VERSION));
            assert!(path.ends_with("limn.json"));
        }
        other => panic!("{other}"),
    }
    let message = error.to_string();
    assert!(
        message.contains("999") && message.contains("newer"),
        "{message}"
    );

    // The version is checked before anything else: a future file whose layout this
    // build cannot make sense of still gets the version message.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("limn.json"),
        br#"{"format": "limn", "version": 2, "scene": "something entirely different"}"#,
    )
    .unwrap();
    assert!(matches!(
        load_project(dir.path()).unwrap_err(),
        FormatError::UnsupportedVersion { found: 2, .. }
    ));
}

#[test]
fn manifests_that_are_not_limn_manifests_are_rejected() {
    let not_limn = with_manifest(|m| m["format"] = serde_json::json!("quill"));
    assert!(
        matches!(not_limn, FormatError::Manifest { .. }),
        "{not_limn}"
    );
    let no_version = with_manifest(|m| {
        m.as_object_mut().unwrap().remove("version");
    });
    assert!(
        matches!(no_version, FormatError::Manifest { .. }),
        "{no_version}"
    );
    let no_root = with_manifest(|m| {
        m.as_object_mut().unwrap().remove("root");
    });
    assert!(matches!(no_root, FormatError::Json { .. }), "{no_root}");
    assert!(no_root.to_string().contains("root"), "{no_root}");
}

#[test]
fn garbage_json_is_rejected() {
    for garbage in [
        &b""[..],
        b"\x00\x01\x02\x03",
        b"not json at all",
        b"[1, 2, 3]",
        b"{\"format\": \"limn\", \"version\": 1, \"root\": ",
        b"{\"format\": \"limn\", \"version\": 1, \"root\": {\"id\": 1, \"type\": \"hologram\"}}",
        b"{\"format\": \"limn\", \"version\": 1, \"root\": {\"id\": \"one\", \"type\": \"group\"}}",
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("limn.json"), garbage).unwrap();
        let error = load_project(dir.path()).unwrap_err();
        assert!(
            matches!(error, FormatError::Json { .. }),
            "{:?} gave: {error}",
            String::from_utf8_lossy(garbage)
        );
        assert!(error.to_string().contains("limn.json"), "{error}");
    }
    // A truncated copy of a real manifest.
    let (dir, _) = saved();
    let path = dir.path().join("limn.json");
    let bytes = std::fs::read(&path).unwrap();
    std::fs::write(&path, &bytes[..bytes.len() * 2 / 3]).unwrap();
    assert!(matches!(
        load_project(dir.path()).unwrap_err(),
        FormatError::Json { .. }
    ));
}

#[test]
fn manifests_that_describe_no_possible_document_are_rejected() {
    // A paint layer pointing at a drawing the table does not have.
    let error = with_manifest(|m| {
        m["drawings"].as_array_mut().unwrap().remove(0);
    });
    assert!(matches!(error, FormatError::Invalid { .. }), "{error}");

    // Two layers with one id.
    let error = with_manifest(|m| {
        let id = m["root"]["children"][0]["id"].clone();
        m["root"]["children"][1]["id"] = id;
    });
    assert!(matches!(error, FormatError::Manifest { .. }), "{error}");
    assert!(error.to_string().contains("more than one layer"), "{error}");

    // A type without its member, a non-group with children, a root that is not layer 1.
    let error = with_manifest(|m| {
        m["root"]["children"][0]
            .as_object_mut()
            .unwrap()
            .remove("paint");
    });
    assert!(error.to_string().contains("no \"paint\" member"), "{error}");
    let error = with_manifest(|m| {
        m["root"]["children"][0]["children"] = serde_json::json!([{"id": 99999, "type": "group"}]);
    });
    assert!(error.to_string().contains("not a group"), "{error}");
    let error = with_manifest(|m| m["root"]["id"] = serde_json::json!(424242));
    assert!(matches!(error, FormatError::Manifest { .. }), "{error}");

    // Names that try to leave the project folder.
    let error =
        with_manifest(|m| m["drawings"][0]["blob"] = serde_json::json!("../../../etc/passwd"));
    assert!(matches!(error, FormatError::Manifest { .. }), "{error}");
    let error = with_manifest(|m| m["media"][0]["file"] = serde_json::json!("../limn.json"));
    assert!(matches!(error, FormatError::Manifest { .. }), "{error}");
}

#[test]
fn things_that_are_not_projects_are_not_projects() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        load_project(dir.path()).unwrap_err(),
        FormatError::NotAProject(_)
    ));
    assert!(matches!(
        load_project(&dir.path().join("nowhere")).unwrap_err(),
        FormatError::Io { .. }
    ));
    let file = dir.path().join("picture.png");
    std::fs::write(&file, b"\x89PNG\r\n\x1a\n").unwrap();
    assert!(matches!(
        load_project(&file).unwrap_err(),
        FormatError::NotAProject(_)
    ));
    assert!(!limner_format::is_limn_project(&file));
}
