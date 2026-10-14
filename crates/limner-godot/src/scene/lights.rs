//! Light layers: real lights in the scene, which shade the layers set to be lit.

use super::nodes::{release, Placement};
use crate::convert::{color_from_linear, transform3d};
use godot::classes::light_3d::Param;
use godot::classes::{DirectionalLight3D, Light3D, OmniLight3D, SpotLight3D};
use godot::prelude::*;
use limner::core::eval::LayerState;
use limner::core::model::{Light, LightKind};

/// The exponent of the renderer's fall-off across a spot light's cone, from the
/// softness of its edge. The renderer dims a spot by `1 - rim^exponent`, where `rim`
/// runs from 0 on the axis to 1 at the edge: a high exponent keeps the light full
/// almost to the edge, and 1 fades it evenly from the middle.
pub(super) fn spot_attenuation(blend: f32) -> f32 {
    (1.0 / blend.max(1e-3)).clamp(1.0, 64.0)
}

/// How far a light reaches in viewer space. The layer gives the reach in its own
/// units; the layer's and the view's scale turn that into metres.
pub(super) fn reach(range: f32, scale: f64) -> f32 {
    (f64::from(range) * scale).clamp(1e-4, 1e9) as f32
}

/// The settings a light node was last given.
#[derive(Clone, PartialEq)]
struct Applied {
    light: Light,
    opacity: f32,
    scale: f64,
}

pub(super) struct LightNode {
    node: Gd<Light3D>,
    kind: LightKind,
    applied: Option<Applied>,
    transform: Option<Transform3D>,
    visible: bool,
}

impl LightNode {
    pub(super) fn new(holder: &mut Gd<Node3D>, state: &LayerState, kind: LightKind) -> Self {
        let mut node: Gd<Light3D> = match kind {
            LightKind::Point => OmniLight3D::new_alloc().upcast(),
            LightKind::Spot => SpotLight3D::new_alloc().upcast(),
            LightKind::Directional => DirectionalLight3D::new_alloc().upcast(),
        };
        node.set_name(&format!("Light_{}", state.id.0));
        node.set_visible(false);
        holder.add_child(&node);
        Self {
            node,
            kind,
            applied: None,
            transform: None,
            visible: false,
        }
    }

    pub(super) fn kind(&self) -> LightKind {
        self.kind
    }

    /// Brings the light in line with its layer. `place` is the layer's place in
    /// viewer space; the light shines along its -Z.
    pub(super) fn sync(&mut self, light: &Light, state: &LayerState, place: &Placement) {
        // A layer that is hidden, outside its clip or faded out gives no light.
        let visible = state.visible && state.opacity > 0.0 && light.energy > 0.0;
        if self.visible != visible {
            self.node.set_visible(visible);
            self.visible = visible;
        }
        if !visible {
            return;
        }
        let applied = Applied {
            light: light.clone(),
            opacity: state.opacity,
            scale: place.scale,
        };
        if self.applied.as_ref() != Some(&applied) {
            self.node.set_color(color_from_linear(light.color));
            self.node
                .set_param(Param::ENERGY, light.energy * state.opacity);
            self.node.set_shadow(light.shadows);
            match self.kind {
                LightKind::Point => {
                    self.node
                        .set_param(Param::RANGE, reach(light.range, place.scale));
                }
                LightKind::Spot => {
                    self.node
                        .set_param(Param::RANGE, reach(light.range, place.scale));
                    self.node.set_param(Param::SPOT_ANGLE, light.spot_angle);
                    self.node
                        .set_param(Param::SPOT_ATTENUATION, spot_attenuation(light.spot_blend));
                }
                LightKind::Directional => {}
            }
            self.applied = Some(applied);
        }
        // Lights take no scale: only where the layer is and which way it faces.
        let transform = transform3d(&place.rigid);
        if self.transform != Some(transform) {
            self.node.set_transform(transform);
            self.transform = Some(transform);
        }
    }

    pub(super) fn free(self) {
        release(self.node.upcast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_soft_spot_fades_from_the_middle_and_a_hard_one_at_the_edge() {
        assert_eq!(spot_attenuation(1.0), 1.0);
        assert_eq!(spot_attenuation(0.5), 2.0);
        assert!((spot_attenuation(0.15) - 6.6667).abs() < 1e-3);
        assert_eq!(spot_attenuation(0.0), 64.0);
        assert_eq!(spot_attenuation(7.0), 1.0);
    }

    #[test]
    fn a_light_reaches_as_far_as_its_layer_and_the_view_make_it() {
        assert_eq!(reach(5.0, 1.0), 5.0);
        assert_eq!(reach(5.0, 0.01), 0.05);
        assert_eq!(reach(5.0, 1e6), 5e6);
        assert_eq!(reach(1e-6, 1e-6), 1e-4, "never a reach of nothing");
    }
}
