//! Loading a project from a folder or a packed file.

use crate::blob;
use crate::error::FormatError;
use crate::hash::ContentHash;
use crate::manifest;
use crate::media;
use crate::store::{blob_path, media_path, tmp_path, Source, MANIFEST_FILE};
use limner_core::model::{Document, Drawing, DrawingId, Media, MediaData, MediaId};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The result of loading a project.
#[derive(Debug)]
pub struct Loaded {
    pub document: Document,
    /// Things that were noticed or repaired while loading. Empty for a clean load.
    pub warnings: Vec<String>,
}

/// The content hash a media file name starts with: `<64 hex digits>.<extension>`.
fn media_hash(file: &str) -> Option<ContentHash> {
    let (stem, extension) = file.split_once('.')?;
    let plain = !extension.is_empty() && extension.bytes().all(|b| b.is_ascii_alphanumeric());
    ContentHash::from_hex(stem).filter(|_| plain)
}

/// Loads the project at `path`: a `.limn` folder, the `limn.json` inside one, or a
/// packed `.limnz` file, which is read in place without being unpacked.
///
/// Every drawing and media file is checked against the hash in its name, and the
/// rebuilt document is validated; a project that fails either check is not loaded.
pub fn load_project(path: &Path) -> Result<Loaded, FormatError> {
    let mut source = Source::open(path)?;
    let manifest_path = source.locate(MANIFEST_FILE);
    let bytes = source
        .read(MANIFEST_FILE)?
        .ok_or_else(|| FormatError::NotAProject(path.to_path_buf()))?;
    let manifest = manifest::parse(&bytes, &manifest_path)?;
    drop(bytes);

    let mut warnings = Vec::new();
    if source
        .dir()
        .is_some_and(|dir| tmp_path(&dir.join(MANIFEST_FILE)).exists())
    {
        warnings.push(
            "an earlier save of this project was interrupted; the last completed save was loaded"
                .to_owned(),
        );
    }

    let mut blobs: HashMap<ContentHash, Arc<Drawing>> = HashMap::new();
    let mut drawings = Vec::with_capacity(manifest.drawings.len());
    for entry in &manifest.drawings {
        let hash = ContentHash::from_hex(&entry.blob).ok_or_else(|| {
            FormatError::manifest(
                &manifest_path,
                format!(
                    "drawing {} names blob \"{}\", which is not a 64-digit lowercase hash",
                    entry.id, entry.blob
                ),
            )
        })?;
        let drawing = match blobs.get(&hash) {
            Some(drawing) => drawing.clone(),
            None => {
                let relative = blob_path(&entry.blob);
                let bytes = source.read_required(&relative)?;
                let drawing = blob::decode(&bytes, Some(&hash))
                    .map_err(|reason| FormatError::corrupt(&source.locate(&relative), reason))?;
                let drawing = Arc::new(drawing);
                blobs.insert(hash, drawing.clone());
                drawing
            }
        };
        drawings.push((DrawingId(entry.id), drawing));
    }

    let mut files: HashMap<(&str, Option<&str>), MediaData> = HashMap::new();
    let mut media_items = Vec::with_capacity(manifest.media.len());
    for entry in &manifest.media {
        let hash = media_hash(&entry.file).ok_or_else(|| {
            FormatError::manifest(
                &manifest_path,
                format!(
                    "media {} names file \"{}\", which is not a hash and an extension",
                    entry.id, entry.file
                ),
            )
        })?;
        let key = (entry.file.as_str(), entry.format.as_deref());
        let data = match files.get(&key) {
            Some(data) => data.clone(),
            None => {
                let relative = media_path(&entry.file);
                let bytes = source.read_required(&relative)?;
                let data = media::decode(
                    entry.storage.into(),
                    entry.format.as_deref(),
                    bytes,
                    Some(&hash),
                )
                .map_err(|reason| FormatError::corrupt(&source.locate(&relative), reason))?;
                files.insert(key, data.clone());
                data
            }
        };
        media_items.push(Media {
            id: MediaId(entry.id),
            name: entry.name.clone(),
            kind: manifest::media_kind(entry),
            data,
        });
    }
    drop(files);

    let document = manifest::into_document(
        manifest,
        drawings,
        media_items,
        &manifest_path,
        &mut warnings,
    )?;
    document.validate().map_err(|source| FormatError::Invalid {
        path: manifest_path,
        source,
    })?;
    Ok(Loaded { document, warnings })
}
