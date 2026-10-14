class_name DesktopHand
extends Node
## Stands in for the drawing hand on the desktop.
##
## The mouse or pen moves the tip of a hand over a plane that faces the viewer, at a
## chosen distance. The left button is the trigger, with pen pressure when there is
## a pen. Shift and Ctrl stand in for the other hand's trigger, the modifier.

const MIN_DEPTH := 0.15
const MAX_DEPTH := 20.0

var world: LimnerWorld
var editor: LimnerEditor
var camera: Camera3D
## How far in front of the head the hand works, in viewer-space metres.
var depth := 1.5

var _pressed := false
var _pressure := 1.0


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		_pressed = event.pressed
		if event.pressed:
			_pressure = 1.0
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
		scene.input_hand(0, tip, rotation, _pressure if _pressed else 0.0, 0.0, Vector2.ZERO, 0)
	scene.input_hand(1, Vector3.ZERO, Quaternion.IDENTITY, _modifier(), 0.0, Vector2.ZERO, 0)
	scene.input_head(camera.global_position)
	for feedback: Dictionary in scene.input_commit(delta):
		if feedback.type == "message":
			editor.message.emit(feedback.text, true)


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
