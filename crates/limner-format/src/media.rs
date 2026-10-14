//! Media files: how each [`MediaData`] variant is written to `media/` and read back.
//!
//! Every item has a canonical form whose BLAKE3 hash names its file:
//!
//! * `Encoded`: the bytes themselves.
//! * `RawImage`: a `LIMNIMG1` container (header below, then the pixel bytes).
//! * `RawAudio`: a `LIMNAUD1` container (header below, then the sample bytes).
//! * `Mesh`: a `LIMNMSH1` container (see [`crate::mesh`]).
//!
//! Raw images and raw audio are *stored* as PNG and WAVE files whenever those can
//! carry them exactly, so other tools can open them; otherwise the container itself
//! is stored. Either way they load back as the same raw variant, bit for bit.
//!
//! ```text
//! LIMNIMG1                           LIMNAUD1
//! 0   8  "LIMNIMG1"                  0   8  "LIMNAUD1"
//! 8   4  flags                       8   4  flags
//! 12  4  width                       12  4  sample rate
//! 16  4  height                      16  2  channels
//! 20  4  channels                    18  2  bits per sample
//! 24  8  pixel byte count            20  4  sample format (0 = integer PCM, 1 = IEEE float)
//! 32  .. pixels                      24  8  sample byte count
//!                                    32  .. samples
//! ```

use crate::container::{self, u16_at, u32_at, u64_at, Canonical};
use crate::hash::ContentHash;
use crate::mesh;
use crate::wav::{self, AudioSpec};
use limner_core::model::MediaData;
use std::borrow::Cow;
use std::sync::Arc;

pub(crate) const IMAGE_MAGIC: &[u8; 8] = b"LIMNIMG1";
pub(crate) const AUDIO_MAGIC: &[u8; 8] = b"LIMNAUD1";
const RAW_HEADER_LEN: usize = 32;
const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

/// Which [`MediaData`] variant a media file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Storage {
    Encoded,
    RawImage,
    RawAudio,
    Mesh,
}

/// A media item ready to be written: its hash is known, its file bytes are produced
/// only if the file does not exist yet.
pub(crate) struct Prepared<'a> {
    pub storage: Storage,
    pub hash: ContentHash,
    pub extension: Cow<'static, str>,
    content: Content<'a>,
}

enum Content<'a> {
    Bytes(&'a [u8]),
    Image {
        header: Vec<u8>,
        width: u32,
        height: u32,
        color: Option<png::ColorType>,
        pixels: &'a [u8],
    },
    Audio {
        header: Vec<u8>,
        wav: Option<AudioSpec>,
        samples: &'a [u8],
    },
    Mesh(Canonical),
}

fn image_header(width: u32, height: u32, channels: u8, len: usize) -> Vec<u8> {
    let mut header = Vec::with_capacity(RAW_HEADER_LEN);
    header.extend_from_slice(IMAGE_MAGIC);
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&width.to_le_bytes());
    header.extend_from_slice(&height.to_le_bytes());
    header.extend_from_slice(&(channels as u32).to_le_bytes());
    header.extend_from_slice(&(len as u64).to_le_bytes());
    header
}

fn audio_header(spec: AudioSpec, len: usize) -> Vec<u8> {
    let mut header = Vec::with_capacity(RAW_HEADER_LEN);
    header.extend_from_slice(AUDIO_MAGIC);
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(&spec.sample_rate.to_le_bytes());
    header.extend_from_slice(&spec.channels.to_le_bytes());
    header.extend_from_slice(&spec.bits_per_sample.to_le_bytes());
    header.extend_from_slice(&(spec.float as u32).to_le_bytes());
    header.extend_from_slice(&(len as u64).to_le_bytes());
    header
}

fn png_color(channels: u8) -> Option<png::ColorType> {
    match channels {
        1 => Some(png::ColorType::Grayscale),
        2 => Some(png::ColorType::GrayscaleAlpha),
        3 => Some(png::ColorType::Rgb),
        4 => Some(png::ColorType::Rgba),
        _ => None,
    }
}

/// The extension for an embedded file: its format when that is a short plain word.
fn encoded_extension(format: &str) -> Cow<'static, str> {
    let plain = !format.is_empty()
        && format.len() <= 8
        && format.bytes().all(|b| b.is_ascii_alphanumeric());
    if plain {
        Cow::Owned(format.to_ascii_lowercase())
    } else {
        Cow::Borrowed("bin")
    }
}

pub(crate) fn prepare(data: &MediaData) -> Result<Prepared<'_>, String> {
    Ok(match data {
        MediaData::Encoded { format, bytes } => Prepared {
            storage: Storage::Encoded,
            hash: ContentHash::of(&[bytes]),
            extension: encoded_extension(format),
            content: Content::Bytes(bytes),
        },
        MediaData::RawImage {
            width,
            height,
            channels,
            pixels,
        } => {
            let header = image_header(*width, *height, *channels, pixels.len());
            let exact = *width > 0
                && *height > 0
                && *width as u64 * *height as u64 * *channels as u64 == pixels.len() as u64;
            let color = png_color(*channels).filter(|_| exact);
            Prepared {
                storage: Storage::RawImage,
                hash: ContentHash::of(&[&header, pixels]),
                extension: Cow::Borrowed(if color.is_some() { "png" } else { "limg" }),
                content: Content::Image {
                    header,
                    width: *width,
                    height: *height,
                    color,
                    pixels,
                },
            }
        }
        MediaData::RawAudio {
            channels,
            sample_rate,
            bits_per_sample,
            float,
            samples,
        } => {
            let spec = AudioSpec {
                channels: *channels,
                sample_rate: *sample_rate,
                bits_per_sample: *bits_per_sample,
                float: *float,
            };
            let header = audio_header(spec, samples.len());
            let wav = spec.fits_wav(samples.len()).then_some(spec);
            Prepared {
                storage: Storage::RawAudio,
                hash: ContentHash::of(&[&header, samples]),
                extension: Cow::Borrowed(if wav.is_some() { "wav" } else { "laud" }),
                content: Content::Audio {
                    header,
                    wav,
                    samples,
                },
            }
        }
        MediaData::Mesh(data) => {
            let canonical = mesh::encode(data)?;
            Prepared {
                storage: Storage::Mesh,
                hash: canonical.hash,
                extension: Cow::Borrowed("lmsh"),
                content: Content::Mesh(canonical),
            }
        }
    })
}

impl Prepared<'_> {
    /// `<hash>.<extension>`: the item's file name inside `media/`.
    pub fn file_name(&self) -> String {
        format!("{}.{}", self.hash, self.extension)
    }

    pub fn file_bytes(&self, level: i32) -> Result<Cow<'_, [u8]>, String> {
        Ok(match &self.content {
            Content::Bytes(bytes) => Cow::Borrowed(*bytes),
            Content::Image {
                width,
                height,
                color: Some(color),
                pixels,
                ..
            } => {
                let mut out = Vec::new();
                let mut encoder = png::Encoder::new(&mut out, *width, *height);
                encoder.set_color(*color);
                encoder.set_depth(png::BitDepth::Eight);
                let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
                writer.write_image_data(pixels).map_err(|e| e.to_string())?;
                writer.finish().map_err(|e| e.to_string())?;
                Cow::Owned(out)
            }
            Content::Image { header, pixels, .. } => {
                Cow::Owned(Canonical::new(header.clone(), pixels.to_vec()).file_bytes(level))
            }
            Content::Audio {
                wav: Some(spec),
                samples,
                ..
            } => Cow::Owned(wav::encode(*spec, samples)),
            Content::Audio {
                header, samples, ..
            } => Cow::Owned(Canonical::new(header.clone(), samples.to_vec()).file_bytes(level)),
            Content::Mesh(canonical) => Cow::Owned(canonical.file_bytes(level)),
        })
    }
}

fn check(hash: ContentHash, expected: Option<&ContentHash>) -> Result<(), String> {
    match expected {
        Some(expected) if hash != *expected => Err(format!(
            "its content hashes to {hash} but its name says {expected}"
        )),
        _ => Ok(()),
    }
}

fn decode_png(bytes: &[u8]) -> Result<(u32, u32, u8, Vec<u8>), String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let (color, depth) = reader.output_color_type();
    let channels = match color {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => return Err("it is a palette image".to_owned()),
    };
    if depth != png::BitDepth::Eight {
        return Err("it is not an 8-bit image".to_owned());
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| "it is too large to decode".to_owned())?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(size)
        .map_err(|_| "it is too large to decode".to_owned())?;
    pixels.resize(size, 0);
    let frame = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
    pixels.truncate(frame.buffer_size());
    Ok((frame.width, frame.height, channels, pixels))
}

/// Reads a media file back into the variant the manifest says it holds.
pub(crate) fn decode(
    storage: Storage,
    format: Option<&str>,
    bytes: Vec<u8>,
    expected: Option<&ContentHash>,
) -> Result<MediaData, String> {
    match storage {
        Storage::Encoded => {
            check(ContentHash::of(&[&bytes]), expected)?;
            Ok(MediaData::Encoded {
                format: format.unwrap_or_default().to_owned(),
                bytes: Arc::from(bytes),
            })
        }
        Storage::RawImage => {
            if bytes.starts_with(PNG_SIGNATURE) {
                let (width, height, channels, pixels) =
                    decode_png(&bytes).map_err(|e| format!("its PNG data cannot be read ({e})"))?;
                let header = image_header(width, height, channels, pixels.len());
                check(ContentHash::of(&[&header, &pixels]), expected)?;
                return Ok(MediaData::RawImage {
                    width,
                    height,
                    channels,
                    pixels: Arc::from(pixels),
                });
            }
            let canonical = container::open(&bytes, IMAGE_MAGIC, RAW_HEADER_LEN, expected, |h| {
                Ok(u64_at(h, 24))
            })?;
            let channels = u8::try_from(u32_at(&canonical.header, 20))
                .map_err(|_| "its channel count is out of range".to_owned())?;
            Ok(MediaData::RawImage {
                width: u32_at(&canonical.header, 12),
                height: u32_at(&canonical.header, 16),
                channels,
                pixels: Arc::from(canonical.payload),
            })
        }
        Storage::RawAudio => {
            if wav::is_wav(&bytes) {
                let (spec, samples) = wav::decode(&bytes)
                    .map_err(|e| format!("its WAVE data cannot be read: {e}"))?;
                let header = audio_header(spec, samples.len());
                check(ContentHash::of(&[&header, samples]), expected)?;
                return Ok(MediaData::RawAudio {
                    channels: spec.channels,
                    sample_rate: spec.sample_rate,
                    bits_per_sample: spec.bits_per_sample,
                    float: spec.float,
                    samples: Arc::from(samples),
                });
            }
            let canonical = container::open(&bytes, AUDIO_MAGIC, RAW_HEADER_LEN, expected, |h| {
                if u32_at(h, 20) > 1 {
                    return Err("it uses a sample format this version does not know".to_owned());
                }
                Ok(u64_at(h, 24))
            })?;
            Ok(MediaData::RawAudio {
                channels: u16_at(&canonical.header, 16),
                sample_rate: u32_at(&canonical.header, 12),
                bits_per_sample: u16_at(&canonical.header, 18),
                float: u32_at(&canonical.header, 20) == 1,
                samples: Arc::from(canonical.payload),
            })
        }
        Storage::Mesh => Ok(MediaData::Mesh(Arc::new(mesh::decode(&bytes, expected)?))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner_core::model::MeshData;

    fn round_trip(data: &MediaData) -> (String, Vec<u8>) {
        let prepared = prepare(data).unwrap();
        let bytes = prepared.file_bytes(3).unwrap().into_owned();
        let format = match data {
            MediaData::Encoded { format, .. } => Some(format.as_str()),
            _ => None,
        };
        let back = decode(
            prepared.storage,
            format,
            bytes.clone(),
            Some(&prepared.hash),
        )
        .unwrap();
        assert_eq!(&back, data);
        // The hash names the content, not the file: preparing the loaded item agrees.
        assert_eq!(prepare(&back).unwrap().file_name(), prepared.file_name());
        (prepared.extension.to_string(), bytes)
    }

    #[test]
    fn images_become_png_when_png_can_hold_them() {
        for channels in 1..=4u8 {
            let pixels: Vec<u8> = (0..3 * 2 * channels as usize)
                .map(|i| (i * 37) as u8)
                .collect();
            let (extension, bytes) = round_trip(&MediaData::RawImage {
                width: 3,
                height: 2,
                channels,
                pixels: Arc::from(pixels),
            });
            assert_eq!(extension, "png");
            assert!(bytes.starts_with(PNG_SIGNATURE));
        }
    }

    #[test]
    fn images_png_cannot_hold_keep_their_bytes_in_a_container() {
        for (width, height, channels, len) in
            [(0, 0, 4, 0), (2, 2, 5, 20), (2, 2, 3, 11), (4, 1, 0, 0)]
        {
            let (extension, bytes) = round_trip(&MediaData::RawImage {
                width,
                height,
                channels,
                pixels: Arc::from(vec![7u8; len]),
            });
            assert_eq!(extension, "limg");
            assert!(bytes.starts_with(IMAGE_MAGIC));
        }
    }

    #[test]
    fn audio_becomes_wave_when_wave_can_hold_it() {
        let samples: Arc<[u8]> = Arc::from((0..96u8).collect::<Vec<_>>());
        let (extension, bytes) = round_trip(&MediaData::RawAudio {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 24,
            float: false,
            samples: samples.clone(),
        });
        assert_eq!(extension, "wav");
        assert!(wav::is_wav(&bytes));
        let (extension, _) = round_trip(&MediaData::RawAudio {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 32,
            float: true,
            samples: samples.clone(),
        });
        assert_eq!(extension, "wav");
        let (extension, bytes) = round_trip(&MediaData::RawAudio {
            channels: 0,
            sample_rate: 0,
            bits_per_sample: 12,
            float: true,
            samples,
        });
        assert_eq!(extension, "laud");
        assert!(bytes.starts_with(AUDIO_MAGIC));
    }

    #[test]
    fn embedded_files_and_meshes_round_trip() {
        let (extension, bytes) = round_trip(&MediaData::Encoded {
            format: "OGG".to_owned(),
            bytes: Arc::from(&b"OggS not really"[..]),
        });
        assert_eq!(extension, "ogg");
        assert_eq!(bytes, b"OggS not really");
        let (extension, _) = round_trip(&MediaData::Encoded {
            format: "../weird format".to_owned(),
            bytes: Arc::from(&b""[..]),
        });
        assert_eq!(extension, "bin");
        let (extension, _) = round_trip(&MediaData::Mesh(Arc::new(MeshData {
            positions: vec![[0.0; 3], [1.0; 3], [2.0; 3]],
            normals: vec![[0.0, 1.0, 0.0]; 3],
            colors: vec![[0.5; 3]; 3],
            indices: vec![0, 1, 2],
        })));
        assert_eq!(extension, "lmsh");
    }

    #[test]
    fn same_pixels_with_another_shape_are_other_content() {
        let pixels: Arc<[u8]> = Arc::from(vec![9u8; 12]);
        let wide = MediaData::RawImage {
            width: 4,
            height: 1,
            channels: 3,
            pixels: pixels.clone(),
        };
        let tall = MediaData::RawImage {
            width: 1,
            height: 4,
            channels: 3,
            pixels,
        };
        assert_ne!(prepare(&wide).unwrap().hash, prepare(&tall).unwrap().hash);
    }

    #[test]
    fn altered_media_is_rejected() {
        let data = MediaData::RawImage {
            width: 8,
            height: 8,
            channels: 3,
            pixels: Arc::from((0..192u8).collect::<Vec<_>>()),
        };
        let prepared = prepare(&data).unwrap();
        let bytes = prepared.file_bytes(3).unwrap().into_owned();
        let truncated = bytes[..bytes.len() - 20].to_vec();
        assert!(decode(Storage::RawImage, None, truncated, Some(&prepared.hash)).is_err());
        let other = MediaData::RawImage {
            width: 8,
            height: 8,
            channels: 3,
            pixels: Arc::from(vec![1u8; 192]),
        };
        let other = prepare(&other).unwrap().hash;
        let err = decode(Storage::RawImage, None, bytes, Some(&other)).unwrap_err();
        assert!(err.contains("hashes to"), "{err}");
        let err = decode(Storage::Encoded, Some("png"), b"abc".to_vec(), Some(&other)).unwrap_err();
        assert!(err.contains("hashes to"), "{err}");
    }
}
