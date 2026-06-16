// Hey Verse stylized sky — gradient bands, sun disc + halo, drifting
// procedural clouds, stars at night. Replaces ProceduralSkyMaterial; the
// lighting presets drive the uniforms.
shader_type sky;

uniform vec3 top : source_color = vec3(0.28, 0.52, 0.80);
uniform vec3 horizon : source_color = vec3(0.65, 0.76, 0.72);
uniform vec3 ground : source_color = vec3(0.42, 0.58, 0.44);
uniform vec3 cloud_col : source_color = vec3(1.0, 1.0, 1.0);
uniform float cloud_amount : hint_range(0.0, 1.0) = 0.40;
uniform float stars : hint_range(0.0, 1.0) = 0.0;

float hash21(vec2 p) {
	p = fract(p * vec2(123.34, 456.21));
	p += dot(p, p + 45.32);
	return fract(p.x * p.y);
}

float vnoise(vec2 p) {
	vec2 i = floor(p);
	vec2 f = fract(p);
	f = f * f * (3.0 - 2.0 * f);
	float a = hash21(i);
	float b = hash21(i + vec2(1.0, 0.0));
	float c = hash21(i + vec2(0.0, 1.0));
	float d = hash21(i + vec2(1.0, 1.0));
	return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

float fbm(vec2 p) {
	float v = 0.0;
	float a = 0.5;
	for (int k = 0; k < 4; k++) {
		v += a * vnoise(p);
		p *= 2.1;
		a *= 0.5;
	}
	return v;
}

void sky() {
	float h = EYEDIR.y;
	vec3 col;
	if (h < 0.0) {
		col = mix(horizon, ground, clamp(-h * 3.0, 0.0, 1.0));
	} else {
		col = mix(horizon, top, pow(clamp(h, 0.0, 1.0), 0.6));
	}
	// sun disc + halo from the directional light
	if (LIGHT0_ENABLED) {
		float sund = max(dot(EYEDIR, LIGHT0_DIRECTION), 0.0);
		col += LIGHT0_COLOR * LIGHT0_ENERGY *
			(smoothstep(0.9995, 0.9999, sund) * 1.6
			+ pow(sund, 80.0) * 0.30
			+ pow(sund, 8.0) * 0.07);
	}
	// drifting clouds, projected onto a high plane
	if (h > 0.02) {
		vec2 cuv = EYEDIR.xz / (h + 0.15) * 0.7 + vec2(TIME * 0.006, TIME * 0.002);
		float c = fbm(cuv * 1.6);
		float m = smoothstep(1.0 - cloud_amount, 1.0 - cloud_amount + 0.35, c)
			* smoothstep(0.02, 0.18, h);
		vec3 cc = cloud_col * (0.85 + 0.30 * fbm(cuv * 3.1 + 7.0));
		col = mix(col, cc, m * 0.85);
	}
	// stars (night preset), gently twinkling
	if (stars > 0.01 && h > 0.05) {
		vec2 sp = EYEDIR.xz / (h + 0.2) * 18.0;
		float s = step(0.997, hash21(floor(sp))) * stars * smoothstep(0.05, 0.3, h);
		s *= 0.6 + 0.4 * sin(TIME * 2.0 + hash21(floor(sp) + 3.0) * 20.0);
		col += vec3(s) * 0.8;
	}
	COLOR = col;
}
