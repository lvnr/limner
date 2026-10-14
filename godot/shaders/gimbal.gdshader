shader_type spatial;
// The handles of the gimbal: flat colour, seen through whatever is in front of them,
// since a handle that cannot be seen cannot be taken hold of.
render_mode unshaded, cull_disabled, depth_test_disabled, depth_draw_never, blend_mix;

uniform vec4 color : source_color = vec4(1.0);

void fragment() {
	ALBEDO = color.rgb;
	ALPHA = color.a;
}
