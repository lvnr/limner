//! An editing session: a document plus everything about working on it that is not
//! part of the document itself (the view, the playhead, the tool in hand).

mod view;

pub use view::{View, DEFAULT_ZOOM_LIMITS};

use crate::eval::{evaluate, Clock, RenderFrame};
use crate::math::Aabb;
use crate::model::{Document, LayerKind};
use glam::DVec3;

pub struct Session {
    doc: Document,
    pub view: View,
    pub clock: Clock,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        let mut session = Self {
            doc,
            view: View::default(),
            clock: Clock::default(),
        };
        session.go_to_default_viewpoint();
        session
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    /// Evaluates the document at the session's clock.
    pub fn evaluate(&self) -> RenderFrame {
        evaluate(&self.doc, self.clock)
    }

    /// Stands the viewer on the document's default spawn area, if it has one.
    pub fn go_to_default_viewpoint(&mut self) {
        if let Some(id) = self.doc.settings.default_viewpoint {
            let mut placement = self.doc.world_xform(id);
            // A spawn area places and scales the viewer; a mirrored viewer makes no sense.
            placement.flip = crate::math::Flip::None;
            if placement.is_finite() && placement.scale > 0.0 {
                self.view.world_from_viewer = placement;
            }
        }
    }

    /// World-space bounds of everything visible at the current time, as a centre and radius.
    pub fn visible_bounds(&self) -> Option<(DVec3, f64)> {
        let frame = self.evaluate();
        let mut min = DVec3::INFINITY;
        let mut max = DVec3::NEG_INFINITY;
        for state in frame.layers.iter().filter(|s| s.visible) {
            let Some(layer) = self.doc.layer(state.id) else {
                continue;
            };
            let bounds: Aabb = match (&layer.kind, state.content) {
                (LayerKind::Paint(_), crate::eval::Content::Paint { drawing, .. }) => self
                    .doc
                    .drawing(drawing)
                    .map_or(Aabb::EMPTY, |d| d.bounds()),
                _ => Aabb::EMPTY,
            };
            if let Some((lo, hi)) = bounds.transformed(&state.world) {
                min = min.min(lo);
                max = max.max(hi);
            }
        }
        (min.x <= max.x).then(|| ((min + max) * 0.5, ((max - min).length() * 0.5).max(1e-9)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::Xform;
    use crate::model::{Brush, Drawing, Point, Stroke};

    #[test]
    fn a_session_starts_on_the_default_spawn_area() {
        let mut doc = Document::new();
        let spawn = doc.settings.default_viewpoint.unwrap();
        doc.layer_mut(spawn).unwrap().transform = Xform {
            scale: 3.0,
            translation: DVec3::new(1.0, 0.0, 2.0),
            flip: crate::math::Flip::X,
            ..Xform::IDENTITY
        };
        let session = Session::new(doc);
        assert_eq!(session.view.world_from_viewer.scale, 3.0);
        assert_eq!(
            session.view.world_from_viewer.translation,
            DVec3::new(1.0, 0.0, 2.0)
        );
        assert!(!session.view.world_from_viewer.flip.is_mirrored());
    }

    #[test]
    fn visible_bounds_cover_the_painted_strokes() {
        let mut doc = Document::new();
        assert!(
            Session::new(doc.clone()).visible_bounds().is_none(),
            "an empty document has no bounds"
        );
        let paint = doc.find_by_path("Root/Paint").unwrap();
        let drawing = doc.layer(paint).unwrap().as_paint().unwrap().drawings[0];
        let p = |x: f32| Point {
            pos: [x, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, 1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.0,
        };
        *doc.drawing_mut(drawing).unwrap() =
            Drawing::from_strokes([Stroke::new(0, Brush::Ribbon, false, vec![p(-1.0), p(3.0)])]);
        doc.layer_mut(paint).unwrap().transform =
            Xform::from_translation(DVec3::new(0.0, 10.0, 0.0));
        let (center, radius) = Session::new(doc.clone()).visible_bounds().unwrap();
        assert!((center - DVec3::new(1.0, 10.0, 0.0)).length() < 1e-9);
        assert!((radius - 2.0).abs() < 1e-9);
        doc.layer_mut(paint).unwrap().visible = false;
        assert!(Session::new(doc).visible_bounds().is_none());
    }
}
