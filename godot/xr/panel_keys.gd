class_name XRPanelKeys
extends Node3D
## The buttons of a sheet, standing off it as keys that go down when pushed.
##
## Every button of the panel (and of its title bar) is a block on the sheet whose top
## shows the button as the panel draws it. Pushed with the tip, a key goes down with
## it and bottoms out a little above the sheet; a button that is on stays half down;
## a disabled one is flush with the sheet and is not drawn. Check boxes and anything
## that is not a button stay flat, as Quill's do. A button can ask to stay flat with
## the meta `flat_key`, or to be round (a coin, like the palette's panel buttons)
## with the meta `key_shape` set to "round".
##
## The keys are drawn with one multimesh for the square ones and one for the round
## ones, in the sheet's own space: its centre at the origin, facing +Z, in metres.

## How far a key stands off its sheet, in metres, and a round one.
const HEIGHT := 0.005
const ROUND_HEIGHT := 0.009
## Where a key bottoms out, and where a button that is on rests, as fractions of its
## height.
const FLOOR := 0.2
const LATCHED := 0.5
## How much of a key must show inside a scrolling panel for it to stand up, as a
## fraction of its height; a key cut shorter than this by the edge stays flat.
const MIN_SHOWING := 0.5
## Segments around a round key.
const ROUND_SEGMENTS := 40

const SHADER := preload("res://xr/key.gdshader")

## The square keys and the round ones.
var square: MultiMeshInstance3D
var round: MultiMeshInstance3D
## How many keys stand up now, square and round.
var count := 0

var _material: ShaderMaterial
var _pixels_per_metre := 1000.0
## The buttons that may be keys, found again whenever the panel's controls change.
var _buttons: Array[BaseButton] = []
var _stale := true
## Each key standing up now: its rectangle in pixels, its resting height and the
## height it is drawn at, in metres.
var _rests: Array[Dictionary] = []


func _init(pixels_per_metre: float) -> void:
	name = "Keys"
	_pixels_per_metre = pixels_per_metre
	_material = ShaderMaterial.new()
	_material.shader = SHADER
	square = _layer("Square", _box_mesh())
	round = _layer("Round", _coin_mesh())


func _layer(layer_name: String, mesh: Mesh) -> MultiMeshInstance3D:
	var multimesh := MultiMesh.new()
	multimesh.transform_format = MultiMesh.TRANSFORM_3D
	multimesh.use_custom_data = true
	multimesh.mesh = mesh
	var node := MultiMeshInstance3D.new()
	node.name = layer_name
	node.multimesh = multimesh
	node.material_override = _material
	node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(node)
	return node


## The three pictures of the panel, sharp to soft, and the size of the sharp one.
func set_pictures(level0: Texture2D, level1: Texture2D, level2: Texture2D, pixels: Vector2) -> void:
	_material.set_shader_parameter("level0", level0)
	_material.set_shader_parameter("level1", level1)
	_material.set_shader_parameter("level2", level2)
	_material.set_shader_parameter("pixels", pixels)


## The panel's controls changed: look for its buttons again at the next update.
func mark_stale() -> void:
	_stale = true


## Stands the keys where the panel's buttons are now.
##
## `root` holds every control of the sheet. `sheet` is the sheet's size in pixels.
## `clip` is the part of the sheet a scrolling panel shows, for the buttons under
## `scroll`. `covers` are the rectangles of the pop-ups open over the sheet. `touch`
## is where the tip is: `pixel`, `height` above the sheet in metres (INF when it is
## not touching), and `pressed`, true while it holds a press at that pixel.
func update(root: Control, sheet: Vector2, scroll: Control, clip: Rect2, covers: Array[Rect2], touch: Dictionary) -> void:
	if _stale:
		_stale = false
		_buttons.clear()
		for node: Node in root.find_children("*", "BaseButton", true, false):
			_buttons.append(node as BaseButton)
	var pixel: Vector2 = touch.get("pixel", Vector2(-1.0, -1.0))
	var tip_height: float = touch.get("height", INF)
	var pressed: bool = touch.get("pressed", false)
	var squares: Array[Array] = []
	var rounds: Array[Array] = []
	_rests.clear()
	for button in _buttons:
		if not is_instance_valid(button) or not _stands(button):
			continue
		var rect := button.get_global_rect()
		var full_height := rect.size.y
		if scroll != null and scroll.is_ancestor_of(button):
			rect = rect.intersection(clip)
			if rect.size.y < full_height * MIN_SHOWING or rect.size.x <= 0.0:
				continue
		if _covered(rect, covers):
			continue
		var is_round: bool = button.get_meta("key_shape", "") == "round"
		var full := ROUND_HEIGHT if is_round else HEIGHT
		var rest := full * (LATCHED if button.toggle_mode and button.button_pressed else 1.0)
		var top := rest
		if rect.has_point(pixel):
			if tip_height < INF:
				# Under the tip the key goes down with it, and comes back up with it.
				top = clampf(tip_height, full * FLOOR, rest)
			elif pressed:
				# Pressed from afar, with the ray: down at once.
				top = full * FLOOR
		_rests.append({"rect": rect, "rest": rest, "top": top})
		var entry := [_transform(rect, sheet, top), Color(rect.position.x / sheet.x, rect.position.y / sheet.y, rect.end.x / sheet.x, rect.end.y / sheet.y)]
		if is_round:
			rounds.append(entry)
		else:
			squares.append(entry)
	_fill(square.multimesh, squares)
	_fill(round.multimesh, rounds)
	count = squares.size() + rounds.size()


## How high the top of the key at `pixel` rests above the sheet, in metres: what the
## tip meets first there. 0 where there is no key.
func rest_at(pixel: Vector2) -> float:
	for entry: Dictionary in _rests:
		var rect: Rect2 = entry.rect
		if rect.has_point(pixel):
			return entry.rest
	return 0.0


## How high the top of the key at `pixel` is drawn now, in metres: lower than its
## rest while it is pushed. 0 where there is no key.
func top_at(pixel: Vector2) -> float:
	for entry: Dictionary in _rests:
		var rect: Rect2 = entry.rect
		if rect.has_point(pixel):
			return entry.top
	return 0.0


## The rectangles of the keys standing up now, in pixels.
func rects() -> Array[Rect2]:
	var list: Array[Rect2] = []
	for entry: Dictionary in _rests:
		list.append(entry.rect)
	return list


## Whether a button stands up as a key: a visible, enabled button that is not a
## check box, not drawn flat, and has not asked to stay flat.
func _stands(button: BaseButton) -> bool:
	if not button.is_visible_in_tree() or button.disabled or button is CheckBox or button is CheckButton:
		return false
	if button.get_meta("flat_key", false):
		return false
	var plain := button as Button
	return plain == null or not plain.flat


func _covered(rect: Rect2, covers: Array[Rect2]) -> bool:
	for cover in covers:
		if cover.intersects(rect):
			return true
	return false


## The transform that stands the unit key on `rect` of the sheet, `top` metres tall.
func _transform(rect: Rect2, sheet: Vector2, top: float) -> Transform3D:
	var centre := rect.get_center()
	var basis := Basis.from_scale(Vector3(rect.size.x / _pixels_per_metre, rect.size.y / _pixels_per_metre, maxf(top, 0.0002)))
	var origin := Vector3((centre.x - sheet.x * 0.5) / _pixels_per_metre, (sheet.y * 0.5 - centre.y) / _pixels_per_metre, 0.0)
	return Transform3D(basis, origin)


func _fill(multimesh: MultiMesh, entries: Array[Array]) -> void:
	if multimesh.instance_count < entries.size():
		multimesh.instance_count = entries.size() + 8
	multimesh.visible_instance_count = entries.size()
	for index in entries.size():
		var entry: Array = entries[index]
		multimesh.set_instance_transform(index, entry[0])
		multimesh.set_instance_custom_data(index, entry[1])


# ---- the two shapes ----

## A block from -0.5 to 0.5 across and 0 to 1 in height, without a bottom.
static func _box_mesh() -> ArrayMesh:
	var tool := SurfaceTool.new()
	tool.begin(Mesh.PRIMITIVE_TRIANGLES)
	# The top, then the four sides.
	_quad(tool, Vector3(-0.5, 0.5, 1.0), Vector3(0.5, 0.5, 1.0), Vector3(0.5, -0.5, 1.0), Vector3(-0.5, -0.5, 1.0), Vector3.BACK)
	_quad(tool, Vector3(-0.5, 0.5, 0.0), Vector3(0.5, 0.5, 0.0), Vector3(0.5, 0.5, 1.0), Vector3(-0.5, 0.5, 1.0), Vector3.UP)
	_quad(tool, Vector3(0.5, -0.5, 0.0), Vector3(-0.5, -0.5, 0.0), Vector3(-0.5, -0.5, 1.0), Vector3(0.5, -0.5, 1.0), Vector3.DOWN)
	_quad(tool, Vector3(-0.5, -0.5, 0.0), Vector3(-0.5, 0.5, 0.0), Vector3(-0.5, 0.5, 1.0), Vector3(-0.5, -0.5, 1.0), Vector3.LEFT)
	_quad(tool, Vector3(0.5, 0.5, 0.0), Vector3(0.5, -0.5, 0.0), Vector3(0.5, -0.5, 1.0), Vector3(0.5, 0.5, 1.0), Vector3.RIGHT)
	return tool.commit()


## A disc 1 across and 1 tall standing on the sheet, without a bottom.
static func _coin_mesh() -> ArrayMesh:
	var tool := SurfaceTool.new()
	tool.begin(Mesh.PRIMITIVE_TRIANGLES)
	for index in ROUND_SEGMENTS:
		var a := TAU * index / ROUND_SEGMENTS
		var b := TAU * (index + 1) / ROUND_SEGMENTS
		var pa := Vector3(cos(a) * 0.5, sin(a) * 0.5, 0.0)
		var pb := Vector3(cos(b) * 0.5, sin(b) * 0.5, 0.0)
		var lift := Vector3(0.0, 0.0, 1.0)
		# The top: a slice of the disc.
		tool.set_normal(Vector3.BACK)
		tool.add_vertex(lift)
		tool.add_vertex(pa + lift)
		tool.add_vertex(pb + lift)
		# The side.
		var na := Vector3(cos(a), sin(a), 0.0)
		var nb := Vector3(cos(b), sin(b), 0.0)
		tool.set_normal(na)
		tool.add_vertex(pa)
		tool.set_normal(nb)
		tool.add_vertex(pb)
		tool.set_normal(na)
		tool.add_vertex(pa + lift)
		tool.set_normal(na)
		tool.add_vertex(pa + lift)
		tool.set_normal(nb)
		tool.add_vertex(pb)
		tool.set_normal(nb)
		tool.add_vertex(pb + lift)
	return tool.commit()


## Two triangles from four corners in order around the face.
static func _quad(tool: SurfaceTool, a: Vector3, b: Vector3, c: Vector3, d: Vector3, normal: Vector3) -> void:
	tool.set_normal(normal)
	for corner: Vector3 in [a, c, b, a, d, c]:
		tool.add_vertex(corner)
