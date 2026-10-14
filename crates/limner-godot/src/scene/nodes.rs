//! Layers that are not paint: pictures, models, sounds, spawn areas, cameras and lights.

use godot::prelude::*;
use limner::core::eval::RenderFrame;
use limner::core::glam::DAffine3;
use limner::core::session::Session;

/// The nodes that show the non-paint layers of the document.
#[derive(Default)]
pub(super) struct LayerNodes {}

impl LayerNodes {
    /// Brings the nodes in line with the document at the evaluated moment.
    pub(super) fn sync(
        &mut self,
        _parent: &mut Gd<Node3D>,
        _session: &Session,
        _frame: &RenderFrame,
        _viewer_from_world: &DAffine3,
    ) {
    }

    /// Frees every node, as when another document is opened.
    pub(super) fn clear(&mut self) {}
}
