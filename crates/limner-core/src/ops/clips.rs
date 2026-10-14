//! Clips: the spans of time in which a layer is on the timeline.
//!
//! In the document a clip is a pair of visibility keys, plus an offset key when its
//! start has been trimmed. Editing those keys directly is fiddly, so every operation
//! here reads the keys as a list of clips, changes the list, and writes it back.

use super::OpError;
use crate::model::{Document, Key, LayerAnim, LayerId, Track};
use crate::time::Flicks;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Clip {
    pub start: Flicks,
    /// `None` for a clip that never ends.
    pub end: Option<Flicks>,
    /// How far into the layer's content the clip begins: the time its start was trimmed by.
    pub trim: Flicks,
}

impl Clip {
    pub fn contains(&self, time: Flicks) -> bool {
        time >= self.start && self.end.is_none_or(|end| time < end)
    }
}

/// The clips of a layer, in order. A layer without visibility keys is one clip that
/// starts at zero and never ends.
pub fn clips_of(anim: &LayerAnim) -> Vec<Clip> {
    let trim_at = |time: Flicks| anim.offset.key_at_or_before(time).map_or(0, |k| k.value);
    if anim.visibility.is_empty() {
        return vec![Clip {
            start: 0,
            end: None,
            trim: trim_at(0),
        }];
    }
    let mut clips: Vec<Clip> = Vec::new();
    let mut open = false;
    for key in anim.visibility.keys() {
        if open {
            if let Some(last) = clips.last_mut() {
                last.end = Some(key.time);
            }
        }
        open = key.value;
        if key.value {
            clips.push(Clip {
                start: key.time,
                end: None,
                trim: trim_at(key.time),
            });
        }
    }
    clips
}

/// Writes `clips` as the layer's visibility and offset keys. Clips are put in order,
/// empty ones are dropped, and one that runs into the next is cut short there.
pub fn set_clips(anim: &mut LayerAnim, clips: &[Clip]) {
    let mut clips: Vec<Clip> = clips
        .iter()
        .copied()
        .filter(|c| c.end.is_none_or(|end| end > c.start))
        .collect();
    clips.sort_by_key(|c| c.start);
    clips.dedup_by_key(|c| c.start);
    let mut visibility = Track::new();
    let mut offset = Track::new();
    for (i, clip) in clips.iter().enumerate() {
        let next_start = clips.get(i + 1).map(|c| c.start);
        visibility.insert(Key::step(clip.start, true));
        offset.insert(Key::step(clip.start, clip.trim));
        let end = match (clip.end, next_start) {
            (Some(end), Some(next)) => Some(end.min(next)),
            (Some(end), None) => Some(end),
            (None, _) => None,
        };
        // A clip that ends exactly where the next begins needs no key of its own there.
        if let Some(end) = end.filter(|end| Some(*end) != next_start) {
            visibility.insert(Key::step(end, false));
        }
    }
    if clips.is_empty() {
        // No clips at all: never on the timeline, which an empty track would not say.
        visibility.insert(Key::step(0, false));
    }
    anim.visibility = visibility;
    anim.offset = offset;
}

fn edit<T>(
    doc: &mut Document,
    id: LayerId,
    change: impl FnOnce(&mut Vec<Clip>) -> Result<T, OpError>,
) -> Result<T, OpError> {
    let layer = doc.layer_mut(id).ok_or(OpError::NoSuchLayer(id))?;
    let mut clips = clips_of(&layer.anim);
    let result = change(&mut clips)?;
    set_clips(&mut layer.anim, &clips);
    Ok(result)
}

fn no_clip(index: usize) -> OpError {
    OpError::Invalid(format!("the layer has no clip number {}", index + 1))
}

/// Ends the clip that covers `time` there; where no clip covers it, starts a new one.
pub fn toggle_clip_at(doc: &mut Document, id: LayerId, time: Flicks) -> Result<(), OpError> {
    edit(doc, id, |clips| {
        match clips.iter_mut().find(|c| c.contains(time)) {
            Some(clip) if clip.start == time => {
                return Err(OpError::Invalid("a clip cannot end where it starts".into()))
            }
            Some(clip) => clip.end = Some(time),
            None => clips.push(Clip {
                start: time,
                end: None,
                trim: 0,
            }),
        }
        Ok(())
    })
}

/// Cuts the clip that covers `time` in two there. The second part carries on where
/// the first left off.
pub fn split_clip(doc: &mut Document, id: LayerId, time: Flicks) -> Result<(), OpError> {
    edit(doc, id, |clips| {
        let clip = clips
            .iter_mut()
            .find(|c| c.contains(time) && c.start != time)
            .ok_or_else(|| OpError::Invalid("there is no clip to split at that time".into()))?;
        let second = Clip {
            start: time,
            end: clip.end,
            trim: clip.trim + (time - clip.start),
        };
        clip.end = Some(time);
        clips.push(second);
        Ok(())
    })
}

/// Shifts the keys of `track` that lie in `from..until` by `delta`.
fn shift_keys<T: Clone>(track: &mut Track<T>, from: Flicks, until: Option<Flicks>, delta: Flicks) {
    let moving: Vec<Key<T>> = track
        .keys()
        .iter()
        .filter(|k| k.time >= from && until.is_none_or(|u| k.time < u))
        .cloned()
        .collect();
    for key in &moving {
        track.remove(key.time);
    }
    for mut key in moving {
        key.time += delta;
        track.insert(key);
    }
}

/// Moves a clip to start at `start`, keeping its length. The layer's own transform
/// and opacity keys inside the clip move with it.
pub fn move_clip(
    doc: &mut Document,
    id: LayerId,
    index: usize,
    start: Flicks,
) -> Result<(), OpError> {
    let start = start.max(0);
    let moved = edit(doc, id, |clips| {
        let clip = clips.get_mut(index).ok_or_else(|| no_clip(index))?;
        let before = *clip;
        let delta = start - clip.start;
        clip.start = start;
        clip.end = clip.end.map(|end| end + delta);
        Ok((before, delta))
    })?;
    let (before, delta) = moved;
    if delta != 0 {
        let anim = &mut doc.layer_mut(id).expect("edited above").anim;
        shift_keys(&mut anim.transform, before.start, before.end, delta);
        shift_keys(&mut anim.opacity, before.start, before.end, delta);
        for track in anim.channels.values_mut() {
            shift_keys(track, before.start, before.end, delta);
        }
    }
    Ok(())
}

/// Changes where a clip starts or ends without moving its content in time.
/// `end` is `Some(None)` to make the clip open-ended.
pub fn trim_clip(
    doc: &mut Document,
    id: LayerId,
    index: usize,
    start: Option<Flicks>,
    end: Option<Option<Flicks>>,
) -> Result<(), OpError> {
    edit(doc, id, |clips| {
        let clip = clips.get_mut(index).ok_or_else(|| no_clip(index))?;
        if let Some(start) = start {
            let start = start.max(0);
            // Trimming the start eats into the content; pulling it back out reveals it
            // again, as far as there is content to reveal.
            clip.trim = (clip.trim + (start - clip.start)).max(0);
            clip.start = start;
        }
        if let Some(end) = end {
            clip.end = end;
        }
        if clip.end.is_some_and(|end| end <= clip.start) {
            return Err(OpError::Invalid("a clip must end after it starts".into()));
        }
        Ok(())
    })
}

pub fn delete_clip(doc: &mut Document, id: LayerId, index: usize) -> Result<(), OpError> {
    edit(doc, id, |clips| {
        if index >= clips.len() {
            return Err(no_clip(index));
        }
        clips.remove(index);
        Ok(())
    })
}

/// Adds a copy of a clip starting at `start`: the same content shown again.
pub fn duplicate_clip(
    doc: &mut Document,
    id: LayerId,
    index: usize,
    start: Flicks,
) -> Result<(), OpError> {
    edit(doc, id, |clips| {
        let source = *clips.get(index).ok_or_else(|| no_clip(index))?;
        let start = start.max(0);
        clips.push(Clip {
            start,
            end: source.end.map(|end| end - source.start + start),
            trim: source.trim,
        });
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::clip_at;
    use crate::math::Xform;
    use crate::model::Interp;
    use glam::DVec3;

    fn doc() -> (Document, LayerId) {
        let doc = Document::new();
        let id = doc.find_by_path("Root/Paint").unwrap();
        (doc, id)
    }

    fn spans(doc: &Document, id: LayerId) -> Vec<(Flicks, Option<Flicks>, Flicks)> {
        clips_of(&doc.layer(id).unwrap().anim)
            .iter()
            .map(|c| (c.start, c.end, c.trim))
            .collect()
    }

    #[test]
    fn a_new_layer_is_one_open_clip_and_keys_round_trip_through_clips() {
        let (mut doc, id) = doc();
        assert_eq!(spans(&doc, id), [(0, None, 0)]);
        let clips = [
            Clip {
                start: 100,
                end: Some(200),
                trim: 30,
            },
            Clip {
                start: 10,
                end: Some(50),
                trim: 0,
            },
            Clip {
                start: 400,
                end: None,
                trim: 5,
            },
        ];
        set_clips(&mut doc.layer_mut(id).unwrap().anim, &clips);
        assert_eq!(
            spans(&doc, id),
            [(10, Some(50), 0), (100, Some(200), 30), (400, None, 5)]
        );
        let anim = &doc.layer(id).unwrap().anim;
        assert_eq!(
            anim.visibility
                .keys()
                .iter()
                .map(|k| (k.time, k.value))
                .collect::<Vec<_>>(),
            [
                (10, true),
                (50, false),
                (100, true),
                (200, false),
                (400, true)
            ]
        );
        // The evaluator agrees: trimmed content starts 30 in.
        assert!(!clip_at(anim, 60).active);
        assert_eq!(clip_at(anim, 150).local_time, 80);
        let mut bare = LayerAnim::default();
        assert_eq!(
            clips_of(&bare),
            [Clip {
                start: 0,
                end: None,
                trim: 0
            }],
            "no keys at all means always there"
        );
        set_clips(&mut bare, &[]);
        assert!(!clip_at(&bare, 1000).active, "no clips means never there");
        assert!(clips_of(&bare).is_empty());
    }

    #[test]
    fn overlapping_clips_are_cut_short_at_the_next_start() {
        let mut anim = LayerAnim::default();
        set_clips(
            &mut anim,
            &[
                Clip {
                    start: 0,
                    end: None,
                    trim: 0,
                },
                Clip {
                    start: 100,
                    end: Some(300),
                    trim: 0,
                },
                Clip {
                    start: 200,
                    end: Some(250),
                    trim: 0,
                },
                Clip {
                    start: 500,
                    end: Some(500),
                    trim: 0,
                },
            ],
        );
        assert_eq!(
            clips_of(&anim)
                .iter()
                .map(|c| (c.start, c.end))
                .collect::<Vec<_>>(),
            [(0, Some(100)), (100, Some(200)), (200, Some(250))]
        );
        assert_eq!(anim.visibility.len(), 4, "abutting clips share a key");
    }

    #[test]
    fn toggling_ends_a_clip_or_starts_one() {
        let (mut doc, id) = doc();
        toggle_clip_at(&mut doc, id, 240).unwrap();
        assert_eq!(spans(&doc, id), [(0, Some(240), 0)]);
        toggle_clip_at(&mut doc, id, 500).unwrap();
        assert_eq!(spans(&doc, id), [(0, Some(240), 0), (500, None, 0)]);
        assert!(toggle_clip_at(&mut doc, id, 500).is_err());
        toggle_clip_at(&mut doc, id, 100).unwrap();
        assert_eq!(spans(&doc, id), [(0, Some(100), 0), (500, None, 0)]);
    }

    #[test]
    fn a_split_clip_carries_on_where_it_left_off() {
        let (mut doc, id) = doc();
        split_clip(&mut doc, id, 300).unwrap();
        assert_eq!(spans(&doc, id), [(0, Some(300), 0), (300, None, 300)]);
        let anim = &doc.layer(id).unwrap().anim;
        assert_eq!(clip_at(anim, 299).local_time, 299);
        assert_eq!(
            clip_at(anim, 300).local_time,
            300,
            "content does not jump at the cut"
        );
        assert!(split_clip(&mut doc, id, 300).is_err());
    }

    #[test]
    fn moving_a_clip_takes_its_keys_along_and_trimming_does_not_shift_content() {
        let (mut doc, id) = doc();
        toggle_clip_at(&mut doc, id, 1000).unwrap();
        let layer = doc.layer_mut(id).unwrap();
        layer
            .anim
            .transform
            .set(200, Xform::from_translation(DVec3::X), Interp::Linear);
        layer
            .anim
            .transform
            .set(5000, Xform::from_translation(DVec3::Y), Interp::Linear);
        layer.anim.opacity.set(900, 0.5, Interp::Step);

        move_clip(&mut doc, id, 0, 2000).unwrap();
        assert_eq!(spans(&doc, id), [(2000, Some(3000), 0)]);
        let anim = &doc.layer(id).unwrap().anim;
        assert_eq!(
            anim.transform
                .keys()
                .iter()
                .map(|k| k.time)
                .collect::<Vec<_>>(),
            [2200, 5000],
            "keys inside the clip moved, the one outside stayed"
        );
        assert_eq!(anim.opacity.keys()[0].time, 2900);

        trim_clip(&mut doc, id, 0, Some(2400), None).unwrap();
        assert_eq!(spans(&doc, id), [(2400, Some(3000), 400)]);
        assert_eq!(
            clip_at(&doc.layer(id).unwrap().anim, 2500).local_time,
            500,
            "the same content at the same time as before"
        );
        trim_clip(&mut doc, id, 0, Some(1900), Some(None)).unwrap();
        assert_eq!(
            spans(&doc, id),
            [(1900, None, 0)],
            "pulled back out past its beginning, and reopened"
        );
        assert!(trim_clip(&mut doc, id, 0, None, Some(Some(1900))).is_err());
        assert!(trim_clip(&mut doc, id, 7, None, None).is_err());
    }

    #[test]
    fn clips_can_be_deleted_and_shown_again_elsewhere() {
        let (mut doc, id) = doc();
        toggle_clip_at(&mut doc, id, 100).unwrap();
        duplicate_clip(&mut doc, id, 0, 400).unwrap();
        assert_eq!(spans(&doc, id), [(0, Some(100), 0), (400, Some(500), 0)]);
        delete_clip(&mut doc, id, 0).unwrap();
        assert_eq!(spans(&doc, id), [(400, Some(500), 0)]);
        delete_clip(&mut doc, id, 0).unwrap();
        assert!(spans(&doc, id).is_empty());
        assert!(delete_clip(&mut doc, id, 0).is_err());
    }
}
