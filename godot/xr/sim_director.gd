class_name XRSimDirector
extends Node
## Sets the simulated rig up for a screenshot, from the `--sim-…` options listed at
## the top of `sim_rig.gd`: where the head and hands are, which panel is out, what is
## pinned, what the drawing hand touches, presses or points at.
##
## It works through the same doors a user does: hands are moved and the panels react.
## Everything is done within a few frames of starting, because `--shot` takes its
## picture eight frames after the app's own options have been applied.

## Frames to wait before touching the panels: the app applies its own options (the
## demo sheet, a document to open) two frames in, and the panels need a frame or two
## to find their sizes.
const START_FRAMES := 3
## Where a panel pinned for a screenshot goes, in the room, in metres, and how far
## it is turned to face the head, in degrees.
const PIN_POSITION := Vector3(0.36, 1.42, -0.72)
const PIN_TURN_DEGREES := -24.0
## How far in front of a control the tip rests when it only touches, in metres.
const TOUCH_LIFT := 0.02

var _shell: XRShell
var _sim: SimRig


func _init(shell: XRShell) -> void:
	name = "SimDirector"
	_shell = shell
	_sim = shell.rig as SimRig


func _ready() -> void:
	var arguments := OS.get_cmdline_user_args()
	var options := {}
	var index := 0
	while index < arguments.size():
		var word := arguments[index]
		if word == "--sim-button-map":
			options[word] = ""
		elif word.begins_with("--sim-") and index + 1 < arguments.size():
			index += 1
			options[word] = arguments[index]
		index += 1
	if options.is_empty():
		queue_free()
		return
	_direct(options)


func _direct(options: Dictionary) -> void:
	_sim.listen_to_devices = false
	_shell.set_button_map_shown(options.has("--sim-button-map"))
	if options.has("--sim-head"):
		_sim.head.position = _vector(options["--sim-head"])
	if options.has("--sim-look"):
		var angles := _numbers(options["--sim-look"])
		_sim.head.rotation_degrees = Vector3(angles[1] if angles.size() > 1 else 0.0, angles[0], 0.0)
	if options.has("--sim-left"):
		_sim.set_tip(XRRig.LEFT, _vector(options["--sim-left"]))
	if options.has("--sim-right"):
		_sim.set_tip(XRRig.RIGHT, _vector(options["--sim-right"]))
	if options.has("--sim-panel"):
		_shell.palette.set_shown(true)
		_shell.palette.show_panel(options["--sim-panel"])
	if options.has("--sim-pin"):
		var turned := Basis(Vector3.UP, deg_to_rad(PIN_TURN_DEGREES))
		_shell.palette.pin(options["--sim-pin"], Transform3D(turned, PIN_POSITION))
	for surface in _shell.touchable():
		surface.tooltip_delay = 0.0
	for i in START_FRAMES:
		await get_tree().process_frame
	var side := _shell.drawing_side()
	for option: String in ["--sim-touch", "--sim-poke", "--sim-ray"]:
		if not options.has(option):
			continue
		var found := _find(options[option])
		if found.is_empty():
			push_error("%s: nothing on a visible panel is called \"%s\"" % [option, options[option]])
			continue
		var surface: XRPanelSurface = found[0]
		var control: Control = found[1]
		var target := surface.world_point(surface.pixel_of(control))
		var normal := surface.global_basis.z.normalized()
		match option:
			"--sim-touch":
				_sim.set_tip(side, target + normal * TOUCH_LIFT)
			"--sim-poke":
				await _sim.play(_sim.poke_steps(side, target, normal, 0.0, SimRig.SCRIPT_STEP))
			"--sim-ray":
				_sim.set_target(side, target)


## The first visible control on a panel within reach whose caption, tooltip or
## placeholder is `text`, and the sheet it is on; or else the first that contains
## `text`. Empty when there is none.
func _find(text: String) -> Array:
	var wanted := text.to_lower()
	var loose: Array = []
	for surface in _shell.touchable():
		for node in surface.content.find_children("*", "Control", true, false):
			var control: Control = node
			if not control.is_visible_in_tree():
				continue
			for caption: String in _captions(control):
				if caption.to_lower() == wanted:
					return [surface, control]
				if loose.is_empty() and caption.to_lower().contains(wanted):
					loose = [surface, control]
	return loose


## What a control is known by on screen.
func _captions(control: Control) -> Array:
	var captions := [control.tooltip_text]
	if control is Button or control is Label:
		captions.append(control.get("text"))
	if control is LineEdit:
		captions.append(control.get("placeholder_text"))
	return captions.filter(func(caption: String) -> bool: return caption != "")


func _numbers(text: String) -> PackedFloat64Array:
	return text.split_floats(",")


func _vector(text: String) -> Vector3:
	var parts := _numbers(text)
	if parts.size() != 3:
		push_error("Expected x,y,z but got \"%s\"" % text)
		return Vector3.ZERO
	return Vector3(parts[0], parts[1], parts[2])
