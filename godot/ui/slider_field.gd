class_name SliderField
extends HBoxContainer
## A labelled slider with a box beside it for typing the exact number.
##
## Reports changes the way `SliderRow` does: `drag_started`, then `changed` for every
## step, then `drag_ended`, so the owner can wrap a drag in one undo step. A typed
## number reports all three at once, and may go beyond the end of the slider.

signal changed(value: float)
signal drag_started
signal drag_ended

var slider: HSlider
var field: NumberField

var _dragging := false


## The slider runs from `minimum` to `maximum`; typing reaches up to `typed_maximum`
## when that is given. `unit` is shown after the number.
func _init(text: String, minimum: float, maximum: float, step: float, tooltip := "", unit := "", exponential := false, typed_maximum := NAN) -> void:
	tooltip_text = tooltip
	var title := Kit.label(text)
	title.custom_minimum_size = Vector2(Form.LABEL_WIDTH, Kit.ROW)
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
	slider.custom_minimum_size = Vector2(60, 20)
	slider.focus_mode = Control.FOCUS_NONE
	slider.tooltip_text = tooltip
	slider.scrollable = false
	add_child(slider)

	field = NumberField.new(tooltip, minimum, maximum if is_nan(typed_maximum) else typed_maximum)
	field.custom_minimum_size = Vector2(70, Kit.ROW)
	field.size_flags_horizontal = Control.SIZE_SHRINK_END
	add_child(field)
	if unit != "":
		var suffix := Kit.label(unit, true)
		suffix.custom_minimum_size = Vector2(22, 0)
		add_child(suffix)

	slider.drag_started.connect(_on_drag_started)
	slider.drag_ended.connect(_on_drag_ended)
	slider.value_changed.connect(_on_slider_changed)
	field.committed.connect(_on_typed)
	set_value(minimum)


## Shows `value` without reporting it as a change. Ignored while the user is dragging.
func set_value(value: float) -> void:
	if _dragging:
		return
	slider.set_value_no_signal(value)
	field.set_value(value)


## The number shown, which may lie beyond the end of the slider.
func get_value() -> float:
	return field.get_value()


func set_editable(editable: bool) -> void:
	slider.editable = editable
	field.editable = editable
	modulate.a = 1.0 if editable else 0.45


func _on_drag_started() -> void:
	_dragging = true
	drag_started.emit()


func _on_drag_ended(_value_changed: bool) -> void:
	_dragging = false
	drag_ended.emit()


func _on_slider_changed(value: float) -> void:
	field.set_value(value)
	if _dragging:
		changed.emit(value)
	else:
		_report(value)


func _on_typed(value: float) -> void:
	slider.set_value_no_signal(value)
	_report(value)


## A complete edit in one go: a click on the track, or a typed number.
func _report(value: float) -> void:
	drag_started.emit()
	changed.emit(value)
	drag_ended.emit()
