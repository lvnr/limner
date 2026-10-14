// The stand-in shapes drawn for the controllers. They carry no lighting of the
// scene's; they are shaded as if by a lamp on the viewer's forehead, which is enough
// to give a plain shape its roundness against any artwork.
shader_type spatial;
render_mode unshaded, cull_back, shadows_disabled;

uniform vec3 color : source_color = vec3(0.32, 0.34, 0.38);
// How dark the shape gets where it turns away from the viewer, as a share of its colour.
uniform float edge_shade = 0.45;

void fragment() {
	float facing = clamp(dot(normalize(NORMAL), normalize(VIEW)), 0.0, 1.0);
	ALBEDO = color * mix(edge_shade, 1.0, facing);
}
