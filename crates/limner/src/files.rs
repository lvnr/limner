//! Opening and saving documents on disk, whatever format they are in.

use limner_core::model::Document;
use limner_format::{SaveOptions, SaveReport};
use limner_quill::QuillState;
use std::path::Path;

/// Where a document came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
    /// A Quill project folder. Imported, never written back.
    Quill,
    /// A native project: a `.limn` folder or a packed `.limnz` file.
    Limn,
}

impl SourceKind {
    pub fn label(self) -> &'static str {
        match self {
            SourceKind::Quill => "Quill project",
            SourceKind::Limn => "Limner project",
        }
    }
}

#[derive(Debug)]
pub struct Opened {
    pub document: Document,
    pub kind: SourceKind,
    /// Editor state carried by a Quill project, when that is the source.
    pub quill_state: Option<QuillState>,
    pub warnings: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    #[error("{0} does not exist")]
    NotFound(String),
    #[error("{0} is not a document Limner can open")]
    Unrecognised(String),
    #[error(transparent)]
    Quill(#[from] limner_quill::ImportError),
    #[error(transparent)]
    Limn(#[from] limner_format::FormatError),
}

/// Opens the document at `path`, detecting its format: a native project (a `.limn`
/// folder, the `limn.json` inside one, or a packed `.limnz` file) or a Quill project.
pub fn open(path: &Path) -> Result<Opened, OpenError> {
    if !path.exists() {
        return Err(OpenError::NotFound(path.display().to_string()));
    }
    if limner_format::is_limn_project(path) {
        let loaded = limner_format::load_project(path)?;
        return Ok(Opened {
            document: loaded.document,
            kind: SourceKind::Limn,
            quill_state: None,
            warnings: loaded.warnings,
        });
    }
    if limner_quill::is_quill_project(path) {
        let imported = limner_quill::import_project(path)?;
        return Ok(Opened {
            document: imported.document,
            kind: SourceKind::Quill,
            quill_state: Some(imported.state),
            warnings: imported.warnings,
        });
    }
    Err(OpenError::Unrecognised(path.display().to_string()))
}

#[derive(Debug, thiserror::Error)]
pub enum SaveError {
    #[error("{0} is a Quill project; Limner imports Quill projects but never writes into them")]
    QuillProject(String),
    #[error(
        "{0} is a folder that already holds other files; choose a new folder or one ending in .limn"
    )]
    ForeignFolder(String),
    #[error(transparent)]
    Format(#[from] limner_format::FormatError),
}

/// True when `path` names a packed project file (`.limnz`) rather than a folder.
pub fn is_packed_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(limner_format::PACKED_EXTENSION))
}

/// Saves `doc` at `path` in the native format: as a single packed file when the
/// path ends in `.limnz`, and as a project folder otherwise.
///
/// Saving to a folder is incremental and safe to interrupt; see
/// [`limner_format::save_project`]. Editor state (`state.json`) is not part of the
/// document and is left as it is.
pub fn save(doc: &Document, path: &Path) -> Result<SaveReport, SaveError> {
    let options = SaveOptions::default();
    if is_packed_path(path) {
        return Ok(limner_format::save_packed(doc, path, &options)?);
    }
    if limner_quill::is_quill_project(path) {
        return Err(SaveError::QuillProject(path.display().to_string()));
    }
    // Do not scatter project files into a folder that was never meant to be one.
    let named_as_project = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(limner_format::PROJECT_EXTENSION));
    let holds_other_things = path.is_dir()
        && !limner_format::is_limn_project(path)
        && std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_some());
    if holds_other_things && !named_as_project {
        return Err(SaveError::ForeignFolder(path.display().to_string()));
    }
    Ok(limner_format::save_project(doc, path, &options)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner_format::testing::{assert_documents_equal, exhaustive_document};
    use limner_quill::testing::{paint, project, tube, write_project, QbinBuilder, StrokeSpec};

    fn quill_project(dir: &Path) {
        let mut qbin = QbinBuilder::default();
        let drawing = qbin.drawing(&[StrokeSpec::new(0, 2, tube(0.0, 6))]);
        let quill = project(vec![paint("Sketch", &[&drawing], &[0])]);
        write_project(dir, &quill, &qbin.bytes, None);
    }

    #[test]
    fn saves_and_opens_folders_and_packed_files() {
        let doc = exhaustive_document();
        let dir = tempfile::tempdir().unwrap();

        let folder = dir.path().join("piece.limn");
        let report = save(&doc, &folder).unwrap();
        assert!(report.manifest_written && report.blobs_written > 0);
        assert!(folder.join("limn.json").is_file());
        let opened = open(&folder).unwrap();
        assert_eq!(opened.kind, SourceKind::Limn);
        assert_eq!(opened.kind.label(), "Limner project");
        assert!(opened.quill_state.is_none());
        assert_eq!(opened.warnings, Vec::<String>::new());
        assert_documents_equal(&doc, &opened.document);
        // The manifest inside the folder opens the same project.
        assert_documents_equal(&doc, &open(&folder.join("limn.json")).unwrap().document);
        assert!(save(&opened.document, &folder).unwrap().wrote_nothing());

        let packed = dir.path().join("piece.LIMNZ");
        save(&doc, &packed).unwrap();
        assert!(packed.is_file());
        let opened = open(&packed).unwrap();
        assert_eq!(opened.kind, SourceKind::Limn);
        assert_documents_equal(&doc, &opened.document);
    }

    #[test]
    fn a_quill_project_opens_as_before_and_saves_as_a_native_one() {
        let dir = tempfile::tempdir().unwrap();
        let quill = dir.path().join("sketch");
        quill_project(&quill);
        let imported = open(&quill).unwrap();
        assert_eq!(imported.kind, SourceKind::Quill);
        assert!(imported.quill_state.is_some());

        let native = dir.path().join("sketch.limn");
        save(&imported.document, &native).unwrap();
        let reopened = open(&native).unwrap();
        assert_eq!(reopened.kind, SourceKind::Limn);
        assert_documents_equal(&imported.document, &reopened.document);
    }

    #[test]
    fn saving_refuses_folders_that_are_not_ours() {
        let doc = Document::new();
        let dir = tempfile::tempdir().unwrap();

        let quill = dir.path().join("sketch");
        quill_project(&quill);
        assert!(matches!(
            save(&doc, &quill),
            Err(SaveError::QuillProject(_))
        ));
        assert!(!quill.join("limn.json").exists());

        let documents = dir.path().join("Documents");
        std::fs::create_dir(&documents).unwrap();
        std::fs::write(documents.join("letter.txt"), b"dear").unwrap();
        assert!(matches!(
            save(&doc, &documents),
            Err(SaveError::ForeignFolder(_))
        ));
        assert!(!documents.join("limn.json").exists());

        // An empty folder, a new folder, and a folder named as a project are all fine,
        // the last even when an interrupted first save left files but no manifest.
        let empty = dir.path().join("empty");
        std::fs::create_dir(&empty).unwrap();
        save(&doc, &empty).unwrap();
        save(&doc, &empty).unwrap();
        save(&doc, &dir.path().join("deep").join("new")).unwrap();
        let interrupted = dir.path().join("interrupted.limn");
        std::fs::create_dir_all(interrupted.join("drawings")).unwrap();
        std::fs::write(interrupted.join("limn.json.tmp"), b"{").unwrap();
        save(&doc, &interrupted).unwrap();
        assert_documents_equal(&doc, &open(&interrupted).unwrap().document);
    }

    #[test]
    fn unknown_and_missing_paths_are_reported() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            open(&dir.path().join("nothing")),
            Err(OpenError::NotFound(_))
        ));
        assert!(matches!(open(dir.path()), Err(OpenError::Unrecognised(_))));
        // A native project that is damaged is reported as such, not as unrecognised.
        std::fs::write(
            dir.path().join("limn.json"),
            b"{\"format\": \"limn\", \"version\": 7}",
        )
        .unwrap();
        assert!(matches!(
            open(dir.path()),
            Err(OpenError::Limn(
                limner_format::FormatError::UnsupportedVersion { .. }
            ))
        ));
    }
}
