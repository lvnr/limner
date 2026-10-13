//! Undo and redo by document snapshots.
//!
//! A document is cheap to clone because its parts are shared, and an edit copies only
//! what it touches. So the history simply keeps whole documents: every operation is
//! undoable without any per-operation revert code, and an undo step costs only the
//! memory of the strokes and layers that step changed.

use crate::model::Document;

/// One undoable step.
#[derive(Clone, Debug)]
struct Entry {
    /// The document as it was before the step.
    before: Document,
    /// What the step did, for menus ("Undo Paint stroke").
    label: String,
}

#[derive(Debug)]
pub struct History {
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    limit: usize,
}

impl Default for History {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: 200,
        }
    }
}

impl History {
    pub fn with_limit(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            ..Self::default()
        }
    }

    /// Records the state before an edit. Clears the redo stack: a new edit starts a new branch.
    pub fn record(&mut self, before: Document, label: impl Into<String>) {
        self.redo.clear();
        self.undo.push(Entry {
            before,
            label: label.into(),
        });
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(..excess);
        }
    }

    /// Drops the most recent record without touching redo, for edits that turned out to change nothing.
    pub fn discard_last(&mut self) -> Option<Document> {
        self.undo.pop().map(|e| e.before)
    }

    /// Steps back: returns the document to restore and the label of the undone step.
    pub fn undo(&mut self, current: &Document) -> Option<(Document, String)> {
        let entry = self.undo.pop()?;
        self.redo.push(Entry {
            before: current.clone(),
            label: entry.label.clone(),
        });
        Some((entry.before, entry.label))
    }

    /// Steps forward again after an undo.
    pub fn redo(&mut self, current: &Document) -> Option<(Document, String)> {
        let entry = self.redo.pop()?;
        self.undo.push(Entry {
            before: current.clone(),
            label: entry.label.clone(),
        });
        Some((entry.before, entry.label))
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }

    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc_named(title: &str) -> Document {
        let mut doc = Document::new();
        doc.settings.title = title.to_owned();
        doc
    }

    #[test]
    fn undo_and_redo_walk_the_same_states() {
        let mut history = History::default();
        let a = doc_named("a");
        let b = doc_named("b");
        let c = doc_named("c");
        history.record(a.clone(), "to b");
        history.record(b.clone(), "to c");
        assert_eq!(history.undo_label(), Some("to c"));

        let (restored, label) = history.undo(&c).unwrap();
        assert_eq!(
            (restored.settings.title.as_str(), label.as_str()),
            ("b", "to c")
        );
        let (restored, _) = history.undo(&b).unwrap();
        assert_eq!(restored.settings.title, "a");
        assert!(history.undo(&a).is_none());

        let (restored, label) = history.redo(&a).unwrap();
        assert_eq!(
            (restored.settings.title.as_str(), label.as_str()),
            ("b", "to b")
        );
        let (restored, _) = history.redo(&b).unwrap();
        assert_eq!(restored.settings.title, "c");
        assert!(history.redo(&c).is_none());
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut history = History::default();
        history.record(doc_named("a"), "edit");
        history.undo(&doc_named("b")).unwrap();
        assert!(history.can_redo());
        history.record(doc_named("a"), "other edit");
        assert!(!history.can_redo());
    }

    #[test]
    fn the_oldest_steps_fall_off_at_the_limit() {
        let mut history = History::with_limit(3);
        for i in 0..10 {
            history.record(doc_named(&i.to_string()), format!("step {i}"));
        }
        assert_eq!(history.undo_depth(), 3);
        let (oldest_kept, _) = {
            let current = doc_named("now");
            history.undo(&current).unwrap();
            history.undo(&current).unwrap();
            history.undo(&current).unwrap()
        };
        assert_eq!(oldest_kept.settings.title, "7");
    }
}
