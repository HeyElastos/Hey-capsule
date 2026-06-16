class_name VerseCatalogWallart
extends RefCounted
## Hey Verse — WALL ART showroom catalog (PREMIUM, sellable NFTs).
##
## A set of cute, toon-styled wall pieces built from pure procedural primitives
## (no .glb, no preloaded art). Every item is a `static func build_<id>()` that
## returns ONE self-contained Node3D, CENTERED on the origin and lying in the
## XY plane, facing +Z (the wall is behind, at -Z). Thin in Z so it reads as
## something that "hangs". Pieces are roughly 0.6–1.1 wide to match the ~1.4-unit
## chibi-robot avatar.
##
## The caller hangs these on a wall (see home.gd PAINT_SLOTS / _add_painting_node):
## position the returned node at the slot and rotate it to face into the room.
##
## QUALITY BAR: each item is a RICH composite of many primitives (~20–55 parts)
## with hero details, cohesive vibrant palettes, real metals (gold/brass/chrome),
## glass + emission, and a RARITY tier that is readable at a glance — higher
## rarity = more gold trim, gemstones, glow, ornament. These are minted + sold.
##
## Self-contained: this module re-declares its own tiny material + mesh helpers
## so it parses and runs standalone, with no dependency on avatar.gd / home.gd.
## It does, however, reuse the project's toon.gdshader + outline.gdshader for the
## exact cel look (loaded lazily; falls back to a plain StandardMaterial3D if the
## shaders are ever missing, so it never crashes).

const TOON_SHADER_PATH := "res://toon.gdshader"
const OUTLINE_SHADER_PATH := "res://outline.gdshader"

static var _toon_shader: Shader
static var _outline_mat: ShaderMaterial


# ───────────────────────────────────────────────────────── local helpers ────
# (re-declared here so the module is self-contained)

## One stylized cel material — the whole cartoon look comes from here. Mirrors
## VerseAvatar.toon_mat: soft two-band ramp + inverted-hull outline as next_pass.
## Falls back to a flat StandardMaterial3D if the shaders aren't present.
static func _toon(c: Color, rim := 0.35, outline := true, spec := 0.0) -> Material:
	if _toon_shader == null and ResourceLoader.exists(TOON_SHADER_PATH):
		_toon_shader = load(TOON_SHADER_PATH)
	if _toon_shader == null:
		var sm := StandardMaterial3D.new()
		sm.albedo_color = c
		sm.roughness = 0.9
		return sm
	var m := ShaderMaterial.new()
	m.shader = _toon_shader
	m.set_shader_parameter("albedo", c)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", spec)
	m.set_shader_parameter("wind_strength", 0.0)
	m.set_shader_parameter("wind_height", 0.5)
	if outline:
		if _outline_mat == null and ResourceLoader.exists(OUTLINE_SHADER_PATH):
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = load(OUTLINE_SHADER_PATH)
		if _outline_mat != null:
			m.next_pass = _outline_mat
	return m


## A bright unshaded emissive material (neon, LEDs, glow, gems). emission ok per brief.
static func _glow(c: Color, energy := 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


## A real metal — brass / gold / chrome — for premium trim. Toon look kept via
## a high-spec cel material; the metallic/rough numbers ride along for any
## fallback StandardMaterial path.
static func _metal(c: Color, rough := 0.25, spec := 0.7) -> Material:
	var m := _toon(c, 0.45, true, spec)
	if m is StandardMaterial3D:
		(m as StandardMaterial3D).metallic = 0.9
		(m as StandardMaterial3D).roughness = rough
	return m


## A translucent shell — glass / acrylic / mirror sheen. Shadow casting is left
## to the caller via _no_shadow() on the returned mesh so glass never punches a
## hole in the glow behind it.
static func _glass(c: Color, alpha := 0.35, emit := 0.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.roughness = 0.08
	m.metallic = 0.6
	m.metallic_specular = 0.9
	if emit > 0.0:
		m.emission_enabled = true
		m.emission = c
		m.emission_energy_multiplier = emit
	return m


## Turn off shadow casting on a mesh (for glass / glow streaks that shouldn't
## cast hard shadows onto the piece). Returns the mesh so it chains.
static func _no_shadow(mi: MeshInstance3D) -> MeshInstance3D:
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	return mi


static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot_z := 0.0) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	mi.rotation.z = rot_z
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, seg := 18) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi


## A thin disc lying in the XY plane (a hoop face / clock face / record) — a
## cylinder rotated so its flat faces look down +Z.
static func _disc(parent: Node3D, r: float, depth: float, mat: Material, pos: Vector3, seg := 28) -> MeshInstance3D:
	var mi := _cyl(parent, r, r, depth, mat, pos, seg)
	mi.rotation.x = PI / 2.0
	return mi


static func _sphere(parent: Node3D, r: float, mat: Material, pos: Vector3, s := Vector3.ONE, seg := 16, rings := 8) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = seg
	sm.rings = rings
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = s
	parent.add_child(mi)
	return mi


## A torus standing up in the XY plane (a hoop / ring of trim) facing +Z.
static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, seg := 30) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = seg
	tm.ring_segments = 12
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation.x = PI / 2.0
	parent.add_child(mi)
	return mi


## A faceted gemstone (squashed low-poly sphere) reading as a cut jewel. The
## rarity-marker of choice — pair with _glow for the premium tiers.
static func _gem(parent: Node3D, r: float, mat: Material, pos: Vector3) -> MeshInstance3D:
	var mi := _sphere(parent, r, mat, pos, Vector3(1.0, 1.25, 0.6), 6, 3)
	mi.rotation.z = PI / 4.0
	return mi


## A faceted "brilliant" gem with a bright glow core behind it — the legendary
## sparkle: a colored cut stone with a tiny white-hot highlight that makes it
## read as cut glass catching light. Returns the stone mesh.
static func _jewel(parent: Node3D, r: float, c: Color, pos: Vector3, energy := 1.5) -> MeshInstance3D:
	var stone := _gem(parent, r, _glow(c, energy), pos)
	_no_shadow(_sphere(parent, r * 0.42, _glow(Color(1, 1, 1), 1.6),
		Vector3(pos.x - r * 0.25, pos.y + r * 0.35, pos.z + r * 0.5), Vector3(1, 1, 0.4), 6, 3))
	return stone


## A ring of little beads around a circle (classic gilt-frame "pearl" detailing).
static func _bead_ring(parent: Node3D, radius: float, bead_r: float, count: int, mat: Material, z: float) -> void:
	for k in count:
		var ang := TAU * float(k) / float(count)
		_sphere(parent, bead_r, mat, Vector3(sin(ang) * radius, cos(ang) * radius, z), Vector3.ONE, 6, 3)


## A short ornate acanthus leaf-curl (a little scroll of stacked beads) used to
## dress crowns / crests on the premium pieces. Grows from `base` along +/-X.
static func _scroll(parent: Node3D, base: Vector3, dir: float, mat: Material, scale := 1.0) -> void:
	var n := 4
	var px := base.x
	var py := base.y
	for i in n:
		var t := float(i) / float(n - 1)
		var r := lerpf(0.02, 0.008, t) * scale
		_sphere(parent, r, mat, Vector3(px, py, base.z), Vector3.ONE, 6, 3)
		px += dir * (0.022 - t * 0.006) * scale
		py += (0.018 - t * 0.014) * scale


## A simple frame border (four bars) around a w×h panel, centered, facing +Z.
## thickness = bar width, depth = how far it stands off the wall.
static func _frame(parent: Node3D, w: float, h: float, thick: float, depth: float, mat: Material, z := 0.0) -> void:
	var hw := w * 0.5
	var hh := h * 0.5
	_box(parent, Vector3(w + thick * 2.0, thick, depth), mat, Vector3(0, hh + thick * 0.5, z))   # top
	_box(parent, Vector3(w + thick * 2.0, thick, depth), mat, Vector3(0, -hh - thick * 0.5, z))  # bottom
	_box(parent, Vector3(thick, h, depth), mat, Vector3(-hw - thick * 0.5, 0, z))                # left
	_box(parent, Vector3(thick, h, depth), mat, Vector3(hw + thick * 0.5, 0, z))                 # right


## Decorative corner blocks — little raised studs that turn a plain frame ornate.
static func _frame_corners(parent: Node3D, w: float, h: float, thick: float, mat: Material, z: float) -> void:
	var hw := w * 0.5 + thick * 0.5
	var hh := h * 0.5 + thick * 0.5
	for sx in [-1.0, 1.0]:
		for sy in [-1.0, 1.0]:
			_box(parent, Vector3(thick * 1.5, thick * 1.5, 0.05), mat, Vector3(sx * hw, sy * hh, z))


## A brass hanging nub at the top so each piece reads as "mounted" to the wall.
static func _hook(parent: Node3D, top_y: float) -> void:
	var brass := _glow(Color(0.86, 0.66, 0.28), 0.25)
	_sphere(parent, 0.03, brass, Vector3(0, top_y + 0.03, -0.01), Vector3(1, 1, 0.6))


## A soft contact-light so glowing pieces tint the wall behind them at dusk/night.
static func _wall_light(parent: Node3D, color: Color, energy: float, rng: float, z := 0.18) -> void:
	var o := OmniLight3D.new()
	o.light_color = color
	o.light_energy = energy
	o.omni_range = rng
	o.shadow_enabled = false
	o.position = Vector3(0, 0, z)
	parent.add_child(o)


# ════════════════════════════════════════════════════════════ the catalog ════

## ORNATE FRAMED PAINTING — a museum-grade gilt masterpiece: a carved gilt frame
## with stepped bands, a pearl bead course, gem-set corner rosettes, an acanthus
## crest, and a deep layered toon sunset-over-hills canvas (graded sky, glowing
## sun + halo, drifting birds, layered hills, a lone tree) lit by a brass museum
## picture light. Rarity: EPIC.
static func build_ornate_painting() -> Node3D:
	var root := Node3D.new()
	var w := 0.74
	var h := 0.96
	var gold := _metal(Color(0.97, 0.80, 0.36), 0.18, 0.8)
	var gold_dk := _metal(Color(0.70, 0.52, 0.20), 0.3, 0.5)
	var gold_lt := _metal(Color(1.0, 0.92, 0.6), 0.14, 0.85)
	# outer carved gilt frame: three stepped bands = carved depth
	_frame(root, w + 0.12, h + 0.12, 0.06, 0.10, gold_dk, -0.03)
	_frame(root, w + 0.06, h + 0.06, 0.08, 0.08, gold, -0.015)
	_frame(root, w, h, 0.05, 0.06, gold_lt, -0.005)
	# a pearl bead course running just inside the gold (the gilt "tell")
	var hw := w * 0.5 + 0.02
	var hh := h * 0.5 + 0.02
	for sx in [-1.0, 1.0]:
		for k in 11:
			var ty := -hh + 2.0 * hh * float(k) / 10.0
			_sphere(root, 0.009, gold_lt, Vector3(sx * hw, ty, 0.02), Vector3.ONE, 6, 3)
	for sy in [-1.0, 1.0]:
		for k in 9:
			var tx := -hw + 2.0 * hw * float(k) / 8.0
			_sphere(root, 0.009, gold_lt, Vector3(tx, sy * hh, 0.02), Vector3.ONE, 6, 3)
	# inner dark liner that lifts the canvas off the gold
	_frame(root, w - 0.02, h - 0.02, 0.02, 0.05, _toon(Color(0.18, 0.13, 0.08), 0.15), 0.0)
	# corner rosettes + a faceted ruby in each (the ornate tells)
	_frame_corners(root, w + 0.12, h + 0.12, 0.07, gold, -0.03)
	for sx in [-1.0, 1.0]:
		for sy in [-1.0, 1.0]:
			_disc(root, 0.03, 0.03, gold_lt, Vector3(sx * (w * 0.5 + 0.09), sy * (h * 0.5 + 0.09), 0.01), 10)
			_jewel(root, 0.022, Color(0.96, 0.26, 0.34), Vector3(sx * (w * 0.5 + 0.09), sy * (h * 0.5 + 0.09), 0.03), 1.3)
	# acanthus crest over the top center with a crowning sapphire
	_scroll(root, Vector3(-0.02, h * 0.5 + 0.07, 0.02), -1.0, gold_lt, 1.2)
	_scroll(root, Vector3(0.02, h * 0.5 + 0.07, 0.02), 1.0, gold_lt, 1.2)
	_disc(root, 0.028, 0.03, gold, Vector3(0, h * 0.5 + 0.07, 0.02), 12)
	_jewel(root, 0.026, Color(0.34, 0.6, 1.0), Vector3(0, h * 0.5 + 0.085, 0.04), 1.4)
	# ── the painted canvas (a deep layered toon landscape) ──
	_box(root, Vector3(w, h, 0.03), _toon(Color(0.99, 0.84, 0.58), 0.08, false), Vector3(0, 0, 0.0))
	var z := 0.025
	# graded sky bands (more layers = more depth)
	_box(root, Vector3(w, 0.22, 0.012), _toon(Color(0.38, 0.40, 0.78), 0.06, false), Vector3(0, 0.40, z))
	_box(root, Vector3(w, 0.16, 0.012), _toon(Color(0.56, 0.52, 0.84), 0.06, false), Vector3(0, 0.22, z + 0.001))
	_box(root, Vector3(w, 0.20, 0.012), _toon(Color(0.98, 0.58, 0.46), 0.06, false), Vector3(0, 0.06, z + 0.002))
	_box(root, Vector3(w, 0.16, 0.012), _toon(Color(1.0, 0.82, 0.52), 0.06, false), Vector3(0, -0.10, z + 0.004))
	# soft toon clouds (overlapping pale lozenges)
	for cp in [Vector2(-0.22, 0.30), Vector2(-0.16, 0.30), Vector2(0.24, 0.40), Vector2(0.30, 0.40)]:
		_no_shadow(_sphere(root, 0.05, _toon(Color(1.0, 0.93, 0.86), 0.05, false), Vector3(cp.x, cp.y, z + 0.005), Vector3(1.5, 0.7, 0.2), 8, 4))
	# the glowing sun + double halo
	_no_shadow(_disc(root, 0.18, 0.02, _glow(Color(1.0, 0.84, 0.38), 0.35), Vector3(0.06, 0.14, z + 0.005), 24))
	_no_shadow(_disc(root, 0.115, 0.02, _glow(Color(1.0, 0.92, 0.58), 1.2), Vector3(0.06, 0.14, z + 0.012), 22))
	_no_shadow(_disc(root, 0.08, 0.02, _glow(Color(1.0, 0.98, 0.85), 1.6), Vector3(0.06, 0.14, z + 0.018), 20))
	# a small flock of birds (tiny tilted bars)
	for bp in [Vector2(-0.2, 0.3), Vector2(-0.12, 0.34), Vector2(-0.26, 0.27)]:
		_box(root, Vector3(0.05, 0.01, 0.008), _toon(Color(0.25, 0.2, 0.3), 0.05, false), Vector3(bp.x, bp.y, z + 0.006), 0.4)
		_box(root, Vector3(0.05, 0.01, 0.008), _toon(Color(0.25, 0.2, 0.3), 0.05, false), Vector3(bp.x + 0.04, bp.y, z + 0.006), -0.4)
	# a mirror-still lake band catching the sun
	_box(root, Vector3(w, 0.12, 0.012), _toon(Color(0.62, 0.74, 0.86), 0.06, false), Vector3(0, -0.24, z + 0.006))
	_no_shadow(_box(root, Vector3(0.1, 0.1, 0.008), _glow(Color(1.0, 0.9, 0.6), 0.6), Vector3(0.06, -0.24, z + 0.01)))
	# rolling layered hills + a lone tree
	_disc(root, 0.4, 0.02, _toon(Color(0.30, 0.54, 0.40), 0.06, false), Vector3(-0.28, -0.58, z + 0.012), 26)
	_disc(root, 0.36, 0.02, _toon(Color(0.40, 0.64, 0.44), 0.06, false), Vector3(0.30, -0.62, z + 0.014), 26)
	_disc(root, 0.30, 0.02, _toon(Color(0.5, 0.74, 0.5), 0.06, false), Vector3(-0.02, -0.68, z + 0.016), 26)
	_box(root, Vector3(0.02, 0.1, 0.01), _toon(Color(0.32, 0.22, 0.16), 0.06, false), Vector3(0.16, -0.38, z + 0.018))
	_sphere(root, 0.06, _toon(Color(0.36, 0.6, 0.4), 0.08, false), Vector3(0.16, -0.30, z + 0.018), Vector3(1, 1, 0.4), 10, 5)
	# brass museum picture light arching over the top
	_cyl(root, 0.012, 0.012, 0.22, gold, Vector3(0, h * 0.5 + 0.14, 0.04)).rotation.z = PI / 2.0
	_cyl(root, 0.018, 0.026, 0.12, gold, Vector3(0, h * 0.5 + 0.18, 0.12)).rotation.x = -1.2
	_no_shadow(_sphere(root, 0.05, _glow(Color(1.0, 0.92, 0.72), 1.2), Vector3(0, h * 0.5 + 0.15, 0.18), Vector3(1.4, 1, 0.6)))
	_wall_light(root, Color(1.0, 0.88, 0.62), 0.7, 1.7, 0.2)
	_hook(root, h * 0.5 + 0.16)
	return root


## NEON WALL ART — a chunky glowing "HEY" wordmark in real glass-tube style
## (tubes with rounded end-caps + dim glass cores) plus a heart, a buzz underline
## and a "flickering off" segment, on a dark matte board with a lit edge channel,
## chrome-capped mounting bolts and a sign transformer hum-box. Casts colored
## light on the wall. Rarity: RARE.
static func build_neon() -> Node3D:
	var root := Node3D.new()
	var w := 0.96
	var h := 0.66
	# dim matte mounting board with a thin lit edge channel
	_box(root, Vector3(w, h, 0.04), _toon(Color(0.07, 0.08, 0.12), 0.1, false), Vector3.ZERO)
	_no_shadow(_box(root, Vector3(w - 0.06, h - 0.06, 0.012), _toon(Color(0.04, 0.05, 0.08), 0.08, false), Vector3(0, 0, 0.02)))
	_frame(root, w - 0.02, h - 0.02, 0.012, 0.05, _glow(Color(0.4, 0.85, 1.0), 0.8), 0.01)
	# corner mounting bolts (dark studs with a chrome cap)
	for sx in [-1.0, 1.0]:
		for sy in [-1.0, 1.0]:
			_sphere(root, 0.02, _metal(Color(0.7, 0.74, 0.8), 0.2, 0.8), Vector3(sx * (w * 0.5 - 0.04), sy * (h * 0.5 - 0.04), 0.03))
	var z := 0.05
	var pink := _glow(Color(1.0, 0.32, 0.6), 2.6)
	var pink_dim := _glow(Color(0.45, 0.12, 0.26), 0.5)
	var cyan := _glow(Color(0.42, 0.92, 1.0), 2.4)
	var cyan_dim := _glow(Color(0.14, 0.34, 0.42), 0.5)
	var amber := _glow(Color(1.0, 0.74, 0.3), 2.4)
	# helper: a glass-tube segment = a bright bar with a faint dim glass core
	# behind it + rounded end-cap dots, so it reads as a real bent neon tube.
	var tube := func(p: Vector3, size: Vector3, lit: Material, dim: Material, rz := 0.0) -> void:
		_no_shadow(_box(root, size * Vector3(1.5, 1.5, 0.6), dim, p + Vector3(0, 0, -0.012), rz))  # glass core
		var bar := _box(root, size, lit, p, rz)
		_no_shadow(bar)
		var caps_dir := Vector3(sin(rz), cos(rz), 0.0) * (size.y * 0.5)
		_no_shadow(_sphere(root, size.x * 0.7, lit, p + caps_dir, Vector3.ONE, 6, 3))
		_no_shadow(_sphere(root, size.x * 0.7, lit, p - caps_dir, Vector3.ONE, 6, 3))
	# ── the wordmark: a chunky neon "H E Y" out of glass-tube segments ──
	# H
	var hx := -0.34
	tube.call(Vector3(hx - 0.06, 0.06, z), Vector3(0.04, 0.26, 0.05), pink, pink_dim)
	tube.call(Vector3(hx + 0.06, 0.06, z), Vector3(0.04, 0.26, 0.05), pink, pink_dim)
	tube.call(Vector3(hx, 0.06, z), Vector3(0.04, 0.12, 0.05), pink, pink_dim, PI / 2.0)
	# E
	var ex := -0.02
	tube.call(Vector3(ex - 0.06, 0.06, z), Vector3(0.04, 0.26, 0.05), cyan, cyan_dim)
	tube.call(Vector3(ex, 0.18, z), Vector3(0.04, 0.13, 0.05), cyan, cyan_dim, PI / 2.0)
	tube.call(Vector3(ex - 0.005, 0.06, z), Vector3(0.04, 0.12, 0.05), cyan, cyan_dim, PI / 2.0)
	tube.call(Vector3(ex, -0.06, z), Vector3(0.04, 0.13, 0.05), cyan, cyan_dim, PI / 2.0)
	# Y
	var yx := 0.30
	tube.call(Vector3(yx - 0.05, 0.13, z), Vector3(0.04, 0.14, 0.05), amber, _glow(Color(0.4, 0.28, 0.1), 0.5), 0.5)
	tube.call(Vector3(yx + 0.05, 0.13, z), Vector3(0.04, 0.14, 0.05), amber, _glow(Color(0.4, 0.28, 0.1), 0.5), -0.5)
	tube.call(Vector3(yx, -0.04, z), Vector3(0.04, 0.16, 0.05), amber, _glow(Color(0.4, 0.28, 0.1), 0.5))
	# a little heart dot over to the side (two lobes + a V point)
	_no_shadow(_torus(root, 0.03, 0.05, pink, Vector3(0.42, 0.22, z), 16))
	_no_shadow(_torus(root, 0.03, 0.05, pink, Vector3(0.5, 0.22, z), 16))
	for sx in [-1.0, 1.0]:
		_no_shadow(_box(root, Vector3(0.022, 0.13, 0.05), pink, Vector3(0.46 + sx * 0.035, 0.13, z), sx * 0.6))
	# buzz underline + a "broken / flickering off" segment (sign character)
	tube.call(Vector3(0, -0.20, z), Vector3(0.025, 0.7, 0.05), cyan, cyan_dim, PI / 2.0)
	_no_shadow(_sphere(root, 0.018, _glow(Color(0.3, 0.18, 0.22), 0.4), Vector3(-0.38, -0.20, z)))   # dead tube end
	_no_shadow(_sphere(root, 0.018, amber, Vector3(0.38, -0.20, z)))
	# the transformer hum-box clipped to the bottom of the board + a power LED
	_box(root, Vector3(0.16, 0.06, 0.06), _toon(Color(0.16, 0.17, 0.22), 0.2), Vector3(0, -h * 0.5 + 0.02, 0.0))
	_no_shadow(_sphere(root, 0.01, _glow(Color(0.4, 1.0, 0.5), 2.0), Vector3(0.05, -h * 0.5 + 0.02, 0.04)))
	# colored wall wash
	_wall_light(root, Color(1.0, 0.42, 0.7), 1.0, 1.9, 0.16)
	return root


## ROUND GILDED MIRROR — a legendary sunburst mirror: a DOUBLE ring of radiating
## gold rays, a beaded twin-hoop frame, a faceted reflective glass disc with shine
## streaks, a full ring of glowing gemstone studs, a gem-set crown finial and a
## warm wall wash. Unmistakably top-tier. Rarity: LEGENDARY.
static func build_gilded_mirror() -> Node3D:
	var root := Node3D.new()
	var r := 0.40
	var gold := _metal(Color(0.99, 0.84, 0.42), 0.16, 0.85)
	var gold_dk := _metal(Color(0.72, 0.54, 0.22), 0.3, 0.5)
	var gold_lt := _metal(Color(1.0, 0.94, 0.66), 0.12, 0.9)
	# OUTER sunburst — long pointed rays (alternating big/small), behind glass
	for k in 24:
		var ang := TAU * float(k) / 24.0
		var big := (k % 2 == 0)
		var lng: float = 0.24 if big else 0.15
		var rr := r + lng * 0.5 - 0.01
		# pointed ray = a thin cone (cylinder tapering to 0) standing radially
		var ray := _cyl(root, 0.0, 0.026 if big else 0.016, lng, gold if big else gold_dk,
			Vector3(sin(ang) * rr, cos(ang) * rr, -0.03), 4)
		ray.rotation.z = -ang
	# INNER sunburst — short bright rays filling the gaps (the legendary density)
	for k in 24:
		var ang2 := TAU * (float(k) + 0.5) / 24.0
		var rr2 := r + 0.05
		var ray2 := _cyl(root, 0.0, 0.012, 0.08, gold_lt, Vector3(sin(ang2) * rr2, cos(ang2) * rr2, -0.02), 4)
		ray2.rotation.z = -ang2
	# beaded twin gold hoop frame
	_torus(root, r - 0.02, r + 0.05, gold, Vector3(0, 0, 0.02), 40)
	_torus(root, r - 0.06, r - 0.02, gold_dk, Vector3(0, 0, 0.03), 40)
	_bead_ring(root, r + 0.015, 0.013, 28, gold_lt, 0.05)
	# the faceted mirror glass — cool reflective disc (no shadow so glow reads)
	_no_shadow(_disc(root, r - 0.05, 0.02, _glass(Color(0.80, 0.88, 0.97), 0.92, 0.05), Vector3(0, 0, 0.01), 32))
	# fake reflection: soft diagonal shine streaks
	_no_shadow(_box(root, Vector3(0.09, r * 1.3, 0.008), _glow(Color(1, 1, 1), 0.5), Vector3(-0.08, 0.06, 0.025), 0.5))
	_no_shadow(_box(root, Vector3(0.045, r * 0.85, 0.008), _glow(Color(1, 1, 1), 0.38), Vector3(0.1, -0.04, 0.025), 0.5))
	# glowing gemstone studs set around the hoop (sapphire + the odd ruby)
	for k in 12:
		var ang3 := TAU * float(k) / 12.0 + PI / 12.0
		var gc := Color(0.4, 0.85, 1.0) if (k % 3 != 0) else Color(0.95, 0.34, 0.5)
		_jewel(root, 0.02, gc, Vector3(sin(ang3) * (r + 0.012), cos(ang3) * (r + 0.012), 0.04), 1.5)
	# a gem-set crown finial at the very top
	_disc(root, 0.04, 0.03, gold, Vector3(0, r + 0.10, 0.02), 12)
	for sx in [-1.0, 1.0]:
		_cyl(root, 0.0, 0.018, 0.08, gold, Vector3(sx * 0.05, r + 0.135, 0.02), 4).rotation.z = sx * -0.5
		_scroll(root, Vector3(sx * 0.045, r + 0.105, 0.02), sx, gold_lt, 1.0)
	_cyl(root, 0.0, 0.022, 0.09, gold_lt, Vector3(0, r + 0.15, 0.02), 4)
	_jewel(root, 0.034, Color(0.96, 0.3, 0.5), Vector3(0, r + 0.18, 0.03), 1.6)
	# warm legendary wall wash
	_wall_light(root, Color(1.0, 0.86, 0.5), 0.7, 1.8, 0.16)
	_hook(root, r + 0.21)
	return root


## GRAND WALL CLOCK — a stately gilt-rimmed clock: a beaded ornate bezel, a
## guilloché inner ring, big Roman-cardinal blocks + minute ticks, ornate hands
## at the friendly 10:10, a glass-dome sheen, a gem center cap, a gem-set crown
## and a swinging gilt pendulum with a glowing bob. Rarity: EPIC.
static func build_grand_clock() -> Node3D:
	var root := Node3D.new()
	var r := 0.36
	var gold := _metal(Color(0.96, 0.82, 0.42), 0.18, 0.8)
	var gold_dk := _metal(Color(0.70, 0.52, 0.22), 0.3, 0.5)
	var gold_lt := _metal(Color(1.0, 0.93, 0.62), 0.14, 0.88)
	# layered ornate bezel (outer carved ring + inner bead ring)
	_torus(root, r - 0.01, r + 0.07, gold, Vector3(0, 0, 0.03), 40)
	_torus(root, r - 0.04, r - 0.01, gold_dk, Vector3(0, 0, 0.04), 40)
	_bead_ring(root, r + 0.035, 0.013, 28, gold_lt, 0.06)
	# the porcelain face + a guilloché (textured) inner ring
	_disc(root, r - 0.03, 0.04, _toon(Color(0.99, 0.97, 0.92), 0.1, false, 0.2), Vector3.ZERO, 32)
	_torus(root, r - 0.05, r - 0.04, gold_lt, Vector3(0, 0, 0.03), 36)
	var z := 0.03
	# big Roman-cardinal blocks (12/3/6/9) + slim ticks elsewhere
	var tick := _toon(Color(0.16, 0.14, 0.20), 0.1, false, 0.3)
	for k in 12:
		var ang := TAU * float(k) / 12.0
		var big := (k % 3 == 0)
		var rad := r - 0.085
		if big:
			# a little "II"-style double bar to suggest a numeral
			for sgn in [-1.0, 1.0]:
				_box(root, Vector3(0.016, 0.07, 0.012), tick,
					Vector3(sin(ang) * rad + cos(ang) * sgn * 0.018, cos(ang) * rad - sin(ang) * sgn * 0.018, z), -ang)
		else:
			_box(root, Vector3(0.012, 0.04, 0.012), tick, Vector3(sin(ang) * rad, cos(ang) * rad, z), -ang)
	# fine minute pips between the hours
	for k in 60:
		if k % 5 == 0:
			continue
		var a := TAU * float(k) / 60.0
		_sphere(root, 0.005, tick, Vector3(sin(a) * (r - 0.045), cos(a) * (r - 0.045), z), Vector3.ONE, 5, 2)
	# ornate hands at the classic friendly 10:10 (built as pivots so they read clean)
	var hand := _toon(Color(0.14, 0.14, 0.22), 0.1, false, 0.3)
	var hour := Node3D.new(); hour.position = Vector3(0, 0, z + 0.006); hour.rotation.z = deg_to_rad(-60.0); root.add_child(hour)
	_box(hour, Vector3(0.03, 0.16, 0.012), hand, Vector3(0, 0.07, 0))
	_box(hour, Vector3(0.05, 0.045, 0.012), hand, Vector3(0, 0.04, 0.001))   # counterweight diamond feel
	var minute := Node3D.new(); minute.position = Vector3(0, 0, z + 0.009); minute.rotation.z = deg_to_rad(60.0); root.add_child(minute)
	_box(minute, Vector3(0.022, 0.24, 0.012), hand, Vector3(0, 0.11, 0))
	var secondh := Node3D.new(); secondh.position = Vector3(0, 0, z + 0.012); secondh.rotation.z = deg_to_rad(150.0); root.add_child(secondh)
	_box(secondh, Vector3(0.008, 0.26, 0.01), gold, Vector3(0, 0.10, 0))
	_box(secondh, Vector3(0.018, 0.05, 0.01), gold, Vector3(0, -0.04, 0))    # tail
	# gemstone center cap
	_disc(root, 0.035, 0.02, gold, Vector3(0, 0, z + 0.014), 16)
	_jewel(root, 0.022, Color(0.5, 0.85, 1.0), Vector3(0, 0, z + 0.034), 1.4)
	# faint glass-dome sheen across the face (no shadow)
	_no_shadow(_box(root, Vector3(0.12, r * 1.5, 0.006), _glow(Color(1, 1, 1), 0.22), Vector3(-0.1, 0.05, z + 0.05), 0.45))
	# crown finial on top with a gem
	_box(root, Vector3(0.08, 0.04, 0.03), gold, Vector3(0, r + 0.10, 0.02))
	for sx in [-1.0, 0.0, 1.0]:
		_cyl(root, 0.0, 0.018, 0.06, gold, Vector3(sx * 0.03, r + 0.15, 0.02), 4)
	_jewel(root, 0.024, Color(0.95, 0.32, 0.46), Vector3(0, r + 0.14, 0.03), 1.4)
	# swinging gilt pendulum below the case
	_cyl(root, 0.008, 0.008, 0.26, gold, Vector3(0, -r - 0.14, 0.0))
	_disc(root, 0.07, 0.025, gold, Vector3(0, -r - 0.28, 0.0), 20)
	_torus(root, 0.05, 0.062, gold_dk, Vector3(0, -r - 0.28, 0.012), 24)
	_no_shadow(_disc(root, 0.035, 0.03, _glow(Color(1.0, 0.9, 0.6), 0.7), Vector3(0, -r - 0.28, 0.02), 16))
	_wall_light(root, Color(1.0, 0.9, 0.66), 0.4, 1.4, 0.16)
	_hook(root, r + 0.17)
	return root


## PIXEL-ART SCREEN — a retro arcade display in a beige CRT bezel showing a chunky
## glowing pixel scene (two-tone sky, a 2×2 sun, stepped hills, a little hero
## sprite) with scanline trim, vent slats, a brushed brand badge, a power LED and
## chunky knobs. Rarity: UNCOMMON.
static func build_pixel_screen() -> Node3D:
	var root := Node3D.new()
	var w := 0.82
	var h := 0.68
	var beige := _toon(Color(0.86, 0.82, 0.72), 0.2, true, 0.2)
	var beige_dk := _toon(Color(0.7, 0.66, 0.56), 0.2)
	# chunky CRT bezel (rounded by a stepped second box)
	_box(root, Vector3(w, h, 0.12), beige, Vector3(0, 0, -0.04))
	_box(root, Vector3(w - 0.06, h - 0.06, 0.06), beige, Vector3(0, 0, 0.02))
	# the inset dark screen recess
	var sw := w - 0.18
	var sh := h - 0.18
	_box(root, Vector3(sw + 0.04, sh + 0.04, 0.04), _toon(Color(0.1, 0.11, 0.14), 0.1, false), Vector3(0, 0.02, 0.04))
	var z := 0.06
	# ── the pixel scene (a grid of emissive blocks) ──
	# sky pixels (two tone)
	_no_shadow(_box(root, Vector3(sw, sh * 0.35, 0.012), _glow(Color(0.3, 0.6, 0.95), 0.7), Vector3(0, 0.02 + sh * 0.26, z)))
	_no_shadow(_box(root, Vector3(sw, sh * 0.2, 0.012), _glow(Color(0.42, 0.72, 1.0), 0.7), Vector3(0, 0.02 + sh * 0.06, z)))
	# sun: a 2x2 pixel cluster
	for dx in [-1.0, 1.0]:
		for dy in [-1.0, 1.0]:
			_no_shadow(_box(root, Vector3(0.05, 0.05, 0.014), _glow(Color(1.0, 0.9, 0.4), 1.6), Vector3(0.18 + dx * 0.027, 0.18 + dy * 0.027, z + 0.002)))
	# rolling pixel hills (stepped green blocks)
	var grass := _glow(Color(0.36, 0.78, 0.4), 0.9)
	_no_shadow(_box(root, Vector3(sw, sh * 0.18, 0.012), grass, Vector3(0, 0.02 - sh * 0.22, z)))
	_no_shadow(_box(root, Vector3(0.16, 0.05, 0.014), grass, Vector3(-0.14, 0.02 - sh * 0.10, z + 0.002)))
	_no_shadow(_box(root, Vector3(0.12, 0.05, 0.014), grass, Vector3(0.12, 0.02 - sh * 0.10, z + 0.002)))
	# a little hero sprite (a 3-pixel chibi) standing on the grass
	_no_shadow(_box(root, Vector3(0.05, 0.05, 0.016), _glow(Color(1.0, 0.85, 0.7), 1.4), Vector3(-0.02, 0.02 - sh * 0.04, z + 0.004)))   # head
	_no_shadow(_box(root, Vector3(0.06, 0.06, 0.016), _glow(Color(0.95, 0.4, 0.5), 1.4), Vector3(-0.02, 0.02 - sh * 0.12, z + 0.004)))   # body
	# faint scanlines (a few dim horizontal bars over the screen)
	for k in 5:
		_no_shadow(_box(root, Vector3(sw, 0.004, 0.006), _glow(Color(0.0, 0.0, 0.0), 0.0), Vector3(0, 0.02 - sh * 0.4 + k * (sh * 0.18), z + 0.01)))
	# bezel trim line + a brushed brand badge + vent slats up top
	_box(root, Vector3(sw + 0.06, 0.015, 0.02), beige_dk, Vector3(0, -sh * 0.5 - 0.03, z - 0.01))
	_box(root, Vector3(0.16, 0.03, 0.012), _metal(Color(0.82, 0.66, 0.34), 0.3, 0.5), Vector3(-0.2, -h * 0.5 + 0.05, 0.07))
	for vk in 5:
		_box(root, Vector3(0.04, 0.006, 0.01), beige_dk, Vector3(w * 0.5 - 0.12 - vk * 0.03, h * 0.5 - 0.04, 0.07))
	# power LED + two chunky knobs along the bottom chin
	_no_shadow(_sphere(root, 0.014, _glow(Color(0.4, 1.0, 0.5), 2.0), Vector3(-w * 0.5 + 0.07, -h * 0.5 + 0.05, 0.06)))
	for kx in [-1.0, 1.0]:
		_cyl(root, 0.03, 0.034, 0.04, beige_dk, Vector3(kx * 0.16, -h * 0.5 + 0.05, 0.07)).rotation.x = PI / 2.0
		_box(root, Vector3(0.008, 0.03, 0.01), _toon(Color(0.2, 0.2, 0.24), 0.1, false), Vector3(kx * 0.16, -h * 0.5 + 0.05, 0.10))
	_wall_light(root, Color(0.4, 0.7, 1.0), 0.5, 1.4, 0.18)
	_hook(root, h * 0.5 + 0.02)
	return root


## PENNANT — a felt championship pennant on a turned wood dowel: a clean two-tone
## flag with a darker under-shadow felt, a striped header band, a roundel patch
## with an appliqué star, a stitched "1", brass dowel caps with a knotted cord
## and tail tassels. Honest, characterful, COMMON.
static func build_pennant() -> Node3D:
	var root := Node3D.new()
	var wood := _toon(Color(0.52, 0.37, 0.22), 0.2, true, 0.3)
	var wood_dk := _toon(Color(0.4, 0.28, 0.16), 0.15)
	# hanging dowel across the top + turned ridge detail + brass end caps + cord
	var rod := _cyl(root, 0.02, 0.02, 0.84, wood, Vector3(0, 0.36, 0.0))
	rod.rotation.z = PI / 2.0
	for rx in [-0.3, 0.3]:
		var ring := _cyl(root, 0.024, 0.024, 0.02, wood_dk, Vector3(rx, 0.36, 0.0))
		ring.rotation.z = PI / 2.0
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.04, _glow(Color(0.86, 0.66, 0.28), 0.3), Vector3(sx * 0.43, 0.36, 0))
	# a knotted hanging cord with a little bead knot
	_box(root, Vector3(0.012, 0.10, 0.008), wood_dk, Vector3(0, 0.44, -0.01))
	_sphere(root, 0.02, _toon(Color(0.6, 0.42, 0.24), 0.15), Vector3(0, 0.40, -0.005), Vector3.ONE, 8, 4)
	# the felt body: a clean prism triangle pointing right
	var pm := PrismMesh.new()
	pm.size = Vector3(0.96, 0.52, 0.03)
	var felt := MeshInstance3D.new()
	felt.mesh = pm
	felt.material_override = _toon(Color(0.18, 0.5, 0.82), 0.12)
	felt.rotation.z = -PI / 2.0
	felt.position = Vector3(0.04, 0.07, 0.0)
	root.add_child(felt)
	# a darker felt under-shadow layer for depth
	var pm2 := PrismMesh.new()
	pm2.size = Vector3(0.96, 0.52, 0.02)
	var felt2 := MeshInstance3D.new()
	felt2.mesh = pm2
	felt2.material_override = _toon(Color(0.12, 0.36, 0.62), 0.1, false)
	felt2.rotation.z = -PI / 2.0
	felt2.position = Vector3(0.05, 0.055, -0.015)
	root.add_child(felt2)
	var z := 0.02
	# striped header band near the dowel side
	_box(root, Vector3(0.06, 0.5, 0.012), _toon(Color(0.99, 0.84, 0.32), 0.1, false), Vector3(-0.34, 0.08, z))
	_box(root, Vector3(0.02, 0.5, 0.013), _toon(Color(0.95, 0.4, 0.36), 0.1, false), Vector3(-0.34, 0.08, z + 0.002))
	# a roundel patch (felt disc + ring) on the band
	_disc(root, 0.07, 0.012, _toon(Color(0.98, 0.97, 0.92), 0.1, false), Vector3(-0.18, 0.08, z + 0.002), 18)
	_torus(root, 0.06, 0.075, _toon(Color(0.95, 0.4, 0.36), 0.1, false), Vector3(-0.18, 0.08, z + 0.006), 20)
	# a bold appliqué star (overlapping tilted bars) inside the roundel
	var star := _toon(Color(0.99, 0.78, 0.26), 0.1, false)
	for a in 5:
		_box(root, Vector3(0.02, 0.085, 0.012), star, Vector3(-0.18, 0.08, z + 0.012), TAU * a / 5.0)
	# a stitched "1" toward the point
	_box(root, Vector3(0.012, 0.1, 0.011), _toon(Color(0.99, 0.97, 0.92), 0.1, false), Vector3(0.08, 0.08, z))
	_box(root, Vector3(0.022, 0.012, 0.011), _toon(Color(0.99, 0.97, 0.92), 0.1, false), Vector3(0.075, 0.04, z))
	# tail tassels at the point
	for dy in [-0.05, -0.017, 0.017, 0.05]:
		_cyl(root, 0.007, 0.005, 0.1, _toon(Color(0.99, 0.84, 0.32), 0.1, false), Vector3(0.52, 0.07 + dy, 0.0))
		_sphere(root, 0.012, _toon(Color(0.95, 0.4, 0.36), 0.1, false), Vector3(0.52, 0.02 + dy, 0.0), Vector3.ONE, 6, 3)
	return root


## BUTTERFLY DISPLAY — a glass entomology case: a soft linen mount in a deep
## shadow-box frame with a row of jewel-bright pinned butterflies (body + wing
## pairs + eye-spots + chrome pin), tiny museum labels, a faint glass sheen and a
## brass title plate. Rarity: RARE.
static func build_butterfly_display() -> Node3D:
	var root := Node3D.new()
	var w := 0.86
	var h := 0.7
	var frame_dk := _toon(Color(0.20, 0.15, 0.12), 0.2, true, 0.3)
	# deep shadow-box frame (two stepped bands) + a thin gilt inner liner
	_frame(root, w + 0.04, h + 0.04, 0.07, 0.10, frame_dk, -0.04)
	_frame(root, w, h, 0.04, 0.08, _toon(Color(0.32, 0.24, 0.18), 0.2), -0.02)
	_frame(root, w - 0.02, h - 0.02, 0.012, 0.05, _metal(Color(0.86, 0.68, 0.34), 0.3, 0.5), -0.005)
	# soft linen mount backing
	_box(root, Vector3(w, h, 0.03), _toon(Color(0.92, 0.88, 0.78), 0.1, false), Vector3(0, 0, -0.01))
	var z := 0.02
	# ── a grid of pinned butterflies (each = body + 2 wing pairs + a pin) ──
	var wing_cols := [
		Color(0.36, 0.62, 0.98),   # blue morpho
		Color(0.98, 0.56, 0.28),   # monarch orange
		Color(0.78, 0.42, 0.95),   # violet
		Color(0.34, 0.82, 0.62),   # emerald
		Color(0.98, 0.78, 0.3),    # sulphur yellow
		Color(0.95, 0.4, 0.55),    # rose
	]
	var positions := [
		Vector2(-0.26, 0.16), Vector2(0.0, 0.16), Vector2(0.26, 0.16),
		Vector2(-0.26, -0.16), Vector2(0.0, -0.16), Vector2(0.26, -0.16),
	]
	for i in positions.size():
		var p: Vector2 = positions[i]
		var col: Color = wing_cols[i]
		var wing := _toon(col, 0.25, true, 0.3)
		var wing_glow := _glow(col.lightened(0.2), 0.5)
		# body (a dark thin capsule)
		_cyl(root, 0.008, 0.008, 0.085, _toon(Color(0.14, 0.12, 0.14), 0.1), Vector3(p.x, p.y, z + 0.01))
		# antennae
		for sx in [-1.0, 1.0]:
			_box(root, Vector3(0.005, 0.04, 0.005), _toon(Color(0.14, 0.12, 0.14), 0.05, false), Vector3(p.x + sx * 0.012, p.y + 0.06, z + 0.01), sx * 0.5)
		# wings: upper (big) + lower (small), mirrored, tilted open
		for sx in [-1.0, 1.0]:
			var uw := _sphere(root, 0.055, wing, Vector3(p.x + sx * 0.055, p.y + 0.02, z + 0.008), Vector3(1.1, 0.85, 0.18), 10, 5)
			uw.rotation.z = sx * 0.5
			var lw := _sphere(root, 0.038, wing, Vector3(p.x + sx * 0.045, p.y - 0.04, z + 0.008), Vector3(1.0, 0.85, 0.18), 8, 4)
			lw.rotation.z = sx * 0.9
			# a bright wing-eye spot
			_no_shadow(_sphere(root, 0.012, wing_glow, Vector3(p.x + sx * 0.06, p.y + 0.02, z + 0.014), Vector3(1, 1, 0.4), 6, 3))
		# the pin head (a tiny chrome sphere) + a museum label below
		_sphere(root, 0.01, _metal(Color(0.8, 0.84, 0.9), 0.15, 0.9), Vector3(p.x, p.y - 0.005, z + 0.03))
		_box(root, Vector3(0.06, 0.018, 0.008), _toon(Color(0.98, 0.96, 0.9), 0.06, false), Vector3(p.x, p.y - 0.085, z))
	# the glass sheen (a faint diagonal streak across the whole case)
	_no_shadow(_box(root, Vector3(0.1, h * 1.2, 0.006), _glow(Color(1, 1, 1), 0.25), Vector3(-0.18, 0.0, z + 0.05), 0.4))
	# a brass title plate at the bottom of the frame
	_box(root, Vector3(0.3, 0.05, 0.02), _metal(Color(0.9, 0.72, 0.34), 0.25, 0.6), Vector3(0, -h * 0.5 - 0.01, 0.0))
	_box(root, Vector3(0.18, 0.012, 0.012), _toon(Color(0.28, 0.2, 0.1), 0.06, false), Vector3(0, -h * 0.5 - 0.01, 0.02))
	_hook(root, h * 0.5 + 0.08)
	return root


## VINYL RECORD WALL — a framed LP display: a glossy black record with concentric
## grooves + a colorful center label spinning over sunburst sleeve art, with a
## glossy rake highlight, a yellow 45 adapter and a gold "now playing" plaque.
## Rarity: UNCOMMON.
static func build_vinyl_wall() -> Node3D:
	var root := Node3D.new()
	var w := 0.86
	var h := 0.86
	# the album-sleeve backboard (bold split-color sleeve art)
	_box(root, Vector3(w, h, 0.03), _toon(Color(0.12, 0.14, 0.2), 0.1, false), Vector3(0, 0, -0.02))
	_box(root, Vector3(w, h * 0.5, 0.012), _toon(Color(0.96, 0.42, 0.34), 0.08, false), Vector3(0, h * 0.25, -0.01))
	# a sunburst graphic on the sleeve
	for k in 10:
		var ang := TAU * float(k) / 10.0
		_box(root, Vector3(0.02, 0.18, 0.008), _toon(Color(1.0, 0.82, 0.36), 0.06, false), Vector3(sin(ang) * 0.18, h * 0.22 + cos(ang) * 0.18, -0.005), -ang)
	# thin frame around the sleeve
	_frame(root, w, h, 0.04, 0.05, _toon(Color(0.3, 0.22, 0.16), 0.2), -0.02)
	var z := 0.02
	# ── the vinyl record: glossy black disc + concentric groove rings ──
	var r := 0.36
	_disc(root, r, 0.02, _toon(Color(0.06, 0.06, 0.08), 0.2, true, 0.5), Vector3(0.0, -0.02, z), 36)
	# groove rings (thin dim tori)
	var groove := _toon(Color(0.16, 0.16, 0.2), 0.1, false)
	for gr in [0.32, 0.27, 0.22, 0.17]:
		_torus(root, gr - 0.004, gr + 0.004, groove, Vector3(0, -0.02, z + 0.004), 36)
	# a glossy highlight arc (reads as light raking across vinyl)
	_no_shadow(_box(root, Vector3(0.06, r * 1.4, 0.006), _glow(Color(1, 1, 1), 0.3), Vector3(-0.1, 0.02, z + 0.008), 0.5))
	# the colorful center label
	_disc(root, 0.12, 0.014, _toon(Color(0.95, 0.78, 0.3), 0.12, false), Vector3(0, -0.02, z + 0.01), 24)
	_torus(root, 0.11, 0.12, _toon(Color(0.86, 0.3, 0.34), 0.1, false), Vector3(0, -0.02, z + 0.014), 24)
	# a few label "text" bars + the spindle hole
	for ly in [0.03, -0.05]:
		_box(root, Vector3(0.1, 0.012, 0.008), _toon(Color(0.3, 0.2, 0.12), 0.06, false), Vector3(0, -0.02 + ly, z + 0.016))
	_disc(root, 0.012, 0.02, _toon(Color(0.1, 0.1, 0.12), 0.1, false), Vector3(0, -0.02, z + 0.02), 12)
	# a yellow 45 adapter clipped to the corner
	_torus(root, 0.02, 0.035, _toon(Color(0.98, 0.82, 0.3), 0.12), Vector3(w * 0.5 - 0.08, -h * 0.5 + 0.08, z), 16)
	# a gold "now playing" plaque at the bottom
	_box(root, Vector3(0.34, 0.07, 0.02), _metal(Color(0.92, 0.74, 0.34), 0.25, 0.6), Vector3(0, -h * 0.5 - 0.01, 0.0))
	_box(root, Vector3(0.2, 0.018, 0.012), _toon(Color(0.3, 0.22, 0.1), 0.06, false), Vector3(0, -h * 0.5 - 0.01, 0.02))
	_hook(root, h * 0.5 + 0.06)
	return root


## HOLOGRAPHIC POSTER — a frameless levitating holo-panel: an iridescent acrylic
## sheet projecting a glowing wireframe planet (latitude rings + meridians) with
## three tilted orbit rings, orbiting node gems, a data-readout chart + glyph
## lines, drifting holographic motes, edge-lit on a slim emitter rail. Future-luxe.
## Rarity: LEGENDARY.
static func build_holo_poster() -> Node3D:
	var root := Node3D.new()
	var w := 0.7
	var h := 0.98
	# slim dark emitter rail along the bottom (where the holo "projects from")
	_box(root, Vector3(w + 0.06, 0.06, 0.08), _toon(Color(0.1, 0.11, 0.16), 0.2, true, 0.4), Vector3(0, -h * 0.5 - 0.05, 0.0))
	_no_shadow(_box(root, Vector3(w - 0.04, 0.012, 0.02), _glow(Color(0.5, 0.9, 1.0), 1.8), Vector3(0, -h * 0.5 - 0.02, 0.05)))
	# little emitter studs on the rail
	for sx in [-1.0, 0.0, 1.0]:
		_no_shadow(_sphere(root, 0.012, _glow(Color(0.6, 0.95, 1.0), 1.6), Vector3(sx * 0.22, -h * 0.5 - 0.05, 0.05), Vector3.ONE, 6, 3))
	# the iridescent acrylic sheet (translucent, faintly emissive edges)
	_no_shadow(_box(root, Vector3(w, h, 0.018), _glass(Color(0.55, 0.8, 1.0), 0.18, 0.4), Vector3(0, 0, 0.0)))
	# edge-light channel around all four sides
	_frame(root, w, h, 0.012, 0.03, _glow(Color(0.6, 0.92, 1.0), 1.6), 0.01)
	var z := 0.03
	# ── the hologram: a glowing wireframe planet + orbit rings ──
	var holo := _glow(Color(0.5, 0.95, 1.0), 1.6)
	var holo_warm := _glow(Color(0.7, 0.55, 1.0), 1.5)
	# planet — a wire globe suggested by stacked latitude rings
	for k in 5:
		var ry := 0.18 - k * 0.045
		var rr := 0.14 * sqrt(maxf(0.0, 1.0 - pow(ry / 0.16, 2.0)))
		if rr > 0.02:
			_no_shadow(_torus(root, rr - 0.006, rr + 0.006, holo, Vector3(0, 0.16 + ry, z), 22))
	# a couple of vertical meridian arcs
	for sx in [-1.0, 1.0]:
		var mer := _torus(root, 0.13, 0.142, holo, Vector3(0, 0.16, z), 22)
		mer.rotation.y = sx * 0.8
		_no_shadow(mer)
	# three tilted orbit rings around the planet
	for i in 3:
		var orb := _torus(root, 0.24 + i * 0.02, 0.252 + i * 0.02, holo_warm if i == 1 else holo, Vector3(0, 0.16, z - 0.002), 28)
		orb.rotation.x = 1.1 + i * 0.2
		orb.rotation.z = i * 0.4
		_no_shadow(orb)
	# little orbiting node gems on the rings
	for nd in [Vector2(0.26, 0.16), Vector2(-0.24, 0.22), Vector2(0.1, -0.06)]:
		_no_shadow(_gem(root, 0.018, holo_warm, Vector3(nd.x, nd.y, z + 0.005)))
	# a glowing data readout: a column of thin bars (a fake chart) lower down
	var bars := [0.06, 0.1, 0.07, 0.13, 0.09, 0.05]
	for i in bars.size():
		_no_shadow(_box(root, Vector3(0.02, bars[i], 0.006), holo, Vector3(-0.22 + i * 0.06, -0.28 + bars[i] * 0.5, z)))
	# a few glyph "text" lines under the chart
	for ly in 3:
		_no_shadow(_box(root, Vector3(0.3, 0.01, 0.005), holo, Vector3(-0.05, -0.40 - ly * 0.04, z)))
	# drifting holographic motes
	var mote := CPUParticles3D.new()
	mote.position = Vector3(0, 0.0, z + 0.02)
	mote.amount = 14
	mote.lifetime = 3.0
	mote.preprocess = 2.5
	mote.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	mote.emission_box_extents = Vector3(w * 0.45, h * 0.45, 0.01)
	mote.direction = Vector3(0, 1, 0)
	mote.spread = 12.0
	mote.gravity = Vector3(0, 0.05, 0)
	mote.initial_velocity_min = 0.01
	mote.initial_velocity_max = 0.05
	mote.scale_amount_min = 0.4
	mote.scale_amount_max = 1.0
	var dot := SphereMesh.new()
	dot.radius = 0.01
	dot.height = 0.02
	dot.radial_segments = 5
	dot.rings = 2
	dot.material = _glow(Color(0.7, 0.95, 1.0), 2.0)
	mote.mesh = dot
	root.add_child(mote)
	# cool wall wash
	_wall_light(root, Color(0.45, 0.8, 1.0), 0.9, 1.8, 0.16)
	_hook(root, h * 0.5 + 0.02)
	return root
