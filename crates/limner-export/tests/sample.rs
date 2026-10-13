//! Local-only: exports the owner's sample Quill project. Skips when it is absent.
//!
//! The sample is private artwork. Outputs go to the git-ignored `local/out/`, and
//! only aggregate numbers are printed (run with `--nocapture` to see them).

mod support;

use limner_export::{
    export_obj_with_report, write_glb, write_obj, ExportOptions, ExportReport, GltfOptions,
};
use std::path::PathBuf;
use support::Glb;

fn sample_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    let dir = PathBuf::from(home)
        .join("projects")
        .join("quill-exporter")
        .join("sample");
    dir.join("Quill.json").is_file().then_some(dir)
}

fn out_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../local/out/export")
        .components()
        .collect::<PathBuf>();
    std::fs::create_dir_all(&dir).expect("local/out is writable");
    dir
}

fn describe(what: &str, report: &ExportReport) {
    println!(
        "{what}: {} nodes, {} meshes, {} vertices, {} triangles, {} frames, {} channels, {} bytes ({:.2} MiB)",
        report.nodes,
        report.meshes,
        report.vertices,
        report.triangles,
        report.animation_frames,
        report.animation_channels,
        report.bytes,
        report.bytes as f64 / (1024.0 * 1024.0),
    );
}

#[test]
fn the_sample_project_exports_to_every_format() {
    let Some(dir) = sample_dir() else {
        eprintln!("skipped: the sample Quill project is not on this machine");
        return;
    };
    let imported = limner_quill::import_project(&dir).expect("the sample imports");
    let doc = imported.document;
    doc.validate().expect("the imported document is consistent");
    let out = out_dir();
    println!(
        "document: {} layers, {} drawings, {} strokes, {} points, range {:.3}..{:.3} s at {}/{} fps",
        doc.layer_count(),
        doc.drawing_count(),
        doc.stroke_count(),
        doc.point_count(),
        limner_core::time::to_seconds(doc.settings.range_start),
        limner_core::time::to_seconds(doc.settings.range_end),
        doc.settings.rate.num,
        doc.settings.rate.den,
    );

    // A still at the start of the working range, hierarchy kept.
    let still_options = ExportOptions::still(doc.settings.range_start);
    let path = out.join("sample-still.glb");
    let still = write_glb(&doc, &path, &still_options, &GltfOptions::default()).unwrap();
    describe("still glb", &still);
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(bytes.len() as u64, still.bytes);
    Glb::checked(&bytes);
    assert!(still.triangles > 0, "the sample has paint");
    assert_eq!(still.animation_frames, 0);

    // The same still, baked into world space.
    let baked_options = ExportOptions {
        bake_transforms: true,
        ..still_options
    };
    let path = out.join("sample-still-baked.glb");
    let baked = write_glb(&doc, &path, &baked_options, &GltfOptions::default()).unwrap();
    describe("baked still glb", &baked);
    Glb::checked(&std::fs::read(&path).unwrap());
    assert_eq!(
        baked.triangles, still.triangles,
        "baking changes no topology"
    );
    assert_eq!(baked.vertices, still.vertices);

    // Animation over the working range.
    let animated_options = ExportOptions::working_range(&doc);
    let path = out.join("sample-animated.glb");
    let animated = write_glb(&doc, &path, &animated_options, &GltfOptions::default()).unwrap();
    describe("animated glb", &animated);
    let glb = Glb::checked(&std::fs::read(&path).unwrap());
    assert_eq!(
        animated.animation_frames,
        animated_options.sample_times().len()
    );
    assert_eq!(glb.channel_count(), animated.animation_channels);
    assert!(animated.triangles >= still.triangles);

    // The same with the optional extensions.
    let extra = GltfOptions {
        node_visibility: true,
        animate_opacity: true,
        ..GltfOptions::default()
    };
    let path = out.join("sample-animated-extensions.glb");
    let extended = write_glb(&doc, &path, &animated_options, &extra).unwrap();
    describe("animated glb with extensions", &extended);
    Glb::checked(&std::fs::read(&path).unwrap());
    assert!(extended.animation_channels >= animated.animation_channels);

    // OBJ: the same triangles as the still.
    let path = out.join("sample-still.obj");
    let obj = write_obj(&doc, &path, doc.settings.range_start, false).unwrap();
    describe("obj", &obj);
    let (text, report) = export_obj_with_report(&doc, doc.settings.range_start, false).unwrap();
    assert_eq!(report, obj);
    assert_eq!(obj.triangles, still.triangles);
    assert_eq!(obj.vertices, still.vertices);
    let (mut v, mut vn, mut f) = (0usize, 0usize, 0usize);
    for line in text.lines() {
        match line.split(' ').next() {
            Some("v") => v += 1,
            Some("vn") => vn += 1,
            Some("f") => f += 1,
            _ => {}
        }
    }
    assert_eq!((v, vn, f), (obj.vertices, obj.vertices, obj.triangles));
}
