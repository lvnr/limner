extends LimnerTest
## The timeline shows what the document holds, follows the playhead, and sends the
## right commands from its buttons and from presses and drags on its tracks.

const SECOND := LimnerEditor.FLICKS_PER_SECOND
## One frame at the 24 frames per second of a new document.
const FRAME := 29_400_000


func _panel(size := Vector2(900, 260)) -> TimelinePanel:
	var panel := TimelinePanel.new(editor)
	mount(panel, size)
	await frames(3)
	return panel


func _paint() -> int:
	return int(row_named("Paint").id)


## What the document says the timeline should draw for a layer, read afresh.
func _line(id: int) -> Dictionary:
	for line: Dictionary in world.scene.timeline_rows():
		if int(line.id) == id:
			return line
	failures.append("no timeline row for layer %d" % id)
	return {}


func _press(panel: TimelinePanel, key: String) -> void:
	var button: BaseButton = panel.controls[key]
	check(not button.disabled, "%s can be pressed" % key)
	button.pressed.emit()


func _switch(panel: TimelinePanel, key: String, on: bool) -> void:
	var button: BaseButton = panel.controls[key]
	check(not button.disabled, "%s can be switched" % key)
	button.button_pressed = on


func _times(keys: Array) -> Array:
	return keys.map(func(key: Dictionary) -> int: return int(key.time))


func _status() -> Dictionary:
	return world.scene.status()


# ---- transport ----

func test_transport_buttons_move_the_playhead_and_follow_playback() -> void:
	var panel := await _panel()
	_press(panel, "next_frame")
	_press(panel, "next_frame")
	check_eq(int(_status().frame), 2, "frame after two steps forward")
	_press(panel, "previous_frame")
	check_eq(int(_status().time), FRAME, "time after one step back")
	await frames(2)
	check_eq(panel.view.playhead, FRAME, "the playhead line follows")
	check_eq(panel.time_text(), "0:00:00.01", "timecode")
	_press(panel, "timecode")
	check_eq((panel.controls.timecode as Button).text, "1", "the readout as a frame number")
	_press(panel, "timecode")
	check_eq((panel.controls.timecode as Button).text, "0:00:00.01", "and as a timecode again")

	_press(panel, "play")
	await frames(3)
	check_eq(_status().playing, true, "playing")
	check_eq((panel.controls.play as Button).tooltip_text, "Stop", "the play button offers to stop")
	_press(panel, "play")
	await frames(2)
	check_eq(_status().playing, false, "stopped")
	check_eq((panel.controls.play as Button).tooltip_text, "Play", "the play button offers to play")

	_press(panel, "fast_forward")
	check_eq(int(_status().time), 10 * SECOND, "forward goes to the end of the range")
	_press(panel, "rewind")
	check_eq(int(_status().time), 0, "rewind goes to its start")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_option_buttons_show_and_switch_the_options() -> void:
	var panel := await _panel()
	for key: String in ["recording", "mute", "preview_spawn_area", "onion_skin", "create_layers_at_playhead"]:
		check_eq((panel.controls[key] as Button).button_pressed, false, "%s starts off" % key)
		_switch(panel, key, true)
		check_eq(world.scene.options()[key], true, "%s switched on" % key)
	for key: String in ["show_extras", "loop_range", "snap_to_clips", "snap_to_frames"]:
		check_eq((panel.controls[key] as Button).button_pressed, true, "%s starts on" % key)
		_switch(panel, key, false)
		check_eq(world.scene.options()[key], false, "%s switched off" % key)
	# The buttons follow options changed elsewhere.
	run({"cmd": "set_options", "values": {"recording": false, "snap_to_frames": true}})
	await frames(2)
	check_eq((panel.controls.recording as Button).button_pressed, false, "record follows the option")
	check_eq((panel.controls.snap_to_frames as Button).button_pressed, true, "snap follows the option")

	var shown := []
	for step in 4:
		_press(panel, "onion_frames")
		await frames(2)
		shown.append([int(world.scene.options().onion_frames), (panel.controls.onion_frames as Button).text])
	check_eq(shown, [[2, "2"], [3, "3"], [0, "All"], [1, "1"]], "the onion skin count steps through 1, 2, 3, all")


func test_frame_rate_menu_sets_the_document_rate() -> void:
	var panel := await _panel()
	var menu: OptionButton = panel.controls.rate
	check_eq(menu.get_item_text(menu.selected), "24 fps", "rate shown")
	check_eq(menu.item_count, TimelinePanel.RATES.size(), "the usual rates")
	menu.item_selected.emit(TimelinePanel.RATES.find(12))
	await frames(2)
	check_eq(editor.get_info().rate, {"num": 12, "den": 1}, "rate after choosing 12")
	check_eq(menu.get_item_text(menu.selected), "12 fps", "rate shown after choosing")
	# A rate that is not on the list is offered while the document has it.
	run({"cmd": "set_document", "rate": {"num": 24000, "den": 1001}})
	await frames(2)
	check_eq(menu.item_count, TimelinePanel.RATES.size() + 1, "the document's own rate is added")
	check_eq(menu.get_item_text(menu.selected), "23.976 fps", "and shown")
	check_eq(panel.view.axis.frame_start(1), int(editor.get_info().frame_duration), "the frame grid agrees with the document")


func test_range_buttons_set_the_working_range() -> void:
	var panel := await _panel()
	var paint := _paint()
	run({"cmd": "seek", "time": 2 * SECOND})
	await frames(2)
	_press(panel, "range_start")
	run({"cmd": "seek", "time": 6 * SECOND})
	await frames(2)
	_press(panel, "range_end")
	await frames(2)
	check_eq([int(panel.info.range_start), int(panel.info.range_end)], [2 * SECOND, 6 * SECOND], "range set at the playhead")
	# A start put after the end moves the range along.
	run({"cmd": "seek", "time": 8 * SECOND})
	await frames(2)
	_press(panel, "range_start")
	await frames(2)
	check_eq([int(panel.info.range_start), int(panel.info.range_end)], [8 * SECOND, 12 * SECOND], "range moved along")

	check((panel.controls.range_clip as Button).disabled, "no clip selected: no range from a clip")
	run({"cmd": "toggle_clip", "layer": paint, "time": 3 * SECOND})
	run({"cmd": "toggle_clip", "layer": paint, "time": 4 * SECOND})
	run({"cmd": "toggle_clip", "layer": paint, "time": 5 * SECOND})
	await frames(2)
	panel.select_clip(paint, 1)
	_press(panel, "range_clip")
	await frames(2)
	check_eq([int(panel.info.range_start), int(panel.info.range_end)], [4 * SECOND, 5 * SECOND], "range of the selected clip")
	_press(panel, "range_all")
	await frames(2)
	check_eq([int(panel.info.range_start), int(panel.info.range_end)], [0, 5 * SECOND], "range of the whole piece")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_dragging_the_ends_of_the_range() -> void:
	var panel := await _panel()
	var view := panel.view
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	var grip := Vector2(view.x_of(10 * SECOND) - 3.0, TimelineView.RULER - 4.0)
	view.press(grip)
	view.drag(Vector2(view.x_of(7 * SECOND), grip.y))
	view.drag(Vector2(view.x_of(5 * SECOND + FRAME / 3), grip.y))
	check_eq(int(panel.info.range_end), 5 * SECOND, "the tracks show the range while it is dragged")
	view.release(grip)
	await frames(2)
	check_eq(int(editor.get_info().range_end), 5 * SECOND, "range end after the drag, on a frame")
	check_eq(int(_status().time), 0, "dragging an end of the range does not move the playhead")
	editor.undo()
	await frames(2)
	check_eq(int(editor.get_info().range_end), 10 * SECOND, "one undo puts the range back")
	check_eq(_status().can_undo, false, "and it was the only step")


# ---- tracks: rows, ruler, scale ----

func test_time_and_pixels_convert_both_ways_at_any_zoom() -> void:
	var axis := TimelineScale.new()
	axis.left = 162.0
	for zoom: float in [0.15, 0.5, 7.3, 96.0, 1440.0, 6000.0]:
		for start: float in [0.0, 12.34, 3000.5]:
			axis.zoom = zoom
			axis.start = start
			for time: int in [0, 1, FRAME, 7 * SECOND + 13, 3599 * SECOND + 12345]:
				check_eq(axis.time_at(axis.x_of(time)), time, "time at zoom %s, start %s" % [zoom, start])
			for x: float in [0.0, 162.0, 400.25, 1900.0]:
				if axis.time_at(x) >= 0:
					check_near(axis.x_of(axis.time_at(x)), x, "pixel at zoom %s, start %s" % [zoom, start], 0.001)
	# Zooming keeps the time under the pointer where it is.
	axis.zoom = 96.0
	axis.start = 4.0
	var under := axis.time_at(500.0)
	axis.zoom_about(500.0, 3.0)
	check_near(axis.zoom, 288.0, "zoom after zooming in")
	check(absi(axis.time_at(500.0) - under) <= 1, "the time under the pointer stays")
	# The frame grid.
	check_eq(axis.frame_at(FRAME * 5 + 7), 5, "frame of a time")
	check_eq(axis.frame_start(5), FRAME * 5, "start of a frame")
	check_eq(axis.snap(FRAME * 5 + FRAME / 2 + 1), FRAME * 6, "nearest frame above")
	check_eq(axis.snap(FRAME * 5 + FRAME / 2 - 1), FRAME * 5, "nearest frame below")
	axis.rate_num = 24000
	axis.rate_den = 1001
	check_eq(axis.frame_at(axis.frame_start(1000)), 1000, "frames of a rate that is not whole")
	check_eq(axis.frame_at(axis.frame_start(1000) - 1), 999, "and the instant before")


func test_rows_follow_the_layer_list() -> void:
	var panel := await _panel(Vector2(900, 420))
	var view := panel.view
	check_eq(panel.rows.map(func(row: Dictionary) -> String: return row.name), ["Root", "Spawn Area", "Paint"], "rows of a new document")
	var group: Dictionary = run({"cmd": "add_layer", "kind": "group", "name": "Body"})
	run({"cmd": "add_layer", "kind": "paint", "parent": int(group.layer), "name": "Arm"})
	await frames(2)
	check_eq(panel.rows.size(), 5, "rows after adding a group with a layer in it")
	check_eq(view.row_of(int(row_named("Arm").id)), 4, "the new layer is listed inside its group")
	check_eq(view.row_index_at(view.row_top(3) + 5.0), 3, "rows are found by where they are drawn")

	# Pressing a name makes that layer the current one.
	view.press(Vector2(70.0, view.row_top(1) + 10.0))
	await frames(2)
	check_eq(editor.current_layer(), int(row_named("Spawn Area").id), "the layer pressed is current")

	# Folding a group takes what is inside it off the list, here as in the layer list.
	var body := view.row_of(int(group.layer))
	view.press(Vector2(5.0 + TimelineView.INDENT + 5.0, view.row_top(body) + 15.0))
	await frames(2)
	check_eq(row_named("Body").collapsed, true, "the arrow folds the group")
	check_eq(panel.rows.size(), 4, "rows with the group folded")
	check_eq(view.row_of(int(row_named("Arm").id)), -1, "the folded layer has no row")
	run({"cmd": "delete_layers", "layers": [int(group.layer)]})
	await frames(2)
	check_eq(panel.rows.size(), 3, "rows after deleting the group")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_pressing_the_ruler_scrubs_and_snaps_to_frames() -> void:
	var panel := await _panel()
	var view := panel.view
	var off_frame := 2 * SECOND + FRAME / 3
	view.press(Vector2(view.x_of(off_frame), 8.0))
	check_eq(int(_status().time), 2 * SECOND, "a press on the ruler seeks to a frame")
	view.drag(Vector2(view.x_of(3 * SECOND), 8.0))
	view.release(Vector2(view.x_of(3 * SECOND), 8.0))
	check_eq(int(_status().time), 3 * SECOND, "dragging scrubs")
	await frames(2)
	check_eq(view.playhead, 3 * SECOND, "the playhead line is there")

	run({"cmd": "set_options", "values": {"snap_to_frames": false}})
	await frames(2)
	var between := Vector2(view.x_of(off_frame), 8.0)
	view.press(between)
	view.release(between)
	check_eq(int(_status().time), view.time_at(between.x), "without snapping the playhead goes where it was pressed")
	check(int(_status().time) % FRAME != 0, "which is between frames")

	# A double press on a track seeks there too.
	var at := Vector2(view.x_of(5 * SECOND), view.row_top(2) + 12.0)
	view.press(at)
	view.release(at)
	view.press(at, false, false, true)
	view.release(at)
	check_eq(int(_status().time), view.time_at(at.x), "a double press on a track seeks")
	check_eq(_status().can_undo, false, "none of this is an edit")


func test_zoom_fit_and_scroll() -> void:
	var panel := await _panel()
	var view := panel.view
	var area := view.tracks_rect()
	check(view.x_of(0) >= area.position.x and view.x_of(10 * SECOND) <= area.end.x, "a new document shows its whole range")
	var before := view.axis.zoom
	_press(panel, "zoom_in")
	check(view.axis.zoom > before * 1.4, "zoom in")
	_press(panel, "zoom_out")
	check_near(view.axis.zoom, before, "zoom out", 0.01)
	for step in 6:
		_press(panel, "zoom_in")
	check(view.x_of(10 * SECOND) > area.end.x, "zoomed in, the end of the range is out of view")
	# The wheel pans along time; with Ctrl it zooms about the pointer.
	var wheel := InputEventMouseButton.new()
	wheel.button_index = MOUSE_BUTTON_WHEEL_DOWN
	wheel.pressed = true
	wheel.position = area.get_center()
	var start := view.axis.start
	view._gui_input(wheel)
	check(view.axis.start > start, "the wheel pans")
	wheel.ctrl_pressed = true
	wheel.button_index = MOUSE_BUTTON_WHEEL_UP
	var under := view.time_at(wheel.position.x)
	var zoom := view.axis.zoom
	view._gui_input(wheel)
	check(view.axis.zoom > zoom, "Ctrl and the wheel zoom")
	check(absi(view.time_at(wheel.position.x) - under) < SECOND / 100, "about the pointer")
	# Playback pages along when the playhead leaves the view.
	run({"cmd": "seek", "time": 9 * SECOND})
	await frames(3)
	var x := view.x_of(9 * SECOND)
	check(x >= area.position.x and x <= area.end.x, "the view follows the playhead")
	_press(panel, "fit")
	check(view.x_of(0) >= area.position.x and view.x_of(10 * SECOND) <= area.end.x, "fit shows everything again")

	# With many layers the rows scroll, and only those in view are looked at.
	for number in 12:
		run({"cmd": "add_layer", "kind": "paint", "name": "Extra %d" % number})
	await frames(2)
	check_eq(panel.rows.size(), 15, "rows")
	view.scroll_rows(10000.0)
	check_near(view.row_top(14) + TimelineView.ROW, view.tracks_rect().end.y, "the last row sits at the bottom when scrolled all the way", 0.5)
	wheel.ctrl_pressed = false
	wheel.meta_pressed = false
	wheel.shift_pressed = true
	wheel.button_index = MOUSE_BUTTON_WHEEL_UP
	var scrolled := view.scroll_y
	view._gui_input(wheel)
	check(view.scroll_y < scrolled, "Shift and the wheel scroll the rows")


func test_panel_fits_narrow_and_wide_and_can_be_resized_and_collapsed() -> void:
	var panel := await _panel(Vector2(480, 600))
	check(panel.size.x <= 480.0, "the panel is no wider than its dock: %s" % panel.size.x)
	for key: String in panel.controls:
		var control: Control = panel.controls[key]
		if not control.is_visible_in_tree():
			continue
		var rect := control.get_global_rect()
		var inside := panel.get_global_rect().grow(0.5)
		check(rect.position.x >= inside.position.x and rect.end.x <= inside.end.x, "%s is inside the panel at 480 px" % key)
		check(control.size.y >= Kit.ROW, "%s is tall enough to press" % key)
	check(panel.view.tracks_rect().size.x > 150.0, "the tracks have room at 480 px")
	panel.get_parent().size = Vector2(1400, 260)
	await frames(3)
	panel.get_parent().size = Vector2(1400, 100)
	await frames(3)
	check_near(panel.size.y, TimelinePanel.DEFAULT_HEIGHT, "default height when there is room", 16.0)

	panel.set_height(400.0)
	await frames(2)
	check(panel.size.y >= 400.0, "taller after set_height")
	check(panel.view.size.y > 250.0, "the extra height goes to the tracks")
	_press(panel, "collapse")
	await frames(2)
	check(not panel.view.is_visible_in_tree(), "collapsed: no tracks")
	check(not (panel.controls.key_set as Control).is_visible_in_tree(), "collapsed: no layer commands")
	check((panel.controls.play as Control).is_visible_in_tree(), "collapsed: playback stays")
	check(panel.get_combined_minimum_size().y < 80.0, "collapsed: one row high")
	_press(panel, "collapse")
	await frames(2)
	check(panel.view.is_visible_in_tree(), "shown again")
	check_eq(panel.custom_minimum_size.y, 400.0, "at the height it had")


# ---- frames ----

func test_frame_commands_change_the_frames() -> void:
	var panel := await _panel()
	var paint := _paint()
	check((panel.controls.frame_delete as Button).disabled, "one frame cannot be deleted")
	check((panel.controls.key_frame_remove as Button).disabled, "nor made a hold")
	_press(panel, "frame_blank")
	await frames(2)
	check_eq(panel.timeline[paint].paint.frames, [0, 1], "frames after a blank key frame")
	check_eq(int(_status().frame), 1, "the playhead moved on to the new frame")
	_press(panel, "hold_after")
	_press(panel, "frame_duplicate")
	await frames(2)
	check_eq(panel.timeline[paint].paint.frames, [0, 1, 1, 2], "frames after a hold and a copy")
	check_eq(int(panel.timeline[paint].paint.frame), 3, "the frame shown")
	_press(panel, "key_frame_previous")
	check_eq(int(_status().frame), 1, "back to the key frame before")
	_press(panel, "key_frame_next")
	check_eq(int(_status().frame), 3, "on to the next key frame")
	await frames(2)
	_press(panel, "key_frame_remove")
	await frames(2)
	check_eq(panel.timeline[paint].paint.frames, [0, 1, 1, 1], "a removed key frame holds the drawing before")
	_press(panel, "frame_delete")
	await frames(2)
	check_eq(panel.timeline[paint].paint.frames, [0, 1, 1], "frames after deleting one")
	_press(panel, "hold_before")
	_press(panel, "frames_double")
	_press(panel, "frames_reverse")
	await frames(2)
	check_eq((_line(paint).paint.frames as Array).size(), 8, "doubled")
	check_eq(_line(paint).paint.frames, [0, 0, 0, 0, 0, 0, 1, 1], "and reversed")

	check_eq((panel.controls.frames_loop as Button).button_pressed, false, "a new layer plays once")
	_switch(panel, "frames_loop", true)
	await frames(2)
	check_eq(int(_line(paint).paint.max_repeat), 0, "looping")
	check_eq((panel.controls.frames_loop as Button).button_pressed, true, "the loop button shows it")
	_switch(panel, "frames_loop", false)
	check_eq(int(_line(paint).paint.max_repeat), 1, "playing once again")
	_press(panel, "frames_clear")
	await frames(2)
	check_eq((panel.timeline[paint].paint.frames as Array).size(), 1, "all frames but one deleted")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_empty_key_frames_are_told_apart() -> void:
	var panel := await _panel()
	var paint := _paint()
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	await frames(2)
	_press(panel, "frame_blank")
	await frames(2)
	check_eq(panel.is_empty_drawing(paint, 0), false, "the drawn frame is not empty")
	check_eq(panel.is_empty_drawing(paint, 1), true, "the new frame is empty")
	LimnerDemo.stroke(world.scene, [Vector3(0, 1.2, -1), Vector3(0.1, 1.2, -1), Vector3(0.2, 1.25, -1)])
	await frames(2)
	check_eq(panel.is_empty_drawing(paint, 1), false, "until something is drawn on it")
	_press(panel, "frame_blank")
	await frames(2)
	check_eq(panel.is_empty_drawing(paint, 2), true, "a second empty frame")
	# Strokes that go away on a frame that is not showing.
	editor.undo()
	editor.undo()
	await frames(2)
	check_eq(panel.is_empty_drawing(paint, 1), true, "after undoing the stroke, empty again")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_copying_the_selection_to_frames() -> void:
	var panel := await _panel()
	var paint := _paint()
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	await frames(2)
	check((panel.controls.copy_to_frame as Button).disabled, "nothing selected: nothing to copy")
	run({"cmd": "set_tool", "tool": "select"})
	run({"cmd": "select_all"})
	_press(panel, "frame_blank")
	_press(panel, "frame_blank")
	await frames(2)
	if int(_status().selection_count) == 0:
		failures.append("select_all selected nothing")
		return
	_press(panel, "copy_to_frame")
	await frames(2)
	check_eq(int(row_named("Paint").strokes), 1, "the stroke was copied into the frame shown")
	_press(panel, "copy_to_all_frames")
	await frames(2)
	check_eq(int(editor.get_info().strokes), 4, "and then into the frame that had none")
	check_eq(_line(paint).paint.frames, [0, 1, 2], "frames")


# ---- clips ----

func test_dragging_a_clip_moves_it_as_one_undo_step() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	run({"cmd": "toggle_clip", "layer": paint, "time": 2 * SECOND})
	run({"cmd": "set_key", "layer": paint, "track": "opacity", "time": SECOND / 2})
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	var from := view.clip_center(paint, 0)
	var second := view.x_of(SECOND) - view.x_of(0)
	view.press(from)
	check_eq([panel.selected_clip_layer, panel.selected_clip], [paint, 0], "pressing a clip selects it")
	view.drag(from + Vector2(second * 2.5, 6.0))
	view.drag(from + Vector2(second * 0.4, 3.0))
	view.drag(from + Vector2(second * 1.02, 0.0))
	check_eq(int(panel.timeline[paint].clips[0].start), SECOND, "the tracks show the clip where it is dragged")
	view.release(from + Vector2(second * 1.02, 0.0))
	await frames(2)
	var clip: Dictionary = _line(paint).clips[0]
	check_eq([int(clip.start), int(clip.end)], [SECOND, 3 * SECOND], "the clip moved by a whole number of frames")
	check_eq(_times(_line(paint).opacity_keys), [SECOND * 3 / 2], "its keys moved with it")
	check_eq([panel.selected_clip_layer, panel.selected_clip], [paint, 0], "and it is still selected")
	check_eq(_status().undo_label, "Move clip", "the drag is an undo step")
	editor.undo()
	await frames(2)
	clip = _line(paint).clips[0]
	check_eq([int(clip.start), int(clip.end)], [0, 2 * SECOND], "one undo takes the whole drag back")
	check_eq(_status().undo_label, "Add key", "and nothing more")

	# A press without a drag changes nothing.
	view.press(from)
	view.release(from)
	await frames(2)
	check_eq(_status().undo_label, "Add key", "a press alone is not an edit")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_dragged_clip_snaps_to_the_playhead_and_to_other_clips() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	var other: Dictionary = run({"cmd": "add_layer", "kind": "paint", "name": "Other"})
	run({"cmd": "toggle_clip", "layer": int(other.layer), "time": 6 * SECOND + 1234})
	run({"cmd": "toggle_clip", "layer": paint, "time": 2 * SECOND})
	# Between frames, to tell snapping to the playhead from snapping to frames.
	var playhead := 3 * SECOND + FRAME / 3
	run({"cmd": "seek", "time": playhead})
	await frames(3)
	check_eq(view.playhead, playhead, "the playhead is between frames")
	var from := view.clip_center(paint, 0)
	view.press(from)
	view.drag(Vector2(from.x + view.x_of(playhead) - view.x_of(0) + 4.0, from.y))
	view.release(from)
	await frames(2)
	check_eq(int(_line(paint).clips[0].start), playhead, "the start of the clip snapped to the playhead")
	# The end of the clip snaps too: here to the end of a clip on another layer.
	from = view.clip_center(paint, 0)
	view.press(from)
	view.drag(Vector2(from.x + view.x_of(6 * SECOND + 1234) - view.x_of(playhead + 2 * SECOND) - 5.0, from.y))
	view.release(from)
	await frames(2)
	check_eq(int(_line(paint).clips[0].end), 6 * SECOND + 1234, "the end of the clip snapped to the other clip's end")
	# With that option off it goes to the nearest frame.
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	from = view.clip_center(paint, 0)
	view.press(from)
	view.drag(from + Vector2(-9.0, 0.0))
	view.release(from)
	await frames(2)
	check_eq(int(_line(paint).clips[0].start) % FRAME, 0, "on a frame without snapping to clips")


func test_trimming_a_clip_by_its_edges() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	run({"cmd": "toggle_clip", "layer": paint, "time": 4 * SECOND})
	run({"cmd": "toggle_clip", "layer": paint, "time": 6 * SECOND})
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	var row := view.row_of(paint)
	var y := view.row_top(row) + 14.0
	check_eq(view.clip_at(view.x_of(0) + 2.0, row), {"clip": 0, "part": "start"}, "the start edge")
	check_eq(view.clip_at(view.x_of(4 * SECOND) - 2.0, row), {"clip": 0, "part": "end"}, "the end edge")
	check_eq(view.clip_at(view.x_of(2 * SECOND), row), {"clip": 0, "part": "move"}, "the middle")
	check_eq(view.clip_at(view.x_of(5 * SECOND), row), {}, "between clips")
	check_eq(view.clip_at(view.x_of(9 * SECOND), row), {"clip": 1, "part": "move"}, "a clip that never stops")

	# The end, dragged right: it stops at the clip after it.
	var end := Vector2(view.x_of(4 * SECOND) - 2.0, y)
	view.press(end)
	view.drag(Vector2(view.x_of(5 * SECOND) - 2.0, y))
	check_eq(int(panel.timeline[paint].clips[0].end), 5 * SECOND, "the end follows the drag")
	view.drag(Vector2(view.x_of(8 * SECOND), y))
	view.release(end)
	await frames(2)
	check_eq(int(_line(paint).clips[0].end), 6 * SECOND, "the end stops at the next clip")
	check_eq((_line(paint).clips as Array).size(), 2, "and both clips are still there")
	editor.undo()
	await frames(2)
	check_eq(int(_line(paint).clips[0].end), 4 * SECOND, "one undo")

	# The start, dragged right, cuts into the content; dragged back it gives it back.
	var start := Vector2(view.x_of(0) + 2.0, y)
	view.press(start)
	view.drag(Vector2(view.x_of(SECOND) + 2.0, y))
	view.drag(Vector2(view.x_of(-SECOND), y))
	view.drag(Vector2(view.x_of(2 * SECOND) + 2.0, y))
	view.release(start)
	await frames(2)
	var clip: Dictionary = _line(paint).clips[0]
	check_eq([int(clip.start), int(clip.end), int(clip.trim)], [2 * SECOND, 4 * SECOND, 2 * SECOND], "start trimmed by two seconds")
	check_eq(_status().undo_label, "Trim clip", "as one step")
	view.press(start + Vector2(view.x_of(2 * SECOND) - view.x_of(0), 0.0))
	view.drag(Vector2(view.x_of(10 * SECOND), y))
	view.release(start)
	await frames(2)
	clip = _line(paint).clips[0]
	check_eq(int(clip.start), 4 * SECOND - FRAME, "a clip keeps at least a frame")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_clip_commands() -> void:
	var panel := await _panel()
	var paint := _paint()
	check((panel.controls.clip_split as Button).disabled, "at the very start there is nothing to split")
	check((panel.controls.clip_delete as Button).disabled, "no clip selected: nothing to delete")
	run({"cmd": "seek", "time": 4 * SECOND})
	await frames(2)
	_press(panel, "clip_toggle")
	await frames(2)
	check_eq(panel.timeline[paint].clips, [{"start": 0, "end": 4 * SECOND, "trim": 0}], "the clip ends at the playhead")
	run({"cmd": "seek", "time": 2 * SECOND})
	await frames(2)
	_press(panel, "clip_split")
	await frames(2)
	check_eq((panel.timeline[paint].clips as Array).size(), 2, "split in two")
	check_eq(int(panel.timeline[paint].clips[1].trim), 2 * SECOND, "the second half goes on where the first stopped")

	panel.select_clip(paint, 1)
	run({"cmd": "seek", "time": 7 * SECOND})
	await frames(2)
	_press(panel, "clip_duplicate")
	await frames(2)
	var clips: Array = panel.timeline[paint].clips
	check_eq([int(clips[2].start), int(clips[2].end)], [7 * SECOND, 9 * SECOND], "a copy of the clip at the playhead")
	check_eq(panel.selected_clip, 2, "the copy is selected")
	_press(panel, "clip_open_end")
	await frames(2)
	check_eq(panel.timeline[paint].clips[2].end, null, "its end removed")
	check((panel.controls.clip_open_end as Button).disabled, "a clip without an end has none to remove")
	_press(panel, "clip_delete")
	await frames(2)
	check_eq((panel.timeline[paint].clips as Array).size(), 2, "deleted")
	check_eq(panel.selected_clip_layer, -1, "nothing selected after deleting")

	# The selection goes when its clip or its layer does, or when another layer is chosen.
	panel.select_clip(paint, 1)
	run({"cmd": "delete_clip", "layer": paint, "clip": 1})
	await frames(2)
	check_eq(panel.selected_clip_layer, -1, "the selection follows the document")
	panel.select_clip(paint, 0)
	run({"cmd": "select_layers", "layers": [int(row_named("Spawn Area").id)]})
	await frames(2)
	check_eq(panel.selected_clip_layer, -1, "choosing another layer drops the selected clip")
	check(messages.is_empty(), "no errors: %s" % [messages])

	# The root is always on the timeline.
	run({"cmd": "select_layers", "layers": [int(row_named("Root").id)]})
	await frames(2)
	check((panel.controls.clip_toggle as Button).disabled, "the root has no clips to change")
	check((panel.controls.clip_toggle as Button).is_visible_in_tree(), "but the buttons stay where they are")


# ---- keys ----

func test_selecting_and_moving_keys() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	for second: int in [1, 2, 4]:
		run({"cmd": "set_key", "layer": paint, "track": "transform", "time": second * SECOND})
	run({"cmd": "set_key", "layer": paint, "track": "opacity", "time": 3 * SECOND})
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	var first := view.key_position(paint, "transform", SECOND)
	var second_key := view.key_position(paint, "transform", 2 * SECOND)
	check_eq(view.key_at(first, view.row_of(paint)), {"track": "transform", "time": SECOND}, "a key is found where it is drawn")
	check_eq(view.key_at(first + Vector2(30.0, 0.0), view.row_of(paint)), {}, "and not elsewhere")
	view.press(first)
	view.release(first)
	check_eq(panel.selected_keys, {paint: [SECOND]}, "pressing a key selects it")
	view.press(second_key, true)
	view.release(second_key)
	check_eq(panel.selected_keys, {paint: [SECOND, 2 * SECOND]}, "with Shift another is added")
	view.press(first, false, true)
	view.release(first)
	check_eq(panel.selected_keys, {paint: [2 * SECOND]}, "with Ctrl one is taken away")
	view.press(first, true)
	view.release(first)

	# Dragging one of the selected keys moves them all, as one step.
	var half := (view.x_of(SECOND) - view.x_of(0)) * 0.5
	view.press(second_key)
	view.drag(second_key + Vector2(half * 3.0, 0.0))
	view.drag(second_key + Vector2(half, 0.0))
	check_eq(_times(panel.timeline[paint].transform_keys), [SECOND * 3 / 2, SECOND * 5 / 2, 4 * SECOND], "the tracks show the keys where they are dragged")
	view.release(second_key)
	await frames(2)
	check_eq(_times(_line(paint).transform_keys), [SECOND * 3 / 2, SECOND * 5 / 2, 4 * SECOND], "both keys moved half a second, onto frames")
	check_eq(panel.selected_keys, {paint: [SECOND * 3 / 2, SECOND * 5 / 2]}, "and stay selected")
	check_eq(_status().undo_label, "Move keys", "one undo step")
	editor.undo()
	await frames(2)
	check_eq(_times(_line(paint).transform_keys), [SECOND, 2 * SECOND, 4 * SECOND], "undone in one")
	check_eq(panel.selected_keys, {}, "keys that are gone are no longer selected")

	# Keys pushed against the start keep their spacing.
	view.press(first)
	view.release(first)
	view.press(second_key, true)
	view.release(second_key)
	view.press(second_key)
	view.drag(second_key + Vector2(-half * 9.0, 0.0))
	view.release(second_key)
	await frames(2)
	check_eq(_times(_line(paint).transform_keys), [0, SECOND, 4 * SECOND], "stopped at zero together")

	# Pressing a key of the other kind switches to that kind.
	var dot := view.key_position(paint, "opacity", 3 * SECOND)
	view.press(dot)
	view.release(dot)
	check_eq(panel.key_track, "opacity", "the key type follows the key pressed")
	check_eq(panel.selected_keys, {paint: [3 * SECOND]}, "and that key is selected")
	# Pressing a clip selects the clip instead; pressing empty track space, nothing.
	var clip := Vector2(view.x_of(8 * SECOND), view.row_top(1) + 12.0)
	view.press(clip)
	view.release(clip)
	check_eq(panel.selected_keys, {}, "no keys selected after pressing a clip")
	check_eq(panel.selected_clip_layer, int(row_named("Spawn Area").id), "but the clip")
	view.press(dot)
	view.release(dot)
	var nowhere := Vector2(view.x_of(8 * SECOND), view.row_top(0) + 12.0)
	view.press(nowhere)
	view.release(nowhere)
	check_eq(panel.selected_keys, {}, "nothing selected after pressing empty space")
	check_eq(panel.selected_clip_layer, -1, "no clip either")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_dragging_over_empty_tracks_selects_a_range_of_keys() -> void:
	var panel := await _panel(Vector2(900, 420))
	var view := panel.view
	var paint := _paint()
	var other: Dictionary = run({"cmd": "add_layer", "kind": "paint", "name": "Other"})
	var other_id := int(other.layer)
	for second: int in [1, 2, 3, 5]:
		run({"cmd": "set_key", "layer": paint, "track": "transform", "time": second * SECOND})
		run({"cmd": "set_key", "layer": other_id, "track": "transform", "time": second * SECOND + FRAME})
	await frames(2)
	var rows := [view.row_of(paint), view.row_of(other_id)]
	rows.sort()
	var from := Vector2(view.x_of(SECOND * 3 / 2), view.row_top(rows[0]) + 2.0)
	var to := Vector2(view.x_of(4 * SECOND), view.row_top(rows[1]) + TimelineView.ROW - 2.0)
	# A drag that starts on a clip moves the clip, unless the switch for selecting is on.
	_switch(panel, "key_select_range", true)
	view.press(from)
	view.drag(to)
	view.release(to)
	check_eq(_status().can_undo and _status().undo_label != "Add key", false, "nothing moved")
	check_eq(panel.selected_keys.get(paint, []), [2 * SECOND, 3 * SECOND], "keys in the range on one layer")
	check_eq(panel.selected_keys.get(other_id, []), [2 * SECOND + FRAME, 3 * SECOND + FRAME], "and on the other")
	# Deleting acts on every layer that has keys selected, as one step.
	_press(panel, "key_delete")
	await frames(2)
	check_eq(_times(_line(paint).transform_keys), [SECOND, 5 * SECOND], "deleted on one layer")
	check_eq(_times(_line(other_id).transform_keys), [SECOND + FRAME, 5 * SECOND + FRAME], "and on the other")
	editor.undo()
	await frames(2)
	check_eq(_times(_line(paint).transform_keys).size() + _times(_line(other_id).transform_keys).size(), 8, "one undo brings them all back")


func test_key_commands() -> void:
	var panel := await _panel()
	var paint := _paint()
	check((panel.controls.key_delete as Button).disabled, "no key to delete yet")
	run({"cmd": "seek", "time": SECOND})
	await frames(2)
	_press(panel, "key_set")
	run({"cmd": "seek", "time": 3 * SECOND})
	await frames(2)
	_press(panel, "key_set")
	await frames(2)
	check_eq(_times(panel.timeline[paint].transform_keys), [SECOND, 3 * SECOND], "transform keys set at the playhead")
	_press(panel, "key_previous")
	check_eq(int(_status().time), SECOND, "back to the key before")
	_press(panel, "key_next")
	check_eq(int(_status().time), 3 * SECOND, "on to the next key")

	# The easing menu changes the selected keys and what new keys get.
	_press(panel, "key_select_all")
	check_eq(panel.selected_keys, {paint: [SECOND, 3 * SECOND]}, "all keys of the layer selected")
	var easing: OptionButton = panel.controls.key_interp
	check_eq(easing.get_item_text(easing.selected), "Linear", "the easing of the selected keys")
	easing.item_selected.emit(TimelinePanel.INTERPS.find("ease_out"))
	await frames(2)
	check_eq(panel.timeline[paint].transform_keys.map(func(key: Dictionary) -> String: return key.interp), ["ease_out", "ease_out"], "easing of the selected keys")
	check_eq(world.scene.options().default_interp, "ease_out", "and of keys made from now on")
	check_eq(easing.get_item_text(easing.selected), "Ease out", "the menu shows it")
	check_eq(_status().undo_label, "Change key easing", "as an undo step")

	# Copy, paste at the playhead, reverse.
	_press(panel, "key_copy")
	run({"cmd": "seek", "time": 5 * SECOND})
	await frames(2)
	_press(panel, "key_paste")
	await frames(2)
	check_eq(_times(panel.timeline[paint].transform_keys), [SECOND, 3 * SECOND, 5 * SECOND, 7 * SECOND], "pasted from the playhead on")
	panel.select_keys("transform", {paint: [SECOND, 3 * SECOND, 7 * SECOND]})
	_press(panel, "key_reverse")
	await frames(2)
	check_eq(_times(panel.timeline[paint].transform_keys), [SECOND, 5 * SECOND, 7 * SECOND], "reversed within their span: 3 s becomes 5 s, on the key that was there")
	check_eq(panel.selected_keys, {paint: [SECOND, 5 * SECOND, 7 * SECOND]}, "the reversed keys stay selected")

	# With nothing selected, delete takes the key at the playhead.
	panel.select_keys("transform", {})
	_press(panel, "key_delete")
	await frames(2)
	check_eq(_times(panel.timeline[paint].transform_keys), [SECOND, 7 * SECOND], "the key at the playhead deleted")

	# Opacity keys are their own kind.
	(panel.controls.key_type as ChoiceRow).buttons.opacity.pressed.emit()
	check_eq(panel.key_track, "opacity", "key type chosen")
	_press(panel, "key_set")
	await frames(2)
	check_eq(_times(panel.timeline[paint].opacity_keys), [5 * SECOND], "an opacity key")
	check_eq(_times(panel.timeline[paint].transform_keys), [SECOND, 7 * SECOND], "transform keys untouched")
	check((panel.controls.key_type as ChoiceRow).buttons.action.disabled, "chapter marks are for the root only")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_keying_all_children() -> void:
	var panel := await _panel()
	var group: Dictionary = run({"cmd": "add_layer", "kind": "group", "name": "Body"})
	var arm: Dictionary = run({"cmd": "add_layer", "kind": "paint", "parent": int(group.layer), "name": "Arm"})
	run({"cmd": "select_layers", "layers": [int(group.layer)]})
	run({"cmd": "seek", "time": 2 * SECOND})
	await frames(2)
	check((panel.controls.key_children as Button).disabled == false, "a group can key its children")
	_press(panel, "key_set")
	await frames(2)
	check_eq(_times(_line(int(arm.layer)).transform_keys), [], "without the switch only the group is keyed")
	_switch(panel, "key_children", true)
	_press(panel, "key_set")
	await frames(2)
	check_eq(_times(_line(int(group.layer)).transform_keys), [2 * SECOND], "the group")
	check_eq(_times(_line(int(arm.layer)).transform_keys), [2 * SECOND], "and the layer inside it")


func test_keys_inside_a_sequence_are_drawn_on_its_clock() -> void:
	var panel := await _panel(Vector2(900, 420))
	var view := panel.view
	var sequence: Dictionary = run({"cmd": "add_layer", "kind": "sequence", "name": "Walk"})
	var leg: Dictionary = run({"cmd": "add_layer", "kind": "paint", "parent": int(sequence.layer), "name": "Leg"})
	var leg_id := int(leg.layer)
	run({"cmd": "move_clip", "layer": int(sequence.layer), "clip": 0, "start": 2 * SECOND})
	run({"cmd": "set_key", "layer": leg_id, "track": "transform", "time": SECOND})
	run({"cmd": "toggle_clip", "layer": leg_id, "time": 3 * SECOND})
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	check_eq(int(panel.timeline[leg_id].origin), 2 * SECOND, "time inside the sequence starts where its clip does")
	var row := view.row_of(leg_id)
	var at := view.key_position(leg_id, "transform", SECOND)
	check_near(at.x, view.x_of(3 * SECOND), "the key at one second is drawn at three", 0.01)
	check_eq(view.clip_at(view.x_of(SECOND), row), {}, "the layer's clip does not reach before the sequence")
	check_eq(view.clip_at(view.x_of(4 * SECOND), row), {"clip": 0, "part": "move"}, "it lasts from two seconds to five")
	check_eq(view.clip_at(view.x_of(5 * SECOND + FRAME * 6), row), {}, "and no longer")

	# Drags send times on the layer's own clock.
	view.press(at)
	view.drag(at + Vector2(view.x_of(SECOND) - view.x_of(0), 0.0))
	view.release(at)
	await frames(2)
	check_eq(_times(_line(leg_id).transform_keys), [2 * SECOND], "the key moved a second later on the sequence's clock")
	var middle := view.clip_center(leg_id, 0)
	view.press(middle)
	view.drag(middle + Vector2(view.x_of(SECOND) - view.x_of(0), 0.0))
	view.release(middle)
	await frames(2)
	var clip: Dictionary = _line(leg_id).clips[0]
	check_eq([int(clip.start), int(clip.end)], [SECOND, 4 * SECOND], "the clip too")
	check_eq(_times(_line(leg_id).transform_keys), [3 * SECOND], "taking its key along")

	# The sequence's own commands: a loop point at the playhead, and baking.
	run({"cmd": "select_layers", "layers": [int(sequence.layer)]})
	run({"cmd": "seek", "time": 4 * SECOND})
	await frames(2)
	check((panel.controls.loop_point as Control).is_visible_in_tree(), "a sequence shows its loop button")
	check(not (panel.controls.frame_blank as Control).is_visible_in_tree(), "in place of the frame buttons")
	check((panel.controls.bake as Button).disabled, "nothing to bake before it loops")
	_switch(panel, "loop_point", true)
	await frames(2)
	check_eq(int(_line(int(sequence.layer)).sequence.loop_duration), 2 * SECOND, "it loops two seconds after it starts")
	_press(panel, "bake")
	await frames(2)
	check_eq(row_named("Walk_baked").get("kind"), "paint", "baked into a paint layer")
	check_eq(int(row_named("Walk_baked").frames), 48, "with a frame for each frame of the loop")
	run({"cmd": "select_layers", "layers": [int(sequence.layer)]})
	await frames(2)
	_switch(panel, "loop_point", false)
	check_eq(int(_line(int(sequence.layer)).sequence.loop_duration), 0, "the loop removed")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_chapter_marks_on_the_root() -> void:
	var panel := await _panel()
	var view := panel.view
	var root := int(row_named("Root").id)
	check(not (panel.controls.mark_stop as Control).is_visible_in_tree(), "a paint layer has no chapter buttons")
	run({"cmd": "select_layers", "layers": [root]})
	run({"cmd": "seek", "time": 2 * SECOND})
	await frames(2)
	check((panel.controls.mark_stop as Control).is_visible_in_tree(), "the root shows them")
	check_eq(panel.key_track, "action", "and its keys are the chapter marks")
	check((panel.controls.mark_remove as Button).disabled, "no mark at the playhead yet")
	_press(panel, "mark_stop")
	run({"cmd": "seek", "time": 4 * SECOND})
	await frames(2)
	_press(panel, "mark_play")
	run({"cmd": "seek", "time": 6 * SECOND})
	await frames(2)
	_press(panel, "mark_loop")
	await frames(2)
	check_eq(panel.timeline[root].actions.map(func(mark: Dictionary) -> String: return mark.action), ["stop", "play", "loop"], "marks on the root")
	_press(panel, "mark_remove")
	await frames(2)
	check_eq((panel.timeline[root].actions as Array).size(), 2, "the mark at the playhead removed")

	# Marks are keys of the root: they are selected and dragged like the others.
	var flag := view.key_position(root, "action", 2 * SECOND)
	view.press(flag)
	check_eq(panel.selected_keys, {root: [2 * SECOND]}, "a mark is selected by pressing it")
	view.drag(flag + Vector2(view.x_of(SECOND) - view.x_of(0), 0.0))
	view.release(flag)
	await frames(2)
	check_eq(_times(_line(root).actions), [3 * SECOND, 4 * SECOND], "and moved by dragging")
	_press(panel, "key_delete")
	await frames(2)
	check_eq(_times(_line(root).actions), [4 * SECOND], "and deleted with the key commands")
	check(messages.is_empty(), "no errors: %s" % [messages])


# ---- odds and ends ----

func test_a_drag_that_ends_where_it_began_is_not_an_edit() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	run({"cmd": "toggle_clip", "layer": paint, "time": 2 * SECOND})
	run({"cmd": "toggle_clip", "layer": paint, "time": 3 * SECOND})
	await frames(2)
	var from := view.clip_center(paint, 0)
	view.press(from)
	# Across the clip after it and back: nothing is cut on the way.
	view.drag(from + Vector2(view.x_of(4 * SECOND) - view.x_of(0), 0.0))
	check_eq(int(panel.timeline[paint].clips[0].start), SECOND, "the clip stops against the next one")
	check_eq((panel.timeline[paint].clips as Array).size(), 2, "which is left alone")
	view.drag(from)
	view.release(from)
	await frames(2)
	check_eq(_line(paint).clips, [{"start": 0, "end": 2 * SECOND, "trim": 0}, {"start": 3 * SECOND, "end": null, "trim": 0}], "the clips are as they were")
	check_eq(_status().undo_label, "Change clip", "and there is nothing new to undo")


func test_a_locked_layer_and_no_layer_switch_the_commands_off() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	run({"cmd": "toggle_clip", "layer": paint, "time": 2 * SECOND})
	run({"cmd": "set_layer", "layer": paint, "locked": true})
	await frames(2)
	for key: String in ["frame_blank", "hold_after", "frames_loop", "clip_toggle", "key_set", "key_paste"]:
		check((panel.controls[key] as BaseButton).disabled, "%s is off for a locked layer" % key)
		check((panel.controls[key] as Control).is_visible_in_tree(), "%s is still shown" % key)
	var from := view.clip_center(paint, 0)
	view.press(from)
	view.drag(from + Vector2(80.0, 0.0))
	view.release(from)
	await frames(2)
	check_eq(int(_line(paint).clips[0].start), 0, "the clip of a locked layer stays put")
	check(messages.is_empty(), "and nothing complains: %s" % [messages])

	run({"cmd": "select_layers", "layers": []})
	await frames(2)
	check_eq(editor.current_layer(), -1, "no current layer")
	for key: String in ["frame_blank", "clip_toggle", "key_set", "key_previous", "key_select_all"]:
		check((panel.controls[key] as BaseButton).disabled, "%s is off without a layer" % key)
	check((panel.controls.play as BaseButton).disabled == false, "playback does not need a layer")


func test_the_frame_shown_is_followed_while_playing() -> void:
	var panel := await _panel()
	var paint := _paint()
	for number in 5:
		run({"cmd": "add_frame", "layer": paint, "kind": "blank"})
	run({"cmd": "set_layer_kind", "layer": paint, "values": {"max_repeat": 0}})
	run({"cmd": "seek", "time": 0})
	await frames(2)
	for step in 8:
		_press(panel, "zoom_in")
	check(panel.view.axis.frame_width() >= TimelineView.MIN_FRAME_PIXELS, "frames are drawn one by one")
	_press(panel, "play")
	var moved := false
	for step in 40:
		await frames(1)
		if int(panel.timeline[paint].paint.frame) > 0:
			moved = true
	check(moved, "the frame the tracks highlight moves on with the playhead")
	check_eq(panel.view.playhead, int(editor.status.time), "the playhead line keeps up")
	var area := panel.view.tracks_rect()
	var x := panel.view.x_of(panel.view.playhead)
	check(x >= area.position.x and x <= area.end.x, "and stays in view")
	_press(panel, "play")


func test_empty_frames_follow_the_drawings_when_frames_are_reversed() -> void:
	var panel := await _panel()
	var paint := _paint()
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	await frames(2)
	_press(panel, "frame_blank")
	await frames(2)
	check_eq([panel.is_empty_drawing(paint, 0), panel.is_empty_drawing(paint, 1)], [false, true], "drawn, then empty")
	# Reversing two frames leaves the frame table as it was, with the drawings swapped.
	_press(panel, "frames_reverse")
	await frames(2)
	check_eq(panel.timeline[paint].paint.frames, [0, 1], "the table reads the same")
	check_eq([panel.is_empty_drawing(paint, 0), panel.is_empty_drawing(paint, 1)], [true, false], "empty, then drawn")
	# The same when it is done from somewhere else, and when it is undone.
	run({"cmd": "reverse_frames", "layer": paint})
	await frames(2)
	check_eq([panel.is_empty_drawing(paint, 0), panel.is_empty_drawing(paint, 1)], [false, true], "reversed elsewhere")
	editor.undo()
	await frames(2)
	check_eq([panel.is_empty_drawing(paint, 0), panel.is_empty_drawing(paint, 1)], [true, false], "and undone")


func test_only_the_clock_is_read_again_when_the_playhead_moves() -> void:
	var panel := await _panel()
	var paint := _paint()
	LimnerDemo.stroke(world.scene, [Vector3(0, 1, -1), Vector3(0.1, 1, -1), Vector3(0.2, 1.05, -1)])
	_press(panel, "frame_blank")
	_press(panel, "frame_blank")
	await frames(2)
	var line: Dictionary = panel.timeline[paint]
	check_eq(int(line.paint.frame), 2, "on the third frame")
	run({"cmd": "seek", "time": FRAME})
	await frames(2)
	check(is_same(panel.timeline[paint], line), "the rows were kept")
	check_eq(int(line.paint.frame), 1, "and show the frame the playhead is on")
	check_eq(int(line.time), FRAME, "and the layer's time")
	check_eq(int(line.local_time), int(_line(paint).local_time), "as the document has it")
	# A change to the document reads them afresh.
	_press(panel, "frame_blank")
	await frames(2)
	check(not is_same(panel.timeline[paint], line), "read again after an edit")
	check_eq((panel.timeline[paint].paint.frames as Array).size(), 4, "with the new frame")


func test_paste_waits_for_keys_to_be_copied() -> void:
	var panel := await _panel()
	var paint := _paint()
	check((panel.controls.key_paste as BaseButton).disabled, "nothing has been copied")
	_press(panel, "key_set")
	await frames(2)
	run({"cmd": "copy_keys", "layer": paint, "track": "transform", "times": [0]})
	await frames(2)
	check(not (panel.controls.key_paste as BaseButton).disabled, "keys are waiting")


func test_a_new_document_clears_the_selection() -> void:
	var panel := await _panel()
	var paint := _paint()
	run({"cmd": "set_key", "layer": paint, "track": "transform", "time": SECOND})
	await frames(2)
	panel.select_keys("transform", {paint: [SECOND]})
	panel.select_clip(paint, 0)
	editor.new_document()
	await frames(3)
	check_eq(panel.selected_keys, {}, "no keys selected in a new document")
	check_eq(panel.selected_clip_layer, -1, "no clip either")
	check_eq(panel.rows.size(), 3, "rows of the new document")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_mouse_events_reach_the_tracks_and_the_grip() -> void:
	var panel := await _panel()
	var view := panel.view
	var paint := _paint()
	run({"cmd": "toggle_clip", "layer": paint, "time": 2 * SECOND})
	run({"cmd": "set_options", "values": {"snap_to_clips": false}})
	await frames(2)
	var from := view.clip_center(paint, 0)
	var to := from + Vector2(view.x_of(SECOND) - view.x_of(0), 0.0)
	var button := InputEventMouseButton.new()
	button.button_index = MOUSE_BUTTON_LEFT
	button.pressed = true
	button.position = from
	view._gui_input(button)
	check(view.is_dragging(), "a press takes hold")
	var motion := InputEventMouseMotion.new()
	motion.position = to
	view._gui_input(motion)
	button = InputEventMouseButton.new()
	button.button_index = MOUSE_BUTTON_LEFT
	button.pressed = false
	button.position = to
	view._gui_input(button)
	check(not view.is_dragging(), "and the release lets go")
	await frames(2)
	check_eq(int(_line(paint).clips[0].start), SECOND, "the clip was dragged with the mouse")

	# The grip on the top edge: dragging up makes the panel taller.
	var before := panel.size.y
	var grab := InputEventMouseButton.new()
	grab.button_index = MOUSE_BUTTON_LEFT
	grab.pressed = true
	grab.global_position = Vector2(200.0, 300.0)
	panel._on_grip_input(grab)
	var pull := InputEventMouseMotion.new()
	pull.global_position = Vector2(200.0, 240.0)
	panel._on_grip_input(pull)
	grab = InputEventMouseButton.new()
	grab.button_index = MOUSE_BUTTON_LEFT
	grab.pressed = false
	panel._on_grip_input(grab)
	pull.global_position = Vector2(200.0, 100.0)
	panel._on_grip_input(pull)
	await frames(2)
	check_near(panel.height, before + 60.0, "the height the grip was dragged to")
	check_near(panel.size.y, before + 60.0, "the panel is that tall", 1.0)
