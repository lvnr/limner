class_name XRHoldingView
extends SubViewport
## What the headset shows while Limner is used on the desktop: an empty, dim room
## with one sheet that says so, and a way back.
##
## OpenXR cannot be stopped and started again in one run of this Godot build, so its
## session stays alive when the user changes to the desktop. A session with nothing
## to show leaves the headset blank (and Godot complains every frame), so this view
## takes the headset's frames instead: a viewport of its own, with a world of its
## own, marked for the headset. The main window goes back to the desktop.
##
## Pulling a trigger asks to carry on in the headset (`return_requested`). If the
## headset's software ends the session meanwhile (its menu's Quit), `ended` says so:
## the app keeps running on the desktop and stops offering the headset.
##
## Nothing in this file has run on a headset yet: see `docs/xr.md`.

## A trigger was pulled: the user wants to carry on in the headset.
signal return_requested
## The headset's software ended the session.
signal ended

const MESSAGE := "Limner is on the desktop.\n\nPull a trigger to carry on here."
## How far in front of the eyes the sheet stands, in metres, and its size.
const SHEET_DISTANCE := 1.4
const SHEET_SIZE := Vector2(0.9, 0.42)
## Where the sheet stands until the head is first seen, in the room, in metres.
const SHEET_REST := Vector3(0.0, 1.5, -1.4)
## Height of the text in the room, in metres per pixel of its 48-pixel font.
const TEXT_PIXEL := 0.0011
## How far a trigger must be pulled to count, and let go before it can count again.
const TRIGGER_PULL := 0.8
const TRIGGER_LET_GO := 0.2
const BACKGROUND := Color(0.11, 0.115, 0.125)
const FLOOR := Color(0.16, 0.165, 0.18)
const SHEET := Color(0.93, 0.93, 0.92)
const INK := Color(0.12, 0.12, 0.13)

var interface: OpenXRInterface
var head: XRCamera3D
var sheet: Node3D

var _hands: Array[XRController3D] = []
## For each hand, whether its trigger has been let go since the view appeared: a
## trigger still held from the press that left the headset must not bring it back.
var _armed: Array[bool] = [false, false]
var _placed := false
var _asked := false


func _init() -> void:
	name = "HeadsetHoldingView"
	own_world_3d = true
	render_target_update_mode = SubViewport.UPDATE_ALWAYS


func _ready() -> void:
	interface = OpenXRRig.find_interface()
	if interface != null:
		var eye := Vector2i(interface.get_render_target_size())
		if interface.is_initialized() and eye.x > 0 and eye.y > 0:
			size = eye
		# Only a running OpenXR takes the frames. Marked without one, the viewport
		# would draw nothing ever after, even once unmarked.
		use_xr = interface.is_initialized()
		interface.session_stopping.connect(_on_session_ending)
		interface.session_loss_pending.connect(_on_session_ending)
		interface.instance_exiting.connect(_on_session_ending)
		interface.pose_recentered.connect(_place_sheet)
	var environment := Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = BACKGROUND
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	var world_environment := WorldEnvironment.new()
	world_environment.environment = environment
	add_child(world_environment)

	var origin := XROrigin3D.new()
	origin.name = "Origin"
	add_child(origin)
	head = XRCamera3D.new()
	head.name = "Head"
	head.near = 0.01
	head.far = 100.0
	origin.add_child(head)
	for side in 2:
		var hand := XRController3D.new()
		hand.name = "%sAim" % XRRig.SIDE_NAMES[side].capitalize()
		hand.tracker = StringName("%s_hand" % XRRig.SIDE_NAMES[side])
		hand.pose = &"aim"
		origin.add_child(hand)
		_hands.append(hand)

	var ground := MeshInstance3D.new()
	ground.name = "Floor"
	var disc := CylinderMesh.new()
	disc.top_radius = 3.0
	disc.bottom_radius = 3.0
	disc.height = 0.01
	disc.material = _flat(FLOOR)
	ground.mesh = disc
	ground.position = Vector3(0.0, -0.005, 0.0)
	add_child(ground)

	sheet = Node3D.new()
	sheet.name = "Sheet"
	sheet.position = SHEET_REST
	add_child(sheet)
	var card := MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = SHEET_SIZE
	quad.material = _flat(SHEET)
	card.mesh = quad
	sheet.add_child(card)
	var text := Label3D.new()
	text.text = MESSAGE
	text.font_size = 48
	text.pixel_size = TEXT_PIXEL
	text.modulate = INK
	text.outline_size = 0
	text.shaded = false
	text.width = SHEET_SIZE.x * 0.85 / TEXT_PIXEL
	text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	text.position = Vector3(0.0, 0.0, 0.002)
	sheet.add_child(text)


func _process(_delta: float) -> void:
	# The head is only known once the headset tracks it: the sheet is then stood
	# in front of the eyes, once.
	if not _placed and head.position != Vector3.ZERO:
		_place_sheet()
	if _asked:
		return
	for side in 2:
		var hand := _hands[side]
		var pull := hand.get_float(&"trigger") if hand.get_is_active() else 0.0
		if pull < TRIGGER_LET_GO:
			_armed[side] = true
		elif _armed[side] and pull > TRIGGER_PULL:
			_asked = true
			return_requested.emit()
			return


## Stands the sheet in front of the eyes, upright and facing them.
func _place_sheet() -> void:
	if head == null or head.position == Vector3.ZERO:
		return
	_placed = true
	var ahead := -head.global_basis.z
	ahead.y = 0.0
	ahead = ahead.normalized() if ahead.length() > 0.01 else Vector3.FORWARD
	var centre := head.global_position + ahead * SHEET_DISTANCE
	sheet.global_transform = Transform3D(Basis.looking_at(ahead, Vector3.UP), centre)


func _on_session_ending() -> void:
	ended.emit()


func _flat(color: Color) -> StandardMaterial3D:
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.albedo_color = color
	return material
