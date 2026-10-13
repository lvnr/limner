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
const GRID_SHADER := preload("res://shaders/grid.gdshader")
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
	scene.document_changed.connect(_apply_document_settings)
	scene.document_replaced.connect(_apply_document_settings)
	_apply_document_settings()


func _setup_materials() -> void:
	_noise_texture = ImageTexture.create_from_image(LimnerScene.blue_noise_image(64))
	var materials := {}
	for key: String in SHADERS:
		var material := ShaderMaterial.new()
		material.shader = SHADERS[key]
		if key.ends_with("see_through"):
			material.set_shader_parameter("blue_noise", _noise_texture)
			_see_through_materials.append(material)
		materials[key] = material
	scene.set_stroke_materials(
		materials["tube_opaque"], materials["tube_see_through"], materials["flat_opaque"], materials["flat_see_through"]
	)


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
