//! Editor options: switches that change how the editor behaves, saved with the
//! project's editor state and not part of the artwork.

use crate::model::Interp;
use serde::{Deserialize, Serialize};

/// What grabbing a stroke moves when nothing was chosen as the transform target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoSelect {
    /// Grabbing always moves the view (or the chosen transform target).
    #[default]
    None,
    /// Grabbing a stroke moves its layer.
    Layer,
    /// Grabbing a stroke moves the group its layer is in.
    Group,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    /// The floor grid.
    pub show_grid: bool,
    pub onion_skin: bool,
    /// How many frames before and after to show; zero means all.
    pub onion_frames: u32,
    pub snap_to_frames: bool,
    pub snap_to_clips: bool,
    /// New layers start their clip at the playhead instead of at the beginning.
    pub create_layers_at_playhead: bool,
    /// Playback loops over the working range.
    pub loop_range: bool,
    /// Record: moving a layer or changing its opacity writes a key at the playhead.
    pub recording: bool,
    /// The Selection tool reaches into every visible, unlocked layer.
    pub select_in_all_layers: bool,
    /// The Selection tool selects in every frame of an animated layer.
    pub select_in_all_frames: bool,
    pub auto_select: AutoSelect,
    pub show_gimbal: bool,
    /// Steps in a full turn when rotating on a gimbal ring with the trigger.
    pub rotation_snap_steps: u32,
    pub allow_translate: bool,
    pub allow_rotate: bool,
    pub allow_scale: bool,
    /// Keep the horizon level while navigating.
    pub level_horizon: bool,
    /// Each repeat of a duplicate-and-move puts the copy on a new frame.
    pub animate_duplicates: bool,
    pub mute: bool,
    /// Follow the initial spawn area while playing.
    pub preview_spawn_area: bool,
    /// Draw gizmos for spawn areas, cameras, lights and sounds.
    pub show_extras: bool,
    /// How new keys move to the key after them.
    pub default_interp: Interp,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            show_grid: true,
            onion_skin: false,
            onion_frames: 1,
            snap_to_frames: true,
            snap_to_clips: true,
            create_layers_at_playhead: false,
            loop_range: true,
            recording: false,
            select_in_all_layers: false,
            select_in_all_frames: false,
            auto_select: AutoSelect::None,
            show_gimbal: false,
            rotation_snap_steps: 12,
            allow_translate: true,
            allow_rotate: true,
            allow_scale: true,
            level_horizon: false,
            animate_duplicates: false,
            mute: false,
            preview_spawn_area: false,
            show_extras: true,
            default_interp: Interp::Linear,
        }
    }
}

impl Options {
    pub fn sanitize(&mut self) {
        self.rotation_snap_steps = self.rotation_snap_steps.clamp(1, 360);
        self.onion_frames = self.onion_frames.min(16);
    }
}
