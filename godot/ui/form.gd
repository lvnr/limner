class_name Form
## Layout pieces shared by the panels that are lists of settings: a caption on the
## left with its control on the right, notes, and numbers shown as text.

## Width of the caption column, so that controls line up from row to row.
const LABEL_WIDTH := 96


## A caption followed by one or more controls.
static func row(text: String, controls: Array, tooltip := "") -> HBoxContainer:
	var title := Kit.label(text)
	title.custom_minimum_size = Vector2(LABEL_WIDTH, Kit.ROW)
	title.tooltip_text = tooltip
	title.mouse_filter = Control.MOUSE_FILTER_PASS
	var node := Kit.row([title])
	for control: Control in controls:
		node.add_child(control)
	return node


## A line of small dim text that wraps instead of widening the panel.
static func note(text := "") -> Label:
	var node := Kit.label(text, true)
	node.add_theme_font_size_override("font_size", 12)
	node.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	node.custom_minimum_size = Vector2(40, 0)
	node.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return node


## Text that fills the rest of a row and is cut short with an ellipsis when it does
## not fit; the tooltip then carries all of it.
static func value(text := "") -> Label:
	var node := Kit.label(text)
	node.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	node.custom_minimum_size = Vector2(20, Kit.ROW)
	node.clip_text = true
	node.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
	node.mouse_filter = Control.MOUSE_FILTER_PASS
	return node


## A table for lengths of time: captions on the left, then a column of boxes for
## seconds and one for frames, with those words above them. Fill it with `time_row`.
static func time_table() -> GridContainer:
	var table := GridContainer.new()
	table.columns = 3
	table.add_child(Control.new())
	for unit: String in ["seconds", "frames"]:
		var header := Kit.label(unit, true)
		header.add_theme_font_size_override("font_size", 12)
		table.add_child(header)
	return table


## Adds a time to a `time_table`: its caption and its two boxes.
static func time_row(table: GridContainer, text: String, seconds: NumberField, frames: NumberField, tooltip := "") -> void:
	var title := Kit.label(text)
	title.custom_minimum_size = Vector2(LABEL_WIDTH, Kit.ROW)
	title.tooltip_text = tooltip
	title.mouse_filter = Control.MOUSE_FILTER_PASS
	table.add_child(title)
	table.add_child(seconds)
	table.add_child(frames)


## A number box between a minus and a plus button, for changing a count without a
## keyboard. Returns the row; the box stays the caller's to read and set.
static func stepper(field: NumberField, tooltip: String) -> HBoxContainer:
	var less := Kit.icon_button("minus", "%s: one fewer" % tooltip, func() -> void: field.nudge(-1.0))
	var more := Kit.icon_button("plus", "%s: one more" % tooltip, func() -> void: field.nudge(1.0))
	field.alignment = HORIZONTAL_ALIGNMENT_CENTER
	var node := Kit.row([less, field, more])
	node.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	return node


## A whole number with thousands separators: 1234567 becomes "1,234,567".
static func thousands(number: int) -> String:
	var text := str(absi(number))
	var out := ""
	while text.length() > 3:
		out = "," + text.right(3) + out
		text = text.left(text.length() - 3)
	return ("-" if number < 0 else "") + text + out


## Makes controls usable or not, dimming those that are not. A row or column in the
## list stands for everything inside it.
static func set_enabled(controls: Array, enabled: bool) -> void:
	for control: Control in controls:
		if control is SliderField or control is SliderRow or control is ColorField:
			control.set_editable(enabled)
		elif control is Cycler:
			(control as Cycler).set_enabled(enabled)
		elif control is LineEdit or control is TextEdit:
			control.editable = enabled
		elif control is BaseButton:
			control.disabled = not enabled
			if control is CheckBox and not control.has_theme_stylebox_override("disabled"):
				# The theme draws a box behind a tick box that is switched off; leave it bare.
				control.add_theme_stylebox_override("disabled", StyleBoxEmpty.new())
		elif control is Container:
			set_enabled(control.get_children(), enabled)
