extends LimnerTest
## The layer list keeps in step with the document and sends the right commands.


func _panel() -> LayersPanel:
	var panel := LayersPanel.new(editor)
	mount(panel)
	await frames(2)
	return panel


func test_lists_the_layers_of_a_new_document() -> void:
	var panel := await _panel()
	var root := panel._tree.get_root()
	check_eq(root.get_text(0), "Root", "the root row")
	check_eq(root.get_child_count(), 2, "rows under the root")
	check_eq(root.get_child(1).get_text(0), "Paint", "the paint layer")
	check(root.get_child(1).is_selected(0), "the paint layer starts selected")


func test_adding_and_removing_layers() -> void:
	var panel := await _panel()
	panel._on_add(5)
	await frames(2)
	var light := row_named("Light")
	check_eq(light.get("kind"), "light", "kind of the new layer")
	check_eq(editor.current_layer(), int(light.id), "the new layer is current")
	check_eq(panel._tree.get_root().get_child_count(), 3, "rows after adding")
	panel._run_on_selected("delete_layers")
	await frames(2)
	check_eq(panel._tree.get_root().get_child_count(), 2, "rows after deleting")
	editor.undo()
	await frames(2)
	check_eq(panel._tree.get_root().get_child_count(), 3, "rows after undo")


func test_row_switches_send_commands() -> void:
	var panel := await _panel()
	var paint := row_named("Paint")
	var item: TreeItem = panel._items[int(paint.id)]
	panel._on_row_button(item, 0, LayersPanel.RowButton.EYE, MOUSE_BUTTON_LEFT)
	panel._on_row_button(item, 0, LayersPanel.RowButton.LOCK, MOUSE_BUTTON_LEFT)
	panel._on_row_button(item, 0, LayersPanel.RowButton.TARGET, MOUSE_BUTTON_LEFT)
	await frames(2)
	paint = row_named("Paint")
	check_eq(paint.visible, false, "hidden by the eye")
	check_eq(paint.locked, true, "locked by the padlock")
	check_eq(paint.transform_target, true, "chosen as what grabbing moves")
	check(not panel._opacity.slider.editable, "a locked layer's opacity cannot be dragged")


func test_opacity_drag_is_one_undo_step() -> void:
	var panel := await _panel()
	panel._opacity.drag_started.emit()
	for value: float in [0.8, 0.5, 0.25]:
		panel._opacity.changed.emit(value)
	panel._opacity.drag_ended.emit()
	await frames(2)
	check_near(row_named("Paint").opacity, 0.25, "opacity after the drag")
	editor.undo()
	await frames(2)
	check_near(row_named("Paint").opacity, 1.0, "opacity after one undo")
	check_eq(editor.status.can_undo, false, "nothing else to undo")


func test_dropping_a_layer_on_a_group_moves_it_inside() -> void:
	var panel := await _panel()
	var group: Dictionary = run({"cmd": "add_layer", "kind": "group", "name": "Body"})
	run({"cmd": "select_layers", "layers": [int(row_named("Paint").id)]})
	await frames(2)
	var target: TreeItem = panel._items[int(group.layer)]
	var rect := panel._tree.get_item_area_rect(target)
	panel._drop(rect.get_center(), {"limner_layers": [int(row_named("Paint").id)]})
	await frames(2)
	check_eq(row_named("Paint").parent, int(group.layer), "parent after the drop")
	check(messages.is_empty(), "no errors: %s" % [messages])


## The names of the root's layers, in list order.
func _order() -> Array:
	var root := int(editor.get_info().root)
	var names := []
	for row: Dictionary in world.scene.layer_rows():
		if row.parent != null and int(row.parent) == root:
			names.append(row.name)
	return names


func test_dropping_two_layers_above_and_below_a_third_keeps_them_together() -> void:
	var panel := await _panel()
	var root := int(editor.get_info().root)
	for layer_name: String in ["A", "B", "C"]:
		run({"cmd": "add_layer", "kind": "group", "name": layer_name, "parent": root, "index": 0})
	await frames(2)
	# Added at the top one after another, so the list reads C, B, A, then the rest.
	var a := int(row_named("A").id)
	var b := int(row_named("B").id)
	var c := int(row_named("C").id)
	var rest := _order().slice(3)
	check_eq(_order().slice(0, 3), ["C", "B", "A"], "the order to start with")

	# B and C dropped on the lower edge of A: below it, in their own order.
	var rect := panel._tree.get_item_area_rect(panel._items[a])
	var below := Vector2(rect.get_center().x, rect.end.y - 2.0)
	check_eq(panel._tree.get_drop_section_at_position(below), 1, "the lower edge of a row is below it")
	panel._drop(below, {"limner_layers": [c, b]})
	await frames(2)
	check_eq(_order(), ["A", "C", "B"] + rest, "dropped below A")
	check_eq(editor.status.undo_label, "Move layers", "as one step")

	# And back above A, on its upper edge.
	rect = panel._tree.get_item_area_rect(panel._items[a])
	var above := Vector2(rect.get_center().x, rect.position.y + 2.0)
	check_eq(panel._tree.get_drop_section_at_position(above), -1, "the upper edge of a row is above it")
	panel._drop(above, {"limner_layers": [c, b]})
	await frames(2)
	check_eq(_order(), ["C", "B", "A"] + rest, "dropped above A")
	editor.undo()
	await frames(2)
	check_eq(_order(), ["A", "C", "B"] + rest, "one undo takes back one drop")
	check(messages.is_empty(), "no errors: %s" % [messages])


func test_the_context_menu_is_for_the_row_clicked() -> void:
	var panel := await _panel()
	var light: Dictionary = run({"cmd": "add_layer", "kind": "light", "name": "Lamp"})
	var paint := int(row_named("Paint").id)
	run({"cmd": "select_layers", "layers": [paint]})
	await frames(2)
	check_eq(editor.current_layer(), paint, "the paint layer is current")

	# A right click on the light: the click selects it, and in the same frame the
	# menu is built.
	var lamp: TreeItem = panel._items[int(light.layer)]
	panel._updating = true
	panel._items[paint].deselect(0)
	lamp.select(0)
	panel._updating = false
	panel._on_item_mouse_selected(panel._tree.get_item_area_rect(lamp).get_center(), MOUSE_BUTTON_RIGHT)
	check_eq(editor.current_layer(), int(light.layer), "the light is current at once")
	check_eq(panel._context.get_item_index(LayersPanel.Menu.MERGE), -1, "a light's menu has no Merge down, which a paint layer's has")
	check(panel._context.get_item_index(LayersPanel.Menu.FLIP_X) >= 0, "and has what a light has")
	panel._context.hide()

	# And back to the paint layer.
	panel._updating = true
	lamp.deselect(0)
	panel._items[paint].select(0)
	panel._updating = false
	panel._on_item_mouse_selected(panel._tree.get_item_area_rect(panel._items[paint]).get_center(), MOUSE_BUTTON_RIGHT)
	check(panel._context.get_item_index(LayersPanel.Menu.MERGE) >= 0, "the paint layer's menu has Merge down")
	panel._context.hide()
	check(messages.is_empty(), "no errors: %s" % [messages])
