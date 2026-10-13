extends SceneTree
# Spike: compare two order-independent transparency modes on the Mobile renderer.
# Left column: screen-door (discard against stroke-seeded noise). Right column: alpha-to-coverage + alpha-to-one.

const SHADER_DOOR := """
shader_type spatial;
render_mode unshaded, cull_disabled;
uniform vec4 col;
uniform float seed = 0.0;
float ign(vec2 p) { return fract(52.9829189 * fract(dot(p, vec2(0.06711056, 0.00583715)))); }
void fragment() {
	ALBEDO = col.rgb;
	vec2 p = FRAGCOORD.xy + vec2(seed * 37.0, seed * 17.0);
	if (col.a < 0.999 && col.a <= ign(p)) { discard; }
}
"""

const SHADER_A2C := """
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_always, alpha_to_coverage_and_one;
uniform vec4 col;
uniform float seed = 0.0;
void fragment() {
	ALBEDO = col.rgb;
	ALPHA = col.a;
}
"""

var frames := 0
var out_path := "res://../local/out/transparency_spike.png"

func quad(shader_code: String, pos: Vector3, size: Vector2, col: Color, seed: float) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var q := QuadMesh.new()
	q.size = size
	mi.mesh = q
	var sh := Shader.new()
	sh.code = shader_code
	var mat := ShaderMaterial.new()
	mat.shader = sh
	mat.set_shader_parameter("col", col)
	mat.set_shader_parameter("seed", seed)
	mi.material_override = mat
	mi.position = pos
	return mi

func column(shader_code: String, x: float) -> void:
	# Three overlapping translucent quads at different depths.
	root.add_child(quad(shader_code, Vector3(x - 0.25, 0.55, -0.2), Vector2(0.9, 0.9), Color(1, 0.1, 0.1, 0.5), 1.0))
	root.add_child(quad(shader_code, Vector3(x + 0.25, 0.55, 0.0), Vector2(0.9, 0.9), Color(0.1, 0.2, 1, 0.5), 2.0))
	root.add_child(quad(shader_code, Vector3(x, 0.25, 0.2), Vector2(0.9, 0.5), Color(0.1, 0.9, 0.2, 0.25), 3.0))
	# Opaque quad behind and alpha ramp in front of a white bar.
	root.add_child(quad(shader_code, Vector3(x, -0.75, -0.3), Vector2(1.8, 0.3), Color(1, 1, 1, 1), 0.0))
	for i in 9:
		root.add_child(quad(shader_code, Vector3(x - 0.8 + 0.2 * i, -0.75, 0.0), Vector2(0.2, 0.5), Color(0, 0, 0, i / 8.0), 4.0))
	# Two stacked quads with the same colour and 50% alpha: build-up test.
	root.add_child(quad(shader_code, Vector3(x - 0.5, -1.35, 0.0), Vector2(0.6, 0.4), Color(0, 0, 0, 0.5), 5.0))
	root.add_child(quad(shader_code, Vector3(x - 0.2, -1.35, 0.1), Vector2(0.6, 0.4), Color(0, 0, 0, 0.5), 6.0))

func _initialize() -> void:
	root.msaa_3d = Viewport.MSAA_4X
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.8, 0.8, 0.8)
	var we := WorldEnvironment.new()
	we.environment = env
	root.add_child(we)
	var cam := Camera3D.new()
	cam.position = Vector3(0, -0.3, 4.2)
	cam.fov = 50
	root.add_child(cam)
	column(SHADER_DOOR, -1.2)
	column(SHADER_A2C, 1.2)

func _process(_delta: float) -> bool:
	frames += 1
	if frames == 12:
		var img := root.get_texture().get_image()
		var abs_path := ProjectSettings.globalize_path(out_path)
		img.save_png(abs_path)
		print("saved ", abs_path, " ", img.get_size(), " driver=", RenderingServer.get_current_rendering_driver_name(), " method=", RenderingServer.get_current_rendering_method())
		return true
	return false
