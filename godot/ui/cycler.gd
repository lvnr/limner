class_name Cycler
extends HBoxContainer
## A choice among many without a list: the value in the middle, a key at each side
## to step to the one before or after. What Quill's panels use where a desktop panel
## would open a drop-down, which in a headset is a list over the panel that scrolls.
##
## Choices are pairs of a key string and the words shown for it. Stepping stops at
## the ends unless `wraps` is set.

signal chosen(key: String)

var wraps := false

var _keys: Array[String] = []
var _texts: Array[String] = []
var _index := -1
var _before: Button
var _after: Button
var _value: Label


func _init(tooltip := "", choices: Array = []) -> void:
	add_theme_constant_override("separation", Kit.u(4))
	_before = Kit.key("previous", "Previous", _step.bind(-1))
	_after = Kit.key("next", "Next", _step.bind(1))
	var well := PanelContainer.new()
	well.theme_type_variation = "Well"
	well.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	well.tooltip_text = tooltip
	well.mouse_filter = Control.MOUSE_FILTER_PASS
	_value = Kit.label("")
	_value.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_value.clip_text = true
	_value.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	_value.custom_minimum_size = Vector2(Kit.u(40), 0)
	well.add_child(_value)
	add_child(_before)
	add_child(well)
	add_child(_after)
	if not choices.is_empty():
		set_choices(choices)


## Replaces the choices: a list of [key, text] pairs. Keeps the current choice when it
## is still among them.
func set_choices(choices: Array) -> void:
	var current := get_value()
	_keys.clear()
	_texts.clear()
	for choice: Array in choices:
		_keys.append(String(choice[0]))
		_texts.append(String(choice[1]))
	_index = _keys.find(current)
	_show()


## Shows `key` as the choice made, without reporting it as a change. A key that is not
## among the choices shows `text` instead (a custom value, say), or nothing.
func set_value(key: String, text := "") -> void:
	_index = _keys.find(key)
	_show(text)


## The key string of the choice shown, or an empty string.
func get_value() -> String:
	return _keys[_index] if _index >= 0 and _index < _keys.size() else ""


func _step(by: int) -> void:
	if _keys.is_empty():
		return
	var next := _index + by
	if _index < 0:
		next = 0 if by > 0 else _keys.size() - 1
	if wraps:
		next = posmod(next, _keys.size())
	next = clampi(next, 0, _keys.size() - 1)
	if next == _index:
		return
	_index = next
	_show()
	chosen.emit(_keys[_index])


func _show(text := "") -> void:
	_value.text = _texts[_index] if _index >= 0 and _index < _texts.size() else text
	_before.disabled = _keys.is_empty() or (not wraps and _index == 0)
	_after.disabled = _keys.is_empty() or (not wraps and _index == _keys.size() - 1)
