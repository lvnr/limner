class_name ExportModelSection
extends VBoxContainer
## "3D file" in the Export panel: the artwork as a file other 3D programs read.

## The formats: file ending, name, and what each is good for (from docs/export.md).
const FORMATS := [
	["glb", "glTF (.glb)", "Layers, animation, lights and cameras in one compact file. The first choice wherever it can be read: Blender, Godot, Unity and Unreal with their glTF importers."],
	["usda", "USD text (.usda)", "The same content for USD pipelines, as text that can be read and compared. Large."],
	["usdz", "USD package (.usdz)", "The USD file packed into one, as Apple's tools and AR viewers take it."],
	["fbx", "FBX (.fbx)", "For tools that want FBX. Materials have to be set up after import: FBX cannot say unlit, two-sided or see-through."],
	["obj", "OBJ (.obj)", "A still mesh with vertex colours and nothing else: no layers, no animation, no see-through strokes."],
]

var editor: LimnerEditor

var _format: OptionButton
var _format_note: Label
var _moving: ChoiceRow
var _moving_row: Control
var _moving_note: Label
var _hidden: CheckBox
var _bake: CheckBox
var _lights: CheckBox
var _cameras: CheckBox
var _fbx_unit: ChoiceRow
var _fbx_colors: ChoiceRow
var _fbx_rows: Control
var _gltf_visibility: CheckBox
var _gltf_opacity: CheckBox
var _gltf_rows: Control
var _target: SaveTarget
var _export: Button
var _report: Label


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Model"

	_format = Kit.fitted_option(FORMATS.map(func(format: Array) -> String: return format[1]), "The kind of file to write", func(_index: int) -> void: refresh())
	add_child(Form.row("Format", [_format]))
	_format_note = Form.note()
	add_child(_format_note)

	_moving = ChoiceRow.new()
	_moving.add_text("still", "Still", "The artwork as it is at the playhead")
	_moving.add_text("animated", "Animated", "The working range, sampled at the document's frame rate")
	_moving.set_value("still")
	_moving.chosen.connect(func(_key: String) -> void: refresh())
	_moving_row = Form.row("Export", [_moving])
	add_child(_moving_row)
	_moving_note = Form.note()
	add_child(_moving_note)

	_hidden = Kit.toggle("Hidden layers too", "Include layers whose eye is switched off", Callable())
	_bake = Kit.toggle("Bake transforms", "Flatten the layer tree: every mesh is written where it is in the world, without parents", Callable())
	_lights = Kit.toggle("Lights", "Include light layers", Callable())
	_cameras = Kit.toggle("Cameras", "Include camera layers", Callable())
	_lights.set_pressed_no_signal(true)
	_cameras.set_pressed_no_signal(true)
	for toggle: CheckBox in [_hidden, _bake, _lights, _cameras]:
		add_child(toggle)

	_fbx_unit = ChoiceRow.new()
	_fbx_unit.add_text("metres", "Metres", "Lengths stay in metres and the file says so. Right for Blender, Unity and Godot.")
	_fbx_unit.add_text("centimetres", "Centimetres", "Lengths are written in centimetres, for importers that ignore the unit a file declares, such as Unreal's classic FBX importer.")
	_fbx_unit.set_value("metres")
	_fbx_colors = ChoiceRow.new()
	_fbx_colors.add_text("srgb", "sRGB", "Colours encoded as most importers expect them")
	_fbx_colors.add_text("linear", "Linear", "Colours as Limner stores them, for pipelines that convert themselves")
	_fbx_colors.set_value("srgb")
	_fbx_rows = Kit.column([Form.row("Unit", [_fbx_unit]), Form.row("Colours", [_fbx_colors])])
	add_child(_fbx_rows)

	_gltf_visibility = Kit.toggle("Visibility extension", "Also write KHR_node_visibility, which really switches hidden lights off. Not every importer accepts it.", Callable())
	_gltf_opacity = Kit.toggle("Animate opacity", "Write opacity keys through KHR_animation_pointer. Not every importer accepts it.", Callable())
	_gltf_rows = Kit.column([_gltf_visibility, _gltf_opacity])
	add_child(_gltf_rows)
	add_child(Kit.separator())

	_target = SaveTarget.new(editor, OS.get_system_dir(OS.SYSTEM_DIR_DOCUMENTS).path_join("Limner"))
	add_child(_target)
	_export = Kit.button("Export", "Write the file", export_file, "export")
	add_child(_export)
	_report = Form.note()
	add_child(_report)
	refresh()


## Shows the options that apply to the format chosen.
func refresh() -> void:
	var format := format_chosen()
	_format_note.text = FORMATS[_format.selected][2]
	var still_only := format == "obj"
	_moving_row.visible = not still_only
	_moving_note.text = "A still at the playhead, with every layer moved to its place in the world." if still_only else ("The working range at the document's frame rate." if _moving.get_value() == "animated" else "A still at the playhead.")
	_bake.visible = not still_only
	_lights.visible = not still_only
	_cameras.visible = not still_only
	_fbx_rows.visible = format == "fbx"
	_gltf_rows.visible = format == "glb"
	_gltf_opacity.visible = _moving.get_value() == "animated"
	_target.set_extension(format)


## The file ending of the format chosen.
func format_chosen() -> String:
	return FORMATS[_format.selected][0]


## The options pressing Export sends to `LimnerScene.export_file`: only those that
## apply to the format chosen.
func options() -> Dictionary:
	var format := format_chosen()
	var animated := format != "obj" and _moving.get_value() == "animated"
	var sent := {"animation": animated, "include_hidden": _hidden.button_pressed}
	if not animated:
		sent["start"] = int(editor.status.time)
	if format != "obj":
		sent["bake_transforms"] = _bake.button_pressed
		sent["lights"] = _lights.button_pressed
		sent["cameras"] = _cameras.button_pressed
	if format == "fbx":
		sent["fbx_unit"] = _fbx_unit.get_value()
		sent["fbx_colors"] = _fbx_colors.get_value()
	if format == "glb":
		sent["gltf_node_visibility"] = _gltf_visibility.button_pressed
		sent["gltf_animate_opacity"] = animated and _gltf_opacity.button_pressed
	return sent


## Writes the file. Returns the reply of `LimnerScene.export_file`, with the path
## added as `path`.
func export_file() -> Dictionary:
	var path := _target.path(format_chosen())
	var sent := options()
	_export.disabled = true
	_report.text = "Writing…"
	# The export runs in this thread and the window stands still meanwhile, for
	# seconds with a large document. Let it show that first.
	await get_tree().process_frame
	await get_tree().process_frame
	var started := Time.get_ticks_msec()
	DirAccess.make_dir_recursive_absolute(path.get_base_dir())
	var reply := editor.scene.export_file(path, sent)
	reply["path"] = path
	_export.disabled = false
	if not reply.ok:
		_report.text = ""
		editor.message.emit("Could not export: %s" % reply.error, true)
		return reply
	var seconds := (Time.get_ticks_msec() - started) / 1000.0
	var lines := PackedStringArray([
		"%s nodes, %s meshes" % [Form.thousands(int(reply.nodes)), Form.thousands(int(reply.meshes))],
		"%s vertices, %s triangles" % [Form.thousands(int(reply.vertices)), Form.thousands(int(reply.triangles))],
	])
	if int(reply.animation_frames) > 0:
		lines.append("%s frames of animation" % Form.thousands(int(reply.animation_frames)))
	lines.append("%s, written in %s s" % [String.humanize_size(int(reply.bytes)), String.num(seconds, 1)])
	if seconds > 0.5:
		lines.append("The window did not respond while the file was written.")
	lines.append(path)
	_report.text = "\n".join(lines)
	editor.message.emit("Exported %s" % path, false)
	return reply
