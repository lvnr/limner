class_name LayerRows
extends VBoxContainer
## The layers as Quill's Layers panel lists them: a page of rows of keys, never a list
## that scrolls. Each row has a fold key (for a group), the layer's name as a key,
## indented by how deep it is, and a column of keys: shown, its kind (which opens its
## settings), moved when grabbing, locked, bounds shown.
##
## Pages show `per_page` rows; page keys under them step a page, and the page follows
## the current layer. Folded groups shorten the list. The rows' height is `pitch()`,
## which the timeline's tracks beside them take too, so that the two line up; the
## space above the first row (`header_height`) is the height of the ruler over them.

## A layer's kind key was pressed: show that layer's settings.
signal settings_requested(layer: int)
## The page shown, or the rows on it, changed.
signal page_changed

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
## How far each level of nesting moves a name to the right, in design pixels.
const INDENT := 14
## Notches of a wheel (or of a controller's stick, which turns one over a panel) that
## step a page.
const NOTCHES_PER_PAGE := 3
enum Menu { RENAME, DUPLICATE, DELETE, GROUP, MERGE, FLATTEN, TO_SEQUENCE, TO_GROUP, FLIP_X, FLIP_Y, FLIP_Z, RESET_TRANSFORM, RESET_PIVOT, PIVOT_ORIGIN, BOUNDS, INITIAL_SPAWN, GO_TO }

var editor: LimnerEditor
## The listed rows (not folded away inside a group), in list order.
var listed: Array = []
var pager: Pager
## The height left above the first row, so that it lines up with the first track.
var header_height := 0.0:
	set(value):
		header_height = value
		if _header != null:
			_header.custom_minimum_size.y = value

var _header: Control
var _lines: Array[Dictionary] = []
var _box: VBoxContainer
var _context: PopupMenu
var _renaming := -1
var _notches := 0
## The modifier keys held when a name was last pressed: "toggle", "range" or "".
var _press_mode := ""


func _init(limner_editor: LimnerEditor, rows_per_page := 10) -> void:
	editor = limner_editor
	name = "Rows"
	add_theme_constant_override("separation", 0)
	_header = Control.new()
	_header.name = "Header"
	add_child(_header)
	_box = VBoxContainer.new()
	_box.add_theme_constant_override("separation", 0)
	add_child(_box)
	pager = Pager.new(rows_per_page)
	pager.page_changed.connect(func(_first: int) -> void: _show())
	var under := Control.new()
	under.custom_minimum_size = Vector2(0, Kit.u(4))
	add_child(under)
	add_child(pager)
	_context = PopupMenu.new()
	_context.id_pressed.connect(_on_context)
	add_child(_context)
	_make_lines(rows_per_page)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


## The height of a row: a key and a little room under it.
static func pitch() -> float:
	return float(Kit.u(Kit.KEY) + Kit.u(4))


## The rows on the page shown, in order.
func page_rows() -> Array:
	return listed.slice(pager.first, pager.first + pager.per_page)


## Changes how many rows a page shows (the desktop fits it to its dock).
func set_per_page(rows: int) -> void:
	rows = maxi(rows, 1)
	if rows == pager.per_page:
		return
	pager.set_per_page(rows)
	_make_lines(rows)
	_show()


## Brings the rows in line with the document, on the page that holds the current layer.
func refresh() -> void:
	listed = editor.get_rows().filter(func(row: Dictionary) -> bool: return not row.hidden_in_list)
	pager.set_count(listed.size())
	var current := editor.current_layer()
	for index in listed.size():
		if int(listed[index].id) == current:
			pager.show_item(index)
			break
	_show()


## The colour scheme changed: names take their colours again.
func apply_scheme() -> void:
	_show()


## Starts renaming the current layer: its name becomes a box to type in.
func start_rename() -> void:
	var current := editor.current_layer()
	for index in listed.size():
		if int(listed[index].id) == current:
			pager.show_item(index)
			_renaming = current
			_show()
			for line: Dictionary in _lines:
				if int(line.get("id", -1)) == current:
					var field: LineEdit = line.field
					field.grab_focus()
					field.select_all()
			return


# ---- the rows ----

func _make_lines(count: int) -> void:
	for line: Dictionary in _lines:
		(line.row as Control).queue_free()
	_lines.clear()
	for index in count:
		var line := {}
		var row := HBoxContainer.new()
		row.custom_minimum_size = Vector2(0, pitch())
		row.add_theme_constant_override("separation", Kit.u(4))
		line.row = row
		line.fold = Kit.key("down", "Fold or unfold the group", _on_fold.bind(index), false, Kit.KEY_SMALL)
		line.fold.size_flags_vertical = Control.SIZE_SHRINK_CENTER
		line.fold_room = Control.new()
		line.fold_room.custom_minimum_size = Vector2(Kit.u(Kit.KEY_SMALL), 0)
		line.indent = Control.new()
		line.name = Kit.text_key("", "", _on_name.bind(index), "paint_layer", true)
		line.name.alignment = HORIZONTAL_ALIGNMENT_LEFT
		line.name.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		line.name.gui_input.connect(_on_name_input.bind(index))
		line.field = TextField.new()
		line.field.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		line.field.custom_minimum_size.y = Kit.u(Kit.KEY)
		line.field.committed.connect(_on_renamed.bind(index))
		line.field.focus_exited.connect(func() -> void: _end_rename())
		line.visible_key = Kit.key("visible", "Shown: show or hide the layer", _on_visible.bind(index), true)
		line.kind = Kit.key("paint_layer", "Settings of this layer", _on_kind.bind(index))
		line.target = Kit.key("move", "Moved when grabbing: the grips move this layer instead of the view", _on_target.bind(index), true)
		line.lock = Kit.key("unlocked", "Locked: nothing in the layer can be changed", _on_lock.bind(index), true)
		line.bounds = Kit.key("bounds", "Bounds: show the box around the layer", _on_bounds.bind(index), true)
		for key: String in ["fold", "fold_room", "indent", "name", "field", "visible_key", "kind", "target", "lock", "bounds"]:
			row.add_child(line[key])
			(line[key] as Control).gui_input.connect(_on_wheel)
		row.gui_input.connect(_on_wheel)
		_box.add_child(row)
		_lines.append(line)


func _show() -> void:
	var rows := page_rows()
	for index in _lines.size():
		var line: Dictionary = _lines[index]
		var row_box: Control = line.row
		if index >= rows.size():
			line.id = -1
			# An empty row keeps its height, so the page and the tracks keep theirs.
			for child: Control in row_box.get_children():
				child.visible = false
			continue
		var row: Dictionary = rows[index]
		var id := int(row.id)
		line.id = id
		var is_root: bool = row.parent == null
		var folds: bool = int(row.children) > 0 and not is_root
		(line.fold as Button).visible = folds
		(line.fold_room as Control).visible = not folds
		Kit.set_key_icon(line.fold, "next" if row.collapsed else "down", Kit.KEY_SMALL)
		(line.indent as Control).visible = true
		(line.indent as Control).custom_minimum_size = Vector2(Kit.u(INDENT) * int(row.depth), 0)
		var renaming := _renaming == id
		var name_key: Button = line.name
		var field: LineEdit = line.field
		name_key.visible = not renaming
		field.visible = renaming
		if renaming and not field.has_focus():
			field.text = row.name
		name_key.text = row.name
		name_key.tooltip_text = _describe(row)
		name_key.set_pressed_no_signal(row.selected)
		Kit.set_key_icon(name_key, KIND_ICONS.get(row.kind, "dot"), Kit.KEY * 0.86)
		# A layer that does not show now (hidden, or outside its clip) is greyed.
		name_key.modulate = Color(1, 1, 1, 1.0 if row.shown else 0.55)
		var visible_key: Button = line.visible_key
		visible_key.visible = true
		visible_key.set_pressed_no_signal(not row.visible)
		Kit.set_key_icon(visible_key, "visible" if row.visible else "hidden")
		var kind: Button = line.kind
		kind.visible = true
		Kit.set_key_icon(kind, KIND_ICONS.get(row.kind, "dot"))
		kind.tooltip_text = "%s settings" % String(row.kind).capitalize()
		var target: Button = line.target
		target.visible = true
		target.disabled = is_root
		target.set_pressed_no_signal(row.transform_target)
		var lock: Button = line.lock
		lock.visible = true
		lock.disabled = is_root
		lock.set_pressed_no_signal(row.locked)
		Kit.set_key_icon(lock, "locked" if row.locked else "unlocked")
		var bounds: Button = line.bounds
		bounds.visible = true
		bounds.set_pressed_no_signal(row.show_bounds)
	page_changed.emit()


func _describe(row: Dictionary) -> String:
	var text: String = "%s: %s" % [row.name, String(row.kind).capitalize()]
	if row.kind == "paint":
		text += ", %d strokes" % row.strokes
		if row.frames > 1:
			text += ", %d frames" % row.frames
	if row.animated:
		text += ", animated"
	if row.default_viewpoint:
		text += ", initial spawn area"
	return text


func _row_at(index: int) -> Dictionary:
	var rows := page_rows()
	return rows[index] if index < rows.size() else {}


# ---- presses ----

## The wheel over the rows, or a controller's stick, steps the pages.
func _on_wheel(event: InputEvent) -> void:
	var wheel := event as InputEventMouseButton
	if wheel == null or not wheel.pressed or wheel.button_index not in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
		return
	_notches += 1 if wheel.button_index == MOUSE_BUTTON_WHEEL_DOWN else -1
	if absi(_notches) >= NOTCHES_PER_PAGE:
		pager.step(signi(_notches))
		_notches = 0
	get_viewport().set_input_as_handled()


## Remembers which modifier keys came with a press on a name: Ctrl (Cmd on a Mac, the
## other hand's trigger in a headset) adds the layer to the selection or takes it out,
## Shift selects every layer from the current one to it.
func _on_name_input(event: InputEvent, _index: int) -> void:
	var press := event as InputEventMouseButton
	if press != null and press.pressed:
		_press_mode = "toggle" if press.is_command_or_control_pressed() else ("range" if press.shift_pressed else "")
		if press.button_index == MOUSE_BUTTON_RIGHT and not Kit.headset:
			_open_menu(_index, press.global_position)


func _on_name(index: int) -> void:
	var row := _row_at(index)
	if row.is_empty():
		return
	var id := int(row.id)
	var chosen: Array = []
	match _press_mode:
		"toggle":
			# The layer pressed joins the selection as the current one, or leaves it; the
			# last one left in it is then current. The selection is never empty.
			for each: Dictionary in editor.get_rows():
				if each.selected and int(each.id) != id:
					chosen.append(int(each.id))
			if not row.selected or chosen.is_empty():
				chosen.append(id)
		"range":
			var from := -1
			var to := -1
			for at in listed.size():
				if int(listed[at].id) == editor.current_layer():
					from = at
				if int(listed[at].id) == id:
					to = at
			if from < 0:
				chosen = [id]
			else:
				for at in range(mini(from, to), maxi(from, to) + 1):
					chosen.append(int(listed[at].id))
				# The layer pressed becomes the current one.
				chosen.erase(id)
				chosen.append(id)
		_:
			chosen = [id]
	_press_mode = ""
	editor.run({"cmd": "select_layers", "layers": chosen})


func _on_fold(index: int) -> void:
	var row := _row_at(index)
	if not row.is_empty():
		editor.run({"cmd": "set_layer", "layer": int(row.id), "collapsed": not row.collapsed})


func _on_visible(index: int) -> void:
	var row := _row_at(index)
	if not row.is_empty():
		editor.run({"cmd": "set_layer", "layer": int(row.id), "visible": not row.visible})


func _on_kind(index: int) -> void:
	var row := _row_at(index)
	if row.is_empty():
		return
	if editor.current_layer() != int(row.id):
		editor.run({"cmd": "select_layers", "layers": [int(row.id)]})
	settings_requested.emit(int(row.id))


func _on_target(index: int) -> void:
	var row := _row_at(index)
	if not row.is_empty():
		editor.run({"cmd": "set_transform_target", "layer": null if row.transform_target else int(row.id)})


func _on_lock(index: int) -> void:
	var row := _row_at(index)
	if not row.is_empty():
		editor.run({"cmd": "set_layer", "layer": int(row.id), "locked": not row.locked})


func _on_bounds(index: int) -> void:
	var row := _row_at(index)
	if not row.is_empty():
		editor.run({"cmd": "set_layer", "layer": int(row.id), "show_bounds": not row.show_bounds})


func _on_renamed(text: String, index: int) -> void:
	var row := _row_at(index)
	_renaming = -1
	if not row.is_empty() and text != row.name:
		editor.run({"cmd": "set_layer", "layer": int(row.id), "name": text})
	_show()


func _end_rename() -> void:
	if _renaming >= 0:
		_renaming = -1
		_show.call_deferred()


# ---- the desktop's context menu ----

## The menu of a row, on the desktop's right click: a shortcut to what the keys of
## the Layers, Transform and Layer panels do.
func _open_menu(index: int, at: Vector2) -> void:
	var row := _row_at(index)
	if row.is_empty():
		return
	if not row.selected:
		editor.run({"cmd": "select_layers", "layers": [int(row.id)]})
		editor.catch_up()
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
	_context.popup(Rect2i(Vector2i(at), Vector2i.ZERO))


func _selected_ids() -> Array:
	var ids: Array = []
	for row: Dictionary in editor.get_rows():
		if row.selected:
			ids.append(int(row.id))
	return ids


func _on_context(id: int) -> void:
	var layer := editor.current_layer()
	var row := editor.row(layer)
	match id:
		Menu.RENAME:
			start_rename()
		Menu.DUPLICATE:
			editor.run({"cmd": "duplicate_layers", "layers": _selected_ids()})
		Menu.DELETE:
			editor.run({"cmd": "delete_layers", "layers": _selected_ids()})
		Menu.GROUP:
			editor.run({"cmd": "group_layers", "layers": _selected_ids()})
		Menu.MERGE:
			editor.run({"cmd": "merge_down", "layer": layer})
		Menu.FLATTEN:
			editor.run({"cmd": "flatten_group", "layer": layer})
		Menu.TO_SEQUENCE, Menu.TO_GROUP:
			editor.run({"cmd": "set_group_is_sequence", "layer": layer, "sequence": id == Menu.TO_SEQUENCE})
		Menu.FLIP_X, Menu.FLIP_Y, Menu.FLIP_Z:
			editor.run({"cmd": "flip_layer", "layer": layer, "axis": ["x", "y", "z"][id - Menu.FLIP_X]})
		Menu.RESET_TRANSFORM:
			editor.run({"cmd": "reset_transform", "layer": layer})
		Menu.RESET_PIVOT:
			editor.run({"cmd": "reset_pivot", "layer": layer})
		Menu.PIVOT_ORIGIN:
			editor.run({"cmd": "make_pivot_origin", "layer": layer})
		Menu.BOUNDS:
			editor.run({"cmd": "set_layer", "layer": layer, "show_bounds": not row.show_bounds})
		Menu.INITIAL_SPAWN:
			editor.run({"cmd": "set_default_viewpoint", "layer": null if row.default_viewpoint else layer})
		Menu.GO_TO:
			editor.scene.view_go_to_viewpoint(layer)
