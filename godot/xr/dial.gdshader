// The disc that rides on the hand holding the palette: a see-through grey disc with
// a dark ring in it, drawn on a square whose corners are left clear.
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never;

uniform vec4 disc_color : source_color = vec4(0.5, 0.5, 0.5, 0.45);
uniform vec4 ring_color : source_color = vec4(0.06, 0.06, 0.06, 1.0);
// Where the ring starts and ends, as fractions of the disc's radius.
uniform float ring_inner = 0.48;
uniform float ring_outer = 0.66;

void fragment() {
	float r = length(UV * 2.0 - 1.0);
	float soft = max(fwidth(r), 1e-4);
	float disc = 1.0 - smoothstep(1.0 - soft, 1.0, r);
	float ring = smoothstep(ring_inner - soft, ring_inner, r) * (1.0 - smoothstep(ring_outer - soft, ring_outer, r));
	vec4 color = mix(disc_color, ring_color, ring);
	ALBEDO = color.rgb;
	ALPHA = color.a * disc;
}
