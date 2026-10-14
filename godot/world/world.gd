class_name LimnerWorld
extends Node3D
## Hosts the document scene, its stroke materials, the environment and the floor grid.
##
## Everything here lives in viewer space: the room the user stands in. The document
## scene places the artwork in that room; navigation moves the artwork, not the user.

const SHADERS := {
	"tube_opaque": preload("res://shaders/stroke_tube_opaque.gdshader"),
	"tube_see_through": preload("res://shaders/stroke_tube_see_through.gdshader"),
	"flat_opaque": preload("res://shaders/stroke_flat_opaque.gdshader"),
	"flat_see_through": preload("res://shaders/stroke_flat_see_through.gdshader"),
}
## The same four for paint layers that scene lights shade.
const LIT_SHADERS := {
	"tube_opaque": preload("res://shaders/stroke_tube_opaque_lit.gdshader"),
	"tube_see_through": preload("res://shaders/stroke_tube_see_through_lit.gdshader"),
	"flat_opaque": preload("res://shaders/stroke_flat_opaque_lit.gdshader"),
	"flat_see_through": preload("res://shaders/stroke_flat_see_through_lit.gdshader"),
}
## Pictures, and the lines of gizmos, bounding boxes and wireframes.
const LAYER_SHADERS := {
	"picture_flat_opaque": preload("res://shaders/picture_flat_opaque.gdshader"),
	"picture_flat_see_through": preload("res://shaders/picture_flat_see_through.gdshader"),
	"picture_sphere_opaque": preload("res://shaders/picture_sphere_opaque.gdshader"),
	"picture_sphere_see_through": preload("res://shaders/picture_sphere_see_through.gdshader"),
	"line": preload("res://shaders/line.gdshader"),
}
const GRID_SHADER := preload("res://shaders/grid.gdshader")
const HIGHLIGHT_SHADER := preload("res://shaders/highlight.gdshader")
## Half the side of the grid plane, in grid cells of the coarse spacing.
const GRID_EXTENT := 40.0

## When true the dither pattern of see-through strokes changes every frame.
var animate_dither := true
var show_grid := true

var scene: LimnerScene
var environment: Environment

var _see_through_materials: Array[ShaderMaterial] = []
var _noise_texture: ImageTexture
var _frame := 0
var _grid: MeshInstance3D
var _grid_material: ShaderMaterial
var _gimbal: GimbalView


func _ready() -> void:
	environment = Environment.new()
	environment.background_mode = Environment.BG_COLOR
	environment.background_color = Color(0.8, 0.8, 0.8)
	environment.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	environment.ambient_light_color = Color.WHITE
	environment.tonemap_mode = Environment.TONE_MAPPER_LINEAR
	var world_environment := WorldEnvironment.new()
	world_environment.name = "Environment"
	world_environment.environment = environment
	add_child(world_environment)

	scene = LimnerScene.new()
	scene.name = "Scene"
	add_child(scene)
	_setup_materials()
	_setup_grid()
	_gimbal = GimbalView.new()
	_gimbal.name = "Gimbal"
	_gimbal.scene = scene
	add_child(_gimbal)
	scene.document_changed.connect(_apply_document_settings)
	scene.document_replaced.connect(_apply_document_settings)
	_apply_document_settings()


func _setup_materials() -> void:
	_noise_texture = ImageTexture.create_from_image(LimnerScene.blue_noise_image(64))
	var materials := _materials(SHADERS)
	scene.set_stroke_materials(
		materials["tube_opaque"], materials["tube_see_through"], materials["flat_opaque"], materials["flat_see_through"]
	)
	var lit := _materials(LIT_SHADERS)
	scene.set_lit_stroke_materials(lit["tube_opaque"], lit["tube_see_through"], lit["flat_opaque"], lit["flat_see_through"])
	# The scene copies the picture materials, one copy per picture, and uses the rest as they are.
	var layer_materials := _materials(LAYER_SHADERS)
	var wire: ShaderMaterial = layer_materials["line"].duplicate()
	wire.set_shader_parameter("against_vertex_color", true)
	layer_materials["wire"] = wire
	# A model is seen from both sides, as a flat stroke is, and fades the same way.
	layer_materials["model_opaque"] = materials["flat_opaque"]
	layer_materials["model_see_through"] = materials["flat_see_through"]
	layer_materials["model_opaque_lit"] = lit["flat_opaque"]
	layer_materials["model_see_through_lit"] = lit["flat_see_through"]
	scene.set_layer_materials(layer_materials)
	var highlight := ShaderMaterial.new()
	highlight.shader = HIGHLIGHT_SHADER
	scene.set_highlight_material(highlight)


## One material per shader, by the same names. Those that dither get the noise they
## dither against and are kept, so the pattern can be moved every frame.
func _materials(shaders: Dictionary) -> Dictionary:
	var materials := {}
	for key: String in shaders:
		var material := ShaderMaterial.new()
		material.shader = shaders[key]
		if key.ends_with("see_through"):
			material.set_shader_parameter("blue_noise", _noise_texture)
			_see_through_materials.append(material)
		materials[key] = material
	return materials


func _setup_grid() -> void:
	var plane := PlaneMesh.new()
	plane.size = Vector2(2.0, 2.0)
	_grid_material = ShaderMaterial.new()
	_grid_material.shader = GRID_SHADER
	_grid = MeshInstance3D.new()
	_grid.name = "FloorGrid"
	_grid.mesh = plane
	_grid.material_override = _grid_material
	_grid.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(_grid)


func _process(_delta: float) -> void:
	_frame += 1
	var index := _frame if animate_dither else 0
	for material in _see_through_materials:
		material.set_shader_parameter("frame_index", index)
	_update_grid()


## Lays the grid on the world's ground plane. Its spacing follows the zoom in powers
## of ten, so there is always a readable grid whatever the scale of the scene.
func _update_grid() -> void:
	_grid.visible = show_grid
	if not show_grid:
		return
	var viewer_from_world := scene.viewer_from_world()
	var world_per_metre := scene.view_scale()
	var spacing := pow(10.0, floorf(log(world_per_metre) / log(10.0)))
	var extent := spacing * 10.0 * GRID_EXTENT
	# Keep the plane under the viewer, snapped to the coarse grid so lines do not swim.
	var viewer_in_world := viewer_from_world.affine_inverse().origin
	var snap := spacing * 10.0
	var center := Vector3(snappedf(viewer_in_world.x, snap), 0.0, snappedf(viewer_in_world.z, snap))
	_grid.transform = viewer_from_world * Transform3D(Basis.from_scale(Vector3(extent, 1.0, extent)), center)
	# The shader works in the plane's own units, where the plane spans -1 to 1.
	_grid_material.set_shader_parameter("spacing", spacing / extent)
	_grid_material.set_shader_parameter("reach", 1.0)
	var background := environment.background_color
	var ink := Color.BLACK if background.get_luminance() > 0.35 else Color.WHITE
	_grid_material.set_shader_parameter("line_color", Color(ink.r, ink.g, ink.b, 0.3))


func _apply_document_settings() -> void:
	environment.background_color = scene.background_color()
