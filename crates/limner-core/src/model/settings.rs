//! Document-wide settings.

use super::ids::LayerId;
use crate::time::{self, Flicks, FrameRate};

/// Number of colour swatches a document carries.
pub const PALETTE_SIZE: usize = 16;

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub title: String,
    pub description: String,
    /// Linear RGB.
    pub background: [f32; 3],
    pub rate: FrameRate,
    /// The working range: what loops in the editor and what cameras render.
    pub range_start: Flicks,
    pub range_end: Flicks,
    /// The spawn area a viewer starts at.
    pub default_viewpoint: Option<LayerId>,
    pub camera_resolution: [u32; 2],
    /// Linear RGB swatches.
    pub palette: Vec<[f32; 3]>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            title: String::new(),
            description: String::new(),
            background: [0.8, 0.8, 0.8],
            rate: FrameRate::whole(24),
            range_start: 0,
            range_end: time::from_seconds(10.0),
            default_viewpoint: None,
            camera_resolution: [1920, 1080],
            palette: vec![[0.82, 0.82, 0.82]; PALETTE_SIZE],
        }
    }
}
