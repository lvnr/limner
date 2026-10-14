//! `limner`: command-line access to Limner documents.

mod dump;
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
    about = "Inspect and convert Limner and Quill documents"
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
}
