// Inverted-hull toon outline — used as next_pass on toon materials.
// One extra cheap draw per mesh; instantly makes the art read as "designed".
shader_type spatial;
render_mode unshaded, cull_front;

uniform float thickness : hint_range(0.0, 0.1) = 0.018;
uniform vec4 line_color : source_color = vec4(0.07, 0.09, 0.13, 1.0);

void vertex() {
	VERTEX += NORMAL * thickness;
}

void fragment() {
	ALBEDO = line_color.rgb;
}
