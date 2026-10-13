//! The playhead: where playback is, and how it moves.

use crate::eval::Clock;
use crate::model::{Action, Document};
use crate::time::{Flicks, FLICKS_PER_SECOND};

/// Something that happened while the playhead advanced, for the interface to react to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackEvent {
    /// The playhead reached a Stop and waits for the viewer to continue.
    ReachedStop,
    /// The playhead wrapped around: the working range looped or a Loop mark restarted the piece.
    Looped,
    /// The playhead reached the end of the working range and playback stopped.
    ReachedEnd,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Playback {
    /// Position of the root playhead.
    pub time: Flicks,
    /// Real time spent waiting at Stops since the last seek. Sequences keep running through it.
    pub held: Flicks,
    pub playing: bool,
    /// The time of the Stop the playhead is waiting at.
    waiting_at: Option<Flicks>,
    /// Fraction of a flick carried over between advances.
    carry: f64,
}

/// The working range, when the document has a usable one.
fn range(doc: &Document) -> Option<(Flicks, Flicks)> {
    let (start, end) = (doc.settings.range_start.max(0), doc.settings.range_end);
    (end > start).then_some((start, end))
}

impl Playback {
    pub fn clock(&self) -> Clock {
        Clock {
            time: self.time,
            held: self.held,
        }
    }

    /// True while playback waits at a Stop.
    pub fn is_waiting(&self) -> bool {
        self.waiting_at.is_some()
    }

    /// Moves the playhead. Scrubbing ignores Stops and forgets time spent waiting at them.
    pub fn seek(&mut self, time: Flicks) {
        self.time = time.max(0);
        self.held = 0;
        self.waiting_at = None;
        self.carry = 0.0;
    }

    /// Starts playing, or continues past the Stop the playhead waits at.
    pub fn play(&mut self, doc: &Document, loop_range: bool) {
        if let Some(stop) = self.waiting_at.take() {
            // Standing exactly on the Stop means the next advance has passed it.
            self.time = stop;
        } else if !self.playing {
            if let Some((start, end)) = range(doc) {
                if self.time >= end || (loop_range && self.time < start) {
                    self.seek(start);
                }
            }
        }
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    /// Advances playback by `seconds` of real time.
    pub fn advance(
        &mut self,
        doc: &Document,
        seconds: f64,
        loop_range: bool,
    ) -> Option<PlaybackEvent> {
        if !self.playing || !(seconds.is_finite() && seconds > 0.0) {
            return None;
        }
        let exact = seconds.min(1.0) * FLICKS_PER_SECOND as f64 + self.carry;
        let step = exact.floor() as Flicks;
        self.carry = exact - step as f64;
        if self.waiting_at.is_some() {
            self.held += step;
            return None;
        }
        let target = self.time + step;

        // The first chapter mark in (time, target].
        let mark = doc.layer(doc.root()).and_then(|root| {
            root.anim
                .actions
                .keys()
                .iter()
                .find(|k| k.time > self.time && k.time <= target && k.value != Action::Play)
                .map(|k| (k.time, k.value))
        });
        let end = range(doc)
            .map(|(_, end)| end)
            .filter(|end| *end > self.time && *end <= target);

        match (mark, end) {
            (Some((at, action)), end) if end.is_none_or(|end| at <= end) => match action {
                Action::Stop => {
                    // Hold just before the Stop, so whatever starts at the same time has not started yet.
                    self.time = at - 1;
                    self.waiting_at = Some(at);
                    Some(PlaybackEvent::ReachedStop)
                }
                _ => {
                    self.seek(target - at);
                    Some(PlaybackEvent::Looped)
                }
            },
            (_, Some(end)) => {
                let (start, _) = range(doc).expect("an end implies a range");
                if loop_range {
                    self.seek(start + (target - end) % (end - start));
                    Some(PlaybackEvent::Looped)
                } else {
                    self.time = end;
                    self.playing = false;
                    Some(PlaybackEvent::ReachedEnd)
                }
            }
            _ => {
                self.time = target;
                None
            }
        }
    }

    /// The chapter mark or range start before the playhead. Chapters are the Play marks on the root.
    pub fn previous_mark(&self, doc: &Document) -> Flicks {
        // While playing, the playhead has always just left a mark; reach for the one before.
        let slack = if self.playing {
            FLICKS_PER_SECOND / 2
        } else {
            0
        };
        let before = self.time - slack;
        let marks = doc
            .layer(doc.root())
            .map(|root| {
                root.anim
                    .actions
                    .keys()
                    .iter()
                    .filter(|k| k.value == Action::Play)
                    .map(|k| k.time)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let start = range(doc).map_or(0, |(start, _)| start);
        marks
            .into_iter()
            .chain([start, 0])
            .filter(|t| *t < before)
            .max()
            .unwrap_or(0)
    }

    /// The chapter mark or range end after the playhead.
    pub fn next_mark(&self, doc: &Document) -> Flicks {
        let marks = doc
            .layer(doc.root())
            .map(|root| {
                root.anim
                    .actions
                    .keys()
                    .iter()
                    .filter(|k| k.value == Action::Play)
                    .map(|k| k.time)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let end = range(doc).map(|(_, end)| end);
        marks
            .into_iter()
            .chain(end)
            .filter(|t| *t > self.time)
            .min()
            .unwrap_or(self.time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Interp;
    use crate::time::from_seconds;

    fn doc_with_range(start: f64, end: f64) -> Document {
        let mut doc = Document::new();
        doc.settings.range_start = from_seconds(start);
        doc.settings.range_end = from_seconds(end);
        doc
    }

    fn mark(doc: &mut Document, seconds: f64, action: Action) {
        let root = doc.root();
        doc.layer_mut(root)
            .unwrap()
            .anim
            .actions
            .set(from_seconds(seconds), action, Interp::Step);
    }

    #[test]
    fn playing_advances_in_real_time_without_drift() {
        let doc = doc_with_range(0.0, 100.0);
        let mut playback = Playback::default();
        playback.play(&doc, false);
        for _ in 0..90 {
            playback.advance(&doc, 1.0 / 90.0, false);
        }
        assert!(
            (playback.time - from_seconds(1.0)).abs() <= 1,
            "{}",
            playback.time
        );
        playback.pause();
        let at = playback.time;
        playback.advance(&doc, 1.0, false);
        assert_eq!(playback.time, at);
    }

    #[test]
    fn the_range_loops_or_ends_playback() {
        let doc = doc_with_range(1.0, 2.0);
        let mut playback = Playback::default();
        playback.play(&doc, true);
        assert_eq!(
            playback.time,
            from_seconds(1.0),
            "looped playback starts inside the range"
        );
        playback.advance(&doc, 0.75, true);
        assert_eq!(
            playback.advance(&doc, 0.5, true),
            Some(PlaybackEvent::Looped)
        );
        assert!((playback.time - from_seconds(1.25)).abs() <= 2);

        let mut once = Playback::default();
        once.seek(from_seconds(1.5));
        once.play(&doc, false);
        assert_eq!(
            once.advance(&doc, 0.75, false),
            Some(PlaybackEvent::ReachedEnd)
        );
        assert_eq!(once.time, from_seconds(2.0));
        assert!(!once.playing);
        once.play(&doc, false);
        assert_eq!(
            once.time,
            from_seconds(1.0),
            "playing again from the end starts over"
        );
    }

    #[test]
    fn a_stop_holds_the_playhead_while_time_keeps_passing_for_sequences() {
        let mut doc = doc_with_range(0.0, 10.0);
        mark(&mut doc, 1.0, Action::Stop);
        let mut playback = Playback::default();
        playback.play(&doc, false);
        assert_eq!(playback.advance(&doc, 0.6, false), None);
        assert_eq!(
            playback.advance(&doc, 0.6, false),
            Some(PlaybackEvent::ReachedStop)
        );
        assert_eq!(playback.time, from_seconds(1.0) - 1);
        assert!(playback.is_waiting() && playback.playing);
        playback.advance(&doc, 2.0, false);
        playback.advance(&doc, 0.5, false);
        assert_eq!(playback.time, from_seconds(1.0) - 1, "the root stays put");
        assert_eq!(
            playback.held,
            from_seconds(1.5),
            "one advance is capped at a second"
        );

        playback.play(&doc, false);
        assert_eq!(
            playback.advance(&doc, 0.25, false),
            None,
            "continuing does not stop at the same mark again"
        );
        assert_eq!(playback.time, from_seconds(1.25));
        assert_eq!(
            playback.held,
            from_seconds(1.5),
            "sequences keep the time they ran while waiting"
        );
        playback.seek(0);
        assert_eq!(playback.held, 0);
    }

    #[test]
    fn a_loop_mark_restarts_the_piece_and_chapters_are_play_marks() {
        let mut doc = doc_with_range(0.0, 10.0);
        mark(&mut doc, 2.0, Action::Play);
        mark(&mut doc, 4.0, Action::Play);
        mark(&mut doc, 5.0, Action::Loop);
        let mut playback = Playback::default();
        playback.seek(from_seconds(4.9));
        playback.play(&doc, false);
        assert_eq!(
            playback.advance(&doc, 0.2, false),
            Some(PlaybackEvent::Looped)
        );
        assert!((playback.time - from_seconds(0.1)).abs() <= 2);

        playback.pause();
        playback.seek(from_seconds(4.5));
        assert_eq!(playback.next_mark(&doc), from_seconds(10.0));
        assert_eq!(playback.previous_mark(&doc), from_seconds(4.0));
        playback.seek(from_seconds(4.0));
        assert_eq!(playback.previous_mark(&doc), from_seconds(2.0));
        playback.seek(from_seconds(1.0));
        assert_eq!(playback.previous_mark(&doc), 0);
        assert_eq!(playback.next_mark(&doc), from_seconds(2.0));
    }
}
