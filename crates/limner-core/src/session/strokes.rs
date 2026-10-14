//! Commands that act on strokes: the selection commands and Optimize.

use super::command::{OptimizeScope, StrokeSpace, StrokeSpec};
use super::tools::stroke_builder::{build_points, Sample, StrokeStyle, MAX_STROKE_POINTS};
use super::tools::Taper;
use super::{CommandError, Session};

/// The most strokes one `add_strokes` command may carry.
const MAX_ADDED_STROKES: usize = 100_000;
use crate::model::{Drawing, DrawingId, LayerId, LayerKind, Paint};
use crate::ops::layers::unique_name;
use crate::ops::simplify::simplify_stroke;
use crate::ops::strokes::transformed_stroke;
use crate::ops::OpError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

impl Session {
    /// Adds strokes described from outside to the frame a paint layer shows.
    pub(crate) fn add_strokes(
        &mut self,
        layer: Option<LayerId>,
        specs: &[StrokeSpec],
        space: StrokeSpace,
        select: bool,
    ) -> Result<Value, CommandError> {
        if specs.len() > MAX_ADDED_STROKES {
            return Err(CommandError::invalid(format!(
                "{} strokes at once is more than the {MAX_ADDED_STROKES} that can be added in one go",
                specs.len()
            )));
        }
        let frame = self.evaluate();
        // With no layer named, the strokes go where the brush would put them.
        let layer = match layer {
            Some(layer) => layer,
            None => self.paint_target().map_err(CommandError::Invalid)?.layer,
        };
        let frame = if frame.get(layer).is_some() {
            frame
        } else {
            self.evaluate()
        };
        let target = self
            .editable_drawing(&frame, layer)
            .map_err(CommandError::Invalid)?;
        // From the space the strokes are given in to the layer's own.
        let (place, scale) = match space {
            StrokeSpace::Layer => (glam::DAffine3::IDENTITY, 1.0),
            StrokeSpace::Viewer => {
                let space = self
                    .layer_space(&frame, layer)
                    .ok_or_else(|| CommandError::invalid("that layer cannot be drawn on here"))?;
                (space.layer_from_viewer, space.scale)
            }
        };
        let paint_color = self.tools.color;
        let mut built = Vec::with_capacity(specs.len());
        for spec in specs {
            let usable = !spec.points.is_empty()
                && spec.points.len() <= MAX_STROKE_POINTS
                && spec.points.iter().flatten().all(|v| v.is_finite())
                && spec.width.is_finite()
                && spec.width > 0.0;
            if !usable {
                return Err(CommandError::invalid(
                    "a stroke needs between one and 8,192 points and a width above zero",
                ));
            }
            let normal = spec
                .normal
                .map(glam::DVec3::from_array)
                .and_then(|n| place.transform_vector3(n).try_normalize())
                .unwrap_or(glam::DVec3::Z);
            let up = normal.any_orthonormal_vector();
            let samples: Vec<Sample> = spec
                .points
                .iter()
                .map(|p| Sample {
                    pos: place.transform_point3(glam::DVec3::from_array(*p)),
                    normal,
                    up,
                    view_dir: -normal,
                    pressure: 1.0,
                    size: (spec.width as f64 * scale) as f32,
                    color: spec.color.unwrap_or(paint_color),
                    opacity: spec.opacity.clamp(0.0, 1.0),
                })
                .collect();
            let style = StrokeStyle {
                brush: spec.brush.brush(),
                cap: spec.brush.cap(),
                width_taper: Taper::None,
                opacity_taper: Taper::None,
                cap_gap: (1e-4 * scale) as f32,
            };
            built.push((spec.brush.brush(), build_points(&samples, &style)));
        }
        let drawing = self
            .doc_mut()
            .drawing_mut(target.drawing)
            .ok_or_else(|| CommandError::invalid("that frame no longer exists"))?;
        let mut ids = BTreeSet::new();
        // Found once and counted up: looking for a free id per stroke would go over
        // every stroke in the drawing each time.
        let mut next_id = drawing.next_stroke_id();
        for (brush, points) in built {
            let id = next_id;
            next_id = next_id.wrapping_add(1);
            drawing.push_stroke(crate::model::Stroke::new(id, brush, false, points));
            ids.insert(id);
        }
        let added = ids.len();
        if select {
            self.selection.clear();
            self.selection.set_drawing(target.drawing, ids);
        }
        Ok(json!({ "strokes": added, "layer": layer }))
    }

    /// Selects every stroke the Selection tool can reach now.
    pub(crate) fn select_all(&mut self) {
        let frame = self.evaluate();
        self.selection.clear();
        for target in self.select_scope(&frame) {
            if let Some(drawing) = self.doc.drawing(target.drawing) {
                let ids: BTreeSet<u32> = drawing.strokes().map(|s| s.id).collect();
                self.selection.set_drawing(target.drawing, ids);
            }
        }
        self.touch_state();
    }

    pub(crate) fn deselect(&mut self) {
        if !self.selection.is_empty() {
            self.selection.clear();
            self.touch_state();
        }
    }

    /// The selection as a list, with the layer each drawing belongs to.
    fn selected(&self) -> Vec<(LayerId, DrawingId, BTreeSet<u32>)> {
        self.selection
            .drawings()
            .filter_map(|(drawing, ids)| {
                Some((self.doc.layer_of_drawing(drawing)?, drawing, ids.clone()))
            })
            .collect()
    }

    pub(crate) fn delete_selection(&mut self) -> Result<Value, CommandError> {
        let selected = self.selected();
        if selected.is_empty() {
            return Err(CommandError::invalid("Nothing is selected"));
        }
        let mut deleted = 0;
        for (layer, drawing, ids) in selected {
            self.require_unlocked(layer)?;
            if let Some(drawing) = self.doc_mut().drawing_mut(drawing) {
                deleted += drawing.retain(|stroke| !ids.contains(&stroke.id));
            }
        }
        self.selection.clear();
        Ok(json!({ "deleted": deleted }))
    }

    /// Moves or copies the selected strokes into a new paint layer placed right after
    /// the layer they come from. Strokes selected in several frames of one animated
    /// layer keep their frames.
    pub(crate) fn selection_to_new_layer(&mut self, copy: bool) -> Result<Value, CommandError> {
        let selected = self.selected();
        if selected.is_empty() {
            return Err(CommandError::invalid("Nothing is selected"));
        }
        if !copy {
            for (layer, _, _) in &selected {
                self.require_unlocked(*layer)?;
            }
        }
        // The new layer takes after the paint layer when strokes of it are selected,
        // otherwise after the first layer in the selection.
        let source_id = self
            .paint_layer()
            .filter(|id| selected.iter().any(|(layer, _, _)| layer == id))
            .unwrap_or(selected[0].0);
        let frame = self.evaluate();
        let source = self
            .doc
            .layer(source_id)
            .ok_or(OpError::NoSuchLayer(source_id))?
            .clone();
        let source_paint = source
            .as_paint()
            .ok_or(OpError::NotPaint(source_id))?
            .clone();
        let parent = source.parent.ok_or(OpError::NoSuchLayer(source_id))?;
        let index = self
            .doc
            .index_in_parent(source_id)
            .map_or(usize::MAX, |i| i + 1);
        let into_source_space = frame.world_xform(&self.doc, source_id).inverse();

        // Which drawing of the new layer each selected drawing's strokes go to.
        let single_layer = selected.iter().all(|(layer, _, _)| *layer == source_id);
        let keeps_frames = single_layer && source_paint.is_animated() && selected.len() > 1;
        let mut new_drawings: Vec<Drawing> = if keeps_frames {
            vec![Drawing::new(); source_paint.drawings.len()]
        } else {
            vec![Drawing::new()]
        };
        let mut moved: BTreeMap<usize, BTreeSet<u32>> = BTreeMap::new();
        // The new drawings start empty, so their ids simply count up from zero.
        let mut next_ids = vec![0u32; new_drawings.len()];
        for (layer, drawing_id, ids) in &selected {
            let slot = if keeps_frames {
                source_paint
                    .drawings
                    .iter()
                    .position(|d| d == drawing_id)
                    .unwrap_or(0)
            } else {
                0
            };
            let to_new = if *layer == source_id {
                None
            } else {
                Some(into_source_space.mul(&frame.world_xform(&self.doc, *layer)))
            };
            let Some(drawing) = self.doc.drawing(*drawing_id).cloned() else {
                continue;
            };
            for stroke in drawing.strokes().filter(|s| ids.contains(&s.id)) {
                let mut copy_of = match &to_new {
                    Some(x) => transformed_stroke(stroke, x),
                    None => (**stroke).clone(),
                };
                copy_of.id = next_ids[slot];
                next_ids[slot] = next_ids[slot].wrapping_add(1);
                moved.entry(slot).or_default().insert(copy_of.id);
                new_drawings[slot].push_stroke(copy_of);
            }
        }

        if !copy {
            for (_, drawing_id, ids) in &selected {
                if let Some(drawing) = self.doc_mut().drawing_mut(*drawing_id) {
                    drawing.retain(|stroke| !ids.contains(&stroke.id));
                }
            }
        }
        let doc = self.doc_mut();
        let drawing_ids: Vec<DrawingId> = new_drawings
            .into_iter()
            .map(|d| doc.add_drawing(d))
            .collect();
        let paint = if keeps_frames {
            Paint {
                drawings: drawing_ids.clone(),
                ..source_paint.clone()
            }
        } else {
            Paint::single(drawing_ids[0], source_paint.rate)
        };
        let name = unique_name(doc, parent, &source.name);
        let mut layer = doc.new_layer(name, LayerKind::Paint(paint), 0);
        layer.transform = source.transform;
        layer.pivot = source.pivot;
        layer.opacity = source.opacity;
        layer.anim = source.anim.clone();
        let id = doc.insert_layer(layer, parent, index)?;

        self.selection.clear();
        for (slot, ids) in moved {
            self.selection.set_drawing(drawing_ids[slot], ids);
        }
        self.set_current_layer(id);
        Ok(json!({ "layer": id }))
    }

    /// Simplifies every stroke in scope with the Optimize tool's threshold.
    pub(crate) fn optimize(&mut self, scope: OptimizeScope) -> Result<Value, CommandError> {
        let layers: Vec<LayerId> = match scope {
            OptimizeScope::Layer => {
                let layer = self.paint_layer().ok_or_else(|| {
                    CommandError::invalid(
                        self.not_a_paint_layer("Choose a paint layer to optimize."),
                    )
                })?;
                self.require_unlocked(layer)?;
                vec![layer]
            }
            OptimizeScope::All => {
                let unlocked = |id: &LayerId| {
                    self.doc.layer(*id).is_some_and(|l| !l.locked)
                        && self
                            .doc
                            .ancestors(*id)
                            .all(|a| self.doc.layer(a).is_some_and(|l| !l.locked))
                };
                self.doc
                    .walk()
                    .into_iter()
                    .filter(|id| self.doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
                    .filter(unlocked)
                    .collect()
            }
        };
        let threshold = self.tools.optimize.threshold;
        let drawings: BTreeSet<DrawingId> = layers
            .iter()
            .filter_map(|id| self.doc.layer(*id)?.as_paint())
            .flat_map(|p| p.drawings.iter().copied())
            .collect();
        let (mut before, mut after) = (0usize, 0usize);
        for id in drawings {
            let Some(points) = self.doc.drawing(id).map(|d| d.point_count()) else {
                continue;
            };
            before += points;
            let mut changed = 0;
            if let Some(drawing) = self.doc.drawing_mut(id) {
                changed =
                    drawing.edit_strokes(|_| true, |stroke| simplify_stroke(stroke, threshold));
                after += drawing.point_count();
            }
            if changed > 0 {
                self.edits += 1;
            }
        }
        Ok(json!({ "points_before": before, "points_after": after }))
    }
}
