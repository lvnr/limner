//! The export scene: a document evaluated over a time range, in a form every
//! writer can consume without knowing anything about timelines.
//!
//! [`build_scene`] samples the document with [`limner_core::eval::evaluate`] at each
//! frame time, so easing, pivots, sequence loops, clips and trims are honoured by
//! the one implementation that defines them. What comes out is:
//!
//! * a node tree that mirrors the layer tree (or a flat list when transforms are baked);
//! * per node, a transform, a visibility and an opacity, each either constant or one
//!   value per sampled frame;
//! * per paint node, the distinct drawings it shows in the range, each tessellated
//!   once, with the frames on which it is the one shown.
//!
//! Decisions that every writer inherits are documented on the items below and in
//! `docs/export.md`.

use limner_core::eval::{evaluate, Clock, Content};
use limner_core::glam::{DAffine3, Vec3};
use limner_core::math::Xform;
use limner_core::model::{
    Attenuation, Document, Drawing, DrawingId, Layer, LayerId, LayerKind, LightKind, MediaId,
    Shading, SoundKind, SoundModifier, Stroke, OPAQUE_THRESHOLD,
};
use limner_core::tess::{point_frame, ring_profile};
use limner_core::time::{self, Flicks, FrameRate};
use std::collections::HashMap;
use std::sync::Arc;

/// What to export and how to sample it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportOptions {
    /// First sampled instant.
    pub start: Flicks,
    /// Last instant that may be sampled (inclusive). `start == end` means a still.
    pub end: Flicks,
    /// Sampling rate for animation. Sample `k` is taken at `start + rate.frame_start(k)`.
    pub rate: FrameRate,
    /// False: export the state at `start` only.
    pub animation: bool,
    /// Include layers hidden by their eye toggle. Clips still apply: a layer outside
    /// its clip is hidden whatever this says.
    pub include_hidden: bool,
    /// Flatten the hierarchy into world-space meshes.
    pub bake_transforms: bool,
    pub lights: bool,
    pub cameras: bool,
}

impl ExportOptions {
    /// The state at one instant, hierarchy kept, lights and cameras included.
    pub fn still(time: Flicks) -> Self {
        Self {
            start: time,
            end: time,
            rate: FrameRate::default(),
            animation: false,
            include_hidden: false,
            bake_transforms: false,
            lights: true,
            cameras: true,
        }
    }

    /// Animation from `start` to `end` inclusive, sampled at `rate`.
    pub fn animated(start: Flicks, end: Flicks, rate: FrameRate) -> Self {
        Self {
            end,
            rate,
            animation: true,
            ..Self::still(start)
        }
    }

    /// Animation over the document's working range at the document's frame rate.
    pub fn working_range(doc: &Document) -> Self {
        Self::animated(
            doc.settings.range_start,
            doc.settings.range_end,
            doc.settings.rate,
        )
    }

    /// How many instants the range holds at the rate, counted without listing them.
    /// The exporters refuse more than [`MAX_FRAMES`].
    pub fn frame_count(&self) -> u64 {
        if !self.animation || self.end <= self.start || !self.rate.is_valid() {
            return 1;
        }
        let span = self.end.saturating_sub(self.start);
        (self.rate.frame_at(span).max(0) as u64).saturating_add(1)
    }

    /// The instants [`build_scene`] samples. Never empty; a single instant means a still.
    /// Never more than [`MAX_FRAMES`]: a range that holds more is cut short here, and
    /// refused by the `export_*` functions.
    pub fn sample_times(&self) -> Vec<Flicks> {
        if !self.animation || self.end <= self.start || !self.rate.is_valid() {
            return vec![self.start];
        }
        let count = self.frame_count().min(MAX_FRAMES as u64);
        let mut times: Vec<Flicks> = Vec::with_capacity(count as usize);
        for frame in 0..count {
            let t = self
                .start
                .saturating_add(self.rate.frame_start(frame as i64));
            // A range that ends at the very last representable instant stops growing
            // when the addition saturates.
            if t > self.end || times.last().is_some_and(|last| t <= *last) {
                break;
            }
            times.push(t);
        }
        times
    }
}

/// The most frames an export samples. Every frame evaluates the whole document, so a
/// slip in the rate or the range (24,000 frames per second, a range of hours) would
/// otherwise keep the machine busy for minutes; 100,000 frames is over 23 minutes at
/// 72 frames per second.
pub const MAX_FRAMES: usize = 100_000;

impl Default for ExportOptions {
    fn default() -> Self {
        Self::still(0)
    }
}

/// A value that is either the same on every sampled frame or given per frame.
#[derive(Clone, Debug, PartialEq)]
pub enum Sampled<T> {
    Constant(T),
    /// One value per entry of [`ExportScene::times`]. At least two of them differ.
    PerFrame(Vec<T>),
}

impl<T: Clone + PartialEq> Sampled<T> {
    /// Collapses samples that are all equal into a constant. `samples` must not be empty.
    pub fn from_samples(samples: Vec<T>) -> Self {
        let first = &samples[0];
        if samples.iter().all(|s| s == first) {
            Sampled::Constant(first.clone())
        } else {
            Sampled::PerFrame(samples)
        }
    }

    /// The value on `frame`. Frames past the end repeat the last sample.
    pub fn at(&self, frame: usize) -> &T {
        match self {
            Sampled::Constant(value) => value,
            Sampled::PerFrame(samples) => &samples[frame.min(samples.len() - 1)],
        }
    }

    /// The value on the first sampled frame, which is also the rest state writers use.
    pub fn first(&self) -> &T {
        self.at(0)
    }

    pub fn is_animated(&self) -> bool {
        matches!(self, Sampled::PerFrame(_))
    }
}

/// Triangles of one kind of surface, with everything a writer needs per vertex.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExportMesh {
    /// Metres. Layer space, or world space once the mesh has been baked.
    pub positions: Vec<[f32; 3]>,
    /// Unit length. Outward for tubes; the stored normal's side for flat strokes.
    pub normals: Vec<[f32; 3]>,
    /// Linear RGB as painted (not clamped) and opacity in `0..=1`.
    pub colors: Vec<[f32; 4]>,
    /// Triangle list, counter-clockwise seen from outside (from the normal's side for
    /// flat strokes).
    pub indices: Vec<u32>,
    /// Flat strokes (ribbons and legacy lines) are visible from both sides.
    pub double_sided: bool,
    /// At least one vertex is see-through, so the mesh needs alpha.
    pub has_transparency: bool,
}

impl ExportMesh {
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// Moves the mesh through `m`. Normals go through the inverse transpose and are
    /// renormalised; when `m` mirrors (negative determinant) every triangle's winding
    /// is reversed so that faces stay counter-clockwise seen from outside.
    pub fn transform(&mut self, m: &DAffine3) {
        let normal_matrix = m.matrix3.inverse().transpose();
        for p in &mut self.positions {
            *p = m
                .transform_point3(Vec3::from_array(*p).as_dvec3())
                .as_vec3()
                .to_array();
        }
        for n in &mut self.normals {
            let original = Vec3::from_array(*n);
            *n = (normal_matrix * original.as_dvec3())
                .as_vec3()
                .normalize_or(original)
                .to_array();
        }
        if m.matrix3.determinant() < 0.0 {
            for triangle in self.indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
    }
}

/// The kind of layer a node stands for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NodeKind {
    Group,
    Sequence,
    Paint,
    Picture,
    Model,
    Sound,
    Viewpoint,
    Camera,
    Light,
}

impl NodeKind {
    pub fn of(layer: &Layer) -> Self {
        match &layer.kind {
            LayerKind::Group(group) if group.sequence.is_some() => NodeKind::Sequence,
            LayerKind::Group(_) => NodeKind::Group,
            LayerKind::Paint(_) => NodeKind::Paint,
            LayerKind::Picture(_) => NodeKind::Picture,
            LayerKind::Model(_) => NodeKind::Model,
            LayerKind::Sound(_) => NodeKind::Sound,
            LayerKind::Viewpoint(_) => NodeKind::Viewpoint,
            LayerKind::Camera(_) => NodeKind::Camera,
            LayerKind::Light(_) => NodeKind::Light,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            NodeKind::Group => "Group",
            NodeKind::Sequence => "Sequence",
            NodeKind::Paint => "Paint",
            NodeKind::Picture => "Picture",
            NodeKind::Model => "Model",
            NodeKind::Sound => "Sound",
            NodeKind::Viewpoint => "Viewpoint",
            NodeKind::Camera => "Camera",
            NodeKind::Light => "Light",
        }
    }
}

/// One drawing of a paint layer.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportDrawing {
    pub drawing: DrawingId,
    /// Position in the layer's list of drawings; stable across exports of the same document.
    pub ordinal: u32,
    /// Whether this is the drawing the layer shows, per frame. The drawing is on screen
    /// on a frame exactly when this is true and the node and all its ancestors are
    /// visible. Frames on which the layer is hidden repeat the neighbouring value, so
    /// hiding a layer does not add changes here.
    pub shown: Sampled<bool>,
    /// Indices into [`ExportScene::meshes`]: up to four, split by sidedness and by
    /// whether the strokes are see-through. Opaque meshes come first.
    pub meshes: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportPaint {
    pub shading: Shading,
    /// The distinct drawings that are on screen on at least one sampled frame and
    /// have at least one triangle, in the layer's own order.
    pub drawings: Vec<ExportDrawing>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportLight {
    pub kind: LightKind,
    /// Linear RGB.
    pub color: [f32; 3],
    /// Limner's unit-less light energy.
    pub energy: f32,
    /// Reach of point and spot lights in layer units, as stored.
    pub range: f32,
    /// The same reach in metres of world space on the first frame (`range` times the
    /// layer's world scale). Formats whose lights ignore node scale want this one.
    pub world_range: f32,
    /// Half-angle of a spot light's cone, in degrees.
    pub spot_angle: f32,
    /// Softness of the cone edge, 0 to 1.
    pub spot_blend: f32,
    pub shadows: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportCamera {
    /// Vertical field of view in degrees.
    pub fov: f32,
    /// The document's camera resolution in pixels; its ratio is the aspect ratio.
    pub resolution: [u32; 2],
}

/// A picture or model layer: the media it shows, by reference.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportMedia {
    pub media: Option<MediaId>,
    /// The media item's file name, or empty when the layer has no media.
    pub name: String,
    /// The path the media was imported from.
    pub source_path: String,
}

/// A sound layer's audio as a file: the encoding it was imported in, or WAV for
/// audio the document holds as samples.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportAudio {
    /// The encoding as a lower-case file extension: `wav`, `mp3` or `ogg`.
    pub format: String,
    /// The whole file.
    pub bytes: Arc<[u8]>,
    /// Length in seconds, when it can be read without decoding (WAV files).
    pub duration: Option<f64>,
}

/// A stretch of the range over which a sound plays on without a jump: from the
/// frame its clip starts (or the range starts) to the frame before it stops, loops
/// back with its sequence, or runs out of audio.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportPlay {
    /// First sampled frame on which the sound is heard.
    pub first: usize,
    /// Last sampled frame on which it is heard.
    pub last: usize,
    /// Seconds into the audio on `first`. For a looping sound of known length this is
    /// already reduced to a place inside the audio.
    pub offset: f64,
}

/// A sound layer: its audio, how it is played, and when it is heard in the range.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportSound {
    /// The media item, by reference, as for pictures and models.
    pub media: ExportMedia,
    /// The audio itself; `None` when the layer has no media or holds nothing a file
    /// can carry.
    pub audio: Option<ExportAudio>,
    pub kind: SoundKind,
    /// Limner's linear volume multiplier. The layer's opacity multiplies it as well,
    /// as it does when Limner plays the sound.
    pub gain: f32,
    pub looped: bool,
    /// Fade with distance; radii in layer units.
    pub attenuation: Attenuation,
    /// Directional shaping; angles in radians.
    pub modifier: SoundModifier,
    /// When the sound is heard, in order. Empty when it is never heard in the range.
    pub plays: Vec<ExportPlay>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NodeContent {
    /// Groups, sequences and spawn areas carry nothing a file format can hold.
    None,
    Paint(ExportPaint),
    Light(ExportLight),
    Camera(ExportCamera),
    Media(ExportMedia),
    Sound(ExportSound),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExportNode {
    pub layer: LayerId,
    pub name: String,
    pub kind: NodeKind,
    /// Index of the parent node, or `None` for a root.
    pub parent: Option<usize>,
    pub children: Vec<usize>,
    /// Hierarchy kept: the layer's own transform, relative to its parent.
    /// Baked: the world transform, which is the identity for paint nodes whose
    /// vertices were moved into world space.
    pub transform: Sampled<Xform>,
    /// Hierarchy kept: the node's own visibility, which is whether it is inside one of
    /// its clips; ancestors apply on top. (The eye toggle never shows up here: a layer
    /// with it off is either left out or, with `include_hidden`, treated as on.)
    /// Baked: the effective visibility, ancestors included.
    pub visible: Sampled<bool>,
    /// Effective opacity: the layer's own opacity times that of every ancestor.
    /// No interchange format inherits opacity, so it is resolved here.
    pub opacity: Sampled<f32>,
    pub content: NodeContent,
}

/// A document evaluated for export.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportScene {
    /// The document title, possibly empty.
    pub name: String,
    pub rate: FrameRate,
    /// The sampled instants. One entry means a still.
    pub times: Vec<Flicks>,
    /// True when the hierarchy was flattened.
    pub baked: bool,
    /// Linear RGB background of the document.
    pub background: [f32; 3],
    /// Indices of the nodes without a parent.
    pub roots: Vec<usize>,
    /// Parents come before their children.
    pub nodes: Vec<ExportNode>,
    pub meshes: Vec<ExportMesh>,
}

impl ExportScene {
    pub fn frame_count(&self) -> usize {
        self.times.len()
    }

    pub fn is_animated(&self) -> bool {
        self.times.len() > 1
    }

    /// Seconds from the first sampled instant to `frame`.
    pub fn seconds(&self, frame: usize) -> f64 {
        time::to_seconds(self.times[frame] - self.times[0])
    }

    pub fn vertex_count(&self) -> usize {
        self.meshes.iter().map(ExportMesh::vertex_count).sum()
    }

    pub fn triangle_count(&self) -> usize {
        self.meshes.iter().map(ExportMesh::triangle_count).sum()
    }

    /// The node that stands for `layer`.
    pub fn find_layer(&self, layer: LayerId) -> Option<usize> {
        self.nodes.iter().position(|n| n.layer == layer)
    }

    /// The node's placement in world space on `frame`, composed up the parent chain.
    pub fn world_transform(&self, node: usize, frame: usize) -> DAffine3 {
        let mut m = self.nodes[node].transform.at(frame).to_affine();
        let mut current = self.nodes[node].parent;
        while let Some(parent) = current {
            m = self.nodes[parent].transform.at(frame).to_affine() * m;
            current = self.nodes[parent].parent;
        }
        m
    }

    /// True when the node and all its ancestors are visible on `frame`.
    pub fn is_visible(&self, node: usize, frame: usize) -> bool {
        let mut current = Some(node);
        while let Some(index) = current {
            if !*self.nodes[index].visible.at(frame) {
                return false;
            }
            current = self.nodes[index].parent;
        }
        true
    }
}

fn is_see_through(stroke: &Stroke) -> bool {
    stroke.points().iter().any(|p| p.opacity < OPAQUE_THRESHOLD)
}

fn append_stroke(stroke: &Stroke, out: &mut ExportMesh) {
    let points = stroke.points();
    let profile = ring_profile(stroke.brush);
    let ring = profile.len();
    let first_vertex = out.positions.len();
    let first_index = out.indices.len();
    let base = first_vertex as u32;
    let mut finite = true;

    for (i, point) in points.iter().enumerate() {
        let frame = point_frame(points, i);
        let centre = point.position();
        let sanitise = |c: f32| if c.is_finite() { c } else { 0.0 };
        let color = [
            sanitise(point.color[0]),
            sanitise(point.color[1]),
            sanitise(point.color[2]),
            sanitise(point.opacity).clamp(0.0, 1.0),
        ];
        for vertex in profile {
            let position = centre
                + frame.u * (vertex.offset[0] * point.width)
                + frame.v * (vertex.offset[1] * point.width);
            let normal =
                (frame.u * vertex.normal[0] + frame.v * vertex.normal[1]).normalize_or(frame.v);
            finite &= position.is_finite() && normal.is_finite();
            out.positions.push(position.to_array());
            out.normals.push(normal.to_array());
            out.colors.push(color);
        }
    }

    // A ribbon has one strip between its two edges; a tube has one per side of the ring.
    let sides = if ring == 2 { 1 } else { ring };
    for i in 0..points.len() - 1 {
        let row = base + (i * ring) as u32;
        for k in 0..sides {
            let a = row + k as u32;
            let b = row + ((k + 1) % ring) as u32;
            let c = a + ring as u32;
            let d = b + ring as u32;
            out.indices.extend_from_slice(&[a, b, c, b, d, c]);
        }
    }

    if !finite {
        // A stroke with a broken point would poison bounds and normals; leave it out whole.
        out.positions.truncate(first_vertex);
        out.normals.truncate(first_vertex);
        out.colors.truncate(first_vertex);
        out.indices.truncate(first_index);
    }
}

/// Tessellates a whole drawing for export, in layer space.
///
/// The geometry is the renderer's: one ring per stored point from
/// [`limner_core::tess::ring_profile`] on [`limner_core::tess::point_frame`], with the
/// same triangle order. The differences from [`limner_core::tess::tessellate_chunk`] are:
///
/// * one mesh set per drawing instead of per chunk, with real layer-space positions
///   (no chunk origin);
/// * colours stay linear floating point;
/// * a stroke counts as see-through only by its opacity. Directional strokes fade
///   with the viewing direction in Limner, which no interchange format can express,
///   so an opaque directional stroke is exported opaque;
/// * strokes that were erased completely (every point at zero opacity), strokes with
///   fewer than two points and strokes with a non-finite point produce nothing.
///
/// Meshes without triangles are dropped, and opaque meshes come first.
pub fn tessellate_drawing(drawing: &Drawing) -> Vec<ExportMesh> {
    let surface = |double_sided: bool, has_transparency: bool| ExportMesh {
        double_sided,
        has_transparency,
        ..ExportMesh::default()
    };
    let mut surfaces = [
        surface(false, false),
        surface(true, false),
        surface(false, true),
        surface(true, true),
    ];
    for stroke in drawing.strokes() {
        if stroke.len() < 2 || stroke.is_fully_erased() {
            continue;
        }
        let index = usize::from(stroke.brush.is_flat()) + 2 * usize::from(is_see_through(stroke));
        append_stroke(stroke, &mut surfaces[index]);
    }
    surfaces
        .into_iter()
        .filter(|mesh| !mesh.indices.is_empty())
        .collect()
}

/// What was sampled for one layer, before nodes are built.
struct LayerSamples {
    id: LayerId,
    parent: Option<usize>,
    /// The layer's own transform per frame.
    local: Vec<Xform>,
    /// World placement per frame.
    world: Vec<DAffine3>,
    /// The layer's own visibility per frame.
    own_visible: Vec<bool>,
    /// Visibility including ancestors.
    visible: Vec<bool>,
    /// Opacity including ancestors.
    opacity: Vec<f32>,
    /// The drawing a paint layer shows, per frame.
    drawing: Vec<Option<DrawingId>>,
    /// Time since the layer's clip began, including its trim, per frame.
    local_time: Vec<Flicks>,
}

fn collect_layers(
    doc: &Document,
    id: LayerId,
    parent: Option<usize>,
    options: &ExportOptions,
    out: &mut Vec<LayerSamples>,
) {
    let Some(layer) = doc.layer(id) else {
        return;
    };
    if !layer.visible && !options.include_hidden {
        return;
    }
    match layer.kind {
        LayerKind::Light(_) if !options.lights => return,
        LayerKind::Camera(_) if !options.cameras => return,
        _ => {}
    }
    let index = out.len();
    out.push(LayerSamples {
        id,
        parent,
        local: Vec::new(),
        world: Vec::new(),
        own_visible: Vec::new(),
        visible: Vec::new(),
        opacity: Vec::new(),
        drawing: Vec::new(),
        local_time: Vec::new(),
    });
    for child in layer.children() {
        collect_layers(doc, *child, Some(index), options, out);
    }
}

/// Fills frames on which the layer is hidden with the neighbouring cared-about value.
fn shown_schedule(visible: &[bool], selected: &[bool]) -> Vec<bool> {
    let first = visible
        .iter()
        .zip(selected)
        .find(|(v, _)| **v)
        .is_some_and(|(_, s)| *s);
    let mut last = first;
    visible
        .iter()
        .zip(selected)
        .map(|(v, s)| {
            if *v {
                last = *s;
            }
            last
        })
        .collect()
}

/// Evaluates `doc` over the range in `options` into a format-independent scene.
///
/// Decisions:
///
/// * **Sampling.** Frame `k` is evaluated at `start + rate.frame_start(k)` for as long
///   as that is not past `end`. With `animation` off, an empty range or an invalid
///   rate, only `start` is evaluated.
/// * **Hidden layers.** A layer whose eye toggle is off is left out together with its
///   subtree unless `include_hidden` is set; then the toggle is ignored. Clips always
///   apply.
/// * **What becomes a node.** With the hierarchy kept, every remaining layer does,
///   except paint layers that show no triangle anywhere in the range. Lights and
///   cameras are left out when their option is off. With `bake_transforms`, groups
///   and spawn areas are dropped as well and every node is a root; sounds stay, on
///   their world transform, since where a sound is matters to formats that play it.
/// * **Sounds.** A sound node carries its audio file and the stretches of the range
///   in which it is heard ([`ExportPlay`]), worked out from clips, trims, sequence
///   loops and, for WAV audio, the audio's length.
/// * **Drawings.** A paint node lists the drawings that are really on screen on at
///   least one sampled frame, each tessellated once. Two layers showing the same
///   drawing share its meshes unless the meshes were baked.
/// * **Baking.** A paint layer whose world transform is the same on every sampled
///   frame has its vertices moved into world space and an identity node transform; a
///   mirrored one gets its winding reversed so faces still point outward. A layer
///   whose world transform changes cannot be baked into static vertices, so its node
///   carries the sampled world transform instead and its mesh stays in layer space.
/// * **Mirrors with the hierarchy kept.** The node keeps the layer's flip in its
///   transform and the winding is left alone; consumers reverse the facing of
///   negatively scaled nodes, as glTF and USD specify.
pub fn build_scene(doc: &Document, options: &ExportOptions) -> ExportScene {
    let times = options.sample_times();
    let frames = times.len();

    let mut layers = Vec::new();
    collect_layers(doc, doc.root(), None, options, &mut layers);

    for time in &times {
        let frame = evaluate(doc, Clock::at(*time));
        for index in 0..layers.len() {
            let (parent_visible, id) = {
                let samples = &layers[index];
                let parent_visible = samples
                    .parent
                    .is_none_or(|p| *layers[p].visible.last().expect("parents come first"));
                (parent_visible, samples.id)
            };
            let samples = &mut layers[index];
            match frame.get(id) {
                Some(state) => {
                    samples.local.push(state.local);
                    samples.world.push(state.world);
                    samples.own_visible.push(state.in_clip);
                    samples.visible.push(parent_visible && state.in_clip);
                    samples.opacity.push(state.opacity);
                    samples.drawing.push(match state.content {
                        Content::Paint { drawing, .. } => Some(drawing),
                        Content::None => None,
                    });
                    samples.local_time.push(state.local_time);
                }
                None => {
                    samples.local.push(Xform::IDENTITY);
                    samples.world.push(DAffine3::IDENTITY);
                    samples.own_visible.push(false);
                    samples.visible.push(false);
                    samples.opacity.push(1.0);
                    samples.drawing.push(None);
                    samples.local_time.push(0);
                }
            }
        }
    }

    let mut scene = ExportScene {
        name: doc.settings.title.clone(),
        rate: options.rate,
        times,
        baked: options.bake_transforms,
        background: doc.settings.background,
        roots: Vec::new(),
        nodes: Vec::new(),
        meshes: Vec::new(),
    };
    // Layer-space meshes by drawing, shared between layers that show the same drawing.
    let mut shared_meshes: HashMap<DrawingId, Vec<usize>> = HashMap::new();
    // Node index of each sampled layer, when it became a node.
    let mut node_of: Vec<Option<usize>> = vec![None; layers.len()];
    // Whether a light or camera that is exported lies somewhere below each layer.
    // Children come after their parents, so walking backwards sees them first.
    let mut holds_lens_or_lamp = vec![false; layers.len()];
    for index in (0..layers.len()).rev() {
        let is_one = doc
            .layer(layers[index].id)
            .is_some_and(|l| matches!(l.kind, LayerKind::Light(_) | LayerKind::Camera(_)));
        if let (true, Some(parent)) = (holds_lens_or_lamp[index] || is_one, layers[index].parent) {
            holds_lens_or_lamp[parent] = true;
        }
    }

    for (index, samples) in layers.iter().enumerate() {
        let Some(layer) = doc.layer(samples.id) else {
            continue;
        };
        let kind = NodeKind::of(layer);
        let world_is_static = samples.world.iter().all(|w| *w == samples.world[0]);
        // Limner does not draw a layer at opacity zero, so on such a frame it is not on
        // screen, whatever its clips say.
        let seen: Vec<bool> = (0..frames)
            .map(|k| samples.visible[k] && samples.opacity[k] > 0.0)
            .collect();
        let bake_vertices = options.bake_transforms && world_is_static;

        let content = match &layer.kind {
            LayerKind::Paint(paint) => {
                let mut drawings = Vec::new();
                for (ordinal, id) in paint.drawings.iter().enumerate() {
                    if paint.drawings[..ordinal].contains(id) {
                        continue;
                    }
                    let selected: Vec<bool> =
                        samples.drawing.iter().map(|d| *d == Some(*id)).collect();
                    let on_screen = (0..frames).any(|k| seen[k] && selected[k]);
                    if !on_screen {
                        continue;
                    }
                    let Some(drawing) = doc.drawing(*id) else {
                        continue;
                    };
                    let meshes = if bake_vertices {
                        let mut indices = Vec::new();
                        for mut mesh in tessellate_drawing(drawing) {
                            mesh.transform(&samples.world[0]);
                            indices.push(scene.meshes.len());
                            scene.meshes.push(mesh);
                        }
                        indices
                    } else {
                        shared_meshes
                            .entry(*id)
                            .or_insert_with(|| {
                                let mut indices = Vec::new();
                                for mesh in tessellate_drawing(drawing) {
                                    indices.push(scene.meshes.len());
                                    scene.meshes.push(mesh);
                                }
                                indices
                            })
                            .clone()
                    };
                    if meshes.is_empty() {
                        continue;
                    }
                    drawings.push(ExportDrawing {
                        drawing: *id,
                        ordinal: ordinal as u32,
                        shown: Sampled::from_samples(shown_schedule(&seen, &selected)),
                        meshes,
                    });
                }
                if drawings.is_empty() {
                    continue;
                }
                NodeContent::Paint(ExportPaint {
                    shading: paint.shading,
                    drawings,
                })
            }
            LayerKind::Light(light) => {
                let world_scale = Xform::from_affine(&samples.world[0]).scale;
                NodeContent::Light(ExportLight {
                    kind: light.kind,
                    color: light.color,
                    energy: light.energy,
                    range: light.range,
                    world_range: (light.range as f64 * world_scale) as f32,
                    spot_angle: light.spot_angle,
                    spot_blend: light.spot_blend,
                    shadows: light.shadows,
                })
            }
            LayerKind::Camera(camera) => NodeContent::Camera(ExportCamera {
                fov: camera.fov,
                resolution: doc.settings.camera_resolution,
            }),
            LayerKind::Picture(picture) => NodeContent::Media(ExportMedia {
                media: picture.media,
                name: media_name(doc, picture.media),
                source_path: picture.source_path.clone(),
            }),
            LayerKind::Model(model) => NodeContent::Media(ExportMedia {
                media: model.media,
                name: media_name(doc, model.media),
                source_path: model.source_path.clone(),
            }),
            LayerKind::Sound(sound) => {
                let audio = sound
                    .media
                    .and_then(|id| doc.media(id))
                    .and_then(|media| crate::audio::export_audio(media));
                let plays = sound_plays(
                    &scene.times,
                    &samples.visible,
                    &samples.local_time,
                    sound.looped,
                    audio.as_ref().and_then(|a| a.duration),
                );
                NodeContent::Sound(ExportSound {
                    media: ExportMedia {
                        media: sound.media,
                        name: media_name(doc, sound.media),
                        source_path: sound.source_path.clone(),
                    },
                    audio,
                    kind: sound.kind,
                    gain: sound.gain,
                    looped: sound.looped,
                    attenuation: sound.attenuation,
                    modifier: sound.modifier,
                    plays,
                })
            }
            LayerKind::Group(_) | LayerKind::Viewpoint(_) => NodeContent::None,
        };
        // A layer at opacity zero throughout shows nothing and, for a sound, is silent;
        // nor does anything under it, since opacity multiplies down the tree. Paint
        // layers were already dropped for having nothing on screen. Lights and cameras
        // ignore opacity, so they stay, and so do the groups they need.
        let transparent = samples.opacity.iter().all(|o| *o <= 0.0);
        let ignores_opacity = matches!(content, NodeContent::Light(_) | NodeContent::Camera(_));
        if transparent && !ignores_opacity && !holds_lens_or_lamp[index] {
            continue;
        }
        if options.bake_transforms && content == NodeContent::None {
            continue;
        }

        let (parent, transform, visible) = if options.bake_transforms {
            let transform = if bake_vertices && matches!(content, NodeContent::Paint(_)) {
                Sampled::Constant(Xform::IDENTITY)
            } else {
                Sampled::from_samples(samples.world.iter().map(Xform::from_affine).collect())
            };
            (None, transform, samples.visible.clone())
        } else {
            (
                samples.parent.and_then(|p| node_of[p]),
                Sampled::from_samples(samples.local.clone()),
                samples.own_visible.clone(),
            )
        };

        let node = scene.nodes.len();
        node_of[index] = Some(node);
        match parent {
            Some(parent) => scene.nodes[parent].children.push(node),
            None => scene.roots.push(node),
        }
        scene.nodes.push(ExportNode {
            layer: samples.id,
            name: layer.name.clone(),
            kind,
            parent,
            children: Vec::new(),
            transform,
            visible: Sampled::from_samples(visible),
            opacity: Sampled::from_samples(samples.opacity.clone()),
            content,
        });
    }
    scene
}

/// When a sound is heard, from its layer's samples. A frame counts when the layer and
/// its ancestors are inside their clips and the sound has started (its time since
/// the clip began is not negative) and, unless it loops, has not run out. Two heard
/// frames in a row belong to the same play when the sound's own time advanced by
/// exactly the time between them; a sequence looping back starts a new play. This
/// is the rule Limner's player follows.
fn sound_plays(
    times: &[Flicks],
    visible: &[bool],
    local: &[Flicks],
    looped: bool,
    duration: Option<f64>,
) -> Vec<ExportPlay> {
    let heard = |k: usize| {
        let seconds = time::to_seconds(local[k]);
        visible[k]
            && local[k] >= 0
            && match duration {
                Some(length) if length <= 0.0 => false,
                Some(length) if !looped => seconds < length,
                _ => true,
            }
    };
    let mut plays: Vec<ExportPlay> = Vec::new();
    for k in 0..times.len() {
        if !heard(k) {
            continue;
        }
        let continues = k > 0
            && plays.last().is_some_and(|p| p.last == k - 1)
            && local[k] - local[k - 1] == times[k] - times[k - 1];
        if continues {
            plays.last_mut().expect("checked above").last = k;
            continue;
        }
        let mut offset = time::to_seconds(local[k]);
        if let (true, Some(length)) = (looped, duration) {
            offset = offset.rem_euclid(length);
        }
        plays.push(ExportPlay {
            first: k,
            last: k,
            offset,
        });
    }
    plays
}

fn media_name(doc: &Document, media: Option<MediaId>) -> String {
    media
        .and_then(|id| doc.media(id))
        .map(|m| m.name.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_times_cover_the_inclusive_range() {
        let rate = FrameRate::whole(24);
        let d = rate.frame_duration();
        let options = ExportOptions::animated(5 * d, 8 * d, rate);
        assert_eq!(options.sample_times(), vec![5 * d, 6 * d, 7 * d, 8 * d]);
        // The end is inclusive only when a frame lands on it.
        let short = ExportOptions::animated(0, 3 * d - 1, rate);
        assert_eq!(short.sample_times(), vec![0, d, 2 * d]);
        // A still, an empty range and a switched-off animation all give one instant.
        assert_eq!(ExportOptions::still(7).sample_times(), vec![7]);
        assert_eq!(
            ExportOptions::animated(9, 3, rate).sample_times(),
            vec![9],
            "a reversed range is a still at its start"
        );
        let mut off = ExportOptions::animated(0, 10 * d, rate);
        off.animation = false;
        assert_eq!(off.sample_times(), vec![0]);
        let invalid = ExportOptions::animated(0, 10 * d, FrameRate::new(0, 1));
        assert_eq!(invalid.sample_times(), vec![0]);
    }

    #[test]
    fn sampling_ends_at_the_end_of_time_and_at_the_frame_limit() {
        let rate = FrameRate::whole(24);
        let d = rate.frame_duration();
        let last = ExportOptions::animated(Flicks::MAX - 2 * d, Flicks::MAX, rate);
        assert_eq!(last.frame_count(), 3);
        assert_eq!(
            last.sample_times(),
            vec![Flicks::MAX - 2 * d, Flicks::MAX - d, Flicks::MAX]
        );
        let endless = ExportOptions::animated(0, Flicks::MAX, rate);
        assert!(endless.frame_count() > MAX_FRAMES as u64);
        let times = endless.sample_times();
        assert_eq!(times.len(), MAX_FRAMES, "cut short at the limit");
        assert_eq!(
            *times.last().unwrap(),
            rate.frame_start(MAX_FRAMES as i64 - 1)
        );
        assert_eq!(ExportOptions::still(Flicks::MAX).frame_count(), 1);
    }

    #[test]
    fn sampled_values_collapse_when_constant() {
        assert_eq!(Sampled::from_samples(vec![3, 3, 3]), Sampled::Constant(3));
        let varying = Sampled::from_samples(vec![1, 2, 2]);
        assert!(varying.is_animated());
        assert_eq!(*varying.first(), 1);
        assert_eq!(*varying.at(2), 2);
        assert_eq!(*varying.at(99), 2, "past the end repeats the last sample");
    }

    #[test]
    fn hidden_frames_do_not_add_changes_to_a_drawing_schedule() {
        // Visible on frames 2..=4 only, showing this drawing throughout.
        let visible = [false, false, true, true, true, false];
        let selected = [false, false, true, true, true, false];
        assert_eq!(shown_schedule(&visible, &selected), vec![true; 6]);
        // Visible throughout, alternating.
        let all = [true; 4];
        let alternating = [true, false, true, false];
        assert_eq!(shown_schedule(&all, &alternating), alternating.to_vec());
        // Never visible: never shown.
        assert_eq!(shown_schedule(&[false; 3], &[true; 3]), vec![false; 3]);
    }

    #[test]
    fn a_sound_plays_while_its_clip_lasts_and_its_audio_runs() {
        let d = FrameRate::whole(24).frame_duration();
        let times: Vec<Flicks> = (0..8).map(|k| k * d).collect();
        // In a clip from frame 2 to frame 5; the layer's time starts there.
        let visible = [false, false, true, true, true, true, false, false];
        let local: Vec<Flicks> = (0..8)
            .map(|k| if (2..6).contains(&k) { (k - 2) * d } else { 0 })
            .collect();
        let play = |first, last, offset| ExportPlay {
            first,
            last,
            offset,
        };
        assert_eq!(
            sound_plays(&times, &visible, &local, false, None),
            vec![play(2, 5, 0.0)]
        );
        // Audio two frames long stops after two frames unless it loops.
        let two = time::to_seconds(2 * d);
        assert_eq!(
            sound_plays(&times, &visible, &local, false, Some(two)),
            vec![play(2, 3, 0.0)]
        );
        assert_eq!(
            sound_plays(&times, &visible, &local, true, Some(two)),
            vec![play(2, 5, 0.0)]
        );
        // Audio of no length is never heard.
        assert!(sound_plays(&times, &visible, &local, true, Some(0.0)).is_empty());
    }

    #[test]
    fn a_jump_in_the_sound_s_own_time_starts_a_new_play() {
        let d = FrameRate::whole(24).frame_duration();
        let times: Vec<Flicks> = (0..6).map(|k| k * d).collect();
        // A sequence looping every three frames, entered a frame and a half late.
        let local: Vec<Flicks> = (0..6).map(|k| (k % 3) * d + d / 2).collect();
        let plays = sound_plays(&times, &[true; 6], &local, false, None);
        assert_eq!(plays.len(), 2);
        assert_eq!((plays[0].first, plays[0].last), (0, 2));
        assert_eq!((plays[1].first, plays[1].last), (3, 5));
        assert_eq!(plays[1].offset, time::to_seconds(d / 2));
        // Not started yet: frames with a negative time are silent.
        let early: Vec<Flicks> = (0..6).map(|k| (k - 2) * d).collect();
        let plays = sound_plays(&times, &[true; 6], &early, false, None);
        assert_eq!(
            (plays[0].first, plays[0].last, plays[0].offset),
            (2, 5, 0.0)
        );
        // A looping sound entered part way through starts inside its audio.
        let late: Vec<Flicks> = (0..6).map(|k| (k + 7) * d).collect();
        let three = time::to_seconds(3 * d);
        let plays = sound_plays(&times, &[true; 6], &late, true, Some(three));
        assert_eq!(plays.len(), 1);
        assert!((plays[0].offset - time::to_seconds(d)).abs() < 1e-12);
    }
}
