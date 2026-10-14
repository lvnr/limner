//! Lights that show: where a new light goes, and which paint layers take light.
//!
//! Paint is drawn as painted until its layer is set to be lit, so a light added to a
//! document whose paint is all unlit would change nothing on screen. Adding the first
//! light therefore lights the paint, and a new light is put where it can be seen and
//! where it shines on the work.

use super::{CommandError, Session};
use crate::math::Xform;
use crate::model::{LayerId, LayerKind, Shading};
use glam::{DMat3, DQuat, DVec3};

/// Where a new light goes, in metres of the room from the eyes: this far ahead and
/// this far up. Ahead of the eyes and a little above them is in sight, in a headset
/// and on a screen, and between the viewer and the work, so it lights the side of the
/// work the viewer sees.
pub const NEW_LIGHT_AHEAD: f64 = 0.8;
pub const NEW_LIGHT_UP: f64 = 0.3;
/// The point a new light is aimed at, from the eyes: where the work usually is.
pub const NEW_LIGHT_AIM_AHEAD: f64 = 1.3;
pub const NEW_LIGHT_AIM_UP: f64 = -0.2;
/// How far a new point or spot light reaches, in metres of the room as the viewer
/// sees it when adding it. A light of energy 1 lights fully at a quarter of its reach.
pub const NEW_LIGHT_RANGE: f64 = 3.0;

/// What the user is told when adding a light lit the paint layers.
pub const PAINT_NOW_LIT: &str =
    "Paint layers now take light. A layer set to Unlit in its properties stays flat.";

impl Session {
    /// The way the viewer faces, level with the floor, in viewer space. The core is
    /// told where the eyes are but not where they look, so it goes by the hands, which
    /// are in front of the user when they work; with no hand in sight, straight ahead
    /// of the room (-Z), which is where a screen's camera looks.
    pub(crate) fn facing(&self) -> DVec3 {
        let input = &self.last_input;
        let hands: Vec<DVec3> = [input.main, input.off]
            .iter()
            .filter(|h| h.tracked && h.position.is_finite())
            .map(|h| h.position)
            .collect();
        if !hands.is_empty() {
            let middle = hands.iter().sum::<DVec3>() / hands.len() as f64;
            let flat = DVec3::new(middle.x - input.head.x, 0.0, middle.z - input.head.z);
            if flat.length() > 0.05 {
                return flat.normalize();
            }
        }
        DVec3::NEG_Z
    }

    /// Where a new light goes in viewer space: ahead of and above the eyes, shining
    /// at the work ahead of them, 45 degrees down.
    pub(crate) fn new_light_place(&self) -> Xform {
        let head = self.last_input.head;
        let ahead = self.facing();
        let position = head + ahead * NEW_LIGHT_AHEAD + DVec3::Y * NEW_LIGHT_UP;
        let aim = head + ahead * NEW_LIGHT_AIM_AHEAD + DVec3::Y * NEW_LIGHT_AIM_UP;
        let rotation = looking_along(aim - position).unwrap_or(DQuat::IDENTITY);
        Xform::new(rotation, 1.0, position, crate::math::Flip::None)
    }

    /// Puts a light layer that was just added where [`Session::new_light_place`]
    /// says, at its parent's scale, reaching as far as suits the view.
    pub(crate) fn place_new_light(&mut self, id: LayerId) -> Result<(), CommandError> {
        let Some(parent) = self.doc.parent_of(id) else {
            return Ok(());
        };
        let frame = self.evaluate();
        let parent_world = frame.world_xform(&self.doc, parent);
        let mut world = self.view.world_from_viewer.mul(&self.new_light_place());
        // The layer keeps a scale of one in its parent: the light's reach says how far it shines.
        world.scale = parent_world.scale;
        let local = parent_world.inverse().mul(&world);
        let range = NEW_LIGHT_RANGE * self.view.scale() / parent_world.scale;
        let Some(local) = local.usable() else {
            return Ok(());
        };
        if !(range.is_finite() && range > 0.0) {
            return Ok(());
        }
        let layer = self
            .doc_mut()
            .layer_mut(id)
            .ok_or(crate::ops::OpError::NoSuchLayer(id))?;
        layer.transform = local;
        if let LayerKind::Light(light) = &mut layer.kind {
            light.range = range as f32;
        }
        Ok(())
    }

    /// The paint layers that the shading of `layers` reaches: each that is a paint
    /// layer, and every paint layer inside each that is a group, leaving out those
    /// that are locked, themselves or through a group they are in. No layers at all
    /// means every paint layer in the document.
    pub(crate) fn shading_targets(&self, layers: &[LayerId]) -> Result<Vec<LayerId>, CommandError> {
        let roots = if layers.is_empty() {
            vec![self.doc.root()]
        } else {
            layers.to_vec()
        };
        let mut out = Vec::new();
        for root in roots {
            if self.doc.layer(root).is_none() {
                return Err(crate::ops::OpError::NoSuchLayer(root).into());
            }
            for id in self.doc.subtree(root) {
                let Some(layer) = self.doc.layer(id) else {
                    continue;
                };
                let locked = layer.locked
                    || self
                        .doc
                        .ancestors(id)
                        .any(|a| self.doc.layer(a).is_some_and(|l| l.locked));
                if layer.as_paint().is_some() && !locked && !out.contains(&id) {
                    out.push(id);
                }
            }
        }
        Ok(out)
    }

    /// Sets the shading of the paint layers [`Session::shading_targets`] finds.
    /// Returns how many changed.
    pub(crate) fn set_shading(
        &mut self,
        layers: &[LayerId],
        shading: Shading,
    ) -> Result<usize, CommandError> {
        let targets = self.shading_targets(layers)?;
        let mut changed = 0;
        for id in targets {
            if self
                .doc
                .layer(id)
                .and_then(|l| l.as_paint())
                .is_some_and(|p| p.shading != shading)
            {
                if let Some(paint) = self.doc_mut().layer_mut(id).and_then(|l| l.as_paint_mut()) {
                    paint.shading = shading;
                    changed += 1;
                }
            }
        }
        Ok(changed)
    }

    /// True when some paint layer in the document takes light.
    pub(crate) fn any_paint_lit(&self) -> bool {
        self.doc.walk().into_iter().any(|id| {
            self.doc
                .layer(id)
                .and_then(|l| l.as_paint())
                .is_some_and(|p| p.shading == Shading::Lit)
        })
    }

    /// True when the document has a light layer.
    pub(crate) fn has_light(&self) -> bool {
        self.doc.walk().into_iter().any(|id| {
            matches!(
                self.doc.layer(id).map(|l| &l.kind),
                Some(LayerKind::Light(_))
            )
        })
    }

    /// After a paint layer was added: it takes light when the document has a light
    /// and some of its paint takes light already, so that what is painted next is
    /// shaded like the rest.
    pub(crate) fn paint_added(&mut self, id: LayerId) {
        if !(self.has_light() && self.any_paint_lit()) {
            return;
        }
        if let Some(paint) = self.doc_mut().layer_mut(id).and_then(|l| l.as_paint_mut()) {
            paint.shading = Shading::Lit;
        }
    }

    /// After a light layer was added: place it, and when no paint takes light yet,
    /// light every paint layer that can be changed. Returns the message to give, if any.
    pub(crate) fn after_light_added(
        &mut self,
        id: LayerId,
    ) -> Result<Option<String>, CommandError> {
        self.place_new_light(id)?;
        if self.any_paint_lit() {
            return Ok(None);
        }
        let changed = self.set_shading(&[], Shading::Lit)?;
        Ok((changed > 0).then(|| PAINT_NOW_LIT.to_owned()))
    }
}

/// The turn that points -Z along `direction`, keeping +Y as near up as it can be.
fn looking_along(direction: DVec3) -> Option<DQuat> {
    let back = (-direction).try_normalize()?;
    let side = DVec3::Y.cross(back).try_normalize().unwrap_or(DVec3::X);
    let up = back.cross(side);
    Some(DQuat::from_mat3(&DMat3::from_cols(side, up, back)).normalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Document, Light, LightKind};
    use crate::session::{HandInput, InputFrame};
    use serde_json::json;

    fn run(session: &mut Session, command: serde_json::Value) -> serde_json::Value {
        session
            .exec_json(&command)
            .unwrap_or_else(|e| panic!("{command} failed: {e}"))
    }

    fn add_light(session: &mut Session) -> (LayerId, serde_json::Value) {
        let result = run(session, json!({"cmd": "add_layer", "kind": "light"}));
        (LayerId(result["layer"].as_u64().unwrap()), result)
    }

    fn light(session: &Session, id: LayerId) -> Light {
        match &session.doc().layer(id).unwrap().kind {
            LayerKind::Light(light) => light.clone(),
            other => panic!("not a light: {other:?}"),
        }
    }

    /// Where a layer is, and which way its -Z points, in viewer space.
    fn seen(session: &Session, id: LayerId) -> (DVec3, DVec3, f64) {
        let frame = session.evaluate();
        let placed = session
            .view
            .viewer_from_world()
            .mul(&frame.world_xform(session.doc(), id));
        (
            placed.translation,
            placed.rotation * DVec3::NEG_Z,
            placed.scale,
        )
    }

    fn shading(session: &Session, id: LayerId) -> Shading {
        session.doc().layer(id).unwrap().as_paint().unwrap().shading
    }

    fn close(a: DVec3, b: DVec3) -> bool {
        (a - b).length() < 1e-9
    }

    #[test]
    fn a_new_light_goes_ahead_of_and_above_the_eyes_and_shines_down_at_the_work() {
        let mut session = Session::new(Document::new());
        let head = InputFrame::default().head;
        let (id, _) = add_light(&mut session);
        let (position, forward, scale) = seen(&session, id);
        assert!(
            close(position, head + DVec3::new(0.0, 0.3, -0.8)),
            "{position:?}"
        );
        // Down at 45 degrees, away from the viewer.
        assert!(
            close(forward, DVec3::new(0.0, -1.0, -1.0).normalize()),
            "{forward:?}"
        );
        assert!((scale - 1.0).abs() < 1e-9);
        assert!((light(&session, id).range - 3.0).abs() < 1e-6);
        assert_eq!(light(&session, id).kind, LightKind::Point);

        // Seen from elsewhere, larger, and with the hands off to one side: the light
        // goes ahead of where the user works, and reaches as far in the room.
        let mut session = Session::new(Document::new());
        session.view.world_from_viewer = Xform::new(
            DQuat::from_rotation_y(0.7),
            4.0,
            DVec3::new(10.0, -2.0, 3.0),
            crate::math::Flip::None,
        );
        let tracked = |position: DVec3| HandInput {
            tracked: true,
            position,
            ..HandInput::default()
        };
        let input = InputFrame {
            head: DVec3::new(0.0, 1.5, 0.0),
            main: tracked(DVec3::new(0.6, 1.2, 0.1)),
            off: tracked(DVec3::new(0.4, 1.2, -0.1)),
            ..InputFrame::default()
        };
        session.update(&input);
        let (id, _) = add_light(&mut session);
        let (position, forward, scale) = seen(&session, id);
        assert!(
            close(position, DVec3::new(0.8, 1.8, 0.0)),
            "ahead is toward the hands: {position:?}"
        );
        assert!(
            close(forward, DVec3::new(1.0, -1.0, 0.0).normalize()),
            "{forward:?}"
        );
        assert!(
            (scale - 0.25).abs() < 1e-9,
            "the root's scale, seen from a view 4 times larger"
        );
        let range = f64::from(light(&session, id).range);
        assert!(
            (range - 12.0).abs() < 1e-5,
            "3 m of the room is 12 units here: {range}"
        );
        let local = session.doc().layer(id).unwrap().transform;
        assert!(
            (local.scale - 1.0).abs() < 1e-9,
            "a scale of one in its parent"
        );
    }

    #[test]
    fn a_light_inside_a_scaled_group_still_lands_ahead_of_the_eyes() {
        let mut session = Session::new(Document::new());
        let group = LayerId(
            run(&mut session, json!({"cmd": "add_layer", "kind": "group"}))["layer"]
                .as_u64()
                .unwrap(),
        );
        let placed = Xform::new(
            DQuat::from_rotation_x(0.3),
            0.5,
            DVec3::new(1.0, 0.0, -2.0),
            crate::math::Flip::None,
        );
        run(
            &mut session,
            json!({"cmd": "set_layer", "layer": group, "transform": placed}),
        );
        let result = run(
            &mut session,
            json!({"cmd": "add_layer", "kind": "light", "parent": group}),
        );
        let id = LayerId(result["layer"].as_u64().unwrap());
        let (position, _, scale) = seen(&session, id);
        let head = InputFrame::default().head;
        assert!(
            close(position, head + DVec3::new(0.0, 0.3, -0.8)),
            "{position:?}"
        );
        assert!((scale - 0.5).abs() < 1e-9, "the group's scale");
        assert!(
            (light(&session, id).range - 6.0).abs() < 1e-5,
            "3 m in a group at half size is 6 of its units"
        );
    }

    #[test]
    fn the_first_light_lights_the_paint_and_one_undo_takes_both_back() {
        let mut session = Session::new(Document::new());
        let paint = session.paint_layer().unwrap();
        let second = LayerId(
            run(&mut session, json!({"cmd": "add_layer", "kind": "paint"}))["layer"]
                .as_u64()
                .unwrap(),
        );
        let locked = LayerId(
            run(&mut session, json!({"cmd": "add_layer", "kind": "paint"}))["layer"]
                .as_u64()
                .unwrap(),
        );
        run(
            &mut session,
            json!({"cmd": "set_layer", "layer": locked, "locked": true}),
        );
        let steps = session.history().undo_depth();
        let (id, result) = add_light(&mut session);
        assert_eq!(result["message"], PAINT_NOW_LIT);
        assert_eq!(shading(&session, paint), Shading::Lit);
        assert_eq!(shading(&session, second), Shading::Lit);
        assert_eq!(
            shading(&session, locked),
            Shading::Unlit,
            "a locked layer is left alone"
        );
        assert_eq!(session.history().undo_depth(), steps + 1, "one undo step");

        // A second light changes nothing else and says nothing.
        let (_, result) = add_light(&mut session);
        assert!(result.get("message").is_none(), "{result}");

        run(&mut session, json!({"cmd": "undo"}));
        run(&mut session, json!({"cmd": "undo"}));
        assert!(session.doc().layer(id).is_none());
        assert_eq!(shading(&session, paint), Shading::Unlit);
        assert_eq!(shading(&session, second), Shading::Unlit);

        // When some paint already takes light, the rest is left as the user set it.
        run(
            &mut session,
            json!({"cmd": "set_layer_kind", "layer": paint, "values": {"shading": "lit"}}),
        );
        let (_, result) = add_light(&mut session);
        assert!(result.get("message").is_none());
        assert_eq!(shading(&session, second), Shading::Unlit);
    }

    #[test]
    fn a_new_paint_layer_takes_light_once_the_document_is_lit() {
        let mut session = Session::new(Document::new());
        let unlit = LayerId(
            run(&mut session, json!({"cmd": "add_layer", "kind": "paint"}))["layer"]
                .as_u64()
                .unwrap(),
        );
        assert_eq!(
            shading(&session, unlit),
            Shading::Unlit,
            "no light, no shading"
        );
        add_light(&mut session);
        let lit = LayerId(
            run(&mut session, json!({"cmd": "add_layer", "kind": "paint"}))["layer"]
                .as_u64()
                .unwrap(),
        );
        assert_eq!(shading(&session, lit), Shading::Lit);
    }

    #[test]
    fn painting_into_a_group_of_a_lit_document_makes_a_lit_layer() {
        let mut session = Session::new(Document::new());
        add_light(&mut session);
        run(&mut session, json!({"cmd": "add_layer", "kind": "group"}));
        session.paint_target().expect("a group can be painted in");
        let made = session.paint_layer().expect("painting made a paint layer");
        assert_eq!(shading(&session, made), Shading::Lit);
    }

    #[test]
    fn set_shading_changes_listed_layers_and_groups_as_one_step() {
        let mut session = Session::new(Document::new());
        let first = session.paint_layer().unwrap();
        let group = LayerId(
            run(&mut session, json!({"cmd": "add_layer", "kind": "group"}))["layer"]
                .as_u64()
                .unwrap(),
        );
        let inside = LayerId(
            run(
                &mut session,
                json!({"cmd": "add_layer", "kind": "paint", "parent": group}),
            )["layer"]
                .as_u64()
                .unwrap(),
        );
        let steps = session.history().undo_depth();
        let result = run(
            &mut session,
            json!({"cmd": "set_shading", "layers": [group], "shading": "lit"}),
        );
        assert_eq!(result["changed"], 1);
        assert_eq!(shading(&session, inside), Shading::Lit);
        assert_eq!(
            shading(&session, first),
            Shading::Unlit,
            "outside the group"
        );
        let info = session.document_info();
        assert_eq!(
            (
                info["paint_layers"].as_u64(),
                info["lit_paint_layers"].as_u64()
            ),
            (Some(2), Some(1))
        );

        let result = run(
            &mut session,
            json!({"cmd": "set_shading", "shading": "lit"}),
        );
        assert_eq!(
            result["changed"], 1,
            "with no layers listed, every paint layer"
        );
        assert_eq!(shading(&session, first), Shading::Lit);
        assert_eq!(session.history().undo_depth(), steps + 2);
        assert_eq!(session.history().undo_label(), Some("Change shading"));

        run(&mut session, json!({"cmd": "undo"}));
        assert_eq!(shading(&session, first), Shading::Unlit);
        assert_eq!(shading(&session, inside), Shading::Lit);

        // A locked group keeps its layers as they are; a missing layer is an error.
        run(
            &mut session,
            json!({"cmd": "set_layer", "layer": group, "locked": true}),
        );
        let result = run(
            &mut session,
            json!({"cmd": "set_shading", "shading": "unlit"}),
        );
        assert_eq!(result["changed"], 0);
        assert_eq!(shading(&session, inside), Shading::Lit);
        assert!(session
            .exec_json(&json!({"cmd": "set_shading", "layers": [999], "shading": "lit"}))
            .is_err());
    }

    #[test]
    fn looking_along_points_minus_z_and_keeps_up_up() {
        for direction in [
            DVec3::NEG_Z,
            DVec3::new(1.0, -1.0, 0.0),
            DVec3::new(0.2, 0.1, 0.9),
            DVec3::NEG_Y,
        ] {
            let turn = looking_along(direction).unwrap();
            assert!(
                close(turn * DVec3::NEG_Z, direction.normalize()),
                "{direction:?}"
            );
            assert!(
                (turn * DVec3::X).y.abs() < 1e-9,
                "no roll for {direction:?}"
            );
        }
        assert!(looking_along(DVec3::ZERO).is_none());
    }
}
