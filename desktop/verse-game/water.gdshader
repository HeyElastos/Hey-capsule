// Cartoon pond water v3 — a living mirror: view-angle sky reflection
// (fresnel), two flowing ripple layers, wave-crest glints, edge foam, sun
// streaks. Fragment-only; runs on anything. `sky_col` is fed by the
// lighting preset so the mirror matches the sky.
shader_type spatial;
render_mode specular_disabled;

uniform vec4 col_deep : source_color = vec4(0.22, 0.50, 0.78, 1.0);
uniform vec4 col_light : source_color = vec4(0.55, 0.84, 0.96, 1.0);
uniform vec3 sky_col : source_color = vec3(0.65, 0.78, 0.85);

void fragment() {
	vec2 uv = UV * 7.0;
	// two ripple fields flowing in different directions = constant movement
	vec2 f1 = uv * 1.30 + vec2(TIME * 0.35, TIME * 0.21);
	vec2 f2 = uv * 1.85 - vec2(TIME * 0.27, TIME * 0.40);
	float w1 = sin(f1.x * 6.28) + sin((f1.x + f1.y) * 4.4);
	float w2 = sin(f2.y * 5.70) + sin((f2.y - f2.x) * 3.7);
	float w = (w1 + w2) * 0.25;            // -1 .. 1
	float r = distance(UV, vec2(0.5)) * 2.0;
	vec3 col = mix(col_deep.rgb * 0.85, col_light.rgb, clamp(0.35 + 0.5 * w + r * 0.25, 0.0, 1.0));
	// the MIRROR: sky reflected in at glancing angles, shimmering with the
	// ripples (brighter sky band wobbles across the surface)
	float fres = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 2.0);
	float mir = clamp(0.22 + fres * 0.85 + w * 0.10, 0.0, 0.92);
	col = mix(col, sky_col * (1.04 + w * 0.10), mir);
	// wave-crest glints
	col += smoothstep(0.90, 1.0, w) * 0.45;
	// soft foam ring hugging the bank, gently wobbling
	float foam = smoothstep(0.84, 0.95, r + sin(TIME * 1.3 + r * 28.0 + uv.x) * 0.018);
	foam *= 1.0 - smoothstep(0.985, 1.0, r);
	col = mix(col, vec3(0.95, 0.98, 1.0), foam * 0.8);
	// sun-glint streaks drifting across
	float g = sin(uv.x * 3.1 - TIME * 0.8) * sin(uv.y * 2.7 + TIME * 0.6);
	col += smoothstep(0.975, 1.0, g) * 0.25;
	ALBEDO = col;
	ROUGHNESS = 0.15;
}
