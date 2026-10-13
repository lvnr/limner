//! Animation keys and tracks.

use crate::time::Flicks;
use serde::{Deserialize, Serialize};

/// How a value moves from one key to the next. Stored on the earlier key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interp {
    /// Hold the earlier value until the next key.
    #[default]
    Step,
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Limner extension: a smooth curve through neighbouring keys.
    Smooth,
}

impl Interp {
    /// Remaps a linear parameter in `0..=1` by the easing curve.
    pub fn ease(self, t: f64) -> f64 {
        match self {
            Interp::Step => 0.0,
            Interp::Linear | Interp::Smooth => t,
            Interp::EaseIn => t * t * t,
            Interp::EaseOut => {
                let u = 1.0 - t;
                1.0 - u * u * u
            }
            Interp::EaseInOut => t * t * (3.0 - 2.0 * t),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key<T> {
    pub time: Flicks,
    pub value: T,
    #[serde(default)]
    pub interp: Interp,
}

impl<T> Key<T> {
    pub fn new(time: Flicks, value: T, interp: Interp) -> Self {
        Self {
            time,
            value,
            interp,
        }
    }

    pub fn step(time: Flicks, value: T) -> Self {
        Self {
            time,
            value,
            interp: Interp::Step,
        }
    }
}

/// Keys sorted by time, at most one per instant.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Track<T> {
    keys: Vec<Key<T>>,
}

impl<T> Default for Track<T> {
    fn default() -> Self {
        Self { keys: Vec::new() }
    }
}

impl<T> Track<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Builds a track from keys in any order. When two keys share a time the later one wins.
    pub fn from_keys(keys: impl IntoIterator<Item = Key<T>>) -> Self {
        let mut track = Self::new();
        for key in keys {
            track.insert(key);
        }
        track
    }

    pub fn keys(&self) -> &[Key<T>] {
        &self.keys
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    fn position(&self, time: Flicks) -> Result<usize, usize> {
        self.keys.binary_search_by_key(&time, |k| k.time)
    }

    /// Inserts a key, replacing any key already at that time.
    pub fn insert(&mut self, key: Key<T>) {
        match self.position(key.time) {
            Ok(i) => self.keys[i] = key,
            Err(i) => self.keys.insert(i, key),
        }
    }

    pub fn set(&mut self, time: Flicks, value: T, interp: Interp) {
        self.insert(Key {
            time,
            value,
            interp,
        });
    }

    pub fn remove(&mut self, time: Flicks) -> Option<Key<T>> {
        self.position(time).ok().map(|i| self.keys.remove(i))
    }

    pub fn get(&self, time: Flicks) -> Option<&Key<T>> {
        self.position(time).ok().map(|i| &self.keys[i])
    }

    pub fn get_mut(&mut self, time: Flicks) -> Option<&mut Key<T>> {
        self.position(time).ok().map(|i| &mut self.keys[i])
    }

    /// Index of the last key at or before `time`.
    pub fn index_at_or_before(&self, time: Flicks) -> Option<usize> {
        self.keys.partition_point(|k| k.time <= time).checked_sub(1)
    }

    pub fn key_at_or_before(&self, time: Flicks) -> Option<&Key<T>> {
        self.index_at_or_before(time).map(|i| &self.keys[i])
    }

    pub fn clear(&mut self) {
        self.keys.clear();
    }

    pub fn retain(&mut self, keep: impl FnMut(&Key<T>) -> bool) {
        self.keys.retain(keep);
    }

    /// Moves every key through `f` and restores the ordering invariant.
    pub fn remap_times(&mut self, mut f: impl FnMut(Flicks) -> Flicks) {
        let keys = std::mem::take(&mut self.keys);
        for mut key in keys {
            key.time = f(key.time);
            self.insert(key);
        }
    }

    pub fn first(&self) -> Option<&Key<T>> {
        self.keys.first()
    }

    pub fn last(&self) -> Option<&Key<T>> {
        self.keys.last()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_keeps_order_and_replaces_duplicates() {
        let mut t = Track::new();
        t.set(30, 'c', Interp::Step);
        t.set(10, 'a', Interp::Linear);
        t.set(20, 'b', Interp::Step);
        t.set(10, 'z', Interp::EaseIn);
        let times: Vec<_> = t.keys().iter().map(|k| (k.time, k.value)).collect();
        assert_eq!(times, vec![(10, 'z'), (20, 'b'), (30, 'c')]);
        assert_eq!(t.get(10).unwrap().interp, Interp::EaseIn);
    }

    #[test]
    fn lookup_at_or_before() {
        let t = Track::from_keys([Key::step(10, 1), Key::step(20, 2)]);
        assert_eq!(t.index_at_or_before(9), None);
        assert_eq!(t.index_at_or_before(10), Some(0));
        assert_eq!(t.index_at_or_before(19), Some(0));
        assert_eq!(t.index_at_or_before(20), Some(1));
        assert_eq!(t.index_at_or_before(1_000), Some(1));
    }

    #[test]
    fn remap_times_resorts() {
        let mut t = Track::from_keys([Key::step(10, 'a'), Key::step(20, 'b'), Key::step(30, 'c')]);
        t.remap_times(|time| 40 - time);
        let order: Vec<_> = t.keys().iter().map(|k| k.value).collect();
        assert_eq!(order, vec!['c', 'b', 'a']);
    }

    #[test]
    fn easing_curves_hit_their_end_points() {
        for interp in [
            Interp::Linear,
            Interp::EaseIn,
            Interp::EaseOut,
            Interp::EaseInOut,
            Interp::Smooth,
        ] {
            assert_eq!(interp.ease(0.0), 0.0);
            assert!((interp.ease(1.0) - 1.0).abs() < 1e-12);
        }
        assert_eq!(Interp::EaseIn.ease(0.5), 0.125);
        assert_eq!(Interp::EaseOut.ease(0.5), 0.875);
        assert_eq!(Interp::EaseInOut.ease(0.5), 0.5);
        assert_eq!(Interp::Step.ease(0.99), 0.0);
    }
}
