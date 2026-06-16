// Hey Verse stylized cel shader — used by every toon material (via
// VerseAvatar.toon_mat). Phone-cheap: base pass only, no extra passes.
//  - soft two-band cel ramp with WARM light / COOL shadow tinting
//  - world-height albedo gradient (fake bounce light)
//  - rim highlight on the lit side + optional stylized specular dot
//  - optional wind sway in the vertex stage (grass/flowers/leaves)
shader_type spatial;

uniform vec4 albedo : source_color = vec4(1.0);
uniform float rim_strength : hint_range(0.0, 1.0) = 0.35;
uniform float spec_strength : hint_range(0.0, 1.0) = 0.0;
uniform float wind_strength = 0.0;
uniform float wind_height = 0.5;

const vec3 WARM = vec3(1.07, 1.0, 0.90);
const vec3 COOL = vec3(0.80, 0.87, 1.10);

void vertex() {
	if (wind_strength > 0.0) {
		vec3 wp = MODEL_MATRIX[3].xyz;
		float ph = wp.x * 1.7 + wp.z * 1.3;
		float w = clamp(VERTEX.y / max(wind_height, 0.001), 0.0, 1.0);
		float sway = sin(TIME * 2.2 + ph) + 0.5 * sin(TIME * 3.7 + ph * 1.4);
		VERTEX.x += sway * 0.06 * wind_strength * w;
		VERTEX.z += cos(TIME * 1.9 + ph) * 0.04 * wind_strength * w;
	}
}

void fragment() {
	vec3 wpos = (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
	float lift = clamp(wpos.y * 0.30, 0.0, 1.0);
	ALBEDO = albedo.rgb * mix(0.90, 1.06, lift);
	ROUGHNESS = 1.0;
	SPECULAR = 0.0;
}

void light() {
	float ndl = dot(NORMAL, LIGHT);
	float band = smoothstep(0.02, 0.34, ndl);
	vec3 lit = ALBEDO * WARM;
	vec3 shade = ALBEDO * COOL * 0.55;
	vec3 base = mix(shade, lit, band);
	float rim = pow(1.0 - clamp(dot(NORMAL, VIEW), 0.0, 1.0), 2.5)
		* rim_strength * clamp(ndl + 0.4, 0.0, 1.0);
	vec3 h = normalize(VIEW + LIGHT);
	float sp = smoothstep(0.86, 0.92, dot(NORMAL, h)) * spec_strength;
	DIFFUSE_LIGHT += base * ATTENUATION * LIGHT_COLOR / PI;
	DIFFUSE_LIGHT += (rim * 0.6 + sp) * LIGHT_COLOR * ATTENUATION / PI;
}
