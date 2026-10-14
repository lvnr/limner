//! Commands: every edit to a document, and every change to the editing session,
//! expressed as data.
//!
//! The desktop interface, the in-headset interface, the command-line tool and the
//! tests all drive the session through the same commands. A command is a JSON object
//! with a `cmd` field naming it, for example `{"cmd": "set_layer", "layer": 4, "visible": false}`.
//! Times are in flicks, colours are linear RGB, and layers are named by their numeric id.

use super::animation::KeyTrack;
use super::tools::ToolKind;
use crate::math::{Flip, Xform};
use crate::model::{Action, Interp, LayerId};
use crate::ops::frames::NewFrame;
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

/// What the Optimize command works through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizeScope {
    /// Every frame of the paint layer.
    Layer,
    /// Every paint layer that is not locked, itself or through a group it is in.
    All,
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

    // ---- frames of a paint layer ----
    /// Adds a frame after the one shown and moves on to it.
    AddFrame {
        layer: LayerId,
        kind: NewFrame,
    },
    /// Makes the frame shown last one frame longer.
    AddHoldFrame {
        layer: LayerId,
        #[serde(default = "yes")]
        after: bool,
    },
    /// Removes the frame shown; the animation gets shorter.
    DeleteFrame {
        layer: LayerId,
    },
    /// Replaces the key frame shown by a hold of the drawing before it.
    RemoveKeyFrame {
        layer: LayerId,
    },
    /// Removes all animation, leaving the first frame.
    ClearFrames {
        layer: LayerId,
    },
    DoubleFrames {
        layer: LayerId,
    },
    ReverseFrames {
        layer: LayerId,
    },
    /// Copies the selected strokes into the frame shown, or into every frame.
    CopySelectionToFrames {
        #[serde(default)]
        all: bool,
    },

    // ---- clips ----
    /// Ends the layer's clip at `time` (the playhead when left out), or starts a new one there.
    ToggleClip {
        layer: LayerId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<Flicks>,
    },
    /// Cuts the clip at `time` in two.
    SplitClip {
        layer: LayerId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<Flicks>,
    },
    /// Moves clip number `clip` (counted from zero) to start at `start`.
    MoveClip {
        layer: LayerId,
        clip: usize,
        start: Flicks,
    },
    /// Changes where a clip starts or ends without shifting its content.
    TrimClip {
        layer: LayerId,
        clip: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Flicks>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        end: Option<Flicks>,
        /// Removes the clip's end, so that it never stops.
        #[serde(default)]
        open_end: bool,
    },
    DeleteClip {
        layer: LayerId,
        clip: usize,
    },
    /// Shows a clip's content again, starting at `start`.
    DuplicateClip {
        layer: LayerId,
        clip: usize,
        start: Flicks,
    },

    // ---- keys ----
    /// Keys the layer's present transform or opacity at `time` (the playhead when left out).
    SetKey {
        layer: LayerId,
        track: KeyTrack,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<Flicks>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        interp: Option<Interp>,
        /// Also key every layer inside this one.
        #[serde(default)]
        children: bool,
    },
    /// Deletes the keys at `times`, or the one at the playhead when none are given.
    DeleteKeys {
        layer: LayerId,
        track: KeyTrack,
        #[serde(default)]
        times: Vec<Flicks>,
    },
    MoveKeys {
        layer: LayerId,
        track: KeyTrack,
        times: Vec<Flicks>,
        delta: Flicks,
    },
    SetKeyInterp {
        layer: LayerId,
        track: KeyTrack,
        times: Vec<Flicks>,
        interp: Interp,
    },
    /// Plays the keys at `times` backwards within the span they cover.
    ReverseKeys {
        layer: LayerId,
        track: KeyTrack,
        times: Vec<Flicks>,
    },
    /// Pastes the keys last copied, the first of them at `time` (the playhead when left out).
    PasteKeys {
        layer: LayerId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<Flicks>,
    },
    /// Puts a chapter mark (stop, play or loop) on the root timeline, or removes the one there.
    SetAction {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        time: Option<Flicks>,
        action: Option<Action>,
    },
    /// Makes a sequence loop at the playhead, or stops it looping.
    SetLoopPoint {
        layer: LayerId,
        enabled: bool,
    },
    /// Turns one loop of a looping sequence into a frame-by-frame paint layer.
    BakeSequence {
        layer: LayerId,
    },

    // ---- strokes ----
    /// Deletes the selected strokes.
    DeleteSelection,
    /// Moves the selected strokes to a new paint layer, or copies them there.
    SelectionToNewLayer {
        #[serde(default)]
        copy: bool,
    },
    /// Simplifies every stroke of the paint layer, or of every unlocked paint layer.
    Optimize {
        scope: OptimizeScope,
    },

    // ---- what is being worked on (not part of the document, not undoable) ----
    /// Selects every stroke the Selection tool can reach now.
    SelectAll,
    Deselect,
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
    /// Remembers the keys at `times` for pasting.
    CopyKeys {
        layer: LayerId,
        track: KeyTrack,
        times: Vec<Flicks>,
    },
    /// Moves the playhead to the key frame before or after the one shown.
    GoToKeyFrame {
        layer: LayerId,
        forward: bool,
    },
    /// Moves the playhead to the key before or after it.
    GoToKey {
        layer: LayerId,
        track: KeyTrack,
        forward: bool,
    },
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
            AddFrame {
                kind: NewFrame::Blank,
                ..
            } => "Add frame",
            AddFrame {
                kind: NewFrame::Duplicate,
                ..
            } => "Duplicate frame",
            AddHoldFrame { .. } => "Hold frame",
            DeleteFrame { .. } => "Delete frame",
            RemoveKeyFrame { .. } => "Remove key frame",
            ClearFrames { .. } => "Delete all frames",
            DoubleFrames { .. } => "Double frames",
            ReverseFrames { .. } => "Reverse frames",
            CopySelectionToFrames { .. } => "Copy strokes to frames",
            ToggleClip { .. } | TrimClip { .. } => "Change clip",
            SplitClip { .. } => "Split clip",
            MoveClip { .. } => "Move clip",
            DeleteClip { .. } => "Delete clip",
            DuplicateClip { .. } => "Duplicate clip",
            SetKey { .. } => "Add key",
            DeleteKeys { .. } => "Delete keys",
            MoveKeys { .. } => "Move keys",
            SetKeyInterp { .. } => "Change key easing",
            ReverseKeys { .. } => "Reverse keys",
            PasteKeys { .. } => "Paste keys",
            SetAction { .. } => "Change chapter mark",
            SetLoopPoint { .. } => "Change loop point",
            BakeSequence { .. } => "Bake sequence",
            DeleteSelection => "Delete strokes",
            SelectionToNewLayer { copy: true } => "Copy strokes to new layer",
            SelectionToNewLayer { copy: false } => "Move strokes to new layer",
            Optimize { .. } => "Optimize strokes",
            Undo
            | Redo
            | CopyKeys { .. }
            | GoToKeyFrame { .. }
            | GoToKey { .. }
            | SelectAll
            | Deselect
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

fn yes() -> bool {
    true
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
