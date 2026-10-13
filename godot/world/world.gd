class_name LimnerWorld
extends Node3D
## Hosts the document scene, its stroke materials and the environment.

const SHADERS := {
	"tube_opaque": preload("res://shaders/stroke_tube_opaque.gdshader"),
	"tube_see_through": preload("res://shaders/stroke_tube_see_through.gdshader"),
	"flat_opaque": preload("res://shaders/stroke_flat_opaque.gdshader"),
	"flat_see_through": preload("res://shaders/stroke_flat_see_through.gdshader"),
}

## When true the dither pattern of see-through strokes changes every frame.
var animate_dither := true

var scene: LimnerScene
var environment: Environment

var _see_through_materials: Array[ShaderMaterial] = []
var _noise_texture: ImageTexture
var _frame := 0


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


func _process(_delta: float) -> void:
	_frame += 1
	var index := _frame if animate_dither else 0
	for material in _see_through_materials:
		material.set_shader_parameter("frame_index", index)


## Opens the document at `path`. Returns the dictionary from `LimnerScene.open`.
func open_document(path: String) -> Dictionary:
	var result := scene.open(path)
	if result.ok:
		environment.background_color = scene.background_color()
	return result
