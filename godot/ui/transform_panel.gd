class_name TransformPanel
extends VBoxContainer
## The Transform panel: what grabbing moves, where a layer sits as numbers that can
## be typed, and the options and commands of Quill's Transform toolbox.

const AXES := ["X", "Y", "Z"]
const AUTO_SELECT := ["none", "layer", "group"]
## What a layer's placement is when nothing has moved it.
const IDENTITY := {"rotation": [0.0, 0.0, 0.0, 1.0], "scale": 1.0, "translation": [0.0, 0.0, 0.0], "flip": "none"}

var editor: LimnerEditor

## The layer the numbers and the buttons under them are about: the transform object
## if there is one, otherwise the current layer, otherwise -1.
var _subject := -1
## Where the playhead was when the numbers were last read.
var _shown_time := -1
var _target_name: Label
var _home: Button
var _use_current: Button
var _caption: Label
var _position: Array[NumberField] = []
var _rotation: Array[NumberField] = []
var _scale: NumberField
var _flip: Label
var _pivot: Array[NumberField] = []
## Everything that is switched off when the layer cannot be moved.
var _layer_controls: Array[Control] = []
var _reset_transform: Button
var _auto_select: Cycler
## The on-or-off editor options shown here, by option name.
var _option_toggles := {}
var _steps: NumberField
var _steps_note: Label

## The angles last typed, for which layer, and the rotation the document made of
## them. The same angles are shown again for as long as that rotation stands, so
## that typing one angle never visibly changes the other two.
var _typed_layer := -1
var _typed_rotation: Array = []
var _typed_degrees: Array = []


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Transform"

	add_child(Kit.heading("Transform object"))
	_home = Kit.icon_button("home", "Back to the view: grabbing moves the view again", _on_home)
	_target_name = Form.value()
	_use_current = Kit.button("Use current layer", "Grabbing moves the current layer", _on_use_current)
	add_child(Kit.row([_home, _target_name, _use_current]))
	add_child(Kit.separator())

	_caption = Form.note()
	add_child(_caption)
	add_child(Kit.heading("Position"))
	_position = _axis_fields("Position along", _on_position)
	add_child(Kit.heading("Rotation, degrees"))
	_rotation = _axis_fields("Turn about", _on_rotation)
	_scale = NumberField.new("Size of the layer; 1 is the size it was drawn at")
	_scale.committed.connect(_on_scale)
	_layer_controls.append(_scale)
	_flip = Kit.label("")
	_flip.custom_minimum_size = Vector2(40, 0)
	_flip.tooltip_text = "The axis the layer is mirrored on. Change it with the Flip buttons."
	_flip.mouse_filter = Control.MOUSE_FILTER_PASS
	add_child(Kit.row([Kit.label("Scale"), _scale, Kit.label("Flip", true), _flip]))
	add_child(Kit.heading("Pivot"))
	_pivot = _axis_fields("Pivot along", _on_pivot)

	var flips := Kit.row()
	for axis in 3:
		var flip := Kit.button("Flip %s" % AXES[axis], "Mirrors the layer along %s" % AXES[axis], _run_on_subject.bind("flip_layer", {"axis": AXES[axis].to_lower()}))
		flip.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		flips.add_child(flip)
		_layer_controls.append(flip)
	add_child(flips)

	_reset_transform = Kit.button("Reset transform", "", _on_reset_transform)
	var reset_pivot := Kit.button("Reset pivot", "Moves the pivot to the centre of the layer's content", _run_on_subject.bind("reset_pivot", {}))
	var pivot_origin := Kit.button("Make pivot origin", "Makes the pivot the layer's origin without moving what is drawn", _run_on_subject.bind("make_pivot_origin", {}))
	var again := Kit.button("Transform again", "Repeats the last duplicate-and-move on a fresh copy", func() -> void: editor.run({"cmd": "transform_again"}))
	_layer_controls.append_array([reset_pivot, pivot_origin])
	var commands := GridContainer.new()
	commands.columns = 2
	for command: Button in [_reset_transform, reset_pivot, pivot_origin, again]:
		command.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		command.clip_text = true
		commands.add_child(command)
	add_child(commands)
	add_child(Kit.separator())

	add_child(Kit.heading("Options"))
	add_child(_option_toggle(Kit.toggle("Keep the horizon level", "Turning the view keeps the floor flat", Callable()), "level_horizon"))
	_auto_select = Kit.choice(["None", "Its layer", "Its group"], "What moves when you grab a stroke instead of empty space", _on_auto_select)
	add_child(Kit.label("Grabbing a stroke moves"))
	add_child(_auto_select)
	add_child(_option_toggle(Kit.toggle("Show gimbal", "Shows the rings and arrows for moving things precisely: the transform object, the current layer while the view is the transform object, or the selection while the Selection tool is in hand", Callable()), "show_gimbal"))
	add_child(_option_toggle(Kit.toggle("Animate duplicates", "Each repeat of a duplicate-and-move puts the copy on a new frame", Callable()), "animate_duplicates"))

	_steps = NumberField.new("How many steps make a full turn when a gimbal ring is turned in steps", 1, 360, true)
	_steps.committed.connect(func(value: float) -> void: editor.set_option("rotation_snap_steps", int(value)))
	var steps_title := Kit.label("Rotation snap steps")
	steps_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	var stepper := Form.stepper(_steps, "Rotation snap steps")
	stepper.size_flags_horizontal = Control.SIZE_SHRINK_END
	_steps.custom_minimum_size = Vector2(52, Kit.ROW)
	add_child(Kit.row([steps_title, stepper]))
	_steps_note = Form.note()
	_steps_note.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	add_child(_steps_note)

	add_child(Kit.label("Free transform"))
	var free := Kit.row()
	free.add_theme_constant_override("separation", 16)
	for entry: Array in [
		["Translate", "allow_translate", "Grabbing can move things. With this off, a grab anywhere turns and scales about the pivot."],
		["Rotate", "allow_rotate", "Grabbing can turn things"],
		["Scale", "allow_scale", "Grabbing with both hands can change the size of things"],
	]:
		free.add_child(_option_toggle(Kit.toggle(entry[0], entry[2], Callable()), entry[1]))
	add_child(free)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


func _process(_delta: float) -> void:
	# An animated layer moves with the playhead.
	if is_visible_in_tree() and int(editor.status.get("time", 0)) != _shown_time:
		_refresh_numbers()


## Brings the panel in line with the document and the editor's options.
func refresh() -> void:
	var target := {}
	var current := {}
	for row: Dictionary in editor.get_rows():
		if row.transform_target:
			target = row
		if row.current:
			current = row
	var on_view := target.is_empty()
	_target_name.text = "View" if on_view else str(target.name)
	_target_name.tooltip_text = "Grabbing moves the view" if on_view else "Grabbing moves the layer %s" % target.name
	_home.disabled = on_view
	_use_current.disabled = current.is_empty() or current.parent == null or current.transform_target

	var subject := current if on_view else target
	_subject = -1 if subject.is_empty() else int(subject.id)
	var movable: bool = _subject >= 0 and subject.parent != null and not subject.locked
	if _subject < 0:
		_caption.text = "No layer is selected."
	elif subject.parent == null:
		_caption.text = "The root layer stays where it is."
	else:
		_caption.text = "Numbers for the %s, %s%s" % ["current layer" if on_view else "transform object", subject.name, " (locked)" if subject.locked else ""]
	Form.set_enabled(_layer_controls, movable)
	# With nothing selected and the view as the transform object, the view is what resets.
	_reset_transform.disabled = not (movable or _subject < 0)
	_reset_transform.tooltip_text = "Takes the view back to the initial spawn area" if _subject < 0 else "Puts the layer back where its parent is: no move, turn, scale or flip"
	_refresh_numbers()

	var options := editor.get_options()
	for key: String in _option_toggles:
		Kit.set_quiet(_option_toggles[key], options[key])
	Kit.set_quiet(_auto_select, maxi(AUTO_SELECT.find(options.auto_select), 0))
	var steps := int(options.rotation_snap_steps)
	_steps.set_value(steps)
	_steps_note.text = "%d %s = %s°" % [steps, "step" if steps == 1 else "steps", NumberField.format(360.0 / steps)]


func _refresh_numbers() -> void:
	_shown_time = int(editor.status.get("time", 0))
	var details := editor.scene.layer_details(_subject)
	var transform: Dictionary = details.get("transform", IDENTITY)
	var pivot: Dictionary = details.get("pivot", IDENTITY)
	var degrees := _degrees_of(transform.rotation)
	for axis in 3:
		_position[axis].set_value(transform.translation[axis])
		_rotation[axis].set_value(degrees[axis])
		_pivot[axis].set_value(pivot.translation[axis])
	_scale.set_value(transform.scale)
	_flip.text = str(transform.flip).to_upper() if transform.flip != "none" else "None"


## Three number boxes in a row, one per axis. `on_committed` is called with the
## number typed and the index of the axis.
func _axis_fields(what: String, on_committed: Callable) -> Array[NumberField]:
	var fields: Array[NumberField] = []
	var line := Kit.row()
	for axis in 3:
		var letter := Kit.label(AXES[axis])
		letter.add_theme_color_override("font_color", [Kit.AXIS_X, Kit.AXIS_Y, Kit.AXIS_Z][axis])
		line.add_child(letter)
		var field := NumberField.new("%s %s" % [what, AXES[axis]])
		# Three boxes share the row: slightly smaller figures let six digits fit.
		field.add_theme_font_size_override("font_size", 13)
		field.committed.connect(on_committed.bind(axis))
		line.add_child(field)
		fields.append(field)
		_layer_controls.append(field)
	add_child(line)
	return fields


## Ties a tick box or a toggle button to the editor option `key`.
func _option_toggle(control: BaseButton, key: String) -> BaseButton:
	control.toggled.connect(func(on: bool) -> void: editor.set_option(key, on))
	_option_toggles[key] = control
	return control


# ---- rotation as angles ----

## The stored rotation `[x, y, z, w]` as degrees about X, Y and Z (applied in the
## order Y, X, Z, as Godot does).
func _degrees_of(rotation: Array) -> Array:
	if _subject == _typed_layer and _same_rotation(rotation, _typed_rotation, 1e-9):
		return _typed_degrees
	var euler := Basis(Quaternion(rotation[0], rotation[1], rotation[2], rotation[3]).normalized()).get_euler(EULER_ORDER_YXZ)
	return [rad_to_deg(euler.x), rad_to_deg(euler.y), rad_to_deg(euler.z)]


func _same_rotation(a: Array, b: Array, tolerance: float) -> bool:
	if a.size() != 4 or b.size() != 4:
		return false
	for i in 4:
		if absf(float(a[i]) - float(b[i])) > tolerance:
			return false
	return true


# ---- commands ----

func _on_home() -> void:
	editor.run({"cmd": "set_transform_target", "layer": null})


func _on_use_current() -> void:
	for row: Dictionary in editor.get_rows():
		if row.current:
			editor.run({"cmd": "set_transform_target", "layer": int(row.id)})


## Changes the subject layer with `set_layer`. Returns whether it worked; when it
## did not, the numbers go back to what the document holds.
func _write(fields: Dictionary) -> bool:
	var command := {"cmd": "set_layer", "layer": _subject}
	command.merge(fields)
	var worked := editor.run(command) != null
	if not worked:
		_refresh_numbers()
	return worked


func _on_position(value: float, axis: int) -> void:
	var transform: Dictionary = editor.scene.layer_details(_subject).get("transform", {})
	if transform.is_empty() or value == float(transform.translation[axis]):
		return
	transform.translation[axis] = value
	_write({"transform": transform})


func _on_rotation(value: float, axis: int) -> void:
	var transform: Dictionary = editor.scene.layer_details(_subject).get("transform", {})
	if transform.is_empty():
		return
	var degrees: Array = _rotation.map(func(field: NumberField) -> float: return field.get_value())
	degrees[axis] = value
	var turn := Basis.from_euler(Vector3(deg_to_rad(degrees[0]), deg_to_rad(degrees[1]), deg_to_rad(degrees[2])), EULER_ORDER_YXZ).get_rotation_quaternion()
	var rotation := [turn.x, turn.y, turn.z, turn.w]
	if _same_rotation(rotation, transform.rotation, 1e-6):
		# The same angle typed another way: nothing to change, nothing to undo.
		_refresh_numbers()
		return
	transform.rotation = rotation
	if _write({"transform": transform}):
		_typed_layer = _subject
		_typed_rotation = editor.scene.layer_details(_subject).transform.rotation
		_typed_degrees = degrees


func _on_scale(value: float) -> void:
	var transform: Dictionary = editor.scene.layer_details(_subject).get("transform", {})
	if transform.is_empty() or value == float(transform.scale):
		return
	if value <= 0.0:
		editor.message.emit("The scale must be above zero", true)
		_refresh_numbers()
		return
	transform.scale = value
	_write({"transform": transform})


func _on_pivot(value: float, axis: int) -> void:
	var pivot: Dictionary = editor.scene.layer_details(_subject).get("pivot", {})
	if pivot.is_empty() or value == float(pivot.translation[axis]):
		return
	pivot.translation[axis] = value
	_write({"pivot": pivot})


## Runs a command that takes the subject layer and, perhaps, `more` fields.
func _run_on_subject(command: String, more: Dictionary) -> void:
	if _subject < 0:
		return
	var request := {"cmd": command, "layer": _subject}
	request.merge(more)
	editor.run(request)


func _on_reset_transform() -> void:
	if _subject >= 0:
		editor.run({"cmd": "reset_transform", "layer": _subject})
	else:
		editor.scene.view_go_to_default_viewpoint()


func _on_auto_select(index: int) -> void:
	if editor.get_options().auto_select != AUTO_SELECT[index]:
		editor.set_option("auto_select", AUTO_SELECT[index])
