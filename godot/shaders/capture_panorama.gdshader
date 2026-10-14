// Unrolls six cube faces into an equirectangular picture: longitude across,
// latitude down, the direction the capture faces in the middle.
//
// The faces hold linear light with colour multiplied by coverage, and are rendered a
// little wider than 90 degrees so that filtering never reads past an edge.
shader_type canvas_item;
render_mode blend_disabled, unshaded;

uniform sampler2D face_front : filter_linear, repeat_disable;
uniform sampler2D face_right : filter_linear, repeat_disable;
uniform sampler2D face_back : filter_linear, repeat_disable;
uniform sampler2D face_left : filter_linear, repeat_disable;
uniform sampler2D face_up : filter_linear, repeat_disable;
uniform sampler2D face_down : filter_linear, repeat_disable;
// Tangent of half the angle a face covers; 1.0 would be exactly 90 degrees.
uniform float reach = 1.0;
// The part of the whole picture this draw covers: its top-left corner and its size, 0 to 1.
uniform vec4 region = vec4(0.0, 0.0, 1.0, 1.0);
// Size of the whole picture in pixels.
uniform vec2 picture_size = vec2(2048.0, 1024.0);
// Samples per output pixel, along each axis.
uniform int taps = 1;

vec3 linear_to_srgb(vec3 c) {
	return mix(c * 12.92, 1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, step(vec3(0.0031308), c));
}

// The colour seen along `d`, given in the capture's own axes: -Z ahead, +Y up, +X right.
vec4 look(vec3 d) {
	vec3 a = abs(d);
	// Across and up on the chosen face, as tangents of the angle from its centre.
	vec2 at;
	if (a.z >= a.x && a.z >= a.y) {
		if (d.z < 0.0) {
			at = vec2(d.x, d.y) / a.z;
			return textureLod(face_front, 0.5 + 0.5 * vec2(at.x, -at.y) / reach, 0.0);
		}
		at = vec2(-d.x, d.y) / a.z;
		return textureLod(face_back, 0.5 + 0.5 * vec2(at.x, -at.y) / reach, 0.0);
	}
	if (a.x >= a.y) {
		if (d.x > 0.0) {
			at = vec2(d.z, d.y) / a.x;
			return textureLod(face_right, 0.5 + 0.5 * vec2(at.x, -at.y) / reach, 0.0);
		}
		at = vec2(-d.z, d.y) / a.x;
		return textureLod(face_left, 0.5 + 0.5 * vec2(at.x, -at.y) / reach, 0.0);
	}
	if (d.y > 0.0) {
		at = vec2(d.x, d.z) / a.y;
		return textureLod(face_up, 0.5 + 0.5 * vec2(at.x, -at.y) / reach, 0.0);
	}
	at = vec2(d.x, -d.z) / a.y;
	return textureLod(face_down, 0.5 + 0.5 * vec2(at.x, -at.y) / reach, 0.0);
}

void fragment() {
	vec4 sum = vec4(0.0);
	for (int y = 0; y < taps; y++) {
		for (int x = 0; x < taps; x++) {
			vec2 offset = (vec2(float(x), float(y)) + 0.5) / float(taps) - 0.5;
			vec2 uv = region.xy + SCREEN_UV * region.zw + offset / picture_size;
			float longitude = (uv.x - 0.5) * TAU;
			float latitude = (0.5 - uv.y) * PI;
			sum += look(vec3(sin(longitude) * cos(latitude), sin(latitude), -cos(longitude) * cos(latitude)));
		}
	}
	vec4 value = sum / float(taps * taps);
	if (value.a > 0.0) {
		value.rgb /= value.a;
	}
	COLOR = vec4(linear_to_srgb(clamp(value.rgb, 0.0, 1.0)), clamp(value.a, 0.0, 1.0));
}
