//! The structural validator the other tests lean on must itself reject broken files.
//! Each case damages a good export in one way and expects the validator to object.

mod support;

use limner_core::model::{Brush, Document, Key, Track};
use limner_export::{export_glb, ExportOptions, GltfOptions};
use serde_json::{json, Value};
use support::{frames, group, pack, paint, straight, Glb, FRAME, RATE};

/// An animated export with a mesh, a hierarchy, clips, frame-by-frame drawings and
/// both pointer extensions.
fn good() -> Glb {
    let mut doc = Document::empty();
    let root = doc.root();
    let body = group(&mut doc, root, "Body");
    let clip = paint(
        &mut doc,
        body,
        "Clip",
        vec![straight(0, Brush::Cylinder, 4, [0.0; 3])],
    );
    doc.layer_mut(clip).unwrap().anim.visibility =
        Track::from_keys([Key::step(2 * FRAME, true), Key::step(5 * FRAME, false)]);
    frames(
        &mut doc,
        body,
        "Flip",
        vec![
            vec![straight(0, Brush::Ribbon, 3, [0.0; 3])],
            vec![straight(0, Brush::Ribbon, 4, [0.0; 3])],
        ],
        0,
    );
    let extra = GltfOptions {
        node_visibility: true,
        ..GltfOptions::default()
    };
    let options = ExportOptions::animated(0, 8 * FRAME, RATE);
    Glb::checked(&export_glb(&doc, &options, &extra).unwrap())
}

fn rejects(what: &str, damage: impl FnOnce(&mut Value, &mut Vec<u8>)) {
    let glb = good();
    let (mut json, mut bin) = (glb.json, glb.bin);
    damage(&mut json, &mut bin);
    let bytes = pack(&json, &bin);
    let result = std::panic::catch_unwind(|| {
        Glb::checked(&bytes);
    });
    assert!(result.is_err(), "the validator accepted a file with {what}");
}

fn view_offset(json: &Value, accessor: usize) -> usize {
    let view = json["accessors"][accessor]["bufferView"].as_u64().unwrap() as usize;
    json["bufferViews"][view]["byteOffset"].as_u64().unwrap() as usize
}

#[test]
fn an_untouched_file_survives_repacking() {
    let glb = good();
    Glb::checked(&pack(&glb.json, &glb.bin));
}

#[test]
fn container_damage_is_caught() {
    let glb = good();
    let bytes = pack(&glb.json, &glb.bin);
    let broken = |edit: &dyn Fn(&mut Vec<u8>)| {
        let mut copy = bytes.clone();
        edit(&mut copy);
        std::panic::catch_unwind(move || {
            Glb::checked(&copy);
        })
        .is_err()
    };
    assert!(broken(&|b| b[0] = b'x'), "bad magic");
    assert!(broken(&|b| b[4] = 1), "container version 1");
    assert!(
        broken(&|b| b[8] = b[8].wrapping_add(4)),
        "wrong total length"
    );
    assert!(broken(&|b| b.extend_from_slice(&[0; 4])), "trailing bytes");
    assert!(broken(&|b| b[16] = b'X'), "first chunk is not JSON");
    assert!(broken(&|b| b.truncate(b.len() - 4)), "truncated buffer");
}

#[test]
fn accessor_and_view_damage_is_caught() {
    rejects("a view past the end of the buffer", |json, _| {
        let last = json["bufferViews"].as_array().unwrap().len() - 1;
        let length = json["bufferViews"][last]["byteLength"].as_u64().unwrap();
        json["bufferViews"][last]["byteLength"] = json!(length + 64);
    });
    rejects("an accessor larger than its view", |json, _| {
        let count = json["accessors"][0]["count"].as_u64().unwrap();
        json["accessors"][0]["count"] = json!(count + 1);
    });
    rejects("a misaligned view", |json, _| {
        json["bufferViews"][1]["byteOffset"] =
            json!(json["bufferViews"][1]["byteOffset"].as_u64().unwrap() + 2);
    });
    rejects("a wrong POSITION minimum", |json, _| {
        let position = json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
            .as_u64()
            .unwrap() as usize;
        json["accessors"][position]["min"][1] = json!(-123.0);
    });
    rejects("a non-finite position", |json, bin| {
        let position = json["meshes"][0]["primitives"][0]["attributes"]["POSITION"]
            .as_u64()
            .unwrap() as usize;
        let at = view_offset(json, position);
        bin[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    });
    rejects("an index past the vertex count", |json, bin| {
        let indices = json["meshes"][0]["primitives"][0]["indices"]
            .as_u64()
            .unwrap() as usize;
        let at = view_offset(json, indices);
        bin[at..at + 2].copy_from_slice(&60_000u16.to_le_bytes());
    });
    rejects("a normal that is not unit length", |json, bin| {
        let normal = json["meshes"][0]["primitives"][0]["attributes"]["NORMAL"]
            .as_u64()
            .unwrap() as usize;
        let at = view_offset(json, normal);
        bin[at..at + 4].copy_from_slice(&3.0f32.to_le_bytes());
    });
    rejects("attributes of different lengths", |json, _| {
        let attributes = &json["meshes"][0]["primitives"][0]["attributes"];
        let other = json["meshes"][1]["primitives"][0]["attributes"]["COLOR_0"].clone();
        assert_ne!(attributes["COLOR_0"], other);
        json["meshes"][0]["primitives"][0]["attributes"]["COLOR_0"] = other;
    });
}

#[test]
fn hierarchy_damage_is_caught() {
    rejects("a cycle", |json, _| {
        // Make the root a child of its own grandchild.
        let nodes = json["nodes"].as_array().unwrap();
        let leaf = nodes
            .iter()
            .position(|n| n.get("children").is_none())
            .unwrap();
        let root = json["scenes"][0]["nodes"][0].clone();
        json["nodes"][leaf]["children"] = json!([root]);
    });
    rejects("a node no scene reaches", |json, _| {
        json["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({ "name": "orphan" }));
    });
    rejects("a node with two parents", |json, _| {
        let children = json["nodes"][0]["children"].clone();
        let other = children[0].as_u64().unwrap() as usize;
        let first = json["nodes"][other]["children"][0].clone();
        json["nodes"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(first);
    });
    rejects("a child index that does not exist", |json, _| {
        json["nodes"][0]["children"]
            .as_array_mut()
            .unwrap()
            .push(json!(9_999));
    });
}

#[test]
fn animation_damage_is_caught() {
    rejects("times that do not increase", |json, bin| {
        let input = json["animations"][0]["samplers"][0]["input"]
            .as_u64()
            .unwrap() as usize;
        let at = view_offset(json, input);
        // Copy the second key's time over the third.
        let second: [u8; 4] = bin[at + 4..at + 8].try_into().unwrap();
        bin[at + 8..at + 12].copy_from_slice(&second);
    });
    rejects("an output with the wrong number of keys", |json, _| {
        let samplers = json["animations"][0]["samplers"].as_array().unwrap();
        let (a, b) = (samplers[0]["input"].clone(), samplers[0]["output"].clone());
        let other = samplers
            .iter()
            .find(|s| {
                json["accessors"][s["input"].as_u64().unwrap() as usize]["count"]
                    != json["accessors"][a.as_u64().unwrap() as usize]["count"]
                    && json["accessors"][s["output"].as_u64().unwrap() as usize]["type"]
                        == json["accessors"][b.as_u64().unwrap() as usize]["type"]
            })
            .expect("two samplers with different key counts")["output"]
            .clone();
        json["animations"][0]["samplers"][0]["output"] = other;
    });
    rejects("two channels on one target", |json, _| {
        let channel = json["animations"][0]["channels"][0].clone();
        json["animations"][0]["channels"]
            .as_array_mut()
            .unwrap()
            .push(channel);
    });
    rejects("a pointer that resolves to nothing", |json, _| {
        for channel in json["animations"][0]["channels"].as_array_mut().unwrap() {
            if channel["target"]["path"] == "pointer" {
                channel["target"]["extensions"]["KHR_animation_pointer"]["pointer"] =
                    json!("/nodes/9999/extensions/KHR_node_visibility/visible");
                return;
            }
        }
        panic!("the good file has a pointer channel");
    });
    rejects("an interpolated boolean", |json, _| {
        let channels = json["animations"][0]["channels"].as_array().unwrap();
        let sampler = channels
            .iter()
            .find(|c| c["target"]["path"] == "pointer")
            .unwrap()["sampler"]
            .as_u64()
            .unwrap() as usize;
        json["animations"][0]["samplers"][sampler]["interpolation"] = json!("LINEAR");
    });
}

#[test]
fn extension_bookkeeping_damage_is_caught() {
    rejects("an extension that is used but not listed", |json, _| {
        json["extensionsUsed"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e != "KHR_materials_unlit");
    });
    rejects("an extension that is listed but not used", |json, _| {
        json["extensionsUsed"]
            .as_array_mut()
            .unwrap()
            .push(json!("KHR_lights_punctual"));
    });
    rejects("an empty array", |json, _| {
        json["cameras"] = json!([]);
    });
}
