extends LimnerTest
## The Transform panel shows a layer's placement as numbers, writes what is typed
## into them, and sends the options and commands of the Transform toolbox.


func _panel() -> TransformPanel:
	var panel := TransformPanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


## Types `text` into a box and presses Enter.
func _type(field: LineEdit, text: String) -> void:
	field.text = text
	field.text_submitted.emit(text)
	await frames(2)


func _press(panel: Control, text: String) -> void:
	for button: Button in panel.find_children("*", "Button", true, false):
		if button.text == text:
			check(not button.disabled, "%s can be pressed" % text)
			button.pressed.emit()
			await frames(2)
			return
	failures.append("there is no button %s" % text)


func _details(layer_name: String) -> Dictionary:
	return world.scene.layer_details(int(row_named(layer_name).id))


## Undoes `steps` times and checks that this is exactly how many steps there were.
func _check_undo_steps(steps: int, what: String) -> void:
	for i in steps:
		check_eq(editor.status.can_undo, true, "%s: step %d of %d can be undone" % [what, i + 1, steps])
		editor.undo()
		await frames(2)
	check_eq(editor.status.can_undo, false, "%s: nothing is left to undo after %d steps" % [what, steps])


func _texts(fields: Array[NumberField]) -> Array:
	return fields.map(func(field: NumberField) -> String: return field.text)


func test_numbers_are_shown_with_six_digits_at_any_size() -> void:
	check_eq(NumberField.format(0.0), "0", "zero")
	check_eq(NumberField.format(1.5), "1.5", "a short number")
	check_eq(NumberField.format(-2.0), "-2", "a whole number")
	check_eq(NumberField.format(1000.0), "1000", "a round thousand")
	check_eq(NumberField.format(123456.789), "123457", "six digits before the point")
	check_eq(NumberField.format(-123.456789), "-123.457", "six digits across the point")
	check_eq(NumberField.format(0.000123456), "0.000123456", "a small number")
	check_eq(NumberField.format(0.00000123456789), "1.23457e-6", "a very small number")
	check_eq(NumberField.format(1e-7), "1e-7", "a very small round number")
	check_eq(NumberField.format(12345678.9), "12345679", "a large number")
	check_eq(NumberField.format(1e9), "1e9", "a very large round number")
	check_eq(NumberField.format(2.5e12), "2.5e12", "a very large number")
	check_near("1.23457e-6".to_float(), 0.00000123457, "what is shown can be typed back", 1e-12)


func test_typing_a_position_moves_the_layer_one_undo_step_each() -> void:
	var panel := await _panel()
	check_eq(panel._caption.text, "Numbers for the current layer, Paint", "whose numbers these are")
	await _type(panel._position[0], "1.5")
	await _type(panel._position[1], "-2.25")
	await _type(panel._position[2], "1234.56789")
	var translation: Array = _details("Paint").transform.translation
	check_eq(translation, [1.5, -2.25, 1234.56789], "position in the document, at full precision")
	check_eq(_texts(panel._position), ["1.5", "-2.25", "1234.57"], "position shown")
	check_eq(editor.status.undo_label, "Move layer", "what undo calls it")
	editor.undo()
	await frames(2)
	check_eq(_details("Paint").transform.translation, [1.5, -2.25, 0.0], "one undo takes back one box")
	check_eq(panel._position[2].text, "0", "the box follows undo")
	await _check_undo_steps(2, "position")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_typing_one_box_keeps_the_others_exact() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	run({"cmd": "set_layer", "layer": paint, "transform": {"rotation": [0.1, 0.2, 0.3, 0.9273618495495703], "scale": 1.23456789012, "translation": [0.111111111111, 0.222222222222, 0.333333333333], "flip": "y"}})
	await frames(2)
	var before: Dictionary = _details("Paint").transform
	await _type(panel._position[0], "5")
	var after: Dictionary = _details("Paint").transform
	check_eq(after.translation, [5.0, before.translation[1], before.translation[2]], "the other two coordinates")
	for i in 4:
		check_near(after.rotation[i], before.rotation[i], "rotation component %d" % i, 1e-12)
	check_eq(after.scale, before.scale, "scale")
	check_eq(after.flip, "y", "flip")
	check_eq(panel._flip.text, "Y", "flip shown")


func test_typing_a_rotation_round_trips_and_leaves_the_other_angles() -> void:
	var panel := await _panel()
	await _type(panel._rotation[0], "30")
	check_eq(_texts(panel._rotation), ["30", "0", "0"], "after X")
	await _type(panel._rotation[1], "45")
	check_eq(_texts(panel._rotation), ["30", "45", "0"], "after Y")
	await _type(panel._rotation[2], "60")
	check_eq(_texts(panel._rotation), ["30", "45", "60"], "after Z")
	var turn := Basis.from_euler(Vector3(deg_to_rad(30.0), deg_to_rad(45.0), deg_to_rad(60.0)), EULER_ORDER_YXZ).get_rotation_quaternion()
	var expected := [turn.x, turn.y, turn.z, turn.w]
	var stored: Array = _details("Paint").transform.rotation
	for i in 4:
		check_near(stored[i], expected[i], "rotation component %d in the document" % i, 1e-6)

	# Past a quarter turn the same rotation has other angles; the ones typed stay.
	await _type(panel._rotation[0], "200")
	check_eq(_texts(panel._rotation), ["200", "45", "60"], "after a large X")
	run({"cmd": "set_options", "values": {"show_grid": false}})
	await frames(2)
	check_eq(_texts(panel._rotation), ["200", "45", "60"], "after an unrelated refresh")

	editor.undo()
	await frames(2)
	check_near(panel._rotation[0].get_value(), 30.0, "X after undo", 1e-3)
	check_near(panel._rotation[1].get_value(), 45.0, "Y after undo", 1e-3)
	check_near(panel._rotation[2].get_value(), 60.0, "Z after undo", 1e-3)
	await _check_undo_steps(3, "rotation")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_scale_is_typed_and_stays_above_zero() -> void:
	var panel := await _panel()
	await _type(panel._scale, "2.5")
	check_eq(_details("Paint").transform.scale, 2.5, "scale in the document")
	await _type(panel._scale, "0")
	await _type(panel._scale, "-3")
	check_eq(_details("Paint").transform.scale, 2.5, "scale after typing zero and less")
	check_eq(panel._scale.text, "2.5", "the box goes back")
	check_eq(messages.size(), 2, "each refusal is explained")
	await _type(panel._scale, "0.000125")
	check_eq(_details("Paint").transform.scale, 0.000125, "a very small scale")
	await _check_undo_steps(2, "scale")


func test_typing_a_pivot_moves_only_the_pivot() -> void:
	var panel := await _panel()
	await _type(panel._pivot[0], "0.75")
	await _type(panel._pivot[2], "-4")
	var details := _details("Paint")
	check_eq(details.pivot.translation, [0.75, 0.0, -4.0], "pivot in the document")
	check_eq(details.transform.translation, [0.0, 0.0, 0.0], "the layer has not moved")
	check_eq(editor.status.undo_label, "Move pivot", "what undo calls it")
	await _check_undo_steps(2, "pivot")
	check_eq(panel._pivot[0].text, "0", "the box follows undo")


func test_leaving_a_box_commits_and_an_unchanged_box_does_nothing() -> void:
	var panel := await _panel()
	await _type(panel._position[0], "0")
	await _type(panel._position[1], "0.0")
	await _type(panel._scale, "1.00")
	await _type(panel._rotation[2], "0.0")
	await _type(panel._pivot[1], "not a number")
	panel._position[2].focus_exited.emit()
	await frames(2)
	check_eq(editor.status.can_undo, false, "nothing was changed, so nothing can be undone")
	check_eq(panel._pivot[1].text, "0", "text that is not a number is put back")

	panel._position[2].text = "3"
	panel._position[2].focus_exited.emit()
	await frames(2)
	check_eq(_details("Paint").transform.translation, [0.0, 0.0, 3.0], "leaving the box commits")
	await _check_undo_steps(1, "leaving a box")


func test_a_box_being_typed_in_is_left_alone() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	var box := panel._position[0]
	box.grab_focus()
	box.edit()
	box.text = "7"
	run({"cmd": "set_layer", "layer": paint, "transform": {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [3.0, 4.0, 0.0], "flip": "none"}})
	await frames(2)
	check_eq(box.text, "7", "the box keeps what is being typed")
	check_eq(panel._position[1].text, "4", "the other boxes follow the document")
	box.release_focus()
	await frames(2)
	check_eq(_details("Paint").transform.translation, [7.0, 4.0, 0.0], "what was typed is written on leaving")


func test_an_animated_layer_is_followed_along_the_timeline() -> void:
	var panel := await _panel()
	run({"cmd": "set_options", "values": {"recording": true}})
	await _type(panel._position[0], "1")
	run({"cmd": "seek", "time": LimnerEditor.to_flicks(1.0)})
	await frames(2)
	await _type(panel._position[0], "5")
	check_eq(panel._position[0].text, "5", "at the second key")
	# Playback moves the playhead without a change signal; so does this.
	world.scene.set_time(0.5)
	await frames(3)
	check_eq(panel._position[0].text, "3", "halfway between the keys")
	world.scene.set_time(0.0)
	await frames(3)
	check_eq(panel._position[0].text, "1", "back at the first key")


func test_option_controls_set_editor_options() -> void:
	var panel := await _panel()
	var keys := ["level_horizon", "show_gimbal", "animate_duplicates", "allow_translate", "allow_rotate", "allow_scale"]
	check_eq(panel._option_toggles.keys(), keys, "the options with a switch")
	for key: String in keys:
		var before: bool = editor.get_options()[key]
		var control: BaseButton = panel._option_toggles[key]
		check_eq(control.button_pressed, before, "%s shown" % key)
		control.button_pressed = not before
		await frames(2)
		check_eq(editor.get_options()[key], not before, "%s after the switch" % key)
		run({"cmd": "set_options", "values": {key: before}})
		await frames(2)
		check_eq(control.button_pressed, before, "%s follows the editor" % key)
	check_eq(panel._option_toggles["animate_duplicates"].tooltip_text, "Each repeat of a duplicate-and-move puts the copy on a new frame", "tooltip of Animate duplicates")

	for index in 3:
		panel._auto_select.select(index)
		panel._auto_select.item_selected.emit(index)
		await frames(2)
		check_eq(editor.get_options().auto_select, ["none", "layer", "group"][index], "what grabbing a stroke moves")
	run({"cmd": "set_options", "values": {"auto_select": "layer"}})
	await frames(2)
	check_eq(panel._auto_select.get_item_text(panel._auto_select.selected), "Its layer", "the choice follows the editor")
	check_eq(editor.status.can_undo, false, "options are not undo steps")


func test_show_gimbal_shows_it_on_the_current_layer_while_the_view_is_the_transform_object() -> void:
	var panel := await _panel()
	var toggle: BaseButton = panel._option_toggles["show_gimbal"]
	check(toggle.tooltip_text.contains("the current layer while the view is the transform object"), "the tooltip says what it shows: %s" % toggle.tooltip_text)
	LimnerDemo.stroke(world.scene, [Vector3(0.0, 1.0, -1.0), Vector3(0.4, 1.2, -1.0)])
	await frames(2)
	check(world.scene.gimbal().is_empty(), "no gimbal until asked for")
	toggle.button_pressed = true
	await frames(2)
	var gimbal := world.scene.gimbal()
	check_eq(gimbal.get("subject"), "layer", "a layer's gimbal")
	check_eq(int(gimbal.get("layer", -1)), int(row_named("Paint").id), "the current layer's, with the view as the transform object")
	var at: Vector3 = gimbal.get("position", Vector3.ZERO)
	check(at.is_equal_approx(Vector3(0.2, 1.1, -1.0)), "in the middle of what is painted: %s" % at)
	check_eq(panel._target_name.text, "View", "the transform object is still the view")


func test_rotation_snap_steps_show_their_angle() -> void:
	var panel := await _panel()
	check_eq(panel._steps_note.text, "12 steps = 30°", "the angle of the usual steps")
	await _type(panel._steps, "8")
	check_eq(int(editor.get_options().rotation_snap_steps), 8, "typed steps")
	check_eq(panel._steps_note.text, "8 steps = 45°", "the angle of 8 steps")
	panel._steps.nudge(-1.0)
	await frames(2)
	check_eq(int(editor.get_options().rotation_snap_steps), 7, "one fewer")
	check_eq(panel._steps_note.text, "7 steps = 51.4286°", "the angle of 7 steps")
	await _type(panel._steps, "1000")
	check_eq(int(editor.get_options().rotation_snap_steps), 360, "steps stop at 360")
	await _type(panel._steps, "0")
	check_eq(int(editor.get_options().rotation_snap_steps), 1, "steps start at 1")
	check_eq(panel._steps_note.text, "1 step = 360°", "the angle of one step")


func test_flip_and_reset_buttons() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	for axis: String in ["X", "Y", "Z"]:
		await _press(panel, "Flip %s" % axis)
		check_eq(_details("Paint").transform.flip, axis.to_lower(), "flip after Flip %s" % axis)
		check_eq(panel._flip.text, axis, "flip shown after Flip %s" % axis)
		editor.undo()
		await frames(2)
		check_eq(panel._flip.text, "None", "flip shown after undo")

	run({"cmd": "set_layer", "layer": paint, "transform": {"rotation": [0.0, 0.7071067811865476, 0.0, 0.7071067811865476], "scale": 3.0, "translation": [1.0, 2.0, 3.0], "flip": "z"}})
	await frames(2)
	check_eq(_texts(panel._rotation), ["0", "90", "0"], "a quarter turn about Y, shown")
	await _press(panel, "Reset transform")
	check_eq(_details("Paint").transform, TransformPanel.IDENTITY, "transform after Reset transform")
	check_eq(_texts(panel._position), ["0", "0", "0"], "position shown after Reset transform")
	check_eq(panel._scale.text, "1", "scale shown after Reset transform")

	var pivot := {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [1.0, 2.0, 3.0], "flip": "none"}
	run({"cmd": "set_layer", "layer": paint, "pivot": pivot})
	await frames(2)
	check_eq(_texts(panel._pivot), ["1", "2", "3"], "pivot shown")
	await _press(panel, "Make pivot origin")
	check_eq(editor.status.undo_label, "Make pivot origin", "the command behind Make pivot origin")
	check_eq(_texts(panel._pivot), ["0", "0", "0"], "the pivot becomes the origin")
	check_eq(_texts(panel._position), ["1", "2", "3"], "and the layer's position takes it up")
	run({"cmd": "set_layer", "layer": paint, "pivot": pivot})
	await frames(2)
	await _press(panel, "Reset pivot")
	check_eq(editor.status.undo_label, "Reset pivot", "the command behind Reset pivot")
	check_eq(_texts(panel._pivot), ["0", "0", "0"], "the pivot of an empty layer goes to its origin")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_follows_the_transform_object_and_the_current_layer() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	check_eq(panel._target_name.text, "View", "grabbing moves the view at first")
	check(panel._home.disabled, "the home button has nothing to do")

	var light: Dictionary = run({"cmd": "add_layer", "kind": "light"})
	await frames(2)
	check_eq(panel._caption.text, "Numbers for the current layer, Light", "the numbers follow the current layer")
	await _type(panel._position[0], "2")
	check_eq(_details("Light").transform.translation[0], 2.0, "typing moves the current layer")
	var placed: Array = _details("Light").transform.translation

	run({"cmd": "set_transform_target", "layer": paint})
	await frames(2)
	check_eq(panel._target_name.text, "Paint", "the transform object is named")
	check_eq(panel._caption.text, "Numbers for the transform object, Paint", "the numbers follow the transform object")
	check_eq(panel._position[0].text, "0", "the transform object's position is shown")
	await _type(panel._position[1], "9")
	check_eq(_details("Paint").transform.translation[1], 9.0, "typing moves the transform object")
	check_eq(_details("Light").transform.translation, placed, "and not the current layer")

	await _press(panel, "Use current layer")
	check_eq(editor.status.transform_target, int(light.layer), "Use current layer")
	check_eq(panel._target_name.text, "Light", "the new transform object is named")
	check(panel._use_current.disabled, "the current layer is already the transform object")
	check(not panel._home.disabled, "the home button is live")
	panel._home.pressed.emit()
	await frames(2)
	check_eq(editor.status.transform_target, null, "the home button gives grabbing back to the view")
	check_eq(panel._target_name.text, "View", "and the panel says so")


func test_the_root_a_locked_layer_and_no_layer_cannot_be_typed_into() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	check(panel._position[0].editable, "a paint layer can be moved")

	run({"cmd": "set_layer", "layer": paint, "locked": true})
	await frames(2)
	for control: Control in panel._layer_controls:
		check(not (control.editable if control is LineEdit else not control.disabled), "%s is off for a locked layer" % control.tooltip_text)
	check(panel._reset_transform.disabled, "Reset transform is off for a locked layer")
	check_eq(panel._caption.text, "Numbers for the current layer, Paint (locked)", "the caption says it is locked")
	run({"cmd": "set_layer", "layer": paint, "locked": false})
	await frames(2)
	check(panel._scale.editable, "unlocking brings the boxes back")

	run({"cmd": "select_layers", "layers": [int(editor.get_info().root)]})
	await frames(2)
	check(not panel._position[0].editable, "the root cannot be moved")
	check(panel._use_current.disabled, "the root cannot be the transform object")
	check(panel._reset_transform.disabled, "the root cannot be reset")

	run({"cmd": "select_layers", "layers": []})
	await frames(2)
	check_eq(panel._caption.text, "No layer is selected.", "the caption with no layer")
	check(not panel._pivot[2].editable, "there is nothing to type into")
	check(panel._use_current.disabled, "there is no current layer to use")
	check(not panel._reset_transform.disabled, "Reset transform then resets the view")
	var revision: int = editor.status.doc_revision
	panel._reset_transform.pressed.emit()
	await frames(2)
	check_eq(editor.status.doc_revision, revision, "resetting the view is not a change to the document")
	check(messages.is_empty(), "no errors: %s" % [messages])
