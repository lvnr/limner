//! Prints what one frame of carrying a large selection costs: the core's work, and
//! the triangles the interface has to make again because strokes changed.
//!
//! The drawing has 40,000 short ribbons; every other one (20,000) is selected, so that
//! every chunk holds some selected strokes. The interface side is imitated the way
//! the Godot bridge works: a chunk is turned into triangles the first time it is seen,
//! the selection highlight is made again when the selection or its drawing changes,
//! and a drawing whose strokes are carried is split once per carry into what stays
//! and what moves (`SelectionMotion::split`).
//!
//!   cargo run -p limner-core --example selection_move

use glam::DVec3;
use limner_core::eval::Content;
use limner_core::model::{Brush, Document, Drawing, DrawingId, Point, Stroke, StrokeChunk};
use limner_core::session::tools::ToolKind;
use limner_core::session::{CarriedSplit, HandInput, InputFrame, Session};
use limner_core::tess::tessellate_chunk;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

const STROKES: usize = 40_000;
const POINTS: usize = 16;
const FRAMES: usize = 60;

fn stroke(id: u32, center: DVec3, seed: usize) -> Stroke {
    // Spread over a cube 0.5 m wide, each a short wavy line.
    let f = |k: usize| ((seed * 7919 + k * 104_729) % 1000) as f64 / 1000.0 - 0.5;
    let start = center + DVec3::new(f(1), f(2), f(3)) * 0.5;
    let points = (0..POINTS)
        .map(|i| Point {
            pos: (start + DVec3::new(i as f64 * 0.002, (i as f64 * 0.7).sin() * 0.001, 0.0))
                .as_vec3()
                .to_array(),
            normal: [0.0, 0.0, 1.0],
            dir: [1.0, 0.0, 0.0],
            color: [0.5, 0.4, 0.3],
            opacity: 1.0,
            width: 0.002,
        })
        .collect();
    Stroke::new(id, Brush::Ribbon, false, points)
}

fn hand(position: DVec3, trigger: f32, grip: f32) -> InputFrame {
    InputFrame {
        main: HandInput {
            tracked: true,
            position,
            trigger,
            grip,
            ..HandInput::default()
        },
        dt: 1.0 / 72.0,
        ..InputFrame::default()
    }
}

/// Imitates the bridge's mesh cache: chunks are keyed by identity and turned into
/// triangles when first seen; chunks no longer shown are dropped.
#[derive(Default)]
struct Bridge {
    seen: HashMap<usize, Arc<StrokeChunk>>,
    /// The selection highlight is made again whenever the selection or its drawing
    /// changes, from the selected strokes alone.
    highlight_from: Option<(u64, u64)>,
    /// Drawings split while their strokes are carried, and what they were split for.
    splits: HashMap<DrawingId, ((u64, u64), CarriedSplit)>,
}

impl Bridge {
    /// Brings the cache up to date with what is shown; returns how many chunks it
    /// had to make triangles for.
    fn sync(&mut self, session: &Session) -> usize {
        let frame = session.evaluate();
        let doc = session.doc();
        let carried = session.selection_motion();
        let mut shown: Vec<Arc<StrokeChunk>> = Vec::new();
        let mut split = HashMap::new();
        for state in &frame.layers {
            let Content::Paint { drawing: id, .. } = state.content else {
                continue;
            };
            let Some(drawing) = doc.drawing(id) else {
                continue;
            };
            match carried.as_ref().filter(|c| c.strokes_in(id).is_some()) {
                Some(carried) => {
                    let from = (carried.serial, session.edit_count());
                    let entry = match self.splits.remove(&id) {
                        Some(entry) if entry.0 == from => entry,
                        _ => (from, carried.split(id, drawing).expect("carried")),
                    };
                    shown.extend(entry.1.staying.iter().cloned());
                    shown.extend(entry.1.carried.chunks().iter().cloned());
                    std::hint::black_box(carried.motion.to_affine());
                    split.insert(id, entry);
                }
                None => shown.extend(drawing.chunks().iter().cloned()),
            }
        }
        self.splits = split;
        let mut built = 0;
        let mut next = HashMap::new();
        for chunk in shown {
            let key = Arc::as_ptr(&chunk) as usize;
            if self.seen.remove(&key).is_none() {
                std::hint::black_box(tessellate_chunk(&chunk));
                built += 1;
            }
            next.insert(key, chunk);
        }
        self.seen = next;
        let from = (session.selection().revision(), session.edit_count());
        if self.highlight_from != Some(from) {
            self.highlight_from = Some(from);
            for (drawing, ids) in session.selection().drawings() {
                let Some(drawing) = doc.drawing(drawing) else {
                    continue;
                };
                let selected = Drawing::from_strokes(
                    drawing
                        .strokes()
                        .filter(|s| ids.contains(&s.id))
                        .map(|s| (**s).clone()),
                );
                for chunk in selected.chunks() {
                    std::hint::black_box(tessellate_chunk(chunk));
                    built += 1;
                }
            }
        }
        built
    }
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let mut doc = Document::new();
    let layer = doc
        .walk()
        .into_iter()
        .find(|id| doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
        .expect("a paint layer");
    let drawing = doc
        .layer(layer)
        .and_then(|l| l.as_paint())
        .and_then(|p| p.drawing_at_frame(0))
        .expect("a drawing");
    let here = DVec3::new(0.0, 1.0, -0.5);
    let far = DVec3::new(20.0, 1.0, -0.5);
    {
        let target = doc.drawing_mut(drawing).expect("the drawing");
        for i in 0..STROKES {
            let center = if i % 2 == 0 { here } else { far };
            target.push_stroke(stroke(i as u32, center, i));
        }
    }
    let mut session = Session::new(doc);
    session.view = Default::default();
    let chunks = session.doc().drawing(drawing).unwrap().chunks().len();

    // Select the strokes near here with one press of a large Selection sphere.
    session.tools.select_tool(ToolKind::Select);
    session.tools.select.size = 0.6;
    session.update(&hand(here, 0.0, 0.0));
    session.update(&hand(here, 1.0, 0.0));
    session.update(&hand(here, 0.0, 0.0));
    let selected = session.selection().count();

    let mut bridge = Bridge::default();
    bridge.sync(&session);

    let mut core = Vec::new();
    let mut tess = Vec::new();
    let mut rebuilt = Vec::new();
    let mut step = |session: &mut Session, input: InputFrame| {
        let start = Instant::now();
        session.update(&input);
        let middle = Instant::now();
        let built = bridge.sync(session);
        let end = Instant::now();
        core.push(ms(middle - start));
        tess.push(ms(end - middle));
        rebuilt.push(built);
    };
    for i in 0..=FRAMES {
        let at = here + DVec3::new(0.3, 0.1, 0.0) * (i as f64 / FRAMES as f64);
        step(&mut session, hand(at, 0.0, 1.0));
    }
    step(
        &mut session,
        hand(here + DVec3::new(0.3, 0.1, 0.0), 0.0, 0.0),
    );

    println!(
        "{STROKES} strokes in {chunks} chunks, {selected} selected; undo: {:?}",
        session.history().undo_label()
    );
    let report = |name: &str, range: std::ops::Range<usize>| {
        let n = range.len() as f64;
        let core: f64 = core[range.clone()].iter().sum::<f64>() / n;
        let tess: f64 = tess[range.clone()].iter().sum::<f64>() / n;
        let built: f64 = rebuilt[range].iter().sum::<usize>() as f64 / n;
        println!(
            "{name:>18}: core {core:7.2} ms, triangles {tess:7.2} ms ({built:.1} chunks), total {:7.2} ms",
            core + tess
        );
    };
    report("taking hold", 0..1);
    report("each frame after", 1..FRAMES + 1);
    report("letting go", FRAMES + 1..FRAMES + 2);
}
