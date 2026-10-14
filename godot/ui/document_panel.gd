class_name DocumentPanel
extends VBoxContainer
## The Document panel: the settings that belong to the whole document, and how much
## is in it.

const RATES := [6, 8, 10, 12, 15, 24, 25, 30, 48, 50, 60]
const RESOLUTIONS := [[1920, 1080], [1080, 1920], [1024, 1024], [2048, 2048], [3840, 2160], [4096, 4096], [7680, 4320]]

var editor: LimnerEditor

var _title: TextField
var _description: TextEdit
var _background: ColorField
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
	refresh()


func _on_document_replaced() -> void:
	_wants_custom_rate = false
	_wants_custom_resolution = false
	refresh()


## Brings the panel in line with the document.
func refresh() -> void:
	var info := editor.get_info()
	_title.set_value(info.title)
	if not _description.has_focus() and _description.text != info.description:
		_description.text = info.description
	_background.set_value(LimnerEditor.screen_color(info.background))

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
