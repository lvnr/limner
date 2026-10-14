extends LimnerTest
## The Layer panel shows the current layer's name and the settings of its kind, and
## each control writes the setting it stands for.


func _panel() -> PropertiesPanel:
	var panel := PropertiesPanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


## Adds a layer of `kind` under the root, which makes it the current layer.
func _add(kind: String) -> int:
	var result: Dictionary = run({"cmd": "add_layer", "kind": kind, "parent": int(editor.get_info().root)})
	await frames(2)
	return int(result.layer)


func _props(layer: int) -> Dictionary:
	return world.scene.layer_details(layer).props


## Types `text` into a box and presses Enter.
func _type(field: LineEdit, text: String) -> void:
	field.text = text
	field.text_submitted.emit(text)
	await frames(2)


func _choose(menu: OptionButton, text: String) -> void:
	for index in menu.item_count:
		if menu.get_item_text(index) == text:
			menu.select(index)
			menu.item_selected.emit(index)
			await frames(2)
			return
	failures.append("there is no choice %s" % text)


## Drags a slider through `values`, the way a hand would.
func _drag(slider: SliderField, values: Array) -> void:
	slider.drag_started.emit()
	for value: float in values:
		slider.changed.emit(value)
	slider.drag_ended.emit()
	await frames(2)


## Picks `colors` one after another in a swatch's picker, then closes it.
func _pick(swatch: ColorField, colors: Array) -> void:
	for color: Color in colors:
		swatch.color_changed.emit(color)
	swatch.popup_closed.emit()
	await frames(2)


## Opens the first sample project that has a layer of `kind` and makes that layer
## current. Returns its id, or -1 on a machine without such a sample: the samples
## are downloads kept outside the repository, and these layers cannot be made by
## command yet.
func _open_sample_with(kind: String) -> int:
	var samples := ProjectSettings.globalize_path("res://").path_join("../local/samples")
	for sample: String in DirAccess.get_directories_at(samples):
		var opened: Dictionary = world.scene.open(samples.path_join(sample))
		if not opened.ok:
			continue
		for row: Dictionary in world.scene.layer_rows():
			if row.kind == kind:
				run({"cmd": "select_layers", "layers": [int(row.id)]})
				await frames(2)
				return int(row.id)
	print("     skipped: no sample project with a %s layer" % kind)
	return -1


func _shown_sections(panel: PropertiesPanel) -> Array:
	return panel._sections.keys().filter(func(kind: String) -> bool: return panel._sections[kind].visible)


func test_header_names_the_layer_and_renames_it() -> void:
	var panel := await _panel()
	check(panel._body.visible and not panel._empty.visible, "a layer is shown")
	check_eq(panel._name.text, "Paint", "name")
	check_eq(panel._kind.text, "Paint layer", "kind")
	check_eq(panel._path.text, "Root/Paint", "path")

	await _type(panel._name, "Sky")
	check_eq(row_named("Sky").get("kind"), "paint", "the layer is renamed")
	check_eq(panel._path.text, "Root/Sky", "the path follows")
	check_eq(editor.status.undo_label, "Rename layer", "what undo calls it")

	# A sibling cannot take the same name; the box shows what the document chose.
	var other := await _add("paint")
	await _type(panel._name, "Sky")
	var given: String = editor.row(other).name
	check(given != "Sky" and given.begins_with("Sky"), "the document chose another name: %s" % given)
	check_eq(panel._name.text, given, "the box shows the name the document chose")

	var revision: int = editor.status.doc_revision
	await _type(panel._name, given)
	panel._name.focus_exited.emit()
	await frames(2)
	check_eq(editor.status.doc_revision, revision, "an unchanged name changes nothing")

	panel._name.text = "Clouds"
	panel._name.focus_exited.emit()
	await frames(2)
	check_eq(editor.row(other).name, "Clouds", "leaving the box renames too")


func test_says_so_when_there_is_no_layer_and_follows_the_current_one() -> void:
	var panel := await _panel()
	check_eq(_shown_sections(panel), ["paint"], "sections for a paint layer")
	run({"cmd": "select_layers", "layers": []})
	await frames(2)
	check(panel._empty.visible and not panel._body.visible, "the panel says no layer is selected")
	check_eq(panel._empty.text, "No layer is selected.", "what it says")

	for kind: String in ["group", "sequence", "light", "camera", "viewpoint", "paint"]:
		var layer := await _add(kind)
		check(panel._body.visible, "%s is shown" % kind)
		check_eq(_shown_sections(panel), [kind], "sections for a %s layer" % kind)
		check_eq(panel._kind.text, PropertiesPanel.KIND_NAMES[kind], "kind shown for %s" % kind)
		check_eq(panel._name.text, editor.row(layer).name, "name shown for %s" % kind)
	run({"cmd": "select_layers", "layers": [int(row_named("Light").id)]})
	await frames(2)
	check_eq(_shown_sections(panel), ["light"], "sections after choosing the light again")
	for kind: String in PropertiesPanel.KIND_NAMES:
		check(panel._sections.has(kind), "there is a section for %s layers" % kind)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_paint_settings() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	var shading: OptionButton = panel._controls["paint.shading"]
	check_eq(shading.get_item_text(shading.selected), "Unlit", "shading shown")
	check_eq(shading.tooltip_text, "Lit layers are shaded by the scene's lights", "shading tooltip")
	await _choose(shading, "Lit")
	check_eq(_props(paint).shading, "lit", "shading")
	var revision: int = editor.status.doc_revision
	await _choose(shading, "Lit")
	check_eq(editor.status.doc_revision, revision, "choosing the same shading changes nothing")

	var loop: CheckBox = panel._controls["paint.loop"]
	check_eq(loop.button_pressed, false, "a new paint layer plays once")
	loop.button_pressed = true
	await frames(2)
	check_eq(int(_props(paint).max_repeat), 0, "looping is max_repeat 0")
	loop.button_pressed = false
	await frames(2)
	check_eq(int(_props(paint).max_repeat), 1, "playing once is max_repeat 1")
	editor.undo()
	await frames(2)
	check_eq(loop.button_pressed, true, "the switch follows undo")
	check_eq(panel._controls["paint.counts"].text, "1 frame, 1 drawing, 0 strokes in the frame shown", "counts")


func test_sequence_and_group_settings() -> void:
	var panel := await _panel()
	var sequence := await _add("sequence")
	var frame := int(editor.get_info().frame_duration)
	var loop: CheckBox = panel._controls["sequence.loop"]
	var seconds: NumberField = panel._controls["sequence.loop_seconds"]
	var frames_box: NumberField = panel._controls["sequence.loop_frames"]
	var repeats: NumberField = panel._controls["sequence.max_repeat"]
	check_eq(loop.button_pressed, false, "a new sequence does not loop")

	loop.button_pressed = true
	await frames(2)
	check_eq(int(_props(sequence).loop_duration), LimnerEditor.FLICKS_PER_SECOND, "looping at the start uses one second")
	check_eq([seconds.text, frames_box.text], ["1", "24"], "loop length shown")
	loop.button_pressed = false
	await frames(2)
	check_eq(int(_props(sequence).loop_duration), 0, "no loop is length 0")

	run({"cmd": "seek", "time": LimnerEditor.to_flicks(1.51)})
	await frames(2)
	loop.button_pressed = true
	await frames(2)
	check_eq(int(_props(sequence).loop_duration), 36 * frame, "looping uses the playhead, on a frame")

	await _type(seconds, "2")
	check_eq(int(_props(sequence).loop_duration), 2 * LimnerEditor.FLICKS_PER_SECOND, "length typed in seconds")
	check_eq(frames_box.text, "48", "frames follow seconds")
	await _type(frames_box, "12")
	check_eq(int(_props(sequence).loop_duration), 12 * frame, "length typed in frames")
	check_eq(seconds.text, "0.5", "seconds follow frames")
	await _type(seconds, "0")
	check_eq(loop.button_pressed, false, "a length of 0 switches the loop off")

	await _type(repeats, "3")
	check_eq(int(_props(sequence).max_repeat), 3, "repeats typed")
	repeats.nudge(1.0)
	await frames(2)
	check_eq(int(_props(sequence).max_repeat), 4, "one more repeat")

	var to_group: Button = panel._controls["sequence.convert"]
	check(to_group.visible, "a sequence can become a group")
	to_group.pressed.emit()
	await frames(2)
	check_eq(editor.row(sequence).kind, "group", "converted to a group")
	check(panel._sections["group"].visible and not panel._sections["sequence"].visible, "the group's section is shown")
	panel._controls["group.convert"].pressed.emit()
	await frames(2)
	check_eq(editor.row(sequence).kind, "sequence", "converted back to a sequence")

	run({"cmd": "select_layers", "layers": [int(editor.get_info().root)]})
	await frames(2)
	check(panel._sections["sequence"].visible, "the root is a sequence")
	check(not to_group.visible, "the root cannot become a group")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_light_settings() -> void:
	var panel := await _panel()
	var light := await _add("light")
	var type: OptionButton = panel._controls["light.kind"]
	var range_row: SliderField = panel._controls["light.range"]
	var angle: SliderField = panel._controls["light.spot_angle"]
	var softness: SliderField = panel._controls["light.spot_blend"]
	check(range_row.visible and not angle.visible and not softness.visible, "a point light has a range and no cone")
	await _choose(type, "Spot")
	check_eq(_props(light).kind, "spot", "type")
	check(range_row.visible and angle.visible and softness.visible, "a spot has a range and a cone")
	await _choose(type, "Directional")
	check_eq(_props(light).kind, "directional", "type")
	check(not range_row.visible and not angle.visible and not softness.visible, "a directional light has neither")
	await _choose(type, "Spot")

	var energy: SliderField = panel._controls["light.energy"]
	var steps_before: int = editor.status.doc_revision
	await _drag(energy, [2.0, 3.0, 4.0])
	check_near(_props(light).energy, 4.0, "energy after the drag")
	check_eq(energy.field.text, "4", "energy shown")
	editor.undo()
	await frames(2)
	check_near(_props(light).energy, 1.0, "one undo takes back the whole drag")
	check(editor.status.doc_revision != steps_before, "the drag did change the document")
	await _type(energy.field, "250")
	check_near(_props(light).energy, 250.0, "energy typed beyond the slider")
	check_eq(energy.field.text, "250", "typed energy stays shown")
	await _type(energy.field, "5000")
	check_near(_props(light).energy, 1000.0, "energy stops at 1000")

	await _drag(range_row, [12.5])
	check_near(_props(light).range, 12.5, "range")
	await _drag(angle, [40.0])
	check_near(_props(light).spot_angle, 40.0, "cone angle")
	await _drag(softness, [50.0])
	check_near(_props(light).spot_blend, 0.5, "cone softness is a percentage of 1")
	check_eq(softness.field.text, "50", "softness shown in percent")

	var shadows: CheckBox = panel._controls["light.shadows"]
	shadows.button_pressed = true
	await frames(2)
	check_eq(_props(light).shadows, true, "shadows")

	var swatch: ColorField = panel._controls["light.color"]
	check(not swatch.edit_alpha, "a light's colour has no transparency")
	var undo_before: String = editor.status.undo_label
	await _pick(swatch, [Color(1.0, 0.0, 0.0), Color(0.2, 0.5, 1.0), Color(0.0, 1.0, 0.0)])
	var color: Array = _props(light).color
	check_near(color[0], 0.0, "red")
	check_near(color[1], 1.0, "green")
	check_near(color[2], 0.0, "blue")
	editor.undo()
	await frames(2)
	check_eq(_props(light).color, [1.0, 1.0, 1.0], "one undo takes back everything picked")
	check_eq(editor.status.undo_label, undo_before, "and it was a single step")
	await _pick(swatch, [Color(0.5, 0.5, 0.5)])
	check_near(_props(light).color[0], 0.2140, "colours are stored linear", 1e-3)
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_camera_settings() -> void:
	var panel := await _panel()
	var camera := await _add("camera")
	var fov: SliderField = panel._controls["camera.fov"]
	var focal: SliderField = panel._controls["camera.focal"]
	check_eq(fov.field.text, "45", "field of view shown")
	check_near(focal.get_value(), 12.0 / tan(deg_to_rad(22.5)), "focal length shown for 45°", 1e-3)

	await _drag(fov, [50.0, 60.0])
	check_near(_props(camera).fov, 60.0, "field of view")
	check_near(focal.get_value(), 12.0 / tan(deg_to_rad(30.0)), "focal length follows the field of view", 1e-3)
	editor.undo()
	await frames(2)
	check_near(_props(camera).fov, 45.0, "one undo takes back the drag")

	await _type(focal.field, "50")
	check_near(_props(camera).fov, rad_to_deg(2.0 * atan(12.0 / 50.0)), "field of view of a 50 mm lens", 1e-4)
	check_near(fov.get_value(), 26.9915, "field of view follows the focal length", 1e-3)
	check_eq(focal.field.text, "50", "the focal length typed stays shown")
	await _drag(focal, [20.0, 24.0])
	check_near(_props(camera).fov, 53.1301, "field of view of a 24 mm lens", 1e-3)
	editor.undo()
	await frames(2)
	check_near(focal.get_value(), 50.0, "one undo takes back the focal drag", 1e-3)

	check(panel._controls["camera.resolution"].text.contains("1920 × 1080"), "the render resolution is shown")
	run({"cmd": "set_document", "camera_resolution": [1024, 1024]})
	await frames(2)
	check(panel._controls["camera.resolution"].text.contains("1024 × 1024"), "and follows the document")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_spawn_area_settings() -> void:
	var panel := await _panel()
	var first := int(row_named("Spawn Area").id)
	var area := await _add("viewpoint")
	var initial: CheckBox = panel._controls["viewpoint.initial"]
	check_eq(initial.button_pressed, false, "the new spawn area is not the initial one")

	var height: OptionButton = panel._controls["viewpoint.floor_level"]
	check_eq(height.get_item_text(height.selected), "Eye level", "height shown")
	check(height.tooltip_text.contains("Eye level") and height.tooltip_text.contains("Floor level"), "the tooltip explains both")
	await _choose(height, "Floor level")
	check_eq(_props(area).floor_level, true, "floor level")
	await _choose(height, "Eye level")
	check_eq(_props(area).floor_level, false, "eye level")

	initial.button_pressed = true
	await frames(2)
	check_eq(int(editor.get_info().default_viewpoint), area, "initial spawn area")
	check_eq(editor.row(first).default_viewpoint, false, "the first one no longer is")
	run({"cmd": "select_layers", "layers": [first]})
	await frames(2)
	check_eq(initial.button_pressed, false, "the switch follows the layer shown")
	run({"cmd": "select_layers", "layers": [area]})
	await frames(2)
	check_eq(initial.button_pressed, true, "and is on for the initial spawn area")
	initial.button_pressed = false
	await frames(2)
	check_eq(editor.get_info().default_viewpoint, null, "switched off, no spawn area is the initial one")

	# "Move it here" keeps the view you have now; "Stand here" gives it back.
	world.scene.view_pan(Vector3(0.4, 0.0, -0.3))
	var kept := world.scene.viewer_from_world()
	var move_here: Button = null
	for button: Button in panel.find_children("*", "Button", true, false):
		if button.text == "Move it here":
			move_here = button
	check(move_here != null, "a Move it here button")
	if move_here != null:
		move_here.pressed.emit()
		await frames(2)
		check_eq(editor.status.undo_label, "Move spawn area here", "one undo step")
		world.scene.view_pan(Vector3(-2.0, 0.0, 1.0))
		editor.scene.view_go_to_viewpoint(area)
		check(world.scene.viewer_from_world().origin.is_equal_approx(kept.origin), "standing in it gives the view back: %s against %s" % [world.scene.viewer_from_world().origin, kept.origin])

	var exporting: CheckBox = panel._controls["viewpoint.exporting"]
	exporting.button_pressed = false
	await frames(2)
	check_eq(_props(area).exporting, false, "include when publishing")
	var volume: CheckBox = panel._controls["viewpoint.show_volume"]
	volume.button_pressed = true
	await frames(2)
	check_eq(_props(area).show_volume, true, "show volume")
	await _pick(panel._controls["viewpoint.color"], [Color(1.0, 1.0, 0.0)])
	check_eq(_props(area).color, [1.0, 1.0, 0.0], "colour")

	var revision: int = editor.status.doc_revision
	for button: Button in panel.find_children("*", "Button", true, false):
		if button.text == "Stand here":
			button.pressed.emit()
	await frames(2)
	check_eq(editor.status.doc_revision, revision, "standing there does not change the document")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_a_locked_layer_cannot_be_changed() -> void:
	var panel := await _panel()
	var light := await _add("light")
	run({"cmd": "set_layer_kind", "layer": light, "values": {"kind": "spot"}})
	run({"cmd": "set_layer", "layer": light, "locked": true})
	await frames(2)
	check_eq(panel._kind.text, "Light, locked", "the header says it is locked")
	check(panel._controls["light.kind"].disabled, "type is off")
	check(panel._controls["light.shadows"].disabled, "shadows are off")
	for key: String in ["light.energy", "light.range", "light.spot_angle", "light.spot_blend"]:
		var slider: SliderField = panel._controls[key]
		check(not slider.slider.editable and not slider.field.editable, "%s is off" % key)
	check_eq(panel._controls["light.color"].mouse_filter, Control.MOUSE_FILTER_IGNORE, "the colour takes no clicks")
	check(panel._name.editable, "a locked layer can still be renamed")

	run({"cmd": "set_layer", "layer": light, "locked": false})
	await frames(2)
	check(not panel._controls["light.kind"].disabled, "type is back")
	check(panel._controls["light.energy"].slider.editable, "energy is back")
	check_eq(panel._controls["light.color"].mouse_filter, Control.MOUSE_FILTER_STOP, "the colour takes clicks again")

	var paint := int(row_named("Paint").id)
	run({"cmd": "select_layers", "layers": [paint]})
	run({"cmd": "set_layer", "layer": paint, "locked": true})
	await frames(2)
	check(panel._controls["paint.shading"].disabled and panel._controls["paint.loop"].disabled, "a locked paint layer's settings are off")
	var sequence := await _add("sequence")
	run({"cmd": "set_layer", "layer": sequence, "locked": true})
	await frames(2)
	check(not panel._controls["sequence.loop_seconds"].editable and not panel._controls["sequence.loop_frames"].editable, "a locked sequence's loop length is off")
	check(not panel._controls["sequence.max_repeat"].editable and panel._controls["sequence.loop"].disabled, "and so are its repeats and its loop switch")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_picture_settings() -> void:
	var panel := await _panel()
	var picture := await _open_sample_with("picture")
	if picture < 0:
		return
	check_eq(_shown_sections(panel), ["picture"], "sections for a picture")
	var type: OptionButton = panel._controls["picture.projection"]
	var stored := {"360": "equirect", "360 stereo": "equirect_stereo", "Cubemap cross": "cubemap_cross", "Cubemap strip": "cubemap_strip", "2D image": "flat"}
	check_eq(type.item_count, stored.size(), "the kinds of picture")
	for text: String in stored:
		await _choose(type, text)
		check_eq(_props(picture).projection, stored[text], "type %s" % text)
	var locked: CheckBox = panel._controls["picture.viewer_locked"]
	locked.button_pressed = not locked.button_pressed
	await frames(2)
	check_eq(_props(picture).viewer_locked, locked.button_pressed, "lock to viewer")
	editor.undo()
	await frames(2)
	check_eq(locked.button_pressed, _props(picture).viewer_locked, "the switch follows undo")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_model_settings() -> void:
	var panel := await _panel()
	var model := await _open_sample_with("model")
	if model < 0:
		return
	check_eq(_shown_sections(panel), ["model"], "sections for a model")
	var lighting: OptionButton = panel._controls["model.shading"]
	await _choose(lighting, "Smooth")
	check_eq(_props(model).shading, "lit", "smooth lighting")
	await _choose(lighting, "Unlit")
	check_eq(_props(model).shading, "unlit", "no lighting")
	var wireframe: CheckBox = panel._controls["model.wireframe"]
	wireframe.button_pressed = not wireframe.button_pressed
	await frames(2)
	check_eq(_props(model).wireframe, wireframe.button_pressed, "wireframe")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_sound_shapes_are_told_apart() -> void:
	check_eq(PropertiesPanel._sound_shape("none"), "none", "no shape")
	check_eq(PropertiesPanel._sound_shape({"cone": {"inner": 0.5, "band": 0.1, "outside": 0.0}}), "cone", "a cone")
	check_eq(PropertiesPanel._sound_shape({"frustum": {"inner_h": 0.6, "inner_v": 0.4, "band": 0.1, "outside": 0.0}}), "frustum", "a frustum")
	check_eq(PropertiesPanel._dig({"attenuation": {"min": 0.25}}, "attenuation.min"), 0.25, "a setting inside another")
	check_eq(PropertiesPanel._dig({"modifier": "none"}, "modifier.cone.inner"), null, "a setting that is not there")
