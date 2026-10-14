//! `limner export`: a document to an interchange file.

use anyhow::{bail, Context, Result};
use clap::{Args, ValueEnum};
use limner::core::time::{self, FrameRate};
use limner::export::{
    write_fbx, write_glb, write_obj, write_usda, write_usdz, ExportOptions, ExportReport,
    FbxColorSpace, FbxOptions, FbxUnit, GltfOptions, UsdOptions,
};
use std::io::Write;
use std::path::PathBuf;

#[derive(Args)]
pub struct ExportArgs {
    /// A Quill project folder, a .limn project folder or a packed .limnz file.
    input: PathBuf,
    /// Where to write. The extension chooses the format: .glb, .obj, .usda, .usdz or .fbx.
    output: PathBuf,
    /// Export the animation over the range instead of a still at its start.
    #[arg(long)]
    animated: bool,
    /// Where the still is taken, or where the animation begins [default: the start of the working range]
    #[arg(long, value_name = "SECONDS", allow_negative_numbers = true)]
    start: Option<f64>,
    /// Where the animation ends [default: the end of the working range]
    #[arg(long, value_name = "SECONDS", allow_negative_numbers = true)]
    end: Option<f64>,
    /// Frames per second the animation is sampled at [default: the document's rate]
    #[arg(long, value_name = "N", allow_negative_numbers = true)]
    fps: Option<f64>,
    /// Include layers hidden by their eye toggle.
    #[arg(long)]
    include_hidden: bool,
    /// Flatten the layer tree into world-space meshes.
    #[arg(long)]
    bake: bool,
    /// Leave light layers out.
    #[arg(long)]
    no_lights: bool,
    /// Leave camera layers out.
    #[arg(long)]
    no_cameras: bool,
    /// The length unit of an FBX file.
    #[arg(long, value_enum, default_value_t = Unit::Metres, value_name = "UNIT")]
    fbx_unit: Unit,
    /// How vertex colours are encoded in an FBX file.
    #[arg(long, value_enum, default_value_t = Colors::Srgb, value_name = "ENCODING")]
    fbx_colors: Colors,
}

#[derive(Clone, Copy, ValueEnum)]
enum Unit {
    /// Metres, declared as such; what Blender, Unity and Godot expect.
    Metres,
    /// Real centimetres, for importers that ignore the declared unit.
    Centimetres,
}

#[derive(Clone, Copy, ValueEnum)]
enum Colors {
    /// sRGB-encoded, as importers assume.
    Srgb,
    /// Limner's own linear values.
    Linear,
}

/// A whole number with thousands separators: 1234567 becomes "1,234,567".
fn thousands(number: usize) -> String {
    let digits = number.to_string();
    let mut out = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// A file size in the largest binary unit that keeps it at or above one.
fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["bytes", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} bytes")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// What an export wrote, as the lines printed under its heading.
fn describe(report: &ExportReport) -> String {
    let mut text = format!(
        "  {} nodes, {} meshes, {} vertices, {} triangles, {}\n",
        thousands(report.nodes),
        thousands(report.meshes),
        thousands(report.vertices),
        thousands(report.triangles),
        size(report.bytes)
    );
    if report.animation_frames > 0 {
        text.push_str(&format!(
            "  animation: {} frames, {} channels\n",
            thousands(report.animation_frames),
            thousands(report.animation_channels)
        ));
    }
    text
}

pub fn run(args: &ExportArgs, out: &mut dyn Write) -> Result<()> {
    let extension = args
        .output
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !["glb", "obj", "usda", "usdz", "fbx"].contains(&extension.as_str()) {
        bail!(
            "the output must end in .glb, .obj, .usda, .usdz or .fbx: {}",
            args.output.display()
        );
    }
    if args.animated && extension == "obj" {
        bail!("an .obj file holds a still; leave out --animated or choose another format");
    }
    if args.end.is_some() && !args.animated {
        bail!("--end only applies with --animated; a still is taken at --start");
    }
    for (name, seconds) in [("--start", args.start), ("--end", args.end)] {
        if seconds.is_some_and(|s| !(s.is_finite() && s >= 0.0)) {
            bail!("{name} must be a time in seconds, zero or later");
        }
    }
    if args.fps.is_some_and(|fps| !(fps.is_finite() && fps > 0.0)) {
        bail!("--fps must be a rate above zero");
    }

    let opened = limner::open(&args.input)?;
    let doc = &opened.document;
    let mut options = ExportOptions::working_range(doc);
    options.animation = args.animated;
    if let Some(start) = args.start {
        options.start = time::from_seconds(start);
    }
    if let Some(end) = args.end {
        options.end = time::from_seconds(end);
    }
    if !args.animated {
        options.end = options.start;
    } else if options.end < options.start {
        bail!(
            "the range ends ({} s) before it starts ({} s)",
            time::to_seconds(options.end),
            time::to_seconds(options.start)
        );
    }
    if let Some(fps) = args.fps {
        options.rate = FrameRate::from_f64(fps);
    }
    options.include_hidden = args.include_hidden;
    options.bake_transforms = args.bake;
    options.lights = !args.no_lights;
    options.cameras = !args.no_cameras;
    let fbx = FbxOptions {
        unit: match args.fbx_unit {
            Unit::Metres => FbxUnit::Metres,
            Unit::Centimetres => FbxUnit::Centimetres,
        },
        color_space: match args.fbx_colors {
            Colors::Srgb => FbxColorSpace::Srgb,
            Colors::Linear => FbxColorSpace::Linear,
        },
        ..FbxOptions::default()
    };

    let path = args.output.as_path();
    let report = match extension.as_str() {
        "glb" => write_glb(doc, path, &options, &GltfOptions::default()),
        "obj" => write_obj(doc, path, options.start, options.include_hidden),
        "usda" => write_usda(doc, path, &options, &UsdOptions::default()),
        "usdz" => write_usdz(doc, path, &options, &UsdOptions::default()),
        _ => write_fbx(doc, path, &options, &fbx),
    }
    .with_context(|| format!("cannot export to {}", path.display()))?;

    writeln!(
        out,
        "{} ({}) -> {}",
        args.input.display(),
        opened.kind.label(),
        path.display()
    )?;
    write!(out, "{}", describe(&report))?;
    for warning in opened.warnings.iter().chain(&report.warnings) {
        writeln!(out, "  warning: {warning}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_sizes_are_readable() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(size(512), "512 bytes");
        assert_eq!(size(1536), "1.5 KiB");
        assert_eq!(size(24_536_678), "23.4 MiB");
    }

    #[test]
    fn a_still_reports_no_animation() {
        let still = ExportReport {
            nodes: 3,
            meshes: 2,
            vertices: 1200,
            triangles: 2300,
            bytes: 2048,
            ..ExportReport::default()
        };
        assert_eq!(
            describe(&still),
            "  3 nodes, 2 meshes, 1,200 vertices, 2,300 triangles, 2.0 KiB\n"
        );
        let moving = ExportReport {
            animation_frames: 25,
            animation_channels: 4,
            ..still
        };
        assert!(describe(&moving).ends_with("  animation: 25 frames, 4 channels\n"));
    }
}
