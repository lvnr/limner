class_name XRPointer
extends Node3D
## How the drawing hand works the panels.
##
## Up close the tip of the hand is a finger: a few centimetres in front of a panel it
## hovers; pushed into a key (a button standing off the panel) the key goes down with
## it and presses 3 mm down; on the flat of the panel it presses 3 mm through the
## surface; pulled back it lets go. No trigger is needed. From farther off, a hand
## that points at a panel shows a thin ray with a dot where it lands, and the trigger
## is the button.
##
## Up close, holding the grip picks the panel up; it follows the hand until the grip
## is let go and stays where it was left. Pointing from afar takes only the trigger
## and the stick's up and down: the grip and the rest go on working the artwork. The
## stick scrolls what is under the pointer, or resizes a panel that is being held.
## The hand's lower face button is the other mouse button: it opens the menu of what
## is under the pointer, where there is one. While the modifier (the other hand's
## trigger) is held, presses come with Ctrl, as a tap with Ctrl adds to a selection.

## A press landed on a panel.
signal clicked(surface: XRPanelSurface)
## A panel was picked up with the grip.
signal grabbed(surface: XRPanelSurface)
## A panel that was being held was let go.
signal dropped(surface: XRPanelSurface)

enum Mode { NONE, POKE, RAY, GRAB }

## How far in front of a panel the tip starts to hover, in metres.
const HOVER_DISTANCE := 0.06
## How far through a panel, or down into a key, the tip must go to press, in metres.
const PRESS_DEPTH := 0.003
## How far back in front of the panel the tip must come to let go, in metres.
## Larger than the press depth so that a trembling hand does not press twice.
const RELEASE_DISTANCE := 0.006
## How far through a panel the tip can go before the panel gives up on it, in metres.
const MAX_DEPTH := 0.1
## How far outside a panel's edge the tip still counts as over it, in pixels.
const EDGE_MARGIN := 6.0
## How far away a panel can be pointed at with the ray, in metres.
const RAY_REACH := 2.0
## How far the trigger is pulled to press with the ray, and how far let go to
## release, from 0 to 1.
const RAY_PRESS := 0.5
const RAY_RELEASE := 0.25
## How far the grip is squeezed to pick a panel up, and how far let go to drop it.
const GRAB_PRESS := 0.6
const GRAB_RELEASE := 0.3
## How far the stick must be pushed before it scrolls or resizes, from 0 to 1.
const STICK_DEADZONE := 0.3
## Notches of scrolling a second with the stick pushed all the way.
const SCROLL_RATE := 14.0
## Seconds for a held panel to grow by a factor of e with the stick pushed all the way.
const RESIZE_SECONDS := 0.8
## Radius of the dot that marks where the pointer is on a panel, in metres.
const DOT_RADIUS := 0.0035
## How far the dot floats in front of the panel, in metres, so that it never sinks into it.
const DOT_LIFT := 0.0015
## Half the thickness of the ray, in metres.
const RAY_RADIUS := 0.0008

## Off while the hand is busy with the artwork: painting, or grabbing the view.
var enabled := true
var mode := Mode.NONE
## The panel the pointer is on or holds, or `null`.
var surface: XRPanelSurface
## The pixel of that panel the pointer is at.
var pixel := Vector2.ZERO

## True once the tip has been clear of the panel's front, so that pushing in counts
## as a press. A tip that arrives from behind never presses.
var _armed := false
var _pressed := false
## True once the trigger has been let go while the ray was on a panel, so that a
## trigger still held from painting does not press.
var _ray_ready := false
var _hold := Transform3D.IDENTITY
var _scroll := 0.0
var _lower_was_down := false
var _dot: MeshInstance3D
var _dot_material: StandardMaterial3D
var _ray: MeshInstance3D


func _ready() -> void:
	var ball := SphereMesh.new()
	ball.radius = DOT_RADIUS
	ball.height = DOT_RADIUS * 2.0
	ball.radial_segments = 12
	ball.rings = 6
	_dot_material = _flat_material(Kit.ACCENT)
	_dot = _instance("Dot", ball, _dot_material)
	var beam := CylinderMesh.new()
	beam.top_radius = RAY_RADIUS
	beam.bottom_radius = RAY_RADIUS
	beam.height = 1.0
	beam.radial_segments = 6
	beam.rings = 0
	_ray = _instance("Ray", beam, _flat_material(Kit.ACCENT.lightened(0.25)))


## Works out what the hand is doing to the panels this frame and tells them.
## `tip` is the tip of the drawing hand in the room, `state` what that hand presses,
## `surfaces` every panel that can be touched now, and `modifier` whether the other
## hand holds the modifier. Returns true while the hand belongs to the panels, when
## it must not paint or grab the artwork; with the ray (`mode` RAY) only its trigger
## and the stick's up and down are the panel's.
func update(tip: Transform3D, state: XRHandState, delta: float, surfaces: Array[XRPanelSurface], modifier := false) -> bool:
	if not enabled or not state.tracked or (surface != null and not surfaces.has(surface)):
		_let_go()
	if not enabled or not state.tracked:
		_show_marks(tip)
		return false
	for each in surfaces:
		each.modifier = modifier
	if mode == Mode.GRAB:
		_hold_panel(tip, state, delta)
	elif _pressed:
		_drag(tip, state)
	else:
		_seek(tip, surfaces)
		if mode != Mode.NONE:
			_hover(tip, state, delta)
	_lower_was_down = state.lower
	_show_marks(tip)
	return mode != Mode.NONE


# ---- finding a panel ----

## Chooses the panel under the pointer: the one the tip is nearest in front of, or
## else the nearest one the hand points at.
func _seek(tip: Transform3D, surfaces: Array[XRPanelSurface]) -> void:
	var found: XRPanelSurface = null
	var found_mode := Mode.NONE
	var found_pixel := Vector2.ZERO
	var nearest := INF
	for candidate in surfaces:
		var at := candidate.locate(tip.origin)
		# A panel the tip was not already on is only taken from its front.
		var deepest := -MAX_DEPTH if candidate == surface and mode == Mode.POKE else 0.0
		if at.z > HOVER_DISTANCE or at.z < deepest or not candidate.contains(Vector2(at.x, at.y), EDGE_MARGIN):
			continue
		var above := at.z - candidate.rest_at(Vector2(at.x, at.y))
		if absf(above) < nearest:
			nearest = absf(above)
			found = candidate
			found_mode = Mode.POKE
			found_pixel = Vector2(at.x, at.y)
	if found == null:
		nearest = RAY_REACH
		var forward := -tip.basis.z.normalized()
		for candidate in surfaces:
			var at := candidate.aim(tip.origin, forward)
			if not at.is_finite() or at.z > nearest or not candidate.contains(Vector2(at.x, at.y)):
				continue
			nearest = at.z
			found = candidate
			found_mode = Mode.RAY
			found_pixel = Vector2(at.x, at.y)
	if found != surface or found_mode != mode:
		var was_ready := _ray_ready and found_mode == Mode.RAY and mode == Mode.RAY
		_let_go()
		surface = found
		mode = found_mode
		_ray_ready = was_ready
	pixel = found_pixel


## Hovering: moves the panel's mouse, and starts a press, a grab or a scroll.
func _hover(tip: Transform3D, state: XRHandState, delta: float) -> void:
	surface.pointer_move(pixel, delta)
	# Only a hand up close takes a panel: pointing at one from afar leaves the grip
	# to the artwork, so that a panel in view never stops the view being grabbed.
	if mode == Mode.POKE and surface.movable and state.grip > GRAB_PRESS:
		_start_grab(tip)
		return
	if mode == Mode.POKE:
		var height := surface.locate(tip.origin).z
		surface.pointer_tip(height)
		# Depth below the top of what is under the tip: a key, or the sheet itself.
		var depth := height - surface.rest_at(pixel)
		if depth > RELEASE_DISTANCE:
			_armed = true
		elif _armed and depth < -PRESS_DEPTH:
			_press()
	else:
		surface.pointer_tip(INF)
		if state.trigger < RAY_RELEASE:
			_ray_ready = true
		elif _ray_ready and state.trigger > RAY_PRESS:
			_press()
	if state.lower and not _lower_was_down:
		surface.pointer_context(pixel)
	# Up and down scroll. Left and right scroll across, but only up close: pointed
	# from afar, they are undo and redo for the artwork.
	var across := mode == Mode.POKE and absf(state.stick.x) > absf(state.stick.y)
	var push := state.stick.x if across else state.stick.y
	if absf(push) > STICK_DEADZONE:
		_scroll += push * SCROLL_RATE * delta
		while absf(_scroll) >= 1.0:
			surface.pointer_scroll(pixel, _scroll > 0.0, across)
			_scroll -= signf(_scroll)
	else:
		_scroll = 0.0


func _press() -> void:
	_pressed = true
	surface.pointer_button(pixel, true)
	clicked.emit(surface)


## Pressed: the panel keeps the pointer, wherever it goes, until it is let go.
func _drag(tip: Transform3D, state: XRHandState) -> void:
	var release := false
	if mode == Mode.POKE:
		var at := surface.locate(tip.origin)
		var rest := surface.rest_at(pixel)
		pixel = Vector2(at.x, at.y)
		surface.pointer_tip(at.z)
		release = at.z > rest + RELEASE_DISTANCE or at.z < -MAX_DEPTH
		if at.z < -MAX_DEPTH:
			_armed = false
	else:
		var at := surface.aim(tip.origin, -tip.basis.z.normalized())
		if at.is_finite():
			pixel = Vector2(at.x, at.y)
		release = state.trigger < RAY_RELEASE
	surface.pointer_move(pixel)
	if release:
		_pressed = false
		surface.pointer_button(pixel, false)


# ---- picking a panel up ----

func _start_grab(tip: Transform3D) -> void:
	surface.pointer_leave()
	mode = Mode.GRAB
	_hold = tip.affine_inverse() * surface.global_transform.orthonormalized()
	grabbed.emit(surface)


func _hold_panel(tip: Transform3D, state: XRHandState, delta: float) -> void:
	if absf(state.stick.y) > STICK_DEADZONE:
		surface.magnification *= exp(state.stick.y * delta / RESIZE_SECONDS)
	surface.place(tip * _hold)
	surface.wake()
	if state.grip < GRAB_RELEASE:
		var held := surface
		surface = null
		mode = Mode.NONE
		dropped.emit(held)


# ---- leaving ----

## Drops whatever the pointer is doing, letting go of anything pressed or held.
func _let_go() -> void:
	if surface != null and is_instance_valid(surface):
		if mode == Mode.GRAB:
			dropped.emit(surface)
		else:
			surface.pointer_leave()
	surface = null
	mode = Mode.NONE
	_pressed = false
	_armed = false
	_ray_ready = false
	_scroll = 0.0


# ---- what is seen ----

func _show_marks(tip: Transform3D) -> void:
	var pointing := surface != null and (mode == Mode.POKE or mode == Mode.RAY)
	_dot.visible = pointing
	_ray.visible = pointing and mode == Mode.RAY
	if not pointing:
		return
	var spot := surface.world_point(pixel, DOT_LIFT + surface.rest_at(pixel))
	_dot.global_position = spot
	_dot.scale = Vector3.ONE * (1.6 if _pressed else 1.0)
	_dot_material.albedo_color = Color.WHITE if _pressed else Kit.ACCENT
	if _ray.visible:
		var along := spot - tip.origin
		var length := along.length()
		if length < 0.001:
			_ray.visible = false
			return
		# The cylinder's own axis is Y; lay it from the tip to the dot.
		var up := along / length
		var side := up.cross(Vector3.UP if absf(up.y) < 0.99 else Vector3.RIGHT).normalized()
		_ray.global_transform = Transform3D(Basis(side, up * length, side.cross(up)), tip.origin + along * 0.5)


func _instance(node_name: String, mesh: Mesh, material: Material) -> MeshInstance3D:
	var node := MeshInstance3D.new()
	node.name = node_name
	node.mesh = mesh
	node.material_override = material
	node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	node.visible = false
	add_child(node)
	return node


func _flat_material(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = color
	return material
