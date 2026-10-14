class_name PerformanceMeter
extends VBoxContainer
## One measured quantity against what the chosen kind of device can take: a title,
## the reading, and a bar that fills toward the limit and turns red past it. A thin
## mark on the bar, and the line under it, show the highest reading since the peak
## was last reset.

const HEIGHT := 6
## Past this share of the limit the bar warns.
const NEAR := 0.8

## The highest value shown since `reset_peak`.
var peak := 0.0

var _title: Label
var _reading: Label
var _peak_label: Label
var _bar: Control
var _fill := 0.0
var _limit := 0.0


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
	_peak_label = Kit.label("", true)
	_peak_label.mouse_filter = Control.MOUSE_FILTER_PASS
	_peak_label.add_theme_font_size_override("font_size", 11)
	_peak_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	add_child(_peak_label)


## Shows `text` as the reading and `value` out of `limit` on the bar. A limit of
## zero means there is nothing to hold the value against. The peak goes up with it.
func show_reading(text: String, value: float, limit: float) -> void:
	_reading.text = text
	_limit = limit
	_fill = value / limit if limit > 0.0 else 0.0
	peak = maxf(peak, value)
	_bar.visible = limit > 0.0
	_bar.queue_redraw()


## Writes out the peak, in the units the reading is in.
func show_peak(text: String) -> void:
	_peak_label.text = text


## Starts the peak again from nothing; the next reading sets it.
func reset_peak() -> void:
	peak = 0.0
	_peak_label.text = ""
	_bar.queue_redraw()


## 0 when comfortably within the limit, 1 when near it, 2 when past it.
func level() -> int:
	return _level_of(_fill)


## The same for the peak.
func peak_level() -> int:
	return _level_of(peak / _limit if _limit > 0.0 else 0.0)


static func _level_of(share: float) -> int:
	return 2 if share > 1.0 else (1 if share > NEAR else 0)


func _draw_bar() -> void:
	var size := _bar.size
	_bar.draw_rect(Rect2(Vector2.ZERO, size), Kit.BG_SUNKEN)
	var colors: Array[Color] = [Kit.OK, Kit.WARN, Kit.DANGER]
	_bar.draw_rect(Rect2(0.0, 0.0, size.x * clampf(_fill, 0.0, 1.0), size.y), colors[level()])
	if _limit > 0.0 and peak > 0.0:
		var x := clampf(size.x * peak / _limit, 1.0, size.x - 1.0)
		_bar.draw_line(Vector2(x, -1.0), Vector2(x, size.y + 1.0), colors[peak_level()].lerp(Kit.INK, 0.3), 2.0)
