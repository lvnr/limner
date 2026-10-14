class_name LimnerCapture
extends Node
## Takes pictures of the artwork off screen: stills at any size and 360 pictures.
##
## A picture shows the artwork only. While one is taken, the floor grid, the gizmos
## of cameras, lights and spawn areas, onion skins and the tool cursor are hidden,
## and they are put back before anything reaches the screen.
##
## A "view" says where a picture is taken from: `{transform, fov, near, far}`, with
## the transform in viewer space looking along its -Z, and `fov` the vertical angle
## in degrees. `current_view` and `layer_view` make one.

## How much work a picture gets. `passes` is how many times it is rendered with the
## dither pattern of see-through strokes in a new place, and `supersample` how many
## pixels are rendered per pixel kept, along each axis.
const QUALITIES := {
	"draft": {"passes": 1, "supersample": 1},
	"good": {"passes": 8, "supersample": 2},
	"best": {"passes": 32, "supersample": 2},
}
## What each quality trades, for the panels to show.
const QUALITY_NOTES := {
	"draft": "One pass at the final size. Fastest; see-through strokes come out stippled.",
	"good": "8 passes at twice the size. See-through strokes are smooth, with a little grain left. Up to 32 times the rendering work of Draft.",
	"best": "32 passes at twice the size. See-through strokes are clean. Up to 128 times the rendering work of Draft.",
}
## The largest picture, along either side.
const MAX_SIZE := 8192
## The longest side of one shot, in rendered pixels. Larger pictures are rendered in
## tiles. A multiple of the 64-pixel dither pattern, so that tiles join without a seam.
const TILE := 2048
## What a 360 picture can be laid out as.
const LAYOUTS := ["equirect", "equirect_stereo", "cube_strip", "cube_cross"]
## Distance between the eyes of a stereo picture, in viewer-space metres.
const EYE_SEPARATION := 0.064
## How far above a floor-level spawn area a standing viewer's eyes are, in the
## area's own metres.
const EYE_HEIGHT := 1.6
## The lens and clipping planes of a view that no layer decides.
const DEFAULT_FOV := 60.0
const NEAR := 0.01
const FAR := 1000.0
## Where each cube face looks, as the axes (right, up, back) of its camera within
## the capture's own axes. Up and down are turned so that they join the front face.
const FACE_AXES := {
	"front": Basis(Vector3.RIGHT, Vector3.UP, Vector3.BACK),
	"right": Basis(Vector3.BACK, Vector3.UP, Vector3.LEFT),
	"back": Basis(Vector3.LEFT, Vector3.UP, Vector3.FORWARD),
	"left": Basis(Vector3.FORWARD, Vector3.UP, Vector3.RIGHT),
	"up": Basis(Vector3.RIGHT, Vector3.BACK, Vector3.DOWN),
	"down": Basis(Vector3.RIGHT, Vector3.FORWARD, Vector3.UP),
}
## Where each face goes in a cube strip and in a cube cross, in faces from the top left.
const STRIP_CELLS := {"right": Vector2i(0, 0), "left": Vector2i(1, 0), "up": Vector2i(2, 0), "down": Vector2i(3, 0), "front": Vector2i(4, 0), "back": Vector2i(5, 0)}
const CROSS_CELLS := {"up": Vector2i(1, 0), "left": Vector2i(0, 1), "front": Vector2i(1, 1), "right": Vector2i(2, 1), "back": Vector2i(3, 1), "down": Vector2i(1, 2)}

var world: LimnerWorld

var _rig: CaptureRig
## How many pictures are being taken, one inside the other.
var _depth := 0
## What was hidden for the picture, to be put back.
var _restore := {}


func _init(limner_world: LimnerWorld) -> void:
	world = limner_world
	name = "Capture"


## The capture that goes with `editor`, made the first time it is asked for.
static func of(editor: LimnerEditor) -> LimnerCapture:
	var capture: LimnerCapture = editor.get_node_or_null("Capture")
	if capture == null:
		capture = LimnerCapture.new(editor.scene.get_parent())
		editor.add_child(capture)
	return capture


## True when pictures can be rendered: there is none without a display to render with.
static func can_render() -> bool:
	return DisplayServer.get_name() != "headless"


# ---- where from ----

## The view the window shows: the desktop view, or the head in a headset.
func current_view() -> Dictionary:
	var camera := world.get_viewport().get_camera_3d()
	if camera == null:
		return {"transform": Transform3D(Basis.IDENTITY, DesktopShell.HEAD), "fov": DEFAULT_FOV, "near": NEAR, "far": FAR}
	return {"transform": camera.global_transform, "fov": camera.fov, "near": camera.near, "far": camera.far}


## The view from a layer at the playhead. A camera layer gives its place, its
## direction and its lens. Its scale changes nothing a lens does; it only moves the
## clipping planes, as in Quill. A floor-level spawn area gives the place of a
## standing viewer's eyes above it.
func layer_view(layer: int) -> Dictionary:
	var placed := world.scene.layer_transform(layer)
	var details := world.scene.layer_details(layer)
	var props: Dictionary = details.get("props", {})
	var size := absf(placed.basis.get_scale().x)
	var axes := placed.basis.orthonormalized()
	if axes.determinant() < 0.0:
		# A mirrored layer still gives an ordinary, unmirrored picture.
		axes.x = -axes.x
	var origin := placed.origin
	if details.get("kind", "") == "viewpoint" and props.get("floor_level", false):
		origin += axes.y * EYE_HEIGHT * size
	var fov: float = props.get("fov", DEFAULT_FOV)
	return {"transform": Transform3D(axes, origin), "fov": fov, "near": NEAR * size, "far": FAR * size}


## `view` turned upright, keeping its place and the compass direction it faces. A
## 360 picture taken from a tilted head would have a tilted horizon.
static func level(view: Dictionary) -> Dictionary:
	var placed: Transform3D = view.transform
	var ahead := -placed.basis.z
	ahead.y = 0.0
	if ahead.length() < 1e-4:
		# Looking straight up or down: the top of the view says which way is ahead.
		ahead = placed.basis.y * signf(placed.basis.z.y)
		ahead.y = 0.0
	var upright := view.duplicate()
	upright.transform = Transform3D(Basis.looking_at(ahead.normalized(), Vector3.UP), placed.origin)
	return upright


# ---- pictures ----

## A still of `view`, `size` pixels large, as a finished RGBA8 image. `quality` is
## a key of `QUALITIES`. With `transparent` the background is left out and alpha is
## how much of each pixel the artwork covers (straight, not multiplied into the colour).
func still(view: Dictionary, size: Vector2i, quality: String, transparent := false) -> Image:
	begin()
	var image := _picture(view, size.clamp(Vector2i.ONE, Vector2i(MAX_SIZE, MAX_SIZE)), quality, transparent, false)
	end()
	return _finished(image, transparent)


## The size of a 360 picture of `layout` that is `width` pixels wide. An
## equirectangular picture is twice as wide as high; a stereo one stacks two.
static func panorama_size(layout: String, width: int) -> Vector2i:
	match layout:
		"equirect_stereo":
			return Vector2i(width, width)
		"cube_strip":
			return Vector2i(width / 6 * 6, width / 6)
		"cube_cross":
			return Vector2i(width / 4 * 4, width / 4 * 3)
	return Vector2i(width, width / 2)


## A picture of everything around the place of `view`, with the direction it faces
## in the middle. `layout` is one of `LAYOUTS`:
##
##   equirect         longitude across, latitude down
##   equirect_stereo  two of those, the left eye's above the right eye's, taken
##                    `eye_separation` metres apart
##   cube_strip       six square faces in a row: right, left, up, down, front, back
##   cube_cross       the faces unfolded: up above front, down below it, and left,
##                    front, right, back in the middle row
func panorama(view: Dictionary, layout: String, width: int, quality: String, transparent := false, eye_separation := EYE_SEPARATION) -> Image:
	var size := panorama_size(layout, clampi(width, 64, MAX_SIZE))
	begin()
	var image: Image
	match layout:
		"cube_strip":
			image = _cube_sheet(view, size, size.y, STRIP_CELLS, quality, transparent)
		"cube_cross":
			image = _cube_sheet(view, size, size.y / 3, CROSS_CELLS, quality, transparent)
		"equirect_stereo":
			image = Image.create_empty(size.x, size.y, false, Image.FORMAT_RGBA8)
			var half := Vector2i(size.x, size.y / 2)
			for eye in 2:
				var side := (eye - 0.5) * eye_separation
				var seen := view.duplicate()
				var placed: Transform3D = view.transform
				seen.transform = Transform3D(placed.basis, placed.origin + placed.basis.x * side)
				image.blit_rect(_equirect(seen, half, quality, transparent), Rect2i(Vector2i.ZERO, half), Vector2i(0, half.y * eye))
		_:
			image = _equirect(view, size, quality, transparent)
	end()
	return _finished(image, transparent)


## A picture without transparency needs no alpha; its file is smaller without.
func _finished(image: Image, transparent: bool) -> Image:
	if not transparent:
		image.convert(Image.FORMAT_RGB8)
	return image


## Hides what is not artwork. Pictures do this themselves; whoever takes many in a
## row, as a video render does, calls it once around them.
func begin() -> void:
	_depth += 1
	if _depth > 1:
		return
	var options := world.scene.options()
	var switched := {}
	for key: String in ["show_extras", "onion_skin"]:
		if options.get(key, false):
			switched[key] = false
	_restore = {"grid": world.show_grid, "dither": world.animate_dither, "options": switched, "nodes": []}
	world.show_grid = false
	world.animate_dither = true
	if not switched.is_empty():
		world.scene.exec({"cmd": "set_options", "values": switched})
	# Whatever shares the room with the artwork: the grid, the tool cursor.
	for child in world.get_children():
		var node := child as Node3D
		if node != null and node != world.scene and not node is Camera3D and node.visible:
			node.visible = false
			_restore.nodes.append(node)
	world.scene.sync()
	if _rig == null:
		_rig = CaptureRig.new(world.get_world_3d())
		_rig.next_pattern = _next_pattern
		add_child(_rig)


## Puts back what `begin` hid and lets go of the render targets.
func end() -> void:
	_depth -= 1
	if _depth > 0:
		return
	world.show_grid = _restore.grid
	world.animate_dither = _restore.dither
	var switched: Dictionary = _restore.options
	if not switched.is_empty():
		var back := {}
		for key: String in switched:
			back[key] = true
		world.scene.exec({"cmd": "set_options", "values": back})
	for node: Variant in _restore.nodes:
		if is_instance_valid(node):
			node.visible = true
	world.scene.sync()
	_rig.free()
	_rig = null


## Brings what the engine draws in line with the document at this instant. The
## engine itself hears of moved nodes only at the end of a frame, which is too late
## for a picture taken within the frame that moved the playhead.
func settle() -> void:
	world.scene.sync()
	_push_transforms(world.scene)


func _push_transforms(node: Node) -> void:
	for child in node.get_children():
		var placed := child as Node3D
		if placed != null:
			placed.force_update_transform()
		_push_transforms(child)


## Moves the dither pattern of see-through strokes on by one step. The world does
## that once per frame as part of its own processing; a capture takes many passes
## within one frame, so it runs that step by hand.
func _next_pattern() -> void:
	world.advance_dither()


## One picture of `view`, in tiles when it is larger than a single shot can be.
func _picture(view: Dictionary, size: Vector2i, quality: String, transparent: bool, linear: bool) -> Image:
	var settings: Dictionary = QUALITIES[quality]
	_rig.passes = settings.passes
	_rig.supersample = settings.supersample
	_rig.transparent = transparent
	_rig.linear = linear
	_rig.backdrop = world.environment.background_color
	var step: int = TILE / int(settings.supersample)
	var image: Image
	for top in range(0, size.y, step):
		for left in range(0, size.x, step):
			var region := Rect2i(left, top, mini(step, size.x - left), mini(step, size.y - top))
			_rig.shoot(view, size, region)
			var tile := _rig.image()
			if region.size == size:
				image = tile
				break
			if image == null:
				image = Image.create_empty(size.x, size.y, false, tile.get_format())
			image.blit_rect(tile, Rect2i(Vector2i.ZERO, region.size), region.position)
	return image


## The view through one face of the cube around `view`. `reach` is the tangent of
## half the angle the face covers: 1 for exactly a quarter turn.
func _face_view(view: Dictionary, face: String, reach: float) -> Dictionary:
	var placed: Transform3D = view.transform
	var axes: Basis = FACE_AXES[face]
	var seen := view.duplicate()
	seen.transform = Transform3D(placed.basis * axes, placed.origin)
	seen.fov = rad_to_deg(2.0 * atan(reach))
	return seen


## Six finished cube faces of `face` pixels, each placed in its cell of the sheet.
func _cube_sheet(view: Dictionary, size: Vector2i, face: int, cells: Dictionary, quality: String, transparent: bool) -> Image:
	var sheet := Image.create_empty(size.x, size.y, false, Image.FORMAT_RGBA8)
	for name: String in cells:
		var cell: Vector2i = cells[name]
		var picture := _picture(_face_view(view, name, 1.0), Vector2i(face, face), quality, transparent, false)
		sheet.blit_rect(picture, Rect2i(0, 0, face, face), cell * face)
	return sheet


## An equirectangular picture from `view`: six cube faces in linear light, unrolled.
func _equirect(view: Dictionary, size: Vector2i, quality: String, transparent: bool) -> Image:
	# A third of the width per face keeps the faces a little finer than the picture
	# is where it is finest, on the horizon.
	var face := int(ceil(size.x / 3.0 / 16.0)) * 16
	# Two extra pixels all round, so that filtering at an edge finds real picture.
	var reach := 1.0 + 4.0 / face
	var faces: Array[Texture2D] = []
	for name: String in CaptureRig.FACES:
		faces.append(ImageTexture.create_from_image(_picture(_face_view(view, name, reach), Vector2i(face, face), quality, transparent, true)))
	var taps: int = QUALITIES[quality].supersample
	var image := Image.create_empty(size.x, size.y, false, Image.FORMAT_RGBA8)
	for top in range(0, size.y, TILE):
		for left in range(0, size.x, TILE):
			var region := Rect2i(left, top, mini(TILE, size.x - left), mini(TILE, size.y - top))
			image.blit_rect(_rig.unroll(faces, reach, size, region, taps), Rect2i(Vector2i.ZERO, region.size), region.position)
	return image


# ---- files ----

## Where pictures go when nobody chooses: `Limner` in the user's Pictures folder.
static func default_folder() -> String:
	return OS.get_system_dir(OS.SYSTEM_DIR_PICTURES).path_join("Limner")


## A file name made of the document's title and the time now: `Title_20261014_153012.png`.
static func stamped_name(title: String, extension: String) -> String:
	var clean := title.strip_edges().validate_filename()
	if clean == "":
		clean = "Untitled"
	var now := Time.get_datetime_dict_from_system()
	return "%s_%04d%02d%02d_%02d%02d%02d.%s" % [clean, now.year, now.month, now.day, now.hour, now.minute, now.second, extension]


## Writes `image` as a PNG at `path`, making its folder if need be.
static func save_png(image: Image, path: String) -> Error:
	var made := DirAccess.make_dir_recursive_absolute(path.get_base_dir())
	if made != OK:
		return made
	return image.save_png(path)
