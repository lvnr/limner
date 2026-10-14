class_name SimRig
extends XRRig
## A headset and two controllers made of a mouse and a keyboard, for machines that
## have no headset. Started with `-- --xr --sim`, or by `-- --xr` alone when no
## headset runtime is found.
##
## An ordinary camera stands where the head would be and does not move. The two
## controllers are driven like this:
##
##   Mouse           moves the tip of the drawing hand over a plane facing the viewer
##   Wheel           moves that plane nearer or farther
##   Left button     the trigger
##   Right button    the grip
##   Q / E           the lower / upper face button
##   Arrow keys      the thumbstick
##   C               a click of the thumbstick
##   Tab (held)      all of the above go to the other hand instead
##   M               the other hand's upper button: shows and hides the palette
##   H               the menu button; held for a second it shows the button map
##   1 / 2           switch the left / right controller off and on, as if it lost tracking
##
## A hand points away from the elbow it would hang from, so moving the tip also
## aims it, which is what the ray onto far-off panels follows.
##
## The rig can also be driven by a script: `play` takes a list of timed controller
## states and performs them at a fixed 90 steps a second, so tests can poke a button,
## drag a slider or paint a stroke and get the same result every time.
##
## For screenshots, these options (after `--`) place things before the picture is
## taken; they are read by `XRSimDirector` (`sim_director.gd`):
##
##   --sim-head x,y,z        where the head is, in metres (default 0,1.5,0)
##   --sim-look yaw,pitch    which way it looks, in degrees (default 0,-10)
##   --sim-left x,y,z        where the tip of the left hand is
##   --sim-right x,y,z       where the tip of the right hand is
##   --sim-panel <title>     take the palette out, showing that panel (Tools, Color, Layers, …)
##   --sim-pin <title>       pin that panel in the room, to the right of the view
##   --sim-touch <text>      rest the drawing hand's tip on the control with that caption or tooltip
##   --sim-poke <text>       press that control with the tip, then rest on it
##   --sim-ray <text>        aim the drawing hand at that control from where the hand is
##   --sim-button-map        show the button map
##
## Example: scripts/shot.sh local/out/xr_tools.png --xr --sim --demo --sim-panel Tools

## Where the head stands, in the room, in metres.
const HEAD_POSITION := Vector3(0.0, 1.5, 0.0)
## How far the head looks down, in degrees, so that the hands are in view.
const HEAD_PITCH_DEGREES := -10.0
## The camera's vertical field of view, in degrees.
const HEAD_FOV_DEGREES := 65.0
## Where the tips of the left and right hand start, in the room, in metres.
const START_TIPS: Array[Vector3] = [Vector3(-0.22, 1.4, -0.5), Vector3(0.2, 1.3, -0.45)]
## Where a hand's elbow is, relative to the head, in metres: this far out to its own
## side, this far down and this far forward. A hand points from its elbow to its tip.
const ELBOW := Vector3(0.2, -0.2, 0.0)
## Where the palm is, relative to a controller's aim pose, in metres, and how far the
## handle slants back from upright, in degrees. Only the drawn controller uses them.
const PALM := Vector3(0.0, -0.03, 0.05)
const HANDLE_SLANT_DEGREES := 60.0
## The nearest and farthest the wheel can put the mouse's plane from the head, in metres.
const MIN_DEPTH := 0.15
const MAX_DEPTH := 3.0
## How much one notch of the wheel changes that distance, as a factor.
const DEPTH_STEP := 1.08
## The time one frame of a script stands for, in seconds.
const SCRIPT_STEP := 1.0 / 90.0
## How far in front of a panel a scripted poke starts and ends, and how far behind
## the panel it reaches, in metres.
const POKE_APPROACH := 0.03
const POKE_THROUGH := 0.012
## How long each half of a scripted poke takes, in seconds.
const POKE_SECONDS := 0.1

## A script given to `play` has run to its end.
signal script_finished

## False once a script or a screenshot option has placed the hands, so that a mouse
## resting over the window does not move them again.
var listen_to_devices := true
## Every vibration asked for, as `{side, strength, seconds}`, for tests to look at.
var pulses: Array[Dictionary] = []

var _tips: Array[Vector3] = [START_TIPS[0], START_TIPS[1]]
## For each side, the point the hand is made to aim at, or `null` to point away from the elbow.
var _targets: Array = [null, null]
var _present: Array[bool] = [true, true]
var _depths: Array[float] = [0.5, 0.5]
var _mouse_seen := false
var _steps: Array = []
var _next_step := 0
var _clock := 0.0
var _playing := false
## For each side, where and when the script last put its tip: where a glide starts.
var _from_tips: Array[Vector3] = [Vector3.ZERO, Vector3.ZERO]
var _from_times: Array[float] = [0.0, 0.0]


func _init() -> void:
	name = "SimRig"
	var camera := Camera3D.new()
	camera.name = "Head"
	camera.position = HEAD_POSITION
	camera.rotation_degrees = Vector3(HEAD_PITCH_DEGREES, 0.0, 0.0)
	camera.fov = HEAD_FOV_DEGREES
	camera.near = 0.01
	camera.far = 2000.0
	add_child(camera)
	head = camera
	for side in 2:
		var aim := Node3D.new()
		aim.name = "%sAim" % SIDE_NAMES[side].capitalize()
		add_child(aim)
		aims.append(aim)
		var grip := Node3D.new()
		grip.name = "%sGrip" % SIDE_NAMES[side].capitalize()
		add_child(grip)
		grips.append(grip)
		states[side].tracked = true
	_pose_hands()


func _ready() -> void:
	head.make_current()


func _unhandled_input(event: InputEvent) -> void:
	if _playing or not listen_to_devices:
		return
	if event is InputEventMouseMotion:
		_mouse_seen = true
	elif event is InputEventMouseButton and event.pressed:
		var side := _active_side()
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			_depths[side] = clampf(_depths[side] * DEPTH_STEP, MIN_DEPTH, MAX_DEPTH)
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			_depths[side] = clampf(_depths[side] / DEPTH_STEP, MIN_DEPTH, MAX_DEPTH)
	elif event is InputEventKey and event.pressed and not event.echo:
		if event.keycode == KEY_1:
			_present[LEFT] = not _present[LEFT]
		elif event.keycode == KEY_2:
			_present[RIGHT] = not _present[RIGHT]


func poll(delta: float) -> float:
	var step := delta
	if _playing:
		_advance_script()
		step = SCRIPT_STEP
	elif listen_to_devices:
		_read_devices()
	_pose_hands()
	return step


func pulse(side: int, strength: float, seconds: float) -> void:
	pulses.append({"side": side, "strength": strength, "seconds": seconds})


# ---- scripts ----

## Performs a list of timed controller states and returns when the last one has been
## reached. Wait for it with `await`.
##
## Each entry is a dictionary with a `time` in seconds from the start, in rising
## order, and optionally a `left` and a `right` dictionary saying what changes for
## that hand at that moment. A hand's dictionary may hold:
##
##   tip          Vector3: where its tip is, in the room. The tip glides there in a
##                straight line from where the entry before left it.
##   toward       Vector3: a point to aim at, or `null` to point away from the elbow again
##   trigger      0 to 1
##   grip         0 to 1
##   stick        Vector2
##   lower, upper, stick_click, menu   true or false
##   tracked      false to take the controller away, true to bring it back
##
## Whatever an entry does not mention stays as it was. An entry with only a `time`
## makes the script wait until then.
func play(steps: Array) -> void:
	listen_to_devices = false
	_steps = steps
	_next_step = 0
	_clock = 0.0
	for side in 2:
		_from_tips[side] = _tips[side]
		_from_times[side] = 0.0
	_playing = true
	await script_finished


## The entries that make a hand's tip press the point `target` on a surface whose
## front faces along `normal`: in front of it at `start`, through it `half` seconds
## later, and back out after as long again.
func poke_steps(side: int, target: Vector3, normal: Vector3, start := 0.0, half := POKE_SECONDS) -> Array:
	var hand: String = SIDE_NAMES[side]
	var front := target + normal * POKE_APPROACH
	return [
		{"time": start, hand: {"tip": front}},
		{"time": start + half, hand: {"tip": target - normal * POKE_THROUGH}},
		{"time": start + half * 2.0, hand: {"tip": front}},
	]


## Puts a hand's tip somewhere at once, outside a script.
func set_tip(side: int, at: Vector3) -> void:
	listen_to_devices = false
	_tips[side] = at
	_pose_hands()


## Makes a hand aim at a point in the room, or point away from its elbow again when
## `target` is `null`.
func set_target(side: int, target: Variant) -> void:
	_targets[side] = target
	_pose_hands()


## Where a hand's tip is, in the room.
func tip_position(side: int) -> Vector3:
	return _tips[side]


func _advance_script() -> void:
	while _next_step < _steps.size() and float(_steps[_next_step].time) <= _clock + 0.5 * SCRIPT_STEP:
		var step: Dictionary = _steps[_next_step]
		for side in 2:
			if step.has(SIDE_NAMES[side]):
				_apply_step(side, step[SIDE_NAMES[side]], float(step.time))
		_next_step += 1
	# Between two entries that place a tip, the tip is part-way along the line.
	for side in 2:
		for index in range(_next_step, _steps.size()):
			var later: Dictionary = _steps[index]
			var hand: Dictionary = later.get(SIDE_NAMES[side], {})
			if hand.has("tip"):
				var span := float(later.time) - _from_times[side]
				var share := clampf((_clock - _from_times[side]) / span, 0.0, 1.0) if span > 0.0 else 1.0
				var goal: Vector3 = hand.tip
				_tips[side] = _from_tips[side].lerp(goal, share)
				break
	_clock += SCRIPT_STEP
	if _next_step >= _steps.size():
		_playing = false
		# Whoever waits for the script carries on after this frame has been acted on.
		script_finished.emit.call_deferred()


func _apply_step(side: int, hand: Dictionary, time: float) -> void:
	var state := states[side]
	if hand.has("tip"):
		_tips[side] = hand.tip
		_from_tips[side] = hand.tip
		_from_times[side] = time
	if hand.has("toward"):
		_targets[side] = hand.toward
	if hand.has("tracked"):
		_present[side] = hand.tracked
	state.tracked = _present[side]
	state.trigger = hand.get("trigger", state.trigger)
	state.grip = hand.get("grip", state.grip)
	state.stick = hand.get("stick", state.stick)
	state.lower = hand.get("lower", state.lower)
	state.upper = hand.get("upper", state.upper)
	state.stick_click = hand.get("stick_click", state.stick_click)
	state.menu = hand.get("menu", state.menu)


# ---- mouse and keyboard ----

## The hand the mouse and keys drive: the drawing hand, or the other while Tab is held.
func _active_side() -> int:
	var drawing := RIGHT if Preferences.shared().text("dominant_hand") == "right" else LEFT
	return opposite(drawing) if Input.is_key_pressed(KEY_TAB) else drawing


func _read_devices() -> void:
	var active := _active_side()
	for side in 2:
		var state := states[side]
		state.release_all()
		state.tracked = _present[side]
	var hand := states[active]
	hand.trigger = 1.0 if Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT) else 0.0
	hand.grip = 1.0 if Input.is_mouse_button_pressed(MOUSE_BUTTON_RIGHT) else 0.0
	hand.lower = Input.is_key_pressed(KEY_Q)
	hand.upper = Input.is_key_pressed(KEY_E)
	hand.stick_click = Input.is_key_pressed(KEY_C)
	hand.menu = Input.is_key_pressed(KEY_H)
	hand.stick = Vector2(
		float(Input.is_key_pressed(KEY_RIGHT)) - float(Input.is_key_pressed(KEY_LEFT)),
		float(Input.is_key_pressed(KEY_UP)) - float(Input.is_key_pressed(KEY_DOWN))
	)
	if Input.is_key_pressed(KEY_M):
		var drawing := active if not Input.is_key_pressed(KEY_TAB) else opposite(active)
		states[opposite(drawing)].upper = true
	if _mouse_seen:
		var mouse := get_viewport().get_mouse_position()
		var from := head.project_ray_origin(mouse)
		var along := head.project_ray_normal(mouse)
		var forward := -head.global_basis.z
		_tips[active] = from + along * (_depths[active] / maxf(along.dot(forward), 0.05))


## Puts the pose nodes where the tips and aims say.
func _pose_hands() -> void:
	for side in 2:
		var outward := -1.0 if side == LEFT else 1.0
		var elbow := head.position + Vector3(ELBOW.x * outward, ELBOW.y, ELBOW.z)
		var aimed_at: Variant = _targets[side]
		var forward := _tips[side] - elbow
		if aimed_at is Vector3:
			var target: Vector3 = aimed_at
			forward = target - _tips[side]
		forward = forward.normalized() if forward.length() > 0.001 else Vector3.FORWARD
		var up := Vector3.UP if absf(forward.y) < 0.99 else Vector3.BACK
		var facing := Basis.looking_at(forward, up)
		aims[side].transform = Transform3D(facing, _tips[side] - forward * TIP_OFFSET)
		# A grip pose lies in the palm: behind and below the aim pose, with its Z axis
		# down the handle.
		grips[side].transform = aims[side].transform * Transform3D(Basis(Vector3.RIGHT, deg_to_rad(HANDLE_SLANT_DEGREES)), PALM)
		aims[side].visible = _present[side]
		grips[side].visible = _present[side]
