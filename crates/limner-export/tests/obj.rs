//! Wavefront OBJ export, checked by parsing the text back.

mod support;

use limner_core::glam::{DQuat, DVec3, Vec3};
use limner_core::math::{Flip, Xform};
use limner_core::model::{Brush, Document, Key, Stroke, Track};
use limner_core::tess::ring_positions;
use limner_export::{export_obj, export_obj_with_report, write_obj};
use support::{group, paint, straight, straight_colored, FRAME};

#[derive(Default)]
struct Object {
    name: String,
    group: String,
    positions: Vec<Vec3>,
    colors: Vec<[f32; 3]>,
    normals: Vec<Vec3>,
    /// Zero-based indices into the object's own vertices.
    triangles: Vec<[usize; 3]>,
}

/// A strict reader for the subset of OBJ the exporter writes.
fn parse(text: &str) -> Vec<Object> {
    let mut objects: Vec<Object> = Vec::new();
    let mut vertices_before = 0usize; // vertices in earlier objects
    let mut normals_before = 0usize;
    for line in text.lines() {
        let mut words = line.split(' ');
        let tag = words.next().unwrap();
        let rest: Vec<&str> = words.collect();
        let numbers = |rest: &[&str]| -> Vec<f32> {
            rest.iter()
                .map(|w| w.parse().unwrap_or_else(|_| panic!("a number: {w:?}")))
                .collect()
        };
        match tag {
            "#" => {}
            "o" => {
                if let Some(previous) = objects.last() {
                    vertices_before += previous.positions.len();
                    normals_before += previous.normals.len();
                }
                assert_eq!(rest.len(), 1, "object names have no spaces");
                objects.push(Object {
                    name: rest[0].to_owned(),
                    ..Object::default()
                });
            }
            "g" => objects.last_mut().unwrap().group = rest.join(" "),
            "v" => {
                let n = numbers(&rest);
                assert_eq!(n.len(), 6, "position and colour");
                assert!(n.iter().all(|x| x.is_finite()));
                let object = objects.last_mut().unwrap();
                object.positions.push(Vec3::new(n[0], n[1], n[2]));
                object.colors.push([n[3], n[4], n[5]]);
            }
            "vn" => {
                let n = numbers(&rest);
                assert_eq!(n.len(), 3);
                objects
                    .last_mut()
                    .unwrap()
                    .normals
                    .push(Vec3::new(n[0], n[1], n[2]));
            }
            "f" => {
                assert_eq!(rest.len(), 3, "triangles only");
                let object = objects.last_mut().unwrap();
                let corner = |word: &str| -> usize {
                    let (v, n) = word.split_once("//").expect("v//vn");
                    let v: usize = v.parse().unwrap();
                    let n: usize = n.parse().unwrap();
                    assert!(v >= 1, "indices start at one");
                    let local = v - 1 - vertices_before;
                    assert_eq!(n - 1 - normals_before, local, "normals pair with vertices");
                    assert!(local < object.positions.len(), "index within the object");
                    local
                };
                let triangle = [corner(rest[0]), corner(rest[1]), corner(rest[2])];
                object.triangles.push(triangle);
            }
            other => panic!("unexpected line: {other:?}"),
        }
    }
    for object in &objects {
        assert_eq!(object.positions.len(), object.normals.len());
        assert_eq!(object.name, object.group);
    }
    objects
}

fn rings(stroke: &Stroke) -> Vec<Vec3> {
    let mut out = Vec::new();
    for i in 0..stroke.len() {
        ring_positions(stroke.points(), i, stroke.brush, &mut out);
    }
    out
}

#[test]
fn counts_names_and_colour_encoding() {
    let mut doc = Document::empty();
    doc.settings.title = "Lantern".to_owned();
    let root = doc.root();
    paint(
        &mut doc,
        root,
        "Left arm",
        vec![
            straight_colored(0, Brush::Cylinder, 5, [0.0; 3], [1.0, 0.5, 0.0], 1.0),
            straight_colored(
                1,
                Brush::Ribbon,
                3,
                [0.0, 1.0, 0.0],
                [0.214_041_14, 0.0, 1.0],
                0.5,
            ),
        ],
    );
    paint(
        &mut doc,
        root,
        "Left arm",
        vec![straight(0, Brush::Cube, 2, [0.0, 2.0, 0.0])],
    );
    let (text, report) = export_obj_with_report(&doc, 0, false).unwrap();
    assert!(text.starts_with("# Limner OBJ export\n# Lantern\n"));
    assert!(text.contains("sRGB"));
    let objects = parse(&text);
    assert_eq!(objects.len(), 2);
    assert_eq!(objects[0].name, "Left_arm");
    assert_ne!(objects[1].name, objects[0].name, "names are unique");
    assert!(objects[1].name.starts_with("Left_arm_"));

    // 35 tube vertices then 6 ribbon vertices; 56 + 4 triangles.
    assert_eq!(objects[0].positions.len(), 41);
    assert_eq!(objects[0].triangles.len(), 60);
    assert_eq!(objects[1].positions.len(), 8);
    assert_eq!(objects[1].triangles.len(), 8);
    assert_eq!(report.nodes, 2);
    assert_eq!(report.meshes, 3);
    assert_eq!(report.vertices, 49);
    assert_eq!(report.triangles, 68);
    assert_eq!(report.bytes, text.len() as u64);
    assert_eq!(report.animation_frames, 0);

    // Linear 0.5 is 0.7354 once sRGB-encoded, and linear 0.214 is mid grey.
    let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(&b).all(|(x, y)| (x - y).abs() < 1e-4);
    assert!(close(objects[0].colors[0], [1.0, 0.7354, 0.0]));
    assert!(close(objects[0].colors[40], [0.5, 0.0, 1.0]));
    assert!(text.contains(" 1.0000 0.7354 0.0000\n"));
}

#[test]
fn vertices_are_in_world_space_with_outward_winding() {
    for flip in [Flip::None, Flip::X, Flip::Z] {
        let mut doc = Document::empty();
        let root = doc.root();
        let rig = group(&mut doc, root, "Rig");
        doc.layer_mut(rig).unwrap().transform = Xform {
            rotation: DQuat::from_rotation_y(0.8),
            scale: 3.0,
            translation: DVec3::new(1.0, -2.0, 0.5),
            flip,
        };
        let stroke = straight(0, Brush::Cylinder, 4, [0.2, 0.1, -0.3]);
        let layer = paint(&mut doc, rig, "Tube", vec![stroke.clone()]);
        doc.layer_mut(layer).unwrap().transform = Xform::from_rotation(DQuat::from_rotation_x(0.4));

        let objects = parse(&export_obj(&doc, 0, false).unwrap());
        assert_eq!(objects.len(), 1);
        let object = &objects[0];
        let world = doc.world_from_layer(layer);
        let expected: Vec<Vec3> = rings(&stroke)
            .iter()
            .map(|p| world.transform_point3(p.as_dvec3()).as_vec3())
            .collect();
        assert_eq!(object.positions.len(), expected.len());
        for (parsed, expected) in object.positions.iter().zip(&expected) {
            assert!((*parsed - *expected).length() < 1e-5, "{flip:?}");
        }

        let points = stroke.points();
        let to_world = |p: Vec3| world.transform_point3(p.as_dvec3()).as_vec3();
        let start = to_world(points[0].position());
        let axis = (to_world(points[3].position()) - start).normalize();
        let outward = |v: Vec3| (v - start) - axis * (v - start).dot(axis);
        assert_eq!(object.triangles.len(), 3 * 7 * 2);
        for triangle in &object.triangles {
            let [a, b, c] = triangle.map(|i| object.positions[i]);
            assert!(
                (b - a).cross(c - a).dot(outward((a + b + c) / 3.0)) > 0.0,
                "{flip:?}: counter-clockwise seen from outside"
            );
        }
        for (position, normal) in object.positions.iter().zip(&object.normals) {
            assert!((normal.length() - 1.0).abs() < 1e-3);
            assert!(normal.dot(outward(*position)) > 0.0, "{flip:?}");
        }
    }
}

#[test]
fn only_what_is_visible_at_the_instant_is_written() {
    let mut doc = Document::empty();
    let root = doc.root();
    let hidden = paint(
        &mut doc,
        root,
        "Hidden",
        vec![straight(0, Brush::Ribbon, 2, [0.0; 3])],
    );
    doc.layer_mut(hidden).unwrap().visible = false;
    let late = paint(
        &mut doc,
        root,
        "Late",
        vec![straight(0, Brush::Ribbon, 2, [0.0; 3])],
    );
    doc.layer_mut(late).unwrap().anim.visibility = Track::from_keys([Key::step(5 * FRAME, true)]);
    support::frames(
        &mut doc,
        root,
        "Flip",
        vec![
            vec![straight(0, Brush::Ribbon, 2, [0.0; 3])],
            vec![straight(0, Brush::Ribbon, 3, [0.0; 3])],
        ],
        0,
    );

    let names = |time, include_hidden| -> Vec<(String, usize)> {
        parse(&export_obj(&doc, time, include_hidden).unwrap())
            .into_iter()
            .map(|o| (o.name, o.positions.len()))
            .collect()
    };
    assert_eq!(names(0, false), vec![("Flip".to_owned(), 4)]);
    assert_eq!(
        names(0, true),
        vec![("Hidden".to_owned(), 4), ("Flip".to_owned(), 4)]
    );
    // Frame 5: the clip has started and the frame-by-frame layer shows its second drawing.
    assert_eq!(
        names(5 * FRAME, false),
        vec![("Late".to_owned(), 4), ("Flip".to_owned(), 6)]
    );
}

#[test]
fn an_empty_document_gives_a_header_only() {
    let (text, report) = export_obj_with_report(&Document::new(), 0, false).unwrap();
    assert!(parse(&text).is_empty());
    assert_eq!(report.vertices, 0);
    assert!(text.lines().all(|line| line.starts_with('#')));
}

#[test]
fn writing_a_file() {
    let mut doc = Document::empty();
    let root = doc.root();
    paint(
        &mut doc,
        root,
        "A",
        vec![straight(0, Brush::Cylinder, 3, [0.0; 3])],
    );
    let dir = std::env::temp_dir().join(format!("limner-export-obj-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("still.obj");
    let report = write_obj(&doc, &path, 0, false).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(report.bytes, text.len() as u64);
    assert_eq!(parse(&text)[0].positions.len(), 21);
    std::fs::remove_dir_all(&dir).unwrap();
}
