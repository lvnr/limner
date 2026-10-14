class_name OpenXRRig
extends XRRig
## A real headset and its controllers, through OpenXR.
##
## Poses and buttons come from the actions in `res://xr/action_map.tres`, which binds
## them for each kind of controller. OpenXR itself is switched on in the project
## settings (`xr/openxr/enabled`); Godot then starts it before anything is drawn.
##
## Nothing in this file has run on a headset yet: the development machine has none.
## See `docs/xr.md`, "Never run on a headset".

## The refresh rates asked for, best first, in hertz. The first one the headset
## offers is used.
const REFRESH_RATES := [120.0, 90.0, 72.0]
## How close a rate the headset offers must be to one of those to count as it, in hertz.
const RATE_TOLERANCE := 0.5

## Names of the actions in the action map.
const ACTION_TRIGGER := &"trigger"
const ACTION_GRIP := &"grip"
const ACTION_STICK := &"stick"
const ACTION_STICK_CLICK := &"stick_click"
const ACTION_TRACKPAD := &"trackpad"
const ACTION_TRACKPAD_CLICK := &"trackpad_click"
const ACTION_LOWER := &"lower_button"
const ACTION_UPPER := &"upper_button"
const ACTION_MENU := &"menu_button"
const ACTION_HAPTIC := &"haptic"

var interface: OpenXRInterface
## The node OpenXR measures everything from. It stays at the room's origin.
var origin: XROrigin3D


## The OpenXR interface, or `null` in a build of Godot without it.
static func find_interface() -> OpenXRInterface:
	return XRServer.find_interface("OpenXR") as OpenXRInterface


## Makes sure OpenXR is running and says whether it is. It is cheap: when the project
## has OpenXR switched on Godot has started it already and this only asks; when not,
## starting fails at once. A failed start is cleaned up, so nothing is left half-made.
static func initialise() -> bool:
	var found := find_interface()
	if found == null:
		return false
	if found.is_initialized():
		return true
	# With OpenXR switched off, or its runtime not answering when the app started,
	# the interface knows of no system at all and there is nothing to start or undo.
	if found.get_system_info().is_empty():
		return false
	if found.initialize():
		return true
	found.uninitialize()
	return false


## Starts the session on `viewport`, the app's main one. Returns an empty string when
## it worked and otherwise the reason it did not, in words for the user.
func start(viewport: Viewport) -> String:
	interface = find_interface()
	if interface == null:
		return "This build has no headset support"
	if not initialise():
		return "No headset was found. Check that it is connected and that its OpenXR runtime is running."
	_build()
	interface.session_begun.connect(_on_session_begun)
	interface.session_visible.connect(_on_session_visible)
	interface.session_focussed.connect(_on_session_focussed)
	interface.session_stopping.connect(_on_session_stopping)
	interface.session_loss_pending.connect(_on_session_stopping)
	interface.pose_recentered.connect(func() -> void: recentred.emit())
	interface.instance_exiting.connect(func() -> void: closed.emit())
	# The headset paces the frames; waiting for the monitor as well would halve them.
	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	XRServer.world_scale = 1.0
	viewport.use_xr = true
	# Until the runtime says the session has the user's attention, the hands do nothing.
	has_attention = false
	return ""


func poll(delta: float) -> float:
	for side in 2:
		var controller: XRController3D = aims[side]
		var state := states[side]
		state.tracked = has_attention and controller.get_is_active() and controller.get_has_tracking_data()
		if not state.tracked:
			state.release_all()
			continue
		state.trigger = controller.get_float(ACTION_TRIGGER)
		state.grip = controller.get_float(ACTION_GRIP)
		state.stick = controller.get_vector2(ACTION_STICK)
		state.stick_click = controller.is_button_pressed(ACTION_STICK_CLICK)
		# A trackpad stands in for a stick only while it is pressed down: a thumb
		# resting on it must not undo or step frames.
		if controller.is_button_pressed(ACTION_TRACKPAD_CLICK):
			state.stick = controller.get_vector2(ACTION_TRACKPAD)
		state.lower = controller.is_button_pressed(ACTION_LOWER)
		state.upper = controller.is_button_pressed(ACTION_UPPER)
		state.menu = controller.is_button_pressed(ACTION_MENU)
	return delta


func pulse(side: int, strength: float, seconds: float) -> void:
	var controller: XRController3D = aims[side]
	controller.trigger_haptic_pulse(ACTION_HAPTIC, 0.0, clampf(strength, 0.0, 1.0), seconds, 0.0)


func _build() -> void:
	origin = XROrigin3D.new()
	origin.name = "Origin"
	add_child(origin)
	var camera := XRCamera3D.new()
	camera.name = "Head"
	camera.near = 0.01
	camera.far = 2000.0
	origin.add_child(camera)
	head = camera
	for side in 2:
		aims.append(_controller(side, "Aim", &"aim"))
		grips.append(_controller(side, "Grip", &"grip"))


func _controller(side: int, kind: String, pose: StringName) -> XRController3D:
	var controller := XRController3D.new()
	controller.name = "%s%s" % [SIDE_NAMES[side].capitalize(), kind]
	controller.tracker = StringName("%s_hand" % SIDE_NAMES[side])
	controller.pose = pose
	controller.show_when_tracked = true
	origin.add_child(controller)
	return controller


func _on_session_begun() -> void:
	var offered := interface.get_available_display_refresh_rates()
	for wanted: float in REFRESH_RATES:
		for rate: float in offered:
			if absf(rate - wanted) <= RATE_TOLERANCE:
				interface.display_refresh_rate = rate
				return


func _on_session_visible() -> void:
	_set_attention(false)


func _on_session_focussed() -> void:
	_set_attention(true)


func _on_session_stopping() -> void:
	_set_attention(false)


func _set_attention(on: bool) -> void:
	if has_attention == on:
		return
	has_attention = on
	attention_changed.emit(on)
