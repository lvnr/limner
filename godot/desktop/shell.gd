class_name DesktopShell
extends Node
## The desktop presentation: a fixed virtual head, mouse navigation and a status bar.
##
## The camera never moves. Navigation changes where the viewer stands in the world,
## exactly as grabbing the world does in a headset, so tool sizes and precision
## behave the same in both.

## Where the virtual head is, in viewer space.
const HEAD := Vector3(0.0, 1.4, 0.0)
## The point navigation turns and zooms about: straight ahead of the head.
const FOCUS := Vector3(0.0, 1.4, -1.5)
const FRAME_RADIUS := 0.6
const ORBIT_SPEED := 0.006
const ZOOM_STEP := 1.12

var world: LimnerWorld
var camera: Camera3D

var _status: Label
var _dragging := false
var _panning := false


func _ready() -> void:
	camera = Camera3D.new()
	camera.name = "Head"
	camera.position = HEAD
	camera.fov = 60.0
	camera.near = 0.01
	camera.far = 2000.0
	world.add_child(camera)
	camera.make_current()

	var layer := CanvasLayer.new()
	layer.name = "Overlay"
	add_child(layer)
	_status = Label.new()
	_status.position = Vector2(12, 8)
	_status.add_theme_color_override("font_color", Color(0.1, 0.1, 0.1))
	_status.add_theme_color_override("font_outline_color", Color(1, 1, 1, 0.8))
	_status.add_theme_constant_override("outline_size", 4)
	layer.add_child(_status)


func _process(_delta: float) -> void:
	if not world.scene.has_document():
		_status.text = "No document. Run with: -- --open <Quill project folder>"
		return
	var info := world.scene.info()
	var stats := world.scene.stats()
	_status.text = "%s  |  %d layers, %d strokes  |  showing %d layers, %s triangles  |  %d fps" % [
		info.title, info.layers, info.strokes, stats.visible_layers, _thousands(stats.triangles), Engine.get_frames_per_second()
	]


func _thousands(value: int) -> String:
	var text := str(value)
	var out := ""
	while text.length() > 3:
		out = "," + text.right(3) + out
		text = text.left(text.length() - 3)
	return text + out


## Fits everything visible in front of the head.
func frame_all() -> void:
	world.scene.view_frame_all(FOCUS, FRAME_RADIUS)


func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		_on_mouse_button(event)
	elif event is InputEventMouseMotion and _dragging:
		_on_drag(event.relative, _panning or event.shift_pressed)
	elif event is InputEventPanGesture:
		# Trackpad: two fingers orbit, with Shift they pan.
		_on_drag(-event.delta * 8.0, event.shift_pressed)
	elif event is InputEventMagnifyGesture:
		world.scene.view_zoom(_pivot_under_mouse(), event.factor)
	elif event is InputEventKey and event.pressed and not event.echo:
		match event.keycode:
			KEY_F:
				frame_all()
			KEY_HOME:
				world.scene.view_go_to_default_viewpoint()


func _on_mouse_button(event: InputEventMouseButton) -> void:
	match event.button_index:
		MOUSE_BUTTON_WHEEL_UP:
			if event.pressed:
				world.scene.view_zoom(_pivot_under_mouse(), ZOOM_STEP)
		MOUSE_BUTTON_WHEEL_DOWN:
			if event.pressed:
				world.scene.view_zoom(_pivot_under_mouse(), 1.0 / ZOOM_STEP)
		MOUSE_BUTTON_RIGHT, MOUSE_BUTTON_MIDDLE:
			_dragging = event.pressed
			_panning = event.shift_pressed
		MOUSE_BUTTON_LEFT:
			if event.alt_pressed or not event.pressed:
				_dragging = event.pressed
				_panning = event.shift_pressed


func _on_drag(relative: Vector2, pan: bool) -> void:
	if pan:
		# One screen height of mouse travel moves the world by the height of the view at the focus.
		var metres_per_pixel := 2.0 * FOCUS.distance_to(HEAD) * tan(deg_to_rad(camera.fov) * 0.5) / get_viewport().get_visible_rect().size.y
		world.scene.view_pan(Vector3(relative.x, -relative.y, 0.0) * metres_per_pixel)
	else:
		world.scene.view_rotate(FOCUS, Vector3.UP, relative.x * ORBIT_SPEED)
		world.scene.view_rotate(FOCUS, Vector3.RIGHT, relative.y * ORBIT_SPEED)


## The point on the focus plane under the mouse, in viewer space.
func _pivot_under_mouse() -> Vector3:
	var mouse := get_viewport().get_mouse_position()
	var origin := camera.project_ray_origin(mouse)
	var direction := camera.project_ray_normal(mouse)
	if absf(direction.z) < 1e-6:
		return FOCUS
	return origin + direction * ((FOCUS.z - origin.z) / direction.z)
