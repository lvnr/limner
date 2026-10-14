shader_type spatial;
// Drawn over selected strokes: a wash of one colour that fades in and out slowly.
render_mode unshaded, cull_disabled, depth_draw_never, blend_mix;

uniform vec4 color : source_color = vec4(0.25, 0.7, 1.0, 1.0);
// How strong the wash is at its faintest and at its fullest.
uniform float low = 0.2;
uniform float high = 0.6;
// Seconds for one fade in and out.
uniform float period = 1.6;

void vertex() {
	// The same shape as the stroke under it, so it is pulled a little toward the eye
	// to win over it. Pulling along the line of sight leaves it in the same place on
	// screen, and by the same share at any distance.
	vec3 eye = (inverse(MODEL_MATRIX) * vec4(CAMERA_POSITION_WORLD, 1.0)).xyz;
	VERTEX = mix(VERTEX, eye, 0.004);
}

void fragment() {
	ALBEDO = color.rgb;
	ALPHA = color.a * mix(low, high, 0.5 + 0.5 * sin(TIME * 6.2831853 / period));
}
