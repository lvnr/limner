class_name DocumentPanel
extends VBoxContainer
## The Document panel: the file the document is kept in, the settings that belong to
## the whole document, and how much is in it.
##
## Its File section saves, saves as, starts, opens and imports without any system
## dialog, in the documents folder (see `Documents`), so that it works in a headset as
## on the desktop. Questions (save the changes first? replace that file?) are asked in
## the panel itself.

## A document was opened from the panel. `native` is false for a Quill project,
## which the shell may want to frame, as it has no view of its own saved.
signal document_opened(native: bool)

const RATES := [6, 8, 10, 12, 15, 24, 25, 30, 48, 50, 60]
const RESOLUTIONS := [[1920, 1080], [1080, 1920], [1024, 1024], [2048, 2048], [3840, 2160], [4096, 4096], [7680, 4320]]
## The most entries the Open and Import lists show, newest first.
const LIST_LIMIT := 30
## The icon of each kind of entry in those lists.
const ENTRY_ICONS := {"limnz": "new_file", "limn": "folder_open", "quill": "folder_open", "picture": "image", "sound": "sound", "model": "model"}

var editor: LimnerEditor

## What the part under the File buttons shows: "" (nothing), "save_as", "confirm"
## (save the changes first?), "open" or "import".
var file_mode := ""
var _file_note: Label
var _file_buttons := {}
var _drawer: VBoxContainer
## What to do once unsaved changes have been saved or given up.
var _pending := Callable()
var _save_name: TextField
var _save_note: Label
var _save_button: Button
## True once the user has been told that the name typed is taken, so that a second
## press replaces that file.
var _replace := false

var _title: TextField
var _description: TextEdit
var _background: ColorField
var _ambient: ColorField
var _rate: OptionButton
var _rate_custom: HBoxContainer
var _rate_num: NumberField
var _rate_den: NumberField
## Says why a half-typed custom rate has not been applied yet.
var _rate_note: Label
## True once the user has asked for a frame rate or a resolution that is not in
## the list, so that the boxes for it stay open while they type.
var _wants_custom_rate := false
var _wants_custom_resolution := false
## Start and end of the working range, each as a box for seconds and one for frames.
var _range_seconds := {}
var _range_frames := {}
var _resolution: OptionButton
var _resolution_custom: HBoxContainer
var _width: NumberField
var _height: NumberField
var _contents: Label


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Document"

	add_child(Kit.heading("File"))
	_file_note = Form.note()
	add_child(_file_note)
	var actions := HFlowContainer.new()
	for entry: Array in [
		["save", "Save", "save", "Saves the document where it was last saved. The first time, it asks for a name.", save],
		["save_as", "Save as", "save", "Saves the document under a new name in the documents folder", _choose.bind("save_as")],
		["new", "New", "new_file", "Starts a new, empty document", start_new],
		["open", "Open", "folder_open", "Opens a project from the documents folder", _choose.bind("open")],
		["import", "Import", "import", "Brings a picture, a sound, a model or another project from the documents folder in as a new layer", _choose.bind("import")],
	]:
		var button := Kit.button(entry[1], entry[3], entry[4], entry[2])
		_file_buttons[entry[0]] = button
		actions.add_child(button)
	add_child(actions)
	_drawer = Kit.column()
	_drawer.visible = false
	add_child(_drawer)
	add_child(Kit.separator())

	_title = TextField.new("Untitled", "The name of the piece")
	_title.committed.connect(func(text: String) -> void: _write({"title": text}))
	add_child(Form.row("Title", [_title]))

	add_child(Kit.label("Description"))
	_description = TextEdit.new()
	_description.tooltip_text = "A few words about the piece. Kept when you leave the box."
	_description.custom_minimum_size = Vector2(0, 84)
	_description.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
	_description.focus_exited.connect(_on_description_left)
	add_child(_description)

	_background = ColorField.new("The colour behind everything")
	_background.edit_started.connect(func() -> void: editor.begin("Change background"))
	_background.changed.connect(func(color: Color) -> void: _write({"background": LimnerEditor.linear_rgb(color)}))
	_background.edit_ended.connect(editor.commit)
	add_child(Form.row("Background", [_background]))
	_ambient = ColorField.new("The light that reaches lit paint and models from everywhere: how bright they are where no light layer reaches. Black leaves only the lights")
	_ambient.edit_started.connect(func() -> void: editor.begin("Change ambient light"))
	_ambient.changed.connect(func(color: Color) -> void: _write({"ambient_light": LimnerEditor.linear_rgb(color)}))
	_ambient.edit_ended.connect(editor.commit)
	add_child(Form.row("Ambient light", [_ambient]))
	add_child(Kit.separator())

	var rates: Array = RATES.map(func(rate: int) -> String: return "%d fps" % rate)
	_rate = Kit.option(rates + ["Custom…"], "How many frames play each second, for every animated layer", _on_rate_chosen)
	add_child(Form.row("Frame rate", [_rate]))
	_rate_num = NumberField.new("How many frames play in the seconds beside this", 1, 480000, true)
	_rate_den = NumberField.new("How many seconds those frames take. 24000 frames in 1001 seconds is the 23.976 of television.", 1, 100000, true)
	_rate_num.committed.connect(_on_custom_rate)
	_rate_den.committed.connect(_on_custom_rate)
	_rate_custom = Kit.row([_rate_num, Kit.label("frames in", true), _rate_den, Kit.label("s", true)])
	add_child(_rate_custom)
	_rate_note = Form.note()
	add_child(_rate_note)

	add_child(Kit.heading("Working range"))
	var times := Form.time_table()
	_add_range_row(times, "Start", "range_start", "Where the part being worked on begins")
	_add_range_row(times, "End", "range_end", "Where the part being worked on ends; it must be after the start")
	add_child(times)

	add_child(Kit.heading("Camera resolution"))
	var sizes: Array = RESOLUTIONS.map(func(size: Array) -> String: return "%d × %d" % size)
	_resolution = Kit.option(sizes + ["Custom…"], "The size of the pictures and video that camera layers render", _on_resolution_chosen)
	add_child(_resolution)
	_width = NumberField.new("Width in pixels, 16 to 16384", 16, 16384, true)
	_height = NumberField.new("Height in pixels, 16 to 16384", 16, 16384, true)
	_width.committed.connect(_on_custom_resolution)
	_height.committed.connect(_on_custom_resolution)
	_resolution_custom = Kit.row([_width, Kit.label("×", true), _height, Kit.label("pixels", true)])
	add_child(_resolution_custom)
	add_child(Kit.separator())

	add_child(Kit.heading("Contents"))
	_contents = Form.note()
	add_child(_contents)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.document_replaced.connect(_on_document_replaced)
	# Saving changes no part of the document, only whether it has unsaved changes.
	editor.state_changed.connect(_show_file)
	refresh()


func _on_document_replaced() -> void:
	_wants_custom_rate = false
	_wants_custom_resolution = false
	# A question or a list about the document that was there is no longer to the point.
	_pending = Callable()
	show_file_part("")
	refresh()


## Brings the panel in line with the document.
func refresh() -> void:
	_show_file()
	var info := editor.get_info()
	_title.set_value(info.title)
	if not _description.has_focus() and _description.text != info.description:
		_description.text = info.description
	_background.set_value(LimnerEditor.screen_color(info.background))
	_ambient.set_value(LimnerEditor.screen_color(info.ambient_light))

	var num := int(info.rate.num)
	var den := int(info.rate.den)
	var preset := RATES.find(num) if den == 1 else -1
	var custom := RATES.size()
	_rate.set_item_text(custom, "Custom…" if preset >= 0 else "%s fps" % String.num(float(num) / den, 3))
	var custom_rate := preset < 0 or _wants_custom_rate
	Kit.set_quiet(_rate, custom if custom_rate else preset)
	_rate_custom.visible = custom_rate
	_rate_note.visible = false
	_rate_num.set_value(num)
	_rate_den.set_value(den)

	var frame := float(info.frame_duration)
	for key: String in _range_seconds:
		_range_seconds[key].set_value(LimnerEditor.to_seconds(int(info[key])))
		_range_frames[key].set_value(float(info[key]) / frame)

	var size: Array = info.camera_resolution
	var listed := RESOLUTIONS.find([int(size[0]), int(size[1])])
	_resolution.set_item_text(RESOLUTIONS.size(), "Custom…" if listed >= 0 else "%d × %d" % [int(size[0]), int(size[1])])
	var custom_size := listed < 0 or _wants_custom_resolution
	Kit.set_quiet(_resolution, RESOLUTIONS.size() if custom_size else listed)
	_resolution_custom.visible = custom_size
	_width.set_value(size[0])
	_height.set_value(size[1])

	_contents.text = "%s layers, %s strokes, %s points" % [Form.thousands(int(info.layers)), Form.thousands(int(info.strokes)), Form.thousands(int(info.points))]


## One end of the working range: a box for seconds and a box for frames.
func _add_range_row(table: GridContainer, text: String, key: String, tooltip: String) -> void:
	var seconds := NumberField.new("%s, in seconds" % tooltip, 0.0, 1e6)
	seconds.committed.connect(func(value: float) -> void: _set_time(key, LimnerEditor.to_flicks(value)))
	var frames := NumberField.new("%s, in frames" % tooltip, 0.0, 1e9)
	frames.committed.connect(func(value: float) -> void: _set_time(key, roundi(value * float(editor.get_info().frame_duration))))
	_range_seconds[key] = seconds
	_range_frames[key] = frames
	Form.time_row(table, text, seconds, frames, tooltip)


# ---- commands ----

## Changes document settings. When the document refuses, the panel goes back to
## what it holds; the reason reaches the user through the editor's message.
func _write(fields: Dictionary) -> void:
	var command := {"cmd": "set_document"}
	command.merge(fields)
	if editor.run(command) == null:
		refresh()


func _set_time(key: String, flicks: int) -> void:
	if flicks == int(editor.get_info()[key]):
		refresh()
	else:
		_write({key: flicks})


func _on_description_left() -> void:
	if _description.text != editor.get_info().description:
		_write({"description": _description.text})


func _set_rate(num: int, den: int) -> void:
	var rate: Dictionary = editor.get_info().rate
	if num == int(rate.num) and den == int(rate.den):
		refresh()
	else:
		_write({"rate": {"num": num, "den": den}})


func _on_rate_chosen(index: int) -> void:
	_wants_custom_rate = index >= RATES.size()
	if _wants_custom_rate:
		refresh()
	else:
		_set_rate(RATES[index], 1)


func _on_custom_rate(_value: float) -> void:
	var num := int(_rate_num.get_value())
	var den := int(_rate_den.get_value())
	var fps := float(num) / den
	if fps < 1.0 or fps > 480.0:
		# Half-typed, as 24000 is until its 1001 follows: wait for the other number.
		_rate_note.text = "%d frames in %d s is %s fps. Nothing changes until the rate is between 1 and 480." % [num, den, NumberField.format(fps)]
		_rate_note.visible = true
		return
	_set_rate(num, den)


func _set_resolution(width: int, height: int) -> void:
	var size: Array = editor.get_info().camera_resolution
	if width == int(size[0]) and height == int(size[1]):
		refresh()
	else:
		_write({"camera_resolution": [width, height]})


func _on_resolution_chosen(index: int) -> void:
	_wants_custom_resolution = index >= RESOLUTIONS.size()
	if _wants_custom_resolution:
		refresh()
	else:
		_set_resolution(RESOLUTIONS[index][0], RESOLUTIONS[index][1])


func _on_custom_resolution(_value: float) -> void:
	_set_resolution(int(_width.get_value()), int(_height.get_value()))


# ---- the file ----

## Saves to the file the document came from, or asks for a name in the panel when it
## has none of its own yet (a new document, or a Quill project). Returns whether it saved.
func save() -> bool:
	if editor.path != "" and editor.path_is_native:
		return editor.save_document(editor.path, Documents.thumbnail(editor))
	_choose("save_as")
	return false


## Saves the document as `title`.limnz in the documents folder, which is made if it
## is missing. A file of that name already there is replaced only when asked twice.
## Then does what was waiting for the changes to be saved, if anything was. Returns
## whether it saved.
func save_as(title: String) -> bool:
	var target := Documents.path_for(title)
	if target == "":
		_tell_save("Type a name for the file first.")
		return false
	if FileAccess.file_exists(target) and target != editor.path and not _replace:
		_replace = true
		_tell_save("There is already a project called %s there. Press Replace to save over it." % target.get_file().get_basename(), "Replace")
		return false
	DirAccess.make_dir_recursive_absolute(target.get_base_dir())
	if not editor.save_document(target, Documents.thumbnail(editor)):
		return false
	_continue()
	return true


## Starts a new, empty document, first asking whether to save unsaved changes.
func start_new() -> void:
	_after_saving_changes(editor.new_document)


## Opens the project at `path`, first asking whether to save unsaved changes.
func open_project(path: String) -> void:
	_after_saving_changes(func() -> void:
		if editor.open_document(path):
			document_opened.emit(editor.path_is_native)
	)


## Brings the file at `path` in as a new layer. Returns the new layer's id, or -1.
func import_entry(path: String) -> int:
	var layer := editor.import_file(path)
	if layer >= 0:
		show_file_part("")
		editor.message.emit("Imported %s" % path.get_file(), false)
	return layer


## Shows what a file action needs under the File buttons: "save_as" (a name),
## "confirm" (save the changes first?), "open" or "import" (the documents folder's
## list), or "" for nothing.
func show_file_part(mode: String) -> void:
	file_mode = mode
	# What was there goes at the end of the frame: the button pressed to get here may
	# be among it, still in the middle of reporting the press.
	for child: Node in _drawer.get_children():
		(child as Control).visible = false
		child.queue_free()
	_save_name = null
	_save_note = null
	_save_button = null
	_replace = false
	_drawer.visible = mode != ""
	match mode:
		"save_as":
			_build_save_as()
		"confirm":
			_build_confirm()
		"open":
			_build_list(Documents.projects(), "No projects in %s yet. Projects saved from this panel go there.", open_project)
		"import":
			_build_list(Documents.importable(), "Nothing to import in %s. Pictures (PNG, JPEG), sounds (WAV, Ogg, MP3), models (OBJ, glTF, FBX) and projects put there can be brought in.", import_entry)


## A File button: shows its part, or puts it away when it is showing already. What
## was waiting for an answer about unsaved changes is given up.
func _choose(mode: String) -> void:
	_pending = Callable()
	show_file_part("" if file_mode == mode and mode != "save_as" else mode)


func _show_file() -> void:
	var dirty: bool = editor.scene.status().get("dirty", false)
	if editor.path == "":
		_file_note.text = "Not saved yet." if dirty or int(editor.get_info().get("strokes", 0)) > 0 else "A new document, not saved yet."
	elif editor.path_is_native:
		_file_note.text = "Kept in %s%s." % [_short(editor.path), ", with unsaved changes" if dirty else ""]
	else:
		_file_note.text = "Opened from the Quill project %s. Save as keeps it as a Limner project." % editor.path.get_file()
	_file_note.tooltip_text = editor.path
	_file_note.mouse_filter = Control.MOUSE_FILTER_PASS


## The last folder and the file: enough to recognise, short enough for a narrow panel.
static func _short(path: String) -> String:
	return path.get_base_dir().get_file().path_join(path.get_file())


## Runs `action` at once when nothing is unsaved, and otherwise asks first.
func _after_saving_changes(action: Callable) -> void:
	if not editor.scene.status().get("dirty", false):
		_pending = Callable()
		show_file_part("")
		action.call()
		return
	_pending = action
	show_file_part("confirm")


## Puts the File part away and does what was waiting, if anything.
func _continue() -> void:
	var action := _pending
	_pending = Callable()
	show_file_part("")
	if action.is_valid():
		action.call()


func _cancel() -> void:
	_pending = Callable()
	show_file_part("")


## Saves the changes to the document's own file and goes on; without a file of its
## own, asks for a name first and goes on once it is saved.
func _save_then_continue() -> void:
	if editor.path != "" and editor.path_is_native:
		if editor.save_document(editor.path, Documents.thumbnail(editor)):
			_continue()
	else:
		var waiting := _pending
		show_file_part("save_as")
		_pending = waiting


func _build_save_as() -> void:
	_save_name = TextField.new("Name", "The name of the project file. It is saved in the documents folder.")
	_save_name.select_all_on_focus = true
	_save_name.set_value(editor.title())
	_save_name.text_changed.connect(func(_text: String) -> void:
		_replace = false
		_show_save_target()
	)
	_save_name.text_submitted.connect(func(text: String) -> void: save_as(text))
	_drawer.add_child(Form.row("Name", [_save_name]))
	_save_note = Form.note()
	_drawer.add_child(_save_note)
	_save_button = Kit.button("Save", "Saves the project under this name", func() -> void: save_as(_save_name.text), "save")
	_save_button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var cancel := Kit.button("Cancel", "Leaves the document as it is", _cancel)
	cancel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_drawer.add_child(Kit.row([_save_button, cancel]))
	_show_save_target()


## Says which file the name typed becomes.
func _show_save_target() -> void:
	if _save_name == null:
		return
	var target := Documents.path_for(_save_name.text)
	if target == "":
		_tell_save("Type a name for the file.")
	else:
		_tell_save("Saved as %s in %s." % [target.get_file(), _short(target.get_base_dir())])
		_save_note.tooltip_text = target
		_save_note.mouse_filter = Control.MOUSE_FILTER_PASS


func _tell_save(text: String, button_text := "Save") -> void:
	if _save_note != null:
		_save_note.text = text
	if _save_button != null:
		_save_button.text = button_text


func _build_confirm() -> void:
	_drawer.add_child(Form.note("Save the changes to %s first?" % editor.title()))
	var buttons := HFlowContainer.new()
	buttons.add_child(Kit.button("Save", "Saves the changes, then goes on", _save_then_continue, "save"))
	buttons.add_child(Kit.button("Don't save", "Gives the changes up and goes on", _continue))
	buttons.add_child(Kit.button("Cancel", "Stays with the document as it is", _cancel))
	_drawer.add_child(buttons)


## One button for each entry, newest first, with its date beside it. `empty` says
## what to do when there is none; `%s` in it is the folder.
func _build_list(entries: Array[Dictionary], empty: String, chosen: Callable) -> void:
	var where := _short(Documents.folder())
	var heading := Form.note(empty % where if entries.is_empty() else "In %s, newest first:" % where)
	heading.tooltip_text = Documents.folder()
	heading.mouse_filter = Control.MOUSE_FILTER_PASS
	_drawer.add_child(heading)
	for entry: Dictionary in entries.slice(0, LIST_LIMIT):
		var path: String = entry.path
		var label := str(entry.name)
		if entry.kind == "quill":
			label += " (Quill)"
		var button := Kit.button(label, path, chosen.bind(path), ENTRY_ICONS.get(entry.kind, "new_file"))
		button.alignment = HORIZONTAL_ALIGNMENT_LEFT
		button.clip_text = true
		button.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
		button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		button.custom_minimum_size.x = 40
		var when := Kit.label(Documents.local_time(int(entry.time)), true)
		when.add_theme_font_size_override("font_size", 12)
		_drawer.add_child(Kit.row([button, when]))
	if entries.size() > LIST_LIMIT:
		_drawer.add_child(Form.note("And %d older, not shown." % (entries.size() - LIST_LIMIT)))
	_drawer.add_child(Kit.button("Close", "Puts the list away", _cancel))
