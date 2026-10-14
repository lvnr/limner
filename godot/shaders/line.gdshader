// Thin lines: gizmos, bounding boxes and the wireframe drawn over a model.
shader_type spatial;
render_mode unshaded, cull_disabled, skip_vertex_transform;

instance uniform vec4 line_color : source_color = vec4(1.0, 1.0, 1.0, 1.0);
// How much of the line is there, for lines that fade with their layer.
instance uniform float fade = 1.0;
// A wireframe ignores `line_color` and takes black or white, whichever stands out
// from the colour of the surface under it.
uniform bool against_vertex_color = false;

varying vec3 v_color;

vec3 srgb_to_linear(vec3 c) {
	return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}

void vertex() {
	// Pulled a little towards the eye, so a line lying on a surface is not buried in it.
	VERTEX = (MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz * 0.998;
	if (against_vertex_color) {
		float luminance = dot(srgb_to_linear(COLOR.rgb), vec3(0.2126, 0.7152, 0.0722));
		v_color = vec3(luminance > 0.18 ? 0.0 : 1.0);
	} else {
		v_color = line_color.rgb;
	}
}

void fragment() {
	ALBEDO = v_color;
	if (fade < 0.999) {
		// Interleaved gradient noise: enough to thin a one-pixel line evenly.
		float noise = fract(52.9829189 * fract(dot(FRAGCOORD.xy, vec2(0.06711056, 0.00583715))));
		if (fade <= noise) {
			discard;
		}
	}
}
