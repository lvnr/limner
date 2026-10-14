// The tool preview at the drawing hand: a soft bubble that is clearest at its rim,
// so the artwork inside it stays visible.
shader_type spatial;
render_mode unshaded, cull_back, depth_draw_never, blend_mix, shadows_disabled;

uniform vec4 fill : source_color = vec4(1.0, 1.0, 1.0, 0.08);
uniform vec4 rim : source_color = vec4(0.0, 0.0, 0.0, 0.7);
uniform float rim_power = 3.0;

void fragment() {
	float edge = pow(1.0 - clamp(dot(normalize(NORMAL), normalize(VIEW)), 0.0, 1.0), rim_power);
	ALBEDO = mix(fill.rgb, rim.rgb, edge);
	ALPHA = clamp(mix(fill.a, rim.a, edge), 0.0, 1.0);
}
