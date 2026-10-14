//! Runs against the project owner's private sample when it is present on this
//! machine, and prints nothing about it but counts and sizes.

mod common;

use common::*;
use limner_format::testing::assert_documents_equal;
use limner_format::{load_project, pack, save_project, SaveOptions};
use std::path::PathBuf;

fn sample_dir() -> Option<PathBuf> {
    std::env::var_os("LIMNER_SAMPLE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|home| PathBuf::from(home).join("projects/quill-exporter/sample"))
        })
        .filter(|dir| dir.join("Quill.json").is_file())
}

#[test]
fn the_local_sample_round_trips_exactly() {
    let Some(sample) = sample_dir() else {
        eprintln!("skipped: no local sample project");
        return;
    };
    let imported = limner_quill::import_project(&sample).unwrap().document;
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("sample.limn");

    let report = save_project(&imported, &project, &SaveOptions::default()).unwrap();
    let loaded = load_project(&project).unwrap();
    assert_eq!(loaded.warnings, Vec::<String>::new());
    assert_documents_equal(&imported, &loaded.document);
    // The importer builds drawings the way the loader does, so even the chunking agrees.
    for (id, drawing) in imported.drawings() {
        assert_eq!(drawing, loaded.document.drawing(id).unwrap());
    }

    let files = snapshot(&project);
    let total: u64 = files.values().map(|bytes| bytes.len() as u64).sum();
    assert_eq!(report.bytes_written, total);
    assert_eq!(
        report.blobs_written + report.blobs_reused,
        files.keys().filter(|name| name.ends_with(".lstk")).count()
    );

    // A second save has nothing to do, and a packed copy loads to the same document.
    assert!(
        save_project(&loaded.document, &project, &SaveOptions::default())
            .unwrap()
            .wrote_nothing()
    );
    let packed = dir.path().join("sample.limnz");
    pack(&project, &packed).unwrap();
    assert_documents_equal(&imported, &load_project(&packed).unwrap().document);

    let source: u64 = ["Quill.json", "Quill.qbin"]
        .iter()
        .filter_map(|name| std::fs::metadata(sample.join(name)).ok())
        .map(|m| m.len())
        .sum();
    eprintln!(
        "local sample: {} layers, {} drawings, {} strokes, {} points; {} blobs and {} media files \
         written, {} bytes in {} files ({} bytes of manifest; packed {} bytes; Quill source {} bytes)",
        imported.layer_count(),
        imported.drawing_count(),
        imported.stroke_count(),
        imported.point_count(),
        report.blobs_written,
        report.media_written,
        total,
        files.len(),
        files["limn.json"].len(),
        std::fs::metadata(&packed).unwrap().len(),
        source,
    );
}
