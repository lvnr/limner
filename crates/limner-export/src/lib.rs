//! Exporters: Limner documents to interchange formats.
//!
//! Every format is written from an [`ExportScene`], which [`build_scene`] produces by
//! evaluating the document over a time range. Writers therefore contain no timeline
//! logic, and all formats agree on what a document looks like at a given moment.
//!
//! * [`export_glb`] / [`write_glb`]: glTF 2.0 binary, with hierarchy, animation,
//!   lights and cameras.
//! * [`export_obj`] / [`write_obj`]: Wavefront OBJ, a still in world space.
//! * [`export_usda`] / [`write_usda`]: a USD text layer, with hierarchy, animation,
//!   lights, cameras, preview materials and sounds (audio files written beside it).
//! * [`export_usdz`] / [`write_usdz`]: the same layer in a USDZ package, with the
//!   audio files inside.
//! * [`export_fbx`] / [`write_fbx`]: binary FBX 7.4, with hierarchy, animation,
//!   lights and cameras.
//!
//! `docs/export.md` describes how each Limner concept maps to each format.

mod audio;
mod common;
pub mod fbx;
pub mod gltf;
pub mod obj;
pub mod scene;
pub mod usd;

pub use fbx::{scene_to_fbx, FbxColorSpace, FbxOptions, FbxUnit};
pub use gltf::{scene_to_glb, GlbOutput, GltfOptions};
pub use obj::scene_to_obj;
pub use scene::{
    build_scene, tessellate_drawing, ExportAudio, ExportCamera, ExportDrawing, ExportLight,
    ExportMedia, ExportMesh, ExportNode, ExportOptions, ExportPaint, ExportPlay, ExportScene,
    ExportSound, NodeContent, NodeKind, Sampled, MAX_FRAMES,
};
pub use usd::{
    scene_to_usda, scene_to_usda_with_assets, scene_to_usdz, usdz_package, UsdAsset, UsdOptions,
    UsdaOutput, USDZ_AUDIO_FORMATS,
};

use limner_core::model::Document;
use limner_core::time::Flicks;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the export range ends ({end}) before it starts ({start})")]
    InvalidRange { start: Flicks, end: Flicks },
    #[error("the sampling rate {num}/{den} is not a frame rate")]
    InvalidRate { num: u32, den: u32 },
    #[error(
        "the range holds {frames} frames at this rate, more than the {max} an export may \
         have; check the frame rate and the start and end"
    )]
    TooManyFrames { frames: u64, max: usize },
    #[error("the export would be {bytes} bytes, more than the 4 GiB a .glb can hold")]
    TooLarge { bytes: usize },
    #[error(
        "the {format} export would be {bytes} bytes, more than the 4 GiB the format can address"
    )]
    FileTooLarge { format: &'static str, bytes: usize },
    #[error("could not encode the glTF description: {0}")]
    Json(#[source] serde_json::Error),
    #[error("could not write {}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// What an export contains.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Nodes (glTF), prims that stand for a layer or a drawing (USD), models (FBX)
    /// or objects (OBJ) written.
    pub nodes: usize,
    /// Distinct meshes written. A drawing has up to four: single- or double-sided,
    /// opaque or see-through. (FBX joins a drawing's meshes into one geometry with a
    /// material slot each; they are still counted one by one.)
    pub meshes: usize,
    pub vertices: usize,
    pub triangles: usize,
    /// Frames the animation was sampled on; zero for a still.
    pub animation_frames: usize,
    /// Animation channels left after constant ones were dropped: glTF channels,
    /// USD properties with time samples, FBX curve nodes.
    pub animation_channels: usize,
    /// Sounds written: `SpatialAudio` prims in USD, one per stretch of the range in
    /// which a sound layer plays. Zero for formats that hold no sound.
    pub sounds: usize,
    /// Size of the file. For a `.usda`, the layer alone, not the audio files beside it.
    pub bytes: u64,
    /// What was left out and why, in plain words: sounds a format cannot hold, audio
    /// a package may not contain. An export with warnings still succeeded.
    pub warnings: Vec<String>,
}

fn check(options: &ExportOptions) -> Result<(), ExportError> {
    if options.end < options.start {
        return Err(ExportError::InvalidRange {
            start: options.start,
            end: options.end,
        });
    }
    if options.animation && options.end > options.start && !options.rate.is_valid() {
        return Err(ExportError::InvalidRate {
            num: options.rate.num,
            den: options.rate.den,
        });
    }
    let frames = options.frame_count();
    if frames > MAX_FRAMES as u64 {
        return Err(ExportError::TooManyFrames {
            frames,
            max: MAX_FRAMES,
        });
    }
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    write_with(path, |file| file.write_all(bytes))
}

/// Writes a file so that it is either the whole new file or still the old one: the
/// bytes go to a hidden file beside it, which is flushed to disk and then renamed over
/// the target. A failure part way (a full disk, say) leaves the earlier export as it
/// was and removes the partial file.
fn write_with(
    path: &Path,
    fill: impl FnOnce(&mut std::fs::File) -> std::io::Result<()>,
) -> Result<(), ExportError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let io = |source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    };
    let Some(name) = path.file_name() else {
        return Err(io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the path names no file",
        )));
    };
    let mut temporary = OsString::from(".");
    temporary.push(name);
    temporary.push(format!(
        ".{}-{}.part",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let temporary = path.with_file_name(temporary);
    let result = std::fs::File::create(&temporary).and_then(|mut file| {
        fill(&mut file)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result.map_err(io)
}

/// Exports the document as glTF 2.0 binary and says what went into it.
pub fn export_glb_with_report(
    doc: &Document,
    options: &ExportOptions,
    extra: &GltfOptions,
) -> Result<GlbOutput, ExportError> {
    check(options)?;
    scene_to_glb(&build_scene(doc, options), extra)
}

/// Exports the document as glTF 2.0 binary (`.glb`), in memory.
pub fn export_glb(
    doc: &Document,
    options: &ExportOptions,
    extra: &GltfOptions,
) -> Result<Vec<u8>, ExportError> {
    export_glb_with_report(doc, options, extra).map(|output| output.bytes)
}

/// Exports the document as a `.glb` file.
pub fn write_glb(
    doc: &Document,
    path: &Path,
    options: &ExportOptions,
    extra: &GltfOptions,
) -> Result<ExportReport, ExportError> {
    let output = export_glb_with_report(doc, options, extra)?;
    write(path, &output.bytes)?;
    Ok(output.report)
}

/// Exports the document at `time` as Wavefront OBJ text and says what went into it.
pub fn export_obj_with_report(
    doc: &Document,
    time: Flicks,
    include_hidden: bool,
) -> Result<(String, ExportReport), ExportError> {
    let options = ExportOptions {
        include_hidden,
        bake_transforms: true,
        lights: false,
        cameras: false,
        ..ExportOptions::still(time)
    };
    Ok(scene_to_obj(&build_scene(doc, &options)))
}

/// Exports the document at `time` as Wavefront OBJ text: world-space, vertex-coloured
/// triangles, one object per paint layer.
pub fn export_obj(
    doc: &Document,
    time: Flicks,
    include_hidden: bool,
) -> Result<String, ExportError> {
    export_obj_with_report(doc, time, include_hidden).map(|(text, _)| text)
}

/// Exports the document at `time` as an `.obj` file.
pub fn write_obj(
    doc: &Document,
    path: &Path,
    time: Flicks,
    include_hidden: bool,
) -> Result<ExportReport, ExportError> {
    let (text, report) = export_obj_with_report(doc, time, include_hidden)?;
    write(path, text.as_bytes())?;
    Ok(report)
}

/// Exports the document as a USD text layer and says what went into it. The layer
/// refers to the audio of sound layers as files under `audio/` beside it; use
/// [`export_usda_with_assets`] to have them as well.
pub fn export_usda_with_report(
    doc: &Document,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<(String, ExportReport), ExportError> {
    export_usda_with_assets(doc, options, extra).map(|output| (output.text, output.report))
}

/// Exports the document as a USD text layer together with the files it refers to.
pub fn export_usda_with_assets(
    doc: &Document,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<UsdaOutput, ExportError> {
    check(options)?;
    scene_to_usda_with_assets(&build_scene(doc, options), extra)
}

/// Exports the document as a USD text layer (`.usda`), in memory.
pub fn export_usda(
    doc: &Document,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<String, ExportError> {
    export_usda_with_report(doc, options, extra).map(|(text, _)| text)
}

/// Exports the document as a `.usda` file. The audio of sound layers is written
/// beside it, in a folder named `audio`; files of the same name there are replaced.
pub fn write_usda(
    doc: &Document,
    path: &Path,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<ExportReport, ExportError> {
    let output = export_usda_with_assets(doc, options, extra)?;
    write(path, output.text.as_bytes())?;
    let folder = path.parent().unwrap_or(Path::new(""));
    for asset in &output.assets {
        let target = folder.join(&asset.path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|source| ExportError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        write(&target, &asset.bytes)?;
    }
    Ok(output.report)
}

/// Exports the document as a USDZ package and says what went into it.
pub fn export_usdz_with_report(
    doc: &Document,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<(Vec<u8>, ExportReport), ExportError> {
    check(options)?;
    scene_to_usdz(&build_scene(doc, options), extra)
}

/// Exports the document as a USDZ package (`.usdz`), in memory.
pub fn export_usdz(
    doc: &Document,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<Vec<u8>, ExportError> {
    export_usdz_with_report(doc, options, extra).map(|(bytes, _)| bytes)
}

/// Exports the document as a `.usdz` file.
pub fn write_usdz(
    doc: &Document,
    path: &Path,
    options: &ExportOptions,
    extra: &UsdOptions,
) -> Result<ExportReport, ExportError> {
    let (bytes, report) = export_usdz_with_report(doc, options, extra)?;
    write(path, &bytes)?;
    Ok(report)
}

/// Exports the document as binary FBX and says what went into it.
pub fn export_fbx_with_report(
    doc: &Document,
    options: &ExportOptions,
    extra: &FbxOptions,
) -> Result<(Vec<u8>, ExportReport), ExportError> {
    check(options)?;
    scene_to_fbx(&build_scene(doc, options), extra)
}

/// Exports the document as binary FBX 7.4 (`.fbx`), in memory.
pub fn export_fbx(
    doc: &Document,
    options: &ExportOptions,
    extra: &FbxOptions,
) -> Result<Vec<u8>, ExportError> {
    export_fbx_with_report(doc, options, extra).map(|(bytes, _)| bytes)
}

/// Exports the document as an `.fbx` file.
pub fn write_fbx(
    doc: &Document,
    path: &Path,
    options: &ExportOptions,
    extra: &FbxOptions,
) -> Result<ExportReport, ExportError> {
    let (bytes, report) = export_fbx_with_report(doc, options, extra)?;
    write(path, &bytes)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("limner-export-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn listing(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_write_that_fails_part_way_leaves_the_earlier_file() {
        let dir = scratch("atomic");
        let path = dir.join("scene.glb");
        write(&path, b"the good export").unwrap();
        assert_eq!(listing(&dir), ["scene.glb"], "nothing is left beside it");

        let failed = write_with(&path, |file| {
            file.write_all(b"half of the ne")?;
            Err(std::io::Error::other("the disk is full"))
        });
        assert!(matches!(failed, Err(ExportError::Io { path: p, .. }) if p == path));
        assert_eq!(std::fs::read(&path).unwrap(), b"the good export");
        assert_eq!(listing(&dir), ["scene.glb"], "the partial file is removed");

        write(&path, b"a newer export").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"a newer export");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_write_that_cannot_replace_its_target_cleans_up() {
        let dir = scratch("blocked");
        // A folder where the file should go cannot be replaced by a file.
        let path = dir.join("scene.glb");
        std::fs::create_dir_all(path.join("inside")).unwrap();
        assert!(matches!(
            write(&path, b"bytes"),
            Err(ExportError::Io { .. })
        ));
        assert_eq!(listing(&dir), ["scene.glb"]);
        assert!(path.join("inside").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_range_with_too_many_frames_is_refused() {
        let doc = Document::empty();
        let second = limner_core::time::from_seconds(1.0);
        let typo =
            ExportOptions::animated(0, 10 * second, limner_core::time::FrameRate::whole(24_000));
        assert_eq!(typo.frame_count(), 240_001);
        match export_glb(&doc, &typo, &GltfOptions::default()) {
            Err(ExportError::TooManyFrames { frames, max }) => {
                assert_eq!((frames, max), (240_001, MAX_FRAMES));
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // The limit itself is allowed.
        let rate = limner_core::time::FrameRate::whole(24);
        let longest = ExportOptions::animated(0, rate.frame_start(MAX_FRAMES as i64 - 1), rate);
        assert_eq!(longest.frame_count(), MAX_FRAMES as u64);
        assert!(check(&longest).is_ok());
        // A range to the end of time is refused rather than sampled for ever.
        let endless = ExportOptions::animated(0, Flicks::MAX, rate);
        assert!(matches!(
            check(&endless),
            Err(ExportError::TooManyFrames { .. })
        ));
    }
}
