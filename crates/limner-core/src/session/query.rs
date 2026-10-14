//! What an interface needs to know to draw itself, as JSON values.

use super::{kind_props, Session};
use crate::eval::Content;
use crate::model::{Layer, LayerId, LayerKind, Track};
use crate::time::{Flicks, FLICKS_PER_SECOND};
use serde::Serialize;
use serde_json::{json, Value};

/// The kind of a layer as the interface names it. Sequences are groups with a clock.
pub fn kind_name(layer: &Layer) -> &'static str {
    match &layer.kind {
        LayerKind::Group(g) if g.sequence.is_some() => "sequence",
        LayerKind::Group(_) => "group",
        LayerKind::Paint(_) => "paint",
        LayerKind::Picture(_) => "picture",
        LayerKind::Model(_) => "model",
        LayerKind::Sound(_) => "sound",
        LayerKind::Viewpoint(_) => "viewpoint",
        LayerKind::Camera(_) => "camera",
        LayerKind::Light(_) => "light",
    }
}

/// The clips of a layer: spans between a `true` visibility key and the next `false` one.
/// An open clip has no end.
pub fn clips(visibility: &Track<bool>) -> Vec<(Flicks, Option<Flicks>)> {
    let mut out = Vec::new();
    let mut open: Option<Flicks> = None;
    for key in visibility.keys() {
        match (open, key.value) {
            (None, true) => open = Some(key.time),
            (Some(start), false) => {
                out.push((start, Some(key.time)));
                open = None;
            }
            _ => {}
        }
    }
    if let Some(start) = open {
        out.push((start, None));
    }
    out
}

fn key_list<T: Serialize>(track: &Track<T>) -> Value {
    Value::Array(
        track
            .keys()
            .iter()
            .map(|k| json!({ "time": k.time, "value": k.value, "interp": k.interp }))
            .collect(),
    )
}

impl Session {
    /// A small summary that is cheap enough to ask for every frame.
    pub fn status(&self) -> Value {
        let rate = self.doc.settings.rate;
        json!({
            "doc_revision": self.doc_revision,
            "state_revision": self.state_revision,
            "dirty": self.is_dirty(),
            "can_undo": self.history.can_undo(),
            // With nothing to redo, Redo repeats the last move of a copy.
            "can_redo": self.history.can_redo() || self.can_transform_again(),
            "undo_label": self.history.undo_label(),
            "redo_label": self.history.redo_label().or(self.can_transform_again().then_some("Transform again")),
            "can_transform_again": self.can_transform_again(),
            "time": self.playback.time,
            "seconds": crate::time::to_seconds(self.playback.time),
            "frame": rate.frame_at(self.playback.time),
            "playing": self.playback.playing,
            "waiting": self.playback.is_waiting(),
            "tool": self.tools.tool,
            "current_layer": self.current_layer(),
            "paint_layer": self.paint_layer,
            "transform_target": self.transform_target,
            "selection_count": self.selection.count(),
            "tool_active": self.is_tool_active(),
            "grabbing": self.grabbed().is_some(),
            // The track the copied keys came from, when keys have been copied.
            "copied_keys": self.copied_keys(),
        })
    }

    /// One row per layer in list order, parents before children.
    pub fn layer_rows(&self) -> Value {
        let frame = self.evaluate();
        let mut rows = Vec::with_capacity(self.doc.layer_count());
        let mut folded_at: Option<usize> = None;
        for id in self.doc.walk() {
            let Some(layer) = self.doc.layer(id) else {
                continue;
            };
            let depth = self.doc.depth(id);
            if folded_at.is_some_and(|d| depth <= d) {
                folded_at = None;
            }
            let hidden_in_list = folded_at.is_some();
            if folded_at.is_none() && layer.collapsed && layer.as_group().is_some() {
                folded_at = Some(depth);
            }
            let state = frame.get(id);
            let (strokes, frames) = match &layer.kind {
                LayerKind::Paint(paint) => {
                    let shown = match state.map(|s| s.content) {
                        Some(Content::Paint { drawing, .. }) => {
                            self.doc.drawing(drawing).map_or(0, |d| d.stroke_count())
                        }
                        _ => 0,
                    };
                    (shown, paint.frames.len())
                }
                _ => (0, 0),
            };
            rows.push(json!({
                "id": id,
                "parent": layer.parent,
                "depth": depth,
                "name": layer.name,
                "kind": kind_name(layer),
                "visible": layer.visible,
                "locked": layer.locked,
                "collapsed": layer.collapsed,
                "show_bounds": layer.show_bounds,
                "opacity": state.map_or(layer.opacity, |s| s.own_opacity),
                "shown": state.is_some_and(|s| s.visible),
                "in_clip": state.is_some_and(|s| s.in_clip),
                "hidden_in_list": hidden_in_list,
                "children": layer.children().len(),
                "strokes": strokes,
                "frames": frames,
                "animated": layer.anim.has_motion_keys(),
                "selected": self.selected_layers.contains(&id),
                "current": self.current_layer() == Some(id),
                "paint_target": self.paint_layer == Some(id),
                "transform_target": self.transform_target == Some(id),
                "default_viewpoint": self.doc.settings.default_viewpoint == Some(id),
            }));
        }
        Value::Array(rows)
    }

    /// Everything about one layer, for a properties panel.
    pub fn layer_details(&self, id: LayerId) -> Value {
        let Some(layer) = self.doc.layer(id) else {
            return Value::Null;
        };
        let frame = self.evaluate();
        let state = frame.get(id);
        let paint = layer.as_paint().map(|paint| {
            json!({
                "frames": paint.frames,
                "drawings": paint.drawings.len(),
                "rate": paint.rate,
                "max_repeat": paint.max_repeat,
                "frame": match state.map(|s| s.content) { Some(Content::Paint { frame, .. }) => frame, _ => 0 },
            })
        });
        json!({
            "id": id,
            "path": self.doc.path_of(id),
            "name": layer.name,
            "kind": kind_name(layer),
            "visible": layer.visible,
            "locked": layer.locked,
            "opacity": state.map_or(layer.opacity, |s| s.own_opacity),
            "transform": state.map_or(layer.transform, |s| s.local),
            "pivot": layer.pivot,
            "props": kind_props(layer),
            "paint": paint,
            "timeline_time": state.map_or(0, |s| s.timeline_time),
            "local_time": state.map_or(0, |s| s.local_time),
            "key_time": self.key_time(id),
            "clips": clips(&layer.anim.visibility).into_iter().map(|(start, end)| json!({ "start": start, "end": end })).collect::<Vec<_>>(),
            "keys": {
                "transform": key_list(&layer.anim.transform),
                "opacity": key_list(&layer.anim.opacity),
                "offset": key_list(&layer.anim.offset),
                "actions": key_list(&layer.anim.actions),
            },
        })
    }

    pub fn document_info(&self) -> Value {
        let settings = &self.doc.settings;
        json!({
            "title": settings.title,
            "description": settings.description,
            "background": settings.background,
            "ambient_light": settings.ambient_light,
            "rate": settings.rate,
            "fps": settings.rate.as_f64(),
            "frame_duration": settings.rate.frame_duration(),
            "range_start": settings.range_start,
            "range_end": settings.range_end,
            "default_viewpoint": settings.default_viewpoint,
            "camera_resolution": settings.camera_resolution,
            "palette": settings.palette,
            "root": self.doc.root(),
            "layers": self.doc.layer_count(),
            "strokes": self.doc.stroke_count(),
            "points": self.doc.point_count(),
            "flicks_per_second": FLICKS_PER_SECOND,
        })
    }

    /// The working state of the editor, saved beside the document so that a project
    /// reopens the way it was left.
    pub fn editor_state(&self) -> Value {
        json!({
            "version": 1,
            "tools": self.tools,
            "options": self.options,
            "paint_layer": self.paint_layer,
            "current_layer": self.current_layer(),
            "transform_target": self.transform_target,
            "view": self.view.world_from_viewer,
            "time": self.playback.time,
        })
    }

    /// Restores what [`Session::editor_state`] saved. Anything missing or unreadable keeps its default.
    pub fn restore_editor_state(&mut self, state: &Value) {
        if let Ok(mut tools) = serde_json::from_value::<super::ToolSettings>(state["tools"].clone())
        {
            tools.sanitize();
            self.tools = tools;
        }
        if let Ok(mut options) = serde_json::from_value::<super::Options>(state["options"].clone())
        {
            options.sanitize();
            // Recording is a momentary mode; a project never reopens with it armed.
            options.recording = false;
            self.options = options;
        }
        let layer = |key: &str| {
            state[key]
                .as_u64()
                .map(LayerId)
                .filter(|id| self.doc.layer(*id).is_some())
        };
        let (paint, current, target) = (
            layer("paint_layer"),
            layer("current_layer"),
            layer("transform_target"),
        );
        if let Some(current) = current {
            self.selected_layers = vec![current];
        }
        if let Some(paint) =
            paint.filter(|id| self.doc.layer(*id).is_some_and(|l| l.as_paint().is_some()))
        {
            self.paint_layer = Some(paint);
        }
        self.transform_target = target.filter(|id| *id != self.doc.root());
        if let Ok(view) = serde_json::from_value::<crate::math::Xform>(state["view"].clone()) {
            if view.is_finite() && view.scale > 0.0 {
                self.view.world_from_viewer = view;
            }
        }
        if let Some(time) = state["time"].as_i64() {
            self.playback.seek(time);
        }
        self.touch_state();
    }
}
