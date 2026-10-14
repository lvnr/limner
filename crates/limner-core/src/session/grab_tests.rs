//! Grabbing with the grips, and the gimbal, driven the way controllers drive them.

use super::tools::tests::{drag, strokes};
use super::tools::{BrushStyle, ToolKind};
use super::*;
use crate::math::Xform;
use crate::time::from_seconds;
use glam::DQuat;
use serde_json::json;
use std::f64::consts::FRAC_PI_2;

const REST_MAIN: DVec3 = DVec3::new(0.2, 1.0, -0.3);
const REST_OFF: DVec3 = DVec3::new(-0.2, 1.0, -0.3);

fn at(position: DVec3) -> HandInput {
    HandInput {
        tracked: true,
        position,
        ..HandInput::default()
    }
}

fn gripping(position: DVec3) -> HandInput {
    HandInput {
        grip: 1.0,
        ..at(position)
    }
}

fn both(main: HandInput, off: HandInput) -> InputFrame {
    InputFrame {
        main,
        off,
        ..InputFrame::default()
    }
}

/// One frame with only the drawing hand doing anything.
fn main_only(main: HandInput) -> InputFrame {
    both(main, at(REST_OFF))
}

/// The modifier held all the way down.
fn alt() -> HandInput {
    HandInput {
        trigger: 1.0,
        ..at(REST_OFF)
    }
}

fn run(session: &mut Session, command: Value) -> Value {
    session
        .exec_json(&command)
        .unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

fn new_layer(session: &mut Session, command: Value) -> LayerId {
    LayerId(run(session, command)["layer"].as_u64().expect("a layer id"))
}

fn session() -> (Session, LayerId) {
    let mut session = Session::new(Document::new());
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.tools.paint.size = 0.005;
    let layer = session.paint_layer().unwrap();
    (session, layer)
}

fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-9
}

/// Where a layer's origin is in viewer space.
fn seen_at(session: &Session, layer: LayerId) -> DVec3 {
    let frame = session.evaluate();
    session
        .view
        .viewer_from_world()
        .mul(&frame.world_xform(session.doc(), layer))
        .translation
}

fn local(session: &Session, layer: LayerId) -> Xform {
    session.evaluate().get(layer).unwrap().local
}

/// Grips with the drawing hand at `from`, carries to `to`, and lets go.
fn carry(session: &mut Session, from: DVec3, to: DVec3, off: HandInput) -> Vec<Feedback> {
    let mut feedback = session.update(&both(at(from), off));
    feedback.extend(session.update(&both(gripping(from), off)));
    for i in 1..=4 {
        let position = from.lerp(to, i as f64 / 4.0);
        feedback.extend(session.update(&both(gripping(position), off)));
    }
    feedback.extend(session.update(&both(at(to), off)));
    feedback
}

#[test]
fn a_grip_carries_the_world_and_a_second_grip_scales_it() {
    let (mut session, _) = session();
    let took = DVec3::new(0.0, 1.0, -0.5);
    session.update(&main_only(at(took)));
    session.update(&main_only(gripping(took)));
    assert_eq!(session.grabbed(), Some(Subject::View));
    assert!(session.is_grabbing(0) && !session.is_grabbing(1));

    // The world point in the hand stays in the hand, wherever the hand goes and turns.
    let turned = HandInput {
        rotation: DQuat::from_rotation_y(FRAC_PI_2),
        ..gripping(DVec3::new(0.1, 1.2, -0.4))
    };
    session.update(&main_only(turned));
    let seen = session.view.viewer_from_world();
    assert!(close(seen.transform_point(took), turned.position));
    // What was to the right of the hand has swung round to in front of it.
    assert!(close(
        seen.transform_point(took + DVec3::X * 0.2),
        turned.position + DVec3::NEG_Z * 0.2
    ));
    assert!((session.view.scale() - 1.0).abs() < 1e-12);

    // The other hand takes hold too; pulling the hands apart makes the world bigger.
    let other = turned.position + DVec3::new(-0.2, 0.0, 0.0);
    session.update(&both(turned, gripping(other)));
    let before = session.view;
    let middle = (turned.position + other) * 0.5;
    let held_world = before.world_from_viewer.transform_point(middle);
    session.update(&both(
        HandInput {
            position: turned.position + DVec3::X * 0.1,
            ..turned
        },
        gripping(other - DVec3::X * 0.1),
    ));
    assert!((session.view.scale() - 0.5).abs() < 1e-9, "twice as big");
    assert!(close(
        session.view.viewer_from_world().transform_point(held_world),
        middle
    ));

    // Letting go with one hand leaves the other holding on, without a jump.
    let scaled = session.view;
    session.update(&both(
        HandInput {
            position: turned.position + DVec3::X * 0.1,
            ..turned
        },
        at(other - DVec3::X * 0.1),
    ));
    assert_eq!(session.view, scaled);
    assert!(session.is_grabbing(0) && !session.is_grabbing(1));
    session.update(&both(at(turned.position), at(other)));
    assert_eq!(session.grabbed(), None);
    assert_eq!(
        session.history().undo_depth(),
        0,
        "moving about is not an edit"
    );
}

#[test]
fn with_a_level_horizon_the_world_turns_only_about_the_vertical() {
    let (mut session, _) = session();
    // Turning the option on straightens a view that was tilted.
    session
        .view
        .rotate_about(DVec3::ZERO, DQuat::from_rotation_x(0.3));
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"level_horizon": true}}),
    );
    let up = |session: &Session| session.view.viewer_from_world().rotation * DVec3::Y;
    assert!(close(up(&session), DVec3::Y));

    let took = DVec3::new(0.0, 1.0, -0.5);
    let held = session.view.world_from_viewer.transform_point(took);
    session.update(&main_only(gripping(took)));
    let tilted = HandInput {
        rotation: DQuat::from_rotation_y(0.4) * DQuat::from_rotation_x(0.7),
        ..gripping(took + DVec3::new(0.1, 0.0, 0.0))
    };
    session.update(&main_only(tilted));
    assert!(close(up(&session), DVec3::Y), "the horizon stays level");
    let yaw = session.view.viewer_from_world().rotation;
    assert!(
        yaw.angle_between(DQuat::IDENTITY) > 0.2,
        "but the world does turn"
    );
    assert!(close(
        session.view.viewer_from_world().transform_point(held),
        tilted.position
    ));
}

#[test]
fn the_stick_zooms_about_the_hand_while_a_grip_is_held() {
    let (mut session, _) = session();
    let hand = DVec3::new(0.1, 1.1, -0.4);
    let size = session.tools.size();
    session.update(&main_only(gripping(hand)));
    for _ in 0..45 {
        session.update(&InputFrame {
            dt: 0.01,
            ..main_only(HandInput {
                stick: [0.0, 1.0],
                ..gripping(hand)
            })
        });
    }
    // 0.45 seconds of full push: bigger by a factor of e.
    assert!((session.view.scale() - (-1.0f64).exp()).abs() < 1e-6);
    assert!(close(
        session.view.viewer_from_world().transform_point(hand),
        hand
    ));
    assert_eq!(
        session.tools.size(),
        size,
        "the stick is not sizing the brush meanwhile"
    );
}

#[test]
fn a_chosen_layer_follows_the_hand_through_any_parents() {
    let (mut session, _) = session();
    let group = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "group"}));
    let layer = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "parent": group}),
    );
    let placed = Xform::new(
        DQuat::from_rotation_y(0.7),
        2.5,
        DVec3::new(1.0, 0.2, -3.0),
        crate::math::Flip::X,
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": group, "transform": placed}),
    );
    session.view.world_from_viewer = Xform::new(
        DQuat::from_rotation_y(-0.3),
        4.0,
        DVec3::new(0.5, 0.0, -2.0),
        crate::math::Flip::None,
    );
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": layer}),
    );
    let depth = session.history().undo_depth();
    let (before, view) = (seen_at(&session, layer), session.view);

    let by = DVec3::new(0.15, -0.05, 0.1);
    let feedback = carry(
        &mut session,
        DVec3::new(0.0, 1.0, -0.5),
        DVec3::new(0.0, 1.0, -0.5) + by,
        at(REST_OFF),
    );
    assert!(feedback
        .iter()
        .any(|f| matches!(f, Feedback::Haptic { main: true, .. })));
    assert!((seen_at(&session, layer) - (before + by)).length() < 1e-9);
    assert_eq!(session.view, view, "the view stays where it is");
    assert_eq!(local(&session, group), placed);
    assert_eq!(local(&session, layer).flip, crate::math::Flip::None);
    assert_eq!(session.history().undo_depth(), depth + 1);
    assert_eq!(session.history().undo_label(), Some("Move layer"));
    session.undo();
    assert!((seen_at(&session, layer) - before).length() < 1e-9);

    // A locked layer does not budge, and neither does the view in its place.
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "locked": true}),
    );
    let feedback = carry(&mut session, DVec3::ZERO, by, at(REST_OFF));
    assert!(feedback
        .iter()
        .any(|f| matches!(f, Feedback::Message(m) if m.contains("locked"))));
    assert!((seen_at(&session, layer) - before).length() < 1e-9);
    assert_eq!(session.view, view);
    assert!(!session.in_transaction());
}

#[test]
fn without_translation_a_grab_turns_the_layer_about_its_pivot() {
    let (mut session, layer) = session();
    let pivot = Xform::from_translation(DVec3::new(0.3, 1.0, -0.5));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "pivot": pivot}),
    );
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": layer}),
    );
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"allow_translate": false, "allow_scale": false}}),
    );

    let took = DVec3::new(0.0, 1.3, -0.2);
    session.update(&main_only(gripping(took)));
    session.update(&main_only(HandInput {
        rotation: DQuat::from_rotation_y(FRAC_PI_2),
        stick: [0.0, 1.0],
        ..gripping(took + DVec3::new(0.2, 0.1, 0.0))
    }));
    session.update(&main_only(at(took)));
    let moved = local(&session, layer);
    assert!(
        close(moved.transform_point(pivot.translation), pivot.translation),
        "the pivot stays put"
    );
    assert!(
        moved
            .rotation
            .angle_between(DQuat::from_rotation_y(FRAC_PI_2))
            < 1e-9
    );
    assert_eq!(moved.scale, 1.0);

    // With only translation allowed the layer slides with the hand and does not turn.
    run(
        &mut session,
        json!({"cmd": "reset_transform", "layer": layer}),
    );
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"allow_translate": true, "allow_rotate": false}}),
    );
    session.update(&main_only(gripping(took)));
    session.update(&main_only(HandInput {
        rotation: DQuat::from_rotation_y(FRAC_PI_2),
        ..gripping(took + DVec3::new(0.2, 0.1, 0.0))
    }));
    session.update(&main_only(at(took)));
    let moved = local(&session, layer);
    assert!(close(moved.translation, DVec3::new(0.2, 0.1, 0.0)));
    assert_eq!(moved.rotation, DQuat::IDENTITY);
}

#[test]
fn holding_the_modifier_moves_a_copy_and_redo_repeats_the_move() {
    let (mut session, layer) = session();
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": layer}),
    );
    let count = session.doc().layer_count();
    assert_eq!(session.status()["can_transform_again"], false);

    let step = DVec3::new(0.1, 0.0, 0.0);
    carry(
        &mut session,
        DVec3::new(0.0, 1.0, -0.5),
        DVec3::new(0.0, 1.0, -0.5) + step,
        alt(),
    );
    assert_eq!(session.doc().layer_count(), count + 1);
    assert_eq!(session.history().undo_label(), Some("Duplicate layer"));
    let copy = session.transform_target().unwrap();
    assert_ne!(copy, layer, "the copy is now what moves");
    assert_eq!(session.current_layer(), Some(copy));
    assert!(
        close(local(&session, layer).translation, DVec3::ZERO),
        "the original stays"
    );
    assert!(close(local(&session, copy).translation, step));
    assert_eq!(session.doc().layer(copy).unwrap().name, "Paint copy");

    // Redo with nothing to redo makes the next copy, moved as far again.
    assert_eq!(session.status()["can_redo"], true);
    assert_eq!(session.status()["redo_label"], "Transform again");
    assert_eq!(session.redo().as_deref(), Some("Transform again"));
    let third = session.transform_target().unwrap();
    assert!(close(local(&session, third).translation, step * 2.0));
    run(&mut session, json!({"cmd": "transform_again"}));
    let fourth = session.transform_target().unwrap();
    assert!(close(local(&session, fourth).translation, step * 3.0));
    assert_eq!(session.doc().layer_count(), count + 3);

    // Each copy is its own undo step, and after anything else there is nothing to repeat.
    session.undo();
    assert_eq!(session.doc().layer_count(), count + 2);
    assert_eq!(session.status()["can_transform_again"], false);
    assert_eq!(
        session.redo().as_deref(),
        Some("Transform again"),
        "the step undone comes back"
    );
    assert_eq!(session.redo(), None);
    assert!(session
        .exec_json(&json!({"cmd": "transform_again"}))
        .is_err());
}

/// Two ribbons, at heights 1.0 and 1.2, the lower one selected.
fn one_of_two_selected() -> (Session, LayerId) {
    let (mut session, layer) = session();
    for y in [1.0, 1.2] {
        drag(
            &mut session,
            DVec3::new(0.0, y, -0.5),
            DVec3::new(0.2, y, -0.5),
            20,
        );
    }
    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.03;
    drag(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.1, 1.0, -0.5),
        1,
    );
    assert_eq!(session.selection().count(), 1);
    (session, layer)
}

fn starts(session: &Session, layer: LayerId) -> Vec<DVec3> {
    strokes(session, layer)
        .iter()
        .map(|s| s.points()[0].position().as_dvec3())
        .collect()
}

#[test]
fn the_selection_is_carried_when_the_selection_tool_reaches_it() {
    let (mut session, layer) = one_of_two_selected();
    let view = session.view;
    let width = strokes(&session, layer)[0].points()[10].width;
    let pivot = session.selection_pivot().unwrap();
    assert!(
        (pivot - DVec3::new(0.1, 1.0, -0.5)).length() < 1e-6,
        "the middle of what is selected"
    );

    let by = DVec3::new(0.0, 0.05, 0.1);
    carry(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.1, 1.0, -0.5) + by,
        at(REST_OFF),
    );
    let moved = starts(&session, layer);
    assert!((moved[0] - (DVec3::new(0.0, 1.0, -0.5) + by)).length() < 1e-6);
    assert!(
        (moved[1] - DVec3::new(0.0, 1.2, -0.5)).length() < 1e-6,
        "the other stroke stays"
    );
    assert_eq!(session.view, view);
    assert_eq!(session.history().undo_label(), Some("Move strokes"));
    assert_eq!(session.selection().count(), 1, "still selected");
    assert!((session.selection_pivot().unwrap() - (pivot + by)).length() < 1e-6);

    // Both hands, pulled apart to twice the distance, make it twice the size.
    let (a, b) = (DVec3::new(0.05, 1.05, -0.4), DVec3::new(0.15, 1.05, -0.4));
    session.update(&both(gripping(a), at(b)));
    session.update(&both(gripping(a), gripping(b)));
    session.update(&both(
        gripping(a - DVec3::X * 0.05),
        gripping(b + DVec3::X * 0.05),
    ));
    session.update(&both(at(a), at(b)));
    let grown = strokes(&session, layer);
    assert!((grown[0].points()[10].width - width * 2.0).abs() < 1e-7);
    assert!((grown[0].arc_length() - 0.4).abs() < 1e-5);
    assert!((grown[1].arc_length() - 0.2).abs() < 1e-5);

    // Away from the selection a grip takes the world as usual; so does any other tool.
    carry(
        &mut session,
        DVec3::new(0.0, 2.0, 0.5),
        DVec3::new(0.1, 2.0, 0.5),
        at(REST_OFF),
    );
    assert_ne!(session.view, view);
    session.view = view;
    session.tools.select_tool(ToolKind::Paint);
    let depth = session.history().undo_depth();
    carry(
        &mut session,
        DVec3::new(0.1, 1.05, -0.4),
        DVec3::new(0.2, 1.05, -0.4),
        at(REST_OFF),
    );
    assert_ne!(session.view, view);
    assert_eq!(session.history().undo_depth(), depth);
}

#[test]
fn a_copy_of_the_selection_can_be_repeated_in_place_or_onto_new_frames() {
    let (mut session, layer) = one_of_two_selected();
    let step = DVec3::new(0.0, 0.0, 0.1);
    let took = DVec3::new(0.1, 1.0, -0.5);
    carry(&mut session, took, took + step, alt());
    assert_eq!(session.history().undo_label(), Some("Duplicate strokes"));
    let now = starts(&session, layer);
    assert_eq!(now.len(), 3);
    assert!(
        (now[0] - DVec3::new(0.0, 1.0, -0.5)).length() < 1e-6,
        "the original stays"
    );
    assert!((now[2] - (DVec3::new(0.0, 1.0, -0.5) + step)).length() < 1e-6);
    assert_eq!(
        session.selection().count(),
        1,
        "the copy is what is selected now"
    );

    assert_eq!(session.redo().as_deref(), Some("Transform again"));
    let now = starts(&session, layer);
    assert_eq!(now.len(), 4);
    assert!((now[3] - (DVec3::new(0.0, 1.0, -0.5) + step * 2.0)).length() < 1e-6);

    // With the option on, each repeat lands on a frame of its own.
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"animate_duplicates": true}}),
    );
    run(&mut session, json!({"cmd": "transform_again"}));
    let paint = session
        .doc()
        .layer(layer)
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    assert_eq!(paint.frames.len(), 2);
    assert_eq!(session.status()["frame"], 1);
    let added = session
        .doc()
        .drawing(paint.drawings[paint.frames[1] as usize])
        .unwrap();
    assert_eq!(added.stroke_count(), 1);
    let first = added.strokes().next().unwrap().points()[0]
        .position()
        .as_dvec3();
    assert!((first - (DVec3::new(0.0, 1.0, -0.5) + step * 3.0)).length() < 1e-6);
    assert_eq!(
        strokes(&session, layer).len(),
        4,
        "the first frame is as it was"
    );
    assert_eq!(session.selection().count(), 1);
    run(&mut session, json!({"cmd": "transform_again"}));
    assert_eq!(
        session
            .doc()
            .layer(layer)
            .unwrap()
            .as_paint()
            .unwrap()
            .frames
            .len(),
        3
    );
}

#[test]
fn grabbing_a_stroke_moves_its_layer_or_its_group_when_asked_to() {
    let (mut session, first) = session();
    drag(
        &mut session,
        DVec3::new(0.0, 1.0, -0.5),
        DVec3::new(0.2, 1.0, -0.5),
        20,
    );
    let group = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "group"}));
    let second = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "parent": group}),
    );
    drag(
        &mut session,
        DVec3::new(0.0, 1.3, -0.5),
        DVec3::new(0.2, 1.3, -0.5),
        20,
    );
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"auto_select": "layer"}}),
    );
    let view = session.view;
    let by = DVec3::new(0.0, 0.0, 0.1);

    carry(
        &mut session,
        DVec3::new(0.1, 1.31, -0.5),
        DVec3::new(0.1, 1.31, -0.5) + by,
        at(REST_OFF),
    );
    assert!(close(local(&session, second).translation, by));
    assert!(local(&session, first).is_identity() && local(&session, group).is_identity());
    assert_eq!(session.view, view);

    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"auto_select": "group"}}),
    );
    carry(
        &mut session,
        DVec3::new(0.1, 1.31, -0.4),
        DVec3::new(0.1, 1.31, -0.4) + by,
        at(REST_OFF),
    );
    assert!(
        close(local(&session, group).translation, by),
        "the group it is in moves"
    );
    assert!(close(local(&session, second).translation, by));
    // A layer straight under the root has no group to move, so it moves itself.
    carry(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.1, 1.0, -0.5) + by,
        at(REST_OFF),
    );
    assert!(close(local(&session, first).translation, by));

    // In empty space the grip takes the world.
    carry(
        &mut session,
        DVec3::new(0.0, 2.0, 0.3),
        DVec3::new(0.0, 2.0, 0.3) + by,
        at(REST_OFF),
    );
    assert_ne!(session.view, view);
}

#[test]
fn recording_while_playing_writes_keys_as_the_layer_is_carried() {
    let (mut session, layer) = session();
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": layer}),
    );
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"recording": true}}),
    );
    run(
        &mut session,
        json!({"cmd": "set_document", "range_end": from_seconds(10.0)}),
    );
    run(&mut session, json!({"cmd": "play"}));
    let took = DVec3::new(0.0, 1.0, -0.5);
    session.update(&main_only(gripping(took)));
    for i in 1..=24 {
        session.advance(1.0 / 48.0);
        session.update(&main_only(gripping(took + DVec3::X * (i as f64 * 0.01))));
    }
    session.update(&main_only(at(took)));
    run(&mut session, json!({"cmd": "pause"}));

    let keys = session
        .doc()
        .layer(layer)
        .unwrap()
        .anim
        .transform
        .keys()
        .to_vec();
    assert!(
        (12..=14).contains(&keys.len()),
        "a key on each frame of half a second: {}",
        keys.len()
    );
    assert!(keys
        .windows(2)
        .all(|w| w[1].value.translation.x > w[0].value.translation.x));
    assert_eq!(session.history().undo_label(), Some("Move layer"));
    session.undo();
    assert!(
        session
            .doc()
            .layer(layer)
            .unwrap()
            .anim
            .transform
            .is_empty(),
        "the take is one step"
    );
}

/// A session whose paint layer is the transform object, in reach, with the gimbal showing.
fn with_gimbal() -> (Session, LayerId, Gimbal) {
    let (mut session, layer) = session();
    let placed = Xform::from_translation(DVec3::new(0.0, 1.2, -0.6));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "transform": placed}),
    );
    assert!(session.gimbal().is_none(), "hidden until asked for");
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"show_gimbal": true}}),
    );
    assert_eq!(
        session.gimbal().map(|g| g.subject),
        Some(Subject::Layer(layer)),
        "with the view as the transform object, the gimbal is the current layer's"
    );
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": layer}),
    );
    session.update(&main_only(at(REST_MAIN)));
    let gimbal = session.gimbal().expect("a gimbal");
    (session, layer, gimbal)
}

#[test]
fn the_gimbal_sits_at_the_pivot_and_knows_which_handle_a_hand_is_at() {
    let (mut session, layer, gimbal) = with_gimbal();
    assert_eq!(gimbal.subject, Subject::Layer(layer));
    assert!(close(gimbal.position, DVec3::new(0.0, 1.2, -0.6)));
    let distance = gimbal.position.distance(InputFrame::default().head);
    assert!(
        (gimbal.size - 0.2 * distance).abs() < 1e-12,
        "the same size to the eye at any distance"
    );
    assert_eq!(gimbal.hovered, [None, None]);

    let s = gimbal.size;
    let o = gimbal.position;
    for (position, expected) in [
        (o, Some(GimbalHandle::Center)),
        (o + DVec3::X * (0.7 * s), Some(GimbalHandle::AxisX)),
        (o + DVec3::Y * (0.5 * s), Some(GimbalHandle::AxisY)),
        (
            o + DVec3::new(0.0, 0.4, 0.4) * s,
            Some(GimbalHandle::PlaneX),
        ),
        (
            o + DVec3::new(0.8, 0.0, 0.8).normalize() * s,
            Some(GimbalHandle::RingY),
        ),
        (
            o + DVec3::new(0.0, 1.0, 1.0).normalize() * s,
            Some(GimbalHandle::RingX),
        ),
        // Between the tip of an arrow and the ring there is room for neither.
        (o + DVec3::X * (0.87 * s), None),
        (o + DVec3::splat(1.5 * s), None),
    ] {
        session.update(&main_only(at(position)));
        assert_eq!(
            session.gimbal().unwrap().hovered[0],
            expected,
            "at {position:?}"
        );
        assert_eq!(session.cursor().at_handle, expected.is_some());
    }
    // The other hand can be at a handle too.
    session.update(&both(at(REST_MAIN), at(o + DVec3::Z * (0.6 * s))));
    assert_eq!(
        session.gimbal().unwrap().hovered,
        [None, Some(GimbalHandle::AxisZ)]
    );

    // The gimbal follows the pivot, turned as the layer is.
    let pivot = Xform::from_translation(DVec3::new(0.1, 0.0, 0.0));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "pivot": pivot}),
    );
    let turned = Xform::new(
        DQuat::from_rotation_y(FRAC_PI_2),
        1.0,
        DVec3::new(0.0, 1.2, -0.6),
        crate::math::Flip::None,
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "transform": turned}),
    );
    let gimbal = session.gimbal().unwrap();
    assert!(close(gimbal.position, DVec3::new(0.0, 1.2, -0.7)));
    assert!(
        gimbal
            .rotation
            .angle_between(DQuat::from_rotation_y(FRAC_PI_2))
            < 1e-9
    );
}

/// Close enough for points stored in single precision.
fn near(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-6
}

/// Strokes from (0.1, 1.0, -0.5) to (0.3, 1.2, -0.5) in `layer`: content whose middle
/// is (0.2, 1.1, -0.5).
fn some_content(session: &mut Session, layer: LayerId) {
    run(
        session,
        json!({"cmd": "add_strokes", "layer": layer, "strokes": [
            {"points": [[0.1, 1.0, -0.5], [0.3, 1.2, -0.5]], "width": 0.01},
        ]}),
    );
}

#[test]
fn a_layer_whose_pivot_was_never_set_has_its_gimbal_at_the_middle_of_its_content() {
    let (mut session, layer) = session();
    some_content(&mut session, layer);
    let middle = DVec3::new(0.2, 1.1, -0.5);
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"show_gimbal": true}}),
    );
    let gimbal = session.gimbal().expect("the current layer's gimbal");
    assert_eq!(gimbal.subject, Subject::Layer(layer));
    assert!(near(gimbal.position, middle), "{:?}", gimbal.position);
    assert!(session.doc().layer(layer).unwrap().pivot.is_identity());

    // A grip on a ring turns the layer about the middle of what is drawn, and from
    // then on the pivot is there, for the layer's animation too.
    let steps = session.history().undo_depth();
    session.update(&main_only(at(on_ring(&gimbal, 0.0))));
    session.update(&main_only(gripping(on_ring(&gimbal, 0.0))));
    for i in 1..=9 {
        session.update(&main_only(gripping(on_ring(
            &gimbal,
            i as f64 * 0.1 * FRAC_PI_2 / 0.9,
        ))));
    }
    session.update(&main_only(at(REST_MAIN)));
    assert_eq!(session.history().undo_depth(), steps + 1, "one undo step");
    let turned = local(&session, layer);
    assert!(
        turned
            .rotation
            .angle_between(DQuat::from_rotation_y(FRAC_PI_2))
            < 1e-9
    );
    assert!(
        near(turned.transform_point(middle), middle),
        "the middle stays where it was"
    );
    let pivot = session.doc().layer(layer).unwrap().pivot;
    assert!(near(pivot.translation, middle), "{pivot:?}");
    assert!(near(session.gimbal().unwrap().position, middle));

    // More paint does not move a pivot that has been set.
    run(
        &mut session,
        json!({"cmd": "add_strokes", "layer": layer, "strokes": [
            {"points": [[2.0, 2.0, 2.0], [2.1, 2.0, 2.0]], "width": 0.01},
        ]}),
    );
    assert!(near(
        session.doc().layer(layer).unwrap().pivot.translation,
        middle
    ));

    // Undo takes the turn and the pivot back together.
    run(&mut session, json!({"cmd": "undo"}));
    run(&mut session, json!({"cmd": "undo"}));
    assert!(session.doc().layer(layer).unwrap().pivot.is_identity());
    assert!(local(&session, layer).is_identity());

    // A pivot moved by the user is where the gimbal goes, even off the content.
    let elsewhere = Xform::from_translation(DVec3::new(0.0, 0.5, 0.0));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "pivot": elsewhere}),
    );
    assert!(near(
        session.gimbal().unwrap().position,
        DVec3::new(0.0, 0.5, 0.0)
    ));

    // A layer with nothing in it keeps its gimbal at its origin.
    let empty = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "paint"}));
    assert_eq!(session.gimbal().unwrap().subject, Subject::Layer(empty));
    assert!(near(session.gimbal().unwrap().position, DVec3::ZERO));
}

#[test]
fn the_stick_click_shows_the_gimbal_of_the_current_layer_or_says_why_there_is_none() {
    let (mut session, layer) = session();
    some_content(&mut session, layer);
    let click = |session: &mut Session| {
        let pressed = HandInput {
            stick_click: true,
            ..at(REST_MAIN)
        };
        let mut feedback = session.update(&main_only(pressed));
        feedback.extend(session.update(&main_only(at(REST_MAIN))));
        feedback
    };
    let said = |feedback: &[Feedback]| {
        feedback.iter().find_map(|f| match f {
            Feedback::Message(m) => Some(m.clone()),
            _ => None,
        })
    };
    assert_eq!(said(&click(&mut session)), None);
    assert!(session.options.show_gimbal);
    assert_eq!(
        session.gimbal().map(|g| g.subject),
        Some(Subject::Layer(layer))
    );
    assert!(near(
        session.gimbal().unwrap().position,
        DVec3::new(0.2, 1.1, -0.5)
    ));
    assert_eq!(said(&click(&mut session)), None, "hiding it says nothing");
    assert!(!session.options.show_gimbal && session.gimbal().is_none());

    // The root layer and a locked layer cannot be moved: the gimbal has nothing to show.
    let root = session.doc().root();
    run(
        &mut session,
        json!({"cmd": "select_layers", "layers": [root]}),
    );
    assert_eq!(
        said(&click(&mut session)).as_deref(),
        Some(super::grab::NOTHING_TO_TRANSFORM)
    );
    assert!(session.options.show_gimbal, "it is on all the same");
    assert!(session.gimbal().is_none());
    click(&mut session);
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "locked": true}),
    );
    run(
        &mut session,
        json!({"cmd": "select_layers", "layers": [layer]}),
    );
    assert!(said(&click(&mut session)).is_some());
    assert!(session.gimbal().is_none());

    // Selected strokes with the Selection tool in hand, or a transform object, are
    // something to move.
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": layer}),
    );
    assert_eq!(
        session.gimbal().map(|g| g.subject),
        Some(Subject::Layer(layer)),
        "a layer chosen as the transform object has its gimbal even when locked"
    );
}

#[test]
fn gripping_an_arrow_moves_along_it_and_a_plane_handle_in_its_plane() {
    let (mut session, layer, gimbal) = with_gimbal();
    let view = session.view;
    let on_arrow = gimbal.position + DVec3::X * (0.6 * gimbal.size);
    let by = DVec3::new(0.1, 0.05, -0.02);
    carry(&mut session, on_arrow, on_arrow + by, at(REST_OFF));
    assert!(
        close(
            local(&session, layer).translation,
            DVec3::new(0.1, 1.2, -0.6)
        ),
        "only along X"
    );
    assert_eq!(session.view, view);
    assert_eq!(session.history().undo_label(), Some("Move layer"));

    let gimbal = session.gimbal().unwrap();
    let on_plane = gimbal.position + DVec3::new(0.4, 0.0, 0.4) * gimbal.size;
    session.update(&main_only(at(on_plane)));
    assert_eq!(
        session.gimbal().unwrap().hovered[0],
        Some(GimbalHandle::PlaneY)
    );
    session.update(&main_only(gripping(on_plane)));
    assert_eq!(session.gimbal().unwrap().active, Some(GimbalHandle::PlaneY));
    session.update(&main_only(gripping(on_plane + by)));
    session.update(&main_only(at(on_plane + by)));
    assert!(
        close(
            local(&session, layer).translation,
            DVec3::new(0.2, 1.2, -0.62)
        ),
        "not up or down"
    );
    assert_eq!(session.gimbal().unwrap().active, None);
}

/// A point on the gimbal's Y ring, `angle` round from the X arrow toward -Z.
fn on_ring(gimbal: &Gimbal, angle: f64) -> DVec3 {
    gimbal.position + DQuat::from_rotation_y(angle) * DVec3::X * gimbal.size
}

#[test]
fn gripping_a_ring_turns_about_the_pivot_and_the_trigger_turns_in_steps() {
    let (mut session, layer, gimbal) = with_gimbal();
    let pivot = Xform::from_translation(DVec3::new(0.0, 0.0, 0.0));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "pivot": pivot}),
    );
    session.update(&main_only(at(on_ring(&gimbal, 0.0))));
    session.update(&main_only(gripping(on_ring(&gimbal, 0.0))));
    for i in 1..=9 {
        session.update(&main_only(gripping(on_ring(
            &gimbal,
            i as f64 * 0.1 * FRAC_PI_2 / 0.9,
        ))));
    }
    session.update(&main_only(at(REST_MAIN)));
    let turned = local(&session, layer);
    assert!(
        turned
            .rotation
            .angle_between(DQuat::from_rotation_y(FRAC_PI_2))
            < 1e-9
    );
    assert!(
        close(turned.translation, DVec3::new(0.0, 1.2, -0.6)),
        "about the pivot"
    );

    // With the trigger the ring turns in steps: twelve to the full turn.
    run(
        &mut session,
        json!({"cmd": "reset_transform", "layer": layer}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "transform": Xform::from_translation(DVec3::new(0.0, 1.2, -0.6))}),
    );
    let pressed = |angle: f64, trigger: f32| {
        main_only(HandInput {
            trigger,
            ..at(on_ring(&gimbal, angle))
        })
    };
    let strokes_before = strokes(&session, layer).len();
    session.update(&pressed(0.0, 0.0));
    session.update(&pressed(0.0, 1.0));
    session.update(&pressed(40f64.to_radians(), 1.0));
    assert!(
        local(&session, layer)
            .rotation
            .angle_between(DQuat::from_rotation_y(30f64.to_radians()))
            < 1e-9
    );
    session.update(&pressed(50f64.to_radians(), 1.0));
    assert!(
        local(&session, layer)
            .rotation
            .angle_between(DQuat::from_rotation_y(60f64.to_radians()))
            < 1e-9
    );
    session.update(&pressed(50f64.to_radians(), 0.0));
    assert_eq!(session.history().undo_label(), Some("Turn layer"));
    assert_eq!(
        strokes(&session, layer).len(),
        strokes_before,
        "the trigger did not paint"
    );
}

#[test]
fn the_trigger_on_an_arrow_moves_the_pivot_and_leaves_the_layer_alone() {
    let (mut session, layer, gimbal) = with_gimbal();
    let on_arrow = gimbal.position + DVec3::Y * (0.6 * gimbal.size);
    let before = local(&session, layer);
    let trigger = |position: DVec3, trigger: f32| {
        main_only(HandInput {
            trigger,
            ..at(position)
        })
    };
    session.update(&trigger(on_arrow, 0.0));
    session.update(&trigger(on_arrow, 1.0));
    session.update(&trigger(on_arrow + DVec3::new(0.03, 0.1, 0.0), 1.0));
    session.update(&trigger(on_arrow + DVec3::new(0.03, 0.1, 0.0), 0.0));
    assert_eq!(local(&session, layer), before);
    assert!(close(
        session.doc().layer(layer).unwrap().pivot.translation,
        DVec3::new(0.0, 0.1, 0.0)
    ));
    assert!(close(
        session.gimbal().unwrap().position,
        DVec3::new(0.0, 1.3, -0.6)
    ));
    assert_eq!(session.history().undo_label(), Some("Move pivot"));
    assert!(strokes(&session, layer).is_empty());
}

#[test]
fn a_pointer_from_a_screen_picks_handles_along_its_line_of_sight() {
    let (mut session, layer, gimbal) = with_gimbal();
    let eye = InputFrame::default().head;
    // The hand itself is nowhere near: it rides a plane in front of the camera.
    let pointing = |target: DVec3, grip: f32| {
        main_only(HandInput {
            grip,
            ray: Some((eye, (target - eye).normalize())),
            ..at(eye + (target - eye).normalize() * 0.2)
        })
    };
    let on_arrow = gimbal.position + DVec3::X * (0.6 * gimbal.size);
    session.update(&pointing(on_arrow, 0.0));
    assert_eq!(
        session.gimbal().unwrap().hovered[0],
        Some(GimbalHandle::AxisX)
    );
    session.update(&pointing(
        gimbal.position + DVec3::new(1.4, 1.4, 0.0) * gimbal.size,
        0.0,
    ));
    assert_eq!(session.gimbal().unwrap().hovered[0], None);

    session.update(&pointing(on_arrow, 0.0));
    session.update(&pointing(on_arrow, 1.0));
    session.update(&pointing(on_arrow + DVec3::new(0.25, 0.1, 0.0), 1.0));
    session.update(&pointing(on_arrow + DVec3::new(0.25, 0.1, 0.0), 0.0));
    let moved = local(&session, layer).translation;
    assert!(
        (moved.y - 1.2).abs() < 1e-9 && (moved.z + 0.6).abs() < 1e-9,
        "only along X"
    );
    assert!(
        moved.x > 0.2 && moved.x < 0.3,
        "to where the pointer is over the arrow's line: {moved:?}"
    );

    // The middle slides the layer across the view.
    let gimbal = session.gimbal().unwrap();
    session.update(&pointing(gimbal.position, 0.0));
    assert_eq!(
        session.gimbal().unwrap().hovered[0],
        Some(GimbalHandle::Center)
    );
    session.update(&pointing(gimbal.position, 1.0));
    session.update(&pointing(gimbal.position + DVec3::Y * 0.1, 1.0));
    session.update(&pointing(gimbal.position + DVec3::Y * 0.1, 0.0));
    let slid = local(&session, layer).translation - moved;
    assert!(slid.length() > 0.08 && slid.length() < 0.12, "{slid:?}");
    let facing = (eye - gimbal.position).normalize();
    assert!(slid.dot(facing).abs() < 1e-9, "it stays at the same depth");
}

#[test]
fn the_selection_has_a_gimbal_square_to_the_world_and_a_pivot_that_can_be_moved() {
    let (mut session, layer) = one_of_two_selected();
    session
        .view
        .rotate_about(DVec3::new(0.1, 1.0, -0.5), DQuat::from_rotation_y(0.5));
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"show_gimbal": true}}),
    );
    session.update(&main_only(at(REST_MAIN)));
    let gimbal = session.gimbal().expect("a gimbal for the selection");
    assert_eq!(gimbal.subject, Subject::Selection);
    assert!((gimbal.position - DVec3::new(0.1, 1.0, -0.5)).length() < 1e-6);
    assert!(
        gimbal
            .rotation
            .angle_between(session.view.viewer_from_world().rotation)
            < 1e-9
    );

    // The trigger on the middle moves the pivot; nothing is selected or deselected by it.
    let trigger = |position: DVec3, trigger: f32| {
        main_only(HandInput {
            trigger,
            ..at(position)
        })
    };
    let lifted = gimbal.position + DVec3::Y * 0.2;
    session.update(&trigger(gimbal.position, 0.0));
    session.update(&trigger(gimbal.position, 1.0));
    session.update(&trigger(lifted, 1.0));
    session.update(&trigger(lifted, 0.0));
    assert_eq!(session.selection().count(), 1);
    let gimbal = session.gimbal().unwrap();
    assert!((gimbal.position - lifted).length() < 1e-6);
    let depth = session.history().undo_depth();

    // A half turn about the new pivot's forward axis puts the stroke above it.
    let axis = gimbal.rotation * DVec3::Z;
    let around = |angle: f64| {
        gimbal.position
            + DQuat::from_axis_angle(axis, angle) * (gimbal.rotation * DVec3::X) * gimbal.size
    };
    // (Starting away from where two rings cross.)
    session.update(&main_only(at(around(0.5))));
    assert_eq!(
        session.gimbal().unwrap().hovered[0],
        Some(GimbalHandle::RingZ)
    );
    session.update(&main_only(gripping(around(0.5))));
    for i in 1..=8 {
        session.update(&main_only(gripping(around(
            0.5 + i as f64 * std::f64::consts::PI / 8.0,
        ))));
    }
    session.update(&main_only(at(REST_MAIN)));
    assert_eq!(session.history().undo_depth(), depth + 1);
    let seen = session.view.viewer_from_world();
    let ends: Vec<DVec3> = strokes(&session, layer)[0]
        .points()
        .iter()
        .map(|p| seen.transform_point(p.position().as_dvec3()))
        .collect();
    assert!(
        ends.iter().all(|p| (p.y - 1.4).abs() < 1e-5),
        "twice the lift above where it was"
    );
    assert!(
        (session.gimbal().unwrap().position - lifted).length() < 1e-6,
        "the pivot did not move"
    );
    // A new selection has its pivot in its own middle again.
    session.update(&trigger(DVec3::new(5.0, 5.0, 5.0), 0.0));
    run(&mut session, json!({"cmd": "select_all"}));
    assert!((session.selection_pivot().unwrap().y - 1.3).abs() < 1e-5);
}

#[test]
fn the_grip_of_the_drawing_hand_belongs_to_a_line_being_drawn() {
    let (mut session, layer) = session();
    session.tools.select_tool(ToolKind::Line);
    let view = session.view;
    let start = DVec3::new(0.0, 1.0, -0.5);
    let pulling = |position: DVec3, grip: f32| {
        main_only(HandInput {
            trigger: 1.0,
            grip,
            ..at(position)
        })
    };
    session.update(&main_only(at(start)));
    session.update(&pulling(start, 0.0));
    session.update(&pulling(start + DVec3::X * 0.1, 0.0));
    session.update(&pulling(start + DVec3::X * 0.1, 1.0));
    session.update(&pulling(start + DVec3::new(0.1, 0.1, 0.0), 1.0));
    assert_eq!(session.grabbed(), None);
    assert_eq!(session.view, view, "the world stays");
    session.update(&main_only(at(start + DVec3::new(0.1, 0.1, 0.0))));
    let line = &strokes(&session, layer)[0];
    assert!(
        (line.points()[0].pos[1] - 1.1).abs() < 1e-6,
        "the start of the line went up with the hand"
    );
}

#[test]
fn the_other_stick_makes_holds_copies_and_deletes_frames() {
    let (mut session, layer) = session();
    drag(
        &mut session,
        DVec3::new(0.0, 1.0, -0.5),
        DVec3::new(0.1, 1.0, -0.5),
        5,
    );
    let frames = |session: &Session| {
        session
            .doc()
            .layer(layer)
            .unwrap()
            .as_paint()
            .unwrap()
            .frames
            .clone()
    };
    let push = |session: &mut Session, y: f32, alt: f32, frames_held: usize| {
        let off = HandInput {
            stick: [0.0, y],
            trigger: alt,
            ..at(REST_OFF)
        };
        for _ in 0..frames_held {
            session.update(&both(at(REST_MAIN), off));
        }
        session.update(&both(
            at(REST_MAIN),
            HandInput {
                trigger: alt,
                ..at(REST_OFF)
            },
        ));
    };
    push(&mut session, 1.0, 0.0, 3);
    assert_eq!(frames(&session), [0, 1]);
    assert_eq!(
        session.status()["frame"],
        1,
        "the playhead is on the new frame"
    );
    push(&mut session, -1.0, 0.0, 3);
    assert_eq!(frames(&session), [0, 1, 1], "held a frame longer");
    push(&mut session, 1.0, 1.0, 3);
    assert_eq!(frames(&session).len(), 4, "a copy of the frame");
    assert_eq!(session.history().undo_label(), Some("Duplicate frame"));
    push(&mut session, -1.0, 1.0, 3);
    assert_eq!(frames(&session).len(), 3);
    assert_eq!(session.history().undo_label(), Some("Delete frame"));
    // Held, the stick repeats: a second and a half makes a run of frames.
    push(&mut session, 1.0, 0.0, 135);
    let made = frames(&session).len() - 3;
    assert!((8..=16).contains(&made), "{made} frames");
}

#[test]
fn in_painting_mode_the_other_stick_and_its_click_do_nothing() {
    let (mut session, layer) = session();
    session
        .exec_json(&json!({ "cmd": "set_options", "values": { "painting_mode": true } }))
        .unwrap();
    let frames = |session: &Session| {
        session
            .doc()
            .layer(layer)
            .unwrap()
            .as_paint()
            .unwrap()
            .frames
            .len()
    };
    for stick in [[0.0, 1.0], [0.0, -1.0], [1.0, 0.0], [-1.0, 0.0]] {
        for _ in 0..60 {
            session.update(&both(
                at(REST_MAIN),
                HandInput {
                    stick,
                    ..at(REST_OFF)
                },
            ));
        }
        session.update(&both(at(REST_MAIN), at(REST_OFF)));
    }
    assert_eq!(frames(&session), 1, "no frame was made or held");
    assert_eq!(session.status()["frame"], 0, "and the playhead stayed");
    session.update(&both(
        at(REST_MAIN),
        HandInput {
            stick_click: true,
            ..at(REST_OFF)
        },
    ));
    assert_eq!(
        session.status()["playing"],
        false,
        "the click does not play"
    );
    assert!(
        !session.status()["can_undo"].as_bool().unwrap(),
        "nothing to undo"
    );

    // Out of painting mode the same push makes a frame again.
    session
        .exec_json(&json!({ "cmd": "set_options", "values": { "painting_mode": false } }))
        .unwrap();
    for _ in 0..3 {
        session.update(&both(
            at(REST_MAIN),
            HandInput {
                stick: [0.0, 1.0],
                ..at(REST_OFF)
            },
        ));
    }
    assert_eq!(frames(&session), 2);
}

#[test]
fn the_stroke_the_lower_button_would_delete_is_named_before_it_is_pressed() {
    let (mut session, layer) = session();
    drag(
        &mut session,
        DVec3::new(0.0, 1.0, -0.5),
        DVec3::new(0.1, 1.0, -0.5),
        5,
    );
    drag(
        &mut session,
        DVec3::new(0.0, 1.2, -0.5),
        DVec3::new(0.1, 1.2, -0.5),
        5,
    );
    let ids: Vec<u32> = strokes(&session, layer).iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 2);
    let away = at(DVec3::new(0.5, 1.6, -0.5));
    let over_second = at(DVec3::new(0.05, 1.2, -0.5));

    // The stroke just painted is under the hand, and is not pointed out.
    session.update(&main_only(at(DVec3::new(0.1, 1.2, -0.5))));
    assert_eq!(session.stroke_at_hand(), None, "not what was just painted");
    // Away from everything, nothing is singled out.
    session.update(&main_only(away));
    assert_eq!(session.stroke_at_hand(), None);
    // Coming back to the upper stroke, it is the one.
    session.update(&main_only(over_second));
    let named = session.stroke_at_hand().expect("a stroke under the hand");
    assert_eq!((named.layer, named.stroke), (layer, ids[1]));

    // Not while a tool is at work, nor after it until the hand has been away.
    session.update(&main_only(HandInput {
        trigger: 1.0,
        ..over_second
    }));
    assert_eq!(session.stroke_at_hand(), None, "the trigger is down");
    session.cancel_gesture();
    session.update(&main_only(over_second));
    assert_eq!(session.stroke_at_hand(), None, "the hand has not left yet");
    session.update(&main_only(away));
    session.update(&main_only(over_second));
    assert!(session.stroke_at_hand().is_some(), "back again");

    // Not while a grip holds something.
    session.update(&main_only(gripping(DVec3::new(0.05, 1.2, -0.5))));
    assert_eq!(session.stroke_at_hand(), None, "the hand is grabbing");
    session.update(&main_only(away));
    session.update(&main_only(over_second));
    assert!(session.stroke_at_hand().is_some());

    // Not while strokes are selected: the button would delete the selection.
    run(&mut session, json!({ "cmd": "select_all" }));
    session.update(&main_only(over_second));
    assert_eq!(session.stroke_at_hand(), None);
    run(&mut session, json!({ "cmd": "deselect" }));

    // A locked layer offers nothing to delete, and the option switches it off.
    run(
        &mut session,
        json!({ "cmd": "set_layer", "layer": layer, "locked": true }),
    );
    session.update(&main_only(over_second));
    assert_eq!(session.stroke_at_hand(), None, "the layer is locked");
    run(
        &mut session,
        json!({ "cmd": "set_layer", "layer": layer, "locked": false }),
    );
    run(
        &mut session,
        json!({ "cmd": "set_options", "values": { "show_stroke_at_hand": false } }),
    );
    session.update(&main_only(over_second));
    assert_eq!(session.stroke_at_hand(), None, "switched off");
    run(
        &mut session,
        json!({ "cmd": "set_options", "values": { "show_stroke_at_hand": true } }),
    );

    // Pressing the button deletes exactly the stroke that was named, and the next
    // one in reach, if there is one, is named at once.
    session.update(&main_only(over_second));
    let named = session.stroke_at_hand().expect("named again");
    assert_eq!(named.stroke, ids[1]);
    session.update(&main_only(HandInput {
        lower_button: true,
        ..over_second
    }));
    let left: Vec<u32> = strokes(&session, layer).iter().map(|s| s.id).collect();
    assert_eq!(left, [ids[0]]);
    session.update(&main_only(over_second));
    assert_eq!(session.stroke_at_hand(), None, "nothing is left in reach");
}

// ---- carrying a selection: shown moved, written once ----

/// The drawing the paint layer shows.
fn shown_drawing(session: &Session, layer: LayerId) -> crate::model::DrawingId {
    let paint = session.doc().layer(layer).unwrap().as_paint().unwrap();
    paint.drawings[paint.frames[0] as usize]
}

/// Where the first point of the first selected stroke is seen, in viewer space, as
/// an interface shows it: moved by the selection's motion while it is carried.
fn first_selected_seen(session: &Session, layer: LayerId) -> DVec3 {
    let drawing = shown_drawing(session, layer);
    let ids = session.selection().in_drawing(drawing).unwrap();
    let stroke = strokes(session, layer)
        .into_iter()
        .find(|s| ids.contains(&s.id))
        .unwrap();
    let in_world = session.evaluate().world_xform(session.doc(), layer);
    let motion = session
        .selection_motion()
        .map_or(Xform::IDENTITY, |m| m.motion);
    session
        .view
        .viewer_from_world()
        .mul(&motion)
        .mul(&in_world)
        .transform_point(stroke.points()[0].position().as_dvec3())
}

#[test]
fn a_carried_selection_is_shown_moved_and_written_into_the_strokes_once() {
    let (mut session, layer) = one_of_two_selected();
    let drawing = shown_drawing(&session, layer);
    let before = starts(&session, layer);
    let depth = session.history().undo_depth();
    let took = DVec3::new(0.1, 1.0, -0.5);
    let by = DVec3::new(0.0, 0.05, 0.1);
    assert!(session.selection_motion().is_none());

    session.update(&main_only(at(took)));
    session.update(&main_only(gripping(took)));
    let carried = session
        .selection_motion()
        .expect("the selection is carried");
    assert_eq!(
        carried.strokes_in(drawing),
        session.selection().in_drawing(drawing)
    );
    assert_eq!(carried.drawings().collect::<Vec<_>>(), [drawing]);
    let serial = carried.serial;
    let edits = session.edit_count();
    for i in 1..=4 {
        session.update(&main_only(gripping(took + by * (i as f64 / 4.0))));
        // The strokes stay where they are in the document while they are carried:
        // nothing has to be made again but the motion.
        assert_eq!(starts(&session, layer), before);
        assert_eq!(session.edit_count(), edits);
    }
    let carried = session.selection_motion().unwrap();
    assert_eq!(carried.serial, serial);
    assert!(close(carried.motion.translation, by));
    let shown = first_selected_seen(&session, layer);
    assert!(close(shown, before[0] + by));
    assert!((session.selection_pivot().unwrap() - (took + by)).length() < 1e-6);

    session.update(&main_only(at(took + by)));
    assert!(session.selection_motion().is_none());
    let after = starts(&session, layer);
    assert!((after[0] - (before[0] + by)).length() < 1e-6);
    assert_eq!(after[1], before[1], "the other stroke stays");
    assert!(
        (first_selected_seen(&session, layer) - shown).length() < 1e-6,
        "it lands where it was shown"
    );
    assert_eq!(session.history().undo_depth(), depth + 1);
    assert_eq!(session.history().undo_label(), Some("Move strokes"));

    // One undo puts it back; the next carry is told apart from the last.
    session.undo();
    assert_eq!(starts(&session, layer), before);
    session.update(&main_only(at(took)));
    session.update(&main_only(gripping(took)));
    assert_ne!(session.selection_motion().unwrap().serial, serial);
    session.update(&main_only(at(took)));
}

#[test]
fn cancelling_a_carry_puts_back_the_selection_and_any_copy() {
    let (mut session, layer) = one_of_two_selected();
    let before = starts(&session, layer);
    let selected = session.selection().clone();
    let depth = session.history().undo_depth();
    let took = DVec3::new(0.1, 1.0, -0.5);

    for (modifier, copies) in [(at(REST_OFF), false), (alt(), true)] {
        session.update(&both(at(took), modifier));
        session.update(&both(gripping(took), modifier));
        session.update(&both(gripping(took + DVec3::Y * 0.1), modifier));
        assert!(session.selection_motion().is_some());
        session.cancel_gesture();
        assert!(session.selection_motion().is_none());
        assert!(!session.is_grabbing(0));
        assert_eq!(starts(&session, layer), before);
        assert_eq!(session.history().undo_depth(), depth);
        // Letting go afterwards does nothing more.
        session.update(&both(at(took + DVec3::Y * 0.1), at(REST_OFF)));
        assert_eq!(starts(&session, layer), before);
        assert_eq!(session.history().undo_depth(), depth);
        if !copies {
            assert_eq!(session.selection(), &selected);
        }
    }
}

#[test]
fn a_copy_is_made_at_once_and_carried_like_any_selection() {
    let (mut session, layer) = one_of_two_selected();
    let drawing = shown_drawing(&session, layer);
    let took = DVec3::new(0.1, 1.0, -0.5);
    let step = DVec3::new(0.0, 0.0, 0.1);
    session.update(&both(at(took), alt()));
    session.update(&both(gripping(took), alt()));
    session.update(&both(gripping(took + step), alt()));
    // The copy is in the document where the original is, and it is what is carried.
    let now = starts(&session, layer);
    assert_eq!(now.len(), 3);
    assert_eq!(now[2], now[0]);
    let carried = session.selection_motion().unwrap();
    let copy = strokes(&session, layer)[2].id;
    assert_eq!(
        carried
            .strokes_in(drawing)
            .map(|ids| ids.iter().copied().collect::<Vec<_>>()),
        Some(vec![copy])
    );
    session.update(&both(at(took + step), at(REST_OFF)));
    let now = starts(&session, layer);
    assert!((now[2] - (now[0] + step)).length() < 1e-6);
    assert_eq!(session.history().undo_label(), Some("Duplicate strokes"));
    assert!(session.can_transform_again());
}

#[test]
fn a_carried_selection_on_a_mirrored_layer_lands_where_it_was_shown() {
    let (mut session, layer) = session();
    let placed = Xform {
        rotation: DQuat::from_rotation_y(0.6),
        scale: 2.0,
        translation: DVec3::new(0.3, -0.2, 0.1),
        flip: crate::math::Flip::X,
    };
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "transform": placed}),
    );
    for y in [1.0, 1.2] {
        drag(
            &mut session,
            DVec3::new(0.0, y, -0.5),
            DVec3::new(0.2, y, -0.5),
            20,
        );
    }
    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.03;
    let lower = DVec3::new(0.1, 1.0, -0.5);
    drag(&mut session, lower, lower, 1);
    assert_eq!(session.selection().count(), 1);
    let before = first_selected_seen(&session, layer);
    assert!((before - DVec3::new(0.0, 1.0, -0.5)).length() < 1e-6);

    // Turned a quarter about the hand and moved.
    let turned = |position: DVec3| HandInput {
        rotation: DQuat::from_rotation_y(FRAC_PI_2),
        ..gripping(position)
    };
    let by = DVec3::new(0.05, 0.1, 0.0);
    session.update(&main_only(at(lower)));
    session.update(&main_only(gripping(lower)));
    session.update(&main_only(turned(lower + by)));
    let shown = first_selected_seen(&session, layer);
    let expected = lower + by + DQuat::from_rotation_y(FRAC_PI_2) * (before - lower);
    assert!((shown - expected).length() < 1e-6);
    session.update(&main_only(at(lower + by)));
    assert!(session.selection_motion().is_none());
    assert!((first_selected_seen(&session, layer) - shown).length() < 1e-6);
}

#[test]
fn turning_a_selection_with_the_gimbal_is_shown_and_written_once() {
    let (mut session, layer) = one_of_two_selected();
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"show_gimbal": true}}),
    );
    session.update(&main_only(at(REST_MAIN)));
    let gimbal = session.gimbal().unwrap();
    let before = starts(&session, layer);
    let depth = session.history().undo_depth();
    let axis = gimbal.rotation * DVec3::Z;
    let around = |angle: f64| {
        gimbal.position
            + DQuat::from_axis_angle(axis, angle) * (gimbal.rotation * DVec3::X) * gimbal.size
    };
    let pressed = |angle: f64, trigger: f32| {
        main_only(HandInput {
            trigger,
            ..at(around(angle))
        })
    };
    // The trigger on a ring turns in steps; the strokes stay put until it lets go.
    session.update(&pressed(0.5, 0.0));
    session.update(&pressed(0.5, 1.0));
    session.update(&pressed(0.5 + FRAC_PI_2, 1.0));
    let carried = session.selection_motion().expect("turned by the gimbal");
    assert!(
        (carried.motion.rotation.angle_between(DQuat::IDENTITY) - FRAC_PI_2).abs() < 1e-9,
        "a quarter turn is a whole number of steps"
    );
    assert_eq!(starts(&session, layer), before);
    let shown = first_selected_seen(&session, layer);
    session.update(&pressed(0.5 + FRAC_PI_2, 0.0));
    assert!(session.selection_motion().is_none());
    assert_eq!(session.history().undo_depth(), depth + 1);
    assert_eq!(session.history().undo_label(), Some("Turn strokes"));
    assert!((first_selected_seen(&session, layer) - shown).length() < 1e-6);
    assert!((session.gimbal().unwrap().position - gimbal.position).length() < 1e-6);
}

#[test]
fn only_strokes_that_can_move_are_carried() {
    let (mut session, layer) = one_of_two_selected();
    let drawing = shown_drawing(&session, layer);
    let other = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "name": "Other"}),
    );
    run(
        &mut session,
        json!({"cmd": "select_layers", "layers": [other]}),
    );
    session.tools.select_tool(ToolKind::Paint);
    drag(
        &mut session,
        DVec3::new(0.0, 1.0, -0.4),
        DVec3::new(0.2, 1.0, -0.4),
        10,
    );
    session.tools.select_tool(ToolKind::Select);
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"select_in_all_layers": true}}),
    );
    run(&mut session, json!({"cmd": "select_all"}));
    assert_eq!(session.selection().drawings().count(), 2);
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": other, "locked": true}),
    );
    let took = DVec3::new(0.1, 1.0, -0.5);
    session.update(&main_only(at(took)));
    session.update(&main_only(gripping(took)));
    let carried = session.selection_motion().unwrap();
    assert_eq!(carried.drawings().collect::<Vec<_>>(), [drawing]);
    assert_eq!(carried.strokes_in(shown_drawing(&session, other)), None);
    session.update(&main_only(at(took)));
}

#[test]
fn a_drawing_splits_into_what_stays_and_what_is_carried() {
    use crate::model::{Brush, Drawing, Point};
    let point = |x: f32| Point {
        pos: [x, 0.0, 0.0],
        normal: [0.0, 0.0, 1.0],
        dir: [1.0, 0.0, 0.0],
        color: [1.0; 3],
        opacity: 1.0,
        width: 0.01,
    };
    let stroke = |id: u32| {
        crate::model::Stroke::new(
            id,
            Brush::Ribbon,
            false,
            (0..10)
                .map(|i| point(id as f32 + i as f32 * 0.01))
                .collect(),
        )
    };
    let mut drawing = Drawing::new();
    for id in 0..6 {
        drawing.push_stroke_in_new_chunk(stroke(id));
        drawing.push_stroke(stroke(id + 100));
    }
    assert_eq!(drawing.chunks().len(), 6);
    let carried: std::collections::BTreeSet<u32> = [1, 101, 103, 4].into();
    let split = super::grab::split_drawing(&drawing, &carried);
    let ids = |chunks: &[std::sync::Arc<crate::model::StrokeChunk>]| -> Vec<Vec<u32>> {
        chunks
            .iter()
            .map(|c| c.strokes().iter().map(|s| s.id).collect())
            .collect()
    };
    // A chunk emptied entirely is left out; untouched chunks are the drawing's own.
    assert_eq!(
        ids(&split.staying),
        [vec![0, 100], vec![2, 102], vec![3], vec![104], vec![5, 105]]
    );
    for (index, chunk) in [(0, 0), (2, 1), (5, 4)] {
        assert!(std::sync::Arc::ptr_eq(
            &split.staying[chunk],
            &drawing.chunks()[index]
        ));
    }
    let carried_ids: Vec<u32> = split.carried.strokes().map(|s| s.id).collect();
    assert_eq!(carried_ids, [1, 101, 103, 4], "in drawing order");
    assert!(std::sync::Arc::ptr_eq(
        split.carried.strokes().next().unwrap(),
        &drawing.chunks()[1].strokes()[0]
    ));
}

#[test]
fn a_copy_is_not_repeated_once_other_strokes_are_selected() {
    let (mut session, layer) = one_of_two_selected();
    let took = DVec3::new(0.1, 1.0, -0.5);
    carry(&mut session, took, took + DVec3::Z * 0.1, alt());
    assert!(session.can_transform_again());
    // Selecting the other stroke changes no stroke, but the repeat was of the copy.
    drag(
        &mut session,
        DVec3::new(0.1, 1.2, -0.5),
        DVec3::new(0.1, 1.2, -0.5),
        1,
    );
    assert_eq!(session.selection().count(), 1);
    assert!(!session.can_transform_again());
    let count = strokes(&session, layer).len();
    assert_eq!(session.redo(), None);
    assert_eq!(strokes(&session, layer).len(), count, "nothing was copied");
}
