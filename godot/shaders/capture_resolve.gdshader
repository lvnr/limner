// Turns the averaged passes of an offline render into the finished picture.
//
// Each output pixel is the mean of the block of supersampled pixels rendered for it.
// The input is linear light with colour multiplied by coverage. For a finished
// picture the colour is divided by coverage again (straight alpha, as PNG wants)
// and encoded as sRGB; a cube face that will be resampled is left as it is.
shader_type canvas_item;
render_mode blend_disabled, unshaded;

uniform sampler2D accumulated : filter_nearest;
// Rendered pixels per output pixel, along each axis.
uniform int supersample = 1;
// True for a finished picture, false for linear data to be resampled.
uniform bool finish = true;

vec3 linear_to_srgb(vec3 c) {
	return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}

void fragment() {
	vec2 texel = 1.0 / vec2(textureSize(accumulated, 0));
	vec4 sum = vec4(0.0);
	for (int y = 0; y < supersample; y++) {
		for (int x = 0; x < supersample; x++) {
			vec2 offset = vec2(float(x), float(y)) + 0.5 - 0.5 * float(supersample);
			sum += texture(accumulated, SCREEN_UV + offset * texel);
		}
	}
	vec4 value = sum / float(supersample * supersample);
	if (finish) {
		if (value.a > 0.0) {
			value.rgb /= value.a;
		}
		value.rgb = linear_to_srgb(clamp(value.rgb, 0.0, 1.0));
		value.a = clamp(value.a, 0.0, 1.0);
	}
	COLOR = value;
}
