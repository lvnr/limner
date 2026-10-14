// Averages the passes of an offline render.
//
// Each pass is one picture of the same view with the dither pattern of see-through
// strokes in a new place. This writes the running mean of the passes so far, in
// linear light and with float precision, reading the mean of the earlier passes from
// a second target (the two take turns).
shader_type canvas_item;
render_mode blend_disabled, unshaded;

// The pass just rendered.
uniform sampler2D source : filter_nearest;
// The mean of the passes before it.
uniform sampler2D previous : filter_nearest;
// One over the number of passes so far, this one included.
uniform float weight = 1.0;
// True when the background is to be left out of the picture.
uniform bool cut_out = false;
// The background colour the pass was cleared to, in linear light.
uniform vec3 backdrop = vec3(0.0);

void fragment() {
	vec4 fresh = texture(source, SCREEN_UV);
	if (cut_out) {
		// The pass holds the artwork over the background, with coverage as alpha.
		// Taking the background's share away leaves colour multiplied by coverage,
		// which is what can be averaged and filtered without fringes.
		fresh.rgb = max(fresh.rgb - backdrop * (1.0 - fresh.a), vec3(0.0));
	}
	// The first pass must not read the other target: nothing has been drawn there
	// yet, and whatever a new target happens to hold would stay in the mean.
	COLOR = weight >= 1.0 ? fresh : mix(texture(previous, SCREEN_UV), fresh, weight);
}
