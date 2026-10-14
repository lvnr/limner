class_name XRControllerModel
extends Node3D
## What is drawn for a controller: a pointed marker that runs from the controller's
## front to the brush tip, so the user sees exactly where the tip is. `handle` makes
## the other part, a plain grip for the palm.
##
## Both are built from simple shapes here. They are stand-ins that show where the
## hands are, not likenesses of any real controller.

## Radius and length of the handle, in metres.
const HANDLE_RADIUS := 0.014
const HANDLE_LENGTH := 0.09
## Radius of the marker where it leaves the controller, and at the tip, in metres.
const MARKER_RADIUS := 0.008
const TIP_RADIUS := 0.0015
const BODY_COLOR := Color(0.32, 0.34, 0.38)
const OTHER_HAND_COLOR := Color(0.62, 0.64, 0.68)
const SHADER := preload("res://xr/controller.gdshader")

## True for the hand that draws; its marker takes the interface's accent colour.
var drawing := false:
	set(value):
		drawing = value
		_material.set_shader_parameter("color", Kit.ACCENT if drawing else OTHER_HAND_COLOR)

var _material: ShaderMaterial


## The marker, for a node at a controller's aim pose.
func _init() -> void:
	name = "Marker"
	_material = _shaded(OTHER_HAND_COLOR)
	var cone := CylinderMesh.new()
	cone.bottom_radius = MARKER_RADIUS
	cone.top_radius = TIP_RADIUS
	cone.height = XRRig.TIP_OFFSET
	cone.radial_segments = 12
	cone.rings = 0
	var body := _part(cone, _material)
	# The cone's own axis is Y, narrow end up; lay it along -Z, the way the hand points.
	body.transform = Transform3D(Basis(Vector3.RIGHT, -PI / 2.0), Vector3(0.0, 0.0, -XRRig.TIP_OFFSET / 2.0))
	add_child(body)


## The handle, for a node at a controller's grip pose, whose Z axis runs along it.
static func handle() -> MeshInstance3D:
	var capsule := CapsuleMesh.new()
	capsule.radius = HANDLE_RADIUS
	capsule.height = HANDLE_LENGTH
	capsule.radial_segments = 16
	capsule.rings = 4
	var part := _part(capsule, _shaded(BODY_COLOR))
	part.name = "Handle"
	part.basis = Basis(Vector3.RIGHT, PI / 2.0)
	return part


static func _part(mesh: Mesh, material: Material) -> MeshInstance3D:
	var part := MeshInstance3D.new()
	part.mesh = mesh
	part.material_override = material
	part.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	return part


static func _shaded(color: Color) -> ShaderMaterial:
	var material := ShaderMaterial.new()
	material.shader = SHADER
	material.set_shader_parameter("color", color)
	return material
