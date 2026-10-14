//! Local-only sweep over real Quill projects.
//!
//! Public sample projects are downloaded by hand into `local/samples/` (git-ignored;
//! `docs/quill-import.md` lists them). Nothing here needs them: the test passes with a
//! note when the folder is absent. Set `LIMNER_SAMPLES_DIR` to sweep another folder.
//!
//! Run with `cargo test -p limner-quill --test samples -- --nocapture` to see the
//! summary lines; add `LIMNER_SAMPLES_VERBOSE=1` to list every warning.

use limner_core::model::{LayerKind, MediaData, MediaKind};
use limner_quill::import_project;
use std::path::{Path, PathBuf};

fn samples_dir() -> PathBuf {
    std::env::var_os("LIMNER_SAMPLES_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../local/samples"))
}

/// Every folder under `dir` that holds a `Quill.json`, in a stable order.
fn find_projects(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    if entries.iter().any(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("Quill.json"))
    }) {
        out.push(dir.to_path_buf());
    }
    if depth > 0 {
        for path in entries.iter().filter(|p| p.is_dir()) {
            find_projects(path, depth - 1, out);
        }
    }
}

#[test]
fn imports_every_local_sample() {
    let dir = samples_dir();
    let mut projects = Vec::new();
    find_projects(&dir, 4, &mut projects);
    if projects.is_empty() {
        eprintln!("skipped: no sample projects under {}", dir.display());
        return;
    }
    let verbose = std::env::var_os("LIMNER_SAMPLES_VERBOSE").is_some();

    let mut failures = Vec::new();
    for project in &projects {
        let name = project
            .strip_prefix(&dir)
            .unwrap_or(project)
            .display()
            .to_string();
        let outcome = std::panic::catch_unwind(|| import_project(project));
        let imported = match outcome {
            Ok(Ok(imported)) => imported,
            Ok(Err(error)) => {
                failures.push(format!("{name}: import failed: {error}"));
                continue;
            }
            Err(_) => {
                failures.push(format!("{name}: the importer panicked"));
                continue;
            }
        };
        let doc = &imported.document;
        if let Err(error) = doc.validate() {
            failures.push(format!("{name}: invalid document: {error}"));
            continue;
        }

        let count =
            |wanted: fn(&LayerKind) -> bool| doc.layers().filter(|l| wanted(&l.kind)).count();
        let media = |kind: MediaKind| doc.media_items().filter(|m| m.kind == kind).count();
        let triangles: usize = doc
            .media_items()
            .map(|m| match &m.data {
                MediaData::Mesh(mesh) => mesh.indices.len() / 3,
                _ => 0,
            })
            .sum();
        // Every layer that should carry media and does not is a hole in the import.
        let empty_media = doc
            .layers()
            .filter(|l| match &l.kind {
                LayerKind::Picture(p) => p.media.is_none(),
                LayerKind::Model(m) => m.media.is_none(),
                LayerKind::Sound(s) => s.media.is_none(),
                _ => false,
            })
            .count();
        println!(
            "{name}: {} layers ({} paint, {} picture, {} model, {} sound), {} drawings, {} strokes, {} points, media {} image / {} mesh ({} triangles) / {} audio, {} layer(s) without media, {} warning(s)",
            doc.layer_count(),
            count(|k| matches!(k, LayerKind::Paint(_))),
            count(|k| matches!(k, LayerKind::Picture(_))),
            count(|k| matches!(k, LayerKind::Model(_))),
            count(|k| matches!(k, LayerKind::Sound(_))),
            doc.drawing_count(),
            doc.stroke_count(),
            doc.point_count(),
            media(MediaKind::Image),
            media(MediaKind::Mesh),
            triangles,
            media(MediaKind::Audio),
            empty_media,
            imported.warnings.len(),
        );
        if verbose {
            for warning in &imported.warnings {
                println!("    warning: {warning}");
            }
        }
    }
    println!("{} project(s) imported", projects.len() - failures.len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn sample(name: &str) -> Option<limner_quill::Imported> {
    let dir = samples_dir().join(name);
    if !dir.join("Quill.json").is_file() || !dir.join("Quill.qbin").is_file() {
        eprintln!("skipped: sample {name} is not on this machine");
        return None;
    }
    Some(import_project(&dir).unwrap())
}

/// The one public project with a model layer: an OBJ import whose record was decoded
/// by hand before the reader was written (16,577 vertices, 32,482 triangles, 6 quads).
#[test]
fn the_public_model_sample_reads_as_measured() {
    let Some(imported) = sample("bab_isaac_cave_04012024") else {
        return;
    };
    assert_eq!(imported.warnings, Vec::<String>::new());
    let doc = &imported.document;
    let models: Vec<_> = doc
        .layers()
        .filter_map(|l| match &l.kind {
            LayerKind::Model(model) => Some(model),
            _ => None,
        })
        .collect();
    assert_eq!(models.len(), 1);
    assert!(models[0].wireframe);
    assert!(models[0].source_path.ends_with(".obj"));
    let media = doc.media(models[0].media.unwrap()).unwrap();
    let MediaData::Mesh(mesh) = &media.data else {
        panic!("not a mesh")
    };
    assert_eq!(mesh.positions.len(), 16_577);
    assert_eq!(mesh.indices.len(), (32_482 + 2 * 6) * 3);
    assert_eq!(mesh.normals.len(), mesh.positions.len());
    assert!(mesh.normals.iter().all(|n| {
        let length: f32 = n.iter().map(|c| c * c).sum::<f32>().sqrt();
        (length - 1.0).abs() < 1e-3
    }));
    // An OBJ has no vertex colours; Quill stores white.
    assert_eq!(mesh.colors.len(), mesh.positions.len());
    assert!(mesh.colors.iter().all(|c| *c == [1.0; 3]));
    assert!(mesh
        .indices
        .iter()
        .all(|i| (*i as usize) < mesh.positions.len()));
}

/// Picture layers of a public project: five photographs stored as decoded RGB.
#[test]
fn the_public_picture_sample_reads_as_measured() {
    let Some(imported) = sample("bab_isaac_gates_00") else {
        return;
    };
    assert_eq!(
        imported.warnings,
        ["2 gallery poster(s) saved for Quill's sharing feature were not imported"]
    );
    let doc = &imported.document;
    let sizes: Vec<(u32, u32, u8)> = doc
        .walk()
        .iter()
        .filter_map(|id| match &doc.layer(*id).unwrap().kind {
            LayerKind::Picture(picture) => {
                assert!(!picture.viewer_locked);
                match &doc.media(picture.media.unwrap()).unwrap().data {
                    MediaData::RawImage {
                        width,
                        height,
                        channels,
                        pixels,
                    } => {
                        assert_eq!(
                            pixels.len(),
                            (*width * *height) as usize * *channels as usize
                        );
                        Some((*width, *height, *channels))
                    }
                    _ => None,
                }
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        sizes,
        [
            (640, 640, 3),
            (480, 640, 3),
            (558, 745, 3),
            (1024, 768, 3),
            (1024, 683, 3)
        ]
    );
}

/// Two public projects in the January 2017 schema.
#[test]
fn the_public_2017_samples_read_as_measured() {
    if let Some(imported) = sample("floatingink_happy_new_year") {
        assert_eq!(imported.warnings, Vec::<String>::new());
        let doc = &imported.document;
        assert_eq!(
            (doc.layer_count(), doc.stroke_count(), doc.point_count()),
            (2, 32, 2_307)
        );
        // Stroke ids count across the file; the header holds the next one.
        let highest = doc
            .drawings()
            .flat_map(|(_, d)| d.strokes().map(|s| s.id).collect::<Vec<_>>())
            .max();
        assert_eq!(highest, Some(41));
        assert_eq!(imported.state.tool_id, Some(0));
        assert_eq!(imported.state.tool_name.as_deref(), Some("Paint"));
        assert_eq!(doc.settings.default_viewpoint, None);
    }
    if let Some(imported) = sample("mountainwaterlove_1scene01") {
        assert_eq!(imported.warnings, Vec::<String>::new());
        let doc = &imported.document;
        assert_eq!(
            (doc.layer_count(), doc.stroke_count(), doc.point_count()),
            (15, 778, 48_819)
        );
        assert_eq!(
            doc.layers().filter(|l| !l.transform.is_identity()).count(),
            7,
            "seven layers carry a matrix that is not the identity"
        );
        assert_eq!(imported.state.tool_id, Some(4));
        assert_eq!(imported.state.tool_name.as_deref(), Some("Select"));
    }
}
