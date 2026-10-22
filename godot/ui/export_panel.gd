class_name ExportPanel
extends LimnerPanel
## The Export panel: everything that leaves the application as a file. Three
## sections, one shown at a time: a picture, a video through a camera, a 3D file.

## The panel's size in design pixels (see `LimnerPanel`).
const DESIGN_SIZE := Vector2(560, 640)

## The sections: key, name on its button, icon, heading, what it is.
const SECTIONS := [
	["image", "Image", "image", "Capture image", "A still of a view, or a 360 picture"],
	["video", "Video", "film", "Render camera", "A video through a camera layer, rendered in the background"],
	["model", "3D file", "model", "Export 3D file", "The artwork as a file for other 3D programs: glTF, USD, FBX or OBJ"],
]

var editor: LimnerEditor
var image: ExportImageSection
var video: ExportVideoSection
var model: ExportModelSection

var _chooser: ChoiceRow
var _title: Label


func _init(limner_editor: LimnerEditor) -> void:
	super(DESIGN_SIZE)
	editor = limner_editor
	name = "Export"
	_chooser = ChoiceRow.new()
	for section: Array in SECTIONS:
		var button := _chooser.add_text(section[0], section[1], section[4])
		button.icon = Icons.texture(section[2], Kit.TEXT, 18)
	_chooser.chosen.connect(show_section)
	add_child(_chooser)
	_title = Kit.heading("")
	add_child(_title)
	image = ExportImageSection.new(editor)
	video = ExportVideoSection.new(editor)
	model = ExportModelSection.new(editor)
	for section: Control in [image, video, model]:
		add_child(section)
	show_section("image")


## Shows one of the sections: "image", "video" or "model".
func show_section(key: String) -> void:
	_chooser.set_value(key)
	image.visible = key == "image"
	video.visible = key == "video"
	model.visible = key == "model"
	for section: Array in SECTIONS:
		if section[0] == key:
			_title.text = String(section[3]).to_upper()
