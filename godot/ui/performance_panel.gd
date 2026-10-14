class_name PerformancePanel
extends VBoxContainer
## The Performance panel. (Being built.)

var editor: LimnerEditor


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Performance"
	add_child(Kit.heading("Performance"))
