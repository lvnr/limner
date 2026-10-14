shader_type spatial;
render_mode cull_back, diffuse_lambert_wrap, specular_disabled, ambient_light_disabled;

#define STROKE_LIT
#include "res://shaders/stroke.gdshaderinc"
