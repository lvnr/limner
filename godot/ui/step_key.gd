class_name StepKey
extends VBoxContainer
## A key that steps through a few settings, one press each, with the setting written
## small under it: the tapers of the Paint tool (none, pressure, auto, both), the
## mirror (off, X, Y, Z). The key is down whenever the setting is not the first one,
## which is "off".
##
## Choices are pairs of a key string and the word shown for it.

signal chosen(key: String)

var key_button: Button

var _keys: Array[String] = []
var _words: Array[String] = []
var _index := 0
var _word: Label


func _init(icon: String, tooltip: String, choices: Array) -> void:
	add_theme_constant_override("separation", Kit.u(2))
	for choice: Array in choices:
		_keys.append(String(choice[0]))
		_words.append(String(choice[1]))
	key_button = Kit.key(icon, tooltip, _step, true)
	key_button.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	_word = Kit.label("", true)
	_word.add_theme_font_size_override("font_size", Kit.text_size("small"))
	_word.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	add_child(key_button)
	add_child(_word)
	_show()


## Shows `key` as the setting, without reporting it as a change.
func set_value(key: String) -> void:
	var found := _keys.find(key)
	if found >= 0:
		_index = found
	_show()


func get_value() -> String:
	return _keys[_index] if not _keys.is_empty() else ""


func _step() -> void:
	if _keys.is_empty():
		return
	_index = (_index + 1) % _keys.size()
	_show()
	chosen.emit(_keys[_index])


func _show() -> void:
	_word.text = _words[_index] if not _words.is_empty() else ""
	key_button.set_pressed_no_signal(_index > 0)
