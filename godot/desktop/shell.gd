class_name DesktopShell
extends Node
## The desktop presentation: a fixed virtual head, mouse navigation, and the panels
## docked around the view.
##
## The camera never moves. Navigation changes where the viewer stands in the world,
## exactly as grabbing the world does in a headset, so tool sizes and precision
## behave the same in both.

## The user asked to carry on in the headset: "headset". The app makes the change
## (`main.gd`, `switch_to`), after this signal's emitter has finished.
signal switch_requested(to: String)

## Where the virtual head is, in viewer space.
const HEAD := Vector3(0.0, 1.4, 0.0)
## How far in front of the head the point is that navigation turns and zooms about.
const FOCUS_DISTANCE := 1.5
const FRAME_RADIUS := 0.55
const ORBIT_SPEED := 0.006
const ZOOM_STEP := 1.12
## Keys that pick a tool.
const TOOL_KEYS := {
	KEY_B: "paint",
	KEY_L: "line",
	KEY_C: "colorize",
	KEY_E: "eraser",
	KEY_S: "select",
	KEY_G: "grab",
	KEY_T: "thicken",
	KEY_N: "nudge",
	KEY_O: "optimize",
}
const LEFT_DOCK_WIDTH := 292
const RIGHT_DOCK_WIDTH := 324

enum FileMenu { NEW, OPEN, SAVE, SAVE_AS, IMPORT, EXPORT, QUIT }
## What File > Import offers: pictures, sounds and models become layers.
const IMPORT_FILTERS := [
	"*.png, *.jpg, *.jpeg ; Pictures",
	"*.wav, *.ogg, *.mp3 ; Sounds",
	"*.obj, *.glb, *.gltf, *.fbx ; Models",
	"*.limnz ; Limner projects (a project folder can be chosen too, as can a Quill project folder)",
]
enum EditMenu { UNDO, REDO, DUPLICATE, DELETE, RENAME, SELECT_ALL, DESELECT, TRANSFORM_AGAIN }
enum ViewMenu { FRAME_ALL, HOME, GRID, EXTRAS, PANELS, FULLSCREEN, GIMBAL, HEADSET }

var world: LimnerWorld
var editor: LimnerEditor
## True when the shell takes over from the headset in a running app, rather than at
## start: the work in hand is not offered as work to recover and the start-up
## preferences are not applied again. Set it before the shell enters the tree.
var switched := false
var camera: Camera3D
## The shape of the tool at the drawing hand, in the world.
var cursor: ToolCursor
## The root of the interface. Hidden with Tab for an unobstructed view.
var ui: Control
## Clicks, and the sounds of undo, redo, picking a colour and deleting.
var sounds: Sounds
var autosave: Autosave
var timeline: TimelinePanel
## The stand-in for the drawing hand.
var hand: DesktopHand
var left_dock: VBoxContainer
var right_dock: VBoxContainer
var bottom_dock: VBoxContainer
## The tabbed panels under the layer list.
var details: TabContainer

var _free_area: Control
var _title: Label
var _stats: Label
var _toast: Label
var _toast_timer := 0.0
var _undo_button: Button
var _redo_button: Button
## The top bar's "Continue in the headset".
var _headset_button: Button
var _view_menu: PopupMenu
var _edit_menu: PopupMenu
var _file_dialog: FileDialog
var _dragging := false
var _panning := false
## How many thumbnails are being taken now, and whether the panels were shown before
## the first of them hid them.
var _thumbnails_taken := 0
var _ui_shown_before_thumbnail := true


func _ready() -> void:
	camera = Camera3D.new()
	camera.name = "Head"
	camera.position = HEAD
	camera.fov = 60.0
	camera.near = 0.01
	camera.far = 2000.0
	world.add_child(camera)
	camera.make_current()
	hand = DesktopHand.new()
	hand.name = "Hand"
	hand.world = world
	hand.editor = editor
	hand.camera = camera
	hand.depth = FOCUS_DISTANCE
	sounds = Sounds.new()
	sounds.name = "Sounds"
	add_child(sounds)
	hand.sounds = sounds
	if not switched:
		Preferences.shared().apply_at_start(editor)
	add_child(hand)
	# The app hands over its own autosave, which lasts the whole run; a shell started
	# alone (a test) makes one.
	if autosave == null:
		autosave = Autosave.new()
		autosave.name = "Autosave"
		autosave.editor = editor
		add_child(autosave)
	world.scene.playback_event.connect(func(event: String) -> void:
		if event == "stop":
			sounds.play("stop")
	)
	cursor = ToolCursor.new()
	cursor.name = "ToolCursor"
	cursor.scene = world.scene
	world.add_child(cursor)
	_build_interface()
	get_tree().auto_accept_quit = false
	editor.message.connect(show_message)
	editor.document_changed.connect(_refresh)
	editor.state_changed.connect(_refresh)
	editor.document_replaced.connect(_refresh)
	ModeSwitch.events.changed.connect(_refresh_mode_switch)
	_refresh()
	_refresh_mode_switch()
	# Work left unsaved when the app last closed badly is offered back, unless this
	# run is an automated one, or the work in hand came over from the headset.
	var args := OS.get_cmdline_user_args()
	if not switched and not Autosave.pending().is_empty() and not (args.has("--shot") or args.has("--bench") or args.has("--no-recovery")):
		_offer_recovery.call_deferred()


func _build_interface() -> void:
	var layer := CanvasLayer.new()
	layer.name = "Interface"
	add_child(layer)
	ui = Control.new()
	ui.name = "Root"
	# The colour scheme follows the preference, and a change of it re-themes the panels at once.
	Kit.set_headset(false)
	Kit.follow_preferences()
	Kit.events.scheme_changed.connect(_on_scheme_changed)
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
	frame.add_child(bottom)
	timeline = TimelinePanel.new(editor)
	bottom_dock.add_child(timeline)

	var tools := ToolsPanel.new(editor)
	tools.text_requested.connect(func() -> void: show_details("Text"))
	left_dock.add_child(tools)
	left_dock.add_child(Kit.separator())
	left_dock.add_child(ColorPanel.new(editor))

	details = TabContainer.new()
	details.name = "Details"
	details.custom_minimum_size = Vector2(0, 330)
	# Its File section opens documents too; a Quill project is framed as File > Open does.
	var document := DocumentPanel.new(editor)
	document.document_opened.connect(func(native: bool) -> void:
		if not native:
			frame_all()
	)
	# Tabs show an icon; the panel's name is the tooltip.
	for entry: Array in [
		[TransformPanel.new(editor), "move"],
		[PropertiesPanel.new(editor), "paint_layer"],
		[document, "new_file"],
		[ExportPanel.new(editor), "export"],
		[MonitorPanel.new(editor), "film"],
		[PerformancePanel.new(editor), "chart"],
		[_settings_panel(), "gear"],
		[TextPanel.new(editor), "text"],
	]:
		var panel: Control = entry[0]
		var index := details.get_tab_count()
		details.add_child(_scrolling(panel))
		details.set_tab_title(index, "")
		details.set_tab_icon(index, Icons.texture(entry[1], Kit.TEXT, 18))
		details.set_tab_tooltip(index, panel.name)
	right_dock.add_child(details)
	# The kind key of a layer row shows that layer's settings; Import shows where from.
	timeline.settings_requested.connect(func(_layer: int) -> void: show_details("Layer"))
	timeline.import_requested.connect(func() -> void:
		show_details("Document")
		document.show_file_part("import")
	)

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


## The colour scheme changed: the theme is made again in the new colours and handed to
## everything that had the old one, and what was built in the old colours follows.
func _on_scheme_changed() -> void:
	Kit.apply_theme(ui)


## The Settings panel. Its "Show button map" opens the sheet of what every control
## of the controllers does, which is worth reading before putting a headset on.
func _settings_panel() -> SettingsPanel:
	var panel := SettingsPanel.new(editor)
	panel.switch_requested.connect(func(to: String) -> void: switch_requested.emit(to))
	panel.button_map_requested.connect(func() -> void:
		var dialog := AcceptDialog.new()
		dialog.title = "Controller buttons"
		dialog.theme = ui.theme
		dialog.add_child(XRButtonMap.new())
		dialog.confirmed.connect(dialog.queue_free)
		dialog.canceled.connect(dialog.queue_free)
		ui.add_child(dialog)
		dialog.popup_centered()
	)
	return panel


## Wraps a panel so that it scrolls when it is taller than the room it is given.
## The wrapper takes the panel's name, which is what a tab shows.
func _scrolling(panel: Control) -> ScrollContainer:
	var scroll := ScrollContainer.new()
	scroll.name = panel.name
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(panel)
	return scroll


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
	file.add_item("Save", FileMenu.SAVE, KEY_MASK_CMD_OR_CTRL | KEY_S)
	file.add_item("Save As…", FileMenu.SAVE_AS, KEY_MASK_CMD_OR_CTRL | KEY_MASK_SHIFT | KEY_S)
	file.add_separator()
	file.add_item("Import…", FileMenu.IMPORT, KEY_MASK_CMD_OR_CTRL | KEY_I)
	file.add_item("Export…", FileMenu.EXPORT, KEY_MASK_CMD_OR_CTRL | KEY_E)
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
	_edit_menu.add_separator()
	_edit_menu.add_item("Select all strokes", EditMenu.SELECT_ALL, KEY_MASK_CMD_OR_CTRL | KEY_A)
	_edit_menu.add_item("Deselect", EditMenu.DESELECT, KEY_MASK_CMD_OR_CTRL | KEY_MASK_SHIFT | KEY_A)
	_edit_menu.add_item("Transform again", EditMenu.TRANSFORM_AGAIN, KEY_MASK_CMD_OR_CTRL | KEY_R)
	_edit_menu.id_pressed.connect(_on_edit_menu)
	menu.add_child(_edit_menu)

	_view_menu = PopupMenu.new()
	_view_menu.name = "View"
	_view_menu.add_item("Frame everything", ViewMenu.FRAME_ALL, KEY_F)
	_view_menu.add_item("Go to initial spawn area", ViewMenu.HOME, KEY_HOME)
	_view_menu.add_separator()
	_view_menu.add_check_item("Floor grid", ViewMenu.GRID, KEY_MASK_SHIFT | KEY_G)
	_view_menu.add_check_item("Gizmos for cameras, lights and spawn areas", ViewMenu.EXTRAS)
	_view_menu.add_check_item("Gimbal", ViewMenu.GIMBAL, KEY_M)
	_view_menu.add_separator()
	_view_menu.add_check_item("Panels", ViewMenu.PANELS, KEY_TAB)
	_view_menu.add_check_item("Full screen", ViewMenu.FULLSCREEN, KEY_MASK_CMD_OR_CTRL | KEY_MASK_SHIFT | KEY_F)
	_view_menu.add_separator()
	_view_menu.add_item(ModeSwitch.TO_HEADSET, ViewMenu.HEADSET)
	_view_menu.id_pressed.connect(_on_view_menu)
	menu.add_child(_view_menu)

	row.add_child(VSeparator.new())
	_undo_button = Kit.icon_button("undo", "Undo", editor.undo)
	_redo_button = Kit.icon_button("redo", "Redo", editor.redo)
	row.add_child(_undo_button)
	row.add_child(_redo_button)
	row.add_child(VSeparator.new())
	_headset_button = Kit.icon_button("headset", ModeSwitch.TO_HEADSET, ask_for_headset)
	row.add_child(_headset_button)
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
	world.show_brush_grid = options.show_brush_grid
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.GRID), options.show_grid)
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.EXTRAS), options.show_extras)
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.GIMBAL), options.show_gimbal)
	_edit_menu.set_item_disabled(_edit_menu.get_item_index(EditMenu.DESELECT), status.selection_count == 0)
	_edit_menu.set_item_disabled(_edit_menu.get_item_index(EditMenu.TRANSFORM_AGAIN), not status.can_transform_again)
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.PANELS), left_dock.is_visible_in_tree())
	_view_menu.set_item_checked(_view_menu.get_item_index(ViewMenu.FULLSCREEN), get_window().mode == Window.MODE_FULLSCREEN)


## Shows a short message at the bottom of the view for a few seconds.
func show_message(text: String, is_error := false) -> void:
	_toast.text = text
	_toast.add_theme_color_override("font_color", Kit.DANGER_TEXT if is_error else Kit.TEXT)
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
			_after_saving_changes(editor.new_document)
		FileMenu.OPEN:
			_after_saving_changes(_choose_document_to_open)
		FileMenu.SAVE:
			save()
		FileMenu.SAVE_AS:
			save_as()
		FileMenu.IMPORT:
			# A project is a folder (or one packed file), so folders can be chosen as well as files.
			_ask_for_path(FileDialog.FILE_MODE_OPEN_ANY, "Import a picture, a sound, a model or another project as a new layer", PackedStringArray(IMPORT_FILTERS), editor.import_file)
		FileMenu.EXPORT:
			show_details("Export")
		FileMenu.QUIT:
			_after_saving_changes(_quit)


func _notification(what: int) -> void:
	if what == NOTIFICATION_WM_CLOSE_REQUEST:
		_after_saving_changes(_quit)


## Takes down what this shell put outside itself, for the headset to take over: the
## camera and the tool cursor in the world, and the hold on the window's close button.
## The shell is freed straight after; whatever it connected to the editor goes with it.
func stop() -> void:
	set_process(false)
	set_process_input(false)
	set_process_unhandled_input(false)
	get_tree().auto_accept_quit = true
	for node: Node3D in [camera, cursor]:
		if node != null and is_instance_valid(node):
			if node.get_parent() != null:
				node.get_parent().remove_child(node)
			node.free()
	camera = null
	cursor = null


## Quits on purpose: whatever is unsaved now was given up knowingly. A copy from last
## time that the recovery question has not been answered for is kept for next time.
func _quit() -> void:
	autosave.quitting()
	get_tree().quit()


func _offer_recovery() -> void:
	var note := Autosave.pending()
	var dialog := ConfirmationDialog.new()
	dialog.title = "Recover unsaved work?"
	dialog.dialog_text = "Limner did not close properly last time.\n\nUnsaved work on \"%s\" from %s was kept. Open it?" % [note.get("title", "Untitled"), str(note.get("time", "")).replace("T", " ")]
	dialog.ok_button_text = "Recover"
	dialog.cancel_button_text = "Not now"
	dialog.add_button("Discard", true, "discard")
	dialog.confirmed.connect(func() -> void:
		if autosave.recover():
			show_message("Recovered. Save it to keep it.")
	)
	dialog.custom_action.connect(func(_name: StringName) -> void:
		autosave.discard()
		dialog.hide()
	)
	# Neither: it is put aside rather than lost.
	dialog.canceled.connect(func() -> void:
		var kept := autosave.set_aside()
		if kept != "":
			show_message("Kept as %s" % kept)
	)
	dialog.visibility_changed.connect(func() -> void:
		if not dialog.visible:
			dialog.queue_free()
	)
	ui.add_child(dialog)
	dialog.popup_centered()


## Shows the file dialog and calls `chosen` with the path picked.
func _ask_for_path(mode: FileDialog.FileMode, title: String, filters: PackedStringArray, chosen: Callable) -> void:
	for selected: Signal in [_file_dialog.file_selected, _file_dialog.dir_selected]:
		for connection: Dictionary in selected.get_connections():
			selected.disconnect(connection.callable)
	_file_dialog.file_mode = mode
	_file_dialog.title = title
	_file_dialog.filters = filters
	_file_dialog.file_selected.connect(chosen, CONNECT_ONE_SHOT)
	if mode == FileDialog.FILE_MODE_OPEN_ANY:
		_file_dialog.dir_selected.connect(chosen, CONNECT_ONE_SHOT)
	_file_dialog.popup_centered_ratio(0.7)


func _choose_document_to_open() -> void:
	_ask_for_path(FileDialog.FILE_MODE_OPEN_ANY, "Open a Limner project or a Quill project folder", PackedStringArray(), _open_path)


func _open_path(path: String) -> void:
	# A file inside a project folder stands for the folder.
	if path.get_file() in ["Quill.json", "Quill.qbin", "State.json", "limn.json"]:
		path = path.get_base_dir()
	if editor.open_document(path):
		# A project saved by Limner reopens with the view it was left with.
		if not editor.path_is_native:
			frame_all()


## Saves to where the document came from, or asks where when it has no place of its own yet.
func save() -> void:
	if editor.path != "" and editor.path_is_native:
		editor.save_document(editor.path, await _thumbnail())
	else:
		save_as()


func save_as() -> void:
	var filters := PackedStringArray(["*.limn ; Limner project (a folder)", "*.limnz ; Limner project packed into one file"])
	_file_dialog.current_file = editor.title() + ".limn"
	_ask_for_path(FileDialog.FILE_MODE_SAVE_FILE, "Save the project", filters, _save_to)


func _save_to(path: String) -> void:
	if path.get_extension().to_lower() not in ["limn", "limnz"]:
		path += ".limn"
	editor.save_document(path, await _thumbnail())


## A small picture of the view without the panels, as PNG data.
func _thumbnail() -> PackedByteArray:
	# Two saves in one frame each take a picture. Only the first hides the panels
	# and only the last shows them again, so that the second does not take "hidden"
	# for how the panels were and leave them that way.
	if _thumbnails_taken == 0:
		_ui_shown_before_thumbnail = ui.visible
		ui.visible = false
	_thumbnails_taken += 1
	await RenderingServer.frame_post_draw
	# Without a display (an automated run) there is no picture to read.
	var image: Image = get_viewport().get_texture().get_image() if LimnerCapture.can_render() else null
	_thumbnails_taken -= 1
	if _thumbnails_taken == 0:
		ui.visible = _ui_shown_before_thumbnail
	if image == null or image.is_empty():
		return PackedByteArray()
	image.resize(512, maxi(1, roundi(512.0 * image.get_height() / image.get_width())), Image.INTERPOLATE_LANCZOS)
	return image.save_png_to_buffer()


## Runs `action` straight away when there is nothing to lose, and otherwise asks first
## whether to save, discard or stay.
func _after_saving_changes(action: Callable) -> void:
	if not world.scene.status().dirty:
		action.call()
		return
	var dialog := ConfirmationDialog.new()
	dialog.title = "Unsaved changes"
	dialog.dialog_text = "Save the changes to %s first?" % editor.title()
	dialog.ok_button_text = "Save"
	dialog.cancel_button_text = "Cancel"
	dialog.add_button("Don't save", true, "discard")
	dialog.confirmed.connect(func() -> void:
		if editor.path != "" and editor.path_is_native:
			if editor.save_document(editor.path, await _thumbnail()):
				action.call()
		else:
			# Nowhere to save to yet: ask, and leave the action for the user to repeat.
			save_as()
	)
	dialog.custom_action.connect(func(_name: StringName) -> void:
		dialog.hide()
		action.call()
	)
	dialog.visibility_changed.connect(func() -> void:
		if not dialog.visible:
			dialog.queue_free()
	)
	ui.add_child(dialog)
	dialog.popup_centered()


func _on_edit_menu(id: int) -> void:
	match id:
		EditMenu.UNDO:
			editor.undo()
		EditMenu.REDO:
			editor.redo()
		EditMenu.RENAME:
			timeline.layer_rows.start_rename()
		EditMenu.DUPLICATE:
			if editor.current_layer() >= 0:
				editor.run({"cmd": "duplicate_layers", "layers": [editor.current_layer()]})
		EditMenu.DELETE:
			if editor.current_layer() >= 0:
				editor.run({"cmd": "delete_layers", "layers": [editor.current_layer()]})
		EditMenu.SELECT_ALL:
			editor.run({"cmd": "select_all"})
		EditMenu.DESELECT:
			editor.run({"cmd": "deselect"})
		EditMenu.TRANSFORM_AGAIN:
			editor.run({"cmd": "transform_again"})


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
		ViewMenu.GIMBAL:
			editor.set_option("show_gimbal", not editor.get_options().show_gimbal)
		ViewMenu.PANELS:
			set_panels_visible(not left_dock.is_visible_in_tree())
		ViewMenu.FULLSCREEN:
			var window := get_window()
			window.mode = Window.MODE_WINDOWED if window.mode == Window.MODE_FULLSCREEN else Window.MODE_FULLSCREEN
		ViewMenu.HEADSET:
			ask_for_headset()
	_refresh()


## Asks the app to carry on in the headset, or says why it cannot.
func ask_for_headset() -> void:
	var reason := ModeSwitch.unavailable_reason()
	if reason != "":
		show_message(reason, true)
		return
	switch_requested.emit("headset")


## The top bar's button and the View menu's entry offer the headset only when there
## is one to change to; otherwise they say why not.
func _refresh_mode_switch() -> void:
	var available := ModeSwitch.unavailable_reason() == ""
	_headset_button.disabled = not available
	_headset_button.tooltip_text = ModeSwitch.headset_tooltip()
	var index := _view_menu.get_item_index(ViewMenu.HEADSET)
	_view_menu.set_item_disabled(index, not available)
	_view_menu.set_item_tooltip(index, ModeSwitch.headset_tooltip())


## Brings one of the tabbed panels under the layer list to the front, by name.
func show_details(panel_name: String) -> void:
	for index in details.get_tab_count():
		if details.get_tab_control(index).name == panel_name:
			details.current_tab = index


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


## A click for every button pressed.
func _input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed and event.button_index == MOUSE_BUTTON_LEFT and get_viewport().gui_get_hovered_control() is BaseButton:
		sounds.play("click")


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
	elif event is InputEventKey and event.pressed:
		_on_key(event)


## Keys that are not menu shortcuts: tools, brush size, the playhead.
func _on_key(event: InputEventKey) -> void:
	if event.ctrl_pressed or event.meta_pressed or event.alt_pressed:
		return
	if TOOL_KEYS.has(event.keycode) and not event.echo and not event.shift_pressed:
		editor.run({"cmd": "set_tool", "tool": TOOL_KEYS[event.keycode]})
		return
	match event.keycode:
		KEY_BRACKETLEFT:
			_scale_tool_size(1.0 / 1.12)
		KEY_BRACKETRIGHT:
			_scale_tool_size(1.12)
		KEY_SPACE:
			if not event.echo:
				editor.run({"cmd": "toggle_play"})
		KEY_ESCAPE:
			# Escape takes back what is half done; with nothing half done, it deselects.
			var status := world.scene.status()
			if status.tool_active or status.grabbing:
				world.scene.input_cancel()
			elif status.selection_count > 0:
				editor.run({"cmd": "deselect"})
		KEY_DELETE, KEY_BACKSPACE:
			if not event.echo and world.scene.status().selection_count > 0:
				editor.run({"cmd": "delete_selection"})
		KEY_LEFT:
			editor.run({"cmd": "step_frames", "frames": -1})
		KEY_RIGHT:
			editor.run({"cmd": "step_frames", "frames": 1})
		KEY_X:
			if not event.echo:
				var tools := editor.get_tools()
				editor.run({"cmd": "set_tool", "tool": tools.previous if tools.tool == "select" else "select"})


func _scale_tool_size(factor: float) -> void:
	# Several wheel steps can arrive in one frame, before the signals of the first
	# have refreshed the settings: each step starts from the size the last one left.
	editor.catch_up()
	var tools := editor.get_tools()
	var tool: String = tools.tool
	editor.set_tool_setting(tool, "size", clampf(tools[tool].size * factor, 0.0005, 0.5))


func _on_mouse_button(event: InputEventMouseButton) -> void:
	match event.button_index:
		MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN:
			if not event.pressed:
				return
			var step := ZOOM_STEP if event.button_index == MOUSE_BUTTON_WHEEL_UP else 1.0 / ZOOM_STEP
			if event.ctrl_pressed or event.meta_pressed:
				# The wheel with Ctrl moves the plane the hand works on, not the view.
				hand.change_depth(step)
			elif event.shift_pressed:
				_scale_tool_size(step)
			else:
				world.scene.view_zoom(_pivot_under_mouse(), step)
		MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE:
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
