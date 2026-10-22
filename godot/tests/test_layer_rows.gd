extends LimnerTest
## The layers in the Layers panel: the keys that add and arrange them and the page of
## rows, which keep in step with the document and send the right commands.


func _panel() -> TimelinePanel:
	var panel := TimelinePanel.new(editor)
	mount(panel, Vector2(1400, 600))
	panel.set_height(560.0)
	await frames(3)
	return panel


## The line of the page that shows the layer called `layer_name`.
func _line(panel: TimelinePanel, layer_name: String) -> Dictionary:
	var id := int(row_named(layer_name).id)
	for line: Dictionary in panel.layer_rows._lines:
		if int(line.get("id", -1)) == id:
			return line
	failures.append("no row shows %s" % layer_name)
	return panel.layer_rows._lines[0]


func _names(panel: TimelinePanel) -> Array:
	return panel.layer_rows.page_rows().map(func(row: Dictionary) -> String: return row.name)


func test_lists_the_layers_of_a_new_document() -> void:
	var panel := await _panel()
	check_eq(_names(panel), ["Root", "Spawn Area", "Paint"], "the rows")
	var paint := _line(panel, "Paint")
	check_eq((paint.name as Button).text, "Paint", "a row shows the layer's name")
	check((paint.name as Button).button_pressed, "the paint layer starts selected")
	var root := _line(panel, "Root")
	check(not (root.fold as Button).visible, "the root has no fold key")
	check((root.target as Button).disabled and (root.lock as Button).disabled, "nor can it be moved or locked")
	check_eq(panel.layer_rows.pager._place.text, "3", "three layers on one page")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_adding_and_removing_layers() -> void:
	var panel := await _panel()
	(panel.controls.add_light as Button).pressed.emit()
	await frames(2)
	var light := row_named("Light")
	check_eq(light.get("kind"), "light", "kind of the new layer")
	check_eq(editor.current_layer(), int(light.id), "the new layer is current")
	check_eq(panel.layer_rows.listed.size(), 4, "rows after adding")
	for entry: Array in TimelinePanel.NEW_LAYER_KINDS:
		check(panel.controls.has("add_" + entry[0]), "a key adds a %s" % entry[0])
	(panel.controls.delete as Button).pressed.emit()
	await frames(2)
	check_eq(panel.layer_rows.listed.size(), 3, "rows after deleting")
	editor.undo()
	await frames(2)
	check_eq(panel.layer_rows.listed.size(), 4, "rows after undo")
	(panel.controls.duplicate as Button).pressed.emit()
	await frames(2)
	check_eq(panel.layer_rows.listed.size(), 5, "rows after duplicating")
	var asked := [0]
	panel.import_requested.connect(func() -> void: asked[0] += 1)
	(panel.controls.import as Button).pressed.emit()
	check_eq(asked[0], 1, "the import key asks for the files to import")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_row_keys_send_commands() -> void:
	var panel := await _panel()
	var paint := row_named("Paint")
	var id := int(paint.id)
	(_line(panel, "Paint").visible_key as Button).pressed.emit()
	await frames(2)
	check_eq(editor.row(id).visible, false, "the eye hides the layer")
	(_line(panel, "Paint").lock as Button).pressed.emit()
	await frames(2)
	check_eq(editor.row(id).locked, true, "the lock locks it")
	(_line(panel, "Paint").target as Button).pressed.emit()
	await frames(2)
	check_eq(editor.row(id).transform_target, true, "the move key makes it what grabbing moves")
	(_line(panel, "Paint").bounds as Button).pressed.emit()
	await frames(2)
	check_eq(editor.row(id).show_bounds, true, "the bounds key shows its box")
	var asked: Array[int] = []
	panel.settings_requested.connect(func(layer: int) -> void: asked.append(layer))
	(_line(panel, "Spawn Area").kind as Button).pressed.emit()
	await frames(2)
	check_eq(asked, [int(row_named("Spawn Area").id)] as Array[int], "the kind key asks for that layer's settings")
	check_eq(editor.current_layer(), int(row_named("Spawn Area").id), "and makes it current")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_opacity_drag_is_one_undo_step() -> void:
	var panel := await _panel()
	var opacity: SliderRow = panel._opacity
	opacity.drag_started.emit()
	for value: float in [0.8, 0.5, 0.3]:
		opacity.changed.emit(value)
	opacity.drag_ended.emit()
	await frames(2)
	check_near(float(row_named("Paint").opacity), 0.3, "opacity after the drag")
	check_eq(editor.status.undo_label, "Change layer opacity", "one undo step")
	editor.undo()
	await frames(2)
	check_near(float(row_named("Paint").opacity), 1.0, "undone at once")


func test_the_modifiers_add_to_the_selection() -> void:
	var panel := await _panel()
	var group: Dictionary = run({"cmd": "add_layer", "kind": "group", "name": "Body"})
	await frames(2)
	var rows := panel.layer_rows
	run({"cmd": "select_layers", "layers": [int(row_named("Paint").id)]})
	await frames(2)
	# Ctrl (the other hand's trigger in a headset) adds a layer to the selection.
	rows._press_mode = "toggle"
	(_line(panel, "Spawn Area").name as Button).pressed.emit()
	await frames(2)
	var selected := editor.get_rows().filter(func(row: Dictionary) -> bool: return row.selected).map(func(row: Dictionary) -> String: return row.name)
	check_eq(selected, ["Spawn Area", "Paint"], "two layers selected")
	check_eq(editor.current_layer(), int(row_named("Spawn Area").id), "the one pressed is current")
	rows._press_mode = "toggle"
	(_line(panel, "Spawn Area").name as Button).pressed.emit()
	await frames(2)
	check_eq(editor.current_layer(), int(row_named("Paint").id), "pressed again, it leaves the selection")
	# Shift selects every row from the current one to the one pressed.
	rows._press_mode = "range"
	(_line(panel, "Body").name as Button).pressed.emit()
	await frames(2)
	selected = editor.get_rows().filter(func(row: Dictionary) -> bool: return row.selected).map(func(row: Dictionary) -> String: return row.name)
	check_eq(selected, ["Paint", "Body"], "the rows between them")
	check_eq(editor.current_layer(), int(group.layer), "and the one pressed is current")
	# The group key puts the selected layers in a new group.
	(panel.controls.group as Button).pressed.emit()
	await frames(2)
	check_eq(int(editor.row(int(row_named("Paint").id)).depth), 2, "the selected layers went into a new group")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_renaming_through_the_name() -> void:
	var panel := await _panel()
	(panel.controls.rename as Button).pressed.emit()
	await frames(1)
	var line := _line(panel, "Paint")
	var field: TextField = line.field
	check(field.visible and not (line.name as Button).visible, "the name becomes a box to type in")
	field.text = "Sky"
	field.committed.emit("Sky")
	await frames(2)
	check(not row_named("Sky").is_empty(), "the layer is renamed")
	line = _line(panel, "Sky")
	check((line.name as Button).visible and not (line.field as Control).visible, "and the name is a key again")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_arranging_keys() -> void:
	var panel := await _panel()
	var paint := int(row_named("Paint").id)
	(panel.controls.up as Button).pressed.emit()
	await frames(2)
	check_eq(_names(panel), ["Root", "Paint", "Spawn Area"], "up moves the current layer up")
	(panel.controls.down as Button).pressed.emit()
	await frames(2)
	check_eq(_names(panel), ["Root", "Spawn Area", "Paint"], "and down again")
	check((panel.controls.flatten as Button).disabled, "a paint layer cannot be flattened")
	check((panel.controls.up as Button).disabled == false, "but moved")
	run({"cmd": "select_layers", "layers": [int(row_named("Root").id)]})
	await frames(2)
	for key: String in ["duplicate", "delete", "up", "down", "group"]:
		check((panel.controls[key] as Button).disabled, "%s does nothing to the root" % key)
	run({"cmd": "select_layers", "layers": [paint]})
	await frames(2)
	check(not (panel.controls.merge as Button).disabled, "a paint layer can be merged down")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_context_menu_is_a_desktop_shortcut_for_the_row_clicked() -> void:
	var panel := await _panel()
	var rows := panel.layer_rows
	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_RIGHT
	press.pressed = true
	var spawn := _line(panel, "Spawn Area")
	var index := rows._lines.find(spawn)
	rows._on_name_input(press, index)
	await frames(1)
	check(rows._context.visible, "a right click opens the menu")
	check_eq(editor.current_layer(), int(row_named("Spawn Area").id), "for the row clicked")
	var entries: Array = []
	for at in rows._context.item_count:
		entries.append(rows._context.get_item_text(at))
	check(entries.has("Stand here") and entries.has("Initial spawn area"), "with what a spawn area offers: %s" % [entries])
	rows._context.hide()
	Kit.set_headset(true)
	rows._on_name_input(press, index)
	await frames(1)
	check(not rows._context.visible, "never in a headset, where every entry is a key somewhere")
	Kit.set_headset(false)


func test_many_layers_show_a_page_at_a_time() -> void:
	var panel := await _panel()
	var rows := panel.layer_rows
	rows.set_per_page(4)
	var last := -1
	for number in 9:
		var added: Dictionary = run({"cmd": "add_layer", "kind": "paint", "name": "Extra %d" % number})
		last = int(added.layer)
	await frames(2)
	check_eq(rows.listed.size(), 12, "twelve layers")
	check_eq(rows.pager.pages(), 3, "on three pages of four")
	check_eq(int(rows.page_rows().back().id), last, "the page with the newest, current layer is shown")
	rows.pager.step(-1)
	rows.pager.step(-1)
	await frames(1)
	check_eq(_names(panel), ["Root", "Spawn Area", "Paint", "Extra 0"], "the first page")
	check_eq(panel.rows.size(), 4, "and the tracks draw the same four")
	run({"cmd": "select_layers", "layers": [last]})
	await frames(2)
	check(rows.page_rows().any(func(row: Dictionary) -> bool: return int(row.id) == last), "choosing a layer elsewhere turns to its page")
	check(messages.is_empty(), "no errors: %s" % [messages])
