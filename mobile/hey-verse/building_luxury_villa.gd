# Hey Verse — premium procedural BUILDING module (NFT, placeable on land).
#
#   id   : "luxury_villa"
#   tier : Luxury Villa (Epic)
#
# A floor-to-ceiling GLASS villa pushed to the top of its tier: two floors, a
# double-height living room open to sky-bright glazing, brushed-gold fascia
# banding, fluted columns, statement statues, a tiered front fountain, an
# infinity pool that spills toward +z, cantilevered balconies, dormer skylights,
# swaying palms, lush landscaping, and a palm-fringed rooftop terrace crowned in
# gold. The interior carries showpieces: a grand floating stair, a long linear
# chandelier, a sculptural fireplace, a chef's marble island, and glowing art.
#
# The ground floor front wall is OMITTED (camera looks in from +z) so a ~1.4u
# robot avatar walks straight into a CLEAN, WALKABLE open plan and furnishes it.
#
# Self-contained: loads res://toon.gdshader + res://outline.gdshader by path with
# ResourceLoader.exists() guards and falls back to StandardMaterial3D so the file
# parses and runs standalone. No preloads, no external assets. Built at the
# origin; entrance faces +z.
class_name VerseBuildingLuxuryVilla
extends RefCounted

const TOON_PATH := "res://toon.gdshader"
const OUTLINE_PATH := "res://outline.gdshader"

# ---------------------------------------------------------------------------
# Material helpers
# ---------------------------------------------------------------------------

# Cached shaders so every material in one build shares the same compiled program.
static func _toon_shader() -> Shader:
	if ResourceLoader.exists(TOON_PATH):
		var s: Resource = ResourceLoader.load(TOON_PATH)
		if s is Shader:
			return s
	return null

static func _outline_shader() -> Shader:
	if ResourceLoader.exists(OUTLINE_PATH):
		var s: Resource = ResourceLoader.load(OUTLINE_PATH)
		if s is Shader:
			return s
	return null

# Core toon material. Falls back to a plausible StandardMaterial3D when the
# shaders are missing so the module renders standalone.
static func _toon(col: Color, rim: float, spec: float, wind: float, wind_h: float, outline: float) -> Material:
	var tsh: Shader = _toon_shader()
	if tsh == null:
		var sm := StandardMaterial3D.new()
		sm.albedo_color = col
		sm.roughness = 0.85
		sm.metallic = 0.0
		return sm
	var m := ShaderMaterial.new()
	m.shader = tsh
	m.set_shader_parameter("albedo", col)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", spec)
	m.set_shader_parameter("wind_strength", wind)
	m.set_shader_parameter("wind_height", wind_h)
	var osh: Shader = _outline_shader()
	if osh != null and outline > 0.0:
		var o := ShaderMaterial.new()
		o.shader = osh
		o.set_shader_parameter("thickness", outline)
		o.set_shader_parameter("line_color", Color(0.05, 0.07, 0.11, 1.0))
		m.next_pass = o
	return m

static func _matte(col: Color) -> Material:
	return _toon(col, 0.30, 0.0, 0.0, 0.5, 0.016)

static func _metal(col: Color) -> Material:
	# Brushed gold / brass / chrome accents — sharp rim + tight spec dot.
	return _toon(col, 0.85, 0.85, 0.0, 0.5, 0.012)

static func _gloss(col: Color) -> Material:
	# Polished stone / marble / lacquer — soft sheen.
	return _toon(col, 0.55, 0.45, 0.0, 0.5, 0.014)

# Translucent toon glass. Needs StandardMaterial3D for real alpha; tints with the
# toon look at the silhouette only via a thin outline so glazing reads as glass.
static func _glass(col: Color, alpha: float) -> Material:
	var sm := StandardMaterial3D.new()
	sm.albedo_color = Color(col.r, col.g, col.b, alpha)
	sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	sm.roughness = 0.05
	sm.metallic = 0.25
	sm.metallic_specular = 0.9
	sm.cull_mode = BaseMaterial3D.CULL_DISABLED
	sm.rim_enabled = true
	sm.rim = 0.7
	sm.refraction_enabled = false
	return sm

static func _glow(col: Color, energy: float) -> Material:
	var sm := StandardMaterial3D.new()
	sm.albedo_color = col
	sm.emission_enabled = true
	sm.emission = col
	sm.emission_energy_multiplier = energy
	sm.roughness = 0.4
	return sm

# Pool / fountain water — translucent, faintly emissive, glassy.
static func _water(col: Color) -> Material:
	var sm := StandardMaterial3D.new()
	sm.albedo_color = Color(col.r, col.g, col.b, 0.82)
	sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	sm.roughness = 0.02
	sm.metallic = 0.2
	sm.metallic_specular = 1.0
	sm.emission_enabled = true
	sm.emission = col * 0.5
	sm.emission_energy_multiplier = 0.35
	return sm

# ---------------------------------------------------------------------------
# Primitive helpers — each returns a MeshInstance3D positioned in local space.
# ---------------------------------------------------------------------------

static func _box(parent: Node3D, pos: Vector3, size: Vector3, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var bm := BoxMesh.new()
	bm.size = size
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _cyl(parent: Node3D, pos: Vector3, r_top: float, r_bot: float, h: float, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = 20
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _ball(parent: Node3D, pos: Vector3, r: float, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 22
	sm.rings = 12
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _torus(parent: Node3D, pos: Vector3, inner: float, outer: float, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 24
	tm.ring_segments = 14
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _prism(parent: Node3D, pos: Vector3, size: Vector3, mat: Material) -> MeshInstance3D:
	var mi := MeshInstance3D.new()
	var pm := PrismMesh.new()
	pm.size = size
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _light(parent: Node3D, pos: Vector3, col: Color, energy: float, rng: float) -> OmniLight3D:
	var l := OmniLight3D.new()
	l.position = pos
	l.light_color = col
	l.light_energy = energy
	l.omni_range = rng
	l.shadow_enabled = false
	parent.add_child(l)
	return l

# ---------------------------------------------------------------------------
# Composite luxury props (self-contained, no assets)
# ---------------------------------------------------------------------------

# Fluted classical column: stepped base, fluted shaft, gold collar + capital.
static func _column(parent: Node3D, pos: Vector3, h: float, shaft: Material, gold: Material) -> void:
	var c := Node3D.new()
	c.position = pos
	parent.add_child(c)
	# stepped plinth base
	_box(c, Vector3(0, 0.06, 0), Vector3(0.7, 0.12, 0.7), gold)
	_box(c, Vector3(0, 0.18, 0), Vector3(0.56, 0.12, 0.56), shaft)
	# tapered fluted shaft
	_cyl(c, Vector3(0, 0.24 + h * 0.5, 0), 0.20, 0.24, h, shaft)
	# six shallow flute reveals around the shaft
	for fa: int in range(6):
		var a: float = float(fa) * (TAU / 6.0)
		_box(c, Vector3(cos(a) * 0.205, 0.24 + h * 0.5, sin(a) * 0.205), Vector3(0.05, h * 0.92, 0.05), gold)
	# gold collar + capital
	_torus(c, Vector3(0, 0.24 + h, 0), 0.18, 0.27, gold)
	_box(c, Vector3(0, 0.30 + h, 0), Vector3(0.6, 0.12, 0.6), gold)

# Tiered fountain: two stacked bowls, a central plume, a gold finial, glow.
static func _fountain(parent: Node3D, pos: Vector3, stone: Material, gold: Material, water: Material, glow: Material) -> void:
	var f := Node3D.new()
	f.position = pos
	parent.add_child(f)
	# octagonal-ish basin (cylinder) + gold rim
	_cyl(f, Vector3(0, 0.25, 0), 1.5, 1.6, 0.5, stone)
	_torus(f, Vector3(0, 0.52, 0), 1.42, 1.56, gold)
	_cyl(f, Vector3(0, 0.5, 0), 1.36, 1.36, 0.08, water)
	# central pedestal
	_cyl(f, Vector3(0, 0.85, 0), 0.22, 0.3, 0.7, stone)
	# upper bowl + gold rim
	_cyl(f, Vector3(0, 1.2, 0), 0.7, 0.62, 0.2, stone)
	_torus(f, Vector3(0, 1.3, 0), 0.58, 0.7, gold)
	_cyl(f, Vector3(0, 1.32, 0), 0.55, 0.55, 0.06, water)
	# rising water plume + gold finial
	_cyl(f, Vector3(0, 1.7, 0), 0.06, 0.1, 0.7, water)
	_ball(f, Vector3(0, 2.1, 0), 0.12, glow)
	_box(f, Vector3(0, 2.3, 0), Vector3(0.08, 0.2, 0.08), gold)
	_light(f, Vector3(0, 0.7, 0), Color(0.5, 0.8, 0.95), 0.8, 4.0)

# Abstract pedestal statue: gold figure on a marble plinth, uplit.
static func _statue(parent: Node3D, pos: Vector3, plinth: Material, gold: Material, glow: Material) -> void:
	var s := Node3D.new()
	s.position = pos
	parent.add_child(s)
	# plinth
	_box(s, Vector3(0, 0.35, 0), Vector3(0.7, 0.7, 0.7), plinth)
	_box(s, Vector3(0, 0.72, 0), Vector3(0.78, 0.06, 0.78), gold)
	# stylized standing figure (torso + head + raised arm) in gold
	_cyl(s, Vector3(0, 1.25, 0), 0.12, 0.18, 0.9, gold)
	_ball(s, Vector3(0, 1.85, 0), 0.14, gold)
	_cyl(s, Vector3(0.22, 1.55, 0), 0.05, 0.05, 0.5, gold)
	_cyl(s, Vector3(0.33, 1.85, 0), 0.04, 0.04, 0.4, gold)
	# upward accent light grazing the figure
	_light(s, Vector3(0, 0.85, 0.0), Color(1.0, 0.86, 0.55), 0.7, 3.0)

# Self-contained palm builder — curved trunk + radial fronds, leaves catch wind.
static func _palm(parent: Node3D, base: Vector3, trunk: Material, leaf: Material, scale: float) -> void:
	var p := Node3D.new()
	p.position = base
	parent.add_child(p)
	# segmented leaning trunk
	var segs: int = 6
	for i: int in range(segs):
		var t: float = float(i) / float(segs)
		var y: float = (0.4 + t * 3.2) * scale
		var lean: float = sin(t * 1.4) * 0.5 * scale
		_cyl(p, Vector3(lean, y, 0), 0.10 * scale, 0.14 * scale, 0.62 * scale, trunk)
	var topx: float = sin(1.0 * 1.4) * 0.5 * scale
	var top := Vector3(topx, 3.7 * scale, 0)
	# crown coconuts
	for cc: int in range(3):
		var a: float = float(cc) * 2.1
		_ball(p, top + Vector3(cos(a) * 0.18, -0.1, sin(a) * 0.18) * scale, 0.1 * scale, trunk)
	# radial fronds (wind-swaying toon leaves)
	var fronds: int = 8
	for fi: int in range(fronds):
		var ang: float = float(fi) * (TAU / float(fronds))
		var frond := MeshInstance3D.new()
		var pm := PrismMesh.new()
		pm.size = Vector3(0.5, 0.1, 2.2) * scale
		frond.mesh = pm
		frond.material_override = leaf
		frond.position = top + Vector3(cos(ang) * 1.0, 0.15, sin(ang) * 1.0) * scale
		frond.rotation = Vector3(deg_to_rad(-18.0), ang, 0)
		p.add_child(frond)

# Conical topiary in a gold-rimmed planter.
static func _topiary(parent: Node3D, pos: Vector3, planter: Material, gold: Material, leaf: Material) -> void:
	_box(parent, pos + Vector3(0, 0.3, 0), Vector3(0.7, 0.6, 0.7), planter)
	_box(parent, pos + Vector3(0, 0.6, 0), Vector3(0.78, 0.06, 0.78), gold)
	_cyl(parent, pos + Vector3(0, 1.4, 0), 0.02, 0.55, 1.6, leaf)
	_cyl(parent, pos + Vector3(0, 2.2, 0), 0.02, 0.32, 0.8, leaf)

# Cantilevered balcony with glass parapet + gold cap, anchored to a wall plane.
static func _balcony(parent: Node3D, pos: Vector3, w: float, depth: float, floor_mat: Material, glass: Material, gold: Material) -> void:
	var b := Node3D.new()
	b.position = pos
	parent.add_child(b)
	# slab + gold underside fascia
	_box(b, Vector3(0, 0, 0), Vector3(w, 0.14, depth), floor_mat)
	_box(b, Vector3(0, -0.09, depth * 0.5 - 0.02), Vector3(w + 0.06, 0.06, 0.08), gold)
	# front + side glass parapet
	_box(b, Vector3(0, 0.55, depth * 0.5 - 0.03), Vector3(w, 1.0, 0.05), glass)
	_box(b, Vector3(0, 1.05, depth * 0.5 - 0.03), Vector3(w, 0.06, 0.08), gold)
	for sb: float in [-1.0, 1.0]:
		_box(b, Vector3(sb * (w * 0.5 - 0.03), 0.55, 0), Vector3(0.05, 1.0, depth), glass)
		_box(b, Vector3(sb * (w * 0.5 - 0.03), 1.05, 0), Vector3(0.08, 0.06, depth), gold)

# ---------------------------------------------------------------------------
# Build
# ---------------------------------------------------------------------------

static func build() -> Node3D:
	var root := Node3D.new()
	root.name = "LuxuryVilla"

	# Palette ---------------------------------------------------------------
	var stucco: Material = _matte(Color(0.95, 0.94, 0.91))      # warm white shell
	var stone: Material = _gloss(Color(0.83, 0.82, 0.80))       # pale travertine
	var pale_stone: Material = _gloss(Color(0.90, 0.89, 0.87))  # bright marble
	var dark_stone: Material = _gloss(Color(0.32, 0.33, 0.36))  # charcoal accent
	var gold: Material = _metal(Color(0.93, 0.76, 0.32))        # brushed gold fascia
	var brass: Material = _metal(Color(0.86, 0.68, 0.34))
	var chrome: Material = _metal(Color(0.82, 0.84, 0.88))
	var wood: Material = _gloss(Color(0.55, 0.38, 0.24))        # warm walnut deck/floor
	var glass: Material = _glass(Color(0.62, 0.78, 0.86), 0.20) # crisp blue glazing
	var glass_dim: Material = _glass(Color(0.55, 0.70, 0.80), 0.28)
	var warm_glow: Material = _glow(Color(1.0, 0.86, 0.6), 2.2) # interior warm light
	var art_glow: Material = _glow(Color(0.55, 0.8, 1.0), 1.8)  # cool art backlight
	var fire_glow: Material = _glow(Color(1.0, 0.5, 0.2), 2.6)
	var pool_w: Material = _water(Color(0.30, 0.72, 0.85))
	var leaf: Material = _toon(Color(0.30, 0.55, 0.28), 0.4, 0.0, 1.0, 1.4, 0.014)
	var topi_leaf: Material = _toon(Color(0.26, 0.50, 0.27), 0.4, 0.0, 0.5, 1.2, 0.014)
	var trunk: Material = _matte(Color(0.55, 0.45, 0.32))
	var hedge: Material = _toon(Color(0.26, 0.48, 0.26), 0.35, 0.0, 0.6, 0.9, 0.014)

	# Footprint: 12 (x) x 10 (z). Shell hugs the back/sides; +z is open glass.
	var W: float = 12.0
	var D: float = 10.0
	var hw: float = W * 0.5      # 6.0
	var wall_t: float = 0.22
	var f1: float = 3.0          # ground ceiling height
	var f2: float = 3.0          # upper ceiling height
	var slab_t: float = 0.3

	# === GROUND PLATFORM / PODIUM ==========================================
	# Raised stone podium reads as wealth + lifts the villa above the land.
	_box(root, Vector3(0, -0.18, 0.6), Vector3(W + 2.4, 0.36, D + 4.6), stone)
	_box(root, Vector3(0, -0.02, 0.6), Vector3(W + 1.6, 0.18, D + 3.8), pale_stone)
	# Gold reveal lines around the podium edge (front + back + sides).
	_box(root, Vector3(0, 0.04, -D * 0.5 + 0.4 - 1.6), Vector3(W + 1.7, 0.06, 0.08), gold)
	_box(root, Vector3(0, 0.04, D * 0.5 + 3.6), Vector3(W + 1.7, 0.06, 0.08), gold)
	for sg: float in [-1.0, 1.0]:
		_box(root, Vector3(sg * (hw + 0.85), 0.04, 0.6), Vector3(0.08, 0.06, D + 3.6), gold)

	# === GROUND FLOOR SLAB =================================================
	_box(root, Vector3(0, 0.06, 0), Vector3(W, 0.12, D), wood)             # walnut floor
	# Inset marble rug zone in the living area, gold-bordered.
	_box(root, Vector3(-1.0, 0.13, 0.4), Vector3(6.0, 0.03, 5.0), pale_stone)
	_box(root, Vector3(-1.0, 0.135, 0.4), Vector3(6.2, 0.02, 5.2), gold)

	# === BACK + SIDE GLASS-AND-STUCCO SHELL (ground) =======================
	# Back wall — solid white stucco core with glowing slot windows + art niche.
	_box(root, Vector3(0, f1 * 0.5, -D * 0.5 + wall_t * 0.5), Vector3(W, f1, wall_t), stucco)
	# Recessed glow windows in the back wall.
	for wx: float in [-3.2, 3.2]:
		_box(root, Vector3(wx, 1.7, -D * 0.5 + wall_t + 0.04), Vector3(1.6, 1.4, 0.06), warm_glow)
		_box(root, Vector3(wx, 1.7, -D * 0.5 + wall_t + 0.02), Vector3(1.8, 1.6, 0.05), gold)
	# Side walls: lower half stucco pier, upper half floor-to-ceiling glass.
	for s: float in [-1.0, 1.0]:
		var sx: float = s * (hw - wall_t * 0.5)
		# stucco pier near the back
		_box(root, Vector3(sx, f1 * 0.5, -D * 0.5 + 1.6), Vector3(wall_t, f1, 3.0), stucco)
		# full-height glass panel toward the front
		_box(root, Vector3(sx, f1 * 0.5, 1.0), Vector3(wall_t * 0.6, f1 - 0.1, D - 3.4), glass)
		# slim gold mullions splitting the glass
		_box(root, Vector3(sx, f1 * 0.5, 1.0), Vector3(wall_t * 0.8, 0.08, D - 3.4), gold)
		_box(root, Vector3(sx, 0.9, 1.0), Vector3(wall_t * 0.8, 0.06, D - 3.4), gold)
		_box(root, Vector3(sx, 2.1, 1.0), Vector3(wall_t * 0.8, 0.06, D - 3.4), gold)

	# Front is OPEN glass-line: a low glass parapet + corner gold posts so the
	# threshold reads, but the avatar walks straight in.
	_box(root, Vector3(0, 0.45, D * 0.5 - 0.1), Vector3(W - 1.8, 0.5, wall_t * 0.5), glass_dim)
	for s2: float in [-1.0, 1.0]:
		_box(root, Vector3(s2 * (hw - 0.5), f1 * 0.5, D * 0.5 - 0.1), Vector3(0.18, f1, 0.18), gold)
		# vertical gold post catches glow at night
		_cyl(root, Vector3(s2 * (hw - 0.5), f1 + 0.15, D * 0.5 - 0.1), 0.05, 0.05, 0.3, gold)

	# === FRONT PORTICO COLUMNS (frame the open threshold) ==================
	# Two fluted columns supporting the entry canopy lift the front silhouette.
	for sc: float in [-1.0, 1.0]:
		_column(root, Vector3(sc * 2.4, 0.06, D * 0.5 + 0.6), f1 - 0.3, pale_stone, gold)

	# === INTERIOR PARTIAL WALLS (keep plan OPEN) ===========================
	# One spine wall splits a back bedroom/kitchen zone from the double-height
	# living room — short, so the space stays airy.
	_box(root, Vector3(-hw + 3.2, f1 * 0.5, -1.4), Vector3(0.16, f1, 4.2), stucco)
	# A waist-high gold-capped divider near the kitchen.
	_box(root, Vector3(1.6, 0.6, -2.2), Vector3(4.0, 1.2, 0.16), _matte(Color(0.90, 0.89, 0.86)))
	_box(root, Vector3(1.6, 1.22, -2.2), Vector3(4.0, 0.08, 0.2), gold)

	# === SHOWPIECE: BACKLIT ART NICHE ======================================
	# A framed cool-glow art panel set into the spine wall — gallery energy.
	_box(root, Vector3(-hw + 3.28, 1.6, -1.4), Vector3(0.04, 1.6, 2.2), art_glow)
	_box(root, Vector3(-hw + 3.30, 1.6, -1.4), Vector3(0.05, 1.8, 2.4), gold)
	_light(root, Vector3(-hw + 3.6, 1.6, -1.4), Color(0.6, 0.82, 1.0), 0.5, 3.0)

	# === SHOWPIECE: KITCHEN ISLAND =========================================
	var island := Node3D.new()
	island.position = Vector3(2.4, 0, -3.2)
	root.add_child(island)
	_box(island, Vector3(0, 0.5, 0), Vector3(2.8, 1.0, 1.1), dark_stone)
	_box(island, Vector3(0, 1.03, 0), Vector3(3.0, 0.08, 1.3), pale_stone)  # marble top
	_box(island, Vector3(0, 1.02, 0), Vector3(3.05, 0.04, 1.35), gold)      # gold rim
	# waterfall marble end panels
	for ie: float in [-1.0, 1.0]:
		_box(island, Vector3(ie * 1.5, 0.52, 0), Vector3(0.06, 1.06, 1.3), pale_stone)
	# pendant lights over island
	for px: float in [-0.8, 0.8]:
		_cyl(island, Vector3(px, 2.6, 0), 0.02, 0.02, 0.8, brass)
		_ball(island, Vector3(px, 2.15, 0), 0.16, warm_glow)

	# === SHOWPIECE: GRAND FLOATING STAIR ===================================
	# Cantilevered glass-and-gold stair to the upper floor, against the spine.
	var stair := Node3D.new()
	stair.position = Vector3(-hw + 3.6, 0, -1.0)
	root.add_child(stair)
	var steps: int = 12
	for i: int in range(steps):
		var sy: float = 0.18 + float(i) * (f1 / float(steps))
		var sz: float = -2.4 + float(i) * 0.40
		_box(stair, Vector3(0, sy, sz), Vector3(1.5, 0.1, 0.42), wood)
		_box(stair, Vector3(0, sy - 0.04, sz), Vector3(1.55, 0.04, 0.46), gold)
	# glass balustrade rail + gold handrail
	_box(stair, Vector3(0.85, 1.7, -0.8), Vector3(0.05, 1.0, 4.6), glass_dim)
	_box(stair, Vector3(0.85, 2.3, -0.8), Vector3(0.07, 0.07, 4.6), gold)

	# === UPPER FLOOR SLAB (partial — leaves the double-height void at front) ==
	# Back ~half of the footprint gets a second floor; the front living room
	# stays open to the rooftop glazing (double-height).
	var slab_z: float = -D * 0.5 + 3.2  # slab covers from back to z = ~3.2-D/2
	_box(root, Vector3(0, f1 + slab_t * 0.5, slab_z), Vector3(W, slab_t, 6.4), wood)
	# gold fascia edge of the void (the dramatic line of the double-height room)
	_box(root, Vector3(0, f1 + 0.02, slab_z + 3.2), Vector3(W, 0.14, 0.16), gold)
	# glass balustrade guarding the void edge upstairs
	_box(root, Vector3(0, f1 + 0.6, slab_z + 3.2), Vector3(W - 0.6, 1.0, 0.06), glass_dim)
	_box(root, Vector3(0, f1 + 1.1, slab_z + 3.2), Vector3(W - 0.6, 0.06, 0.1), gold)

	# === UPPER FLOOR SHELL =================================================
	var u0: float = f1 + slab_t  # upper floor base y
	# back wall upper
	_box(root, Vector3(0, u0 + f2 * 0.5, -D * 0.5 + wall_t * 0.5), Vector3(W, f2, wall_t), stucco)
	# upper glow windows
	for wx2: float in [-3.0, 0.0, 3.0]:
		_box(root, Vector3(wx2, u0 + 1.5, -D * 0.5 + wall_t + 0.04), Vector3(1.4, 1.4, 0.06), warm_glow)
		_box(root, Vector3(wx2, u0 + 1.5, -D * 0.5 + wall_t + 0.02), Vector3(1.6, 1.6, 0.05), gold)
	# upper side walls (full glass over the occupied back zone)
	for s3: float in [-1.0, 1.0]:
		var sx2: float = s3 * (hw - wall_t * 0.5)
		_box(root, Vector3(sx2, u0 + f2 * 0.5, slab_z), Vector3(wall_t * 0.6, f2, 6.0), glass)
		_box(root, Vector3(sx2, u0 + f2 * 0.5, slab_z), Vector3(wall_t * 0.8, 0.08, 6.0), gold)
		_box(root, Vector3(sx2, u0 + 0.9, slab_z), Vector3(wall_t * 0.8, 0.06, 6.0), gold)
	# upper front glass band (overlooks the pool) for the back bedroom
	_box(root, Vector3(0, u0 + f2 * 0.5, slab_z + 3.1), Vector3(W - 1.2, f2 - 0.2, 0.1), glass)

	# === CANTILEVERED MASTER BALCONIES (overlook the pool) =================
	# Two glass balconies project from the upper front glass band — premium read.
	for sb2: float in [-1.0, 1.0]:
		_balcony(root, Vector3(sb2 * 3.0, u0 + 0.02, slab_z + 3.6), 3.0, 1.4, wood, glass_dim, gold)

	# === ROOF + ROOFTOP TERRACE ===========================================
	# Flat slab over the upper back zone = the rooftop terrace floor of the
	# whole villa, framed in gold fascia. Reads as a sky-deck.
	var roof_y: float = u0 + f2 + slab_t * 0.5
	_box(root, Vector3(0, roof_y, slab_z), Vector3(W + 0.6, slab_t, 6.6), stone)
	# Big gold fascia band wrapping the roofline (the "go hard" wealth read).
	_box(root, Vector3(0, roof_y - 0.05, slab_z + 3.4), Vector3(W + 0.8, 0.22, 0.18), gold)
	for s4: float in [-1.0, 1.0]:
		_box(root, Vector3(s4 * (hw + 0.35), roof_y - 0.05, slab_z), Vector3(0.18, 0.22, 6.7), gold)
	_box(root, Vector3(0, roof_y - 0.05, -D * 0.5 + 0.1), Vector3(W + 0.8, 0.22, 0.18), gold)

	# Full flat roof over the double-height front living room too (so it's
	# enclosed) — a thin glass skylight strip lets light pour onto the void.
	_box(root, Vector3(0, roof_y, slab_z + 4.8), Vector3(W, slab_t * 0.7, 3.0), stucco)
	_box(root, Vector3(0, roof_y + 0.02, slab_z + 4.8), Vector3(3.2, 0.06, 2.4), glass)  # skylight

	# Dormer skylight lanterns popping above the front roof (architectural read).
	for dx: float in [-3.4, 3.4]:
		_prism(root, Vector3(dx, roof_y + 0.45, slab_z + 4.8), Vector3(1.4, 0.6, 1.6), stucco)
		_box(root, Vector3(dx, roof_y + 0.3, slab_z + 4.8), Vector3(1.0, 0.4, 1.2), glass)
		_box(root, Vector3(dx, roof_y + 0.62, slab_z + 4.8), Vector3(0.06, 0.06, 1.7), gold)

	# Rooftop terrace: glass parapet + planters + a fire pit + a pergola hint.
	var rtop: float = roof_y + slab_t * 0.5
	for s5: float in [-1.0, 1.0]:
		_box(root, Vector3(s5 * (hw - 0.2), rtop + 0.5, slab_z), Vector3(0.06, 1.0, 6.0), glass_dim)
		_box(root, Vector3(s5 * (hw - 0.2), rtop + 1.0, slab_z), Vector3(0.08, 0.06, 6.0), gold)
	_box(root, Vector3(0, rtop + 0.5, -D * 0.5 + 0.3), Vector3(W - 0.4, 1.0, 0.06), glass_dim)
	_box(root, Vector3(0, rtop + 1.0, -D * 0.5 + 0.3), Vector3(W - 0.4, 0.06, 0.08), gold)
	_box(root, Vector3(0, rtop + 0.5, slab_z + 3.0), Vector3(W - 0.4, 1.0, 0.06), glass_dim)
	# slim gold pergola frame over part of the terrace
	for pgx: float in [-2.6, 2.6]:
		_cyl(root, Vector3(pgx, rtop + 1.2, slab_z - 1.6), 0.05, 0.05, 2.4, gold)
	for pgz: float in [-2.0, -0.4, 1.2]:
		_box(root, Vector3(0, rtop + 2.4, slab_z + pgz), Vector3(5.6, 0.08, 0.08), gold)
	# rooftop fire pit
	_cyl(root, Vector3(-2.4, rtop + 0.25, slab_z - 0.5), 0.5, 0.55, 0.5, dark_stone)
	_cyl(root, Vector3(-2.4, rtop + 0.45, slab_z - 0.5), 0.42, 0.42, 0.12, _glow(Color(1.0, 0.55, 0.2), 3.0))
	_light(root, Vector3(-2.4, rtop + 0.7, slab_z - 0.5), Color(1.0, 0.5, 0.2), 1.2, 4.0)
	# rooftop planters
	for px2: float in [-1.0, 1.0]:
		_box(root, Vector3(px2 * 3.0, rtop + 0.3, slab_z + 2.4), Vector3(1.2, 0.5, 0.6), dark_stone)
		_box(root, Vector3(px2 * 3.0, rtop + 0.6, slab_z + 2.4), Vector3(1.0, 0.4, 0.4), hedge)

	# === INFINITY POOL (spills toward +z, in front of the villa) ===========
	var pool := Node3D.new()
	pool.position = Vector3(0, 0, D * 0.5 + 2.6)
	root.add_child(pool)
	# pool shell
	_box(pool, Vector3(0, -0.25, 0), Vector3(8.0, 0.5, 4.0), dark_stone)
	# walnut deck surround
	_box(pool, Vector3(0, 0.0, 0), Vector3(10.5, 0.1, 6.0), wood)
	# gold coping band around the deck edge
	_box(pool, Vector3(0, 0.06, -3.0), Vector3(10.5, 0.05, 0.12), gold)
	# water surface (slightly recessed)
	_box(pool, Vector3(0, 0.06, -0.2), Vector3(7.4, 0.06, 3.4), pool_w)
	# infinity edge: gold lip at the far (+z) side, water flush to the brim
	_box(pool, Vector3(0, 0.09, 1.9), Vector3(7.6, 0.05, 0.1), gold)
	_box(pool, Vector3(0, -0.1, 2.05), Vector3(7.6, 0.3, 0.1), dark_stone)  # catch basin lip
	# underwater glow for night swims
	_light(pool, Vector3(-2.0, -0.1, 0), Color(0.3, 0.75, 0.9), 1.4, 4.0)
	_light(pool, Vector3(2.0, -0.1, 0), Color(0.3, 0.75, 0.9), 1.4, 4.0)
	# four chrome deck posts with rope-light caps
	for cx: float in [-4.6, 4.6]:
		for cz: float in [-2.6, 2.6]:
			_cyl(pool, Vector3(cx, 0.45, cz), 0.05, 0.05, 0.8, chrome)
			_ball(pool, Vector3(cx, 0.9, cz), 0.09, warm_glow)
	# a pair of sun loungers (gold-frame + cushion) on the deck
	for lx: float in [-3.4, 3.4]:
		_box(pool, Vector3(lx, 0.22, -2.2), Vector3(0.8, 0.12, 2.0), pale_stone)
		_prism(pool, Vector3(lx, 0.45, -2.9), Vector3(0.8, 0.4, 0.8), pale_stone)
		_box(pool, Vector3(lx, 0.1, -2.2), Vector3(0.86, 0.06, 2.1), gold)

	# === FRONT FOUNTAIN (axis centrepiece between steps and pool) ==========
	_fountain(root, Vector3(0, 0.06, D * 0.5 + 6.6), stone, gold, pool_w, warm_glow)

	# === STATEMENT STATUES flanking the entry path =========================
	for sst: float in [-1.0, 1.0]:
		_statue(root, Vector3(sst * 3.6, 0.06, D * 0.5 + 4.2), pale_stone, gold, warm_glow)

	# === ENTRANCE: PIVOT DOOR + STEPS + CANOPY =============================
	# Steps up onto the podium from +z, leading between pool and entry.
	for st: int in range(3):
		var stz: float = D * 0.5 + 0.5 + float(st) * 0.5
		_box(root, Vector3(0, 0.06 - float(st) * 0.12, stz), Vector3(3.2 + float(st) * 0.6, 0.12, 0.5), stone)
	# Statement glass pivot door (~2.2 tall) set just inside the open front.
	_box(root, Vector3(-1.4, 1.1, D * 0.5 - 0.3), Vector3(1.1, 2.2, 0.08), _glass(Color(0.5, 0.65, 0.75), 0.35))
	_box(root, Vector3(-1.4, 1.1, D * 0.5 - 0.3), Vector3(1.16, 2.24, 0.04), gold)  # gold frame
	_box(root, Vector3(-1.0, 1.1, D * 0.5 - 0.24), Vector3(0.05, 1.0, 0.04), brass) # vertical handle
	# Slim cantilevered entry canopy with hidden downlight (rests on the columns).
	_box(root, Vector3(0, f1 + 0.25, D * 0.5 + 0.6), Vector3(5.4, 0.12, 1.8), stone)
	_box(root, Vector3(0, f1 + 0.19, D * 0.5 + 0.6), Vector3(5.5, 0.05, 1.9), gold)
	_light(root, Vector3(0, f1 - 0.1, D * 0.5 + 0.4), Color(1.0, 0.86, 0.6), 1.0, 4.0)

	# === SHOWPIECE: DOUBLE-HEIGHT LIVING ROOM CHANDELIER ===================
	# A long linear gold-and-glass fixture dropping into the void.
	var chand := Node3D.new()
	chand.position = Vector3(0, f1 + 2.4, 1.6)
	root.add_child(chand)
	_cyl(chand, Vector3(0, 0.6, 0), 0.02, 0.02, 1.2, brass)
	_box(chand, Vector3(0, 0, 0), Vector3(0.1, 0.1, 2.4), gold)
	for ci: int in range(7):
		var cz2: float = -1.0 + float(ci) * 0.33
		_cyl(chand, Vector3(0, -0.4, cz2), 0.03, 0.03, 0.8, glass_dim)
		_ball(chand, Vector3(0, -0.85, cz2), 0.08, warm_glow)
	_light(root, Vector3(0, f1 + 1.6, 1.6), Color(1.0, 0.88, 0.66), 1.6, 7.0)

	# === FIREPLACE FEATURE (back of living room) ===========================
	_box(root, Vector3(-1.0, 1.0, -D * 0.5 + wall_t + 0.12), Vector3(2.6, 2.0, 0.2), dark_stone)
	_box(root, Vector3(-1.0, 0.7, -D * 0.5 + wall_t + 0.2), Vector3(1.4, 0.7, 0.12), fire_glow)
	_box(root, Vector3(-1.0, 1.45, -D * 0.5 + wall_t + 0.22), Vector3(2.0, 0.1, 0.18), gold)  # mantel band
	# a tall gold flue rising from the mantel
	_box(root, Vector3(-1.0, 2.3, -D * 0.5 + wall_t + 0.14), Vector3(0.9, 1.0, 0.12), gold)
	_light(root, Vector3(-1.0, 0.8, -D * 0.5 + 0.6), Color(1.0, 0.5, 0.2), 0.9, 4.0)

	# === LANDSCAPING: PALMS, TOPIARY, HEDGES, PATH LANTERNS ================
	# Swaying palms (wind via toon shader) flanking the pool deck + grounds.
	var palm_spots: Array = [
		Vector3(-5.6, 0, D * 0.5 + 1.5),
		Vector3(5.6, 0, D * 0.5 + 1.5),
		Vector3(5.4, 0, D * 0.5 + 5.2),
		Vector3(-5.4, 0, D * 0.5 + 5.2),
	]
	for ps: Vector3 in palm_spots:
		_palm(root, ps, trunk, leaf, 1.0)
	# a taller signature palm by the fountain
	_palm(root, Vector3(-5.2, 0, D * 0.5 + 7.4), trunk, leaf, 1.25)
	# Manicured conical topiary framing the entry axis.
	for stp: float in [-1.0, 1.0]:
		_topiary(root, Vector3(stp * 2.0, 0.06, D * 0.5 + 7.6), dark_stone, gold, topi_leaf)
	# Low manicured hedges hugging the side podium edges.
	for s6: float in [-1.0, 1.0]:
		for hz: float in [-2.0, 0.0, 2.0]:
			_box(root, Vector3(s6 * (hw + 1.0), 0.35, hz), Vector3(0.8, 0.7, 1.6), hedge)
	# Gold path lanterns leading to the door.
	for lz: float in [1.0, 3.0, 5.0]:
		for s7: float in [-1.0, 1.0]:
			_cyl(root, Vector3(s7 * 2.0, 0.4, D * 0.5 + lz), 0.06, 0.08, 0.8, gold)
			_ball(root, Vector3(s7 * 2.0, 0.85, D * 0.5 + lz), 0.1, warm_glow)
			_light(root, Vector3(s7 * 2.0, 0.9, D * 0.5 + lz), Color(1.0, 0.86, 0.6), 0.4, 2.5)

	# === INTERIOR FILL LIGHTS (so the open plan reads warm) ================
	_light(root, Vector3(2.0, 2.4, -2.0), Color(1.0, 0.9, 0.72), 0.8, 6.0)
	_light(root, Vector3(0, u0 + 1.6, slab_z), Color(1.0, 0.9, 0.72), 0.7, 6.0)
	_light(root, Vector3(-2.0, 1.8, 1.0), Color(1.0, 0.9, 0.75), 0.7, 6.0)

	return root

# ---------------------------------------------------------------------------
# Metadata
# ---------------------------------------------------------------------------

static func meta() -> Dictionary:
	return {
		"id": "luxury_villa",
		"name": "Azure Glass Villa",
		"tier": "Luxury Villa",
		"rarity": "Epic",
		"description": "A two-storey glass villa where floor-to-ceiling glazing meets a brushed-gold fascia: fluted columns and statues frame a tiered fountain, an infinity pool spills toward the water, cantilevered glass balconies overlook the deck, and a double-height living room with a grand floating stair and linear chandelier opens beneath a palm-fringed rooftop terrace.",
		"footprint": [12, 10],
		"floors": 2,
		"attributes": [
			["Style", "Modern Glass"],
			["Material", "Glass, Gold Trim, Travertine, Walnut"],
			["Feature", "Infinity Pool, Fountain & Rooftop Terrace"],
			["Showpiece", "Grand Floating Stair & Linear Chandelier"],
			["Floors", "2"],
			["Vibe", "Sun-Drenched Luxury"]
		]
	}
