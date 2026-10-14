class_name MonitorPanel
extends VBoxContainer
## The Monitor panel: a viewfinder for camera layers. It shows what the chosen
## camera sees, live and in the shape of the document's camera resolution, and
## holds what goes with framing a shot: stepping between cameras, framing lines,
## the resolution, moving the camera by grabbing, looking through it in the main
## view, and saving the frame as a picture.

## The framing lines offered: how many parts each way, and the name shown.
const GRIDS := [[0, "None"], [3, "3 × 3"], [4, "4 × 4"]]

var editor: LimnerEditor
var viewfinder: Viewfinder
## Where "Export current frame" puts its pictures.
var folder := LimnerCapture.default_folder()

var _camera: LayerChoice
var _previous: Button
var _next: Button
var _grid: ChoiceRow
var _resolution: OptionButton
var _resolution_custom: HBoxContainer
var _width: NumberField
var _height: NumberField
var _wants_custom_resolution := false
var _transform_target: CheckBox
var _look_through: CheckBox
var _export: Button
var _with_camera: VBoxContainer
var _without_camera: VBoxContainer
var _result: Label


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Monitor"

	_camera = LayerChoice.new(editor, ["camera"], "", "The camera layer whose view is shown")
	_camera.chosen.connect(func(_layer: int) -> void: _on_camera_chosen())
	_previous = Kit.icon_button("left", "Previous camera", func() -> void: step_camera(-1))
	_next = Kit.icon_button("right", "Next camera", func() -> void: step_camera(1))
	_with_camera = Kit.column([Kit.row([_previous, _camera, _next])])
	add_child(_with_camera)

	viewfinder = Viewfinder.new()
	_with_camera.add_child(viewfinder)

	_grid = ChoiceRow.new()
	for grid: Array in GRIDS:
		_grid.add_text(str(grid[0]), grid[1], "No framing lines" if grid[0] == 0 else "Lines dividing the picture into %d parts each way" % grid[0])
	_grid.set_value("0")
	_grid.chosen.connect(func(key: String) -> void: viewfinder.divisions = int(key))
	_with_camera.add_child(Form.row("Framing", [_grid]))

	var sizes: Array = DocumentPanel.RESOLUTIONS.map(func(size: Array) -> String: return "%d × %d" % size)
	_resolution = Kit.option(sizes + ["Custom…"], "The size of the pictures and video that camera layers render. It belongs to the document.", _on_resolution_chosen)
	_with_camera.add_child(Form.row("Resolution", [_resolution]))
	_width = NumberField.new("Width in pixels, 16 to 16384", 16, 16384, true)
	_height = NumberField.new("Height in pixels, 16 to 16384", 16, 16384, true)
	_width.committed.connect(_on_custom_resolution)
	_height.committed.connect(_on_custom_resolution)
	_resolution_custom = Kit.row([_width, Kit.label("×", true), _height, Kit.label("pixels", true)])
	_with_camera.add_child(_resolution_custom)

	_transform_target = Kit.toggle("Use this camera for transforms", "Grabbing moves this camera, and the Transform panel shows it", _on_transform_target)
	_with_camera.add_child(_transform_target)
	_look_through = Kit.toggle("Look through camera", "Keep the main view behind this camera, with its lens. Navigating is held while this is on.", _on_look_through)
	_with_camera.add_child(_look_through)
	_export = Kit.button("Export current frame", "Save what the camera sees now as a PNG in %s" % folder, export_frame, "image")
	_with_camera.add_child(_export)
	_result = Form.note()
	_with_camera.add_child(_result)

	_without_camera = Kit.column([
		Form.note("The Monitor shows what a camera layer sees. This document has no camera layer yet."),
		Kit.button("Add a camera", "Add a camera layer to the document", func() -> void: editor.run({"cmd": "add_layer", "kind": "camera"}), "camera"),
	])
	add_child(_without_camera)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(_on_document_replaced)
	CameraView.of(editor).changed.connect(refresh)
	refresh()


func _on_document_replaced() -> void:
	_wants_custom_resolution = false
	refresh()


## The camera layer shown, or -1 when the document has none.
func camera_chosen() -> int:
	return _camera.get_layer()


## Brings the panel in line with the document.
func refresh() -> void:
	var cameras := _camera.layers()
	_with_camera.visible = not cameras.is_empty()
	_without_camera.visible = cameras.is_empty()
	_previous.disabled = cameras.size() < 2
	_next.disabled = cameras.size() < 2

	var size: Array = editor.get_info().camera_resolution
	var resolution := Vector2i(int(size[0]), int(size[1]))
	viewfinder.set_shape(resolution)
	var listed := DocumentPanel.RESOLUTIONS.find([resolution.x, resolution.y])
	var custom := DocumentPanel.RESOLUTIONS.size()
	_resolution.set_item_text(custom, "Custom…" if listed >= 0 else "%d × %d" % [resolution.x, resolution.y])
	var custom_size := listed < 0 or _wants_custom_resolution
	Kit.set_quiet(_resolution, custom if custom_size else listed)
	_resolution_custom.visible = custom_size
	_width.set_value(resolution.x)
	_height.set_value(resolution.y)

	var layer := camera_chosen()
	var row := editor.row(layer)
	Kit.set_quiet(_transform_target, not row.is_empty() and row.transform_target)
	Kit.set_quiet(_look_through, layer >= 0 and CameraView.of(editor).following == layer)
	_show_camera()


func _process(_delta: float) -> void:
	# The camera's place in the room changes with every navigation, without a signal.
	if is_visible_in_tree():
		_show_camera()


func _show_camera() -> void:
	var layer := camera_chosen()
	if layer < 0 or editor.row(layer).is_empty():
		viewfinder.show_view({})
	else:
		viewfinder.show_view(LimnerCapture.of(editor).layer_view(layer))


## Goes to the camera `step` places further down the list, around the ends.
func step_camera(step: int) -> void:
	var cameras := _camera.layers()
	if cameras.is_empty():
		return
	var index := cameras.find(camera_chosen())
	_camera.set_layer(cameras[posmod(index + step, cameras.size())])
	_on_camera_chosen()


func _on_camera_chosen() -> void:
	var view := CameraView.of(editor)
	if view.following >= 0 and not view.previewing:
		# Looking through "the camera" means the one shown here.
		view.follow(camera_chosen())
	refresh()


# ---- commands ----

func _set_resolution(width: int, height: int) -> void:
	var size: Array = editor.get_info().camera_resolution
	if width == int(size[0]) and height == int(size[1]):
		refresh()
	elif editor.run({"cmd": "set_document", "camera_resolution": [width, height]}) == null:
		refresh()


func _on_resolution_chosen(index: int) -> void:
	_wants_custom_resolution = index >= DocumentPanel.RESOLUTIONS.size()
	if _wants_custom_resolution:
		refresh()
	else:
		_set_resolution(DocumentPanel.RESOLUTIONS[index][0], DocumentPanel.RESOLUTIONS[index][1])


func _on_custom_resolution(_value: float) -> void:
	_set_resolution(int(_width.get_value()), int(_height.get_value()))


func _on_transform_target(pressed: bool) -> void:
	editor.run({"cmd": "set_transform_target", "layer": camera_chosen() if pressed else null})


func _on_look_through(pressed: bool) -> void:
	var view := CameraView.of(editor)
	if pressed:
		view.follow(camera_chosen())
	else:
		view.stop()


## Saves what the camera sees now as a PNG of the document's camera resolution, in
## the default pictures folder. Returns the path, or an empty string after telling
## the user why there is no file.
func export_frame() -> String:
	var layer := camera_chosen()
	if layer < 0:
		return ""
	if not LimnerCapture.can_render():
		editor.message.emit("Pictures cannot be rendered without a display", true)
		return ""
	_export.disabled = true
	_result.text = "Rendering…"
	await get_tree().process_frame
	await get_tree().process_frame
	var capture := LimnerCapture.of(editor)
	var size: Array = editor.get_info().camera_resolution
	var image := capture.still(capture.layer_view(layer), Vector2i(int(size[0]), int(size[1])), "good")
	var path := folder.path_join(LimnerCapture.stamped_name(editor.title(), "png"))
	var error := LimnerCapture.save_png(image, path)
	_export.disabled = false
	if error != OK:
		_result.text = ""
		editor.message.emit("Could not write %s: %s" % [path, error_string(error)], true)
		return ""
	_result.text = path
	editor.message.emit("Saved %s" % path, false)
	return path
