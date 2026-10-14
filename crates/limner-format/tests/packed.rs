mod common;

use common::*;
use limner_format::testing::{assert_documents_equal, exhaustive_document};
use limner_format::{
    is_limn_project, load_project, pack, read_state, read_thumbnail, save_packed, save_project,
    unpack, write_state, write_thumbnail, FormatError,
};
use std::io::{Read, Write};

#[test]
fn pack_and_unpack_round_trip() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("piece.limn");
    save_project(&doc, &project, &fast()).unwrap();
    let state =
        serde_json::json!({"tool": "paint", "view": {"scale": 2.5}, "palette": [[1, 0, 0]]});
    write_state(&project, &state).unwrap();
    write_thumbnail(&project, b"\x89PNG\r\n\x1a\n pretend thumbnail").unwrap();
    // Leftovers and strangers are not part of the project and are not packed.
    std::fs::write(project.join("limn.json.tmp"), b"half").unwrap();
    std::fs::write(project.join("notes.txt"), b"private").unwrap();
    std::fs::write(project.join("media").join("stray.png"), b"stray").unwrap();

    let packed = dir.path().join("out").join("piece.limnz");
    pack(&project, &packed).unwrap();
    assert!(is_limn_project(&packed));
    assert!(!packed.with_file_name("piece.limnz.tmp").exists());

    let unpacked = dir.path().join("unpacked.limn");
    unpack(&packed, &unpacked).unwrap();
    let mut expected = snapshot(&project);
    for stranger in ["limn.json.tmp", "notes.txt", "media/stray.png"] {
        assert!(expected.remove(stranger).is_some());
    }
    assert_eq!(snapshot(&unpacked), expected);
    assert_documents_equal(&doc, &load_project(&unpacked).unwrap().document);
    assert_eq!(read_state(&unpacked).unwrap(), Some(state));

    // Packing is deterministic: the same project gives the same bytes.
    let again = dir.path().join("again.limnz");
    pack(&unpacked, &again).unwrap();
    assert_eq!(
        std::fs::read(&packed).unwrap(),
        std::fs::read(&again).unwrap()
    );

    // Unpacking never writes into a folder that already has something in it.
    assert!(matches!(
        unpack(&packed, &unpacked).unwrap_err(),
        FormatError::AlreadyExists(_)
    ));
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    unpack(&packed, &empty).unwrap();
    assert_eq!(snapshot(&empty), expected);
}

#[test]
fn a_packed_project_loads_without_being_unpacked() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("piece.limn");
    save_project(&doc, &project, &fast()).unwrap();
    let state = serde_json::json!({"last_tool": "eraser"});
    write_state(&project, &state).unwrap();
    write_thumbnail(&project, b"thumb").unwrap();

    let alone = tempfile::tempdir().unwrap();
    // The extension is a convention; the content is what is recognised.
    for name in ["piece.limnz", "renamed.zip"] {
        let packed = alone.path().join(name);
        pack(&project, &packed).unwrap();
        let loaded = load_project(&packed).unwrap();
        assert_eq!(loaded.warnings, Vec::<String>::new());
        assert_documents_equal(&doc, &loaded.document);
        assert_eq!(read_state(&packed).unwrap(), Some(state.clone()));
        assert_eq!(
            read_thumbnail(&packed).unwrap().as_deref(),
            Some(&b"thumb"[..])
        );
    }
    // Nothing appeared beside the packed files.
    assert_eq!(file_names(alone.path()), ["piece.limnz", "renamed.zip"]);

    // Entries are stored, not compressed: the manifest is readable in place.
    let bytes = std::fs::read(alone.path().join("piece.limnz")).unwrap();
    let needle = b"\"format\": \"limn\"";
    assert!(bytes.windows(needle.len()).any(|w| w == needle));
    assert!(bytes.windows(8).any(|w| w == b"LIMNSTK1"));
}

#[test]
fn saving_straight_to_a_packed_file() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    let packed = dir.path().join("direct.limnz");
    let report = save_packed(&doc, &packed, &fast()).unwrap();
    assert_eq!((report.blobs_written, report.media_written), (5, 11));
    assert!(report.manifest_written);
    assert_eq!(file_names(dir.path()), ["direct.limnz"]);
    assert_documents_equal(&doc, &load_project(&packed).unwrap().document);
    assert_eq!(read_state(&packed).unwrap(), None);

    // It holds the same files a folder save produces.
    let folder = dir.path().join("folder.limn");
    save_project(&doc, &folder, &fast()).unwrap();
    let unpacked = dir.path().join("unpacked.limn");
    unpack(&packed, &unpacked).unwrap();
    assert_eq!(snapshot(&unpacked), snapshot(&folder));

    // Saving over a packed project keeps its editor state and thumbnail.
    write_state(&unpacked, &serde_json::json!({"zoom": 3})).unwrap();
    write_thumbnail(&unpacked, b"thumb").unwrap();
    pack(&unpacked, &packed).unwrap();
    let mut edited = doc.clone();
    nudge(&mut edited, drawing_with_strokes(&doc, 30));
    save_packed(&edited, &packed, &fast()).unwrap();
    assert_documents_equal(&edited, &load_project(&packed).unwrap().document);
    assert_eq!(
        read_state(&packed).unwrap(),
        Some(serde_json::json!({"zoom": 3}))
    );
    assert_eq!(
        read_thumbnail(&packed).unwrap().as_deref(),
        Some(&b"thumb"[..])
    );
}

#[test]
fn damage_inside_a_packed_project_is_reported() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    let packed = dir.path().join("piece.limnz");
    save_packed(&doc, &packed, &fast()).unwrap();
    let original = std::fs::read(&packed).unwrap();

    // Alter a byte inside the large drawing's blob: the archive's own checksum or
    // the content hash catches it, and the error names the entry.
    let hex = limner_format::drawing_hash(doc.drawing(drawing_with_strokes(&doc, 30)).unwrap())
        .unwrap()
        .to_hex();
    let name = format!("drawings/{}/{hex}.lstk", &hex[..2]);
    let at = original
        .windows(name.len())
        .position(|w| w == name.as_bytes())
        .unwrap()
        + name.len()
        + 400;
    let mut bytes = original.clone();
    bytes[at] ^= 0x20;
    std::fs::write(&packed, &bytes).unwrap();
    let error = load_project(&packed).unwrap_err();
    assert!(matches!(error, FormatError::Corrupt { .. }), "{error}");
    assert!(
        error.to_string().contains(&format!("{hex}.lstk")),
        "{error}"
    );

    // A truncated archive is not a project at all.
    std::fs::write(&packed, &original[..original.len() / 2]).unwrap();
    assert!(load_project(&packed).is_err());
    assert!(!is_limn_project(&packed));
}

#[test]
fn archives_that_are_not_projects_or_not_safe_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    let other = dir.path().join("other.zip");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&other).unwrap());
    writer.start_file("readme.txt", options).unwrap();
    writer.write_all(b"hello").unwrap();
    writer.finish().unwrap();
    assert!(!is_limn_project(&other));
    assert!(matches!(
        load_project(&other).unwrap_err(),
        FormatError::NotAProject(_)
    ));
    assert!(matches!(
        unpack(&other, &dir.path().join("x")).unwrap_err(),
        FormatError::NotAProject(_)
    ));

    // An entry that would land outside the target folder stops the unpack.
    let project = dir.path().join("p.limn");
    save_project(&limner_core::model::Document::new(), &project, &fast()).unwrap();
    let hostile = dir.path().join("hostile.limnz");
    let mut writer = zip::ZipWriter::new(std::fs::File::create(&hostile).unwrap());
    writer.start_file("limn.json", options).unwrap();
    writer
        .write_all(&std::fs::read(project.join("limn.json")).unwrap())
        .unwrap();
    writer.start_file("../escaped.txt", options).unwrap();
    writer.write_all(b"gotcha").unwrap();
    writer.finish().unwrap();
    let target = dir.path().join("target").join("inner");
    let error = unpack(&hostile, &target).unwrap_err();
    assert!(matches!(error, FormatError::Corrupt { .. }), "{error}");
    assert!(!dir.path().join("target").join("escaped.txt").exists());
    assert!(!target.join("limn.json").exists());

    // Packing something that is not a project, or one with a file missing.
    assert!(matches!(
        pack(dir.path(), &dir.path().join("no.limnz")).unwrap_err(),
        FormatError::NotAProject(_)
    ));
    let blob = blob_files(&project).remove(0);
    std::fs::remove_file(project.join(&blob)).unwrap();
    let error = pack(&project, &dir.path().join("no.limnz")).unwrap_err();
    assert!(matches!(error, FormatError::MissingFile { .. }), "{error}");
    assert!(!dir.path().join("no.limnz").exists() && !dir.path().join("no.limnz.tmp").exists());
}

#[test]
fn editor_state_is_kept_apart_from_the_document() {
    let doc = exhaustive_document();
    let dir = tempfile::tempdir().unwrap();
    save_project(&doc, dir.path(), &fast()).unwrap();
    assert_eq!(read_state(dir.path()).unwrap(), None);
    assert_eq!(read_thumbnail(dir.path()).unwrap(), None);

    let state = serde_json::json!({"b": [1.5, "two", null], "a": {"nested": true}});
    write_state(dir.path(), &state).unwrap();
    assert_eq!(read_state(dir.path()).unwrap(), Some(state.clone()));
    let mut text = String::new();
    std::fs::File::open(dir.path().join("state.json"))
        .unwrap()
        .read_to_string(&mut text)
        .unwrap();
    assert!(text.ends_with("}\n") && text.contains("\n  \"a\""));

    // Saving the document, with garbage collection, leaves the state alone.
    let manifest = std::fs::read(dir.path().join("limn.json")).unwrap();
    let mut edited = doc.clone();
    edited.settings.title = "renamed".to_owned();
    save_project(&edited, dir.path(), &fast()).unwrap();
    assert_eq!(read_state(dir.path()).unwrap(), Some(state));
    // And writing state leaves the document alone.
    write_state(dir.path(), &serde_json::json!(null)).unwrap();
    assert_ne!(
        std::fs::read(dir.path().join("limn.json")).unwrap(),
        manifest
    );
    assert_documents_equal(&edited, &load_project(dir.path()).unwrap().document);

    std::fs::write(dir.path().join("state.json"), b"{ broken").unwrap();
    assert!(matches!(
        read_state(dir.path()).unwrap_err(),
        FormatError::Json { .. }
    ));
    // A broken state file does not stop the document from loading.
    assert_documents_equal(&edited, &load_project(dir.path()).unwrap().document);
}
