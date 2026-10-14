//! `State.json`: what the editor was doing when the project was last saved.
//!
//! The file changed shape between versions and nothing in it is needed to read a
//! project, so every field is optional and every type is read leniently.
//!
//! | Field | January 2017 | Quill 2.x (2019 and later) |
//! |---|---|---|
//! | Tool | `ToolID` (a number) | `ToolName` (a string) |
//! | Tapers | booleans | strings such as "None" and "Pressure" |
//! | Grid | `ShowGrid` on the top level | `Rulers.ShowGrid`, plus `DetailRender.Surface` |
//! | `DirectManipulation` | boolean | boolean, later 0 or 1 |

use crate::json;
use serde_json::Value;

/// Editor state carried by a Quill project. Everything is optional; old files lack most of it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct QuillState {
    /// Sixteen linear RGB swatches.
    pub palette: Vec<[f32; 3]>,
    /// Layer paths of the layers that were selected for painting, moving and camera work.
    pub paint_layer: Option<String>,
    pub move_layer: Option<String>,
    pub camera_layer: Option<String>,
    pub show_grid: Option<bool>,
    /// The floor texture ("Grid" or "None") and its scale.
    pub grid_texture: Option<String>,
    pub grid_scale: Option<f64>,
    /// Whether layers could be grabbed directly instead of through the gimbal.
    pub direct_manipulation: Option<bool>,
    /// "Paint", "Select", "Colorize" and so on. Files from 2017 store only a number;
    /// the name is then worked out from the tool's settings when they identify it.
    pub tool_name: Option<String>,
    /// The numeric tool id of files from 2017.
    pub tool_id: Option<i64>,
    /// The tool's settings exactly as stored.
    pub tool: Option<Value>,
    /// The same settings, typed.
    pub settings: ToolSettings,
}

/// The settings of the tool that was active. Which ones are present depends on the tool.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ToolSettings {
    /// Position of the brush in Quill's brush panel, not the stored stroke type.
    pub brush_id: Option<i64>,
    /// Linear RGB.
    pub color: Option<[f32; 3]>,
    pub opacity: Option<f32>,
    /// Brush radius in metres at the scale the user was working at.
    pub size: Option<f32>,
    /// Inner radius of the colorize tool's falloff.
    pub inner_size: Option<f32>,
    pub directional_stroke: Option<bool>,
    pub width_taper: Option<Taper>,
    pub transparency_taper: Option<Taper>,
    pub select_in_all_layers: Option<bool>,
}

/// How stroke width or opacity falls off along a stroke.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Taper {
    None,
    /// Follows trigger pressure. Files from 2017 store only on or off; on is read as this.
    Pressure,
    /// A mode this importer has no name for, kept as written.
    Other(String),
}

impl Taper {
    fn parse(v: &Value) -> Option<Self> {
        match v {
            Value::String(s) if s.eq_ignore_ascii_case("none") => Some(Taper::None),
            Value::String(s) if s.eq_ignore_ascii_case("pressure") => Some(Taper::Pressure),
            Value::String(s) => Some(Taper::Other(s.clone())),
            other => json::as_bool(other).map(|on| if on { Taper::Pressure } else { Taper::None }),
        }
    }
}

pub(crate) fn parse(root: &Value) -> QuillState {
    let Some(q) = root.get("Quill").filter(|q| q.is_object()) else {
        return QuillState::default();
    };
    let non_empty = |key: &str| {
        json::get_str(q, key)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let palette = q
        .get("ColorPalette")
        .and_then(Value::as_array)
        .map(|values| {
            let numbers: Vec<f32> = values
                .iter()
                .filter_map(json::as_f64)
                .map(|v| v as f32)
                .collect();
            numbers.as_chunks::<3>().0.to_vec()
        })
        .unwrap_or_default();

    let tool = q.get("Tool").filter(|t| t.is_object());
    let settings = tool.map(parse_settings).unwrap_or_default();
    let surface = q.get("DetailRender").and_then(|d| d.get("Surface"));
    QuillState {
        palette,
        paint_layer: non_empty("PaintLayer"),
        move_layer: non_empty("MoveLayer"),
        camera_layer: non_empty("CameraLayer"),
        show_grid: q
            .get("Rulers")
            .and_then(|r| json::get_bool(r, "ShowGrid"))
            .or_else(|| json::get_bool(q, "ShowGrid")),
        grid_texture: surface
            .and_then(|s| json::get_str(s, "Texture"))
            .map(str::to_owned),
        grid_scale: surface.and_then(|s| json::get_f64(s, "Scale")),
        direct_manipulation: json::get_bool(q, "DirectManipulation"),
        tool_name: non_empty("ToolName").or_else(|| tool.and_then(guess_tool_name)),
        tool_id: json::get_i64(q, "ToolID"),
        tool: tool.cloned(),
        settings,
    }
}

fn parse_settings(tool: &Value) -> ToolSettings {
    let float = |key: &str| json::get_f64(tool, key).map(|v| v as f32);
    ToolSettings {
        brush_id: json::get_i64(tool, "BrushID"),
        color: tool
            .get("Color")
            .and_then(json::floats::<3>)
            .map(|c| c.map(|v| v as f32)),
        opacity: float("Opacity"),
        size: float("Size"),
        inner_size: float("InnerSize"),
        directional_stroke: json::get_bool(tool, "DirectionalStroke"),
        width_taper: tool.get("WidthTaper").and_then(Taper::parse),
        transparency_taper: tool.get("TransparentTaper").and_then(Taper::parse),
        select_in_all_layers: json::get_bool(tool, "SelectInAllLayers"),
    }
}

/// Names the tool of a file that stores only a number, from settings no other tool has.
/// The numbering itself is not documented anywhere.
fn guess_tool_name(tool: &Value) -> Option<String> {
    let has = |key: &str| tool.get(key).is_some();
    let name = if has("WidthTaper") {
        "Paint"
    } else if has("SelectInAllLayers") {
        "Select"
    } else if has("InnerSize") {
        "Colorize"
    } else {
        return None;
    };
    Some(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_modern_state_file() {
        let state = parse(&json!({"Quill": {
            "Rulers": {"ShowGrid": true},
            "DetailRender": {"Surface": {"Texture": "Grid", "Scale": 2.5}},
            "MoveLayer": "", "PaintLayer": "Root/Paint", "CameraLayer": "Root/Cam",
            "DirectManipulation": 0,
            "ToolName": "Paint",
            "Tool": {"BrushID": 4, "Color": [0.1, 0.2, 0.3], "DirectionalStroke": true, "Opacity": 0.5,
                     "Size": 0.01, "TransparentTaper": "None", "WidthTaper": "Pressure"},
            "ColorPalette": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6]
        }}));
        assert_eq!(state.palette, vec![[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]);
        assert_eq!(state.paint_layer.as_deref(), Some("Root/Paint"));
        assert_eq!(state.move_layer, None);
        assert_eq!(state.camera_layer.as_deref(), Some("Root/Cam"));
        assert_eq!(state.show_grid, Some(true));
        assert_eq!(state.grid_texture.as_deref(), Some("Grid"));
        assert_eq!(state.grid_scale, Some(2.5));
        assert_eq!(state.direct_manipulation, Some(false));
        assert_eq!(state.tool_name.as_deref(), Some("Paint"));
        assert_eq!(state.tool_id, None);
        assert_eq!(
            state.settings,
            ToolSettings {
                brush_id: Some(4),
                color: Some([0.1, 0.2, 0.3]),
                opacity: Some(0.5),
                size: Some(0.01),
                inner_size: None,
                directional_stroke: Some(true),
                width_taper: Some(Taper::Pressure),
                transparency_taper: Some(Taper::None),
                select_in_all_layers: None,
            }
        );
        assert!(state.tool.is_some());
    }

    #[test]
    fn parses_a_2017_state_file() {
        let state = parse(&crate::testing::state_2017());
        assert_eq!(state.show_grid, Some(false));
        assert_eq!(state.direct_manipulation, Some(false));
        assert_eq!(state.tool_id, Some(0));
        assert_eq!(
            state.tool_name.as_deref(),
            Some("Paint"),
            "named from its settings"
        );
        assert_eq!(state.settings.width_taper, Some(Taper::Pressure));
        assert_eq!(state.settings.transparency_taper, Some(Taper::Pressure));
        assert_eq!(state.settings.brush_id, Some(3));
        assert_eq!(state.palette.len(), 16);

        let select = parse(&json!({"Quill": {
            "ToolID": 4, "DirectManipulation": true,
            "Tool": {"Size": 0.0129, "SelectInAllLayers": false}
        }}));
        assert_eq!(select.tool_name.as_deref(), Some("Select"));
        assert_eq!(select.direct_manipulation, Some(true));
        assert_eq!(select.settings.select_in_all_layers, Some(false));

        // A tool whose settings do not identify it keeps only its number.
        let unknown =
            parse(&json!({"Quill": {"ToolID": 7, "Tool": {"Size": 0.5, "Opacity": 1.0}}}));
        assert_eq!(unknown.tool_name, None);
        assert_eq!(unknown.tool_id, Some(7));
    }

    #[test]
    fn tolerates_missing_and_mistyped_sections() {
        assert_eq!(parse(&json!({})), QuillState::default());
        assert_eq!(parse(&json!({"Quill": 3})), QuillState::default());
        let odd = parse(&json!({"Quill": {
            "Rulers": "yes", "DetailRender": [], "Tool": "brush", "ToolName": 5, "ToolID": "2",
            "DirectManipulation": "true", "ColorPalette": {"a": 1}, "PaintLayer": 9
        }}));
        assert_eq!(odd.tool_id, Some(2));
        assert_eq!(odd.direct_manipulation, Some(true));
        assert_eq!(odd.tool, None);
        assert_eq!(odd.settings, ToolSettings::default());
        assert!(odd.palette.is_empty() && odd.paint_layer.is_none() && odd.show_grid.is_none());

        let taper =
            parse(&json!({"Quill": {"Tool": {"WidthTaper": "Speed", "TransparentTaper": 0}}}));
        assert_eq!(
            taper.settings.width_taper,
            Some(Taper::Other("Speed".into()))
        );
        assert_eq!(taper.settings.transparency_taper, Some(Taper::None));
    }
}
