// The floor grid: lines on the world's ground plane that fade with distance.
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never, blend_mix;

// Distance between lines, in the plane's own units.
uniform float spacing = 1.0;
// Distance at which the grid has faded out, in the plane's own units.
uniform float reach = 20.0;
uniform vec4 line_color : source_color = vec4(0.0, 0.0, 0.0, 0.35);

varying vec3 v_position;

float lines(vec2 position, float step_size, float width) {
	vec2 cell = position / step_size;
	vec2 cells_per_pixel = fwidth(cell);
	vec2 distance_to_line = abs(fract(cell - 0.5) - 0.5) / cells_per_pixel;
	float line = 1.0 - clamp(min(distance_to_line.x, distance_to_line.y) - width + 1.0, 0.0, 1.0);
	// Where lines come closer than a few pixels they would merge into a grey band; let them go.
	return line * (1.0 - smoothstep(0.15, 0.5, max(cells_per_pixel.x, cells_per_pixel.y)));
}

void vertex() {
	v_position = VERTEX;
}

void fragment() {
	float fine = lines(v_position.xz, spacing, 1.0);
	float coarse = lines(v_position.xz, spacing * 10.0, 1.6);
	float fade = 1.0 - smoothstep(reach * 0.35, reach, length(v_position.xz));
	ALBEDO = line_color.rgb;
	ALPHA = max(fine * 0.45, coarse) * fade * line_color.a;
}
