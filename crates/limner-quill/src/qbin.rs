//! Reader for `Quill.qbin`, the little-endian blob that holds strokes and media.
//!
//! Drawings and pictures are read here; model and sound records live in
//! [`crate::mesh`] and [`crate::sound`].

use bytemuck::Zeroable;
use limner_core::model::{Brush, Drawing, MediaData, Point, Stroke};
use std::sync::Arc;

/// Why a media record could not be read. The text ends up in a warning.
pub(crate) type Unreadable = String;

pub(crate) fn past_the_end() -> Unreadable {
    "the record runs past the end of Quill.qbin".to_owned()
}

/// A forward-only reader over the file. Every read is bounds-checked.
pub(crate) struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn remaining(&self) -> usize {
        self.bytes.len() - self.pos
    }

    pub(crate) fn peek(&self, len: usize) -> Option<&'a [u8]> {
        self.bytes.get(self.pos..self.pos.checked_add(len)?)
    }

    pub(crate) fn take(&mut self, len: usize) -> Result<&'a [u8], Unreadable> {
        let data = self.peek(len).ok_or_else(past_the_end)?;
        self.pos += len;
        Ok(data)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], Unreadable> {
        Ok(self.take(N)?.try_into().expect("take returned N bytes"))
    }

    pub(crate) fn u8(&mut self) -> Result<u8, Unreadable> {
        Ok(self.array::<1>()?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, Unreadable> {
        self.array().map(u16::from_le_bytes)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, Unreadable> {
        self.array().map(u32::from_le_bytes)
    }

    pub(crate) fn u64(&mut self) -> Result<u64, Unreadable> {
        self.array().map(u64::from_le_bytes)
    }
}

/// What was unusual about a picture record that could still be read.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct PictureNotes {
    /// The image type byte when it is not 2 (a flat image).
    pub unexpected_type: Option<u8>,
}

pub(crate) const STROKE_HEADER_LEN: usize = 40;
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

    /// A reader positioned at `offset`, or `None` when that is beyond the file.
    pub(crate) fn cursor(&self, offset: u64) -> Option<Cursor<'a>> {
        let pos = usize::try_from(offset).ok()?;
        (pos <= self.bytes.len()).then_some(Cursor {
            bytes: self.bytes,
            pos,
        })
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

    /// Reads a picture record: a 20-byte header followed by raw 8-bit pixels, rows top
    /// to bottom.
    ///
    /// Header: `00 00 00 01`, an image type byte (2 = flat), a pixel format byte
    /// (Quill's image enum: 2 grey, 6 RGB, 7 RGBA), `00 00`, then width, height and
    /// depth as `u32`. Only the RGB form has been seen in files; the others follow the
    /// enum in the IMM sources.
    pub(crate) fn read_picture(
        &self,
        offset: u64,
    ) -> Result<(MediaData, PictureNotes), Unreadable> {
        let mut c = self.cursor(offset).ok_or_else(past_the_end)?;
        let header = c.take(8)?;
        let image_type = header[4];
        let stored_channels: usize = match header[5] {
            2 => 1,
            6 => 3,
            7 => 4,
            other => return Err(format!("pixel format {other} is not supported")),
        };
        let width = c.u32()?;
        let height = c.u32()?;
        let depth = c.u32()?;
        if width == 0 || height == 0 {
            return Err("the picture has no pixels".to_owned());
        }
        if depth > 1 {
            return Err(format!("the picture is a volume of {depth} slices"));
        }
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(stored_channels))
            .ok_or_else(past_the_end)?;
        let stored = c.take(len)?;
        let (channels, pixels): (u8, Arc<[u8]>) = if stored_channels == 1 {
            (3, stored.iter().flat_map(|v| [*v; 3]).collect())
        } else {
            (stored_channels as u8, Arc::from(stored))
        };
        let notes = PictureNotes {
            unexpected_type: (image_type != 2).then_some(image_type),
        };
        Ok((
            MediaData::RawImage {
                width,
                height,
                channels,
                pixels,
            },
            notes,
        ))
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

    fn picture_bytes(format: u8, width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0u8; 8];
        bytes.extend_from_slice(&[0, 0, 0, 1, 2, format, 0, 0]);
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(pixels);
        bytes
    }

    fn image(data: MediaData) -> (u32, u32, u8, Vec<u8>) {
        match data {
            MediaData::RawImage {
                width,
                height,
                channels,
                pixels,
            } => (width, height, channels, pixels.to_vec()),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn reads_rgb_pictures() {
        let bytes = picture_bytes(6, 2, 1, &[10, 20, 30, 40, 50, 60]);
        let (data, notes) = Qbin::new(&bytes).read_picture(8).unwrap();
        assert_eq!(notes, PictureNotes::default());
        assert_eq!(image(data), (2, 1, 3, vec![10, 20, 30, 40, 50, 60]));
        assert!(Qbin::new(&bytes[..bytes.len() - 1])
            .read_picture(8)
            .is_err());
    }

    #[test]
    fn reads_rgba_and_grey_pictures() {
        let rgba = picture_bytes(7, 1, 2, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let (data, _) = Qbin::new(&rgba).read_picture(8).unwrap();
        assert_eq!(image(data), (1, 2, 4, vec![1, 2, 3, 4, 5, 6, 7, 8]));

        let grey = picture_bytes(2, 2, 1, &[9, 200]);
        let (data, _) = Qbin::new(&grey).read_picture(8).unwrap();
        assert_eq!(image(data), (2, 1, 3, vec![9, 9, 9, 200, 200, 200]));
    }

    #[test]
    fn rejects_pictures_it_cannot_describe() {
        let float_rgb = picture_bytes(10, 1, 1, &[0; 12]);
        assert!(Qbin::new(&float_rgb)
            .read_picture(8)
            .unwrap_err()
            .contains("pixel format 10"));
        let empty = picture_bytes(6, 0, 4, &[]);
        assert!(Qbin::new(&empty).read_picture(8).is_err());
        let huge = picture_bytes(6, u32::MAX, u32::MAX, &[]);
        assert!(Qbin::new(&huge).read_picture(8).is_err());
        assert!(Qbin::new(&huge).read_picture(u64::MAX).is_err());

        let mut cube = picture_bytes(6, 1, 1, &[1, 2, 3]);
        cube[12] = 4;
        let (_, notes) = Qbin::new(&cube).read_picture(8).unwrap();
        assert_eq!(notes.unexpected_type, Some(4));
    }
}
