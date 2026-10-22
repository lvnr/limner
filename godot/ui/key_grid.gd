class_name KeyGrid
extends GridContainer
## Keys in rows of `columns`, of which exactly one is down: a choice among a few,
## each one press away, as Quill's panels make it (never a drop-down list).
##
## Keys are added under a key string; pressing one reports `chosen` with it. Keys
## with words share the width of the grid; keys with icons keep their square size.

signal chosen(key: String)

## The keys by key string, in the order they were added.
var buttons := {}


func _init(per_row := 3) -> void:
	columns = per_row
	add_theme_constant_override("h_separation", Kit.u(Kit.SPACE))
	add_theme_constant_override("v_separation", Kit.u(Kit.SPACE))


## Adds a choice shown as an icon.
func add_icon(key: String, icon: String, tooltip: String) -> Button:
	return _add(key, Kit.key(icon, tooltip, Callable(), true))


## Adds a choice shown as a word, with an icon before it when `icon` is given.
func add_text(key: String, text: String, tooltip := "", icon := "") -> Button:
	var button := Kit.text_key(text, tooltip, Callable(), icon, true)
	button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return _add(key, button)


## Shows `key` as the choice made, without reporting it as a change.
func set_value(key: String) -> void:
	for candidate: String in buttons:
		var button: Button = buttons[candidate]
		button.set_pressed_no_signal(candidate == key)


## The key string of the key that is down, or an empty string.
func get_value() -> String:
	for candidate: String in buttons:
		var button: Button = buttons[candidate]
		if button.button_pressed:
			return candidate
	return ""


func _add(key: String, button: Button) -> Button:
	button.pressed.connect(_on_pressed.bind(key))
	buttons[key] = button
	add_child(button)
	return button


func _on_pressed(key: String) -> void:
	# Pressing the key that is already down must leave it down.
	set_value(key)
	chosen.emit(key)
