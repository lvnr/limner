use super::*;
use crate::math::Flip;
use crate::model::{Brush, Drawing, Interp, Point, Stroke};
use crate::time::{from_seconds, FrameRate};
use glam::DQuat;
use serde_json::json;

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

fn layer_id(value: &Value) -> LayerId {
    LayerId(
        value["layer"]
            .as_u64()
            .expect("the command returns a layer id"),
    )
}

fn names(session: &Session, parent: LayerId) -> Vec<String> {
    session
        .doc()
        .children(parent)
        .iter()
        .map(|c| session.doc().layer(*c).unwrap().name.clone())
        .collect()
}

fn stroke_at(x: f32) -> Stroke {
    let p = |y: f32| Point {
        pos: [x, y, 0.0],
        normal: [0.0, 0.0, 1.0],
        dir: [0.0, 0.0, -1.0],
        color: [1.0; 3],
        opacity: 1.0,
        width: 0.01,
    };
    Stroke::new(0, Brush::Ribbon, false, vec![p(0.0), p(1.0)])
}

#[test]
fn a_session_starts_on_the_default_spawn_area() {
    let mut doc = Document::new();
    let spawn = doc.settings.default_viewpoint.unwrap();
    doc.layer_mut(spawn).unwrap().transform = Xform {
        scale: 3.0,
        translation: DVec3::new(1.0, 0.0, 2.0),
        flip: Flip::X,
        ..Xform::IDENTITY
    };
    let session = Session::new(doc);
    assert_eq!(session.view.world_from_viewer.scale, 3.0);
    assert_eq!(
        session.view.world_from_viewer.translation,
        DVec3::new(1.0, 0.0, 2.0)
    );
    assert!(!session.view.world_from_viewer.flip.is_mirrored());
    assert_eq!(
        session.paint_layer(),
        session.doc().find_by_path("Root/Paint")
    );
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
    doc.layer_mut(paint).unwrap().transform = Xform::from_translation(DVec3::new(0.0, 10.0, 0.0));
    let (center, radius) = Session::new(doc.clone()).visible_bounds().unwrap();
    assert!((center - DVec3::new(1.0, 10.0, 0.0)).length() < 1e-9);
    assert!((radius - 2.0).abs() < 1e-9);
    doc.layer_mut(paint).unwrap().visible = false;
    assert!(Session::new(doc).visible_bounds().is_none());
}

#[test]
fn commands_are_json_and_every_edit_can_be_undone_and_redone() {
    let mut session = Session::new(Document::new());
    let root = session.doc().root();
    let original = session.doc().clone();
    assert!(!session.is_dirty());

    let light = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "light", "name": "Key light"}),
    ));
    run(
        &mut session,
        json!({"cmd": "set_layer_kind", "layer": light, "values": {"kind": "spot", "energy": 2.5}}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": light, "visible": false, "opacity": 0.5}),
    );
    assert_eq!(
        names(&session, root),
        ["Spawn Area", "Paint", "Key light"],
        "a new layer goes right after the current one"
    );
    assert_eq!(session.current_layer(), Some(light));
    assert!(session.is_dirty());
    assert_eq!(session.history().undo_depth(), 3);
    let edited = session.doc().clone();

    assert_eq!(
        run(&mut session, json!({"cmd": "undo"}))["undone"],
        "Change layer opacity"
    );
    run(&mut session, json!({"cmd": "undo"}));
    run(&mut session, json!({"cmd": "undo"}));
    assert_eq!(session.doc(), &original);
    assert_eq!(
        session.current_layer(),
        None,
        "the deleted layer is no longer selected"
    );
    assert_eq!(
        run(&mut session, json!({"cmd": "undo"}))["undone"],
        Value::Null,
        "nothing left to undo"
    );
    for _ in 0..3 {
        run(&mut session, json!({"cmd": "redo"}));
    }
    assert_eq!(session.doc(), &edited);
    assert_eq!(session.status()["can_redo"], false);
}

#[test]
fn a_failed_command_changes_nothing_and_leaves_no_undo_step() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    let before = session.doc().clone();
    assert!(matches!(
        fail(&mut session, json!({"cmd": "merge_down", "layer": paint})),
        CommandError::Op(_)
    ));
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "set_document", "range_start": 10, "range_end": 5})
        ),
        CommandError::Invalid(_)
    ));
    assert!(matches!(
        fail(&mut session, json!({"cmd": "fly_to_the_moon"})),
        CommandError::Malformed(_)
    ));
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "set_layer", "layer": 99999, "visible": true})
        ),
        CommandError::Op(_)
    ));
    assert_eq!(session.doc(), &before);
    assert!(!session.history().can_undo());
}

#[test]
fn a_transaction_turns_a_slider_drag_into_one_undo_step() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    session.begin_transaction("Change layer opacity");
    for opacity in [0.9, 0.6, 0.3] {
        run(
            &mut session,
            json!({"cmd": "set_layer", "layer": paint, "opacity": opacity}),
        );
    }
    session.commit_transaction();
    assert_eq!(session.history().undo_depth(), 1);
    assert_eq!(session.doc().layer(paint).unwrap().opacity, 0.3);
    session.undo();
    assert_eq!(session.doc().layer(paint).unwrap().opacity, 1.0);

    session.begin_transaction("Change layer opacity");
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "opacity": 0.1}),
    );
    session.cancel_transaction();
    assert_eq!(session.doc().layer(paint).unwrap().opacity, 1.0);
    assert_eq!(session.history().undo_depth(), 0);

    session.begin_transaction("Nothing");
    session.commit_transaction();
    assert_eq!(
        session.history().undo_depth(),
        0,
        "a transaction that changed nothing is not a step"
    );
}

#[test]
fn locked_layers_refuse_edits_but_can_be_unlocked() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "locked": true}),
    );
    assert_eq!(
        fail(
            &mut session,
            json!({"cmd": "set_layer", "layer": paint, "opacity": 0.2})
        ),
        CommandError::Locked("Paint".into())
    );
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "delete_layers", "layers": [paint]})
        ),
        CommandError::Locked(_)
    ));
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "flip_layer", "layer": paint, "axis": "x"})
        ),
        CommandError::Locked(_)
    ));
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "name": "Still renamable", "locked": false}),
    );
    run(
        &mut session,
        json!({"cmd": "delete_layers", "layers": [paint]}),
    );
    assert_eq!(session.paint_layer(), None, "no paint layer is left");
}

#[test]
fn renaming_keeps_sibling_names_unique_and_usable_in_paths() {
    let mut session = Session::new(Document::new());
    let root = session.doc().root();
    let a = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint"}),
    ));
    assert_eq!(names(&session, root), ["Spawn Area", "Paint", "Paint 2"]);
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": a, "name": "Paint"}),
    );
    assert_eq!(
        session.doc().layer(a).unwrap().name,
        "Paint 2",
        "the name of a sibling is taken"
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": a, "name": "Paint 2"}),
    );
    assert_eq!(
        session.doc().layer(a).unwrap().name,
        "Paint 2",
        "keeping its own name is fine"
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": a, "name": "sky/far"}),
    );
    assert_eq!(session.doc().find_by_path("Root/sky-far"), Some(a));
}

#[test]
fn new_layers_go_into_an_open_group_and_can_start_at_the_playhead() {
    let mut session = Session::new(Document::new());
    let group = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "group"}),
    ));
    let inside = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint"}),
    ));
    assert_eq!(session.doc().parent_of(inside), Some(group));
    assert_eq!(
        session.paint_layer(),
        Some(inside),
        "a new paint layer becomes the one painted on"
    );
    assert_eq!(
        session.doc().layer(inside).unwrap().anim.visibility.keys()[0].time,
        0
    );

    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"create_layers_at_playhead": true}}),
    );
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(2.03)}),
    );
    let late = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "parent": group}),
    ));
    let start = session.doc().layer(late).unwrap().anim.visibility.keys()[0].time;
    assert_eq!(
        start,
        FrameRate::whole(24).frame_start(48),
        "the clip starts on the frame under the playhead"
    );
    assert!(!session.evaluate().get(late).unwrap().in_clip || session.playback.time >= start);
}

#[test]
fn moving_a_layer_between_groups_keeps_it_in_place() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    let root = session.doc().root();
    let group = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "group", "parent": root}),
    ));
    let placed = Xform {
        rotation: DQuat::from_rotation_y(0.7),
        scale: 2.0,
        translation: DVec3::new(1.0, 2.0, 3.0),
        flip: Flip::Z,
    };
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": group, "transform": placed}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "transform": Xform::from_translation(DVec3::new(0.0, 0.5, 0.0))}),
    );
    let probe = DVec3::new(0.3, -0.2, 0.9);
    let before = session
        .doc()
        .world_from_layer(paint)
        .transform_point3(probe);

    run(
        &mut session,
        json!({"cmd": "move_layer", "layer": paint, "parent": group, "index": 0}),
    );
    assert_eq!(session.doc().parent_of(paint), Some(group));
    let inside = session
        .doc()
        .world_from_layer(paint)
        .transform_point3(probe);
    assert!(
        (inside - before).length() < 1e-9,
        "{inside:?} != {before:?}"
    );

    // Stepping up out of the group again also keeps it in place.
    run(
        &mut session,
        json!({"cmd": "step_layer", "layer": paint, "up": true}),
    );
    assert_eq!(session.doc().parent_of(paint), Some(session.doc().root()));
    let outside = session
        .doc()
        .world_from_layer(paint)
        .transform_point3(probe);
    assert!((outside - before).length() < 1e-9);
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "move_layer", "layer": group, "parent": group, "index": 0})
        ),
        CommandError::Op(_)
    ));
}

#[test]
fn moving_an_animated_layer_or_recording_writes_keys_at_the_playhead() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    let to = |x: f64| Xform::from_translation(DVec3::new(x, 0.0, 0.0));

    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "transform": to(1.0)}),
    );
    assert!(
        session
            .doc()
            .layer(paint)
            .unwrap()
            .anim
            .transform
            .is_empty(),
        "a still layer just moves"
    );

    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"recording": true}}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "transform": to(2.0)}),
    );
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"recording": false}}),
    );
    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(1.01)}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "transform": to(4.0), "opacity": 0.5}),
    );

    let layer = session.doc().layer(paint).unwrap().clone();
    let keys = layer.anim.transform.keys();
    assert_eq!(
        keys.len(),
        2,
        "once animated, moving the layer keys it without recording"
    );
    assert_eq!(
        (keys[0].time, keys[1].time),
        (0, FrameRate::whole(24).frame_start(24))
    );
    assert_eq!(keys[1].interp, Interp::Linear);
    assert!(
        layer.anim.opacity.is_empty(),
        "opacity was never animated, so it stays a plain value"
    );
    assert_eq!(layer.opacity, 0.5);

    run(
        &mut session,
        json!({"cmd": "seek", "time": from_seconds(0.5)}),
    );
    let halfway = session.evaluate().get(paint).unwrap().local.translation.x;
    assert!((halfway - 3.0).abs() < 1e-6, "{halfway}");
    assert_eq!(
        session.layer_details(paint)["keys"]["transform"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn the_document_frame_rate_applies_to_every_paint_layer() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    run(
        &mut session,
        json!({"cmd": "set_document", "rate": {"num": 12, "den": 1}, "title": "Short", "background": [0.1, 0.2, 0.3]}),
    );
    assert_eq!(
        session.doc().layer(paint).unwrap().as_paint().unwrap().rate,
        FrameRate::whole(12)
    );
    assert_eq!(session.document_info()["fps"], 12.0);
    assert_eq!(session.doc().settings.title, "Short");
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "set_document", "rate": {"num": 0, "den": 1}})
        ),
        CommandError::Invalid(_)
    ));
    run(&mut session, json!({"cmd": "step_frames", "frames": 3}));
    assert_eq!(session.playback.time, from_seconds(0.25));
    run(&mut session, json!({"cmd": "step_frames", "frames": -10}));
    assert_eq!(session.playback.time, 0);
}

#[test]
fn options_and_tool_settings_take_partial_patches() {
    let mut session = Session::new(Document::new());
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"show_grid": false, "rotation_snap_steps": 100000}}),
    );
    assert!(!session.options.show_grid);
    assert_eq!(session.options.rotation_snap_steps, 360);
    run(
        &mut session,
        json!({"cmd": "set_tool_settings", "values": {"paint": {"size": 0.05, "brush": "cube"}, "colorize": {"mode": "dodge"}}}),
    );
    assert_eq!(session.tools.paint.size, 0.05);
    assert_eq!(session.tools.paint.opacity, 1.0);
    assert_eq!(session.tools.colorize.mode, tools::BlendMode::Dodge);
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "set_tool_settings", "values": {"paint": {"sise": 1}}})
        ),
        CommandError::Invalid(_)
    ));
    run(&mut session, json!({"cmd": "set_tool", "tool": "eraser"}));
    run(
        &mut session,
        json!({"cmd": "set_color", "color": [1, 0.5, 0]}),
    );
    assert_eq!(session.status()["tool"], "eraser");
    assert_eq!(session.tools.color, [1.0, 0.5, 0.0]);
    assert!(
        !session.history().can_undo(),
        "none of this is part of the document"
    );
}

#[test]
fn grouping_merging_and_flattening_select_their_result() {
    let mut session = Session::new(Document::new());
    let root = session.doc().root();
    let a = session.paint_layer().unwrap();
    let b = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "name": "B"}),
    ));
    for (layer, x) in [(a, 1.0), (b, 2.0)] {
        let drawing = session
            .doc()
            .layer(layer)
            .unwrap()
            .as_paint()
            .unwrap()
            .drawings[0];
        session
            .edit("Test strokes", |s| {
                s.doc_mut()
                    .drawing_mut(drawing)
                    .unwrap()
                    .push_stroke(stroke_at(x));
                Ok(())
            })
            .unwrap();
    }
    let spawn = session.doc().find_by_path("Root/Spawn Area").unwrap();
    assert!(matches!(
        fail(
            &mut session,
            json!({"cmd": "group_layers", "layers": [a, root]})
        ),
        CommandError::Invalid(_)
    ));

    let group = layer_id(&run(
        &mut session,
        json!({"cmd": "group_layers", "layers": [b, a], "name": "Pair"}),
    ));
    assert_eq!(names(&session, root), ["Spawn Area", "Pair"]);
    assert_eq!(
        names(&session, group),
        ["Paint", "B"],
        "list order is kept whatever order they were picked in"
    );
    assert_eq!(session.current_layer(), Some(group));

    let flat = layer_id(&run(
        &mut session,
        json!({"cmd": "flatten_group", "layer": group}),
    ));
    assert_eq!(session.current_layer(), Some(flat));
    assert_eq!(session.paint_layer(), Some(flat));
    assert_eq!(
        session
            .layer_rows()
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == flat.0)
            .unwrap()["strokes"],
        2
    );
    assert!(matches!(
        fail(&mut session, json!({"cmd": "group_layers", "layers": []})),
        CommandError::Invalid(_)
    ));
    let _ = spawn;
}

#[test]
fn layer_rows_describe_the_tree_for_a_list() {
    let mut session = Session::new(Document::new());
    let group = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "sequence", "name": "Loop"}),
    ));
    let child = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "camera"}),
    ));
    let inner = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "group", "parent": group}),
    ));
    run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "paint", "parent": inner}),
    );
    let depth_before = session.history().undo_depth();
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": group, "collapsed": true}),
    );
    assert_eq!(
        session.history().undo_depth(),
        depth_before,
        "folding a group is not an undo step"
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": group, "visible": false}),
    );

    let rows = session.layer_rows();
    let rows = rows.as_array().unwrap();
    let row = |id: LayerId| rows.iter().find(|r| r["id"] == id.0).unwrap();
    assert_eq!(rows[0]["kind"], "sequence", "the root comes first");
    assert_eq!(row(group)["kind"], "sequence");
    assert_eq!(row(group)["hidden_in_list"], false);
    assert_eq!(
        row(child)["hidden_in_list"],
        true,
        "rows inside a folded group are not listed"
    );
    assert_eq!(row(child)["depth"], 2);
    assert_eq!(row(child)["visible"], true);
    assert_eq!(row(child)["shown"], false, "hidden with its group");
    assert_eq!(row(child)["kind"], "camera");
    assert_eq!(rows.len(), 7);
    assert!(rows.iter().filter(|r| r["hidden_in_list"] == true).count() == 3);
    assert_eq!(session.layer_details(child)["props"]["fov"], 45.0);
    assert_eq!(session.layer_details(LayerId(424242)), Value::Null);
}

#[test]
fn editor_state_round_trips_and_tolerates_rubbish() {
    let mut session = Session::new(Document::new());
    let camera = layer_id(&run(
        &mut session,
        json!({"cmd": "add_layer", "kind": "camera"}),
    ));
    run(&mut session, json!({"cmd": "set_tool", "tool": "grab"}));
    run(
        &mut session,
        json!({"cmd": "set_options", "values": {"onion_skin": true, "recording": true}}),
    );
    run(
        &mut session,
        json!({"cmd": "set_transform_target", "layer": camera}),
    );
    run(&mut session, json!({"cmd": "seek", "time": 12345}));
    session.view.pan(DVec3::new(1.0, 2.0, 3.0));
    let saved = session.editor_state();

    let mut reopened = Session::new(session.doc().clone());
    reopened.restore_editor_state(&saved);
    assert_eq!(reopened.tools.tool, tools::ToolKind::Grab);
    assert!(reopened.options.onion_skin);
    assert!(
        !reopened.options.recording,
        "recording is never armed on open"
    );
    assert_eq!(reopened.transform_target(), Some(camera));
    assert_eq!(reopened.current_layer(), Some(camera));
    assert_eq!(reopened.view, session.view);
    assert_eq!(reopened.playback.time, 12345);

    let mut fresh = Session::new(Document::new());
    fresh.restore_editor_state(
        &json!({"tools": 7, "options": "no", "paint_layer": 99999, "view": {"scale": -1}}),
    );
    assert_eq!(fresh.tools, tools::ToolSettings::default());
    assert_eq!(fresh.paint_layer(), fresh.doc().find_by_path("Root/Paint"));
}

#[test]
fn playback_commands_drive_the_playhead() {
    let mut session = Session::new(Document::new());
    run(&mut session, json!({"cmd": "toggle_play"}));
    assert!(session.playback.playing);
    session.advance(0.5);
    assert_eq!(session.status()["frame"], 12);
    run(&mut session, json!({"cmd": "toggle_play"}));
    assert!(!session.playback.playing);
    run(&mut session, json!({"cmd": "fast_forward"}));
    assert_eq!(session.playback.time, session.doc().settings.range_end);
    run(&mut session, json!({"cmd": "rewind"}));
    assert_eq!(session.playback.time, 0);
}

#[test]
fn an_edit_that_changes_nothing_is_not_an_undo_step() {
    let mut session = Session::new(Document::new());
    let paint = session.paint_layer().unwrap();
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "opacity": 1.0}),
    );
    run(
        &mut session,
        json!({"cmd": "set_document", "background": [0.8, 0.8, 0.8]}),
    );
    assert_eq!(session.history().undo_depth(), 0);
    session.begin_transaction("Change layer opacity");
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "opacity": 0.4}),
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "opacity": 1.0}),
    );
    session.commit_transaction();
    assert_eq!(
        session.history().undo_depth(),
        0,
        "dragged away and back again"
    );
    run(
        &mut session,
        json!({"cmd": "set_layer", "layer": paint, "opacity": 0.4}),
    );
    assert_eq!(session.history().undo_depth(), 1);
}

#[test]
fn framing_counts_flat_pictures_and_models_but_not_what_surrounds_the_viewer() {
    use crate::model::{MediaData, MeshData, Projection};
    use crate::session::media::{MediaLayerKind, NewMedia};
    use std::sync::Arc;

    let mut session = Session::new(Document::new());
    assert!(
        session.visible_bounds().is_none(),
        "an empty document has nothing to frame"
    );
    let picture = session
        .add_media_layer(NewMedia {
            kind: MediaLayerKind::Picture,
            file_name: "wide.png".into(),
            source_path: String::new(),
            data: MediaData::RawImage {
                width: 4,
                height: 2,
                channels: 3,
                pixels: Arc::from(vec![0u8; 24]),
            },
        })
        .unwrap();
    // A picture is one unit tall and as wide as its shape: here two units.
    let (center, radius) = session.visible_bounds().expect("the picture");
    assert!(center.length() < 1e-9);
    assert!((radius - (1.0f64 + 0.25).sqrt()).abs() < 1e-6);

    // Wrapped around the viewer it is no longer something to stand back from.
    let mut values = serde_json::Map::new();
    values.insert(
        "projection".into(),
        serde_json::to_value(Projection::Equirect).unwrap(),
    );
    session
        .exec(Command::SetLayerKind {
            layer: picture,
            values,
        })
        .unwrap();
    assert!(session.visible_bounds().is_none());

    session
        .add_media_layer(NewMedia {
            kind: MediaLayerKind::Model,
            file_name: "box.obj".into(),
            source_path: String::new(),
            data: MediaData::Mesh(Arc::new(MeshData {
                positions: vec![[2.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 2.0, 0.0]],
                normals: Vec::new(),
                colors: Vec::new(),
                indices: vec![0, 1, 2],
            })),
        })
        .unwrap();
    let (center, radius) = session.visible_bounds().expect("the model");
    assert!((center - DVec3::new(3.0, 1.0, 0.0)).length() < 1e-9);
    assert!((radius - 2f64.sqrt()).abs() < 1e-6);
}

#[test]
fn layers_cannot_be_nested_deeper_than_the_format_holds() {
    use crate::model::MAX_LAYER_DEPTH;
    let mut session = Session::new(Document::new());
    let mut parent = session.doc().root();
    let mut groups = Vec::new();
    for _ in 0..MAX_LAYER_DEPTH {
        let result = session
            .exec_json(&json!({"cmd": "add_layer", "kind": "group", "parent": parent}))
            .unwrap();
        parent = LayerId(result["layer"].as_u64().unwrap());
        groups.push(parent);
    }
    assert_eq!(session.doc().depth(parent), MAX_LAYER_DEPTH);
    let count = session.doc().layer_count();
    let refused = session
        .exec_json(&json!({"cmd": "add_layer", "kind": "paint", "parent": parent}))
        .unwrap_err();
    assert!(refused.to_string().contains("nested"), "{refused}");
    assert_eq!(session.doc().layer_count(), count);

    // Moving a group counts what is inside it: the second group holds 46 more levels.
    let refused = session
        .exec_json(
            &json!({"cmd": "move_layer", "layer": groups[1], "parent": groups[0], "index": 0}),
        )
        .is_ok();
    assert!(refused, "moving it to where it already is changes no depth");
    let top = LayerId(
        session
            .exec_json(
                &json!({"cmd": "add_layer", "kind": "group", "parent": session.doc().root()}),
            )
            .unwrap()["layer"]
            .as_u64()
            .unwrap(),
    );
    let inner = LayerId(
        session
            .exec_json(&json!({"cmd": "add_layer", "kind": "group", "parent": top}))
            .unwrap()["layer"]
            .as_u64()
            .unwrap(),
    );
    assert!(session
        .exec_json(&json!({"cmd": "move_layer", "layer": groups[0], "parent": inner, "index": 0}))
        .is_err());
    session.doc().validate().unwrap();
}
