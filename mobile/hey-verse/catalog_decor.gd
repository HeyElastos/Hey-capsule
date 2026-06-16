class_name VerseCatalogDecor
extends RefCounted
## Hey Verse — DECOR & MISC catalog (showroom set, 11 items).
##
## The "wow" shelf: a hand-knotted medallion rug, an ornate ceramic vase, a
## golden hero statue, a tiered grand fountain, a wintry snow globe, a singing
## gramophone, a brass observatory telescope, a living reef aquarium, a hovering
## arcane crystal, a party balloon bunch and a champions' trophy. A mix of
## floor-scale hero pieces and tabletop trinkets, all scaled for the ~1.4-unit
## chibi-robot world (a piece on the floor should read at robot-knee to
## robot-shoulder height; tabletop trinkets sit on their own base).
##
## Pure procedural primitives only (no art assets). Every item is a static
## `build_<id>() -> Node3D` returning ONE self-contained Node3D, built at the
## ORIGIN and resting on the floor plane y=0 (tabletop pieces sit on a base so
## they read at table height too). Each builder is standalone: it re-declares the
## tiny material/mesh helpers it needs, pulling only the shared toon + outline
## shaders for the look — no home.gd / avatar.gd internals, no .glb.
##
## Look conventions (matched to the rest of the Verse):
##  - solid surfaces : toon cel material + inverted-hull outline       (_toon)
##  - metals         : toon with high spec — gold / brass / chrome     (_metal)
##  - glowing parts  : unshaded emissive StandardMaterial3D            (_glow)
##  - glass / water  : alpha StandardMaterial3D, shadow casting OFF     (_glass)
##  - gemstones      : bright glassy emissive jewels for high rarities  (_gem)
##  - rarity is READABLE: higher tier = more gold trim, gemstones, glow,
##    particle sparkle and richer silhouettes.

const TOON_SHADER := preload("res://toon.gdshader")
const OUTLINE_SHADER := preload("res://outline.gdshader")

static var _outline_mat: ShaderMaterial


# ───────────────────────────── shared helpers (self-contained) ──────────────

## Cel material + inverted-hull outline — the Verse "designed" look on solids.
static func _toon(c: Color, rim := 0.3, outline := true, spec := 0.0) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = TOON_SHADER
	m.set_shader_parameter("albedo", c)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", spec)
	m.set_shader_parameter("wind_strength", 0.0)
	m.set_shader_parameter("wind_height", 0.5)
	if outline:
		if _outline_mat == null:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = OUTLINE_SHADER
		m.next_pass = _outline_mat
	return m


## A polished-metal toon look — strong rim + spec dot reads as gold/brass/chrome.
static func _metal(c: Color, spec := 0.7) -> ShaderMaterial:
	return _toon(c, 0.5, true, spec)


## Unshaded emissive — bulbs, gems, neon, fireflies, glowing water.
static func _glow(c: Color, energy := 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


## A faceted jewel — glassy, lightly emissive so gems on Epic/Legendary pieces
## pop and read instantly as "valuable". Keep small; one draw, no outline.
static func _gem(c: Color, alpha := 0.85) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.05
	m.metallic = 0.2
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.7
	return m


## Translucent glass / water dome — soft, slightly glossy, no shadow.
static func _glass(c: Color, alpha := 0.35) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.12
	m.metallic = 0.1
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.12
	return m


static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 16) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, s := Vector3.ONE) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 20
	sm.rings = 10
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = s
	parent.add_child(mi)
	return mi


static func _capsule(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var cm := CapsuleMesh.new()
	cm.radius = r
	cm.height = h
	cm.radial_segments = 16
	cm.rings = 6
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, ring_seg := 12) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 20
	tm.ring_segments = ring_seg
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _prism(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var pm := PrismMesh.new()
	pm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


## A faceted gemstone — an octahedron (two prisms tip-to-tip) for cut sparkle.
static func _jewel(parent: Node3D, w: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> Node3D:
	var g := Node3D.new()
	g.position = pos
	g.rotation = rot
	parent.add_child(g)
	_prism(g, Vector3(w, h * 0.6, w), mat, Vector3(0, h * 0.15, 0))
	_prism(g, Vector3(w, h * 0.4, w), mat, Vector3(0, -h * 0.05, 0), Vector3(PI, 0, 0))
	return g


## A faint contact shadow blob on the floor — grounds the piece like the avatar's.
## `c_off` shifts the disc in the XZ plane (use for off-center pieces).
static func _contact(parent: Node3D, r: float, c_off := Vector3.ZERO) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = r
	disc.bottom_radius = r
	disc.height = 0.01
	disc.radial_segments = 24
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = Color(0, 0, 0, 0.12)
	var mi := MeshInstance3D.new()
	mi.mesh = disc
	mi.material_override = m
	mi.position = Vector3(c_off.x, 0.012, c_off.z)
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)


## A small OmniLight tucked inside a glowing piece (fountain, aquarium, crystal).
static func _light(parent: Node3D, c: Color, energy: float, rng: float, pos: Vector3) -> OmniLight3D:
	var l := OmniLight3D.new()
	l.light_color = c
	l.light_energy = energy
	l.omni_range = rng
	l.position = pos
	l.shadow_enabled = false
	parent.add_child(l)
	return l


## A reusable sparkle/spray particle node — cheap CPU particles for spectacle.
static func _particles(parent: Node3D, pos: Vector3, amount: int, life: float, up_vel: Vector3, spread: float, grav: Vector3, mesh_r: float, mesh_mat: Material, sphere_r := 0.0) -> CPUParticles3D:
	var p := CPUParticles3D.new()
	p.position = pos
	p.amount = amount
	p.lifetime = life
	p.preprocess = life * 0.6
	p.direction = up_vel.normalized()
	p.spread = spread
	p.gravity = grav
	p.initial_velocity_min = up_vel.length() * 0.7
	p.initial_velocity_max = up_vel.length() * 1.2
	p.scale_amount_min = 0.4
	p.scale_amount_max = 1.0
	if sphere_r > 0.0:
		p.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
		p.emission_sphere_radius = sphere_r
	var sm := SphereMesh.new()
	sm.radius = mesh_r
	sm.height = mesh_r * 2.0
	sm.radial_segments = 6
	sm.rings = 3
	sm.material = mesh_mat
	p.mesh = sm
	parent.add_child(p)
	return p


# ════════════════════════════════════════════════════════════════════ ITEMS


## 1 · PATTERNED RUG — a hand-knotted Persian medallion rug with a fringed border.
##     Common: a homey floor piece, warm madder wool, a radiating center
##     medallion with petal points, twin guard-borders and corner spandrels.
static func build_rug() -> Node3D:
	var root := Node3D.new()
	var field := _toon(Color(0.60, 0.18, 0.20), 0.10)          # deep madder red
	var navy := _toon(Color(0.13, 0.22, 0.38), 0.10)           # indigo
	var navy_l := _toon(Color(0.20, 0.32, 0.50), 0.10)
	var cream := _toon(Color(0.95, 0.91, 0.80), 0.10)          # ivory
	var gold := _metal(Color(0.86, 0.70, 0.34), 0.4)
	var teal := _toon(Color(0.20, 0.52, 0.50), 0.10)
	var w := 1.9
	var d := 1.3
	# the wool field — a thin slab just above the floor
	_box(root, Vector3(w, 0.020, d), field, Vector3(0, 0.010, 0))
	# nested borders: outer navy → ivory guard → inner field → gold keyline
	_box(root, Vector3(w - 0.12, 0.024, d - 0.12), navy, Vector3(0, 0.012, 0))
	_box(root, Vector3(w - 0.20, 0.028, d - 0.20), cream, Vector3(0, 0.014, 0))
	_box(root, Vector3(w - 0.26, 0.032, d - 0.26), gold, Vector3(0, 0.016, 0))
	_box(root, Vector3(w - 0.30, 0.036, d - 0.30), field, Vector3(0, 0.018, 0))
	# a running ivory "boteh" dotted line in the main border (top & bottom)
	for i in 11:
		var bx := -w / 2.0 + 0.14 + i * (w - 0.28) / 10.0
		_box(root, Vector3(0.05, 0.026, 0.05), cream, Vector3(bx, 0.013, d / 2.0 - 0.08), Vector3(0, PI / 4.0, 0))
		_box(root, Vector3(0.05, 0.026, 0.05), cream, Vector3(bx, 0.013, -(d / 2.0 - 0.08)), Vector3(0, PI / 4.0, 0))
	for i in 7:
		var bz := -d / 2.0 + 0.14 + i * (d - 0.28) / 6.0
		_box(root, Vector3(0.05, 0.026, 0.05), cream, Vector3(w / 2.0 - 0.08, 0.013, bz), Vector3(0, PI / 4.0, 0))
		_box(root, Vector3(0.05, 0.026, 0.05), cream, Vector3(-(w / 2.0 - 0.08), 0.013, bz), Vector3(0, PI / 4.0, 0))
	# the central medallion — a radiating star: navy core, gold ring, ivory petals
	_box(root, Vector3(0.62, 0.040, 0.42), navy, Vector3(0, 0.020, 0), Vector3(0, PI / 4.0, 0))
	_box(root, Vector3(0.46, 0.044, 0.32), gold, Vector3(0, 0.022, 0), Vector3(0, PI / 4.0, 0))
	_box(root, Vector3(0.32, 0.050, 0.22), field, Vector3(0, 0.025, 0), Vector3(0, PI / 4.0, 0))
	_box(root, Vector3(0.14, 0.056, 0.14), cream, Vector3(0, 0.028, 0), Vector3(0, PI / 4.0, 0))
	# eight radiating petal points around the medallion (alternating ivory/teal)
	for k in 8:
		var ang := TAU * float(k) / 8.0
		var pr := 0.30
		var pc: Material = cream if k % 2 == 0 else teal
		_box(root, Vector3(0.10, 0.052, 0.05), pc, Vector3(cos(ang) * pr, 0.026, sin(ang) * pr * 0.7), Vector3(0, -ang, 0))
	# four corner spandrels (ivory + navy quarter motifs)
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			var cx: float = sx * (w / 2.0 - 0.28)
			var cz: float = sz * (d / 2.0 - 0.22)
			_box(root, Vector3(0.20, 0.040, 0.20), navy_l, Vector3(cx, 0.020, cz), Vector3(0, PI / 4.0, 0))
			_box(root, Vector3(0.10, 0.046, 0.10), cream, Vector3(cx, 0.023, cz), Vector3(0, PI / 4.0, 0))
	# fringe tassels along the two short ends
	var fringe := _toon(Color(0.90, 0.86, 0.74), 0.1)
	for i in 15:
		var fx := -w / 2.0 + 0.07 + i * (w - 0.14) / 14.0
		_box(root, Vector3(0.03, 0.018, 0.11), fringe, Vector3(fx, 0.009, d / 2.0 + 0.07))
		_box(root, Vector3(0.03, 0.018, 0.11), fringe, Vector3(fx, 0.009, -(d / 2.0 + 0.07)))
	return root


## 2 · ORNATE VASE — a curvy glazed-porcelain vase with gold filigree handles
##     and a fresh spring bouquet.
##     Uncommon: a glossy tabletop hero — sculpted profile, twin gold handles,
##     a painted cartouche and a five-bloom posy with a glowing center.
static func build_vase() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.34)
	var ceramic := _toon(Color(0.18, 0.44, 0.60), 0.45, true, 0.7)    # glossy teal porcelain
	var ceramic_l := _toon(Color(0.30, 0.58, 0.72), 0.45, true, 0.6)
	var gold := _metal(Color(0.93, 0.77, 0.37), 0.8)
	# sculpted body: foot -> swelling belly -> pinched neck -> flared lip
	_cyl(root, 0.13, 0.18, 0.07, gold, Vector3(0, 0.035, 0), Vector3.ZERO, 20)        # gold foot
	_cyl(root, 0.16, 0.13, 0.06, ceramic, Vector3(0, 0.10, 0), Vector3.ZERO, 20)      # base flare
	_ball(root, 0.26, ceramic, Vector3(0, 0.36, 0), Vector3(1.0, 1.08, 1.0))          # belly
	_cyl(root, 0.10, 0.245, 0.18, ceramic, Vector3(0, 0.60, 0), Vector3.ZERO, 20)     # shoulder taper
	_cyl(root, 0.105, 0.085, 0.16, ceramic_l, Vector3(0, 0.76, 0), Vector3.ZERO, 20)  # neck
	_cyl(root, 0.17, 0.10, 0.07, ceramic, Vector3(0, 0.87, 0), Vector3.ZERO, 20)      # flared lip
	_torus(root, 0.155, 0.185, gold, Vector3(0, 0.90, 0), Vector3(PI / 2.0, 0, 0), 20)  # gold lip ring
	# gold filigree bands at shoulder + belly
	_torus(root, 0.095, 0.125, gold, Vector3(0, 0.685, 0), Vector3(PI / 2.0, 0, 0), 20)
	_torus(root, 0.205, 0.235, gold, Vector3(0, 0.215, 0), Vector3(PI / 2.0, 0, 0), 20)
	# twin S-curve gold handles (each a small torus, squashed + tilted)
	for sx in [-1.0, 1.0]:
		var hdl := _torus(root, 0.045, 0.10, gold, Vector3(sx * 0.245, 0.50, 0), Vector3(0, PI / 2.0, 0), 10)
		hdl.scale = Vector3(1.0, 1.5, 0.5)
	# a painted ivory cartouche medallion on the belly front (proud of the glaze)
	_ball(root, 0.10, _toon(Color(0.96, 0.93, 0.85), 0.3, true, 0.4), Vector3(0, 0.36, 0.235), Vector3(1.1, 1.4, 0.18))
	_torus(root, 0.085, 0.105, gold, Vector3(0, 0.36, 0.245), Vector3(0, 0, 0), 14)   # gold frame
	# a fresh five-stem bouquet rising from the mouth
	var stem := _toon(Color(0.34, 0.58, 0.32), 0.2)
	var blooms := [Color(0.97, 0.45, 0.56), Color(0.99, 0.79, 0.36), Color(0.74, 0.56, 0.92), Color(0.96, 0.62, 0.40), Color(0.55, 0.78, 0.96)]
	for k in 5:
		var ang := TAU * float(k) / 5.0
		var bx := cos(ang) * 0.08
		var bz := sin(ang) * 0.08
		_cyl(root, 0.012, 0.014, 0.30, stem, Vector3(bx, 1.04, bz), Vector3(bz * 0.7, 0, -bx * 0.7), 6)
		# flower head: a ring of petal balls + a glowing center
		var top := Vector3(bx * 2.6, 1.20, bz * 2.6)
		var fcol: Color = blooms[k]
		for p in 6:
			var pa := TAU * float(p) / 6.0
			_ball(root, 0.042, _toon(fcol, 0.3), top + Vector3(cos(pa) * 0.05, 0, sin(pa) * 0.05), Vector3(1, 0.55, 1))
		_ball(root, 0.034, _glow(Color(1.0, 0.86, 0.42), 0.9), top)
		# two leaves on the stem
		_ball(root, 0.04, stem, Vector3(bx * 1.7, 1.02, bz * 1.7), Vector3(1.7, 0.3, 0.7))
		_ball(root, 0.032, stem, Vector3(bx * 1.3, 0.92, bz * 1.3), Vector3(1.4, 0.3, 0.6))
	return root


## 3 · GOLDEN STATUE — a heroic chibi-robot monument, solid gold on a banded
##     marble plinth, with gem-set corners and a laureled engraved plaque.
##     Epic: a proud civic monument of your species, victory pose, glowing core.
static func build_statue() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.5)
	var gold := _metal(Color(0.96, 0.79, 0.31), 0.85)
	var gold_d := _metal(Color(0.80, 0.62, 0.22), 0.7)
	var marble := _toon(Color(0.90, 0.90, 0.94), 0.35, true, 0.45)
	var marble_d := _toon(Color(0.74, 0.74, 0.80), 0.30, true, 0.30)
	var sapphire := _gem(Color(0.30, 0.55, 1.0))
	# marble plinth — stepped base, gold cornice, gem-set corners, name-plaque
	_box(root, Vector3(0.80, 0.10, 0.80), marble_d, Vector3(0, 0.05, 0))           # base slab
	_box(root, Vector3(0.84, 0.04, 0.84), gold_d, Vector3(0, 0.115, 0))            # base trim
	_box(root, Vector3(0.58, 0.36, 0.58), marble, Vector3(0, 0.30, 0))            # column
	# fluted pilasters on the column faces
	for k in 4:
		var ang := TAU * float(k) / 4.0
		_cyl(root, 0.03, 0.03, 0.34, marble_d, Vector3(cos(ang) * 0.295, 0.30, sin(ang) * 0.295), Vector3.ZERO, 8)
	_box(root, Vector3(0.66, 0.06, 0.66), gold, Vector3(0, 0.50, 0))              # gold cornice band
	_box(root, Vector3(0.52, 0.06, 0.52), marble_d, Vector3(0, 0.56, 0))         # cap
	# gem-set corners on the cornice
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_jewel(root, 0.05, 0.07, sapphire, Vector3(sx * 0.30, 0.53, sz * 0.30))
	# engraved gold name plaque with two laurel sprigs (front)
	_box(root, Vector3(0.30, 0.10, 0.02), gold, Vector3(0, 0.34, 0.30))
	for sx in [-1.0, 1.0]:
		for li in 3:
			_ball(root, 0.012, gold_d, Vector3(sx * (0.10 + li * 0.018), 0.34 + li * 0.012, 0.315), Vector3(1.6, 0.5, 0.4))
	# --- the golden hero: a small statue of the mascot robot, posed proud ---
	var hy := 0.59   # statue stands on top of the plinth cap
	# legs (slightly apart, planted) with navy-gold boots
	for sx in [-1.0, 1.0]:
		_capsule(root, 0.052, 0.22, gold, Vector3(sx * 0.10, hy + 0.13, 0))
		_ball(root, 0.075, gold_d, Vector3(sx * 0.10, hy + 0.0, 0.02), Vector3(1.0, 0.6, 1.4))   # boot
	# torso (rounded chest tucking to a waist) + a glowing chest gem
	_capsule(root, 0.155, 0.34, gold, Vector3(0, hy + 0.42, 0))
	_ball(root, 0.115, gold_d, Vector3(0, hy + 0.27, 0), Vector3(1, 0.6, 1))    # pelvis
	_jewel(root, 0.05, 0.07, sapphire, Vector3(0, hy + 0.46, 0.135), Vector3(0.25, 0, 0))   # heart gem
	# one arm raised triumphantly, one on the hip
	_capsule(root, 0.046, 0.26, gold, Vector3(-0.22, hy + 0.62, 0), Vector3(0, 0, -1.1))  # raised
	_ball(root, 0.062, gold, Vector3(-0.36, hy + 0.78, 0))                                 # raised fist
	_capsule(root, 0.046, 0.24, gold, Vector3(0.21, hy + 0.42, 0), Vector3(0, 0, 0.5))     # on hip
	_ball(root, 0.062, gold, Vector3(0.28, hy + 0.30, 0))
	# neck + TV-style head with a face panel
	_cyl(root, 0.06, 0.075, 0.06, gold_d, Vector3(0, hy + 0.62, 0))
	_box(root, Vector3(0.28, 0.22, 0.24), gold, Vector3(0, hy + 0.75, 0))
	_box(root, Vector3(0.21, 0.14, 0.04), gold_d, Vector3(0, hy + 0.75, 0.12))   # face plate
	# glowing eyes on the gold face (the spark of life)
	for sx in [-1.0, 1.0]:
		_ball(root, 0.022, _glow(Color(0.55, 0.88, 1.0), 1.8), Vector3(sx * 0.05, hy + 0.77, 0.135))
	# little antenna with a glowing tip
	_cyl(root, 0.012, 0.012, 0.10, gold_d, Vector3(0, hy + 0.91, 0), Vector3.ZERO, 6)
	_ball(root, 0.032, _glow(Color(0.5, 0.85, 1.0), 1.8), Vector3(0, hy + 0.99, 0))
	# a triumphant glow pool at the statue's feet + slow sparkle motes
	_light(root, Color(1.0, 0.86, 0.5), 1.0, 2.4, Vector3(0, hy + 0.5, 0.25))
	_particles(root, Vector3(0, hy + 0.4, 0), 10, 2.4, Vector3(0, 0.18, 0), 28.0, Vector3(0, 0.1, 0), 0.014, _glow(Color(1.0, 0.9, 0.55), 1.4), 0.30)
	return root


## 4 · TIERED FOUNTAIN — a grand three-tier stone fountain with gold dolphin
##     spouts, scalloped basins, lily pads and lit, flowing, sparkling water.
##     Legendary: the centerpiece — sculpted basins, gem inlays, cascading
##     water sheets, an arcing top jet and rising spray.
static func build_fountain() -> Node3D:
	var root := Node3D.new()
	_contact(root, 1.0)
	var stone := _toon(Color(0.82, 0.82, 0.88), 0.32, true, 0.40)
	var stone_d := _toon(Color(0.66, 0.66, 0.74), 0.28, true, 0.25)
	var gold := _metal(Color(0.96, 0.81, 0.35), 0.85)
	var gold_d := _metal(Color(0.80, 0.62, 0.22), 0.7)
	var water := _glass(Color(0.45, 0.82, 0.98), 0.45)
	var sheet := _glass(Color(0.62, 0.90, 1.0), 0.22)
	var ruby := _gem(Color(0.95, 0.30, 0.42))
	# --- big base pool (octagonal: a wide low cylinder + scalloped rim wall) ---
	_cyl(root, 0.96, 1.02, 0.16, stone_d, Vector3(0, 0.08, 0), Vector3.ZERO, 8)
	_torus(root, 0.84, 1.0, stone, Vector3(0, 0.22, 0), Vector3(PI / 2.0, 0, 0), 8)   # rim wall
	_torus(root, 0.92, 0.98, gold, Vector3(0, 0.30, 0), Vector3(PI / 2.0, 0, 0), 8)   # gold trim ring
	_cyl(root, 0.86, 0.86, 0.05, water, Vector3(0, 0.19, 0), Vector3.ZERO, 24)        # pool water
	# scalloped merlons + gem inlays around the rim
	for k in 8:
		var ang := TAU * float(k) / 8.0
		_ball(root, 0.07, stone, Vector3(cos(ang) * 0.93, 0.31, sin(ang) * 0.93), Vector3(1, 0.8, 1))
		_jewel(root, 0.035, 0.05, ruby, Vector3(cos(ang) * 0.93, 0.36, sin(ang) * 0.93))
	# four gold dolphin/fish spouts arcing inward over the pool
	for k in 4:
		var ang := TAU * float(k) / 4.0 + PI / 4.0
		var fx := cos(ang)
		var fz := sin(ang)
		var base := Vector3(fx * 0.7, 0.34, fz * 0.7)
		_capsule(root, 0.05, 0.22, gold, base + Vector3(0, 0.10, 0), Vector3(fz * 0.9, -ang, -fx * 0.9))   # arched body
		_ball(root, 0.06, gold, base + Vector3(-fx * 0.10, 0.20, -fz * 0.10))                              # head
		_prism(root, Vector3(0.10, 0.10, 0.04), gold_d, base + Vector3(fx * 0.10, 0.02, fz * 0.10), Vector3(0, -ang, 1.2))  # tail fin
		_ball(root, 0.025, _glow(Color(0.7, 0.92, 1.0), 1.4), base + Vector3(-fx * 0.13, 0.20, -fz * 0.13))  # spout mouth glow
	# floating lily pads + a pink bloom on the pool
	for k in 3:
		var ang := TAU * float(k) / 3.0 + 0.5
		_cyl(root, 0.10, 0.10, 0.012, _toon(Color(0.26, 0.56, 0.34), 0.2), Vector3(cos(ang) * 0.55, 0.205, sin(ang) * 0.55), Vector3.ZERO, 12)
	_ball(root, 0.05, _toon(Color(0.97, 0.55, 0.66), 0.3), Vector3(0.55, 0.235, 0.0), Vector3(1, 0.6, 1))   # lily flower
	# central column rising through the tiers
	_cyl(root, 0.10, 0.14, 0.5, stone, Vector3(0, 0.42, 0), Vector3.ZERO, 12)
	_torus(root, 0.11, 0.16, gold_d, Vector3(0, 0.46, 0), Vector3(PI / 2.0, 0, 0), 12)
	# --- tier 2: a mid basin ---
	_cyl(root, 0.46, 0.40, 0.08, stone_d, Vector3(0, 0.62, 0), Vector3.ZERO, 16)
	_torus(root, 0.40, 0.50, stone, Vector3(0, 0.68, 0), Vector3(PI / 2.0, 0, 0), 16)
	_torus(root, 0.46, 0.50, gold, Vector3(0, 0.72, 0), Vector3(PI / 2.0, 0, 0), 16)
	_cyl(root, 0.42, 0.42, 0.04, water, Vector3(0, 0.66, 0), Vector3.ZERO, 20)
	_cyl(root, 0.06, 0.09, 0.34, stone, Vector3(0, 0.84, 0), Vector3.ZERO, 10)
	# --- tier 3: top basin + gold finial jet ---
	_cyl(root, 0.24, 0.20, 0.06, stone_d, Vector3(0, 1.0, 0), Vector3.ZERO, 14)
	_torus(root, 0.20, 0.27, stone, Vector3(0, 1.04, 0), Vector3(PI / 2.0, 0, 0), 14)
	_torus(root, 0.24, 0.28, gold, Vector3(0, 1.07, 0), Vector3(PI / 2.0, 0, 0), 14)
	_cyl(root, 0.22, 0.22, 0.03, water, Vector3(0, 1.03, 0), Vector3.ZERO, 16)
	_cyl(root, 0.04, 0.06, 0.12, gold, Vector3(0, 1.12, 0), Vector3.ZERO, 10)
	_jewel(root, 0.07, 0.12, ruby, Vector3(0, 1.24, 0))                               # crowning gem finial
	_ball(root, 0.04, _glow(Color(0.6, 0.9, 1.0), 1.8), Vector3(0, 1.18, 0))          # spout glow
	# water sheets cascading between tiers (thin translucent flared rings)
	_cyl(root, 0.22, 0.40, 0.12, sheet, Vector3(0, 0.73, 0), Vector3.ZERO, 16)
	_cyl(root, 0.42, 0.80, 0.13, sheet, Vector3(0, 0.29, 0), Vector3.ZERO, 20)
	# a soft underwater glow + drifting spray
	_light(root, Color(0.45, 0.8, 1.0), 1.5, 3.6, Vector3(0, 0.5, 0))
	_particles(root, Vector3(0, 1.22, 0), 26, 1.6, Vector3(0, 1.1, 0), 24.0, Vector3(0, -2.2, 0), 0.025, _glow(Color(0.7, 0.92, 1.0), 1.3))
	return root


## 5 · SNOW GLOBE — a wintry village under a glass dome on a carved wood base.
##     Rare: a tabletop charm — a lit cottage, a pine, a snowman, a glowing
##     lamppost and a tiny path, all under drifting snow with a gold seat ring.
static func build_snowglobe() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.27)
	var wood := _toon(Color(0.46, 0.30, 0.18), 0.25, true, 0.2)
	var wood_l := _toon(Color(0.58, 0.40, 0.24), 0.25, true, 0.2)
	var gold := _metal(Color(0.92, 0.76, 0.36), 0.65)
	var snow := _toon(Color(0.96, 0.97, 1.0), 0.3, true, 0.3)
	# carved wooden base (turned profile) with twin gold bands + a plaque
	_cyl(root, 0.26, 0.28, 0.10, wood, Vector3(0, 0.05, 0), Vector3.ZERO, 18)
	_torus(root, 0.20, 0.26, wood_l, Vector3(0, 0.11, 0), Vector3(PI / 2.0, 0, 0), 18)
	_torus(root, 0.245, 0.275, gold, Vector3(0, 0.05, 0), Vector3(PI / 2.0, 0, 0), 18)
	_cyl(root, 0.21, 0.23, 0.06, wood, Vector3(0, 0.17, 0), Vector3.ZERO, 18)
	_box(root, Vector3(0.18, 0.04, 0.02), gold, Vector3(0, 0.13, 0.24))               # gold plaque
	_torus(root, 0.17, 0.21, gold, Vector3(0, 0.21, 0), Vector3(PI / 2.0, 0, 0), 18)  # gold ring seat
	# snowy ground disc inside
	_cyl(root, 0.18, 0.19, 0.05, snow, Vector3(0, 0.25, 0), Vector3.ZERO, 18)
	# a winding path across the snow (warm tan flagstones)
	for i in 4:
		_box(root, Vector3(0.035, 0.012, 0.045), _toon(Color(0.74, 0.66, 0.52), 0.2), Vector3(-0.10 + i * 0.05, 0.28, 0.06 - i * 0.02), Vector3(0, 0.3 * i, 0))
	# --- the little scene ---
	# a cozy cottage: walls + snowy gabled roof + warm window glow + chimney
	_box(root, Vector3(0.14, 0.11, 0.12), _toon(Color(0.88, 0.74, 0.52), 0.25), Vector3(-0.05, 0.34, 0.02))
	_prism(root, Vector3(0.18, 0.10, 0.16), _toon(Color(0.78, 0.32, 0.30), 0.25), Vector3(-0.05, 0.445, 0.02))
	_prism(root, Vector3(0.19, 0.04, 0.17), snow, Vector3(-0.05, 0.50, 0.02))                       # snow on roof
	_box(root, Vector3(0.03, 0.06, 0.03), _toon(Color(0.5, 0.34, 0.26), 0.2), Vector3(0.0, 0.50, -0.02))  # chimney
	_ball(root, 0.026, _glow(Color(1.0, 0.78, 0.40), 1.8), Vector3(-0.05, 0.33, 0.082))             # window glow
	# a snowy pine tree
	_cyl(root, 0.01, 0.014, 0.05, wood, Vector3(0.10, 0.29, -0.04), Vector3.ZERO, 6)
	_prism(root, Vector3(0.13, 0.09, 0.13), _toon(Color(0.20, 0.48, 0.30), 0.25), Vector3(0.10, 0.34, -0.04))
	_prism(root, Vector3(0.10, 0.08, 0.10), _toon(Color(0.26, 0.56, 0.36), 0.25), Vector3(0.10, 0.40, -0.04))
	_ball(root, 0.014, _glow(Color(1.0, 0.42, 0.42), 1.4), Vector3(0.10, 0.46, 0.0))                # tree-top star
	# a little snowman with a carrot nose + a top hat
	_ball(root, 0.035, snow, Vector3(0.07, 0.30, 0.10))
	_ball(root, 0.026, snow, Vector3(0.07, 0.355, 0.10))
	_cyl(root, 0.018, 0.022, 0.02, _toon(Color(0.10, 0.10, 0.12), 0.3), Vector3(0.07, 0.385, 0.10), Vector3.ZERO, 10)
	_ball(root, 0.008, _toon(Color(0.95, 0.5, 0.2), 0.3), Vector3(0.07, 0.355, 0.128), Vector3(0.6, 0.6, 1.6))  # nose
	# a glowing wrought lamppost
	_cyl(root, 0.006, 0.008, 0.10, _toon(Color(0.12, 0.12, 0.14), 0.3), Vector3(-0.13, 0.30, 0.10), Vector3.ZERO, 6)
	_ball(root, 0.018, _glow(Color(1.0, 0.85, 0.5), 1.8), Vector3(-0.13, 0.355, 0.10))
	# --- the glass dome + gold finial knob on top ---
	_ball(root, 0.21, _glass(Color(0.82, 0.93, 1.0), 0.16), Vector3(0, 0.36, 0), Vector3(1, 1.05, 1))
	_ball(root, 0.022, gold, Vector3(0, 0.585, 0))
	# drifting snow inside the globe
	_particles(root, Vector3(0, 0.52, 0), 30, 3.0, Vector3(0, -0.05, 0), 18.0, Vector3(0, -0.18, 0), 0.012, _glow(Color(1.0, 1.0, 1.0), 0.9), 0.17)
	return root


## 6 · GRAMOPHONE — a vintage phonograph: a great fluted brass horn, a spinning
##     record, a tonearm, a hand crank and drifting music notes.
##     Rare: a characterful antique — inlaid wood plinth, gold corner studs.
static func build_gramophone() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.42)
	var wood := _toon(Color(0.42, 0.26, 0.16), 0.28, true, 0.25)
	var wood_l := _toon(Color(0.55, 0.36, 0.22), 0.28, true, 0.25)
	var brass := _metal(Color(0.92, 0.72, 0.32), 0.85)
	var brass_d := _metal(Color(0.74, 0.55, 0.22), 0.7)
	var black := _toon(Color(0.10, 0.10, 0.12), 0.3, true, 0.4)
	# wooden plinth box with a beveled lid trim + an inlaid top panel
	_box(root, Vector3(0.46, 0.16, 0.44), wood, Vector3(0, 0.10, 0))
	_box(root, Vector3(0.50, 0.04, 0.48), wood_l, Vector3(0, 0.20, 0))
	_box(root, Vector3(0.38, 0.012, 0.36), brass_d, Vector3(0, 0.222, 0))            # brass inlay panel
	_box(root, Vector3(0.46, 0.04, 0.44), brass_d, Vector3(0, 0.225, 0))            # brass platter base
	# gold corner studs on the lid
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_ball(root, 0.018, brass, Vector3(sx * 0.225, 0.22, sz * 0.215))
	# turntable + a glossy black record with a label
	_cyl(root, 0.17, 0.17, 0.012, black, Vector3(0, 0.25, 0), Vector3.ZERO, 24)
	_torus(root, 0.10, 0.16, black, Vector3(0, 0.255, 0), Vector3(PI / 2.0, 0, 0), 24)   # groove ridge
	_cyl(root, 0.05, 0.05, 0.014, _toon(Color(0.86, 0.22, 0.24), 0.25), Vector3(0, 0.258, 0), Vector3.ZERO, 18)  # red label
	_cyl(root, 0.008, 0.008, 0.03, brass, Vector3(0, 0.26, 0), Vector3.ZERO, 6)     # spindle
	# tonearm: a slim brass arm reaching from a back pivot to the record
	_cyl(root, 0.018, 0.022, 0.05, brass_d, Vector3(0.18, 0.27, -0.14), Vector3.ZERO, 8)  # pivot post
	_cyl(root, 0.012, 0.012, 0.30, brass, Vector3(0.06, 0.30, -0.04), Vector3(0.5, 0, 0.9), 8)  # arm
	_ball(root, 0.022, black, Vector3(-0.05, 0.27, 0.04))                           # head
	# the great fluted horn — stacked widening cones to a bell, tilted up
	var horn := Node3D.new()
	horn.position = Vector3(-0.02, 0.34, 0.05)
	horn.rotation = Vector3(-0.6, 0.0, 0.0)
	root.add_child(horn)
	_cyl(horn, 0.05, 0.03, 0.12, brass_d, Vector3(0, 0.0, 0), Vector3.ZERO, 14)         # throat
	_cyl(horn, 0.12, 0.05, 0.16, brass, Vector3(0, 0.14, 0), Vector3.ZERO, 16)
	_cyl(horn, 0.22, 0.12, 0.18, brass, Vector3(0, 0.31, 0), Vector3.ZERO, 18)
	_cyl(horn, 0.34, 0.22, 0.14, brass, Vector3(0, 0.46, 0), Vector3.ZERO, 20)          # bell mouth
	_torus(horn, 0.32, 0.37, brass_d, Vector3(0, 0.53, 0), Vector3(0, 0, 0), 20)        # bell lip
	# raised brass flutes radiating up the bell (the "morning glory" look)
	for k in 8:
		var ang := TAU * float(k) / 8.0
		_box(horn, Vector3(0.02, 0.16, 0.02), brass_d, Vector3(cos(ang) * 0.24, 0.42, sin(ang) * 0.24), Vector3(0.3 * sin(ang), -ang, 0.3 * cos(ang)))
	# a soft warm glow inside the bell (it's "playing")
	_ball(horn, 0.16, _glow(Color(1.0, 0.82, 0.4), 0.5), Vector3(0, 0.5, 0), Vector3(1, 1, 0.3))
	# brass support strut from plinth to horn throat
	_cyl(root, 0.012, 0.012, 0.18, brass_d, Vector3(0.08, 0.30, -0.02), Vector3(0.4, 0, 0.5), 6)
	# the hand crank on the side
	_cyl(root, 0.012, 0.012, 0.10, brass, Vector3(0.25, 0.12, 0), Vector3(0, 0, PI / 2.0), 6)
	_cyl(root, 0.01, 0.01, 0.06, brass, Vector3(0.30, 0.16, 0), Vector3.ZERO, 6)
	_ball(root, 0.02, wood_l, Vector3(0.30, 0.20, 0))                               # crank knob
	# floating music notes (glow) drifting from the bell
	_ball(root, 0.03, _glow(Color(0.6, 0.85, 1.0), 1.2), Vector3(-0.28, 0.70, 0.20), Vector3(1, 1.6, 0.4))
	_cyl(root, 0.008, 0.008, 0.08, _glow(Color(0.6, 0.85, 1.0), 1.2), Vector3(-0.265, 0.74, 0.20), Vector3.ZERO, 5)  # note stem
	_ball(root, 0.024, _glow(Color(1.0, 0.7, 0.85), 1.2), Vector3(-0.38, 0.62, 0.28), Vector3(1, 1.5, 0.4))
	_ball(root, 0.020, _glow(Color(0.8, 1.0, 0.7), 1.2), Vector3(-0.20, 0.80, 0.14), Vector3(1, 1.5, 0.4))
	return root


## 7 · TELESCOPE — a polished brass refractor on a wooden tripod, aimed skyward,
##     with a star-map azimuth ring, finder scope, focus knobs and a counterweight.
##     Epic: an explorer's instrument trained on a tiny glowing constellation.
static func build_telescope() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.5)
	var wood := _toon(Color(0.40, 0.25, 0.15), 0.26, true, 0.25)
	var brass := _metal(Color(0.93, 0.73, 0.33), 0.85)
	var brass_d := _metal(Color(0.74, 0.55, 0.22), 0.7)
	var navy := _toon(Color(0.12, 0.16, 0.30), 0.3, true, 0.4)
	# --- wooden tripod: three splayed legs to a top collar ---
	var collar_y := 0.92
	for k in 3:
		var ang := TAU * float(k) / 3.0
		var fx := cos(ang)
		var fz := sin(ang)
		var leg := _cyl(root, 0.022, 0.03, 1.0, wood, Vector3(fx * 0.18, collar_y / 2.0, fz * 0.18), Vector3.ZERO, 8)
		leg.rotation = Vector3(fz * 0.34, 0, -fx * 0.34)
		_ball(root, 0.035, brass_d, Vector3(fx * 0.34, 0.03, fz * 0.34), Vector3(1, 0.6, 1))   # brass foot cap
		_cyl(root, 0.012, 0.012, 0.26, brass_d, Vector3(fx * 0.14, 0.34, fz * 0.14), Vector3(fz * 0.4, -ang, -fx * 0.4), 6)  # cross-brace
	# top collar / hub + a brass azimuth ring engraved with star marks
	_cyl(root, 0.07, 0.08, 0.08, brass, Vector3(0, collar_y, 0), Vector3.ZERO, 12)
	_torus(root, 0.10, 0.14, brass_d, Vector3(0, collar_y - 0.03, 0), Vector3(PI / 2.0, 0, 0), 16)   # azimuth ring
	for k in 12:
		var ang := TAU * float(k) / 12.0
		_ball(root, 0.01, brass, Vector3(cos(ang) * 0.12, collar_y - 0.03, sin(ang) * 0.12))
	_ball(root, 0.06, brass_d, Vector3(0, collar_y + 0.04, 0))     # the alt-az pivot ball
	# --- the optical tube: a long navy+brass barrel tilted up ---
	var tube := Node3D.new()
	tube.position = Vector3(0, collar_y + 0.06, 0)
	tube.rotation = Vector3(-0.85, 0.4, 0.0)
	root.add_child(tube)
	_cyl(tube, 0.07, 0.07, 0.62, navy, Vector3(0, 0.28, 0), Vector3.ZERO, 16)        # main barrel
	_torus(tube, 0.07, 0.10, brass, Vector3(0, 0.10, 0), Vector3(0, 0, 0), 16)        # rear ring
	_torus(tube, 0.07, 0.10, brass, Vector3(0, 0.28, 0), Vector3(0, 0, 0), 16)        # mid ring
	_torus(tube, 0.07, 0.10, brass, Vector3(0, 0.46, 0), Vector3(0, 0, 0), 16)        # front ring
	_cyl(tube, 0.085, 0.075, 0.10, brass, Vector3(0, 0.58, 0), Vector3.ZERO, 16)      # objective dew shield
	_cyl(tube, 0.075, 0.075, 0.02, _glass(Color(0.5, 0.8, 1.0), 0.55), Vector3(0, 0.64, 0), Vector3.ZERO, 16)  # lens
	_ball(tube, 0.02, _glow(Color(0.7, 0.9, 1.0), 1.2), Vector3(0, 0.64, 0))          # lens glint
	# eyepiece + draw tube at the rear
	_cyl(tube, 0.03, 0.035, 0.10, brass_d, Vector3(0, 0.0, 0), Vector3.ZERO, 12)
	_cyl(tube, 0.025, 0.025, 0.05, navy, Vector3(0, -0.07, 0), Vector3.ZERO, 10)
	# focus knobs
	for sx in [-1.0, 1.0]:
		_cyl(tube, 0.03, 0.03, 0.02, brass, Vector3(sx * 0.08, 0.06, 0), Vector3(0, 0, PI / 2.0), 10)
	# a small finder scope clamped on top
	_cyl(tube, 0.022, 0.022, 0.18, brass_d, Vector3(0.07, 0.34, 0), Vector3.ZERO, 8)
	_cyl(tube, 0.012, 0.012, 0.04, brass, Vector3(0.07, 0.46, 0), Vector3.ZERO, 8)
	# counterweight bar + brass weight
	_cyl(root, 0.014, 0.014, 0.24, brass_d, Vector3(0.0, collar_y + 0.02, -0.12), Vector3(0.7, 0, 0), 6)
	_ball(root, 0.05, brass, Vector3(0.0, collar_y - 0.10, -0.22))
	# the constellation it's pointed at — a little cluster of glows up the sight line
	_ball(root, 0.045, _glow(Color(0.8, 0.92, 1.0), 1.9), Vector3(0.42, 1.96, 0.5))
	_ball(root, 0.022, _glow(Color(1.0, 0.95, 0.7), 1.6), Vector3(0.30, 1.74, 0.62))
	_ball(root, 0.018, _glow(Color(0.7, 0.85, 1.0), 1.6), Vector3(0.52, 1.78, 0.40))
	_ball(root, 0.014, _glow(Color(1.0, 0.8, 0.9), 1.5), Vector3(0.38, 2.10, 0.55))
	return root


## 8 · AQUARIUM — a lit reef tank on a wood cabinet: gravel, coral, waving
##     plants, a castle ornament, darting neon fish, a bubble curtain and a hood.
##     Epic: a living tabletop — gradient water, glowing fish, rising bubbles.
static func build_aquarium() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.55)
	var wood := _toon(Color(0.30, 0.22, 0.16), 0.28, true, 0.25)
	var wood_l := _toon(Color(0.42, 0.30, 0.20), 0.28, true, 0.25)
	var frame := _metal(Color(0.62, 0.65, 0.70), 0.6)
	var water := _glass(Color(0.30, 0.70, 0.92), 0.30)
	var gravel := _toon(Color(0.70, 0.62, 0.46), 0.2)
	var w := 0.9
	var h := 0.5
	var d := 0.42
	# wooden stand / cabinet with door panels + brass handles
	_box(root, Vector3(w + 0.06, 0.34, d + 0.06), wood, Vector3(0, 0.17, 0))
	_box(root, Vector3(w + 0.12, 0.05, d + 0.12), wood_l, Vector3(0, 0.36, 0))     # top trim
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.30, 0.26, 0.02), wood_l, Vector3(sx * 0.20, 0.16, d / 2.0 + 0.035))  # door
		_ball(root, 0.012, frame, Vector3(sx * 0.06, 0.16, d / 2.0 + 0.05))                        # handle
	# the glass tank body
	var tank_y := 0.36 + h / 2.0
	_box(root, Vector3(w, h, d), water, Vector3(0, tank_y, 0))
	# slim frame edges (verticals + top/bottom rails)
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_box(root, Vector3(0.03, h, 0.03), frame, Vector3(sx * w / 2.0, tank_y, sz * d / 2.0))
	_box(root, Vector3(w + 0.02, 0.05, d + 0.02), frame, Vector3(0, tank_y + h / 2.0, 0))   # top rim (hood)
	_box(root, Vector3(0.10, 0.02, 0.06), frame, Vector3(0, tank_y + h / 2.0 + 0.03, 0))     # hood handle
	_box(root, Vector3(w + 0.02, 0.04, d + 0.02), frame, Vector3(0, tank_y - h / 2.0, 0))   # base rim
	# gravel bed + a few pebbles
	_box(root, Vector3(w - 0.04, 0.06, d - 0.04), gravel, Vector3(0, 0.40, 0))
	for i in 6:
		_ball(root, 0.03, _toon(Color(0.5, 0.5, 0.55), 0.2), Vector3(-0.32 + i * 0.13, 0.44, -0.06 + (i % 2) * 0.12), Vector3(1.2, 0.7, 1))
	# a little stone castle ornament
	_box(root, Vector3(0.10, 0.14, 0.08), _toon(Color(0.6, 0.6, 0.64), 0.25), Vector3(-0.30, 0.50, 0.0))
	for sx in [-1.0, 1.0]:
		_cyl(root, 0.03, 0.035, 0.16, _toon(Color(0.6, 0.6, 0.64), 0.25), Vector3(-0.30 + sx * 0.06, 0.52, 0.0), Vector3.ZERO, 10)
		_prism(root, Vector3(0.06, 0.05, 0.06), _toon(Color(0.7, 0.32, 0.30), 0.25), Vector3(-0.30 + sx * 0.06, 0.61, 0.0))
	# branching coral + waving plants
	_ball(root, 0.07, _toon(Color(0.95, 0.45, 0.55), 0.3), Vector3(-0.10, 0.50, 0.06), Vector3(0.8, 1.4, 0.8))   # coral
	_ball(root, 0.05, _toon(Color(0.98, 0.62, 0.40), 0.3), Vector3(-0.04, 0.52, 0.02), Vector3(0.8, 1.3, 0.8))
	for k in 5:
		_capsule(root, 0.018, 0.20 + 0.06 * k, _toon(Color(0.28, 0.66, 0.34), 0.25), Vector3(0.16 + k * 0.04, 0.50 + 0.03 * k, -0.06), Vector3(0.12 * (k - 2), 0, 0.1 * (k - 2)))
	# a treasure-chest trinket with a gold glow
	_box(root, Vector3(0.1, 0.06, 0.07), wood, Vector3(0.0, 0.46, 0.10))
	_ball(root, 0.025, _glow(Color(1.0, 0.85, 0.4), 1.4), Vector3(0.0, 0.49, 0.10))
	# darting neon fish (glow so they pop through the glass) with tails + eyes
	var fish_cols := [Color(1.0, 0.6, 0.2), Color(0.4, 0.8, 1.0), Color(1.0, 0.85, 0.3), Color(0.9, 0.4, 0.9), Color(0.5, 1.0, 0.6)]
	var fish_pos := [Vector3(-0.2, 0.62, 0.05), Vector3(0.15, 0.70, -0.04), Vector3(0.25, 0.56, 0.06), Vector3(-0.1, 0.74, -0.02), Vector3(0.32, 0.66, 0.0)]
	for k in 5:
		var fm := _glow(fish_cols[k], 1.3)
		_ball(root, 0.04, fm, fish_pos[k], Vector3(1.5, 0.9, 0.7))     # body
		_box(root, Vector3(0.04, 0.05, 0.012), fm, fish_pos[k] + Vector3(0.055, 0, 0), Vector3(0, 0, 0.4))  # tail
		_ball(root, 0.008, _glow(Color(0.05, 0.05, 0.08), 0.0), fish_pos[k] + Vector3(-0.035, 0.012, 0.02))  # eye
	# a bubble curtain wand at the back
	_cyl(root, 0.01, 0.01, w - 0.1, frame, Vector3(0, 0.42, -d / 2.0 + 0.06), Vector3(0, 0, PI / 2.0), 8)
	# rising bubbles (two streams)
	_particles(root, Vector3(0.3, 0.42, -0.06), 16, 1.8, Vector3(0, 0.15, 0), 8.0, Vector3(0, 0.4, 0), 0.014, _glass(Color(0.9, 0.97, 1.0), 0.5))
	_particles(root, Vector3(-0.25, 0.42, -0.10), 12, 1.8, Vector3(0, 0.13, 0), 8.0, Vector3(0, 0.4, 0), 0.012, _glass(Color(0.9, 0.97, 1.0), 0.5))
	# soft hood light glowing down into the water
	_light(root, Color(0.5, 0.85, 1.0), 1.0, 1.6, Vector3(0, tank_y + 0.1, 0))
	return root


## 9 · FLOATING CRYSTAL — a slow-hovering arcane gem ringed by orbiting shards,
##     anchored to a runed stone pedestal by a beam of energy.
##     Legendary: pure spectacle — a faceted core, glow halo, runic ring,
##     floating glyphs and rising motes.
static func build_crystal() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.4)
	var stone := _toon(Color(0.22, 0.20, 0.30), 0.3, true, 0.3)
	var stone_l := _toon(Color(0.32, 0.30, 0.42), 0.3, true, 0.3)
	var rune := _glow(Color(0.55, 0.85, 1.0), 1.6)
	var crystal_c := Color(0.55, 0.45, 0.95)
	# --- a stone pedestal with glowing runes ---
	_cyl(root, 0.28, 0.32, 0.08, stone, Vector3(0, 0.04, 0), Vector3.ZERO, 8)
	_torus(root, 0.26, 0.32, stone_l, Vector3(0, 0.08, 0), Vector3(PI / 2.0, 0, 0), 8)
	_cyl(root, 0.20, 0.26, 0.10, stone, Vector3(0, 0.14, 0), Vector3.ZERO, 8)
	_torus(root, 0.18, 0.24, rune, Vector3(0, 0.19, 0), Vector3(PI / 2.0, 0, 0), 8)     # rune ring
	# little glowing rune marks set into the column faces
	for k in 6:
		var ang := TAU * float(k) / 6.0
		_box(root, Vector3(0.02, 0.05, 0.01), rune, Vector3(cos(ang) * 0.215, 0.14, sin(ang) * 0.215), Vector3(0, -ang, 0))
	_cyl(root, 0.14, 0.18, 0.06, stone, Vector3(0, 0.22, 0), Vector3.ZERO, 8)
	# --- the floating crystal: two prisms tip-to-tip = a faceted gem ---
	var gem := _glass(crystal_c, 0.6)
	var core_y := 0.92
	_prism(root, Vector3(0.28, 0.46, 0.28), gem, Vector3(0, core_y + 0.06, 0))               # upper point
	_prism(root, Vector3(0.28, 0.34, 0.28), gem, Vector3(0, core_y - 0.32, 0), Vector3(PI, 0, 0))  # lower point
	# bright facet edges (thin glowing struts up the upper point)
	for k in 4:
		var ang := TAU * float(k) / 4.0
		_cyl(root, 0.006, 0.006, 0.40, _glow(crystal_c.lightened(0.3), 1.4), Vector3(cos(ang) * 0.07, core_y + 0.10, sin(ang) * 0.07), Vector3(0.18 * sin(ang), 0, -0.18 * cos(ang)), 4)
	# a bright glowing inner core
	_ball(root, 0.08, _glow(Color(0.85, 0.75, 1.0), 2.2), Vector3(0, core_y, 0))
	# soft outer halo (big faint transparent sphere)
	_ball(root, 0.34, _glass(Color(0.7, 0.6, 1.0), 0.10), Vector3(0, core_y, 0))
	# --- a ring of smaller orbiting shards around the core ---
	for k in 8:
		var ang := TAU * float(k) / 8.0
		var sx := cos(ang) * 0.36
		var sz := sin(ang) * 0.36
		var sh := _prism(root, Vector3(0.07, 0.17, 0.07), _glass(crystal_c.lightened(0.1), 0.65), Vector3(sx, core_y + sin(ang * 2.0) * 0.07, sz))
		sh.rotation = Vector3(0.4, ang, 0.3)
	# floating glowing rune glyphs hovering near the gem
	for k in 3:
		var ang := TAU * float(k) / 3.0 + 0.4
		_box(root, Vector3(0.05, 0.05, 0.01), _glow(Color(0.7, 0.9, 1.0), 1.6), Vector3(cos(ang) * 0.48, core_y + 0.18, sin(ang) * 0.48), Vector3(0, -ang, PI / 4.0))
	# a thin glowing energy beam down to the pedestal
	_cyl(root, 0.02, 0.05, 0.40, _glow(Color(0.6, 0.5, 1.0), 1.1), Vector3(0, 0.46, 0), Vector3.ZERO, 8)
	# the arcane light it casts
	_light(root, crystal_c, 1.7, 3.2, Vector3(0, core_y, 0))
	# rising motes of energy
	_particles(root, Vector3(0, 0.5, 0), 22, 2.6, Vector3(0, 0.22, 0), 30.0, Vector3(0, 0.25, 0), 0.02, _glow(Color(0.78, 0.72, 1.0), 1.6), 0.3)
	return root


## 10 · BALLOON BUNCH — a cheery cluster of glossy helium balloons (one a gold
##      foil STAR) on curled ribbons, tied to a wrapped gift box, with confetti.
##      Uncommon: instant party — jewel-tone sheen, highlight glints, a bow.
static func build_balloons() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.28)
	# a wrapped gift-box anchor weight
	var box_c := _toon(Color(0.92, 0.42, 0.50), 0.3, true, 0.3)
	var ribbon := _metal(Color(0.95, 0.82, 0.40), 0.5)
	_box(root, Vector3(0.22, 0.20, 0.22), box_c, Vector3(0, 0.10, 0))
	_box(root, Vector3(0.235, 0.04, 0.06), ribbon, Vector3(0, 0.10, 0))
	_box(root, Vector3(0.06, 0.04, 0.235), ribbon, Vector3(0, 0.10, 0))
	_ball(root, 0.04, ribbon, Vector3(0, 0.21, 0), Vector3(1.4, 0.8, 1.4))         # bow knot
	for sx in [-1.0, 1.0]:
		_ball(root, 0.05, ribbon, Vector3(sx * 0.05, 0.23, 0), Vector3(0.9, 1.3, 0.6))  # bow loops
	# --- the balloons: glossy jewel-tone spheres at varied heights ---
	var cols := [
		Color(0.92, 0.26, 0.32),   # cherry red
		Color(0.25, 0.55, 0.95),   # bright blue
		Color(0.98, 0.78, 0.28),   # sunny yellow (the gold foil star)
		Color(0.45, 0.80, 0.45),   # leaf green
		Color(0.80, 0.45, 0.92),   # orchid purple
	]
	var offs := [
		Vector3(0.0, 1.15, 0.0),
		Vector3(-0.26, 1.0, 0.08),
		Vector3(0.26, 1.02, -0.06),
		Vector3(-0.14, 1.28, -0.12),
		Vector3(0.18, 1.30, 0.12),
	]
	for k in 5:
		var c: Color = cols[k]
		var p: Vector3 = offs[k]
		if k == 2:
			# a GOLD FOIL STAR balloon — five glossy points + a glowing center
			var foil := _metal(Color(0.99, 0.82, 0.30), 0.95)
			_ball(root, 0.09, foil, p, Vector3(1.0, 1.0, 0.45))                       # star hub
			for s in 5:
				var ang := TAU * float(s) / 5.0 + PI / 2.0
				_prism(root, Vector3(0.10, 0.18, 0.05), foil, p + Vector3(cos(ang) * 0.11, sin(ang) * 0.11, 0), Vector3(0, 0, ang - PI / 2.0))
			_ball(root, 0.05, _glow(Color(1.0, 0.95, 0.6), 0.9), p)                   # foil sheen
			_cyl(root, 0.015, 0.03, 0.04, foil, p + Vector3(0, -0.205, 0), Vector3.ZERO, 6)  # neck tab
		else:
			var glossy := _toon(c, 0.5, true, 0.85)      # high spec = balloon sheen
			_ball(root, 0.16, glossy, p, Vector3(1.0, 1.15, 1.0))      # teardrop balloon
			_cyl(root, 0.015, 0.03, 0.04, glossy, p + Vector3(0, -0.185, 0), Vector3.ZERO, 6)  # knot
			_ball(root, 0.04, _glow(c.lightened(0.6), 0.6), p + Vector3(-0.05, 0.06, 0.12))    # highlight glint
		# a curling ribbon from the knot down to the box (segmented arc)
		var rmat := _toon(c.darkened(0.1), 0.2)
		var prev := p + Vector3(0, -0.22, 0)
		var anchor := Vector3(0, 0.22, 0)
		for s in 4:
			var t := float(s + 1) / 4.0
			var pt := prev.lerp(anchor, 1.0 / float(4 - s)) if s < 3 else anchor
			pt.x += sin(t * PI * 3.0 + k) * 0.03   # the curl
			var mid := (prev + pt) * 0.5
			var dir := pt - prev
			var seg_len := dir.length()
			var seg := _cyl(root, 0.006, 0.006, max(seg_len, 0.01), rmat, mid, Vector3.ZERO, 5)
			seg.look_at_from_position(mid, mid + dir, Vector3.UP)
			seg.rotate_object_local(Vector3(1, 0, 0), PI / 2.0)
			prev = pt
	# drifting confetti motes for that party pop
	_particles(root, Vector3(0, 1.3, 0), 12, 2.4, Vector3(0, 0.05, 0), 40.0, Vector3(0, -0.12, 0), 0.012, _glow(Color(1.0, 0.85, 0.5), 1.2), 0.3)
	return root


## 11 · TROPHY — a gleaming champions' cup on a tiered obsidian base, with
##      looping handles, a laurel wreath, gem-set rim and a bursting glow star.
##      Rare: bragging rights — gold cup, engraved plaque, victory sparkle.
static func build_trophy() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.3)
	var gold := _metal(Color(0.96, 0.80, 0.34), 0.9)
	var gold_d := _metal(Color(0.80, 0.62, 0.22), 0.75)
	var marble := _toon(Color(0.18, 0.20, 0.26), 0.3, true, 0.4)     # dark obsidian base
	var marble_l := _toon(Color(0.28, 0.30, 0.38), 0.3, true, 0.35)
	var emerald := _gem(Color(0.25, 0.85, 0.55))
	# --- stacked base: two obsidian tiers + a gold plaque + gem corners ---
	_box(root, Vector3(0.40, 0.08, 0.34), marble, Vector3(0, 0.04, 0))
	_box(root, Vector3(0.44, 0.03, 0.38), gold_d, Vector3(0, 0.095, 0))            # gold base trim
	_box(root, Vector3(0.30, 0.10, 0.26), marble_l, Vector3(0, 0.155, 0))
	_box(root, Vector3(0.22, 0.05, 0.02), gold, Vector3(0, 0.13, 0.135))           # engraved plaque
	for sx in [-1.0, 1.0]:
		_jewel(root, 0.035, 0.05, emerald, Vector3(sx * 0.16, 0.10, 0.14))         # gem accents
	# gold pedestal stem rising from the base
	_cyl(root, 0.05, 0.08, 0.10, gold_d, Vector3(0, 0.255, 0), Vector3.ZERO, 14)
	_ball(root, 0.06, gold, Vector3(0, 0.335, 0), Vector3(1, 0.7, 1))               # knop
	_cyl(root, 0.035, 0.04, 0.10, gold, Vector3(0, 0.425, 0), Vector3.ZERO, 12)     # stem
	# --- the cup bowl: a wide flaring goblet ---
	_cyl(root, 0.22, 0.10, 0.26, gold, Vector3(0, 0.60, 0), Vector3.ZERO, 20)       # bowl
	_torus(root, 0.20, 0.24, gold_d, Vector3(0, 0.72, 0), Vector3(PI / 2.0, 0, 0), 18)  # rim
	_cyl(root, 0.21, 0.21, 0.03, gold_d, Vector3(0, 0.50, 0), Vector3.ZERO, 20)     # bottom of bowl
	# a gem set into the front of the cup
	_jewel(root, 0.05, 0.07, emerald, Vector3(0, 0.58, 0.20), Vector3(0.3, 0, 0))
	# --- two looping handles ---
	for sx in [-1.0, 1.0]:
		var hdl := _torus(root, 0.06, 0.11, gold, Vector3(sx * 0.24, 0.62, 0), Vector3(0, PI / 2.0, 0), 12)
		hdl.scale = Vector3(1.0, 1.3, 0.6)
	# --- a laurel wreath hugging the cup base ---
	for sx in [-1.0, 1.0]:
		for li in 5:
			var t := float(li) / 4.0
			_ball(root, 0.022, _toon(Color(0.86, 0.72, 0.30), 0.4, true, 0.6), Vector3(sx * (0.14 + t * 0.10), 0.50 + t * 0.16, 0.10 + 0.02 * li), Vector3(1.6, 0.5, 0.5))
	# --- a glowing star bursting from the cup ---
	_ball(root, 0.05, _glow(Color(1.0, 0.92, 0.5), 1.9), Vector3(0, 0.80, 0))
	for k in 5:
		var ang := TAU * float(k) / 5.0 + PI / 2.0
		_prism(root, Vector3(0.05, 0.12, 0.04), _glow(Color(1.0, 0.88, 0.45), 1.7), Vector3(cos(ang) * 0.07, 0.80 + sin(ang) * 0.07, 0), Vector3(0, 0, ang - PI / 2.0))
	# a victorious warm glow + rising sparkle confetti
	_light(root, Color(1.0, 0.86, 0.45), 0.9, 1.9, Vector3(0, 0.72, 0))
	_particles(root, Vector3(0, 0.80, 0), 14, 2.0, Vector3(0, 0.25, 0), 35.0, Vector3(0, -0.1, 0), 0.018, _glow(Color(1.0, 0.9, 0.5), 1.6), 0.18)
	return root
