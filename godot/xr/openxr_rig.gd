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

## Names of the actions in the action map. None may be "aim", "grip", "palm" or
## "default": Godot gives those names to the pose actions ("aim_pose" and so on), and
## an action of the same name is then refused, with every binding to it. The grip
## was once called "grip", and no grip worked in a headset.
const ACTION_TRIGGER := &"trigger"
const ACTION_GRIP := &"squeeze"
const ACTION_STICK := &"stick"
const ACTION_STICK_CLICK := &"stick_click"
const ACTION_STICK_TOUCH := &"stick_touch"
const ACTION_TRACKPAD := &"trackpad"
const ACTION_TRACKPAD_CLICK := &"trackpad_click"
const ACTION_LOWER := &"lower_button"
const ACTION_UPPER := &"upper_button"
const ACTION_MENU := &"menu_button"
const ACTION_HAPTIC := &"haptic"

## The interaction profiles a runtime gives a bare hand, seen by the headset's
## cameras, in place of a controller. A hand on one of these does nothing unless the
## `hand_tracking` preference is on: put the controllers down and the hands must not
## start painting.
const HAND_PROFILES := [
	"/interaction_profiles/khr/simple_controller",
	"/interaction_profiles/ext/hand_interaction_ext",
	"/interaction_profiles/microsoft/hand_interaction",
]

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
	# Coming back from the desktop, the session already has it and will not say so
	# again, so the state it is in now is read.
	has_attention = session_state() == OpenXRInterface.SESSION_STATE_FOCUSED
	return ""


## Gives the main viewport back to the desktop: it is drawn on the screen again, at
## the pace the project asks for. OpenXR itself keeps running (in this build of Godot
## it cannot be started again once stopped), so the headset needs something else to
## show meanwhile: see `XRHoldingView`.
func stop(viewport: Viewport) -> void:
	_set_attention(false)
	for connected: Signal in [interface.session_begun, interface.session_visible, interface.session_focussed, interface.session_stopping, interface.session_loss_pending, interface.pose_recentered, interface.instance_exiting]:
		for connection: Dictionary in connected.get_connections():
			var callable: Callable = connection.callable
			if callable.get_object() == self:
				connected.disconnect(callable)
	give_back(viewport)
	DisplayServer.window_set_vsync_mode(int(ProjectSettings.get_setting("display/window/vsync/vsync_mode", DisplayServer.VSYNC_ENABLED)))
	Engine.max_fps = int(ProjectSettings.get_setting("application/run/max_fps", 0))


## Takes `viewport` off the headset and gives it back its own size and pictures.
static func give_back(viewport: Viewport) -> void:
	viewport.use_xr = false
	# While it drew for the headset the viewport took the headset's size and two eyes,
	# and its render buffers were made while it drew into the headset's images.
	# Turning that off gives none of it back, and the window does not lay its viewport
	# out again until it is resized. On Windows, with a Quest 3 over Link, the desktop's
	# 3D view then showed old pictures of the headset's panels where the scene should
	# be. So the viewport's pictures are made anew, at a size they cannot have now, and
	# the window then lays the viewport out as a resize would (setting a content
	# scale does that, even to the same value).
	var rid := viewport.get_viewport_rid()
	RenderingServer.viewport_set_size(rid, 2, 2)
	var window := viewport as Window
	if window != null:
		window.content_scale_size = window.content_scale_size
	var drawn := Vector2i(viewport.get_visible_rect().size)
	RenderingServer.viewport_set_size(rid, drawn.x, drawn.y)


## The OpenXR session's state, one of `OpenXRInterface.SessionState`; unknown in a
## build of Godot that cannot say.
func session_state() -> int:
	if interface == null or not interface.has_method("get_session_state"):
		return OpenXRInterface.SESSION_STATE_UNKNOWN
	return int(interface.call("get_session_state"))


func poll(delta: float) -> float:
	for side in 2:
		var controller: XRController3D = aims[side]
		var state := states[side]
		state.tracked = has_attention and controller.get_is_active() and controller.get_has_tracking_data()
		if state.tracked and is_bare_hand(side) and not Preferences.shared().flag("hand_tracking"):
			state.tracked = false
		if not state.tracked:
			state.release_all()
			continue
		state.trigger = controller.get_float(ACTION_TRIGGER)
		state.grip = controller.get_float(ACTION_GRIP)
		state.stick = controller.get_vector2(ACTION_STICK)
		state.stick_click = controller.is_button_pressed(ACTION_STICK_CLICK)
		state.stick_touch = controller.is_button_pressed(ACTION_STICK_TOUCH)
		# A trackpad stands in for a stick only while it is pressed down: a thumb
		# resting on it must not undo or step frames.
		if controller.is_button_pressed(ACTION_TRACKPAD_CLICK):
			state.stick = controller.get_vector2(ACTION_TRACKPAD)
		state.lower = controller.is_button_pressed(ACTION_LOWER)
		state.upper = controller.is_button_pressed(ACTION_UPPER)
		state.menu = controller.is_button_pressed(ACTION_MENU)
	return delta


## Whether the hand on `side` is seen by the headset's cameras instead of holding a
## controller: the runtime says so by the interaction profile it binds to the hand,
## or, where it can tell, by where the hand tracker's data comes from.
func is_bare_hand(side: int) -> bool:
	var controller := XRServer.get_tracker(StringName("%s_hand" % SIDE_NAMES[side])) as XRPositionalTracker
	if controller != null and HAND_PROFILES.has(controller.profile):
		return true
	var hand := XRServer.get_tracker(StringName("/user/hand_tracker/%s" % SIDE_NAMES[side])) as XRHandTracker
	return hand != null and hand.has_tracking_data and hand.hand_tracking_source == XRHandTracker.HAND_TRACKING_SOURCE_UNOBSTRUCTED


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
