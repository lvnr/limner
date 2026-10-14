//! The stroke selection: whole strokes, possibly in several layers and frames.
//!
//! A selection names strokes; it is not a region of space. It is not part of the
//! document and not part of undo history: it is cleaned up after every change so that
//! it never names a stroke that no longer exists.

use crate::model::{Document, DrawingId};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// For each drawing, the ids of its selected strokes. No entry is ever empty.
    strokes: BTreeMap<DrawingId, BTreeSet<u32>>,
    /// Counts the changes to what is selected.
    revision: u64,
}

impl Selection {
    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    /// A number that changes whenever the set of selected strokes does.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The number of selected strokes.
    pub fn count(&self) -> usize {
        self.strokes.values().map(BTreeSet::len).sum()
    }

    pub fn contains(&self, drawing: DrawingId, stroke: u32) -> bool {
        self.strokes
            .get(&drawing)
            .is_some_and(|ids| ids.contains(&stroke))
    }

    /// Returns true when the stroke was not selected before.
    pub fn insert(&mut self, drawing: DrawingId, stroke: u32) -> bool {
        let added = self.strokes.entry(drawing).or_default().insert(stroke);
        self.revision += u64::from(added);
        added
    }

    /// Returns true when the stroke was selected.
    pub fn remove(&mut self, drawing: DrawingId, stroke: u32) -> bool {
        let Some(ids) = self.strokes.get_mut(&drawing) else {
            return false;
        };
        let removed = ids.remove(&stroke);
        if ids.is_empty() {
            self.strokes.remove(&drawing);
        }
        self.revision += u64::from(removed);
        removed
    }

    pub fn clear(&mut self) {
        self.revision += u64::from(!self.strokes.is_empty());
        self.strokes.clear();
    }

    /// The selected strokes of one drawing.
    pub fn in_drawing(&self, drawing: DrawingId) -> Option<&BTreeSet<u32>> {
        self.strokes.get(&drawing)
    }

    /// Every drawing with selected strokes, and their ids.
    pub fn drawings(&self) -> impl Iterator<Item = (DrawingId, &BTreeSet<u32>)> + '_ {
        self.strokes.iter().map(|(drawing, ids)| (*drawing, ids))
    }

    /// Replaces the selected strokes of one drawing; an empty set deselects it.
    pub fn set_drawing(&mut self, drawing: DrawingId, ids: BTreeSet<u32>) {
        self.revision += 1;
        if ids.is_empty() {
            self.strokes.remove(&drawing);
        } else {
            self.strokes.insert(drawing, ids);
        }
    }

    /// Forgets strokes and drawings that are no longer in the document.
    pub fn retain_existing(&mut self, doc: &Document) {
        let before = self.count();
        self.strokes.retain(|drawing, ids| {
            let Some(drawing) = doc.drawing(*drawing) else {
                return false;
            };
            let existing: BTreeSet<u32> = drawing.strokes().map(|s| s.id).collect();
            ids.retain(|id| existing.contains(id));
            !ids.is_empty()
        });
        self.revision += u64::from(self.count() != before);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Brush, Drawing, Point, Stroke};

    fn stroke(id: u32) -> Stroke {
        let p = Point {
            pos: [0.0; 3],
            normal: [0.0, 0.0, 1.0],
            dir: [0.0, 0.0, -1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.1,
        };
        Stroke::new(id, Brush::Ribbon, false, vec![p, p])
    }

    #[test]
    fn strokes_are_counted_across_drawings_and_empty_entries_vanish() {
        let mut selection = Selection::default();
        assert!(selection.insert(DrawingId(1), 4));
        assert!(!selection.insert(DrawingId(1), 4));
        selection.insert(DrawingId(1), 5);
        selection.insert(DrawingId(2), 4);
        assert_eq!(selection.count(), 3);
        assert!(selection.contains(DrawingId(2), 4) && !selection.contains(DrawingId(2), 5));
        assert!(selection.remove(DrawingId(2), 4));
        assert!(!selection.remove(DrawingId(2), 4));
        assert_eq!(selection.drawings().count(), 1);
        selection.set_drawing(DrawingId(1), BTreeSet::new());
        assert!(selection.is_empty());
    }

    #[test]
    fn strokes_that_no_longer_exist_are_forgotten() {
        let mut doc = Document::new();
        let paint = doc.find_by_path("Root/Paint").unwrap();
        let drawing = doc.layer(paint).unwrap().as_paint().unwrap().drawings[0];
        *doc.drawing_mut(drawing).unwrap() = Drawing::from_strokes([stroke(0), stroke(1)]);
        let mut selection = Selection::default();
        selection.insert(drawing, 0);
        selection.insert(drawing, 9);
        selection.insert(DrawingId(999_999), 0);
        selection.retain_existing(&doc);
        assert_eq!(selection.count(), 1);
        assert!(selection.contains(drawing, 0));
    }
}
