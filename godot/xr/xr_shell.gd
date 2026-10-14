class_name XRShell
extends Node
## The headset presentation. (Being built.)

var world: LimnerWorld
var editor: LimnerEditor


## True when a headset runtime is available and the shell could start in it.
static func is_available() -> bool:
	return false


## Fits everything visible in front of the viewer.
func frame_all() -> void:
	pass
