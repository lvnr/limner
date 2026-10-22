extends LimnerTest
## The Color panel edits the paint colour and the background, keeps its own hue,
## saturation and value, and stores colours in the palette.

const PAINT := ColorPanel.Target.PAINT
const BACKGROUND := ColorPanel.Target.BACKGROUND
const UNUSED := [0.82, 0.82, 0.82]
const NEW_BACKGROUND := [0.8, 0.8, 0.8]


func _panel() -> ColorPanel:
	var panel := ColorPanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


## The paint colour as the session has it now, without waiting for a change signal.
func _paint() -> Array:
	return world.scene.tool_settings()["color"]


func _background() -> Array:
	return world.scene.document_info()["background"]


func _swatch(index: int) -> Array:
	var palette: Array = world.scene.document_info()["palette"]
	return palette[index]


## What the document stores for a colour chosen as hue, saturation and value.
func _linear(hue: float, saturation: float, value: float) -> Array:
	return LimnerEditor.linear_rgb(Color.from_hsv(hue, saturation, value))


func check_color(actual: Array, expected: Array, what: String) -> void:
	for channel in 3:
		var a: float = actual[channel]
		var b: float = expected[channel]
		if absf(a - b) > 1e-4:
			failures.append("%s: got %s, expected %s" % [what, actual, expected])
			return


## One whole drag over part of the picker, through the places given.
func _drag(area: PickerArea, places: Array) -> void:
	area.drag_started.emit()
	for place: Vector2 in places:
		area.changed.emit(place)
	area.drag_ended.emit()


## A mouse button or touch event over a control, at a place given as fractions of its size.
func _pointer(event: InputEvent, control: Control, place: Vector2) -> void:
	event.set("position", control.global_position + control.size * place)
	event.set("global_position", event.get("position"))
	get_viewport().push_input(event)


func _mouse_button(control: Control, place: Vector2, pressed: bool) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = MOUSE_BUTTON_LEFT
	event.pressed = pressed
	event.button_mask = MOUSE_BUTTON_MASK_LEFT if pressed else 0
	_pointer(event, control, place)


func _mouse_move(control: Control, place: Vector2) -> void:
	var event := InputEventMouseMotion.new()
	event.button_mask = MOUSE_BUTTON_MASK_LEFT
	_pointer(event, control, place)


func test_shows_the_colours_of_a_new_document() -> void:
	var panel := await _panel()
	var paint: ColorChip = panel._bars[PAINT]
	var background: ColorChip = panel._bars[BACKGROUND]
	check_eq(paint.text, "Paint colour", "first bar")
	check_eq(background.text, "Background colour", "second bar")
	check(paint.selected and not background.selected, "the paint colour is the one edited at first")
	check_eq(paint.color.to_html(false), "000000", "paint bar")
	check_eq(background.color.to_html(false), LimnerEditor.screen_color(NEW_BACKGROUND).to_html(false), "background bar")
	check_eq(panel._hex.text, "#000000", "hex of the paint colour")
	check_eq(panel._swatches.size(), 16, "swatches")
	check_eq(panel._swatches[5].color.to_html(false), LimnerEditor.screen_color(UNUSED).to_html(false), "an unused swatch")
	check_fits(panel)


func test_the_picker_sets_the_paint_colour() -> void:
	var panel := await _panel()
	_drag(panel._strip, [Vector2(0.5, 0.2), Vector2(0.5, 0.6)])
	_drag(panel._square, [Vector2(0.1, 0.9), Vector2(0.25, 0.75)])
	check_color(_paint(), _linear(0.6, 0.25, 0.75), "paint colour after dragging")
	check_color(_background(), NEW_BACKGROUND, "the background is left alone")
	await frames(2)
	check_eq((panel._bars[PAINT] as ColorChip).color.to_html(false), Color.from_hsv(0.6, 0.25, 0.75).to_html(false), "paint bar")
	check_eq(panel._hex.text, "#" + Color.from_hsv(0.6, 0.25, 0.75).to_html(false), "hex field")
	check_eq(panel._square.marker, Vector2(0.25, 0.75), "ring on the square")
	check_near(panel._strip.marker.y, 0.6, "marker on the hue strip")
	check_near(panel._square.hue, 0.6, "the square is drawn for the hue")

	_drag(panel._strip, [Vector2(0.5, 0.25)])
	_drag(panel._square, [Vector2(0.5, 0.4)])
	check_color(_paint(), _linear(0.25, 0.5, 0.4), "paint colour after moving to another hue, saturation and value")
	check_eq(editor.status.can_undo, false, "the paint colour is not an undo step")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_picker_is_dragged_with_a_mouse_or_a_finger() -> void:
	var panel := await _panel()
	_mouse_button(panel._strip, Vector2(0.5, 0.1), true)
	_mouse_move(panel._strip, Vector2(0.5, 0.5))
	check(panel._dragging, "a drag is under way while the button is down")
	# The pointer may leave the strip during a drag; the place stays inside it.
	_mouse_move(panel._strip, Vector2(3.0, 0.25))
	_mouse_button(panel._strip, Vector2(3.0, 0.25), false)
	check(not panel._dragging, "the drag ends with the button")
	check_near(panel._hue, 0.25, "hue after a mouse drag on the strip", 1e-3)

	_mouse_button(panel._square, Vector2(0.5, 0.5), true)
	_mouse_move(panel._square, Vector2(0.75, 0.25))
	_mouse_button(panel._square, Vector2(0.75, 0.25), false)
	check_near(panel._saturation, 0.75, "saturation after a mouse drag on the square", 1e-3)
	check_near(panel._value, 0.75, "value after a mouse drag on the square", 1e-3)
	# Moving without a button down is only hovering, and must do nothing.
	var hover := InputEventMouseMotion.new()
	_pointer(hover, panel._square, Vector2(0.1, 0.9))
	check_near(panel._saturation, 0.75, "hovering changes nothing", 1e-3)

	var touch := InputEventScreenTouch.new()
	touch.pressed = true
	_pointer(touch, panel._square, Vector2(0.2, 0.2))
	var drag := InputEventScreenDrag.new()
	_pointer(drag, panel._square, Vector2(0.4, 0.6))
	var lift := InputEventScreenTouch.new()
	lift.pressed = false
	_pointer(lift, panel._square, Vector2(0.4, 0.6))
	check(not panel._dragging, "the drag ends when the finger lifts")
	check_near(panel._saturation, 0.4, "saturation after a finger drag", 1e-3)
	check_near(panel._value, 0.4, "value after a finger drag", 1e-3)
	check_color(_paint(), _linear(panel._hue, panel._saturation, panel._value), "paint colour after the drags")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_round_trip_does_not_move_the_picker() -> void:
	var panel := await _panel()
	var on_strip := Vector2(0.5, 0.123456)
	var on_square := Vector2(0.654321, 0.456789)
	_drag(panel._strip, [on_strip])
	_drag(panel._square, [on_square])
	await frames(3)
	check_eq(panel._hue, float(on_strip.y), "hue after the colour came back")
	check_eq(panel._saturation, float(on_square.x), "saturation after the colour came back")
	check_eq(panel._value, float(on_square.y), "value after the colour came back")

	# Grey has no hue and black no saturation; the picker remembers both.
	_drag(panel._square, [Vector2(0.0, 0.5)])
	await frames(3)
	check_eq(panel._hue, float(on_strip.y), "hue of a grey picked on the square")
	_drag(panel._square, [Vector2(0.75, 0.0)])
	await frames(3)
	check_eq(panel._hue, float(on_strip.y), "hue of black picked on the square")
	check_eq(panel._saturation, 0.75, "saturation of black picked on the square")
	check_near(panel._strip.marker.y, 0.123456, "the hue marker stays put")

	run({"cmd": "set_color", "color": [0.2, 0.4, 0.9]})
	await frames(2)
	var blue := LimnerEditor.screen_color([0.2, 0.4, 0.9])
	check_near(panel._hue, blue.h, "hue of a colour set elsewhere")
	run({"cmd": "set_color", "color": [0.3, 0.3, 0.3]})
	await frames(2)
	check_near(panel._hue, blue.h, "hue kept for a grey set elsewhere")
	check_near(panel._saturation, 0.0, "a grey has no saturation")
	check_near(panel._value, LimnerEditor.screen_color([0.3, 0.3, 0.3]).v, "value of the grey")
	run({"cmd": "set_color", "color": [0.2, 0.4, 0.9]})
	run({"cmd": "set_color", "color": [0.0, 0.0, 0.0]})
	await frames(2)
	check_near(panel._hue, blue.h, "hue kept for black set elsewhere")
	check_near(panel._value, 0.0, "black has no value")


func test_a_background_drag_is_one_undo_step() -> void:
	var panel := await _panel()
	(panel._bars[BACKGROUND] as ColorChip).chosen.emit()
	await frames(2)
	check((panel._bars[BACKGROUND] as ColorChip).selected, "the background bar is selected")
	check(not (panel._bars[PAINT] as ColorChip).selected, "the paint bar is not")
	check_eq(panel._hex.text, "#" + LimnerEditor.screen_color(NEW_BACKGROUND).to_html(false), "the picker shows the background")

	# Dragging over the hue of a grey changes nothing, so it must not leave an undo step.
	_drag(panel._strip, [Vector2(0.5, 0.9), Vector2(0.5, 0.3)])
	await frames(2)
	check_color(_background(), NEW_BACKGROUND, "a grey stays the same grey whatever its hue")
	check_eq(editor.status.can_undo, false, "a drag that changed nothing is not an undo step")
	_drag(panel._square, [Vector2(0.2, 0.9), Vector2(0.5, 0.6), Vector2(0.8, 0.4)])
	check_color(_background(), _linear(0.3, 0.8, 0.4), "background after dragging")
	check_color(_paint(), [0.0, 0.0, 0.0], "the paint colour is left alone")
	await frames(2)
	check_eq(editor.status.undo_label, "Change background", "name of the undo step")
	check_eq((panel._bars[BACKGROUND] as ColorChip).color.to_html(false), Color.from_hsv(0.3, 0.8, 0.4).to_html(false), "background bar")

	editor.undo()
	await frames(2)
	check_color(_background(), NEW_BACKGROUND, "background after one undo")
	check_eq(editor.status.can_undo, false, "nothing else to undo")
	check_eq(panel._hex.text, "#" + LimnerEditor.screen_color(NEW_BACKGROUND).to_html(false), "the picker follows the undo")
	check_near(panel._hue, 0.3, "and keeps the hue it had")

	_drag(panel._square, [Vector2(0.5, 0.8), Vector2(0.5, 0.5), Vector2(0.5, 0.2)])
	await frames(2)
	check_near(LimnerEditor.screen_color(_background()).v, 0.2, "background after dragging down the square")
	editor.undo()
	await frames(2)
	check_color(_background(), NEW_BACKGROUND, "that drag was one step too")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_hex_entry() -> void:
	var panel := await _panel()
	panel._hex.text = "#ff8000"
	panel._hex.text_submitted.emit("#ff8000")
	check_color(_paint(), LimnerEditor.linear_rgb(Color.html("ff8000")), "paint colour typed as hex")
	await frames(2)
	check_eq(panel._hex.text, "#ff8000", "hex field")
	check_near(panel._hue, Color.html("ff8000").h, "the picker takes the typed colour")

	panel._hex.text_submitted.emit(" 3D8BFD ")
	check_color(_paint(), LimnerEditor.linear_rgb(Color.html("3d8bfd")), "hex without the # and in capitals")
	check_eq(panel._hex.text, "#3d8bfd", "the field shows it the usual way")

	panel._hex.text = "nonsense"
	panel._hex.text_submitted.emit("nonsense")
	check_color(_paint(), LimnerEditor.linear_rgb(Color.html("3d8bfd")), "text that is not a colour changes nothing")
	check_eq(panel._hex.text, "#3d8bfd", "and the field goes back")
	check_eq(messages.size(), 1, "the user is told why")

	# Leaving the field enters what was typed, and only what was typed: a field
	# that was merely clicked in follows the picker and changes nothing when left.
	panel._hex.grab_focus()
	panel._hex.text = "#00ff00"
	panel._hex.text_changed.emit("#00ff00")
	await frames(2)
	check_eq(panel._hex.text, "#00ff00", "text being typed is left alone")
	panel._hex.release_focus()
	check_color(_paint(), [0.0, 1.0, 0.0], "leaving the field enters the colour")
	panel._hex.grab_focus()
	_drag(panel._square, [Vector2(0.5, 0.5)])
	check_eq(panel._hex.text, "#" + Color.from_hsv(1.0 / 3.0, 0.5, 0.5).to_html(false), "the field follows the picker while it has focus")
	panel._hex.release_focus()
	check_color(_paint(), _linear(1.0 / 3.0, 0.5, 0.5), "leaving an untouched field changes nothing")

	(panel._bars[BACKGROUND] as ColorChip).chosen.emit()
	panel._hex.text_submitted.emit("#102030")
	check_color(_background(), LimnerEditor.linear_rgb(Color.html("102030")), "background typed as hex")
	await frames(2)
	editor.undo()
	await frames(2)
	check_color(_background(), NEW_BACKGROUND, "one undo takes the typed background back")


func test_swatches() -> void:
	var panel := await _panel()
	var add: Button = panel.find_children("*", "Button", true, false).filter(func(node: Button) -> bool: return node.text == "Add")[0]
	run({"cmd": "set_color", "color": [1.0, 0.0, 0.0]})
	await frames(2)
	add.pressed.emit()
	await frames(2)
	check_color(_swatch(0), [1.0, 0.0, 0.0], "the first colour added goes in the first swatch")
	check_eq(panel._swatches[0].color.to_html(false), "ff0000", "and is shown there")
	run({"cmd": "set_color", "color": [0.0, 1.0, 0.0]})
	await frames(2)
	add.pressed.emit()
	await frames(2)
	check_color(_swatch(1), [0.0, 1.0, 0.0], "the second goes in the second")
	check_color(_swatch(2), UNUSED, "the third is still unused")

	editor.undo()
	await frames(2)
	check_color(_swatch(1), UNUSED, "adding a swatch can be undone")
	check_eq(panel._swatches[1].color.to_html(false), LimnerEditor.screen_color(UNUSED).to_html(false), "and the swatch shows it")
	add.pressed.emit()
	await frames(2)
	check_color(_swatch(1), [0.0, 1.0, 0.0], "the freed swatch is the next one used")

	panel._swatches[0].chosen.emit()
	check_color(_paint(), [1.0, 0.0, 0.0], "pressing a swatch makes it the paint colour")
	await frames(2)
	check_eq(panel._hex.text, "#ff0000", "and the picker shows it")
	(panel._bars[BACKGROUND] as ColorChip).chosen.emit()
	await frames(2)
	panel._swatches[1].chosen.emit()
	check_color(_background(), [0.0, 1.0, 0.0], "with the background bar selected it becomes the background")
	check_color(_paint(), [1.0, 0.0, 0.0], "and the paint colour stays")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_replacing_a_swatch() -> void:
	var panel := await _panel()
	run({"cmd": "set_color", "color": [0.0, 0.0, 1.0]})
	await frames(2)
	var swatch := panel._swatches[7]
	var chosen := [0]
	swatch.chosen.connect(func() -> void: chosen[0] += 1)

	# A short press is a choice, not a replacement.
	swatch.button_down.emit()
	await get_tree().create_timer(0.1).timeout
	swatch.button_up.emit()
	swatch._pressed()
	await get_tree().create_timer(0.6).timeout
	check_eq(chosen[0], 1, "a short press chooses the swatch")
	check_color(_swatch(7), UNUSED, "and leaves it as it was")
	check_color(_paint(), UNUSED, "its colour became the paint colour")

	run({"cmd": "set_color", "color": [0.0, 0.0, 1.0]})
	await frames(2)
	swatch.button_down.emit()
	await get_tree().create_timer(0.6).timeout
	check_color(_swatch(7), [0.0, 0.0, 1.0], "holding a swatch for half a second replaces it")
	swatch.button_up.emit()
	swatch._pressed()
	check_eq(chosen[0], 1, "letting go after a hold is not a choice")
	await frames(2)
	check_eq(swatch.color.to_html(false), "0000ff", "the swatch shows its new colour")

	var right_click := InputEventMouseButton.new()
	right_click.button_index = MOUSE_BUTTON_RIGHT
	right_click.pressed = true
	_pointer(right_click, panel._swatches[12], Vector2(0.5, 0.5))
	check_color(_swatch(12), [0.0, 0.0, 1.0], "a right-click replaces a swatch too")
	check_color(_swatch(11), UNUSED, "and only that one")

	editor.undo()
	await frames(2)
	check_color(_swatch(12), UNUSED, "replacing a swatch can be undone")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_add_goes_round_when_every_swatch_is_used() -> void:
	var panel := await _panel()
	for index in 18:
		run({"cmd": "set_color", "color": [0.05 * (index + 1), 0.0, 0.5]})
		await frames(2)
		panel._add_swatch()
		await frames(2)
	check_color(_swatch(15), [0.05 * 16, 0.0, 0.5], "the sixteenth colour fills the last swatch")
	check_color(_swatch(0), [0.05 * 17, 0.0, 0.5], "the seventeenth starts again at the first")
	check_color(_swatch(1), [0.05 * 18, 0.0, 0.5], "the eighteenth takes the second")
	check_color(_swatch(2), [0.05 * 3, 0.0, 0.5], "the third is still the third colour")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_follows_changes_made_behind_its_back() -> void:
	var panel := await _panel()
	run({"cmd": "set_color", "color": [0.8, 0.2, 0.05]})
	run({"cmd": "set_document", "background": [0.1, 0.1, 0.3]})
	run({"cmd": "set_palette_color", "index": 9, "color": [0.0, 0.5, 0.5]})
	await frames(2)
	var paint := LimnerEditor.screen_color([0.8, 0.2, 0.05])
	check_eq((panel._bars[PAINT] as ColorChip).color.to_html(false), paint.to_html(false), "paint bar")
	check_eq((panel._bars[BACKGROUND] as ColorChip).color.to_html(false), LimnerEditor.screen_color([0.1, 0.1, 0.3]).to_html(false), "background bar")
	check_eq(panel._swatches[9].color.to_html(false), LimnerEditor.screen_color([0.0, 0.5, 0.5]).to_html(false), "swatch")
	check_eq(panel._hex.text, "#" + paint.to_html(false), "hex field")
	check_near(panel._strip.marker.y, paint.h, "marker on the hue strip")
	check_near(panel._square.marker.x, paint.s, "ring on the square, across")
	check_near(panel._square.marker.y, paint.v, "ring on the square, up")

	editor.undo()
	editor.undo()
	await frames(2)
	check_eq((panel._bars[BACKGROUND] as ColorChip).color.to_html(false), LimnerEditor.screen_color(NEW_BACKGROUND).to_html(false), "background bar after undo")
	check_eq(panel._swatches[9].color.to_html(false), LimnerEditor.screen_color(UNUSED).to_html(false), "swatch after undo")
	check_eq((panel._bars[PAINT] as ColorChip).color.to_html(false), paint.to_html(false), "the paint colour is not undone")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_it_fits_in_a_headset() -> void:
	Kit.set_headset(true)
	var panel := ColorPanel.new(editor)
	mount(panel)
	await frames(2)
	check_fits(panel, "in a headset")
	check_eq(int(panel.pixel_size().x), int(ColorPanel.DESIGN_SIZE.x), "full size")
	Kit.set_headset(false)
