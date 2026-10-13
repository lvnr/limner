//! Debug dumps used by the verification scripts under `scripts/verify`.

use limner::core::glam::Vec3;
use limner::core::model::Document;
use limner::core::tess::ring_positions;

/// Layout, all little-endian:
/// `"LRNG"`, u32 layer count, then per paint layer: u32 name length, UTF-8 layer
/// path, u8 visible (own flag and all ancestors), u8 mirrored, u32 stroke count, then
/// per stroke: u32 brush code, u32 point count, u32 ring size, f64 xyz per ring vertex.
pub fn rings(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"LRNG");
    let layers: Vec<_> = doc
        .walk()
        .into_iter()
        .filter(|id| doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
        .collect();
    out.extend_from_slice(&(layers.len() as u32).to_le_bytes());
    let mut ring: Vec<Vec3> = Vec::new();
    for id in layers {
        let layer = doc.layer(id).expect("listed above");
        let paint = layer.as_paint().expect("filtered above");
        let path = doc.path_of(id);
        out.extend_from_slice(&(path.len() as u32).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        let visible = layer.visible
            && doc
                .ancestors(id)
                .all(|a| doc.layer(a).is_some_and(|l| l.visible));
        out.push(visible as u8);
        let world = doc.world_from_layer(id);
        out.push((world.matrix3.determinant() < 0.0) as u8);
        let drawing = paint.drawing_at_frame(0).and_then(|d| doc.drawing(d));
        let strokes: Vec<_> = drawing.map(|d| d.strokes().collect()).unwrap_or_default();
        out.extend_from_slice(&(strokes.len() as u32).to_le_bytes());
        for stroke in strokes {
            out.extend_from_slice(&(stroke.brush.code() as u32).to_le_bytes());
            out.extend_from_slice(&(stroke.len() as u32).to_le_bytes());
            out.extend_from_slice(&(stroke.brush.ring_size() as u32).to_le_bytes());
            for i in 0..stroke.len() {
                ring.clear();
                ring_positions(stroke.points(), i, stroke.brush, &mut ring);
                for v in &ring {
                    let w = world.transform_point3(v.as_dvec3());
                    for c in w.to_array() {
                        out.extend_from_slice(&c.to_le_bytes());
                    }
                }
            }
        }
    }
    out
}
