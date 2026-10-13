//! `State.json`: what the editor was doing when the project was last saved.

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
    pub tool_name: Option<String>,
    /// The tool's settings exactly as stored.
    pub tool: Option<Value>,
}

pub(crate) fn parse(root: &Value) -> QuillState {
    let Some(q) = root.get("Quill") else {
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
    QuillState {
        palette,
        paint_layer: non_empty("PaintLayer"),
        move_layer: non_empty("MoveLayer"),
        camera_layer: non_empty("CameraLayer"),
        show_grid: q.get("Rulers").and_then(|r| json::get_bool(r, "ShowGrid")),
        tool_name: non_empty("ToolName"),
        tool: q.get("Tool").cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_a_modern_state_file() {
        let state = parse(&json!({"Quill": {
            "Rulers": {"ShowGrid": true},
            "MoveLayer": "", "PaintLayer": "Root/Paint", "CameraLayer": "Root/Cam",
            "ToolName": "Select", "Tool": {"Size": 0.01},
            "ColorPalette": [0.1, 0.2, 0.3, 0.4, 0.5, 0.6]
        }}));
        assert_eq!(state.palette, vec![[0.1, 0.2, 0.3], [0.4, 0.5, 0.6]]);
        assert_eq!(state.paint_layer.as_deref(), Some("Root/Paint"));
        assert_eq!(state.move_layer, None);
        assert_eq!(state.show_grid, Some(true));
        assert_eq!(state.tool_name.as_deref(), Some("Select"));
    }

    #[test]
    fn tolerates_missing_sections() {
        assert_eq!(parse(&json!({})), QuillState::default());
        assert_eq!(
            parse(&json!({"Quill": {"ToolID": 0}})),
            QuillState::default()
        );
    }
}
