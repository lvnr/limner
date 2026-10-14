//! Models, sounds and pictures, built byte for byte in Quill's layouts.

use limner_core::model::{
    Attenuation, AttenuationMode, Document, Layer, LayerKind, MediaData, MediaKind, MeshData,
    Model, Picture, Projection, Shading, Sound, SoundKind, SoundModifier,
};
use limner_quill::testing::*;
use limner_quill::{import_project, Imported};
use serde_json::{json, Value};
use std::sync::Arc;

fn import(quill: &Value, qbin: &[u8]) -> Imported {
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), quill, qbin, None);
    let imported = import_project(dir.path()).unwrap();
    imported.document.validate().unwrap();
    imported
}

fn layer_at<'a>(doc: &'a Document, path: &str) -> &'a Layer {
    let id = doc
        .find_by_path(path)
        .unwrap_or_else(|| panic!("no layer at {path}"));
    doc.layer(id).unwrap()
}

fn model_at<'a>(doc: &'a Document, path: &str) -> &'a Model {
    match &layer_at(doc, path).kind {
        LayerKind::Model(model) => model,
        other => panic!("{path} is not a model: {other:?}"),
    }
}

fn sound_at<'a>(doc: &'a Document, path: &str) -> &'a Sound {
    match &layer_at(doc, path).kind {
        LayerKind::Sound(sound) => sound,
        other => panic!("{path} is not a sound: {other:?}"),
    }
}

fn picture_at<'a>(doc: &'a Document, path: &str) -> &'a Picture {
    match &layer_at(doc, path).kind {
        LayerKind::Picture(picture) => picture,
        other => panic!("{path} is not a picture: {other:?}"),
    }
}

fn mesh_of<'a>(doc: &'a Document, model: &Model) -> &'a MeshData {
    let media = doc
        .media(model.media.expect("the model has a mesh"))
        .unwrap();
    assert_eq!(media.kind, MediaKind::Mesh);
    match &media.data {
        MediaData::Mesh(mesh) => mesh,
        other => panic!("not a mesh: {other:?}"),
    }
}

/// A unit square in the XY plane and a triangle hanging off its right edge.
fn square_and_triangle() -> MeshSpec {
    MeshSpec {
        positions: vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [2.0, 0.5, 0.0],
        ],
        colors: Some(vec![
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.5, 0.25, 0.125],
        ]),
        normals: Some(vec![[0.0, 0.0, 1.0]; 5]),
        faces: vec![vec![0, 1, 2, 3], vec![1, 4, 2]],
    }
}

fn only_strip() -> MeshSpec {
    MeshSpec {
        positions: vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
        colors: Some(vec![[1.0; 3]; 3]),
        normals: Some(vec![[1.0, 0.0, 0.0]; 3]),
        faces: vec![vec![0, 1, 2]],
    }
}

#[test]
fn imports_model_layers() {
    let mut qbin = QbinBuilder::default();
    let first = qbin.mesh(&square_and_triangle());
    let second = qbin.mesh(&only_strip());

    // A single-layer import, as Quill writes an OBJ.
    let single = model("Rock", &first, "E:/assets/rock.obj");

    // An import that kept its hierarchy: the top layer is a group flagged as the model
    // root, with one model layer per mesh. Two of them use the same record.
    let mut smooth = model("Body", &second, "E:/assets/robot.fbx");
    smooth["IsModelTopLayer"] = json!(false);
    smooth["Implementation"]["ShadingModel"] = json!("Smooth");
    smooth["Implementation"]["RenderWireFrame"] = json!(true);
    let mut twin = model("Twin", &second, "E:/assets/robot.fbx");
    twin["IsModelTopLayer"] = json!(false);
    let mut hierarchy = group("Robot", vec![smooth, twin]);
    hierarchy["IsModelTopLayer"] = json!(true);
    hierarchy["Transform"] = xform([0.0, 0.0, 0.0, 1.0], 0.01, "N", [0.0, 1.0, 0.0]);

    // A node that is a mesh and a parent at once.
    let mut child = model("Hand", &second, "");
    child["IsModelTopLayer"] = json!(false);
    let mut node = model("Arm", &first, "E:/assets/arm.fbx");
    node["Implementation"]["Children"] = json!([child]);

    let mut quill = project(vec![single, hierarchy, node]);
    quill["Sequence"]["DefaultViewpoint"] = json!("");
    let imported = import(&quill, &qbin.bytes);
    assert_eq!(imported.warnings, Vec::<String>::new());
    let doc = &imported.document;

    let rock = model_at(doc, "Root/Rock");
    assert_eq!(rock.shading, Shading::Unlit);
    assert!(!rock.wireframe);
    assert_eq!(rock.source_path, "E:/assets/rock.obj");
    assert_eq!(doc.media(rock.media.unwrap()).unwrap().name, "rock.obj");
    let spec = square_and_triangle();
    assert_eq!(
        mesh_of(doc, rock),
        &MeshData {
            positions: spec.positions.clone(),
            normals: spec.normals.clone().unwrap(),
            colors: spec.colors.clone().unwrap(),
            // The quad becomes two triangles that keep its winding.
            indices: vec![0, 1, 2, 0, 2, 3, 1, 4, 2],
        }
    );

    let robot = layer_at(doc, "Root/Robot");
    assert!(robot.as_group().is_some());
    assert_eq!(robot.transform.scale, 0.01);
    assert_eq!(robot.children().len(), 2);
    let body = model_at(doc, "Root/Robot/Body");
    assert_eq!(body.shading, Shading::Lit);
    assert!(body.wireframe);
    assert_eq!(mesh_of(doc, body).indices, [0, 1, 2]);
    assert_eq!(
        model_at(doc, "Root/Robot/Twin").media,
        body.media,
        "one file record, one mesh"
    );

    // The mesh of a node with children moves into a model layer of the same name.
    let arm = layer_at(doc, "Root/Arm");
    assert!(arm.as_group().is_some());
    let names: Vec<&str> = arm
        .children()
        .iter()
        .map(|id| doc.layer(*id).unwrap().name.as_str())
        .collect();
    assert_eq!(names, ["Arm", "Hand"]);
    let arm_mesh = model_at(doc, "Root/Arm/Arm");
    assert_eq!(arm_mesh.media, rock.media);
    assert_eq!(arm_mesh.source_path, "E:/assets/arm.fbx");
    assert_eq!(model_at(doc, "Root/Arm/Hand").media, body.media);

    assert_eq!(doc.media_items().count(), 2);
}

#[test]
fn damaged_models_import_with_warnings() {
    let mut spec = square_and_triangle();
    spec.positions[4] = [f32::NAN, 0.0, 0.0];
    spec.faces.push(vec![0, 1, 2, 3, 4]);
    spec.faces.push(vec![0, 1, 77]);
    let mut qbin = QbinBuilder::default();
    let damaged = qbin.mesh(&spec);
    let cut_short = qbin.mesh(&only_strip());
    let keep = qbin.bytes.len() - 10;

    let mut odd_shading = model("Odd", &damaged, "odd.obj");
    odd_shading["Implementation"]["ShadingModel"] = json!("Toon");
    let mut no_offset = model("NoOffset", &damaged, "none.obj");
    no_offset["Implementation"]
        .as_object_mut()
        .unwrap()
        .remove("DataFileOffset");
    let quill = project(vec![
        odd_shading,
        model("Cut", &cut_short, "cut.obj"),
        model("Far", "00000000ffffffff", "far.obj"),
        no_offset,
    ]);
    let imported = import(&quill, &qbin.bytes[..keep]);
    let doc = &imported.document;

    let odd = model_at(doc, "Root/Odd");
    assert_eq!(odd.shading, Shading::Unlit);
    assert_eq!(mesh_of(doc, odd).indices, [0, 1, 2, 0, 2, 3]);
    for path in ["Root/Cut", "Root/Far", "Root/NoOffset"] {
        assert_eq!(model_at(doc, path).media, None, "{path}");
    }

    let all = imported.warnings.join("\n");
    for expected in [
        "Root/Odd: 1 vertex position(s) of the model were not finite; the faces using them were skipped",
        "Root/Odd: 3 face(s) of the model were skipped",
        "Root/Odd: unknown shading model \"Toon\" read as unlit",
        "Root/Cut: the model's geometry could not be read (the record runs past the end of Quill.qbin); the layer is kept empty",
        "Root/Far: the model's geometry could not be read",
        "Root/NoOffset: the model has no readable file offset",
    ] {
        assert!(all.contains(expected), "missing {expected:?} in:\n{all}");
    }
}

#[test]
fn imports_sounds_in_either_layout() {
    let pcm: Vec<u8> = (0u8..48).collect();
    let floats: Vec<u8> = [0.25f32, -0.5, 1.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let wav = wav_file(2, 16, 44_100, &pcm);
    let ogg = b"OggS\0\x02 not a real stream, only its signature".to_vec();

    let mut qbin = QbinBuilder::default();
    let raw = qbin.sound_wave(2, 16, 48_000, &pcm);
    let raw_float = qbin.sound_wave(1, 32, 44_100, &floats);
    let file_wav = qbin.sound_file(&wav);
    let file_ogg = qbin.sound_file(&ogg);
    // Something after the last record, so reading too far would be noticed.
    qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 4))]);

    let mut positional = sound("Voice", &raw, "C:/audio/voice line.wav");
    positional["Opacity"] = json!(0.5);
    positional["Implementation"]["Type"] = json!("Positional");
    positional["Implementation"]["Gain"] = json!(1.5);
    positional["Implementation"]["Loop"] = json!(true);
    positional["Implementation"]["Attenuation"] =
        json!({"Mode": "Linear", "Minimum": 0.5, "Maximum": 12.0});
    let mut ambisonic = sound("Ambience", &raw_float, "C:/audio/room.wav");
    ambisonic["Implementation"]["Type"] = json!("Ambisonic");
    ambisonic["Implementation"]["Attenuation"]["Mode"] = json!("Logarithmic");
    let flat = sound("Music", &file_wav, "C:/audio/music.wav");
    let mut compressed = sound("Loop", &file_ogg, "");
    compressed["Implementation"]
        .as_object_mut()
        .unwrap()
        .remove("Modifiers");
    let again = sound("Again", &raw, "C:/audio/voice line.wav");

    let mut quill = project(vec![positional, ambisonic, flat, compressed, again]);
    quill["Sequence"]["DefaultViewpoint"] = json!("");
    let imported = import(&quill, &qbin.bytes);
    assert_eq!(imported.warnings, Vec::<String>::new());
    let doc = &imported.document;

    let voice = sound_at(doc, "Root/Voice");
    assert_eq!(
        voice,
        &Sound {
            media: voice.media,
            kind: SoundKind::Positional,
            gain: 1.5,
            looped: true,
            attenuation: Attenuation {
                mode: AttenuationMode::Linear,
                min: 0.5,
                max: 12.0
            },
            modifier: SoundModifier::None,
            source_path: "C:/audio/voice line.wav".to_owned(),
        }
    );
    assert_eq!(
        layer_at(doc, "Root/Voice").opacity,
        0.5,
        "the layer's volume"
    );
    let media = doc.media(voice.media.unwrap()).unwrap();
    assert_eq!(
        (media.name.as_str(), media.kind),
        ("voice line.wav", MediaKind::Audio)
    );
    assert_eq!(
        media.data,
        MediaData::RawAudio {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            float: false,
            samples: Arc::from(pcm.as_slice()),
        }
    );
    assert_eq!(sound_at(doc, "Root/Again").media, voice.media);

    let ambience = sound_at(doc, "Root/Ambience");
    assert_eq!(ambience.kind, SoundKind::Ambisonic);
    assert_eq!(ambience.attenuation.mode, AttenuationMode::Logarithmic);
    assert_eq!(
        doc.media(ambience.media.unwrap()).unwrap().data,
        MediaData::RawAudio {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 32,
            float: true,
            samples: Arc::from(floats.as_slice()),
        }
    );

    let music = sound_at(doc, "Root/Music");
    assert_eq!(
        (music.kind, music.gain, music.looped),
        (SoundKind::Flat, 1.0, false)
    );
    assert_eq!(
        doc.media(music.media.unwrap()).unwrap().data,
        MediaData::Encoded {
            format: "wav".to_owned(),
            bytes: Arc::from(wav.as_slice()),
        }
    );
    let looped = doc
        .media(sound_at(doc, "Root/Loop").media.unwrap())
        .unwrap();
    assert_eq!(
        looped.name, "Loop",
        "named after the layer when no path is stored"
    );
    assert_eq!(
        looped.data,
        MediaData::Encoded {
            format: "ogg".to_owned(),
            bytes: Arc::from(ogg.as_slice()),
        }
    );
    assert_eq!(doc.media_items().count(), 4);
}

#[test]
fn sound_modifiers_are_read_tolerantly_and_reported() {
    let mut qbin = QbinBuilder::default();
    let audio = qbin.sound_wave(1, 16, 8_000, &[0; 8]);
    let with_modifiers = |name: &str, modifiers: Value| {
        let mut layer = sound(name, &audio, "");
        layer["Implementation"]["Type"] = json!("Positional");
        layer["Implementation"]["Modifiers"] = modifiers;
        layer
    };

    let quill = project(vec![
        // Radians, in the names Quill's runtime uses for these values.
        with_modifiers(
            "Cone",
            json!([{"Type": "DirectionalCone", "AngleIn": 0.5, "AngleBand": 0.25, "AttenOut": 0.1}]),
        ),
        // Degrees, nested, in another plausible spelling; one reader spells the type "DirectionFrus".
        with_modifiers(
            "Frustum",
            json!([{"Type": "None"}, {"Type": "DirectionFrus", "Parameters": {
                "InnerAngleX": 90.0, "InnerAngleY": 45.0, "Band": 10.0, "OuterAttenuation": 0.2}}]),
        ),
        with_modifiers("Bare", json!([{"Type": "DirectionalCone"}])),
        with_modifiers("Mystery", json!([{"Type": "Doppler", "Amount": 2.0}])),
        with_modifiers(
            "Two",
            json!([{"Type": "DirectionalCone", "AngleIn": 1.0}, {"Type": "DirectionalFrus"}]),
        ),
    ]);
    let imported = import(&quill, &qbin.bytes);
    let doc = &imported.document;

    assert_eq!(
        sound_at(doc, "Root/Cone").modifier,
        SoundModifier::Cone {
            inner: 0.5,
            band: 0.25,
            outside: 0.1
        }
    );
    assert_eq!(
        sound_at(doc, "Root/Frustum").modifier,
        SoundModifier::Frustum {
            inner_h: 90f64.to_radians() as f32,
            inner_v: 45f64.to_radians() as f32,
            band: 10f64.to_radians() as f32,
            outside: 0.2
        }
    );
    assert_eq!(sound_at(doc, "Root/Bare").modifier, SoundModifier::None);
    assert_eq!(sound_at(doc, "Root/Mystery").modifier, SoundModifier::None);
    assert_eq!(
        sound_at(doc, "Root/Two").modifier,
        SoundModifier::Cone {
            inner: 1.0,
            band: 0.0,
            outside: 0.0
        }
    );

    let all = imported.warnings.join("\n");
    for expected in [
        "Root/Cone: the sound's directional cone was read from parameter names no Quill file has confirmed (angles taken as radians)",
        "Root/Frustum: the sound's directional frustum was read from parameter names no Quill file has confirmed (angles taken as degrees)",
        "Root/Bare: the sound's directional cone has no parameters this importer recognises; the sound plays in all directions",
        "Root/Mystery: unknown sound modifier \"Doppler\" was not imported",
        "Root/Two: the sound has 2 directional modifiers; only the first is imported",
        "default spawn area",
    ] {
        assert!(all.contains(expected), "missing {expected:?} in:\n{all}");
    }
    assert_eq!(imported.warnings.len(), 7, "{all}");
}

#[test]
fn unreadable_sounds_are_kept_silent_with_a_warning() {
    let mut qbin = QbinBuilder::default();
    let not_a_sound = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 4))]);
    let cut = qbin.sound_wave(2, 16, 44_100, &[0; 64]);
    let keep = qbin.bytes.len() - 1;

    let mut odd = sound("Odd", &cut, "");
    odd["Implementation"]["Type"] = json!("Binaural");
    odd["Implementation"]["Gain"] = json!(-3.0);
    odd["Implementation"]["Attenuation"] =
        json!({"Mode": "Cubic", "Minimum": -1.0, "Maximum": "far"});
    let quill = project(vec![sound("Wrong", &not_a_sound, ""), odd]);
    let imported = import(&quill, &qbin.bytes[..keep]);
    let doc = &imported.document;

    assert_eq!(sound_at(doc, "Root/Wrong").media, None);
    let odd = sound_at(doc, "Root/Odd");
    assert_eq!(odd.media, None);
    assert_eq!((odd.kind, odd.gain), (SoundKind::Flat, 1.0));
    assert_eq!(odd.attenuation, Attenuation::default());

    let all = imported.warnings.join("\n");
    for expected in [
        "Root/Wrong: the sound's audio could not be read (the record matches no known sound layout); the layer is kept silent",
        "Root/Odd: the sound's audio could not be read (the record runs past the end of Quill.qbin); the layer is kept silent",
        "Root/Odd: unknown sound type \"Binaural\" read as flat",
        "Root/Odd: unknown attenuation mode \"Cubic\"; the sound does not fade with distance",
    ] {
        assert!(all.contains(expected), "missing {expected:?} in:\n{all}");
    }
}

fn image_of(doc: &Document, picture: &Picture) -> (u32, u32, u8, Vec<u8>) {
    match &doc
        .media(picture.media.expect("the picture has pixels"))
        .unwrap()
        .data
    {
        MediaData::RawImage {
            width,
            height,
            channels,
            pixels,
        } => (*width, *height, *channels, pixels.to_vec()),
        other => panic!("not an image: {other:?}"),
    }
}

#[test]
fn imports_pictures_of_every_kind() {
    let mut qbin = QbinBuilder::default();
    // Two rows that differ, so a flipped import would show.
    let rgba_pixels = [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 9, 9, 9, 0];
    let rgba = qbin.picture_rgba(2, 2, &rgba_pixels);
    let wide = qbin.picture_rgb(4, 2, &[7; 24]);
    let tall = qbin.picture_rgb(1, 6, &[3; 18]);
    let cross = qbin.picture_rgb(4, 3, &[5; 36]);
    let plain = qbin.picture_rgb(1, 1, &[1, 2, 3]);

    let mut locked = picture("Cutout", "2D", &rgba, "C:/Users/me/Desktop/cutout.png");
    locked["Implementation"]["ViewerLocked"] = json!(true);
    let quill = project(vec![
        locked,
        picture("Sky", "360 Equirectangular Mono", &wide, "C:/sky.jpg"),
        picture("Sky3D", "360 Equirectangular Stereo", &wide, "C:/sky.jpg"),
        picture("Depth", "2DDepth", &plain, ""),
        picture("Cross", "360 Cubemap Cross Mono", &cross, ""),
        picture("Strip", "360 Cubemap Vstrip Mono", &tall, ""),
        picture("CubeWide", "360 Cubemap", &cross, ""),
        picture("CubeTall", "Cubemap", &tall, ""),
        picture("Hologram", "Lightfield", &wide, ""),
    ]);
    let imported = import(&quill, &qbin.bytes);
    let doc = &imported.document;

    let cutout = picture_at(doc, "Root/Cutout");
    assert_eq!(cutout.projection, Projection::Flat);
    assert!(cutout.viewer_locked);
    assert_eq!(cutout.source_path, "C:/Users/me/Desktop/cutout.png");
    assert_eq!(doc.media(cutout.media.unwrap()).unwrap().name, "cutout.png");
    assert_eq!(
        image_of(doc, cutout),
        (2, 2, 4, rgba_pixels.to_vec()),
        "rows are stored top to bottom and are not flipped"
    );

    let sky = picture_at(doc, "Root/Sky");
    assert_eq!(sky.projection, Projection::Equirect);
    assert!(!sky.viewer_locked);
    let stereo = picture_at(doc, "Root/Sky3D");
    assert_eq!(stereo.projection, Projection::EquirectStereo);
    assert_eq!(stereo.media, sky.media, "one file record, one image");
    assert_eq!(image_of(doc, sky).0, 4);

    for (path, projection) in [
        ("Root/Depth", Projection::Flat),
        ("Root/Cross", Projection::CubemapCross),
        ("Root/Strip", Projection::CubemapStrip),
        ("Root/CubeWide", Projection::CubemapCross),
        ("Root/CubeTall", Projection::CubemapStrip),
        ("Root/Hologram", Projection::Flat),
    ] {
        assert_eq!(picture_at(doc, path).projection, projection, "{path}");
    }
    assert_eq!(
        doc.media(picture_at(doc, "Root/Depth").media.unwrap())
            .unwrap()
            .name,
        "Depth",
        "named after the layer when no path is stored"
    );

    let mut expected = vec![
        "Root/Depth: picture type \"2DDepth\" is shown as a flat image; its depth is not imported".to_owned(),
        "Root/Cross: picture type \"360 Cubemap Cross Mono\" is not a name this importer knows; it is shown as a cubemap cross".to_owned(),
        "Root/Strip: picture type \"360 Cubemap Vstrip Mono\" is not a name this importer knows; it is shown as a cubemap strip".to_owned(),
        "Root/CubeWide: picture type \"360 Cubemap\" is not a name this importer knows; it is shown as a cubemap cross".to_owned(),
        "Root/CubeTall: picture type \"Cubemap\" is not a name this importer knows; it is shown as a cubemap strip".to_owned(),
        "Root/Hologram: picture type \"Lightfield\" is not a name this importer knows; it is shown as a flat image".to_owned(),
        "the default spawn area \"Root/InitialSpawnArea\" was not found".to_owned(),
    ];
    let mut warnings = imported.warnings.clone();
    expected.sort();
    warnings.sort();
    assert_eq!(warnings, expected);
}

#[test]
fn unreadable_pictures_are_reported() {
    let mut qbin = QbinBuilder::default();
    let float_pixels = qbin.picture(10, 1, 1, &[0; 12]);
    let cut = qbin.picture_rgb(2, 2, &[1; 12]);
    let keep = qbin.bytes.len() - 1;

    let mut spawn = layer(
        "InitialSpawnArea",
        "Viewpoint",
        json!({"Version": 1, "DataFileOffset": cut, "TypeStr": "EyeLevel"}),
    );
    spawn["Visible"] = json!(false);
    let mut no_offset = picture("NoOffset", "2D", "", "");
    no_offset["Implementation"]["DataFileOffset"] = json!("not hex");
    let quill = project(vec![
        spawn,
        picture("Float", "2D", &float_pixels, ""),
        picture("Cut", "2D", &cut, ""),
        no_offset,
    ]);
    let imported = import(&quill, &qbin.bytes[..keep]);
    let doc = &imported.document;
    for path in ["Root/Float", "Root/Cut", "Root/NoOffset"] {
        assert_eq!(picture_at(doc, path).media, None, "{path}");
    }
    let LayerKind::Viewpoint(viewpoint) = &layer_at(doc, "Root/InitialSpawnArea").kind else {
        panic!("not a spawn area")
    };
    assert!(!viewpoint.floor_level);
    assert_eq!(viewpoint.thumbnail, None);

    let mut warnings = imported.warnings.clone();
    warnings.sort();
    assert_eq!(
        warnings,
        [
            "Root/Cut: the picture could not be read (the record runs past the end of Quill.qbin)",
            "Root/Float: the picture could not be read (pixel format 10 is not supported)",
            "Root/InitialSpawnArea: the spawn area thumbnail could not be read (the record runs past the end of Quill.qbin)",
            "Root/NoOffset: the picture has no readable file offset and is empty",
        ]
    );
}

#[test]
fn a_project_without_binary_data_says_so_once() {
    let mut qbin = QbinBuilder::default();
    let pixels = qbin.picture_rgb(1, 1, &[1, 2, 3]);
    let mesh = qbin.mesh(&only_strip());
    let audio = qbin.sound_wave(1, 16, 8_000, &[0; 4]);
    let strokes = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 4))]);
    let mut quill = project(vec![
        picture("Pic", "2D", &pixels, ""),
        model("Mesh", &mesh, ""),
        sound("Snd", &audio, ""),
        paint("Paint", &[&strokes], &[0]),
    ]);
    quill["Sequence"]["DefaultViewpoint"] = json!("");

    // No file at all.
    let dir = tempfile::tempdir().unwrap();
    write_project(dir.path(), &quill, &[], None);
    std::fs::remove_file(dir.path().join("Quill.qbin")).unwrap();
    let missing = import_project(dir.path()).unwrap();
    assert_eq!(
        missing.warnings,
        ["Quill.qbin is missing; all drawings and media are empty"]
    );
    missing.document.validate().unwrap();
    assert_eq!(missing.document.layer_count(), 5);
    assert_eq!(missing.document.media_items().count(), 0);

    // A file with nothing in it.
    let empty = import(&quill, &[]);
    assert_eq!(
        empty.warnings,
        ["Quill.qbin is empty; all drawings and media are empty"]
    );

    // The whole file imports cleanly, to show the layers above were otherwise sound.
    let whole = import(&quill, &qbin.bytes);
    assert_eq!(whole.warnings, Vec::<String>::new());
    assert_eq!(whole.document.media_items().count(), 3);
    assert_eq!(whole.document.stroke_count(), 1);
}
