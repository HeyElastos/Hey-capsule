class_name VerseCatalogLighting
extends RefCounted
## Hey Verse — LIGHTING catalog (premium, procedural, toon-styled, avatar-scaled).
##
## A SELLABLE showroom set of glowing fixtures for the ~1.4-unit chibi-robot
## world. Every piece is a RICH composite of primitives (dozens of parts) with a
## strong silhouette, cohesive premium materials and a readable rarity tier — no
## blocky placeholders. The set (9 items):
##
##   build_art_deco_lamp     · art-deco floor lamp     (Rare)
##   build_lantern_string    · paper lantern string    (Uncommon)
##   build_chandelier        · crystal chandelier      (Legendary)
##   build_neon_sign         · neon "HEY" sign         (Epic)
##   build_lava_lamp         · lava lamp               (Uncommon)
##   build_campfire          · campfire / firepit      (Common)
##   build_fairy_jar         · fairy-jar (jar of stars)(Rare)
##   build_street_lamp       · vintage street lamp     (Common)
##   build_mushroom_lamp     · bioluminescent mushroom (Epic)
##
## Each `build_<id>() -> Node3D` returns ONE self-contained Node3D built at the
## ORIGIN, resting on the floor plane y=0 (a lamp's base sits at y=0; light glows
## from its bulb). Each builder is standalone: it re-declares its own tiny
## material/mesh helpers and pulls only the shared toon + outline shaders for the
## look — no home.gd / avatar.gd internals, no .glb, no external art. If the
## shaders are ever missing (e.g. parsing this file outside the project) the
## material helpers fall back to a plain toon-ish StandardMaterial3D, so the
## module never hard-fails.
##
## Look + materials (this is what makes them premium):
##  - solids   : toon cel material + inverted-hull outline (_toon), with proper
##               metallic / roughness on the PBR-ish accents (brass/chrome/gold)
##  - metals   : _metal — gold / brass / chrome with real metallic + low roughness
##  - glow     : unshaded emissive StandardMaterial3D (_glow) — bulbs, neon, lava,
##               embers, gems, fireflies; the only thing that should HALO
##  - glass    : translucent faintly-emissive shell (_shell) — shades, jars, paper
##  - real light: one (sometimes two) cheap OmniLight3D inside the fixture (_light)
##
## RARITY is expressed VISUALLY: higher tiers add gold trim, faceted gemstones,
## stronger emission, crystal drops and gentle particles. Mobile budget: low
## segment counts, ≤2 OmniLights per item, glow/glass cast no shadow, particles
## only where they sell the effect (flame, embers, fireflies, lava drift).

const TOON_SHADER_PATH := "res://toon.gdshader"
const OUTLINE_SHADER_PATH := "res://outline.gdshader"

static var _toon_shader: Shader
static var _outline_mat: ShaderMaterial


# ───────────────────────────── shared helpers (self-contained) ──────────────

## Cel material + inverted-hull outline — the Verse "designed" look on solids.
## Falls back to a plain toon-ish StandardMaterial3D if the shaders are missing.
static func _toon(c: Color, rim := 0.3, outline := true, spec := 0.0) -> Material:
	if _toon_shader == null and ResourceLoader.exists(TOON_SHADER_PATH):
		_toon_shader = load(TOON_SHADER_PATH)
	if _toon_shader == null:
		var sm := StandardMaterial3D.new()
		sm.albedo_color = c
		sm.roughness = 0.9
		sm.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
		sm.specular_mode = BaseMaterial3D.SPECULAR_DISABLED
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


## A real metal — gold / brass / chrome with PBR metallic + low roughness so it
## catches a bright spec highlight (premium trim). Toon-outlined for cohesion.
## Kept as a StandardMaterial3D (the cel shader has no metallic channel) but with
## a toon diffuse so it sits beside the cel surfaces happily.
static func _metal(c: Color, rough := 0.18, metallic := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	m.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	# a faint warm self-tint so metals read rich even in flat ambient
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.05
	return m


## Unshaded emissive — the glow surfaces (bulbs, neon, lava, embers, gems).
static func _glow(c: Color, energy := 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


## A faceted gem — emissive + a touch of metallic sheen, reads as cut crystal.
static func _gem(c: Color, energy := 1.6) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.4
	m.roughness = 0.05
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	return m


## Clear faceted crystal (chandelier drops) — glassy, lightly tinted, glints.
static func _crystal(tint := Color(0.86, 0.92, 1.0)) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(tint.r, tint.g, tint.b, 0.45)
	m.metallic = 0.6
	m.roughness = 0.04
	m.emission_enabled = true
	m.emission = tint
	m.emission_energy_multiplier = 0.5
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	return m


## Soft translucent shell — lamp glass, lantern paper, frosted globes, jars. No
## shadow casting (it would punch holes in the glow).
static func _shell(c: Color, alpha := 0.32, glow := 0.6) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.roughness = 0.18
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = glow
	return m


static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, no_shadow := false) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, seg := 14, no_shadow := false) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _sphere(parent: Node3D, r: float, mat: Material, pos: Vector3, sc := Vector3.ONE, seg := 16, rings := 8, no_shadow := false) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = seg
	sm.rings = rings
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = sc
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


## A hemisphere (dome / bowl shade / mushroom cap). `up` false = bowl opening up.
static func _dome(parent: Node3D, r: float, mat: Material, pos: Vector3, sc := Vector3.ONE, seg := 16, no_shadow := false) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.is_hemisphere = true
	sm.radial_segments = seg
	sm.rings = 7
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = sc
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, seg := 18, no_shadow := false) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = seg
	tm.ring_segments = 8
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


## A faceted octahedron-ish gem cut from two cones (cheap, reads as cut crystal).
static func _facet(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, seg := 6, no_shadow := true) -> Node3D:
	var node := Node3D.new()
	node.position = pos
	# top pyramid
	var top := CylinderMesh.new()
	top.top_radius = 0.0
	top.bottom_radius = r
	top.height = h * 0.55
	top.radial_segments = seg
	var tmi := MeshInstance3D.new()
	tmi.mesh = top
	tmi.material_override = mat
	tmi.position.y = h * 0.275
	# bottom pyramid (point down)
	var bot := CylinderMesh.new()
	bot.top_radius = r
	bot.bottom_radius = 0.0
	bot.height = h * 0.45
	bot.radial_segments = seg
	var bmi := MeshInstance3D.new()
	bmi.mesh = bot
	bmi.material_override = mat
	bmi.position.y = -h * 0.225
	if no_shadow:
		tmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		bmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	node.add_child(tmi)
	node.add_child(bmi)
	parent.add_child(node)
	return node


## A small warm point light inside a fixture. Cheap: modest range, no shadow.
static func _light(parent: Node3D, pos: Vector3, color: Color, energy: float, rng: float) -> OmniLight3D:
	var o := OmniLight3D.new()
	o.position = pos
	o.light_color = color
	o.light_energy = energy
	o.omni_range = rng
	o.shadow_enabled = false
	parent.add_child(o)
	return o


## A drifting-up particle puff (embers, fireflies, sparkle, lava motes).
static func _motes(parent: Node3D, pos: Vector3, amount: int, life: float, radius: float, rise: float, mat: StandardMaterial3D, spread := 20.0, dot_r := 0.016) -> CPUParticles3D:
	var p := CPUParticles3D.new()
	p.position = pos
	p.amount = amount
	p.lifetime = life
	p.preprocess = life
	p.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	p.emission_sphere_radius = radius
	p.direction = Vector3(0, 1, 0)
	p.spread = spread
	p.gravity = Vector3(0, rise, 0)
	p.initial_velocity_min = rise * 0.4
	p.initial_velocity_max = rise * 1.0
	p.scale_amount_min = 0.4
	p.scale_amount_max = 1.0
	var dot := SphereMesh.new()
	dot.radius = dot_r
	dot.height = dot_r * 2.0
	dot.radial_segments = 5
	dot.rings = 2
	dot.material = mat
	p.mesh = dot
	parent.add_child(p)
	return p


## A short glowing tube segment between two 2D points on a backboard at depth z
## (neon glass). Returns the bright core; the outer halo shell is added too.
static func _tube2d(parent: Node3D, a: Vector2, b: Vector2, z: float, core: StandardMaterial3D, halo: StandardMaterial3D, r := 0.022) -> void:
	var pa := Vector3(a.x, a.y, z)
	var pb := Vector3(b.x, b.y, z)
	var mid := (pa + pb) * 0.5
	var seg_len := pa.distance_to(pb)
	var ang := atan2((pb - pa).y, (pb - pa).x) - PI / 2.0
	# soft outer halo (fatter, dimmer) then the bright core
	_cyl(parent, r * 1.9, r * 1.9, seg_len + r, halo, mid, 8, true).rotation.z = ang
	_cyl(parent, r, r, seg_len + r * 1.4, core, mid, 9, true).rotation.z = ang
	# rounded glass end caps so the strokes read continuous (no cut tube ends)
	_sphere(parent, r, core, pa, Vector3.ONE, 7, 4, true)
	_sphere(parent, r, core, pb, Vector3.ONE, 7, 4, true)


# ════════════════════════════════════════════════════════════════════════════
#  THE CATALOG  —  one self-contained premium Node3D per item, at the origin,
#  resting on the floor y=0 (lamps / fire / jar) or hung from y≈0 down (string,
#  chandelier — caller mounts the top at ceiling height).
# ════════════════════════════════════════════════════════════════════════════


## ART-DECO FLOOR LAMP (Rare) — a Gatsby-grade torchiere: a stepped black-marble
## ziggurat base with inlaid jade cabochons, a fluted brass column with gold
## reeded ribs and collar rings, a sunburst fan-shade of stacked golden tiers
## with splaying rays, an alabaster glowing globe, a faceted finial gem and a
## little pull-chain. Tall, glamorous, symmetrical. ~1.9 tall.
static func build_art_deco_lamp() -> Node3D:
	var root := Node3D.new()
	var marble := _toon(Color(0.10, 0.11, 0.15), 0.25, true, 0.5)
	var brass := _metal(Color(0.86, 0.66, 0.30), 0.16)
	var brass_dk := _metal(Color(0.66, 0.48, 0.20), 0.22)
	var alabaster := _shell(Color(1.0, 0.92, 0.74), 0.55, 0.9)
	var jade := _gem(Color(0.30, 0.86, 0.62), 1.2)
	# stepped ziggurat base (3 tiers of black marble) + gold inlay rings
	_cyl(root, 0.20, 0.24, 0.06, marble, Vector3(0, 0.03, 0), 22)
	_cyl(root, 0.155, 0.18, 0.05, marble, Vector3(0, 0.085, 0), 22)
	_cyl(root, 0.12, 0.14, 0.05, marble, Vector3(0, 0.135, 0), 22)
	_torus(root, 0.175, 0.195, brass, Vector3(0, 0.065, 0), 24)
	_torus(root, 0.13, 0.15, brass, Vector3(0, 0.115, 0), 24)
	# four jade cabochons inlaid around the base (Rare gemstone accent)
	for k in 4:
		var ja := TAU * float(k) / 4.0 + PI / 4.0
		_sphere(root, 0.022, jade, Vector3(cos(ja) * 0.205, 0.055, sin(ja) * 0.205), Vector3(1, 0.6, 1), 8, 4, true)
	# fluted brass column (reeded — vertical ribs) + gold collar rings
	_cyl(root, 0.045, 0.06, 1.06, brass, Vector3(0, 0.69, 0), 16)
	for k in 8:
		var ang := TAU * float(k) / 8.0
		_cyl(root, 0.009, 0.009, 1.0, brass_dk, Vector3(cos(ang) * 0.052, 0.69, sin(ang) * 0.052), 4)
	_torus(root, 0.052, 0.072, brass, Vector3(0, 0.30, 0), 20)
	_torus(root, 0.046, 0.066, brass, Vector3(0, 0.96, 0), 20)
	# the sunburst fan-shade: stacked golden tiers fanning OUT and up (the deco look)
	var radii := [0.16, 0.205, 0.25, 0.295, 0.34]
	var hy := [1.30, 1.38, 1.47, 1.57, 1.68]
	for i in radii.size():
		var tier := _cyl(root, radii[i] + 0.01, radii[i] - 0.04, 0.045, brass if i % 2 == 0 else brass_dk, Vector3(0, hy[i], 0), 28)
		tier.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# vertical fan ribs splaying up the shade (sunburst rays)
	for k in 16:
		var ang2 := TAU * float(k) / 16.0
		var rib := _box(root, Vector3(0.012, 0.40, 0.014), brass_dk, Vector3(cos(ang2) * 0.22, 1.48, sin(ang2) * 0.22), true)
		rib.rotation.y = -ang2
		rib.rotation.z = -0.16
	# the alabaster glowing globe nested in the fan
	_sphere(root, 0.135, alabaster, Vector3(0, 1.46, 0), Vector3(1.0, 1.05, 1.0), 18, 9, true)
	_sphere(root, 0.10, _glow(Color(1.0, 0.90, 0.66), 1.6), Vector3(0, 1.46, 0), Vector3.ONE, 10, 5, true)
	# crowning faceted gem finial
	_facet(root, 0.045, 0.10, _gem(Color(1.0, 0.86, 0.5), 1.2), Vector3(0, 1.78, 0), 6)
	# little pull-chain with a teardrop pull
	_cyl(root, 0.004, 0.004, 0.16, brass_dk, Vector3(0.16, 1.18, 0.0), 5)
	_sphere(root, 0.018, brass, Vector3(0.16, 1.10, 0.0), Vector3(1, 1.4, 1), 8, 4)
	_light(root, Vector3(0, 1.5, 0), Color(1.0, 0.86, 0.6), 2.0, 7.0)
	return root


## PAPER LANTERN STRING (Uncommon) — a gentle catenary cord strung with five
## pleated rice-paper lanterns in candy festival colors, each ribbed with bamboo
## hoops, capped with little gold crowns top and bottom, hung on a brass loop and
## finished with a swaying tassel, glowing from within. Hangs from y≈0 (mount the
## cord ends at two posts/ceiling). Sag ~0.6 deep, ~2.4 wide.
static func build_lantern_string() -> Node3D:
	var root := Node3D.new()
	var cord_mat := _toon(Color(0.18, 0.15, 0.12), 0.15)
	var cap_mat := _toon(Color(0.55, 0.38, 0.24), 0.2)
	var gold := _metal(Color(0.88, 0.70, 0.34), 0.2)
	# the catenary cord — a series of short segments dipping then rising
	var span := 2.4
	var sag := 0.55
	var n := 22
	var prev: Vector3 = Vector3(-span * 0.5, 0.0, 0.0)
	for i in range(1, n + 1):
		var t := float(i) / float(n)
		var x := -span * 0.5 + span * t
		# catenary-ish: parabola dip
		var y := -sag * 4.0 * t * (1.0 - t)
		var cur := Vector3(x, y, 0.0)
		var mid := (prev + cur) * 0.5
		var seg := _cyl(root, 0.01, 0.01, prev.distance_to(cur) + 0.01, cord_mat, mid, 5)
		var dir := (cur - prev).normalized()
		seg.rotation.z = atan2(dir.y, dir.x) - PI / 2.0
		prev = cur
	# tiny pennant flags fluttering between the lanterns (festival bunting)
	var flag_cols := [Color(1.0, 0.5, 0.45), Color(0.55, 0.82, 0.62), Color(0.66, 0.62, 0.95), Color(1.0, 0.78, 0.36)]
	for i in 5:
		var t := float(i) / 5.0 + 0.1
		var fx := -span * 0.5 + span * t
		var fy := -sag * 4.0 * t * (1.0 - t)
		var fl := _box(root, Vector3(0.05, 0.07, 0.006), _shell(flag_cols[i % 4], 0.75, 0.4), Vector3(fx, fy - 0.05, 0.0), true)
		fl.rotation.z = 0.18
	# five lanterns hung along the dip, each a different festive color
	var cols := [
		Color(1.0, 0.42, 0.40),   # coral red
		Color(1.0, 0.74, 0.34),   # amber
		Color(1.0, 0.92, 0.55),   # warm yellow
		Color(0.55, 0.82, 0.62),  # jade
		Color(0.66, 0.62, 0.95),  # lilac
	]
	for i in 5:
		var t := (float(i) + 0.5) / 5.0
		var x := -span * 0.5 + span * t
		var y := -sag * 4.0 * t * (1.0 - t)
		var c: Color = cols[i]
		var hub := Node3D.new()
		# the lantern hangs BELOW the cord; the wire connects up to the cord
		hub.position = Vector3(x, y - 0.24, 0.0)
		root.add_child(hub)
		# drop wire up to the cord + a little brass hanging loop
		_cyl(hub, 0.005, 0.005, 0.10, cord_mat, Vector3(0, 0.19, 0), 4)
		_torus(hub, 0.012, 0.022, gold, Vector3(0, 0.155, 0), 10)
		# the pleated paper globe (squashed) — pleats hinted by vertical ribs
		var paper := _shell(c, 0.5, 0.95)
		_sphere(hub, 0.16, paper, Vector3(0, 0.0, 0), Vector3(1.0, 0.86, 1.0), 16, 8, true)
		for pl in 8:
			var pa := TAU * float(pl) / 8.0
			_box(hub, Vector3(0.006, 0.22, 0.006), _toon(c.darkened(0.35), 0.15),
				Vector3(cos(pa) * 0.155, 0.0, sin(pa) * 0.155), true).rotation.y = -pa
		# bamboo rib hoops
		for r in 2:
			var ry := -0.05 + float(r) * 0.10
			var rr := 0.16 * sqrt(maxf(0.0, 1.0 - pow(ry / 0.14, 2.0)))
			if rr > 0.03:
				_torus(hub, rr - 0.004, rr + 0.006, _toon(c.darkened(0.4), 0.15), Vector3(0, ry, 0), 16, true)
		# top + bottom gold crown caps
		_cyl(hub, 0.03, 0.05, 0.03, cap_mat, Vector3(0, 0.135, 0), 10)
		_torus(hub, 0.018, 0.038, gold, Vector3(0, 0.135, 0), 10)
		_cyl(hub, 0.05, 0.03, 0.025, cap_mat, Vector3(0, -0.12, 0), 10)
		_torus(hub, 0.018, 0.038, gold, Vector3(0, -0.12, 0), 10)
		# little tassel on the bottom
		_cyl(hub, 0.006, 0.006, 0.06, _toon(c.darkened(0.2), 0.2), Vector3(0, -0.17, 0), 4)
		_sphere(hub, 0.014, _toon(c.darkened(0.2), 0.2), Vector3(0, -0.21, 0), Vector3(1, 1.5, 1), 6, 3)
		# inner glow bead
		_sphere(hub, 0.06, _glow(c.lightened(0.3), 1.6), Vector3(0, 0, 0), Vector3.ONE, 8, 4, true)
		_light(hub, Vector3(0, 0, 0), c, 0.7, 2.6)
	return root


## CRYSTAL CHANDELIER (Legendary) — the crown jewel: a multi-tier gold frame, a
## cascade of faceted crystal drops, six glowing candle-arms each with a flame,
## strung crystal swags, a big central teardrop and a sparkle of rising glints.
## Hangs from y≈0 down (mount the canopy at ceiling). ~1.0 wide, ~1.0 tall.
static func build_chandelier() -> Node3D:
	var root := Node3D.new()
	var gold := _metal(Color(0.95, 0.78, 0.36), 0.12)
	var gold_dk := _metal(Color(0.72, 0.56, 0.24), 0.2)
	var crystal := _crystal(Color(0.90, 0.94, 1.0))
	var ruby := _gem(Color(1.0, 0.30, 0.40), 1.5)
	var candle_glow := _glow(Color(1.0, 0.88, 0.6), 1.8)
	# ceiling canopy + suspension chain
	_dome(root, 0.10, gold, Vector3(0, -0.02, 0), Vector3(1, 0.5, 1), 16)
	for c in 4:
		_torus(root, 0.018, 0.034, gold, Vector3(0, -0.10 - float(c) * 0.07, 0), 10)
	# central column / hub
	_cyl(root, 0.03, 0.05, 0.30, gold, Vector3(0, -0.50, 0), 14)
	_sphere(root, 0.085, gold, Vector3(0, -0.66, 0), Vector3(1, 1.2, 1), 14, 7)   # central orb
	# two gold tier rings (the frame), upper smaller, lower wider
	_torus(root, 0.20, 0.225, gold, Vector3(0, -0.46, 0), 26)
	_torus(root, 0.31, 0.34, gold_dk, Vector3(0, -0.60, 0), 30)
	# six candle arms swooping out from the upper ring, each ending in a cup+flame
	for k in 6:
		var ang := TAU * float(k) / 6.0
		var ax := cos(ang)
		var az := sin(ang)
		# S-curve arm (two short segments)
		var a1 := _cyl(root, 0.012, 0.016, 0.18, gold, Vector3(ax * 0.16, -0.47, az * 0.16), 8)
		a1.look_at_from_position(a1.position, a1.position + Vector3(ax, 0.3, az), Vector3.UP)
		var cup := Vector3(ax * 0.30, -0.40, az * 0.30)
		# a ruby gem set where each arm meets the ring (Legendary jewels)
		_facet(root, 0.018, 0.05, ruby, Vector3(ax * 0.20, -0.455, az * 0.20), 6)
		# bobeche dish + candle cup
		_cyl(root, 0.045, 0.03, 0.02, gold, cup + Vector3(0, -0.02, 0), 12)
		_cyl(root, 0.022, 0.024, 0.07, _toon(Color(0.98, 0.95, 0.86), 0.3), cup + Vector3(0, 0.04, 0), 10)
		# flame
		_sphere(root, 0.026, candle_glow, cup + Vector3(0, 0.10, 0), Vector3(1, 1.7, 1), 8, 4, true)
		_sphere(root, 0.045, _shell(Color(1.0, 0.7, 0.35), 0.3, 1.0), cup + Vector3(0, 0.10, 0), Vector3(1, 1.5, 1), 8, 4, true)
		# a crystal drop hanging under each arm
		_facet(root, 0.026, 0.075, crystal, cup + Vector3(0, -0.10, 0), 6)
	# crystal swags: strands of faceted beads strung between the lower ring points
	for k in 12:
		var ang2 := TAU * float(k) / 12.0
		var px := cos(ang2) * 0.325
		var pz := sin(ang2) * 0.325
		# a short vertical drop of 3 beads at each ring node
		for b in 3:
			var by := -0.62 - float(b) * 0.055
			_facet(root, 0.016 - float(b) * 0.002, 0.05, crystal, Vector3(px, by, pz), 5)
	# big central teardrop crystal
	_facet(root, 0.06, 0.20, crystal, Vector3(0, -0.86, 0), 8)
	_sphere(root, 0.05, _glow(Color(1.0, 0.94, 0.78), 1.4), Vector3(0, -0.66, 0), Vector3.ONE, 10, 5, true)
	# the two real warm lights
	_light(root, Vector3(0, -0.55, 0), Color(1.0, 0.88, 0.62), 2.4, 8.0)
	_light(root, Vector3(0, -0.85, 0), Color(1.0, 0.9, 0.7), 1.0, 4.0)
	# faint rising glints (Legendary sparkle)
	_motes(root, Vector3(0, -0.7, 0), 10, 2.4, 0.34, 0.05,
		_glow(Color(1.0, 0.97, 0.85), 2.2), 60.0, 0.01)
	return root


## NEON SIGN (Epic) — a glowing cursive "HEY" in hot magenta tubing with a cyan
## underline swoosh and a little amber star, mounted on a dark brushed backboard
## with a buzzing transformer box, chrome standoffs and a colored floor wash. The
## retro-bar centerpiece. ~0.95 tall on its little feet.
static func build_neon_sign() -> Node3D:
	var root := Node3D.new()
	var board := _toon(Color(0.08, 0.09, 0.13), 0.12, true, 0.3)
	var frame := _metal(Color(0.55, 0.57, 0.62), 0.25)
	var chrome := _metal(Color(0.82, 0.84, 0.9), 0.1)
	var pink := _glow(Color(1.0, 0.30, 0.66), 2.2)
	var pink_h := _shell(Color(1.0, 0.30, 0.66), 0.28, 1.4)   # magenta halo
	var cyan := _glow(Color(0.35, 0.92, 1.0), 2.2)
	var cyan_h := _shell(Color(0.35, 0.92, 1.0), 0.28, 1.4)
	var amber := _glow(Color(1.0, 0.78, 0.3), 1.8)
	var z := 0.05
	# dark backboard + thin metal frame + feet
	_box(root, Vector3(1.0, 0.62, 0.05), board, Vector3(0, 0.62, 0))
	_box(root, Vector3(1.04, 0.04, 0.06), frame, Vector3(0, 0.93, 0))
	_box(root, Vector3(1.04, 0.04, 0.06), frame, Vector3(0, 0.31, 0))
	_box(root, Vector3(0.04, 0.62, 0.06), frame, Vector3(-0.50, 0.62, 0))
	_box(root, Vector3(0.04, 0.62, 0.06), frame, Vector3(0.50, 0.62, 0))
	# transformer box + power conduit on the back-bottom
	_box(root, Vector3(0.22, 0.10, 0.10), _toon(Color(0.13, 0.14, 0.19), 0.2), Vector3(0.30, 0.36, -0.06))
	_cyl(root, 0.012, 0.012, 0.16, chrome, Vector3(0.30, 0.30, -0.06), 6)
	# little feet + chrome legs
	_box(root, Vector3(0.40, 0.05, 0.20), _toon(Color(0.12, 0.13, 0.18), 0.15), Vector3(0, 0.025, 0))
	_cyl(root, 0.02, 0.02, 0.30, chrome, Vector3(-0.18, 0.20, 0), 8)
	_cyl(root, 0.02, 0.02, 0.30, chrome, Vector3(0.18, 0.20, 0), 8)
	# "H" — two posts + a crossbar (each stroke = halo + bright core + round caps)
	_tube2d(root, Vector2(-0.38, 0.46), Vector2(-0.38, 0.78), z, pink, pink_h)
	_tube2d(root, Vector2(-0.26, 0.46), Vector2(-0.26, 0.78), z, pink, pink_h)
	_tube2d(root, Vector2(-0.38, 0.62), Vector2(-0.26, 0.62), z, pink, pink_h)
	# "E" — post + three arms
	_tube2d(root, Vector2(-0.14, 0.46), Vector2(-0.14, 0.78), z, pink, pink_h)
	_tube2d(root, Vector2(-0.14, 0.78), Vector2(-0.02, 0.78), z, pink, pink_h)
	_tube2d(root, Vector2(-0.14, 0.62), Vector2(-0.04, 0.62), z, pink, pink_h)
	_tube2d(root, Vector2(-0.14, 0.46), Vector2(-0.02, 0.46), z, pink, pink_h)
	# "Y" — two upper arms into a stem
	_tube2d(root, Vector2(0.10, 0.78), Vector2(0.18, 0.62), z, pink, pink_h)
	_tube2d(root, Vector2(0.26, 0.78), Vector2(0.18, 0.62), z, pink, pink_h)
	_tube2d(root, Vector2(0.18, 0.62), Vector2(0.18, 0.46), z, pink, pink_h)
	# cyan underline swoosh + a little star
	_tube2d(root, Vector2(-0.40, 0.40), Vector2(0.0, 0.36), z, cyan, cyan_h, 0.018)
	_tube2d(root, Vector2(0.0, 0.36), Vector2(0.34, 0.42), z, cyan, cyan_h, 0.018)
	# the amber star (5 proper radiating spokes — each rotated about the hub)
	_sphere(root, 0.03, amber, Vector3(0.40, 0.80, z), Vector3.ONE, 8, 4, true)
	for k in 5:
		var ang := TAU * float(k) / 5.0 + 0.3
		var spoke := _box(root, Vector3(0.012, 0.07, 0.012), amber,
			Vector3(0.40 + cos(ang + PI / 2.0) * 0.035, 0.80 + sin(ang + PI / 2.0) * 0.035, z + 0.01), true)
		spoke.rotation.z = ang
	# the round tube standoffs (little chrome dots holding the glass off the board)
	for p in [Vector2(-0.38, 0.78), Vector2(-0.26, 0.46), Vector2(0.18, 0.46), Vector2(-0.02, 0.78)]:
		_sphere(root, 0.014, chrome, Vector3(p.x, p.y, z + 0.005), Vector3.ONE, 6, 3, true)
	# coloured wash lights (back-glow on the board + a floor pool out front)
	_light(root, Vector3(-0.1, 0.62, 0.3), Color(1.0, 0.35, 0.7), 1.4, 5.0)
	_light(root, Vector3(0.1, 0.42, 0.3), Color(0.4, 0.9, 1.0), 0.8, 4.0)
	return root


## LAVA LAMP (Uncommon) — groovy retro classic: a brushed-chrome stepped cone
## base with a glowing power knob and three peg feet, a tall amber glass vessel, a
## column of drifting glowing wax blobs in hot orange + magenta, a glowing bottom
## puddle, a chrome domed cap and slow rising motes. ~0.8 tall.
static func build_lava_lamp() -> Node3D:
	var root := Node3D.new()
	var chrome := _metal(Color(0.80, 0.82, 0.88), 0.1)
	var chrome_dk := _metal(Color(0.58, 0.60, 0.66), 0.2)
	var glass := _shell(Color(0.98, 0.58, 0.22), 0.20, 0.5)
	# three little peg feet
	for k in 3:
		var fa := TAU * float(k) / 3.0
		_sphere(root, 0.022, chrome_dk, Vector3(cos(fa) * 0.115, 0.012, sin(fa) * 0.115), Vector3(1, 0.6, 1), 8, 4)
	# stepped chrome cone base with reeded grooves
	_cyl(root, 0.13, 0.16, 0.05, chrome_dk, Vector3(0, 0.04, 0), 20)
	_cyl(root, 0.075, 0.135, 0.16, chrome, Vector3(0, 0.145, 0), 22)
	for k in 12:
		var ang := TAU * float(k) / 12.0
		_box(root, Vector3(0.008, 0.15, 0.01), chrome_dk, Vector3(cos(ang) * 0.105, 0.145, sin(ang) * 0.105), true).rotation.y = -ang
	_torus(root, 0.072, 0.088, chrome, Vector3(0, 0.225, 0), 18)
	# a glowing power knob on the base (the "on" tell)
	_cyl(root, 0.016, 0.018, 0.02, chrome_dk, Vector3(0.13, 0.07, 0.06), 10)
	_sphere(root, 0.01, _glow(Color(1.0, 0.5, 0.25), 2.0), Vector3(0.13, 0.085, 0.075), Vector3.ONE, 6, 3, true)
	# tall amber glass vessel
	_cyl(root, 0.058, 0.088, 0.42, glass, Vector3(0, 0.445, 0), 20, true)
	# glowing puddle at the bottom of the glass
	_cyl(root, 0.078, 0.055, 0.04, _glow(Color(1.0, 0.5, 0.20), 1.6), Vector3(0, 0.265, 0), 16, true)
	# the wax: a column of squashed glowing blobs, hot orange / magenta
	var wax_a := _glow(Color(1.0, 0.44, 0.18), 1.7)
	var wax_b := _glow(Color(1.0, 0.30, 0.58), 1.7)
	_sphere(root, 0.06, wax_a, Vector3(0.0, 0.32, 0.0), Vector3(1.1, 0.8, 1.1), 12, 6, true)
	_sphere(root, 0.045, wax_b, Vector3(0.014, 0.44, 0.0), Vector3(1.0, 1.35, 1.0), 12, 6, true)
	_sphere(root, 0.034, wax_a, Vector3(-0.012, 0.56, 0.0), Vector3(1.0, 1.2, 1.0), 10, 5, true)
	_sphere(root, 0.024, wax_b, Vector3(0.01, 0.50, 0.0), Vector3.ONE, 8, 4, true)
	_sphere(root, 0.018, wax_a, Vector3(-0.01, 0.62, 0.0), Vector3.ONE, 8, 4, true)
	# rising motes inside (the drifting bubbles)
	_motes(root, Vector3(0, 0.36, 0), 8, 2.6, 0.04, 0.05,
		_glow(Color(1.0, 0.6, 0.3), 1.8), 8.0, 0.01)
	# chrome collar + domed cap
	_cyl(root, 0.072, 0.058, 0.05, chrome, Vector3(0, 0.68, 0), 20)
	_dome(root, 0.062, chrome, Vector3(0, 0.71, 0), Vector3.ONE, 18)
	_sphere(root, 0.016, chrome_dk, Vector3(0, 0.775, 0), Vector3.ONE, 8, 4)
	_light(root, Vector3(0, 0.44, 0), Color(1.0, 0.46, 0.3), 1.1, 3.6)
	return root


## CAMPFIRE / FIREPIT (Common) — the cozy gathering centerpiece: a ring of round
## river stones (one capped with moss), a charred ash bed, crossed logs (some
## burnt) with bark caps, a layered leaping flame (hot core to cool tip), a soft
## halo and a spray of rising embers. ~0.55 tall flame, ~0.8 wide stone ring.
static func build_campfire() -> Node3D:
	var root := Node3D.new()
	# ring of round stones (3 greys, squashed)
	var stones := [
		_toon(Color(0.56, 0.57, 0.61), 0.2),
		_toon(Color(0.45, 0.46, 0.50), 0.2),
		_toon(Color(0.64, 0.62, 0.60), 0.2),
	]
	var moss := _toon(Color(0.34, 0.54, 0.32), 0.3)
	for k in 9:
		var ang := TAU * float(k) / 9.0
		var s: Material = stones[k % 3]
		var st := _sphere(root, 0.10, s, Vector3(cos(ang) * 0.34, 0.05, sin(ang) * 0.34),
			Vector3(1.0, 0.7, 1.0), 10, 5)
		st.rotation.y = ang
		# a couple of stones wear a little moss cap (cozy detail)
		if k % 4 == 0:
			_sphere(root, 0.05, moss, Vector3(cos(ang) * 0.34, 0.10, sin(ang) * 0.34), Vector3(1.3, 0.4, 1.3), 8, 4)
	# charred ash bed
	_cyl(root, 0.30, 0.32, 0.03, _toon(Color(0.15, 0.14, 0.15), 0.1, false), Vector3(0, 0.02, 0), 18)
	# crossed logs (alternating fresh / burnt) with bark caps
	var log_mat := _toon(Color(0.45, 0.29, 0.17), 0.2)
	var burnt := _toon(Color(0.22, 0.17, 0.14), 0.15)
	var bark := _toon(Color(0.78, 0.68, 0.45), 0.2)
	for i in 4:
		var ang := PI * float(i) / 4.0
		var lg := _cyl(root, 0.035, 0.045, 0.5, log_mat if i % 2 == 0 else burnt,
			Vector3(0, 0.10, 0), 8)
		lg.rotation = Vector3(PI / 2.0, ang, 0.18)
		# bark end caps on each visible log end
		for e in [-1.0, 1.0]:
			var cap := _cyl(root, 0.046, 0.046, 0.014, bark,
				Vector3(cos(ang) * 0.24 * e, 0.10, sin(ang) * 0.24 * e), 8)
			cap.rotation = Vector3(PI / 2.0, ang, 0.18)
	# glowing coals at the base
	_sphere(root, 0.12, _glow(Color(1.0, 0.45, 0.15), 1.6), Vector3(0, 0.08, 0), Vector3(1.0, 0.5, 1.0), 12, 6, true)
	# the flame — stacked glowing teardrops, hot core to cool tip
	_sphere(root, 0.16, _glow(Color(1.0, 0.55, 0.18), 1.8), Vector3(0, 0.22, 0), Vector3(1.0, 1.4, 1.0), 12, 6, true)
	_sphere(root, 0.11, _glow(Color(1.0, 0.78, 0.32), 2.1), Vector3(0.01, 0.34, 0), Vector3(1.0, 1.5, 1.0), 10, 5, true)
	_sphere(root, 0.06, _glow(Color(1.0, 0.96, 0.62), 2.5), Vector3(-0.01, 0.46, 0), Vector3(1.0, 1.6, 1.0), 8, 4, true)
	# soft outer flame halo
	_sphere(root, 0.20, _shell(Color(1.0, 0.5, 0.2), 0.22, 1.0), Vector3(0, 0.28, 0), Vector3(1.0, 1.5, 1.0), 10, 5, true)
	_light(root, Vector3(0, 0.30, 0), Color(1.0, 0.55, 0.25), 2.4, 7.5)
	# rising embers
	_motes(root, Vector3(0, 0.2, 0), 18, 1.8, 0.12, 0.5,
		_glow(Color(1.0, 0.6, 0.25), 2.2), 18.0, 0.018)
	return root


## FAIRY JAR (Rare) — a corked mason jar full of captured fireflies: a faceted
## clear-glass jar with embossed rings, a wooden cork, a wrapped twine bow with a
## tiny brass tag, a swirl of glowing golden motes inside and a soft inner glow.
## A jar of stars. ~0.46 tall.
static func build_fairy_jar() -> Node3D:
	var root := Node3D.new()
	var glass := _shell(Color(0.86, 0.94, 1.0), 0.16, 0.35)
	var cork := _toon(Color(0.74, 0.55, 0.33), 0.25)
	var cork_dk := _toon(Color(0.58, 0.41, 0.24), 0.2)
	var twine := _toon(Color(0.84, 0.76, 0.55), 0.2)
	var brass := _metal(Color(0.86, 0.66, 0.30), 0.18)
	# the jar body — a faceted clear cylinder with embossed rings + rounded shoulder
	_cyl(root, 0.115, 0.125, 0.26, glass, Vector3(0, 0.16, 0), 18, true)
	_dome(root, 0.115, glass, Vector3(0, 0.29, 0), Vector3(1, 0.55, 1), 18, true)
	# embossed mason rings near the neck
	_torus(root, 0.10, 0.118, glass, Vector3(0, 0.32, 0), 18, true)
	_torus(root, 0.10, 0.118, glass, Vector3(0, 0.345, 0), 18, true)
	# base lip
	_cyl(root, 0.118, 0.12, 0.02, glass, Vector3(0, 0.035, 0), 18, true)
	# neck + wooden cork
	_cyl(root, 0.07, 0.085, 0.05, glass, Vector3(0, 0.375, 0), 14, true)
	_cyl(root, 0.072, 0.066, 0.06, cork, Vector3(0, 0.42, 0), 12)
	_dome(root, 0.072, cork_dk, Vector3(0, 0.45, 0), Vector3(1, 0.5, 1), 12)
	# wrapped twine bow around the neck + a brass keepsake tag
	_torus(root, 0.078, 0.092, twine, Vector3(0, 0.375, 0), 16)
	_sphere(root, 0.022, twine, Vector3(0.085, 0.378, 0.04), Vector3(1.3, 0.7, 0.6), 8, 4)
	_box(root, Vector3(0.012, 0.06, 0.01), twine, Vector3(0.10, 0.34, 0.05)).rotation.z = 0.4
	_box(root, Vector3(0.03, 0.04, 0.006), brass, Vector3(-0.085, 0.34, 0.05)).rotation.z = -0.3
	# the captured fireflies — a swirl of glowing golden beads inside
	var bead := _glow(Color(1.0, 0.92, 0.55), 2.4)
	var pts := [
		Vector3(0.0, 0.16, 0.0), Vector3(0.05, 0.22, 0.03), Vector3(-0.045, 0.12, -0.03),
		Vector3(0.035, 0.10, 0.04), Vector3(-0.05, 0.20, 0.04), Vector3(0.06, 0.18, -0.03),
		Vector3(-0.03, 0.26, 0.02), Vector3(0.02, 0.08, -0.04), Vector3(0.05, 0.27, 0.03),
		Vector3(-0.055, 0.16, 0.0), Vector3(0.0, 0.21, -0.05),
	]
	for p in pts:
		_sphere(root, 0.014, bead, p, Vector3.ONE, 6, 3, true)
	# soft inner glow body + warm light
	_sphere(root, 0.085, _glow(Color(1.0, 0.86, 0.5), 0.7), Vector3(0, 0.17, 0), Vector3.ONE, 10, 5, true)
	_light(root, Vector3(0, 0.18, 0), Color(1.0, 0.88, 0.55), 1.0, 3.2)
	# slow drifting twinkle inside
	_motes(root, Vector3(0, 0.14, 0), 10, 2.4, 0.09, 0.04,
		_glow(Color(1.0, 0.95, 0.7), 2.4), 50.0, 0.01)
	return root


## VINTAGE STREET LAMP (Common) — a classic gas-lamp silhouette: a stepped iron
## base, a fluted post with a scrolled collar, decorative bracket scrolls, a
## six-sided glass lantern head with a cage of ribs, a warm bulb, a vented copper
## crown roof, a ball finial — and a perched little birdhouse for charm. Tall
## outdoor piece. ~2.8 tall.
static func build_street_lamp() -> Node3D:
	var root := Node3D.new()
	var iron := _toon(Color(0.14, 0.16, 0.21), 0.25, true, 0.3)
	var iron_dk := _toon(Color(0.09, 0.10, 0.14), 0.2)
	var collar := _metal(Color(0.55, 0.42, 0.22), 0.3, 0.8)
	var copper := _metal(Color(0.72, 0.45, 0.30), 0.28, 0.9)
	var glass := _shell(Color(1.0, 0.86, 0.55), 0.28, 0.6)
	# stepped iron base
	_box(root, Vector3(0.32, 0.10, 0.32), iron_dk, Vector3(0, 0.05, 0))
	_box(root, Vector3(0.24, 0.06, 0.24), iron, Vector3(0, 0.13, 0))
	_cyl(root, 0.10, 0.15, 0.16, iron, Vector3(0, 0.20, 0), 12)
	_torus(root, 0.10, 0.13, collar, Vector3(0, 0.27, 0), 16)
	# fluted post + reeded ribs
	_cyl(root, 0.05, 0.078, 2.0, iron, Vector3(0, 1.25, 0), 12)
	for k in 8:
		var ang := TAU * float(k) / 8.0
		_cyl(root, 0.006, 0.006, 1.9, iron_dk, Vector3(cos(ang) * 0.062, 1.25, sin(ang) * 0.062), 4)
	# decorative scroll collar near the top + four C-scroll brackets
	_torus(root, 0.058, 0.085, collar, Vector3(0, 2.18, 0), 16)
	for k in 4:
		var ang2 := TAU * float(k) / 4.0
		_torus(root, 0.018, 0.034, collar, Vector3(cos(ang2) * 0.085, 2.22, sin(ang2) * 0.085), 10).rotation.x = PI / 2.0
		# little curling bracket arm under the lantern
		var br := _torus(root, 0.02, 0.05, iron, Vector3(cos(ang2) * 0.10, 2.28, sin(ang2) * 0.10), 8)
		br.rotation.x = PI / 2.0
		br.rotation.y = -ang2
	# the lantern cage: 6-sided glass head + corner ribs
	_cyl(root, 0.10, 0.135, 0.28, glass, Vector3(0, 2.42, 0), 6, true)
	for k in 6:
		var ang3 := TAU * float(k) / 6.0
		_cyl(root, 0.008, 0.008, 0.30, iron, Vector3(cos(ang3) * 0.12, 2.42, sin(ang3) * 0.12), 4)
	_cyl(root, 0.13, 0.14, 0.02, iron, Vector3(0, 2.27, 0), 12)   # bottom frame ring
	_cyl(root, 0.11, 0.12, 0.02, iron, Vector3(0, 2.57, 0), 12)   # top frame ring
	# warm bulb inside + a glowing filament hint
	_sphere(root, 0.075, _glow(Color(1.0, 0.84, 0.5), 1.8), Vector3(0, 2.42, 0), Vector3.ONE, 10, 5, true)
	_cyl(root, 0.006, 0.006, 0.10, _glow(Color(1.0, 0.95, 0.7), 2.6), Vector3(0, 2.42, 0), 4, true)
	# vented copper crown roof (two tiers) + ball finial
	_cyl(root, 0.07, 0.15, 0.10, copper, Vector3(0, 2.63, 0), 6)
	_cyl(root, 0.0, 0.085, 0.10, copper, Vector3(0, 0.0 + 2.72, 0), 6)
	_torus(root, 0.05, 0.07, collar, Vector3(0, 2.61, 0), 12)
	_sphere(root, 0.032, collar, Vector3(0, 2.80, 0), Vector3.ONE, 8, 4)
	# a tiny birdhouse perched on a top bracket (storybook charm)
	var bh := Node3D.new()
	bh.position = Vector3(0.16, 2.50, 0.0)
	root.add_child(bh)
	_box(bh, Vector3(0.10, 0.10, 0.09), _toon(Color(0.80, 0.55, 0.40), 0.2), Vector3.ZERO)
	_cyl(bh, 0.0, 0.085, 0.06, copper, Vector3(0, 0.075, 0), 4).rotation.y = PI / 4.0
	_sphere(bh, 0.02, _toon(Color(0.12, 0.12, 0.15), 0.1), Vector3(0, 0.0, 0.05), Vector3.ONE, 8, 4)   # entrance hole
	_cyl(bh, 0.004, 0.004, 0.05, iron, Vector3(-0.06, -0.02, 0.0), 4).rotation.z = PI / 2.0   # perch peg back to post
	_light(root, Vector3(0, 2.42, 0), Color(1.0, 0.82, 0.5), 2.0, 8.5)
	return root


## BIOLUMINESCENT MUSHROOM LAMP (Epic) — an enchanted-forest cluster: a mossy
## rock + soil mound, three glowing toadstools of different sizes with spotted
## caps and frilled gills, smaller sprout caps, a scatter of glowing pebbles, a
## curled fern frond and drifting spores. The fairytale-grove light. ~0.7 tall.
static func build_mushroom_lamp() -> Node3D:
	var root := Node3D.new()
	var soil := _toon(Color(0.30, 0.22, 0.16), 0.2)
	var moss := _toon(Color(0.32, 0.52, 0.30), 0.3)
	var fern := _toon(Color(0.36, 0.62, 0.34), 0.3)
	var stem := _toon(Color(0.93, 0.92, 0.85), 0.3, true, 0.3)
	# mossy mound base (soil dome + moss patches)
	_dome(root, 0.22, soil, Vector3(0, 0.0, 0), Vector3(1.0, 0.6, 1.0), 18)
	for k in 6:
		var ang := TAU * float(k) / 6.0
		_sphere(root, 0.06, moss, Vector3(cos(ang) * 0.16, 0.05, sin(ang) * 0.16),
			Vector3(1.4, 0.5, 1.4), 8, 4)
	_sphere(root, 0.10, moss, Vector3(0.0, 0.10, 0.0), Vector3(1.4, 0.5, 1.4), 10, 5)
	# a curled fern frond arcing over the cluster (forest detail)
	for f in 7:
		var t := float(f) / 6.0
		var fx := -0.20 + t * 0.22
		var fy := 0.10 + sin(t * PI) * 0.26
		var fz := -0.14 + t * 0.04
		_sphere(root, 0.026 * (1.0 - t * 0.6), fern, Vector3(fx, fy, fz), Vector3(1, 0.7, 1), 6, 3)
	# helper: one glowing toadstool (stem + glowing cap + spots + gills + light)
	var shroom := func(pos: Vector3, scale: float, cap_col: Color, glow_col: Color) -> void:
		var hub := Node3D.new()
		hub.position = pos
		hub.scale = Vector3.ONE * scale
		root.add_child(hub)
		# plump curved stem
		_cyl(hub, 0.035, 0.05, 0.20, stem, Vector3(0, 0.10, 0), 12)
		_torus(hub, 0.045, 0.062, stem, Vector3(0, 0.04, 0), 12)   # skirt ring
		# the glowing underside (gills) — a disc of glow under the cap
		_cyl(hub, 0.13, 0.10, 0.02, _glow(glow_col, 1.8), Vector3(0, 0.20, 0), 18, true)
		# frilled gills hint — short radial glow ribs
		for g in 10:
			var ga := TAU * float(g) / 10.0
			_box(hub, Vector3(0.012, 0.012, 0.08), _glow(glow_col, 1.4),
				Vector3(cos(ga) * 0.07, 0.205, sin(ga) * 0.07), true).rotation.y = -ga
		# the cap — a glowing translucent dome with a colored rim
		_dome(hub, 0.155, _shell(cap_col, 0.7, 1.0), Vector3(0, 0.21, 0), Vector3(1.0, 0.85, 1.0), 18, true)
		_torus(hub, 0.135, 0.155, _toon(cap_col.darkened(0.2), 0.3), Vector3(0, 0.215, 0), 18)
		# white spots scattered on the cap
		for s in 6:
			var sa := TAU * float(s) / 6.0 + 0.4
			var rr := 0.10
			_sphere(hub, 0.022, _glow(Color(1.0, 1.0, 0.95), 1.2),
				Vector3(cos(sa) * rr, 0.27 - rr * 0.4, sin(sa) * rr), Vector3(1, 0.6, 1), 8, 4, true)
		# tiny glow tip
		_sphere(hub, 0.03, _glow(glow_col, 1.6), Vector3(0, 0.31, 0), Vector3(1, 0.7, 1), 8, 4, true)
		_light(hub, Vector3(0, 0.24, 0), glow_col, 1.1, 3.2)
	# three toadstools of different sizes + colors, clustered
	shroom.call(Vector3(0.0, 0.06, 0.02), 1.0, Color(0.55, 0.78, 1.0), Color(0.5, 0.85, 1.0))     # big blue
	shroom.call(Vector3(-0.16, 0.05, -0.04), 0.7, Color(0.78, 0.55, 1.0), Color(0.78, 0.5, 1.0))  # purple
	shroom.call(Vector3(0.15, 0.05, -0.06), 0.55, Color(0.5, 1.0, 0.74), Color(0.45, 1.0, 0.7))   # teal
	# little sprout caps poking from the moss
	_sphere(root, 0.03, _glow(Color(0.6, 0.95, 1.0), 1.4), Vector3(0.1, 0.07, 0.12), Vector3(1, 0.7, 1), 8, 4, true)
	_sphere(root, 0.025, _glow(Color(0.8, 0.6, 1.0), 1.4), Vector3(-0.12, 0.06, 0.1), Vector3(1, 0.7, 1), 8, 4, true)
	# glowing pebbles scattered at the base
	for k in 5:
		var ang2 := TAU * float(k) / 5.0 + 0.5
		_sphere(root, 0.02, _glow(Color(0.55, 0.9, 1.0), 1.0),
			Vector3(cos(ang2) * 0.20, 0.02, sin(ang2) * 0.20), Vector3(1.3, 0.6, 1.3), 6, 3, true)
	# drifting spores (gentle cool sparkle rising from the cluster)
	_motes(root, Vector3(0, 0.25, 0), 12, 3.0, 0.20, 0.05,
		_glow(Color(0.7, 0.95, 1.0), 1.8), 40.0, 0.012)
	return root
