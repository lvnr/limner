class_name ExportPanel
extends VBoxContainer
## The Export panel. (Being built.)

var editor: LimnerEditor


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Export"
	add_child(Kit.heading("Export"))
