//! Helpers for tests, here and in other crates (feature `testing`): an exact
//! comparison of two documents and a document that exercises the whole model.

use limner_core::glam::{DQuat, DVec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{
    Action, Attenuation, AttenuationMode, Brush, Camera, Document, Drawing, DrawingId, Group,
    Interp, Key, Layer, LayerId, LayerKind, Light, LightKind, Media, MediaData, MediaKind,
    MeshData, Model, Paint, Picture, Point, Projection, Sequence, Shading, Sound, SoundKind,
    SoundModifier, Stroke, Track, Viewpoint, MAX_CHUNK_VERTICES,
};
use limner_core::time::{from_seconds, FrameRate};
use std::fmt::Debug;
use std::sync::Arc;

/// A small deterministic generator, so tests need no dependency for random data.
pub struct Rng(pub u64);

impl Rng {
    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// Any finite `f32` bit pattern: both signs, subnormals, huge and tiny values.
    pub fn finite_f32(&mut self) -> f32 {
        loop {
            let value = f32::from_bits(self.next_u64() as u32);
            if value.is_finite() {
                return value;
            }
        }
    }

    /// Any finite `f64` bit pattern.
    pub fn finite_f64(&mut self) -> f64 {
        loop {
            let value = f64::from_bits(self.next_u64());
            if value.is_finite() {
                return value;
            }
        }
    }

    pub fn point(&mut self) -> Point {
        let mut values = [0f32; 14];
        for value in &mut values {
            *value = self.finite_f32();
        }
        Point {
            pos: [values[0], values[1], values[2]],
            normal: [values[3], values[4], values[5]],
            dir: [values[6], values[7], values[8]],
            color: [values[9], values[10], values[11]],
            opacity: values[12],
            width: values[13],
        }
    }
}

struct Diff {
    found: Vec<String>,
}

impl Diff {
    /// Compares through `Debug`, which prints floats with enough digits to tell any
    /// two bit patterns apart (so `-0.0` differs from `0.0`) and treats NaN as equal
    /// to itself.
    fn field<T: Debug + ?Sized>(&mut self, what: impl FnOnce() -> String, a: &T, b: &T) {
        let (left, right) = (format!("{a:?}"), format!("{b:?}"));
        if left != right {
            let clip = |s: String| {
                if s.len() > 200 {
                    format!("{}…", s.chars().take(200).collect::<String>())
                } else {
                    s
                }
            };
            self.found
                .push(format!("{}: {} != {}", what(), clip(left), clip(right)));
        }
    }
}

fn point_bits(p: &Point) -> [u32; 14] {
    [
        p.pos[0],
        p.pos[1],
        p.pos[2],
        p.normal[0],
        p.normal[1],
        p.normal[2],
        p.dir[0],
        p.dir[1],
        p.dir[2],
        p.color[0],
        p.color[1],
        p.color[2],
        p.opacity,
        p.width,
    ]
    .map(f32::to_bits)
}

fn diff_layer(diff: &mut Diff, id: LayerId, a: &Layer, b: &Layer) {
    macro_rules! fields {
        ($($field:ident),+) => {
            $(diff.field(|| format!("layer {id} ({}) {}", a.name, stringify!($field)), &a.$field, &b.$field);)+
        };
    }
    fields!(
        id,
        parent,
        name,
        visible,
        locked,
        collapsed,
        show_bounds,
        opacity,
        transform,
        pivot
    );
    macro_rules! tracks {
        ($($track:ident),+) => {
            $(diff.field(|| format!("layer {id} ({}) anim.{}", a.name, stringify!($track)), &a.anim.$track, &b.anim.$track);)+
        };
    }
    tracks!(visibility, offset, opacity, transform, actions, channels);
    diff.field(|| format!("layer {id} ({}) kind", a.name), &a.kind, &b.kind);
}

fn diff_drawing(diff: &mut Diff, id: DrawingId, a: &Drawing, b: &Drawing) {
    diff.field(
        || format!("drawing {id} stroke count"),
        &a.stroke_count(),
        &b.stroke_count(),
    );
    for (index, (x, y)) in a.strokes().zip(b.strokes()).enumerate() {
        let what = |part: &str| format!("drawing {id} stroke {index} {part}");
        diff.field(|| what("id"), &x.id, &y.id);
        diff.field(|| what("brush"), &x.brush, &y.brush);
        diff.field(|| what("directional"), &x.directional, &y.directional);
        diff.field(|| what("point count"), &x.len(), &y.len());
        let mismatch = x
            .points()
            .iter()
            .zip(y.points())
            .position(|(p, q)| point_bits(p) != point_bits(q));
        if let Some(at) = mismatch {
            diff.field(
                || what(&format!("point {at}")),
                &x.points()[at],
                &y.points()[at],
            );
        }
        if diff.found.len() > 20 {
            return;
        }
    }
}

fn diff_media(diff: &mut Diff, a: &Media, b: &Media) {
    let id = a.id;
    diff.field(|| format!("media {id} id"), &a.id, &b.id);
    diff.field(|| format!("media {id} name"), &a.name, &b.name);
    diff.field(|| format!("media {id} kind"), &a.kind, &b.kind);
    if a.data != b.data {
        diff.field(|| format!("media {id} data"), &a.data, &b.data);
        // Byte arrays that differ beyond what the clipped text shows.
        diff.found.push(format!("media {id} data differs"));
    }
}

/// Every difference between two documents that the model can express, as text.
///
/// Compared: settings, the id counter, the root, tree order, every layer field and
/// key, every stroke and point bit for bit, every media item. Not compared: how a
/// drawing's strokes are grouped into chunks, which is a memory layout and not
/// content.
pub fn diff_documents(a: &Document, b: &Document) -> Vec<String> {
    let mut diff = Diff { found: Vec::new() };
    diff.field(|| "next_id".to_owned(), &a.next_id(), &b.next_id());
    diff.field(|| "root".to_owned(), &a.root(), &b.root());
    macro_rules! settings {
        ($($field:ident),+) => {
            $(diff.field(|| format!("settings.{}", stringify!($field)), &a.settings.$field, &b.settings.$field);)+
        };
    }
    settings!(
        title,
        description,
        background,
        rate,
        range_start,
        range_end,
        default_viewpoint,
        camera_resolution,
        palette
    );

    let (order_a, order_b) = (a.walk(), b.walk());
    diff.field(|| "layer tree order".to_owned(), &order_a, &order_b);
    diff.field(
        || "layer count".to_owned(),
        &a.layer_count(),
        &b.layer_count(),
    );
    for id in &order_a {
        if let (Some(x), Some(y)) = (a.layer(*id), b.layer(*id)) {
            diff_layer(&mut diff, *id, x, y);
        }
    }

    let ids = |doc: &Document| doc.drawings().map(|(id, _)| id).collect::<Vec<_>>();
    diff.field(|| "drawing ids".to_owned(), &ids(a), &ids(b));
    for (id, x) in a.drawings() {
        if let Some(y) = b.drawing(id) {
            diff_drawing(&mut diff, id, x, y);
        }
    }

    let ids = |doc: &Document| doc.media_items().map(|m| m.id).collect::<Vec<_>>();
    diff.field(|| "media ids".to_owned(), &ids(a), &ids(b));
    for x in a.media_items() {
        if let Some(y) = b.media(x.id) {
            diff_media(&mut diff, x, y);
        }
    }
    diff.found
}

/// Panics, listing the differences, unless the two documents are equal in every
/// respect [`diff_documents`] compares.
#[track_caller]
pub fn assert_documents_equal(a: &Document, b: &Document) {
    let differences = diff_documents(a, b);
    assert!(
        differences.is_empty(),
        "documents differ in {} places:\n  {}",
        differences.len(),
        differences
            .iter()
            .take(25)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n  ")
    );
}

pub const ALL_INTERPS: [Interp; 6] = [
    Interp::Step,
    Interp::Linear,
    Interp::EaseIn,
    Interp::EaseOut,
    Interp::EaseInOut,
    Interp::Smooth,
];

fn plain_point(x: f32, width: f32) -> Point {
    Point {
        pos: [x, 1.0 + x * 0.5, -0.25 * x],
        normal: [0.0, 1.0, 0.0],
        dir: [0.0, 0.0, -1.0],
        color: [0.25, 0.5, 0.75],
        opacity: 1.0,
        width,
    }
}

fn odd_xform(seed: f64, flip: Flip) -> Xform {
    Xform {
        rotation: DQuat::from_euler(
            limner_core::glam::EulerRot::YXZ,
            0.3 + seed,
            -1.1 * seed,
            0.7,
        ),
        scale: 1.0 / 3.0 + seed,
        translation: DVec3::new(0.1 + seed, -2.0e-9, 1.0e12 / 7.0),
        flip,
    }
}

/// A document that uses every layer kind, every animation channel, every
/// interpolation mode, every enumeration value, mirrored transforms, shared, equal,
/// empty and multi-chunk drawings, and every media variant, with ids whose order
/// differs from tree order and an id counter ahead of the ids in use.
pub fn exhaustive_document() -> Document {
    let mut rng = Rng(0x00C0_FFEE_1234_5678);
    let mut doc = Document::empty();
    let root = doc.root();

    // ----- media: every variant, including the ones PNG and WAVE cannot carry -----
    let thumbnail = doc.add_media(
        "thumb",
        MediaKind::Image,
        MediaData::RawImage {
            width: 3,
            height: 2,
            channels: 3,
            pixels: Arc::from((0..18u8).map(|i| i * 13).collect::<Vec<_>>()),
        },
    );
    let rgba = doc.add_media(
        "overlay.png",
        MediaKind::Image,
        MediaData::RawImage {
            width: 2,
            height: 2,
            channels: 4,
            pixels: Arc::from((0..16u8).map(|i| 255 - i * 7).collect::<Vec<_>>()),
        },
    );
    let odd_image = doc.add_media(
        "not a real image",
        MediaKind::Image,
        MediaData::RawImage {
            width: 5,
            height: 0,
            channels: 7,
            pixels: Arc::from(vec![1u8, 2, 3]),
        },
    );
    let encoded_image = doc.add_media(
        "référence \"final\".jpg",
        MediaKind::Image,
        MediaData::Encoded {
            format: "jpg".to_owned(),
            bytes: Arc::from(&b"\xff\xd8\xff\xe0 pretend jpeg \x00\x01\x02"[..]),
        },
    );
    let pcm = doc.add_media(
        "voice.wav",
        MediaKind::Audio,
        MediaData::RawAudio {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            float: false,
            samples: Arc::from((0..64u8).collect::<Vec<_>>()),
        },
    );
    let float_audio = doc.add_media(
        "ambience",
        MediaKind::Audio,
        MediaData::RawAudio {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 32,
            float: true,
            samples: Arc::from(
                [0.0f32, 0.5, -0.5, 1.0]
                    .iter()
                    .flat_map(|s| s.to_le_bytes())
                    .collect::<Vec<_>>(),
            ),
        },
    );
    let odd_audio = doc.add_media(
        "odd audio",
        MediaKind::Audio,
        MediaData::RawAudio {
            channels: 3,
            sample_rate: 0,
            bits_per_sample: 12,
            float: false,
            samples: Arc::from(vec![9u8; 5]),
        },
    );
    let encoded_audio = doc.add_media(
        "music.ogg",
        MediaKind::Audio,
        MediaData::Encoded {
            format: "ogg".to_owned(),
            bytes: Arc::from(&b"OggS pretend vorbis"[..]),
        },
    );
    let mesh = doc.add_media(
        "prop.obj",
        MediaKind::Mesh,
        MediaData::Mesh(Arc::new(MeshData {
            positions: (0..4)
                .map(|_| [rng.finite_f32(), rng.finite_f32(), rng.finite_f32()])
                .collect(),
            normals: vec![[0.0, 1.0, 0.0]; 4],
            colors: vec![
                [0.1, 0.2, 0.3],
                [0.4, 0.5, 0.6],
                [0.7, 0.8, 0.9],
                [1.0, 0.0, -0.0],
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
        })),
    );
    let encoded_model = doc.add_media(
        "set.glb",
        MediaKind::Mesh,
        MediaData::Encoded {
            format: "glb".to_owned(),
            bytes: Arc::from(&b"glTF pretend binary"[..]),
        },
    );
    // Media nothing refers to is still part of the document.
    doc.add_media(
        "unused",
        MediaKind::Image,
        MediaData::Encoded {
            format: "A format/with strange characters".to_owned(),
            bytes: Arc::from(&b""[..]),
        },
    );

    // ----- drawings -----
    let all_brushes = Drawing::from_strokes([
        Stroke::new(
            0,
            Brush::Line,
            false,
            vec![plain_point(0.0, 0.0), plain_point(0.1, 0.01)],
        ),
        Stroke::new(
            1,
            Brush::Ribbon,
            true,
            (0..5).map(|i| plain_point(i as f32, 0.02)).collect(),
        ),
        Stroke::new(
            2,
            Brush::Cylinder,
            false,
            (0..9).map(|_| rng.point()).collect(),
        ),
        Stroke::new(
            3,
            Brush::Ellipse,
            true,
            (0..4).map(|_| rng.point()).collect(),
        ),
        Stroke::new(u32::MAX, Brush::Cube, false, vec![rng.point()]),
        Stroke::new(77, Brush::Cylinder, false, Vec::new()),
    ]);
    let shared = Arc::new(all_brushes);
    let drawing_a = doc.add_shared_drawing(shared.clone());
    // The same `Arc` under a second id, and equal content in a separate allocation.
    let drawing_a_again = doc.add_shared_drawing(shared.clone());
    let drawing_a_copy = doc.add_drawing(Drawing::from_strokes(
        shared.strokes().map(|s| Stroke::clone(s)),
    ));
    let drawing_b = doc.add_drawing(Drawing::from_strokes([Stroke::new(
        5,
        Brush::Ribbon,
        false,
        (0..12).map(|_| rng.point()).collect(),
    )]));
    let empty = doc.add_drawing(Drawing::new());
    // 30 tubes of 400 points are 84,000 vertices: more than one chunk.
    let big = Drawing::from_strokes((0..30).map(|s| {
        Stroke::new(
            s,
            Brush::Cylinder,
            s % 7 == 0,
            (0..400)
                .map(|i| {
                    let mut p = plain_point(s as f32 + i as f32 * 0.001, 0.01);
                    p.opacity = (i % 10) as f32 / 9.0;
                    p.color = [s as f32 / 30.0, i as f32 / 400.0, 0.5];
                    p
                })
                .collect(),
        )
    }));
    assert!(big.vertex_count() > MAX_CHUNK_VERTICES && big.chunks().len() > 1);
    let drawing_big = doc.add_drawing(big);
    // A drawing nothing refers to is still part of the document.
    doc.add_drawing(Drawing::from_strokes([
        Stroke::new(9, Brush::Cube, false, vec![plain_point(3.0, 0.3)]),
        Stroke::new(10, Brush::Line, true, vec![plain_point(4.0, 0.4)]),
    ]));

    // ----- layers -----
    let add = |doc: &mut Document, parent: LayerId, index: usize, name: &str, kind: LayerKind| {
        let layer = doc.new_layer(name, kind, 0);
        doc.insert_layer(layer, parent, index)
            .expect("parent is a group")
    };

    let spawn = add(
        &mut doc,
        root,
        usize::MAX,
        "Spawn Area",
        LayerKind::Viewpoint(Viewpoint {
            floor_level: true,
            color: [0.9, 0.1, 0.2],
            volume: [0.5, 1.5, -0.5, 3.25],
            allow_translation: [true, false, true],
            exporting: false,
            show_volume: true,
            thumbnail: Some(thumbnail),
        }),
    );
    add(
        &mut doc,
        root,
        usize::MAX,
        "Default viewpoint",
        LayerKind::Viewpoint(Viewpoint::default()),
    );

    let camera = add(
        &mut doc,
        root,
        usize::MAX,
        "Camera",
        LayerKind::Camera(Camera { fov: 37.5 }),
    );
    {
        let layer = doc.layer_mut(camera).expect("just added");
        layer.transform = odd_xform(0.25, Flip::None);
        let mut fov = Track::new();
        for (i, interp) in ALL_INTERPS.iter().enumerate() {
            fov.insert(Key::new(i as i64 * 1_000, 30.0 + i as f64 / 3.0, *interp));
        }
        layer.anim.channels.insert("camera.fov".to_owned(), fov);
        layer
            .anim
            .channels
            .insert("empty channel".to_owned(), Track::new());
    }

    let body = add(
        &mut doc,
        root,
        usize::MAX,
        "Body",
        LayerKind::Group(Group::default()),
    );
    {
        let layer = doc.layer_mut(body).expect("just added");
        layer.opacity = 0.35;
        layer.collapsed = true;
        layer.show_bounds = true;
        layer.transform = odd_xform(0.5, Flip::X);
        layer.pivot = odd_xform(-0.125, Flip::None);
        layer.anim.visibility = Track::from_keys([
            Key::step(0, true),
            Key::step(from_seconds(1.0), false),
            Key::new(from_seconds(2.0), true, Interp::Linear),
            Key::step(from_seconds(3.5), false),
        ]);
        layer.anim.offset = Track::from_keys([
            Key::step(0, 0),
            Key::step(from_seconds(2.0), from_seconds(0.25)),
            Key::new(-5, -123_456_789_012_345, Interp::Smooth),
        ]);
        for (i, interp) in ALL_INTERPS.iter().enumerate() {
            layer.anim.opacity.insert(Key::new(
                i as i64 * 29_400_000,
                [0.0, 1.0, 0.1, 0.333_333_34, 0.999, 0.5][i],
                *interp,
            ));
            let flip = [Flip::None, Flip::X, Flip::Y, Flip::Z, Flip::None, Flip::X][i];
            layer.anim.transform.insert(Key::new(
                i as i64 * 29_400_000 - 7,
                odd_xform(i as f64 * 0.1, flip),
                *interp,
            ));
        }
        // Raw bit patterns in a transform key, to hold the JSON numbers to exactness.
        layer.anim.transform.insert(Key::new(
            i64::MAX,
            Xform {
                rotation: DQuat::from_xyzw(rng.finite_f64(), rng.finite_f64(), -0.0, 5e-324),
                scale: rng.finite_f64(),
                translation: DVec3::new(rng.finite_f64(), f64::MAX, f64::MIN_POSITIVE),
                flip: Flip::Z,
            },
            Interp::Smooth,
        ));
        layer
            .anim
            .transform
            .insert(Key::step(i64::MIN, Xform::IDENTITY));
    }

    let sequence = add(
        &mut doc,
        body,
        usize::MAX,
        "Loop",
        LayerKind::Group(Group {
            children: Vec::new(),
            sequence: Some(Sequence {
                loop_duration: from_seconds(1.5),
                max_repeat: 4,
            }),
        }),
    );
    add(
        &mut doc,
        sequence,
        usize::MAX,
        "Anim",
        LayerKind::Paint(Paint {
            rate: FrameRate::new(24_000, 1001),
            drawings: vec![drawing_a, drawing_b, drawing_big],
            frames: vec![0, 0, 1, 2, 1],
            max_repeat: 3,
            shading: Shading::Lit,
        }),
    );
    // Inserted first among its siblings although created later: tree order is not id order.
    let shared_layer = add(
        &mut doc,
        body,
        0,
        "Shared",
        LayerKind::Paint(Paint::single(drawing_a, FrameRate::whole(12))),
    );
    {
        let layer = doc.layer_mut(shared_layer).expect("just added");
        layer.visible = false;
        layer.locked = true;
        layer.transform = odd_xform(0.75, Flip::Y);
    }
    add(
        &mut doc,
        body,
        1,
        "Twins",
        LayerKind::Paint(Paint {
            rate: FrameRate::whole(24),
            drawings: vec![drawing_a_again, drawing_a_copy, empty],
            frames: vec![2, 0, 1],
            max_repeat: 0,
            shading: Shading::Unlit,
        }),
    );
    let mirrored = add(
        &mut doc,
        root,
        0,
        "",
        LayerKind::Paint(Paint::single(empty, FrameRate::whole(60))),
    );
    doc.layer_mut(mirrored).expect("just added").transform = odd_xform(1.0, Flip::Z);

    let pictures = add(
        &mut doc,
        root,
        usize::MAX,
        "Pictures",
        LayerKind::Group(Group::default()),
    );
    let projections = [
        (Projection::Flat, Some(rgba)),
        (Projection::Equirect, Some(encoded_image)),
        (Projection::EquirectStereo, Some(odd_image)),
        (Projection::CubemapCross, None),
        (Projection::CubemapStrip, Some(rgba)),
    ];
    for (i, (projection, media)) in projections.into_iter().enumerate() {
        add(
            &mut doc,
            pictures,
            usize::MAX,
            &format!("Picture {i}"),
            LayerKind::Picture(Picture {
                media,
                projection,
                viewer_locked: i % 2 == 1,
                source_path: "C:\\art\\réf \"1\".png".to_owned(),
            }),
        );
    }

    for (i, (media, shading)) in [
        (Some(mesh), Shading::Lit),
        (Some(encoded_model), Shading::Unlit),
        (None, Shading::Unlit),
    ]
    .into_iter()
    .enumerate()
    {
        add(
            &mut doc,
            root,
            usize::MAX,
            &format!("Model {i}"),
            LayerKind::Model(Model {
                media,
                shading,
                wireframe: i == 0,
                source_path: "/home/artist/props/prop.obj".to_owned(),
            }),
        );
    }

    let sounds = [
        (
            SoundKind::Flat,
            AttenuationMode::None,
            SoundModifier::None,
            Some(pcm),
        ),
        (
            SoundKind::Positional,
            AttenuationMode::Linear,
            SoundModifier::Cone {
                inner: 0.5,
                band: 0.25,
                outside: 0.1,
            },
            Some(float_audio),
        ),
        (
            SoundKind::Ambisonic,
            AttenuationMode::Logarithmic,
            SoundModifier::Frustum {
                inner_h: 1.2,
                inner_v: 0.8,
                band: 0.3,
                outside: 0.0,
            },
            Some(encoded_audio),
        ),
        (
            SoundKind::Flat,
            AttenuationMode::None,
            SoundModifier::None,
            Some(odd_audio),
        ),
        (
            SoundKind::Positional,
            AttenuationMode::Linear,
            SoundModifier::None,
            None,
        ),
    ];
    for (i, (kind, mode, modifier, media)) in sounds.into_iter().enumerate() {
        add(
            &mut doc,
            root,
            usize::MAX,
            &format!("Sound {i}"),
            LayerKind::Sound(Sound {
                media,
                kind,
                gain: 0.1 + i as f32,
                looped: i % 2 == 0,
                attenuation: Attenuation {
                    mode,
                    min: 0.3,
                    max: 12.5,
                },
                modifier,
                source_path: String::new(),
            }),
        );
    }

    for (i, kind) in [LightKind::Point, LightKind::Spot, LightKind::Directional]
        .into_iter()
        .enumerate()
    {
        let light = add(
            &mut doc,
            root,
            usize::MAX,
            &format!("Light {i}"),
            LayerKind::Light(Light {
                kind,
                color: [1.0, 0.9, 0.8 - i as f32 * 0.1],
                energy: 2.5,
                range: 7.25,
                spot_angle: 22.5,
                spot_blend: 0.3,
                shadows: i == 1,
            }),
        );
        doc.layer_mut(light)
            .expect("just added")
            .anim
            .channels
            .insert(
                "light.energy".to_owned(),
                Track::from_keys([
                    Key::new(0, 0.0, Interp::EaseInOut),
                    Key::step(705_600_000, 2.5),
                ]),
            );
    }

    // ----- root and settings -----
    {
        let layer = doc.layer_mut(root).expect("root exists");
        layer.name = "Root".to_owned();
        if let Some(group) = layer.as_group_mut() {
            group.sequence = Some(Sequence {
                loop_duration: from_seconds(12.0),
                max_repeat: 0,
            });
        }
        layer.anim.actions = Track::from_keys([
            Key::step(from_seconds(1.0), Action::Stop),
            Key::step(from_seconds(2.0), Action::Play),
            Key::new(from_seconds(3.0), Action::Loop, Interp::Linear),
        ]);
    }
    doc.settings.title = "Exhaustive \"test\" — プロジェクト".to_owned();
    doc.settings.description = "Line one\nLine two\ttabbed \\ backslash".to_owned();
    doc.settings.background = [0.8, 0.1, 0.333_333_34];
    doc.settings.rate = FrameRate::new(30_000, 1001);
    doc.settings.range_start = -from_seconds(0.5);
    doc.settings.range_end = from_seconds(3600.0);
    doc.settings.default_viewpoint = Some(spawn);
    doc.settings.camera_resolution = [3840, 2160];
    doc.settings.palette = (0..16)
        .map(|i| {
            [
                i as f32 / 15.0,
                1.0 - i as f32 / 15.0,
                rng.finite_f32().abs().fract(),
            ]
        })
        .collect();

    // An id counter ahead of every id in use must be kept: ids are never reused.
    doc.reserve_ids(doc.next_id() + 1_000);
    doc.validate()
        .expect("the exhaustive document is consistent");
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_equals_itself_and_differences_are_found() {
        let doc = exhaustive_document();
        assert_documents_equal(&doc, &doc.clone());

        let mut other = doc.clone();
        let id = other.find_by_path("Root/Body").unwrap();
        other.layer_mut(id).unwrap().pivot.translation.x = -0.0;
        other.layer_mut(id).unwrap().transform.scale += 1e-15;
        other.settings.palette[3][1] = f32::from_bits(other.settings.palette[3][1].to_bits() ^ 1);
        let differences = diff_documents(&doc, &other);
        assert_eq!(differences.len(), 2 + 1, "{differences:#?}");

        let mut other = doc.clone();
        let (drawing, _) = other
            .drawings()
            .find(|(_, d)| d.stroke_count() == 30)
            .unwrap();
        let drawing = other.drawing_mut(drawing).unwrap();
        let addr = drawing.find_stroke(29).unwrap();
        let point = &mut drawing.stroke_mut(addr).unwrap().points_mut()[399];
        point.width = f32::from_bits(point.width.to_bits() + 1);
        assert_eq!(diff_documents(&doc, &other).len(), 1);

        let mut other = doc.clone();
        other.reserve_ids(doc.next_id() + 1);
        assert_eq!(diff_documents(&doc, &other).len(), 1);
    }
}
