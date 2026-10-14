use super::tools::tests::{drag, hand, strokes};
use super::*;
use crate::model::{Action, Interp};
use crate::time::{from_seconds, FrameRate};
use serde_json::json;

const FRAME: Flicks = 705_600_000 / 24;

fn run(session: &mut Session, command: Value) -> Value {
    session
        .exec_json(&command)
        .unwrap_or_else(|e| panic!("{command} failed: {e}"))
}

fn fail(session: &mut Session, command: Value) -> CommandError {
    session
        .exec_json(&command)
        .expect_err("the command should have failed")
}

fn new_layer(session: &mut Session, command: Value) -> LayerId {
    LayerId(run(session, command)["layer"].as_u64().expect("a layer id"))
}

fn session() -> (Session, LayerId) {
    let mut session = Session::new(Document::new());
    session.tools.paint.brush = tools::BrushStyle::Ribbon;
    let layer = session.paint_layer().unwrap();
    (session, layer)
}

/// Paints a short stroke at height `y`, to tell frames apart by.
fn mark(session: &mut Session, y: f64) {
    drag(
        session,
        DVec3::new(0.0, y, -0.5),
        DVec3::new(0.1, y, -0.5),
        5,
    );
}

/// The heights of the strokes on each frame of a layer, in centimetres.
fn frames_of(session: &Session, layer: LayerId) -> Vec<Vec<i32>> {
    let paint = session.doc().layer(layer).unwrap().as_paint().unwrap();
    paint
        .frames
        .iter()
        .map(|i| {
            session
                .doc()
                .drawing(paint.drawings[*i as usize])
                .unwrap()
                .strokes()
                .map(|s| (s.points()[0].pos[1] * 100.0).round() as i32)
                .collect()
        })
        .collect()
}

#[test]
fn new_frames_take_the_next_strokes_and_holds_stretch_time() {
    let (mut session, layer) = session();
    mark(&mut session, 1.0);
    run(
        &mut session,
        json!({"cmd": "add_frame", "layer": layer, "kind": "blank"}),
    );
    assert_eq!(
        session.playback.time, FRAME,
        "the playhead moves on to the new frame"
    );
    mark(&mut session, 1.1);
    run(
        &mut session,
        json!({"cmd": "add_frame", "layer": layer, "kind": "duplicate"}),
    );
    mark(&mut session, 1.2);
    assert_eq!(
        frames_of(&session, layer),
        [vec![100], vec![110], vec![110, 120]]
    );

    run(
        &mut session,
        json!({"cmd": "add_hold_frame", "layer": layer}),
    );
    assert_eq!(session.status()["frame"], 3);
    assert_eq!(frames_of(&session, layer).len(), 4);
    assert_eq!(
        session.doc().drawing_count(),
        3,
        "a hold is the same drawing shown longer"
    );

    // Stepping back and painting changes the frame under the playhead only.
    run(&mut session, json!({"cmd": "seek", "time": 0}));
    mark(&mut session, 1.5);
    assert_eq!(frames_of(&session, layer)[0], [100, 150]);
    assert_eq!(frames_of(&session, layer)[1], [110]);

    run(&mut session, json!({"cmd": "delete_frame", "layer": layer}));
    assert_eq!(
        frames_of(&session, layer),
        [vec![110], vec![110, 120], vec![110, 120]]
    );
    session.undo();
    assert_eq!(frames_of(&session, layer).len(), 4);
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "locked": true}),
    );
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "double_frames", "layer": layer})
        ),
        CommandError::Locked(_)
    ));
}

#[test]
fn key_frames_can_be_stepped_through_and_removed() {
    let (mut session, layer) = session();
    for kind in ["blank", "blank"] {
        run(
            &mut session,
            json!({"cmd": "add_hold_frame", "layer": layer}),
        );
        run(
            &mut session,
            json!({"cmd": "add_frame", "layer": layer, "kind": kind}),
        );
    }
    // Frames: A A B B C, with the playhead on C.
    assert_eq!(session.status()["frame"], 4);
    run(
        &mut session,
        json!({"cmd": "go_to_key_frame", "layer": layer, "forward": false}),
    );
    assert_eq!(session.status()["frame"], 2);
    run(&mut session, json!({"cmd": "step_frames", "frames": 1}));
    run(
        &mut session,
        json!({"cmd": "go_to_key_frame", "layer": layer, "forward": false}),
    );
    assert_eq!(
        session.status()["frame"],
        2,
        "from a hold, back goes to its own key frame"
    );
    run(
        &mut session,
        json!({"cmd": "go_to_key_frame", "layer": layer, "forward": true}),
    );
    assert_eq!(session.status()["frame"], 4);

    run(&mut session, json!({"cmd": "seek", "time": FRAME * 3}));
    run(
        &mut session,
        json!({"cmd": "remove_key_frame", "layer": layer}),
    );
    let paint = session
        .doc()
        .layer(layer)
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    assert_eq!(paint.frames.len(), 5, "the animation keeps its length");
    assert_eq!(
        paint.frames[..4],
        [paint.frames[0]; 4],
        "the removed key now holds the drawing before it"
    );
    run(
        &mut session,
        json!({"cmd": "reverse_frames", "layer": layer}),
    );
    run(
        &mut session,
        json!({"cmd": "double_frames", "layer": layer}),
    );
    run(&mut session, json!({"cmd": "clear_frames", "layer": layer}));
    assert_eq!(
        session
            .doc()
            .layer(layer)
            .unwrap()
            .as_paint()
            .unwrap()
            .frames
            .len(),
        1
    );
    session.doc().validate().unwrap();
}

#[test]
fn selected_strokes_copy_to_the_frame_shown_or_to_all_frames() {
    let (mut session, layer) = session();
    mark(&mut session, 1.0);
    run(
        &mut session,
        json!({"cmd": "add_frame", "layer": layer, "kind": "blank"}),
    );
    run(
        &mut session,
        json!({"cmd": "add_frame", "layer": layer, "kind": "blank"}),
    );
    run(&mut session, json!({"cmd": "seek", "time": 0}));
    run(&mut session, json!({"cmd": "select_all"}));
    run(&mut session, json!({"cmd": "seek", "time": FRAME}));
    assert_eq!(
        run(&mut session, json!({"cmd": "copy_selection_to_frames"}))["copied"],
        1
    );
    assert_eq!(frames_of(&session, layer), [vec![100], vec![100], vec![]]);
    assert_eq!(
        run(
            &mut session,
            json!({"cmd": "copy_selection_to_frames", "all": true})
        )["copied"],
        2
    );
    assert_eq!(
        frames_of(&session, layer),
        [vec![100], vec![100, 100], vec![100]],
        "never onto the frame it is already on"
    );
    run(&mut session, json!({"cmd": "deselect"}));
    assert!(
        fail(&mut session, json!({"cmd": "copy_selection_to_frames"}))
            .to_string()
            .contains("Nothing is selected")
    );
}

#[test]
fn clips_put_a_layer_on_and_off_the_timeline() {
    let (mut session, layer) = session();
    let shown = |session: &Session| session.evaluate().get(layer).unwrap().visible;
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(1.02)}),
    );
    run(&mut session, json!({"cmd": "toggle_clip", "layer": layer}));
    let clips = session.layer_details(layer)["clips"].clone();
    assert_eq!(
        clips,
        json!([{"start": 0, "end": FRAME * 24}]),
        "the clip ends on the frame under the playhead"
    );
    assert!(!shown(&session));
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(0.5)}),
    );
    assert!(shown(&session));

    run(&mut session, json!({"cmd": "split_clip", "layer": layer}));
    run(
        &mut session,
        json!({"cmd": "move_clip", "layer": layer, "clip": 1, "start": from_seconds(3.0)}),
    );
    run(
        &mut session,
        json!({"cmd": "trim_clip", "layer": layer, "clip": 0, "end": from_seconds(0.25)}),
    );
    run(
        &mut session,
        json!({"cmd": "duplicate_clip", "layer": layer, "clip": 0, "start": from_seconds(8.0)}),
    );
    let rows = session.timeline_rows();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == layer.0)
        .unwrap();
    assert_eq!(
        row["clips"],
        json!([
            {"start": 0, "end": from_seconds(0.25), "trim": 0},
            {"start": from_seconds(3.0), "end": from_seconds(3.5), "trim": from_seconds(0.5)},
            {"start": from_seconds(8.0), "end": from_seconds(8.25), "trim": 0},
        ])
    );
    run(
        &mut session,
        json!({"cmd": "trim_clip", "layer": layer, "clip": 2, "open_end": true}),
    );
    run(
        &mut session,
        json!({"cmd": "delete_clip", "layer": layer, "clip": 0}),
    );
    assert_eq!(
        session.layer_details(layer)["clips"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(session.layer_details(layer)["clips"][1]["end"], Value::Null);
    let root = session.doc().root();
    assert!(matches!(
        fail(&mut session, json!({"cmd": "toggle_clip", "layer": root})),
        CommandError::Invalid(_)
    ));
    assert_eq!(session.history().undo_label(), Some("Delete clip"));
}

#[test]
fn keys_hold_the_value_of_the_moment_and_can_be_moved_eased_copied_and_reversed() {
    let (mut session, layer) = session();
    let at = |x: f64| Xform::from_translation(DVec3::new(x, 0.0, 0.0));
    let x_now = |session: &Session| session.evaluate().get(layer).unwrap().local.translation.x;

    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "transform": at(1.0)}),
    );
    run(
        &mut session,
        json!({"cmd": "set_key", "layer": layer, "track": "transform"}),
    );
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(1.0)}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": layer, "transform": at(3.0)}),
    );
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(0.5)}),
    );
    assert!(
        (x_now(&session) - 2.0).abs() < 1e-9,
        "half way between the keys"
    );
    // A key added in between takes the value the layer has there.
    run(
        &mut session,
        json!({"cmd": "set_key", "layer": layer, "track": "transform", "interp": "step"}),
    );
    let keys = session.layer_details(layer)["keys"]["transform"].clone();
    assert_eq!(keys.as_array().unwrap().len(), 3);
    assert_eq!(keys[1]["value"]["translation"][0], 2.0);
    assert_eq!(keys[1]["interp"], "step");
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(0.75)}),
    );
    assert_eq!(x_now(&session), 2.0, "a stepped key holds");

    run(
        &mut session,
        json!({"cmd": "set_key_interp", "layer": layer, "track": "transform", "times": [from_seconds(0.5)], "interp": "ease_in_out"}),
    );
    run(
        &mut session,
        json!({"cmd": "move_keys", "layer": layer, "track": "transform", "times": [from_seconds(0.5)], "delta": from_seconds(0.25)}),
    );
    assert_eq!(
        session.layer_details(layer)["keys"]["transform"][1]["time"],
        from_seconds(0.75)
    );
    run(
        &mut session,
        json!({"cmd": "go_to_key", "layer": layer, "track": "transform", "forward": true}),
    );
    assert_eq!(session.playback.time, from_seconds(1.0));
    run(
        &mut session,
        json!({"cmd": "go_to_key", "layer": layer, "track": "transform", "forward": false}),
    );
    assert_eq!(session.playback.time, from_seconds(0.75));

    let all = [0, from_seconds(0.75), from_seconds(1.0)];
    run(
        &mut session,
        json!({"cmd": "reverse_keys", "layer": layer, "track": "transform", "times": all}),
    );
    run(&mut session, json!({"cmd": "seek", "time": 0}));
    assert_eq!(x_now(&session), 3.0, "the motion now runs backwards");

    // Copied keys paste onto another layer from the playhead on.
    let other = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "group"}));
    run(
        &mut session,
        json!({"cmd": "copy_keys", "layer": layer, "track": "transform", "times": [0, from_seconds(1.0)]}),
    );
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(2.0)}),
    );
    run(&mut session, json!({"cmd": "paste_keys", "layer": other}));
    let pasted = session.layer_details(other)["keys"]["transform"].clone();
    assert_eq!(
        (pasted[0]["time"].as_i64(), pasted[1]["time"].as_i64()),
        (Some(from_seconds(2.0)), Some(from_seconds(3.0)))
    );

    run(
        &mut session,
        json!({"cmd": "delete_keys", "layer": other, "track": "transform"}),
    );
    assert_eq!(
        session.layer_details(other)["keys"]["transform"]
            .as_array()
            .unwrap()
            .len(),
        1,
        "with no times given, the key at the playhead goes"
    );
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "delete_keys", "layer": other, "track": "opacity"})
        ),
        CommandError::Invalid(_)
    ));
}

#[test]
fn keying_a_group_can_key_everything_in_it_and_marks_live_on_the_root() {
    let (mut session, _) = session();
    let group = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "group"}));
    let inside = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "paint"}));
    let written = run(
        &mut session,
        json!({"cmd": "set_key", "layer": group, "track": "opacity", "children": true}),
    );
    assert_eq!(written["keys"], 2);
    assert_eq!(
        session.layer_details(inside)["keys"]["opacity"]
            .as_array()
            .unwrap()
            .len(),
        1
    );

    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(2.0)}),
    );
    run(&mut session, json!({"cmd": "set_action", "action": "stop"}));
    run(
        &mut session,
        json!({"cmd": "set_action", "time": from_seconds(4.0), "action": "loop"}),
    );
    let root = session.doc().root();
    let marks = session
        .doc()
        .layer(root)
        .unwrap()
        .anim
        .actions
        .keys()
        .to_vec();
    assert_eq!(
        marks.iter().map(|k| (k.time, k.value)).collect::<Vec<_>>(),
        [
            (from_seconds(2.0), Action::Stop),
            (from_seconds(4.0), Action::Loop)
        ]
    );
    run(&mut session, json!({"cmd": "set_action", "action": null}));
    assert_eq!(session.doc().layer(root).unwrap().anim.actions.len(), 1);
    assert!(matches!(
        fail(&mut session, json!({"cmd": "set_action", "action": null})),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "move_keys", "layer": group, "track": "action", "times": [0], "delta": 5})
        ),
        CommandError::Invalid(_)
    ));
}

#[test]
fn a_looping_sequence_bakes_into_frames_that_hold_everything_in_place() {
    let (mut session, _) = session();
    let sequence = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "sequence", "name": "Walk"}),
    );
    let moving = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "name": "Leg"}),
    );
    let still = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "parent": sequence, "name": "Body"}),
    );
    run(
        &mut session,
        json!({"cmd": "select_layers", "layers": [moving]}),
    );
    mark(&mut session, 1.0);
    run(
        &mut session,
        json!({"cmd": "select_layers", "layers": [still]}),
    );
    mark(&mut session, 2.0);
    // The leg moves up by one unit over the first quarter second, then stays.
    run(
        &mut session,
        json!({"cmd": "set_key", "layer": moving, "track": "transform", "interp": "linear"}),
    );
    run(&mut session, json!({"cmd": "seek", "time": FRAME * 6}));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": moving, "transform": Xform::from_translation(DVec3::Y)}),
    );
    assert!(
        matches!(
            fail(
                &mut session,
                json!({"cmd": "bake_sequence", "layer": sequence})
            ),
            CommandError::Invalid(_)
        ),
        "it does not loop yet"
    );
    run(&mut session, json!({"cmd": "seek", "time": FRAME * 12}));
    assert_eq!(
        run(
            &mut session,
            json!({"cmd": "set_loop_point", "layer": sequence, "enabled": true})
        )["loop_duration"],
        FRAME * 12
    );

    let baked = run(
        &mut session,
        json!({"cmd": "bake_sequence", "layer": sequence}),
    );
    assert_eq!(baked["frames"], 12);
    let layer = LayerId(baked["layer"].as_u64().unwrap());
    assert_eq!(session.doc().layer(layer).unwrap().name, "Walk_baked");
    assert!(
        !session.doc().layer(sequence).unwrap().visible,
        "the sequence is kept, hidden"
    );
    let frames = frames_of(&session, layer);
    assert_eq!(frames.len(), 12);
    assert_eq!(frames[0], [100, 200]);
    assert_eq!(frames[3], [150, 200], "half way up on frame three");
    assert_eq!(frames[6], [200, 200]);
    let paint = session
        .doc()
        .layer(layer)
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    assert_eq!(
        paint.drawings.len(),
        7,
        "the frames after the move are one drawing held"
    );
    assert_eq!(paint.max_repeat, 0, "it loops as the sequence did");
    session.doc().validate().unwrap();
    session.undo();
    assert!(session.doc().layer(layer).is_none());
    assert!(session.doc().layer(sequence).unwrap().visible);
}

#[test]
fn onion_skins_are_the_neighbouring_frames_while_stopped() {
    let (mut session, layer) = session();
    for _ in 0..4 {
        run(
            &mut session,
            json!({"cmd": "add_frame", "layer": layer, "kind": "blank"}),
        );
    }
    run(&mut session, json!({"cmd": "seek", "time": FRAME * 2}));
    assert!(session.onion_skins().is_empty(), "off until asked for");
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"onion_skin": true, "onion_frames": 1}}),
    );
    let offsets = |session: &Session| {
        session
            .onion_skins()
            .iter()
            .map(|s| s.offset)
            .collect::<Vec<_>>()
    };
    assert_eq!(offsets(&session), [-1, 1]);
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"onion_frames": 2}}),
    );
    assert_eq!(offsets(&session), [-1, 1, -2, 2]);

    // At the first frame of a layer that plays once there is nothing before it...
    run(&mut session, json!({"cmd": "seek", "time": 0}));
    assert_eq!(offsets(&session), [1, 2]);
    // ...but a looping layer shows its last frames there, for closing the loop.
    run(
        &mut session,
        json!({"cmd": "set_layer_kind", "layer": layer, "values": {"max_repeat": 0}}),
    );
    assert_eq!(offsets(&session), [-1, 1, -2, 2]);
    run(&mut session, json!({"cmd": "play"}));
    assert!(session.onion_skins().is_empty(), "never during playback");
}

#[test]
fn painting_while_playing_spreads_the_stroke_over_the_frames_that_play() {
    let (mut session, layer) = session();
    for _ in 0..5 {
        run(
            &mut session,
            json!({"cmd": "add_frame", "layer": layer, "kind": "blank"}),
        );
    }
    run(
        &mut session,
        json!({"cmd": "set_layer_kind", "layer": layer, "values": {"max_repeat": 0}}),
    );
    run(
        &mut session,
        json!({"cmd": "set_document", "range_end": FRAME * 6}),
    );
    session.tools.paint.anim_duration = 1.0 / 24.0;
    run(&mut session, json!({"cmd": "seek", "time": 0}));
    run(&mut session, json!({"cmd": "play"}));

    // Three frames go by while the hand moves to the right: 4 updates per frame at 96 Hz.
    let start = DVec3::new(0.0, 1.0, -0.5);
    session.update(&hand(start, 1.0));
    for i in 1..=12 {
        session.advance(1.0 / 96.0);
        session.update(&hand(start + DVec3::X * (i as f64 * 0.01), 1.0));
    }
    session.update(&hand(start + DVec3::X * 0.12, 0.0));
    session.exec(Command::Pause).unwrap();

    let paint = session
        .doc()
        .layer(layer)
        .unwrap()
        .as_paint()
        .unwrap()
        .clone();
    let counts: Vec<usize> = paint
        .frames
        .iter()
        .map(|i| {
            session
                .doc()
                .drawing(paint.drawings[*i as usize])
                .unwrap()
                .stroke_count()
        })
        .collect();
    // Drawn on frames 0 to 3; each piece lingers one frame more.
    assert_eq!(counts, [1, 2, 2, 2, 1, 0], "{counts:?}");
    let first = strokes(&session, layer);
    assert!(
        first[0].points().last().unwrap().pos[0] < 0.045,
        "the first frame holds only the start of the stroke"
    );
    assert_eq!(session.history().undo_label(), Some("Paint stroke"));
    session.undo();
    assert!(
        session
            .doc()
            .layer(layer)
            .unwrap()
            .as_paint()
            .unwrap()
            .drawings
            .iter()
            .all(|d| session.doc().drawing(*d).unwrap().is_empty()),
        "one undo takes back the whole gesture"
    );
}

#[test]
fn timeline_rows_place_nested_time_on_the_root_timeline() {
    let (mut session, _) = session();
    let sequence = new_layer(
        &mut session,
        json!({"cmd": "add_layer", "kind": "sequence"}),
    );
    let inside = new_layer(&mut session, json!({"cmd": "add_layer", "kind": "paint"}));
    run(
        &mut session,
        json!({"cmd": "move_clip", "layer": sequence, "clip": 0, "start": from_seconds(2.0)}),
    );
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(2.5)}),
    );
    run(
        &mut session,
        json!({"cmd": "set_key", "layer": inside, "track": "opacity"}),
    );
    let rows = session.timeline_rows();
    let row = |id: LayerId| {
        rows.as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id.0)
            .unwrap()
            .clone()
    };
    assert_eq!(row(sequence)["origin"], 0);
    assert_eq!(row(sequence)["sequence"]["loop_duration"], 0);
    assert_eq!(
        row(inside)["origin"],
        from_seconds(2.0),
        "time inside the sequence starts where its clip starts"
    );
    assert_eq!(
        row(inside)["opacity_keys"][0]["time"],
        from_seconds(0.5),
        "keys are in the sequence's own time"
    );
    assert_eq!(row(inside)["time"], from_seconds(0.5));
    assert_eq!(row(inside)["paint"]["frames"], json!([0]));
    assert_eq!(row(inside)["paint"]["frame_duration"], FRAME);
    let _ = (Interp::Linear, FrameRate::whole(24));
}
