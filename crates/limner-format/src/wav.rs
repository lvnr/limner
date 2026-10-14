//! A minimal RIFF/WAVE writer and reader for uncompressed audio.

use crate::container::{u16_at, u32_at};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AudioSpec {
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
    pub float: bool,
}

const FORMAT_PCM: u16 = 1;
const FORMAT_FLOAT: u16 = 3;
const FORMAT_EXTENSIBLE: u16 = 0xFFFE;

impl AudioSpec {
    fn block_align(&self) -> u64 {
        self.channels as u64 * (self.bits_per_sample as u64 / 8)
    }

    /// Whether a standard WAVE file can carry `len` bytes of samples in this layout.
    pub fn fits_wav(&self, len: usize) -> bool {
        let bits_ok = if self.float {
            matches!(self.bits_per_sample, 32 | 64)
        } else {
            matches!(self.bits_per_sample, 8 | 16 | 24 | 32)
        };
        let align = self.block_align();
        bits_ok
            && self.channels >= 1
            && self.sample_rate >= 1
            && align <= u16::MAX as u64
            && self.sample_rate as u64 * align <= u32::MAX as u64
            && (len as u64).is_multiple_of(align)
            && len as u64 <= u32::MAX as u64 - 64
    }
}

fn chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}

/// Writes a WAVE file. The caller has checked [`AudioSpec::fits_wav`].
pub(crate) fn encode(spec: AudioSpec, samples: &[u8]) -> Vec<u8> {
    let align = spec.block_align();
    let mut fmt = Vec::with_capacity(18);
    let tag = if spec.float { FORMAT_FLOAT } else { FORMAT_PCM };
    fmt.extend_from_slice(&tag.to_le_bytes());
    fmt.extend_from_slice(&spec.channels.to_le_bytes());
    fmt.extend_from_slice(&spec.sample_rate.to_le_bytes());
    fmt.extend_from_slice(&((spec.sample_rate as u64 * align) as u32).to_le_bytes());
    fmt.extend_from_slice(&(align as u16).to_le_bytes());
    fmt.extend_from_slice(&spec.bits_per_sample.to_le_bytes());

    let mut body = Vec::with_capacity(samples.len() + 64);
    body.extend_from_slice(b"WAVE");
    if spec.float {
        // Non-PCM formats carry an (empty) extension size and a `fact` chunk.
        fmt.extend_from_slice(&0u16.to_le_bytes());
        chunk(&mut body, b"fmt ", &fmt);
        let frames = (samples.len() as u64 / align) as u32;
        chunk(&mut body, b"fact", &frames.to_le_bytes());
    } else {
        chunk(&mut body, b"fmt ", &fmt);
    }
    chunk(&mut body, b"data", samples);

    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

pub(crate) fn is_wav(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WAVE"
}

/// Reads the layout and the sample bytes of an uncompressed WAVE file.
pub(crate) fn decode(bytes: &[u8]) -> Result<(AudioSpec, &[u8]), String> {
    if !is_wav(bytes) {
        return Err("it is not a RIFF/WAVE file".to_owned());
    }
    let mut spec = None;
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32_at(bytes, at + 4) as usize;
        let start = at + 8;
        let end = start
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "a chunk runs past the end of the file".to_owned())?;
        let data = &bytes[start..end];
        match id {
            b"fmt " => {
                if data.len() < 16 {
                    return Err("its format chunk is too short".to_owned());
                }
                let mut tag = u16_at(data, 0);
                if tag == FORMAT_EXTENSIBLE && data.len() >= 26 {
                    tag = u16_at(data, 24);
                }
                if tag != FORMAT_PCM && tag != FORMAT_FLOAT {
                    return Err(format!("it uses audio encoding {tag}, not PCM or float"));
                }
                spec = Some(AudioSpec {
                    channels: u16_at(data, 2),
                    sample_rate: u32_at(data, 4),
                    bits_per_sample: u16_at(data, 14),
                    float: tag == FORMAT_FLOAT,
                });
            }
            b"data" => {
                let spec = spec.ok_or_else(|| "its data comes before its format".to_owned())?;
                return Ok((spec, data));
            }
            _ => {}
        }
        at = end + size % 2;
    }
    Err("it has no complete data chunk".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_and_float_files_round_trip() {
        let pcm = AudioSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            float: false,
        };
        let samples: Vec<u8> = (0..40u8).collect();
        assert!(pcm.fits_wav(samples.len()));
        let file = encode(pcm, &samples);
        assert_eq!(file.len(), 44 + 40);
        assert_eq!(&file[..4], b"RIFF");
        assert_eq!(u32_at(&file, 4) as usize, file.len() - 8);
        assert_eq!(u16_at(&file, 20), 1);
        assert_eq!(u32_at(&file, 28), 192_000);
        assert_eq!(u16_at(&file, 32), 4);
        assert_eq!(decode(&file).unwrap(), (pcm, samples.as_slice()));

        let float = AudioSpec {
            channels: 1,
            sample_rate: 44_100,
            bits_per_sample: 32,
            float: true,
        };
        let file = encode(float, &samples);
        assert_eq!(u16_at(&file, 20), 3);
        assert_eq!(decode(&file).unwrap(), (float, samples.as_slice()));

        // An odd number of 8-bit samples is padded, and the padding is not read back.
        let mono8 = AudioSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 8,
            float: false,
        };
        let file = encode(mono8, &samples[..5]);
        assert_eq!(file.len() % 2, 0);
        assert_eq!(decode(&file).unwrap(), (mono8, &samples[..5]));
    }

    #[test]
    fn layouts_a_wave_file_cannot_carry_are_recognised() {
        let spec = |channels, bits_per_sample, float| AudioSpec {
            channels,
            sample_rate: 48_000,
            bits_per_sample,
            float,
        };
        assert!(!spec(0, 16, false).fits_wav(0));
        assert!(!spec(2, 12, false).fits_wav(0));
        assert!(!spec(2, 16, true).fits_wav(0));
        assert!(!spec(2, 16, false).fits_wav(3));
        assert!(spec(2, 64, true).fits_wav(32));
    }

    #[test]
    fn truncated_files_are_rejected() {
        let spec = AudioSpec {
            channels: 1,
            sample_rate: 8_000,
            bits_per_sample: 16,
            float: false,
        };
        let file = encode(spec, &[0u8; 64]);
        assert!(decode(&file[..file.len() - 1]).is_err());
        assert!(decode(&file[..30]).is_err());
        assert!(decode(b"RIFF").is_err());
    }
}
