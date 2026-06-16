class_name VerseCatalogPlants
extends RefCounted
## Hey Verse — PLANTS & GARDEN catalog (showroom set, 11 items).
##
## Premium, sellable greenery for the ~1.4-unit chibi-robot world: a monstera in
## a deco pot, a wired bonsai, a sakura with fairy-lights, a bioluminescent
## crystal plant, a cactus trio, a brass-collared topiary cat, a boho hanging
## fern, a sunflower crate, a brass potted palm, a glowing carnivorous bog, and a
## gilded zen garden.
##
## Pure procedural primitives only (no art assets). Every item is a static
## `build_<id>() -> Node3D` returning ONE self-contained Node3D, built at the
## ORIGIN and resting on the floor plane y=0. Sizes suit the chibi avatar
## (tabletop pots ~0.4 tall, trees up to ~1.8 — readable next to a 1.4u robot).
##
## Style matches avatar.gd / home.gd / the other catalogs: soft rounded toon
## shapes, decorated pots, LAYERED foliage, flat-bright cohesive palettes, an
## inverted-hull outline on every solid, polished-metal trim via the toon spec
## dot, faceted EMISSIVE gems, and tasteful glow + drifting particles where they
## sell the magic. Rarity reads at a glance: Common pieces are honest greenery;
## Rare+ gain real metal trim, gemstones, glow and floating accents.
##
## Self-contained: re-declares its own tiny material + mesh helpers and pulls
## only the shared toon + outline shaders for the look — no avatar.gd / home.gd
## internals, no .glb, no external art.

const TOON_SHADER := preload("res://toon.gdshader")
const OUTLINE_SHADER := preload("res://outline.gdshader")

static var _outline_mat: ShaderMaterial


# ───────────────────────────── material + primitive helpers ────────────────
# Self-contained copies so this file parses + runs standalone.

## The cel material every solid surface uses (toon ramp + inverted-hull outline).
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


## Polished metal — a toon cel with a strong rim + bright spec dot so it reads as
## gold / brass / chrome. The premium-trim material for Rare+ pieces.
static func _metal(c: Color, spec := 0.8) -> ShaderMaterial:
	return _toon(c, 0.55, true, spec)


## Leaf material — a toon cel with a gentle wind sway baked in via the shader
## (the leaves rustle in place, mobile-cheap). `wind_h` = how high the sway
## ramps to full strength.
static func _leaf(c: Color, wind := 0.5, wind_h := 0.6, rim := 0.3) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = TOON_SHADER
	m.set_shader_parameter("albedo", c)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", 0.0)
	m.set_shader_parameter("wind_strength", wind)
	m.set_shader_parameter("wind_height", wind_h)
	if _outline_mat == null:
		_outline_mat = ShaderMaterial.new()
		_outline_mat.shader = OUTLINE_SHADER
	m.next_pass = _outline_mat
	return m


## Unshaded emissive — glow surfaces (crystal facets, glow-flowers, fireflies).
static func _glow(c: Color, energy := 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


## Soft translucent shell — crystal bodies, glass cloches, dewdrops. No shadow
## casting (it would punch holes in the glow).
static func _shell(c: Color, alpha := 0.4, emit := 0.6) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.roughness = 0.15
	m.metallic = 0.2
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = emit
	return m


## A faceted gemstone: a low-seg double-cone (brilliant cut) that glows. Used as
## the rarity tell on Rare+ pieces. Returns the gem Node3D (positioned by caller).
static func _gem(parent: Node3D, r: float, c: Color, pos: Vector3, emit := 1.4) -> Node3D:
	var g := Node3D.new()
	g.position = pos
	parent.add_child(g)
	var mat := _shell(c, 0.7, emit)
	# crown (up cone) + pavilion (down cone) = a cut jewel
	var crown := CylinderMesh.new()
	crown.top_radius = 0.0
	crown.bottom_radius = r
	crown.height = r * 1.3
	crown.radial_segments = 6
	var cmi := MeshInstance3D.new()
	cmi.mesh = crown
	cmi.material_override = mat
	cmi.position = Vector3(0, r * 0.55, 0)
	cmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	g.add_child(cmi)
	var pav := CylinderMesh.new()
	pav.top_radius = r
	pav.bottom_radius = 0.0
	pav.height = r * 1.0
	pav.radial_segments = 6
	var pmi := MeshInstance3D.new()
	pmi.mesh = pav
	pmi.material_override = mat
	pmi.position = Vector3(0, -r * 0.4, 0)
	pmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	g.add_child(pmi)
	# a tiny bright spark in the core
	var spark := SphereMesh.new()
	spark.radius = r * 0.35
	spark.height = r * 0.7
	spark.radial_segments = 6
	spark.rings = 3
	var smi := MeshInstance3D.new()
	smi.mesh = spark
	smi.material_override = _glow(c.lightened(0.4), emit + 1.0)
	smi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	g.add_child(smi)
	return g


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


## A faceted cone — a low-seg cone with a flat or pointed top; used for crystal
## shards, gem-cut petals and lantern roofs. Returns the MeshInstance3D.
static func _cone(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 6, no_shadow := false) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = 0.0
	cm.bottom_radius = r
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _sphere(parent: Node3D, r: float, mat: Material, pos: Vector3, s := Vector3.ONE, seg := 16, rings := 8, no_shadow := false) -> MeshInstance3D:
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
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _capsule(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 14) -> MeshInstance3D:
	var cm := CapsuleMesh.new()
	cm.radius = r
	cm.height = h
	cm.radial_segments = seg
	cm.rings = 6
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 18) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = seg
	tm.ring_segments = 10
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


## A flat-ish prism leaf "blade" — a thin squashed box, good for foliage paddles
## and grass when you scale + rotate it. Cheap; reads as a leaf with the outline.
static func _blade(parent: Node3D, w: float, l: float, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = Vector3(w, 0.02, l)
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


## A faint contact-shadow blob on the floor — grounds the piece like the avatar's.
static func _contact(parent: Node3D, r: float, z_off := 0.0) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = r
	disc.bottom_radius = r
	disc.height = 0.01
	disc.radial_segments = 22
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = Color(0, 0, 0, 0.12)
	var mi := MeshInstance3D.new()
	mi.mesh = disc
	mi.material_override = m
	mi.position = Vector3(0, 0.012, z_off)
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)


## A decorated terracotta-style pot: tapered body, a rolled rim, a saucer, and an
## optional painted band. Used by several plants so the bases feel cohesive +
## premium. `rim_metal` swaps the rolled rim for a polished-metal band (Rare+).
## Returns the soil-surface Y so the plant knows where to sit.
static func _pot(parent: Node3D, top_r: float, bot_r: float, h: float, body: Color, rim_c: Color, soil := true, band := Color(0, 0, 0, 0), rim_metal := false) -> float:
	var body_mat := _toon(body, 0.32, true, 0.3)
	# saucer
	_cyl(parent, top_r * 1.06, top_r * 1.02, 0.03, _toon(body.darkened(0.12), 0.25), Vector3(0, 0.015, 0), Vector3.ZERO, 20)
	# tapered body
	_cyl(parent, top_r, bot_r, h, body_mat, Vector3(0, 0.03 + h * 0.5, 0), Vector3.ZERO, 20)
	# rolled rim — plain toon or polished metal
	var rim_mat := _metal(rim_c, 0.85) if rim_metal else _toon(rim_c, 0.34, true, 0.35)
	_torus(parent, top_r * 0.86, top_r * 1.04, rim_mat, Vector3(0, 0.03 + h, 0), Vector3(PI / 2.0, 0, 0), 22)
	# optional painted decorative band
	if band.a > 0.01:
		_cyl(parent, top_r * 0.98 - (top_r - bot_r) * 0.35, bot_r * 1.005, h * 0.30, _toon(band, 0.3, true, 0.2), Vector3(0, 0.03 + h * 0.42, 0), Vector3.ZERO, 20)
	var soil_y := 0.03 + h - 0.02
	if soil:
		_cyl(parent, top_r * 0.86, top_r * 0.86, 0.04, _toon(Color(0.22, 0.15, 0.10), 0.12), Vector3(0, soil_y, 0), Vector3.ZERO, 20)
		# a few little pebbles on the soil
		for k in 5:
			var a := TAU * float(k) / 5.0 + 0.4
			_sphere(parent, 0.016, _toon(Color(0.5, 0.48, 0.46), 0.2), Vector3(cos(a) * top_r * 0.55, soil_y + 0.02, sin(a) * top_r * 0.55), Vector3(1.0, 0.7, 1.0), 6, 3)
	return soil_y + 0.02


# ════════════════════════════════════════════════════════════════════ ITEMS


## 1 · MONSTERA — the iconic split-leaf in a striped deco pot. Big glossy paddles
## on arching stems. Common: friendly, leafy, instantly recognisable.
static func build_monstera() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.42)
	# white deco pot with a teal painted band
	var soil := _pot(root, 0.24, 0.19, 0.30, Color(0.95, 0.95, 0.96), Color(0.88, 0.88, 0.90), true, Color(0.28, 0.62, 0.60))
	var stem_c := _toon(Color(0.36, 0.56, 0.30), 0.2)
	var leaf_a := _leaf(Color(0.22, 0.55, 0.30), 0.45, 0.9)
	var leaf_b := _leaf(Color(0.28, 0.62, 0.34), 0.5, 0.9)
	var leaf_dark := _leaf(Color(0.16, 0.44, 0.26), 0.4, 0.9)
	var leaf_glint := _leaf(Color(0.40, 0.74, 0.42), 0.5, 0.9, 0.45)
	# 8 monstera leaves on arching stems, fanned around
	var specs := [
		[0.0, 0.98, 0.34, leaf_b, 1.2],
		[0.9, 0.80, 0.30, leaf_a, 1.05],
		[-0.9, 0.84, 0.30, leaf_dark, 1.05],
		[2.1, 0.70, 0.26, leaf_a, 0.88],
		[-2.0, 0.74, 0.28, leaf_b, 0.92],
		[1.5, 0.57, 0.22, leaf_dark, 0.8],
		[-1.4, 0.52, 0.22, leaf_a, 0.78],
		[2.9, 0.60, 0.24, leaf_glint, 0.82],
	]
	for s in specs:
		var ang: float = s[0]
		var hgt: float = s[1]
		var reach: float = s[2]
		var lm: ShaderMaterial = s[3]
		var sc: float = s[4]
		var tipx := cos(ang) * reach
		var tipz := sin(ang) * reach
		# arching stem
		_cyl(root, 0.012, 0.018, hgt, stem_c, Vector3(tipx * 0.45, soil + hgt * 0.5, tipz * 0.45), Vector3(tipz * 0.5, 0, -tipx * 0.5), 6)
		# the big paddle leaf: a fat squashed sphere + radial "split" slits as gaps
		var lp := Vector3(tipx, soil + hgt + 0.04, tipz)
		var leaf := _sphere(root, 0.18, lm, lp, Vector3(1.55 * sc, 0.12, 1.08 * sc), 14, 7)
		leaf.rotation = Vector3(-0.5, ang + PI / 2.0, 0)
		# a darker underleaf for thickness + depth
		_sphere(root, 0.16, leaf_dark, lp + Vector3(0, -0.03, 0), Vector3(1.4 * sc, 0.08, 0.95 * sc), 12, 6).rotation = Vector3(-0.5, ang + PI / 2.0, 0)
		# the signature monstera "splits" — small dark wedge cuts on the paddle
		for j in 3:
			var off := -0.10 + j * 0.10
			_box(root, Vector3(0.05, 0.03, 0.13 * sc), leaf_dark, lp + Vector3(off * cos(ang + PI / 2.0), -0.005, off * sin(ang + PI / 2.0)), Vector3(-0.5, ang + PI / 2.0, 0))
		# bright central vein
		_box(root, Vector3(0.012, 0.025, 0.30 * sc), stem_c, lp + Vector3(0, 0.01, 0), Vector3(-0.5, ang + PI / 2.0, 0))
	# a couple of low baby leaves near the soil
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.07, leaf_a, Vector3(sx * 0.10, soil + 0.10, 0.06), Vector3(1.4, 0.12, 1.0), 10, 5).rotation = Vector3(-0.4, sx * 0.6, 0)
	return root


## 2 · BONSAI — a tiny ancient tree on a glazed ceramic tray, gnarled trunk,
## copper training wire and cloud-pad canopies. Uncommon: refined, sculpted, a
## collector's calm — the copper wire reads as the cultivated upgrade.
static func build_bonsai() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.4)
	# shallow glazed rectangular tray + feet
	var glaze := _toon(Color(0.36, 0.22, 0.20), 0.34, true, 0.5)   # oxblood glaze, glossy
	_box(root, Vector3(0.56, 0.10, 0.40), glaze, Vector3(0, 0.07, 0))
	_box(root, Vector3(0.50, 0.05, 0.34), _toon(Color(0.30, 0.18, 0.16), 0.2), Vector3(0, 0.135, 0))
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_box(root, Vector3(0.05, 0.04, 0.05), glaze, Vector3(sx * 0.23, 0.02, sz * 0.15))
	# moss + soil top
	_box(root, Vector3(0.48, 0.03, 0.32), _toon(Color(0.30, 0.45, 0.22), 0.18), Vector3(0, 0.165, 0))
	for k in 6:
		var a := TAU * float(k) / 6.0
		_sphere(root, 0.025, _toon(Color(0.40, 0.58, 0.28), 0.2), Vector3(cos(a) * 0.16, 0.19, sin(a) * 0.11), Vector3(1.0, 0.5, 1.0), 6, 3)
	# gnarled trunk: a few angled segments
	var bark := _toon(Color(0.42, 0.30, 0.20), 0.24, true, 0.25)
	var bark_d := _toon(Color(0.34, 0.24, 0.16), 0.18)
	_cyl(root, 0.045, 0.06, 0.18, bark, Vector3(-0.04, 0.27, 0.0), Vector3(0, 0, 0.25), 10)
	_cyl(root, 0.035, 0.045, 0.16, bark_d, Vector3(0.02, 0.42, -0.02), Vector3(0.1, 0, -0.5), 10)
	_cyl(root, 0.025, 0.035, 0.14, bark, Vector3(0.12, 0.52, 0.02), Vector3(0, 0, -0.9), 8)
	# a low spreading branch to the left
	_cyl(root, 0.02, 0.03, 0.14, bark, Vector3(-0.16, 0.44, 0.0), Vector3(0, 0, 1.1), 8)
	# copper training wire spiralling up the trunk — the cultivated tell
	var copper := _metal(Color(0.86, 0.50, 0.30), 0.9)
	for i in 6:
		var t := float(i) / 5.0
		var wy := 0.28 + t * 0.24
		var wa := t * 9.0
		_torus(root, 0.006, 0.052 - t * 0.018, copper, Vector3(-0.02 + t * 0.06, wy, -0.01), Vector3(PI / 2.0 + 0.3, wa, 0), 8)
	# cloud-pad canopies: clustered flattened green spheres, one accent pad
	var pad := _leaf(Color(0.26, 0.52, 0.30), 0.25, 0.4)
	var pad_d := _leaf(Color(0.20, 0.44, 0.26), 0.2, 0.4)
	var pads := [
		[Vector3(0.20, 0.62, 0.04), 0.16],
		[Vector3(0.10, 0.66, -0.06), 0.13],
		[Vector3(-0.20, 0.54, 0.02), 0.13],
		[Vector3(0.26, 0.56, -0.04), 0.11],
		[Vector3(0.0, 0.58, 0.08), 0.10],
	]
	for p in pads:
		var pc: Vector3 = p[0]
		var pr: float = p[1]
		_sphere(root, pr, pad, pc, Vector3(1.3, 0.55, 1.3), 12, 6)
		_sphere(root, pr * 0.7, pad_d, pc + Vector3(0, -0.03, 0.04), Vector3(1.2, 0.5, 1.2), 10, 5)
	# a sprinkle of tiny pink blossoms on the canopy for charm
	for k in 7:
		var a2 := TAU * float(k) / 7.0
		_sphere(root, 0.018, _toon(Color(0.98, 0.72, 0.80), 0.25), Vector3(0.15 + cos(a2) * 0.14, 0.64, cos(a2 * 1.7) * 0.06), Vector3.ONE, 6, 3)
	# a small engraved brass nameplate on the front of the tray
	_box(root, Vector3(0.14, 0.04, 0.012), _metal(Color(0.90, 0.74, 0.36), 0.9), Vector3(0, 0.07, 0.205))
	return root


## 3 · CHERRY-BLOSSOM TREE — a small sakura strung with warm fairy-lights on a
## brass-banded stone planter, drifting petals + glowing blossoms. Rare: a
## showpiece — the lights, gold glints and brass band lift it clear of Common.
static func build_cherry_blossom() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.6)
	# stone planter ring with a polished-brass band
	var stone := _toon(Color(0.66, 0.64, 0.62), 0.25, true, 0.2)
	var brass := _metal(Color(0.90, 0.72, 0.34), 0.85)
	_torus(root, 0.30, 0.42, stone, Vector3(0, 0.07, 0), Vector3(PI / 2.0, 0, 0), 22)
	_torus(root, 0.355, 0.40, brass, Vector3(0, 0.12, 0), Vector3(PI / 2.0, 0, 0), 22)
	_cyl(root, 0.32, 0.32, 0.04, _toon(Color(0.28, 0.18, 0.12), 0.12), Vector3(0, 0.05, 0), Vector3.ZERO, 22)
	for k in 5:
		var a := TAU * float(k) / 5.0
		_sphere(root, 0.03, _toon(Color(0.40, 0.55, 0.26), 0.2), Vector3(cos(a) * 0.18, 0.08, sin(a) * 0.18), Vector3(1, 0.5, 1), 6, 3)
	# trunk + branches
	var bark := _toon(Color(0.40, 0.27, 0.24), 0.22, true, 0.2)
	_cyl(root, 0.06, 0.10, 0.7, bark, Vector3(0, 0.42, 0), Vector3(0, 0, 0.04), 12)
	var branch_dirs := [
		[Vector3(0.18, 0.78, 0.05), Vector3(0, 0, -0.8)],
		[Vector3(-0.20, 0.82, -0.04), Vector3(0, 0, 0.85)],
		[Vector3(0.04, 0.92, 0.18), Vector3(-0.7, 0, 0)],
		[Vector3(0.0, 0.95, -0.16), Vector3(0.7, 0, 0)],
	]
	for b in branch_dirs:
		_cyl(root, 0.025, 0.04, 0.30, bark, b[0], b[1], 8)
	# blossom crown: layered soft-pink puffs, lighter at the top
	var pink_a := _leaf(Color(0.99, 0.74, 0.82), 0.3, 0.5)
	var pink_b := _leaf(Color(0.99, 0.84, 0.90), 0.3, 0.5)
	var pink_c := _leaf(Color(0.97, 0.62, 0.74), 0.25, 0.5)
	var puffs := [
		[Vector3(0.0, 1.18, 0.0), 0.34, pink_a],
		[Vector3(0.26, 1.02, 0.06), 0.24, pink_c],
		[Vector3(-0.28, 1.06, -0.04), 0.24, pink_b],
		[Vector3(0.10, 1.10, 0.26), 0.22, pink_b],
		[Vector3(-0.06, 1.12, -0.26), 0.22, pink_c],
		[Vector3(0.0, 1.34, 0.0), 0.22, pink_b],
		[Vector3(0.18, 1.26, -0.12), 0.16, pink_a],
	]
	for p in puffs:
		_sphere(root, p[1], p[2], p[0], Vector3(1.1, 0.9, 1.1), 14, 7)
	# warm fairy-lights woven through the crown — the rare flourish
	var bulb := _glow(Color(1.0, 0.86, 0.5), 2.2)
	for k in 14:
		var a2 := TAU * float(k) / 14.0
		var rr := 0.30 + (k % 3) * 0.04
		var by := 1.06 + sin(a2 * 3.0) * 0.14
		_sphere(root, 0.022, bulb, Vector3(cos(a2) * rr, by, sin(a2) * rr), Vector3.ONE, 8, 4, true)
	# a few bright blossom clusters dotted on the crown
	for k in 9:
		var a3 := TAU * float(k) / 9.0
		var rr2 := 0.30 + (k % 2) * 0.05
		_sphere(root, 0.04, _toon(Color(1.0, 0.66, 0.78), 0.3), Vector3(cos(a3) * rr2, 1.15 + sin(a3 * 2.0) * 0.12, sin(a3) * rr2), Vector3.ONE, 8, 4)
	# soft warm light from the lights
	var o := OmniLight3D.new()
	o.position = Vector3(0, 1.15, 0)
	o.light_color = Color(1.0, 0.86, 0.6)
	o.light_energy = 0.9
	o.omni_range = 3.4
	o.shadow_enabled = false
	root.add_child(o)
	# drifting petals
	var pet := CPUParticles3D.new()
	pet.position = Vector3(0, 1.25, 0)
	pet.amount = 16
	pet.lifetime = 4.0
	pet.preprocess = 3.5
	pet.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	pet.emission_sphere_radius = 0.4
	pet.direction = Vector3(0.3, -1, 0)
	pet.spread = 30.0
	pet.gravity = Vector3(0.05, -0.18, 0)
	pet.initial_velocity_min = 0.05
	pet.initial_velocity_max = 0.2
	pet.angular_velocity_min = -60.0
	pet.angular_velocity_max = 60.0
	pet.scale_amount_min = 0.7
	pet.scale_amount_max = 1.2
	var petal := BoxMesh.new()
	petal.size = Vector3(0.04, 0.008, 0.03)
	petal.material = _toon(Color(1.0, 0.78, 0.86), 0.25)
	pet.mesh = petal
	root.add_child(pet)
	return root


## 4 · CRYSTAL PLANT — a bioluminescent succulent of glowing gem shards rising
## from a gold-trimmed obsidian geode, a hovering keystone gem above. Epic: pure
## emission magic, gold geode trim, a floating jewel + a rising-mote aura.
static func build_crystal_plant() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.34)
	# dark obsidian geode pot with a faceted look + gold trim
	var obsidian := _toon(Color(0.12, 0.13, 0.20), 0.3, true, 0.6)
	var gold := _metal(Color(0.96, 0.80, 0.36), 0.9)
	_cyl(root, 0.20, 0.16, 0.22, obsidian, Vector3(0, 0.14, 0), Vector3.ZERO, 8)
	# gold base ring + rim band (the geode is set like a jewel)
	_torus(root, 0.17, 0.205, gold, Vector3(0, 0.04, 0), Vector3(PI / 2.0, 0, 0), 8)
	_torus(root, 0.16, 0.21, gold, Vector3(0, 0.25, 0), Vector3(PI / 2.0, 0, 0), 8)
	# four gold prongs holding the geode
	for k in 4:
		var pa := TAU * float(k) / 4.0 + 0.4
		_cyl(root, 0.008, 0.012, 0.22, gold, Vector3(cos(pa) * 0.185, 0.14, sin(pa) * 0.185), Vector3(0, 0, 0), 6)
	# glowing geode interior ring
	_cyl(root, 0.15, 0.15, 0.02, _glow(Color(0.5, 0.85, 1.0), 1.4), Vector3(0, 0.245, 0), Vector3.ZERO, 12)
	# the crystals: tall faceted shards (low-seg cones) in cyan / violet, glowing
	var cyan := _shell(Color(0.45, 0.9, 1.0), 0.55, 1.6)
	var viol := _shell(Color(0.7, 0.5, 1.0), 0.55, 1.5)
	var teal := _shell(Color(0.4, 1.0, 0.85), 0.55, 1.5)
	var shards := [
		[Vector3(0.0, 0.55, 0.0), 0.06, 0.5, cyan, 0.0, 0.0],
		[Vector3(0.10, 0.44, 0.04), 0.045, 0.34, viol, 0.4, 0.5],
		[Vector3(-0.09, 0.40, -0.05), 0.04, 0.28, teal, -0.4, -0.4],
		[Vector3(0.05, 0.38, -0.10), 0.035, 0.24, cyan, 0.3, -0.7],
		[Vector3(-0.06, 0.36, 0.09), 0.035, 0.22, viol, -0.3, 0.7],
		[Vector3(0.12, 0.34, -0.04), 0.03, 0.18, teal, 0.55, -0.2],
	]
	for s in shards:
		var pos: Vector3 = s[0]
		var rr: float = s[1]
		var hh: float = s[2]
		var mat: StandardMaterial3D = s[3]
		var rx: float = s[4]
		var rz: float = s[5]
		# faceted shard = a 6-sided cone (no shadow so the glow stays clean)
		_cone(root, rr, hh, mat, pos, Vector3(rx, 0, rz), 6, true)
		# a brilliant solid core inside each shard
		_cone(root, rr * 0.4, hh * 0.8, _glow(mat.emission.lightened(0.2), 2.2), pos, Vector3(rx, 0, rz), 6, true)
	# a hovering keystone gem above the cluster — the Epic centerpiece
	var keystone := _gem(root, 0.06, Color(0.6, 0.9, 1.0), Vector3(0, 0.82, 0), 2.2)
	# tiny orbiting sparks frozen in a ring around the keystone
	for k in 5:
		var oa := TAU * float(k) / 5.0
		_sphere(root, 0.012, _glow(Color(0.8, 0.95, 1.0), 2.4), Vector3(cos(oa) * 0.12, 0.82, sin(oa) * 0.12), Vector3.ONE, 6, 3, true)
	# real light spilling from the crystals
	var o := OmniLight3D.new()
	o.position = Vector3(0, 0.6, 0)
	o.light_color = Color(0.5, 0.85, 1.0)
	o.light_energy = 1.8
	o.omni_range = 4.5
	o.shadow_enabled = false
	root.add_child(o)
	# rising glow motes
	var mo := CPUParticles3D.new()
	mo.position = Vector3(0, 0.4, 0)
	mo.amount = 12
	mo.lifetime = 2.8
	mo.preprocess = 2.0
	mo.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	mo.emission_sphere_radius = 0.18
	mo.direction = Vector3(0, 1, 0)
	mo.spread = 25.0
	mo.gravity = Vector3(0, 0.12, 0)
	mo.initial_velocity_min = 0.05
	mo.initial_velocity_max = 0.16
	mo.scale_amount_min = 0.4
	mo.scale_amount_max = 1.0
	var mote := SphereMesh.new()
	mote.radius = 0.014
	mote.height = 0.028
	mote.radial_segments = 5
	mote.rings = 2
	mote.material = _glow(Color(0.7, 0.95, 1.0), 2.4)
	mo.mesh = mote
	root.add_child(mo)
	return root


## 5 · CACTUS TRIO — three cheerful desert cacti (saguaro, barrel, prickly pear)
## sharing one painted pot, with flowers + a dotted pattern. Common: desk buddy.
static func build_cactus_trio() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.34)
	# wide painted pot with a zigzag band
	var soil := _pot(root, 0.26, 0.20, 0.24, Color(0.92, 0.55, 0.32), Color(0.86, 0.48, 0.28), true, Color(0.95, 0.80, 0.40))
	var green := _toon(Color(0.36, 0.62, 0.36), 0.25, true, 0.2)
	var green_d := _toon(Color(0.28, 0.52, 0.30), 0.2)
	var spine := _toon(Color(0.95, 0.92, 0.7), 0.15)
	# saguaro (tall, with two arms)
	_capsule(root, 0.07, 0.42, green, Vector3(-0.02, soil + 0.20, 0.0), Vector3.ZERO, 12)
	_capsule(root, 0.035, 0.16, green, Vector3(-0.12, soil + 0.26, 0.0), Vector3(0, 0, 0.9), 10)
	_capsule(root, 0.035, 0.14, green, Vector3(-0.12, soil + 0.34, 0.0), Vector3(0, 0, 0.0), 10)
	_capsule(root, 0.035, 0.16, green, Vector3(0.08, soil + 0.22, 0.0), Vector3(0, 0, -0.9), 10)
	_capsule(root, 0.035, 0.14, green, Vector3(0.08, soil + 0.30, 0.0), Vector3(0, 0, 0.0), 10)
	# vertical ribs on the saguaro body for read
	for k in 6:
		var ra := TAU * float(k) / 6.0
		_capsule(root, 0.006, 0.36, green_d, Vector3(-0.02 + cos(ra) * 0.066, soil + 0.20, sin(ra) * 0.066), Vector3.ZERO, 5)
	# saguaro spine cluster + crown flower
	for k in 5:
		var a := TAU * float(k) / 5.0
		_capsule(root, 0.005, 0.08, spine, Vector3(-0.02 + cos(a) * 0.07, soil + 0.16, sin(a) * 0.06), Vector3.ZERO, 5)
	_sphere(root, 0.05, _toon(Color(1.0, 0.42, 0.55), 0.3), Vector3(-0.02, soil + 0.42, 0), Vector3(1, 0.7, 1), 8, 4)
	_sphere(root, 0.025, _toon(Color(1.0, 0.78, 0.3), 0.3), Vector3(-0.02, soil + 0.44, 0), Vector3.ONE, 6, 3)
	# barrel cactus (front-right, fat + round, ribbed)
	_sphere(root, 0.11, green_d, Vector3(0.16, soil + 0.10, 0.10), Vector3(1.0, 1.2, 1.0), 12, 8)
	for k in 8:
		var a2 := TAU * float(k) / 8.0
		_capsule(root, 0.005, 0.20, spine, Vector3(0.16 + cos(a2) * 0.10, soil + 0.10, 0.10 + sin(a2) * 0.10), Vector3(PI / 2.0 * sin(a2), 0, PI / 2.0 * cos(a2)), 4)
	# a little ring of buds crowning the barrel
	for k in 3:
		var a3 := TAU * float(k) / 3.0 + 0.5
		_sphere(root, 0.028, _toon(Color(1.0, 0.84, 0.36), 0.3), Vector3(0.16 + cos(a3) * 0.04, soil + 0.20, 0.10 + sin(a3) * 0.04), Vector3.ONE, 8, 4)
	# prickly pear (left, two pads)
	_sphere(root, 0.08, green, Vector3(-0.16, soil + 0.10, -0.10), Vector3(1.0, 1.2, 0.4), 12, 7)
	_sphere(root, 0.06, green, Vector3(-0.20, soil + 0.22, -0.10), Vector3(1.0, 1.2, 0.4), 12, 7).rotation = Vector3(0, 0, 0.4)
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.018, _toon(Color(1.0, 0.5, 0.4), 0.25), Vector3(-0.18 + sx * 0.05, soil + 0.18, -0.08), Vector3.ONE, 6, 3)
	# little pebble mulch dots scattered on the soil
	for k in 4:
		var pa := TAU * float(k) / 4.0 + 0.2
		_sphere(root, 0.018, _toon(Color(0.92, 0.72, 0.5), 0.2), Vector3(cos(pa) * 0.14, soil + 0.01, sin(pa) * 0.12), Vector3(1, 0.6, 1), 6, 3)
	return root


## 6 · TOPIARY CAT — a sculpted hedge shaped like a sitting cat in a stone
## planter, leafy texture + a real gold collar with an engraved bell. Rare:
## garden whimsy lifted by the polished-gold trim + glowing eyes.
static func build_topiary_cat() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.4)
	# square stone planter with a gold cap rim
	var stone := _toon(Color(0.80, 0.78, 0.74), 0.25, true, 0.2)
	var gold := _metal(Color(0.94, 0.78, 0.36), 0.9)
	_box(root, Vector3(0.42, 0.26, 0.42), stone, Vector3(0, 0.13, 0))
	_box(root, Vector3(0.46, 0.05, 0.46), gold, Vector3(0, 0.26, 0))
	_box(root, Vector3(0.46, 0.04, 0.46), stone, Vector3(0, 0.01, 0))
	# decorative inset panels with a gold pinstripe
	for sz in [-1.0, 1.0]:
		_box(root, Vector3(0.30, 0.14, 0.01), _toon(Color(0.72, 0.70, 0.66), 0.2), Vector3(0, 0.13, sz * 0.215))
		_box(root, Vector3(0.32, 0.012, 0.012), gold, Vector3(0, 0.20, sz * 0.218))
	# soil
	_box(root, Vector3(0.36, 0.03, 0.36), _toon(Color(0.24, 0.16, 0.10), 0.12), Vector3(0, 0.27, 0))
	# the cat — clipped-hedge body from green spheres
	var hedge := _leaf(Color(0.26, 0.50, 0.28), 0.15, 0.3, 0.25)
	var hedge_d := _leaf(Color(0.20, 0.42, 0.24), 0.12, 0.3, 0.2)
	# haunches / seated body
	_sphere(root, 0.20, hedge, Vector3(0, 0.46, -0.02), Vector3(1.0, 1.1, 0.95), 16, 9)
	# chest
	_sphere(root, 0.14, hedge, Vector3(0, 0.52, 0.12), Vector3(1.0, 1.0, 0.9), 14, 7)
	# head
	_sphere(root, 0.13, hedge, Vector3(0, 0.74, 0.10), Vector3(1.05, 0.95, 1.0), 16, 8)
	# ears
	for sx in [-1.0, 1.0]:
		_cone(root, 0.05, 0.12, hedge_d, Vector3(sx * 0.08, 0.86, 0.08), Vector3.ZERO, 6)
	# front paws
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.05, hedge, Vector3(sx * 0.08, 0.32, 0.18), Vector3(1.0, 0.8, 1.3), 10, 5)
	# curled tail
	_capsule(root, 0.04, 0.24, hedge_d, Vector3(0.18, 0.40, -0.10), Vector3(0.4, 0, -0.6), 10)
	_sphere(root, 0.06, hedge, Vector3(0.22, 0.52, -0.02), Vector3.ONE, 10, 5)
	# clipped-leaf texture: little tufts dotted on the body
	for k in 14:
		var a := TAU * float(k) / 14.0
		var ry := 0.40 + (k % 4) * 0.10
		_sphere(root, 0.03, hedge if k % 2 == 0 else hedge_d, Vector3(cos(a) * 0.20, ry, sin(a) * 0.16 - 0.02), Vector3(1.0, 0.7, 1.0), 6, 3)
	# cute face: glowing eyes + a little nose
	_sphere(root, 0.022, _glow(Color(0.5, 1.0, 0.7), 1.8), Vector3(-0.05, 0.76, 0.21), Vector3.ONE, 8, 4, true)
	_sphere(root, 0.022, _glow(Color(0.5, 1.0, 0.7), 1.8), Vector3(0.05, 0.76, 0.21), Vector3.ONE, 8, 4, true)
	_sphere(root, 0.014, _toon(Color(0.95, 0.55, 0.6), 0.3), Vector3(0, 0.71, 0.225), Vector3(1.2, 0.8, 1.0), 6, 3)
	# real GOLD collar with a faceted amber bell — the rare flourish
	_torus(root, 0.10, 0.122, gold, Vector3(0, 0.64, 0.12), Vector3(1.2, 0, 0), 16)
	_sphere(root, 0.035, gold, Vector3(0, 0.585, 0.18), Vector3.ONE, 10, 5)
	_sphere(root, 0.012, _glow(Color(1.0, 0.78, 0.3), 1.6), Vector3(0, 0.555, 0.205), Vector3.ONE, 6, 3, true)
	# a faceted gemstone charm hanging on the collar
	_gem(root, 0.028, Color(1.0, 0.55, 0.65), Vector3(0, 0.54, 0.16), 1.4)
	return root


## 7 · HANGING FERN — a lush trailing fern in a macramé-style hanger on a brass
## hook. Built to HANG: the hook is at the top (~y=1.3), fronds spill downward.
## Uncommon: cozy boho greenery — the brass ring + beaded cords sell it.
static func build_hanging_fern() -> Node3D:
	var root := Node3D.new()
	# brass ceiling hook + cord up high
	var rope := _toon(Color(0.80, 0.70, 0.52), 0.2)
	var bead_c := _toon(Color(0.86, 0.62, 0.40), 0.25, true, 0.3)
	_torus(root, 0.02, 0.04, _metal(Color(0.90, 0.72, 0.34), 0.85), Vector3(0, 1.30, 0), Vector3(PI / 2.0, 0, 0), 12)
	# three macramé cords from the hook down to the pot rim
	for k in 3:
		var a := TAU * float(k) / 3.0
		var px := cos(a) * 0.16
		var pz := sin(a) * 0.16
		_cyl(root, 0.008, 0.008, 0.46, rope, Vector3(px * 0.5, 1.05, pz * 0.5), Vector3(pz * 0.6, 0, -px * 0.6), 5)
		# stacked knotted beads
		_sphere(root, 0.022, bead_c, Vector3(px * 0.78, 0.92, pz * 0.78), Vector3.ONE, 6, 3)
		_sphere(root, 0.02, rope, Vector3(px, 0.84, pz), Vector3.ONE, 6, 3)
	# the hanging pot
	_cyl(root, 0.20, 0.15, 0.22, _toon(Color(0.90, 0.86, 0.78), 0.28, true, 0.25), Vector3(0, 0.72, 0), Vector3.ZERO, 18)
	_torus(root, 0.16, 0.21, _metal(Color(0.88, 0.70, 0.36), 0.8), Vector3(0, 0.83, 0), Vector3(PI / 2.0, 0, 0), 18)
	_cyl(root, 0.16, 0.16, 0.03, _toon(Color(0.22, 0.15, 0.10), 0.1), Vector3(0, 0.82, 0), Vector3.ZERO, 16)
	# arching + trailing fronds — feathery, spilling down and out
	var fr_a := _leaf(Color(0.30, 0.58, 0.30), 0.55, 0.9)
	var fr_b := _leaf(Color(0.24, 0.50, 0.28), 0.5, 0.9)
	for k in 11:
		var a := TAU * float(k) / 11.0
		var droop := 0.6 + (k % 3) * 0.2     # how far it spills
		var fm: ShaderMaterial = fr_a if k % 2 == 0 else fr_b
		# a frond = a stem of paired leaflets, curving down
		var segs := 5
		for i in segs:
			var t := float(i) / float(segs - 1)
			var rad := 0.14 + t * 0.22 * droop
			var fy := 0.86 - t * t * (0.55 + droop * 0.3)
			var px := cos(a) * rad
			var pz := sin(a) * rad
			# the central leaflet pair
			_blade(root, 0.06, 0.12 - t * 0.05, fm, Vector3(px, fy, pz), Vector3(-0.5 - t, a + PI / 2.0, 0))
			if i > 0:
				_blade(root, 0.04, 0.07, fm, Vector3(px, fy - 0.02, pz), Vector3(-0.3 - t, a + PI / 2.0 + 0.5, 0))
				_blade(root, 0.04, 0.07, fm, Vector3(px, fy - 0.02, pz), Vector3(-0.3 - t, a + PI / 2.0 - 0.5, 0))
	# a fuller mound on top of the soil
	for k in 7:
		var a2 := TAU * float(k) / 7.0
		_blade(root, 0.07, 0.18, fr_a, Vector3(cos(a2) * 0.08, 0.92, sin(a2) * 0.08), Vector3(-0.9, a2, 0))
	# a couple of tiny coral spore-bells nestled in the fronds for charm
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.016, _toon(Color(1.0, 0.6, 0.5), 0.3), Vector3(sx * 0.18, 0.78, 0.02), Vector3(1, 1.3, 1), 6, 3)
	return root


## 8 · SUNFLOWER PATCH — a happy cluster of tall sunflowers + grass in a wooden
## crate planter, faces turned to the sun. Common: instant cheer + warm color.
static func build_sunflower_patch() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.42)
	# wooden crate planter with slats
	var wood := _toon(Color(0.74, 0.54, 0.34), 0.2, true, 0.15)
	var wood_d := _toon(Color(0.62, 0.44, 0.28), 0.18)
	_box(root, Vector3(0.6, 0.28, 0.4), wood, Vector3(0, 0.15, 0))
	for sz in [-1.0, 1.0]:
		for i in 3:
			_box(root, Vector3(0.6, 0.02, 0.01), wood_d, Vector3(0, 0.07 + i * 0.08, sz * 0.205))
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.03, 0.34, 0.42), wood_d, Vector3(sx * 0.30, 0.16, 0))
	# soil + grass
	_box(root, Vector3(0.54, 0.03, 0.34), _toon(Color(0.22, 0.15, 0.10), 0.1), Vector3(0, 0.29, 0))
	var grass := _leaf(Color(0.40, 0.62, 0.30), 0.6, 0.4)
	for k in 14:
		var gx := randf_range(-0.24, 0.24)
		var gz := randf_range(-0.14, 0.14)
		_blade(root, 0.03, 0.14, grass, Vector3(gx, 0.36, gz), Vector3(randf_range(-0.3, 0.3), randf_range(0, TAU), 0))
	# three sunflowers of varying height
	var stem_c := _toon(Color(0.32, 0.54, 0.26), 0.2)
	var leaf_c := _leaf(Color(0.34, 0.58, 0.28), 0.55, 0.7)
	var petal := _toon(Color(1.0, 0.78, 0.18), 0.32, true, 0.25)
	var petal_d := _toon(Color(0.96, 0.66, 0.12), 0.3, true, 0.2)
	var center := _toon(Color(0.42, 0.26, 0.14), 0.2)
	var heads := [
		[Vector3(0.0, 0.0, 0.04), 0.70, 0.14],
		[Vector3(-0.18, 0.0, -0.06), 0.56, 0.11],
		[Vector3(0.18, 0.0, -0.04), 0.50, 0.10],
	]
	for h in heads:
		var basep: Vector3 = h[0]
		var stem_h: float = h[1]
		var flr: float = h[2]
		var sy := 0.30 + stem_h * 0.5
		_cyl(root, 0.018, 0.024, stem_h, stem_c, basep + Vector3(0, sy, 0), Vector3(0.05, 0, 0.03), 6)
		# a couple of leaves on the stem
		for sx in [-1.0, 1.0]:
			_blade(root, 0.08, 0.14, leaf_c, basep + Vector3(sx * 0.05, 0.30 + stem_h * 0.5, 0), Vector3(-0.3, sx * 0.8, sx * 0.5))
		var hp := basep + Vector3(0, 0.30 + stem_h, 0.02)
		# face turned up toward +z/+y — two staggered petal rings for fullness
		for ring in 2:
			var n := 12
			var roff := 0.0 if ring == 0 else PI / 12.0
			var pm: ShaderMaterial = petal if ring == 0 else petal_d
			var pscale := 1.0 if ring == 0 else 0.78
			for k in n:
				var a := TAU * float(k) / float(n) + roff
				_cone(root, flr * 0.4 * pscale, flr * 1.1 * pscale, pm, hp + Vector3(cos(a) * flr * 0.9 * pscale, sin(a) * flr * 0.9 * 0.5 * pscale, 0.05 - ring * 0.02), Vector3(-0.5, 0, a + PI / 2.0), 4)
		# brown seed center (front) + back disc
		_sphere(root, flr * 0.6, center, hp + Vector3(0, 0, 0.07), Vector3(1.0, 1.0, 0.5), 14, 7).rotation.x = -0.5
		# a sprinkle of seed dots in the center
		for k in 6:
			var sa := TAU * float(k) / 6.0
			_sphere(root, 0.01, _toon(Color(0.30, 0.18, 0.08), 0.15), hp + Vector3(cos(sa) * flr * 0.3, sin(sa) * flr * 0.15, 0.10), Vector3.ONE, 5, 3)
		_cyl(root, flr * 0.7, flr * 0.7, 0.04, _toon(Color(0.30, 0.50, 0.24), 0.18), hp, Vector3(PI / 2.0 - 0.5, 0, 0), 14)
	# a little bee hovering by the tallest bloom
	_sphere(root, 0.018, _toon(Color(1.0, 0.82, 0.2), 0.3), Vector3(0.16, 1.02, 0.18), Vector3(1.2, 0.9, 1.0), 6, 3)
	_box(root, Vector3(0.022, 0.018, 0.006), _toon(Color(0.12, 0.12, 0.14), 0.2), Vector3(0.16, 1.02, 0.19))
	return root


## 9 · POTTED PALM — a tall fan palm in a woven basket with brass bands, big
## arching fronds and a couple of coconuts. Uncommon: tropical, breezy, fills a
## room corner — the polished brass bands are the cohesive upgrade.
static func build_potted_palm() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.4)
	# woven basket pot
	var basket := _toon(Color(0.80, 0.62, 0.36), 0.22, true, 0.2)
	var brass := _metal(Color(0.90, 0.72, 0.32), 0.85)
	_cyl(root, 0.24, 0.20, 0.34, basket, Vector3(0, 0.20, 0), Vector3.ZERO, 22)
	# brass woven horizontal bands
	for i in 4:
		_torus(root, 0.205 + i * 0.005, 0.245 + i * 0.005, brass, Vector3(0, 0.10 + i * 0.08, 0), Vector3(PI / 2.0, 0, 0), 22)
	_torus(root, 0.20, 0.26, brass, Vector3(0, 0.37, 0), Vector3(PI / 2.0, 0, 0), 22)
	var soil_y := 0.36
	_cyl(root, 0.18, 0.18, 0.03, _toon(Color(0.22, 0.15, 0.10), 0.1), Vector3(0, soil_y, 0), Vector3.ZERO, 18)
	# ringed trunk (stacked tapering segments with collar rings)
	var trunk := _toon(Color(0.62, 0.48, 0.30), 0.2, true, 0.2)
	for i in 5:
		var ty := soil_y + 0.06 + i * 0.14
		_cyl(root, 0.05 - i * 0.004, 0.06 - i * 0.004, 0.14, trunk, Vector3(0.0, ty, 0), Vector3(0.03 * i, 0, 0.02 * i), 10)
		_torus(root, 0.045, 0.062, trunk, Vector3(0.0, ty + 0.07, 0), Vector3(PI / 2.0, 0, 0), 10)
	var crown := Vector3(0.06, soil_y + 0.78, 0)
	# big arching fronds — each a midrib + leaflets, fanning out + drooping
	var fr := _leaf(Color(0.26, 0.56, 0.30), 0.5, 0.8)
	var fr_d := _leaf(Color(0.20, 0.48, 0.26), 0.45, 0.8)
	for k in 8:
		var a := TAU * float(k) / 8.0
		var fm: ShaderMaterial = fr if k % 2 == 0 else fr_d
		var segs := 5
		for i in segs:
			var t := float(i) / float(segs - 1)
			var rad := 0.10 + t * 0.42
			var fy := crown.y + 0.10 - t * t * 0.45
			var px := crown.x + cos(a) * rad
			var pz := sin(a) * rad
			_blade(root, 0.10 - t * 0.04, 0.20, fm, Vector3(px, fy, pz), Vector3(-0.3 - t * 0.9, a + PI / 2.0, 0))
		# a bright midrib down the frond tip
		_cyl(root, 0.008, 0.012, 0.4, fr_d, Vector3(crown.x + cos(a) * 0.28, crown.y - 0.05, sin(a) * 0.28), Vector3(-0.9, 0, 0).rotated(Vector3.UP, a), 5)
	# a couple of coconuts under the crown
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.05, _toon(Color(0.45, 0.32, 0.20), 0.22, true, 0.2), crown + Vector3(sx * 0.06, -0.04, 0.04), Vector3.ONE, 10, 5)
	return root


## 10 · CARNIVOROUS BOG — a cheeky Venus-flytrap cluster + a glowing pitcher in a
## gold-rimmed mossy bog pot, amber dewdrops + a lured fly. Epic: characterful,
## toothy, with a lure-glow, dripping nectar gems + a hovering victim.
static func build_carnivorous() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.34)
	# dark mossy bog pot with a gold rim
	var pot := _toon(Color(0.34, 0.30, 0.24), 0.25, true, 0.3)
	var gold := _metal(Color(0.92, 0.74, 0.34), 0.85)
	_cyl(root, 0.22, 0.17, 0.24, pot, Vector3(0, 0.15, 0), Vector3.ZERO, 18)
	_torus(root, 0.18, 0.225, gold, Vector3(0, 0.27, 0), Vector3(PI / 2.0, 0, 0), 18)
	# spongy moss top
	_cyl(root, 0.17, 0.17, 0.05, _toon(Color(0.30, 0.46, 0.24), 0.15), Vector3(0, 0.27, 0), Vector3.ZERO, 18)
	for k in 8:
		var a := TAU * float(k) / 8.0
		_sphere(root, 0.03, _toon(Color(0.36, 0.54, 0.26), 0.2), Vector3(cos(a) * 0.12, 0.30, sin(a) * 0.12), Vector3(1.0, 0.6, 1.0), 6, 3)
	# 3 flytrap heads: a green stalk topped by two toothy lobes (open jaws)
	var stalk := _toon(Color(0.40, 0.60, 0.32), 0.2)
	var lobe := _toon(Color(0.45, 0.70, 0.34), 0.3, true, 0.2)
	var maw := _glow(Color(0.95, 0.35, 0.45), 1.0)        # red lure interior
	var tooth := _toon(Color(0.92, 0.95, 0.80), 0.2)
	var traps := [
		[Vector3(0.0, 0.0, 0.0), 0.22, 0.0],
		[Vector3(0.10, 0.0, 0.05), 0.16, 0.6],
		[Vector3(-0.09, 0.0, -0.05), 0.14, -0.5],
	]
	for tr in traps:
		var basep: Vector3 = tr[0]
		var sh: float = tr[1]
		var tilt: float = tr[2]
		var topy := 0.30 + sh
		_cyl(root, 0.018, 0.028, sh, stalk, basep + Vector3(0, 0.30 + sh * 0.5, 0), Vector3(tilt * 0.2, 0, 0), 6)
		var hp := basep + Vector3(sin(tilt) * 0.04, topy, 0)
		# two open lobes (squashed half-spheres)
		for sx in [-1.0, 1.0]:
			var lm := _sphere(root, 0.07, lobe, hp + Vector3(sx * 0.05, 0.02, 0), Vector3(1.0, 1.2, 0.7), 12, 6)
			lm.rotation = Vector3(0, 0, sx * 0.7)
			# red glowing maw lining
			_sphere(root, 0.05, maw, hp + Vector3(sx * 0.03, 0.01, 0), Vector3(0.9, 1.0, 0.5), 10, 5, true).rotation = Vector3(0, 0, sx * 0.7)
			# interlocking teeth along the rim
			for j in 4:
				var jt := -0.05 + j * 0.033
				_cone(root, 0.012, 0.05, tooth, hp + Vector3(sx * 0.10, 0.05, jt), Vector3(0, 0, sx * 1.2), 4)
		# a faceted nectar dewdrop on the lure of each trap
		_gem(root, 0.018, Color(1.0, 0.7, 0.3), hp + Vector3(0, 0.06, 0), 1.4)
	# a glowing pitcher plant rising at the back
	var pitcher := _toon(Color(0.55, 0.72, 0.40), 0.3, true, 0.3)
	_cyl(root, 0.05, 0.025, 0.30, pitcher, Vector3(-0.04, 0.46, -0.10), Vector3(0.2, 0, 0.1), 12)
	_torus(root, 0.035, 0.06, _toon(Color(0.85, 0.40, 0.40), 0.3), Vector3(-0.05, 0.61, -0.10), Vector3(PI / 2.0 - 0.2, 0, 0), 14)
	_sphere(root, 0.03, _glow(Color(0.6, 1.0, 0.6), 1.6), Vector3(-0.05, 0.60, -0.10), Vector3(1, 0.6, 1), 8, 4, true)
	# a thin glowing nectar trail running down inside the pitcher mouth
	_sphere(root, 0.02, _glow(Color(0.7, 1.0, 0.7), 1.2), Vector3(-0.045, 0.52, -0.10), Vector3(0.6, 1.4, 0.6), 6, 3, true)
	# soft lure light + a hovering fly being lured in
	var o := OmniLight3D.new()
	o.position = Vector3(0, 0.5, 0)
	o.light_color = Color(0.7, 1.0, 0.7)
	o.light_energy = 0.9
	o.omni_range = 2.8
	o.shadow_enabled = false
	root.add_child(o)
	_sphere(root, 0.012, _toon(Color(0.15, 0.15, 0.18), 0.2), Vector3(0.05, 0.52, 0.14), Vector3.ONE, 6, 3)
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.016, 0.004, 0.01), _shell(Color(0.8, 0.85, 1.0), 0.5, 0.4), Vector3(0.05 + sx * 0.012, 0.535, 0.14))
	return root


## 11 · ZEN GARDEN — a raked-sand tray with mossy stacked stones, a tiny maple
## sprig, a gilded stone lantern set with gems, a glowing koi-pond chip and a
## hovering jade orb. Legendary: a serene centerpiece — the most gold, gems and
## glow in the set, plus a floating relic, so it reads top-tier at a glance.
static func build_zen_garden() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.6)
	# dark wood tray with a gold inlay edge
	var wood := _toon(Color(0.34, 0.24, 0.18), 0.25, true, 0.3)
	var gold := _metal(Color(1.0, 0.82, 0.34), 0.9)
	var gold_d := _metal(Color(0.82, 0.64, 0.24), 0.8)
	_box(root, Vector3(0.92, 0.10, 0.72), wood, Vector3(0, 0.07, 0))
	# gold inlay frame around the rim
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.02, 0.04, 0.70), gold, Vector3(sx * 0.45, 0.115, 0))
	for sz in [-1.0, 1.0]:
		_box(root, Vector3(0.90, 0.04, 0.02), gold, Vector3(0, 0.115, sz * 0.35))
	# gold corner studs
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_sphere(root, 0.022, gold, Vector3(sx * 0.45, 0.13, sz * 0.35), Vector3.ONE, 8, 4)
	# raked white sand bed
	var sand := _toon(Color(0.92, 0.90, 0.84), 0.2)
	_box(root, Vector3(0.84, 0.04, 0.64), sand, Vector3(0, 0.13, 0))
	# raked concentric rings (thin grooves) around the main stone group
	for i in 3:
		_torus(root, 0.14 + i * 0.07, 0.155 + i * 0.07, _toon(Color(0.84, 0.82, 0.76), 0.15), Vector3(-0.18, 0.145, 0.08), Vector3(PI / 2.0, 0, 0), 26)
	# straight rake lines on the open side
	for i in 4:
		_box(root, Vector3(0.30, 0.012, 0.012), _toon(Color(0.84, 0.82, 0.76), 0.15), Vector3(0.24, 0.15, -0.18 + i * 0.10))
	# stacked balancing stones (a cairn) with moss
	var stone := _toon(Color(0.42, 0.44, 0.46), 0.25, true, 0.2)
	var stone_d := _toon(Color(0.34, 0.36, 0.40), 0.2)
	var moss := _toon(Color(0.34, 0.52, 0.28), 0.15)
	_sphere(root, 0.10, stone, Vector3(-0.18, 0.20, 0.08), Vector3(1.2, 0.7, 1.1), 12, 6)
	_sphere(root, 0.07, stone_d, Vector3(-0.18, 0.30, 0.08), Vector3(1.1, 0.8, 1.0), 12, 6)
	_sphere(root, 0.05, stone, Vector3(-0.17, 0.38, 0.08), Vector3(1.0, 0.9, 1.0), 10, 5)
	_sphere(root, 0.04, moss, Vector3(-0.20, 0.235, 0.12), Vector3(1.0, 0.5, 1.0), 6, 3)
	_sphere(root, 0.03, moss, Vector3(-0.15, 0.34, 0.06), Vector3(1.0, 0.5, 1.0), 6, 3)
	# a second low mossy boulder
	_sphere(root, 0.08, stone_d, Vector3(0.10, 0.18, 0.18), Vector3(1.3, 0.6, 1.1), 12, 6)
	_sphere(root, 0.04, moss, Vector3(0.08, 0.21, 0.20), Vector3(1.2, 0.4, 1.0), 6, 3)
	# tiny red maple sprig by the stones
	_cyl(root, 0.01, 0.015, 0.16, _toon(Color(0.40, 0.27, 0.20), 0.2), Vector3(0.0, 0.22, 0.04), Vector3(0, 0, 0.2), 6)
	for k in 6:
		var a := TAU * float(k) / 6.0
		_sphere(root, 0.04, _leaf(Color(0.86, 0.32, 0.24), 0.3, 0.3), Vector3(cos(a) * 0.07, 0.31 + sin(a) * 0.02, 0.04 + sin(a) * 0.05), Vector3(1.3, 0.4, 1.3), 8, 4)
	# a gilded stone lantern (tōrō) set with gems — the legendary flourish
	var grey := _toon(Color(0.62, 0.62, 0.60), 0.3, true, 0.3)
	var lx := 0.30
	_cyl(root, 0.06, 0.08, 0.05, gold_d, Vector3(lx, 0.16, 0.16), Vector3.ZERO, 8)        # gold footed base
	_cyl(root, 0.025, 0.03, 0.12, grey, Vector3(lx, 0.24, 0.16), Vector3.ZERO, 8)          # post
	_torus(root, 0.026, 0.04, gold, Vector3(lx, 0.24, 0.16), Vector3(PI / 2.0, 0, 0), 8)   # gold post ring
	_cyl(root, 0.075, 0.055, 0.04, gold_d, Vector3(lx, 0.31, 0.16), Vector3.ZERO, 8)       # gold platform
	# the light box with a warm glow + gold corner posts
	_box(root, Vector3(0.10, 0.09, 0.10), grey, Vector3(lx, 0.37, 0.16))
	_box(root, Vector3(0.06, 0.07, 0.11), _glow(Color(1.0, 0.78, 0.4), 1.8), Vector3(lx, 0.37, 0.16))
	_box(root, Vector3(0.11, 0.07, 0.06), _glow(Color(1.0, 0.78, 0.4), 1.8), Vector3(lx, 0.37, 0.16))
	for cx in [-1.0, 1.0]:
		for cz in [-1.0, 1.0]:
			_box(root, Vector3(0.012, 0.10, 0.012), gold, Vector3(lx + cx * 0.05, 0.37, 0.16 + cz * 0.05))
	# gold pagoda roof + jewelled finial
	_cone(root, 0.10, 0.07, gold, Vector3(lx, 0.45, 0.16), Vector3.ZERO, 6)
	_gem(root, 0.025, Color(0.4, 0.95, 0.7), Vector3(lx, 0.49, 0.16), 1.8)   # jade finial gem
	# warm lantern light
	var o := OmniLight3D.new()
	o.position = Vector3(lx, 0.37, 0.16)
	o.light_color = Color(1.0, 0.78, 0.42)
	o.light_energy = 1.2
	o.omni_range = 3.2
	o.shadow_enabled = false
	root.add_child(o)
	# a glowing koi-pond chip set into the sand, ringed in gold
	_torus(root, 0.09, 0.105, gold, Vector3(0.18, 0.15, 0.18), Vector3(PI / 2.0, 0, 0), 18)
	_cyl(root, 0.09, 0.09, 0.012, _shell(Color(0.4, 0.8, 1.0), 0.5, 0.9), Vector3(0.18, 0.155, 0.18), Vector3.ZERO, 18)
	_sphere(root, 0.02, _glow(Color(1.0, 0.6, 0.3), 1.6), Vector3(0.18, 0.16, 0.18), Vector3(1.6, 0.3, 1.0), 8, 4, true)
	# a hovering jade relic orb above the garden — the floating legendary accent
	_gem(root, 0.05, Color(0.45, 0.95, 0.7), Vector3(-0.10, 0.62, 0.08), 1.6)
	var ho := OmniLight3D.new()
	ho.position = Vector3(-0.10, 0.62, 0.08)
	ho.light_color = Color(0.5, 1.0, 0.75)
	ho.light_energy = 0.7
	ho.omni_range = 1.8
	ho.shadow_enabled = false
	root.add_child(ho)
	# drifting incense-style motes from the lantern
	var mo := CPUParticles3D.new()
	mo.position = Vector3(lx, 0.5, 0.16)
	mo.amount = 8
	mo.lifetime = 3.0
	mo.preprocess = 2.5
	mo.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	mo.emission_sphere_radius = 0.03
	mo.direction = Vector3(0, 1, 0)
	mo.spread = 12.0
	mo.gravity = Vector3(0.02, 0.1, 0)
	mo.initial_velocity_min = 0.02
	mo.initial_velocity_max = 0.08
	mo.scale_amount_min = 0.4
	mo.scale_amount_max = 1.0
	var mote := SphereMesh.new()
	mote.radius = 0.01
	mote.height = 0.02
	mote.radial_segments = 5
	mote.rings = 2
	mote.material = _glow(Color(1.0, 0.85, 0.5), 1.6)
	mo.mesh = mote
	root.add_child(mo)
	return root
