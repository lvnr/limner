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
