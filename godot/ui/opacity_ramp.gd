class_name OpacityRamp
extends Control
## The opacity of the paint as a strip: the paint colour fading from clear at the left
## to solid at the right, over a checker, with a mark at the value. Pressed or dragged
## anywhere along it, it reports the value there, between `drag_started` and
## `drag_ended`, so the owner can make one undo step of a drag. Works the same with a
## mouse, a finger and a controller tip.

signal drag_started
signal changed(value: float)
signal drag_ended

## The colour shown fading out.
var color := Color.WHITE:
	set(value):
		if color != value:
			color = value
			queue_redraw()

var _value := 1.0
var _dragging := false


func _init() -> void:
	focus_mode = Control.FOCUS_NONE
	custom_minimum_size = Vector2(Kit.u(120), Kit.u(32))
	size_flags_horizontal = Control.SIZE_EXPAND_FILL


## Shows `value` (0 to 1) without reporting it. Ignored while it is being dragged.
func set_value(value: float) -> void:
	if _dragging:
		return
	_value = clampf(value, 0.0, 1.0)
	queue_redraw()


func get_value() -> float:
	return _value


func _gui_input(event: InputEvent) -> void:
	# A touch usually arrives a second time as a mouse event: every step is safe twice.
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
	_value = clampf(at.x / maxf(size.x, 1.0), 0.0, 1.0)
	queue_redraw()
	changed.emit(_value)


func _draw() -> void:
	# The checker shows through where the colour is clear.
	var cell := maxf(size.y / 4.0, 4.0)
	var light := Color(0.78, 0.79, 0.8)
	var dark := Color(0.5, 0.51, 0.54)
	var rows := ceili(size.y / cell)
	var columns := ceili(size.x / cell)
	for y in rows:
		for x in columns:
			var square := Rect2(x * cell, y * cell, cell, cell).intersection(Rect2(Vector2.ZERO, size))
			draw_rect(square, light if (x + y) % 2 == 0 else dark)
	var clear := Color(color, 0.0)
	var solid := Color(color, 1.0)
	draw_polygon(
		PackedVector2Array([Vector2.ZERO, Vector2(size.x, 0.0), size, Vector2(0.0, size.y)]),
		PackedColorArray([clear, solid, solid, clear])
	)
	draw_rect(Rect2(Vector2.ZERO, size), Kit.BORDER, false, 1.0)
	# The mark at the value: white edged with black, to show on any colour.
	var x := clampf(_value * size.x, 2.0, size.x - 2.0)
	draw_line(Vector2(x, 0.0), Vector2(x, size.y), Color.BLACK, 4.0)
	draw_line(Vector2(x, 0.0), Vector2(x, size.y), Color.WHITE, 2.0)
