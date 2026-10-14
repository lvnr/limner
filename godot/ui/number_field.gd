class_name NumberField
extends LineEdit
## A box for typing a number.
##
## The number is reported once, when the user presses Enter or leaves the box, and
## only if they changed it. While the user is typing, the box keeps their text
## whatever happens in the document, and catches up when they are done.

signal committed(value: float)

## How many significant digits a number is shown with.
const DIGITS := 6

## The range typed numbers are brought into.
var minimum := -INF
var maximum := INF
## True when only whole numbers make sense.
var whole := false

var _value := 0.0
var _shown := ""
var _committing := false


func _init(tooltip := "", lowest := -INF, highest := INF, whole_numbers := false) -> void:
	minimum = lowest
	maximum = highest
	whole = whole_numbers
	tooltip_text = tooltip
	custom_minimum_size = Vector2(44, Kit.ROW)
	# Let the box be as narrow as its row needs; it widens with the panel.
	add_theme_constant_override("minimum_character_width", 2)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	select_all_on_focus = true
	virtual_keyboard_type = LineEdit.KEYBOARD_TYPE_NUMBER_DECIMAL
	text_submitted.connect(func(_text: String) -> void: _finish())
	focus_exited.connect(_finish)
	_show()


## Shows `value` without reporting it as a change. While the user is typing it is
## kept for when they finish.
func set_value(value: float) -> void:
	_value = value
	if is_editing() and not _committing:
		return
	_show()


## The number last shown or typed, at full precision.
func get_value() -> float:
	return _value


## `value` as text: six significant digits, no trailing zeros, and a power of ten
## where that is shorter for a very large number or needed for a very small one.
static func format(value: float) -> String:
	if value == 0.0 or not is_finite(value):
		return "0"
	var exponent := floori(log(absf(value)) / log(10.0))
	if exponent >= 7 or exponent < -4:
		var mantissa := snappedf(value / pow(10.0, exponent), pow(10.0, 1 - DIGITS))
		if absf(mantissa) >= 10.0:
			mantissa /= 10.0
			exponent += 1
		elif absf(mantissa) < 1.0:
			mantissa *= 10.0
			exponent -= 1
		var powered := "%se%d" % [_plain(mantissa, DIGITS - 1), exponent]
		var whole_number := _plain(value, 0)
		return powered if exponent < 0 or powered.length() < whole_number.length() else whole_number
	return _plain(value, maxi(DIGITS - 1 - exponent, 0))


## `value` with at most `decimals` decimal places and no zeros it does not need.
static func _plain(value: float, decimals: int) -> String:
	var text := String.num(value, decimals)
	if text.contains("."):
		text = text.rstrip("0").rstrip(".")
	return "0" if text == "-0" else text


## Changes the number by `amount` and reports it, as the buttons of a stepper do.
func nudge(amount: float) -> void:
	var value := clampf(_value + amount, minimum, maximum)
	if value == _value:
		return
	_value = value
	_show()
	committed.emit(_value)


func _show() -> void:
	text = format(_value)
	_shown = text


## Takes what was typed, if anything was, and reports it.
func _finish() -> void:
	var typed := text.strip_edges().replace(",", ".")
	if text == _shown or not typed.is_valid_float():
		_show()
		return
	var value := clampf(typed.to_float(), minimum, maximum)
	_value = roundf(value) if whole else value
	_show()
	_committing = true
	committed.emit(_value)
	_committing = false


func _gui_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel") and is_editing():
		# Escape puts back the number that was there and leaves the box.
		_show()
		release_focus()
		accept_event()
