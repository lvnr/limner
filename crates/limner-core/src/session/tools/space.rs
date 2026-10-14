//! Where the hand is in a layer's own space.
//!
//! Strokes are stored in the space of their layer. The hand is known in viewer space.
//! Between the two lie the viewer's placement in the world and the layer's (possibly
//! animated, possibly mirrored) placement in the world.

use crate::eval::{Content, RenderFrame};
use crate::model::{DrawingId, LayerId};
use crate::session::Session;
use glam::{DAffine3, DVec3};

#[derive(Clone, Copy, Debug)]
pub struct LayerSpace {
    pub layer: LayerId,
    /// Maps viewer space into the layer's space.
    pub layer_from_viewer: DAffine3,
    /// Layer units per viewer-space metre.
    pub scale: f64,
}

impl LayerSpace {
    pub fn point(&self, viewer: DVec3) -> DVec3 {
        self.layer_from_viewer.transform_point3(viewer)
    }

    /// A direction, kept at unit length.
    pub fn direction(&self, viewer: DVec3) -> DVec3 {
        self.layer_from_viewer
            .transform_vector3(viewer)
            .normalize_or_zero()
    }

    /// A displacement, scaled with the space.
    pub fn vector(&self, viewer: DVec3) -> DVec3 {
        self.layer_from_viewer.transform_vector3(viewer)
    }

    pub fn length(&self, metres: f64) -> f64 {
        metres * self.scale
    }
}

/// The drawing a tool works on: the frame a paint layer shows now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintTarget {
    pub layer: LayerId,
    pub drawing: DrawingId,
}

impl Session {
    /// The mirror the tools work across in `layer`'s own space, when mirrored working
    /// is on: a reflection in the plane through the layer's pivot.
    pub(crate) fn mirror_in(&self, layer: LayerId) -> Option<crate::math::Xform> {
        let flip = self.tools.symmetry.flip()?;
        let pivot = self.doc.layer(layer)?.pivot;
        Some(
            pivot
                .mul(&crate::math::Xform::from_flip(flip))
                .mul(&pivot.inverse()),
        )
    }

    /// Where the mirror plane is in viewer space, for drawing it: a point on it, and
    /// the turn that takes the Z axis onto the direction the plane is square to.
    pub fn mirror_plane(&self) -> Option<(DVec3, glam::DQuat)> {
        let flip = self.tools.symmetry.flip()?;
        let layer = self.paint_layer?;
        let pivot = self.doc.layer(layer)?.pivot;
        let frame = self.evaluate();
        frame.get(layer)?;
        let placed = self
            .view
            .viewer_from_world()
            .mul(&frame.world_xform(&self.doc, layer))
            .mul(&pivot);
        let normal = placed.rotation * flip.signs().map(|s| if s < 0.0 { 1.0 } else { 0.0 });
        let turn = glam::DQuat::from_rotation_arc(DVec3::Z, normal.try_normalize()?);
        placed
            .translation
            .is_finite()
            .then_some((placed.translation, turn))
    }

    pub(crate) fn layer_space(&self, frame: &RenderFrame, layer: LayerId) -> Option<LayerSpace> {
        let state = frame.get(layer)?;
        let layer_from_viewer = state.world.inverse() * self.view.world_from_viewer.to_affine();
        let scale = layer_from_viewer.matrix3.determinant().abs().cbrt();
        (scale.is_finite() && scale > 0.0 && layer_from_viewer.is_finite()).then_some(LayerSpace {
            layer,
            layer_from_viewer,
            scale,
        })
    }

    /// The drawing a paint layer shows at the playhead, if the layer can be worked on:
    /// it must be unlocked and on screen.
    pub(crate) fn editable_drawing(
        &self,
        frame: &RenderFrame,
        id: LayerId,
    ) -> Result<PaintTarget, String> {
        let layer = self.doc.layer(id).ok_or("That layer no longer exists")?;
        if layer.as_paint().is_none() {
            return Err(format!("\"{}\" is not a paint layer", layer.name));
        }
        if layer.locked {
            return Err(format!("Layer \"{}\" is locked", layer.name));
        }
        let state = frame.get(id).ok_or("That layer no longer exists")?;
        if !state.visible {
            return Err(if state.in_clip {
                format!("Layer \"{}\" is hidden", layer.name)
            } else {
                format!("Layer \"{}\" is not on screen at this time", layer.name)
            });
        }
        match state.content {
            Content::Paint { drawing, .. } => Ok(PaintTarget { layer: id, drawing }),
            Content::None => Err(format!("Layer \"{}\" has no frame to draw on", layer.name)),
        }
    }
}
