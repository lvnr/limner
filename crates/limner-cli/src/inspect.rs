//! The `inspect` report: a document's tree and statistics.

use limner::core::model::{Document, Layer, LayerId, LayerKind};
use limner::core::time;
use limner::Opened;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fmt::Write;

pub struct Report<'a> {
    opened: &'a Opened,
}

impl<'a> Report<'a> {
    pub fn new(opened: &'a Opened) -> Self {
        Self { opened }
    }

    fn doc(&self) -> &Document {
        &self.opened.document
    }

    fn type_counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for layer in self.doc().layers() {
            *counts.entry(layer.type_name()).or_insert(0) += 1;
        }
        counts
    }

    fn layer_stats(&self, layer: &Layer) -> (usize, usize) {
        match &layer.kind {
            LayerKind::Paint(paint) => {
                let mut seen = std::collections::BTreeSet::new();
                paint
                    .drawings
                    .iter()
                    .filter(|d| seen.insert(**d))
                    .filter_map(|d| self.doc().drawing(*d))
                    .fold((0, 0), |(s, p), d| {
                        (s + d.stroke_count(), p + d.point_count())
                    })
            }
            _ => (0, 0),
        }
    }

    fn layer_notes(&self, layer: &Layer) -> Vec<String> {
        let mut notes = Vec::new();
        if !layer.visible {
            notes.push("hidden".to_owned());
        }
        if layer.locked {
            notes.push("locked".to_owned());
        }
        if layer.opacity < 1.0 {
            notes.push(format!("opacity {:.0}%", layer.opacity * 100.0));
        }
        match &layer.kind {
            LayerKind::Group(group) => {
                if let Some(sequence) = group.sequence.filter(|s| s.loop_duration > 0) {
                    notes.push(format!(
                        "loops every {:.2} s",
                        time::to_seconds(sequence.loop_duration)
                    ));
                }
            }
            LayerKind::Paint(paint) => {
                let (strokes, points) = self.layer_stats(layer);
                notes.push(format!("{strokes} strokes, {points} points"));
                if paint.is_animated() {
                    notes.push(format!(
                        "{} frames of {} drawings at {} fps",
                        paint.frames.len(),
                        paint.drawings.len(),
                        paint.rate.as_f64()
                    ));
                }
            }
            LayerKind::Camera(camera) => notes.push(format!("fov {:.0}", camera.fov)),
            LayerKind::Viewpoint(viewpoint) => notes.push(
                if viewpoint.floor_level {
                    "floor level"
                } else {
                    "eye level"
                }
                .to_owned(),
            ),
            _ => {}
        }
        let clips = layer
            .anim
            .visibility
            .keys()
            .iter()
            .filter(|k| k.value)
            .count();
        if clips > 1 {
            notes.push(format!("{clips} clips"));
        }
        for (label, count) in [
            ("transform keys", layer.anim.transform.len()),
            ("opacity keys", layer.anim.opacity.len()),
            ("action keys", layer.anim.actions.len()),
        ] {
            if count > 0 {
                notes.push(format!("{count} {label}"));
            }
        }
        notes
    }

    pub fn to_text(&self) -> String {
        let doc = self.doc();
        let mut out = String::new();
        let title = if doc.settings.title.is_empty() {
            "Untitled"
        } else {
            &doc.settings.title
        };
        let _ = writeln!(out, "{title} ({})", self.opened.kind.label());
        let counts: Vec<String> = self
            .type_counts()
            .iter()
            .map(|(name, count)| format!("{count} {}", name.to_lowercase()))
            .collect();
        let _ = writeln!(
            out,
            "  layers:   {} ({})",
            doc.layer_count(),
            counts.join(", ")
        );
        let _ = writeln!(
            out,
            "  drawings: {}, strokes: {}, points: {}",
            doc.drawing_count(),
            doc.stroke_count(),
            doc.point_count()
        );
        let _ = writeln!(
            out,
            "  timing:   {} fps, range {:.2} to {:.2} s",
            doc.settings.rate.as_f64(),
            time::to_seconds(doc.settings.range_start),
            time::to_seconds(doc.settings.range_end)
        );
        let _ = writeln!(out);
        for id in doc.walk() {
            let layer = doc.layer(id).expect("walk yields existing layers");
            let notes = self.layer_notes(layer);
            let suffix = if notes.is_empty() {
                String::new()
            } else {
                format!("  ({})", notes.join("; "))
            };
            let _ = writeln!(
                out,
                "{}{} [{}]{}",
                "  ".repeat(doc.depth(id)),
                layer.name,
                layer.type_name().to_lowercase(),
                suffix
            );
        }
        if !self.opened.warnings.is_empty() {
            let _ = writeln!(out, "\nwarnings:");
            for warning in &self.opened.warnings {
                let _ = writeln!(out, "  - {warning}");
            }
        }
        out
    }

    fn layer_json(&self, id: LayerId) -> Value {
        let doc = self.doc();
        let layer = doc.layer(id).expect("walk yields existing layers");
        let (strokes, points) = self.layer_stats(layer);
        json!({
            "id": id.0,
            "name": layer.name,
            "path": doc.path_of(id),
            "type": layer.type_name(),
            "visible": layer.visible,
            "locked": layer.locked,
            "opacity": layer.opacity,
            "strokes": strokes,
            "points": points,
            "transformKeys": layer.anim.transform.len(),
            "opacityKeys": layer.anim.opacity.len(),
            "children": layer.children().iter().map(|c| self.layer_json(*c)).collect::<Vec<_>>(),
        })
    }

    pub fn to_json(&self) -> Value {
        let doc = self.doc();
        json!({
            "title": doc.settings.title,
            "source": self.opened.kind.label(),
            "layers": doc.layer_count(),
            "layerTypes": self.type_counts(),
            "drawings": doc.drawing_count(),
            "strokes": doc.stroke_count(),
            "points": doc.point_count(),
            "fps": doc.settings.rate.as_f64(),
            "rangeSeconds": [time::to_seconds(doc.settings.range_start), time::to_seconds(doc.settings.range_end)],
            "root": self.layer_json(doc.root()),
            "warnings": self.opened.warnings,
        })
    }
}
