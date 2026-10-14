class_name SettingsPanel
extends VBoxContainer
## The Settings panel. (Being built.)

var editor: LimnerEditor


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Settings"
	add_child(Kit.heading("Settings"))
