//! Builds Godot meshes from tessellated stroke chunks and caches them by chunk identity.

use godot::classes::mesh::{ArrayType, PrimitiveType};
use godot::classes::ArrayMesh;
use godot::prelude::*;
use limner::core::glam::DVec3;
use limner::core::model::StrokeChunk;
use limner::core::tess::{tessellate_chunk, ChunkMesh, SurfaceKind};
use std::collections::HashMap;
use std::sync::Arc;

/// A chunk's mesh as Godot holds it. Surface `i` of `mesh` is of kind `kinds[i]`.
pub struct BuiltMesh {
    pub mesh: Gd<ArrayMesh>,
    pub kinds: Vec<SurfaceKind>,
    /// Chunk centre in layer space; vertices are relative to it.
    pub origin: DVec3,
    pub vertices: usize,
    pub triangles: usize,
}

fn build(chunk_mesh: &ChunkMesh) -> Option<BuiltMesh> {
    let mut mesh = ArrayMesh::new_gd();
    let mut kinds = Vec::new();
    for kind in SurfaceKind::ALL {
        let surface = chunk_mesh.surface(kind);
        if surface.is_empty() {
            continue;
        }
        let positions: PackedVector3Array = surface
            .positions
            .iter()
            .map(|p| Vector3::new(p[0], p[1], p[2]))
            .collect();
        let normals: PackedVector3Array = surface
            .normals
            .iter()
            .map(|n| Vector3::new(n[0], n[1], n[2]))
            .collect();
        let colors: PackedColorArray = surface
            .colors
            .iter()
            .map(|c| Color::from_rgba8(c[0], c[1], c[2], c[3]))
            .collect();
        let extra: PackedByteArray = surface.extra.iter().flatten().copied().collect();
        // The core winds triangles counter-clockwise; Godot's front faces are clockwise.
        let indices: PackedInt32Array = surface
            .indices
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|t| [t[0] as i32, t[2] as i32, t[1] as i32])
            .collect();

        let mut arrays = VarArray::new();
        arrays.resize(ArrayType::MAX.ord() as usize, &Variant::nil());
        arrays.set(ArrayType::VERTEX.ord() as usize, &positions.to_variant());
        arrays.set(ArrayType::NORMAL.ord() as usize, &normals.to_variant());
        arrays.set(ArrayType::COLOR.ord() as usize, &colors.to_variant());
        arrays.set(ArrayType::CUSTOM0.ord() as usize, &extra.to_variant());
        arrays.set(ArrayType::INDEX.ord() as usize, &indices.to_variant());
        mesh.add_surface_from_arrays(PrimitiveType::TRIANGLES, &arrays);
        kinds.push(kind);
    }
    if kinds.is_empty() {
        return None;
    }
    Some(BuiltMesh {
        mesh,
        kinds,
        origin: DVec3::from_array(chunk_mesh.origin.map(f64::from)),
        vertices: chunk_mesh.vertex_count(),
        triangles: chunk_mesh.triangle_count(),
    })
}

struct Entry {
    /// Keeps the chunk alive so its address cannot be reused while it is a cache key.
    chunk: Arc<StrokeChunk>,
    /// `None` when the chunk produces no geometry.
    built: Option<BuiltMesh>,
    last_used: u64,
}

/// Meshes keyed by the identity of the chunk they were built from. Document versions
/// share unchanged chunks, so an edit rebuilds only the chunks it touched, and frames
/// that share a drawing share its meshes.
#[derive(Default)]
pub struct MeshCache {
    entries: HashMap<usize, Entry>,
    frame: u64,
    pub built_this_frame: usize,
}

/// Upper bound on cached vertices before least-recently-used meshes are dropped.
const VERTEX_BUDGET: usize = 12_000_000;

pub fn chunk_key(chunk: &Arc<StrokeChunk>) -> usize {
    Arc::as_ptr(chunk) as usize
}

impl MeshCache {
    pub fn begin_frame(&mut self) {
        self.frame += 1;
        self.built_this_frame = 0;
    }

    /// The mesh for `chunk`, built on first use.
    pub fn get(&mut self, chunk: &Arc<StrokeChunk>) -> Option<&BuiltMesh> {
        let frame = self.frame;
        let entry = self.entries.entry(chunk_key(chunk)).or_insert_with(|| {
            self.built_this_frame += 1;
            Entry {
                chunk: chunk.clone(),
                built: build(&tessellate_chunk(chunk)),
                last_used: frame,
            }
        });
        entry.last_used = frame;
        entry.built.as_ref()
    }

    /// Drops meshes whose chunk no longer exists anywhere else, and the least recently
    /// used ones when the cache is over budget.
    pub fn end_frame(&mut self) {
        if self.frame % 120 != 0 {
            return;
        }
        let frame = self.frame;
        self.entries
            .retain(|_, e| e.last_used == frame || Arc::strong_count(&e.chunk) > 1);
        let mut total: usize = self
            .entries
            .values()
            .filter_map(|e| e.built.as_ref())
            .map(|b| b.vertices)
            .sum();
        if total <= VERTEX_BUDGET {
            return;
        }
        let mut by_age: Vec<(u64, usize, usize)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used != frame)
            .map(|(k, e)| (e.last_used, *k, e.built.as_ref().map_or(0, |b| b.vertices)))
            .collect();
        by_age.sort_unstable();
        for (_, key, vertices) in by_age {
            if total <= VERTEX_BUDGET {
                break;
            }
            self.entries.remove(&key);
            total -= vertices;
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
