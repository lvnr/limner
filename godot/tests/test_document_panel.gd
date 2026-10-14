extends LimnerTest
## The Document panel shows the document's settings, writes what is entered, and
## follows undo.


func _panel() -> DocumentPanel:
	var panel := DocumentPanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


## Types `text` into a box and presses Enter.
func _type(field: LineEdit, text: String) -> void:
	field.text = text
	field.text_submitted.emit(text)
	await frames(2)


func _choose(menu: OptionButton, index: int) -> void:
	menu.select(index)
	menu.item_selected.emit(index)
	await frames(2)


## Undoes `steps` times and checks that this is exactly how many steps there were.
func _check_undo_steps(steps: int, what: String) -> void:
	for i in steps:
		check_eq(editor.status.can_undo, true, "%s: step %d of %d can be undone" % [what, i + 1, steps])
		editor.undo()
		await frames(2)
	check_eq(editor.status.can_undo, false, "%s: nothing is left to undo after %d steps" % [what, steps])


func test_title_and_description_write_and_follow_undo() -> void:
	var panel := await _panel()
	await _type(panel._title, "Night garden")
	check_eq(editor.get_info().title, "Night garden", "title")
	check_eq(editor.status.undo_label, "Change document settings", "what undo calls it")

	panel._description.text = "Fireflies over a pond.\nSecond line."
	panel._description.focus_exited.emit()
	await frames(2)
	check_eq(editor.get_info().description, "Fireflies over a pond.\nSecond line.", "description")

	editor.undo()
	await frames(2)
	check_eq(editor.get_info().description, "", "description after undo")
	check_eq(panel._description.text, "", "the box follows undo")
	check_eq(panel._title.text, "Night garden", "the title is still there")
	editor.undo()
	await frames(2)
	check_eq(panel._title.text, "", "the title follows undo")
	editor.redo()
	await frames(2)
	check_eq(panel._title.text, "Night garden", "and redo")
	await _check_undo_steps(1, "title")


func test_unchanged_boxes_add_no_undo_step() -> void:
	var panel := await _panel()
	await _type(panel._title, "")
	panel._title.focus_exited.emit()
	panel._description.focus_exited.emit()
	await _type(panel._range_seconds["range_end"], "10")
	await _type(panel._range_seconds["range_end"], "10.0")
	await _type(panel._range_frames["range_start"], "0.0")
	await _type(panel._rate_num, "24.0")
	await _type(panel._width, "1920.0")
	await _choose(panel._rate, DocumentPanel.RATES.find(24))
	await _choose(panel._resolution, 0)
	await frames(2)
	check_eq(editor.status.can_undo, false, "nothing changed, so there is nothing to undo")

	await _type(panel._title, "Once")
	await _type(panel._title, "Once")
	panel._title.focus_exited.emit()
	await frames(2)
	await _check_undo_steps(1, "the same title entered three times")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_background_picked_in_one_go_is_one_undo_step() -> void:
	var panel := await _panel()
	check(not panel._background.edit_alpha, "the background has no transparency")
	check_near(panel._background.color.r, Color(0.8, 0.8, 0.8).linear_to_srgb().r, "the background shown is the screen colour")
	for color: Color in [Color(0.9, 0.1, 0.1), Color(0.1, 0.2, 0.3), Color(0.5, 0.5, 0.5)]:
		panel._background.color_changed.emit(color)
	panel._background.popup_closed.emit()
	await frames(2)
	var background: Array = editor.get_info().background
	for i in 3:
		check_near(background[i], 0.2140, "the background is stored linear (%d)" % i, 1e-3)
	editor.undo()
	await frames(2)
	check_near(editor.get_info().background[0], 0.8, "one undo takes back everything picked")
	check_eq(editor.status.can_undo, false, "it was a single step")
	check_near(panel._background.color.r, Color(0.8, 0.8, 0.8).linear_to_srgb().r, "the swatch follows undo")


func test_frame_rate_choices_and_custom_rates() -> void:
	var panel := await _panel()
	check_eq(panel._rate.get_item_text(panel._rate.selected), "24 fps", "rate shown")
	check_eq(panel._rate.item_count, 12, "eleven rates and Custom")
	check(not panel._rate_custom.visible, "no custom boxes for a listed rate")

	await _choose(panel._rate, DocumentPanel.RATES.find(30))
	check_eq(editor.get_info().rate, {"num": 30, "den": 1}, "30 fps")
	check_eq(int(editor.get_info().frame_duration), 23520000, "the frame length follows")

	await _choose(panel._rate, DocumentPanel.RATES.size())
	check(panel._rate_custom.visible, "Custom shows its boxes")
	check_eq([panel._rate_num.text, panel._rate_den.text], ["30", "1"], "which start at the current rate")
	check_eq(editor.get_info().rate, {"num": 30, "den": 1}, "choosing Custom changes nothing yet")

	# Half of 24000/1001 is no usable rate; the panel waits for the other half.
	await _type(panel._rate_num, "24000")
	check_eq(editor.get_info().rate, {"num": 30, "den": 1}, "a half-typed rate changes nothing")
	check(panel._rate_note.visible, "and the panel says why")
	check(messages.is_empty(), "without an error: %s" % [messages])
	await _type(panel._rate_den, "1001")
	check_eq(editor.get_info().rate, {"num": 24000, "den": 1001}, "24000/1001")
	check_eq(panel._rate.get_item_text(panel._rate.selected), "23.976 fps", "shown as 23.976")
	check(panel._rate_custom.visible and not panel._rate_note.visible, "the custom boxes stay, the note goes")

	editor.undo()
	await frames(2)
	check_eq(editor.get_info().rate, {"num": 30, "den": 1}, "the custom rate was one step")
	await _choose(panel._rate, DocumentPanel.RATES.find(12))
	check(not panel._rate_custom.visible, "a listed rate puts the custom boxes away")
	run({"cmd": "set_document", "rate": {"num": 30000, "den": 1001}})
	await frames(2)
	check_eq(panel._rate.get_item_text(panel._rate.selected), "29.97 fps", "a custom rate set elsewhere is shown")
	check(panel._rate_custom.visible, "with its boxes")
	check_eq([panel._rate_num.text, panel._rate_den.text], ["30000", "1001"], "holding the rate")


func test_working_range_in_seconds_and_frames() -> void:
	var panel := await _panel()
	var start_seconds: NumberField = panel._range_seconds["range_start"]
	var start_frames: NumberField = panel._range_frames["range_start"]
	var end_seconds: NumberField = panel._range_seconds["range_end"]
	var end_frames: NumberField = panel._range_frames["range_end"]
	check_eq([start_seconds.text, start_frames.text, end_seconds.text, end_frames.text], ["0", "0", "10", "240"], "the range of a new document")

	await _type(end_seconds, "5")
	check_eq(int(editor.get_info().range_end), 5 * LimnerEditor.FLICKS_PER_SECOND, "end typed in seconds")
	check_eq(end_frames.text, "120", "frames follow seconds")
	await _type(start_frames, "24")
	check_eq(int(editor.get_info().range_start), LimnerEditor.FLICKS_PER_SECOND, "start typed in frames")
	check_eq(start_seconds.text, "1", "seconds follow frames")
	await _type(end_frames, "60")
	check_eq(int(editor.get_info().range_end), 60 * int(editor.get_info().frame_duration), "end typed in frames")

	await _type(end_seconds, "0.5")
	check_eq(int(editor.get_info().range_end), 60 * int(editor.get_info().frame_duration), "an end before the start is refused")
	check_eq(messages, ["the range must start at or after zero and end after it starts"] as Array[String], "with the document's reason")
	check_eq(end_seconds.text, "2.5", "and the box goes back")
	await _check_undo_steps(3, "range")
	check_eq(end_seconds.text, "10", "the boxes follow undo")


func test_camera_resolution_choices_and_custom_sizes() -> void:
	var panel := await _panel()
	check_eq(panel._resolution.get_item_text(panel._resolution.selected), "1920 × 1080", "resolution shown")
	check_eq(panel._resolution.item_count, 8, "seven sizes and Custom")
	check(not panel._resolution_custom.visible, "no custom boxes for a listed size")

	await _choose(panel._resolution, 1)
	check_eq(editor.get_info().camera_resolution, [1080, 1920], "the second size in the list")
	await _choose(panel._resolution, DocumentPanel.RESOLUTIONS.size())
	check(panel._resolution_custom.visible, "Custom shows its boxes")
	check_eq([panel._width.text, panel._height.text], ["1080", "1920"], "which start at the current size")

	await _type(panel._width, "1234")
	check_eq(editor.get_info().camera_resolution, [1234, 1920], "a typed width")
	check_eq(panel._resolution.get_item_text(panel._resolution.selected), "1234 × 1920", "the custom size is shown")
	await _type(panel._height, "5")
	check_eq(editor.get_info().camera_resolution, [1234, 16], "sizes start at 16")
	await _type(panel._height, "99999")
	check_eq(editor.get_info().camera_resolution, [1234, 16384], "and stop at 16384")
	await _check_undo_steps(4, "resolution")
	check_eq(panel._resolution.get_item_text(panel._resolution.selected), "Custom…", "Custom stays chosen until a listed size is picked")
	check_eq([panel._width.text, panel._height.text], ["1920", "1080"], "its boxes follow undo")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_contents_are_counted() -> void:
	var panel := await _panel()
	check_eq(panel._contents.text, "3 layers, 0 strokes, 0 points", "a new document")
	run({"cmd": "add_layer", "kind": "light"})
	await frames(2)
	check_eq(panel._contents.text, "4 layers, 0 strokes, 0 points", "after adding a layer")
	check_eq(Form.thousands(1234567), "1,234,567", "large counts are grouped in threes")
	check_eq(Form.thousands(999), "999", "small ones are left alone")


func test_follows_another_document() -> void:
	var panel := await _panel()
	await _type(panel._title, "First")
	await _choose(panel._rate, DocumentPanel.RATES.size())
	editor.new_document()
	await frames(2)
	check_eq(panel._title.text, "", "the title of the new document")
	check(not panel._rate_custom.visible, "the custom boxes are put away")
