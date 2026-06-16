# Hey Verse — premium procedural BUILDING module.
# id: desert_palace  ·  Legendary Desert Palace
# Golden onion domes, horseshoe arches, intricate lattice screens, a colonnaded
# courtyard with a fountain + palms, a jewelled throne hall — gold + ivory + turquoise.
#
# LUXURY ENHANCE pass: richer silhouette (great drum + 4 corner towers + minarets),
# more tasteful brass/gold trim (string courses, finials, balustrades), statues,
# twin entrance fountains, jewelled balconies, fluted columns, dormer cupolas,
# generous landscaping, glowing lattice windows, and interior showpieces — a grand
# stair, a colossal chandelier, a marble fireplace-style brazier, and a jewelled
# throne — all over a CLEAN, WALKABLE, OPEN ground floor with NO front wall.
#
# Self-contained: loads res://toon.gdshader + res://outline.gdshader by path with
# guards and a StandardMaterial3D fallback so this parses + runs standalone.
# Built at the origin; entrance faces +z.
class_name VerseBuildingDesertPalace
extends RefCounted

# ----------------------------------------------------------------------------
# Palette (warm desert luxury)
# ----------------------------------------------------------------------------
const IVORY: Color    = Color(0.96, 0.93, 0.84)   # stucco walls
const IVORY_HI: Color = Color(0.99, 0.97, 0.90)   # bright trim stucco
const SANDST: Color   = Color(0.86, 0.76, 0.58)   # carved sandstone
const GOLD: Color     = Color(1.00, 0.80, 0.28)   # domes / brass trim
const GOLD_HI: Color  = Color(1.00, 0.90, 0.55)   # polished gold highlight
const GOLD_DK: Color  = Color(0.78, 0.56, 0.16)   # deep gold
const BRASS: Color    = Color(0.86, 0.66, 0.30)   # warm brass trim
const TURQ: Color     = Color(0.16, 0.72, 0.72)   # turquoise tile
const TURQ_DK: Color  = Color(0.07, 0.45, 0.50)
const LAPIS: Color    = Color(0.13, 0.26, 0.62)   # deep blue mosaic
const CRIMSON: Color  = Color(0.62, 0.13, 0.18)   # throne carpet
const TERRA: Color    = Color(0.74, 0.36, 0.22)   # terracotta floor
const MARBLE: Color   = Color(0.93, 0.90, 0.86)   # ivory marble floor
const MARBLE_DK: Color = Color(0.80, 0.76, 0.70)  # veined marble shade
const PALM_TR: Color  = Color(0.45, 0.33, 0.20)   # palm trunk
const PALM_LF: Color  = Color(0.30, 0.56, 0.26)   # palm fronds
const WATER: Color    = Color(0.30, 0.74, 0.82)   # fountain water
const GLOWWARM: Color = Color(1.0, 0.86, 0.55)    # lantern / window glow
const GLOWCOOL: Color = Color(0.55, 0.82, 0.92)   # cool tile glow
const HEDGE: Color    = Color(0.27, 0.45, 0.24)
const EMERALD: Color  = Color(0.10, 0.60, 0.38)   # jewel accent
const RUBY: Color     = Color(0.74, 0.10, 0.22)   # jewel accent

# ----------------------------------------------------------------------------
# Shader cache
# ----------------------------------------------------------------------------
static var _toon_sh: Shader = null
static var _outline_sh: Shader = null
static var _loaded: bool = false

static func _ensure_shaders() -> void:
	if _loaded:
		return
	_loaded = true
	if ResourceLoader.exists("res://toon.gdshader"):
		var s: Resource = ResourceLoader.load("res://toon.gdshader")
		if s is Shader:
			_toon_sh = s
	if ResourceLoader.exists("res://outline.gdshader"):
		var o: Resource = ResourceLoader.load("res://outline.gdshader")
		if o is Shader:
			_outline_sh = o

# ----------------------------------------------------------------------------
# Material helpers
# ----------------------------------------------------------------------------
static func _outline_pass(thickness: float) -> Material:
	if _outline_sh != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = _outline_sh
		m.set_shader_parameter("thickness", thickness)
		m.set_shader_parameter("line_color", Color(0.08, 0.06, 0.05, 1.0))
		return m
	return null

static func _toon(col: Color, rim: float, spec: float, outline: float) -> Material:
	_ensure_shaders()
	if _toon_sh != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = _toon_sh
		m.set_shader_parameter("albedo", col)
		m.set_shader_parameter("rim_strength", rim)
		m.set_shader_parameter("spec_strength", spec)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		var op: Material = _outline_pass(outline)
		if op != null:
			m.next_pass = op
		return m
	# Fallback so the module runs standalone.
	var sm: StandardMaterial3D = StandardMaterial3D.new()
	sm.albedo_color = col
	sm.roughness = 0.85
	sm.metallic = 0.0
	return sm

static func _metal(col: Color, rim: float) -> Material:
	# brushed gold / brass — high rim + spec sparkle
	var m: Material = _toon(col, rim, 0.55, 0.020)
	if m is StandardMaterial3D:
		var sm: StandardMaterial3D = m as StandardMaterial3D
		sm.metallic = 0.9
		sm.roughness = 0.28
	return m

static func _gloss(col: Color) -> Material:
	# glazed tile / polished marble
	var m: Material = _toon(col, 0.30, 0.40, 0.016)
	if m is StandardMaterial3D:
		(m as StandardMaterial3D).roughness = 0.25
	return m

static func _glass(col: Color) -> Material:
	var m: Material = _toon(col, 0.5, 0.6, 0.012)
	if m is StandardMaterial3D:
		var sm: StandardMaterial3D = m as StandardMaterial3D
		sm.albedo_color = Color(col.r, col.g, col.b, 0.55)
		sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		sm.roughness = 0.1
	return m

static func _jewel(col: Color) -> Material:
	# faceted gem — bright rim, hard spec, slight translucence in fallback
	var m: Material = _toon(col, 0.7, 0.8, 0.014)
	if m is StandardMaterial3D:
		var sm: StandardMaterial3D = m as StandardMaterial3D
		sm.albedo_color = Color(col.r, col.g, col.b, 0.78)
		sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		sm.roughness = 0.05
		sm.metallic = 0.2
	return m

static func _glow(col: Color, energy: float) -> Material:
	_ensure_shaders()
	if _toon_sh != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = _toon_sh
		m.set_shader_parameter("albedo", col)
		m.set_shader_parameter("rim_strength", 0.6)
		m.set_shader_parameter("spec_strength", 0.0)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		# emissive cheat via fallback path below isn't available on shader mat,
		# so pair with a real OmniLight where it matters; tint stays bright.
		return m
	var sm: StandardMaterial3D = StandardMaterial3D.new()
	sm.albedo_color = col
	sm.emission_enabled = true
	sm.emission = col
	sm.emission_energy_multiplier = energy
	return sm

# ----------------------------------------------------------------------------
# Primitive helpers — every one returns a MeshInstance3D parented + placed.
# ----------------------------------------------------------------------------
static func _box(parent: Node3D, size: Vector3, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var bm: BoxMesh = BoxMesh.new()
	bm.size = size
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _cyl(parent: Node3D, rt: float, rb: float, h: float, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var cm: CylinderMesh = CylinderMesh.new()
	cm.top_radius = rt
	cm.bottom_radius = rb
	cm.height = h
	cm.radial_segments = 20
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _ball(parent: Node3D, r: float, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var sm: SphereMesh = SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 22
	sm.rings = 14
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _torus(parent: Node3D, inner: float, outer: float, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var tm: TorusMesh = TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 28
	tm.ring_segments = 16
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _prism(parent: Node3D, size: Vector3, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var pm: PrismMesh = PrismMesh.new()
	pm.size = size
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _lamp(parent: Node3D, pos: Vector3, col: Color, energy: float, rng: float) -> void:
	var l: OmniLight3D = OmniLight3D.new()
	l.position = pos
	l.light_color = col
	l.light_energy = energy
	l.omni_range = rng
	l.omni_attenuation = 1.4
	parent.add_child(l)

# ----------------------------------------------------------------------------
# Composite helpers
# ----------------------------------------------------------------------------

# An onion (bulb) dome built from stacked spheres + a finial — the signature shape.
# A gold gore-banding (vertical ribs) gives it the lavish, fluted melon-dome look.
static func _onion_dome(parent: Node3D, base_r: float, pos: Vector3, mat: Material, gold: Material) -> void:
	# bulb: a slightly-squashed sphere that pinches at top, then a tip
	var d: Node3D = Node3D.new()
	d.position = pos
	parent.add_child(d)
	var lower: MeshInstance3D = _ball(d, base_r, Vector3(0, base_r * 0.55, 0), mat)
	lower.scale = Vector3(1.12, 1.18, 1.12)
	var mid: MeshInstance3D = _ball(d, base_r * 0.74, Vector3(0, base_r * 1.55, 0), mat)
	mid.scale = Vector3(1.0, 1.25, 1.0)
	# vertical gold ribs (gores) wrapping the bulb — luxury detailing
	for rib: float in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]:
		var rr: float = base_r * 1.14
		var rib_box: MeshInstance3D = _box(d, Vector3(0.07, base_r * 1.7, 0.07), Vector3(sin(deg_to_rad(rib)) * rr, base_r * 0.95, cos(deg_to_rad(rib)) * rr), gold)
		rib_box.rotation_degrees = Vector3(0, rib, 0)
	# pinch / neck
	_cyl(d, base_r * 0.30, base_r * 0.55, base_r * 0.45, Vector3(0, base_r * 2.25, 0), mat)
	# spire tip
	_cyl(d, 0.0, base_r * 0.30, base_r * 0.9, Vector3(0, base_r * 2.85, 0), gold)
	# finial ball + crescent
	_ball(d, base_r * 0.18, Vector3(0, base_r * 3.45, 0), gold)
	_torus(d, base_r * 0.10, base_r * 0.22, Vector3(0, base_r * 3.75, 0), gold)
	# gold banding ring at the dome base
	_torus(d, base_r * 0.90, base_r * 1.18, Vector3(0, base_r * 0.18, 0), gold)

# A horseshoe (keyhole) arch opening cut into a wall plane, framed in gold.
# We don't CSG; we suggest the arch with a thick frame ring + keystone so the
# entry reads clearly while the opening stays walkable.
static func _horseshoe_arch(parent: Node3D, w: float, h: float, pos: Vector3, frame: Material, gold: Material) -> void:
	var a: Node3D = Node3D.new()
	a.position = pos
	parent.add_child(a)
	# jambs (sides of the opening)
	_box(a, Vector3(0.22, h, 0.40), Vector3(-w * 0.5, h * 0.5, 0), frame)
	_box(a, Vector3(0.22, h, 0.40), Vector3(w * 0.5, h * 0.5, 0), frame)
	# the round/horseshoe head — a torus arc above the opening
	var ring: MeshInstance3D = _torus(a, w * 0.40, w * 0.62, Vector3(0, h, 0), frame)
	ring.rotation_degrees = Vector3(90, 0, 0)
	# gold keystone + voussoir studs
	_prism(a, Vector3(0.34, 0.42, 0.42), Vector3(0, h + w * 0.52, 0), gold)
	for ang: float in [-50.0, -25.0, 25.0, 50.0]:
		var rad: float = deg_to_rad(ang)
		var rr: float = w * 0.51
		_ball(a, 0.10, Vector3(sin(rad) * rr, h + cos(rad) * rr, 0.21), gold)

# Lattice (mashrabiya) screen — a grid of small bars in a thin frame.
static func _lattice(parent: Node3D, w: float, h: float, pos: Vector3, mat: Material, gold: Material) -> void:
	var l: Node3D = Node3D.new()
	l.position = pos
	parent.add_child(l)
	# frame
	_box(l, Vector3(w + 0.16, 0.14, 0.14), Vector3(0, h * 0.5, 0), gold)
	_box(l, Vector3(w + 0.16, 0.14, 0.14), Vector3(0, -h * 0.5, 0), gold)
	_box(l, Vector3(0.14, h + 0.16, 0.14), Vector3(-w * 0.5, 0, 0), gold)
	_box(l, Vector3(0.14, h + 0.16, 0.14), Vector3(w * 0.5, 0, 0), gold)
	# diagonal lattice bars both ways
	var n: int = 5
	for i: int in range(-n, n + 1):
		var off: float = float(i) * (w / float(n)) * 0.7
		var d1: MeshInstance3D = _box(l, Vector3(0.05, sqrt(w * w + h * h) * 0.55, 0.05), Vector3(off, 0, 0.03), mat)
		d1.rotation_degrees = Vector3(0, 0, 45)
		var d2: MeshInstance3D = _box(l, Vector3(0.05, sqrt(w * w + h * h) * 0.55, 0.05), Vector3(off, 0, 0.03), mat)
		d2.rotation_degrees = Vector3(0, 0, -45)

# A slender column with a moulded base, fluted shaft and a foliate gold capital.
static func _column(parent: Node3D, h: float, pos: Vector3, shaft: Material, gold: Material) -> void:
	var c: Node3D = Node3D.new()
	c.position = pos
	parent.add_child(c)
	# base
	_box(c, Vector3(0.62, 0.18, 0.62), Vector3(0, 0.09, 0), gold)
	_box(c, Vector3(0.50, 0.16, 0.50), Vector3(0, 0.24, 0), shaft)
	# shaft
	_cyl(c, 0.20, 0.24, h - 0.9, Vector3(0, 0.32 + (h - 0.9) * 0.5, 0), shaft)
	# fluting accents — slim vertical ribs around the shaft
	for fl: float in [0.0, 60.0, 120.0, 180.0, 240.0, 300.0]:
		var fr: float = 0.225
		var fb: MeshInstance3D = _box(c, Vector3(0.04, h - 1.1, 0.04), Vector3(sin(deg_to_rad(fl)) * fr, 0.32 + (h - 0.9) * 0.5, cos(deg_to_rad(fl)) * fr), gold)
		fb.rotation_degrees = Vector3(0, fl, 0)
	# capital (gold, flared)
	_cyl(c, 0.42, 0.22, 0.30, Vector3(0, h - 0.45, 0), gold)
	_box(c, Vector3(0.56, 0.16, 0.56), Vector3(0, h - 0.22, 0), gold)
	# little turquoise ring accent
	_torus(c, 0.20, 0.27, Vector3(0, 0.34, 0), _gloss(TURQ))

# A robed guardian statue on a plinth — flanks the throne and the entry.
static func _statue(parent: Node3D, pos: Vector3, body: Material, gold: Material) -> void:
	var s: Node3D = Node3D.new()
	s.position = pos
	parent.add_child(s)
	# plinth
	_box(s, Vector3(1.0, 0.5, 1.0), Vector3(0, 0.25, 0), gold)
	_box(s, Vector3(0.84, 0.16, 0.84), Vector3(0, 0.58, 0), body)
	# robed body — tapered base
	_cyl(s, 0.30, 0.46, 1.5, Vector3(0, 1.4, 0), body)
	# torso + shoulders
	_box(s, Vector3(0.66, 0.7, 0.42), Vector3(0, 2.4, 0), body)
	# arms folded forward
	_cyl(s, 0.12, 0.12, 0.7, Vector3(-0.32, 2.35, 0.18), body)
	_cyl(s, 0.12, 0.12, 0.7, Vector3(0.32, 2.35, 0.18), body)
	# head
	_ball(s, 0.24, Vector3(0, 2.95, 0), body)
	# turban / gold crown
	_torus(s, 0.16, 0.30, Vector3(0, 3.02, 0), gold)
	_ball(s, 0.12, Vector3(0, 3.28, 0), gold)
	# a held staff with a jewel
	_cyl(s, 0.04, 0.04, 2.0, Vector3(0.42, 2.0, 0.20), gold)
	_ball(s, 0.13, Vector3(0.42, 3.05, 0.20), _jewel(TURQ))

# A tiered fountain — octagonal basin, gold rim, stacked dishes + jets.
static func _fountain(parent: Node3D, pos: Vector3, basin_r: float, marble: Material, gold: Material, turq_dk: Material, water: Material) -> void:
	var f: Node3D = Node3D.new()
	f.position = pos
	parent.add_child(f)
	_cyl(f, basin_r, basin_r + 0.2, 0.7, Vector3(0, 0.35, 0), turq_dk)
	_torus(f, basin_r - 0.1, basin_r + 0.3, Vector3(0, 0.7, 0), gold)
	_cyl(f, basin_r - 0.2, basin_r - 0.2, 0.3, Vector3(0, 0.55, 0), water)  # water surface
	# tiered center
	_cyl(f, 0.6, 0.8, 0.8, Vector3(0, 1.1, 0), marble)
	_cyl(f, 1.1, 0.0, 0.25, Vector3(0, 1.5, 0), water)
	_cyl(f, 0.3, 0.45, 0.7, Vector3(0, 1.9, 0), marble)
	_cyl(f, 0.7, 0.0, 0.2, Vector3(0, 2.25, 0), water)
	_ball(f, 0.30, Vector3(0, 2.55, 0), gold)
	# jet droplets
	for ang4: float in [0.0, 90.0, 180.0, 270.0]:
		_ball(f, 0.10, Vector3(sin(deg_to_rad(ang4)) * 0.6, 2.7, cos(deg_to_rad(ang4)) * 0.6), water)

# A balcony with a turned gold balustrade — bolts onto a wall face.
static func _balcony(parent: Node3D, w: float, pos: Vector3, slab: Material, gold: Material) -> void:
	var b: Node3D = Node3D.new()
	b.position = pos
	parent.add_child(b)
	# floor slab
	_box(b, Vector3(w, 0.18, 1.2), Vector3(0, 0, 0.6), slab)
	# corbels under it
	for cx: float in [-1.0, 1.0]:
		_prism(b, Vector3(0.4, 0.4, 0.8), Vector3(cx * (w * 0.5 - 0.3), -0.3, 0.5), gold)
	# top rail
	_box(b, Vector3(w, 0.10, 0.10), Vector3(0, 0.7, 1.18), gold)
	_box(b, Vector3(0.10, 0.10, 1.2), Vector3(-w * 0.5, 0.7, 0.6), gold)
	_box(b, Vector3(0.10, 0.10, 1.2), Vector3(w * 0.5, 0.7, 0.6), gold)
	# balusters
	var bn: int = int(round(w / 0.45))
	for i: int in range(bn + 1):
		var bx: float = -w * 0.5 + float(i) * (w / float(bn))
		_cyl(b, 0.05, 0.07, 0.62, Vector3(bx, 0.39, 1.18), gold)
	# jewel finials at corners
	for jx: float in [-1.0, 1.0]:
		_ball(b, 0.10, Vector3(jx * w * 0.5, 0.82, 1.18), _jewel(LAPIS))

# A date palm.
static func _palm(parent: Node3D, pos: Vector3, scale: float) -> void:
	var p: Node3D = Node3D.new()
	p.position = pos
	p.scale = Vector3(scale, scale, scale)
	parent.add_child(p)
	var trunk: MeshInstance3D = _cyl(p, 0.16, 0.26, 3.4, Vector3(0, 1.7, 0), _toon(PALM_TR, 0.2, 0.0, 0.016))
	trunk.rotation_degrees = Vector3(0, 0, 4)
	# trunk ring texture
	for i: int in range(5):
		_torus(p, 0.18, 0.28, Vector3(0, 0.5 + float(i) * 0.55, 0), _toon(PALM_TR.darkened(0.15), 0.1, 0.0, 0.012))
	# fronds — radial arc of flattened boxes
	var leaf: Material = _toon(PALM_LF, 0.35, 0.1, 0.014)
	for ang: float in [0.0, 51.0, 102.0, 153.0, 204.0, 255.0, 306.0]:
		var f: MeshInstance3D = _box(p, Vector3(0.10, 1.7, 0.42), Vector3(0, 3.6, 0), leaf)
		f.rotation_degrees = Vector3(58, ang, 0)
		f.position = Vector3(sin(deg_to_rad(ang)) * 0.5, 3.5, cos(deg_to_rad(ang)) * 0.5)
	# date clusters
	for ang2: float in [30.0, 150.0, 270.0]:
		_ball(p, 0.16, Vector3(sin(deg_to_rad(ang2)) * 0.4, 3.2, cos(deg_to_rad(ang2)) * 0.4), _toon(GOLD_DK, 0.2, 0.2, 0.012))

# A slender minaret — drum shaft, balcony gallery, lantern + onion cap.
static func _minaret(parent: Node3D, pos: Vector3, h: float, shaft: Material, gold: Material, glow: Material) -> void:
	var m: Node3D = Node3D.new()
	m.position = pos
	parent.add_child(m)
	# base block
	_box(m, Vector3(1.1, 0.8, 1.1), Vector3(0, 0.4, 0), shaft)
	# tapered shaft
	_cyl(m, 0.42, 0.60, h, Vector3(0, h * 0.5 + 0.8, 0), shaft)
	# gold string courses up the shaft
	for sc: float in [0.30, 0.55, 0.78]:
		_torus(m, 0.42, 0.56, Vector3(0, 0.8 + h * sc, 0), gold)
	# muezzin gallery balcony
	_cyl(m, 0.78, 0.78, 0.2, Vector3(0, 0.8 + h, 0), gold)
	_torus(m, 0.70, 0.86, Vector3(0, 0.95 + h, 0), gold)
	# gallery balusters + glow
	for ang: float in [0.0, 60.0, 120.0, 180.0, 240.0, 300.0]:
		_cyl(m, 0.04, 0.04, 0.35, Vector3(sin(deg_to_rad(ang)) * 0.72, 1.0 + h, cos(deg_to_rad(ang)) * 0.72), gold)
	_box(m, Vector3(0.5, 0.6, 0.5), Vector3(0, 0.6 + h, 0), glow)
	# lantern drum
	_cyl(m, 0.36, 0.40, 0.9, Vector3(0, 1.5 + h, 0), shaft)
	# onion cap
	_onion_dome(m, 0.42, Vector3(0, 1.95 + h, 0), gold, gold)

# ----------------------------------------------------------------------------
# BUILD
# ----------------------------------------------------------------------------
static func build() -> Node3D:
	_ensure_shaders()
	var root: Node3D = Node3D.new()
	root.name = "DesertPalace"

	# materials
	var ivory: Material = _toon(IVORY, 0.25, 0.05, 0.020)
	var ivory_hi: Material = _toon(IVORY_HI, 0.30, 0.10, 0.018)
	var sandst: Material = _toon(SANDST, 0.22, 0.05, 0.020)
	var gold: Material = _metal(GOLD, 0.6)
	var gold_hi: Material = _metal(GOLD_HI, 0.75)
	var gold_dk: Material = _metal(GOLD_DK, 0.5)
	var brass: Material = _metal(BRASS, 0.55)
	var turq: Material = _gloss(TURQ)
	var turq_dk: Material = _gloss(TURQ_DK)
	var lapis: Material = _gloss(LAPIS)
	var marble: Material = _gloss(MARBLE)
	var marble_dk: Material = _gloss(MARBLE_DK)
	var terra: Material = _toon(TERRA, 0.2, 0.1, 0.018)
	var crimson: Material = _toon(CRIMSON, 0.25, 0.1, 0.016)
	var glow: Material = _glow(GLOWWARM, 3.0)
	var glow_cool: Material = _glow(GLOWCOOL, 2.6)
	var water: Material = _glass(WATER)

	# footprint constants (palace ~22 x 18)
	var W: float = 22.0
	var D: float = 18.0
	var WALL_H: float = 4.2
	var T: float = 0.5    # wall thickness

	# ===== GROUND PLATFORM / RAISED TERRACE =================================
	_box(root, Vector3(W + 6.0, 0.6, D + 6.0), Vector3(0, -0.3, 0), sandst)
	# carved plinth fascia with a brass cap moulding
	_box(root, Vector3(W + 6.2, 0.18, D + 6.2), Vector3(0, 0.05, 0), brass)
	_box(root, Vector3(W + 2.4, 0.4, D + 2.4), Vector3(0, 0.0, 0), ivory_hi)
	# corner plinth jewels on the terrace edge
	for px: float in [-1.0, 1.0]:
		for pz: float in [-1.0, 1.0]:
			_ball(root, 0.16, Vector3(px * (W * 0.5 + 2.6), 0.2, pz * (D * 0.5 + 2.6)), _jewel(TURQ))
	# inlaid marble courtyard floor (interior)
	_box(root, Vector3(W - 1.0, 0.12, D - 1.0), Vector3(0, 0.16, 0), marble)
	# central star medallion inlay
	_box(root, Vector3(5.5, 0.13, 5.5), Vector3(0, 0.17, 1.0), marble_dk)
	var star: MeshInstance3D = _box(root, Vector3(4.4, 0.14, 4.4), Vector3(0, 0.18, 1.0), lapis)
	star.rotation_degrees = Vector3(0, 45, 0)
	_box(root, Vector3(4.4, 0.15, 4.4), Vector3(0, 0.185, 1.0), turq)
	_ball(root, 0.4, Vector3(0, 0.3, 1.0), gold)
	# turquoise + lapis mosaic border bands on the floor
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.6, 0.14, D - 1.2), Vector3(sx * (W * 0.5 - 1.4), 0.17, 0), turq)
		_box(root, Vector3(0.6, 0.14, D - 1.2), Vector3(sx * (W * 0.5 - 2.2), 0.17, 0), lapis)
	for sz: float in [-1.0, 1.0]:
		_box(root, Vector3(W - 1.2, 0.14, 0.6), Vector3(0, 0.17, sz * (D * 0.5 - 1.4)), turq)

	# ===== ENTRANCE STEPS (facing +z) ======================================
	for i: int in range(5):
		var sw: float = 8.0 - float(i) * 0.4
		_box(root, Vector3(sw, 0.22, 0.7), Vector3(0, 0.11 + float(i) * 0.22, D * 0.5 + 2.0 - float(i) * 0.55), ivory_hi)
		# brass tread-nosing on each step
		_box(root, Vector3(sw, 0.05, 0.12), Vector3(0, 0.23 + float(i) * 0.22, D * 0.5 + 2.35 - float(i) * 0.55), brass)
	# step cheek-walls with finials
	for cw: float in [-1.0, 1.0]:
		_box(root, Vector3(0.6, 1.4, 3.4), Vector3(cw * 4.2, 0.6, D * 0.5 + 1.4), ivory_hi)
		_ball(root, 0.26, Vector3(cw * 4.2, 1.5, D * 0.5 + 2.9), gold)

	# ===== PERIMETER WALLS (front wall OMITTED for walkable interior) =======
	# back wall (-z)
	_box(root, Vector3(W, WALL_H, T), Vector3(0, WALL_H * 0.5 + 0.2, -D * 0.5), ivory)
	# back-wall blind-arch arcade (carved relief) for richness
	for ba: int in range(5):
		var bax: float = -W * 0.5 + 3.0 + float(ba) * 4.0
		_box(root, Vector3(2.4, 2.8, 0.18), Vector3(bax, 1.8, -D * 0.5 + 0.30), sandst)
		_horseshoe_arch(root, 2.2, 2.0, Vector3(bax, 1.1, -D * 0.5 + 0.42), ivory_hi, gold)
	# side walls (left/right) — leave the +z front fully open
	for sx2: float in [-1.0, 1.0]:
		_box(root, Vector3(T, WALL_H, D), Vector3(sx2 * W * 0.5, WALL_H * 0.5 + 0.2, 0), ivory)
		# carved sandstone string course
		_box(root, Vector3(T + 0.12, 0.3, D), Vector3(sx2 * W * 0.5, WALL_H + 0.2, 0), sandst)
		# brass dado band lower down
		_box(root, Vector3(T + 0.10, 0.16, D), Vector3(sx2 * W * 0.5, 1.0, 0), brass)
	# low front parapet / threshold (camera looks over it)
	for sx3: float in [-1.0, 1.0]:
		_box(root, Vector3(5.5, 0.7, T), Vector3(sx3 * (W * 0.5 - 3.0), 0.55, D * 0.5), ivory_hi)
		_box(root, Vector3(5.5, 0.12, T + 0.06), Vector3(sx3 * (W * 0.5 - 3.0), 0.95, D * 0.5), brass)
	# back-wall string course + crenellation
	_box(root, Vector3(W + 0.2, 0.3, T + 0.12), Vector3(0, WALL_H + 0.2, -D * 0.5), sandst)

	# merlons (stepped battlement) along back + sides
	for i2: int in range(11):
		var mx: float = -W * 0.5 + 1.0 + float(i2) * 2.0
		_box(root, Vector3(0.7, 0.55, 0.55), Vector3(mx, WALL_H + 0.7, -D * 0.5), ivory_hi)
		_prism(root, Vector3(0.7, 0.4, 0.55), Vector3(mx, WALL_H + 1.15, -D * 0.5), gold)
	for sx4: float in [-1.0, 1.0]:
		for j: int in range(8):
			var mz: float = -D * 0.5 + 1.2 + float(j) * 2.1
			_box(root, Vector3(0.55, 0.55, 0.7), Vector3(sx4 * W * 0.5, WALL_H + 0.7, mz), ivory_hi)
			_prism(root, Vector3(0.55, 0.35, 0.7), Vector3(sx4 * W * 0.5, WALL_H + 1.1, mz), gold)

	# ===== GLOWING LATTICE / ARCH WINDOWS in the side walls =================
	for sx5: float in [-1.0, 1.0]:
		for j2: int in range(3):
			var wz: float = -3.5 + float(j2) * 3.5
			# glowing backing panel
			_box(root, Vector3(0.14, 1.5, 1.3), Vector3(sx5 * (W * 0.5 - 0.10), 2.2, wz), glow)
			_lamp(root, Vector3(sx5 * (W * 0.5 - 0.9), 2.2, wz), GLOWWARM, 2.2, 5.0)
			# lattice screen over it
			var scr: Node3D = Node3D.new()
			scr.rotation_degrees = Vector3(0, 90 * sx5, 0)
			scr.position = Vector3(sx5 * (W * 0.5 - 0.05), 2.2, wz)
			root.add_child(scr)
			_lattice(scr, 1.3, 1.5, Vector3.ZERO, sandst, gold)
			# horseshoe top accent over the screen
			_horseshoe_arch(root, 1.5, 1.6, Vector3(sx5 * (W * 0.5 - 0.05), 1.5, wz), sandst, gold)
		# jewelled clerestory balcony halfway along each side wall
		_balcony(root, 3.2, Vector3(sx5 * (W * 0.5 - 0.4), 3.1, 0), marble, gold)

	# ===== GRAND ENTRANCE — IWAN (huge horseshoe portal) ===================
	# A tall pishtaq frame around the open +z front, ceremonial arched gateway.
	var portal: Node3D = Node3D.new()
	portal.position = Vector3(0, 0, D * 0.5 - 0.1)
	root.add_child(portal)
	# two massive piers framing the open entry
	for sx6: float in [-1.0, 1.0]:
		_box(portal, Vector3(1.6, WALL_H + 1.6, 1.6), Vector3(sx6 * 4.4, (WALL_H + 1.6) * 0.5 + 0.2, 0), ivory)
		_box(portal, Vector3(1.8, 0.4, 1.8), Vector3(sx6 * 4.4, WALL_H + 1.7, 0), sandst)
		# brass pier banding
		_box(portal, Vector3(1.7, 0.18, 1.7), Vector3(sx6 * 4.4, 2.4, 0), brass)
		_column(portal, WALL_H + 1.2, Vector3(sx6 * 3.2, 0.2, 0.6), ivory_hi, gold)
		# pier-top finial dome cupola
		_onion_dome(portal, 0.55, Vector3(sx6 * 4.4, WALL_H + 1.9, 0), turq, gold)
	# the big horseshoe arch spanning the piers
	var bigring: MeshInstance3D = _torus(portal, 3.0, 3.6, Vector3(0, WALL_H + 0.4, 0), gold)
	bigring.rotation_degrees = Vector3(90, 0, 0)
	_prism(portal, Vector3(0.8, 1.0, 1.0), Vector3(0, WALL_H + 3.6, 0), gold)
	# gold inscription band lintel
	_box(portal, Vector3(9.0, 0.6, 0.7), Vector3(0, WALL_H + 0.5, 0), gold_dk)
	# muqarnas-style stalactite studs under the lintel
	for mq: int in range(9):
		_prism(portal, Vector3(0.5, 0.5, 0.5), Vector3(-4.0 + float(mq) * 1.0, WALL_H + 0.1, 0.4), gold_hi).rotation_degrees = Vector3(180, 0, 0)
	# turquoise tile spandrel studs
	for ang3: float in [-60.0, -30.0, 30.0, 60.0]:
		var rr2: float = 3.3
		_ball(portal, 0.22, Vector3(sin(deg_to_rad(ang3)) * rr2, WALL_H + 0.4 + cos(deg_to_rad(ang3)) * rr2, 0.4), lapis)
	# guardian statues flanking the entrance, just inside the threshold
	for gx: float in [-1.0, 1.0]:
		_statue(portal, Vector3(gx * 2.4, 0.2, -0.4), marble, gold)

	# ===== INTERIOR: THRONE DAIS at the back, OPEN center for furnishing ====
	# crimson carpet runner from entry to throne
	_box(root, Vector3(3.0, 0.06, D - 4.0), Vector3(0, 0.24, 0.5), crimson)
	# gold carpet border rails
	for cb: float in [-1.0, 1.0]:
		_box(root, Vector3(0.18, 0.05, D - 4.0), Vector3(cb * 1.5, 0.26, 0.5), gold)
	for i3: int in range(6):
		_box(root, Vector3(3.4, 0.04, 0.18), Vector3(0, 0.27, -D * 0.5 + 3.5 + float(i3) * 2.2), gold)

	# ===== GRAND STAIR rising to the dais (showpiece) ======================
	var stair: Node3D = Node3D.new()
	stair.position = Vector3(0, 0, -D * 0.5 + 5.4)
	root.add_child(stair)
	for st: int in range(4):
		var stw: float = 5.2 - float(st) * 0.3
		_box(stair, Vector3(stw, 0.24, 0.6), Vector3(0, 0.32 + float(st) * 0.24, float(st) * -0.55), marble)
		_box(stair, Vector3(stw, 0.05, 0.10), Vector3(0, 0.45 + float(st) * 0.24, float(st) * -0.55 + 0.28), gold)
	# carved newel posts with jewel finials
	for nx: float in [-1.0, 1.0]:
		_box(stair, Vector3(0.5, 1.4, 0.5), Vector3(nx * 2.7, 0.7, 0.3), marble)
		_ball(stair, 0.22, Vector3(nx * 2.7, 1.6, 0.3), gold)
		_ball(stair, 0.13, Vector3(nx * 2.7, 1.85, 0.3), _jewel(RUBY))

	# throne dais (raised platform, back-center)
	var dais: Node3D = Node3D.new()
	dais.position = Vector3(0, 0, -D * 0.5 + 2.6)
	root.add_child(dais)
	for i4: int in range(3):
		var ds: float = 6.0 - float(i4) * 1.2
		_box(dais, Vector3(ds, 0.3, ds * 0.6), Vector3(0, 0.35 + float(i4) * 0.3, 0), marble if i4 % 2 == 0 else turq)
	# the throne itself
	_box(dais, Vector3(1.8, 0.4, 1.4), Vector3(0, 1.5, -0.2), gold)
	_box(dais, Vector3(1.8, 2.2, 0.3), Vector3(0, 2.5, -0.8), crimson)
	_box(dais, Vector3(0.3, 1.4, 1.4), Vector3(-0.95, 2.0, -0.2), gold)
	_box(dais, Vector3(0.3, 1.4, 1.4), Vector3(0.95, 2.0, -0.2), gold)
	_prism(dais, Vector3(1.9, 0.9, 0.4), Vector3(0, 3.7, -0.8), gold)
	_ball(dais, 0.22, Vector3(0, 4.2, -0.8), turq)
	# throne-back jewel inlay row
	for jy: float in [1.8, 2.4, 3.0]:
		_ball(dais, 0.12, Vector3(0, jy, -0.62), _jewel(RUBY))
	for jx: float in [-0.5, 0.5]:
		_ball(dais, 0.10, Vector3(jx, 2.7, -0.62), _jewel(EMERALD))
	# flanking throne pillars with finials
	for sx7: float in [-1.0, 1.0]:
		_column(dais, 3.4, Vector3(sx7 * 2.6, 0.6, 0.4), ivory_hi, gold)
		_ball(dais, 0.25, Vector3(sx7 * 2.6, 4.2, 0.4), lapis)
	# baldachin (canopy) over the throne, slung between the pillars
	_box(dais, Vector3(5.6, 0.25, 2.4), Vector3(0, 4.2, 0.0), gold)
	_box(dais, Vector3(5.2, 0.12, 2.0), Vector3(0, 4.0, 0.0), crimson)
	for tassel: float in [-2.4, -1.2, 0.0, 1.2, 2.4]:
		_ball(dais, 0.10, Vector3(tassel, 3.95, 1.05), gold)

	# ===== COLONNADE around the courtyard interior (open, walkable) =========
	for sx8: float in [-1.0, 1.0]:
		for j3: int in range(4):
			var cz: float = -2.5 + float(j3) * 3.2
			_column(root, WALL_H - 0.2, Vector3(sx8 * (W * 0.5 - 1.5), 0.2, cz), ivory_hi, gold)
			# horseshoe arches spanning between columns (decorative spandrel)
			if j3 < 3:
				_horseshoe_arch(root, 2.4, 2.2, Vector3(sx8 * (W * 0.5 - 1.5), 1.0, cz + 1.6), sandst, gold)
			# hanging lanterns between the columns
			_box(root, Vector3(0.3, 0.5, 0.3), Vector3(sx8 * (W * 0.5 - 1.5), WALL_H - 1.4, cz), glow)
			_prism(root, Vector3(0.34, 0.28, 0.34), Vector3(sx8 * (W * 0.5 - 1.5), WALL_H - 1.05, cz), gold)
			_lamp(root, Vector3(sx8 * (W * 0.5 - 1.5), WALL_H - 1.4, cz), GLOWWARM, 1.4, 4.0)

	# ===== CENTERPIECE COURTYARD FOUNTAIN ==================================
	_fountain(root, Vector3(0, 0.22, 3.2), 2.3, marble, gold, turq_dk, water)
	_lamp(root, Vector3(0, 2.0, 3.2), TURQ.lightened(0.3), 1.6, 6.0)
	# four small corner braziers ringing the fountain
	for bx: float in [-1.0, 1.0]:
		for bz: float in [-1.0, 1.0]:
			_cyl(root, 0.22, 0.30, 1.0, Vector3(bx * 3.4, 0.7, 3.2 + bz * 3.0), gold)
			_cyl(root, 0.34, 0.20, 0.3, Vector3(bx * 3.4, 1.3, 3.2 + bz * 3.0), brass)
			_ball(root, 0.22, Vector3(bx * 3.4, 1.55, 3.2 + bz * 3.0), glow)
			_lamp(root, Vector3(bx * 3.4, 1.55, 3.2 + bz * 3.0), GLOWWARM, 1.2, 4.0)

	# ===== MARBLE BRAZIER / HEARTH SHOWPIECE on the back-left ==============
	var hearth: Node3D = Node3D.new()
	hearth.position = Vector3(-W * 0.5 + 3.2, 0.2, -D * 0.5 + 6.5)
	root.add_child(hearth)
	_box(hearth, Vector3(2.6, 1.8, 1.0), Vector3(0, 0.9, 0), marble)
	_box(hearth, Vector3(2.0, 1.0, 0.5), Vector3(0, 0.7, 0.4), gold_dk)
	# glowing fire bed
	_box(hearth, Vector3(1.6, 0.4, 0.4), Vector3(0, 0.5, 0.45), glow)
	_lamp(hearth, Vector3(0, 0.7, 0.6), GLOWWARM, 2.2, 5.0)
	# carved mantel + overmantel with a jewelled medallion
	_box(hearth, Vector3(2.9, 0.22, 1.2), Vector3(0, 1.85, 0), gold)
	_box(hearth, Vector3(2.2, 1.4, 0.3), Vector3(0, 2.7, -0.3), marble)
	_torus(hearth, 0.4, 0.6, Vector3(0, 2.7, -0.12), gold).rotation_degrees = Vector3(90, 0, 0)
	_ball(hearth, 0.26, Vector3(0, 2.7, 0.0), _jewel(TURQ))

	# ===== INTERIOR PALMS in planters (corners, open) ======================
	for px2: float in [-1.0, 1.0]:
		_box(root, Vector3(1.6, 0.6, 1.6), Vector3(px2 * (W * 0.5 - 2.3), 0.45, -D * 0.5 + 5.5), sandst)
		_box(root, Vector3(1.7, 0.16, 1.7), Vector3(px2 * (W * 0.5 - 2.3), 0.78, -D * 0.5 + 5.5), gold)
		_palm(root, Vector3(px2 * (W * 0.5 - 2.3), 0.7, -D * 0.5 + 5.5), 0.9)

	# ===== CEILING / CLERESTORY (partial — keeps interior visible) =========
	# We roof only the side aisles + throne, leaving the central court open to sky.
	for sx9: float in [-1.0, 1.0]:
		_box(root, Vector3(4.0, 0.3, D - 1.0), Vector3(sx9 * (W * 0.5 - 2.2), WALL_H + 0.3, 0), ivory)
	# coffered ceiling panels over throne
	_box(root, Vector3(8.0, 0.3, 5.0), Vector3(0, WALL_H + 0.3, -D * 0.5 + 2.8), ivory)
	for i5: int in range(3):
		for j4: int in range(2):
			_box(root, Vector3(2.0, 0.18, 1.8), Vector3(-2.4 + float(i5) * 2.4, WALL_H + 0.1, -D * 0.5 + 1.8 + float(j4) * 2.0), lapis)
			_ball(root, 0.10, Vector3(-2.4 + float(i5) * 2.4, WALL_H + 0.0, -D * 0.5 + 1.8 + float(j4) * 2.0), gold)

	# ===== GRAND CHANDELIER over the throne =================================
	var chand: Node3D = Node3D.new()
	chand.position = Vector3(0, WALL_H - 0.2, -D * 0.5 + 3.0)
	root.add_child(chand)
	_cyl(chand, 0.04, 0.04, 0.9, Vector3(0, 0.45, 0), gold)
	_torus(chand, 0.9, 1.1, Vector3(0, 0, 0), gold)
	_torus(chand, 0.55, 0.7, Vector3(0, -0.35, 0), gold)
	for ang5: float in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]:
		var cr: float = 1.0
		_ball(chand, 0.14, Vector3(sin(deg_to_rad(ang5)) * cr, -0.1, cos(deg_to_rad(ang5)) * cr), glow)
		# crystal drops on the outer ring
		_ball(chand, 0.07, Vector3(sin(deg_to_rad(ang5)) * cr, -0.4, cos(deg_to_rad(ang5)) * cr), _jewel(GLOWCOOL))
	# inner tier of candle lights
	for ang7: float in [0.0, 72.0, 144.0, 216.0, 288.0]:
		var cr2: float = 0.6
		_ball(chand, 0.10, Vector3(sin(deg_to_rad(ang7)) * cr2, -0.45, cos(deg_to_rad(ang7)) * cr2), glow)
	_lamp(root, Vector3(0, WALL_H - 0.6, -D * 0.5 + 3.0), GLOWWARM, 3.5, 9.0)

	# courtyard centre chandelier-lantern hung from a beam frame for the open hall
	var court_lant: Node3D = Node3D.new()
	court_lant.position = Vector3(0, WALL_H + 0.2, 3.2)
	root.add_child(court_lant)
	_cyl(court_lant, 0.05, 0.05, 1.2, Vector3(0, -0.6, 0), gold)
	_box(court_lant, Vector3(1.0, 1.2, 1.0), Vector3(0, -1.6, 0), glow_cool)
	_torus(court_lant, 0.6, 0.8, Vector3(0, -1.0, 0), gold)
	_prism(court_lant, Vector3(1.1, 0.7, 1.1), Vector3(0, -1.0, 0), gold)
	_lamp(root, Vector3(0, WALL_H - 1.4, 3.2), GLOWCOOL, 2.0, 7.0)

	# ===== ROOF: GOLDEN ONION DOMES (signature silhouette) =================
	# central great dome on a tall drum over the throne hall
	var drum: Node3D = Node3D.new()
	drum.position = Vector3(0, WALL_H + 0.5, -D * 0.5 + 3.0)
	root.add_child(drum)
	_cyl(drum, 2.7, 2.9, 1.6, Vector3(0, 0.8, 0), ivory_hi)
	# brass drum cornice + base ring
	_torus(drum, 2.9, 3.2, Vector3(0, 0.05, 0), brass)
	# drum lattice windows (glowing)
	for ang6: float in [0.0, 45.0, 90.0, 135.0, 180.0, 225.0, 270.0, 315.0]:
		_box(drum, Vector3(0.5, 1.0, 0.2), Vector3(sin(deg_to_rad(ang6)) * 2.85, 0.8, cos(deg_to_rad(ang6)) * 2.85), glow)
		_horseshoe_arch(drum, 0.7, 0.8, Vector3(sin(deg_to_rad(ang6)) * 2.86, 0.3, cos(deg_to_rad(ang6)) * 2.86), ivory_hi, gold)
	_torus(drum, 2.7, 3.0, Vector3(0, 1.6, 0), gold)
	_onion_dome(drum, 2.6, Vector3(0, 1.7, 0), gold, gold)
	_lamp(root, Vector3(0, WALL_H + 5.0, -D * 0.5 + 3.0), GOLD, 1.2, 8.0)

	# corner onion domes (4) on towers
	for cx: float in [-1.0, 1.0]:
		for cz2: float in [-1.0, 1.0]:
			var tx: float = cx * (W * 0.5 - 1.4)
			var tz: float = cz2 * (D * 0.5 - 1.4)
			# tower drum
			_cyl(root, 1.0, 1.1, WALL_H + 1.4, Vector3(tx, (WALL_H + 1.4) * 0.5 + 0.2, tz), ivory)
			_torus(root, 1.0, 1.25, Vector3(tx, WALL_H + 1.6, tz), gold)
			# brass mid-band on each tower
			_torus(root, 1.0, 1.18, Vector3(tx, WALL_H * 0.55, tz), brass)
			# small glow window in each tower
			_box(root, Vector3(0.5, 0.9, 0.2), Vector3(tx, WALL_H * 0.6, tz + 1.05 * cz2), glow)
			_onion_dome(root, 1.0, Vector3(tx, WALL_H + 1.7, tz), turq, gold)

	# two mid-side smaller domes on the side aisle roofs for rhythm
	for sx10: float in [-1.0, 1.0]:
		_onion_dome(root, 0.8, Vector3(sx10 * (W * 0.5 - 2.2), WALL_H + 0.6, 0), gold, gold)

	# dormer cupolas along the side-aisle roof ridge for added richness
	for sx11: float in [-1.0, 1.0]:
		for dm: int in range(2):
			var dz: float = -4.0 + float(dm) * 8.0
			_box(root, Vector3(1.0, 0.7, 1.0), Vector3(sx11 * (W * 0.5 - 2.2), WALL_H + 0.7, dz), ivory_hi)
			_horseshoe_arch(root, 0.8, 0.7, Vector3(sx11 * (W * 0.5 - 2.2), WALL_H + 0.5, dz + 0.5), ivory_hi, gold)
			_onion_dome(root, 0.4, Vector3(sx11 * (W * 0.5 - 2.2), WALL_H + 1.0, dz), turq, gold)

	# ===== FOUR TALL MINARETS at the outer corners (strong silhouette) =====
	for mx2: float in [-1.0, 1.0]:
		for mz2: float in [-1.0, 1.0]:
			_minaret(root, Vector3(mx2 * (W * 0.5 + 2.0), 0.0, mz2 * (D * 0.5 + 2.0)), 7.5, ivory, gold, glow)

	# ===== FRONT-COURT LANDSCAPING (exterior, beyond the steps) ============
	# twin entrance fountains flanking the approach
	for fx: float in [-1.0, 1.0]:
		_fountain(root, Vector3(fx * 8.5, 0.2, D * 0.5 + 5.5), 1.4, marble, gold, turq_dk, water)
		_lamp(root, Vector3(fx * 8.5, 1.8, D * 0.5 + 5.5), TURQ.lightened(0.3), 1.2, 5.0)
	# flanking palms + lanterns at the entrance
	for ex: float in [-1.0, 1.0]:
		_palm(root, Vector3(ex * 5.5, 0.2, D * 0.5 + 3.0), 1.1)
		_palm(root, Vector3(ex * 9.5, 0.2, D * 0.5 + 8.0), 1.2)
		# entrance lantern on a post
		_cyl(root, 0.10, 0.12, 1.8, Vector3(ex * 3.2, 1.1, D * 0.5 + 2.2), gold)
		_box(root, Vector3(0.5, 0.6, 0.5), Vector3(ex * 3.2, 2.2, D * 0.5 + 2.2), glow)
		_prism(root, Vector3(0.6, 0.4, 0.6), Vector3(ex * 3.2, 2.6, D * 0.5 + 2.2), gold)
		_lamp(root, Vector3(ex * 3.2, 2.2, D * 0.5 + 2.2), GLOWWARM, 2.0, 6.0)
	# guardian statues at the foot of the approach
	for gx2: float in [-1.0, 1.0]:
		_statue(root, Vector3(gx2 * 6.5, 0.2, D * 0.5 + 8.0), sandst, gold)
	# low hedges lining the approach
	for hz: int in range(5):
		var hzp: float = D * 0.5 + 1.5 + float(hz) * 1.6
		for hx: float in [-1.0, 1.0]:
			_box(root, Vector3(0.9, 0.7, 1.2), Vector3(hx * 7.0, 0.55, hzp), _toon(HEDGE, 0.3, 0.0, 0.016))
			# clipped topiary balls on the hedge line
			_ball(root, 0.4, Vector3(hx * 7.0, 1.1, hzp), _toon(HEDGE.lightened(0.05), 0.3, 0.0, 0.014))
	# reflecting-pool path inlay with a gold kerb
	_box(root, Vector3(2.0, 0.06, 7.0), Vector3(0, 0.22, D * 0.5 + 4.0), water)
	_box(root, Vector3(2.4, 0.1, 7.4), Vector3(0, 0.18, D * 0.5 + 4.0), gold)
	# pool-edge jewel studs
	for pz2: int in range(5):
		var ppz: float = D * 0.5 + 1.4 + float(pz2) * 1.6
		for ppx: float in [-1.0, 1.0]:
			_ball(root, 0.10, Vector3(ppx * 1.25, 0.26, ppz), _jewel(TURQ))

	# ===== AMBIENT FILL — warm key light over the whole palace =============
	_lamp(root, Vector3(0, WALL_H + 8.0, 2.0), GLOWWARM, 1.0, 26.0)

	return root

# ----------------------------------------------------------------------------
# META
# ----------------------------------------------------------------------------
static func meta() -> Dictionary:
	return {
		"id": "desert_palace",
		"name": "Sultan's Mirage Palace",
		"tier": "Palace",
		"rarity": "Legendary",
		"description": "A sun-drenched desert palace crowned with golden onion domes, four soaring minarets and horseshoe arches, where intricate ivory lattice screens glow over a colonnaded courtyard. Twin entrance fountains and palm-lined reflecting pools lead past guardian statues into a jewelled throne hall — a grand marble stair, a colossal crystal chandelier, a carved marble hearth and a baldachin-canopied throne set with rubies, emeralds and lapis beneath a lapis-coffered ceiling.",
		"footprint": [22, 18],
		"floors": 1,
		"attributes": [
			["Style", "Moorish / Mughal Desert Palace"],
			["Material", "Ivory Stucco, Brushed Gold & Brass, Turquoise Tile, Lapis, Marble & Jewels"],
			["Feature", "Golden Onion Domes, Four Minarets, Jewelled Throne Hall, Grand Stair & Twin Fountains"],
			["Floors", "1 (grand double-height hall)"],
			["Vibe", "Opulent, Sun-Soaked, Regal"]
		]
	}
