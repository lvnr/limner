//! The tools and their settings: what the Tools panel shows.

use crate::model::Brush;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolKind {
    #[default]
    Paint,
    Line,
    Colorize,
    Eraser,
    Select,
    Grab,
    Thicken,
    Nudge,
    Optimize,
}

impl ToolKind {
    pub const ALL: [ToolKind; 9] = [
        ToolKind::Paint,
        ToolKind::Line,
        ToolKind::Colorize,
        ToolKind::Eraser,
        ToolKind::Select,
        ToolKind::Grab,
        ToolKind::Thicken,
        ToolKind::Nudge,
        ToolKind::Optimize,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ToolKind::Paint => "Paint",
            ToolKind::Line => "Straight line",
            ToolKind::Colorize => "Colorize",
            ToolKind::Eraser => "Eraser",
            ToolKind::Select => "Selection",
            ToolKind::Grab => "Grab",
            ToolKind::Thicken => "Thicken / Thin",
            ToolKind::Nudge => "Nudge",
            ToolKind::Optimize => "Optimize",
        }
    }

    /// Tools that add strokes, as opposed to changing existing ones.
    pub fn paints(self) -> bool {
        matches!(self, ToolKind::Paint | ToolKind::Line)
    }
}

/// How the end of a stroke is closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    /// Open ends: flat ribbons.
    None,
    /// A zero-width point just beyond the end closes the tube with a flat face.
    Flat,
    /// The width swells from zero along a semicircle.
    Round,
}

/// The seven brushes of the Tools panel. They share four cross-sections; the rest
/// is how the ends are closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrushStyle {
    CappedCylinder,
    CappedEllipse,
    RoundedRibbon,
    Cylinder,
    #[default]
    Ellipse,
    Ribbon,
    Cube,
}

impl BrushStyle {
    pub const ALL: [BrushStyle; 7] = [
        BrushStyle::CappedCylinder,
        BrushStyle::CappedEllipse,
        BrushStyle::RoundedRibbon,
        BrushStyle::Cylinder,
        BrushStyle::Ellipse,
        BrushStyle::Ribbon,
        BrushStyle::Cube,
    ];

    /// The cross-section stored with the stroke.
    pub fn brush(self) -> Brush {
        match self {
            BrushStyle::CappedCylinder | BrushStyle::Cylinder => Brush::Cylinder,
            BrushStyle::CappedEllipse | BrushStyle::Ellipse => Brush::Ellipse,
            BrushStyle::RoundedRibbon | BrushStyle::Ribbon => Brush::Ribbon,
            BrushStyle::Cube => Brush::Cube,
        }
    }

    pub fn cap(self) -> Cap {
        match self {
            BrushStyle::CappedCylinder | BrushStyle::CappedEllipse | BrushStyle::RoundedRibbon => {
                Cap::Round
            }
            BrushStyle::Cylinder | BrushStyle::Ellipse | BrushStyle::Cube => Cap::Flat,
            BrushStyle::Ribbon => Cap::None,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            BrushStyle::CappedCylinder => "Capped cylinder",
            BrushStyle::CappedEllipse => "Capped ellipse",
            BrushStyle::RoundedRibbon => "Rounded ribbon",
            BrushStyle::Cylinder => "Cylinder",
            BrushStyle::Ellipse => "Ellipse",
            BrushStyle::Ribbon => "Ribbon",
            BrushStyle::Cube => "Cube",
        }
    }
}

/// What makes a stroke's width or opacity vary along its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Taper {
    #[default]
    None,
    /// Follows how hard the trigger (or pen) is pressed.
    Pressure,
    /// Tapers automatically at both ends.
    Auto,
    /// Pressure and automatic tapering together.
    Both,
}

impl Taper {
    pub fn uses_pressure(self) -> bool {
        matches!(self, Taper::Pressure | Taper::Both)
    }

    pub fn is_automatic(self) -> bool {
        matches!(self, Taper::Auto | Taper::Both)
    }
}

/// How the Colorize tool combines the paint colour with a stroke's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    #[default]
    Normal,
    Darken,
    Multiply,
    Overlay,
    Burn,
    Lighten,
    Screen,
    Add,
    Dodge,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Opacity,
}

impl BlendMode {
    pub const ALL: [BlendMode; 14] = [
        BlendMode::Normal,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::Overlay,
        BlendMode::Burn,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::Add,
        BlendMode::Dodge,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::Opacity,
    ];

    pub fn title(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal (RGB)",
            BlendMode::Darken => "Darken",
            BlendMode::Multiply => "Multiply",
            BlendMode::Overlay => "Overlay",
            BlendMode::Burn => "Burn",
            BlendMode::Lighten => "Lighten",
            BlendMode::Screen => "Screen",
            BlendMode::Add => "Add",
            BlendMode::Dodge => "Dodge",
            BlendMode::Hue => "Hue (H)",
            BlendMode::Saturation => "Saturation (S)",
            BlendMode::Color => "Color (HS)",
            BlendMode::Luminosity => "Luminosity (V)",
            BlendMode::Opacity => "Opacity (A)",
        }
    }
}

/// Sizes are radii in viewer-space metres: zooming the scene changes how much of the
/// artwork a tool covers, exactly as moving closer to a canvas does.
pub const MIN_TOOL_SIZE: f32 = 0.0005;
pub const MAX_TOOL_SIZE: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PaintSettings {
    pub brush: BrushStyle,
    /// Half-width of a stroke at full pressure.
    pub size: f32,
    pub opacity: f32,
    pub width_taper: Taper,
    pub opacity_taper: Taper,
    /// Strokes fade when seen from another angle than they were drawn from.
    pub directional: bool,
    /// While painting during playback: how long each piece of a stroke stays visible, in seconds.
    pub anim_duration: f32,
}

impl Default for PaintSettings {
    fn default() -> Self {
        Self {
            brush: BrushStyle::Ellipse,
            size: 0.01,
            opacity: 1.0,
            width_taper: Taper::Pressure,
            opacity_taper: Taper::None,
            directional: false,
            anim_duration: 0.1,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LineSettings {
    pub brush: BrushStyle,
    pub size: f32,
    pub opacity: f32,
    pub directional: bool,
}

impl Default for LineSettings {
    fn default() -> Self {
        Self {
            brush: BrushStyle::Cylinder,
            size: 0.01,
            opacity: 1.0,
            directional: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ColorizeSettings {
    pub size: f32,
    /// Radius of the inner sphere, inside which the effect is at full strength, as a
    /// fraction of `size`. Zero is fully soft.
    pub inner: f32,
    pub strength: f32,
    pub mode: BlendMode,
}

impl Default for ColorizeSettings {
    fn default() -> Self {
        Self {
            size: 0.02,
            inner: 0.0,
            strength: 1.0,
            mode: BlendMode::Normal,
        }
    }
}

/// Settings shared by the tools that are a sphere with a strength.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SphereSettings {
    pub size: f32,
    /// See [`ColorizeSettings::inner`]. Only the Grab tool shows it.
    pub inner: f32,
    pub strength: f32,
}

impl Default for SphereSettings {
    fn default() -> Self {
        Self {
            size: 0.03,
            inner: 0.0,
            strength: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectSettings {
    pub size: f32,
}

impl Default for SelectSettings {
    fn default() -> Self {
        Self { size: 0.02 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OptimizeSettings {
    pub size: f32,
    /// How much a stroke may deviate after simplifying, relative to its width.
    pub threshold: f32,
}

impl Default for OptimizeSettings {
    fn default() -> Self {
        Self {
            size: 0.03,
            threshold: 0.02,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolSettings {
    pub tool: ToolKind,
    /// The tool used before the Selection tool, for switching back.
    pub previous: ToolKind,
    /// The paint colour, linear RGB. Shared by Paint, Line and Colorize.
    pub color: [f32; 3],
    pub paint: PaintSettings,
    pub line: LineSettings,
    pub colorize: ColorizeSettings,
    pub eraser: SphereSettings,
    pub select: SelectSettings,
    pub grab: SphereSettings,
    pub thicken: SphereSettings,
    pub nudge: SphereSettings,
    pub optimize: OptimizeSettings,
}

impl Default for ToolSettings {
    fn default() -> Self {
        Self {
            tool: ToolKind::Paint,
            previous: ToolKind::Paint,
            color: [0.0, 0.0, 0.0],
            paint: PaintSettings::default(),
            line: LineSettings::default(),
            colorize: ColorizeSettings::default(),
            eraser: SphereSettings {
                size: 0.02,
                ..SphereSettings::default()
            },
            select: SelectSettings::default(),
            grab: SphereSettings {
                size: 0.06,
                ..SphereSettings::default()
            },
            thicken: SphereSettings {
                strength: 0.5,
                ..SphereSettings::default()
            },
            nudge: SphereSettings {
                strength: 0.5,
                ..SphereSettings::default()
            },
            optimize: OptimizeSettings::default(),
        }
    }
}

impl ToolSettings {
    /// The size of the current tool: a stroke half-width for Paint and Line, a sphere
    /// radius for the others.
    pub fn size(&self) -> f32 {
        match self.tool {
            ToolKind::Paint => self.paint.size,
            ToolKind::Line => self.line.size,
            ToolKind::Colorize => self.colorize.size,
            ToolKind::Eraser => self.eraser.size,
            ToolKind::Select => self.select.size,
            ToolKind::Grab => self.grab.size,
            ToolKind::Thicken => self.thicken.size,
            ToolKind::Nudge => self.nudge.size,
            ToolKind::Optimize => self.optimize.size,
        }
    }

    pub fn set_size(&mut self, size: f32) {
        let size = size.clamp(MIN_TOOL_SIZE, MAX_TOOL_SIZE);
        match self.tool {
            ToolKind::Paint => self.paint.size = size,
            ToolKind::Line => self.line.size = size,
            ToolKind::Colorize => self.colorize.size = size,
            ToolKind::Eraser => self.eraser.size = size,
            ToolKind::Select => self.select.size = size,
            ToolKind::Grab => self.grab.size = size,
            ToolKind::Thicken => self.thicken.size = size,
            ToolKind::Nudge => self.nudge.size = size,
            ToolKind::Optimize => self.optimize.size = size,
        }
    }

    /// The inner-sphere fraction of the current tool, for the tools that have one.
    pub fn inner(&self) -> Option<f32> {
        match self.tool {
            ToolKind::Colorize => Some(self.colorize.inner),
            ToolKind::Grab => Some(self.grab.inner),
            _ => None,
        }
    }

    pub fn set_inner(&mut self, inner: f32) {
        let inner = inner.clamp(0.0, 1.0);
        match self.tool {
            ToolKind::Colorize => self.colorize.inner = inner,
            ToolKind::Grab => self.grab.inner = inner,
            _ => {}
        }
    }

    /// Switches tool, remembering the one to return to from the Selection tool.
    pub fn select_tool(&mut self, tool: ToolKind) {
        if tool == self.tool {
            return;
        }
        if self.tool != ToolKind::Select {
            self.previous = self.tool;
        }
        self.tool = tool;
    }

    /// Switches between the Selection tool and the tool used before it.
    pub fn toggle_select_tool(&mut self) {
        if self.tool == ToolKind::Select {
            self.tool = if self.previous == ToolKind::Select {
                ToolKind::Paint
            } else {
                self.previous
            };
        } else {
            self.select_tool(ToolKind::Select);
        }
    }

    /// Keeps values within their valid ranges after they were set from outside.
    pub fn sanitize(&mut self) {
        let size = |s: &mut f32| {
            *s = if s.is_finite() {
                s.clamp(MIN_TOOL_SIZE, MAX_TOOL_SIZE)
            } else {
                0.02
            }
        };
        let unit = |s: &mut f32| {
            *s = if s.is_finite() {
                s.clamp(0.0, 1.0)
            } else {
                1.0
            }
        };
        size(&mut self.paint.size);
        size(&mut self.line.size);
        size(&mut self.colorize.size);
        size(&mut self.eraser.size);
        size(&mut self.select.size);
        size(&mut self.grab.size);
        size(&mut self.thicken.size);
        size(&mut self.nudge.size);
        size(&mut self.optimize.size);
        unit(&mut self.paint.opacity);
        unit(&mut self.line.opacity);
        unit(&mut self.colorize.strength);
        unit(&mut self.colorize.inner);
        unit(&mut self.eraser.strength);
        unit(&mut self.grab.strength);
        unit(&mut self.grab.inner);
        unit(&mut self.thicken.strength);
        unit(&mut self.nudge.strength);
        self.optimize.threshold = if self.optimize.threshold.is_finite() {
            self.optimize.threshold.clamp(0.0, 1.0)
        } else {
            0.02
        };
        self.paint.anim_duration = if self.paint.anim_duration.is_finite() {
            self.paint.anim_duration.clamp(0.0, 10.0)
        } else {
            0.1
        };
        for c in &mut self.color {
            *c = if c.is_finite() { c.max(0.0) } else { 0.0 };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_brushes_share_four_cross_sections() {
        let stored: std::collections::BTreeSet<u8> =
            BrushStyle::ALL.iter().map(|b| b.brush().code()).collect();
        assert_eq!(stored.len(), 4);
        assert_eq!(BrushStyle::Ribbon.cap(), Cap::None);
        assert_eq!(BrushStyle::RoundedRibbon.cap(), Cap::Round);
        assert_eq!(BrushStyle::Cube.cap(), Cap::Flat);
    }

    #[test]
    fn the_selection_tool_toggles_back_to_the_tool_in_use() {
        let mut tools = ToolSettings::default();
        tools.select_tool(ToolKind::Eraser);
        tools.toggle_select_tool();
        assert_eq!(tools.tool, ToolKind::Select);
        tools.toggle_select_tool();
        assert_eq!(tools.tool, ToolKind::Eraser);
        tools.select_tool(ToolKind::Select);
        tools.select_tool(ToolKind::Grab);
        tools.toggle_select_tool();
        tools.toggle_select_tool();
        assert_eq!(tools.tool, ToolKind::Grab);
    }

    #[test]
    fn sizes_stay_in_range_and_settings_survive_json() {
        let mut tools = ToolSettings::default();
        tools.set_size(100.0);
        assert_eq!(tools.size(), MAX_TOOL_SIZE);
        tools.select_tool(ToolKind::Colorize);
        tools.set_size(0.0);
        assert_eq!(tools.size(), MIN_TOOL_SIZE);
        assert_eq!(
            tools.paint.size, MAX_TOOL_SIZE,
            "each tool keeps its own size"
        );
        let json = serde_json::to_value(&tools).unwrap();
        assert_eq!(json["colorize"]["mode"], "normal");
        assert_eq!(json["paint"]["brush"], "ellipse");
        let back: ToolSettings = serde_json::from_value(json).unwrap();
        assert_eq!(back, tools);
        // Settings written by an older version, with fields missing, still load.
        let partial: ToolSettings =
            serde_json::from_str(r#"{"tool":"nudge","paint":{"size":0.05}}"#).unwrap();
        assert_eq!(partial.tool, ToolKind::Nudge);
        assert_eq!(partial.paint.size, 0.05);
        assert_eq!(partial.paint.opacity, 1.0);
    }
}
