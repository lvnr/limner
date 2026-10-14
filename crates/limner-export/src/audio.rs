//! A sound layer's media as a file a format can carry: the original encoding when the
//! document kept one, or a WAV file written from the samples it holds.

use crate::scene::ExportAudio;
use limner_core::model::{Media, MediaData};
use std::sync::Arc;

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// The audio of a media item, or `None` when it holds no audio a file can carry.
/// The encoding's name must be a plain extension, since it names the file.
pub(crate) fn export_audio(media: &Media) -> Option<ExportAudio> {
    match &media.data {
        MediaData::Encoded { format, bytes } => {
            let format = format.to_ascii_lowercase();
            let plain = !format.is_empty() && format.chars().all(|c| c.is_ascii_alphanumeric());
            if !plain || bytes.is_empty() {
                return None;
            }
            let duration = if format == "wav" {
                wav_duration(bytes)
            } else {
                None
            };
            Some(ExportAudio {
                format,
                bytes: bytes.clone(),
                duration,
            })
        }
        MediaData::RawAudio {
            channels,
            sample_rate,
            bits_per_sample,
            float,
            samples,
        } => {
            let bytes = encode_wav(*channels, *sample_rate, *bits_per_sample, *float, samples)?;
            let duration = wav_duration(&bytes);
            Some(ExportAudio {
                format: "wav".to_owned(),
                bytes: Arc::from(bytes),
                duration,
            })
        }
        MediaData::RawImage { .. } | MediaData::Mesh(_) => None,
    }
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

/// A WAV file of interleaved little-endian samples, or `None` for a layout WAV cannot
/// hold (the same rule the `.limn` format uses when it stores samples as WAV).
pub(crate) fn encode_wav(
    channels: u16,
    sample_rate: u32,
    bits: u16,
    float: bool,
    samples: &[u8],
) -> Option<Vec<u8>> {
    let bits_ok = if float {
        matches!(bits, 32 | 64)
    } else {
        matches!(bits, 8 | 16 | 24 | 32)
    };
    let align = u64::from(channels) * u64::from(bits / 8);
    let fits = bits_ok
        && channels >= 1
        && sample_rate >= 1
        && align <= u64::from(u16::MAX)
        && u64::from(sample_rate) * align <= u64::from(u32::MAX)
        && (samples.len() as u64).is_multiple_of(align)
        && samples.len() as u64 <= u64::from(u32::MAX) - 64;
    if !fits {
        return None;
    }
    let mut fmt = Vec::with_capacity(18);
    fmt.extend_from_slice(&(if float { FORMAT_FLOAT } else { FORMAT_PCM }).to_le_bytes());
    fmt.extend_from_slice(&channels.to_le_bytes());
    fmt.extend_from_slice(&sample_rate.to_le_bytes());
    fmt.extend_from_slice(&((u64::from(sample_rate) * align) as u32).to_le_bytes());
    fmt.extend_from_slice(&(align as u16).to_le_bytes());
    fmt.extend_from_slice(&bits.to_le_bytes());

    let mut body = Vec::with_capacity(samples.len() + 64);
    body.extend_from_slice(b"WAVE");
    if float {
        // Formats other than plain PCM carry an (empty) extension size and a `fact` chunk.
        fmt.extend_from_slice(&0u16.to_le_bytes());
        chunk(&mut body, b"fmt ", &fmt);
        chunk(
            &mut body,
            b"fact",
            &((samples.len() as u64 / align) as u32).to_le_bytes(),
        );
    } else {
        chunk(&mut body, b"fmt ", &fmt);
    }
    chunk(&mut body, b"data", samples);
    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Some(out)
}

/// The length in seconds of an uncompressed (PCM or float) WAV file, read from its
/// header: the size of the data over the bytes per second. `None` for anything else.
pub(crate) fn wav_duration(bytes: &[u8]) -> Option<f64> {
    if bytes.get(..4)? != b"RIFF" || bytes.get(8..12)? != b"WAVE" {
        return None;
    }
    let mut byte_rate = None;
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4)? as usize;
        let start = at + 8;
        match id {
            b"fmt " => {
                let mut tag = u16_at(bytes, start)?;
                if tag == FORMAT_EXTENSIBLE && size >= 26 {
                    tag = u16_at(bytes, start + 24)?;
                }
                if tag != FORMAT_PCM && tag != FORMAT_FLOAT {
                    return None;
                }
                byte_rate = Some(u32_at(bytes, start + 8)?);
            }
            b"data" => {
                let rate = byte_rate.filter(|r| *r > 0)?;
                // A file cut short holds less than its header says.
                let size = size.min(bytes.len() - start);
                return Some(size as f64 / f64::from(rate));
            }
            _ => {}
        }
        at = start.checked_add(size)?.checked_add(size % 2)?;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use limner_core::model::{MediaId, MediaKind};

    #[test]
    fn samples_become_a_wav_file_that_states_its_length() {
        let samples = vec![0u8; 4 * 8000];
        let file = encode_wav(2, 8000, 16, false, &samples).unwrap();
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(u32_at(&file, 4).unwrap() as usize, file.len() - 8);
        assert_eq!(file.len(), 44 + samples.len());
        assert_eq!(u16_at(&file, 20), Some(FORMAT_PCM));
        assert_eq!(u32_at(&file, 28), Some(32_000), "bytes per second");
        assert_eq!(wav_duration(&file), Some(1.0));

        let floats = encode_wav(1, 1000, 32, true, &[0; 2000]).unwrap();
        assert_eq!(u16_at(&floats, 20), Some(FORMAT_FLOAT));
        assert_eq!(wav_duration(&floats), Some(0.5));

        // Layouts WAV has no code for, and sample data that does not fill whole frames.
        assert!(encode_wav(1, 1000, 12, false, &[0; 6]).is_none());
        assert!(encode_wav(1, 1000, 16, true, &[0; 6]).is_none());
        assert!(encode_wav(0, 1000, 16, false, &[]).is_none());
        assert!(encode_wav(2, 1000, 16, false, &[0; 6]).is_none());
    }

    #[test]
    fn only_uncompressed_wav_files_have_a_known_length() {
        assert_eq!(wav_duration(b"ID3\x04 not a wave file"), None);
        assert_eq!(wav_duration(b"RIFF"), None);
        let mut file = encode_wav(1, 100, 8, false, &[128; 50]).unwrap();
        assert_eq!(wav_duration(&file), Some(0.5));
        // A compressed encoding (tag 2, ADPCM) has no length without decoding.
        file[20] = 2;
        assert_eq!(wav_duration(&file), None);
        // A file cut short: what is there counts.
        let short = encode_wav(1, 100, 8, false, &[128; 50]).unwrap();
        assert_eq!(wav_duration(&short[..short.len() - 25]), Some(0.25));
    }

    #[test]
    fn media_items_become_audio_files_when_they_can() {
        let item = |data| Media {
            id: MediaId(1),
            name: "rain.WAV".to_owned(),
            kind: MediaKind::Audio,
            data,
        };
        let wav = encode_wav(1, 100, 8, false, &[128; 100]).unwrap();
        let audio = export_audio(&item(MediaData::Encoded {
            format: "WAV".to_owned(),
            bytes: Arc::from(wav.clone()),
        }))
        .unwrap();
        assert_eq!(audio.format, "wav");
        assert_eq!(&audio.bytes[..], &wav[..]);
        assert_eq!(audio.duration, Some(1.0));

        let mp3 = export_audio(&item(MediaData::Encoded {
            format: "mp3".to_owned(),
            bytes: Arc::from(&b"ID3"[..]),
        }))
        .unwrap();
        assert_eq!((mp3.format.as_str(), mp3.duration), ("mp3", None));

        let raw = export_audio(&item(MediaData::RawAudio {
            channels: 1,
            sample_rate: 100,
            bits_per_sample: 8,
            float: false,
            samples: Arc::from(vec![128u8; 100]),
        }))
        .unwrap();
        assert_eq!(&raw.bytes[..], &wav[..]);
        assert_eq!(raw.duration, Some(1.0));

        for data in [
            MediaData::Encoded {
                format: "../x".to_owned(),
                bytes: Arc::from(&b"x"[..]),
            },
            MediaData::Encoded {
                format: "ogg".to_owned(),
                bytes: Arc::from(&b""[..]),
            },
            MediaData::RawAudio {
                channels: 1,
                sample_rate: 100,
                bits_per_sample: 12,
                float: false,
                samples: Arc::from(vec![0u8; 3]),
            },
        ] {
            assert!(export_audio(&item(data)).is_none());
        }
    }
}
