class_name PropertiesPanel
extends VBoxContainer
## The Layer panel: the name and kind of the current layer, and the settings that
## kind of layer has.
##
## Each kind has a section of controls, built once; only the current layer's is
## shown. A control is tied to one field of the layer's settings (`props` in
## `layer_details`), named by a dotted path such as "attenuation.min".

const KIND_NAMES := {
	"group": "Group",
	"sequence": "Sequence",
	"paint": "Paint layer",
	"picture": "Picture",
	"model": "Model",
	"sound": "Sound",
	"viewpoint": "Spawn area",
	"camera": "Camera",
	"light": "Light",
}
## Height in millimetres of the film that focal lengths are quoted for.
const SENSOR_HEIGHT := 24.0
const SOUND_SHAPES := ["none", "cone", "frustum"]
## What a sound's cone or frustum starts as: Quill's own starting angles, in radians.
const SOUND_SHAPE_DEFAULTS := {
	"cone": {"inner": 0.5, "band": 0.1, "outside": 0.0},
	"frustum": {"inner_h": 0.6, "inner_v": 0.4, "band": 0.1, "outside": 0.0},
}

var editor: LimnerEditor

## The layer shown, or -1. Edits go to it even if another layer has since become current.
var _layer := -1
var _empty: Label
var _body: VBoxContainer
var _icon: TextureRect
var _name: TextField
var _kind: Label
var _path: Label
## The section of controls for each kind of layer.
var _sections := {}
## Every control by "kind.field", so that a section can find its own and tests can reach them.
var _controls := {}
## For each kind, the functions that bring its controls in line with a layer's details.
var _readers := {}
## Everything a locked layer switches off.
var _editable: Array[Control] = []


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Layer"

	_empty = Form.note("No layer is selected.")
	add_child(_empty)
	_body = Kit.column()
	add_child(_body)

	_icon = TextureRect.new()
	_icon.stretch_mode = TextureRect.STRETCH_KEEP_CENTERED
	_icon.custom_minimum_size = Vector2(22, Kit.ROW)
	_name = TextField.new("Layer name", "The layer's name. Layers in the same group are kept from sharing one.")
	_name.committed.connect(_on_renamed)
	_body.add_child(Kit.row([_icon, _name]))
	_kind = Kit.label("")
	_body.add_child(_kind)
	_path = Form.note()
	_path.tooltip_text = "Where the layer is: the groups it is inside, from the root down"
	_path.mouse_filter = Control.MOUSE_FILTER_PASS
	_body.add_child(_path)
	_body.add_child(Kit.separator())

	_build_paint()
	_build_sequence()
	_build_group()
	_build_light()
	_build_camera()
	_build_viewpoint()
	_build_sound()
	_build_picture()
	_build_model()


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


## Brings the panel in line with the current layer.
func refresh() -> void:
	_layer = -1
	for row: Dictionary in editor.get_rows():
		if row.current:
			_layer = int(row.id)
	var details := editor.scene.layer_details(_layer)
	_empty.visible = details.is_empty()
	_body.visible = not details.is_empty()
	if details.is_empty():
		return
	var kind: String = details.kind
	_icon.texture = Icons.texture(LayersPanel.KIND_ICONS.get(kind, "dot"), Kit.TEXT, 20)
	_name.set_value(details.name)
	_kind.text = "%s%s" % [KIND_NAMES.get(kind, kind.capitalize()), ", locked" if details.locked else ""]
	_path.text = details.path
	for key: String in _sections:
		_sections[key].visible = key == kind
	for reader: Callable in _readers.get(kind, []):
		reader.call(details)
	Form.set_enabled(_editable, not details.locked)


# ---- building blocks ----

func _section(kind: String) -> VBoxContainer:
	var section := Kit.column()
	section.visible = false
	_sections[kind] = section
	_readers[kind] = []
	_body.add_child(section)
	return section


## Puts `control` in the section for `kind` and records it as "kind.key". `line` is
## what goes in the section when the control sits inside a row of its own.
func _add(kind: String, key: String, control: Control, line: Control = null) -> void:
	_controls["%s.%s" % [kind, key]] = control
	_editable.append(control)
	_sections[kind].add_child(control if line == null else line)


## A tick box for a true-or-false setting.
func _toggle(kind: String, field: String, text: String, tooltip: String) -> void:
	var box := Kit.toggle(text, tooltip, func(on: bool) -> void: _write(field, on))
	_add(kind, field, box)
	_readers[kind].append(func(details: Dictionary) -> void: Kit.set_quiet(box, _dig(details.props, field) == true))


## A drop-down for a setting that takes one of `values`; `names` are what the user reads.
func _choice(kind: String, field: String, text: String, tooltip: String, values: Array, names: Array) -> void:
	var menu := Kit.option(names, tooltip, func(index: int) -> void:
		if _prop(field) != values[index]:
			_write(field, values[index])
	)
	_add(kind, field, menu, Form.row(text, [menu], tooltip))
	_readers[kind].append(func(details: Dictionary) -> void: Kit.set_quiet(menu, maxi(values.find(_dig(details.props, field)), 0)))


## A slider for a number. It shows the stored number times `shown_per_stored`:
## degrees for radians, percent for a fraction. A drag is one undo step.
func _slider(kind: String, field: String, slider: SliderField, shown_per_stored := 1.0) -> void:
	slider.drag_started.connect(func() -> void: editor.begin("Change layer settings"))
	slider.changed.connect(func(value: float) -> void: _write(field, value / shown_per_stored))
	slider.drag_ended.connect(editor.commit)
	_add(kind, field, slider)
	_readers[kind].append(func(details: Dictionary) -> void:
		var stored: Variant = _dig(details.props, field)
		if stored != null:
			slider.set_value(float(stored) * shown_per_stored)
	)


## A colour swatch. Everything picked while its picker is open is one undo step.
func _color(kind: String, field: String, text: String, tooltip: String) -> void:
	var swatch := ColorField.new(tooltip)
	swatch.edit_started.connect(func() -> void: editor.begin("Change layer colour"))
	swatch.changed.connect(func(color: Color) -> void: _write(field, LimnerEditor.linear_rgb(color)))
	swatch.edit_ended.connect(editor.commit)
	_add(kind, field, swatch, Form.row(text, [swatch], tooltip))
	_readers[kind].append(func(details: Dictionary) -> void: swatch.set_value(LimnerEditor.screen_color(_dig(details.props, field))))


## The value at a dotted path in a layer's settings, or null when it is not there.
static func _dig(props: Dictionary, path: String) -> Variant:
	var value: Variant = props
	for key: String in path.split("."):
		if not (value is Dictionary and value.has(key)):
			return null
		value = value[key]
	return value


## The shown layer's setting at a dotted path, as the document holds it now.
func _prop(path: String) -> Variant:
	return _dig(editor.scene.layer_details(_layer).get("props", {}), path)


## Changes the setting at a dotted path. When the document refuses, the controls
## go back to what it holds.
func _write(path: String, value: Variant) -> void:
	var keys := path.split(".")
	var values: Variant = value
	for i in range(keys.size() - 1, -1, -1):
		values = {keys[i]: values}
	if editor.run({"cmd": "set_layer_kind", "layer": _layer, "values": values}) == null:
		refresh()


func _on_renamed(text: String) -> void:
	editor.run({"cmd": "set_layer", "layer": _layer, "name": text})
	# The document may settle on another name: layers in a group never share one.
	_name.set_value(str(editor.scene.layer_details(_layer).get("name", text)))


static func _count(number: int, noun: String) -> String:
	return "%s %s%s" % [Form.thousands(number), noun, "" if number == 1 else "s"]


# ---- paint ----

func _build_paint() -> void:
	var section := _section("paint")
	_choice("paint", "shading", "Shading", "Lit layers are shaded by the scene's lights", ["unlit", "lit"], ["Unlit", "Lit"])
	var bake := Kit.button("Bake lighting into paint", "Paints the light that falls on this layer now into its strokes, on every frame, and makes it unlit: it then looks the same without the lights, here and in anything it is exported to. Shadows are not baked.", func() -> void: editor.run({"cmd": "bake_lighting", "layer": _layer}))
	bake.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_controls["paint.bake"] = bake
	section.add_child(bake)
	_readers.paint.append(func(details: Dictionary) -> void: bake.disabled = details.props.shading != "lit")
	var loop := Kit.toggle("Loop animation", "The frames play over and over. Off, they play once and stay on the last one.", func(on: bool) -> void: _write("max_repeat", 0 if on else 1))
	_add("paint", "loop", loop)
	var counts := Form.note()
	_controls["paint.counts"] = counts
	section.add_child(counts)
	_readers.paint.append(func(details: Dictionary) -> void:
		Kit.set_quiet(loop, int(details.props.max_repeat) == 0)
		var frames: Variant = details.paint.frames
		counts.text = "%s, %s, %s in the frame shown" % [
			_count(frames.size() if frames is Array else int(frames), "frame"),
			_count(int(details.paint.drawings), "drawing"),
			_count(int(editor.row(_layer).get("strokes", 0)), "stroke"),
		]
	)


# ---- sequence and group ----

func _build_sequence() -> void:
	var section := _section("sequence")
	var loop := Kit.toggle("Loop", "The sequence starts over when it reaches its loop length", _on_sequence_loop)
	_add("sequence", "loop", loop)

	var seconds := NumberField.new("Loop length in seconds; 0 for no loop", 0.0, 86400.0)
	seconds.committed.connect(func(value: float) -> void: _write("loop_duration", LimnerEditor.to_flicks(value)))
	var frames := NumberField.new("Loop length in frames; 0 for no loop", 0.0, 1e7)
	frames.committed.connect(func(value: float) -> void: _write("loop_duration", roundi(value * _frame_duration())))
	var times := Form.time_table()
	Form.time_row(times, "Loop length", seconds, frames, "Where the sequence starts over, in its own time; 0 for no loop")
	_add("sequence", "loop_seconds", seconds, times)
	_controls["sequence.loop_frames"] = frames
	_editable.append(frames)

	var repeats := NumberField.new("How many times the sequence plays; 0 for ever", 0, 100000, true)
	repeats.committed.connect(func(value: float) -> void: _write("max_repeat", int(value)))
	var stepper := Form.stepper(repeats, "Repeats")
	_add("sequence", "max_repeat", repeats, Form.row("Repeats", [stepper]))
	_editable.append(stepper)
	section.add_child(Form.note("0 repeats for ever."))

	var convert := Kit.button("Convert to group", "Gives up the sequence's own timeline: its layers follow the timeline above", func() -> void: editor.run({"cmd": "set_group_is_sequence", "layer": _layer, "sequence": false}))
	_add("sequence", "convert", convert)
	_readers.sequence.append(func(details: Dictionary) -> void:
		var length := int(details.props.loop_duration)
		Kit.set_quiet(loop, length > 0)
		seconds.set_value(LimnerEditor.to_seconds(length))
		frames.set_value(float(length) / _frame_duration())
		repeats.set_value(int(details.props.max_repeat))
		convert.visible = int(details.id) != int(editor.get_info().root)
	)


func _build_group() -> void:
	var section := _section("group")
	section.add_child(Form.note("A group has no settings of its own."))
	var convert := Kit.button("Convert to sequence", "Gives the group its own timeline, which can loop and start at any time", func() -> void: editor.run({"cmd": "set_group_is_sequence", "layer": _layer, "sequence": true}))
	_add("group", "convert", convert)


func _frame_duration() -> int:
	return int(editor.get_info().frame_duration)


func _on_sequence_loop(on: bool) -> void:
	var length := 0
	if on:
		# Loop where the playhead is in the sequence's own time, on a frame.
		var frame := _frame_duration()
		length = roundi(float(editor.scene.layer_details(_layer).local_time) / frame) * frame
		if length <= 0:
			length = LimnerEditor.FLICKS_PER_SECOND
	_write("loop_duration", length)


# ---- light ----

func _build_light() -> void:
	_section("light")
	_choice("light", "kind", "Type", "A point shines all round, a spot in a cone, a directional light everywhere from one side", ["point", "spot", "directional"], ["Point", "Spot", "Directional"])
	_color("light", "color", "Colour", "The colour of the light")
	_slider("light", "energy", SliderField.new("Energy", 0.0, 16.0, 0.01, "How bright the light is. Type a number to go beyond 16.", "", false, 1000.0))
	_slider("light", "range", SliderField.new("Range", 0.01, 1000.0, 0.01, "How far the light reaches", "m", true))
	_slider("light", "spot_angle", SliderField.new("Cone angle", 1.0, 89.0, 0.5, "From the middle of the beam to its edge", "°"))
	_slider("light", "spot_blend", SliderField.new("Cone softness", 0.0, 100.0, 1.0, "How much of the beam fades out towards its edge", "%"), 100.0)
	_toggle("light", "shadows", "Cast shadows", "Lit layers block this light")
	_readers.light.append(func(details: Dictionary) -> void:
		_controls["light.range"].visible = details.props.kind != "directional"
		_controls["light.spot_angle"].visible = details.props.kind == "spot"
		_controls["light.spot_blend"].visible = details.props.kind == "spot"
	)


# ---- camera ----

func _build_camera() -> void:
	var section := _section("camera")
	_slider("camera", "fov", SliderField.new("Field of view", 1.0, 170.0, 0.1, "How much the camera sees from top to bottom", "°"))
	var focal := SliderField.new("Focal length", _focal_length(170.0), _focal_length(1.0), 0.01, "The lens that gives this view on film 24 mm tall", "mm", true)
	focal.drag_started.connect(func() -> void: editor.begin("Change focal length"))
	focal.changed.connect(func(millimetres: float) -> void: _write("fov", _field_of_view(millimetres)))
	focal.drag_ended.connect(editor.commit)
	_add("camera", "focal", focal)
	var resolution := Form.note()
	_controls["camera.resolution"] = resolution
	section.add_child(resolution)
	_readers.camera.append(func(details: Dictionary) -> void:
		focal.set_value(_focal_length(float(details.props.fov)))
		var size: Array = editor.get_info().camera_resolution
		resolution.text = "Renders %d × %d pixels, set in the Document tab." % [int(size[0]), int(size[1])]
	)


## The focal length in millimetres that sees `degrees` from top to bottom.
static func _focal_length(degrees: float) -> float:
	return SENSOR_HEIGHT * 0.5 / tan(deg_to_rad(degrees) * 0.5)


## The vertical field of view in degrees of a lens `millimetres` long.
static func _field_of_view(millimetres: float) -> float:
	return rad_to_deg(2.0 * atan(SENSOR_HEIGHT * 0.5 / millimetres))


# ---- spawn area ----

func _build_viewpoint() -> void:
	var section := _section("viewpoint")
	_choice("viewpoint", "floor_level", "Viewer at", "Eye level puts the headset at the height you set, whoever is watching. Floor level puts the ring on the viewer's own floor, so everyone stands on it at their own height.", [false, true], ["Eye level", "Floor level"])
	var initial := Kit.toggle("Initial spawn area", "The viewer starts here when the document opens", func(on: bool) -> void: editor.run({"cmd": "set_default_viewpoint", "layer": _layer if on else null}))
	_add("viewpoint", "initial", initial)
	section.add_child(Kit.button("Stand here", "Moves your view to this spawn area", func() -> void: editor.scene.view_go_to_viewpoint(_layer), "viewpoint"))
	_toggle("viewpoint", "exporting", "Include when publishing", "Viewers of the published piece can use this spawn area")
	_toggle("viewpoint", "show_volume", "Show volume", "Shows the space the viewer may move in")
	_color("viewpoint", "color", "Colour", "The colour of the ring and its volume")
	_readers.viewpoint.append(func(_details: Dictionary) -> void: Kit.set_quiet(initial, editor.row(_layer).get("default_viewpoint", false)))


# ---- sound ----

func _build_sound() -> void:
	var section := _section("sound")
	_slider("sound", "gain", SliderField.new("Gain", 0.0, 1.0, 0.01, "How loud the sound plays. Type a number to go up to 4.", "", false, 4.0))
	_toggle("sound", "looped", "Loop", "The sound starts over when it ends")
	var spatial := Kit.toggle("Spatialize", "The sound comes from where the layer is. Off, it plays the same everywhere.", func(on: bool) -> void: _write("kind", "positional" if on else "flat"))
	_add("sound", "spatialize", spatial)
	var ambisonic := Form.note("Ambisonic: the recording surrounds the viewer.")
	section.add_child(ambisonic)

	_choice("sound", "attenuation.mode", "Attenuation", "How the sound fades with distance", ["none", "linear", "logarithmic"], ["None", "Linear", "Logarithm"])
	var nearest := NumberField.new("Full volume inside this distance", 0.0, 1e6)
	nearest.committed.connect(func(value: float) -> void: _write("attenuation", {"min": value, "max": maxf(value, float(_prop("attenuation.max")))}))
	var farthest := NumberField.new("Silent beyond this distance; never less than Min", 0.0, 1e6)
	farthest.committed.connect(func(value: float) -> void: _write("attenuation.max", maxf(value, float(_prop("attenuation.min")))))
	var radii := Form.row("Radius", [Kit.label("Min", true), nearest, Kit.label("Max", true), farthest])
	_add("sound", "attenuation.min", nearest, radii)
	_controls["sound.attenuation.max"] = farthest
	_editable.append(farthest)

	var direction := Kit.option(["None", "Cone", "Frustum"], "Whether the sound is louder one way than the others", _on_sound_direction)
	_add("sound", "direction", direction, Form.row("Direction", [direction]))
	var degrees := 180.0 / PI
	_slider("sound", "modifier.cone.inner", SliderField.new("Inner angle", 0.0, 180.0, 0.1, "Full volume inside this angle", "°"), degrees)
	_slider("sound", "modifier.cone.band", SliderField.new("Band", 0.0, 90.0, 0.1, "The angle over which the sound fades to its outside volume", "°"), degrees)
	_slider("sound", "modifier.cone.outside", SliderField.new("Outside", 0.0, 100.0, 1.0, "Volume outside the cone", "%"), 100.0)
	_slider("sound", "modifier.frustum.inner_h", SliderField.new("Inner width", 0.0, 180.0, 0.1, "Full volume inside this angle from side to side", "°"), degrees)
	_slider("sound", "modifier.frustum.inner_v", SliderField.new("Inner height", 0.0, 180.0, 0.1, "Full volume inside this angle from top to bottom", "°"), degrees)
	_slider("sound", "modifier.frustum.band", SliderField.new("Band", 0.0, 90.0, 0.1, "The angle over which the sound fades to its outside volume", "°"), degrees)
	_slider("sound", "modifier.frustum.outside", SliderField.new("Outside", 0.0, 100.0, 1.0, "Volume outside the frustum", "%"), 100.0)

	_readers.sound.append(func(details: Dictionary) -> void:
		var props: Dictionary = details.props
		Kit.set_quiet(spatial, props.kind == "positional")
		spatial.visible = props.kind != "ambisonic"
		ambisonic.visible = props.kind == "ambisonic"
		nearest.set_value(props.attenuation.min)
		farthest.set_value(props.attenuation.max)
		radii.visible = props.attenuation.mode != "none"
		var shape := _sound_shape(props.modifier)
		Kit.set_quiet(direction, SOUND_SHAPES.find(shape))
		for key: String in _controls:
			if key.begins_with("sound.modifier."):
				_controls[key].visible = key.begins_with("sound.modifier.%s." % shape)
	)


## Which shape a sound's `modifier` is: "none", "cone" or "frustum".
static func _sound_shape(modifier: Variant) -> String:
	if modifier is Dictionary and not modifier.is_empty():
		return str(modifier.keys()[0])
	return "none"


func _on_sound_direction(index: int) -> void:
	var shape: String = SOUND_SHAPES[index]
	var before: Variant = _prop("modifier")
	if shape == _sound_shape(before):
		return
	editor.begin("Change sound direction")
	# One shape cannot be changed into the other in place, so clear it first.
	_write("modifier", "none")
	if shape != "none":
		var settings: Dictionary = SOUND_SHAPE_DEFAULTS[shape].duplicate()
		if before is Dictionary:
			# Going between cone and frustum keeps the fade and the outside volume.
			var old: Dictionary = before.values()[0]
			settings.band = old.band
			settings.outside = old.outside
		_write("modifier", {shape: settings})
	editor.commit()


# ---- picture and model ----

func _build_picture() -> void:
	_section("picture")
	_choice("picture", "projection", "Type", "A flat image, or one that wraps all round the viewer", ["flat", "equirect", "equirect_stereo", "cubemap_cross", "cubemap_strip"], ["2D image", "360", "360 stereo", "Cubemap cross", "Cubemap strip"])
	_toggle("picture", "viewer_locked", "Lock to viewer", "The picture stays in front of the viewer wherever they look or go")


func _build_model() -> void:
	_section("model")
	_choice("model", "shading", "Lighting", "Smooth models are shaded by the scene's lights", ["unlit", "lit"], ["Unlit", "Smooth"])
	_toggle("model", "wireframe", "Wireframe", "Draws only the edges of the model's triangles")
