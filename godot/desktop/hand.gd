class_name DesktopHand
extends Node
## Stands in for the drawing hand on the desktop.
##
## The mouse or pen moves the tip of a hand over a plane that faces the viewer, at a
## chosen distance. The left button is the trigger, with pen pressure when there is
## a pen. Shift and Ctrl stand in for the other hand's trigger, the modifier.
##
## Over a handle of the gimbal the left button takes hold of the handle instead, as
## a grip does: it moves or turns what the gimbal is on. With Alt held it moves the
## pivot (the trigger with the modifier held, in a headset), or turns a ring in steps
## (the trigger).

const MIN_DEPTH := 0.15
const MAX_DEPTH := 20.0

var world: LimnerWorld
var editor: LimnerEditor
var sounds: Sounds
var camera: Camera3D
## How far in front of the head the hand works, in viewer-space metres.
var depth := 1.5

var _pressed := false
var _pressure := 1.0
## The press now held began on a handle of the gimbal, and with Alt.
var _on_handle := false
var _pivot_mode := false
## The press with Alt began on a ring, which turns in steps rather than moving the pivot.
var _on_ring := false


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		_pressed = event.pressed
		if event.pressed:
			_pressure = 1.0
			var gimbal := world.scene.gimbal()
			_on_handle = not gimbal.is_empty() and gimbal.hovered[0] != ""
			_pivot_mode = _on_handle and Input.is_key_pressed(KEY_ALT)
			_on_ring = _on_handle and str(gimbal.hovered[0]).begins_with("ring")
	elif event is InputEventMouseMotion:
		# A pen reports how hard it presses; a mouse reports nothing and counts as full pressure.
		_pressure = event.pressure if event.pressure > 0.0 else 1.0


func _process(delta: float) -> void:
	var viewport := get_viewport()
	if not Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT):
		_pressed = false
	var over_interface := viewport.gui_get_hovered_control() != null
	var scene := world.scene
	if over_interface and not _pressed:
		scene.input_hand_absent(0)
	else:
		var mouse := viewport.get_mouse_position()
		var origin := camera.project_ray_origin(mouse)
		var direction := camera.project_ray_normal(mouse)
		var forward := -camera.global_basis.z
		var tip := origin + direction * (depth / maxf(direction.dot(forward), 0.05))
		var rotation := Basis.looking_at(direction, camera.global_basis.y).get_rotation_quaternion()
		var trigger := _pressure if _pressed else 0.0
		var grip := 0.0
		if _pressed and _on_handle and not _pivot_mode:
			grip = 1.0
			trigger = 0.0
		scene.input_hand(0, tip, rotation, trigger, grip, Vector2.ZERO, 0)
		# The hand cannot reach into the scene, so handles are picked along the line of sight.
		scene.input_hand_ray(0, origin, direction)
	# The pivot moves under the trigger with the modifier held.
	var modifier := 1.0 if _pressed and _pivot_mode and not _on_ring else _modifier()
	scene.input_hand(1, Vector3.ZERO, Quaternion.IDENTITY, modifier, 0.0, Vector2.ZERO, 0)
	scene.input_head(camera.global_position)
	for feedback: Dictionary in scene.input_commit(delta):
		if feedback.type == "message":
			editor.message.emit(feedback.text, true)
		elif feedback.type == "sound" and sounds != null:
			sounds.play(feedback.name)
	# The soft hiss of paint going down.
	if sounds != null:
		sounds.follow_tool(scene)


## How far the modifier is "pressed". Shift is a full press. With the Selection tool,
## where a half press adds and a full press removes, Shift adds and Ctrl removes, as
## desktop programs have it.
func _modifier() -> float:
	var shift := Input.is_key_pressed(KEY_SHIFT)
	var control := Input.is_key_pressed(KEY_CTRL) or Input.is_key_pressed(KEY_META)
	if editor.status.get("tool", "") == "select":
		return 1.0 if control else (0.5 if shift else 0.0)
	return 1.0 if shift else 0.0


## Moves the working plane nearer or farther by a factor.
func change_depth(factor: float) -> void:
	depth = clampf(depth * factor, MIN_DEPTH, MAX_DEPTH)
