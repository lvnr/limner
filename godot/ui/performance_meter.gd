class_name PerformanceMeter
extends VBoxContainer
## One measured quantity against what the chosen kind of device can take: a title,
## the reading, and a bar that fills toward the limit and turns red past it.

const HEIGHT := 6
## Past this share of the limit the bar warns.
const NEAR := 0.8
const OK_COLOR := Color(0.35, 0.75, 0.45)
const NEAR_COLOR := Color(0.95, 0.72, 0.25)

var _title: Label
var _reading: Label
var _bar: Control
var _fill := 0.0


func _init(title: String, tooltip: String) -> void:
	add_theme_constant_override("separation", 2)
	tooltip_text = tooltip
	_title = Kit.label(title)
	_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_title.mouse_filter = Control.MOUSE_FILTER_PASS
	_reading = Kit.label("", true)
	_reading.mouse_filter = Control.MOUSE_FILTER_PASS
	add_child(Kit.row([_title, _reading]))
	_bar = Control.new()
	_bar.custom_minimum_size = Vector2(0, HEIGHT)
	_bar.mouse_filter = Control.MOUSE_FILTER_PASS
	_bar.draw.connect(_draw_bar)
	add_child(_bar)


## Shows `text` as the reading and `value` out of `limit` on the bar. A limit of
## zero means there is nothing to hold the value against.
func show_reading(text: String, value: float, limit: float) -> void:
	_reading.text = text
	_fill = value / limit if limit > 0.0 else 0.0
	_bar.visible = limit > 0.0
	_bar.queue_redraw()


## 0 when comfortably within the limit, 1 when near it, 2 when past it.
func level() -> int:
	return 2 if _fill > 1.0 else (1 if _fill > NEAR else 0)


func _draw_bar() -> void:
	var size := _bar.size
	_bar.draw_rect(Rect2(Vector2.ZERO, size), Kit.BG_SUNKEN)
	var color: Color = [OK_COLOR, NEAR_COLOR, Kit.DANGER][level()]
	_bar.draw_rect(Rect2(0.0, 0.0, size.x * clampf(_fill, 0.0, 1.0), size.y), color)
