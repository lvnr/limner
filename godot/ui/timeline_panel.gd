class_name TimelinePanel
extends VBoxContainer
## The Timeline: playback, the working range, and every layer's clips, frames and keys.
##
## From the top: a grip that makes the panel taller or shorter, the playback and
## option buttons, the commands for the current layer (they change with its kind),
## and the tracks, which `TimelineView` draws. What is selected in the tracks (a
## clip, some keys) is kept here; everything else is read from the editor whenever
## it says something changed.

const DEFAULT_HEIGHT := 230.0
const MIN_HEIGHT := 150.0
## Frame rates always offered; the document's own is added when it is another.
const RATES := [6, 8, 10, 12, 15, 24, 25, 30, 60]
const INTERPS := ["step", "linear", "ease_in", "ease_out", "ease_in_out", "smooth"]
const INTERP_NAMES := ["Step", "Linear", "Ease in", "Ease out", "Ease in and out", "Smooth"]
## Onion skin counts in the order the button steps through them; 0 is all frames.
const ONION_COUNTS := [1, 2, 3, 0]

var editor: LimnerEditor

## The layer rows that are listed (not folded away inside a group), in list order.
var rows: Array = []
## What the timeline draws for each layer, by layer id. See "Timeline rows" in docs/interface-api.md.
var timeline := {}
var info := {}
var options := {}

## The kind of key the key commands act on and the tracks let you select:
## "transform", "opacity" or "action".
var key_track := "transform"
## The selected clip: its layer and its number in that layer's clips, or -1 and -1.
var selected_clip_layer := -1
var selected_clip := -1
## The selected keys of the kind `key_track`: layer id to a list of key times on
## that layer's own clock.
var selected_keys := {}
## Whether setting a key also keys every layer inside the current one.
var key_children := false
## Whether a drag that starts on a clip selects the keys it passes over instead of
## moving the clip. (A drag that starts on empty track space always does.)
var select_range := false
## The height the panel takes when it is not collapsed.
var height := DEFAULT_HEIGHT
var collapsed := false
## Whether the time readout shows a frame number instead of a timecode.
var show_frames := false

var view: TimelineView
## Every button and drop-down, by name.
var controls := {}

var _bar: HFlowContainer
var _context: HFlowContainer
## The run of controls being built. See `_run_of`.
var _box: HBoxContainer
var _grip: Control
var _timecode: Button
var _rate: OptionButton
var _rate_values: Array = []
var _interp: OptionButton
var _key_type: ChoiceRow
## The buttons that show an editor option, by option name.
var _option_buttons := {}
## The controls of each group of the context row, by group name.
var _groups := {}
## The layer each layer is inside of, by id; -1 for the root.
var _parents := {}
var _refresh_queued := false
var _selected_clip_start := 0
var _content_end := -1
var _current := -2
var _has_frames := false
var _shown_time := -1
var _shown_frame := -1
var _shown_playing := false
var _grip_from := NAN
var _grip_height := 0.0
## The drawings of each paint layer that hold no strokes: layer id to {drawing: true}.
var _empty_drawings := {}
## The document revision the rows were last read at. See `refresh`.
var _read_revision := -1


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Timeline"
	add_theme_constant_override("separation", 4)
	custom_minimum_size = Vector2(0, height)

	_grip = Control.new()
	_grip.name = "Grip"
	_grip.custom_minimum_size = Vector2(0, 8)
	_grip.mouse_default_cursor_shape = Control.CURSOR_VSIZE
	_grip.tooltip_text = "Drag to make the timeline taller or shorter"
	_grip.draw.connect(_draw_grip)
	_grip.gui_input.connect(_on_grip_input)
	add_child(_grip)

	_bar = _flow()
	add_child(_bar)
	_build_transport()
	_build_options()
	_context = _flow()
	add_child(_context)
	_build_context()
	view = TimelineView.new(self)
	add_child(view)


func _ready() -> void:
	editor.document_changed.connect(_queue_refresh)
	editor.state_changed.connect(_queue_refresh)
	editor.document_replaced.connect(_on_document_replaced)
	refresh()


func _process(_delta: float) -> void:
	# Only the playhead is followed here; everything else waits for a signal.
	if not is_visible_in_tree():
		return
	var status := editor.status
	var time := int(status.get("time", 0))
	var playing: bool = status.get("playing", false)
	if time == _shown_time and playing == _shown_playing:
		return
	if playing and not collapsed and _has_frames and int(status.get("frame", 0)) != _shown_frame and view.axis.frame_width() >= TimelineView.MIN_FRAME_PIXELS:
		# The frame each paint layer shows is part of the rows; while playing no
		# signal says that it moved on.
		_read_clock()
		view.queue_redraw()
	_show_playhead()


## The document and the editor's state often change in the same frame; the panel
## is brought up to date once for both.
func _queue_refresh() -> void:
	if not _refresh_queued:
		_refresh_queued = true
		_refresh_queued_now.call_deferred()


func _refresh_queued_now() -> void:
	_refresh_queued = false
	refresh()


## Brings the panel in line with the document and the editor's state.
func refresh() -> void:
	info = editor.get_info()
	options = editor.get_options()
	var all := editor.get_rows()
	rows = all.filter(func(row: Dictionary) -> bool: return not row.hidden_in_list)
	_parents = {}
	for row: Dictionary in all:
		_parents[int(row.id)] = -1 if row.parent == null else int(row.parent)
	# Clips, frames and keys change only with the document. When it is the editor's
	# state that changed (the playhead, the current layer, an option), only the
	# part of the rows that moves with the playhead is read again.
	if timeline.is_empty() or int(editor.status.get("doc_revision", -1)) != _read_revision:
		_read_timeline()
	else:
		_read_clock()
	_follow_current_layer()
	_prune_selection()
	_update_controls()
	_show_playhead()
	view.refresh()


## Reads again what the tracks draw, straight from the document, without waiting
## for the editor's signal: a drag shows its result at once. With `settings`, the
## document's settings (the working range) are read again too.
func reload(settings := false) -> void:
	if settings:
		info = editor.scene.document_info()
	_read_timeline()
	_prune_selection()
	_update_controls()
	view.refresh()


func _on_document_replaced() -> void:
	selected_clip_layer = -1
	selected_clip = -1
	selected_keys = {}
	_current = -2
	_read_revision = -1
	refresh()
	view.fit()


func _read_timeline() -> void:
	timeline = {}
	_empty_drawings = {}
	_has_frames = false
	for line: Dictionary in editor.scene.timeline_rows():
		var id := int(line.id)
		timeline[id] = line
		if line.paint == null:
			continue
		if (line.paint.frames as Array).size() > 1:
			_has_frames = true
		var empty := {}
		for drawing: int in line.paint.empty:
			empty[drawing] = true
		_empty_drawings[id] = empty
	_content_end = -1
	_read_revision = int(editor.scene.status().doc_revision)


## Brings the part of the rows that moves with the playhead up to date: where each
## layer's clock stands and the frame a paint layer shows.
func _read_clock() -> void:
	for tick: Dictionary in editor.scene.timeline_clock():
		var line: Dictionary = timeline.get(int(tick.id), {})
		if line.is_empty():
			continue
		line.origin = tick.origin
		line.time = tick.time
		line.local_time = tick.local_time
		if line.paint != null:
			(line.paint as Dictionary).frame = tick.frame
	# Layers inside a sequence sit where its clip at the playhead puts them.
	_content_end = -1


func _show_playhead() -> void:
	var status := editor.status
	_shown_time = int(status.get("time", 0))
	_shown_frame = int(status.get("frame", 0))
	_shown_playing = status.get("playing", false)
	_timecode.text = time_text()
	var play: Button = controls.play
	play.icon = Icons.texture("pause" if _shown_playing else "play", Kit.TEXT, 20)
	play.tooltip_text = "Stop" if _shown_playing else "Play"
	view.set_playhead(_shown_time, true)


## The playhead as H:MM:SS.FF, or as a frame number when the readout was pressed.
func time_text() -> String:
	var status := editor.status
	var frame := int(status.get("frame", 0))
	if show_frames:
		return str(frame)
	@warning_ignore("integer_division")
	var seconds := int(status.get("time", 0)) / TimelineScale.FLICKS
	var in_second := frame - view.axis.frame_at(seconds * TimelineScale.FLICKS)
	@warning_ignore("integer_division")
	return "%d:%02d:%02d.%02d" % [seconds / 3600, seconds / 60 % 60, seconds % 60, in_second]


## The end of everything that has an end: clips, drawn frames, keys, chapter marks
## and loops. Zero when nothing has.
func content_end() -> int:
	if _content_end >= 0:
		return _content_end
	var end := 0
	for id: int in timeline:
		var line: Dictionary = timeline[id]
		var origin := int(line.origin)
		for clip: Dictionary in line.clips:
			var zero := origin + int(clip.start) - int(clip.trim)
			var reach := 0
			if clip.end != null:
				reach = origin + int(clip.end)
			elif line.paint != null and (line.paint.frames as Array).size() > 1:
				reach = zero + (line.paint.frames as Array).size() * int(line.paint.frame_duration)
			elif line.sequence != null and int(line.sequence.loop_duration) > 0 and id != int(info.get("root", -1)):
				reach = zero + int(line.sequence.loop_duration)
			end = maxi(end, reach)
		for track: String in TimelineView.TRACKS:
			var keys := TimelineView.keys_of(line, track)
			if not keys.is_empty():
				end = maxi(end, origin + int(keys[-1].time))
	_content_end = end
	return end


## The time on a layer's own clock at which a key or clip made now would land:
## the playhead there, moved back to a frame when snapping to frames is on.
func key_time(line: Dictionary) -> int:
	var time := maxi(int(line.get("time", 0)), 0)
	return view.axis.snap_floor(time) if options.get("snap_to_frames", false) else time


## Whether the playhead is inside a clip of every layer that `layer` is inside of,
## which is when the layer plays at all.
func is_inside_clips(layer: int) -> bool:
	var id: int = _parents.get(layer, -1)
	while id >= 0:
		var line: Dictionary = timeline.get(id, {})
		var now := int(line.get("time", 0))
		var inside := false
		for clip: Dictionary in line.get("clips", []):
			if now >= int(clip.start) and (clip.end == null or now < int(clip.end)):
				inside = true
				break
		if not inside:
			return false
		id = _parents.get(id, -1)
	return true


## Whether a drawing of a paint layer holds no strokes.
func is_empty_drawing(layer: int, drawing: int) -> bool:
	var empty: Dictionary = _empty_drawings.get(layer, {})
	return empty.has(drawing)


# ---- selection ----

func select_clip(layer: int, clip: int) -> void:
	var line: Dictionary = timeline.get(layer, {})
	if line.is_empty() or clip < 0 or clip >= (line.clips as Array).size():
		layer = -1
		clip = -1
	if layer == selected_clip_layer and clip == selected_clip:
		return
	selected_clip_layer = layer
	selected_clip = clip
	_selected_clip_start = int(line.clips[clip].start) if layer >= 0 else 0
	_update_controls()
	view.queue_redraw()


## Selects keys of one kind: `chosen` maps layer ids to key times.
func select_keys(track: String, chosen: Dictionary) -> void:
	key_track = track
	selected_keys = {}
	for id: int in chosen:
		var times: Array = (chosen[id] as Array).duplicate()
		if not times.is_empty():
			times.sort()
			selected_keys[id] = times
	_update_controls()
	view.queue_redraw()


## Chooses the kind of key the tracks select and the key commands act on.
func set_key_track(track: String) -> void:
	if track != key_track:
		select_keys(track, {})


## Makes `layer` the current layer, if it is not.
func make_current(layer: int) -> void:
	if editor.current_layer() != layer:
		editor.run({"cmd": "select_layers", "layers": [layer]})


func _follow_current_layer() -> void:
	var current := editor.current_layer()
	if current == _current:
		return
	_current = current
	var is_root := current >= 0 and current == int(info.get("root", -1))
	# Chapter marks are the root's keys; no other layer has them.
	if is_root and key_track != "action" and selected_keys.is_empty():
		key_track = "action"
	elif not is_root and key_track == "action":
		key_track = "transform"
		selected_keys = {}
	if selected_clip_layer != current:
		selected_clip_layer = -1
		selected_clip = -1


## Drops from the selection what is no longer in the document.
func _prune_selection() -> void:
	if selected_clip_layer >= 0:
		var line: Dictionary = timeline.get(selected_clip_layer, {})
		var clips: Array = line.get("clips", [])
		var found := -1
		if view != null and view.is_dragging():
			# The clip being dragged keeps its place among the others.
			found = selected_clip if selected_clip < clips.size() else -1
		else:
			for number in clips.size():
				if int(clips[number].start) == _selected_clip_start:
					found = number
		if found < 0:
			selected_clip_layer = -1
		else:
			_selected_clip_start = int(clips[found].start)
		selected_clip = found
	for id: int in selected_keys.keys():
		var line: Dictionary = timeline.get(id, {})
		if line.is_empty():
			selected_keys.erase(id)
			continue
		var there := {}
		for key: Dictionary in TimelineView.keys_of(line, key_track):
			there[int(key.time)] = true
		var times: Array = (selected_keys[id] as Array).filter(func(time: int) -> bool: return there.has(time))
		if times.is_empty():
			selected_keys.erase(id)
		else:
			selected_keys[id] = times


# ---- building ----

func _flow() -> HFlowContainer:
	var flow := HFlowContainer.new()
	flow.add_theme_constant_override("h_separation", 9)
	flow.add_theme_constant_override("v_separation", 3)
	return flow


## Starts a run of controls that stay side by side when the row wraps. With `group`,
## the run belongs to that group of the context row, and `heading` names the group.
func _run_of(flow: HFlowContainer, group := "", heading := "") -> void:
	_box = HBoxContainer.new()
	_box.add_theme_constant_override("separation", 3)
	flow.add_child(_box)
	if group == "":
		return
	if not _groups.has(group):
		_groups[group] = []
	_groups[group].append(_box)
	if heading != "":
		var label := Kit.heading(heading)
		label.custom_minimum_size = Vector2(0, Kit.BUTTON)
		_box.add_child(label)


func _add(key: String, control: Control) -> Control:
	controls[key] = control
	_box.add_child(control)
	return control


func _button(key: String, icon: String, tooltip: String, action: Callable) -> Button:
	var button := Kit.icon_button(icon, tooltip, action)
	_add(key, button)
	return button


## A button that stays down while something is on. `action` is called with the new state.
func _toggle(key: String, icon: String, tooltip: String, action: Callable) -> Button:
	var button := Kit.icon_button(icon, tooltip, Callable(), true)
	button.toggled.connect(action)
	_add(key, button)
	return button


## A button that shows and switches the editor option `key`.
func _option_toggle(key: String, icon: String, tooltip: String) -> Button:
	var button := _toggle(key, icon, tooltip, func(on: bool) -> void: editor.set_option(key, on))
	_option_buttons[key] = button
	return button


func _build_transport() -> void:
	_run_of(_bar)
	_button("rewind", "rewind", "Rewind to the chapter before, or the start of the range", func() -> void: editor.run({"cmd": "rewind"}))
	_button("previous_frame", "prev_frame", "Go to the previous frame", func() -> void: editor.run({"cmd": "step_frames", "frames": -1}))
	_button("play", "play", "Play", func() -> void: editor.run({"cmd": "toggle_play"}))
	var record := _option_toggle("recording", "record", "Record: while on, moving a layer or changing its opacity writes a key at the playhead")
	record.add_theme_stylebox_override("pressed", Kit._box(Kit.DANGER))
	record.add_theme_stylebox_override("hover_pressed", Kit._box(Kit.DANGER.lightened(0.12)))
	_button("next_frame", "next_frame", "Go to the next frame", func() -> void: editor.run({"cmd": "step_frames", "frames": 1}))
	_button("fast_forward", "forward", "Forward to the next chapter, or the end of the range", func() -> void: editor.run({"cmd": "fast_forward"}))

	_run_of(_bar)
	_timecode = Kit.button("0:00:00.00", "Time at the playhead as hours, minutes, seconds and frames. Press to show the frame number instead.", _on_timecode_pressed)
	_timecode.custom_minimum_size = Vector2(98, Kit.BUTTON)
	_add("timecode", _timecode)
	_rate = Kit.option([], "Frames per second of the document", _on_rate_selected)
	_rate.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_rate.custom_minimum_size = Vector2(86, Kit.BUTTON)
	_add("rate", _rate)

	_run_of(_bar)
	_option_toggle("mute", "mute", "Mute all sounds")
	_option_toggle("show_extras", "tl_extras", "Show the extras: the outlines of spawn areas, cameras, lights and sounds")
	_option_toggle("preview_spawn_area", "viewpoint", "Preview the spawn area: watch from the initial spawn area while playing")

	_run_of(_bar)
	_button("zoom_out", "tl_zoom_out", "Show more time", func() -> void: view.zoom_by(1.0 / 1.5))
	_button("zoom_in", "tl_zoom_in", "Show less time, larger", func() -> void: view.zoom_by(1.5))
	_button("fit", "tl_fit", "Show everything", func() -> void: view.fit())


func _build_options() -> void:
	_run_of(_bar)
	_option_toggle("onion_skin", "onion", "Onion skin: show the frames around the current one faintly")
	var count := Kit.button("1", "How many frames the onion skin shows each way: 1, 2, 3 or all", _on_onion_count)
	count.custom_minimum_size = Vector2(40, Kit.BUTTON)
	_add("onion_frames", count)

	_run_of(_bar)
	_button("range_start", "tl_range_start", "Start the working range at the playhead", _on_range_start)
	_button("range_end", "tl_range_end", "End the working range at the playhead", _on_range_end)
	_button("range_all", "tl_range_all", "Make the working range the whole piece", _on_range_all)
	_button("range_clip", "tl_range_clip", "Make the working range the selected clip", _on_range_clip)
	_option_toggle("loop_range", "loop", "Play the working range over and over")

	_run_of(_bar)
	_option_toggle("snap_to_clips", "snap", "Snap to clips: what is dragged sticks to the playhead and to the ends of clips")
	_option_toggle("snap_to_frames", "tl_snap_frames", "Snap to frames: the playhead, clips and keys stay on whole frames")
	_option_toggle("create_layers_at_playhead", "tl_layer_at_playhead", "New layers begin at the playhead instead of at the start")

	_run_of(_bar)
	_box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_box.alignment = BoxContainer.ALIGNMENT_END
	var resize := _button("resize", "tl_resize", "Drag up or down to make the timeline taller or shorter", Callable())
	resize.mouse_default_cursor_shape = Control.CURSOR_VSIZE
	resize.gui_input.connect(_on_grip_input)
	_button("collapse", "tl_chevron_down", "Hide the tracks", func() -> void: set_collapsed(not collapsed))


func _build_context() -> void:
	var flow := _context
	_run_of(flow, "frames", "Frames")
	_button("key_frame_previous", "tl_key_frame_prev", "Go to the key frame before", func() -> void: _on_layer("go_to_key_frame", {"forward": false}))
	_button("key_frame_next", "tl_key_frame_next", "Go to the next key frame", func() -> void: _on_layer("go_to_key_frame", {"forward": true}))
	_button("hold_before", "tl_hold_before", "Add a hold frame before this one", func() -> void: _on_layer("add_hold_frame", {"after": false}))
	_button("hold_after", "tl_hold_after", "Add a hold frame after this one: the drawing shows one frame longer", func() -> void: _on_layer("add_hold_frame", {"after": true}))
	_button("frame_delete", "tl_frame_delete", "Delete this frame; the animation gets one frame shorter", func() -> void: _on_layer("delete_frame"))
	_run_of(flow, "frames")
	_button("frame_blank", "tl_frame_blank", "Add a blank key frame", func() -> void: _on_layer("add_frame", {"kind": "blank"}))
	_button("frame_duplicate", "tl_frame_duplicate", "Add a key frame that is a copy of this one", func() -> void: _on_layer("add_frame", {"kind": "duplicate"}))
	_button("key_frame_remove", "tl_key_frame_remove", "Remove this key frame: the drawing before it is held instead", func() -> void: _on_layer("remove_key_frame"))
	_button("copy_to_frame", "tl_paste_frame", "Copy the selected strokes into this frame", func() -> void: _run({"cmd": "copy_selection_to_frames", "all": false}))
	_button("copy_to_all_frames", "tl_paste_frames_all", "Copy the selected strokes into every frame", func() -> void: _run({"cmd": "copy_selection_to_frames", "all": true}))
	_run_of(flow, "frames")
	_button("frames_clear", "tl_frames_clear", "Delete all frames but the first", func() -> void: _on_layer("clear_frames"))
	_button("frames_double", "tl_frames_double", "Double the frames: every frame is shown twice as long", func() -> void: _on_layer("double_frames"))
	_button("frames_reverse", "tl_reverse", "Reverse the order of the frames", _on_frames_reverse)
	_toggle("frames_loop", "repeat", "Loop: the frames repeat for as long as the clip lasts", func(on: bool) -> void: _on_layer("set_layer_kind", {"values": {"max_repeat": 0 if on else 1}}))

	_run_of(flow, "sequence", "Sequence")
	_toggle("loop_point", "tl_loop_point", "Loop point: the sequence starts over from the playhead", func(on: bool) -> void: _on_layer("set_loop_point", {"enabled": on}))
	_button("bake", "tl_bake", "Bake one loop of the sequence into a frame-by-frame paint layer", func() -> void: _on_layer("bake_sequence"))

	_run_of(flow, "marks", "Chapters")
	for action: String in ["stop", "play", "loop"]:
		var tooltip: String = {
			"stop": "Add a Stop at the playhead: playback waits there for the viewer",
			"play": "Add a Play mark at the playhead: a chapter starts there, and rewinding lands on it",
			"loop": "Add a Loop at the playhead: the piece starts over from there",
		}[action]
		var mark := Kit.button(action.capitalize(), tooltip, func() -> void: _run({"cmd": "set_action", "action": action}))
		mark.icon = Icons.texture("tl_flag", TimelineView.MARK_COLORS[action], 18)
		_add("mark_" + action, mark)
	_button("mark_remove", "tl_flag_remove", "Remove the chapter mark at the playhead", func() -> void: _run({"cmd": "set_action", "action": null}))

	_run_of(flow, "clips", "Clips")
	_button("clip_toggle", "tl_clip_toggle", "End the clip at the playhead, or start a new one there", func() -> void: _on_layer("toggle_clip"))
	_button("clip_split", "tl_clip_split", "Split the clip at the playhead", func() -> void: _on_layer("split_clip"))
	_button("clip_open_end", "tl_clip_open_end", "Remove the end of the selected clip, so that it never stops", _on_clip_open_end)
	_button("clip_delete", "tl_clip_delete", "Delete the selected clip", _on_clip_delete)
	_button("clip_duplicate", "tl_clip_duplicate", "Show the selected clip again, starting at the playhead", _on_clip_duplicate)

	_run_of(flow, "keys", "Keys")
	_key_type = ChoiceRow.new()
	_key_type.add_icon("transform", "move", "Transform keys: where the layer is, how it is turned and how large")
	_key_type.add_icon("opacity", "tl_opacity", "Opacity keys")
	_key_type.add_icon("action", "tl_flag", "Chapter marks (the root layer only)")
	_key_type.chosen.connect(set_key_track)
	_add("key_type", _key_type)
	_run_of(flow, "keys")
	_button("key_previous", "tl_key_prev", "Go to the key before", func() -> void: _on_layer("go_to_key", {"track": key_track, "forward": false}))
	_button("key_set", "tl_key_add", "Set a key at the playhead", _on_key_set)
	_toggle("key_children", "tl_key_children", "Also key every layer inside this one", func(on: bool) -> void: key_children = on)
	_button("key_delete", "tl_key_delete", "Delete the selected keys, or the key at the playhead", _on_key_delete)
	_button("key_next", "tl_key_next", "Go to the next key", func() -> void: _on_layer("go_to_key", {"track": key_track, "forward": true}))
	_run_of(flow, "keys")
	_interp = Kit.option(INTERP_NAMES, "How a value moves on from a key to the next: applied to the selected keys, and used for new keys", _on_key_interp)
	_interp.size_flags_horizontal = Control.SIZE_SHRINK_BEGIN
	_interp.custom_minimum_size = Vector2(108, Kit.BUTTON)
	_interp.fit_to_longest_item = false
	_interp.clip_text = true
	_add("key_interp", _interp)
	_toggle("key_select_range", "bbox", "Select keys by dragging across the tracks. While this is on, clips stay where they are.", func(on: bool) -> void: select_range = on)
	_button("key_select_all", "tl_key_select_all", "Select all keys of this kind on the layer", _on_key_select_all)
	_run_of(flow, "keys")
	_button("key_copy", "duplicate", "Copy the selected keys", _on_key_copy)
	_button("key_paste", "tl_paste", "Paste the copied keys, the first of them at the playhead", func() -> void: _on_layer("paste_keys"))
	_button("key_reverse", "tl_reverse", "Reverse the selected keys in time", _on_key_reverse)


# ---- showing the state ----

func _enable(key: String, enabled: bool) -> void:
	var button: BaseButton = controls[key]
	button.disabled = not enabled


func _show_group(group: String, shown: bool) -> void:
	for control: Control in _groups[group]:
		control.visible = shown


func _update_controls() -> void:
	if view == null:
		return
	for key: String in _option_buttons:
		Kit.set_quiet(_option_buttons[key], bool(options.get(key, false)))
	var recording: bool = options.get("recording", false)
	(controls.recording as Button).icon = Icons.texture("record", Color.WHITE if recording else Kit.DANGER.lightened(0.15), 20)
	(controls.mute as Button).icon = Icons.texture("mute" if options.get("mute", false) else "sound", Kit.TEXT, 20)
	var onion := int(options.get("onion_frames", 1))
	(controls.onion_frames as Button).text = "All" if onion == 0 else str(onion)
	_show_rates()
	_enable("range_clip", selected_clip_layer >= 0 and timeline.has(selected_clip_layer))
	(controls.collapse as Button).icon = Icons.texture("tl_chevron_up" if collapsed else "tl_chevron_down", Kit.TEXT, 20)
	(controls.collapse as Button).tooltip_text = "Show the tracks" if collapsed else "Hide the tracks"

	var current := editor.row(editor.current_layer())
	var has := not current.is_empty()
	var id := int(current.id) if has else -1
	var line: Dictionary = timeline.get(id, {})
	has = has and not line.is_empty()
	var kind := str(current.get("kind", ""))
	var is_root: bool = has and current.parent == null
	var editable: bool = has and not current.locked
	var is_paint := has and kind == "paint" and line.paint != null
	var is_sequence := has and kind == "sequence" and not is_root
	_show_group("frames", not is_sequence and not is_root)
	_show_group("sequence", is_sequence)
	_show_group("marks", is_root)

	# Frames of a paint layer.
	var frames: Array = line.paint.frames if is_paint else []
	var frame := int(line.paint.frame) if is_paint else 0
	var count := frames.size()
	var on_key_frame := frame < count and (frame == 0 or int(frames[frame]) != int(frames[frame - 1]))
	var can_frames := is_paint and editable
	_enable("key_frame_previous", is_paint and count > 1)
	_enable("key_frame_next", is_paint and count > 1)
	for key: String in ["hold_before", "hold_after", "frame_blank", "frame_duplicate", "frames_double", "frames_loop"]:
		_enable(key, can_frames)
	for key: String in ["frame_delete", "frames_clear", "frames_reverse"]:
		_enable(key, can_frames and count > 1)
	_enable("key_frame_remove", can_frames and on_key_frame and frame > 0)
	var strokes_selected := int(editor.status.get("selection_count", 0)) > 0
	_enable("copy_to_frame", can_frames and strokes_selected)
	_enable("copy_to_all_frames", can_frames and strokes_selected)
	Kit.set_quiet(controls.frames_loop, is_paint and int(line.paint.max_repeat) == 0)

	# A sequence's loop.
	var loops := is_sequence and line.sequence != null and int(line.sequence.loop_duration) > 0
	_enable("loop_point", is_sequence and editable)
	Kit.set_quiet(controls.loop_point, loops)
	_enable("bake", loops)

	# Chapter marks on the root.
	var now := key_time(line) if has else 0
	for key: String in ["mark_stop", "mark_play", "mark_loop"]:
		_enable(key, is_root)
	_enable("mark_remove", is_root and _has_key(line, "action", now))

	# Clips.
	var clips: Array = line.get("clips", [])
	var can_clips := editable and not is_root
	var inside := false
	for clip: Dictionary in clips:
		if now > int(clip.start) and (clip.end == null or now < int(clip.end)):
			inside = true
	var selected_clips: Array = timeline.get(selected_clip_layer, {}).get("clips", [])
	var clip_selected := selected_clip >= 0 and selected_clip < selected_clips.size()
	var selected_has_end := false
	if clip_selected:
		var chosen: Dictionary = selected_clips[selected_clip]
		selected_has_end = chosen.end != null
	_enable("clip_toggle", can_clips)
	_enable("clip_split", can_clips and inside)
	_enable("clip_open_end", clip_selected and selected_has_end)
	_enable("clip_delete", clip_selected)
	_enable("clip_duplicate", clip_selected)

	# Keys.
	_key_type.set_value(key_track)
	(_key_type.buttons.action as Button).disabled = not is_root
	var keys: Array = TimelineView.keys_of(line, key_track) if has else []
	var any_selected := not selected_keys.is_empty()
	var most := 0
	for layer: int in selected_keys:
		most = maxi(most, (selected_keys[layer] as Array).size())
	_enable("key_previous", not keys.is_empty())
	_enable("key_next", not keys.is_empty())
	_enable("key_set", editable and key_track != "action")
	_enable("key_children", has and kind in ["group", "sequence"] and key_track != "action")
	Kit.set_quiet(controls.key_children, key_children)
	Kit.set_quiet(controls.key_select_range, select_range)
	_enable("key_delete", any_selected or (editable and _has_key(line, key_track, now)))
	_enable("key_select_all", not keys.is_empty())
	_enable("key_copy", any_selected)
	# Chapter marks can only be pasted onto the root, which is the one layer that has them.
	var copied: Variant = editor.status.get("copied_keys")
	_enable("key_paste", editable and copied != null and (copied != "action" or is_root))
	_enable("key_reverse", most >= 2)
	_interp.disabled = key_track == "action"
	Kit.set_quiet(_interp, maxi(INTERPS.find(_shown_interp()), 0))


## The easing the drop-down shows: that of the selected keys when they agree,
## otherwise the one new keys get.
func _shown_interp() -> String:
	var shared := ""
	for id: int in selected_keys:
		var line: Dictionary = timeline.get(id, {})
		if line.is_empty() or key_track == "action":
			continue
		var times: Array = selected_keys[id]
		for key: Dictionary in TimelineView.keys_of(line, key_track):
			if times.has(int(key.time)):
				if shared != "" and shared != str(key.interp):
					return str(options.get("default_interp", "linear"))
				shared = str(key.interp)
	return shared if shared != "" else str(options.get("default_interp", "linear"))


func _has_key(line: Dictionary, track: String, time: int) -> bool:
	if line.is_empty():
		return false
	var keys := TimelineView.keys_of(line, track)
	var at := TimelineView.first_at_or_after(keys, time)
	return at < keys.size() and int(keys[at].time) == time


func _show_rates() -> void:
	var rate: Dictionary = info.get("rate", {"num": 24, "den": 1})
	var own := [int(rate.num), int(rate.den)]
	var values := []
	for fps: int in RATES:
		values.append([fps, 1])
	if not values.has(own):
		values.append(own)
		values.sort_custom(func(a: Array, b: Array) -> bool: return float(a[0]) / a[1] < float(b[0]) / b[1])
	if values != _rate_values:
		_rate_values = values
		_rate.clear()
		for value: Array in values:
			_rate.add_item("%d fps" % value[0] if value[1] == 1 else "%.3f fps" % (float(value[0]) / value[1]))
	_rate.select(values.find(own))


func _draw_grip() -> void:
	var middle := _grip.size * 0.5
	_grip.draw_rect(Rect2(middle.x - 22.0, middle.y - 1.5, 44.0, 3.0), Color(1.0, 1.0, 1.0, 0.22))


# ---- the size of the panel ----

## Sets the height the panel takes while it is not collapsed.
func set_height(value: float) -> void:
	height = maxf(value, MIN_HEIGHT)
	if not collapsed:
		custom_minimum_size.y = height


## Hides everything under the rows of buttons, or shows it again.
func set_collapsed(on: bool) -> void:
	collapsed = on
	_context.visible = not on
	view.visible = not on
	custom_minimum_size.y = 0.0 if on else height
	_update_controls()


func _on_grip_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var button: InputEventMouseButton = event
		if button.button_index == MOUSE_BUTTON_LEFT:
			_grip_from = button.global_position.y if button.pressed and not collapsed else NAN
			_grip_height = size.y
	elif event is InputEventMouseMotion and not is_nan(_grip_from):
		var motion: InputEventMouseMotion = event
		var wanted := _grip_height - (motion.global_position.y - _grip_from)
		# Leave some of the window to the picture.
		var most := get_viewport_rect().size.y * 0.8
		set_height(minf(wanted, most) if most > MIN_HEIGHT else wanted)


# ---- commands ----

## Runs a command. When it fails the buttons go back to showing what is.
func _run(command: Dictionary) -> Variant:
	var result: Variant = editor.run(command)
	if result == null:
		_update_controls()
	return result


## Runs a command that takes the current layer and, perhaps, `more` fields.
func _on_layer(command: String, more := {}) -> Variant:
	var id := editor.current_layer()
	if id < 0:
		_update_controls()
		return null
	var request := {"cmd": command, "layer": id}
	request.merge(more)
	return _run(request)


func _on_frames_reverse() -> void:
	_on_layer("reverse_frames")


func _on_timecode_pressed() -> void:
	show_frames = not show_frames
	_timecode.text = time_text()
	_timecode.tooltip_text = "Frame at the playhead. Press to show the time instead." if show_frames else "Time at the playhead as hours, minutes, seconds and frames. Press to show the frame number instead."


func _on_rate_selected(index: int) -> void:
	var value: Array = _rate_values[index]
	var rate: Dictionary = info.get("rate", {})
	if int(rate.get("num", 0)) != value[0] or int(rate.get("den", 0)) != value[1]:
		_run({"cmd": "set_document", "rate": {"num": value[0], "den": value[1]}})


func _on_onion_count() -> void:
	var at := ONION_COUNTS.find(int(options.get("onion_frames", 1)))
	editor.set_option("onion_frames", ONION_COUNTS[(at + 1) % ONION_COUNTS.size()])


func _on_range_start() -> void:
	var time := int(editor.status.get("time", 0))
	var start := int(info.range_start)
	var end := int(info.range_end)
	var request := {"cmd": "set_document", "range_start": time}
	if time >= end:
		# The range keeps its length when its start is put past its end.
		request["range_end"] = time + (end - start)
	_run(request)


func _on_range_end() -> void:
	var time := int(editor.status.get("time", 0))
	var start := int(info.range_start)
	var end := int(info.range_end)
	var request := {"cmd": "set_document", "range_end": time}
	if time <= start:
		request["range_start"] = maxi(time - (end - start), 0)
	_run(request)


func _on_range_all() -> void:
	var end := content_end()
	_run({"cmd": "set_document", "range_start": 0, "range_end": end if end > 0 else int(info.range_end)})


func _on_range_clip() -> void:
	var line: Dictionary = timeline.get(selected_clip_layer, {})
	if line.is_empty():
		return
	var clip: Dictionary = line.clips[selected_clip]
	var start := int(line.origin) + int(clip.start)
	var end := 0
	if clip.end != null:
		end = int(line.origin) + int(clip.end)
	else:
		# A clip that never stops: up to where the piece ends, or as long as the range was.
		end = content_end()
		if end <= start:
			end = start + int(info.range_end) - int(info.range_start)
	_run({"cmd": "set_document", "range_start": start, "range_end": end})


func _on_clip_open_end() -> void:
	if selected_clip_layer >= 0:
		_run({"cmd": "trim_clip", "layer": selected_clip_layer, "clip": selected_clip, "open_end": true})


func _on_clip_delete() -> void:
	if selected_clip_layer < 0:
		return
	var layer := selected_clip_layer
	var clip := selected_clip
	select_clip(-1, -1)
	_run({"cmd": "delete_clip", "layer": layer, "clip": clip})


func _on_clip_duplicate() -> void:
	var line: Dictionary = timeline.get(selected_clip_layer, {})
	if line.is_empty():
		return
	var start := key_time(line)
	if _run({"cmd": "duplicate_clip", "layer": selected_clip_layer, "clip": selected_clip, "start": start}) != null:
		# The copy is the one selected afterwards.
		_selected_clip_start = start
		reload()


func _on_key_set() -> void:
	_on_layer("set_key", {"track": key_track, "children": key_children})


## Runs a key command on the selected keys of every layer that has some, as one undo step.
func _on_selected_keys(label: String, command: String, more := {}) -> void:
	editor.begin(label)
	for id: int in selected_keys:
		var request := {"cmd": command, "layer": id, "track": key_track, "times": selected_keys[id]}
		request.merge(more)
		editor.run(request)
	editor.commit()


func _on_key_delete() -> void:
	if selected_keys.is_empty():
		_on_layer("delete_keys", {"track": key_track})
		return
	_on_selected_keys("Delete keys", "delete_keys")
	select_keys(key_track, {})


func _on_key_interp(index: int) -> void:
	var interp: String = INTERPS[index]
	if not selected_keys.is_empty() and key_track != "action":
		_on_selected_keys("Change key easing", "set_key_interp", {"interp": interp})
	editor.set_option("default_interp", interp)


func _on_key_select_all() -> void:
	var id := editor.current_layer()
	var line: Dictionary = timeline.get(id, {})
	if line.is_empty():
		return
	var times := TimelineView.keys_of(line, key_track).map(func(key: Dictionary) -> int: return int(key.time))
	select_keys(key_track, {id: times})


func _on_key_copy() -> void:
	if selected_keys.is_empty():
		return
	# One layer's keys are copied at a time: the current layer's when it has some selected.
	var id := editor.current_layer()
	if not selected_keys.has(id):
		id = selected_keys.keys()[0]
	if _run({"cmd": "copy_keys", "layer": id, "track": key_track, "times": selected_keys[id]}) != null:
		editor.message.emit("Copied %d keys" % (selected_keys[id] as Array).size(), false)


func _on_key_reverse() -> void:
	var reversed := {}
	editor.begin("Reverse keys")
	for id: int in selected_keys:
		var times: Array = selected_keys[id]
		if times.size() >= 2:
			editor.run({"cmd": "reverse_keys", "layer": id, "track": key_track, "times": times})
			var span := int(times.min()) + int(times.max())
			reversed[id] = times.map(func(time: int) -> int: return span - time)
		else:
			reversed[id] = times
	editor.commit()
	select_keys(key_track, reversed)
	reload()
