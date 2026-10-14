// The keys of a panel in a headset: each button stands off its sheet as a block whose
// top shows the button as the panel draws it, and whose sides are the colour of the
// button's edge, shaded as if lit from above.
//
// One mesh serves every key of a sheet. Its own coordinates run from -0.5 to 0.5
// across the face and from 0 (on the sheet) to 1 (the top) in height; each key's
// transform scales it to the button's size and height. Each key's custom data is the
// button's rectangle in the picture, as fractions: left, top, right, bottom.
shader_type spatial;
// Both faces are drawn, so that the meshes need no particular winding.
render_mode unshaded, cull_disabled;

uniform sampler2D level0 : source_color, filter_linear, repeat_disable;
uniform sampler2D level1 : source_color, filter_linear, repeat_disable;
uniform sampler2D level2 : source_color, filter_linear, repeat_disable;
// Size of the full picture in its own pixels.
uniform vec2 pixels = vec2(352.0, 480.0);
// Leans toward the sharper copy by this many halvings, as the sheet does.
uniform float sharpen = 0.35;
// How bright the sides are against the edge of the top: the side facing up, and the
// side facing down.
uniform float side_lit = 0.78;
uniform float side_shaded = 0.48;

varying vec3 local;
varying vec3 local_normal;
varying flat vec4 rect;

void vertex() {
	local = VERTEX;
	local_normal = NORMAL;
	rect = INSTANCE_CUSTOM;
}

vec3 picture(vec2 uv) {
	vec2 at = uv * pixels;
	vec2 across = dFdx(at);
	vec2 down = dFdy(at);
	float shrink = 0.5 * log2(max(max(dot(across, across), dot(down, down)), 1e-8));
	float level = clamp(shrink - sharpen, 0.0, 2.0);
	if (level < 1.0) {
		return mix(texture(level0, uv).rgb, texture(level1, uv).rgb, level);
	}
	return mix(texture(level1, uv).rgb, texture(level2, uv).rgb, level - 1.0);
}

void fragment() {
	vec2 across = vec2(local.x + 0.5, 0.5 - local.y);
	if (local_normal.z > 0.5) {
		ALBEDO = picture(mix(rect.xy, rect.zw, across));
	} else {
		// The side takes the colour just inside the edge of the top: the button's
		// outline where it has one.
		vec2 inset = 1.5 / max(pixels * (rect.zw - rect.xy), vec2(1.0));
		vec2 edge = clamp(across, inset, 1.0 - inset);
		vec3 color = texture(level0, mix(rect.xy, rect.zw, edge)).rgb;
		float light = mix(side_shaded, side_lit, 0.5 + 0.5 * local_normal.y);
		ALBEDO = color * light;
	}
}
