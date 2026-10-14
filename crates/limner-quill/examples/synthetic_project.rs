//! Writes a small Quill project that has one layer of every kind, for trying the
//! importer and the app on data no public sample provides (sound layers above all).
//!
//! ```text
//! cargo run -p limner-quill --example synthetic_project -- local/out/quill-synthetic
//! cargo run -p limner-cli -- inspect local/out/quill-synthetic
//! ```
//!
//! Every record is written in the layout the importer reads, so this shows what the
//! importer does with such a project; it is not evidence of what Quill itself writes.

use limner_quill::testing::*;
use serde_json::json;
use std::f32::consts::TAU;

/// One second of a 440 Hz tone, 16-bit mono.
fn tone(sample_rate: u32) -> Vec<u8> {
    (0..sample_rate)
        .flat_map(|i| {
            let t = i as f32 / sample_rate as f32;
            let fade = (t * 20.0).min(1.0) * ((1.0 - t) * 20.0).min(1.0);
            (((t * 440.0 * TAU).sin() * fade * 12_000.0) as i16).to_le_bytes()
        })
        .collect()
}

/// A picture whose top row is red and whose left column is brighter, so a flip shows.
fn gradient(width: u32, height: u32, channels: u32) -> Vec<u8> {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let fade = 255 - (x * 200 / width.max(1)) as u8;
            let rgb = if y == 0 {
                [fade, 0, 0]
            } else {
                [fade / 4, fade / 2, fade]
            };
            pixels.extend_from_slice(&rgb);
            if channels == 4 {
                pixels.push(if (x + y) % 2 == 0 { 255 } else { 96 });
            }
        }
    }
    pixels
}

fn main() {
    let Some(dir) = std::env::args_os().nth(1).map(std::path::PathBuf::from) else {
        eprintln!("usage: synthetic_project <output folder>");
        std::process::exit(2);
    };

    let mut qbin = QbinBuilder::default();
    let thumbnail = qbin.picture_rgb(8, 8, &gradient(8, 8, 3));
    let stroke_a = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 12))]);
    let stroke_b = qbin.drawing(&[StrokeSpec::new(0, 4, tube(0.2, 8))]);
    let photo = qbin.picture_rgb(16, 8, &gradient(16, 8, 3));
    let cutout = qbin.picture_rgba(8, 8, &gradient(8, 8, 4));
    let pyramid = qbin.mesh(&MeshSpec {
        positions: vec![
            [-0.5, 0.0, -0.5],
            [0.5, 0.0, -0.5],
            [0.5, 0.0, 0.5],
            [-0.5, 0.0, 0.5],
            [0.0, 0.8, 0.0],
        ],
        colors: Some(vec![
            [0.8, 0.1, 0.1],
            [0.1, 0.8, 0.1],
            [0.1, 0.1, 0.8],
            [0.8, 0.8, 0.1],
            [1.0, 1.0, 1.0],
        ]),
        normals: Some(vec![
            [-0.577, -0.577, -0.577],
            [0.577, -0.577, -0.577],
            [0.577, -0.577, 0.577],
            [-0.577, -0.577, 0.577],
            [0.0, 1.0, 0.0],
        ]),
        faces: vec![
            vec![0, 1, 2, 3],
            vec![0, 4, 1],
            vec![1, 4, 2],
            vec![2, 4, 3],
            vec![3, 4, 0],
        ],
    });
    let pcm = tone(22_050);
    let raw_audio = qbin.sound_wave(1, 16, 22_050, &pcm);
    let file_audio = qbin.sound_file(&wav_file(1, 16, 22_050, &pcm));

    let mut spawn = layer(
        "InitialSpawnArea",
        "Viewpoint",
        json!({"Version": 1, "Color": [0.26, 0.61, 0.95], "Sphere": [0.0, 1.0, 0.0, 2.0],
               "AllowTranslationX": true, "AllowTranslationY": true, "AllowTranslationZ": true,
               "DataFileOffset": thumbnail, "Exporting": true, "ShowingVolume": false, "TypeStr": "FloorLevel"}),
    );
    spawn["Visible"] = json!(false);

    let mut locked = picture(
        "Cutout",
        "2D",
        &cutout,
        "C:/Users/artist/Pictures/cutout.png",
    );
    locked["Implementation"]["ViewerLocked"] = json!(true);
    let mut voice = sound("Voice", &raw_audio, "C:/Users/artist/Music/voice.wav");
    voice["Implementation"]["Type"] = json!("Positional");
    voice["Implementation"]["Loop"] = json!(true);
    voice["Implementation"]["Attenuation"] =
        json!({"Mode": "Linear", "Minimum": 0.5, "Maximum": 8.0});
    let mut statue = model("Pyramid", &pyramid, "C:/Users/artist/Models/pyramid.obj");
    statue["Transform"] = xform([0.0, 0.0, 0.0, 1.0], 0.5, "N", [1.0, 0.0, -1.0]);

    let quill = project(vec![
        spawn,
        paint("Paint", &[&stroke_a, &stroke_b], &[0, 0, 1, 1]),
        picture("Photo", "2D", &photo, "C:/Users/artist/Pictures/photo.jpg"),
        picture(
            "Sky",
            "360 Equirectangular Mono",
            &photo,
            "C:/Users/artist/Pictures/photo.jpg",
        ),
        locked,
        statue,
        voice,
        sound("Music", &file_audio, "C:/Users/artist/Music/music.wav"),
        layer("Camera", "Camera", json!({"FOV": 40})),
    ]);
    let state = json!({"Quill": {
        "Rulers": {"ShowGrid": true}, "PaintLayer": "Root/Paint", "MoveLayer": "", "CameraLayer": "Root/Camera",
        "DirectManipulation": 0, "ToolName": "Paint",
        "Tool": {"BrushID": 4, "Color": [0.8, 0.5, 0.1], "DirectionalStroke": false, "Opacity": 1.0,
                 "Size": 0.01, "TransparentTaper": "None", "WidthTaper": "Pressure"},
        "ColorPalette": vec![0.82; 48]
    }});
    write_project(&dir, &quill, &qbin.bytes, Some(&state));
    println!("wrote {}", dir.display());
}
