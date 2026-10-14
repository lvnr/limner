//! Reading picture, sound and model files so they can be brought into a document.
//!
//! Pictures and sounds are embedded as the files they are; this module only looks at
//! their headers to refuse what cannot be shown or played and to choose sensible
//! settings. OBJ models are parsed here. Formats that need the engine's own readers
//! (glTF, FBX) are recognised by [`kind_of`] and decoded by the caller, which hands
//! the geometry to [`model_from_mesh`].

pub use limner_core::model::{jpeg_size, png_size};
use limner_core::model::{MediaData, MeshData, SoundKind};
use limner_core::session::media::{MediaLayerKind, NewMedia};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The longest side a picture may have. Graphics hardware does not take larger textures.
pub const MAX_PICTURE_SIDE: u32 = 16384;

/// What a file is, going by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Png,
    Jpg,
    Wav,
    Ogg,
    Mp3,
    Obj,
    /// `.glb` or `.gltf`: decoded by the engine.
    Gltf,
    /// Decoded by the engine.
    Fbx,
}

impl FileKind {
    /// True for the model formats this module cannot decode itself.
    pub fn needs_engine(self) -> bool {
        matches!(self, FileKind::Gltf | FileKind::Fbx)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("{0} files cannot be imported; pictures are .png or .jpg, sounds .wav, .ogg or .mp3, models .obj, .glb, .gltf or .fbx")]
    UnknownKind(String),
    #[error("the file could not be read ({0})")]
    Read(#[from] std::io::Error),
    #[error("the file is not a {0} picture, or it is damaged")]
    NotAPicture(&'static str),
    #[error("the picture is {width} by {height} pixels; the most that can be shown is {MAX_PICTURE_SIDE} on a side")]
    PictureTooLarge { width: u32, height: u32 },
    #[error("the file is not a {0} sound, or it is damaged")]
    NotASound(&'static str),
    #[error("the sound is Ogg {0}; only Ogg Vorbis can be played")]
    OggCodec(&'static str),
    #[error("the model has no triangles")]
    EmptyModel,
    #[error("this kind of model is read by the application, not by this function")]
    NeedsEngine,
}

/// A file ready to become a layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Import {
    pub media: NewMedia,
    /// What was left out or repaired, one sentence each.
    pub warnings: Vec<String>,
}

/// What `path` is, going by its extension.
pub fn kind_of(path: &Path) -> Result<FileKind, MediaError> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    Ok(match extension.as_str() {
        "png" => FileKind::Png,
        "jpg" | "jpeg" => FileKind::Jpg,
        "wav" => FileKind::Wav,
        "ogg" => FileKind::Ogg,
        "mp3" => FileKind::Mp3,
        "obj" => FileKind::Obj,
        "glb" | "gltf" => FileKind::Gltf,
        "fbx" => FileKind::Fbx,
        "" => return Err(MediaError::UnknownKind("files without an extension".into())),
        other => return Err(MediaError::UnknownKind(format!(".{other}"))),
    })
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn new_media(path: &Path, kind: MediaLayerKind, data: MediaData) -> NewMedia {
    NewMedia {
        kind,
        file_name: file_name(path),
        source_path: path.display().to_string(),
        data,
    }
}

/// Reads a picture, a sound or an OBJ model from disk.
pub fn read(path: &Path) -> Result<Import, MediaError> {
    let kind = kind_of(path)?;
    if kind.needs_engine() {
        return Err(MediaError::NeedsEngine);
    }
    let bytes = std::fs::read(path)?;
    let mut warnings = Vec::new();
    let (layer_kind, data) = match kind {
        FileKind::Png | FileKind::Jpg => {
            let (format, size) = match kind {
                FileKind::Png => (
                    "png",
                    png_size(&bytes).ok_or(MediaError::NotAPicture("PNG"))?,
                ),
                _ => (
                    "jpg",
                    jpeg_size(&bytes).ok_or(MediaError::NotAPicture("JPEG"))?,
                ),
            };
            check_picture_size(size)?;
            (MediaLayerKind::Picture, encoded(format, bytes))
        }
        FileKind::Wav | FileKind::Ogg | FileKind::Mp3 => {
            let (format, channels) = match kind {
                FileKind::Wav => ("wav", wav_channels(&bytes)?),
                FileKind::Ogg => ("ogg", ogg_channels(&bytes)?),
                _ => ("mp3", mp3_channels(&bytes)?),
            };
            (
                MediaLayerKind::Sound {
                    kind: sound_kind_for(channels),
                },
                encoded(format, bytes),
            )
        }
        FileKind::Obj => {
            let parsed = parse_obj(&String::from_utf8_lossy(&bytes));
            warnings = parsed.warnings;
            if parsed.mesh.indices.is_empty() {
                return Err(MediaError::EmptyModel);
            }
            (
                MediaLayerKind::Model,
                MediaData::Mesh(Arc::new(parsed.mesh)),
            )
        }
        FileKind::Gltf | FileKind::Fbx => unreachable!("returned above"),
    };
    Ok(Import {
        media: new_media(path, layer_kind, data),
        warnings,
    })
}

/// Wraps geometry the engine decoded from `path` as a model import. Triangles that
/// point at missing vertices or have non-finite corners are dropped, with a warning.
pub fn model_from_mesh(
    path: &Path,
    mut mesh: MeshData,
    mut warnings: Vec<String>,
) -> Result<Import, MediaError> {
    let dropped = sanitize_mesh(&mut mesh);
    if dropped > 0 {
        warnings.push(format!(
            "{dropped} triangle(s) of the model were unusable and were left out"
        ));
    }
    if mesh.indices.is_empty() {
        return Err(MediaError::EmptyModel);
    }
    Ok(Import {
        media: new_media(path, MediaLayerKind::Model, MediaData::Mesh(Arc::new(mesh))),
        warnings,
    })
}

fn encoded(format: &str, bytes: Vec<u8>) -> MediaData {
    MediaData::Encoded {
        format: format.to_owned(),
        bytes: Arc::from(bytes),
    }
}

/// A sound with one channel starts as a point in space; anything wider plays the same everywhere.
pub fn sound_kind_for(channels: u16) -> SoundKind {
    if channels == 1 {
        SoundKind::Positional
    } else {
        SoundKind::Flat
    }
}

fn check_picture_size((width, height): (u32, u32)) -> Result<(), MediaError> {
    if width == 0 || height == 0 {
        return Err(MediaError::NotAPicture("usable"));
    }
    if width > MAX_PICTURE_SIDE || height > MAX_PICTURE_SIDE {
        return Err(MediaError::PictureTooLarge { width, height });
    }
    Ok(())
}

// ----- headers -----

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

/// The number of channels of a WAV file.
fn wav_channels(bytes: &[u8]) -> Result<u16, MediaError> {
    let bad = MediaError::NotASound("WAV");
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(bad);
    }
    let mut at = 12;
    let mut channels = None;
    let mut has_data = false;
    while let (Some(id), Some(size)) = (bytes.get(at..at + 4), le_u32(bytes, at + 4)) {
        match id {
            b"fmt " => channels = le_u16(bytes, at + 10),
            b"data" => has_data = true,
            _ => {}
        }
        // Chunks are padded to an even length.
        at = at.saturating_add(8 + size as usize + (size as usize & 1));
    }
    match channels {
        Some(channels) if channels > 0 && has_data => Ok(channels),
        _ => Err(bad),
    }
}

/// The number of channels of an Ogg Vorbis file.
fn ogg_channels(bytes: &[u8]) -> Result<u16, MediaError> {
    let bad = MediaError::NotASound("Ogg Vorbis");
    if bytes.get(0..4) != Some(b"OggS") {
        return Err(bad);
    }
    // The first page holds one packet: the codec's identification header.
    let segments = *bytes.get(26).ok_or(MediaError::NotASound("Ogg Vorbis"))? as usize;
    let packet = bytes.get(27 + segments..).unwrap_or_default();
    if packet.starts_with(b"OpusHead") {
        return Err(MediaError::OggCodec("Opus"));
    }
    if packet.starts_with(b"\x7fFLAC") {
        return Err(MediaError::OggCodec("FLAC"));
    }
    if !packet.starts_with(b"\x01vorbis") {
        return Err(bad);
    }
    match packet.get(11) {
        Some(channels) if *channels > 0 => Ok(*channels as u16),
        _ => Err(bad),
    }
}

/// The number of channels of an MP3 file, from its first frame.
fn mp3_channels(bytes: &[u8]) -> Result<u16, MediaError> {
    let mut at = 0;
    // An ID3v2 tag in front says how long it is in four 7-bit bytes.
    if bytes.starts_with(b"ID3") {
        let size = bytes
            .get(6..10)
            .ok_or(MediaError::NotASound("MP3"))?
            .iter()
            .fold(0usize, |n, b| (n << 7) | (*b & 0x7F) as usize);
        at = 10 + size;
    }
    // Some writers pad between the tag and the audio; look a little way for the first frame.
    let window = bytes.get(at..).unwrap_or_default();
    for header in window.windows(4).take(64 * 1024) {
        let sync = header[0] == 0xFF && header[1] & 0xE0 == 0xE0;
        let version_ok = header[1] & 0x18 != 0x08;
        let layer_ok = header[1] & 0x06 != 0;
        let bitrate_ok = header[2] & 0xF0 != 0xF0;
        let rate_ok = header[2] & 0x0C != 0x0C;
        if sync && version_ok && layer_ok && bitrate_ok && rate_ok {
            return Ok(if header[3] >> 6 == 3 { 1 } else { 2 });
        }
    }
    Err(MediaError::NotASound("MP3"))
}

// ----- OBJ -----

/// What an OBJ file holds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParsedObj {
    pub mesh: MeshData,
    pub warnings: Vec<String>,
}

fn srgb_to_linear(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.040_45 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// An index as OBJ writes it: counted from one, or backwards from the end when negative.
fn resolve_index(text: &str, count: usize) -> Option<usize> {
    let n: i64 = text.parse().ok()?;
    let index = if n > 0 { n - 1 } else { count as i64 + n };
    (n != 0 && index >= 0 && (index as usize) < count).then_some(index as usize)
}

/// Parses the text of an OBJ file: positions, normals, vertex colours written after a
/// position (`v x y z r g b`), and faces of any size, which are split into triangles
/// as a fan. Everything is one mesh; objects, groups, materials and texture
/// coordinates are left out.
pub fn parse_obj(text: &str) -> ParsedObj {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut colors: Vec<Option<[f32; 3]>> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    // A vertex of the result is a position paired with a normal.
    let mut corners: HashMap<(usize, Option<usize>), u32> = HashMap::new();
    let mut mesh = MeshData::default();
    let mut corner_normals: Vec<Option<usize>> = Vec::new();
    let mut corner_colors: Vec<Option<[f32; 3]>> = Vec::new();
    let mut skipped_faces = 0usize;
    let mut bad_numbers = 0usize;
    let mut has_materials = false;
    let mut has_curves = false;

    let numbers = |fields: std::str::SplitWhitespace| -> Vec<Option<f32>> {
        fields
            .map(|f| f.parse::<f32>().ok().filter(|v| v.is_finite()))
            .collect()
    };

    for line in text.lines() {
        let line = line.split('#').next().unwrap_or_default();
        let mut fields = line.split_whitespace();
        let Some(keyword) = fields.next() else {
            continue;
        };
        match keyword {
            "v" => {
                let values = numbers(fields);
                let xyz = [0, 1, 2].map(|i| values.get(i).copied().flatten());
                if xyz.iter().any(Option::is_none) {
                    bad_numbers += 1;
                }
                positions.push(xyz.map(|v| v.unwrap_or(f32::NAN)));
                // Colours follow the position; a lone fourth number is a weight.
                let rgb = [3, 4, 5].map(|i| values.get(i).copied().flatten());
                colors.push(
                    rgb.iter()
                        .all(Option::is_some)
                        .then(|| rgb.map(|v| srgb_to_linear(v.unwrap_or(0.0)))),
                );
            }
            "vn" => {
                let values = numbers(fields);
                let n = [0, 1, 2].map(|i| values.get(i).copied().flatten().unwrap_or(0.0));
                normals.push(n);
            }
            "f" => {
                let mut face: Vec<u32> = Vec::new();
                let mut usable = true;
                for corner in fields {
                    let mut parts = corner.split('/');
                    let position = parts.next().and_then(|p| resolve_index(p, positions.len()));
                    let normal = parts
                        .nth(1)
                        .filter(|n| !n.is_empty())
                        .and_then(|n| resolve_index(n, normals.len()));
                    let Some(position) = position.filter(|p| positions[*p][0].is_finite()) else {
                        usable = false;
                        break;
                    };
                    let index = *corners.entry((position, normal)).or_insert_with(|| {
                        mesh.positions.push(positions[position]);
                        corner_normals.push(normal);
                        corner_colors.push(colors[position]);
                        (mesh.positions.len() - 1) as u32
                    });
                    face.push(index);
                }
                if !usable || face.len() < 3 {
                    skipped_faces += 1;
                    continue;
                }
                for i in 1..face.len() - 1 {
                    mesh.indices.extend([face[0], face[i], face[i + 1]]);
                }
            }
            "mtllib" | "usemtl" => has_materials = true,
            "curv" | "curv2" | "surf" | "cstype" => has_curves = true,
            _ => {}
        }
    }

    // Normals and colours are kept only when every vertex in use has one.
    if !corner_normals.is_empty() && corner_normals.iter().all(Option::is_some) {
        mesh.normals = corner_normals
            .iter()
            .map(|n| normals[n.expect("checked above")])
            .collect();
    }
    let has_some_normals = corner_normals.iter().any(Option::is_some);
    let colored = corner_colors.iter().filter(|c| c.is_some()).count();
    if colored > 0 {
        mesh.colors = corner_colors
            .iter()
            .map(|c| c.unwrap_or([0.5, 0.5, 0.5]))
            .collect();
    }

    let mut warnings = Vec::new();
    if has_materials {
        warnings.push("the model's materials and textures are not imported".to_owned());
    }
    if has_curves {
        warnings.push("the model's curves and free-form surfaces are not imported".to_owned());
    }
    if skipped_faces > 0 {
        warnings.push(format!(
            "{skipped_faces} face(s) of the model were skipped (fewer than three corners, or pointing at a missing vertex)"
        ));
    }
    if bad_numbers > 0 {
        warnings.push(format!(
            "{bad_numbers} vertex position(s) of the model were not numbers; the faces using them were skipped"
        ));
    }
    if has_some_normals && mesh.normals.is_empty() {
        warnings.push(
            "only some faces of the model have normals; flat ones are used for all".to_owned(),
        );
    }
    if colored > 0 && colored < corner_colors.len() {
        warnings
            .push("only some vertices of the model have a colour; the rest are grey".to_owned());
    }
    ParsedObj { mesh, warnings }
}

/// Drops triangles that cannot be drawn and brings the optional streams to the length
/// of the positions. Returns how many triangles were dropped.
fn sanitize_mesh(mesh: &mut MeshData) -> usize {
    let count = mesh.positions.len();
    if mesh.normals.len() != count {
        mesh.normals.clear();
    }
    if mesh.colors.len() != count {
        mesh.colors.clear();
    }
    for value in mesh.normals.iter_mut().chain(&mut mesh.colors).flatten() {
        if !value.is_finite() {
            *value = 0.0;
        }
    }
    let usable = |i: &u32| {
        mesh.positions
            .get(*i as usize)
            .is_some_and(|p| p.iter().all(|v| v.is_finite()))
    };
    let before = mesh.indices.len() / 3;
    let kept: Vec<u32> = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter(|t| t.iter().all(usable))
        .flatten()
        .copied()
        .collect();
    mesh.indices = kept;
    before - mesh.indices.len() / 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        bytes.extend([8, 2, 0, 0, 0]);
        bytes
    }

    fn jpeg_header(width: u16, height: u16) -> Vec<u8> {
        let mut bytes = vec![0xFF, 0xD8];
        // An application segment and a quantisation table come before the frame header.
        bytes.extend([0xFF, 0xE0, 0x00, 0x04, 0x4A, 0x46]);
        bytes.extend([0xFF, 0xDB, 0x00, 0x03, 0x00]);
        bytes.extend([0xFF, 0xC2, 0x00, 0x0B, 0x08]);
        bytes.extend(height.to_be_bytes());
        bytes.extend(width.to_be_bytes());
        bytes.extend([0x01, 0x01, 0x11, 0x00]);
        bytes
    }

    fn wav(channels: u16) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
        // An odd-sized chunk in front checks the padding rule.
        bytes.extend(b"LIST\x03\0\0\0abc\0");
        bytes.extend(b"fmt \x10\0\0\0");
        bytes.extend(1u16.to_le_bytes());
        bytes.extend(channels.to_le_bytes());
        bytes.extend(44_100u32.to_le_bytes());
        bytes.extend((44_100u32 * 2 * channels as u32).to_le_bytes());
        bytes.extend((2 * channels).to_le_bytes());
        bytes.extend(16u16.to_le_bytes());
        bytes.extend(b"data\x04\0\0\0\0\0\0\0");
        bytes
    }

    fn ogg(packet: &[u8]) -> Vec<u8> {
        let mut bytes = b"OggS".to_vec();
        bytes.extend([0u8; 22]);
        bytes.extend([1, packet.len() as u8]);
        bytes.extend(packet);
        bytes
    }

    fn write(dir: &tempfile::TempDir, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn extensions_decide_what_a_file_is() {
        assert_eq!(kind_of(Path::new("a/b.PNG")).unwrap(), FileKind::Png);
        assert_eq!(kind_of(Path::new("b.jpeg")).unwrap(), FileKind::Jpg);
        assert_eq!(kind_of(Path::new("b.Glb")).unwrap(), FileKind::Gltf);
        assert!(kind_of(Path::new("b.fbx")).unwrap().needs_engine());
        assert!(!kind_of(Path::new("b.obj")).unwrap().needs_engine());
        let error = kind_of(Path::new("notes.txt")).unwrap_err().to_string();
        assert!(
            error.starts_with(".txt files cannot be imported"),
            "{error}"
        );
        assert!(kind_of(Path::new("README")).is_err());
    }

    #[test]
    fn picture_sizes_are_read_from_headers() {
        assert_eq!(png_size(&png_header(640, 480)), Some((640, 480)));
        assert_eq!(png_size(b"\x89PNG\r\n\x1a\n"), None);
        assert_eq!(png_size(b"GIF89a"), None);
        assert_eq!(jpeg_size(&jpeg_header(1920, 1080)), Some((1920, 1080)));
        assert_eq!(jpeg_size(&[0xFF, 0xD8, 0xFF, 0xD9]), None);
        assert_eq!(jpeg_size(&[0xFF, 0xD8, 0xFF]), None);
        assert_eq!(jpeg_size(&png_header(1, 1)), None);
    }

    #[test]
    fn a_picture_is_embedded_as_the_file_it_is() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = png_header(32, 16);
        let path = write(&dir, "Tree.png", &bytes);
        let import = read(&path).unwrap();
        assert_eq!(import.media.kind, MediaLayerKind::Picture);
        assert_eq!(import.media.file_name, "Tree.png");
        assert_eq!(import.media.source_path, path.display().to_string());
        assert_eq!(import.media.data, encoded("png", bytes));
        assert!(import.warnings.is_empty());

        let jpeg = write(&dir, "photo.JPG", &jpeg_header(8, 8));
        let MediaData::Encoded { format, .. } = read(&jpeg).unwrap().media.data else {
            panic!("not embedded as a file")
        };
        assert_eq!(format, "jpg");
    }

    #[test]
    fn pictures_that_are_too_large_or_not_pictures_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let huge = write(&dir, "huge.png", &png_header(20_000, 100));
        assert_eq!(
            read(&huge).unwrap_err().to_string(),
            "the picture is 20000 by 100 pixels; the most that can be shown is 16384 on a side"
        );
        let edge = write(&dir, "edge.png", &png_header(16384, 16384));
        assert!(read(&edge).is_ok());
        let text = write(&dir, "text.png", b"hello");
        assert!(matches!(
            read(&text).unwrap_err(),
            MediaError::NotAPicture("PNG")
        ));
        let empty = write(&dir, "empty.jpg", &jpeg_header(0, 10));
        assert!(read(&empty).is_err());
        assert!(matches!(
            read(&dir.path().join("missing.png")).unwrap_err(),
            MediaError::Read(_)
        ));
        assert!(matches!(
            read(Path::new("scene.glb")).unwrap_err(),
            MediaError::NeedsEngine
        ));
    }

    #[test]
    fn mono_sounds_start_positional_and_wider_ones_flat() {
        let dir = tempfile::tempdir().unwrap();
        let kind = |name: &str, bytes: &[u8]| match read(&write(&dir, name, bytes)) {
            Ok(import) => match import.media.kind {
                MediaLayerKind::Sound { kind } => Ok(kind),
                other => panic!("{name} became {other:?}"),
            },
            Err(error) => Err(error.to_string()),
        };
        assert_eq!(kind("mono.wav", &wav(1)), Ok(SoundKind::Positional));
        assert_eq!(kind("stereo.wav", &wav(2)), Ok(SoundKind::Flat));
        assert_eq!(kind("quad.wav", &wav(4)), Ok(SoundKind::Flat));

        let vorbis = |channels: u8| {
            let mut packet = b"\x01vorbis\0\0\0\0".to_vec();
            packet.push(channels);
            packet.extend(44_100u32.to_le_bytes());
            ogg(&packet)
        };
        assert_eq!(kind("mono.ogg", &vorbis(1)), Ok(SoundKind::Positional));
        assert_eq!(kind("stereo.ogg", &vorbis(2)), Ok(SoundKind::Flat));
        assert_eq!(
            kind("voice.ogg", &ogg(b"OpusHead\x01\x02")),
            Err("the sound is Ogg Opus; only Ogg Vorbis can be played".to_owned())
        );

        // MPEG-1 layer III, 128 kbit/s, 44.1 kHz; the last byte holds the channel mode.
        let mut tagged = b"ID3\x04\0\0\0\0\0\x05tag!!".to_vec();
        tagged.extend([0xFF, 0xFB, 0x90, 0xC4]);
        assert_eq!(kind("mono.mp3", &tagged), Ok(SoundKind::Positional));
        assert_eq!(
            kind("stereo.mp3", &[0xFF, 0xFB, 0x90, 0x44]),
            Ok(SoundKind::Flat)
        );

        assert!(kind("noise.wav", b"RIFF\0\0\0\0AVI ").is_err());
        assert!(kind("headless.wav", b"RIFF\0\0\0\0WAVEdata\0\0\0\0").is_err());
        assert!(kind("noise.ogg", b"not a sound").is_err());
        assert!(kind("noise.mp3", &[0u8; 300]).is_err());
    }

    #[test]
    fn obj_faces_are_triangulated_and_vertices_are_shared() {
        let parsed = parse_obj(
            "# a square and a triangle\n\
             v 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\nv 2 0 0\n\
             f 1 2 3 4\n\
             f 2 5 3\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.mesh.positions.len(), 5);
        assert_eq!(parsed.mesh.indices, [0, 1, 2, 0, 2, 3, 1, 4, 2]);
        assert!(parsed.mesh.normals.is_empty());
        assert!(parsed.mesh.colors.is_empty());
    }

    #[test]
    fn obj_normals_split_vertices_and_indices_may_count_from_the_end() {
        let parsed = parse_obj(
            "v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\n\
             vt 0 0\n\
             vn 0 0 1\nvn 1 0 0\n\
             f 1/1/1 2/1/1 3/1/1\n\
             f -4//2 -2//2 -1//2\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        // The first and third positions are used with two different normals.
        assert_eq!(parsed.mesh.positions.len(), 6);
        assert_eq!(parsed.mesh.normals.len(), 6);
        assert_eq!(parsed.mesh.indices, [0, 1, 2, 3, 4, 5]);
        assert_eq!(parsed.mesh.normals[0], [0.0, 0.0, 1.0]);
        assert_eq!(parsed.mesh.normals[3], [1.0, 0.0, 0.0]);
        assert_eq!(parsed.mesh.positions[3], [0.0, 0.0, 0.0]);
        assert_eq!(parsed.mesh.positions[5], [0.0, 0.0, 1.0]);
    }

    #[test]
    fn obj_vertex_colours_are_read_as_srgb() {
        let parsed = parse_obj(
            "v 0 0 0 1 0 0\nv 1 0 0 0 0.5 0\nv 0 1 0 0 0 1\nv 5 5 5 0.5\n\
             f 1 2 3\n",
        );
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.mesh.colors.len(), 3);
        assert_eq!(parsed.mesh.colors[0], [1.0, 0.0, 0.0]);
        assert!((parsed.mesh.colors[1][1] - 0.214).abs() < 0.001);
    }

    #[test]
    fn obj_problems_are_reported_and_do_not_stop_the_import() {
        let parsed = parse_obj(
            "mtllib scene.mtl\nusemtl wood\n\
             v 0 0 0 1 1 1\nv 1 0 0\nv 0 1 0\nv x 0 0\n\
             vn 0 0 1\n\
             f 1//1 2//1 3//1\n\
             f 1 2 3\n\
             f 1 2\n\
             f 1 2 9\n\
             f 1 2 4\n",
        );
        assert_eq!(parsed.mesh.indices.len(), 6);
        assert!(parsed.mesh.normals.is_empty(), "not every corner has one");
        assert_eq!(parsed.mesh.colors.len(), parsed.mesh.positions.len());
        assert_eq!(
            parsed.warnings,
            [
                "the model's materials and textures are not imported",
                "3 face(s) of the model were skipped (fewer than three corners, or pointing at a missing vertex)",
                "1 vertex position(s) of the model were not numbers; the faces using them were skipped",
                "only some faces of the model have normals; flat ones are used for all",
                "only some vertices of the model have a colour; the rest are grey",
            ]
        );
    }

    #[test]
    fn an_obj_file_becomes_a_model_and_an_empty_one_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(&dir, "rock.obj", b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n");
        let import = read(&path).unwrap();
        assert_eq!(import.media.kind, MediaLayerKind::Model);
        let MediaData::Mesh(mesh) = &import.media.data else {
            panic!("not a mesh")
        };
        assert_eq!(mesh.indices, [0, 1, 2]);
        let empty = write(&dir, "points.obj", b"v 0 0 0\nv 1 0 0\n");
        assert!(matches!(read(&empty).unwrap_err(), MediaError::EmptyModel));
    }

    #[test]
    fn engine_decoded_geometry_is_checked_before_it_becomes_a_model() {
        let mesh = MeshData {
            positions: vec![
                [0.0; 3],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [f32::NAN, 0.0, 0.0],
            ],
            normals: vec![[0.0, 0.0, 1.0]; 2],
            colors: vec![[1.0, f32::INFINITY, 0.0]; 4],
            indices: vec![0, 1, 2, 0, 1, 3, 0, 1, 7, 2, 1],
        };
        let import = model_from_mesh(
            Path::new("/models/ship.glb"),
            mesh,
            vec!["the model's animations are not imported".into()],
        )
        .unwrap();
        let MediaData::Mesh(mesh) = &import.media.data else {
            panic!("not a mesh")
        };
        assert_eq!(mesh.indices, [0, 1, 2]);
        assert!(mesh.normals.is_empty(), "a stream of the wrong length goes");
        assert_eq!(mesh.colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(import.media.file_name, "ship.glb");
        assert_eq!(
            import.warnings,
            [
                "the model's animations are not imported",
                "2 triangle(s) of the model were unusable and were left out"
            ]
        );
        assert!(matches!(
            model_from_mesh(Path::new("a.glb"), MeshData::default(), Vec::new()).unwrap_err(),
            MediaError::EmptyModel
        ));
    }
}
