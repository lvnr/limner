//! Commands: every edit to a document, and every change to the editing session,
//! expressed as data.
//!
//! The desktop interface, the in-headset interface, the command-line tool and the
//! tests all drive the session through the same commands. A command is a JSON object
//! with a `cmd` field naming it, for example `{"cmd": "set_layer", "layer": 4, "visible": false}`.
//! Times are in flicks, colours are linear RGB, and layers are named by their numeric id.

use super::tools::ToolKind;
use crate::math::{Flip, Xform};
use crate::model::LayerId;
use crate::ops::layers::NewLayerKind;
use crate::ops::OpError;
use crate::time::{Flicks, FrameRate};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// How a selection command combines with what is already selected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectMode {
    #[default]
    Replace,
    Add,
    Remove,
    Toggle,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    // ---- history ----
    Undo,
    Redo,

    // ---- document ----
    /// Changes document settings. Fields left out stay as they are.
    SetDocument {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        background: Option<[f32; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        rate: Option<FrameRate>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range_start: Option<Flicks>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range_end: Option<Flicks>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        camera_resolution: Option<[u32; 2]>,
    },
    /// Chooses the spawn area a viewer starts at, or none.
    SetDefaultViewpoint {
        layer: Option<LayerId>,
    },
    SetPaletteColor {
        index: usize,
        color: [f32; 3],
    },

    // ---- layers ----
    /// Creates an empty layer. Without `parent` it goes next to the current layer.
    AddLayer {
        kind: NewLayerKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parent: Option<LayerId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        index: Option<usize>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    DeleteLayers {
        layers: Vec<LayerId>,
    },
    DuplicateLayers {
        layers: Vec<LayerId>,
    },
    /// Changes the properties every layer has. Fields left out stay as they are.
    SetLayer {
        layer: LayerId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visible: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        locked: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        collapsed: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        show_bounds: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        opacity: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        transform: Option<Xform>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pivot: Option<Xform>,
    },
    /// Changes properties specific to the layer's kind: a light's colour, a camera's
    /// lens, a paint layer's shading. `values` holds the fields to change.
    SetLayerKind {
        layer: LayerId,
        values: Map<String, Value>,
    },
    /// Moves a layer to `index` under `parent`, keeping it where it is in the world.
    MoveLayer {
        layer: LayerId,
        parent: LayerId,
        index: usize,
    },
    /// Moves a layer one row up or down the list, entering and leaving groups on the way.
    StepLayer {
        layer: LayerId,
        up: bool,
    },
    /// Puts sibling layers into a new group.
    GroupLayers {
        layers: Vec<LayerId>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    MergeDown {
        layer: LayerId,
    },
    FlattenGroup {
        layer: LayerId,
    },
    SetGroupIsSequence {
        layer: LayerId,
        sequence: bool,
    },
    FlipLayer {
        layer: LayerId,
        axis: Flip,
    },
    ResetTransform {
        layer: LayerId,
    },
    ResetPivot {
        layer: LayerId,
    },
    MakePivotOrigin {
        layer: LayerId,
    },

    // ---- what is being worked on (not part of the document, not undoable) ----
    /// Selects rows in the layer list. The last one becomes the current layer.
    SelectLayers {
        layers: Vec<LayerId>,
        #[serde(default)]
        mode: SelectMode,
    },
    /// Chooses what grabbing moves: a layer, or the view when `layer` is absent.
    SetTransformTarget {
        layer: Option<LayerId>,
    },
    SetOptions {
        values: Map<String, Value>,
    },
    SetTool {
        tool: ToolKind,
    },
    SetToolSettings {
        values: Map<String, Value>,
    },
    SetColor {
        color: [f32; 3],
    },

    // ---- playback (not undoable) ----
    Play,
    Pause,
    TogglePlay,
    Seek {
        time: Flicks,
    },
    StepFrames {
        frames: i64,
    },
    /// Jumps to the previous chapter mark, or the start of the range.
    Rewind,
    /// Jumps to the next chapter mark, or the end of the range.
    FastForward,
}

impl Command {
    /// A short name for the undo menu, or `None` for commands that do not edit the document.
    pub fn label(&self) -> Option<&'static str> {
        use Command::*;
        Some(match self {
            SetDocument { .. } => "Change document settings",
            SetDefaultViewpoint { .. } => "Set initial spawn area",
            SetPaletteColor { .. } => "Change palette",
            AddLayer { .. } => "Add layer",
            DeleteLayers { .. } => "Delete layer",
            DuplicateLayers { .. } => "Duplicate layer",
            SetLayer { name: Some(_), .. } => "Rename layer",
            SetLayer {
                opacity: Some(_), ..
            } => "Change layer opacity",
            SetLayer {
                transform: Some(_), ..
            } => "Move layer",
            SetLayer { pivot: Some(_), .. } => "Move pivot",
            SetLayer {
                visible: Some(_), ..
            } => "Show or hide layer",
            SetLayer {
                locked: Some(_), ..
            } => "Lock or unlock layer",
            SetLayer { .. } => "Change layer",
            SetLayerKind { .. } => "Change layer settings",
            MoveLayer { .. } | StepLayer { .. } => "Move layer in list",
            GroupLayers { .. } => "Group layers",
            MergeDown { .. } => "Merge down",
            FlattenGroup { .. } => "Flatten group",
            SetGroupIsSequence { .. } => "Convert group",
            FlipLayer { .. } => "Flip layer",
            ResetTransform { .. } => "Reset transform",
            ResetPivot { .. } => "Reset pivot",
            MakePivotOrigin { .. } => "Make pivot origin",
            Undo
            | Redo
            | SelectLayers { .. }
            | SetTransformTarget { .. }
            | SetOptions { .. }
            | SetTool { .. }
            | SetToolSettings { .. }
            | SetColor { .. }
            | Play
            | Pause
            | TogglePlay
            | Seek { .. }
            | StepFrames { .. }
            | Rewind
            | FastForward => return None,
        })
    }

    /// True for changes that are not worth an undo step of their own: folding a group
    /// in the list is saved with the document but is not something to undo.
    pub fn is_cosmetic(&self) -> bool {
        matches!(
            self,
            Command::SetLayer {
                name: None,
                visible: None,
                locked: None,
                collapsed: Some(_),
                show_bounds: None,
                opacity: None,
                transform: None,
                pivot: None,
                ..
            }
        )
    }
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum CommandError {
    #[error(transparent)]
    Op(#[from] OpError),
    #[error("{0}")]
    Invalid(String),
    #[error("layer \"{0}\" is locked")]
    Locked(String),
    #[error("could not read the command: {0}")]
    Malformed(String),
}

impl From<crate::model::DocumentError> for CommandError {
    fn from(e: crate::model::DocumentError) -> Self {
        CommandError::Op(OpError::Document(e))
    }
}

impl CommandError {
    pub fn invalid(message: impl Into<String>) -> Self {
        CommandError::Invalid(message.into())
    }
}
