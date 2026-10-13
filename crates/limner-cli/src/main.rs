//! `limner`: command-line access to Limner documents.

mod dump;
mod inspect;

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

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
        /// A Quill project folder (or a file inside it).
        path: PathBuf,
        /// Print machine-readable JSON instead of text.
        #[arg(long)]
        json: bool,
    },
    /// Write the world-space cross-section rings of every paint layer's first frame
    /// to a binary file, for checking geometry against other exporters.
    #[command(hide = true)]
    DumpRings { path: PathBuf, out: PathBuf },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Inspect { path, json } => {
            let opened = limner::open(&path)?;
            let report = inspect::Report::new(&opened);
            if json {
                println!("{}", serde_json::to_string_pretty(&report.to_json())?);
            } else {
                print!("{}", report.to_text());
            }
        }
        Command::DumpRings { path, out } => {
            let opened = limner::open(&path)?;
            std::fs::write(&out, dump::rings(&opened.document))?;
        }
    }
    Ok(())
}
