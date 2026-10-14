shader_type spatial;
render_mode cull_disabled, diffuse_lambert_wrap, specular_disabled, ambient_light_disabled;

#define STROKE_LIT
#define STROKE_TWO_SIDED
#define STROKE_SEE_THROUGH
#include "res://shaders/stroke.gdshaderinc"
