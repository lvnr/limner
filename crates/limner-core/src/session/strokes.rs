//! Commands that act on strokes: the selection commands and Optimize.

use super::command::OptimizeScope;
use super::{CommandError, Session};
use crate::model::{Drawing, DrawingId, LayerId, LayerKind, Paint};
use crate::ops::layers::unique_name;
use crate::ops::simplify::simplify_stroke;
use crate::ops::strokes::transformed_stroke;
use crate::ops::OpError;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

impl Session {
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
            .paint_layer
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
                copy_of.id = new_drawings[slot].next_stroke_id();
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
                let layer = self
                    .paint_layer
                    .ok_or_else(|| CommandError::invalid("There is no paint layer to optimize"))?;
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
