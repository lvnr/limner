//! Sound layers in every format: `SpatialAudio` prims and audio files in USD and
//! USDZ, and a warning from the formats that cannot hold sound.

mod support;

use limner_core::glam::DVec3;
use limner_core::math::Xform;
use limner_core::model::{
    Attenuation, AttenuationMode, Document, Interp, Key, LayerKind, MediaData, MediaKind,
    SoundKind, SoundModifier, Track,
};
use limner_export::{
    build_scene, export_fbx_with_report, export_glb_with_report, export_obj_with_report,
    export_usda_with_assets, export_usda_with_report, export_usdz_with_report, write_usda,
    ExportOptions, FbxOptions, GltfOptions, NodeContent, UsdOptions,
};
use std::sync::Arc;
use support::fbx::Fbx;
use support::stage::{stage, STAGE_LAST};
use support::usda::{numbers, unquote, Usda};
use support::zip::read_usdz;
use support::{audio, group, sequence, sound, wav, Glb, FRAME, RATE};

/// The last frame of the range the sound documents are exported over.
const LAST: i64 = 12;

/// Fake MP3 and Ogg files: the exporter copies audio without reading it.
fn mp3() -> Vec<u8> {
    let mut bytes = b"ID3\x04\x00\x00\x00\x00\x00\x00".to_vec();
    bytes.resize(64, 0);
    bytes
}

fn ogg() -> Vec<u8> {
    let mut bytes = b"OggS".to_vec();
    bytes.resize(48, 0);
    bytes
}

/// "Garden" (moved to (1, 2, 3)) holding "Rain": a positional WAV of 0.25 s, not
/// looped, in a clip from frame 2 to frame 10, with linear attenuation and a cone.
/// "Wind": a flat, looping MP3 with no clips, fading from full opacity to half over
/// the range, at a gain of 0.8. "Bell": a positional Ogg file. "Silent": a sound
/// layer without media.
fn sounds() -> Document {
    let mut doc = Document::empty();
    doc.settings.title = "Sounds".to_owned();
    let root = doc.root();

    let garden = group(&mut doc, root, "Garden");
    doc.layer_mut(garden).unwrap().transform = Xform::from_translation(DVec3::new(1.0, 2.0, 3.0));
    // 2000 samples at 8000 per second: exactly six frames at 24 fps.
    let rain_audio = audio(&mut doc, "rain.wav", "wav", wav(2000, 8000));
    let rain = sound(
        &mut doc,
        garden,
        "Rain",
        Some(rain_audio),
        SoundKind::Positional,
        false,
    );
    {
        let layer = doc.layer_mut(rain).unwrap();
        layer.anim.visibility =
            Track::from_keys([Key::step(2 * FRAME, true), Key::step(10 * FRAME, false)]);
        let LayerKind::Sound(sound) = &mut layer.kind else {
            unreachable!()
        };
        sound.attenuation = Attenuation {
            mode: AttenuationMode::Linear,
            min: 1.0,
            max: 4.0,
        };
        sound.modifier = SoundModifier::Cone {
            inner: 0.5,
            band: 0.25,
            outside: 0.2,
        };
    }

    let wind_audio = audio(&mut doc, "Wind.MP3", "mp3", mp3());
    let wind = sound(
        &mut doc,
        root,
        "Wind",
        Some(wind_audio),
        SoundKind::Flat,
        true,
    );
    {
        let layer = doc.layer_mut(wind).unwrap();
        layer.anim.opacity = Track::from_keys([
            Key::new(0, 1.0, Interp::Linear),
            Key::new(LAST * FRAME, 0.5, Interp::Linear),
        ]);
        let LayerKind::Sound(sound) = &mut layer.kind else {
            unreachable!()
        };
        sound.gain = 0.8;
    }

    let bell_audio = audio(&mut doc, "bell.ogg", "ogg", ogg());
    sound(
        &mut doc,
        root,
        "Bell",
        Some(bell_audio),
        SoundKind::Positional,
        false,
    );
    sound(&mut doc, root, "Silent", None, SoundKind::Flat, false);
    doc.validate().unwrap();
    doc
}

fn range() -> ExportOptions {
    ExportOptions::animated(0, LAST * FRAME, RATE)
}

fn number(value: &str) -> f64 {
    let n = numbers(value);
    assert_eq!(n.len(), 1, "{value}");
    n[0]
}

#[test]
fn the_scene_knows_when_each_sound_is_heard() {
    let scene = build_scene(&sounds(), &range());
    let plays = |name: &str| {
        let node = scene.nodes.iter().find(|n| n.name == name).unwrap();
        let NodeContent::Sound(sound) = &node.content else {
            panic!("{name} is a sound")
        };
        sound
            .plays
            .iter()
            .map(|p| (p.first, p.last, p.offset))
            .collect::<Vec<_>>()
    };
    // Heard from the clip's start until the audio runs out, two frames before the clip ends.
    assert_eq!(plays("Rain"), [(2, 7, 0.0)]);
    assert_eq!(plays("Wind"), [(0, 12, 0.0)]);
    assert_eq!(plays("Bell"), [(0, 12, 0.0)]);
    assert_eq!(plays("Silent"), [(0, 12, 0.0)]);
}

#[test]
fn each_sound_is_a_spatial_audio_prim_under_its_layer() {
    let output = export_usda_with_assets(&sounds(), &range(), &UsdOptions::default()).unwrap();
    let usda = Usda::parse(&output.text);
    assert_eq!(output.report.sounds, 3);
    assert!(
        output.report.warnings.is_empty(),
        "{:?}",
        output.report.warnings
    );

    let rain = usda.prim("/Sounds/Root/Garden/Rain/Audio");
    assert_eq!(rain.type_name(), "SpatialAudio");
    assert_eq!(rain.attr("filePath"), Some("@audio/rain.wav@"));
    assert_eq!(rain.attr_type("filePath"), Some("uniform asset"));
    assert_eq!(rain.attr("auralMode").map(unquote), Some("spatial"));
    assert_eq!(
        rain.attr("playbackMode").map(unquote),
        Some("onceFromStartToEnd")
    );
    assert_eq!(rain.attr_type("startTime"), Some("uniform timecode"));
    assert_eq!(rain.attr("startTime").map(number), Some(2.0));
    assert_eq!(rain.attr("endTime").map(number), Some(8.0));
    assert_eq!(rain.attr_type("mediaOffset"), Some("uniform double"));
    assert_eq!(rain.attr("mediaOffset").map(number), Some(0.0));
    assert_eq!(rain.attr("gain").map(number), Some(1.0));
    assert!(rain.time_samples("gain").is_none());
    // It is placed by its layer: the garden's translation reaches it.
    let world = usda.world_matrix("/Sounds/Root/Garden/Rain/Audio", 0.0);
    assert_eq!(world.translation, DVec3::new(1.0, 2.0, 3.0));

    // What USD has no place for is kept on the layer's prim.
    let layer = usda.prim("/Sounds/Root/Garden/Rain");
    assert_eq!(layer.type_name(), "Xform");
    let custom = |key: &str| layer.metadata_value(key).unwrap();
    assert_eq!(unquote(custom("kind")), "Sound");
    assert_eq!(unquote(custom("soundKind")), "positional");
    assert_eq!(unquote(custom("soundAttenuation")), "linear");
    assert_eq!(number(custom("soundAttenuationMin")), 1.0);
    assert_eq!(number(custom("soundAttenuationMax")), 4.0);
    assert_eq!(unquote(custom("soundModifier")), "cone");
    assert_eq!(number(custom("soundModifierInner")), 0.5);
    assert_eq!(number(custom("soundModifierBand")), 0.25);
    assert_eq!(number(custom("soundModifierOutside")), 0.2);
    assert_eq!(unquote(custom("media")), "rain.wav");
    assert_eq!(unquote(custom("sourcePath")), "/sounds/Rain");
    // The clip is also the layer's visibility, as for any layer.
    assert!(!layer.visible_at(1.0) && layer.visible_at(2.0) && !layer.visible_at(10.0));

    let wind = usda.prim("/Sounds/Root/Wind/Audio");
    assert_eq!(wind.attr("filePath"), Some("@audio/Wind.mp3@"));
    assert_eq!(wind.attr("auralMode").map(unquote), Some("nonSpatial"));
    assert_eq!(
        wind.attr("playbackMode").map(unquote),
        Some("loopFromStart")
    );
    assert_eq!(wind.attr("startTime").map(number), Some(0.0));
    assert!(wind.attr("endTime").is_none(), "it plays on past the range");
    // Gain times the fading opacity, frame by frame.
    let samples = wind
        .time_samples("gain")
        .expect("the gain follows the opacity");
    assert_eq!(number(samples[0].1), 0.8);
    assert!((number(samples.last().unwrap().1) - 0.4).abs() < 1e-6);
    let halfway = wind.sample_numbers("gain", 6.0, false).unwrap()[0];
    assert!((halfway - 0.6).abs() < 1e-6, "{halfway}");

    let bell = usda.prim("/Sounds/Root/Bell/Audio");
    assert_eq!(bell.attr("filePath"), Some("@audio/bell.ogg@"));
    // A sound layer without media keeps its prim and has nothing to play.
    assert!(usda.prim("/Sounds/Root/Silent").children().is_empty());

    let paths: Vec<&str> = output.assets.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(
        paths,
        ["audio/rain.wav", "audio/Wind.mp3", "audio/bell.ogg"]
    );
    assert_eq!(&output.assets[0].bytes[..], &wav(2000, 8000)[..]);
    assert_eq!(&output.assets[1].bytes[..], &mp3()[..]);
    assert_eq!(&output.assets[2].bytes[..], &ogg()[..]);
}

#[test]
fn two_layers_playing_the_same_audio_share_one_file() {
    let mut doc = Document::empty();
    let root = doc.root();
    let media = audio(&mut doc, "loop.wav", "wav", wav(800, 8000));
    sound(&mut doc, root, "A", Some(media), SoundKind::Flat, true);
    sound(
        &mut doc,
        root,
        "B",
        Some(media),
        SoundKind::Positional,
        true,
    );
    let other = audio(&mut doc, "loop.wav", "wav", wav(80, 8000));
    sound(&mut doc, root, "C", Some(other), SoundKind::Flat, true);
    let output = export_usda_with_assets(&doc, &range(), &UsdOptions::default()).unwrap();
    let paths: Vec<&str> = output.assets.iter().map(|a| a.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "audio/loop.wav",
            format!("audio/loop_{}.wav", other.0).as_str()
        ]
    );
    let usda = Usda::parse(&output.text);
    assert_eq!(
        usda.prim("/Limner_scene/Root/A/Audio").attr("filePath"),
        Some("@audio/loop.wav@")
    );
    assert_eq!(
        usda.prim("/Limner_scene/Root/B/Audio").attr("filePath"),
        Some("@audio/loop.wav@")
    );
    assert_eq!(output.report.sounds, 3);
}

#[test]
fn a_sequence_that_loops_back_restarts_its_sound() {
    let mut doc = Document::empty();
    let root = doc.root();
    let looping = sequence(&mut doc, root, "Loop", 4 * FRAME, 0);
    // One second of audio: longer than the loop, so each pass is cut by the next.
    let media = audio(&mut doc, "chime.wav", "wav", wav(8000, 8000));
    sound(
        &mut doc,
        looping,
        "Chime",
        Some(media),
        SoundKind::Positional,
        false,
    );
    let text = export_usda_with_report(&doc, &range(), &UsdOptions::default())
        .unwrap()
        .0;
    let usda = Usda::parse(&text);
    let chime = usda.prim("/Limner_scene/Root/Loop/Chime");
    let names: Vec<&str> = chime.children().iter().map(|c| c.name()).collect();
    assert_eq!(names, ["Audio_000", "Audio_001", "Audio_002", "Audio_003"]);
    for (i, prim) in chime.children().iter().enumerate() {
        assert_eq!(prim.attr("startTime").map(number), Some(4.0 * i as f64));
        assert_eq!(prim.attr("mediaOffset").map(number), Some(0.0));
        if i < 3 {
            assert_eq!(prim.attr("endTime").map(number), Some(4.0 * (i + 1) as f64));
            assert_eq!(
                prim.attr("playbackMode").map(unquote),
                Some("onceFromStartToEnd")
            );
        } else {
            // The last pass starts on the last frame and goes on past the range.
            assert!(prim.attr("endTime").is_none());
            assert_eq!(
                prim.attr("playbackMode").map(unquote),
                Some("onceFromStart")
            );
        }
    }
}

#[test]
fn a_range_that_starts_inside_a_clip_starts_inside_the_audio() {
    let mut doc = Document::empty();
    let root = doc.root();
    // Six frames of audio, looping; the range starts eight frames in.
    let media = audio(&mut doc, "hum.wav", "wav", wav(2000, 8000));
    sound(&mut doc, root, "Hum", Some(media), SoundKind::Flat, true);
    let options = ExportOptions::animated(8 * FRAME, 12 * FRAME, RATE);
    let text = export_usda_with_report(&doc, &options, &UsdOptions::default())
        .unwrap()
        .0;
    let usda = Usda::parse(&text);
    let hum = usda.prim("/Limner_scene/Root/Hum/Audio");
    assert_eq!(hum.attr("startTime").map(number), Some(0.0));
    // Two frames into its second pass.
    let offset = hum.attr("mediaOffset").map(number).unwrap();
    assert!((offset - 2.0 / 24.0).abs() < 1e-9, "{offset}");
    assert_eq!(hum.attr("playbackMode").map(unquote), Some("loopFromStart"));
}

#[test]
fn audio_held_as_samples_is_written_as_wav() {
    let mut doc = Document::empty();
    let root = doc.root();
    // Half a second: 50 one-byte samples at 100 per second.
    let media = doc.add_media(
        "voice",
        MediaKind::Audio,
        MediaData::RawAudio {
            channels: 1,
            sample_rate: 100,
            bits_per_sample: 8,
            float: false,
            samples: Arc::from(vec![128u8; 50]),
        },
    );
    sound(&mut doc, root, "Voice", Some(media), SoundKind::Flat, false);
    let options = ExportOptions::animated(0, 24 * FRAME, RATE);
    let output = export_usda_with_assets(&doc, &options, &UsdOptions::default()).unwrap();
    assert_eq!(output.assets.len(), 1);
    let file = &output.assets[0];
    assert_eq!(file.path, "audio/voice.wav");
    assert_eq!(&file.bytes[..4], b"RIFF");
    assert_eq!(&file.bytes[8..16], b"WAVEfmt ");
    assert_eq!(file.bytes.len(), 44 + 50);
    // Its length is known, so the play ends when the audio does: after 12 frames.
    let usda = Usda::parse(&output.text);
    let voice = usda.prim("/Limner_scene/Root/Voice/Audio");
    assert_eq!(voice.attr("endTime").map(number), Some(12.0));
}

#[test]
fn a_package_holds_the_audio_it_may_and_says_what_it_left_out() {
    let doc = sounds();
    let (bytes, report) = export_usdz_with_report(&doc, &range(), &UsdOptions::default()).unwrap();
    let entries = read_usdz(&bytes);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Sounds.usda", "audio/rain.wav", "audio/Wind.mp3"]);
    assert_eq!(entries[1].data, wav(2000, 8000));
    assert_eq!(entries[2].data, mp3());
    for entry in &entries {
        assert_eq!(entry.data_offset % 64, 0, "{} is aligned", entry.name);
    }
    assert_eq!(report.sounds, 2);
    assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
    let warning = &report.warnings[0];
    assert!(
        warning.contains("\"Bell\"") && warning.contains("OGG"),
        "{warning}"
    );

    let usda = Usda::parse(std::str::from_utf8(&entries[0].data).unwrap());
    assert!(
        usda.find("/Sounds/Root/Bell/Audio").is_none(),
        "nothing refers to a missing file"
    );
    assert!(
        usda.find("/Sounds/Root/Bell").is_some(),
        "the layer itself stays"
    );
    assert_eq!(
        usda.prim("/Sounds/Root/Garden/Rain/Audio").attr("filePath"),
        Some("@audio/rain.wav@")
    );
}

#[test]
fn a_usda_file_is_written_with_its_audio_beside_it() {
    let doc = sounds();
    let dir = std::env::temp_dir().join(format!("limner-sound-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("sounds.usda");
    let report = write_usda(&doc, &path, &range(), &UsdOptions::default()).unwrap();
    assert_eq!(report.sounds, 3);
    let text = std::fs::read_to_string(&path).unwrap();
    assert_eq!(report.bytes, text.len() as u64, "the layer's own size");
    assert_eq!(
        std::fs::read(dir.join("audio/rain.wav")).unwrap(),
        wav(2000, 8000)
    );
    assert_eq!(std::fs::read(dir.join("audio/Wind.mp3")).unwrap(), mp3());
    assert_eq!(std::fs::read(dir.join("audio/bell.ogg")).unwrap(), ogg());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn baking_keeps_each_sound_at_its_place_in_the_world() {
    let options = ExportOptions {
        bake_transforms: true,
        ..range()
    };
    let text = export_usda_with_report(&sounds(), &options, &UsdOptions::default())
        .unwrap()
        .0;
    let usda = Usda::parse(&text);
    assert!(
        usda.find("/Sounds/Garden").is_none() && usda.find("/Sounds/Root").is_none(),
        "groups are flattened away"
    );
    let world = usda.world_matrix("/Sounds/Rain/Audio", 0.0);
    assert_eq!(world.translation, DVec3::new(1.0, 2.0, 3.0));
}

#[test]
fn formats_without_sound_say_which_sounds_they_left_out() {
    let doc = sounds();
    let glb = export_glb_with_report(&doc, &range(), &GltfOptions::default()).unwrap();
    let checked = Glb::checked(&glb.bytes);
    let warnings = &glb.report.warnings;
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].starts_with("3 sound layers were left out (\"Rain\", \"Wind\", \"Bell\")"),
        "{}",
        warnings[0]
    );
    assert!(!checked.json.to_string().contains("KHR_audio"));
    // The node keeps the name of its audio file, as picture and model nodes do.
    let nodes = checked.json["nodes"].as_array().unwrap();
    let rain = nodes.iter().find(|n| n["name"] == "Rain").unwrap();
    assert_eq!(rain["extras"]["limner"]["media"], "rain.wav");
    assert_eq!(rain["extras"]["limner"]["sourcePath"], "/sounds/Rain");

    let (fbx, report) = export_fbx_with_report(&doc, &range(), &FbxOptions::default()).unwrap();
    Fbx::checked(&fbx);
    assert_eq!(report.warnings.len(), 1);
    assert!(report.warnings[0].contains("FBX"), "{}", report.warnings[0]);
    assert_eq!(report.sounds, 0);

    // The still at frame 0 is before Rain's clip, so only two sounds are lost.
    let (_, report) = export_obj_with_report(&doc, 0, false).unwrap();
    assert_eq!(
        report.warnings,
        ["2 sound layers were left out (\"Wind\", \"Bell\"): an .obj file holds only meshes"]
    );
}

#[test]
fn documents_without_sound_have_no_warnings() {
    let stage = stage();
    let range = ExportOptions::animated(0, STAGE_LAST * FRAME, RATE);
    let glb = export_glb_with_report(&stage.doc, &range, &GltfOptions::default()).unwrap();
    assert!(glb.report.warnings.is_empty());
    let (_, report) = export_fbx_with_report(&stage.doc, &range, &FbxOptions::default()).unwrap();
    assert!(report.warnings.is_empty());
    let (_, report) = export_usdz_with_report(&stage.doc, &range, &UsdOptions::default()).unwrap();
    assert!(report.warnings.is_empty());
    assert_eq!(report.sounds, 0);
    let output = export_usda_with_assets(&stage.doc, &range, &UsdOptions::default()).unwrap();
    assert!(output.assets.is_empty());
}

/// Runs Apple's `usdchecker` (also with `--arkit` on the package) and `usdcat` on
/// files with sounds. Ignored by default because it depends on the machine:
///
/// ```text
/// cargo test -p limner-export --test sound -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs Apple's USD tools in /usr/bin"]
fn apple_usd_tools_accept_sounds() {
    let checker = std::path::Path::new("/usr/bin/usdchecker");
    let cat = std::path::Path::new("/usr/bin/usdcat");
    if !checker.is_file() || !cat.is_file() {
        eprintln!("skipped: Apple's USD tools are not on this machine");
        return;
    }
    let doc = sounds();
    let dir = std::env::temp_dir().join(format!("limner-sound-tools-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let usda = dir.join("sounds.usda");
    write_usda(&doc, &usda, &range(), &UsdOptions::default()).unwrap();
    let usdz = dir.join("sounds.usdz");
    limner_export::write_usdz(&doc, &usdz, &range(), &UsdOptions::default()).unwrap();

    let run = |tool: &std::path::Path, args: &[&std::ffi::OsStr]| {
        let output = std::process::Command::new(tool)
            .args(args)
            .output()
            .expect("the tool runs");
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        println!(
            "{} {args:?}:\n{}\n{}",
            tool.display(),
            stdout.trim(),
            stderr.trim()
        );
        (output.status.success(), stdout)
    };
    for args in [
        vec![usda.as_os_str()],
        vec![usdz.as_os_str()],
        vec!["--arkit".as_ref(), usdz.as_os_str()],
    ] {
        let (ok, stdout) = run(checker, &args);
        assert!(
            ok && stdout.contains("Success"),
            "usdchecker {args:?}: {stdout}"
        );
    }
    for path in [&usda, &usdz] {
        let (ok, stdout) = run(cat, &["--loadOnly".as_ref(), path.as_os_str()]);
        assert!(ok && stdout.contains("OK"), "{stdout}");
        // usdcat reads the prims back as USD sees them.
        let (ok, flat) = run(cat, &["--flatten".as_ref(), path.as_os_str()]);
        assert!(ok, "{flat}");
        assert!(flat.contains("def SpatialAudio \"Audio\""), "{flat}");
        assert!(flat.contains("loopFromStart"), "{flat}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
