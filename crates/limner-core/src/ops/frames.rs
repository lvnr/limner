//! Frame-by-frame animation: the frames of a paint layer.
//!
//! A paint layer holds drawings and a frame table that says which drawing each frame
//! shows. A hold is a repeated entry, so "key frames" and "hold frames" are a way of
//! reading the table, not something stored.

use super::OpError;
use crate::model::{Document, Drawing, LayerId, Paint};

/// The most frames a layer may have.
pub const MAX_FRAMES: usize = 100_000;

/// What a new frame starts out as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NewFrame {
    /// An empty drawing.
    Blank,
    /// A copy of the frame it is added after.
    Duplicate,
}

fn paint(doc: &Document, id: LayerId) -> Result<&Paint, OpError> {
    doc.layer(id)
        .ok_or(OpError::NoSuchLayer(id))?
        .as_paint()
        .ok_or(OpError::NotPaint(id))
}

fn paint_mut(doc: &mut Document, id: LayerId) -> Result<&mut Paint, OpError> {
    doc.layer_mut(id)
        .ok_or(OpError::NoSuchLayer(id))?
        .as_paint_mut()
        .ok_or(OpError::NotPaint(id))
}

fn has_room(paint: &Paint, extra: usize) -> Result<(), OpError> {
    if paint.frames.len() + extra > MAX_FRAMES {
        return Err(OpError::Invalid(format!(
            "a layer can have at most {MAX_FRAMES} frames"
        )));
    }
    Ok(())
}

/// Drops drawings no frame shows any more and renumbers the frame table.
fn compact(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    let paint = paint_mut(doc, id)?;
    let mut new_index = vec![u32::MAX; paint.drawings.len()];
    let mut kept = Vec::new();
    for frame in &mut paint.frames {
        let old = *frame as usize;
        if new_index[old] == u32::MAX {
            new_index[old] = kept.len() as u32;
            kept.push(paint.drawings[old]);
        }
        *frame = new_index[old];
    }
    if kept.len() != paint.drawings.len() || kept != paint.drawings {
        paint.drawings = kept;
        doc.collect_garbage();
    }
    Ok(())
}

/// The frames at which the drawing changes: frame 0 and every frame that shows a
/// different drawing from the one before it.
pub fn key_frames(paint: &Paint) -> Vec<usize> {
    (0..paint.frames.len())
        .filter(|i| *i == 0 || paint.frames[*i] != paint.frames[*i - 1])
        .collect()
}

/// The run of frames around `frame` that show the same drawing: a key frame and its holds.
pub fn hold_run(paint: &Paint, frame: usize) -> std::ops::Range<usize> {
    let frame = frame.min(paint.frames.len() - 1);
    let drawing = paint.frames[frame];
    let start = (0..=frame)
        .rev()
        .take_while(|i| paint.frames[*i] == drawing)
        .last()
        .unwrap_or(frame);
    let end = (frame..paint.frames.len())
        .take_while(|i| paint.frames[*i] == drawing)
        .last()
        .unwrap_or(frame)
        + 1;
    start..end
}

/// Adds a frame after `frame`. Returns the index of the new frame.
pub fn add_frame(
    doc: &mut Document,
    id: LayerId,
    frame: usize,
    kind: NewFrame,
) -> Result<usize, OpError> {
    let current = paint(doc, id)?;
    has_room(current, 1)?;
    let frame = frame.min(current.frames.len() - 1);
    let shown = current
        .drawing_at_frame(frame)
        .ok_or_else(|| OpError::Invalid("the layer's frame table is broken".into()))?;
    let drawing = match kind {
        NewFrame::Blank => doc.add_drawing(Drawing::new()),
        // The copy shares its strokes with the original until one of them is edited.
        NewFrame::Duplicate => {
            doc.add_shared_drawing(doc.drawing(shown).cloned().unwrap_or_default())
        }
    };
    let paint = paint_mut(doc, id)?;
    paint.drawings.push(drawing);
    paint
        .frames
        .insert(frame + 1, (paint.drawings.len() - 1) as u32);
    Ok(frame + 1)
}

/// Makes the drawing at `frame` last one frame longer, by adding a hold before or
/// after it. Returns the index of the added frame.
pub fn add_hold_frame(
    doc: &mut Document,
    id: LayerId,
    frame: usize,
    after: bool,
) -> Result<usize, OpError> {
    let paint = paint_mut(doc, id)?;
    has_room(paint, 1)?;
    let frame = frame.min(paint.frames.len() - 1);
    let at = if after { frame + 1 } else { frame };
    paint.frames.insert(at, paint.frames[frame]);
    Ok(at)
}

/// Removes a frame, making the animation one frame shorter.
pub fn delete_frame(doc: &mut Document, id: LayerId, frame: usize) -> Result<(), OpError> {
    let paint = paint_mut(doc, id)?;
    if paint.frames.len() <= 1 {
        return Err(OpError::Invalid("a layer needs at least one frame".into()));
    }
    let frame = frame.min(paint.frames.len() - 1);
    paint.frames.remove(frame);
    compact(doc, id)
}

/// Turns the key frame at `frame` (and its holds) into holds of the drawing before
/// it. The animation keeps its length.
pub fn remove_key_frame(doc: &mut Document, id: LayerId, frame: usize) -> Result<(), OpError> {
    let paint = paint_mut(doc, id)?;
    let run = hold_run(paint, frame);
    if run.start == 0 {
        return Err(OpError::Invalid(
            "the first key frame has nothing before it to hold".into(),
        ));
    }
    let before = paint.frames[run.start - 1];
    paint.frames[run].fill(before);
    compact(doc, id)
}

/// Removes all animation, leaving the first frame.
pub fn clear_frames(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    paint_mut(doc, id)?.frames.truncate(1);
    compact(doc, id)
}

/// Shows every frame twice as long.
pub fn double_frames(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    let paint = paint_mut(doc, id)?;
    has_room(paint, paint.frames.len())?;
    paint.frames = paint.frames.iter().flat_map(|f| [*f, *f]).collect();
    Ok(())
}

/// Plays the frames in the opposite order.
pub fn reverse_frames(doc: &mut Document, id: LayerId) -> Result<(), OpError> {
    paint_mut(doc, id)?.frames.reverse();
    compact(doc, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Brush, Point, Stroke};
    use std::sync::Arc;

    fn stroke(x: f32) -> Stroke {
        let p = |y: f32| Point {
            pos: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            dir: [0.0, 0.0, -1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.01,
        };
        Stroke::new(0, Brush::Ribbon, false, vec![p(0.0), p(1.0)])
    }

    /// A layer whose frames show drawings marked by the x of their one stroke: the table given.
    fn layer(table: &[u32]) -> (Document, LayerId) {
        let mut doc = Document::new();
        let id = doc.find_by_path("Root/Paint").unwrap();
        let count = *table.iter().max().unwrap() as usize + 1;
        let mut drawings = vec![doc.layer(id).unwrap().as_paint().unwrap().drawings[0]];
        for _ in 1..count {
            drawings.push(doc.add_drawing(Drawing::new()));
        }
        for (i, drawing) in drawings.iter().enumerate() {
            doc.drawing_mut(*drawing)
                .unwrap()
                .push_stroke(stroke(i as f32));
        }
        let paint = doc.layer_mut(id).unwrap().as_paint_mut().unwrap();
        paint.drawings = drawings;
        paint.frames = table.to_vec();
        doc.validate().unwrap();
        (doc, id)
    }

    /// What each frame shows, by the mark of its drawing; -1 for an empty drawing.
    fn shown(doc: &Document, id: LayerId) -> Vec<i32> {
        let paint = doc.layer(id).unwrap().as_paint().unwrap();
        (0..paint.frames.len())
            .map(|f| {
                doc.drawing(paint.drawing_at_frame(f).unwrap())
                    .unwrap()
                    .strokes()
                    .next()
                    .map_or(-1, |s| s.points()[0].pos[0] as i32)
            })
            .collect()
    }

    #[test]
    fn new_frames_go_after_the_current_one_blank_or_copied() {
        let (mut doc, id) = layer(&[0, 1]);
        assert_eq!(add_frame(&mut doc, id, 0, NewFrame::Blank).unwrap(), 1);
        assert_eq!(shown(&doc, id), [0, -1, 1]);
        assert_eq!(add_frame(&mut doc, id, 2, NewFrame::Duplicate).unwrap(), 3);
        assert_eq!(shown(&doc, id), [0, -1, 1, 1]);
        let paint = doc.layer(id).unwrap().as_paint().unwrap().clone();
        assert_ne!(
            paint.frames[2], paint.frames[3],
            "a copy is a frame of its own"
        );
        assert!(
            Arc::ptr_eq(
                doc.drawing(paint.drawing_at_frame(2).unwrap()).unwrap(),
                doc.drawing(paint.drawing_at_frame(3).unwrap()).unwrap()
            ),
            "that shares its strokes until edited"
        );
        assert_eq!(
            add_frame(&mut doc, id, 99, NewFrame::Blank).unwrap(),
            4,
            "past the end means after the last frame"
        );
        doc.validate().unwrap();
        assert_eq!(
            key_frames(doc.layer(id).unwrap().as_paint().unwrap()),
            [0, 1, 2, 3, 4]
        );
    }

    #[test]
    fn holds_lengthen_a_drawing_and_deleting_shortens_the_animation() {
        let (mut doc, id) = layer(&[0, 1, 2]);
        assert_eq!(add_hold_frame(&mut doc, id, 1, true).unwrap(), 2);
        assert_eq!(add_hold_frame(&mut doc, id, 0, false).unwrap(), 0);
        assert_eq!(shown(&doc, id), [0, 0, 1, 1, 2]);
        let paint = doc.layer(id).unwrap().as_paint().unwrap().clone();
        assert_eq!(key_frames(&paint), [0, 2, 4]);
        assert_eq!(hold_run(&paint, 3), 2..4);
        assert_eq!(doc.drawing_count(), 3, "holds add no drawings");

        delete_frame(&mut doc, id, 4).unwrap();
        assert_eq!(shown(&doc, id), [0, 0, 1, 1]);
        assert_eq!(
            doc.drawing_count(),
            2,
            "a drawing no frame shows is dropped"
        );
        assert_eq!(doc.layer(id).unwrap().as_paint().unwrap().drawings.len(), 2);
        doc.validate().unwrap();
        for _ in 0..3 {
            delete_frame(&mut doc, id, 0).unwrap();
        }
        assert_eq!(shown(&doc, id), [1]);
        assert!(
            delete_frame(&mut doc, id, 0).is_err(),
            "the last frame stays"
        );
    }

    #[test]
    fn removing_a_key_frame_holds_the_drawing_before_it() {
        let (mut doc, id) = layer(&[0, 1, 1, 2]);
        remove_key_frame(&mut doc, id, 2).unwrap();
        assert_eq!(
            shown(&doc, id),
            [0, 0, 0, 2],
            "same length, one drawing fewer"
        );
        assert_eq!(doc.drawing_count(), 2);
        assert!(
            remove_key_frame(&mut doc, id, 1).is_err(),
            "the first key frame cannot be removed"
        );
        doc.validate().unwrap();
    }

    #[test]
    fn frames_can_be_doubled_reversed_and_cleared() {
        let (mut doc, id) = layer(&[0, 1, 2]);
        double_frames(&mut doc, id).unwrap();
        assert_eq!(shown(&doc, id), [0, 0, 1, 1, 2, 2]);
        reverse_frames(&mut doc, id).unwrap();
        assert_eq!(shown(&doc, id), [2, 2, 1, 1, 0, 0]);
        doc.validate().unwrap();
        clear_frames(&mut doc, id).unwrap();
        assert_eq!(shown(&doc, id), [2]);
        assert_eq!(doc.drawing_count(), 1);
        doc.validate().unwrap();
    }

    #[test]
    fn only_paint_layers_have_frames() {
        let mut doc = Document::new();
        let root = doc.root();
        assert_eq!(double_frames(&mut doc, root), Err(OpError::NotPaint(root)));
    }
}
