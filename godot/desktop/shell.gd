class_name DesktopShell
extends Node
## The desktop presentation: a fixed virtual head, mouse navigation, and the panels
## docked around the view.
##
## The camera never moves. Navigation changes where the viewer stands in the world,
## exactly as grabbing the world does in a headset, so tool sizes and precision
## behave the same in both.

## Where the virtual head is, in viewer space.
const HEAD := Vector3(0.0, 1.4, 0.0)
## How far in front of the head the point is that navigation turns and zooms about.
const FOCUS_DISTANCE := 1.5
const FRAME_RADIUS := 0.55
const ORBIT_SPEED := 0.006
const ZOOM_STEP := 1.12
const LEFT_DOCK_WIDTH := 292
const RIGHT_DOCK_WIDTH := 324

enum FileMenu { NEW, OPEN, SAVE, SAVE_AS, QUIT }
enum EditMenu { UNDO, REDO, DUPLICATE, DELETE, RENAME }
enum ViewMenu { FRAME_ALL, HOME, GRID, EXTRAS, PANELS, FULLSCREEN }

var world: LimnerWorld
var editor: LimnerEditor
var camera: Camera3D
## The root of the interface. Hidden with Tab for an unobstructed view.
var ui: Control
var layers_panel: LayersPanel
var left_dock: VBoxContainer
var right_dock: VBoxContainer
var bottom_dock: VBoxContainer

var _free_area: Control
var _title: Label
var _stats: Label
var _toast: Label
var _toast_timer := 0.0
var _undo_button: Button
var _redo_button: Button
var _view_menu: PopupMenu
var _edit_menu: PopupMenu
var _file_dialog: FileDialog
var _dragging := false
var _panning := false


func _ready() -> void:
	camera = Camera3D.new()
	camera.name = "Head"
	camera.position = HEAD
	camera.fov = 60.0
	camera.near = 0.01
	camera.far = 2000.0
	world.add_child(camera)
	camera.make_current()
	_build_interface()
	editor.message.connect(show_message)
	editor.document_changed.connect(_refresh)
	editor.state_changed.connect(_refresh)
	editor.document_replaced.connect(_refresh)
	_refresh()


func _build_interface() -> void:
	var layer := CanvasLayer.new()
	layer.name = "Interface"
	add_child(layer)
	ui = Control.new()
	ui.name = "Root"
	ui.theme = Kit.make_theme()
	ui.set_anchors_preset(Control.PRESET_FULL_RECT)
	ui.mouse_filter = Control.MOUSE_FILTER_IGNORE
	layer.add_child(ui)

	var frame := VBoxContainer.new()
	frame.set_anchors_preset(Control.PRESET_FULL_RECT)
	frame.mouse_filter = Control.MOUSE_FILTER_IGNORE
	frame.add_theme_constant_override("separation", 0)
	ui.add_child(frame)
	frame.add_child(_build_top_bar())

	var middle := HBoxContainer.new()
	middle.size_flags_vertical = Control.SIZE_EXPAND_FILL
	middle.mouse_filter = Control.MOUSE_FILTER_IGNORE
	middle.add_theme_constant_override("separation", 0)
	frame.add_child(middle)

	left_dock = _dock(middle, LEFT_DOCK_WIDTH)
	_free_area = Control.new()
	_free_area.name = "View"
	_free_area.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_free_area.mouse_filter = Control.MOUSE_FILTER_IGNORE
	middle.add_child(_free_area)
	right_dock = _dock(middle, RIGHT_DOCK_WIDTH)

	var bottom := PanelContainer.new()
	bottom.name = "BottomDock"
	bottom_dock = VBoxContainer.new()
	bottom.add_child(bottom_dock)
	bottom.visible = false
	frame.add_child(bottom)

	layers_panel = LayersPanel.new(editor)
	right_dock.add_child(layers_panel)

	_toast = Kit.label("")
	_toast.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_toast.add_theme_stylebox_override("normal", Kit._box(Kit.BG_SUNKEN, 6, 1, Kit.BORDER, 10))
	_toast.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	_toast.grow_horizontal = Control.GROW_DIRECTION_BOTH
	_toast.grow_vertical = Control.GROW_DIRECTION_BEGIN
	_toast.offset_bottom = -16
	_toast.visible = false
	_toast.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_free_area.add_child(_toast)

	_file_dialog = FileDialog.new()
	_file_dialog.access = FileDialog.ACCESS_FILESYSTEM
	_file_dialog.use_native_dialog = true
	ui.add_child(_file_dialog)


func _dock(parent: Control, width: int) -> VBoxContainer:
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(width, 0)
	var scroll := ScrollContainer.new()
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	panel.add_child(scroll)
	var content := VBoxContainer.new()
	content.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	content.size_flags_vertical = Control.SIZE_EXPAND_FILL
	content.add_theme_constant_override("separation", 10)
	scroll.add_child(content)
	parent.add_child(panel)
	return content


func _build_top_bar() -> Control:
	var bar := PanelContainer.new()
	bar.name = "TopBar"
	var row := HBoxContainer.new()
	bar.add_child(row)

	var menu := MenuBar.new()
	menu.prefer_global_menu = false
	menu.flat = true
	row.add_child(menu)

	var file := PopupMenu.new()
	file.name = "File"
	file.add_item("New", FileMenu.NEW, KEY_MASK_CMD_OR_CTRL | KEY_N)
	file.add_item("Open…", FileMenu.OPEN, KEY_MASK_CMD_OR_CTRL | KEY_O)
	file.add_separator()
	file.add_item("Quit", FileMenu.QUIT, KEY_MASK_CMD_OR_CTRL | KEY_Q)
	file.id_pressed.connect(_on_file_menu)
	menu.add_child(file)

	_edit_menu = PopupMenu.new()
	_edit_menu.name = "Edit"
	_edit_menu.add_item("Undo", EditMenu.UNDO, KEY_MASK_CMD_OR_CTRL | KEY_Z)
	_edit_menu.add_item("Redo", EditMenu.REDO, KEY_MASK_CMD_OR_CTRL | KEY_MASK_SHIFT | KEY_Z)
	_edit_menu.add_separator()
	_edit_menu.add_item("Rename layer", EditMenu.RENAME, KEY_F2)
	_edit_menu.add_item("Duplicate layer", EditMenu.DUPLICATE, KEY_MASK_CMD_OR_CTRL | KEY_D)
	_edit_menu.add_item("Delete layer", EditMenu.DELETE, KEY_MASK_CMD_OR_CTRL | KEY_BACKSPACE)
	_edit_menu.id_pressed.connect(_on_edit_menu)
	menu.add_child(_edit_menu)

	_view_menu = PopupMenu.new()
	_view_menu.name = "View"
	_view_menu.add_item("Frame everything", ViewMenu.FRAME_ALL, KEY_F)
	_view_menu.add_item("Go to initial spawn area", ViewMenu.HOME, KEY_HOME)
	_view_menu.add_separator()
	_view_menu.add_check_item("Floor grid", ViewMenu.GRID, KEY_G)
	_view_menu.add_check_item("Gizmos for cameras, lights and spawn areas", ViewMenu.EXTRAS)
	_view_menu.add_separator()
	_view_menu.add_check_item("Panels", ViewMenu.PANELS, KEY_TAB)
	_view_menu.add_check_item("Full screen", ViewMenu.FULLSCREEN, KEY_MASK_CMD_OR_CTRL | KEY_MASK_SHIFT | KEY_F)
	_view_menu.id_pressed.connect(_on_view_menu)
	menu.add_child(_view_menu)

	row.add_child(VSeparator.new())
	_undo_button = Kit.icon_button("undo", "Undo", editor.undo)
	_redo_button = Kit.icon_button("redo", "Redo", editor.redo)
	row.add_child(_undo_button)
	row.add_child(_redo_button)
	row.add_child(VSeparator.new())
	_title = Kit.label("")
	row.add_child(_title)
	row.add_child(Kit.spacer())
	_stats = Kit.label("", true)
	row.add_child(_stats)
	return bar


func _process(delta: float) -> void:
	var stats := world.scene.stats()
	_stats.text = "%s triangles   %d fps" % [_thousands(stats.triangles), Engine.get_frames_per_second()]
	if _toast.visible:
		_toast_timer -= delta
		_toast.modulate.a = clampf(_toast_timer * 2.0, 0.0, 1.0)
		if _toast_timer <= 0.0:
			_toast.visible = false


func _refresh() -> void:
	var status := world.scene.status()
	_title.text = "%s%s" % [editor.title(), "  •" if status.dirty else ""]
	get_window().title = "%s%s — Limner" % [editor.title(), " (edited)" if status.dirty else ""]
	_undo_button.disabled = not status.can_undo
	_redo_button.disabled = not status.can_redo
	_undo_button.tooltip_text = "Undo %s" % status.undo_label if status.can_undo else "Nothing to undo"
	_redo_button.tooltip_text = "Redo %s" % status.redo_label if status.can_redo else "Nothing to redo"
	_edit_menu.set_item_disabled(_edit_menu.get_item_index(EditMenu.UNDO), not status.can_undo)
	_edit_menu.set_item_disabled(_edit_menu.get_item_index(EditMenu.REDO), not status.can_redo)
	var options := editor.get_options()
	world.show_grid = options.show_grid
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.GRID), options.show_grid)
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.EXTRAS), options.show_extras)
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.PANELS), left_dock.is_visible_in_tree())
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.FULLSCREEN), get_window().mode == Window.MODE_FULLSCREEN)


## Shows a short message at the bottom of the view for a few seconds.
func show_message(text: String, is_error := false) -> void:
	_toast.text = text
	_toast.add_theme_color_override("font_color", Kit.DANGER.lightened(0.3) if is_error else Kit.TEXT)
	_toast.visible = true
	_toast.modulate.a = 1.0
	_toast_timer = 4.0 if is_error else 2.5
	_toast.reset_size()
	if is_error:
		push_warning(text)


func _thousands(value: int) -> String:
	var text := str(value)
	var out := ""
	while text.length() > 3:
		out = "," + text.right(3) + out
		text = text.left(text.length() - 3)
	return text + out


# ---- menus ----

func _on_file_menu(id: int) -> void:
	match id:
		FileMenu.NEW:
			editor.new_document()
		FileMenu.OPEN:
			_file_dialog.file_mode = FileDialog.FILE_MODE_OPEN_ANY
			_file_dialog.title = "Open a Limner document or a Quill project folder"
			for connection in _file_dialog.file_selected.get_connections():
				_file_dialog.file_selected.disconnect(connection.callable)
			for connection in _file_dialog.dir_selected.get_connections():
				_file_dialog.dir_selected.disconnect(connection.callable)
			_file_dialog.file_selected.connect(_open_path)
			_file_dialog.dir_selected.connect(_open_path)
			_file_dialog.popup_centered_ratio(0.7)
		FileMenu.QUIT:
			get_tree().quit()


func _open_path(path: String) -> void:
	# A file inside a project folder stands for the folder.
	if path.get_file() in ["Quill.json", "Quill.qbin", "State.json"]:
		path = path.get_base_dir()
	if editor.open_document(path):
		frame_all()


func _on_edit_menu(id: int) -> void:
	match id:
		EditMenu.UNDO:
			editor.undo()
		EditMenu.REDO:
			editor.redo()
		EditMenu.RENAME:
			layers_panel.start_rename()
		EditMenu.DUPLICATE:
			if editor.current_layer() >= 0:
				editor.run({"cmd": "duplicate_layers", "layers": [editor.current_layer()]})
		EditMenu.DELETE:
			if editor.current_layer() >= 0:
				editor.run({"cmd": "delete_layers", "layers": [editor.current_layer()]})


func _on_view_menu(id: int) -> void:
	match id:
		ViewMenu.FRAME_ALL:
			frame_all()
		ViewMenu.HOME:
			world.scene.view_go_to_default_viewpoint()
		ViewMenu.GRID:
			editor.set_option("show_grid", not editor.get_options().show_grid)
		ViewMenu.EXTRAS:
			editor.set_option("show_extras", not editor.get_options().show_extras)
		ViewMenu.PANELS:
			set_panels_visible(not left_dock.is_visible_in_tree())
		ViewMenu.FULLSCREEN:
			var window := get_window()
			window.mode = Window.MODE_WINDOWED if window.mode == Window.MODE_FULLSCREEN else Window.MODE_FULLSCREEN
	_refresh()


## Shows or hides the docks, leaving the menu bar.
func set_panels_visible(visible: bool) -> void:
	left_dock.get_parent().get_parent().visible = visible
	right_dock.get_parent().get_parent().visible = visible
	bottom_dock.get_parent().visible = visible and bottom_dock.get_child_count() > 0


# ---- the view ----

## The point navigation turns and zooms about: straight through the middle of the
## part of the window the panels leave free.
func focus_point() -> Vector3:
	var rect := _free_area.get_global_rect()
	if rect.size.x < 8.0 or rect.size.y < 8.0:
		rect = get_viewport().get_visible_rect()
	var center := rect.get_center()
	return camera.project_ray_origin(center) + camera.project_ray_normal(center) * FOCUS_DISTANCE


## How large a sphere at the focus point fits in the free part of the window.
func _frame_radius() -> float:
	var rect := _free_area.get_global_rect()
	var full := get_viewport().get_visible_rect().size
	if rect.size.y < 8.0 or full.y < 8.0:
		return FRAME_RADIUS
	var view_height := 2.0 * FOCUS_DISTANCE * tan(deg_to_rad(camera.fov) * 0.5)
	var metres_per_pixel := view_height / full.y
	return minf(rect.size.x, rect.size.y) * metres_per_pixel * 0.42


## Fits everything visible in the free part of the window.
func frame_all() -> void:
	world.scene.view_frame_all(focus_point(), _frame_radius())


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		_on_mouse_button(event)
	elif event is InputEventMouseMotion and _dragging:
		_on_drag(event.relative, _panning or event.shift_pressed)
	elif event is InputEventPanGesture:
		# Trackpad: two fingers orbit, with Shift they pan.
		_on_drag(-event.delta * 8.0, event.shift_pressed)
	elif event is InputEventMagnifyGesture:
		world.scene.view_zoom(_pivot_under_mouse(), event.factor)
	elif event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_SPACE:
				editor.run({"cmd": "toggle_play"})
			KEY_LEFT:
				editor.run({"cmd": "step_frames", "frames": -1})
			KEY_RIGHT:
				editor.run({"cmd": "step_frames", "frames": 1})


func _on_mouse_button(event: InputEventMouseButton) -> void:
	match event.button_index:
		MOUSE_BUTTON_WHEEL_UP:
			if event.pressed:
				world.scene.view_zoom(_pivot_under_mouse(), ZOOM_STEP)
		MOUSE_BUTTON_WHEEL_DOWN:
			if event.pressed:
				world.scene.view_zoom(_pivot_under_mouse(), 1.0 / ZOOM_STEP)
		MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE:
			_dragging = event.pressed
			_panning = event.shift_pressed
		MOUSE_BUTTON_LEFT:
			if event.alt_pressed or not event.pressed:
				_dragging = event.pressed
				_panning = event.shift_pressed


func _on_drag(relative: Vector2, pan: bool) -> void:
	var focus := focus_point()
	if pan:
		# One screen height of mouse travel moves the world by the height of the view at the focus.
		var metres_per_pixel := 2.0 * FOCUS_DISTANCE * tan(deg_to_rad(camera.fov) * 0.5) / get_viewport().get_visible_rect().size.y
		world.scene.view_pan(Vector3(relative.x, -relative.y, 0.0) * metres_per_pixel)
	else:
		world.scene.view_rotate(focus, Vector3.UP, relative.x * ORBIT_SPEED)
		world.scene.view_rotate(focus, Vector3.RIGHT, relative.y * ORBIT_SPEED)


## The point under the mouse at the focus distance, in viewer space.
func _pivot_under_mouse() -> Vector3:
	var mouse := get_viewport().get_mouse_position()
	var origin := camera.project_ray_origin(mouse)
	var direction := camera.project_ray_normal(mouse)
	var focus_z := HEAD.z - FOCUS_DISTANCE
	if absf(direction.z) < 1e-6:
		return focus_point()
	return origin + direction * ((focus_z - origin.z) / direction.z)
