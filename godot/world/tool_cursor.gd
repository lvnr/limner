class_name ToolCursor
extends Node3D
## The tool preview at the drawing hand, in viewer space.
##
## Paint and Line show the brush: a ball of the paint colour, as wide and as opaque as
## the stroke will be. Every other tool shows its sphere of influence, with the inner
## sphere of full effect for the tools that have one.

const SHADER := preload("res://shaders/cursor.gdshader")

var scene: LimnerScene

var _brush: MeshInstance3D
var _sphere: MeshInstance3D
var _inner: MeshInstance3D
var _brush_material: ShaderMaterial
var _sphere_material: ShaderMaterial
var _inner_material: ShaderMaterial


func _ready() -> void:
	var ball := SphereMesh.new()
	ball.radius = 1.0
	ball.height = 2.0
	ball.radial_segments = 32
	ball.rings = 16
	_brush_material = _material()
	_sphere_material = _material()
	_inner_material = _material()
	_brush = _instance(ball, _brush_material)
	_sphere = _instance(ball, _sphere_material)
	_inner = _instance(ball, _inner_material)


func _material() -> ShaderMaterial:
	var material := ShaderMaterial.new()
	material.shader = SHADER
	return material


func _instance(mesh: Mesh, material: Material) -> MeshInstance3D:
	var node := MeshInstance3D.new()
	node.mesh = mesh
	node.material_override = material
	node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	node.visible = false
	add_child(node)
	return node


func _process(_delta: float) -> void:
	var cursor := scene.tool_cursor()
	visible = cursor.tracked
	if not visible:
		return
	position = cursor.position
	var radius: float = cursor.radius
	var paints: bool = cursor.tool in ["paint", "line"]
	_brush.visible = paints
	_sphere.visible = not paints
	_inner.visible = not paints and cursor.inner != null and cursor.inner > radius * 0.02
	if paints:
		var color: Color = cursor.color
		_brush.scale = Vector3.ONE * radius
		_brush_material.set_shader_parameter("fill", Color(color.r, color.g, color.b, clampf(cursor.opacity, 0.15, 1.0)))
		_brush_material.set_shader_parameter("rim", Color(color.r, color.g, color.b, 1.0))
		return
	# Dark on light scenes would vanish on dark ones, so the rim takes the opposite of the background.
	var background := get_viewport().get_camera_3d().get_world_3d().environment.background_color if _has_environment() else Color(0.8, 0.8, 0.8)
	var ink := Color.BLACK if background.get_luminance() > 0.35 else Color.WHITE
	var strength := 0.85 if cursor.active else 0.55
	_sphere.scale = Vector3.ONE * radius
	_sphere_material.set_shader_parameter("fill", Color(ink.r, ink.g, ink.b, 0.04))
	_sphere_material.set_shader_parameter("rim", Color(ink.r, ink.g, ink.b, strength))
	if _inner.visible:
		_inner.scale = Vector3.ONE * float(cursor.inner)
		_inner_material.set_shader_parameter("fill", Color(ink.r, ink.g, ink.b, 0.03))
		_inner_material.set_shader_parameter("rim", Color(ink.r, ink.g, ink.b, strength * 0.6))


func _has_environment() -> bool:
	var camera := get_viewport().get_camera_3d()
	return camera != null and camera.get_world_3d() != null and camera.get_world_3d().environment != null
