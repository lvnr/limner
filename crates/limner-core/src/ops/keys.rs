//! Keys on a track: moving them, changing how they move to the next, reversing them.
//!
//! These work on any track. Which track a command means, and what value a new key
//! takes, is decided by the session.

use crate::model::{Interp, Key, Track};
use crate::time::Flicks;

/// Shifts the keys at `times` by `delta`. A moved key replaces one already at its
/// new time. Keys that start at or after zero are kept there; keys already before
/// zero (from an imported file, say) move freely.
pub fn move_keys<T: Clone>(track: &mut Track<T>, times: &[Flicks], delta: Flicks) -> usize {
    let moving: Vec<Key<T>> = times.iter().filter_map(|t| track.remove(*t)).collect();
    let count = moving.len();
    // The earliest moved key decides how far left the group can go, so that keys
    // pushed against zero keep their spacing. The ends of the range of time stop
    // the group the same way.
    let earliest = moving.iter().map(|k| k.time).min().unwrap_or(0);
    let latest = moving.iter().map(|k| k.time).max().unwrap_or(0);
    let floor = if earliest >= 0 { 0 } else { Flicks::MIN };
    let delta = delta
        .max(floor.saturating_sub(earliest))
        .min(Flicks::MAX.saturating_sub(latest));
    for mut key in moving {
        key.time = key.time.saturating_add(delta);
        track.insert(key);
    }
    count
}

/// Sets how the keys at `times` move toward the keys after them.
pub fn set_interp<T>(track: &mut Track<T>, times: &[Flicks], interp: Interp) -> usize {
    let mut count = 0;
    for time in times {
        if let Some(key) = track.get_mut(*time) {
            if key.interp != interp {
                key.interp = interp;
                count += 1;
            }
        }
    }
    count
}

pub fn delete_keys<T>(track: &mut Track<T>, times: &[Flicks]) -> usize {
    times.iter().filter(|t| track.remove(**t).is_some()).count()
}

/// Easing as seen when time runs the other way.
fn mirrored(interp: Interp) -> Interp {
    match interp {
        Interp::EaseIn => Interp::EaseOut,
        Interp::EaseOut => Interp::EaseIn,
        other => other,
    }
}

/// Plays the keys at `times` backwards within the span they cover: the last value
/// comes first, and the gaps between keys are mirrored. Other keys are left alone.
pub fn reverse_keys<T: Clone>(track: &mut Track<T>, times: &[Flicks]) -> usize {
    let mut chosen: Vec<Key<T>> = times
        .iter()
        .filter_map(|t| track.get(*t).cloned())
        .collect();
    chosen.sort_by_key(|k| k.time);
    chosen.dedup_by_key(|k| k.time);
    let count = chosen.len();
    if count < 2 {
        return 0;
    }
    let (first, last) = (chosen[0].time, chosen[count - 1].time);
    for key in &chosen {
        track.remove(key.time);
    }
    for (i, key) in chosen.iter().enumerate().rev() {
        // A key's interpolation describes the stretch after it. Reversed, that
        // stretch is the one that used to come before it.
        let interp = if i == 0 {
            chosen[count - 1].interp
        } else {
            mirrored(chosen[i - 1].interp)
        };
        // first + last - time lies between first and last, so it fits even when
        // the sum on its way there would not.
        let mirrored_time = first as i128 + last as i128 - key.time as i128;
        track.insert(Key {
            time: mirrored_time as Flicks,
            value: key.value.clone(),
            interp,
        });
    }
    count
}

/// The time of the key before or after `time`.
pub fn neighbour<T>(track: &Track<T>, time: Flicks, forward: bool) -> Option<Flicks> {
    if forward {
        track.keys().iter().map(|k| k.time).find(|t| *t > time)
    } else {
        track
            .keys()
            .iter()
            .map(|k| k.time)
            .rev()
            .find(|t| *t < time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(keys: &[(Flicks, i32, Interp)]) -> Track<i32> {
        Track::from_keys(
            keys.iter()
                .map(|(time, value, interp)| Key::new(*time, *value, *interp)),
        )
    }

    fn contents(track: &Track<i32>) -> Vec<(Flicks, i32, Interp)> {
        track
            .keys()
            .iter()
            .map(|k| (k.time, k.value, k.interp))
            .collect()
    }

    #[test]
    fn keys_move_together_and_stop_at_zero() {
        let mut t = track(&[
            (10, 1, Interp::Step),
            (20, 2, Interp::Linear),
            (30, 3, Interp::Step),
        ]);
        assert_eq!(move_keys(&mut t, &[10, 20, 999], 15), 2);
        assert_eq!(
            contents(&t),
            [
                (25, 1, Interp::Step),
                (30, 3, Interp::Step),
                (35, 2, Interp::Linear)
            ]
        );
        move_keys(&mut t, &[25, 35], -100);
        assert_eq!(
            contents(&t),
            [
                (0, 1, Interp::Step),
                (10, 2, Interp::Linear),
                (30, 3, Interp::Step)
            ],
            "pushed against zero with their spacing kept"
        );
        move_keys(&mut t, &[10], 20);
        assert_eq!(
            contents(&t),
            [(0, 1, Interp::Step), (30, 2, Interp::Linear)],
            "a moved key replaces the one it lands on"
        );
    }

    #[test]
    fn keys_before_zero_move_the_way_they_are_dragged() {
        let mut t = track(&[(-100, 1, Interp::Step), (50, 2, Interp::Step)]);
        move_keys(&mut t, &[-100, 50], -10);
        assert_eq!(
            contents(&t),
            [(-110, 1, Interp::Step), (40, 2, Interp::Step)],
            "a key already before zero does not push the group to the right"
        );
        move_keys(&mut t, &[-110], 30);
        assert_eq!(contents(&t)[0], (-80, 1, Interp::Step));
    }

    #[test]
    fn extreme_key_times_do_not_overflow() {
        let mut t = track(&[
            (Flicks::MIN, 1, Interp::Step),
            (Flicks::MAX, 2, Interp::Step),
        ]);
        move_keys(&mut t, &[Flicks::MIN], -1);
        move_keys(&mut t, &[Flicks::MAX], 1);
        assert_eq!(
            contents(&t),
            [
                (Flicks::MIN, 1, Interp::Step),
                (Flicks::MAX, 2, Interp::Step)
            ]
        );
        assert_eq!(reverse_keys(&mut t, &[Flicks::MIN, Flicks::MAX]), 2);
        assert_eq!(
            contents(&t),
            [
                (Flicks::MIN, 2, Interp::Step),
                (Flicks::MAX, 1, Interp::Step)
            ]
        );
    }

    #[test]
    fn interpolation_and_deletion_apply_to_the_chosen_keys() {
        let mut t = track(&[
            (10, 1, Interp::Step),
            (20, 2, Interp::Step),
            (30, 3, Interp::Step),
        ]);
        assert_eq!(set_interp(&mut t, &[10, 30, 31], Interp::EaseInOut), 2);
        assert_eq!(
            set_interp(&mut t, &[10], Interp::EaseInOut),
            0,
            "already so"
        );
        assert_eq!(contents(&t)[1].2, Interp::Step);
        assert_eq!(delete_keys(&mut t, &[20, 21]), 1);
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn reversing_mirrors_values_gaps_and_easing() {
        let mut t = track(&[
            (0, 9, Interp::Step),
            (100, 1, Interp::EaseIn),
            (130, 2, Interp::Linear),
            (200, 3, Interp::Step),
            (500, 8, Interp::Step),
        ]);
        assert_eq!(reverse_keys(&mut t, &[100, 130, 200]), 3);
        assert_eq!(
            contents(&t),
            [
                (0, 9, Interp::Step),
                (100, 3, Interp::Linear),
                (170, 2, Interp::EaseOut),
                (200, 1, Interp::Step),
                (500, 8, Interp::Step)
            ],
            "the 30-long eased stretch is now the last one, easing the other way"
        );
        assert_eq!(reverse_keys(&mut t, &[100]), 0);
    }

    #[test]
    fn neighbours_are_the_keys_on_either_side() {
        let t = track(&[(10, 1, Interp::Step), (20, 2, Interp::Step)]);
        assert_eq!(neighbour(&t, 10, true), Some(20));
        assert_eq!(neighbour(&t, 15, false), Some(10));
        assert_eq!(neighbour(&t, 20, true), None);
        assert_eq!(neighbour(&t, 10, false), None);
    }
}
