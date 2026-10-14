class_name CaptureRig
extends Node
## The off-screen camera and targets that offline rendering is done with.
##
## A shot is one rectangle of a picture, rendered several times over ("passes") and
## averaged. See-through strokes are drawn by keeping or discarding whole pixels
## against a noise pattern, so a single pass is stippled; each pass sees the pattern
## in a new place, and their mean is the smooth transparency the stipple stands for.
##
## Three kinds of target, nested so that the engine draws them in this order within
## one draw:
##
##   the scene     the artwork, in linear light, larger than the result by the
##                 supersampling factor, with 4x MSAA
##   two means     take turns holding the mean of the passes so far
##   the result    the mean brought down to the final size
##
## Everything happens inside the call: `shoot` asks the engine to draw once per pass
## and returns with the result ready, without waiting for a frame and without
## showing anything on screen.

const ACCUMULATE := preload("res://shaders/capture_accumulate.gdshader")
const RESOLVE := preload("res://shaders/capture_resolve.gdshader")
const PANORAMA := preload("res://shaders/capture_panorama.gdshader")
## The cube faces a 360 picture is made from, in the order `unroll` takes them.
const FACES := ["front", "right", "back", "left", "up", "down"]
## Render layers that hold artwork. Anything on other layers is left out of captures.
const ARTWORK_LAYERS := 1

## How many times each shot is rendered.
var passes := 1
## Rendered pixels per pixel of the result, along each axis.
var supersample := 1
## True to leave the background out; the result then carries coverage as alpha.
var transparent := false
## True for linear light with colour multiplied by coverage, in float precision, to
## be processed further. False for a finished sRGB picture with straight alpha.
var linear := false
## The background colour as the screen shows it, needed to leave it out.
var backdrop := Color.BLACK
## Called before every pass; must move the dither pattern of see-through strokes.
var next_pattern: Callable

var camera: Camera3D

var _scene: SubViewport
var _means: Array[SubViewport] = []
var _mean_materials: Array[ShaderMaterial] = []
var _result: SubViewport
var _result_material: ShaderMaterial
var _unrolled: SubViewport
var _unroll_material: ShaderMaterial
## Canvases and canvas items made directly on the rendering server, freed with the rig.
var _server_items: Array[RID] = []


## `world` is the 3D world the artwork lives in: that of the window showing it.
func _init(world: World3D) -> void:
	_result = _target(false)
	_result.name = "Result"
	_result_material = _fill(_result, RESOLVE)
	for i in 2:
		var mean := _target(true)
		mean.name = "Mean%d" % i
		_mean_materials.append(_fill(mean, ACCUMULATE))
		_means.append(mean)
		_result.add_child(mean)

	_scene = SubViewport.new()
	_scene.name = "Scene"
	_scene.world_3d = world
	_scene.use_hdr_2d = true
	_scene.msaa_3d = Viewport.MSAA_4X
	_scene.render_target_update_mode = SubViewport.UPDATE_DISABLED
	camera = Camera3D.new()
	camera.cull_mask = ARTWORK_LAYERS
	_scene.add_child(camera)
	# A child is drawn before its parent, so the scene is ready when the means need it.
	_means[0].add_child(_scene)
	add_child(_result)

	_unrolled = _target(false)
	_unrolled.name = "Unrolled"
	_unroll_material = _fill(_unrolled, PANORAMA)
	add_child(_unrolled)

	for i in 2:
		_mean_materials[i].set_shader_parameter("source", _scene.get_texture())
		_mean_materials[i].set_shader_parameter("previous", _means[1 - i].get_texture())


## A target for flat drawing only, in float precision when `precise`.
func _target(precise: bool) -> SubViewport:
	var target := SubViewport.new()
	target.disable_3d = true
	target.transparent_bg = true
	target.use_hdr_2d = precise
	target.render_target_update_mode = SubViewport.UPDATE_DISABLED
	return target


## Covers `target` with a rectangle drawn by `shader`.
##
## The rectangle is given to the rendering server directly. A node would only hand
## over its drawing at the end of a frame, and a shot may be asked for before that.
func _fill(target: SubViewport, shader: Shader) -> ShaderMaterial:
	var material := ShaderMaterial.new()
	material.shader = shader
	var canvas := RenderingServer.canvas_create()
	var item := RenderingServer.canvas_item_create()
	RenderingServer.canvas_item_set_parent(item, canvas)
	# Larger than any target; the shaders work from the position on the target.
	RenderingServer.canvas_item_add_rect(item, Rect2(0.0, 0.0, 16384.0, 16384.0), Color.WHITE)
	RenderingServer.canvas_item_set_material(item, material.get_rid())
	RenderingServer.viewport_attach_canvas(target.get_viewport_rid(), canvas)
	_server_items.append(item)
	_server_items.append(canvas)
	return material


func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE:
		for item in _server_items:
			RenderingServer.free_rid(item)


## Renders `region` of a picture `full_size` pixels large, seen as `view` says:
## `{transform, fov, near, far}`, where `fov` is the angle the whole picture covers
## from top to bottom, in degrees. The result is then in `texture()` and `image()`.
func shoot(view: Dictionary, full_size: Vector2i, region: Rect2i) -> void:
	var rendered := region.size * supersample
	_scene.size = rendered
	_scene.transparent_bg = transparent
	for mean in _means:
		mean.size = rendered
	_result.size = region.size
	_result.use_hdr_2d = linear
	_result_material.set_shader_parameter("supersample", supersample)
	_result_material.set_shader_parameter("finish", not linear)
	var clear := backdrop.srgb_to_linear()
	for material in _mean_materials:
		material.set_shader_parameter("cut_out", transparent)
		material.set_shader_parameter("backdrop", Vector3(clear.r, clear.g, clear.b))
	_aim(view, full_size, region)
	for pass_index in passes:
		next_pattern.call()
		var turn := pass_index % 2
		_mean_materials[turn].set_shader_parameter("weight", 1.0 / (pass_index + 1))
		_draw_once(_scene)
		_draw_once(_means[turn])
		if pass_index == passes - 1:
			_result_material.set_shader_parameter("accumulated", _means[turn].get_texture())
			_draw_once(_result)
		RenderingServer.force_draw(false)


## The last shot as a texture, valid until the next one.
func texture() -> ViewportTexture:
	return _result.get_texture()


## The last shot as an image: RGBA8 for a finished picture, float for linear data.
func image() -> Image:
	return _result.get_texture().get_image()


## Draws `region` of an equirectangular picture `full_size` pixels large from six
## cube faces, given in the order of `FACES` as linear shots. `reach` is the tangent
## of half the angle each face covers, and `taps` the samples taken per pixel along
## each axis. Returns the finished region.
func unroll(faces: Array[Texture2D], reach: float, full_size: Vector2i, region: Rect2i, taps: int) -> Image:
	for i in FACES.size():
		_unroll_material.set_shader_parameter("face_" + FACES[i], faces[i])
	_unroll_material.set_shader_parameter("reach", reach)
	_unroll_material.set_shader_parameter("picture_size", Vector2(full_size))
	_unroll_material.set_shader_parameter("region", Vector4(
		float(region.position.x) / full_size.x, float(region.position.y) / full_size.y,
		float(region.size.x) / full_size.x, float(region.size.y) / full_size.y))
	_unroll_material.set_shader_parameter("taps", taps)
	_unrolled.size = region.size
	_draw_once(_unrolled)
	RenderingServer.force_draw(false)
	return _unrolled.get_texture().get_image()


## Points the camera at the part of the view that `region` is.
func _aim(view: Dictionary, full_size: Vector2i, region: Rect2i) -> void:
	var near: float = view.near
	var fov: float = view.fov
	# The whole picture on the near plane, and the region's share of it.
	var height := 2.0 * near * tan(deg_to_rad(fov) * 0.5)
	var width := height * full_size.x / full_size.y
	var center := (Vector2(region.position) + Vector2(region.size) * 0.5) / Vector2(full_size) - Vector2(0.5, 0.5)
	camera.keep_aspect = Camera3D.KEEP_HEIGHT
	camera.set_frustum(height * region.size.y / full_size.y, Vector2(center.x * width, -center.y * height), near, view.far)
	camera.global_transform = view.transform
	# The engine hears of a moved node at the end of a frame; the draw is now.
	camera.force_update_transform()


## Has the engine draw `target` in its next draw, and only in that one.
func _draw_once(target: SubViewport) -> void:
	RenderingServer.viewport_set_update_mode(target.get_viewport_rid(), RenderingServer.VIEWPORT_UPDATE_ONCE)
