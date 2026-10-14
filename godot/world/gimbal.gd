class_name GimbalView
extends Node3D
## The gimbal, drawn in viewer space from what the scene says it is.
##
## Three arrows move along an axis, three squares in a plane, three rings turn about
## an axis, and the ball in the middle moves freely. The scene decides where it is,
## how big, and which handle a hand is at; this node only draws that.

const SHADER := preload("res://shaders/gimbal.gdshader")
const AXIS_COLORS: Array[Color] = [Color(0.95, 0.26, 0.21), Color(0.36, 0.78, 0.25), Color(0.2, 0.52, 0.96)]
const CENTER_COLOR := Color(0.9, 0.9, 0.9)
const HOVER_COLOR := Color(1.0, 0.95, 0.55)
const ACTIVE_COLOR := Color(1.0, 0.8, 0.1)
const AXES := ["x", "y", "z"]
## Thickness of the rings and of the arrows' shafts, as fractions of the rings' radius.
const RING_THICKNESS := 0.012
const SHAFT_RADIUS := 0.012
const TIP_RADIUS := 0.045
const TIP_LENGTH := 0.13

var scene: LimnerScene
## The render layers the handles are drawn on.
var layers := 1

## Handle name to the material its parts share.
var _materials := {}
## Handle name to its resting colour.
var _colors := {}
var _built_for := {}


func _ready() -> void:
	visible = false


## The rotation that takes the Y axis, which Godot's primitive meshes are built
## around, to axis `index`.
static func _toward(index: int) -> Basis:
	match index:
		0:
			return Basis(Vector3.FORWARD, PI / 2.0)
		2:
			return Basis(Vector3.RIGHT, PI / 2.0)
	return Basis.IDENTITY


func _material(handle: String, color: Color) -> ShaderMaterial:
	var material := ShaderMaterial.new()
	material.shader = SHADER
	material.render_priority = 10
	material.set_shader_parameter("color", color)
	_materials[handle] = material
	_colors[handle] = color
	return material


func _part(mesh: Mesh, material: Material, at: Transform3D) -> void:
	var node := MeshInstance3D.new()
	node.mesh = mesh
	node.material_override = material
	node.transform = at
	node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	node.layers = layers
	add_child(node)


## Builds the handles at unit size, from the proportions the scene works with.
func _build(gimbal: Dictionary) -> void:
	for child in get_children():
		child.queue_free()
	_materials.clear()
	_colors.clear()
	var arrow_start: float = gimbal.arrow_start
	var arrow_end: float = gimbal.arrow_end
	var plane_near: float = gimbal.plane_near
	var plane_far: float = gimbal.plane_far

	var ball := SphereMesh.new()
	ball.radius = gimbal.center_radius
	ball.height = ball.radius * 2.0
	ball.radial_segments = 24
	ball.rings = 12
	_part(ball, _material("center", CENTER_COLOR), Transform3D.IDENTITY)

	var shaft := CylinderMesh.new()
	shaft.top_radius = SHAFT_RADIUS
	shaft.bottom_radius = SHAFT_RADIUS
	shaft.height = arrow_end - TIP_LENGTH - arrow_start
	shaft.radial_segments = 8
	var tip := CylinderMesh.new()
	tip.top_radius = 0.0
	tip.bottom_radius = TIP_RADIUS
	tip.height = TIP_LENGTH
	tip.radial_segments = 16
	var ring := TorusMesh.new()
	ring.inner_radius = 1.0 - RING_THICKNESS
	ring.outer_radius = 1.0 + RING_THICKNESS
	ring.rings = 64
	ring.ring_segments = 8
	var square := BoxMesh.new()
	square.size = Vector3(plane_far - plane_near, 0.004, plane_far - plane_near)

	for index in 3:
		var toward := _toward(index)
		var color := AXIS_COLORS[index]
		var axis := _material("axis_" + AXES[index], color)
		_part(shaft, axis, Transform3D(toward, toward * Vector3(0.0, arrow_start + shaft.height * 0.5, 0.0)))
		_part(tip, axis, Transform3D(toward, toward * Vector3(0.0, arrow_end - TIP_LENGTH * 0.5, 0.0)))
		_part(ring, _material("ring_" + AXES[index], color), Transform3D(toward, Vector3.ZERO))
		# A square sits between the other two axes, square to this one.
		var middle := (plane_near + plane_far) * 0.5
		var corner := Vector3.ONE * middle
		corner[index] = 0.0
		var faint := Color(color.r, color.g, color.b, 0.65)
		_part(square, _material("plane_" + AXES[index], faint), Transform3D(toward, corner))
	_built_for = {
		"arrow_start": arrow_start,
		"arrow_end": arrow_end,
		"plane_near": plane_near,
		"plane_far": plane_far,
		"center_radius": gimbal.center_radius,
	}


func _process(_delta: float) -> void:
	var gimbal := scene.gimbal()
	visible = not gimbal.is_empty()
	if not visible:
		return
	for key: String in ["arrow_start", "arrow_end", "plane_near", "plane_far", "center_radius"]:
		if _built_for.get(key) != gimbal[key]:
			_build(gimbal)
			break
	var size: float = gimbal.size
	transform = Transform3D(Basis(gimbal.rotation as Quaternion).scaled(Vector3.ONE * size), gimbal.position)
	var hovered: Array = gimbal.hovered
	var active: String = gimbal.active
	for handle: String in _materials:
		var color: Color = _colors[handle]
		if handle == active:
			color = ACTIVE_COLOR
		elif active.is_empty() and handle in hovered:
			color = HOVER_COLOR
		elif not active.is_empty():
			# While one handle is at work the others step back.
			color.a *= 0.25
		(_materials[handle] as ShaderMaterial).set_shader_parameter("color", color)
