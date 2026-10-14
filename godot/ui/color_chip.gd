class_name ColorChip
extends BaseButton
## A patch of colour that can be pressed: one of the two colour bars of the Color
## panel, or a swatch of the palette.
##
## A short press reports `chosen`. A chip made `holdable` also reports `held` when it
## is kept down for half a second or pressed with the right mouse button; the
## release that follows a hold is not a choice.

signal chosen
signal held

const HOLD_SECONDS := 0.5
const FONT_SIZE := 13

## The colour shown, as it appears on screen.
var color := Color.GRAY:
	set(value):
		if color != value:
			color = value
			queue_redraw()
## Words written over the colour, if any.
var text := "":
	set(value):
		text = value
		update_minimum_size()
		queue_redraw()
## Draws a ring round the chip: it is the one being edited.
var selected := false:
	set(value):
		if selected != value:
			selected = value
			queue_redraw()
## Whether holding the chip down means something other than pressing it.
var holdable := false

var _timer: Timer
var _held := false


func _init(tooltip := "", label := "") -> void:
	text = label
	tooltip_text = tooltip
	focus_mode = Control.FOCUS_NONE
	custom_minimum_size = Vector2(24, Kit.ROW)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_timer = Timer.new()
	_timer.one_shot = true
	_timer.wait_time = HOLD_SECONDS
	_timer.timeout.connect(_on_hold)
	add_child(_timer)
	button_down.connect(_on_button_down)
	button_up.connect(_timer.stop)


func _get_minimum_size() -> Vector2:
	if text == "":
		return Vector2.ZERO
	return Vector2(get_theme_default_font().get_string_size(text, HORIZONTAL_ALIGNMENT_LEFT, -1, FONT_SIZE).x + 12.0, 0.0)


func _pressed() -> void:
	if not _held:
		chosen.emit()


func _gui_input(event: InputEvent) -> void:
	if holdable and event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_RIGHT and event.pressed:
		held.emit()
		accept_event()


func _on_button_down() -> void:
	_held = false
	if holdable:
		_timer.start()


func _on_hold() -> void:
	_held = true
	held.emit()


func _draw() -> void:
	var rect := Rect2(Vector2.ZERO, size)
	if get_draw_mode() in [DRAW_PRESSED, DRAW_HOVER_PRESSED]:
		rect = rect.grow(-1.5)
	draw_style_box(Kit._box(color, 4, 1), rect)
	if selected:
		# White edged with black shows on any colour, and against the panel.
		var white := Kit._box(Color.WHITE, 4, 2, Color.WHITE)
		white.draw_center = false
		draw_style_box(white, rect)
		var black := Kit._box(Color.BLACK, 3, 1, Color.BLACK)
		black.draw_center = false
		draw_style_box(black, rect.grow(-2.0))
	if text != "":
		var font := get_theme_default_font()
		var baseline := (size.y - font.get_height(FONT_SIZE)) * 0.5 + font.get_ascent(FONT_SIZE)
		var ink := Color.BLACK if color.get_luminance() > 0.4 else Color.WHITE
		draw_string(font, Vector2(0.0, baseline), text, HORIZONTAL_ALIGNMENT_CENTER, size.x, FONT_SIZE, ink)
