// Shared body of the stroke shaders.
//
// The including shader sets its render modes and may define STROKE_SEE_THROUGH before
// including this file. Opaque strokes on fully opaque layers use the plain variant,
// which never discards and is the fast path on mobile GPUs.
//
// See-through strokes are never blended. A fragment is kept or discarded against a
// blue-noise threshold, so strokes write depth and the result does not depend on the
// order they are drawn in.
//
// Lit strokes define STROKE_LIT (and leave out the `unshaded` render mode): the
// scene's lights shade them, wrapped a little way round the form so that thin
// strokes do not fall into black. Surfaces seen from both sides also define
// STROKE_TWO_SIDED, which turns the normal towards the eye.

instance uniform float layer_opacity = 1.0;
instance uniform float layer_seed = 0.0;
// Tint colour and amount, for onion skins and highlights.
instance uniform vec4 layer_tint : source_color = vec4(1.0, 1.0, 1.0, 0.0);

varying vec3 v_color;

#ifdef STROKE_LIT
// Light that reaches a lit surface from everywhere, as a share of its own colour.
// A fixed soft value for now; it is to become a setting of the document.
uniform vec3 ambient_light = vec3(0.35);
// How far past the terminator a light still reaches: 0 is plain Lambert shading.
const float LIGHT_WRAP = 0.5;
#endif

#ifdef STROKE_SEE_THROUGH
uniform sampler2D blue_noise : filter_nearest, repeat_enable;
uniform int frame_index = 0;
varying float v_alpha;
varying flat float v_seed;
#endif

vec3 srgb_to_linear(vec3 c) {
	return mix(c / 12.92, pow((c + 0.055) / 1.055, vec3(2.4)), step(vec3(0.04045), c));
}

void vertex() {
	// Vertex colours are stored sRGB-encoded so eight bits per channel are enough.
	v_color = srgb_to_linear(COLOR.rgb);
#ifdef STROKE_SEE_THROUGH
	float alpha = COLOR.a;
	vec3 direction = CUSTOM0.rgb;
	if (dot(direction, direction) > 0.00001) {
		// A directional stroke fades as the view departs from the direction it was drawn from.
		vec3 drawn_from = normalize((MODELVIEW_MATRIX * vec4(direction * 2.0 - 1.0, 0.0)).xyz);
		vec3 seen_along = normalize((MODELVIEW_MATRIX * vec4(VERTEX, 1.0)).xyz);
		float facing = clamp(dot(drawn_from, seen_along), 0.0, 1.0);
		alpha *= facing * facing;
	}
	// Opaque points on a faded layer share the layer's pattern, so the layer fades
	// evenly. See-through points use their own stroke's pattern, so overlapping faint
	// strokes build up.
	v_seed = COLOR.a > 0.999 ? layer_seed : CUSTOM0.a * 255.0 + 37.0;
	v_alpha = alpha * layer_opacity;
#endif
}

void fragment() {
	ALBEDO = mix(v_color, layer_tint.rgb, layer_tint.a);
#ifdef STROKE_LIT
#ifdef STROKE_TWO_SIDED
	NORMAL = dot(NORMAL, VIEW) < 0.0 ? -NORMAL : NORMAL;
#endif
	// With the wrapped Lambert model, roughness is the amount of wrap.
	ROUGHNESS = LIGHT_WRAP;
	EMISSION = ALBEDO * ambient_light;
#endif
#ifdef STROKE_SEE_THROUGH
	// The pattern moves every frame along a low-discrepancy sequence and is shifted
	// per seed, so no two strokes and no two frames punch the same holes.
	ivec2 seed = ivec2(int(v_seed)) * ivec2(23, 41);
	ivec2 drift = ivec2(fract(vec2(0.7548776662, 0.5698402910) * float(frame_index)) * 64.0);
	float threshold = texelFetch(blue_noise, (ivec2(FRAGCOORD.xy) + seed + drift) & ivec2(63), 0).r * (255.0 / 256.0);
	if (v_alpha <= threshold) {
		discard;
	}
#endif
}
