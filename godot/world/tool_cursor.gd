class_name ToolCursor
extends Node3D
## The tool preview at the drawing hand, in viewer space.
##
## Paint and Line show the brush: a ball of the paint colour, as wide and as opaque as
## the stroke will be. Every other tool shows its sphere of influence, with the inner
## sphere of full effect for the tools that have one. With mirrored working on, a
## fainter twin of the preview shows where the tool also acts, at the mirror image
## of the hand.
##
## When the trigger would do nothing because the current layer is not one the tool
## can work on (a camera, a light, a locked layer…), the preview is a hollow grey
## ring with no colour in it.

const SHADER := preload("res://shaders/cursor.gdshader")
## How strongly the twin at the mirror image is drawn, against the preview at the hand.
const TWIN_STRENGTH := 0.55
## The rim of a preview whose trigger would do nothing.
const BLOCKED_RIM := Color(0.5, 0.5, 0.5, 0.75)

var scene: LimnerScene

var _hand: Look
var _twin: Look


## One preview: the brush ball, the sphere of influence and its inner sphere.
class Look:
	var root: Node3D
	var brush: MeshInstance3D
	var sphere: MeshInstance3D
	var inner: MeshInstance3D
	var brush_material: ShaderMaterial
	var sphere_material: ShaderMaterial
	var inner_material: ShaderMaterial

	func _init(parent: Node3D, ball: Mesh, name: String) -> void:
		root = Node3D.new()
		root.name = name
		parent.add_child(root)
		brush_material = _material()
		sphere_material = _material()
		inner_material = _material()
		brush = _instance(ball, brush_material)
		sphere = _instance(ball, sphere_material)
		inner = _instance(ball, inner_material)

	func _material() -> ShaderMaterial:
		var material := ShaderMaterial.new()
		material.shader = SHADER
		return material

	func _instance(mesh: Mesh, material: Material) -> MeshInstance3D:
		var node := MeshInstance3D.new()
		node.mesh = mesh
		node.material_override = material
		node.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# With the other things that help while working and are not part of the picture.
		node.layers = LimnerWorld.HELPER_LAYERS
		node.visible = false
		root.add_child(node)
		return node

	## Shows the preview for `cursor`, drawn at `strength` of full.
	func show_for(cursor: Dictionary, ink: Color, strength: float) -> void:
		root.visible = true
		var radius: float = cursor.radius
		var paints: bool = cursor.tool in ["paint", "line"]
		var blocked := String(cursor.get("blocked", "")) != ""
		brush.visible = paints
		sphere.visible = not paints
		inner.visible = not paints and not blocked and cursor.inner != null and cursor.inner > radius * 0.02
		if blocked:
			# A hollow grey ring: nothing would happen here.
			var shown := brush if paints else sphere
			var material := brush_material if paints else sphere_material
			shown.scale = Vector3.ONE * radius
			material.set_shader_parameter("fill", Color(BLOCKED_RIM.r, BLOCKED_RIM.g, BLOCKED_RIM.b, 0.0))
			material.set_shader_parameter("rim", Color(BLOCKED_RIM.r, BLOCKED_RIM.g, BLOCKED_RIM.b, BLOCKED_RIM.a * strength))
			return
		if paints:
			var color: Color = cursor.color
			brush.scale = Vector3.ONE * radius
			brush_material.set_shader_parameter("fill", Color(color.r, color.g, color.b, clampf(cursor.opacity, 0.15, 1.0) * strength))
			brush_material.set_shader_parameter("rim", Color(color.r, color.g, color.b, strength))
			return
		var rim := (0.85 if cursor.active else 0.55) * strength
		sphere.scale = Vector3.ONE * radius
		sphere_material.set_shader_parameter("fill", Color(ink.r, ink.g, ink.b, 0.04 * strength))
		sphere_material.set_shader_parameter("rim", Color(ink.r, ink.g, ink.b, rim))
		if inner.visible:
			inner.scale = Vector3.ONE * float(cursor.inner)
			inner_material.set_shader_parameter("fill", Color(ink.r, ink.g, ink.b, 0.03 * strength))
			inner_material.set_shader_parameter("rim", Color(ink.r, ink.g, ink.b, rim * 0.6))


func _ready() -> void:
	var ball := SphereMesh.new()
	ball.radius = 1.0
	ball.height = 2.0
	ball.radial_segments = 32
	ball.rings = 16
	_hand = Look.new(self, ball, "AtHand")
	_twin = Look.new(self, ball, "AtMirror")


func _process(_delta: float) -> void:
	var cursor := scene.tool_cursor()
	# At a handle of the gimbal the trigger works the handle, so the tool steps aside.
	visible = cursor.tracked and not cursor.at_handle
	if not visible:
		return
	position = cursor.position
	# Dark on light scenes would vanish on dark ones, so the rim takes the opposite of the background.
	var background := get_viewport().get_camera_3d().get_world_3d().environment.background_color if _has_environment() else Color(0.8, 0.8, 0.8)
	var ink := Color.BLACK if background.get_luminance() > 0.35 else Color.WHITE
	_hand.show_for(cursor, ink, 1.0)
	var mirror: Variant = cursor.get("mirror_position")
	_twin.root.visible = mirror != null
	if mirror != null:
		# The twin hangs off the preview at the hand, which has no turn or scale of its own.
		_twin.root.position = (mirror as Vector3) - position
		_twin.show_for(cursor, ink, TWIN_STRENGTH)


## True when the preview shows that the trigger would do nothing.
func is_blocked() -> bool:
	return visible and String(scene.tool_cursor().get("blocked", "")) != ""


## The colour the brush ball is filled with now, for tests.
func brush_fill() -> Color:
	return _hand.brush_material.get_shader_parameter("fill")


## Where the twin at the mirror image is, in viewer space, or null when there is none.
func mirror_position() -> Variant:
	return _twin.root.position + position if visible and _twin.root.visible else null


func _has_environment() -> bool:
	var camera := get_viewport().get_camera_3d()
	return camera != null and camera.get_world_3d() != null and camera.get_world_3d().environment != null
