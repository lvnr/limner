//! Layers: the nodes of the document tree.

use super::ids::{DrawingId, LayerId, MediaId};
use super::keys::{Key, Track};
use crate::math::Xform;
use crate::time::{Flicks, FrameRate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub id: LayerId,
    pub parent: Option<LayerId>,
    pub name: String,
    /// The eye toggle. Clips (visibility keys) are a separate, animated condition.
    pub visible: bool,
    pub locked: bool,
    pub collapsed: bool,
    pub show_bounds: bool,
    /// Opacity for visual layers, volume for sound layers.
    pub opacity: f32,
    /// Placement relative to the parent layer.
    pub transform: Xform,
    /// The point and frame, in layer space, that keyed transforms turn about.
    pub pivot: Xform,
    pub anim: LayerAnim,
    pub kind: LayerKind,
}

impl Layer {
    pub fn new(id: LayerId, name: impl Into<String>, kind: LayerKind) -> Self {
        Self {
            id,
            parent: None,
            name: name.into(),
            visible: true,
            locked: false,
            collapsed: false,
            show_bounds: false,
            opacity: 1.0,
            transform: Xform::IDENTITY,
            pivot: Xform::IDENTITY,
            anim: LayerAnim::default(),
            kind,
        }
    }

    pub fn children(&self) -> &[LayerId] {
        match &self.kind {
            LayerKind::Group(group) => &group.children,
            _ => &[],
        }
    }

    pub fn as_group(&self) -> Option<&Group> {
        match &self.kind {
            LayerKind::Group(group) => Some(group),
            _ => None,
        }
    }

    pub fn as_group_mut(&mut self) -> Option<&mut Group> {
        match &mut self.kind {
            LayerKind::Group(group) => Some(group),
            _ => None,
        }
    }

    pub fn as_paint(&self) -> Option<&Paint> {
        match &self.kind {
            LayerKind::Paint(paint) => Some(paint),
            _ => None,
        }
    }

    pub fn as_paint_mut(&mut self) -> Option<&mut Paint> {
        match &mut self.kind {
            LayerKind::Paint(paint) => Some(paint),
            _ => None,
        }
    }

    /// True for groups that run their own clock.
    pub fn is_sequence(&self) -> bool {
        self.as_group().is_some_and(|g| g.sequence.is_some())
    }

    pub fn type_name(&self) -> &'static str {
        match &self.kind {
            LayerKind::Group(g) if g.sequence.is_some() => "Sequence",
            LayerKind::Group(_) => "Group",
            LayerKind::Paint(_) => "Paint",
            LayerKind::Picture(_) => "Picture",
            LayerKind::Model(_) => "Model",
            LayerKind::Sound(_) => "Sound",
            LayerKind::Viewpoint(_) => "Viewpoint",
            LayerKind::Camera(_) => "Camera",
            LayerKind::Light(_) => "Light",
        }
    }
}

/// Animation keys of a layer. Times are local to the nearest enclosing sequence.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerAnim {
    /// Clips: a `true` key starts one, a `false` key ends it. An empty track means always present.
    pub visibility: Track<bool>,
    /// Left trim of the clip starting at the same time: how far into the content it begins.
    pub offset: Track<Flicks>,
    pub opacity: Track<f32>,
    pub transform: Track<Xform>,
    /// Story structure. Only meaningful on the root layer.
    pub actions: Track<Action>,
    /// Limner extension: keyed scalar properties by name, such as `light.energy` or `camera.fov`.
    pub channels: BTreeMap<String, Track<f64>>,
}

impl LayerAnim {
    /// The animation of a new layer: one clip that starts at `start` and never ends.
    pub fn starting_at(start: Flicks) -> Self {
        let mut anim = Self::default();
        anim.visibility.insert(Key::step(start, true));
        anim.offset.insert(Key::step(start, 0));
        anim
    }

    pub fn has_motion_keys(&self) -> bool {
        !self.transform.is_empty() || !self.opacity.is_empty() || !self.channels.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Action {
    /// Pause until the viewer continues.
    Stop,
    /// A chapter start: where rewinding lands.
    Play,
    /// Restart the piece.
    Loop,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayerKind {
    Group(Group),
    Paint(Paint),
    Picture(Picture),
    Model(Model),
    Sound(Sound),
    Viewpoint(Viewpoint),
    Camera(Camera),
    Light(Light),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    pub children: Vec<LayerId>,
    /// Present when the group is a sequence: a nested timeline with its own clock.
    pub sequence: Option<Sequence>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequence {
    /// Loop point. Zero means the sequence does not loop.
    pub loop_duration: Flicks,
    /// How many times the loop plays before holding at its end. Zero means forever.
    pub max_repeat: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Shading {
    /// Vertex colour only: what was painted is what is shown.
    #[default]
    Unlit,
    /// Scene lights shade the surface.
    Lit,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Paint {
    pub rate: FrameRate,
    /// Drawings in creation order. Several frames may show the same drawing.
    pub drawings: Vec<DrawingId>,
    /// For each frame, an index into `drawings`. Never empty.
    pub frames: Vec<u32>,
    /// How many times the frames play before holding on the last one. Zero means forever.
    pub max_repeat: u32,
    pub shading: Shading,
}

impl Paint {
    /// A paint layer with a single frame showing `drawing`.
    pub fn single(drawing: DrawingId, rate: FrameRate) -> Self {
        Self {
            rate,
            drawings: vec![drawing],
            frames: vec![0],
            max_repeat: 1,
            shading: Shading::Unlit,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    pub fn drawing_at_frame(&self, frame: usize) -> Option<DrawingId> {
        self.drawings
            .get(*self.frames.get(frame)? as usize)
            .copied()
    }

    /// The frame shown `local` flicks after the layer's clip started.
    pub fn frame_at(&self, local: Flicks) -> usize {
        let count = self.frames.len() as i64;
        if count <= 1 {
            return 0;
        }
        let since_start = self.rate.frame_at(local).max(0);
        if self.max_repeat != 0 && since_start / count >= self.max_repeat as i64 {
            (count - 1) as usize
        } else {
            (since_start % count) as usize
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Projection {
    #[default]
    Flat,
    Equirect,
    /// Top half left eye, bottom half right eye.
    EquirectStereo,
    CubemapCross,
    CubemapStrip,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub media: Option<MediaId>,
    pub projection: Projection,
    /// Stays fixed in front of the viewer, ignoring navigation.
    pub viewer_locked: bool,
    /// Path the picture was imported from, kept for "reimport".
    pub source_path: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub media: Option<MediaId>,
    pub shading: Shading,
    pub wireframe: bool,
    pub source_path: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SoundKind {
    /// Plays the same everywhere.
    #[default]
    Flat,
    /// A point source in space.
    Positional,
    Ambisonic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum AttenuationMode {
    #[default]
    None,
    Linear,
    Logarithmic,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attenuation {
    pub mode: AttenuationMode,
    /// Full volume inside this radius (layer units).
    pub min: f32,
    /// Silent beyond this radius.
    pub max: f32,
}

impl Default for Attenuation {
    fn default() -> Self {
        Self {
            mode: AttenuationMode::None,
            min: 0.1,
            max: 0.5,
        }
    }
}

/// Directional shaping of a positional sound. Angles are in radians.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub enum SoundModifier {
    #[default]
    None,
    Cone {
        inner: f32,
        band: f32,
        outside: f32,
    },
    Frustum {
        inner_h: f32,
        inner_v: f32,
        band: f32,
        outside: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Sound {
    pub media: Option<MediaId>,
    pub kind: SoundKind,
    pub gain: f32,
    pub looped: bool,
    pub attenuation: Attenuation,
    pub modifier: SoundModifier,
    pub source_path: String,
}

/// A spawn area: where a viewer stands, and at what scale.
#[derive(Clone, Debug, PartialEq)]
pub struct Viewpoint {
    /// True: the ring maps to the viewer's real floor. False: the headset maps to eye height.
    pub floor_level: bool,
    pub color: [f32; 3],
    /// Centre and radius of the volume the viewer may move in.
    pub volume: [f32; 4],
    pub allow_translation: [bool; 3],
    /// Included when the piece is exported for viewers.
    pub exporting: bool,
    pub show_volume: bool,
    pub thumbnail: Option<MediaId>,
}

impl Default for Viewpoint {
    fn default() -> Self {
        Self {
            floor_level: false,
            color: [0.26, 0.61, 0.95],
            volume: [0.0, 1.0, 0.0, 2.0],
            allow_translation: [true; 3],
            exporting: true,
            show_volume: false,
            thumbnail: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    /// Vertical field of view in degrees.
    pub fov: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self { fov: 45.0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LightKind {
    #[default]
    Point,
    Spot,
    Directional,
}

/// Limner extension: a scene light. Affects layers whose shading is `Lit`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Light {
    pub kind: LightKind,
    /// Linear RGB.
    pub color: [f32; 3],
    pub energy: f32,
    /// Reach of point and spot lights, in layer units.
    pub range: f32,
    /// Half-angle of a spot light's cone, in degrees.
    pub spot_angle: f32,
    /// Softness of the cone edge, 0 to 1.
    pub spot_blend: f32,
    pub shadows: bool,
}

impl Default for Light {
    fn default() -> Self {
        Self {
            kind: LightKind::Point,
            color: [1.0, 1.0, 1.0],
            energy: 1.0,
            range: 5.0,
            spot_angle: 35.0,
            spot_blend: 0.15,
            shadows: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint(frames: Vec<u32>, max_repeat: u32) -> Paint {
        let drawings = (0..=frames.iter().copied().max().unwrap_or(0))
            .map(|i| DrawingId(100 + i as u64))
            .collect();
        Paint {
            rate: FrameRate::whole(24),
            drawings,
            frames,
            max_repeat,
            shading: Shading::Unlit,
        }
    }

    #[test]
    fn frame_selection_loops_forever_when_repeat_is_zero() {
        let p = paint(vec![0, 0, 1, 2], 0);
        let d = p.rate.frame_duration();
        assert_eq!(p.frame_at(0), 0);
        assert_eq!(p.frame_at(d - 1), 0);
        assert_eq!(p.frame_at(d), 1);
        assert_eq!(p.frame_at(3 * d), 3);
        assert_eq!(p.frame_at(4 * d), 0);
        assert_eq!(p.frame_at(4_000 * d + 2 * d), 2);
        assert_eq!(p.drawing_at_frame(1), Some(DrawingId(100)));
        assert_eq!(p.drawing_at_frame(3), Some(DrawingId(102)));
    }

    #[test]
    fn frame_selection_holds_the_last_frame_after_the_repeat_count() {
        let once = paint(vec![0, 1, 2], 1);
        let d = once.rate.frame_duration();
        assert_eq!(once.frame_at(2 * d), 2);
        assert_eq!(once.frame_at(3 * d), 2);
        assert_eq!(once.frame_at(300 * d), 2);
        let twice = paint(vec![0, 1, 2], 2);
        assert_eq!(twice.frame_at(4 * d), 1);
        assert_eq!(twice.frame_at(6 * d), 2);
    }

    #[test]
    fn single_frame_layers_ignore_time() {
        let p = Paint::single(DrawingId(1), FrameRate::whole(12));
        assert_eq!(p.frame_at(-5), 0);
        assert_eq!(p.frame_at(1_000_000_000_000), 0);
        assert!(!p.is_animated());
    }

    #[test]
    fn negative_local_time_shows_the_first_frame() {
        let p = paint(vec![0, 1, 2], 0);
        assert_eq!(p.frame_at(-1), 0);
    }
}
