class_name TextField
extends LineEdit
## A one-line text box that reports its text once, when the user presses Enter or
## leaves the box, and only if they changed it.

signal committed(value: String)

var _value := ""
var _committing := false


func _init(placeholder := "", tooltip := "") -> void:
	placeholder_text = placeholder
	tooltip_text = tooltip
	custom_minimum_size = Vector2(60, Kit.ROW)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	text_submitted.connect(func(_text: String) -> void: _finish())
	focus_exited.connect(_finish)


## Shows `value` without reporting it as a change. While the user is typing it is
## kept for when they finish.
func set_value(value: String) -> void:
	_value = value
	if is_editing() and not _committing:
		return
	if text != value:
		text = value


func _finish() -> void:
	if text == _value:
		return
	_value = text
	_committing = true
	committed.emit(text)
	_committing = false


func _gui_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel") and is_editing():
		# Escape puts back the text that was there and leaves the box.
		text = _value
		release_focus()
		accept_event()
