// A panel of the interface shown on a sheet in the room.
//
// A viewport's picture has no smaller copies of itself for the graphics card to use
// when the sheet is far away or seen at a slant, so the panel supplies two, each half
// the size of the one before, and this shader blends between the three by how many
// of the picture's pixels land on one pixel of the screen. Text then stays sharp up
// close and does not shimmer at a distance.
shader_type spatial;
render_mode unshaded, cull_disabled;

uniform sampler2D level0 : source_color, filter_linear, repeat_disable;
uniform sampler2D level1 : source_color, filter_linear, repeat_disable;
uniform sampler2D level2 : source_color, filter_linear, repeat_disable;
// Size of the full picture in its own pixels.
uniform vec2 pixels = vec2(352.0, 480.0);
// What the sheet shows from behind.
uniform vec3 back_color : source_color = vec3(0.1, 0.1, 0.11);
// Leans toward the sharper copy by this many halvings; text reads better slightly
// too sharp than slightly too soft.
uniform float sharpen = 0.35;

void fragment() {
	vec2 at = UV * pixels;
	vec2 across = dFdx(at);
	vec2 down = dFdy(at);
	float shrink = 0.5 * log2(max(max(dot(across, across), dot(down, down)), 1e-8));
	float level = clamp(shrink - sharpen, 0.0, 2.0);
	vec3 color;
	if (level < 1.0) {
		color = mix(texture(level0, UV).rgb, texture(level1, UV).rgb, level);
	} else {
		color = mix(texture(level1, UV).rgb, texture(level2, UV).rgb, level - 1.0);
	}
	ALBEDO = FRONT_FACING ? color : back_color;
}
