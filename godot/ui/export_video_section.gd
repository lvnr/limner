class_name ExportVideoSection
extends VBoxContainer
## "Render camera" in the Export panel: a camera's view of a range of the timeline
## as a video, rendered by a second copy of the application while work goes on.

const PROJECTIONS := [
	["flat", "2D"],
	["equirect", "360"],
	["equirect_stereo", "360 stereo over/under"],
]

var editor: LimnerEditor
var launcher: RenderLauncher

var _camera: LayerChoice
var _projection: OptionButton
var _range: ChoiceRow
var _times: GridContainer
var _start_seconds: NumberField
var _start_frames: NumberField
var _end_seconds: NumberField
var _end_frames: NumberField
var _rate: OptionButton
var _size: SizeChoice
var _format: OptionButton
var _format_note: Label
var _quality: ChoiceRow
var _quality_note: Label
var _sound: CheckBox
var _target: SaveTarget
var _preview: Button
var _render: Button
var _progress: ProgressBar
var _progress_text: Label
var _progress_row: Control
var _result: Label
## The custom range, in seconds.
var _start := 0.0
var _end := 1.0


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Video"
	launcher = RenderLauncher.new()
	launcher.name = "Launcher"
	launcher.progressed.connect(_on_progress)
	launcher.finished.connect(_on_finished)
	add_child(launcher)

	_camera = LayerChoice.new(editor, ["camera"], "Current view", "The camera layer the video is seen through, or the view as it is now")
	add_child(Form.row("Camera", [_camera]))
	_projection = Kit.fitted_option(PROJECTIONS.map(func(entry: Array) -> String: return entry[1]), "A flat picture through the camera's lens, or everything around the camera", func(_index: int) -> void: refresh())
	add_child(Form.row("Picture", [_projection]))

	_range = ChoiceRow.new()
	_range.add_text("document", "Document", "Render the working range of the document")
	_range.add_text("custom", "Custom", "Render from one time to another")
	_range.set_value("document")
	_range.chosen.connect(func(_key: String) -> void: refresh())
	add_child(Form.row("Range", [_range]))
	_times = Form.time_table()
	_start_seconds = NumberField.new("Where the video starts, in seconds", 0.0, 1e6)
	_start_frames = NumberField.new("Where the video starts, in frames of the video", 0.0, 1e9)
	_end_seconds = NumberField.new("Where the video ends, in seconds", 0.0, 1e6)
	_end_frames = NumberField.new("Where the video ends, in frames of the video", 0.0, 1e9)
	_start_seconds.committed.connect(func(value: float) -> void: _set_range(value, _end))
	_start_frames.committed.connect(func(value: float) -> void: _set_range(value / rate_chosen(), _end))
	_end_seconds.committed.connect(func(value: float) -> void: _set_range(_start, value))
	_end_frames.committed.connect(func(value: float) -> void: _set_range(_start, value / rate_chosen()))
	Form.time_row(_times, "Start", _start_seconds, _start_frames)
	Form.time_row(_times, "End", _end_seconds, _end_frames)
	add_child(_times)

	var rates: Array = DocumentPanel.RATES.map(func(rate: int) -> String: return "%d fps" % rate)
	_rate = Kit.fitted_option(["Document"] + rates, "How many frames the video has each second", func(_index: int) -> void: refresh())
	add_child(Form.row("Frame rate", [_rate]))
	_size = SizeChoice.new(editor, LimnerCapture.MAX_SIZE, "The size of the video's frames. For a 360 video the width counts and the shape follows.")
	add_child(Form.row("Size", [_size]))

	var formats: Array = RenderLauncher.FORMATS.values()
	_format = Kit.fitted_option(formats, "The kind of file the video is written as", func(_index: int) -> void: refresh())
	add_child(Form.row("Format", [_format]))
	_format_note = Form.note()
	add_child(_format_note)
	_quality = ChoiceRow.new()
	for key: String in ExportImageSection.QUALITY_NAMES:
		_quality.add_text(key, ExportImageSection.QUALITY_NAMES[key], LimnerCapture.QUALITY_NOTES[key])
	_quality.set_value("good")
	_quality.chosen.connect(func(_key: String) -> void: refresh())
	add_child(Form.row("Quality", [_quality]))
	_quality_note = Form.note()
	add_child(_quality_note)
	_sound = Kit.toggle("With sound", "Record the document's sound with the picture", Callable())
	_sound.set_pressed_no_signal(true)
	add_child(_sound)
	add_child(Kit.separator())

	_target = SaveTarget.new(editor, OS.get_system_dir(OS.SYSTEM_DIR_MOVIES).path_join("Limner"))
	add_child(_target)
	_preview = Kit.button("Preview", "Play the range in a loop, seen through the camera", Callable(), "play")
	_preview.toggle_mode = true
	_preview.toggled.connect(_on_preview)
	_preview.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_render = Kit.button("Render", "Render the video in the background", render, "film")
	_render.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	add_child(Kit.row([_preview, _render]))

	_progress = ProgressBar.new()
	_progress.custom_minimum_size = Vector2(60, Kit.ROW)
	_progress.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_progress.show_percentage = false
	_progress_text = Kit.label("", true)
	_progress_row = Kit.column([_progress_text, Kit.row([_progress, Kit.button("Cancel", "Stop the render; what was rendered so far is kept", launcher.cancel)])])
	_progress_row.visible = false
	add_child(_progress_row)
	_result = Form.note("The video shows the document as it is when Render is pressed. You can keep working while it renders; later changes are not in it.")
	add_child(_result)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.document_replaced.connect(_on_document_replaced)
	CameraView.of(editor).changed.connect(_show_preview_state)
	_on_document_replaced()


func _on_document_replaced() -> void:
	var info := editor.get_info()
	_start = LimnerEditor.to_seconds(int(info.range_start))
	_end = LimnerEditor.to_seconds(int(info.range_end))
	refresh()


## Brings the controls in line with each other and with the document.
func refresh() -> void:
	var info := editor.get_info()
	_rate.set_item_text(0, "Document (%s fps)" % NumberField.format(float(info.fps)))
	var custom := _range.get_value() == "custom"
	_times.visible = custom
	var fps := rate_chosen()
	_start_seconds.set_value(_start)
	_start_frames.set_value(_start * fps)
	_end_seconds.set_value(_end)
	_end_frames.set_value(_end * fps)
	var format: String = RenderLauncher.FORMATS.keys()[_format.selected]
	_format_note.text = RenderLauncher.FORMAT_NOTES[format]
	_target.set_extension(format)
	_quality_note.text = "%s Each frame is rendered that way." % LimnerCapture.QUALITY_NOTES[_quality.get_value()]


## Frames per second of the video.
func rate_chosen() -> float:
	if _rate.selected <= 0:
		return float(editor.get_info().fps)
	return float(DocumentPanel.RATES[_rate.selected - 1])


## The range that will be rendered, in seconds: `[start, end]`.
func range_chosen() -> Array:
	if _range.get_value() == "custom":
		return [_start, _end]
	var info := editor.get_info()
	return [LimnerEditor.to_seconds(int(info.range_start)), LimnerEditor.to_seconds(int(info.range_end))]


func _set_range(start: float, end: float) -> void:
	if end <= start:
		editor.message.emit("The range must end after it starts", true)
	else:
		_start = start
		_end = end
	refresh()


## What pressing Render asks the launcher for; see `RenderLauncher.command_line`.
func settings() -> Dictionary:
	var projection: String = PROJECTIONS[_projection.selected][0]
	var size := _size.get_size_chosen()
	if projection != "flat":
		size = LimnerCapture.panorama_size(projection, size.x)
	var layer := _camera.get_layer()
	return {
		"output": _target.path(RenderLauncher.FORMATS.keys()[_format.selected]),
		"size": size,
		"fps": rate_chosen(),
		"quality": _quality.get_value(),
		"projection": projection,
		"sound": _sound.button_pressed,
		"camera": "view" if layer < 0 else str(layer),
		"range": range_chosen(),
		"minimized": true,
	}


## Starts the render.
func render() -> void:
	var problem := launcher.render_document(editor, settings())
	if problem != "":
		editor.message.emit(problem, true)
		return
	_render.disabled = true
	_progress.value = 0.0
	_progress_text.text = "Starting…"
	_progress_row.visible = true
	_result.text = "Rendering the document as it was when Render was pressed. You can keep working; later changes are not in the video."


func _on_progress(frame: int, frames: int) -> void:
	_progress.max_value = maxi(frames, 1)
	_progress.value = frame
	_progress_text.text = "Frame %d of %d" % [frame, frames]


func _on_finished(ok: bool, text: String) -> void:
	_render.disabled = false
	_progress_row.visible = false
	_result.text = text
	editor.message.emit(text, not ok)


func _on_preview(pressed: bool) -> void:
	var view := CameraView.of(editor)
	if pressed:
		var span := range_chosen()
		view.preview(_camera.get_layer(), span[0], span[1])
	else:
		view.stop()


func _show_preview_state() -> void:
	_preview.set_pressed_no_signal(CameraView.of(editor).previewing)
