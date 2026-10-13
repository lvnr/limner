//! Reader for `Quill.qbin`, the little-endian blob that holds strokes and media.

use bytemuck::Zeroable;
use limner_core::model::{Brush, Drawing, MediaData, Point, Stroke};
use std::sync::Arc;

const STROKE_HEADER_LEN: usize = 40;
const POINT_LEN: usize = std::mem::size_of::<Point>();

/// What went wrong, or was repaired, while reading one drawing.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DrawingReport {
    /// Strokes dropped because they had no points or non-finite data.
    pub dropped_strokes: usize,
    /// Stroke ids were not unique and were renumbered in order.
    pub renumbered: bool,
    /// The record ran past the end of the file and was cut short.
    pub truncated: bool,
    pub unknown_brushes: usize,
}

impl DrawingReport {
    pub fn is_clean(&self) -> bool {
        *self == Self::default()
    }
}

pub(crate) struct Qbin<'a> {
    bytes: &'a [u8],
}

impl<'a> Qbin<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn slice(&self, offset: usize, len: usize) -> Option<&'a [u8]> {
        self.bytes.get(offset..offset.checked_add(len)?)
    }

    fn u32_at(&self, offset: usize) -> Option<u32> {
        self.slice(offset, 4)
            .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")))
    }

    fn i32_at(&self, offset: usize) -> Option<i32> {
        self.u32_at(offset).map(|v| v as i32)
    }

    fn i16_at(&self, offset: usize) -> Option<i16> {
        self.slice(offset, 2)
            .map(|b| i16::from_le_bytes(b.try_into().expect("two bytes")))
    }

    /// Reads the drawing record at `offset`: a stroke count followed by strokes.
    /// Returns `None` when not even the count can be read.
    pub(crate) fn read_drawing(&self, offset: u64) -> Option<(Drawing, DrawingReport)> {
        let mut cursor = usize::try_from(offset).ok()?;
        let stroke_count = self.u32_at(cursor)? as usize;
        cursor += 4;

        let mut report = DrawingReport::default();
        let mut strokes: Vec<Stroke> = Vec::with_capacity(stroke_count.min(1 << 20));
        for _ in 0..stroke_count {
            if self.slice(cursor, STROKE_HEADER_LEN).is_none() {
                report.truncated = true;
                break;
            }
            let id = self.u32_at(cursor).expect("header is in range");
            let brush_code = self.i16_at(cursor + 0x20).expect("header is in range");
            let visibility = self.bytes[cursor + 0x22];
            let point_count = self.i32_at(cursor + 0x24).expect("header is in range");
            cursor += STROKE_HEADER_LEN;

            let Ok(point_count) = usize::try_from(point_count) else {
                report.truncated = true;
                break;
            };
            let Some(data) = point_count
                .checked_mul(POINT_LEN)
                .and_then(|len| self.slice(cursor, len))
            else {
                report.truncated = true;
                break;
            };
            cursor += data.len();

            let mut points = vec![Point::zeroed(); point_count];
            bytemuck::cast_slice_mut::<Point, u8>(&mut points).copy_from_slice(data);

            let finite = points.iter().all(|p| {
                p.pos
                    .iter()
                    .chain(&p.normal)
                    .chain(&p.dir)
                    .chain(&p.color)
                    .all(|v| v.is_finite())
                    && p.opacity.is_finite()
                    && p.width.is_finite()
            });
            if points.is_empty() || !finite {
                report.dropped_strokes += 1;
                continue;
            }
            let brush = Brush::from_code(brush_code as i32).unwrap_or_else(|| {
                report.unknown_brushes += 1;
                Brush::Cylinder
            });
            // Byte 0x22: 0 means the stroke fades with viewing direction, 1 means always visible.
            strokes.push(Stroke::new(id, brush, visibility == 0, points));
        }

        let mut seen = std::collections::HashSet::with_capacity(strokes.len());
        if !strokes.iter().all(|s| seen.insert(s.id)) {
            report.renumbered = true;
            for (index, stroke) in strokes.iter_mut().enumerate() {
                stroke.id = index as u32;
            }
        }
        Some((Drawing::from_strokes(strokes), report))
    }

    /// Reads a picture record: a 20-byte header followed by raw 8-bit pixels.
    pub(crate) fn read_picture(&self, offset: u64) -> Option<MediaData> {
        let start = usize::try_from(offset).ok()?;
        let header = self.slice(start, 20)?;
        // Byte 5 is the pixel format: 6 for RGB, 7 for RGBA.
        let channels: u8 = match header[5] {
            6 => 3,
            7 => 4,
            _ => return None,
        };
        let width = self.u32_at(start + 8)?;
        let height = self.u32_at(start + 12)?;
        let len = (width as usize)
            .checked_mul(height as usize)?
            .checked_mul(channels as usize)?;
        if width == 0 || height == 0 {
            return None;
        }
        let pixels = self.slice(start + 20, len)?;
        Some(MediaData::RawImage {
            width,
            height,
            channels,
            pixels: Arc::from(pixels),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f32) -> Point {
        Point {
            pos: [x, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, 1.0],
            color: [0.1, 0.2, 0.3],
            opacity: 1.0,
            width: 0.01,
        }
    }

    fn stroke_bytes(id: u32, brush: i16, visibility: u8, points: &[Point]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&0i32.to_le_bytes());
        out.extend_from_slice(&[0u8; 24]);
        out.extend_from_slice(&brush.to_le_bytes());
        out.push(visibility);
        out.push(0);
        out.extend_from_slice(&(points.len() as i32).to_le_bytes());
        out.extend_from_slice(bytemuck::cast_slice(points));
        out
    }

    fn drawing_bytes(strokes: &[Vec<u8>]) -> Vec<u8> {
        let mut out = vec![0u8; 8];
        out.extend_from_slice(&(strokes.len() as u32).to_le_bytes());
        for s in strokes {
            out.extend_from_slice(s);
        }
        out
    }

    #[test]
    fn reads_strokes_exactly() {
        let pts = [point(0.0), point(1.0), point(2.0)];
        let bytes = drawing_bytes(&[
            stroke_bytes(0, 2, 1, &pts),
            stroke_bytes(1, 1, 0, &pts[..2]),
        ]);
        let (drawing, report) = Qbin::new(&bytes).read_drawing(8).unwrap();
        assert!(report.is_clean());
        let strokes: Vec<_> = drawing.strokes().collect();
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes[0].brush, Brush::Cylinder);
        assert!(!strokes[0].directional);
        assert_eq!(strokes[0].points(), &pts);
        assert_eq!(strokes[1].brush, Brush::Ribbon);
        assert!(strokes[1].directional);
        assert_eq!(strokes[1].len(), 2);
    }

    #[test]
    fn empty_drawing() {
        let bytes = drawing_bytes(&[]);
        let (drawing, report) = Qbin::new(&bytes).read_drawing(8).unwrap();
        assert!(drawing.is_empty());
        assert!(report.is_clean());
    }

    #[test]
    fn damaged_data_is_reported_not_fatal() {
        let pts = [point(0.0), point(1.0)];
        let mut bad = point(0.0);
        bad.pos[1] = f32::NAN;
        let mut bytes = drawing_bytes(&[
            stroke_bytes(5, 2, 1, &pts),
            stroke_bytes(5, 9, 1, &pts),
            stroke_bytes(7, 2, 1, &[bad]),
            stroke_bytes(8, 2, 1, &[]),
        ]);
        // Claim one more stroke than is present.
        bytes[8] = 5;
        let (drawing, report) = Qbin::new(&bytes).read_drawing(8).unwrap();
        assert_eq!(drawing.stroke_count(), 2);
        assert_eq!(report.dropped_strokes, 2);
        assert_eq!(report.unknown_brushes, 1);
        assert!(report.renumbered);
        assert!(report.truncated);
        assert_eq!(
            drawing.strokes().map(|s| s.id).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert!(Qbin::new(&bytes).read_drawing(bytes.len() as u64).is_none());
    }

    #[test]
    fn reads_rgb_pictures() {
        let mut bytes = vec![0u8; 8];
        bytes.extend_from_slice(&[0, 0, 0, 1, 2, 6, 0, 0]);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&[10, 20, 30, 40, 50, 60]);
        match Qbin::new(&bytes).read_picture(8).unwrap() {
            MediaData::RawImage {
                width,
                height,
                channels,
                pixels,
            } => {
                assert_eq!((width, height, channels), (2, 1, 3));
                assert_eq!(&pixels[..], &[10, 20, 30, 40, 50, 60]);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(Qbin::new(&bytes[..bytes.len() - 1])
            .read_picture(8)
            .is_none());
    }
}
