//! Prints how long it takes to turn one full chunk of strokes into triangles: the
//! cost a tool pays every frame for each chunk it changes.
//!
//!   cargo run -p limner-core --example tess_cost

use limner_core::model::{Brush, Drawing, Point, Stroke, MAX_CHUNK_VERTICES};
use limner_core::tess::tessellate_chunk;
use std::time::Instant;

fn main() {
    for brush in [Brush::Ribbon, Brush::Cylinder] {
        let points_per_stroke = 60;
        let strokes = MAX_CHUNK_VERTICES / (points_per_stroke * brush.ring_size());
        let drawing = Drawing::from_strokes((0..strokes).map(|s| {
            let points = (0..points_per_stroke)
                .map(|i| Point {
                    pos: [
                        i as f32 * 0.01,
                        s as f32 * 0.01,
                        (i as f32 * 0.3).sin() * 0.02,
                    ],
                    normal: [0.0, 0.0, 1.0],
                    dir: [0.0, 0.0, -1.0],
                    color: [0.5, 0.4, 0.3],
                    opacity: 1.0,
                    width: 0.004,
                })
                .collect();
            Stroke::new(s as u32, brush, false, points)
        }));
        let chunk = &drawing.chunks()[0];
        let rounds = 50;
        let start = Instant::now();
        let mut triangles = 0;
        for _ in 0..rounds {
            triangles = tessellate_chunk(chunk).triangle_count();
        }
        let each = start.elapsed().as_secs_f64() * 1000.0 / rounds as f64;
        println!(
            "{brush:?}: {} strokes, {} vertices, {triangles} triangles: {each:.2} ms per chunk",
            chunk.strokes().len(),
            chunk.vertex_count(),
        );
    }
}
