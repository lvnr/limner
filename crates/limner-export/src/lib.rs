//! Exporters: Limner documents to interchange formats.
//!
//! Every format is written from an [`ExportScene`], which [`build_scene`] produces by
//! evaluating the document over a time range. Writers therefore contain no timeline
//! logic, and all formats agree on what a document looks like at a given moment.
//!
//! * [`export_glb`] / [`write_glb`]: glTF 2.0 binary, with hierarchy, animation,
//!   lights and cameras.
//! * [`export_obj`] / [`write_obj`]: Wavefront OBJ, a still in world space.
//!
//! `docs/export.md` describes how each Limner concept maps to each format.

pub mod gltf;
pub mod obj;
pub mod scene;

pub use gltf::{scene_to_glb, GlbOutput, GltfOptions};
pub use obj::scene_to_obj;
pub use scene::{
    build_scene, tessellate_drawing, ExportCamera, ExportDrawing, ExportLight, ExportMedia,
    ExportMesh, ExportNode, ExportOptions, ExportPaint, ExportScene, NodeContent, NodeKind,
    Sampled,
};

use limner_core::model::Document;
use limner_core::time::Flicks;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("the export range ends ({end}) before it starts ({start})")]
    InvalidRange { start: Flicks, end: Flicks },
    #[error("the sampling rate {num}/{den} is not a frame rate")]
    InvalidRate { num: u32, den: u32 },
    #[error("the export would be {bytes} bytes, more than the 4 GiB a .glb can hold")]
    TooLarge { bytes: usize },
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Nodes (glTF) or objects (OBJ) written.
    pub nodes: usize,
    /// Distinct meshes written. A drawing has up to four: single- or double-sided,
    /// opaque or see-through.
    pub meshes: usize,
    pub vertices: usize,
    pub triangles: usize,
    /// Frames the animation was sampled on; zero for a still.
    pub animation_frames: usize,
    /// Animation channels left after constant ones were dropped.
    pub animation_channels: usize,
    /// Size of the file.
    pub bytes: u64,
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
    Ok(())
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), ExportError> {
    std::fs::write(path, bytes).map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })
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
