class_name ColorPanel
extends VBoxContainer
## The colours: the paint colour and the background, a picker that edits whichever
## of the two is selected, and the document's palette of sixteen swatches.
##
## The picker works in the hue, saturation and value of the colour as it looks on
## screen, and keeps those three itself. The document only stores the mixed colour,
## which cannot say what hue a grey had or how saturated black was, so the picker
## takes a colour from the document only when it is not the one it is showing.

enum Target { PAINT, BACKGROUND }

const SWATCHES := 16
## What every channel of a swatch holds until a colour is stored in it (linear RGB).
const UNUSED_SWATCH := 0.82
const AREA_HEIGHT := 140
## Colours closer than this in every channel look the same on screen.
const SAME_COLOR := 0.002

var editor: LimnerEditor

var _target := Target.PAINT
var _hue := 0.0
var _saturation := 0.0
var _value := 0.0
var _dragging := false
## What the picker showed when the drag under way began.
var _before_drag: Color
## Where "Add" stores a colour once no swatch is unused any more.
var _next_swatch := 0
var _bars := {}
var _square: PickerArea
var _strip: PickerArea
var _hue_row: SliderRow
var _saturation_row: SliderRow
var _value_row: SliderRow
var _hex: LineEdit
## True while the hex field holds something typed that has not been entered yet.
var _hex_typed := false
var _swatches: Array[ColorChip] = []


func _init(limner_editor: LimnerEditor) -> void:
	editor = limner_editor
	name = "Color"
	add_theme_constant_override("separation", ToolsPanel.SEPARATION)

	_bars[Target.PAINT] = ColorChip.new("Edit the paint colour, which Paint, Line and Colorize use", "Paint colour")
	_bars[Target.BACKGROUND] = ColorChip.new("Edit the background colour of the document", "Background colour")
	for target: Target in _bars:
		var bar: ColorChip = _bars[target]
		bar.chosen.connect(_select.bind(target))
	add_child(Kit.row([_bars[Target.PAINT], _bars[Target.BACKGROUND]]))

	_square = PickerArea.new(PickerArea.Kind.SATURATION_VALUE)
	_square.custom_minimum_size = Vector2(120, AREA_HEIGHT)
	_square.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_square.changed.connect(func(at: Vector2) -> void: _pick(_hue, at.x, at.y))
	_strip = PickerArea.new(PickerArea.Kind.HUE)
	_strip.custom_minimum_size = Vector2(28, AREA_HEIGHT)
	_strip.changed.connect(func(at: Vector2) -> void: _pick(at.y, _saturation, _value))
	for area: PickerArea in [_square, _strip]:
		area.drag_started.connect(_begin_edit)
		area.drag_ended.connect(_end_edit)
	add_child(Kit.row([_square, _strip]))

	_hue_row = SliderRow.new("Hue", 0.0, 360.0, 1.0, "Hue, as an angle round the colour wheel", func(degrees: float) -> String: return "%d°" % roundi(degrees))
	_hue_row.changed.connect(func(degrees: float) -> void: _pick(degrees / 360.0, _saturation, _value))
	_saturation_row = SliderRow.new("Saturation", 0.0, 1.0, 0.01, "From grey to the pure colour")
	_saturation_row.changed.connect(func(saturation: float) -> void: _pick(_hue, saturation, _value))
	_value_row = SliderRow.new("Value", 0.0, 1.0, 0.01, "From black to the full colour")
	_value_row.changed.connect(func(value: float) -> void: _pick(_hue, _saturation, value))
	for slider: SliderRow in [_hue_row, _saturation_row, _value_row]:
		slider.drag_started.connect(_begin_edit)
		slider.drag_ended.connect(_end_edit)
		add_child(slider)

	var hex_title := Kit.label("Hex")
	hex_title.custom_minimum_size = Vector2(ToolsPanel.TITLE_WIDTH, Kit.ROW)
	_hex = LineEdit.new()
	_hex.tooltip_text = "The colour as #rrggbb, as it looks on screen (sRGB)"
	_hex.custom_minimum_size = Vector2(0, Kit.ROW)
	_hex.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_hex.select_all_on_focus = true
	_hex.text_changed.connect(func(_text: String) -> void: _hex_typed = true)
	_hex.text_submitted.connect(_on_hex_submitted)
	_hex.focus_exited.connect(_on_hex_left)
	add_child(Kit.row([hex_title, _hex, Kit.button("Add", "Store this colour in the next swatch", _add_swatch, "plus")]))

	var grid := GridContainer.new()
	grid.columns = 8
	for index in SWATCHES:
		var swatch := ColorChip.new("Press to use this colour. Hold, or right-click, to replace it with the current colour.")
		swatch.holdable = true
		swatch.chosen.connect(_use_swatch.bind(index))
		swatch.held.connect(_store_swatch.bind(index))
		_swatches.append(swatch)
		grid.add_child(swatch)
	add_child(grid)


func _ready() -> void:
	editor.document_changed.connect(refresh)
	editor.state_changed.connect(refresh)
	editor.document_replaced.connect(refresh)
	refresh()


## Brings the bars, the swatches and the picker in line with the document.
func refresh() -> void:
	var colors := {
		Target.PAINT: LimnerEditor.screen_color(editor.get_tools().color),
		Target.BACKGROUND: LimnerEditor.screen_color(editor.get_info().background),
	}
	for target: Target in _bars:
		var bar: ColorChip = _bars[target]
		bar.color = colors[target]
		bar.selected = target == _target
	var palette: Array = editor.get_info().palette
	for index in SWATCHES:
		_swatches[index].color = LimnerEditor.screen_color(palette[index])

	var current: Color = colors[_target]
	if not _dragging and not _looks_same(current, _picked()):
		_adopt(current)
	_show_picker()


## The colour the picker shows, as it looks on screen.
func _picked() -> Color:
	return Color.from_hsv(_hue, _saturation, _value)


## Makes the paint colour or the background the one the picker edits.
func _select(target: Target) -> void:
	_target = target
	refresh()


## Takes a colour from outside the picker. A grey has no hue of its own, and black
## no saturation either, so those stay where the picker had them.
func _adopt(color: Color) -> void:
	_value = color.v
	if color.v > 0.0:
		_saturation = color.s
		if color.s > 0.0:
			_hue = color.h


func _looks_same(a: Color, b: Color) -> bool:
	return absf(a.r - b.r) < SAME_COLOR and absf(a.g - b.g) < SAME_COLOR and absf(a.b - b.b) < SAME_COLOR


func _show_picker() -> void:
	_square.hue = _hue
	_square.marker = Vector2(_saturation, _value)
	_strip.marker = Vector2(0.5, _hue)
	_hue_row.set_value(_hue * 360.0)
	_saturation_row.set_value(_saturation)
	_value_row.set_value(_value)
	# Text being typed is left alone; anything else in the field is replaced.
	var hex := "#" + _picked().to_html(false)
	if not _hex_typed and _hex.text != hex:
		_hex.text = hex


# ---- commands ----

## A change made in the picker: one step of a drag, or a whole edit.
func _pick(hue: float, saturation: float, value: float) -> void:
	_hue = hue
	_saturation = saturation
	_value = value
	_send()
	_show_picker()


## Sets the selected colour of the document to the one the picker shows.
func _send() -> void:
	var color := LimnerEditor.linear_rgb(_picked())
	if _target == Target.PAINT:
		editor.run({"cmd": "set_color", "color": color})
	else:
		editor.run({"cmd": "set_document", "background": color})


func _begin_edit() -> void:
	_dragging = true
	_before_drag = _picked()
	# The background belongs to the document, so a drag over it is one undo step.
	if _target == Target.BACKGROUND:
		editor.begin("Change background")


func _end_edit() -> void:
	if _target == Target.BACKGROUND:
		# A drag that ends on the colour it began with, such as one over the hue of a
		# grey, changed nothing and is not worth an undo step.
		if _picked().is_equal_approx(_before_drag):
			editor.cancel()
		else:
			editor.commit()
	_dragging = false


func _on_hex_submitted(text: String) -> void:
	_enter_hex(text)
	_hex.release_focus()


## Leaving the field enters what was typed in it, as pressing Enter does.
func _on_hex_left() -> void:
	if _hex_typed:
		_enter_hex(_hex.text)


## Sets the selected colour from text typed as `#rrggbb` (the `#` may be left out).
func _enter_hex(text: String) -> void:
	_hex_typed = false
	var digits := text.strip_edges().trim_prefix("#")
	if digits.length() != 6 or not Color.html_is_valid(digits):
		editor.message.emit("\"%s\" is not a colour written as #rrggbb" % text, true)
	elif digits.to_lower() != _picked().to_html(false):
		_adopt(Color.html(digits))
		_send()
	_show_picker()


## Makes a swatch the paint colour or the background, whichever is selected.
func _use_swatch(index: int) -> void:
	var palette: Array = editor.get_info().palette
	var color := LimnerEditor.screen_color(palette[index])
	if not color.is_equal_approx(_picked()):
		_adopt(color)
		_send()
		_show_picker()


## Stores the current colour in the first swatch that has never been used, or, when
## all have been, in the one after the swatch stored last.
func _add_swatch() -> void:
	var palette: Array = editor.get_info().palette
	for index in SWATCHES:
		var color: Array = palette[index]
		if color.all(func(channel: float) -> bool: return absf(channel - UNUSED_SWATCH) < 1e-4):
			_next_swatch = index
			break
	_store_swatch(_next_swatch)
	_next_swatch = (_next_swatch + 1) % SWATCHES


## Replaces a swatch with the current colour.
func _store_swatch(index: int) -> void:
	if editor.run({"cmd": "set_palette_color", "index": index, "color": LimnerEditor.linear_rgb(_picked())}) != null:
		# These are now the swatches every new document starts with.
		Preferences.shared().set_value("palette", editor.scene.document_info().palette)
