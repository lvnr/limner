//! The Selection tool, and finding the stroke nearest the hand.

use super::space::PaintTarget;
use super::sphere::reach;
use crate::eval::{Content, RenderFrame};
use crate::model::{LayerId, Point};
use crate::session::input::InputFrame;
use crate::session::Session;
use glam::DVec3;

/// The modifier pressed this far adds to the selection; pressed all the way it subtracts.
const ADD_FROM: f32 = 0.1;
const SUBTRACT_FROM: f32 = 0.9;

/// The Selection tool at work: strokes the sphere sweeps over join the selection, or leave it.
pub(crate) struct SelectGesture {
    subtract: bool,
}

/// The stroke point nearest the hand.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Nearest {
    pub target: PaintTarget,
    pub stroke: u32,
    pub point: Point,
    /// Distance from the hand to the stroke's surface, in viewer-space metres.
    pub distance: f64,
}

impl Session {
    /// The paint layers the Selection tool reaches now: the paint layer, or every
    /// visible unlocked one when that option is on.
    fn selectable_layers(&self, frame: &RenderFrame) -> Vec<LayerId> {
        let usable = |id: &LayerId| {
            self.doc
                .layer(*id)
                .is_some_and(|l| l.as_paint().is_some() && !l.locked)
                && frame.get(*id).is_some_and(|s| s.visible)
        };
        if self.options.select_in_all_layers {
            self.doc.walk().into_iter().filter(usable).collect()
        } else {
            self.paint_layer.into_iter().filter(usable).collect()
        }
    }

    /// The drawings the Selection tool reaches now: the frame each layer shows, or
    /// all of a layer's frames when that option is on.
    pub(crate) fn select_scope(&self, frame: &RenderFrame) -> Vec<PaintTarget> {
        let mut scope = Vec::new();
        for layer in self.selectable_layers(frame) {
            let Some(paint) = self.doc.layer(layer).and_then(|l| l.as_paint()) else {
                continue;
            };
            if self.options.select_in_all_frames {
                scope.extend(paint.drawings.iter().map(|drawing| PaintTarget {
                    layer,
                    drawing: *drawing,
                }));
            } else if let Some(Content::Paint { drawing, .. }) = frame.get(layer).map(|s| s.content)
            {
                scope.push(PaintTarget { layer, drawing });
            }
        }
        scope
    }

    /// The stroke point nearest `hand` among the frames shown by `layers`, no further
    /// from a stroke's surface than `limit` viewer-space metres.
    pub(crate) fn nearest_stroke(
        &self,
        frame: &RenderFrame,
        layers: &[LayerId],
        hand: DVec3,
        limit: f64,
    ) -> Option<Nearest> {
        let mut best: Option<Nearest> = None;
        for layer in layers {
            let (Some(space), Some(Content::Paint { drawing, .. })) = (
                self.layer_space(frame, *layer),
                frame.get(*layer).map(|s| s.content),
            ) else {
                continue;
            };
            let Some(strokes) = self.doc.drawing(drawing) else {
                continue;
            };
            let center = space.point(hand).as_vec3();
            let mut limit_here = space.length(best.map_or(limit, |b| b.distance)) as f32;
            for chunk in strokes.chunks() {
                if !chunk.bounds().intersects_sphere(center, limit_here) {
                    continue;
                }
                for stroke in chunk.strokes() {
                    if !stroke.bounds().intersects_sphere(center, limit_here) {
                        continue;
                    }
                    for point in stroke.points() {
                        let distance = reach(point, center);
                        if distance < limit_here {
                            limit_here = distance;
                            best = Some(Nearest {
                                target: PaintTarget {
                                    layer: *layer,
                                    drawing,
                                },
                                stroke: stroke.id,
                                point: *point,
                                distance: distance as f64 / space.scale,
                            });
                        }
                    }
                }
            }
        }
        best
    }

    /// Visible paint layers, for picking a colour from anything on screen.
    pub(crate) fn visible_paint_layers(&self, frame: &RenderFrame) -> Vec<LayerId> {
        frame
            .layers
            .iter()
            .filter(|s| s.visible && matches!(s.content, Content::Paint { .. }))
            .map(|s| s.id)
            .collect()
    }
}

impl SelectGesture {
    pub(crate) fn begin(session: &mut Session, input: &InputFrame) -> Self {
        let alt = input.alt();
        // A plain press starts a new selection, so pressing in empty space deselects everything.
        if alt < ADD_FROM && !session.selection.is_empty() {
            session.selection.clear();
            session.touch_state();
        }
        let mut gesture = Self {
            subtract: alt >= SUBTRACT_FROM,
        };
        gesture.update(session, input);
        gesture
    }

    pub(crate) fn update(&mut self, session: &mut Session, input: &InputFrame) -> bool {
        let frame = session.evaluate();
        let size = session.tools.select.size;
        let mut changed = false;
        for target in session.select_scope(&frame) {
            let (Some(space), Some(drawing)) = (
                session.layer_space(&frame, target.layer),
                session.doc.drawing(target.drawing).cloned(),
            ) else {
                continue;
            };
            let center = space.point(input.main.position).as_vec3();
            let radius = space.length(size as f64) as f32;
            for chunk in drawing.chunks() {
                if !chunk.bounds().intersects_sphere(center, radius) {
                    continue;
                }
                for stroke in chunk.strokes() {
                    // Erased parts of a stroke are still part of it and can still be touched.
                    let touched = stroke.bounds().intersects_sphere(center, radius)
                        && stroke.points().iter().any(|p| reach(p, center) < radius);
                    if touched {
                        changed |= if self.subtract {
                            session.selection.remove(target.drawing, stroke.id)
                        } else {
                            session.selection.insert(target.drawing, stroke.id)
                        };
                    }
                }
            }
        }
        if changed {
            session.touch_state();
        }
        true
    }
}
