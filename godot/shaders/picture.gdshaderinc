// Shared body of the picture shaders.
//
// The including shader sets its render modes and may define PICTURE_SEE_THROUGH (the
// picture has transparent parts or its layer is faded) and PICTURE_SPHERE (the
// picture surrounds the viewer: it is drawn on the inside of a sphere and looked up
// by direction).
//
// Like strokes, pictures are never blended: transparency is dithered against blue
// noise, so a picture writes depth and sits correctly among strokes without sorting.

instance uniform float layer_opacity = 1.0;
instance uniform float layer_seed = 0.0;

#ifdef PICTURE_SPHERE
uniform sampler2D picture : source_color, filter_linear, repeat_enable;
// How the picture wraps the sphere: 0 equirectangular; 1 equirectangular with the
// left eye in the top half and the right eye in the bottom half; 2 and 3 a cube
// unfolded as a cross, lying and standing; 4 and 5 a strip of six cube faces,
// standing and lying.
uniform int projection = 0;
varying vec3 v_direction;
#else
uniform sampler2D picture : source_color, filter_linear_mipmap, repeat_disable;
#endif

#ifdef PICTURE_SEE_THROUGH
uniform sampler2D blue_noise : filter_nearest, repeat_enable;
uniform int frame_index = 0;
#endif

#ifdef PICTURE_SPHERE
// Where each cube face sits in its picture, in face-sized cells. Faces are in the
// usual cube-map order: +X, -X, +Y, -Y, +Z (ahead), -Z (behind).
const vec2 CROSS_LYING[6] = vec2[](vec2(2.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 2.0), vec2(1.0, 1.0), vec2(3.0, 1.0));
const vec2 CROSS_STANDING[6] = vec2[](vec2(2.0, 1.0), vec2(0.0, 1.0), vec2(1.0, 0.0), vec2(1.0, 2.0), vec2(1.0, 1.0), vec2(1.0, 3.0));

vec2 equirect_uv(vec3 direction, float eye, vec2 size) {
	// The middle of the picture is straight ahead (-Z) and its top is up.
	vec2 uv = vec2(atan(direction.x, -direction.z) / TAU + 0.5, acos(clamp(direction.y, -1.0, 1.0)) / PI);
	if (projection == 1) {
		// Stay half a pixel inside the eye's own half so the other eye does not bleed in.
		float margin = 0.5 / size.y;
		uv.y = clamp(uv.y * 0.5, margin, 0.5 - margin) + eye * 0.5;
	}
	return uv;
}

vec2 cube_uv(vec3 direction, vec2 size) {
	// Cube maps are described looking along +Z; a picture's "ahead" is -Z.
	vec3 c = vec3(direction.x, direction.y, -direction.z);
	vec3 a = abs(c);
	int face;
	vec2 st;
	if (a.x >= a.y && a.x >= a.z) {
		face = c.x > 0.0 ? 0 : 1;
		st = vec2(c.x > 0.0 ? -c.z : c.z, -c.y) / a.x;
	} else if (a.y >= a.z) {
		face = c.y > 0.0 ? 2 : 3;
		st = vec2(c.x, c.y > 0.0 ? c.z : -c.z) / a.y;
	} else {
		face = c.z > 0.0 ? 4 : 5;
		st = vec2(c.z > 0.0 ? c.x : -c.x, -c.y) / a.z;
	}
	st = st * 0.5 + 0.5;
	vec2 grid;
	vec2 cell;
	if (projection == 2) {
		grid = vec2(4.0, 3.0);
		cell = CROSS_LYING[face];
	} else if (projection == 3) {
		grid = vec2(3.0, 4.0);
		cell = CROSS_STANDING[face];
		if (face == 5) {
			// In a standing cross the face behind hangs below the floor, upside down.
			st = 1.0 - st;
		}
	} else if (projection == 4) {
		grid = vec2(1.0, 6.0);
		cell = vec2(0.0, float(face));
	} else {
		grid = vec2(6.0, 1.0);
		cell = vec2(float(face), 0.0);
	}
	// Stay half a pixel inside the face so its neighbours in the picture do not bleed in.
	vec2 margin = 0.5 * grid / size;
	return (cell + clamp(st, margin, 1.0 - margin)) / grid;
}
#endif

void vertex() {
#ifdef PICTURE_SPHERE
	v_direction = VERTEX;
#endif
}

void fragment() {
#ifdef PICTURE_SPHERE
	vec3 direction = normalize(v_direction);
	vec2 size = vec2(textureSize(picture, 0));
	float eye = VIEW_INDEX == VIEW_RIGHT ? 1.0 : 0.0;
	vec2 uv = projection < 2 ? equirect_uv(direction, eye, size) : cube_uv(direction, size);
	vec4 texel = textureLod(picture, uv, 0.0);
#else
	vec4 texel = texture(picture, UV);
#endif
	ALBEDO = texel.rgb;
#ifdef PICTURE_SEE_THROUGH
	// The same moving blue-noise pattern as see-through strokes, one per layer.
	ivec2 seed = ivec2(int(layer_seed)) * ivec2(23, 41);
	ivec2 drift = ivec2(fract(vec2(0.7548776662, 0.5698402910) * float(frame_index)) * 64.0);
	float threshold = texelFetch(blue_noise, (ivec2(FRAGCOORD.xy) + seed + drift) & ivec2(63), 0).r * (255.0 / 256.0);
	if (texel.a * layer_opacity <= threshold) {
		discard;
	}
#endif
}
