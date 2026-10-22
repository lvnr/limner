class_name ExportImageSection
extends VBoxContainer
## "Capture image" in the Export panel: a still of a view, or a 360 picture from a place.

## The kinds of picture, as the key `LimnerCapture` knows them by and the name shown.
const KINDS := [
	["flat", "Flat picture"],
	["equirect", "360 equirectangular"],
	["equirect_stereo", "360 stereo over/under"],
	["cube_strip", "360 cube faces in a row"],
	["cube_cross", "360 cube faces unfolded"],
]
## Widths offered for each kind of 360 picture. Those of a row of cube faces give
## faces of 256, 512 and 1024 pixels.
const WIDTHS := {
	"equirect": [1024, 2048, 4096, 8192],
	"equirect_stereo": [1024, 2048, 4096, 8192],
	"cube_strip": [1536, 3072, 6144],
	"cube_cross": [1024, 2048, 4096, 8192],
}
const QUALITY_NAMES := {"draft": "Draft", "good": "Good", "best": "Best"}

var editor: LimnerEditor

var _kind: Cycler
var _from: LayerChoice
var _size: SizeChoice
var _size_row: Control
var _width: Cycler
var _width_row: Control
var _eyes: NumberField
var _eyes_row: Control
var _quality: ChoiceRow
var _quality_note: Label
var _transparent: CheckBox
var _target: SaveTarget
var _capture: Button
var _result: Label


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Image"

	_kind = Kit.choice(KINDS.map(func(kind: Array) -> String: return kind[1]), "A picture of what a view shows, or of everything around a place", func(_index: int) -> void: refresh())
	add_child(Form.row("Picture", [_kind]))
	_from = LayerChoice.new(editor, ["camera"], "Current view", "Where the picture is taken from")
	add_child(Form.row("From", [_from]))

	_size = SizeChoice.new(editor, LimnerCapture.MAX_SIZE, "The size of the picture, up to 8192 pixels a side")
	_size_row = Form.row("Size", [_size])
	add_child(_size_row)
	_width = Kit.choice([], "The size of the picture; its shape follows from the kind of picture", Callable())
	_width_row = Form.row("Size", [_width])
	add_child(_width_row)
	_eyes = NumberField.new("How far apart the two eyes are, in metres of the room you stand in", 0.0, 1.0)
	_eyes.set_value(LimnerCapture.EYE_SEPARATION)
	_eyes_row = Form.row("Eyes apart", [_eyes, Kit.label("m", true)])
	add_child(_eyes_row)

	_quality = ChoiceRow.new()
	for key: String in QUALITY_NAMES:
		_quality.add_text(key, QUALITY_NAMES[key], LimnerCapture.QUALITY_NOTES[key])
	_quality.set_value("good")
	_quality.chosen.connect(func(_key: String) -> void: refresh())
	add_child(Form.row("Quality", [_quality]))
	_quality_note = Form.note()
	add_child(_quality_note)
	_transparent = Kit.toggle("Transparent background", "Leave the background colour out, so the picture can be laid over another", Callable())
	add_child(_transparent)
	add_child(Kit.separator())

	_target = SaveTarget.new(editor, LimnerCapture.default_folder())
	_target.set_extension("png")
	add_child(_target)
	_capture = Kit.button("Capture", "Render the picture and save it as a PNG", capture, "image")
	add_child(_capture)
	_result = Form.note()
	add_child(_result)
	refresh()


## Shows the controls that apply to the kind of picture chosen.
func refresh() -> void:
	var kind := kind_chosen()
	var flat := kind == "flat"
	_size_row.visible = flat
	_width_row.visible = not flat
	_eyes_row.visible = kind == "equirect_stereo"
	# A 360 picture needs a place more than a lens, so spawn areas serve as well.
	var kinds: Array = ["camera"] if flat else ["camera", "viewpoint"]
	if _from.kinds != kinds:
		_from.set_kinds(kinds)
	if not flat:
		var widths: Array = WIDTHS[kind]
		var chosen := _width.selected if _width.selected >= 0 else 1
		_width.clear()
		for width: int in widths:
			var size := LimnerCapture.panorama_size(kind, width)
			_width.add_item("%d × %d" % [size.x, size.y])
		_width.select(mini(chosen, widths.size() - 1))
	_quality_note.text = LimnerCapture.QUALITY_NOTES[_quality.get_value()]


## The key of the kind of picture chosen.
func kind_chosen() -> String:
	return KINDS[_kind.selected][0]


## What pressing Capture asks for: `{kind, layer (-1 for the current view), size,
## quality, transparent, eye_separation}`. For a 360 picture only the width of
## `size` counts.
func request() -> Dictionary:
	var kind := kind_chosen()
	var size := _size.get_size_chosen()
	if kind != "flat":
		size = LimnerCapture.panorama_size(kind, WIDTHS[kind][_width.selected])
	return {
		"kind": kind,
		"layer": _from.get_layer(),
		"size": size,
		"quality": _quality.get_value(),
		"transparent": _transparent.button_pressed,
		"eye_separation": _eyes.get_value(),
	}


## Renders the picture and saves it. Returns the path of the file, or an empty
## string after telling the user why there is none.
func capture() -> String:
	if not LimnerCapture.can_render():
		editor.message.emit("Pictures cannot be rendered without a display", true)
		return ""
	var asked := request()
	_capture.disabled = true
	_result.text = "Rendering…"
	# The window stands still while a picture renders. Let it show that first.
	await get_tree().process_frame
	await get_tree().process_frame
	var started := Time.get_ticks_msec()
	var path := _target.path("png")
	var image := take(LimnerCapture.of(editor), asked)
	var error := LimnerCapture.save_png(image, path)
	_capture.disabled = false
	if error != OK:
		_result.text = ""
		editor.message.emit("Could not write %s: %s" % [path, error_string(error)], true)
		return ""
	_result.text = "%d × %d pixels in %s s\n%s" % [image.get_width(), image.get_height(), String.num((Time.get_ticks_msec() - started) / 1000.0, 1), path]
	editor.message.emit("Saved %s" % path, false)
	return path


## The picture a request describes.
static func take(capture: LimnerCapture, asked: Dictionary) -> Image:
	var layer: int = asked.layer
	var view := capture.current_view() if layer < 0 else capture.layer_view(layer)
	if asked.kind == "flat":
		return capture.still(view, asked.size, asked.quality, asked.transparent)
	if layer < 0:
		view = LimnerCapture.level(view)
	var size: Vector2i = asked.size
	return capture.panorama(view, asked.kind, size.x, asked.quality, asked.transparent, asked.eye_separation)
