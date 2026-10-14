class_name PickerArea
extends Control
## One of the two draggable parts of the colour picker: the square of saturation
## against value for one hue, or the strip of hues.
##
## It only shows and reports places. Pressing or dragging on it reports where, as
## fractions of its size, between `drag_started` and `drag_ended`; the owner decides
## what colour that is and moves the marker. A mouse, a finger and a controller tip
## all work the same way, and nothing depends on hovering.

signal drag_started
signal changed(at: Vector2)
signal drag_ended

enum Kind { SATURATION_VALUE, HUE }

const RING_RADIUS := 6.0

var kind: Kind
## The hue the square is drawn for, 0 to 1. The strip ignores it.
var hue := 0.0:
	set(value):
		if hue != value:
			hue = value
			queue_redraw()
## Where the marker is, as fractions of the size: saturation across and value
## upwards on the square, hue downwards on the strip.
var marker := Vector2.ZERO:
	set(value):
		if marker != value:
			marker = value
			queue_redraw()

var _dragging := false


func _init(area_kind: Kind) -> void:
	kind = area_kind
	focus_mode = Control.FOCUS_NONE


func _gui_input(event: InputEvent) -> void:
	# A touch usually arrives a second time as a mouse event, so every step here is
	# safe to take twice.
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			_press(event.position)
		else:
			_release()
	elif event is InputEventScreenTouch:
		if event.pressed:
			_press(event.position)
		else:
			_release()
	elif event is InputEventMouseMotion or event is InputEventScreenDrag:
		if _dragging:
			_report(event.position)
	else:
		return
	accept_event()


func _press(at: Vector2) -> void:
	if not _dragging:
		_dragging = true
		drag_started.emit()
	_report(at)


func _release() -> void:
	if _dragging:
		_dragging = false
		drag_ended.emit()


func _report(at: Vector2) -> void:
	var place := Vector2(at.x / maxf(size.x, 1.0), at.y / maxf(size.y, 1.0)).clamp(Vector2.ZERO, Vector2.ONE)
	changed.emit(Vector2(place.x, 1.0 - place.y) if kind == Kind.SATURATION_VALUE else place)


func _draw() -> void:
	var corners := PackedVector2Array([Vector2.ZERO, Vector2(size.x, 0.0), size, Vector2(0.0, size.y)])
	if kind == Kind.SATURATION_VALUE:
		# White to the full colour across, then darkened to black downwards.
		var full := Color.from_hsv(hue, 1.0, 1.0)
		draw_polygon(corners, PackedColorArray([Color.WHITE, full, full, Color.WHITE]))
		draw_polygon(corners, PackedColorArray([Color(0, 0, 0, 0), Color(0, 0, 0, 0), Color.BLACK, Color.BLACK]))
	else:
		# Between two neighbouring primary and secondary colours a hue is a plain blend.
		for band in 6:
			var top := size.y * band / 6.0
			var bottom := size.y * (band + 1) / 6.0
			var from := Color.from_hsv(band / 6.0, 1.0, 1.0)
			var to := Color.from_hsv((band + 1) / 6.0, 1.0, 1.0)
			draw_polygon(
				PackedVector2Array([Vector2(0.0, top), Vector2(size.x, top), Vector2(size.x, bottom), Vector2(0.0, bottom)]),
				PackedColorArray([from, from, to, to])
			)
	draw_rect(Rect2(Vector2.ZERO, size), Kit.BORDER, false, 1.0)

	# The marker is white edged with black, so it shows on any colour. At the edges it
	# stops just inside, where a panel that clips its contents cannot cut it off.
	if kind == Kind.SATURATION_VALUE:
		var reach := Vector2(RING_RADIUS + 2.0, RING_RADIUS + 2.0)
		var center := (Vector2(marker.x, 1.0 - marker.y) * size).clamp(reach, size - reach)
		draw_arc(center, RING_RADIUS, 0.0, TAU, 32, Color.BLACK, 4.0, true)
		draw_arc(center, RING_RADIUS, 0.0, TAU, 32, Color.WHITE, 2.0, true)
	else:
		var bar := Rect2(2.0, clampf(marker.y * size.y, 5.0, size.y - 5.0) - 3.0, size.x - 4.0, 6.0)
		draw_rect(bar, Color.BLACK, false, 4.0)
		draw_rect(bar, Color.WHITE, false, 2.0)
