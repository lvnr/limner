//! The tools, driven the way a controller drives them.

use crate::math::{Flip, Xform};
use crate::model::{Brush, Document, LayerId, Stroke};
use crate::session::input::{Feedback, HandInput, InputFrame};
use crate::session::tools::{BrushStyle, ToolKind};
use crate::session::Session;
use glam::{DQuat, DVec3};
use std::sync::Arc;

pub(crate) fn hand(position: DVec3, trigger: f32) -> InputFrame {
    InputFrame {
        main: HandInput {
            tracked: true,
            position,
            trigger,
            ..HandInput::default()
        },
        ..InputFrame::default()
    }
}

/// Presses the trigger at `from`, moves to `to` in `steps` frames, and lets go.
pub(crate) fn drag(session: &mut Session, from: DVec3, to: DVec3, steps: usize) -> Vec<Feedback> {
    let mut feedback = session.update(&hand(from, 0.0));
    for i in 0..=steps {
        let t = i as f64 / steps.max(1) as f64;
        feedback.extend(session.update(&hand(from.lerp(to, t), 1.0)));
    }
    feedback.extend(session.update(&hand(to, 0.0)));
    feedback
}

pub(crate) fn strokes(session: &Session, layer: LayerId) -> Vec<Arc<Stroke>> {
    let paint = session.doc().layer(layer).unwrap().as_paint().unwrap();
    session
        .doc()
        .drawing(paint.drawings[paint.frames[0] as usize])
        .unwrap()
        .strokes()
        .cloned()
        .collect()
}

fn session() -> (Session, LayerId) {
    let session = Session::new(Document::new());
    let layer = session.paint_layer().unwrap();
    (session, layer)
}

#[test]
fn a_trigger_press_paints_one_stroke_and_one_undo_step() {
    let (mut session, layer) = session();
    session.tools.color = [0.9, 0.1, 0.2];
    session.tools.paint.brush = BrushStyle::Ribbon;
    let revision = session.doc_revision();
    session.update(&hand(DVec3::new(0.0, 1.0, -0.5), 0.0));
    session.update(&hand(DVec3::new(0.0, 1.0, -0.5), 1.0));
    assert!(session.is_tool_active());
    for i in 1..=10 {
        session.update(&hand(DVec3::new(i as f64 * 0.01, 1.0, -0.5), 1.0));
    }
    assert_eq!(
        session.doc_revision(),
        revision,
        "interfaces are not told while the stroke grows"
    );
    assert_eq!(
        strokes(&session, layer).len(),
        1,
        "the stroke is on screen while it is drawn"
    );
    session.update(&hand(DVec3::new(0.1, 1.0, -0.5), 0.0));
    assert!(!session.is_tool_active());
    assert_ne!(session.doc_revision(), revision);

    let drawn = strokes(&session, layer);
    assert_eq!(drawn.len(), 1);
    let stroke = &drawn[0];
    assert_eq!(stroke.brush, Brush::Ribbon);
    assert_eq!(stroke.len(), 11, "one point per centimetre moved");
    let (first, last) = (stroke.points()[0], stroke.points()[10]);
    assert_eq!(first.pos, [0.0, 1.0, -0.5]);
    assert!((last.pos[0] - 0.1).abs() < 1e-6);
    assert_eq!(first.color, [0.9, 0.1, 0.2]);
    assert_eq!(
        first.normal,
        [0.0, 0.0, 1.0],
        "a ribbon faces back along the hand"
    );
    assert!((last.width - 0.01).abs() < 1e-7);
    // Drawn from the eye toward the point.
    let eye = InputFrame::default().head;
    let expected = (DVec3::new(0.0, 1.0, -0.5) - eye).normalize().as_vec3();
    assert!((glam::Vec3::from_array(first.dir) - expected).length() < 1e-6);

    assert_eq!(session.history().undo_depth(), 1);
    assert_eq!(session.history().undo_label(), Some("Paint stroke"));
    session.undo();
    assert!(strokes(&session, layer).is_empty());
    session.redo();
    assert_eq!(strokes(&session, layer).len(), 1);
}

#[test]
fn strokes_land_where_the_hand_is_whatever_the_view_and_layer_transforms() {
    let (mut session, layer) = session();
    let placed = Xform {
        rotation: DQuat::from_rotation_y(0.6),
        scale: 4.0,
        translation: DVec3::new(3.0, -1.0, 2.0),
        flip: Flip::X,
    };
    session
        .exec_json(&serde_json::json!({"cmd": "set_layer", "layer": layer, "transform": placed}))
        .unwrap();
    session.view.world_from_viewer = Xform {
        rotation: DQuat::from_rotation_x(-0.3),
        scale: 0.25,
        translation: DVec3::new(-5.0, 0.5, 1.0),
        flip: Flip::None,
    };
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.tools.paint.size = 0.02;
    let (from, to) = (DVec3::new(0.1, 1.2, -0.4), DVec3::new(0.3, 1.3, -0.6));
    drag(&mut session, from, to, 30);

    let stroke = strokes(&session, layer)[0].clone();
    let world_from_layer = session.doc().world_from_layer(layer);
    let world_from_viewer = session.view.world_from_viewer;
    let first = world_from_layer.transform_point3(stroke.points()[0].position().as_dvec3());
    let last =
        world_from_layer.transform_point3(stroke.points().last().unwrap().position().as_dvec3());
    assert!(
        (first - world_from_viewer.transform_point(from)).length() < 1e-5,
        "{first:?}"
    );
    assert!((last - world_from_viewer.transform_point(to)).length() < 1e-5);
    // Two centimetres in the viewer's space: a quarter scale view onto a four times
    // scaled layer makes that 0.02 * 0.25 / 4 layer units.
    assert!(
        (stroke.points()[5].width - 0.02 * 0.25 / 4.0).abs() < 1e-8,
        "{}",
        stroke.points()[5].width
    );
}

#[test]
fn a_click_leaves_a_dot_and_tube_brushes_get_closed_ends() {
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::CappedCylinder;
    let at = DVec3::new(0.0, 1.0, -0.5);
    drag(&mut session, at, at, 2);
    let ball = strokes(&session, layer)[0].clone();
    assert_eq!(ball.brush, Brush::Cylinder);
    assert_eq!(ball.points()[0].width, 0.0);
    assert_eq!(ball.points().last().unwrap().width, 0.0);
    let widest = ball.points().iter().map(|p| p.width).fold(0.0, f32::max);
    assert!(
        (widest - 0.01).abs() < 1e-6,
        "a ball as wide as the brush whatever the pressure"
    );

    session.tools.paint.brush = BrushStyle::Cube;
    drag(&mut session, at, at + DVec3::X * 0.05, 10);
    let bar = strokes(&session, layer)[1].clone();
    assert_eq!(bar.points()[0].width, 0.0);
    assert!(
        (bar.points()[0].pos[0] + 1e-4).abs() < 1e-6,
        "a flat cap closes a tenth of a millimetre beyond the end"
    );
    assert!((bar.points()[1].width - 0.01).abs() < 1e-7);
    assert_ne!(ball.id, bar.id);
}

#[test]
fn pressure_shapes_the_width_when_the_taper_says_so() {
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.update(&hand(DVec3::new(0.0, 1.0, -0.5), 0.5));
    session.update(&hand(DVec3::new(0.01, 1.0, -0.5), 0.5));
    session.update(&hand(DVec3::new(0.02, 1.0, -0.5), 1.0));
    session.update(&hand(DVec3::new(0.03, 1.0, -0.5), 0.0));
    let widths: Vec<f32> = strokes(&session, layer)[0]
        .points()
        .iter()
        .map(|p| p.width)
        .collect();
    assert_eq!(widths.len(), 3);
    assert!(
        (widths[0] - 0.005).abs() < 1e-7 && (widths[2] - 0.01).abs() < 1e-7,
        "{widths:?}"
    );
}

#[test]
fn a_locked_or_hidden_layer_refuses_paint_and_says_why() {
    let (mut session, layer) = session();
    session
        .exec_json(&serde_json::json!({"cmd": "set_layer", "layer": layer, "locked": true}))
        .unwrap();
    let depth = session.history().undo_depth();
    let feedback = drag(&mut session, DVec3::ZERO, DVec3::X * 0.1, 5);
    assert_eq!(
        feedback,
        [Feedback::Message("Layer \"Paint\" is locked".into())]
    );
    assert!(strokes(&session, layer).is_empty());
    assert_eq!(session.history().undo_depth(), depth);

    session.exec_json(&serde_json::json!({"cmd": "set_layer", "layer": layer, "locked": false, "visible": false})).unwrap();
    let feedback = drag(&mut session, DVec3::ZERO, DVec3::X * 0.1, 5);
    assert_eq!(
        feedback,
        [Feedback::Message("Layer \"Paint\" is hidden".into())]
    );
}

#[test]
fn painting_with_no_paint_layer_makes_one() {
    let mut session = Session::new(Document::empty());
    assert_eq!(session.paint_layer(), None);
    drag(
        &mut session,
        DVec3::new(0.0, 1.0, -0.5),
        DVec3::new(0.1, 1.0, -0.5),
        5,
    );
    let layer = session.paint_layer().expect("a paint layer was created");
    assert_eq!(strokes(&session, layer).len(), 1);
    session.undo();
    assert_eq!(
        session.paint_layer(),
        None,
        "one undo takes back the stroke and the layer"
    );
}

#[test]
fn the_line_tool_pulls_a_straight_evenly_divided_line() {
    let (mut session, layer) = session();
    session.tools.select_tool(ToolKind::Line);
    session.tools.line.brush = BrushStyle::Cube;
    let from = DVec3::new(0.0, 1.0, -0.5);
    // A wobbly path: only the end points matter.
    session.update(&hand(from, 1.0));
    session.update(&hand(from + DVec3::new(0.01, 0.03, 0.0), 1.0));
    session.update(&hand(from + DVec3::new(0.0208, 0.0, 0.0), 1.0));
    session.update(&hand(from + DVec3::new(0.0208, 0.0, 0.0), 0.0));
    let line = strokes(&session, layer)[0].clone();
    // 20.8 mm at one point per 5 mm: five segments, six points, plus the two flat caps.
    assert_eq!(line.len(), 8);
    let xs: Vec<f32> = line.points().iter().map(|p| p.pos[0]).collect();
    assert!((xs[2] - xs[1] - 0.00416).abs() < 1e-6, "{xs:?}");
    assert!(line.points().iter().all(|p| p.pos[1] == 1.0), "straight");
    assert_eq!(session.history().undo_label(), Some("Straight line"));
}

#[test]
fn alt_turns_paint_into_line_and_snaps_a_line_to_the_world_axes() {
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.tools.line.brush = BrushStyle::Ribbon;
    let with_alt = |position: DVec3, trigger: f32, alt: f32| InputFrame {
        off: HandInput {
            tracked: true,
            trigger: alt,
            ..HandInput::default()
        },
        ..hand(position, trigger)
    };
    let from = DVec3::new(0.0, 1.0, -0.5);
    // Alt held when the trigger goes down: the Paint tool draws a line.
    session.update(&with_alt(from, 1.0, 1.0));
    session.update(&with_alt(from + DVec3::new(0.1, 0.02, 0.01), 1.0, 0.0));
    session.update(&with_alt(from + DVec3::new(0.1, 0.02, 0.01), 0.0, 0.0));
    let free = strokes(&session, layer)[0].clone();
    assert_eq!(free.len(), 22, "a line, not a two-sample freehand stroke");
    assert!((free.points().last().unwrap().pos[1] - 1.02).abs() < 1e-6);

    // With the Line tool, Alt held while pulling keeps the line on a world axis.
    session.tools.select_tool(ToolKind::Line);
    session.update(&with_alt(from, 1.0, 0.0));
    session.update(&with_alt(from + DVec3::new(0.1, 0.02, 0.01), 1.0, 1.0));
    session.update(&with_alt(from + DVec3::new(0.1, 0.02, 0.01), 0.0, 1.0));
    let snapped = strokes(&session, layer)[1].clone();
    let end = snapped.points().last().unwrap().pos;
    assert!(
        (end[0] - 0.1).abs() < 1e-6 && end[1] == 1.0 && end[2] == -0.5,
        "{end:?}"
    );
}

#[test]
fn a_stroke_that_fills_up_continues_in_a_new_one() {
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::Ribbon;
    let start = DVec3::new(0.0, 1.0, -0.5);
    session.update(&hand(start, 1.0));
    for i in 1..=9000 {
        session.update(&hand(start + DVec3::X * (i as f64 * 0.0011), 1.0));
    }
    session.update(&hand(start, 0.0));
    let drawn = strokes(&session, layer);
    assert_eq!(drawn.len(), 2);
    assert_eq!(drawn[0].len(), 8192);
    let gap = drawn[1].points()[0]
        .position()
        .distance(drawn[0].points().last().unwrap().position());
    assert!(
        gap < 0.0012,
        "the second stroke picks up where the first ended ({gap})"
    );
}

// ---- tools that change strokes ----

use crate::session::tools::BlendMode;
use serde_json::json;

/// A session with three parallel ribbons along X at y = 1.0, 1.1 and 1.2, each 20 cm
/// long with 21 points, painted white.
fn three_ribbons() -> (Session, LayerId) {
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.tools.paint.size = 0.005;
    session.tools.color = [1.0, 1.0, 1.0];
    for i in 0..3 {
        let y = 1.0 + i as f64 * 0.1;
        drag(
            &mut session,
            DVec3::new(0.0, y, -0.5),
            DVec3::new(0.2, y, -0.5),
            20,
        );
    }
    assert_eq!(strokes(&session, layer).len(), 3);
    assert_eq!(strokes(&session, layer)[0].len(), 21);
    (session, layer)
}

fn tap(session: &mut Session, at: DVec3) -> Vec<Feedback> {
    drag(session, at, at, 1)
}

#[test]
fn colorize_recolours_what_the_sphere_touches_once_per_gesture() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Colorize);
    session.tools.color = [1.0, 0.0, 0.0];
    session.tools.colorize.size = 0.02;
    session.tools.colorize.inner = 1.0;
    session.tools.colorize.strength = 0.5;
    let at = DVec3::new(0.1, 1.0, -0.5);
    // Hovering in place for several frames is still one touch.
    drag(&mut session, at, at, 10);
    let after_one = strokes(&session, layer);
    assert_eq!(
        after_one[0].points()[10].color,
        [1.0, 0.5, 0.5],
        "half way to red"
    );
    assert_eq!(
        after_one[0].points()[0].color,
        [1.0, 1.0, 1.0],
        "out of reach"
    );
    assert_eq!(
        after_one[1].points()[10].color,
        [1.0, 1.0, 1.0],
        "the next ribbon is out of reach"
    );
    assert_eq!(session.history().undo_label(), Some("Colorize"));
    // A second gesture goes half of the remaining way.
    tap(&mut session, at);
    assert_eq!(
        strokes(&session, layer)[0].points()[10].color,
        [1.0, 0.25, 0.25]
    );
    session.undo();
    assert_eq!(
        strokes(&session, layer)[0].points()[10].color,
        [1.0, 0.5, 0.5]
    );

    // A wave through empty space is not an undo step.
    let depth = session.history().undo_depth();
    tap(&mut session, DVec3::new(5.0, 5.0, 5.0));
    assert_eq!(session.history().undo_depth(), depth);
}

#[test]
fn colorize_fades_with_distance_and_blends_by_mode() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Colorize);
    session.tools.color = [0.0, 0.0, 0.0];
    session.tools.colorize.size = 0.05;
    session.tools.colorize.inner = 0.0;
    tap(&mut session, DVec3::new(0.1, 1.0, -0.5));
    let reds: Vec<f32> = strokes(&session, layer)[0]
        .points()
        .iter()
        .map(|p| p.color[0])
        .collect();
    assert!(reds[10] < 0.05, "black at the centre: {}", reds[10]);
    assert!(
        reds[8] > reds[10] && reds[6] > reds[8],
        "lighter with distance: {reds:?}"
    );
    assert_eq!(reds[4], 1.0, "untouched beyond the sphere");

    session.tools.colorize.mode = BlendMode::Multiply;
    session.tools.colorize.inner = 1.0;
    session.tools.color = [0.2, 0.4, 0.6];
    tap(&mut session, DVec3::new(0.1, 1.1, -0.5));
    let c = strokes(&session, layer)[1].points()[10].color;
    assert!(
        (c[0] - 0.2).abs() < 1e-3 && (c[2] - 0.6).abs() < 1e-3,
        "white times a colour is that colour: {c:?}"
    );
}

#[test]
fn the_eraser_hides_points_and_removes_strokes_erased_end_to_end() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Eraser);
    session.tools.eraser.size = 0.02;
    tap(&mut session, DVec3::new(0.1, 1.0, -0.5));
    let partly = strokes(&session, layer);
    assert_eq!(partly.len(), 3, "a stroke with a hole is still a stroke");
    assert_eq!(partly[0].points()[10].opacity, 0.0);
    assert_eq!(partly[0].points()[0].opacity, 1.0);
    assert_eq!(partly[0].len(), 21, "erasing removes no points");

    // Colorize in Opacity mode at full strength brings the erased part back.
    session.tools.select_tool(ToolKind::Colorize);
    session.tools.colorize.mode = BlendMode::Opacity;
    session.tools.colorize.size = 0.02;
    session.tools.colorize.inner = 1.0;
    tap(&mut session, DVec3::new(0.1, 1.0, -0.5));
    assert_eq!(strokes(&session, layer)[0].points()[10].opacity, 1.0);

    // Sweeping the whole stroke removes it.
    session.tools.select_tool(ToolKind::Eraser);
    drag(
        &mut session,
        DVec3::new(-0.01, 1.1, -0.5),
        DVec3::new(0.21, 1.1, -0.5),
        60,
    );
    assert_eq!(strokes(&session, layer).len(), 2);
    session.undo();
    assert_eq!(strokes(&session, layer).len(), 3);
    assert_eq!(strokes(&session, layer)[1].points()[5].opacity, 1.0);
}

#[test]
fn thicken_widens_over_time_and_thins_with_the_modifier() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Thicken);
    session.tools.thicken.size = 0.03;
    session.tools.thicken.strength = 1.0;
    let at = DVec3::new(0.1, 1.0, -0.5);
    drag(&mut session, at, at, 36);
    let thick = strokes(&session, layer)[0].points()[10].width;
    assert!(
        thick > 0.012 && thick < 0.014,
        "about e times wider after 0.4 s: {thick}"
    );
    assert_eq!(
        strokes(&session, layer)[0].points()[0].width,
        0.005,
        "out of reach"
    );
    assert_eq!(session.history().undo_label(), Some("Thicken strokes"));

    let with_alt = |trigger: f32| InputFrame {
        off: HandInput {
            tracked: true,
            trigger: 1.0,
            ..HandInput::default()
        },
        ..hand(at, trigger)
    };
    for _ in 0..37 {
        session.update(&with_alt(1.0));
    }
    session.update(&with_alt(0.0));
    let thin = strokes(&session, layer)[0].points()[10].width;
    assert!(
        (thin - 0.005).abs() < 3e-4,
        "back to where it started: {thin}"
    );
    assert_eq!(session.history().undo_label(), Some("Thin strokes"));
}

#[test]
fn nudge_pushes_points_along_with_the_hand() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Nudge);
    session.tools.nudge.size = 0.03;
    session.tools.nudge.strength = 1.0;
    drag(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.1, 1.01, -0.5),
        10,
    );
    let nudged = strokes(&session, layer)[0].clone();
    assert!(
        (nudged.points()[10].pos[1] - 1.01).abs() < 1e-4,
        "the point under the hand went along: {}",
        nudged.points()[10].pos[1]
    );
    assert_eq!(nudged.points()[0].pos[1], 1.0);
    assert!(
        nudged.points()[9].pos[1] > 1.0 && nudged.points()[9].pos[1] < nudged.points()[10].pos[1],
        "neighbours follow less"
    );
}

#[test]
fn grab_carries_strokes_rigidly_inside_and_not_at_all_outside() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Grab);
    session.tools.grab.size = 0.04;
    session.tools.grab.inner = 0.5;
    let start = DVec3::new(0.1, 1.0, -0.5);
    session.update(&hand(start, 1.0));
    // Move up and turn a quarter turn about the view axis.
    let turned = InputFrame {
        main: HandInput {
            tracked: true,
            position: start + DVec3::Y * 0.05,
            rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
            trigger: 1.0,
            ..HandInput::default()
        },
        ..InputFrame::default()
    };
    session.update(&turned);
    session.update(&InputFrame {
        main: HandInput {
            trigger: 0.0,
            ..turned.main
        },
        ..turned
    });
    let grabbed = strokes(&session, layer)[0].clone();
    // The point one centimetre to the right of the hand is now one centimetre above where the hand went.
    let p = grabbed.points()[11].position();
    assert!(
        (p - glam::Vec3::new(0.1, 1.06, -0.5)).length() < 1e-5,
        "{p:?}"
    );
    assert_eq!(
        grabbed.points()[0].pos,
        [0.0, 1.0, -0.5],
        "outside the outer sphere nothing moves"
    );
    let between = grabbed.points()[13].position();
    assert!(
        between.y > 1.0 && between.y < 1.08 && between.x > 0.1 && between.x < 0.13,
        "in between, points follow part of the way: {between:?}"
    );
    assert_eq!(
        strokes(&session, layer)[1].points()[10].pos,
        [0.1, 1.1, -0.5]
    );
    assert_eq!(session.history().undo_label(), Some("Reshape strokes"));
}

#[test]
fn optimize_simplifies_the_strokes_it_touches_and_the_command_does_layers() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Optimize);
    session.tools.optimize.size = 0.02;
    tap(&mut session, DVec3::new(0.1, 1.0, -0.5));
    let lengths: Vec<usize> = strokes(&session, layer).iter().map(|s| s.len()).collect();
    assert_eq!(lengths, [2, 21, 21], "only the stroke that was touched");

    let result = session
        .exec_json(&json!({"cmd": "optimize", "scope": "layer"}))
        .unwrap();
    assert_eq!(
        (
            result["points_before"].as_u64(),
            result["points_after"].as_u64()
        ),
        (Some(44), Some(6))
    );
    session.undo();
    assert_eq!(strokes(&session, layer)[1].len(), 21);

    session
        .exec_json(&json!({"cmd": "set_layer", "layer": layer, "locked": true}))
        .unwrap();
    assert!(session
        .exec_json(&json!({"cmd": "optimize", "scope": "layer"}))
        .is_err());
    let all = session
        .exec_json(&json!({"cmd": "optimize", "scope": "all"}))
        .unwrap();
    assert_eq!(all["points_before"], 0, "locked layers are skipped");
}

#[test]
fn the_selection_tool_sweeps_adds_subtracts_and_deselects() {
    let (mut session, _) = three_ribbons();
    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.02;
    let with_alt = |position: DVec3, trigger: f32, alt: f32| InputFrame {
        off: HandInput {
            tracked: true,
            trigger: alt,
            ..HandInput::default()
        },
        ..hand(position, trigger)
    };
    let on = |i: usize| DVec3::new(0.1, 1.0 + i as f64 * 0.1, -0.5);

    tap(&mut session, on(0));
    assert_eq!(session.selection().count(), 1);
    // A new press starts a new selection.
    tap(&mut session, on(1));
    assert_eq!(session.selection().count(), 1);
    // A half press of the modifier adds.
    session.update(&with_alt(on(2), 1.0, 0.5));
    session.update(&with_alt(on(2), 0.0, 0.5));
    assert_eq!(session.selection().count(), 2);
    // A full press subtracts.
    session.update(&with_alt(on(1), 1.0, 1.0));
    session.update(&with_alt(on(1), 0.0, 1.0));
    assert_eq!(session.selection().count(), 1);
    assert_eq!(session.status()["selection_count"], 1);
    assert!(
        !session.history().can_undo() || session.history().undo_label() == Some("Paint stroke"),
        "selecting is not an edit"
    );
    // A press in empty space deselects.
    tap(&mut session, DVec3::new(3.0, 3.0, 3.0));
    assert!(session.selection().is_empty());
}

#[test]
fn with_a_selection_tools_touch_only_the_selected_strokes() {
    let (mut session, layer) = three_ribbons();
    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.02;
    tap(&mut session, DVec3::new(0.1, 1.1, -0.5));
    session.tools.select_tool(ToolKind::Eraser);
    session.tools.eraser.size = 0.3;
    tap(&mut session, DVec3::new(0.1, 1.1, -0.5));
    let left = strokes(&session, layer);
    assert_eq!(
        left.len(),
        2,
        "the selected stroke was erased entirely, the others not at all"
    );
    assert!(left
        .iter()
        .all(|s| s.points().iter().all(|p| p.opacity == 1.0)));
    assert!(
        session.selection().is_empty(),
        "a stroke that is gone is no longer selected"
    );
}

#[test]
fn selection_commands_select_delete_and_move_to_a_new_layer() {
    let (mut session, layer) = three_ribbons();
    assert_eq!(
        session.exec_json(&json!({"cmd": "select_all"})).unwrap()["selected"],
        3
    );
    session.exec_json(&json!({"cmd": "deselect"})).unwrap();
    assert!(
        session
            .exec_json(&json!({"cmd": "delete_selection"}))
            .is_err(),
        "nothing selected"
    );

    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.02;
    tap(&mut session, DVec3::new(0.1, 1.2, -0.5));
    session.exec_json(&json!({"cmd": "set_layer", "layer": layer, "transform": Xform::from_translation(DVec3::new(0.0, 0.0, 2.0))})).unwrap();
    let moved = session
        .exec_json(&json!({"cmd": "selection_to_new_layer"}))
        .unwrap();
    let new_layer = LayerId(moved["layer"].as_u64().unwrap());
    assert_eq!(strokes(&session, layer).len(), 2);
    assert_eq!(strokes(&session, new_layer).len(), 1);
    assert_eq!(session.paint_layer(), Some(new_layer));
    assert_eq!(
        session.doc().layer(new_layer).unwrap().transform,
        session.doc().layer(layer).unwrap().transform,
        "the new layer sits where the old one does"
    );
    assert_eq!(session.doc().layer(new_layer).unwrap().name, "Paint 2");
    assert_eq!(
        session.selection().count(),
        1,
        "the moved strokes stay selected"
    );
    assert_eq!(
        strokes(&session, new_layer)[0].points()[0].pos,
        [0.0, 1.2, -0.5]
    );

    let copied = session
        .exec_json(&json!({"cmd": "selection_to_new_layer", "copy": true}))
        .unwrap();
    let copy_layer = LayerId(copied["layer"].as_u64().unwrap());
    assert_eq!(
        strokes(&session, new_layer).len(),
        1,
        "a copy leaves the original"
    );
    assert_eq!(strokes(&session, copy_layer).len(), 1);

    session
        .exec_json(&json!({"cmd": "delete_selection"}))
        .unwrap();
    assert!(strokes(&session, copy_layer).is_empty());
    session.undo();
    assert_eq!(strokes(&session, copy_layer).len(), 1);
    session.doc().validate().unwrap();
}

#[test]
fn moving_a_selection_across_layers_keeps_strokes_in_place() {
    let (mut session, first) = three_ribbons();
    let second = LayerId(
        session
            .exec_json(&json!({"cmd": "add_layer", "kind": "paint"}))
            .unwrap()["layer"]
            .as_u64()
            .unwrap(),
    );
    session.exec_json(&json!({"cmd": "set_layer", "layer": second, "transform": Xform { scale: 2.0, translation: DVec3::new(1.0, 0.0, 0.0), ..Xform::IDENTITY }})).unwrap();
    session.tools.paint.size = 0.005;
    drag(
        &mut session,
        DVec3::new(0.0, 1.5, -0.5),
        DVec3::new(0.2, 1.5, -0.5),
        20,
    );
    let world_before = session.doc().world_from_layer(second).transform_point3(
        strokes(&session, second)[0].points()[0]
            .position()
            .as_dvec3(),
    );

    session
        .exec_json(&json!({"cmd": "set_options", "values": {"select_in_all_layers": true}}))
        .unwrap();
    session.exec_json(&json!({"cmd": "select_all"})).unwrap();
    assert_eq!(session.selection().count(), 4);
    session
        .exec_json(&json!({"cmd": "select_layers", "layers": [first]}))
        .unwrap();
    let merged = LayerId(
        session
            .exec_json(&json!({"cmd": "selection_to_new_layer"}))
            .unwrap()["layer"]
            .as_u64()
            .unwrap(),
    );
    let gathered = strokes(&session, merged);
    assert_eq!(gathered.len(), 4);
    let from_second = gathered
        .iter()
        .find(|s| (s.points()[0].pos[1] - 1.5).abs() < 1e-4)
        .expect("the stroke from the other layer");
    let world_after = session
        .doc()
        .world_from_layer(merged)
        .transform_point3(from_second.points()[0].position().as_dvec3());
    assert!((world_after - world_before).length() < 1e-5);
    assert!(
        (from_second.points()[5].width - 0.005).abs() < 1e-7,
        "and keeps its size on screen"
    );
}

// ---- buttons and sticks ----

fn frame_with(main: HandInput, off: HandInput) -> InputFrame {
    InputFrame {
        main: HandInput {
            tracked: true,
            ..main
        },
        off: HandInput {
            tracked: true,
            ..off
        },
        ..InputFrame::default()
    }
}

#[test]
fn the_lower_button_deletes_the_nearest_stroke_or_the_selection() {
    let (mut session, layer) = three_ribbons();
    let at = HandInput {
        position: DVec3::new(0.1, 1.005, -0.5),
        ..HandInput::default()
    };
    session.update(&frame_with(
        HandInput {
            lower_button: true,
            ..at
        },
        HandInput::default(),
    ));
    session.update(&frame_with(
        HandInput {
            lower_button: true,
            ..at
        },
        HandInput::default(),
    ));
    assert_eq!(
        strokes(&session, layer).len(),
        2,
        "one press deletes one stroke, however long it is held"
    );
    assert!((strokes(&session, layer)[0].points()[0].pos[1] - 1.1).abs() < 1e-5);
    session.update(&frame_with(at, HandInput::default()));
    // Far from any stroke nothing happens.
    session.update(&frame_with(
        HandInput {
            lower_button: true,
            position: DVec3::new(0.1, 3.0, -0.5),
            ..at
        },
        HandInput::default(),
    ));
    assert_eq!(strokes(&session, layer).len(), 2);
    session.update(&frame_with(at, HandInput::default()));

    session.exec_json(&json!({"cmd": "select_all"})).unwrap();
    let feedback = session.update(&frame_with(
        HandInput {
            lower_button: true,
            position: DVec3::new(9.0, 9.0, 9.0),
            ..at
        },
        HandInput::default(),
    ));
    assert!(
        strokes(&session, layer).is_empty(),
        "with a selection the button deletes it, wherever the hand is"
    );
    assert!(feedback.contains(&Feedback::Sound("delete")));
    session.undo();
    assert_eq!(strokes(&session, layer).len(), 2);
}

#[test]
fn the_upper_button_picks_the_colour_under_the_hand() {
    let (mut session, _) = three_ribbons();
    session.tools.color = [0.1, 0.7, 0.3];
    session.tools.paint.brush = BrushStyle::Ribbon;
    drag(
        &mut session,
        DVec3::new(0.0, 1.5, -0.5),
        DVec3::new(0.2, 1.5, -0.5),
        10,
    );
    session.tools.color = [0.0, 0.0, 0.0];
    let feedback = session.update(&frame_with(
        HandInput {
            upper_button: true,
            position: DVec3::new(0.05, 1.5, -0.5),
            ..HandInput::default()
        },
        HandInput::default(),
    ));
    assert_eq!(session.tools.color, [0.1, 0.7, 0.3]);
    assert!(feedback.contains(&Feedback::Sound("pick")));
}

#[test]
fn the_drawing_hand_stick_sizes_the_tool_and_steps_through_history() {
    let (mut session, layer) = three_ribbons();
    let push = |x: f32, y: f32| {
        frame_with(
            HandInput {
                stick: [x, y],
                ..HandInput::default()
            },
            HandInput::default(),
        )
    };
    let size = session.tools.paint.size;
    for _ in 0..54 {
        session.update(&push(0.0, 1.0));
    }
    let grown = session.tools.paint.size;
    assert!(
        (grown / size - std::f32::consts::E).abs() < 0.05,
        "e times bigger after 0.6 s: {}",
        grown / size
    );

    // A flick left undoes once; holding it repeats after a moment.
    let undone = session.update(&push(-1.0, 0.0));
    assert!(undone.contains(&Feedback::Sound("undo")));
    assert_eq!(strokes(&session, layer).len(), 2);
    for _ in 0..30 {
        session.update(&push(-1.0, 0.0));
    }
    assert_eq!(strokes(&session, layer).len(), 2, "not yet repeating");
    for _ in 0..12 {
        session.update(&push(-1.0, 0.0));
    }
    assert!(
        strokes(&session, layer).len() < 2,
        "holding the stick repeats"
    );
    session.update(&push(0.0, 0.0));
    session.update(&push(1.0, 0.0));
    session.update(&push(0.0, 0.0));
    session.update(&push(1.0, 0.0));
    assert!(session.history().can_undo());
}

#[test]
fn the_other_hand_switches_to_selection_steps_frames_and_plays() {
    let (mut session, _) = three_ribbons();
    session.tools.select_tool(ToolKind::Nudge);
    let off = |hand: HandInput| frame_with(HandInput::default(), hand);
    session.update(&off(HandInput {
        lower_button: true,
        ..HandInput::default()
    }));
    assert_eq!(session.tools.tool, ToolKind::Select);
    session.update(&off(HandInput::default()));
    session.update(&off(HandInput {
        lower_button: true,
        ..HandInput::default()
    }));
    assert_eq!(
        session.tools.tool,
        ToolKind::Nudge,
        "and back to the tool in use"
    );
    session.update(&off(HandInput::default()));

    session.update(&off(HandInput {
        stick: [1.0, 0.0],
        ..HandInput::default()
    }));
    session.update(&off(HandInput::default()));
    session.update(&off(HandInput {
        stick: [1.0, 0.0],
        ..HandInput::default()
    }));
    assert_eq!(session.status()["frame"], 2);
    session.update(&off(HandInput::default()));
    session.update(&off(HandInput {
        stick: [-1.0, 0.1],
        ..HandInput::default()
    }));
    assert_eq!(session.status()["frame"], 1);
    session.update(&off(HandInput::default()));

    let menu = session.update(&off(HandInput {
        upper_button: true,
        ..HandInput::default()
    }));
    assert_eq!(menu, [Feedback::Menu]);
    session.update(&off(HandInput {
        stick_click: true,
        ..HandInput::default()
    }));
    assert!(session.playback.playing);
}

#[test]
fn mirrored_working_paints_and_edits_on_both_sides_of_the_pivot_plane() {
    use crate::session::tools::Symmetry;
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.tools.paint.size = 0.005;
    session.tools.color = [1.0, 1.0, 1.0];
    assert!(session.mirror_plane().is_none(), "off until asked for");
    session.tools.symmetry = Symmetry::X;
    // The mirror stands at the layer's pivot: here half a metre to the right.
    session
        .exec_json(&serde_json::json!({
            "cmd": "set_layer", "layer": layer,
            "pivot": Xform::from_translation(DVec3::new(0.5, 0.0, 0.0)),
        }))
        .unwrap();
    let (on_plane, turn) = session.mirror_plane().expect("a mirror");
    assert!((on_plane - DVec3::new(0.5, 0.0, 0.0)).length() < 1e-12);
    assert!((turn * DVec3::Z - DVec3::X).length() < 1e-12, "square to X");

    drag(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.3, 1.0, -0.5),
        20,
    );
    let drawn = strokes(&session, layer);
    assert_eq!(drawn.len(), 2, "the stroke and its twin");
    assert_eq!(drawn[0].len(), drawn[1].len());
    for (p, q) in drawn[0].points().iter().zip(drawn[1].points()) {
        assert!((q.pos[0] - (1.0 - p.pos[0])).abs() < 1e-6, "mirrored in x");
        assert_eq!((q.pos[1], q.pos[2], q.width), (p.pos[1], p.pos[2], p.width));
        assert_eq!(q.normal, [-p.normal[0], p.normal[1], p.normal[2]]);
    }
    assert_eq!(
        session.history().undo_depth(),
        2,
        "the pivot, then one stroke"
    );
    session.undo();
    assert!(strokes(&session, layer).is_empty(), "one undo takes both");
    session.redo();

    // A line gets a twin as well.
    session.tools.select_tool(ToolKind::Line);
    drag(
        &mut session,
        DVec3::new(0.1, 1.2, -0.5),
        DVec3::new(0.3, 1.3, -0.5),
        4,
    );
    let drawn = strokes(&session, layer);
    assert_eq!(drawn.len(), 4);
    let (line, twin) = (&drawn[2], &drawn[3]);
    let (end, twin_end) = (line.points().last().unwrap(), twin.points().last().unwrap());
    assert!((twin_end.pos[0] - (1.0 - end.pos[0])).abs() < 1e-6);
    assert!((twin_end.pos[1] - end.pos[1]).abs() < 1e-6);

    // Colour laid on one side lands on the other too, and so does the eraser.
    session.tools.select_tool(ToolKind::Colorize);
    session.tools.color = [1.0, 0.0, 0.0];
    session.tools.colorize.size = 0.02;
    session.tools.colorize.inner = 1.0;
    session.tools.colorize.strength = 1.0;
    let at = DVec3::new(0.2, 1.0, -0.5);
    drag(&mut session, at, at, 1);
    let drawn = strokes(&session, layer);
    assert_eq!(drawn[0].points()[10].color, [1.0, 0.0, 0.0]);
    assert_eq!(
        drawn[1].points()[10].color,
        [1.0, 0.0, 0.0],
        "the twin took the colour"
    );
    assert_eq!(drawn[0].points()[0].color, [1.0, 1.0, 1.0]);

    session.tools.select_tool(ToolKind::Eraser);
    session.tools.eraser.size = 0.4;
    session.tools.eraser.strength = 1.0;
    // Over the strokes on the near side, and too far from the twins to reach them:
    // those are erased by the eraser's own reflection.
    let at = DVec3::new(0.2, 1.1, -0.5);
    drag(&mut session, at, at, 1);
    assert!(
        strokes(&session, layer).is_empty(),
        "both sides erased from one side"
    );

    // With mirroring off again a stroke is alone.
    session.tools.symmetry = Symmetry::Off;
    session.tools.select_tool(ToolKind::Paint);
    drag(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.3, 1.0, -0.5),
        5,
    );
    assert_eq!(strokes(&session, layer).len(), 1);
}

/// A session with mirrored working on X across a pivot half a metre to the right, so
/// the mirror is the plane x = 0.5, painting white 5 mm ribbons.
fn mirrored_session() -> (Session, LayerId) {
    use crate::session::tools::Symmetry;
    let (mut session, layer) = session();
    session.tools.paint.brush = BrushStyle::Ribbon;
    session.tools.paint.size = 0.005;
    session.tools.color = [1.0, 1.0, 1.0];
    session
        .exec_json(&serde_json::json!({
            "cmd": "set_layer", "layer": layer,
            "pivot": Xform::from_translation(DVec3::new(0.5, 0.0, 0.0)),
        }))
        .unwrap();
    session.tools.symmetry = Symmetry::X;
    (session, layer)
}

/// Asserts that `twin` is `stroke` reflected in the plane x = 0.5.
fn assert_mirrored(stroke: &Stroke, twin: &Stroke) {
    assert_eq!(stroke.len(), twin.len());
    for (p, q) in stroke.points().iter().zip(twin.points()) {
        assert!(
            (q.pos[0] - (1.0 - p.pos[0])).abs() < 1e-5
                && (q.pos[1] - p.pos[1]).abs() < 1e-5
                && (q.pos[2] - p.pos[2]).abs() < 1e-5,
            "{:?} is not the mirror image of {:?}",
            q.pos,
            p.pos
        );
        assert!((q.normal[0] + p.normal[0]).abs() < 1e-5);
        assert!((q.normal[1] - p.normal[1]).abs() < 1e-5);
        assert!((q.width - p.width).abs() < 1e-7);
    }
}

#[test]
fn mirrored_selection_takes_the_twin_too() {
    use crate::session::tools::Symmetry;
    let (mut session, layer) = mirrored_session();
    for y in [1.0, 1.1] {
        drag(
            &mut session,
            DVec3::new(0.1, y, -0.5),
            DVec3::new(0.3, y, -0.5),
            20,
        );
    }
    assert_eq!(strokes(&session, layer).len(), 4, "two strokes, two twins");
    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.02;
    let depth = session.history().undo_depth();

    // Touching the first stroke on the near side selects its twin on the far side.
    tap(&mut session, DVec3::new(0.2, 1.0, -0.5));
    let drawing = {
        let paint = session.doc().layer(layer).unwrap().as_paint().unwrap();
        paint.drawings[paint.frames[0] as usize]
    };
    let ids: Vec<u32> = strokes(&session, layer).iter().map(|s| s.id).collect();
    assert_eq!(session.selection().count(), 2);
    assert!(session.selection().contains(drawing, ids[0]));
    assert!(session.selection().contains(drawing, ids[1]), "the twin");

    // The same from the far side, adding with a half press of the modifier.
    let with_alt = |position: DVec3, trigger: f32, alt: f32| InputFrame {
        off: HandInput {
            tracked: true,
            trigger: alt,
            ..HandInput::default()
        },
        ..hand(position, trigger)
    };
    let far = DVec3::new(0.8, 1.1, -0.5);
    session.update(&with_alt(far, 1.0, 0.5));
    session.update(&with_alt(far, 0.0, 0.5));
    assert_eq!(session.selection().count(), 4);

    // A full press of the modifier takes a stroke and its twin out again.
    let near = DVec3::new(0.2, 1.0, -0.5);
    session.update(&with_alt(near, 1.0, 1.0));
    session.update(&with_alt(near, 0.0, 1.0));
    assert_eq!(session.selection().count(), 2);
    assert!(!session.selection().contains(drawing, ids[0]));
    assert!(!session.selection().contains(drawing, ids[1]));
    assert_eq!(
        session.history().undo_depth(),
        depth,
        "selecting is not an edit"
    );

    // With the mirror off the sphere reaches only what it touches.
    session.tools.symmetry = Symmetry::Off;
    tap(&mut session, near);
    assert_eq!(session.selection().count(), 1);
}

#[test]
fn mirrored_grab_moves_the_twin_by_the_mirrored_motion() {
    let (mut session, layer) = mirrored_session();
    drag(
        &mut session,
        DVec3::new(0.1, 1.0, -0.5),
        DVec3::new(0.3, 1.0, -0.5),
        20,
    );
    let before = strokes(&session, layer);
    assert_eq!(before.len(), 2);
    session.tools.select_tool(ToolKind::Grab);
    session.tools.grab.size = 0.04;
    session.tools.grab.inner = 0.5;
    session.tools.grab.strength = 1.0;
    let depth = session.history().undo_depth();

    // Take hold near the middle of the stroke, then move toward the mirror, up, and
    // turn a quarter turn about the view axis.
    let start = DVec3::new(0.2, 1.0, -0.5);
    session.update(&hand(start, 1.0));
    let turned = InputFrame {
        main: HandInput {
            tracked: true,
            position: start + DVec3::new(0.03, 0.05, 0.0),
            rotation: DQuat::from_rotation_z(std::f64::consts::FRAC_PI_2),
            trigger: 1.0,
            ..HandInput::default()
        },
        ..InputFrame::default()
    };
    session.update(&turned);
    session.update(&InputFrame {
        main: HandInput {
            trigger: 0.0,
            ..turned.main
        },
        ..turned
    });

    let after = strokes(&session, layer);
    let p = after[0].points()[11].position();
    assert!(
        (p - glam::Vec3::new(0.23, 1.06, -0.5)).length() < 1e-5,
        "the near side follows the hand: {p:?}"
    );
    assert_ne!(after[1].points()[11].pos, before[1].points()[11].pos);
    assert_mirrored(&after[0], &after[1]);
    assert_eq!(
        after[1].points()[0].pos,
        before[1].points()[0].pos,
        "outside the mirrored sphere nothing moves"
    );

    assert_eq!(session.history().undo_depth(), depth + 1, "one undo step");
    session.undo();
    let undone = strokes(&session, layer);
    assert_eq!(undone[0].points(), before[0].points());
    assert_eq!(
        undone[1].points(),
        before[1].points(),
        "one undo puts both back"
    );
}

#[test]
fn mirrored_grab_moves_each_point_once_and_keeps_the_mirror_seam() {
    use crate::session::tools::Symmetry;
    // A stroke across the mirror, painted with the mirror off so it has no twin:
    // 21 points from x = 0.4 to 0.6, the middle one on the mirror.
    let (mut session, layer) = mirrored_session();
    session.tools.symmetry = Symmetry::Off;
    drag(
        &mut session,
        DVec3::new(0.4, 1.0, -0.5),
        DVec3::new(0.6, 1.0, -0.5),
        20,
    );
    assert_eq!(strokes(&session, layer).len(), 1);
    session.tools.symmetry = Symmetry::X;
    session.tools.select_tool(ToolKind::Grab);
    session.tools.grab.size = 0.04;
    session.tools.grab.inner = 0.5;
    session.tools.grab.strength = 1.0;

    // The hand 2 cm short of the mirror, its reflection 2 cm past it: the points
    // between are within the inner sphere of both. Each moves with the nearer one;
    // the point on the mirror takes the average of the two motions.
    let start = DVec3::new(0.48, 1.0, -0.5);
    let moved = start + DVec3::new(0.01, 0.02, 0.0);
    session.update(&hand(start, 1.0));
    session.update(&hand(moved, 1.0));
    session.update(&hand(moved, 0.0));
    let grabbed = strokes(&session, layer)[0].clone();
    let at = |i: usize| grabbed.points()[i].position();
    let expect = [
        (8, glam::Vec3::new(0.49, 1.02, -0.5)),
        (9, glam::Vec3::new(0.50, 1.02, -0.5)),
        (10, glam::Vec3::new(0.50, 1.02, -0.5)),
        (11, glam::Vec3::new(0.50, 1.02, -0.5)),
        (12, glam::Vec3::new(0.51, 1.02, -0.5)),
    ];
    for (i, want) in expect {
        assert!(
            (at(i) - want).length() < 1e-5,
            "point {i} went to {:?}, not {want:?}",
            at(i)
        );
    }
    // The result is as symmetric as the gesture.
    for i in 0..=10 {
        let (p, q) = (at(i), at(20 - i));
        assert!((q.x - (1.0 - p.x)).abs() < 1e-5 && (q.y - p.y).abs() < 1e-5);
    }

    // However the hand turns, the point on the mirror stays on it.
    let start = DVec3::new(0.49, 1.02, -0.5);
    session.update(&hand(start, 1.0));
    let turned = InputFrame {
        main: HandInput {
            tracked: true,
            position: start + DVec3::new(-0.01, 0.01, 0.005),
            rotation: DQuat::from_rotation_y(0.4) * DQuat::from_rotation_z(0.3),
            trigger: 1.0,
            ..HandInput::default()
        },
        ..InputFrame::default()
    };
    session.update(&turned);
    session.update(&InputFrame {
        main: HandInput {
            trigger: 0.0,
            ..turned.main
        },
        ..turned
    });
    let middle = strokes(&session, layer)[0].points()[10].position();
    assert!(
        (middle.x - 0.5).abs() < 1e-6,
        "on the mirror still: {middle:?}"
    );
    assert!(
        (middle - glam::Vec3::new(0.5, 1.02, -0.5)).length() > 1e-3,
        "but moved"
    );
}
