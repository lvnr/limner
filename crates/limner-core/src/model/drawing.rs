//! A drawing: the ordered strokes shown by one frame of a paint layer.
//!
//! Strokes are held in a rope of chunks. A chunk is the unit of sharing between
//! document versions and the unit the renderer builds a mesh for, so an edit only
//! copies, and only re-meshes, the chunks it touches.

use super::stroke::Stroke;
use crate::math::Aabb;
use std::sync::Arc;

/// Soft limit on the tessellated vertices of one chunk. Strokes are never split to
/// meet it; a chunk holding a single long stroke may exceed it.
pub const MAX_CHUNK_VERTICES: usize = 60_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StrokeChunk {
    strokes: Vec<Arc<Stroke>>,
}

impl StrokeChunk {
    pub fn strokes(&self) -> &[Arc<Stroke>] {
        &self.strokes
    }

    pub fn len(&self) -> usize {
        self.strokes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strokes.is_empty()
    }

    pub fn vertex_count(&self) -> usize {
        self.strokes.iter().map(|s| s.vertex_count()).sum()
    }

    pub fn point_count(&self) -> usize {
        self.strokes.iter().map(|s| s.len()).sum()
    }

    pub fn bounds(&self) -> Aabb {
        self.strokes
            .iter()
            .fold(Aabb::EMPTY, |b, s| b.union(&s.bounds()))
    }
}

/// Position of a stroke inside a drawing. Valid until strokes are added or removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StrokeAddr {
    pub chunk: u32,
    pub index: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    chunks: Vec<Arc<StrokeChunk>>,
}

impl Drawing {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_strokes(strokes: impl IntoIterator<Item = Stroke>) -> Self {
        let mut drawing = Self::new();
        for stroke in strokes {
            drawing.push_stroke(stroke);
        }
        drawing
    }

    /// True when both drawings hold the same strokes in the same chunks. Chunks and
    /// strokes the two share are recognised without looking inside them, so comparing
    /// a drawing with an edited copy of itself costs only what the edit touched.
    pub fn same_as(&self, other: &Drawing) -> bool {
        self.chunks.len() == other.chunks.len()
            && self.chunks.iter().zip(&other.chunks).all(|(a, b)| {
                Arc::ptr_eq(a, b)
                    || (a.strokes.len() == b.strokes.len()
                        && a.strokes
                            .iter()
                            .zip(&b.strokes)
                            .all(|(x, y)| Arc::ptr_eq(x, y) || x == y))
            })
    }

    pub fn chunks(&self) -> &[Arc<StrokeChunk>] {
        &self.chunks
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    pub fn stroke_count(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum()
    }

    pub fn point_count(&self) -> usize {
        self.chunks.iter().map(|c| c.point_count()).sum()
    }

    pub fn vertex_count(&self) -> usize {
        self.chunks.iter().map(|c| c.vertex_count()).sum()
    }

    pub fn bounds(&self) -> Aabb {
        self.chunks
            .iter()
            .fold(Aabb::EMPTY, |b, c| b.union(&c.bounds()))
    }

    /// Strokes in drawing order.
    pub fn strokes(&self) -> impl Iterator<Item = &Arc<Stroke>> + '_ {
        self.chunks.iter().flat_map(|c| c.strokes.iter())
    }

    /// Strokes in drawing order with their addresses.
    pub fn strokes_with_addr(&self) -> impl Iterator<Item = (StrokeAddr, &Arc<Stroke>)> + '_ {
        self.chunks.iter().enumerate().flat_map(|(ci, c)| {
            c.strokes.iter().enumerate().map(move |(si, s)| {
                (
                    StrokeAddr {
                        chunk: ci as u32,
                        index: si as u32,
                    },
                    s,
                )
            })
        })
    }

    pub fn stroke(&self, addr: StrokeAddr) -> Option<&Arc<Stroke>> {
        self.chunks
            .get(addr.chunk as usize)?
            .strokes
            .get(addr.index as usize)
    }

    /// Mutable access to one stroke, copying the chunk and the stroke if they are shared.
    pub fn stroke_mut(&mut self, addr: StrokeAddr) -> Option<&mut Stroke> {
        let chunk = Arc::make_mut(self.chunks.get_mut(addr.chunk as usize)?);
        Some(Arc::make_mut(chunk.strokes.get_mut(addr.index as usize)?))
    }

    pub fn find_stroke(&self, id: u32) -> Option<StrokeAddr> {
        self.strokes_with_addr()
            .find(|(_, s)| s.id == id)
            .map(|(addr, _)| addr)
    }

    pub fn last_addr(&self) -> Option<StrokeAddr> {
        let chunk = self.chunks.len().checked_sub(1)?;
        let index = self.chunks[chunk].len().checked_sub(1)?;
        Some(StrokeAddr {
            chunk: chunk as u32,
            index: index as u32,
        })
    }

    /// An id not used by any stroke in this drawing.
    pub fn next_stroke_id(&self) -> u32 {
        self.strokes()
            .map(|s| s.id)
            .max()
            .map_or(0, |m| m.wrapping_add(1))
    }

    /// Appends a stroke, to the last chunk if it has room and otherwise to a new one.
    pub fn push_stroke(&mut self, stroke: Stroke) -> StrokeAddr {
        self.push_shared(Arc::new(stroke))
    }

    /// Appends a stroke that may be shared with another drawing.
    pub fn push_shared(&mut self, stroke: Arc<Stroke>) -> StrokeAddr {
        let fits = self
            .chunks
            .last()
            .is_some_and(|c| c.vertex_count() + stroke.vertex_count() <= MAX_CHUNK_VERTICES);
        if !fits {
            self.chunks.push(Arc::new(StrokeChunk::default()));
        }
        let chunk_index = self.chunks.len() - 1;
        let chunk = Arc::make_mut(&mut self.chunks[chunk_index]);
        chunk.strokes.push(stroke);
        StrokeAddr {
            chunk: chunk_index as u32,
            index: (chunk.strokes.len() - 1) as u32,
        }
    }

    /// Appends a stroke in a chunk of its own. Used while a stroke is being drawn, so
    /// only a small mesh is rebuilt as it grows; call [`Drawing::merge_tail`] afterwards.
    pub fn push_stroke_in_new_chunk(&mut self, stroke: Stroke) -> StrokeAddr {
        self.chunks.push(Arc::new(StrokeChunk {
            strokes: vec![Arc::new(stroke)],
        }));
        StrokeAddr {
            chunk: (self.chunks.len() - 1) as u32,
            index: 0,
        }
    }

    /// Folds the last chunk into the one before it when both fit in one chunk.
    pub fn merge_tail(&mut self) {
        let n = self.chunks.len();
        if n < 2 {
            return;
        }
        if self.chunks[n - 2].vertex_count() + self.chunks[n - 1].vertex_count()
            > MAX_CHUNK_VERTICES
        {
            return;
        }
        let tail = self.chunks.pop().expect("checked length");
        let target = Arc::make_mut(&mut self.chunks[n - 2]);
        target.strokes.extend(tail.strokes.iter().cloned());
    }

    /// Removes the strokes for which `keep` returns false and returns how many were removed.
    /// Chunks without a removed stroke stay shared.
    pub fn retain(&mut self, mut keep: impl FnMut(&Stroke) -> bool) -> usize {
        let mut removed = 0;
        for chunk in &mut self.chunks {
            let drop_any = chunk.strokes.iter().any(|s| !keep(s));
            if drop_any {
                let chunk = Arc::make_mut(chunk);
                let before = chunk.strokes.len();
                chunk.strokes.retain(|s| keep(s));
                removed += before - chunk.strokes.len();
            }
        }
        self.chunks.retain(|c| !c.is_empty());
        removed
    }

    /// Calls `edit` for every stroke whose bounds pass `candidate`; `edit` receives
    /// the stroke for inspection and returns a replacement if it changes. Strokes and
    /// chunks that do not change stay shared. Returns the number of strokes replaced.
    pub fn edit_strokes(
        &mut self,
        mut candidate: impl FnMut(&Aabb) -> bool,
        mut edit: impl FnMut(&Stroke) -> Option<Stroke>,
    ) -> usize {
        let mut changed = 0;
        for chunk in &mut self.chunks {
            if !candidate(&chunk.bounds()) {
                continue;
            }
            for index in 0..chunk.strokes.len() {
                let stroke = &chunk.strokes[index];
                if !candidate(&stroke.bounds()) {
                    continue;
                }
                if let Some(replacement) = edit(stroke) {
                    Arc::make_mut(chunk).strokes[index] = Arc::new(replacement);
                    changed += 1;
                }
            }
        }
        changed
    }

    /// Splits chunks that have grown past the soft limit.
    pub fn rebalance(&mut self) {
        if self
            .chunks
            .iter()
            .all(|c| c.vertex_count() <= MAX_CHUNK_VERTICES || c.len() <= 1)
        {
            return;
        }
        let old = std::mem::take(&mut self.chunks);
        for chunk in old {
            if chunk.vertex_count() <= MAX_CHUNK_VERTICES || chunk.len() <= 1 {
                self.chunks.push(chunk);
                continue;
            }
            let mut current = StrokeChunk::default();
            for stroke in &chunk.strokes {
                if !current.is_empty()
                    && current.vertex_count() + stroke.vertex_count() > MAX_CHUNK_VERTICES
                {
                    self.chunks.push(Arc::new(std::mem::take(&mut current)));
                }
                current.strokes.push(stroke.clone());
            }
            if !current.is_empty() {
                self.chunks.push(Arc::new(current));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::stroke::{Brush, Point};

    fn stroke(id: u32, points: usize) -> Stroke {
        let p = Point {
            pos: [id as f32, 0.0, 0.0],
            normal: [0.0, 1.0, 0.0],
            dir: [0.0, 0.0, 1.0],
            color: [1.0; 3],
            opacity: 1.0,
            width: 0.01,
        };
        Stroke::new(id, Brush::Cylinder, false, vec![p; points])
    }

    #[test]
    fn strokes_keep_their_order_across_chunks() {
        // 1,000 points of a 7-sided tube are 7,000 vertices, so nine strokes overflow one chunk.
        let d = Drawing::from_strokes((0..20).map(|i| stroke(i, 1_000)));
        assert!(d.chunks().len() >= 3);
        assert!(d
            .chunks()
            .iter()
            .all(|c| c.vertex_count() <= MAX_CHUNK_VERTICES));
        let ids: Vec<u32> = d.strokes().map(|s| s.id).collect();
        assert_eq!(ids, (0..20).collect::<Vec<_>>());
        assert_eq!(d.stroke_count(), 20);
        assert_eq!(d.point_count(), 20_000);
        assert_eq!(d.next_stroke_id(), 20);
    }

    #[test]
    fn a_single_long_stroke_gets_its_own_chunk() {
        let d = Drawing::from_strokes([stroke(0, 100), stroke(1, 20_000), stroke(2, 100)]);
        assert_eq!(d.chunks().len(), 3);
        assert_eq!(d.chunks()[1].vertex_count(), 140_000);
    }

    #[test]
    fn cloning_shares_chunks_and_editing_copies_only_the_touched_one() {
        let original = Drawing::from_strokes((0..20).map(|i| stroke(i, 1_000)));
        let mut edited = original.clone();
        for (a, b) in original.chunks().iter().zip(edited.chunks()) {
            assert!(Arc::ptr_eq(a, b));
        }
        let addr = edited.find_stroke(12).unwrap();
        edited.stroke_mut(addr).unwrap().points_mut()[0].opacity = 0.5;
        for (index, (a, b)) in original.chunks().iter().zip(edited.chunks()).enumerate() {
            assert_eq!(
                Arc::ptr_eq(a, b),
                index != addr.chunk as usize,
                "chunk {index}"
            );
        }
        // Within the touched chunk, untouched strokes are still shared.
        let touched_original = &original.chunks()[addr.chunk as usize];
        let touched_edited = &edited.chunks()[addr.chunk as usize];
        for (index, (a, b)) in touched_original
            .strokes()
            .iter()
            .zip(touched_edited.strokes())
            .enumerate()
        {
            assert_eq!(Arc::ptr_eq(a, b), index != addr.index as usize);
        }
        assert_eq!(
            original.stroke(addr).unwrap().points()[0].opacity,
            1.0,
            "the original is untouched"
        );
    }

    #[test]
    fn retain_removes_strokes_and_empty_chunks() {
        let mut d = Drawing::from_strokes((0..20).map(|i| stroke(i, 1_000)));
        let first_chunk = d.chunks()[0].clone();
        let first_chunk_ids: Vec<u32> = first_chunk.strokes().iter().map(|s| s.id).collect();
        let removed = d.retain(|s| first_chunk_ids.contains(&s.id) || s.id == 19);
        assert_eq!(removed, 20 - first_chunk_ids.len() - 1);
        assert!(
            Arc::ptr_eq(&d.chunks()[0], &first_chunk),
            "an untouched chunk stays shared"
        );
        assert_eq!(d.strokes().last().unwrap().id, 19);
        assert!(d.chunks().iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn live_stroke_chunk_merges_back() {
        let mut d = Drawing::from_strokes([stroke(0, 10), stroke(1, 10)]);
        let addr = d.push_stroke_in_new_chunk(stroke(2, 5));
        assert_eq!(d.chunks().len(), 2);
        d.stroke_mut(addr)
            .unwrap()
            .points_mut()
            .extend(stroke(2, 5).into_points());
        d.merge_tail();
        assert_eq!(d.chunks().len(), 1);
        assert_eq!(d.strokes().map(|s| s.id).collect::<Vec<_>>(), vec![0, 1, 2]);
        assert_eq!(d.stroke(d.last_addr().unwrap()).unwrap().len(), 10);
    }

    #[test]
    fn edit_strokes_replaces_only_what_changes() {
        let mut d = Drawing::from_strokes((0..20).map(|i| stroke(i, 1_000)));
        let before = d.clone();
        let changed = d.edit_strokes(
            |bounds| bounds.intersects_sphere(glam::Vec3::new(3.0, 0.0, 0.0), 0.2),
            |s| {
                let mut copy = s.clone();
                copy.points_mut()[0].width = 0.5;
                Some(copy)
            },
        );
        assert_eq!(changed, 1);
        assert_eq!(
            d.stroke(d.find_stroke(3).unwrap()).unwrap().points()[0].width,
            0.5
        );
        let shared = before
            .chunks()
            .iter()
            .zip(d.chunks())
            .filter(|(a, b)| Arc::ptr_eq(a, b))
            .count();
        assert_eq!(shared, before.chunks().len() - 1);
    }

    #[test]
    fn rebalance_splits_overgrown_chunks() {
        let mut d = Drawing::from_strokes((0..4).map(|i| stroke(i, 1_000)));
        assert_eq!(d.chunks().len(), 1);
        for index in 0..4 {
            let addr = StrokeAddr { chunk: 0, index };
            let extra = stroke(index, 3_000).into_points();
            d.stroke_mut(addr).unwrap().points_mut().extend(extra);
        }
        assert!(d.chunks()[0].vertex_count() > MAX_CHUNK_VERTICES);
        d.rebalance();
        assert!(d.chunks().len() > 1);
        assert!(d
            .chunks()
            .iter()
            .all(|c| c.vertex_count() <= MAX_CHUNK_VERTICES));
        assert_eq!(
            d.strokes().map(|s| s.id).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }
}
