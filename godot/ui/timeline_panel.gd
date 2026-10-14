class_name TimelinePanel
extends VBoxContainer
## The Timeline: playback, the working range, and every layer's clips, frames and keys.
## (Being built.)

var editor: LimnerEditor


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Timeline"
