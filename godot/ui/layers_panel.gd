class_name LayersPanel
extends VBoxContainer
## The layer list: the tree of layers with their switches, and the commands that
## create, remove and rearrange them.

const KIND_ICONS := {
	"group": "group",
	"sequence": "sequence",
	"paint": "paint_layer",
	"picture": "picture",
	"model": "model",
	"sound": "sound",
	"viewpoint": "viewpoint",
	"camera": "camera",
	"light": "light",
}
const NEW_LAYER_KINDS := [
	["Paint layer", "paint", "paint_layer"],
	["Group", "group", "group"],
	["Sequence", "sequence", "sequence"],
	["Spawn area", "viewpoint", "viewpoint"],
	["Camera", "camera", "camera"],
	["Light", "light", "light"],
]
enum RowButton { TARGET, EYE, LOCK }
enum Menu { RENAME, DUPLICATE, DELETE, GROUP, MERGE, FLATTEN, TO_SEQUENCE, TO_GROUP, FLIP_X, FLIP_Y, FLIP_Z, RESET_TRANSFORM, RESET_PIVOT, PIVOT_ORIGIN, BOUNDS, INITIAL_SPAWN, GO_TO }

var editor: LimnerEditor

var _tree: Tree
var _items := {}
var _structure := []
var _opacity: SliderRow
var _add_menu: PopupMenu
var _context: PopupMenu
var _updating := false
var _selection_queued := false
var _buttons := {}


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Layers"
	size_flags_vertical = Control.SIZE_EXPAND_FILL

	var add := Kit.icon_button("plus", "New layer", Callable())
	_add_menu = PopupMenu.new()
	for i in NEW_LAYER_KINDS.size():
		_add_menu.add_icon_item(Icons.texture(NEW_LAYER_KINDS[i][2], Kit.TEXT, 18), NEW_LAYER_KINDS[i][0], i)
	_add_menu.id_pressed.connect(_on_add)
	add.add_child(_add_menu)
	add.pressed.connect(func() -> void: _popup_below(_add_menu, add))

	_buttons = {
		"duplicate": Kit.icon_button("duplicate", "Duplicate layer", func() -> void: _run_on_selected("duplicate_layers")),
		"delete": Kit.icon_button("trash", "Delete layer", func() -> void: _run_on_selected("delete_layers")),
		"up": Kit.icon_button("up", "Move up (into and out of groups)", func() -> void: _step(true)),
		"down": Kit.icon_button("down", "Move down (into and out of groups)", func() -> void: _step(false)),
		"merge": Kit.icon_button("merge", "Merge with the layer below", func() -> void: _run_on_current("merge_down")),
		"flatten": Kit.icon_button("flatten", "Flatten group into one paint layer", func() -> void: _run_on_current("flatten_group")),
		"group": Kit.icon_button("folder_plus", "Put the selected layers in a new group", func() -> void: _run_on_selected("group_layers")),
	}
	var bar := HFlowContainer.new()
	bar.add_child(add)
	for key: String in _buttons:
		bar.add_child(_buttons[key])
	add_child(bar)

	_tree = Tree.new()
	_tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tree.custom_minimum_size = Vector2(0, 140)
	_tree.select_mode = Tree.SELECT_MULTI
	_tree.allow_rmb_select = true
	_tree.hide_root = false
	_tree.focus_mode = Control.FOCUS_CLICK
	_tree.drop_mode_flags = Tree.DROP_MODE_ON_ITEM | Tree.DROP_MODE_INBETWEEN
	_tree.set_drag_forwarding(_get_drag, _can_drop, _drop)
	_tree.multi_selected.connect(_on_multi_selected)
	_tree.button_clicked.connect(_on_row_button)
	_tree.item_collapsed.connect(_on_collapsed)
	_tree.item_edited.connect(_on_renamed)
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	add_child(_tree)

	_opacity = SliderRow.new("Opacity", 0.0, 1.0, 0.01, "Opacity of the current layer (volume, for a sound)")
	_opacity.drag_started.connect(func() -> void: editor.begin("Change layer opacity"))
	_opacity.changed.connect(func(value: float) -> void: editor.run({"cmd": "set_layer", "layer": editor.current_layer(), "opacity": value}))
	_opacity.drag_ended.connect(editor.commit)
	add_child(_opacity)

	_context = PopupMenu.new()
	_context.id_pressed.connect(_on_context)
	add_child(_context)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


## The colour scheme changed: the rows take their text colours and icons again.
func apply_scheme() -> void:
	refresh()


## Brings the list in line with the document.
func refresh() -> void:
	var rows := editor.get_rows()
	var structure := rows.map(func(row: Dictionary) -> Array: return [row.id, row.parent])
	_updating = true
	if structure != _structure:
		_rebuild(rows)
		_structure = structure
	for row: Dictionary in rows:
		_update_item(_items[int(row.id)], row)
	_updating = false

	var current := editor.row(editor.current_layer())
	var has_current := not current.is_empty()
	var is_root := has_current and current.parent == null
	_opacity.set_editable(has_current and not current.locked)
	if has_current:
		_opacity.set_value(current.opacity)
	_buttons.duplicate.disabled = not has_current or is_root
	_buttons.delete.disabled = not has_current or is_root
	_buttons.up.disabled = not has_current or is_root
	_buttons.down.disabled = not has_current or is_root
	_buttons.merge.disabled = not has_current or current.kind != "paint"
	_buttons.flatten.disabled = not has_current or is_root or current.kind not in ["group", "sequence"]
	_buttons.group.disabled = not has_current or is_root


func _rebuild(rows: Array) -> void:
	_tree.clear()
	_items.clear()
	for row: Dictionary in rows:
		var parent: TreeItem = null if row.parent == null else _items.get(int(row.parent))
		var item := _tree.create_item(parent)
		item.set_metadata(0, int(row.id))
		item.set_editable(0, false)
		item.add_button(0, Icons.texture("move", Kit.TEXT, 16), RowButton.TARGET, row.parent == null, "Move this layer when grabbing")
		item.add_button(0, Icons.texture("eye", Kit.TEXT, 16), RowButton.EYE, false, "Show or hide")
		item.add_button(0, Icons.texture("unlock", Kit.TEXT, 16), RowButton.LOCK, false, "Lock or unlock")
		_items[int(row.id)] = item


func _update_item(item: TreeItem, row: Dictionary) -> void:
	if item.get_text(0) != row.name:
		item.set_text(0, row.name)
	var tint := Kit.ACCENT_TEXT if row.paint_target else (Kit.TEXT if row.shown else Kit.TEXT_DIM)
	item.set_icon(0, Icons.texture(KIND_ICONS.get(row.kind, "dot"), tint, 18))
	item.set_custom_color(0, Kit.TEXT if row.shown else Kit.TEXT_DIM)
	item.set_tooltip_text(0, _describe(row))
	item.set_button(0, RowButton.TARGET, Icons.texture("move", Kit.ACCENT_TEXT if row.transform_target else Kit.TEXT_DISABLED, 16))
	item.set_button(0, RowButton.EYE, Icons.texture("eye" if row.visible else "eye_off", Kit.TEXT if row.visible else Kit.TEXT_DIM, 16))
	item.set_button(0, RowButton.LOCK, Icons.texture("lock" if row.locked else "unlock", Kit.DANGER_TEXT if row.locked else Kit.TEXT_DISABLED, 16))
	if item.collapsed != row.collapsed and row.children > 0:
		item.collapsed = row.collapsed
	if row.selected != item.is_selected(0):
		if row.selected:
			item.select(0)
		else:
			item.deselect(0)


func _describe(row: Dictionary) -> String:
	var text: String = row.kind.capitalize()
	if row.kind == "paint":
		text += ", %d strokes" % row.strokes
		if row.frames > 1:
			text += ", %d frames" % row.frames
	if row.animated:
		text += ", animated"
	if row.default_viewpoint:
		text += ", initial spawn area"
	return text


func _selected_ids() -> Array:
	var ids := []
	var item := _tree.get_next_selected(null)
	while item != null:
		ids.append(item.get_metadata(0))
		item = _tree.get_next_selected(item)
	return ids


func _popup_below(menu: PopupMenu, anchor: Control) -> void:
	var at := anchor.get_screen_position() + Vector2(0, anchor.size.y + 2)
	menu.popup(Rect2i(Vector2i(at), Vector2i.ZERO))


# ---- commands ----

func _on_add(index: int) -> void:
	editor.run({"cmd": "add_layer", "kind": NEW_LAYER_KINDS[index][1]})


func _run_on_current(command: String) -> void:
	if editor.current_layer() >= 0:
		editor.run({"cmd": command, "layer": editor.current_layer()})


func _run_on_selected(command: String) -> void:
	var ids := _selected_ids()
	if not ids.is_empty():
		editor.run({"cmd": command, "layers": ids})


func _step(up: bool) -> void:
	if editor.current_layer() >= 0:
		editor.run({"cmd": "step_layer", "layer": editor.current_layer(), "up": up})


func _on_multi_selected(_item: TreeItem, _column: int, _selected: bool) -> void:
	if _updating or _selection_queued:
		return
	# A click that changes several rows reports each of them; act once, afterwards.
	_selection_queued = true
	_apply_selection.call_deferred()


func _apply_selection() -> void:
	_selection_queued = false
	var ids := _selected_ids()
	# The row clicked last becomes the current layer.
	var focused := _tree.get_selected()
	if focused != null and ids.has(focused.get_metadata(0)):
		ids.erase(focused.get_metadata(0))
		ids.append(focused.get_metadata(0))
	editor.run({"cmd": "select_layers", "layers": ids})


func _on_row_button(item: TreeItem, _column: int, id: int, _mouse_button: int) -> void:
	var layer: int = item.get_metadata(0)
	var row := editor.row(layer)
	match id:
		RowButton.EYE:
			editor.run({"cmd": "set_layer", "layer": layer, "visible": not row.visible})
		RowButton.LOCK:
			editor.run({"cmd": "set_layer", "layer": layer, "locked": not row.locked})
		RowButton.TARGET:
			editor.run({"cmd": "set_transform_target", "layer": null if row.transform_target else layer})


func _on_collapsed(item: TreeItem) -> void:
	if _updating:
		return
	editor.run({"cmd": "set_layer", "layer": item.get_metadata(0), "collapsed": item.collapsed})


func start_rename() -> void:
	var item: TreeItem = _items.get(editor.current_layer())
	if item == null:
		return
	item.set_editable(0, true)
	_tree.set_selected(item, 0)
	_tree.edit_selected(true)


func _on_renamed() -> void:
	var item := _tree.get_edited()
	if item == null:
		return
	item.set_editable(0, false)
	var layer: int = item.get_metadata(0)
	var text := item.get_text(0)
	# Show the stored name until the command has decided what the name becomes.
	item.set_text(0, editor.row(layer).get("name", text))
	editor.run({"cmd": "set_layer", "layer": layer, "name": text})


func _on_item_mouse_selected(mouse_position: Vector2, mouse_button: int) -> void:
	if mouse_button != MOUSE_BUTTON_RIGHT:
		return
	_apply_selection()
	# The selection was changed just now; its signals come next frame. The menu is
	# built for the row clicked, so read the state that command left.
	editor.catch_up()
	var row := editor.row(editor.current_layer())
	if row.is_empty():
		return
	var is_root: bool = row.parent == null
	_context.clear()
	_context.add_item("Rename", Menu.RENAME)
	if not is_root:
		_context.add_item("Duplicate", Menu.DUPLICATE)
		_context.add_item("Delete", Menu.DELETE)
		_context.add_item("Group selected", Menu.GROUP)
	if row.kind == "paint":
		_context.add_item("Merge down", Menu.MERGE)
	if row.kind in ["group", "sequence"] and not is_root:
		_context.add_item("Flatten", Menu.FLATTEN)
		_context.add_item("Convert to group" if row.kind == "sequence" else "Convert to sequence", Menu.TO_GROUP if row.kind == "sequence" else Menu.TO_SEQUENCE)
	if not is_root:
		_context.add_separator()
		_context.add_item("Flip X", Menu.FLIP_X)
		_context.add_item("Flip Y", Menu.FLIP_Y)
		_context.add_item("Flip Z", Menu.FLIP_Z)
		_context.add_item("Reset transform", Menu.RESET_TRANSFORM)
		_context.add_item("Reset pivot to centre", Menu.RESET_PIVOT)
		_context.add_item("Make pivot the origin", Menu.PIVOT_ORIGIN)
	_context.add_separator()
	_context.add_check_item("Show bounds", Menu.BOUNDS)
	_context.set_item_checked(_context.get_item_index(Menu.BOUNDS), row.show_bounds)
	if row.kind == "viewpoint":
		_context.add_item("Stand here", Menu.GO_TO)
		_context.add_check_item("Initial spawn area", Menu.INITIAL_SPAWN)
		_context.set_item_checked(_context.get_item_index(Menu.INITIAL_SPAWN), row.default_viewpoint)
	_context.popup(Rect2i(Vector2i(_tree.get_screen_position() + mouse_position), Vector2i.ZERO))


func _on_context(id: int) -> void:
	var layer := editor.current_layer()
	var row := editor.row(layer)
	match id:
		Menu.RENAME:
			start_rename()
		Menu.DUPLICATE:
			_run_on_selected("duplicate_layers")
		Menu.DELETE:
			_run_on_selected("delete_layers")
		Menu.GROUP:
			_run_on_selected("group_layers")
		Menu.MERGE:
			_run_on_current("merge_down")
		Menu.FLATTEN:
			_run_on_current("flatten_group")
		Menu.TO_SEQUENCE, Menu.TO_GROUP:
			editor.run({"cmd": "set_group_is_sequence", "layer": layer, "sequence": id == Menu.TO_SEQUENCE})
		Menu.FLIP_X, Menu.FLIP_Y, Menu.FLIP_Z:
			editor.run({"cmd": "flip_layer", "layer": layer, "axis": ["x", "y", "z"][id - Menu.FLIP_X]})
		Menu.RESET_TRANSFORM:
			_run_on_current("reset_transform")
		Menu.RESET_PIVOT:
			_run_on_current("reset_pivot")
		Menu.PIVOT_ORIGIN:
			_run_on_current("make_pivot_origin")
		Menu.BOUNDS:
			editor.run({"cmd": "set_layer", "layer": layer, "show_bounds": not row.show_bounds})
		Menu.INITIAL_SPAWN:
			editor.run({"cmd": "set_default_viewpoint", "layer": null if row.default_viewpoint else layer})
		Menu.GO_TO:
			editor.scene.view_go_to_viewpoint(layer)


# ---- drag and drop ----

func _get_drag(_at: Vector2) -> Variant:
	var ids := _selected_ids()
	if ids.is_empty():
		return null
	var preview := Kit.label("%d layers" % ids.size() if ids.size() > 1 else editor.row(ids[0]).get("name", ""))
	_tree.set_drag_preview(preview)
	return {"limner_layers": ids}


func _can_drop(at: Vector2, data: Variant) -> bool:
	if not (data is Dictionary and data.has("limner_layers")):
		return false
	return _tree.get_item_at_position(at) != null


func _drop(at: Vector2, data: Variant) -> void:
	var target := _tree.get_item_at_position(at)
	if target == null:
		return
	editor.catch_up()
	var target_row := editor.row(target.get_metadata(0))
	var section := _tree.get_drop_section_at_position(at)
	var into_group: bool = section == 0 and target_row.kind in ["group", "sequence"]
	if target_row.parent == null:
		into_group = true
	editor.begin("Move layers")
	var ids: Array = data.limner_layers
	if section > 0 or into_group:
		# Dropping several keeps their order: insert them last to first at the same place.
		ids = ids.duplicate()
		ids.reverse()
	for id: int in ids:
		if id == int(target_row.id):
			continue
		var parent: int = int(target_row.id) if into_group else int(target_row.parent)
		var index := 0
		if not into_group:
			index = _index_of(int(target_row.id), id) + (1 if section > 0 else 0)
		editor.run({"cmd": "move_layer", "layer": id, "parent": parent, "index": index})
		# The next layer's place is counted in the list as this move left it, not as
		# it was before the drop: the rows are read again now, not next frame.
		editor.catch_up()
	editor.commit()


## Position of `target` among its siblings once `moving` has been taken out of the list.
func _index_of(target: int, moving: int) -> int:
	var parent = editor.row(target).parent
	var index := 0
	for row: Dictionary in editor.get_rows():
		if row.parent == parent and int(row.id) != moving:
			if int(row.id) == target:
				return index
			index += 1
	return index
