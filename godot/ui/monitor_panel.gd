class_name MonitorPanel
extends VBoxContainer
## The Monitor panel. (Being built.)

var editor: LimnerEditor


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Monitor"
	add_child(Kit.heading("Monitor"))
