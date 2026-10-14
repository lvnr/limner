// The brush grid: a small lattice of lines in the air around the drawing hand,
// lined up with the world, that fades out away from the hand.
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never, blend_mix;

// Where the hand is, in the lattice's own units (one cell is 1).
uniform vec3 center = vec3(0.0);
// Distance from the hand at which the lines have faded out, in cells.
uniform float reach = 2.5;
uniform vec4 line_color : source_color = vec4(0.0, 0.0, 0.0, 0.5);

varying vec3 v_position;

void vertex() {
	v_position = VERTEX;
}

void fragment() {
	float fade = 1.0 - smoothstep(reach * 0.25, reach, distance(v_position, center));
	ALBEDO = line_color.rgb;
	ALPHA = line_color.a * fade;
}
