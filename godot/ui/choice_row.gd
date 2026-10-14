class_name ChoiceRow
extends HBoxContainer
## A row of buttons of which exactly one is down: a choice between a few values,
## each one press away.
##
## The buttons share the width of the row equally. Pressing one reports `chosen`
## with the key it was added under.

signal chosen(key: String)

## The buttons by key, in the order they were added.
var buttons := {}


func _init() -> void:
	add_theme_constant_override("separation", 2)


## Adds a choice shown as an icon.
func add_icon(key: String, icon: String, tooltip: String) -> Button:
	return _add(key, Kit.icon_button(icon, tooltip, Callable(), true))


## Adds a choice shown as a word.
func add_text(key: String, text: String, tooltip: String) -> Button:
	var button := Kit.button(text, tooltip, Callable())
	button.toggle_mode = true
	return _add(key, button)


## Shows `key` as the choice made, without reporting it as a change.
func set_value(key: String) -> void:
	for candidate: String in buttons:
		var button: Button = buttons[candidate]
		button.set_pressed_no_signal(candidate == key)


## The key of the button that is down, or an empty string.
func get_value() -> String:
	for candidate: String in buttons:
		var button: Button = buttons[candidate]
		if button.button_pressed:
			return candidate
	return ""


func _add(key: String, button: Button) -> Button:
	button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	button.pressed.connect(_on_pressed.bind(key))
	buttons[key] = button
	add_child(button)
	return button


func _on_pressed(key: String) -> void:
	# Pressing the button that is already down must leave it down.
	set_value(key)
	chosen.emit(key)
