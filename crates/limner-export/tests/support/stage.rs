//! Documents shared by the format tests: a mirrored chain of layers, and a small
//! stage with one of everything a writer has to handle.

use super::{frames, group, paint, sequence, straight, straight_colored, FRAME};
use limner_core::glam::{DQuat, DVec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Brush, Camera, Document, Interp, Key, LayerId, LayerKind, Light, LightKind, Shading, Stroke,
    Track,
};

/// Root > A > B > P, each with its own rotation, scale, translation and flip.
pub fn chain(flips: [Flip; 3]) -> (Document, LayerId, Vec<Stroke>) {
    let mut doc = Document::empty();
    let root = doc.root();
    let a = group(&mut doc, root, "A");
    let b = group(&mut doc, a, "B");
    let strokes = vec![
        straight(0, Brush::Cylinder, 4, [0.3, 0.2, -0.1]),
        straight(1, Brush::Ribbon, 4, [-0.2, 0.5, 0.4]),
    ];
    let p = paint(&mut doc, b, "P", strokes.clone());
    doc.layer_mut(a).unwrap().transform = Xform {
        rotation: DQuat::from_rotation_y(0.7),
        scale: 2.0,
        translation: DVec3::new(1.0, 2.0, 3.0),
        flip: flips[0],
    };
    doc.layer_mut(b).unwrap().transform = Xform {
        rotation: DQuat::from_rotation_x(-0.3) * DQuat::from_rotation_z(0.4),
        scale: 0.5,
        translation: DVec3::new(-4.0, 0.0, 1.0),
        flip: flips[1],
    };
    doc.layer_mut(p).unwrap().transform = Xform {
        rotation: DQuat::from_axis_angle(DVec3::new(1.0, -2.0, 0.5).normalize(), 2.1),
        scale: 1.5,
        translation: DVec3::new(0.2, -0.1, 0.3),
        flip: flips[2],
    };
    (doc, p, strokes)
}

pub const FLIP_CASES: [[Flip; 3]; 5] = [
    [Flip::None, Flip::None, Flip::None],
    [Flip::X, Flip::None, Flip::None],
    [Flip::None, Flip::None, Flip::Y],
    [Flip::X, Flip::Z, Flip::None],
    [Flip::Y, Flip::Z, Flip::X],
];

/// The last frame of the stage's range: frames 0 to 12 at 24 fps.
pub const STAGE_LAST: i64 = 12;

/// The first and last frame on which the stage's "Clip" layer is shown.
pub const CLIP_SHOWN: (i64, i64) = (4, 8);

pub struct Stage {
    pub doc: Document,
    /// A group mirrored on X, rotated and scaled.
    pub rig: LayerId,
    /// "Left arm", inside the rig: an opaque tube and a see-through ribbon, with
    /// transform keys that turn it through more than half a turn.
    pub arm: LayerId,
    /// "Clip": rotated, shown on frames 4 to 8 only.
    pub clip: LayerId,
    /// "Flicker": three drawings, one per frame, inside a sequence that loops every
    /// four frames.
    pub flicker: LayerId,
    /// "Ghost": a lit layer at half opacity.
    pub ghost: LayerId,
    pub bulb: LayerId,
    pub torch: LayerId,
    pub sun: LayerId,
    pub shot: LayerId,
}

/// The strokes of the stage's "Left arm" layer.
pub fn arm_strokes() -> Vec<Stroke> {
    vec![
        straight(0, Brush::Cylinder, 4, [0.0, 0.5, 0.0]),
        straight_colored(1, Brush::Ribbon, 3, [0.0, 0.8, 0.0], [0.1, 0.6, 0.9], 0.5),
    ]
}

/// A mirrored rig, an animated layer, a clip that starts mid-range, a frame-by-frame
/// layer, a faded lit layer, three lights and a camera. Export frames 0 to
/// [`STAGE_LAST`] at 24 fps.
pub fn stage() -> Stage {
    let mut doc = Document::empty();
    doc.settings.title = "Stage".to_owned();
    doc.settings.camera_resolution = [1920, 1080];
    let root = doc.root();

    let rig = group(&mut doc, root, "Rig");
    doc.layer_mut(rig).unwrap().transform = Xform::new(
        DQuat::from_rotation_y(0.6),
        1.5,
        DVec3::new(-1.0, 0.25, 0.5),
        Flip::X,
    );
    let arm = paint(&mut doc, rig, "Left arm", arm_strokes());
    {
        let layer = doc.layer_mut(arm).unwrap();
        layer.pivot = Xform::from_translation(DVec3::new(0.0, 0.5, 0.0));
        // From 2.5 to 5 radians about Z the turn passes half a turn, where Euler
        // angles wrap from +180 to -180 degrees unless they are unrolled.
        layer.anim.transform = Track::from_keys([
            Key::new(
                2 * FRAME,
                Xform::new(
                    DQuat::from_rotation_z(0.5),
                    1.0,
                    DVec3::new(0.0, 0.0, 0.0),
                    Flip::None,
                ),
                Interp::Linear,
            ),
            Key::new(
                6 * FRAME,
                Xform::new(
                    DQuat::from_rotation_z(2.5) * DQuat::from_rotation_x(0.3),
                    1.5,
                    DVec3::new(0.0, 0.5, 0.25),
                    Flip::None,
                ),
                Interp::Linear,
            ),
            Key::new(
                10 * FRAME,
                Xform::new(
                    DQuat::from_rotation_z(5.0),
                    0.75,
                    DVec3::new(0.5, 0.0, 0.0),
                    Flip::None,
                ),
                Interp::Linear,
            ),
        ]);
    }

    let looped = sequence(&mut doc, root, "Loop", 4 * FRAME, 0);
    let flicker = frames(
        &mut doc,
        looped,
        "Flicker",
        (0..3)
            .map(|i| {
                vec![straight_colored(
                    0,
                    Brush::Cylinder,
                    2 + i,
                    [1.0, 0.3 * i as f32, 0.0],
                    [0.2 + 0.3 * i as f32, 0.8, 0.3],
                    1.0,
                )]
            })
            .collect(),
        0,
    );

    let clip = paint(
        &mut doc,
        root,
        "Clip",
        vec![straight(0, Brush::Ribbon, 4, [0.0, 0.0, 1.0])],
    );
    {
        let layer = doc.layer_mut(clip).unwrap();
        layer.transform = Xform::new(
            DQuat::from_rotation_y(std::f64::consts::FRAC_PI_2),
            2.0,
            DVec3::new(0.0, 0.25, 0.0),
            Flip::None,
        );
        layer.anim.visibility = Track::from_keys([
            Key::step(CLIP_SHOWN.0 * FRAME, true),
            Key::step((CLIP_SHOWN.1 + 1) * FRAME, false),
        ]);
        layer.anim.offset.clear();
    }

    let ghost = paint(
        &mut doc,
        root,
        "Ghost",
        vec![straight(0, Brush::Cylinder, 3, [0.0, 0.0, 2.0])],
    );
    {
        let layer = doc.layer_mut(ghost).unwrap();
        layer.opacity = 0.5;
        layer.as_paint_mut().unwrap().shading = Shading::Lit;
    }

    let mut add_light = |name: &str, light: Light, transform: Xform| {
        let mut layer = doc.new_layer(name, LayerKind::Light(light), 0);
        layer.transform = transform;
        doc.insert_layer(layer, root, usize::MAX).unwrap()
    };
    let bulb = add_light(
        "Bulb",
        Light {
            kind: LightKind::Point,
            color: [1.0, 0.5, 0.25],
            energy: 3.0,
            range: 4.0,
            ..Light::default()
        },
        Xform::new(DQuat::IDENTITY, 2.0, DVec3::new(0.0, 2.0, 0.0), Flip::None),
    );
    let torch = add_light(
        "Torch",
        Light {
            kind: LightKind::Spot,
            color: [2.0, 1.0, 0.5],
            energy: 1.5,
            range: 6.0,
            spot_angle: 30.0,
            spot_blend: 0.25,
            shadows: true,
        },
        Xform::new(
            DQuat::from_rotation_x(-1.0),
            1.0,
            DVec3::new(2.0, 2.0, 2.0),
            Flip::None,
        ),
    );
    let sun = add_light(
        "Sun",
        Light {
            kind: LightKind::Directional,
            energy: 0.75,
            ..Light::default()
        },
        Xform::from_rotation(DQuat::from_rotation_x(-0.8) * DQuat::from_rotation_y(0.3)),
    );
    let mut camera = doc.new_layer("Shot", LayerKind::Camera(Camera { fov: 50.0 }), 0);
    camera.transform = Xform::new(
        DQuat::from_rotation_y(0.4),
        1.0,
        DVec3::new(0.0, 1.6, 4.0),
        Flip::None,
    );
    let shot = doc.insert_layer(camera, root, usize::MAX).unwrap();

    doc.validate().expect("the stage is a consistent document");
    Stage {
        doc,
        rig,
        arm,
        clip,
        flicker,
        ghost,
        bulb,
        torch,
        sun,
        shot,
    }
}
