class_name TimelineView
extends Control
## The tracks of the timeline, drawn directly: the layer names, the ruler with the
## working range and the playhead, and every layer's clips, frames and keys.
##
## Only what is in view is drawn. Everything shown is read from the panel, which
## reads it from the editor; what is selected lives in the panel too. Presses and
## drags arrive through `press`, `drag` and `release`, whatever device they come from.

const ROW := Kit.ROW
const RULER := 30.0
## The strip at the foot of the ruler that shows the working range and holds its ends.
const RANGE_BAND := 12.0
const BAR := 14.0
## Room left of time zero, so that what sits at zero can be seen and pressed.
const PAD := 12.0
const INDENT := 10.0
const SNAP_PIXELS := 8.0
const DRAG_THRESHOLD := 4.0
## Frames are drawn one by one when each is at least this wide.
const MIN_FRAME_PIXELS := 6.0
const KEY_REACH := 9.0
const EDGE_REACH := 8.0
## While a drag holds the pointer this close to an edge of the tracks, or past it,
## the view scrolls by itself.
const SCROLL_ZONE := 28.0
## Pixels a second the view scrolls with the pointer on the edge itself; it goes
## faster in step with how far into the zone, or past the edge, the pointer is.
const SCROLL_SPEED := 450.0
const SCROLL_SPEED_MAX := 5000.0
## Pixels a unit of a trackpad's two-finger pan moves the view. Godot on macOS makes
## a unit of about 33 points of finger travel, so that the content follows the fingers.
const PAN_PIXELS := 33.0
const PLAYHEAD := Color(1.0, 0.58, 0.12)
const CHOSEN := Color(1.0, 0.85, 0.32)
const KIND_COLORS := {
	"paint": Color(0.22, 0.40, 0.72),
	"group": Color(0.40, 0.42, 0.50),
	"sequence": Color(0.14, 0.50, 0.49),
	"picture": Color(0.50, 0.36, 0.62),
	"model": Color(0.48, 0.42, 0.32),
	"sound": Color(0.27, 0.51, 0.30),
	"viewpoint": Color(0.60, 0.44, 0.22),
	"camera": Color(0.60, 0.34, 0.34),
	"light": Color(0.60, 0.54, 0.22),
}
const MARK_COLORS := {"stop": Color(0.94, 0.36, 0.34), "play": Color(0.38, 0.64, 1.0), "loop": Color(0.44, 0.80, 0.46)}
const MARK_LETTERS := {"stop": "S", "play": "P", "loop": "L"}
const TRACKS := ["transform", "opacity", "action"]

var panel: TimelinePanel
var editor: LimnerEditor
## Zoom and scroll along time.
var axis := TimelineScale.new()
## How far the rows are scrolled, in pixels.
var scroll_y := 0.0
## Width of the column of layer names.
var names_width := 150.0
## The root-timeline time the playhead is drawn at.
var playhead := 0

var _h_bar: HScrollBar
var _v_bar: VScrollBar
var _overlay: Control
var _fitted := false
var _syncing := false
## What the pointer is doing between a press and its release. Empty when nothing.
var _drag := {}
## The rectangle being dragged out to select keys.
var _band := Rect2()


func _init(timeline_panel: TimelinePanel) -> void:
	panel = timeline_panel
	editor = panel.editor
	name = "Tracks"
	clip_contents = true
	focus_mode = Control.FOCUS_NONE
	custom_minimum_size = Vector2(200, RULER + ROW * 3 + BAR)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	size_flags_vertical = Control.SIZE_EXPAND_FILL

	_overlay = Control.new()
	_overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_overlay.set_anchors_preset(Control.PRESET_FULL_RECT)
	_overlay.draw.connect(_draw_overlay)
	add_child(_overlay)
	_h_bar = HScrollBar.new()
	_h_bar.step = 0.0001
	_h_bar.tooltip_text = "Scroll along time"
	_h_bar.value_changed.connect(_on_h_scrolled)
	_h_bar.gui_input.connect(_on_bar_input.bind(_h_bar))
	add_child(_h_bar)
	_v_bar = VScrollBar.new()
	_v_bar.step = 1.0
	_v_bar.tooltip_text = "Scroll the layers"
	_v_bar.value_changed.connect(_on_v_scrolled)
	_v_bar.gui_input.connect(_on_bar_input.bind(_v_bar))
	add_child(_v_bar)


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		_layout()


func _layout() -> void:
	names_width = clampf(size.x * 0.2, 96.0, 190.0)
	axis.left = names_width + PAD
	_h_bar.position = Vector2(names_width, size.y - BAR)
	_h_bar.size = Vector2(maxf(size.x - names_width - BAR, 0.0), BAR)
	_v_bar.position = Vector2(size.x - BAR, RULER)
	_v_bar.size = Vector2(BAR, maxf(size.y - RULER - BAR, 0.0))
	if not _fitted and size.x > names_width + 80.0 and not panel.info.is_empty():
		_fitted = true
		fit()
	refresh()


# ---- geometry ----

## The part of the view the tracks are drawn in: right of the names, under the ruler.
func tracks_rect() -> Rect2:
	return Rect2(names_width, RULER, maxf(size.x - names_width - BAR, 0.0), maxf(size.y - RULER - BAR, 0.0))


## Where a time on the root timeline is drawn.
func x_of(time: int) -> float:
	return axis.x_of(time)


## The time on the root timeline drawn at `x`.
func time_at(x: float) -> int:
	return axis.time_at(x)


## The top of row number `index` (rows are the panel's `rows`).
func row_top(index: int) -> float:
	return RULER + index * ROW - scroll_y


## The number of the row drawn at `y`; it may be past either end of the list.
func row_index_at(y: float) -> int:
	return floori((y - RULER + scroll_y) / ROW)


## The number of the row that shows layer `id`, or -1.
func row_of(id: int) -> int:
	for index in panel.rows.size():
		var row: Dictionary = panel.rows[index]
		if int(row.id) == id:
			return index
	return -1


## The middle of clip number `clip` of layer `id` as drawn, for pressing it.
func clip_center(id: int, clip: int) -> Vector2:
	var line: Dictionary = panel.timeline.get(id, {})
	var entry: Dictionary = line.clips[clip]
	var from := x_of(int(line.origin) + int(entry.start))
	var to := from + 80.0 if entry.end == null else x_of(int(line.origin) + int(entry.end))
	return Vector2((from + to) * 0.5, row_top(row_of(id)) + ROW * 0.5)


## Where the key at `time` (on the layer's own clock) of layer `id` is drawn.
func key_position(id: int, track: String, time: int) -> Vector2:
	var line: Dictionary = panel.timeline.get(id, {})
	return Vector2(x_of(int(line.origin) + time), row_top(row_of(id)) + ROW * 0.5)


## True between a press on the tracks or the ruler and its release.
func is_dragging() -> bool:
	return not _drag.is_empty()


## What the drag under way does: "scrub", "range_start", "range_end", "clip", "keys"
## or "band"; empty when there is none.
func drag_kind() -> String:
	return str(_drag.get("kind", ""))


# ---- zoom and scroll ----

## Called by the panel when anything that is drawn here changed.
func refresh() -> void:
	var rate: Dictionary = panel.info.get("rate", {"num": 24, "den": 1})
	axis.rate_num = maxi(int(rate.num), 1)
	axis.rate_den = maxi(int(rate.den), 1)
	_sync_bars()
	queue_redraw()
	_overlay.queue_redraw()


## Moves the playhead line. With `follow`, the view pages along when it leaves.
func set_playhead(time: int, follow := false) -> void:
	if time == playhead:
		return
	playhead = time
	if follow and _drag.is_empty():
		reveal(time)
	_overlay.queue_redraw()


## Scrolls along time, if needed, so that `time` is in view.
func reveal(time: int) -> void:
	var area := tracks_rect()
	var x := x_of(time)
	if x >= area.position.x and x <= area.end.x - 6.0:
		return
	var seconds := float(time) / TimelineScale.FLICKS
	var visible_seconds := maxf(area.size.x - PAD, 1.0) / axis.zoom
	axis.start = maxf(seconds - visible_seconds * (0.08 if x > area.position.x else 0.5), 0.0)
	refresh()


## Zooms about the playhead when it is in view, otherwise about the middle.
func zoom_by(factor: float) -> void:
	var area := tracks_rect()
	var x := x_of(playhead)
	if x < area.position.x or x > area.end.x:
		x = area.get_center().x
	zoom_at(x, factor)


func zoom_at(x: float, factor: float) -> void:
	axis.zoom_about(x, factor)
	refresh()


## Shows everything: the working range and all that is animated.
func fit() -> void:
	var end := maxi(maxi(panel.content_end(), int(panel.info.get("range_end", 0))), playhead)
	axis.fit(roundi(end * 1.04), tracks_rect().size.x - PAD * 2.0)
	scroll_y = 0.0
	refresh()


func pan_pixels(pixels: float) -> void:
	axis.start = maxf(axis.start + pixels / axis.zoom, 0.0)
	refresh()


func scroll_rows(pixels: float) -> void:
	scroll_y += pixels
	refresh()


func _sync_bars() -> void:
	var area := tracks_rect()
	_syncing = true
	var visible_seconds := maxf(area.size.x - PAD, 1.0) / axis.zoom
	var content := float(maxi(maxi(panel.content_end(), int(panel.info.get("range_end", 0))), playhead)) / TimelineScale.FLICKS
	_h_bar.max_value = maxf(content * 1.2 + visible_seconds * 0.25, axis.start + visible_seconds)
	_h_bar.page = visible_seconds
	_h_bar.value = axis.start
	var rows_height := float(panel.rows.size() * ROW)
	scroll_y = clampf(scroll_y, 0.0, maxf(rows_height - area.size.y, 0.0))
	_v_bar.max_value = maxf(rows_height, area.size.y)
	_v_bar.page = area.size.y
	_v_bar.value = scroll_y
	_syncing = false


func _on_h_scrolled(value: float) -> void:
	if _syncing:
		return
	axis.start = maxf(value, 0.0)
	queue_redraw()
	_overlay.queue_redraw()


func _on_v_scrolled(value: float) -> void:
	if _syncing:
		return
	scroll_y = value
	queue_redraw()


# ---- drawing ----

func _draw() -> void:
	var font := get_theme_default_font()
	var area := tracks_rect()
	draw_rect(Rect2(Vector2.ZERO, size), Kit.BG_SUNKEN)
	var rows := panel.rows
	var first := maxi(floori(scroll_y / ROW), 0)
	var last := mini(ceili((scroll_y + area.size.y) / ROW), rows.size())
	var from := time_at(area.position.x)
	var to := time_at(area.end.x)
	for index in range(first, last):
		_draw_track(index, rows[index], area, from, to, font)
	_draw_range_shade(area)
	_draw_mark_lines(area, from, to)
	if _band.has_area():
		draw_rect(_band, Color(Kit.ACCENT, 0.18))
		draw_rect(_band, Color(Kit.ACCENT, 0.9), false, 1.0)
	for index in range(first, last):
		_draw_name(index, rows[index], font)
	draw_rect(Rect2(0.0, area.end.y, size.x, BAR), Kit.BG_SUNKEN)
	_draw_ruler(area, from, to, font)
	draw_rect(Rect2(0.0, 0.0, names_width, RULER), Kit.BG_RAISED.darkened(0.3))
	draw_string(font, Vector2(8.0, RULER * 0.5 + 4.0), "%d layers" % rows.size(), HORIZONTAL_ALIGNMENT_LEFT, names_width - 12.0, 11, Kit.TEXT_DIM)
	draw_line(Vector2(names_width - 0.5, 0.0), Vector2(names_width - 0.5, size.y), Kit.BORDER)
	draw_line(Vector2(0.0, RULER - 0.5), Vector2(size.x, RULER - 0.5), Kit.BORDER)


func _draw_track(index: int, row: Dictionary, area: Rect2, from: int, to: int, font: Font) -> void:
	var id := int(row.id)
	var top := row_top(index)
	if row.current:
		draw_rect(Rect2(area.position.x, top, area.size.x, ROW), Color(Kit.ACCENT, 0.14))
	elif row.selected:
		draw_rect(Rect2(area.position.x, top, area.size.x, ROW), Color(Kit.ACCENT, 0.07))
	elif index % 2 == 1:
		draw_rect(Rect2(area.position.x, top, area.size.x, ROW), Color(1.0, 1.0, 1.0, 0.02))
	draw_line(Vector2(area.position.x, top + ROW - 0.5), Vector2(area.end.x, top + ROW - 0.5), Color(0.0, 0.0, 0.0, 0.25))
	var line: Dictionary = panel.timeline.get(id, {})
	if line.is_empty():
		return
	var origin := int(line.origin)
	var is_root: bool = row.parent == null
	var base: Color = KIND_COLORS.get(row.kind, KIND_COLORS.group)
	if not row.visible:
		base = base.lerp(Kit.BG_RAISED, 0.6)
	var clips: Array = line.clips
	for number in clips.size():
		var clip: Dictionary = clips[number]
		var start := origin + int(clip.start)
		var open: bool = clip.end == null
		var end := to + 1 if open else origin + int(clip.end)
		if end < from or start > to:
			continue
		var edge := x_of(start)
		var left := maxf(edge, area.position.x - 6.0)
		var right := area.end.x + 6.0 if open else minf(x_of(end), area.end.x + 6.0)
		var bar := Rect2(left, top + 4.0, maxf(right - left, 2.0), ROW - 8.0)
		var chosen := panel.selected_clip_layer == id and panel.selected_clip == number
		var fill := base.lightened(0.22) if chosen else base
		draw_rect(bar, Color(fill, 0.4) if is_root else fill)
		match str(row.kind):
			"group":
				_draw_hatch(bar, edge, Color(1.0, 1.0, 1.0, 0.16))
			"sequence":
				if not is_root:
					_draw_chevrons(bar, edge, Color(1.0, 1.0, 1.0, 0.22))
				_draw_loop_points(line, clip, bar, start, end)
			"paint":
				_draw_frames(id, line, clip, bar, start, end, from, to, fill)
		if chosen:
			draw_rect(bar.grow(-0.5), Color.WHITE, false, 2.0)
		elif not is_root:
			draw_rect(bar.grow(-0.5), fill.darkened(0.5), false, 1.0)
			# The grips at the ends, where a clip is trimmed.
			if bar.size.x > 28.0:
				if edge >= area.position.x:
					draw_rect(Rect2(left + 2.5, bar.position.y + 5.0, 2.0, bar.size.y - 10.0), Color(1.0, 1.0, 1.0, 0.28))
				if not open and right <= area.end.x:
					draw_rect(Rect2(right - 4.5, bar.position.y + 5.0, 2.0, bar.size.y - 10.0), Color(1.0, 1.0, 1.0, 0.28))
	_draw_opacity_keys(id, line, top, area, from, to)
	_draw_transform_keys(id, line, top, from, to)
	_draw_marks(id, line, top, from, to, font)


## Slanted lines over the clip of a group.
func _draw_hatch(bar: Rect2, anchor: float, color: Color) -> void:
	var height := bar.size.y
	var points := PackedVector2Array()
	var x := bar.position.x - height + fposmod(anchor - bar.position.x, 9.0)
	while x < bar.end.x:
		var a := Vector2(x, bar.end.y)
		var b := Vector2(x + height, bar.position.y)
		if a.x < bar.position.x:
			a = Vector2(bar.position.x, bar.end.y - (bar.position.x - x))
		if b.x > bar.end.x:
			b = Vector2(bar.end.x, bar.position.y + (x + height - bar.end.x))
		if a.x < b.x:
			points.append(a)
			points.append(b)
		x += 9.0
	if points.size() >= 2:
		draw_multiline(points, color, 1.0)


## Arrowheads over the clip of a sequence: time runs on its own clock in there.
func _draw_chevrons(bar: Rect2, anchor: float, color: Color) -> void:
	var points := PackedVector2Array()
	var middle := bar.get_center().y
	var x := bar.position.x + fposmod(anchor - bar.position.x, 14.0) + 5.0
	while x + 5.0 < bar.end.x:
		points.append(Vector2(x, bar.position.y + 4.0))
		points.append(Vector2(x + 5.0, middle))
		points.append(Vector2(x + 5.0, middle))
		points.append(Vector2(x, bar.end.y - 4.0))
		x += 14.0
	if points.size() >= 2:
		draw_multiline(points, color, 1.0)


## Where a looping sequence starts over: a bright mark at the loop point, fainter
## ones at each repeat after it.
func _draw_loop_points(line: Dictionary, clip: Dictionary, bar: Rect2, start: int, end: int) -> void:
	if line.sequence == null:
		return
	var loop := int(line.sequence.loop_duration)
	if loop <= 0:
		return
	var zero := start - int(clip.trim)
	var step := maxf(float(loop) / TimelineScale.FLICKS * axis.zoom, 0.001)
	@warning_ignore("integer_division")
	var number := maxi((time_at(bar.position.x) - zero) / loop, 1)
	while true:
		var time := zero + number * loop
		if time >= end:
			break
		var x := x_of(time)
		if x > bar.end.x:
			break
		if x >= bar.position.x and (number == 1 or step >= 6.0):
			var color := Color(1.0, 1.0, 1.0, 0.95 if number == 1 else 0.35)
			draw_line(Vector2(x, bar.position.y), Vector2(x, bar.end.y), color, 2.0 if number == 1 else 1.0)
			if number == 1:
				draw_primitive(PackedVector2Array([Vector2(x - 6.0, bar.position.y), Vector2(x, bar.position.y), Vector2(x, bar.position.y + 6.0)]), PackedColorArray([color, color, color]), PackedVector2Array())
		if step < 6.0:
			break
		number += 1


## The frames of a paint layer inside one of its clips.
func _draw_frames(id: int, line: Dictionary, clip: Dictionary, bar: Rect2, start: int, end: int, from: int, to: int, fill: Color) -> void:
	if line.paint == null:
		return
	var paint: Dictionary = line.paint
	var frames: Array = paint.frames
	var count := frames.size()
	var duration := int(paint.frame_duration)
	# A layer with one frame is a still picture: its clip says all there is to say.
	if count < 2 or duration <= 0:
		return
	var width := float(duration) / TimelineScale.FLICKS * axis.zoom
	# The root-timeline time at which the first frame begins; a trimmed clip starts later.
	var zero := start - int(clip.trim)
	var loops := int(paint.max_repeat) == 0
	if width < MIN_FRAME_PIXELS:
		# Too small to tell frames apart: show how far the drawn animation reaches.
		var reach := minf(x_of(mini(zero + count * duration, end)), bar.end.x)
		if reach > bar.position.x:
			draw_rect(Rect2(bar.position.x, bar.position.y + 3.0, reach - bar.position.x, bar.size.y - 6.0), Color(1.0, 1.0, 1.0, 0.16))
		return
	@warning_ignore("integer_division")
	var first := maxi((maxi(start, from) - zero) / duration, 0)
	@warning_ignore("integer_division")
	var last := (mini(end, to) - zero + duration - 1) / duration
	if not loops:
		last = mini(last, count)
	# The frame showing now, counted from the start of the clip's content.
	var current := -1
	var now := int(line.time)
	if now >= int(clip.start) and (clip.end == null or now < int(clip.end)) and panel.is_inside_clips(id):
		@warning_ignore("integer_division")
		current = int(line.local_time) / duration
		if not loops:
			current = mini(current, count - 1)
	var key_fill := fill.lightened(0.5)
	var hold_fill := fill.lightened(0.2)
	var empty_fill := fill.darkened(0.55)
	var middle := bar.position.y + bar.size.y * 0.5
	draw_rect(bar, fill.darkened(0.3))
	for number in range(first, last):
		var index := number % count
		var drawing := int(frames[index])
		var is_key := index == 0 or int(frames[index - 1]) != drawing
		var x := x_of(zero + number * duration)
		var cell_left := maxf(x, bar.position.x)
		var cell_right := minf(x + width, bar.end.x)
		if cell_right - cell_left < 0.5:
			continue
		var cell := Rect2(cell_left, bar.position.y + 1.0, cell_right - cell_left, bar.size.y - 2.0)
		# Repeats of a looping layer are the same drawings again: drawn dimmer.
		var strength := 1.0 if number < count else 0.55
		var empty := is_key and panel.is_empty_drawing(id, drawing)
		if empty:
			draw_rect(cell, Color(empty_fill, strength))
		else:
			draw_rect(cell, Color(key_fill if is_key else hold_fill, strength))
		if is_key and x >= bar.position.x:
			draw_line(Vector2(x + 0.5, bar.position.y), Vector2(x + 0.5, bar.end.y), Color(0.0, 0.0, 0.0, 0.6 * strength), 1.0)
			var dot := Vector2(x + minf(width, 16.0) * 0.5 + 0.5, middle + 4.5)
			var radius := clampf(width * 0.3, 2.0, 3.2)
			if empty:
				draw_arc(dot, radius, 0.0, TAU, 12, Color(1.0, 1.0, 1.0, 0.9 * strength), 1.4, true)
			else:
				draw_circle(dot, radius, Color(0.04, 0.06, 0.12, 0.9 * strength))
		elif not is_key:
			# A hold: the drawing before it goes on, shown as a line carried through.
			draw_line(Vector2(cell_left, middle + 4.5), Vector2(cell_right, middle + 4.5), Color(0.04, 0.06, 0.12, 0.55 * strength), 1.5)
			if width >= 10.0 and x >= bar.position.x:
				draw_line(Vector2(x + 0.5, bar.position.y + 7.0), Vector2(x + 0.5, bar.end.y - 7.0), Color(0.0, 0.0, 0.0, 0.2 * strength), 1.0)
		if number == current:
			draw_rect(cell, Color(PLAYHEAD, 0.55))
			draw_rect(cell.grow(-1.0), PLAYHEAD, false, 2.0)
	if not loops:
		# After the last frame, a layer that plays once goes on showing it.
		var tail := maxf(x_of(zero + count * duration), bar.position.x)
		if tail < bar.end.x - 4.0:
			draw_line(Vector2(tail + 3.0, middle), Vector2(bar.end.x - 3.0, middle), Color(1.0, 1.0, 1.0, 0.22), 1.0)


func _draw_opacity_keys(id: int, line: Dictionary, top: float, area: Rect2, from: int, to: int) -> void:
	var keys: Array = line.opacity_keys
	if keys.is_empty():
		return
	var origin := int(line.origin)
	var active := panel.key_track == "opacity"
	var chosen := _chosen_times(id, "opacity")
	var low := top + ROW - 6.0
	var high := top + 6.0
	var first := maxi(first_at_or_after(keys, from - origin) - 1, 0)
	var curve := PackedVector2Array()
	var before: Dictionary = keys[first]
	var x_before := x_of(origin + int(before.time))
	var y_before := lerpf(low, high, clampf(float(before.value), 0.0, 1.0))
	# Before its first key and after its last, a layer keeps the value of that key.
	var from_x := maxf(area.position.x, x_of(origin))
	if first == 0 and x_before > from_x:
		curve.append(Vector2(from_x, y_before))
	curve.append(Vector2(clampf(x_before, area.position.x - 8.0, area.end.x + 8.0), y_before))
	var last := first
	for number in range(first + 1, keys.size()):
		var key: Dictionary = keys[number]
		var x := x_of(origin + int(key.time))
		var y := lerpf(low, high, clampf(float(key.value), 0.0, 1.0))
		_append_eased(curve, x_before, y_before, x, y, str(before.interp), area)
		before = key
		x_before = x
		y_before = y
		last = number
		if int(key.time) > to - origin:
			break
	if last == keys.size() - 1 and x_before < area.end.x:
		curve.append(Vector2(area.end.x, y_before))
	if curve.size() >= 2:
		draw_polyline(curve, Color(1.0, 1.0, 1.0, 0.6 if active else 0.32), 1.0, true)
	var drawn := -100.0
	for number in range(first, last + 1):
		var key: Dictionary = keys[number]
		var time := int(key.time)
		var at := Vector2(x_of(origin + time), lerpf(low, high, clampf(float(key.value), 0.0, 1.0)))
		var is_chosen := chosen.has(time)
		if at.x - drawn < 1.5 and not is_chosen:
			continue
		drawn = at.x
		if is_chosen:
			draw_circle(at, 5.5, Color.WHITE)
			draw_circle(at, 4.0, CHOSEN)
		else:
			draw_circle(at, 4.2, Color(0.0, 0.0, 0.0, 0.7))
			draw_circle(at, 3.0, Color(1.0, 1.0, 1.0, 1.0 if active else 0.6))


## Adds the stretch of an opacity curve between two keys, as far as it is in view.
func _append_eased(curve: PackedVector2Array, x0: float, y0: float, x1: float, y1: float, interp: String, area: Rect2) -> void:
	var left := maxf(x0, area.position.x - 8.0)
	var right := minf(x1, area.end.x + 8.0)
	if right <= left or x1 <= x0:
		curve.append(Vector2(clampf(x1, area.position.x - 8.0, area.end.x + 8.0), y1))
		return
	if interp == "step":
		curve.append(Vector2(right, y0))
		if right == x1:
			curve.append(Vector2(x1, y1))
		return
	var steps := 1 if interp == "linear" else clampi(ceili((right - left) / 6.0), 1, 24)
	for step in range(1, steps + 1):
		var x := lerpf(left, right, float(step) / steps)
		var t := (x - x0) / (x1 - x0)
		curve.append(Vector2(x, lerpf(y0, y1, ease_of(interp, t))))


## How far along a value is at fraction `t` of the way to the next key.
static func ease_of(interp: String, t: float) -> float:
	match interp:
		"step":
			return 0.0 if t < 1.0 else 1.0
		"ease_in":
			return t * t * t
		"ease_out":
			return 1.0 - pow(1.0 - t, 3.0)
		"ease_in_out", "smooth":
			return t * t * (3.0 - 2.0 * t)
	return t


func _draw_transform_keys(id: int, line: Dictionary, top: float, from: int, to: int) -> void:
	var keys: Array = line.transform_keys
	if keys.is_empty():
		return
	var origin := int(line.origin)
	var active := panel.key_track == "transform"
	var chosen := _chosen_times(id, "transform")
	var reach := axis.duration_of(8.0)
	var middle := top + ROW * 0.5
	var drawn := -100.0
	for number in range(first_at_or_after(keys, from - origin - reach), keys.size()):
		var key: Dictionary = keys[number]
		var time := int(key.time)
		if time > to - origin + reach:
			break
		var x := x_of(origin + time)
		var is_chosen := chosen.has(time)
		if x - drawn < 1.5 and not is_chosen:
			continue
		drawn = x
		if is_chosen:
			_draw_diamond(Vector2(x, middle), 7.5, Color.WHITE)
			_draw_diamond(Vector2(x, middle), 5.5, CHOSEN)
		else:
			_draw_diamond(Vector2(x, middle), 6.5, Color(0.0, 0.0, 0.0, 0.75))
			_draw_diamond(Vector2(x, middle), 4.8, Color(1.0, 1.0, 1.0, 1.0 if active else 0.6))


## The selected key times of a layer as a set, when `track` is the kind being selected.
func _chosen_times(id: int, track: String) -> Dictionary:
	var chosen := {}
	if panel.key_track == track:
		for time: int in panel.selected_keys.get(id, []):
			chosen[time] = true
	return chosen


func _draw_diamond(at: Vector2, radius: float, color: Color) -> void:
	draw_primitive(
		PackedVector2Array([at + Vector2(0.0, -radius), at + Vector2(radius, 0.0), at + Vector2(0.0, radius), at + Vector2(-radius, 0.0)]),
		PackedColorArray([color, color, color, color]),
		PackedVector2Array()
	)


## Chapter marks on the root's row: a flag with the letter of what it does.
func _draw_marks(id: int, line: Dictionary, top: float, from: int, to: int, font: Font) -> void:
	var marks: Array = line.actions
	if marks.is_empty():
		return
	var origin := int(line.origin)
	var chosen := _chosen_times(id, "action")
	var reach := axis.duration_of(16.0)
	for number in range(first_at_or_after(marks, from - origin - reach), marks.size()):
		var mark: Dictionary = marks[number]
		var time := int(mark.time)
		if time > to - origin + reach:
			break
		var x := x_of(origin + time)
		var color: Color = MARK_COLORS.get(mark.action, Kit.TEXT)
		var flag := Rect2(x, top + 3.0, 13.0, 13.0)
		draw_line(Vector2(x + 0.5, top + 2.0), Vector2(x + 0.5, top + ROW - 2.0), color, 2.0)
		draw_rect(flag, color)
		if chosen.has(time):
			draw_rect(flag.grow(1.5), Color.WHITE, false, 2.0)
		draw_string(font, Vector2(x + 2.5, top + 13.5), MARK_LETTERS.get(mark.action, "?"), HORIZONTAL_ALIGNMENT_LEFT, -1.0, 11, Color(0.05, 0.05, 0.07))


## Chapter marks reach down across every track, since they stop or restart the whole piece.
func _draw_mark_lines(area: Rect2, from: int, to: int) -> void:
	var root := int(panel.info.get("root", -1))
	var line: Dictionary = panel.timeline.get(root, {})
	if line.is_empty():
		return
	var marks: Array = line.actions
	for number in range(first_at_or_after(marks, from), marks.size()):
		var mark: Dictionary = marks[number]
		if int(mark.time) > to:
			break
		var x := x_of(int(mark.time)) + 0.5
		var color: Color = MARK_COLORS.get(mark.action, Kit.TEXT)
		draw_line(Vector2(x, area.position.y), Vector2(x, area.end.y), Color(color, 0.45), 1.0)


## Dims the tracks outside the working range.
func _draw_range_shade(area: Rect2) -> void:
	var start := clampf(x_of(int(panel.info.get("range_start", 0))), area.position.x, area.end.x)
	var end := clampf(x_of(int(panel.info.get("range_end", 0))), area.position.x, area.end.x)
	var shade := Color(0.0, 0.0, 0.0, 0.28)
	if start > area.position.x:
		draw_rect(Rect2(area.position.x, area.position.y, start - area.position.x, area.size.y), shade)
	if end < area.end.x:
		draw_rect(Rect2(end, area.position.y, area.end.x - end, area.size.y), shade)
	for x: float in [start, end]:
		if x > area.position.x and x < area.end.x:
			draw_line(Vector2(x + 0.5, area.position.y), Vector2(x + 0.5, area.end.y), Color(Kit.ACCENT, 0.45), 1.0)


func _draw_name(index: int, row: Dictionary, font: Font) -> void:
	var top := row_top(index)
	var back := Kit.BG_RAISED.darkened(0.22)
	if row.current:
		back = Kit.ACCENT.darkened(0.25)
	elif row.selected:
		back = Kit.ACCENT.darkened(0.55)
	draw_rect(Rect2(0.0, top, names_width, ROW), back)
	draw_line(Vector2(0.0, top + ROW - 0.5), Vector2(names_width, top + ROW - 0.5), Color(0.0, 0.0, 0.0, 0.3))
	var color := Kit.TEXT if row.shown else Kit.TEXT_DIM
	if row.current:
		color = Color.WHITE
	var x := 5.0 + int(row.depth) * INDENT
	var middle := top + ROW * 0.5
	if int(row.children) > 0:
		# The arrow that folds a group away, as in the layer list.
		var arrow := PackedVector2Array([Vector2(x + 2.0, middle - 4.0), Vector2(x + 9.0, middle), Vector2(x + 2.0, middle + 4.0)])
		if not row.collapsed:
			arrow = PackedVector2Array([Vector2(x + 1.0, middle - 3.0), Vector2(x + 10.0, middle - 3.0), Vector2(x + 5.5, middle + 4.0)])
		draw_primitive(arrow, PackedColorArray([color, color, color]), PackedVector2Array())
	x += 14.0
	var icon: String = LayersPanel.KIND_ICONS.get(row.kind, "dot")
	if x + 16.0 < names_width:
		draw_texture_rect(Icons.texture(icon, color, 16), Rect2(x, middle - 8.0, 16.0, 16.0), false)
	x += 20.0
	if names_width - x > 12.0:
		var baseline := middle + (font.get_ascent(13) - font.get_descent(13)) * 0.5
		draw_string(font, Vector2(x, baseline), str(row.name), HORIZONTAL_ALIGNMENT_LEFT, names_width - x - 4.0, 13, color)


func _draw_ruler(area: Rect2, from: int, to: int, font: Font) -> void:
	var foot := RULER - RANGE_BAND
	draw_rect(Rect2(names_width, 0.0, size.x - names_width, RULER), Kit.BG_RAISED.darkened(0.3))
	draw_rect(Rect2(names_width, foot, size.x - names_width, RANGE_BAND), Kit.BG_SUNKEN)

	# Ticks: frames when they are far enough apart, then seconds, then labelled steps.
	var frame_width := axis.frame_width()
	var ticks := PackedVector2Array()
	if frame_width >= 5.0:
		var every := 1
		for candidate: int in [1, 2, 5, 10, 20, 50]:
			every = candidate
			if frame_width * candidate >= 34.0:
				break
		var frame := maxi(axis.frame_at(from), 0)
		var last_frame := axis.frame_at(to) + 1
		var per_second := float(axis.rate_num) / axis.rate_den
		while frame <= last_frame:
			var time := axis.frame_start(frame)
			var x := x_of(time)
			if x >= names_width:
				ticks.append(Vector2(x + 0.5, foot - 4.0))
				ticks.append(Vector2(x + 0.5, foot))
				var in_second := frame - axis.frame_at(TimelineScale.FLICKS * floori(float(time) / TimelineScale.FLICKS))
				if frame_width * every >= 34.0 and in_second % every == 0 and in_second != 0 and in_second + every * 0.5 < per_second:
					draw_string(font, Vector2(x + 4.0, 12.5), "%02d" % in_second, HORIZONTAL_ALIGNMENT_LEFT, -1.0, 10, Kit.TEXT_DIM.darkened(0.2))
			frame += 1
	var step := 3600
	for candidate: int in [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600]:
		step = candidate
		if candidate * axis.zoom >= 64.0:
			break
	var minor := 0
	for candidate: int in [1, 5, 10, 30, 60, 300, 600]:
		if candidate < step and step % candidate == 0 and candidate * axis.zoom >= 6.0:
			minor = candidate
			break
	var first_second := maxi(floori(float(from) / TimelineScale.FLICKS), 0)
	var last_second := ceili(float(to) / TimelineScale.FLICKS)
	if minor > 0:
		@warning_ignore("integer_division")
		var second := first_second / minor * minor
		while second <= last_second:
			var x := x_of(second * TimelineScale.FLICKS)
			if x >= names_width and second % step != 0:
				ticks.append(Vector2(x + 0.5, foot - 7.0))
				ticks.append(Vector2(x + 0.5, foot))
			second += minor
	if ticks.size() >= 2:
		draw_multiline(ticks, Color(1.0, 1.0, 1.0, 0.28), 1.0)
	@warning_ignore("integer_division")
	var second := first_second / step * step
	while second <= last_second:
		var x := x_of(second * TimelineScale.FLICKS)
		if x >= names_width - 40.0:
			draw_line(Vector2(x + 0.5, 3.0), Vector2(x + 0.5, foot), Color(1.0, 1.0, 1.0, 0.5), 1.0)
			draw_string(font, Vector2(x + 6.0, 13.0), clock_text(second), HORIZONTAL_ALIGNMENT_LEFT, -1.0, 11, Kit.TEXT)
		second += step

	# The working range, with a grip at each end.
	var range_start := int(panel.info.get("range_start", 0))
	var range_end := int(panel.info.get("range_end", 0))
	var start_x := x_of(range_start)
	var end_x := x_of(range_end)
	var left := clampf(start_x, names_width, area.end.x)
	var right := clampf(end_x, names_width, area.end.x)
	if right > left:
		draw_rect(Rect2(left, foot + 2.0, right - left, RANGE_BAND - 4.0), Color(Kit.ACCENT, 0.6))
	var grip := Kit.ACCENT.lightened(0.45)
	if start_x >= names_width - 1.0 and start_x <= area.end.x:
		draw_rect(Rect2(start_x, foot, 3.0, RANGE_BAND), grip)
		draw_rect(Rect2(start_x, foot, 7.0, 2.5), grip)
		draw_rect(Rect2(start_x, RULER - 2.5, 7.0, 2.5), grip)
	if end_x >= names_width and end_x <= area.end.x + 1.0:
		draw_rect(Rect2(end_x - 3.0, foot, 3.0, RANGE_BAND), grip)
		draw_rect(Rect2(end_x - 7.0, foot, 7.0, 2.5), grip)
		draw_rect(Rect2(end_x - 7.0, RULER - 2.5, 7.0, 2.5), grip)


## Whole seconds as M:SS, or H:MM:SS from an hour on.
static func clock_text(seconds: int) -> String:
	@warning_ignore("integer_division")
	var minutes := seconds / 60
	if minutes >= 60:
		@warning_ignore("integer_division")
		return "%d:%02d:%02d" % [minutes / 60, minutes % 60, seconds % 60]
	return "%d:%02d" % [minutes, seconds % 60]


## The playhead is drawn on a layer of its own, so that moving it redraws nothing else.
func _draw_overlay() -> void:
	var area := tracks_rect()
	var x := x_of(playhead)
	if x < names_width - 0.5 or x > area.end.x:
		return
	x = roundf(x) + 0.5
	_overlay.draw_line(Vector2(x, 0.0), Vector2(x, area.end.y), PLAYHEAD, 1.5)
	var handle := PackedVector2Array([Vector2(x - 5.5, 0.0), Vector2(x + 5.5, 0.0), Vector2(x + 5.5, 8.0), Vector2(x, 14.0), Vector2(x - 5.5, 8.0)])
	_overlay.draw_colored_polygon(handle, PLAYHEAD)


## Index of the first entry of `keys` (sorted by time) at or after `time`.
static func first_at_or_after(keys: Array, time: int) -> int:
	var low := 0
	var high := keys.size()
	while low < high:
		var middle := (low + high) >> 1
		var key: Dictionary = keys[middle]
		if int(key.time) < time:
			low = middle + 1
		else:
			high = middle
	return low


static func keys_of(line: Dictionary, track: String) -> Array:
	match track:
		"opacity":
			return line.opacity_keys
		"action":
			return line.actions
	return line.transform_keys


# ---- what is under the pointer ----

## The key drawn at `at` in row `index`, as `{track, time}`, or nothing. Keys of the
## type chosen in the panel are found first.
func key_at(at: Vector2, index: int) -> Dictionary:
	if index < 0 or index >= panel.rows.size():
		return {}
	var row: Dictionary = panel.rows[index]
	var line: Dictionary = panel.timeline.get(int(row.id), {})
	if line.is_empty():
		return {}
	var origin := int(line.origin)
	var time := time_at(at.x) - origin
	var reach := axis.duration_of(KEY_REACH)
	var order := [panel.key_track]
	for track: String in TRACKS:
		if track != panel.key_track:
			order.append(track)
	for track: String in order:
		var keys := keys_of(line, track)
		var best := -1
		var best_distance := reach + 1
		for number in range(first_at_or_after(keys, time - reach), keys.size()):
			var key: Dictionary = keys[number]
			var distance := absi(int(key.time) - time)
			if int(key.time) > time + reach:
				break
			if distance < best_distance:
				best_distance = distance
				best = int(key.time)
		if best >= 0:
			return {"track": track, "time": best}
	return {}


## The clip drawn at `x` in row `index`, as `{clip, part}` where `part` says what a
## drag there does: "move", or trim its "start" or "end". Nothing when there is none.
func clip_at(x: float, index: int) -> Dictionary:
	if index < 0 or index >= panel.rows.size():
		return {}
	var row: Dictionary = panel.rows[index]
	var line: Dictionary = panel.timeline.get(int(row.id), {})
	if line.is_empty() or row.parent == null:
		return {}
	var origin := int(line.origin)
	var clips: Array = line.clips
	for number in clips.size():
		var clip: Dictionary = clips[number]
		var open: bool = clip.end == null
		var left := x_of(origin + int(clip.start))
		var right := INF if open else x_of(origin + int(clip.end))
		if x < left - 3.0 or x > right + 3.0:
			continue
		var reach := EDGE_REACH if open else minf(EDGE_REACH, (right - left) / 3.0)
		var part := "move"
		if x <= left + reach:
			part = "start"
		elif not open and x >= right - reach:
			part = "end"
		return {"clip": number, "part": part}
	return {}


# ---- snapping ----

## A time on a layer's own clock moved to what it should stick to: the playhead or
## the edge of a clip when one is within a few pixels and that option is on,
## otherwise the nearest frame when that option is on.
func snap_time(time: int, origin: int, skip_layer := -1, skip_clip := -1) -> int:
	var edge := snap_edge(time, origin, skip_layer, skip_clip)
	if edge >= 0:
		return edge
	if panel.options.get("snap_to_frames", false):
		return axis.snap(time)
	return time


## The playhead or clip edge within a few pixels of `time`, on the same clock, or -1.
func snap_edge(time: int, origin: int, skip_layer := -1, skip_clip := -1) -> int:
	if not panel.options.get("snap_to_clips", false):
		return -1
	var target := origin + time
	var reach := axis.duration_of(SNAP_PIXELS)
	var best := -1
	var best_distance := reach + 1
	var candidates: Array[int] = [playhead]
	for row: Dictionary in panel.rows:
		var id := int(row.id)
		var line: Dictionary = panel.timeline.get(id, {})
		if line.is_empty() or row.parent == null:
			continue
		var clips: Array = line.clips
		for number in clips.size():
			if id == skip_layer and number == skip_clip:
				continue
			var clip: Dictionary = clips[number]
			candidates.append(int(line.origin) + int(clip.start))
			if clip.end != null:
				candidates.append(int(line.origin) + int(clip.end))
	for candidate in candidates:
		var distance := absi(candidate - target)
		if distance < best_distance and candidate - origin >= 0:
			best_distance = distance
			best = candidate - origin
	return best


# ---- presses and drags ----

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		var button: InputEventMouseButton = event
		if button.button_index == MOUSE_BUTTON_LEFT:
			if button.pressed:
				press(button.position, button.shift_pressed, button.ctrl_pressed or button.meta_pressed, button.double_click)
			else:
				release(button.position)
			accept_event()
		elif button.pressed and button.button_index in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN, MOUSE_BUTTON_WHEEL_LEFT, MOUSE_BUTTON_WHEEL_RIGHT]:
			_on_wheel(button)
			_follow_pointer()
			accept_event()
	elif event is InputEventMouseMotion:
		var motion: InputEventMouseMotion = event
		if _drag.is_empty():
			_show_cursor(motion.position)
		else:
			drag(motion.position)
			accept_event()
	elif event is InputEventGesture:
		on_gesture(event)
		accept_event()


## Escape during a drag puts back what the drag did and ends it. The key is taken
## here, before the shell's own Escape (which cancels a stroke or deselects) sees it.
func _input(event: InputEvent) -> void:
	if _drag.is_empty() or not event is InputEventKey:
		return
	var key: InputEventKey = event
	if key.pressed and not key.echo and key.keycode == KEY_ESCAPE:
		cancel_drag()
		get_viewport().set_input_as_handled()


func _process(delta: float) -> void:
	if not _drag.is_empty():
		auto_scroll(delta)


## A trackpad gesture over the view, at its position in the view.
##
## Pinching (`InputEventMagnifyGesture`, whose factor is above 1 as the fingers
## spread) zooms about the fingers. Two fingers (`InputEventPanGesture`, whose delta
## is positive toward later times and lower rows, the way a wheel turned down
## scrolls) move along time and through the rows together, as a scroll view does;
## with Shift they only scroll the rows, and with Ctrl or Cmd up and down zooms.
func on_gesture(event: InputEventGesture) -> void:
	if event is InputEventMagnifyGesture:
		var pinch: InputEventMagnifyGesture = event
		if pinch.factor > 0.0 and is_finite(pinch.factor):
			var x := pinch.position.x if pinch.position.x >= names_width else tracks_rect().get_center().x
			zoom_at(x, clampf(pinch.factor, 0.5, 2.0))
	elif event is InputEventPanGesture:
		var pan: InputEventPanGesture = event
		var delta := pan.delta * PAN_PIXELS * DisplayServer.screen_get_scale()
		if pan.shift_pressed:
			scroll_rows(delta.y + delta.x)
		elif (pan.ctrl_pressed or pan.meta_pressed) and pan.position.x >= names_width:
			zoom_at(pan.position.x, pow(1.08, -pan.delta.y))
		else:
			if delta.x != 0.0:
				pan_pixels(delta.x)
			if delta.y != 0.0:
				scroll_rows(delta.y)
	_follow_pointer()


## A gesture over a scroll bar is meant for the view.
func _on_bar_input(event: InputEvent, bar: Control) -> void:
	if event is InputEventGesture:
		var gesture := event.duplicate() as InputEventGesture
		gesture.position += bar.position
		on_gesture(gesture)
		bar.accept_event()


## The wheel pans along time; with Ctrl or Cmd it zooms about the pointer; with
## Shift, or over the names, it scrolls the rows.
func _on_wheel(button: InputEventMouseButton) -> void:
	var amount := button.factor if button.factor > 0.0 else 1.0
	var back := button.button_index in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_LEFT]
	var over_names := button.position.x < names_width
	if (button.ctrl_pressed or button.meta_pressed) and not over_names:
		zoom_at(button.position.x, pow(1.18, amount if back else -amount))
	elif button.shift_pressed or over_names:
		scroll_rows((-1.0 if back else 1.0) * ROW * amount)
	else:
		pan_pixels((-1.0 if back else 1.0) * 48.0 * amount)


func _show_cursor(at: Vector2) -> void:
	var shape := Control.CURSOR_ARROW
	if at.x >= names_width and at.y < RULER:
		if at.y >= RULER - RANGE_BAND and not _range_end_at(at.x).is_empty():
			shape = Control.CURSOR_HSIZE
	elif at.x >= names_width and at.y < tracks_rect().end.y:
		var index := row_index_at(at.y)
		if not key_at(at, index).is_empty():
			shape = Control.CURSOR_POINTING_HAND
		else:
			var hit := clip_at(at.x, index)
			if not hit.is_empty():
				shape = Control.CURSOR_DRAG if hit.part == "move" else Control.CURSOR_HSIZE
	mouse_default_cursor_shape = shape


## A press at `at`. `add` (Shift) and `toggle` (Ctrl or Cmd) add to or take from
## what is selected; `double` is the second press of a double press.
func press(at: Vector2, add := false, toggle := false, double := false) -> void:
	_finish_drag(false)
	# Asked of the scene: the editor's copy of the status may be a frame old.
	var before := int(editor.scene.status().time)
	if at.x > size.x - BAR or at.y > size.y - BAR:
		return
	if at.y < RULER:
		if at.x >= names_width:
			_press_ruler(at)
			_begin_drag(at, before)
		return
	var index := row_index_at(at.y)
	if at.x < names_width:
		_press_name(index, at, add, toggle)
		return
	if double:
		seek_to(at.x)
		return
	if index >= 0 and index < panel.rows.size():
		var row: Dictionary = panel.rows[index]
		var key := key_at(at, index)
		if not key.is_empty():
			_press_key(at, row, key, add or toggle)
			_begin_drag(at, before)
			return
		# Clips cover most of a track, so there is a way to drag out a range of keys
		# over them: with Shift held, or with the panel's switch for it on.
		var hit := {} if add or panel.select_range else clip_at(at.x, index)
		if not hit.is_empty():
			_press_clip(at, row, hit)
			_begin_drag(at, before)
			return
	var keep: Dictionary = panel.selected_keys.duplicate(true) if add or toggle else {}
	_drag = {"kind": "band", "from": at, "moved": false, "keep": keep, "before": panel.selected_keys.duplicate(true), "track": panel.key_track}
	_begin_drag(at, before)


## What every drag remembers of its press: the pointer, the time and the row under
## it (so that the view can scroll under a drag), and where the playhead was.
func _begin_drag(at: Vector2, playhead_before: int) -> void:
	if _drag.is_empty():
		return
	_drag["at"] = at
	_drag["from_time"] = time_at(at.x)
	_drag["from_y"] = at.y + scroll_y
	_drag["playhead_before"] = playhead_before


func drag(at: Vector2) -> void:
	if _drag.is_empty():
		return
	_drag["at"] = at
	var from: Vector2 = _drag.from
	if not _drag.moved:
		if from.distance_to(at) < DRAG_THRESHOLD:
			return
		_drag.moved = true
	_drag_to(at)


## Carries the drag under way to the pointer at `at`, against the view as it is now.
func _drag_to(at: Vector2) -> void:
	match str(_drag.kind):
		"scrub":
			seek_to(at.x)
		"range_start", "range_end":
			_drag_range(at)
		"clip":
			_drag_clip(at)
		"keys":
			_drag_keys(at)
		"band":
			_drag_band(at)


func release(_at: Vector2) -> void:
	_finish_drag(true)


## Ends what the pointer was doing. A drag that changed the document becomes one undo step.
func _finish_drag(by_release: bool) -> void:
	if _drag.is_empty():
		return
	var done := _drag
	_drag = {}
	var moved: bool = done.moved
	match str(done.kind):
		"clip", "range_start", "range_end":
			if moved:
				editor.commit()
		"keys":
			if moved:
				editor.commit()
			elif by_release and not (done.alone as Dictionary).is_empty():
				panel.select_keys(str(done.track), done.alone)
		"band":
			_band = Rect2()
			if not moved and by_release:
				panel.select_keys(panel.key_track, done.keep)
				panel.select_clip(-1, -1)
	queue_redraw()


## Puts back everything the drag under way has changed (the document, the keys a
## band selected, the playhead a scrub moved) and ends it, as Escape does. What the
## press itself selected stays selected. False when nothing was being dragged.
func cancel_drag() -> bool:
	if _drag.is_empty():
		return false
	var kind := str(_drag.kind)
	match kind:
		"clip", "keys", "range_start", "range_end":
			editor.cancel()
			if kind == "keys":
				panel.selected_keys = (_drag.original as Dictionary).duplicate(true)
			# Read while the drag is still on, so that the clip being dragged stays selected.
			panel.reload(kind.begins_with("range"))
		"band":
			panel.select_keys(str(_drag.track), _drag.before)
		"scrub":
			editor.run({"cmd": "seek", "time": int(_drag.playhead_before)})
	_drag = {}
	_band = Rect2()
	queue_redraw()
	return true


## How fast the view should scroll, in pixels a second, for a pointer at `x` along
## an extent from `low` to `high`: below zero toward `low`, above toward `high`,
## zero away from both. The same for up and down.
static func edge_speed(x: float, low: float, high: float) -> float:
	var zone := minf(SCROLL_ZONE, (high - low) * 0.25)
	if zone <= 0.0:
		return 0.0
	var depth := 0.0
	if x < low + zone:
		depth = x - (low + zone)
	elif x > high - zone:
		depth = x - (high - zone)
	return clampf(depth / zone * SCROLL_SPEED, -SCROLL_SPEED_MAX, SCROLL_SPEED_MAX)


## Scrolls the view while a drag holds the pointer near an edge of the tracks, or
## past it, and carries the drag along so that what is dragged stays at the time
## under the pointer. Called every frame; `delta` is in seconds. A drag that selects
## keys scrolls the rows too. Returns whether the view moved.
func auto_scroll(delta: float) -> bool:
	if _drag.is_empty() or not _drag.moved or not _drag.has("at"):
		return false
	var at: Vector2 = _drag.at
	var area := tracks_rect()
	var start := axis.start
	var top := scroll_y
	var speed := edge_speed(at.x, area.position.x, area.end.x)
	if speed != 0.0:
		axis.start = maxf(axis.start + speed * delta / axis.zoom, 0.0)
	if _drag.kind == "band":
		scroll_y += edge_speed(at.y, area.position.y, area.end.y) * delta
	if axis.start == start and scroll_y == top:
		return false
	refresh()
	if axis.start == start and scroll_y == top:
		return false
	_drag_to(at)
	return true


## After the view zoomed or scrolled under a drag, what is dragged catches up with
## the pointer.
func _follow_pointer() -> void:
	if not _drag.is_empty() and _drag.moved and _drag.has("at"):
		_drag_to(_drag.at)


func _press_ruler(at: Vector2) -> void:
	if at.y >= RULER - RANGE_BAND:
		var end := _range_end_at(at.x)
		if not end.is_empty():
			_drag = {"kind": end.kind, "from": at, "moved": false, "start": int(panel.info.range_start), "end": int(panel.info.range_end)}
			return
	_drag = {"kind": "scrub", "from": at, "moved": true}
	seek_to(at.x)


## Which end of the working range has its grip at `x`: `{kind}` or nothing.
func _range_end_at(x: float) -> Dictionary:
	var to_start := absf(x - (x_of(int(panel.info.get("range_start", 0))) + 3.0))
	var to_end := absf(x - (x_of(int(panel.info.get("range_end", 0))) - 3.0))
	if minf(to_start, to_end) > EDGE_REACH + 2.0:
		return {}
	return {"kind": "range_end" if to_end <= to_start else "range_start"}


func _press_name(index: int, at: Vector2, add: bool, toggle: bool) -> void:
	if index < 0 or index >= panel.rows.size():
		return
	var row: Dictionary = panel.rows[index]
	var id := int(row.id)
	var fold := 5.0 + int(row.depth) * INDENT
	if int(row.children) > 0 and at.x >= fold - 4.0 and at.x <= fold + 14.0:
		editor.run({"cmd": "set_layer", "layer": id, "collapsed": not row.collapsed})
		return
	editor.run({"cmd": "select_layers", "layers": [id], "mode": "toggle" if toggle else ("add" if add else "replace")})


func _press_key(at: Vector2, row: Dictionary, key: Dictionary, extend: bool) -> void:
	var id := int(row.id)
	var track: String = key.track
	var time: int = key.time
	var chosen: Dictionary = panel.selected_keys.duplicate(true) if track == panel.key_track else {}
	var times: Array = chosen.get(id, [])
	var was_chosen := times.has(time)
	if extend:
		if was_chosen:
			times.erase(time)
		else:
			times.append(time)
		chosen[id] = times
		panel.select_keys(track, chosen)
		return
	if not was_chosen:
		chosen = {id: [time]}
	panel.select_keys(track, chosen)
	panel.make_current(id)
	var line: Dictionary = panel.timeline.get(id, {})
	# Pressing one of several selected keys drags them all; letting go without
	# dragging leaves only that one selected.
	var alone := {id: [time]} if was_chosen else {}
	_drag = {"kind": "keys", "from": at, "moved": false, "track": track, "time": time, "origin": int(line.origin), "original": panel.selected_keys.duplicate(true), "alone": alone, "locked": _any_locked(panel.selected_keys)}


func _any_locked(layers: Dictionary) -> bool:
	for id: int in layers:
		if editor.row(id).get("locked", false):
			return true
	return false


func _press_clip(at: Vector2, row: Dictionary, hit: Dictionary) -> void:
	var id := int(row.id)
	var number: int = hit.clip
	if not panel.selected_keys.is_empty():
		panel.select_keys(panel.key_track, {})
	panel.select_clip(id, number)
	panel.make_current(id)
	var line: Dictionary = panel.timeline.get(id, {})
	var clips: Array = line.clips
	var clip: Dictionary = clips[number]
	# A clip stays between its neighbours while it is dragged.
	var lower := 0
	if number > 0:
		var before: Dictionary = clips[number - 1]
		lower = int(before.start) + 1 if before.end == null else int(before.end)
	var upper: Variant = null
	if number + 1 < clips.size():
		var after: Dictionary = clips[number + 1]
		upper = int(after.start)
	_drag = {"kind": "clip", "part": hit.part, "from": at, "moved": false, "layer": id, "clip": number, "start": int(clip.start), "end": clip.end, "origin": int(line.origin), "lower": lower, "upper": upper, "locked": bool(row.locked)}


## Replaces what the drag under way has done so far by `commands`, all inside one
## undo step that `release` closes.
func _apply(label: String, commands: Array, settings := false) -> void:
	editor.cancel()
	if not commands.is_empty():
		editor.begin(label)
		for command: Dictionary in commands:
			editor.try_run(command)
	panel.reload(settings)


func _drag_clip(at: Vector2) -> void:
	if _drag.locked:
		return
	var id: int = _drag.layer
	var number: int = _drag.clip
	var origin: int = _drag.origin
	var start: int = _drag.start
	var open: bool = _drag.end == null
	var end: int = start if open else int(_drag.end)
	var lower: int = _drag.lower
	var has_upper: bool = _drag.upper != null
	var upper: int = int(_drag.upper) if has_upper else 0
	var shift := time_at(at.x) - int(_drag.from_time)
	var frame := maxi(int(panel.info.get("frame_duration", 1)), 1)
	var shortest := frame if open else mini(frame, end - start)
	var commands := []
	match str(_drag.part):
		"move":
			var target := snap_time(start + shift, origin, id, number)
			if not open:
				# Whichever end of the clip is nearer to something it can stick to wins.
				var by_start := snap_edge(start + shift, origin, id, number)
				var by_end := snap_edge(end + shift, origin, id, number)
				if by_end >= 0 and (by_start < 0 or absi(by_end - end - shift) < absi(by_start - start - shift)):
					target = by_end - (end - start)
				if has_upper:
					target = mini(target, upper - (end - start))
			target = maxi(target, lower)
			if target != start:
				commands.append({"cmd": "move_clip", "layer": id, "clip": number, "start": target})
		"start":
			var target := maxi(snap_time(start + shift, origin, id, number), lower)
			if not open:
				target = mini(target, end - shortest)
			if target != start:
				commands.append({"cmd": "trim_clip", "layer": id, "clip": number, "start": target})
		"end":
			var target := maxi(snap_time(end + shift, origin, id, number), start + shortest)
			if has_upper:
				target = mini(target, upper)
			if target != end:
				commands.append({"cmd": "trim_clip", "layer": id, "clip": number, "end": target})
	_apply("Move clip" if _drag.part == "move" else "Trim clip", commands)


func _drag_keys(at: Vector2) -> void:
	if _drag.locked:
		return
	var track: String = _drag.track
	var time: int = _drag.time
	var original: Dictionary = _drag.original
	var delta := maxi(snap_time(time + time_at(at.x) - int(_drag.from_time), int(_drag.origin)), 0) - time
	# Keys pushed against zero keep their spacing, on every layer alike.
	for id: int in original:
		for other: int in original[id]:
			delta = maxi(delta, -other)
	var commands := []
	var moved := {}
	for id: int in original:
		var times: Array = original[id]
		if delta != 0:
			commands.append({"cmd": "move_keys", "layer": id, "track": track, "times": times, "delta": delta})
		moved[id] = times.map(func(other: int) -> int: return other + delta)
	panel.selected_keys = moved
	_apply("Move keys", commands)


func _drag_range(at: Vector2) -> void:
	var start: int = _drag.start
	var end: int = _drag.end
	var frame := maxi(int(panel.info.get("frame_duration", 1)), 1)
	var time := snap_time(maxi(time_at(at.x), 0), 0)
	var commands := []
	if _drag.kind == "range_start":
		time = clampi(time, 0, end - mini(frame, end - start))
		if time != start:
			commands.append({"cmd": "set_document", "range_start": time})
	else:
		time = maxi(time, start + mini(frame, end - start))
		if time != end:
			commands.append({"cmd": "set_document", "range_end": time})
	_apply("Change working range", commands, true)


## Selects the keys of the chosen type that the dragged rectangle covers.
## Where it began is held in time and in the rows, so that it stays put while the
## view scrolls under it.
func _drag_band(at: Vector2) -> void:
	var anchor_time := int(_drag.from_time)
	var from := Vector2(x_of(anchor_time), float(_drag.from_y) - scroll_y)
	_band = Rect2(from, at - from).abs()
	var chosen: Dictionary = (_drag.keep as Dictionary).duplicate(true)
	var first := clampi(row_index_at(_band.position.y), 0, panel.rows.size())
	var last := clampi(row_index_at(_band.end.y) + 1, 0, panel.rows.size())
	var pointer_time := time_at(at.x)
	var from_time := mini(anchor_time, pointer_time)
	var to_time := maxi(anchor_time, pointer_time)
	for index in range(first, last):
		var row: Dictionary = panel.rows[index]
		var id := int(row.id)
		var line: Dictionary = panel.timeline.get(id, {})
		if line.is_empty():
			continue
		var origin := int(line.origin)
		var keys := keys_of(line, panel.key_track)
		var times := {}
		for time: int in chosen.get(id, []):
			times[time] = true
		for number in range(first_at_or_after(keys, from_time - origin), keys.size()):
			var key: Dictionary = keys[number]
			if int(key.time) > to_time - origin:
				break
			times[int(key.time)] = true
		if not times.is_empty():
			chosen[id] = times.keys()
	panel.select_keys(panel.key_track, chosen)
	queue_redraw()


## Moves the playhead to the time drawn at `x`, on a frame when that option is on.
func seek_to(x: float) -> void:
	var time := maxi(time_at(x), 0)
	if panel.options.get("snap_to_frames", false):
		time = axis.snap(time)
	# While scrubbing, most moves of the pointer stay on the frame it is on already.
	if is_dragging():
		if int(_drag.get("sought", -1)) == time:
			return
		_drag["sought"] = time
	editor.run({"cmd": "seek", "time": time})
