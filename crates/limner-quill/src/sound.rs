//! Sound records.
//!
//! No public Quill project with a sound layer could be found, so the layout is not
//! assumed. Three candidates are told apart by their first bytes:
//!
//! 1. Quill's wave serializer (`piWav` in the IMM sources), also what quill-blender reads:
//!    `u32 version = 0`, `u16 0`, `u8 channels`, `u8 bits per sample`, `u32 sample rate`,
//!    `u64 frame count`, then interleaved PCM. 32-bit samples are floats, as in Quill's
//!    own encoders.
//! 2. The reading in the Icosa fork of SharpQuill: `i32 length`, then a whole audio
//!    file (WAV, OGG or MP3).
//! 3. A WAV file with no prefix, which is how Quill's audio page describes the data.
//!
//! The first starts with four zero bytes, the second with a positive length followed by
//! a file signature, and the third with `RIFF`, so no record can match two of them.

use crate::qbin::{past_the_end, Qbin, Unreadable};
use limner_core::model::MediaData;
use std::sync::Arc;

/// How the audio of a sound layer was found to be stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SoundLayout {
    /// Quill's wave serializer: a 20-byte header and raw PCM.
    Wave,
    /// A length followed by an audio file.
    LengthPrefixedFile,
    /// An audio file with nothing in front of it.
    BareFile,
}

/// Names the audio file format that starts with `bytes`.
fn sniff(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("wav")
    } else if bytes.starts_with(b"OggS") {
        Some("ogg")
    } else if bytes.starts_with(b"ID3")
        || (bytes.len() >= 2 && bytes[0] == 0xff && bytes[1] & 0xe0 == 0xe0)
    {
        Some("mp3")
    } else {
        None
    }
}

impl Qbin<'_> {
    pub(crate) fn read_sound(&self, offset: u64) -> Result<(MediaData, SoundLayout), Unreadable> {
        let mut c = self.cursor(offset).ok_or_else(past_the_end)?;
        let encoded = |format: &str, bytes: &[u8]| MediaData::Encoded {
            format: format.to_owned(),
            bytes: Arc::from(bytes),
        };

        if c.peek(12).and_then(sniff) == Some("wav") {
            // The RIFF size field counts everything after itself.
            let riff = c.peek(8).expect("twelve bytes were just read");
            let declared = u32::from_le_bytes(riff[4..8].try_into().expect("four bytes")) as usize;
            let len = declared.saturating_add(8).min(c.remaining());
            return Ok((encoded("wav", c.take(len)?), SoundLayout::BareFile));
        }

        let first = c.u32()?;
        if first == 0 {
            let _reserved = c.u16()?;
            let channels = c.u8()?;
            let bits = c.u8()?;
            let sample_rate = c.u32()?;
            let frames = c.u64()?;
            if channels == 0 || sample_rate == 0 || !matches!(bits, 8 | 16 | 24 | 32) {
                return Err(format!(
                    "the header describes {channels} channel(s) of {bits}-bit audio at {sample_rate} Hz"
                ));
            }
            let len = usize::try_from(frames)
                .ok()
                .and_then(|n| n.checked_mul(channels as usize * (bits as usize / 8)))
                .ok_or_else(past_the_end)?;
            let samples = c.take(len)?;
            return Ok((
                MediaData::RawAudio {
                    channels: channels.into(),
                    sample_rate,
                    bits_per_sample: bits.into(),
                    float: bits == 32,
                    samples: Arc::from(samples),
                },
                SoundLayout::Wave,
            ));
        }

        let len = usize::try_from(first)
            .ok()
            .filter(|len| first <= i32::MAX as u32 && *len <= c.remaining());
        let format = c.peek(12.min(c.remaining())).and_then(sniff);
        match (len, format) {
            (Some(len), Some(format)) => Ok((
                encoded(format, c.take(len)?),
                SoundLayout::LengthPrefixedFile,
            )),
            _ => Err("the record matches no known sound layout".to_owned()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{wav_file, QbinBuilder};

    fn offset(hex: &str) -> u64 {
        u64::from_str_radix(hex, 16).unwrap()
    }

    #[test]
    fn reads_the_wave_serializer_layout() {
        let pcm: Vec<u8> = (0..24).collect();
        let mut qbin = QbinBuilder::default();
        let at = offset(&qbin.sound_wave(2, 16, 44_100, &pcm));
        qbin.bytes.extend_from_slice(&[0xaa; 7]);
        let (data, layout) = Qbin::new(&qbin.bytes).read_sound(at).unwrap();
        assert_eq!(layout, SoundLayout::Wave);
        assert_eq!(
            data,
            MediaData::RawAudio {
                channels: 2,
                sample_rate: 44_100,
                bits_per_sample: 16,
                float: false,
                samples: Arc::from(pcm.as_slice()),
            }
        );
    }

    #[test]
    fn thirty_two_bit_samples_are_floats() {
        let pcm: Vec<u8> = [0.5f32, -0.25]
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect();
        let mut qbin = QbinBuilder::default();
        let at = offset(&qbin.sound_wave(1, 32, 48_000, &pcm));
        let (data, _) = Qbin::new(&qbin.bytes).read_sound(at).unwrap();
        assert!(matches!(
            data,
            MediaData::RawAudio {
                float: true,
                bits_per_sample: 32,
                channels: 1,
                ..
            }
        ));
    }

    #[test]
    fn reads_a_length_prefixed_file() {
        let wav = wav_file(1, 16, 22_050, &[1, 2, 3, 4]);
        for (file, format) in [
            (wav.clone(), "wav"),
            (b"OggS\0\x02rest of an ogg stream".to_vec(), "ogg"),
            (b"ID3\x04tagged mp3".to_vec(), "mp3"),
            (vec![0xff, 0xfb, 0x90, 0x00, 1, 2, 3], "mp3"),
        ] {
            let mut qbin = QbinBuilder::default();
            let at = offset(&qbin.sound_file(&file));
            qbin.bytes.extend_from_slice(b"following record");
            let (data, layout) = Qbin::new(&qbin.bytes).read_sound(at).unwrap();
            assert_eq!(layout, SoundLayout::LengthPrefixedFile);
            assert_eq!(
                data,
                MediaData::Encoded {
                    format: format.to_owned(),
                    bytes: Arc::from(file.as_slice()),
                }
            );
        }
    }

    #[test]
    fn reads_a_bare_wav_file() {
        let wav = wav_file(2, 8, 8_000, &[9, 8, 7, 6]);
        let mut bytes = vec![0u8; 8];
        bytes.extend_from_slice(&wav);
        bytes.extend_from_slice(b"following record");
        let (data, layout) = Qbin::new(&bytes).read_sound(8).unwrap();
        assert_eq!(layout, SoundLayout::BareFile);
        assert_eq!(
            data,
            MediaData::Encoded {
                format: "wav".to_owned(),
                bytes: Arc::from(wav.as_slice()),
            }
        );
    }

    #[test]
    fn rejects_what_matches_no_layout() {
        // A drawing record is not a sound: a stroke count, then stroke bytes.
        let mut qbin = QbinBuilder::default();
        let drawing = offset(&qbin.drawing(&[crate::testing::StrokeSpec::new(
            0,
            2,
            crate::testing::tube(0.0, 4),
        )]));
        assert!(Qbin::new(&qbin.bytes).read_sound(drawing).is_err());

        let mut qbin = QbinBuilder::default();
        let wave = offset(&qbin.sound_wave(2, 16, 44_100, &[0; 8]));
        let full = qbin.bytes.clone();
        for len in 0..full.len() {
            assert!(Qbin::new(&full[..len]).read_sound(wave).is_err());
        }
        // Zero channels, an odd sample size, a frame count far past the file.
        for (at, value) in [(6usize, 0u8), (7, 12), (19, 0x7f)] {
            let mut bytes = full.clone();
            bytes[wave as usize + at] = value;
            assert!(Qbin::new(&bytes).read_sound(wave).is_err());
        }

        let mut qbin = QbinBuilder::default();
        let file = offset(&qbin.sound_file(b"OggS and so on"));
        let cut = &qbin.bytes[..qbin.bytes.len() - 1];
        assert!(Qbin::new(cut).read_sound(file).is_err());
        assert!(Qbin::new(&qbin.bytes).read_sound(u64::MAX).is_err());
    }
}
