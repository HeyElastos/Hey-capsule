class_name VerseBuildingSkyPenthouse
extends RefCounted

# ============================================================================
# HEY VERSE — premium procedural building
#   id   : sky_penthouse
#   tier : Penthouse (Epic)
#   A glass-walled top-floor suite on a structural slab: wraparound terrace
#   with a glowing skyline edge, sunken lounge + bar, gold + marble + glass.
#   Built at origin, ground floor y=0, entrance facing +z, front wall OMITTED
#   so the camera looks straight into the walkable open interior.
#
#   LUXURY ENHANCE pass: grand stair, full crystal chandelier, marble columns
#   with brass capitals, sculpted statues + tiered fountains, cantilevered
#   balconies + dormered cabana, deeper landscaping, glowing windows, and a
#   strengthened gold-and-marble silhouette — while the ground floor stays
#   open, walkable, and free of a front wall.
# ============================================================================

# ---------------------------------------------------------------------------
# Shader / material helpers (self-contained; guarded path loads + fallback)
# ---------------------------------------------------------------------------

const TOON_PATH: String = "res://toon.gdshader"
const OUTLINE_PATH: String = "res://outline.gdshader"

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

# Core material builder. Falls back to StandardMaterial3D so the module
# parses + runs standalone outside the real Verse project.
static func _mat(color: Color, rim: float, spec: float, metal: float, rough: float, emit: Color, emit_e: float) -> Material:
	var toon: Shader = _toon_shader()
	if toon != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = toon
		m.set_shader_parameter("albedo", color)
		m.set_shader_parameter("rim_strength", rim)
		m.set_shader_parameter("spec_strength", spec)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		var outline: Shader = _outline_shader()
		if outline != null:
			var o: ShaderMaterial = ShaderMaterial.new()
			o.shader = outline
			o.set_shader_parameter("thickness", 0.016)
			o.set_shader_parameter("line_color", Color(0.06, 0.08, 0.12, 1.0))
			m.next_pass = o
		return m
	# --- StandardMaterial3D fallback ---
	var std: StandardMaterial3D = StandardMaterial3D.new()
	std.albedo_color = color
	std.metallic = metal
	std.roughness = rough
	std.rim_enabled = rim > 0.0
	std.rim = rim
	if emit_e > 0.0:
		std.emission_enabled = true
		std.emission = emit
		std.emission_energy_multiplier = emit_e
	return std

static func _toon(color: Color) -> Material:
	return _mat(color, 0.32, 0.0, 0.0, 0.9, Color.BLACK, 0.0)

static func _metal(color: Color) -> Material:
	# brushed gold / brass / chrome accent
	return _mat(color, 0.55, 0.7, 0.95, 0.28, Color.BLACK, 0.0)

static func _gloss(color: Color) -> Material:
	# polished marble / lacquer
	return _mat(color, 0.4, 0.45, 0.1, 0.18, Color.BLACK, 0.0)

static func _glass(color: Color) -> Material:
	var toon: Shader = _toon_shader()
	if toon != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = toon
		m.set_shader_parameter("albedo", Color(color.r, color.g, color.b, 0.34))
		m.set_shader_parameter("rim_strength", 0.85)
		m.set_shader_parameter("spec_strength", 0.6)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		return m
	var std: StandardMaterial3D = StandardMaterial3D.new()
	std.albedo_color = Color(color.r, color.g, color.b, 0.34)
	std.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	std.metallic = 0.0
	std.roughness = 0.05
	std.rim_enabled = true
	std.rim = 0.9
	return std

static func _glow(color: Color, energy: float) -> Material:
	var toon: Shader = _toon_shader()
	if toon != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = toon
		# emissive look is faked via a bright, rim-heavy albedo for the toon path
		var lit: Color = Color(
			clamp(color.r * (1.0 + energy * 0.5), 0.0, 1.0),
			clamp(color.g * (1.0 + energy * 0.5), 0.0, 1.0),
			clamp(color.b * (1.0 + energy * 0.5), 0.0, 1.0),
			1.0)
		m.set_shader_parameter("albedo", lit)
		m.set_shader_parameter("rim_strength", 0.9)
		m.set_shader_parameter("spec_strength", 0.4)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		return m
	var std: StandardMaterial3D = StandardMaterial3D.new()
	std.albedo_color = color
	std.emission_enabled = true
	std.emission = color
	std.emission_energy_multiplier = energy
	std.roughness = 0.5
	return std

# ---------------------------------------------------------------------------
# Primitive helpers — every one returns a positioned MeshInstance3D
# ---------------------------------------------------------------------------

static func _box(parent: Node3D, pos: Vector3, size: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var bm: BoxMesh = BoxMesh.new()
	bm.size = size
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _cyl(parent: Node3D, pos: Vector3, r_top: float, r_bot: float, h: float, mat: Material, sides: int = 18) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var cm: CylinderMesh = CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = sides
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _ball(parent: Node3D, pos: Vector3, r: float, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var sm: SphereMesh = SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 20
	sm.rings = 12
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _torus(parent: Node3D, pos: Vector3, inner: float, outer: float, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var tm: TorusMesh = TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 28
	tm.ring_segments = 14
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _prism(parent: Node3D, pos: Vector3, size: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var pm: PrismMesh = PrismMesh.new()
	pm.size = size
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _cone(parent: Node3D, pos: Vector3, r: float, h: float, mat: Material, sides: int = 16) -> MeshInstance3D:
	return _cyl(parent, pos, 0.0, r, h, mat, sides)

# A simple classical column: fluted shaft + brass base ring + brass capital.
static func _column(parent: Node3D, base_pos: Vector3, r: float, h: float, shaft_mat: Material, trim_mat: Material) -> void:
	_cyl(parent, base_pos + Vector3(0.0, h * 0.5, 0.0), r, r, h, shaft_mat, 16)
	# base
	_cyl(parent, base_pos + Vector3(0.0, 0.12, 0.0), r * 1.35, r * 1.5, 0.24, trim_mat, 16)
	_box(parent, base_pos + Vector3(0.0, 0.04, 0.0), Vector3(r * 3.0, 0.08, r * 3.0), trim_mat)
	# capital
	_cyl(parent, base_pos + Vector3(0.0, h - 0.16, 0.0), r * 1.5, r * 1.3, 0.3, trim_mat, 16)
	_box(parent, base_pos + Vector3(0.0, h + 0.02, 0.0), Vector3(r * 3.0, 0.12, r * 3.0), trim_mat)

# An abstract sculpted figure on a plinth — reads as a luxury statue.
static func _statue(parent: Node3D, base_pos: Vector3, scale_h: float, body_mat: Material, plinth_mat: Material) -> void:
	# plinth
	_box(parent, base_pos + Vector3(0.0, 0.25 * scale_h, 0.0), Vector3(0.9 * scale_h, 0.5 * scale_h, 0.9 * scale_h), plinth_mat)
	_box(parent, base_pos + Vector3(0.0, 0.52 * scale_h, 0.0), Vector3(0.98 * scale_h, 0.06 * scale_h, 0.98 * scale_h), body_mat)
	var fy: float = base_pos.y + 0.55 * scale_h
	# legs / lower drape (tapered cylinder)
	_cyl(parent, Vector3(base_pos.x, fy + 0.55 * scale_h, base_pos.z), 0.16 * scale_h, 0.34 * scale_h, 1.1 * scale_h, body_mat, 14)
	# torso
	_cyl(parent, Vector3(base_pos.x, fy + 1.4 * scale_h, base_pos.z), 0.22 * scale_h, 0.2 * scale_h, 0.7 * scale_h, body_mat, 14)
	# shoulders
	_ball(parent, Vector3(base_pos.x, fy + 1.78 * scale_h, base_pos.z), 0.26 * scale_h, body_mat)
	# head
	_ball(parent, Vector3(base_pos.x, fy + 2.12 * scale_h, base_pos.z), 0.17 * scale_h, body_mat)
	# raised arm (a graceful diagonal)
	_cyl(parent, Vector3(base_pos.x + 0.3 * scale_h, fy + 1.95 * scale_h, base_pos.z), 0.07 * scale_h, 0.08 * scale_h, 0.9 * scale_h, body_mat, 10).rotation = Vector3(0.0, 0.0, deg_to_rad(38.0))

# A tiered circular fountain with a glowing water disc + central jet.
static func _fountain(parent: Node3D, center: Vector3, r: float, basin_mat: Material, trim_mat: Material, water_mat: Material) -> void:
	# outer basin wall
	_cyl(parent, center + Vector3(0.0, 0.3, 0.0), r, r + 0.18, 0.6, basin_mat, 24)
	_cyl(parent, center + Vector3(0.0, 0.6, 0.0), r + 0.02, r + 0.02, 0.1, trim_mat, 24)
	# water disc
	_cyl(parent, center + Vector3(0.0, 0.48, 0.0), r - 0.22, r - 0.22, 0.1, water_mat, 24)
	# pedestal
	_cyl(parent, center + Vector3(0.0, 0.78, 0.0), r * 0.34, r * 0.42, 0.5, trim_mat, 18)
	# upper bowl
	_cyl(parent, center + Vector3(0.0, 1.06, 0.0), r * 0.55, r * 0.3, 0.18, basin_mat, 20)
	_cyl(parent, center + Vector3(0.0, 1.12, 0.0), r * 0.46, r * 0.46, 0.06, water_mat, 20)
	# central jet + crown droplet
	_cyl(parent, center + Vector3(0.0, 1.42, 0.0), 0.05, 0.05, 0.6, water_mat, 10)
	_ball(parent, center + Vector3(0.0, 1.78, 0.0), 0.14, water_mat)

# ---------------------------------------------------------------------------
# BUILD
# ---------------------------------------------------------------------------

static func build() -> Node3D:
	var root: Node3D = Node3D.new()
	root.name = "SkyPenthouse"

	# Footprint (penthouse suite) ~ 16 wide x 13 deep. Terrace wraps outside it.
	var w: float = 16.0
	var d: float = 13.0
	var suite_h: float = 3.4          # ceiling height of the suite (a touch grander)
	var slab_top: float = 0.0          # interior floor lives at y=0

	# ---- Palette ---------------------------------------------------------
	var col_slab: Color = Color(0.30, 0.32, 0.37)        # cool structural concrete
	var col_marble: Color = Color(0.93, 0.92, 0.90)      # warm white marble
	var col_marble_dk: Color = Color(0.80, 0.79, 0.78)   # veined band
	var col_gold: Color = Color(0.92, 0.75, 0.36)        # brushed gold trim
	var col_gold_br: Color = Color(0.98, 0.84, 0.50)     # bright polished gold
	var col_brass: Color = Color(0.78, 0.62, 0.30)
	var col_chrome: Color = Color(0.78, 0.82, 0.88)
	var col_glass: Color = Color(0.55, 0.74, 0.86)       # sky-tinted glazing
	var col_charcoal: Color = Color(0.16, 0.17, 0.20)    # mullions / lounge
	var col_wood: Color = Color(0.46, 0.30, 0.18)        # warm walnut
	var col_warmglow: Color = Color(1.0, 0.86, 0.58)     # warm interior glow
	var col_skyline: Color = Color(0.30, 0.78, 1.0)      # cool skyline edge glow
	var col_water: Color = Color(0.40, 0.70, 0.92)
	var col_ivory: Color = Color(0.97, 0.95, 0.91)       # statuary marble
	var col_hedge: Color = Color(0.22, 0.45, 0.25)       # toon greenery

	var m_slab: Material = _toon(col_slab)
	var m_marble: Material = _gloss(col_marble)
	var m_marble_dk: Material = _gloss(col_marble_dk)
	var m_gold: Material = _metal(col_gold)
	var m_gold_br: Material = _metal(col_gold_br)
	var m_brass: Material = _metal(col_brass)
	var m_chrome: Material = _metal(col_chrome)
	var m_glass: Material = _glass(col_glass)
	var m_charcoal: Material = _toon(col_charcoal)
	var m_wood: Material = _gloss(col_wood)
	var m_warmglow: Material = _glow(col_warmglow, 2.0)
	var m_skyline: Material = _glow(col_skyline, 2.6)
	var m_water: Material = _glass(col_water)
	var m_ivory: Material = _gloss(col_ivory)
	var m_hedge: Material = _toon(col_hedge)
	var m_window: Material = _glow(col_warmglow, 1.4)

	# =====================================================================
	# 1. STRUCTURAL SLAB + WRAPAROUND TERRACE  (this is a top floor of a tower)
	# =====================================================================
	# Terrace deck extends ~2.6 beyond the suite on all sides.
	var terr_w: float = w + 5.2
	var terr_d: float = d + 5.2
	var slab_thick: float = 0.7

	# Main slab (the building floorplate the suite sits on)
	_box(root, Vector3(0.0, -slab_thick * 0.5, 0.0), Vector3(terr_w, slab_thick, terr_d), m_slab)
	# Gold soffit band along the slab edge — the underside of the cantilever reads luxe
	_box(root, Vector3(0.0, -slab_thick - 0.05, 0.0), Vector3(terr_w - 0.4, 0.12, terr_d - 0.4), m_gold)
	# Marble terrace floor finish on top of the slab
	_box(root, Vector3(0.0, -0.06, 0.0), Vector3(terr_w - 0.3, 0.12, terr_d - 0.3), m_marble)
	# Dark inlay border on the terrace marble
	_box(root, Vector3(0.0, -0.02, 0.0), Vector3(terr_w - 1.6, 0.06, terr_d - 1.6), m_marble_dk)
	# Gold pinstripe inside the inlay — a tailored detail
	_box(root, Vector3(0.0, 0.0, 0.0), Vector3(terr_w - 2.0, 0.04, terr_d - 2.0), m_gold)
	# Suite interior raised floor (warm marble) with a gold threshold trim
	_box(root, Vector3(0.0, 0.05, 0.0), Vector3(w, 0.1, d), m_marble)
	_box(root, Vector3(0.0, 0.11, 0.0), Vector3(w - 0.6, 0.04, d - 0.6), m_marble_dk)

	# Tower trunk hint below the slab — sells the "high in the sky" silhouette.
	_box(root, Vector3(0.0, -4.5, 0.0), Vector3(w - 1.0, 8.0, d - 1.0), m_charcoal)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(sx * (w * 0.5 - 1.2), -4.5, 0.0), Vector3(0.5, 8.0, d - 2.0), m_slab)
	# vertical gold expansion-joint fins down the trunk — reads as a real facade
	for jx: float in [-3.0, -1.0, 1.0, 3.0]:
		_box(root, Vector3(jx * (w * 0.12), -4.5, (d - 1.0) * 0.5), Vector3(0.12, 7.6, 0.12), m_gold)

	# ---- Glowing skyline edge: a glow strip running the terrace perimeter ----
	var edge_y: float = 0.16
	var eo_w: float = terr_w - 0.5
	var eo_d: float = terr_d - 0.5
	# four glow strips (N/S along x, E/W along z)
	_box(root, Vector3(0.0, edge_y, eo_d * 0.5), Vector3(eo_w, 0.14, 0.14), m_skyline)   # +z front
	_box(root, Vector3(0.0, edge_y, -eo_d * 0.5), Vector3(eo_w, 0.14, 0.14), m_skyline)  # -z back
	_box(root, Vector3(eo_w * 0.5, edge_y, 0.0), Vector3(0.14, 0.14, eo_d), m_skyline)   # +x
	_box(root, Vector3(-eo_w * 0.5, edge_y, 0.0), Vector3(0.14, 0.14, eo_d), m_skyline)  # -x

	# ---- Glass terrace balustrade (frameless panels + gold cap rail) ------
	var rail_h: float = 1.1
	var n_x: int = 11
	var n_z: int = 9
	# panels along +x and -x edges (run in z)
	for sx: float in [-1.0, 1.0]:
		var px: float = sx * (eo_w * 0.5)
		for i: int in range(n_z):
			var t: float = float(i) / float(n_z - 1)
			var pz: float = lerp(-eo_d * 0.5, eo_d * 0.5, t)
			_box(root, Vector3(px, rail_h * 0.5 + 0.16, pz), Vector3(0.05, rail_h, eo_d / float(n_z) * 0.92), m_glass)
		_cyl(root, Vector3(px, rail_h + 0.16, 0.0), 0.05, 0.05, eo_d, m_gold, 10).rotation = Vector3(deg_to_rad(90.0), 0.0, 0.0)
	# panels along -z (back) and +z (front, but leave a wide gap for entrance)
	for sz: float in [-1.0, 1.0]:
		var pz2: float = sz * (eo_d * 0.5)
		for i: int in range(n_x):
			var t: float = float(i) / float(n_x - 1)
			var px2: float = lerp(-eo_w * 0.5, eo_w * 0.5, t)
			# leave a 3-panel gap at the front-center for the terrace stair/approach
			if sz > 0.0 and absf(px2) < eo_w * 0.18:
				continue
			_box(root, Vector3(px2, rail_h * 0.5 + 0.16, pz2), Vector3(eo_w / float(n_x) * 0.92, rail_h, 0.05), m_glass)
		_cyl(root, Vector3(0.0, rail_h + 0.16, pz2), 0.05, 0.05, eo_w, m_gold, 10).rotation = Vector3(0.0, 0.0, deg_to_rad(90.0))

	# Gold corner posts of the balustrade, each capped with a glowing finial sphere
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_cyl(root, Vector3(sx * eo_w * 0.5, rail_h * 0.5 + 0.16, sz * eo_d * 0.5), 0.07, 0.08, rail_h + 0.2, m_gold, 12)
			_ball(root, Vector3(sx * eo_w * 0.5, rail_h + 0.42, sz * eo_d * 0.5), 0.11, m_gold_br)
			_ball(root, Vector3(sx * eo_w * 0.5, rail_h + 0.42, sz * eo_d * 0.5), 0.05, m_warmglow)

	# =====================================================================
	# 2. SUITE SHELL — glass walls + gold mullions; FRONT (+z) WALL OMITTED
	# =====================================================================
	var hw: float = w * 0.5
	var hd: float = d * 0.5
	var wall_y: float = suite_h * 0.5 + 0.1
	var top_y: float = suite_h + 0.1

	# ---- Back wall (-z): solid marble feature wall (TV / fireplace backdrop)
	_box(root, Vector3(0.0, wall_y, -hd), Vector3(w, suite_h, 0.24), m_marble)
	# veined band + gold reveal lines on the feature wall
	_box(root, Vector3(0.0, suite_h * 0.62 + 0.1, -hd + 0.13), Vector3(w - 1.0, 0.5, 0.05), m_marble_dk)
	for gx: float in [-1.0, 1.0]:
		_box(root, Vector3(gx * (w * 0.5 - 0.9), wall_y, -hd + 0.14), Vector3(0.08, suite_h - 0.4, 0.04), m_gold)
	# a long gold cornice capping the feature wall
	_box(root, Vector3(0.0, suite_h - 0.05, -hd + 0.16), Vector3(w - 0.4, 0.12, 0.1), m_gold)

	# ---- Side walls (±x): floor-to-ceiling glazing in gold frames ----
	for sx: float in [-1.0, 1.0]:
		var x: float = sx * hw
		# top + bottom gold frame rails
		_box(root, Vector3(x, 0.22, 0.0), Vector3(0.18, 0.2, d), m_gold)
		_box(root, Vector3(x, top_y - 0.12, 0.0), Vector3(0.18, 0.22, d), m_gold)
		# vertical mullions + glass infill
		var panes: int = 6
		for i: int in range(panes):
			var t: float = (float(i) + 0.5) / float(panes)
			var z: float = lerp(-hd + 0.2, hd - 0.2, t)
			_box(root, Vector3(x, wall_y, z), Vector3(0.1, suite_h - 0.5, d / float(panes) * 0.9), m_glass)
		for i: int in range(panes + 1):
			var t2: float = float(i) / float(panes)
			var z2: float = lerp(-hd + 0.1, hd - 0.1, t2)
			_cyl(root, Vector3(x, wall_y, z2), 0.06, 0.06, suite_h - 0.5, m_gold, 8)

	# ---- Front (+z): OMITTED wall. Only corner posts + a low glass threshold
	# parapet + a slim header beam so the silhouette still reads as enclosed.
	for sx: float in [-1.0, 1.0]:
		_cyl(root, Vector3(sx * (hw - 0.1), wall_y, hd), 0.12, 0.13, suite_h, m_gold, 12)
	# low threshold parapet (knee height) across the front, with a center opening
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(sx * (w * 0.30), 0.45, hd), Vector3(w * 0.34, 0.7, 0.18), m_marble)
		_box(root, Vector3(sx * (w * 0.30), 0.82, hd), Vector3(w * 0.34, 0.06, 0.2), m_gold)
	# slim header beam across the top of the open front, dressed in bright gold
	_box(root, Vector3(0.0, top_y - 0.1, hd), Vector3(w, 0.3, 0.2), m_gold)
	_box(root, Vector3(0.0, top_y - 0.1, hd + 0.06), Vector3(w - 0.4, 0.16, 0.06), m_gold_br)
	# a sculpted gold crest medallion centered on the header — house signature
	_torus(root, Vector3(0.0, top_y - 0.1, hd + 0.12), 0.18, 0.34, m_gold_br)
	_ball(root, Vector3(0.0, top_y - 0.1, hd + 0.12), 0.16, m_warmglow)

	# =====================================================================
	# 3. ROOF SLAB — flat with a gold-edged overhang + clerestory + pergola
	# =====================================================================
	_box(root, Vector3(0.0, top_y + 0.14, 0.0), Vector3(w + 1.4, 0.28, d + 1.4), m_slab)
	# gold drip-edge fascia around the roof overhang
	_box(root, Vector3(0.0, top_y + 0.02, (d + 1.4) * 0.5), Vector3(w + 1.4, 0.12, 0.12), m_gold)
	_box(root, Vector3(0.0, top_y + 0.02, -(d + 1.4) * 0.5), Vector3(w + 1.4, 0.12, 0.12), m_gold)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(sx * (w + 1.4) * 0.5, top_y + 0.02, 0.0), Vector3(0.12, 0.12, d + 1.4), m_gold)
	# raised roof-deck lip + skylight band glowing warm
	_box(root, Vector3(0.0, top_y + 0.34, 0.0), Vector3(w - 3.0, 0.12, d - 3.0), m_warmglow)
	_box(root, Vector3(0.0, top_y + 0.46, 0.0), Vector3(w - 3.4, 0.08, d - 3.4), m_glass)

	# Pergola over the front terrace lounge — gold beams on chrome posts
	var perg_z: float = hd + 2.2
	for sx: float in [-1.0, 1.0]:
		_cyl(root, Vector3(sx * (w * 0.32), 1.4, perg_z), 0.1, 0.1, 2.8, m_chrome, 12)
		_cyl(root, Vector3(sx * (w * 0.32), 1.4, hd + 0.4), 0.1, 0.1, 2.8, m_chrome, 12)
	_box(root, Vector3(w * 0.32, 2.85, hd + 1.3), Vector3(0.14, 0.16, 4.0), m_gold)
	_box(root, Vector3(-w * 0.32, 2.85, hd + 1.3), Vector3(0.14, 0.16, 4.0), m_gold)
	for i: int in range(7):
		var t: float = float(i) / 6.0
		var px: float = lerp(-w * 0.32, w * 0.32, t)
		_box(root, Vector3(px, 2.9, hd + 1.3), Vector3(0.1, 0.1, 4.2), m_brass)

	# =====================================================================
	# 4. INTERIOR — ceiling, sunken lounge, bar, partial walls, showpieces
	#    Kept OPEN + uncluttered so the owner can furnish + walk through.
	# =====================================================================
	# Ceiling slab (interior side) with a recessed cove
	_box(root, Vector3(0.0, top_y - 0.02, 0.0), Vector3(w - 0.4, 0.16, d - 0.4), m_marble)
	_box(root, Vector3(0.0, top_y - 0.12, 0.0), Vector3(w - 3.5, 0.1, d - 3.5), m_warmglow)
	# coffered gold cove frame around the warm ceiling light
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(sx * (w - 3.2) * 0.5, top_y - 0.1, 0.0), Vector3(0.1, 0.12, d - 3.2), m_gold)
		_box(root, Vector3(0.0, top_y - 0.1, sx * (d - 3.2) * 0.5), Vector3(w - 3.2, 0.12, 0.1), m_gold)

	# ---- Partial interior wall splitting a private suite (back-left) -----
	_box(root, Vector3(-w * 0.5 + 3.4, wall_y, -hd + 2.6), Vector3(0.2, suite_h - 0.3, 5.0), m_marble)
	_box(root, Vector3(-w * 0.5 + 5.6, wall_y, -hd + 0.1), Vector3(4.6, suite_h - 0.3, 0.2), m_marble)
	# gold reveal trim on the partition edge
	_box(root, Vector3(-w * 0.5 + 3.4, wall_y, -hd + 5.05), Vector3(0.24, suite_h - 0.3, 0.06), m_gold)

	# ---- GRAND MARBLE COLUMNS with brass capitals defining the lounge ----
	# Four columns frame an axial promenade from the open front to the bar.
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			var col_x: float = sx * (w * 0.26)
			var col_z: float = sz * (d * 0.22)
			_column(root, Vector3(col_x, 0.1, col_z), 0.26, suite_h - 0.2, m_marble, m_brass)

	# ---- GRAND STAIR (showpiece): a sculptural marble half-flight rising to a
	# mezzanine landing against the back-right, with gold balustrade + runner.
	var st_x: float = w * 0.5 - 2.6
	var st_z0: float = hd - 1.2
	var steps: int = 7
	var step_rise: float = 0.22
	var step_run: float = 0.42
	for i: int in range(steps):
		var sy: float = 0.1 + step_rise * (float(i) + 0.5)
		var sz: float = st_z0 - step_run * float(i)
		_box(root, Vector3(st_x, sy, sz), Vector3(3.0, step_rise, step_run + 0.06), m_marble)
		# gold nosing on each tread
		_box(root, Vector3(st_x, sy + step_rise * 0.5 + 0.01, sz + step_run * 0.5), Vector3(3.0, 0.03, 0.05), m_gold)
		# warm step-light strip on the open (+x) stringer
		_box(root, Vector3(st_x + 1.52, sy, sz), Vector3(0.04, 0.06, step_run), m_warmglow)
	# mezzanine landing the stair arrives on
	var land_y: float = 0.1 + step_rise * float(steps)
	var land_z: float = st_z0 - step_run * float(steps) - 1.0
	_box(root, Vector3(st_x, land_y, land_z), Vector3(3.0, 0.16, 2.4), m_marble)
	_box(root, Vector3(st_x, land_y + 0.1, land_z), Vector3(2.6, 0.04, 2.0), m_marble_dk)
	# gold balustrade posts + cap rail along the open side of the stair
	for i: int in range(steps + 1):
		var bz: float = st_z0 + step_run * 0.5 - step_run * float(i)
		var by: float = 0.1 + step_rise * float(i)
		_cyl(root, Vector3(st_x - 1.55, by + 0.5, bz), 0.04, 0.04, 1.0, m_gold, 8)
	_box(root, Vector3(st_x - 1.55, land_y + 0.95, (st_z0 + land_z) * 0.5), Vector3(0.06, 0.06, abs(st_z0 - land_z) + 0.4), m_gold).rotation = Vector3(deg_to_rad(-18.0), 0.0, 0.0)

	# ---- SUNKEN LOUNGE (center-front): recessed pit framed in gold --------
	var pit_w: float = 6.0
	var pit_d: float = 4.4
	var pit_depth: float = 0.5
	# pit floor (lower than the suite floor)
	_box(root, Vector3(0.0, 0.1 - pit_depth, hd - 3.6), Vector3(pit_w, 0.1, pit_d), m_marble_dk)
	# step ring around the pit (the raised lip you step down from)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(sx * (pit_w * 0.5 + 0.25), 0.1 - pit_depth * 0.5, hd - 3.6), Vector3(0.4, pit_depth, pit_d + 0.5), m_marble)
		_box(root, Vector3(sx * (pit_w * 0.5 + 0.25), 0.12, hd - 3.6), Vector3(0.46, 0.06, pit_d + 0.5), m_gold)
	_box(root, Vector3(0.0, 0.1 - pit_depth * 0.5, hd - 3.6 - pit_d * 0.5 - 0.25), Vector3(pit_w + 1.0, pit_depth, 0.4), m_marble)
	_box(root, Vector3(0.0, 0.12, hd - 3.6 - pit_d * 0.5 - 0.25), Vector3(pit_w + 1.0, 0.06, 0.46), m_gold)
	# low built-in bench seating ringing the back of the pit (showpiece, low)
	_box(root, Vector3(0.0, 0.1 - pit_depth + 0.28, hd - 3.6 - pit_d * 0.5 + 0.35), Vector3(pit_w - 0.6, 0.5, 0.6), m_wood)
	_box(root, Vector3(0.0, 0.1 - pit_depth + 0.5, hd - 3.6 - pit_d * 0.5 + 0.1), Vector3(pit_w - 0.6, 0.16, 0.18), m_gold)
	# warm-glow firepit table in the pit center
	_cyl(root, Vector3(0.0, 0.1 - pit_depth + 0.22, hd - 3.6), 0.7, 0.8, 0.4, m_charcoal, 20)
	_cyl(root, Vector3(0.0, 0.1 - pit_depth + 0.44, hd - 3.6), 0.55, 0.55, 0.08, m_warmglow, 20)

	# ---- BAR (back-right): marble counter, gold rail, backbar shelving ----
	var bar_x: float = w * 0.5 - 3.0
	var bar_z: float = -hd + 3.0
	# counter body
	_box(root, Vector3(bar_x, 0.6, bar_z), Vector3(4.2, 1.0, 1.0), m_marble)
	# veined front panel + gold kick + top
	_box(root, Vector3(bar_x, 0.6, bar_z + 0.52), Vector3(4.2, 0.9, 0.06), m_marble_dk)
	_box(root, Vector3(bar_x, 0.18, bar_z + 0.52), Vector3(4.2, 0.12, 0.04), m_gold)
	_box(root, Vector3(bar_x, 1.12, bar_z), Vector3(4.4, 0.08, 1.1), m_wood)
	_box(root, Vector3(bar_x, 1.17, bar_z + 0.5), Vector3(4.4, 0.04, 0.08), m_gold)
	# backbar: floating glowing shelves against the feature wall
	for i: int in range(3):
		var sy: float = 1.4 + float(i) * 0.55
		_box(root, Vector3(bar_x, sy, -hd + 0.5), Vector3(4.0, 0.06, 0.4), m_wood)
		_box(root, Vector3(bar_x, sy - 0.02, -hd + 0.38), Vector3(4.0, 0.04, 0.06), m_warmglow)
	# a few bottle hints (gold + glass)
	for i: int in range(6):
		var t: float = float(i) / 5.0
		var bx: float = lerp(bar_x - 1.7, bar_x + 1.7, t)
		_cyl(root, Vector3(bx, 1.65, -hd + 0.5), 0.05, 0.06, 0.4, m_glass if i % 2 == 0 else m_gold, 8)
	# three bar stools (chrome stem + gold seat)
	for i: int in range(3):
		var bx2: float = bar_x - 1.3 + float(i) * 1.3
		_cyl(root, Vector3(bx2, 0.45, bar_z + 1.1), 0.04, 0.05, 0.9, m_chrome, 10)
		_cyl(root, Vector3(bx2, 0.92, bar_z + 1.1), 0.3, 0.3, 0.1, m_gold, 16)

	# ---- GRAND CRYSTAL CHANDELIER over the lounge (showpiece) ------------
	_cyl(root, Vector3(0.0, top_y - 0.2, hd - 3.6), 0.04, 0.04, 0.5, m_gold, 8)
	_torus(root, Vector3(0.0, top_y - 0.55, hd - 3.6), 0.55, 0.95, m_gold_br)
	_torus(root, Vector3(0.0, top_y - 0.95, hd - 3.6), 0.4, 0.68, m_gold_br)
	_torus(root, Vector3(0.0, top_y - 1.3, hd - 3.6), 0.22, 0.4, m_gold)
	# three tiers of crystal droplets + a glowing core
	for i: int in range(16):
		var a: float = TAU * float(i) / 16.0
		_ball(root, Vector3(cos(a) * 0.82, top_y - 0.72, hd - 3.6 + sin(a) * 0.82), 0.09, m_warmglow)
		_ball(root, Vector3(cos(a + 0.2) * 0.56, top_y - 1.12, hd - 3.6 + sin(a + 0.2) * 0.56), 0.075, m_warmglow)
	for i: int in range(8):
		var a2: float = TAU * float(i) / 8.0
		_ball(root, Vector3(cos(a2) * 0.3, top_y - 1.46, hd - 3.6 + sin(a2) * 0.3), 0.07, m_warmglow)
	_ball(root, Vector3(0.0, top_y - 1.5, hd - 3.6), 0.18, m_warmglow)

	# ---- Feature-wall fireplace + media niche (centered low on back wall) -
	_box(root, Vector3(0.0, 0.7, -hd + 0.16), Vector3(3.2, 1.2, 0.12), m_charcoal)
	_box(root, Vector3(0.0, 0.45, -hd + 0.22), Vector3(2.6, 0.4, 0.06), m_warmglow)
	_box(root, Vector3(0.0, 1.4, -hd + 0.18), Vector3(3.4, 0.12, 0.14), m_gold)
	# carved gold mantel + flanking pilasters
	_box(root, Vector3(0.0, 1.5, -hd + 0.26), Vector3(3.6, 0.16, 0.22), m_gold)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(sx * 1.7, 0.7, -hd + 0.2), Vector3(0.16, 1.4, 0.12), m_gold)

	# ---- Sculptural gold floor lamps flanking the open front -------------
	for sx: float in [-1.0, 1.0]:
		var lx: float = sx * (w * 0.5 - 1.4)
		_cyl(root, Vector3(lx, 0.05, hd - 1.4), 0.18, 0.22, 0.1, m_chrome, 14)
		_cyl(root, Vector3(lx, 1.1, hd - 1.4), 0.04, 0.04, 2.1, m_gold, 10)
		_ball(root, Vector3(lx, 2.2, hd - 1.4), 0.22, m_warmglow)

	# ---- Interior statues on plinths flanking the back feature wall ------
	for sx: float in [-1.0, 1.0]:
		_statue(root, Vector3(sx * (w * 0.5 - 1.0), 0.1, -hd + 1.5), 0.62, m_ivory, m_marble_dk)

	# =====================================================================
	# 5. TERRACE FEATURES — pool, planters, loungers, fountains, statues
	# =====================================================================
	# Infinity-edge reflecting pool along the +x terrace wing
	var pool_x: float = eo_w * 0.5 - 1.8
	_box(root, Vector3(pool_x, 0.12, 0.0), Vector3(2.2, 0.24, 7.0), m_marble)
	_box(root, Vector3(pool_x, 0.16, 0.0), Vector3(1.7, 0.18, 6.5), m_water)
	_box(root, Vector3(pool_x, 0.2, 0.0), Vector3(1.8, 0.06, 6.6), m_gold)
	# glowing underwater edge line for the infinity drop
	_box(root, Vector3(pool_x + 0.85, 0.06, 0.0), Vector3(0.05, 0.1, 6.6), m_skyline)

	# Grand tiered fountain centered on the front approach (between fire bowls)
	_fountain(root, Vector3(0.0, 0.0, eo_d * 0.5 - 1.6), 1.5, m_marble, m_gold, m_water)

	# Heraldic statues guarding the front approach, on the terrace edge
	for sx: float in [-1.0, 1.0]:
		_statue(root, Vector3(sx * (w * 0.5 + 0.6), 0.0, hd + 3.4), 0.74, m_ivory, m_marble)

	# Gold fire bowls flanking the front approach
	for sx: float in [-1.0, 1.0]:
		var fx: float = sx * (w * 0.5 - 0.5)
		_cyl(root, Vector3(fx, 0.5, hd + 2.6), 0.45, 0.3, 0.8, m_gold, 18)
		_cyl(root, Vector3(fx, 0.92, hd + 2.6), 0.4, 0.4, 0.12, m_warmglow, 18)

	# Sculpted hedge planters with toon greenery along the back terrace
	for i: int in range(5):
		var t: float = float(i) / 4.0
		var px: float = lerp(-eo_w * 0.5 + 1.5, eo_w * 0.5 - 1.5, t)
		_box(root, Vector3(px, 0.35, -eo_d * 0.5 + 1.2), Vector3(1.4, 0.5, 0.8), m_marble)
		_box(root, Vector3(px, 0.55, -eo_d * 0.5 + 1.2), Vector3(1.46, 0.1, 0.86), m_gold)
		_box(root, Vector3(px, 0.95, -eo_d * 0.5 + 1.2), Vector3(1.2, 0.6, 0.6), m_hedge)
		_ball(root, Vector3(px, 1.3, -eo_d * 0.5 + 1.2), 0.5, m_hedge)

	# Manicured topiary cones in gold urns flanking the front gap
	for sx: float in [-1.0, 1.0]:
		var tx: float = sx * (eo_w * 0.18 + 0.6)
		_cyl(root, Vector3(tx, 0.3, eo_d * 0.5 - 0.9), 0.35, 0.28, 0.6, m_gold, 16)
		_cone(root, Vector3(tx, 1.2, eo_d * 0.5 - 0.9), 0.42, 1.5, m_hedge, 14)
		_cone(root, Vector3(tx, 1.85, eo_d * 0.5 - 0.9), 0.28, 0.9, m_hedge, 14)

	# Two daybeds / loungers on the -x terrace wing
	var m_cushion: Material = _gloss(Color(0.90, 0.88, 0.84))
	for i: int in range(2):
		var lz: float = -2.4 + float(i) * 4.8
		_box(root, Vector3(-eo_w * 0.5 + 1.8, 0.3, lz), Vector3(1.2, 0.3, 2.4), m_wood)
		_box(root, Vector3(-eo_w * 0.5 + 1.8, 0.52, lz), Vector3(1.1, 0.22, 2.2), m_cushion)
		_box(root, Vector3(-eo_w * 0.5 + 1.8, 0.72, lz - 0.9), Vector3(1.1, 0.5, 0.18), m_cushion)
		_box(root, Vector3(-eo_w * 0.5 + 1.8, 0.18, lz), Vector3(1.26, 0.06, 2.46), m_gold)

	# =====================================================================
	# 6. CANTILEVERED BALCONIES + PRIVATE ROOFTOP CABANA (silhouette toppers)
	# =====================================================================
	# Two slim cantilevered Juliet balconies projecting off the side glazing —
	# small gold-railed platforms that enrich the silhouette at mid height.
	for sx: float in [-1.0, 1.0]:
		var balc_x: float = sx * (hw + 0.9)
		_box(root, Vector3(balc_x, 1.0, 0.0), Vector3(1.8, 0.14, 4.4), m_marble)
		_box(root, Vector3(balc_x, 0.93, 0.0), Vector3(1.9, 0.06, 4.5), m_gold)
		# gold rail around the outer edge
		_cyl(root, Vector3(sx * (hw + 1.7), 1.5, 0.0), 0.04, 0.04, 4.4, m_gold, 8).rotation = Vector3(deg_to_rad(90.0), 0.0, 0.0)
		for sz: float in [-1.0, 1.0]:
			_cyl(root, Vector3(sx * (hw + 1.7), 1.25, sz * 2.1), 0.04, 0.04, 0.9, m_gold, 6)
		# diagonal brass cantilever brackets under the slab
		_box(root, Vector3(sx * (hw + 0.7), 0.7, 0.0), Vector3(0.1, 0.5, 0.5), m_brass).rotation = Vector3(0.0, 0.0, deg_to_rad(sx * 35.0))

	# slim glass cabana box on the roof, back-left, now with a dormered gable
	_box(root, Vector3(-w * 0.25, top_y + 1.3, -d * 0.2), Vector3(4.5, 2.2, 4.0), m_glass)
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_cyl(root, Vector3(-w * 0.25 + sx * 2.1, top_y + 1.3, -d * 0.2 + sz * 1.85), 0.08, 0.08, 2.3, m_gold, 10)
	_box(root, Vector3(-w * 0.25, top_y + 2.5, -d * 0.2), Vector3(4.8, 0.16, 4.3), m_slab)
	_box(root, Vector3(-w * 0.25, top_y + 2.42, -d * 0.2), Vector3(4.9, 0.06, 4.4), m_gold)
	# dormer gable crowning the cabana (a luxe roof accent)
	_prism(root, Vector3(-w * 0.25, top_y + 3.1, -d * 0.2), Vector3(4.4, 1.0, 4.0), m_marble)
	_box(root, Vector3(-w * 0.25, top_y + 2.62, -d * 0.2), Vector3(4.6, 0.1, 4.2), m_gold)
	# glowing window in the gable face (+z side of the cabana)
	_box(root, Vector3(-w * 0.25, top_y + 3.15, -d * 0.2 + 2.0), Vector3(1.4, 0.7, 0.06), m_window)

	# glowing landing-pad ring on the roof, front-right
	_torus(root, Vector3(w * 0.25, top_y + 0.32, d * 0.2), 1.3, 1.6, m_skyline)
	_box(root, Vector3(w * 0.25, top_y + 0.3, d * 0.2), Vector3(0.3, 0.06, 1.8), m_warmglow)
	_box(root, Vector3(w * 0.25, top_y + 0.3, d * 0.2), Vector3(1.8, 0.06, 0.3), m_warmglow)

	# crowning gold obelisk + glowing finials at the roof corners — a tier topper
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			var ox: float = sx * (w * 0.5 + 0.3)
			var oz: float = sz * (d * 0.5 + 0.3)
			_box(root, Vector3(ox, top_y + 0.7, oz), Vector3(0.34, 0.9, 0.34), m_gold)
			_cone(root, Vector3(ox, top_y + 1.35, oz), 0.22, 0.5, m_gold_br, 4)
			_ball(root, Vector3(ox, top_y + 1.7, oz), 0.1, m_warmglow)

	# antenna / beacon spire — final silhouette accent
	_cyl(root, Vector3(w * 0.4, top_y + 2.0, -d * 0.4), 0.06, 0.1, 3.6, m_chrome, 10)
	_ball(root, Vector3(w * 0.4, top_y + 4.0, -d * 0.4), 0.18, m_skyline)

	return root

# ---------------------------------------------------------------------------
# META
# ---------------------------------------------------------------------------

static func meta() -> Dictionary:
	return {
		"id": "sky_penthouse",
		"name": "Aurelia Sky Penthouse",
		"tier": "Penthouse",
		"rarity": "Epic",
		"description": "A glass-walled sky suite crowning a private tower: a wraparound marble terrace with a glowing skyline edge, tiered fountain, statues and infinity pool, a sunken firepit lounge framed by marble columns, a grand marble stair to a mezzanine landing, and a gold-and-marble bar beneath a cascading crystal chandelier. Cantilevered balconies, a dormered rooftop cabana, and warm interior light make it the most coveted address in the Verse.",
		"footprint": [16.0, 13.0],
		"floors": 1,
		"attributes": [
			["Style", "Modern Glass Penthouse"],
			["Material", "Gold, Marble & Glass"],
			["Feature", "Grand Stair, Crystal Chandelier & Sky Terrace"],
			["Showpiece", "Tiered Fountain, Statues & Sunken Bar"],
			["Floors", "1 (Sky Slab)"],
			["Vibe", "Luxe City-Light Glamour"]
		]
	}
