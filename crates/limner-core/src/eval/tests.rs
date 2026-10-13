use super::*;
use crate::math::Flip;
use crate::model::{Drawing, Group, Key, Paint, Shading};
use crate::time::FrameRate;
use glam::{DQuat, DVec3};

/// A document builder for evaluation tests. Times in these tests are small numbers
/// of flicks unless a frame rate is involved.
struct Scene {
    doc: Document,
}

impl Scene {
    fn new() -> Self {
        Self {
            doc: Document::empty(),
        }
    }

    fn group(&mut self, parent: LayerId, name: &str) -> LayerId {
        let layer = self
            .doc
            .new_layer(name, LayerKind::Group(Group::default()), 0);
        self.doc.insert_layer(layer, parent, usize::MAX).unwrap()
    }

    fn sequence(
        &mut self,
        parent: LayerId,
        name: &str,
        loop_duration: Flicks,
        max_repeat: u32,
    ) -> LayerId {
        let kind = LayerKind::Group(Group {
            children: Vec::new(),
            sequence: Some(Sequence {
                loop_duration,
                max_repeat,
            }),
        });
        let layer = self.doc.new_layer(name, kind, 0);
        self.doc.insert_layer(layer, parent, usize::MAX).unwrap()
    }

    /// A paint layer with `frames` distinct drawings, one per frame, at 24 fps.
    fn paint(&mut self, parent: LayerId, name: &str, frames: u32, max_repeat: u32) -> LayerId {
        let drawings: Vec<DrawingId> = (0..frames)
            .map(|_| self.doc.add_drawing(Drawing::new()))
            .collect();
        let paint = Paint {
            rate: FrameRate::whole(24),
            drawings,
            frames: (0..frames).collect(),
            max_repeat,
            shading: Shading::Unlit,
        };
        let layer = self.doc.new_layer(name, LayerKind::Paint(paint), 0);
        self.doc.insert_layer(layer, parent, usize::MAX).unwrap()
    }

    fn clips(&mut self, id: LayerId, keys: &[(Flicks, bool)]) {
        let layer = self.doc.layer_mut(id).unwrap();
        layer.anim.visibility = Track::from_keys(keys.iter().map(|(t, v)| Key::step(*t, *v)));
        layer.anim.offset.clear();
    }

    fn at(&self, time: Flicks) -> RenderFrame {
        evaluate(&self.doc, Clock::at(time))
    }
}

fn frame_of(frame: &RenderFrame, id: LayerId) -> u32 {
    match frame.get(id).unwrap().content {
        Content::Paint { frame, .. } => frame,
        Content::None => panic!("not a paint layer"),
    }
}

const FRAME: Flicks = 29_400_000; // one frame at 24 fps

#[test]
fn clips_switch_a_layer_on_and_off() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let layer = s.paint(root, "P", 1, 0);
    s.clips(layer, &[(10, true), (20, false), (30, true)]);

    for (time, visible, local) in [
        (0, false, 0),
        (9, false, 0),
        (10, true, 0),
        (15, true, 5),
        (19, true, 9),
        (20, false, 0),
        (29, false, 0),
        (30, true, 0),
        (500, true, 470),
    ] {
        let frame = s.at(time);
        let state = frame.get(layer).unwrap();
        assert_eq!(state.visible, visible, "time {time}");
        assert_eq!(state.in_clip, visible, "time {time}");
        if visible {
            assert_eq!(state.local_time, local, "time {time}");
        }
    }
}

#[test]
fn a_trim_offsets_the_content_of_its_clip() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let layer = s.paint(root, "P", 1, 0);
    s.clips(layer, &[(10, true), (20, false), (30, true)]);
    s.doc.layer_mut(layer).unwrap().anim.offset =
        Track::from_keys([Key::step(10, 0), Key::step(30, 100)]);
    assert_eq!(s.at(15).get(layer).unwrap().local_time, 5);
    assert_eq!(s.at(35).get(layer).unwrap().local_time, 105);
}

#[test]
fn a_layer_without_visibility_keys_is_always_present() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let layer = s.paint(root, "P", 1, 0);
    s.clips(layer, &[]);
    for time in [0, 7, 1_000_000] {
        let frame = s.at(time);
        assert!(frame.get(layer).unwrap().visible);
        assert_eq!(frame.get(layer).unwrap().local_time, time);
    }
}

#[test]
fn eye_toggles_and_clips_of_ancestors_hide_descendants() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let outer = s.group(root, "Outer");
    let inner = s.group(outer, "Inner");
    let leaf = s.paint(inner, "Leaf", 1, 0);
    assert!(s.at(0).get(leaf).unwrap().visible);

    s.doc.layer_mut(outer).unwrap().visible = false;
    let frame = s.at(0);
    assert!(!frame.get(inner).unwrap().visible);
    assert!(!frame.get(leaf).unwrap().visible);
    assert!(
        frame.get(leaf).unwrap().in_clip,
        "the leaf's own clip is still running"
    );

    s.doc.layer_mut(outer).unwrap().visible = true;
    s.clips(outer, &[(0, true), (50, false)]);
    assert!(s.at(49).get(leaf).unwrap().visible);
    assert!(!s.at(50).get(leaf).unwrap().visible);
}

#[test]
fn plain_groups_do_not_restart_time_but_sequences_do() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let group = s.group(root, "Group");
    let in_group = s.paint(group, "InGroup", 1, 0);
    let sequence = s.sequence(root, "Sequence", 0, 0);
    let in_sequence = s.paint(sequence, "InSequence", 1, 0);
    s.clips(group, &[(100, true)]);
    s.clips(sequence, &[(100, true)]);
    s.clips(in_group, &[(110, true)]);
    s.clips(in_sequence, &[(10, true)]);

    let frame = s.at(115);
    // The group's child reads its keys at root time 115: its clip began at 110.
    assert_eq!(frame.get(in_group).unwrap().timeline_time, 115);
    assert_eq!(frame.get(in_group).unwrap().local_time, 5);
    // The sequence began at 100, so its children are at 15: the child's clip began at 10.
    assert_eq!(frame.get(sequence).unwrap().local_time, 15);
    assert_eq!(frame.get(in_sequence).unwrap().timeline_time, 15);
    assert_eq!(frame.get(in_sequence).unwrap().local_time, 5);
    assert!(frame.get(in_sequence).unwrap().visible);
    // Before the child's clip inside the sequence starts, it is hidden.
    assert!(!s.at(105).get(in_sequence).unwrap().visible);
}

#[test]
fn sequences_loop_at_their_loop_point() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let sequence = s.sequence(root, "Loop", 40, 0);
    let child = s.paint(sequence, "Child", 1, 0);
    s.clips(sequence, &[(100, true)]);
    s.clips(child, &[(10, true), (30, false)]);

    for (root_time, child_time, visible) in [
        (100, 0, false),
        (112, 12, true),
        (135, 35, false),
        (140, 0, false),
        (152, 12, true),
        (100 + 40 * 1000 + 15, 15, true),
    ] {
        let frame = s.at(root_time);
        assert_eq!(
            frame.get(child).unwrap().timeline_time,
            child_time,
            "root time {root_time}"
        );
        assert_eq!(
            frame.get(child).unwrap().visible,
            visible,
            "root time {root_time}"
        );
    }
}

#[test]
fn a_repeat_count_holds_the_sequence_at_its_end() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let sequence = s.sequence(root, "Twice", 40, 2);
    let child = s.paint(sequence, "Child", 1, 0);
    assert_eq!(s.at(35).get(child).unwrap().timeline_time, 35);
    assert_eq!(
        s.at(75).get(child).unwrap().timeline_time,
        35,
        "second play"
    );
    assert_eq!(
        s.at(80).get(child).unwrap().timeline_time,
        40,
        "held at the loop point"
    );
    assert_eq!(s.at(5_000).get(child).unwrap().timeline_time, 40);
}

#[test]
fn nested_sequences_restart_when_their_parent_loops() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let outer = s.sequence(root, "Outer", 100, 0);
    let inner = s.sequence(outer, "Inner", 0, 0);
    let leaf = s.paint(inner, "Leaf", 1, 0);
    s.clips(inner, &[(20, true)]);
    assert_eq!(s.at(50).get(leaf).unwrap().timeline_time, 30);
    // One loop of the outer sequence later the inner sequence has started again.
    assert_eq!(s.at(150).get(leaf).unwrap().timeline_time, 30);
    assert!(
        !s.at(110).get(leaf).unwrap().visible,
        "before the inner clip in the second pass"
    );
}

#[test]
fn paint_frames_play_from_the_start_of_the_clip() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let looping = s.paint(root, "Looping", 3, 0);
    let once = s.paint(root, "Once", 3, 1);
    s.clips(looping, &[(10 * FRAME, true)]);
    s.clips(once, &[(10 * FRAME, true)]);

    let frame = s.at(10 * FRAME);
    assert_eq!((frame_of(&frame, looping), frame_of(&frame, once)), (0, 0));
    let frame = s.at(11 * FRAME + FRAME / 2);
    assert_eq!((frame_of(&frame, looping), frame_of(&frame, once)), (1, 1));
    let frame = s.at(13 * FRAME);
    assert_eq!((frame_of(&frame, looping), frame_of(&frame, once)), (0, 2));
    let frame = s.at(17 * FRAME);
    assert_eq!((frame_of(&frame, looping), frame_of(&frame, once)), (1, 2));

    // The drawing reported is the one behind the frame.
    let paint = s.doc.layer(looping).unwrap().as_paint().unwrap().clone();
    match s.at(12 * FRAME).get(looping).unwrap().content {
        Content::Paint { frame, drawing } => {
            assert_eq!(frame, 2);
            assert_eq!(drawing, paint.drawings[2]);
        }
        Content::None => panic!("expected a frame"),
    }
}

#[test]
fn paint_frames_inside_a_looping_sequence_restart_with_it() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let sequence = s.sequence(root, "Loop", 4 * FRAME, 0);
    let layer = s.paint(sequence, "P", 8, 1);
    for (root_frame, shown) in [(0, 0), (3, 3), (4, 0), (6, 2), (403, 3)] {
        assert_eq!(
            frame_of(&s.at(root_frame * FRAME), layer),
            shown,
            "root frame {root_frame}"
        );
    }
}

#[test]
fn keyed_values_interpolate_by_the_earlier_keys_mode() {
    let track = Track::from_keys([
        Key::new(100, 0.0f32, Interp::Linear),
        Key::new(200, 1.0, Interp::EaseIn),
        Key::new(300, 3.0, Interp::Step),
        Key::new(400, 5.0, Interp::Linear),
    ]);
    let mix = |a: &f32, b: &f32, t: f64| a + (b - a) * t as f32;
    let at = |time| eval_track(&track, time, mix).unwrap();
    assert_eq!(at(0), 0.0, "before the first key the first value holds");
    assert_eq!(at(125), 0.25);
    assert_eq!(at(200), 1.0);
    assert_eq!(
        at(250),
        1.0 + 2.0 * 0.125,
        "ease in is the cube of the parameter"
    );
    assert_eq!(at(350), 3.0, "a stepped key holds");
    assert_eq!(at(400), 5.0);
    assert_eq!(at(9_999), 5.0, "after the last key the last value holds");
    assert_eq!(eval_track(&Track::<f32>::new(), 5, mix), None);
}

#[test]
fn opacity_multiplies_down_the_tree_and_keys_override_the_static_value() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let group = s.group(root, "G");
    let leaf = s.paint(group, "Leaf", 1, 0);
    s.doc.layer_mut(group).unwrap().opacity = 0.5;
    s.doc.layer_mut(leaf).unwrap().opacity = 0.9;
    s.doc.layer_mut(leaf).unwrap().anim.opacity = Track::from_keys([
        Key::new(0, 0.0, Interp::Linear),
        Key::new(100, 1.0, Interp::Step),
    ]);
    let frame = s.at(50);
    let state = frame.get(leaf).unwrap();
    assert_eq!(state.own_opacity, 0.5);
    assert_eq!(state.opacity, 0.25);
    assert_eq!(s.at(200).get(leaf).unwrap().opacity, 0.5);
}

#[test]
fn transforms_compose_down_the_tree() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let group = s.group(root, "G");
    let leaf = s.paint(group, "Leaf", 1, 0);
    s.doc.layer_mut(group).unwrap().transform = Xform {
        rotation: DQuat::from_rotation_z(0.5),
        scale: 3.0,
        translation: DVec3::new(1.0, 0.0, -2.0),
        flip: Flip::Y,
    };
    s.doc.layer_mut(leaf).unwrap().transform = Xform::from_translation(DVec3::new(0.0, 4.0, 0.0));
    let frame = s.at(0);
    let p = DVec3::new(0.25, -1.0, 2.0);
    assert!(
        (frame.get(leaf).unwrap().world.transform_point3(p)
            - s.doc.world_from_layer(leaf).transform_point3(p))
        .length()
            < 1e-12
    );
}

#[test]
fn keyed_transforms_turn_about_the_pivot() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let layer = s.paint(root, "P", 1, 0);
    let a = Xform::IDENTITY;
    let b = Xform::from_translation(DVec3::new(0.0, 0.0, 3.0))
        .mul(&Xform::from_rotation(DQuat::from_rotation_y(1.2)));
    {
        let layer = s.doc.layer_mut(layer).unwrap();
        layer.pivot = Xform::from_translation(DVec3::X);
        layer.transform = Xform::from_translation(DVec3::splat(99.0));
        layer.anim.transform = Track::from_keys([
            Key::new(0, a, Interp::Linear),
            Key::new(100, b, Interp::Step),
        ]);
    }
    let pivot_start = a.transform_point(DVec3::X);
    let pivot_end = b.transform_point(DVec3::X);
    let frame = s.at(50);
    let state = frame.get(layer).unwrap();
    assert!(
        (state.local.transform_point(DVec3::X) - pivot_start.lerp(pivot_end, 0.5)).length() < 1e-12
    );
    // Keys win over the static transform, and the last key holds afterwards.
    assert!(
        (s.at(1_000)
            .get(layer)
            .unwrap()
            .local
            .transform_point(DVec3::ZERO)
            - b.transform_point(DVec3::ZERO))
        .length()
            < 1e-12
    );
}

#[test]
fn held_time_runs_sequences_but_not_the_root_timeline() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let direct = s.paint(root, "Direct", 100, 0);
    let group = s.group(root, "Group");
    let sequence = s.sequence(group, "Sequence", 0, 0);
    let in_sequence = s.paint(sequence, "InSequence", 100, 0);
    let nested = s.sequence(sequence, "Nested", 0, 0);
    let in_nested = s.paint(nested, "InNested", 100, 0);

    let frame = evaluate(
        &s.doc,
        Clock {
            time: 10 * FRAME,
            held: 5 * FRAME,
        },
    );
    assert_eq!(
        frame_of(&frame, direct),
        10,
        "layers on the root timeline are frozen at the stop"
    );
    assert_eq!(
        frame_of(&frame, in_sequence),
        15,
        "a sequence keeps running"
    );
    assert_eq!(
        frame_of(&frame, in_nested),
        15,
        "held time is counted once, not per nesting level"
    );
}

#[test]
fn every_layer_is_reported_parents_first() {
    let mut s = Scene::new();
    let root = s.doc.root();
    let a = s.group(root, "A");
    let b = s.paint(a, "B", 1, 0);
    let c = s.paint(root, "C", 1, 0);
    let frame = s.at(0);
    let order: Vec<LayerId> = frame.layers.iter().map(|l| l.id).collect();
    assert_eq!(order, vec![root, a, b, c]);
    assert_eq!(frame.clock, Clock::at(0));
    assert!(frame.get(LayerId(9_999)).is_none());
}
