class_name SliderRow
extends HBoxContainer
## A labelled slider with its value shown beside it.
##
## Dragging reports `drag_started`, then `changed` for every step, then `drag_ended`,
## so the owner can wrap a drag in one undo step. A click on the track reports all
## three at once.

signal changed(value: float)
signal drag_started
signal drag_ended

var slider: HSlider

var _value: Label
var _format: Callable
var _dragging := false


## `format` turns a value into the text shown at the right; it defaults to a percentage.
func _init(text: String, minimum: float, maximum: float, step: float, tooltip := "", format := Callable(), exponential := false) -> void:
	_format = format
	var title := Kit.label(text)
	title.custom_minimum_size = Vector2(84, Kit.ROW)
	title.tooltip_text = tooltip
	title.mouse_filter = Control.MOUSE_FILTER_PASS
	add_child(title)

	slider = HSlider.new()
	slider.min_value = minimum
	slider.max_value = maximum
	slider.step = step
	slider.exp_edit = exponential
	slider.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	slider.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	slider.custom_minimum_size = Vector2(80, 20)
	slider.focus_mode = Control.FOCUS_NONE
	slider.tooltip_text = tooltip
	slider.scrollable = false
	add_child(slider)

	_value = Kit.label("", true)
	_value.custom_minimum_size = Vector2(52, 0)
	_value.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	add_child(_value)

	slider.drag_started.connect(_on_drag_started)
	slider.drag_ended.connect(_on_drag_ended)
	slider.value_changed.connect(_on_value_changed)
	_show(slider.value)


## Shows `value` without reporting it as a change. Ignored while the user is dragging.
func set_value(value: float) -> void:
	if _dragging:
		return
	slider.set_value_no_signal(value)
	_show(slider.value)


func get_value() -> float:
	return slider.value


func set_editable(editable: bool) -> void:
	slider.editable = editable
	modulate.a = 1.0 if editable else 0.45


func _show(value: float) -> void:
	_value.text = _format.call(value) if _format.is_valid() else "%d%%" % roundi(value * 100.0)


func _on_drag_started() -> void:
	_dragging = true
	drag_started.emit()


func _on_drag_ended(_value_changed: bool) -> void:
	_dragging = false
	drag_ended.emit()


func _on_value_changed(value: float) -> void:
	_show(value)
	if _dragging:
		changed.emit(value)
	else:
		# A click on the track, or a key press: a complete edit in one go.
		drag_started.emit()
		changed.emit(value)
		drag_ended.emit()
