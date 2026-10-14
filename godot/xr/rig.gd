class_name XRRig
extends Node3D
## The viewer's head and hands in the room: where they are and what the hands press.
##
## This node is the room's origin and never moves; navigation moves the artwork, not
## the viewer. `OpenXRRig` fills it from a headset and `SimRig` from the keyboard and
## mouse or from a script. The headset shell reads either through this class alone,
## so everything after this point is the same code whichever it is.

## The session gained or lost the user's attention. Without it (the headset is off
## the head, or the system's own menu is open) the hands must not do anything.
signal attention_changed(has_attention: bool)
## The user asked the system to recentre the headset.
signal recentred
## The session is over and the app should close.
signal closed

const LEFT := 0
const RIGHT := 1
## What each side is called: in node names, and as the start of OpenXR's tracker names.
const SIDE_NAMES := ["left", "right"]
## How far in front of a controller's aim pose the brush tip is, in metres. The tip
## is where paint appears and what pokes the panels. This is a first guess made
## without a headset and MUST be tuned on real hardware: too short and the
## controller's own body hides the stroke, too long and the tip feels detached.
const TIP_OFFSET := 0.035

## The eyes.
var head: Camera3D
## For each side, a node at the controller's aim pose: at its front, with -Z along
## the direction it points and +Y up.
var aims: Array[Node3D] = []
## For each side, a node at the controller's grip pose: in the palm, with its Z axis
## along the handle. Only used to draw the controller.
var grips: Array[Node3D] = []
## For each side, what the controller is pressing, as of the last `poll`.
var states: Array[XRHandState] = [XRHandState.new(), XRHandState.new()]
## False while the session does not have the user's attention.
var has_attention := true


## Reads the controllers into `states`. Returns the time step to treat this frame as,
## in seconds: the real one, except when a script is driving a simulated rig.
func poll(delta: float) -> float:
	return delta


## Vibrates a controller. `strength` runs from 0 to 1.
func pulse(_side: int, _strength: float, _seconds: float) -> void:
	pass


## The brush tip of a hand in the room: its place, and its facing with -Z forward.
func tip(side: int) -> Transform3D:
	return aims[side].global_transform.translated_local(Vector3(0.0, 0.0, -TIP_OFFSET))


## The other side.
static func opposite(side: int) -> int:
	return LEFT if side == RIGHT else RIGHT
