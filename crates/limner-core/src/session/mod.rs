//! An editing session: a document plus everything about working on it that is not
//! part of the document itself (the view, the playhead, the tool in hand, undo).
//!
//! The session is the one place edits happen. Interfaces send it [`Command`]s and
//! controller input, and ask it what to show.

mod animation;
mod command;
mod gesture;
mod gimbal;
mod grab;
mod history;
pub mod input;
pub mod media;
mod options;
mod playback;
mod props;
mod query;
mod selection;
mod strokes;
pub mod tools;
mod view;

pub use animation::{KeyTrack, OnionSkin};
pub use command::{Command, CommandError, OptimizeScope, SelectMode};
pub use gesture::Cursor;
pub use gimbal::{
    Gimbal, GimbalHandle, ARROW_END, ARROW_START, CENTER_RADIUS, PLANE_FAR, PLANE_NEAR,
};
pub use grab::Subject;
pub use history::History;
pub use input::{Feedback, HandInput, InputFrame};
pub use options::{AutoSelect, Options};
pub use playback::{Playback, PlaybackEvent};
pub use props::{apply_kind_patch, kind_props};
pub use selection::Selection;
pub use view::{View, DEFAULT_ZOOM_LIMITS};

use crate::eval::{evaluate, Clock, RenderFrame};
use crate::json::patched;
use crate::math::{Aabb, Xform};
use crate::model::{Document, LayerId, LayerKind, PALETTE_SIZE};
use crate::ops::layers::{self, NewLayerKind};
use crate::time::Flicks;
use glam::DVec3;
use serde_json::{json, Value};
use tools::ToolSettings;

/// An edit in progress that spans several commands or frames of input, such as
/// dragging a slider. It becomes one undo step.
struct Transaction {
    before: Document,
    revision: u64,
    label: String,
    depth: u32,
}

pub struct Session {
    doc: Document,
    history: History,
    pub view: View,
    pub playback: Playback,
    pub tools: ToolSettings,
    pub options: Options,
    /// Rows selected in the layer list; the last one is the current layer.
    selected_layers: Vec<LayerId>,
    /// The layer the painting tools draw into.
    paint_layer: Option<LayerId>,
    /// What grabbing moves; `None` is the view.
    transform_target: Option<LayerId>,
    /// Counts every change to the document: whether an edit did anything, and
    /// whether there is something to save.
    edits: u64,
    saved_edits: u64,
    /// Counts document changes an interface should redraw for. A tool at work changes
    /// strokes every frame without touching this until it is done.
    doc_revision: u64,
    /// Counts changes to everything else an interface shows.
    state_revision: u64,
    transaction: Option<Transaction>,
    /// The tool at work, between a trigger press and its release.
    gesture: Option<gesture::Gesture>,
    trigger: input::Latch,
    controls: gesture::Controls,
    last_input: InputFrame,
    /// The selected strokes.
    selection: Selection,
    /// Keys copied for pasting.
    key_clipboard: Option<animation::KeyClipboard>,
    /// What the grips hold.
    grab: Option<grab::Grab>,
    grips: [input::Latch; 2],
    /// The handle of the gimbal at each hand.
    hover: [Option<GimbalHandle>; 2],
    /// The last duplicate-and-move, and the state of the document right after it:
    /// it can be repeated only while nothing else has been done since.
    repeat: Option<(grab::Repeat, u64)>,
    /// A repeat to remember once the gesture that made it is closed.
    pending_repeat: Option<grab::Repeat>,
    /// Where the selection's pivot was put, if it was.
    selection_pivot: Option<grab::SelectionPivot>,
}

impl Session {
    pub fn new(doc: Document) -> Self {
        let mut session = Self {
            doc,
            history: History::default(),
            view: View::default(),
            playback: Playback::default(),
            tools: ToolSettings::default(),
            options: Options::default(),
            selected_layers: Vec::new(),
            paint_layer: None,
            transform_target: None,
            edits: 1,
            saved_edits: 1,
            doc_revision: 1,
            state_revision: 1,
            transaction: None,
            gesture: None,
            trigger: input::Latch::default(),
            controls: gesture::Controls::default(),
            last_input: InputFrame::default(),
            selection: Selection::default(),
            key_clipboard: None,
            grab: None,
            grips: [input::Latch::default(); 2],
            hover: [None, None],
            repeat: None,
            pending_repeat: None,
            selection_pivot: None,
        };
        session.go_to_default_viewpoint();
        session.adopt_document();
        session
    }

    /// Replaces the document, as when opening a file. History and selection start over.
    pub fn replace_document(&mut self, doc: Document) {
        self.doc = doc;
        self.history.clear();
        self.transaction = None;
        self.gesture = None;
        self.grab = None;
        self.hover = [None, None];
        self.repeat = None;
        self.pending_repeat = None;
        self.selection_pivot = None;
        self.selection.clear();
        self.playback = Playback::default();
        self.selected_layers.clear();
        self.paint_layer = None;
        self.transform_target = None;
        self.edits += 1;
        self.saved_edits = self.edits;
        self.doc_revision += 1;
        self.state_revision += 1;
        self.view = View::default();
        self.go_to_default_viewpoint();
        self.adopt_document();
    }

    fn adopt_document(&mut self) {
        self.paint_layer = self.first_paint_layer();
        self.selected_layers = self.paint_layer.into_iter().collect();
    }

    fn first_paint_layer(&self) -> Option<LayerId> {
        self.doc
            .walk()
            .into_iter()
            .find(|id| self.doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
    }

    pub fn doc(&self) -> &Document {
        &self.doc
    }

    /// The document for editing. Every access counts as a change.
    fn doc_mut(&mut self) -> &mut Document {
        self.edits += 1;
        self.doc_revision += 1;
        &mut self.doc
    }

    /// The document for a tool that changes strokes many times a second. Interfaces
    /// are told once, when the tool is done.
    pub(crate) fn strokes_mut(&mut self) -> &mut Document {
        self.edits += 1;
        &mut self.doc
    }

    pub fn history(&self) -> &History {
        &self.history
    }

    pub fn doc_revision(&self) -> u64 {
        self.doc_revision
    }

    /// Counts every change to the document, including each small one a tool makes
    /// while it is at work.
    pub fn edit_count(&self) -> u64 {
        self.edits
    }

    pub fn state_revision(&self) -> u64 {
        self.state_revision
    }

    fn touch_state(&mut self) {
        self.state_revision += 1;
    }

    /// True when the document has changed since it was last saved or opened.
    pub fn is_dirty(&self) -> bool {
        self.edits != self.saved_edits
    }

    pub fn mark_saved(&mut self) {
        self.saved_edits = self.edits;
        self.touch_state();
    }

    pub fn clock(&self) -> Clock {
        self.playback.clock()
    }

    /// Evaluates the document at the playhead.
    pub fn evaluate(&self) -> RenderFrame {
        evaluate(&self.doc, self.clock())
    }

    /// The selected strokes.
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    pub fn selected_layers(&self) -> &[LayerId] {
        &self.selected_layers
    }

    /// The row last selected in the layer list.
    pub fn current_layer(&self) -> Option<LayerId> {
        self.selected_layers.last().copied()
    }

    pub fn paint_layer(&self) -> Option<LayerId> {
        self.paint_layer
    }

    pub fn transform_target(&self) -> Option<LayerId> {
        self.transform_target
    }

    /// Makes `id` the current layer, and the paint layer if it can be painted on.
    pub fn set_current_layer(&mut self, id: LayerId) {
        if self.doc.layer(id).is_none() {
            return;
        }
        self.selected_layers = vec![id];
        if self.doc.layer(id).is_some_and(|l| l.as_paint().is_some()) {
            self.paint_layer = Some(id);
        }
        self.touch_state();
    }

    /// Advances playback by `seconds` of real time.
    pub fn advance(&mut self, seconds: f64) -> Option<PlaybackEvent> {
        let before = (self.playback.playing, self.playback.is_waiting());
        let event = self
            .playback
            .advance(&self.doc, seconds, self.options.loop_range);
        if before != (self.playback.playing, self.playback.is_waiting()) {
            self.touch_state();
        }
        event
    }

    /// Stands the viewer on the document's default spawn area, if it has one.
    pub fn go_to_default_viewpoint(&mut self) {
        if let Some(id) = self.doc.settings.default_viewpoint {
            self.go_to_viewpoint(id);
        }
    }

    /// Stands the viewer on a spawn area (or at any layer's origin).
    pub fn go_to_viewpoint(&mut self, id: LayerId) {
        let mut placement = self.evaluate().world_xform(&self.doc, id);
        // A spawn area places and scales the viewer; a mirrored viewer makes no sense.
        placement.flip = crate::math::Flip::None;
        if placement.is_finite() && placement.scale > 0.0 {
            self.view.world_from_viewer = placement;
        }
    }

    /// World-space bounds of everything visible at the current time, as a centre and radius.
    pub fn visible_bounds(&self) -> Option<(DVec3, f64)> {
        let frame = self.evaluate();
        let mut min = DVec3::INFINITY;
        let mut max = DVec3::NEG_INFINITY;
        for state in frame.layers.iter().filter(|s| s.visible) {
            let Some(layer) = self.doc.layer(state.id) else {
                continue;
            };
            let media = |id: &Option<crate::model::MediaId>| {
                id.and_then(|id| self.doc.media(id)).map(|m| &m.data)
            };
            let bounds: Aabb = match (&layer.kind, state.content) {
                (LayerKind::Paint(_), crate::eval::Content::Paint { drawing, .. }) => self
                    .doc
                    .drawing(drawing)
                    .map_or(Aabb::EMPTY, |d| d.bounds()),
                // A flat picture is one unit tall and as wide as its shape makes it.
                // One that surrounds the viewer, or rides along with them, is not
                // something to stand back from.
                (LayerKind::Picture(picture), _)
                    if picture.projection == crate::model::Projection::Flat
                        && !picture.viewer_locked =>
                {
                    let (width, height) = media(&picture.media)
                        .and_then(|data| data.image_size())
                        .unwrap_or((1, 1));
                    let half = width as f32 / height.max(1) as f32 * 0.5;
                    Aabb::new(
                        glam::Vec3::new(-half, -0.5, 0.0),
                        glam::Vec3::new(half, 0.5, 0.0),
                    )
                }
                (LayerKind::Model(model), _) => match media(&model.media) {
                    Some(crate::model::MediaData::Mesh(mesh)) => {
                        let mut bounds = Aabb::EMPTY;
                        for position in &mesh.positions {
                            bounds.extend(glam::Vec3::from_array(*position));
                        }
                        bounds
                    }
                    _ => Aabb::EMPTY,
                },
                _ => Aabb::EMPTY,
            };
            if let Some((lo, hi)) = bounds.transformed(&state.world) {
                min = min.min(lo);
                max = max.max(hi);
            }
        }
        (min.x <= max.x).then(|| ((min + max) * 0.5, ((max - min).length() * 0.5).max(1e-9)))
    }

    // ---- transactions and history ----

    /// Starts an edit that spans several commands, such as a slider drag. Everything
    /// until the matching [`Session::commit_transaction`] becomes one undo step.
    pub fn begin_transaction(&mut self, label: impl Into<String>) {
        match &mut self.transaction {
            Some(transaction) => transaction.depth += 1,
            None => {
                self.transaction = Some(Transaction {
                    before: self.doc.clone(),
                    revision: self.edits,
                    label: label.into(),
                    depth: 1,
                })
            }
        }
    }

    pub fn commit_transaction(&mut self) {
        let Some(transaction) = &mut self.transaction else {
            return;
        };
        transaction.depth -= 1;
        if transaction.depth > 0 {
            return;
        }
        let transaction = self.transaction.take().expect("checked above");
        if transaction.revision != self.edits {
            // Many small changes can add up to none at all, such as a slider dragged
            // back to where it started. That is not a step worth undoing.
            if !self.doc.same_as(&transaction.before) {
                self.history.record(transaction.before, transaction.label);
            }
            self.after_document_change();
        }
    }

    /// Abandons the edit in progress and puts the document back as it was.
    pub fn cancel_transaction(&mut self) {
        if let Some(transaction) = self.transaction.take() {
            if transaction.revision != self.edits {
                self.doc = transaction.before;
                self.edits += 1;
                self.after_document_change();
            }
        }
    }

    pub fn in_transaction(&self) -> bool {
        self.transaction.is_some()
    }

    /// Runs an edit on the document as one undo step. If the edit fails the document
    /// is left as it was.
    pub(crate) fn edit<T>(
        &mut self,
        label: &str,
        edit: impl FnOnce(&mut Session) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let before = self.doc.clone();
        let revision = self.edits;
        match edit(self) {
            Ok(value) => {
                if self.edits != revision {
                    if self.transaction.is_none() && !self.doc.same_as(&before) {
                        self.history.record(before, label);
                    }
                    self.after_document_change();
                }
                Ok(value)
            }
            Err(error) => {
                if self.edits != revision {
                    self.doc = before;
                    self.edits += 1;
                    self.after_document_change();
                }
                Err(error)
            }
        }
    }

    /// Drops references to layers that no longer exist, after any change to the tree.
    fn after_document_change(&mut self) {
        self.selection.retain_existing(&self.doc);
        let doc = &self.doc;
        self.selected_layers.retain(|id| doc.layer(*id).is_some());
        if self
            .transform_target
            .is_some_and(|id| doc.layer(id).is_none())
        {
            self.transform_target = None;
        }
        if self
            .paint_layer
            .is_none_or(|id| doc.layer(id).is_none_or(|l| l.as_paint().is_none()))
        {
            self.paint_layer = self
                .selected_layers
                .iter()
                .rev()
                .copied()
                .find(|id| doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
                .or_else(|| self.first_paint_layer());
        }
        self.doc_revision += 1;
        self.touch_state();
        debug_assert_eq!(self.doc.validate(), Ok(()));
    }

    pub fn undo(&mut self) -> Option<String> {
        if self.transaction.is_some() {
            return None;
        }
        self.cancel_gesture();
        let (doc, label) = self.history.undo(&self.doc)?;
        self.doc = doc;
        self.edits += 1;
        self.selection_pivot = None;
        self.after_document_change();
        Some(label)
    }

    /// Redoes the step last undone. When there is none and the last thing done was to
    /// move a copy, makes another copy moved as far again.
    pub fn redo(&mut self) -> Option<String> {
        if self.transaction.is_some() {
            return None;
        }
        if !self.history.can_redo() && self.can_transform_again() {
            let label = Command::TransformAgain.label()?;
            return self
                .exec(Command::TransformAgain)
                .ok()
                .map(|_| label.to_owned());
        }
        let (doc, label) = self.history.redo(&self.doc)?;
        self.doc = doc;
        self.edits += 1;
        self.selection_pivot = None;
        self.after_document_change();
        Some(label)
    }

    /// Where a new stroke goes: the frame the paint layer shows now. A document with
    /// no paint layer at all gets one.
    pub(crate) fn paint_target(&mut self) -> Result<tools::PaintTarget, String> {
        let layer = match self.paint_layer {
            Some(layer) => layer,
            None => {
                let root = self.doc.root();
                let created = layers::add_layer(
                    self.doc_mut(),
                    NewLayerKind::Paint,
                    root,
                    usize::MAX,
                    None,
                    0,
                )
                .map_err(|e| e.to_string())?;
                self.set_current_layer(created);
                created
            }
        };
        let frame = self.evaluate();
        self.editable_drawing(&frame, layer)
    }

    // ---- commands ----

    /// Runs a command given as JSON. See [`Command`] for the vocabulary.
    pub fn exec_json(&mut self, command: &Value) -> Result<Value, CommandError> {
        let command: Command = serde_json::from_value(command.clone())
            .map_err(|e| CommandError::Malformed(e.to_string()))?;
        self.exec(command)
    }

    pub fn exec(&mut self, command: Command) -> Result<Value, CommandError> {
        let Some(label) = command.label() else {
            return self.run_state_command(command);
        };
        if command.is_cosmetic() {
            // Saved with the document, but not worth an undo step.
            let before = self.doc.clone();
            let result = self.run_edit(command);
            if result.is_err() {
                self.doc = before;
            }
            self.after_document_change();
            return result;
        }
        self.edit(label, |session| session.run_edit(command))
    }

    /// The time at which a key for `layer` would be written now: the playhead in the
    /// layer's own time base, on a frame boundary when snapping is on.
    pub fn key_time(&self, layer: LayerId) -> Flicks {
        let frame = self.evaluate();
        let time = frame
            .get(layer)
            .map_or(self.playback.time, |s| s.timeline_time);
        if self.options.snap_to_frames {
            self.doc.settings.rate.snap_floor(time)
        } else {
            time
        }
    }

    fn require_unlocked(&self, id: LayerId) -> Result<(), CommandError> {
        let layer = self
            .doc
            .layer(id)
            .ok_or(crate::ops::OpError::NoSuchLayer(id))?;
        if layer.locked {
            return Err(CommandError::Locked(layer.name.clone()));
        }
        Ok(())
    }

    /// Moves a layer: writes a key at the playhead when the layer's transform is
    /// animated or recording is on, and the resting transform otherwise.
    pub(crate) fn write_transform(
        &mut self,
        id: LayerId,
        value: Xform,
    ) -> Result<(), CommandError> {
        self.write_transform_as(id, value, false)
    }

    /// As [`Session::write_transform`]. A `quiet` write is one of many in a row, as
    /// while a layer is carried by the hand: interfaces are told when it is over.
    pub(crate) fn write_transform_as(
        &mut self,
        id: LayerId,
        value: Xform,
        quiet: bool,
    ) -> Result<(), CommandError> {
        if !(value.is_finite() && value.scale > 0.0) {
            return Err(CommandError::invalid("the transform is not usable"));
        }
        let time = self.key_time(id);
        let (recording, interp) = (self.options.recording, self.options.default_interp);
        let doc = if quiet {
            self.strokes_mut()
        } else {
            self.doc_mut()
        };
        let layer = doc
            .layer_mut(id)
            .ok_or(crate::ops::OpError::NoSuchLayer(id))?;
        let mut value = value;
        value.rotation = value.rotation.normalize();
        if !layer.anim.transform.is_empty() || recording {
            let interp = layer.anim.transform.get(time).map_or(interp, |k| k.interp);
            layer.anim.transform.set(time, value, interp);
        }
        layer.transform = value;
        Ok(())
    }

    /// Sets a layer's opacity, as a key when it is animated or recording is on.
    pub(crate) fn write_opacity(&mut self, id: LayerId, value: f32) -> Result<(), CommandError> {
        if !value.is_finite() {
            return Err(CommandError::invalid("the opacity is not a number"));
        }
        let value = value.clamp(0.0, 1.0);
        let time = self.key_time(id);
        let (recording, interp) = (self.options.recording, self.options.default_interp);
        let layer = self
            .doc_mut()
            .layer_mut(id)
            .ok_or(crate::ops::OpError::NoSuchLayer(id))?;
        if !layer.anim.opacity.is_empty() || recording {
            let interp = layer.anim.opacity.get(time).map_or(interp, |k| k.interp);
            layer.anim.opacity.set(time, value, interp);
        }
        layer.opacity = value;
        Ok(())
    }

    /// Where a new layer goes when the command does not say: inside the current
    /// layer when that is an open group, otherwise right after it.
    fn default_insertion_point(&self) -> (LayerId, usize) {
        let root = self.doc.root();
        match self.current_layer().and_then(|id| self.doc.layer(id)) {
            Some(layer) if layer.as_group().is_some() && (!layer.collapsed || layer.id == root) => {
                (layer.id, 0)
            }
            Some(layer) => match layer.parent {
                Some(parent) => (
                    parent,
                    self.doc
                        .index_in_parent(layer.id)
                        .map_or(usize::MAX, |i| i + 1),
                ),
                None => (root, 0),
            },
            None => (root, usize::MAX),
        }
    }

    /// Moves a layer under another parent without moving it in the world.
    fn reparent(&mut self, id: LayerId, parent: LayerId, index: usize) -> Result<(), CommandError> {
        let old_parent = self
            .doc
            .parent_of(id)
            .ok_or(crate::ops::OpError::NoSuchLayer(id))?;
        let frame = self.evaluate();
        self.doc_mut().move_layer(id, parent, index)?;
        self.keep_placement(&frame, id, old_parent)
    }

    /// After `id` moved from `old_parent` to wherever it is now, adjusts its transform
    /// so that it stays where it was in the world.
    fn keep_placement(
        &mut self,
        frame: &RenderFrame,
        id: LayerId,
        old_parent: LayerId,
    ) -> Result<(), CommandError> {
        let new_parent = self
            .doc
            .parent_of(id)
            .ok_or(crate::ops::OpError::NoSuchLayer(id))?;
        if new_parent == old_parent {
            return Ok(());
        }
        let delta = frame
            .world_xform(&self.doc, new_parent)
            .inverse()
            .mul(&frame.world_xform(&self.doc, old_parent));
        if delta.is_finite() && !delta.is_identity() {
            layers::rebase_transform(self.doc_mut(), id, &delta)?;
        }
        Ok(())
    }

    fn run_edit(&mut self, command: Command) -> Result<Value, CommandError> {
        let no_layer = |id: LayerId| CommandError::from(crate::ops::OpError::NoSuchLayer(id));
        match command {
            Command::SetDocument {
                title,
                description,
                background,
                rate,
                range_start,
                range_end,
                camera_resolution,
            } => {
                if rate.is_some_and(|r| !r.is_valid() || r.as_f64() > 480.0 || r.as_f64() < 1.0) {
                    return Err(CommandError::invalid(
                        "the frame rate must be between 1 and 480 frames per second",
                    ));
                }
                if background.is_some_and(|c| c.iter().any(|v| !v.is_finite() || *v < 0.0)) {
                    return Err(CommandError::invalid("the background colour is not usable"));
                }
                if camera_resolution.is_some_and(|r| r.iter().any(|v| !(16..=16384).contains(v))) {
                    return Err(CommandError::invalid(
                        "the camera resolution must be between 16 and 16384 pixels each way",
                    ));
                }
                let start = range_start.unwrap_or(self.doc.settings.range_start);
                let end = range_end.unwrap_or(self.doc.settings.range_end);
                if start < 0 || end <= start {
                    return Err(CommandError::invalid(
                        "the range must start at or after zero and end after it starts",
                    ));
                }
                let doc = self.doc_mut();
                let settings = &mut doc.settings;
                if let Some(title) = title {
                    settings.title = title;
                }
                if let Some(description) = description {
                    settings.description = description;
                }
                if let Some(background) = background {
                    settings.background = background;
                }
                if let Some(resolution) = camera_resolution {
                    settings.camera_resolution = resolution;
                }
                settings.range_start = start;
                settings.range_end = end;
                if let Some(rate) = rate {
                    // One frame rate for the whole document: every paint layer follows it.
                    settings.rate = rate;
                    for id in doc.walk() {
                        if let Some(paint) = doc.layer(id).and_then(|l| l.as_paint()) {
                            if paint.rate != rate {
                                doc.layer_mut(id)
                                    .and_then(|l| l.as_paint_mut())
                                    .expect("checked above")
                                    .rate = rate;
                            }
                        }
                    }
                }
                Ok(Value::Null)
            }
            Command::SetDefaultViewpoint { layer } => {
                if let Some(id) = layer {
                    if !matches!(
                        self.doc.layer(id).ok_or_else(|| no_layer(id))?.kind,
                        LayerKind::Viewpoint(_)
                    ) {
                        return Err(CommandError::invalid(
                            "only a spawn area can be the initial spawn area",
                        ));
                    }
                }
                self.doc_mut().settings.default_viewpoint = layer;
                Ok(Value::Null)
            }
            Command::SetPaletteColor { index, color } => {
                if index >= PALETTE_SIZE || color.iter().any(|v| !v.is_finite() || *v < 0.0) {
                    return Err(CommandError::invalid(
                        "there is no such palette slot, or the colour is not usable",
                    ));
                }
                let palette = &mut self.doc_mut().settings.palette;
                palette.resize(PALETTE_SIZE, [0.82, 0.82, 0.82]);
                palette[index] = color;
                Ok(Value::Null)
            }
            Command::AddLayer {
                kind,
                parent,
                index,
                name,
            } => {
                let (parent, index) = match parent {
                    Some(parent) => (parent, index.unwrap_or(usize::MAX)),
                    None => self.default_insertion_point(),
                };
                let clip_start = if self.options.create_layers_at_playhead {
                    self.doc
                        .settings
                        .rate
                        .snap_floor(self.evaluate().child_time(&self.doc, parent))
                        .max(0)
                } else {
                    0
                };
                let id = layers::add_layer(
                    self.doc_mut(),
                    kind,
                    parent,
                    index,
                    name.as_deref(),
                    clip_start,
                )?;
                if kind == NewLayerKind::Viewpoint && self.doc.settings.default_viewpoint.is_none()
                {
                    self.doc_mut().settings.default_viewpoint = Some(id);
                }
                self.set_current_layer(id);
                Ok(json!({ "layer": id }))
            }
            Command::DeleteLayers { layers: ids } => {
                let mut successor = None;
                for id in ids {
                    if self.doc.layer(id).is_none() {
                        continue; // Already gone with a parent deleted earlier in the list.
                    }
                    for member in self.doc.subtree(id) {
                        self.require_unlocked(member)?;
                    }
                    let parent = self
                        .doc
                        .parent_of(id)
                        .ok_or(crate::model::DocumentError::RootIsFixed)?;
                    let index = self.doc.index_in_parent(id).unwrap_or(0);
                    layers::delete_layer(self.doc_mut(), id)?;
                    let siblings = self.doc.children(parent);
                    successor = Some(
                        siblings
                            .get(index)
                            .or(siblings.last())
                            .copied()
                            .unwrap_or(parent),
                    );
                }
                if let Some(next) = successor {
                    self.set_current_layer(next);
                }
                Ok(Value::Null)
            }
            Command::DuplicateLayers { layers: ids } => {
                let mut copies = Vec::new();
                for id in ids {
                    copies.push(layers::duplicate_layer(self.doc_mut(), id)?);
                }
                if let Some(last) = copies.last() {
                    self.set_current_layer(*last);
                    self.selected_layers = copies.clone();
                }
                Ok(json!({ "layers": copies }))
            }
            Command::SetLayer {
                layer: id,
                name,
                visible,
                locked,
                collapsed,
                show_bounds,
                opacity,
                transform,
                pivot,
            } => {
                self.doc.layer(id).ok_or_else(|| no_layer(id))?;
                if opacity.is_some() || transform.is_some() || pivot.is_some() {
                    self.require_unlocked(id)?;
                }
                if let Some(name) = name {
                    let clean = layers::clean_name(&name);
                    if self.doc.layer(id).is_some_and(|l| l.name != clean) {
                        let unique = match self.doc.parent_of(id) {
                            Some(parent) => {
                                // The layer's own current name does not count as taken.
                                self.doc_mut().layer_mut(id).expect("checked above").name =
                                    String::new();
                                layers::unique_name(&self.doc, parent, &clean)
                            }
                            None => clean,
                        };
                        self.doc_mut().layer_mut(id).expect("checked above").name = unique;
                    }
                }
                if visible.is_some()
                    || locked.is_some()
                    || collapsed.is_some()
                    || show_bounds.is_some()
                {
                    let layer = self.doc_mut().layer_mut(id).expect("checked above");
                    layer.visible = visible.unwrap_or(layer.visible);
                    layer.locked = locked.unwrap_or(layer.locked);
                    layer.collapsed = collapsed.unwrap_or(layer.collapsed);
                    layer.show_bounds = show_bounds.unwrap_or(layer.show_bounds);
                }
                if let Some(opacity) = opacity {
                    self.write_opacity(id, opacity)?;
                }
                if let Some(transform) = transform {
                    self.write_transform(id, transform)?;
                }
                if let Some(pivot) = pivot {
                    if !(pivot.is_finite() && pivot.scale > 0.0) {
                        return Err(CommandError::invalid("the pivot is not usable"));
                    }
                    self.doc_mut().layer_mut(id).expect("checked above").pivot = pivot;
                }
                Ok(Value::Null)
            }
            Command::SetLayerKind { layer: id, values } => {
                self.doc.layer(id).ok_or_else(|| no_layer(id))?;
                self.require_unlocked(id)?;
                apply_kind_patch(
                    self.doc_mut().layer_mut(id).expect("checked above"),
                    &values,
                )
                .map_err(CommandError::Invalid)?;
                Ok(Value::Null)
            }
            Command::MoveLayer {
                layer: id,
                parent,
                index,
            } => {
                self.reparent(id, parent, index)?;
                Ok(Value::Null)
            }
            Command::StepLayer { layer: id, up } => {
                let old_parent = self
                    .doc
                    .parent_of(id)
                    .ok_or(crate::model::DocumentError::RootIsFixed)?;
                let frame = self.evaluate();
                let before = self.doc.index_in_parent(id);
                if up {
                    layers::move_layer_up(&mut self.doc, id)?;
                } else {
                    layers::move_layer_down(&mut self.doc, id)?;
                }
                if self.doc.parent_of(id) != Some(old_parent)
                    || self.doc.index_in_parent(id) != before
                {
                    self.edits += 1;
                    self.keep_placement(&frame, id, old_parent)?;
                }
                Ok(Value::Null)
            }
            Command::GroupLayers { layers: ids, name } => {
                let first = *ids
                    .first()
                    .ok_or_else(|| CommandError::invalid("no layers were given to group"))?;
                let parent = self
                    .doc
                    .parent_of(first)
                    .ok_or(crate::model::DocumentError::RootIsFixed)?;
                if ids.iter().any(|id| self.doc.parent_of(*id) != Some(parent)) {
                    return Err(CommandError::invalid(
                        "only layers in the same group can be grouped together",
                    ));
                }
                // Keep the order they have in the list, wherever they were clicked.
                let mut ordered: Vec<(usize, LayerId)> = ids
                    .iter()
                    .filter_map(|id| Some((self.doc.index_in_parent(*id)?, *id)))
                    .collect();
                ordered.sort_unstable();
                ordered.dedup();
                let group = layers::add_layer(
                    self.doc_mut(),
                    NewLayerKind::Group,
                    parent,
                    ordered[0].0,
                    name.as_deref(),
                    0,
                )?;
                for (_, id) in ordered {
                    self.doc_mut().move_layer(id, group, usize::MAX)?;
                }
                self.set_current_layer(group);
                Ok(json!({ "layer": group }))
            }
            Command::MergeDown { layer: id } => {
                self.require_unlocked(id)?;
                let merged = layers::merge_down(self.doc_mut(), id)?;
                self.set_current_layer(merged);
                Ok(json!({ "layer": merged }))
            }
            Command::FlattenGroup { layer: id } => {
                for member in self.doc.subtree(id) {
                    self.require_unlocked(member)?;
                }
                let flat = layers::flatten_group(self.doc_mut(), id)?;
                self.set_current_layer(flat);
                Ok(json!({ "layer": flat }))
            }
            Command::SetGroupIsSequence {
                layer: id,
                sequence,
            } => {
                if self
                    .doc
                    .layer(id)
                    .ok_or_else(|| no_layer(id))?
                    .is_sequence()
                    != sequence
                {
                    layers::set_group_is_sequence(self.doc_mut(), id, sequence)?;
                }
                Ok(Value::Null)
            }
            Command::FlipLayer { layer: id, axis } => {
                self.require_unlocked(id)?;
                if axis.is_mirrored() {
                    layers::flip_layer(self.doc_mut(), id, axis)?;
                }
                Ok(Value::Null)
            }
            Command::ResetTransform { layer: id } => {
                self.require_unlocked(id)?;
                self.write_transform(id, Xform::IDENTITY)?;
                Ok(Value::Null)
            }
            Command::ResetPivot { layer: id } => {
                self.require_unlocked(id)?;
                layers::reset_pivot(self.doc_mut(), id)?;
                Ok(Value::Null)
            }
            Command::MakePivotOrigin { layer: id } => {
                self.require_unlocked(id)?;
                layers::make_pivot_origin(self.doc_mut(), id)?;
                Ok(Value::Null)
            }
            Command::AddFrame { layer, kind } => self.add_frame(layer, kind),
            Command::AddHoldFrame { layer, after } => self.add_hold_frame(layer, after),
            Command::DeleteFrame { layer } => {
                self.edit_frames(layer, crate::ops::frames::delete_frame)
            }
            Command::RemoveKeyFrame { layer } => {
                self.edit_frames(layer, crate::ops::frames::remove_key_frame)
            }
            Command::ClearFrames { layer } => self.edit_frames(layer, |doc, id, _| {
                crate::ops::frames::clear_frames(doc, id)
            }),
            Command::DoubleFrames { layer } => self.edit_frames(layer, |doc, id, _| {
                crate::ops::frames::double_frames(doc, id)
            }),
            Command::ReverseFrames { layer } => self.edit_frames(layer, |doc, id, _| {
                crate::ops::frames::reverse_frames(doc, id)
            }),
            Command::CopySelectionToFrames { all } => self.copy_selection_to_frames(all),
            Command::ToggleClip { layer, time } => self.edit_clips(
                layer,
                animation::clip_op(layer, animation::toggle_clip_at),
                time,
            ),
            Command::SplitClip { layer, time } => self.edit_clips(
                layer,
                animation::clip_op(layer, animation::split_clip),
                time,
            ),
            Command::MoveClip { layer, clip, start } => self.edit_clips(
                layer,
                move |doc, _| animation::move_clip(doc, layer, clip, start),
                Some(0),
            ),
            Command::TrimClip {
                layer,
                clip,
                start,
                end,
                open_end,
            } => {
                let end = if open_end { Some(None) } else { end.map(Some) };
                self.edit_clips(
                    layer,
                    move |doc, _| animation::trim_clip(doc, layer, clip, start, end),
                    Some(0),
                )
            }
            Command::DeleteClip { layer, clip } => self.edit_clips(
                layer,
                move |doc, _| animation::delete_clip(doc, layer, clip),
                Some(0),
            ),
            Command::DuplicateClip { layer, clip, start } => self.edit_clips(
                layer,
                move |doc, _| animation::duplicate_clip(doc, layer, clip, start),
                Some(0),
            ),
            Command::SetKey {
                layer,
                track,
                time,
                interp,
                children,
            } => self.set_key(layer, track, time, interp, children),
            Command::DeleteKeys {
                layer,
                track,
                times,
            } => self.edit_track(layer, track, times, animation::TrackEdit::Delete),
            Command::MoveKeys {
                layer,
                track,
                times,
                delta,
            } => self.edit_track(layer, track, times, animation::TrackEdit::Move(delta)),
            Command::SetKeyInterp {
                layer,
                track,
                times,
                interp,
            } => self.edit_track(layer, track, times, animation::TrackEdit::SetInterp(interp)),
            Command::ReverseKeys {
                layer,
                track,
                times,
            } => self.edit_track(layer, track, times, animation::TrackEdit::Reverse),
            Command::PasteKeys { layer, time } => self.paste_keys(layer, time),
            Command::SetAction { time, action } => self.set_action(time, action),
            Command::SetLoopPoint { layer, enabled } => self.set_loop_point(layer, enabled),
            Command::BakeSequence { layer } => self.bake_sequence(layer),
            Command::DeleteSelection => self.delete_selection(),
            Command::SelectionToNewLayer { copy } => self.selection_to_new_layer(copy),
            Command::Optimize { scope } => self.optimize(scope),
            Command::TransformAgain => self.transform_again(),
            other => unreachable!("{other:?} does not edit the document"),
        }
    }

    fn run_state_command(&mut self, command: Command) -> Result<Value, CommandError> {
        let result = match command {
            Command::Undo => json!({ "undone": self.undo() }),
            Command::Redo => json!({ "redone": self.redo() }),
            Command::CopyKeys {
                layer,
                track,
                times,
            } => self.copy_keys(layer, track, times)?,
            Command::GoToKeyFrame { layer, forward } => {
                self.go_to_key_frame(layer, forward)?;
                Value::Null
            }
            Command::GoToKey {
                layer,
                track,
                forward,
            } => {
                self.go_to_key(layer, track, forward)?;
                Value::Null
            }
            Command::SelectAll => {
                self.select_all();
                json!({ "selected": self.selection.count() })
            }
            Command::Deselect => {
                self.deselect();
                Value::Null
            }
            Command::SelectLayers { layers: ids, mode } => {
                let ids: Vec<LayerId> = ids
                    .into_iter()
                    .filter(|id| self.doc.layer(*id).is_some())
                    .collect();
                match mode {
                    SelectMode::Replace => self.selected_layers = ids,
                    SelectMode::Add => {
                        self.selected_layers.retain(|id| !ids.contains(id));
                        self.selected_layers.extend(ids);
                    }
                    SelectMode::Remove => self.selected_layers.retain(|id| !ids.contains(id)),
                    SelectMode::Toggle => {
                        for id in ids {
                            match self.selected_layers.iter().position(|s| *s == id) {
                                Some(at) => {
                                    self.selected_layers.remove(at);
                                }
                                None => self.selected_layers.push(id),
                            }
                        }
                    }
                }
                if let Some(paint) = self
                    .selected_layers
                    .iter()
                    .rev()
                    .copied()
                    .find(|id| self.doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
                {
                    self.paint_layer = Some(paint);
                }
                Value::Null
            }
            Command::SetTransformTarget { layer } => {
                if let Some(id) = layer {
                    if id == self.doc.root() || self.doc.layer(id).is_none() {
                        return Err(CommandError::invalid("that layer cannot be moved"));
                    }
                }
                self.transform_target = layer;
                Value::Null
            }
            Command::SetOptions { values } => {
                let mut options = patched(&self.options, &values).map_err(CommandError::Invalid)?;
                options.sanitize();
                if options.level_horizon && !self.options.level_horizon {
                    self.view.level(self.last_input.head);
                }
                self.options = options;
                Value::Null
            }
            Command::SetTool { tool } => {
                self.tools.select_tool(tool);
                Value::Null
            }
            Command::SetToolSettings { values } => {
                let mut tools = patched(&self.tools, &values).map_err(CommandError::Invalid)?;
                tools.sanitize();
                self.tools = tools;
                Value::Null
            }
            Command::SetColor { color } => {
                if color.iter().any(|v| !v.is_finite() || *v < 0.0) {
                    return Err(CommandError::invalid("the colour is not usable"));
                }
                self.tools.color = color;
                Value::Null
            }
            Command::Play => {
                self.playback.play(&self.doc, self.options.loop_range);
                Value::Null
            }
            Command::Pause => {
                self.playback.pause();
                Value::Null
            }
            Command::TogglePlay => {
                if self.playback.playing && !self.playback.is_waiting() {
                    self.playback.pause();
                } else {
                    self.playback.play(&self.doc, self.options.loop_range);
                }
                Value::Null
            }
            Command::Seek { time } => {
                self.playback.seek(time);
                Value::Null
            }
            Command::StepFrames { frames } => {
                let rate = self.doc.settings.rate;
                self.playback.pause();
                self.playback
                    .seek(rate.frame_start((rate.frame_at(self.playback.time) + frames).max(0)));
                Value::Null
            }
            Command::Rewind => {
                self.playback.seek(self.playback.previous_mark(&self.doc));
                Value::Null
            }
            Command::FastForward => {
                self.playback.seek(self.playback.next_mark(&self.doc));
                Value::Null
            }
            other => unreachable!("{other:?} edits the document"),
        };
        self.touch_state();
        Ok(result)
    }
}

#[cfg(test)]
mod animation_tests;
#[cfg(test)]
mod grab_tests;
#[cfg(test)]
mod tests;
