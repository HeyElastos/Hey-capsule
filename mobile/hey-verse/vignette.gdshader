// Subtle corner vignette — reads as color grading, costs nothing.
shader_type canvas_item;

void fragment() {
	vec2 c = UV - 0.5;
	float d = length(c) * 1.35;
	float v = smoothstep(0.52, 1.05, d) * 0.30;
	COLOR = vec4(0.02, 0.03, 0.06, v);
}
