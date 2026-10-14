// The body behind a panel's face in a headset: a plain block in one colour, its sides
// shaded as if lit from above, so that the sheet reads as something with thickness.
shader_type spatial;
render_mode unshaded;

uniform vec3 color : source_color = vec3(0.8, 0.8, 0.78);
// How bright the side facing up and the side facing down are, and the back.
uniform float side_lit = 0.92;
uniform float side_shaded = 0.62;
uniform float back = 0.86;

varying vec3 local_normal;

void vertex() {
	local_normal = NORMAL;
}

void fragment() {
	float light = local_normal.z < -0.5 ? back : mix(side_shaded, side_lit, 0.5 + 0.5 * local_normal.y);
	ALBEDO = color * light;
}
