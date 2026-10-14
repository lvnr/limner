//! `limner`: command-line access to Limner documents.

mod dump;
mod export;
mod inspect;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use limner::format::{self, SaveOptions};
use limner::SourceKind;
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "limner",
    version,
    about = "Inspect, convert and export Limner and Quill documents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print a document's layer tree and statistics.
    Inspect {
        /// A Quill project folder, a .limn project folder or a packed .limnz file.
        path: PathBuf,
        /// Print machine-readable JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Convert a document to the native format.
    ///
    /// Reads a Quill project, a .limn folder or a .limnz file, and writes a .limn
    /// project folder or a single packed .limnz file. Writing to an existing .limn
    /// folder updates it in place and rewrites only what changed.
    Convert {
        /// A Quill project folder, a .limn project folder or a packed .limnz file.
        input: PathBuf,
        /// Where to write: a path ending in .limn (a folder) or .limnz (one file).
        output: PathBuf,
    },
    /// Export a document to an interchange file: glTF, OBJ, USD or FBX.
    ///
    /// Writes a still at the start of the working range, or with --animated the
    /// working range sampled at the document's frame rate. docs/export.md says what
    /// each format holds.
    Export(export::ExportArgs),
    /// Write the world-space cross-section rings of every paint layer's first frame
    /// to a binary file, for checking geometry against other exporters.
    #[command(hide = true)]
    DumpRings { path: PathBuf, out: PathBuf },
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

fn convert(input: &Path, output: &Path, out: &mut dyn Write) -> Result<()> {
    let packed = has_extension(output, format::PACKED_EXTENSION);
    if !packed && !has_extension(output, format::PROJECT_EXTENSION) {
        bail!(
            "the output must end in .limn (a project folder) or .limnz (a packed file): {}",
            output.display()
        );
    }
    let opened = limner::open(input)?;

    // A native project carries editor state and a thumbnail beside the document;
    // they travel with it. Updating a project in place leaves its own untouched.
    let in_place = match (std::fs::canonicalize(input), std::fs::canonicalize(output)) {
        (Ok(a), Ok(b)) => a == b || a.parent() == Some(b.as_path()),
        _ => false,
    };
    let (state, thumbnail) = if opened.kind == SourceKind::Limn && !in_place {
        (format::read_state(input)?, format::read_thumbnail(input)?)
    } else {
        (None, None)
    };

    let report = if in_place || !packed {
        let report = limner::save(&opened.document, output)?;
        if let Some(state) = &state {
            format::write_state(output, state)?;
        }
        if let Some(thumbnail) = &thumbnail {
            format::write_thumbnail(output, thumbnail)?;
        }
        report
    } else {
        format::save_packed_with(
            &opened.document,
            output,
            &SaveOptions::default(),
            state.as_ref(),
            thumbnail.as_deref(),
        )?
    };

    writeln!(
        out,
        "{} ({}) -> {}",
        input.display(),
        opened.kind.label(),
        output.display()
    )?;
    writeln!(out, "  {report}")?;
    for warning in &opened.warnings {
        writeln!(out, "  warning: {warning}")?;
    }
    Ok(())
}

fn run(command: Command, out: &mut dyn Write) -> Result<()> {
    match command {
        Command::Inspect { path, json } => {
            let opened = limner::open(&path)?;
            let report = inspect::Report::new(&opened);
            if json {
                writeln!(out, "{}", serde_json::to_string_pretty(&report.to_json())?)?;
            } else {
                write!(out, "{}", report.to_text())?;
            }
        }
        Command::Convert { input, output } => convert(&input, &output, out)?,
        Command::Export(args) => export::run(&args, out)?,
        Command::DumpRings { path, out: file } => {
            let opened = limner::open(&path)?;
            std::fs::write(&file, dump::rings(&opened.document))
                .with_context(|| format!("cannot write {}", file.display()))?;
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut stdout = std::io::stdout().lock();
    run(Cli::parse().command, &mut stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner_quill::testing::{
        group, paint, project, tube, write_project, QbinBuilder, StrokeSpec,
    };
    use serde_json::{json, Value};

    /// Runs the tool as the shell would and returns what it printed.
    fn limner(args: &[&str]) -> Result<String> {
        let cli = Cli::try_parse_from(std::iter::once("limner").chain(args.iter().copied()))?;
        let mut out = Vec::new();
        run(cli.command, &mut out)?;
        Ok(String::from_utf8(out)?)
    }

    fn inspect(path: &Path) -> Value {
        serde_json::from_str(&limner(&["inspect", "--json", path.to_str().unwrap()]).unwrap())
            .unwrap()
    }

    /// The parts of an inspection that describe the document rather than its source
    /// (importing reports warnings of its own, such as a missing spawn area).
    fn substance(mut report: Value) -> Value {
        let members = report.as_object_mut().unwrap();
        members.remove("source");
        members.remove("warnings");
        report
    }

    fn quill_project(dir: &Path) {
        let mut qbin = QbinBuilder::default();
        let a = qbin.drawing(&[
            StrokeSpec::new(0, 2, tube(0.0, 12)),
            StrokeSpec::new(1, 1, tube(1.0, 7)),
        ]);
        let b = qbin.drawing(&[StrokeSpec::new(0, 4, tube(2.0, 5))]);
        let mut body = group(
            "Body",
            vec![
                paint("Walk", &[&a, &b], &[0, 1, 1, 0]),
                paint("Still", &[&a], &[0]),
            ],
        );
        body["Opacity"] = json!(0.5);
        let quill = project(vec![body, paint("Sky", &[&b], &[0])]);
        write_project(dir, &quill, &qbin.bytes, None);
    }

    #[test]
    fn convert_then_inspect() {
        let dir = tempfile::tempdir().unwrap();
        let quill = dir.path().join("walk");
        quill_project(&quill);
        let folder = dir.path().join("walk.limn");
        let packed = dir.path().join("walk.limnz");
        let path = |p: &Path| p.to_str().unwrap().to_owned();

        // Quill to a project folder.
        let printed = limner(&["convert", &path(&quill), &path(&folder)]).unwrap();
        assert!(printed.contains("(Quill project) ->"), "{printed}");
        // Two distinct drawings, however many layers and frames show them.
        assert!(
            printed.contains("drawings: 2 written, 0 reused"),
            "{printed}"
        );
        assert!(printed.contains("manifest written"), "{printed}");
        assert!(folder.join("limn.json").is_file());

        let from_quill = inspect(&quill);
        let from_folder = inspect(&folder);
        assert_eq!(from_quill["source"], "Quill project");
        assert_eq!(from_folder["source"], "Limner project");
        assert_eq!(from_folder["strokes"], from_quill["strokes"]);
        assert_eq!(from_folder["layers"], 5);
        assert_eq!(substance(from_folder.clone()), substance(from_quill));
        let text = limner(&["inspect", &path(&folder)]).unwrap();
        assert!(text.starts_with("walk (Limner project)\n"), "{text}");
        assert!(
            text.contains("Walk [paint]") && text.contains("4 frames of 2 drawings"),
            "{text}"
        );

        // Converting again finds everything in place.
        let printed = limner(&["convert", &path(&quill), &path(&folder)]).unwrap();
        assert!(
            printed.contains("drawings: 0 written, 2 reused"),
            "{printed}"
        );
        assert!(printed.contains("manifest unchanged"), "{printed}");

        // Folder to packed file, taking the editor state and thumbnail along.
        limner::format::write_state(&folder, &json!({"tool": "paint"})).unwrap();
        limner::format::write_thumbnail(&folder, b"thumb").unwrap();
        let printed = limner(&["convert", &path(&folder), &path(&packed)]).unwrap();
        assert!(printed.contains("(Limner project) ->"), "{printed}");
        assert!(packed.is_file());
        assert_eq!(substance(inspect(&packed)), substance(from_folder.clone()));
        assert_eq!(
            limner::format::read_state(&packed).unwrap(),
            Some(json!({"tool": "paint"}))
        );
        assert_eq!(
            limner::format::read_thumbnail(&packed).unwrap().as_deref(),
            Some(&b"thumb"[..])
        );

        // Packed file back to a folder: the same files as the first folder.
        let unpacked = dir.path().join("again.limn");
        limner(&["convert", &path(&packed), &path(&unpacked)]).unwrap();
        assert_eq!(substance(inspect(&unpacked)), substance(from_folder));
        for name in ["limn.json", "state.json", "thumbnail.png"] {
            assert_eq!(
                std::fs::read(unpacked.join(name)).unwrap(),
                std::fs::read(folder.join(name)).unwrap(),
                "{name}"
            );
        }

        // Converting a project onto itself is a no-op that keeps its state.
        let printed = limner(&["convert", &path(&folder), &path(&folder)]).unwrap();
        assert!(printed.contains("manifest unchanged"), "{printed}");
        assert!(folder.join("state.json").is_file());
    }

    #[test]
    fn convert_refuses_other_outputs_and_reports_bad_inputs() {
        let dir = tempfile::tempdir().unwrap();
        let quill = dir.path().join("walk");
        quill_project(&quill);
        let path = |p: &Path| p.to_str().unwrap().to_owned();

        let error =
            limner(&["convert", &path(&quill), &path(&dir.path().join("out.glb"))]).unwrap_err();
        assert!(error.to_string().contains(".limn"), "{error}");
        assert!(!dir.path().join("out.glb").exists());

        let error = limner(&[
            "convert",
            &path(&dir.path().join("missing")),
            &path(&dir.path().join("out.limn")),
        ])
        .unwrap_err();
        assert!(error.to_string().contains("does not exist"), "{error}");
        assert!(!dir.path().join("out.limn").exists());

        assert!(limner(&["convert", &path(&quill)]).is_err());
        assert!(limner(&["inspect", &path(dir.path())]).is_err());
    }

    /// The number printed before `word` on the report line, without its separators.
    fn reported(printed: &str, word: &str) -> usize {
        let before = printed.split(&format!(" {word}")).next().unwrap();
        let number = before.rsplit([' ', '\n']).next().unwrap();
        number.replace(',', "").parse().unwrap()
    }

    #[test]
    fn export_writes_every_format_and_reports_what_went_in() {
        let dir = tempfile::tempdir().unwrap();
        let quill = dir.path().join("walk");
        quill_project(&quill);
        let path = |p: &Path| p.to_str().unwrap().to_owned();
        let source = path(&quill);

        // A still as OBJ: the file holds exactly the vertices and triangles reported.
        let obj = dir.path().join("walk.obj");
        let printed = limner(&["export", &source, &path(&obj)]).unwrap();
        assert!(printed.contains("(Quill project) ->"), "{printed}");
        assert!(!printed.contains("animation:"), "{printed}");
        let text = std::fs::read_to_string(&obj).unwrap();
        let lines = |prefix: &str| text.lines().filter(|l| l.starts_with(prefix)).count();
        assert!(reported(&printed, "vertices") > 0, "{printed}");
        assert_eq!(lines("v "), reported(&printed, "vertices"));
        assert_eq!(lines("f "), reported(&printed, "triangles"));
        assert_eq!(lines("o "), reported(&printed, "nodes"));

        // Animated glTF: the description inside the file agrees with the report.
        let glb = dir.path().join("walk.glb");
        let printed = limner(&[
            "export",
            &source,
            &path(&glb),
            "--animated",
            "--start",
            "0",
            "--end",
            "0.5",
            "--fps",
            "12",
        ])
        .unwrap();
        assert!(printed.contains("animation: 7 frames"), "{printed}");
        let bytes = std::fs::read(&glb).unwrap();
        assert_eq!(&bytes[0..4], b"glTF");
        assert_eq!(bytes.len() as u64, std::fs::metadata(&glb).unwrap().len());
        let json_length = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        let gltf: Value = serde_json::from_slice(&bytes[20..20 + json_length]).unwrap();
        assert_eq!(
            gltf["nodes"].as_array().unwrap().len(),
            reported(&printed, "nodes")
        );
        assert_eq!(gltf["animations"].as_array().unwrap().len(), 1);
        // The Walk layer shows two drawings in the range; each is a child of its own.
        let names: Vec<&str> = gltf["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|node| node["name"].as_str())
            .collect();
        assert!(
            names.contains(&"Walk.000") && names.contains(&"Walk.001"),
            "{names:?}"
        );

        // The same still in the other formats, each with its own signature.
        for (name, signature) in [
            ("walk.usda", &b"#usda 1.0"[..]),
            ("walk.usdz", &b"PK"[..]),
            ("walk.fbx", &b"Kaydara FBX Binary"[..]),
        ] {
            let file = dir.path().join(name);
            let printed = limner(&["export", &source, &path(&file), "--bake"]).unwrap();
            assert!(
                std::fs::read(&file).unwrap().starts_with(signature),
                "{name}"
            );
            assert_eq!(
                reported(&printed, "vertices"),
                lines("v "),
                "{name}: {printed}"
            );
        }

        // Hidden layers are left out unless asked for.
        let hidden = dir.path().join("hidden.obj");
        let all = limner(&["export", &source, &path(&hidden), "--include-hidden"]).unwrap();
        assert_eq!(reported(&all, "vertices"), lines("v "));

        // With the layer tree kept, two layers showing one drawing share its mesh.
        let cm = dir.path().join("cm.fbx");
        let printed = limner(&[
            "export",
            &source,
            &path(&cm),
            "--fbx-unit",
            "centimetres",
            "--fbx-colors",
            "linear",
            "--no-lights",
            "--no-cameras",
        ])
        .unwrap();
        let shared = reported(&printed, "triangles");
        assert!(shared > 0 && shared < lines("f "), "{printed}");
        assert!(std::fs::read(&cm)
            .unwrap()
            .starts_with(b"Kaydara FBX Binary"));
    }

    #[test]
    fn export_checks_its_arguments() {
        let dir = tempfile::tempdir().unwrap();
        let quill = dir.path().join("walk");
        quill_project(&quill);
        let path = |p: &Path| p.to_str().unwrap().to_owned();
        let source = path(&quill);
        let refused = |args: &[&str], reason: &str| {
            let mut all = vec!["export", source.as_str()];
            all.extend_from_slice(args);
            let error = format!("{:#}", limner(&all).unwrap_err());
            assert!(error.contains(reason), "{args:?}: {error}");
        };

        let limn = path(&dir.path().join("out.limn"));
        refused(&[&limn], ".glb, .obj, .usda, .usdz or .fbx");
        let obj = path(&dir.path().join("out.obj"));
        refused(&[&obj, "--animated"], "holds a still");
        let glb = path(&dir.path().join("out.glb"));
        refused(&[&glb, "--end", "2"], "--end only applies with --animated");
        refused(
            &[&glb, "--animated", "--start", "2", "--end", "1"],
            "before it starts",
        );
        refused(&[&glb, "--start", "-1"], "zero or later");
        refused(&[&glb, "--animated", "--fps", "0"], "above zero");
        refused(&[&glb, "--fbx-unit", "inches"], "invalid value 'inches'");
        refused(&[&glb, "--fbx-colors", "p3"], "invalid value 'p3'");
        refused(&[&glb, "--loud"], "unexpected argument '--loud'");
        for name in ["out.obj", "out.glb", "out.limn"] {
            assert!(!dir.path().join(name).exists(), "{name} was written");
        }

        // A folder that is not there is reported, not created.
        let nowhere = path(&dir.path().join("missing").join("out.glb"));
        refused(&[&nowhere], "cannot export to");
        let missing = limner(&["export", &path(&dir.path().join("none")), &glb]).unwrap_err();
        assert!(missing.to_string().contains("does not exist"), "{missing}");
        assert!(limner(&["export", &source]).is_err());
    }
}
