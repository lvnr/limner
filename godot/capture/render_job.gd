class_name RenderJob
extends Node
## Renders a camera's view of the working range to a video or an image sequence, in a
## process of its own. (Being built.)

var world: LimnerWorld
var editor: LimnerEditor


## Starts the job described by the command-line arguments after `--render`.
func start(_arguments: PackedStringArray) -> void:
	push_error("Rendering is not available in this build yet")
	get_tree().quit(1)
