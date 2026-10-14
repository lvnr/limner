class_name XRPanelSurface
extends Node3D
## One panel of the interface as a sheet in the room.
##
## The panel is an ordinary `Control`, the same one the desktop docks. Here it is
## drawn into a viewport of its own and shown on a flat sheet, with a title bar above
## it and a line for tooltips below. A pointer (the tip of the drawing hand, or a ray
## from it) is turned into the mouse events the panel would get on a screen, so the
## panel needs to know nothing about headsets.
##
## The sheet's centre is this node's origin and it faces along +Z. To save the
## graphics card work it is only redrawn while something is happening to it, plus now
## and then to catch changes nobody announced.

## The button in the corner of the title bar was pressed.
signal corner_pressed
## The sheet changed size, because its panel needs more or less room.
signal resized

## How many of the panel's pixels make one metre of sheet: 1.4 to the millimetre,
## which suits a first-generation headset at arm's length. Tune on real hardware.
const PIXELS_PER_METRE := 1400.0
## How many pixels are drawn for each of the panel's own, per side. Raise it to 2 for
## headsets sharp enough to show the difference; it costs four times the drawing.
const TEXELS_PER_PIXEL := 1
## Height of the title bar, in the panel's pixels.
const HEADER_HEIGHT := 32
## Height of the tooltip line, in the panel's pixels: room for two lines of small text.
const FOOTER_HEIGHT := 40
## The least and, unless a sheet says otherwise, the most room the panel itself is
## given, in pixels. A panel that needs more than the most scrolls.
const MIN_BODY_HEIGHT := 64
const MAX_BODY_HEIGHT := 560
## Space between the sheet's edge and what is on it, in pixels.
const MARGIN := 8
## Space between a panel and the scroll bar at its right, in pixels.
const SCROLL_GUTTER := 6
## How long the sheet keeps redrawing after it was last touched or told of a change, in seconds.
const AWAKE_SECONDS := 0.4
## How often an untouched sheet is redrawn anyway, in seconds.
const IDLE_REFRESH_SECONDS := 0.5
## How many frames in a row a redraw lasts, so that the smaller copies of the
## picture, each drawn from the one before, all catch up.
const SETTLE_FRAMES := 3
## How long the pointer usually rests on a control before its tooltip is shown, in seconds.
const TOOLTIP_DELAY := 0.5
## How small and how large a sheet can be made, as a factor of its normal size.
const MIN_MAGNIFICATION := 0.5
const MAX_MAGNIFICATION := 3.0

## A pixel that is on no sheet, for events that no control should think are over it.
const OFF_SHEET := Vector2(-16.0, -16.0)

const SHADER := preload("res://xr/panel.gdshader")

## What the title bar says, and what the palette knows the panel by.
var title := ""
## The panel itself.
var content: Control
## The viewport the panel is drawn in. Pointer events are pushed into it.
var viewport: SubViewport
## Size of the whole sheet in the panel's pixels: title bar, panel and tooltip line.
var pixel_size := Vector2i(352, 200)
## Whether the sheet can be picked up with the grip and left somewhere else.
var movable := true
## How long the pointer rests on a control before its tooltip is shown, in seconds.
var tooltip_delay := TOOLTIP_DELAY
## The most room the panel itself is given before it scrolls, in pixels. Set it
## before the sheet enters the tree.
var max_body_height := MAX_BODY_HEIGHT
## How much larger than normal the sheet is shown.
var magnification := 1.0:
	set(value):
		magnification = clampf(value, MIN_MAGNIFICATION, MAX_MAGNIFICATION)
		scale = Vector3.ONE * magnification

var _width := 352
var _body_height := 0
var _root: PanelContainer
var _scroll: ScrollContainer
var _corner: Button
var _footer: Label
var _quad: QuadMesh
var _material: ShaderMaterial
## The viewports that hold the half-size and quarter-size copies of the picture.
var _copies: Array[SubViewport] = []
var _awake := 0.0
var _idle := 0.0
var _frames_to_draw := 0
var _fit_queued := false
var _inside := false
var _pressed := false
var _press_opened_pop_up := false
var _press_closed_pop_ups := false
## The far corner of the pop-ups that are open, in pixels: the sheet is kept at least
## this large while they are, so that none of a pop-up hangs off its edge.
var _pop_up_reach := Vector2i.ZERO
var _last_pixel := Vector2.ZERO
var _hovered_tooltip := ""
var _hover_time := 0.0


## `width` is the sheet's width in the panel's pixels. `body_height` fixes the room
## the panel gets; left at 0 the sheet takes the height the panel asks for. A sheet
## without `chrome` has no title bar and no tooltip line.
func _init(surface_title: String, panel: Control, width: int, body_height := 0, chrome := true) -> void:
	title = surface_title
	name = surface_title.validate_node_name() if surface_title != "" else "Surface"
	content = panel
	_width = width
	_body_height = body_height

	viewport = _new_viewport("Panel")
	viewport.gui_embed_subwindows = true
	add_child(viewport)
	_root = PanelContainer.new()
	# A pop-up panel (a colour picker, say) opens over the panel it belongs to, not
	# as a window of its own; Kit's theme gives pop-ups a solid back for that.
	_root.theme = Kit.make_theme()
	_root.add_theme_stylebox_override("panel", Kit._box(Color(Kit.BG, 1.0), 0, 1, Kit.BORDER, MARGIN))
	Kit.events.scheme_changed.connect(_on_scheme_changed)
	viewport.add_child(_root)
	var column := Kit.column()
	_root.add_child(column)

	if chrome:
		var heading := Kit.heading(surface_title)
		_corner = Kit.icon_button("dock", "", func() -> void: corner_pressed.emit())
		_corner.visible = false
		var bar := Kit.row([heading, Kit.spacer(), _corner])
		bar.custom_minimum_size = Vector2(0, HEADER_HEIGHT)
		column.add_child(bar)

	_scroll = ScrollContainer.new()
	_scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	# Room for the scroll bar is kept whether or not it shows. Were it to take its
	# width only when needed, a panel whose text wraps could flip for ever between
	# "narrower, so taller, so it scrolls" and "wider, so shorter, so it does not".
	_scroll.vertical_scroll_mode = ScrollContainer.SCROLL_MODE_RESERVE if chrome else ScrollContainer.SCROLL_MODE_DISABLED
	_scroll.size_flags_vertical = Control.SIZE_EXPAND_FILL
	panel.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	panel.size_flags_vertical = Control.SIZE_EXPAND_FILL
	var gutter := MarginContainer.new()
	gutter.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	gutter.size_flags_vertical = Control.SIZE_EXPAND_FILL
	gutter.add_theme_constant_override("margin_right", SCROLL_GUTTER if chrome else 0)
	for side: String in ["margin_left", "margin_top", "margin_bottom"]:
		gutter.add_theme_constant_override(side, 0)
	gutter.add_child(panel)
	_scroll.add_child(gutter)
	column.add_child(_scroll)

	if chrome:
		_footer = Form.note()
		_footer.custom_minimum_size = Vector2(0, FOOTER_HEIGHT)
		_footer.clip_text = true
		_footer.max_lines_visible = 2
		_footer.vertical_alignment = VERTICAL_ALIGNMENT_TOP
		_footer.add_theme_stylebox_override("normal", Kit._box(Kit.BG_SUNKEN, 4, 0, Kit.BORDER, 5))
		column.add_child(_footer)

	for index in 2:
		var copy := _new_viewport("Copy%d" % (index + 1))
		var picture := TextureRect.new()
		picture.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		picture.stretch_mode = TextureRect.STRETCH_SCALE
		picture.set_anchors_preset(Control.PRESET_FULL_RECT)
		copy.add_child(picture)
		add_child(copy)
		_copies.append(copy)

	_quad = QuadMesh.new()
	_material = ShaderMaterial.new()
	_material.shader = SHADER
	var sheet := MeshInstance3D.new()
	sheet.name = "Sheet"
	sheet.mesh = _quad
	sheet.material_override = _material
	sheet.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(sheet)
	# The sheet is sized again whenever the panel needs more or less room.
	content.minimum_size_changed.connect(_queue_fit)
	visibility_changed.connect(_on_visibility_changed)


func _ready() -> void:
	var source := viewport
	for index in _copies.size():
		var picture: TextureRect = _copies[index].get_child(0)
		picture.texture = source.get_texture()
		_material.set_shader_parameter("level%d" % (index + 1), _copies[index].get_texture())
		source = _copies[index]
	_material.set_shader_parameter("level0", viewport.get_texture())
	_material.set_shader_parameter("back_color", Kit.BG_SUNKEN)
	_root.visible = is_visible_in_tree()
	_fit()
	# Text that wraps only knows its height once it has been given its width.
	_queue_fit()


## The panel inside is shown exactly when the sheet is. Its viewport is a world of
## its own, where a hidden sheet would leave the panel "visible", so panels that work
## only while they are seen (a live viewfinder, say) would go on working unseen.
func _on_visibility_changed() -> void:
	var seen := is_visible_in_tree()
	if _root.visible != seen:
		_root.visible = seen
		if seen:
			_queue_fit()
	wake()


func _process(delta: float) -> void:
	if not is_visible_in_tree():
		return
	_awake -= delta
	_idle += delta
	var open := viewport.get_embedded_subwindows()
	var reach := Vector2i.ZERO
	for window in open:
		reach = reach.max(window.position + window.size)
	if reach != _pop_up_reach:
		_pop_up_reach = reach
		_queue_fit()
	if _inside or _awake > 0.0 or _idle >= IDLE_REFRESH_SECONDS or not open.is_empty():
		_idle = 0.0
		_frames_to_draw = SETTLE_FRAMES
	if _frames_to_draw > 0:
		_frames_to_draw -= 1
		viewport.render_target_update_mode = SubViewport.UPDATE_ONCE
		for copy in _copies:
			copy.render_target_update_mode = SubViewport.UPDATE_ONCE


## The colour scheme changed: the sheet takes a theme in the new colours and draws again.
func _on_scheme_changed() -> void:
	Kit.apply_theme(_root)
	_material.set_shader_parameter("back_color", Kit.BG_SUNKEN)
	wake()


## Keeps the sheet redrawing for a short while. Call it when what the panel shows may
## have changed.
func wake() -> void:
	_awake = AWAKE_SECONDS


## Shows a button in the corner of the title bar, which reports `corner_pressed`.
func show_corner_button(icon: String, tooltip: String) -> void:
	_corner.icon = Icons.texture(icon, Kit.TEXT, 20)
	_corner.tooltip_text = tooltip
	_corner.visible = true
	wake()


func hide_corner_button() -> void:
	_corner.visible = false
	wake()


## Width and height of the sheet in the room, in metres, at its normal size.
func metres() -> Vector2:
	return Vector2(pixel_size) / PIXELS_PER_METRE


## Puts the sheet somewhere in the room, keeping its size.
func place(pose: Transform3D) -> void:
	global_transform = pose.orthonormalized()
	scale = Vector3.ONE * magnification


# ---- where things are ----

## Where a point in the room is relative to the sheet: `x` and `y` are the pixel of
## the panel it is in front of, and `z` is how far in front it is in metres, negative
## when it is behind.
func locate(point: Vector3) -> Vector3:
	var local := global_transform.affine_inverse() * point
	return Vector3(
		local.x * PIXELS_PER_METRE + pixel_size.x * 0.5,
		pixel_size.y * 0.5 - local.y * PIXELS_PER_METRE,
		local.z * magnification
	)


## Where a ray from `origin` along `direction` meets the front of the sheet: `x` and
## `y` are the pixel and `z` the distance along the ray in metres. Returns
## `Vector3.INF` when the ray runs away from the sheet or meets it from behind.
func aim(origin: Vector3, direction: Vector3) -> Vector3:
	var facing := global_basis.z.normalized()
	var approach := direction.dot(facing)
	if approach > -0.01:
		return Vector3.INF
	var distance := (global_position - origin).dot(facing) / approach
	if distance < 0.0:
		return Vector3.INF
	var at := locate(origin + direction * distance)
	return Vector3(at.x, at.y, distance)


## Whether a pixel is on the sheet, or within `margin` pixels of its edge.
func contains(pixel: Vector2, margin := 0.0) -> bool:
	return Rect2(Vector2.ZERO, pixel_size).grow(margin).has_point(pixel)


## The point in the room that is `lift` metres in front of a pixel of the panel.
func world_point(pixel: Vector2, lift := 0.0) -> Vector3:
	var local := Vector3(
		(pixel.x - pixel_size.x * 0.5) / PIXELS_PER_METRE,
		(pixel_size.y * 0.5 - pixel.y) / PIXELS_PER_METRE,
		lift / magnification
	)
	return global_transform * local


## The pixel at the middle of a control that is somewhere in this panel.
func pixel_of(control: Control) -> Vector2:
	return control.get_global_transform() * (control.size * 0.5)


# ---- the pointer ----

## The pointer is over `pixel`. Call it every frame while it is, moved or not, with
## the time since the last call in seconds, which is what a tooltip waits by.
func pointer_move(pixel: Vector2, delta := 0.0) -> void:
	if not _inside:
		_inside = true
		_last_pixel = pixel
		viewport.notify_mouse_entered()
	var event := InputEventMouseMotion.new()
	event.position = pixel
	event.global_position = pixel
	event.relative = pixel - _last_pixel
	event.button_mask = MOUSE_BUTTON_MASK_LEFT if _pressed else 0
	viewport.push_input(event, true)
	_last_pixel = pixel
	_follow_tooltip(delta)


## The pointer pressed down on the panel, or let go of it.
func pointer_button(pixel: Vector2, pressed: bool) -> void:
	_pressed = pressed
	if pressed:
		_press_closed_pop_ups = _close_pop_ups_not_under(pixel)
		if _press_closed_pop_ups:
			wake()
			return
		var open_before := viewport.get_embedded_subwindows().size()
		_push_button(pixel, MOUSE_BUTTON_LEFT, true)
		_press_opened_pop_up = viewport.get_embedded_subwindows().size() > open_before
	elif _press_closed_pop_ups:
		# The press that put a pop-up away was not passed on, so nor is its release.
		_press_closed_pop_ups = false
	else:
		# A drop-down that opens under a press would take the release as a choice of
		# whatever entry lies under the pointer. The release is given off the sheet
		# instead: the button that was pressed still hears it, the drop-down does not.
		_push_button(OFF_SHEET if _press_opened_pop_up else pixel, MOUSE_BUTTON_LEFT, false)
		_press_opened_pop_up = false
	wake()


## A press of the other mouse button at `pixel`, which is what opens the menu of
## whatever is there (a row of the layer list, say).
func pointer_context(pixel: Vector2) -> void:
	_close_pop_ups_not_under(pixel)
	var open_before := viewport.get_embedded_subwindows().size()
	_push_button(pixel, MOUSE_BUTTON_RIGHT, true)
	var opened := viewport.get_embedded_subwindows().size() > open_before
	_push_button(OFF_SHEET if opened else pixel, MOUSE_BUTTON_RIGHT, false)
	wake()


## One notch of scrolling at `pixel`, up or down.
func pointer_scroll(pixel: Vector2, up: bool) -> void:
	var wheel := MOUSE_BUTTON_WHEEL_UP if up else MOUSE_BUTTON_WHEEL_DOWN
	_push_button(pixel, wheel, true)
	_push_button(pixel, wheel, false)
	wake()


## The pointer went away. Anything it was holding down is let go.
func pointer_leave() -> void:
	if _pressed:
		pointer_button(_last_pixel, false)
	if _inside:
		_inside = false
		viewport.notify_mouse_exited()
	_hovered_tooltip = ""
	_hover_time = 0.0
	if _footer != null:
		_footer.text = ""
	wake()


# ---- typing ----

## The text box in this panel that is being typed into, or `null`. A box inside a
## pop-up of the panel counts.
func typing_control() -> Control:
	var places: Array[Viewport] = [viewport]
	for window in viewport.get_embedded_subwindows():
		places.append(window)
	for place_to_look in places:
		var focused := place_to_look.gui_get_focus_owner()
		var line := focused as LineEdit
		if line != null and line.editable and line.is_editing():
			return line
		var area := focused as TextEdit
		if area != null and area.editable:
			return area
	return null


## Ends typing: the text box gives up the keyboard.
func stop_typing() -> void:
	var box := typing_control()
	if box != null:
		box.release_focus()
	wake()


# ---- inside ----

func _new_viewport(viewport_name: String) -> SubViewport:
	var made := SubViewport.new()
	made.name = viewport_name
	made.disable_3d = true
	made.transparent_bg = false
	made.render_target_update_mode = SubViewport.UPDATE_DISABLED
	made.render_target_clear_mode = SubViewport.CLEAR_MODE_ALWAYS
	return made


func _push_button(pixel: Vector2, button: MouseButton, pressed: bool) -> void:
	var event := InputEventMouseButton.new()
	event.button_index = button
	event.pressed = pressed
	event.position = pixel
	event.global_position = pixel
	event.button_mask = MOUSE_BUTTON_MASK_LEFT if _pressed else 0
	viewport.push_input(event, true)


## Puts away the pop-ups of this panel when `pixel` is on none of them, as a click
## beside a menu does on a screen. Returns whether there were any to put away.
## (Godot does this itself only for pop-ups whose host is a window; a panel's host
## here is a viewport of its own.)
func _close_pop_ups_not_under(pixel: Vector2) -> bool:
	var open := viewport.get_embedded_subwindows()
	for window in open:
		if Rect2(window.position, window.size).has_point(pixel):
			return false
	var closed := false
	for window in open:
		if window is Popup:
			window.hide()
			closed = true
	return closed


func _queue_fit() -> void:
	if _fit_queued:
		return
	_fit_queued = true
	_fit.call_deferred()


## Sizes the sheet for what the panel needs.
func _fit() -> void:
	_fit_queued = false
	var body := _body_height
	if body <= 0:
		body = clampi(ceili(content.get_combined_minimum_size().y), MIN_BODY_HEIGHT, max_body_height)
	_scroll.custom_minimum_size = Vector2(0, body)
	var needed := _root.get_combined_minimum_size()
	var fitted := Vector2i(maxi(_width, ceili(needed.x)), ceili(needed.y)).max(_pop_up_reach)
	if fitted == pixel_size and viewport.size == fitted * TEXELS_PER_PIXEL:
		return
	# The sheet grows and shrinks downward: its top edge stays where it is, so that
	# a button just pressed near the top does not slide out from under the tip.
	translate_object_local(Vector3(0.0, (pixel_size.y - fitted.y) * 0.5 / PIXELS_PER_METRE, 0.0))
	pixel_size = fitted
	viewport.size = fitted * TEXELS_PER_PIXEL
	viewport.size_2d_override = fitted
	viewport.size_2d_override_stretch = true
	_root.position = Vector2.ZERO
	_root.size = fitted
	var smaller := viewport.size
	for copy in _copies:
		smaller = Vector2i(maxi(1, smaller.x >> 1), maxi(1, smaller.y >> 1))
		copy.size = smaller
	_quad.size = metres()
	_material.set_shader_parameter("pixels", Vector2(viewport.size))
	wake()
	resized.emit()


## Shows the tooltip of the control under the pointer once the pointer has rested on
## it for a moment.
func _follow_tooltip(delta: float) -> void:
	if _footer == null:
		return
	var text := ""
	var node: Node = viewport.gui_get_hovered_control()
	while node is Control and text == "":
		var control: Control = node
		text = control.get_tooltip(control.get_global_transform().affine_inverse() * _last_pixel)
		node = control.get_parent()
	if text != _hovered_tooltip:
		_hovered_tooltip = text
		_hover_time = 0.0
	_hover_time += delta
	var shown := text if _hover_time >= tooltip_delay else ""
	if _footer.text != shown:
		_footer.text = shown
