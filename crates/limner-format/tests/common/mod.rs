//! Shared by the integration tests.
#![allow(dead_code)]

use limner_core::model::{Document, DrawingId, StrokeAddr};
use limner_format::SaveOptions;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Save options for tests: the protocol is the same, only the flushes to the
/// physical disk are skipped so the suite stays fast.
pub fn fast() -> SaveOptions {
    SaveOptions {
        sync: false,
        ..SaveOptions::default()
    }
}

/// Every file under `dir` as `relative/path -> content`.
pub fn snapshot(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                out.insert(relative, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out);
    out
}

pub fn file_names(dir: &Path) -> Vec<String> {
    snapshot(dir).into_keys().collect()
}

pub fn blob_files(dir: &Path) -> Vec<String> {
    file_names(dir)
        .into_iter()
        .filter(|name| name.ends_with(".lstk"))
        .collect()
}

/// The path of the stroke blob that holds `drawing`.
pub fn blob_file(dir: &Path, doc: &Document, drawing: DrawingId) -> PathBuf {
    let hex = limner_format::drawing_hash(doc.drawing(drawing).unwrap())
        .unwrap()
        .to_hex();
    dir.join("drawings")
        .join(&hex[..2])
        .join(format!("{hex}.lstk"))
}

/// The drawing with the given number of strokes (the test documents make this unique enough).
pub fn drawing_with_strokes(doc: &Document, strokes: usize) -> DrawingId {
    doc.drawings()
        .find(|(_, d)| d.stroke_count() == strokes)
        .map(|(id, _)| id)
        .unwrap()
}

/// Changes one point of the first stroke of `drawing`.
pub fn nudge(doc: &mut Document, drawing: DrawingId) {
    let drawing = doc.drawing_mut(drawing).unwrap();
    let stroke = drawing
        .stroke_mut(StrokeAddr { chunk: 0, index: 0 })
        .unwrap();
    stroke.points_mut()[0].pos[0] += 0.125;
}
