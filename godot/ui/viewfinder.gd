class_name Viewfinder
extends Control
## The picture of the Monitor: what a camera layer sees, in the shape of the
## document's camera resolution, with bars above and below or left and right where
## that shape does not fill the panel, and framing lines over it on request.
##
## The picture is live. It is rendered by a small viewport that shares the world
## with the main view, and only while the viewfinder is on screen.

## The longest side of the picture rendered, in pixels.
const LONGEST := 640

## Lines dividing the picture into this many parts each way; 0 for none.
var divisions := 0:
	set(value):
		divisions = value
		queue_redraw()

var camera: Camera3D

var _viewport: SubViewport
var _shape := Vector2(16, 9)
var _showing := false


func _init() -> void:
	custom_minimum_size = Vector2(120, 168)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	clip_contents = true
	_viewport = SubViewport.new()
	_viewport.name = "Picture"
	_viewport.msaa_3d = Viewport.MSAA_4X
	_viewport.render_target_update_mode = SubViewport.UPDATE_DISABLED
	camera = Camera3D.new()
	camera.cull_mask = CaptureRig.ARTWORK_LAYERS
	_viewport.add_child(camera)
	add_child(_viewport)
	set_shape(Vector2i(1920, 1080))


func _ready() -> void:
	# The same world as the main view: the artwork is drawn once and seen twice.
	_viewport.world_3d = get_viewport().world_3d
	visibility_changed.connect(_update_rendering)
	_update_rendering()


## Gives the picture the shape of a `resolution` in pixels.
func set_shape(resolution: Vector2i) -> void:
	_shape = Vector2(resolution)
	var scale := float(LONGEST) / maxi(resolution.x, resolution.y)
	_viewport.size = Vector2i(maxi(roundi(resolution.x * scale), 2), maxi(roundi(resolution.y * scale), 2))
	queue_redraw()


## Shows `view` (see `LimnerCapture`), or nothing when it is empty.
func show_view(view: Dictionary) -> void:
	_showing = not view.is_empty()
	if _showing:
		camera.global_transform = view.transform
		camera.fov = view.fov
		camera.near = view.near
		camera.far = view.far
	_update_rendering()
	queue_redraw()


## True while the picture is being rendered: there is a view and the panel is on screen.
func is_live() -> bool:
	return _viewport.render_target_update_mode == SubViewport.UPDATE_ALWAYS


## Where the picture is inside the viewfinder: as large as fits, centred.
func picture_rect() -> Rect2:
	var fit := minf(size.x / _shape.x, size.y / _shape.y)
	var extent := _shape * fit
	return Rect2((size - extent) * 0.5, extent)


func _update_rendering() -> void:
	var live := _showing and is_visible_in_tree()
	_viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS if live else SubViewport.UPDATE_DISABLED


func _notification(what: int) -> void:
	if what == NOTIFICATION_RESIZED:
		queue_redraw()


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color.BLACK)
	if not _showing:
		return
	var picture := picture_rect()
	draw_texture_rect(_viewport.get_texture(), picture, false)
	for i in range(1, divisions):
		var along := float(i) / divisions
		var x := picture.position.x + picture.size.x * along
		var y := picture.position.y + picture.size.y * along
		for line: Array in [[Vector2(x, picture.position.y), Vector2(x, picture.end.y)], [Vector2(picture.position.x, y), Vector2(picture.end.x, y)]]:
			# Dark under light, so the lines show on any picture.
			draw_line(line[0], line[1], Color(0, 0, 0, 0.45), 3.0)
			draw_line(line[0], line[1], Color(1, 1, 1, 0.8), 1.0)
