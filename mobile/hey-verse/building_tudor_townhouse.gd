# Hey Verse — premium procedural BUILDING module.
# id: tudor_townhouse  (Rare Tudor townhouse: black-and-white half-timbering,
# jettied upper floor, leaded bay windows, steep gable, warm 2-floor interior).
#
# LUXURY ENHANCE pass: heritage Tudor read kept intact but pushed firmly into
# the high-end tier — tasteful brass/gold trim, carved stone statues, a tiered
# garden fountain, a wrought-iron front balcony, gilded entrance columns,
# roof dormers, layered landscaping, glowing leaded windows, and a richer
# interior showpiece set (grand carved stair, crystal-and-brass chandelier,
# inglenook fireplace). The ground floor stays CLEAN, WALKABLE and open with
# NO front wall so the camera looks straight in.
#
# Standalone: loads res://toon.gdshader + res://outline.gdshader by path with
# ResourceLoader.exists() guards and a StandardMaterial3D fallback, so the module
# parses and runs even without the shaders present. No preloads, no external assets.
class_name VerseBuildingTudorTownhouse
extends RefCounted

# ---------------------------------------------------------------------------
# Shader loading (guarded) + material helpers
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

# Core toon material factory. Returns a ShaderMaterial when the toon shader is
# available (with an inverted-hull outline as next_pass), else a tasteful
# StandardMaterial3D fallback so the build still renders.
static func _mk(col: Color, rim: float, spec: float, metal: float, rough: float, emit: Color, emit_e: float) -> Material:
	var toon: Shader = _toon_shader()
	if toon != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = toon
		m.set_shader_parameter("albedo", col)
		m.set_shader_parameter("rim_strength", rim)
		m.set_shader_parameter("spec_strength", spec)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		var outline: Shader = _outline_shader()
		if outline != null:
			var o: ShaderMaterial = ShaderMaterial.new()
			o.shader = outline
			o.set_shader_parameter("thickness", 0.016)
			o.set_shader_parameter("line_color", Color(0.06, 0.07, 0.10, 1.0))
			m.next_pass = o
		return m
	# Fallback — keeps the wealth read even without the cel shader.
	var sm: StandardMaterial3D = StandardMaterial3D.new()
	sm.albedo_color = col
	sm.metallic = metal
	sm.roughness = rough
	sm.rim_enabled = rim > 0.0
	sm.rim = rim
	if emit_e > 0.0:
		sm.emission_enabled = true
		sm.emission = emit
		sm.emission_energy_multiplier = emit_e
	return sm

# Wind-enabled toon material (for hedges/foliage sway).
static func _mk_wind(col: Color, wind: float, wind_h: float) -> Material:
	var toon: Shader = _toon_shader()
	if toon != null:
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = toon
		m.set_shader_parameter("albedo", col)
		m.set_shader_parameter("rim_strength", 0.30)
		m.set_shader_parameter("spec_strength", 0.0)
		m.set_shader_parameter("wind_strength", wind)
		m.set_shader_parameter("wind_height", wind_h)
		var outline: Shader = _outline_shader()
		if outline != null:
			var o: ShaderMaterial = ShaderMaterial.new()
			o.shader = outline
			o.set_shader_parameter("thickness", 0.014)
			o.set_shader_parameter("line_color", Color(0.05, 0.10, 0.06, 1.0))
			m.next_pass = o
		return m
	var sm: StandardMaterial3D = StandardMaterial3D.new()
	sm.albedo_color = col
	sm.roughness = 1.0
	return sm

# Semantic material helpers --------------------------------------------------
static func _toon(col: Color) -> Material:
	return _mk(col, 0.32, 0.0, 0.0, 1.0, Color.BLACK, 0.0)

static func _metal(col: Color) -> Material:
	return _mk(col, 0.45, 0.6, 0.9, 0.25, Color.BLACK, 0.0)

# Polished gilded brass/gold — the luxury accent material (faint warm glow so
# the trim catches light even in the toon-shaded fallback).
static func _gold(col: Color) -> Material:
	return _mk(col, 0.55, 0.85, 1.0, 0.14, Color(0.55, 0.42, 0.14), 0.45)

static func _gloss(col: Color) -> Material:
	return _mk(col, 0.40, 0.5, 0.1, 0.2, Color.BLACK, 0.0)

# Polished marble/ashlar for statues + columns (cool spec, near-white).
static func _marble(col: Color) -> Material:
	return _mk(col, 0.42, 0.45, 0.05, 0.3, Color.BLACK, 0.0)

static func _glass(col: Color) -> Material:
	var m: Material = _mk(col, 0.55, 0.7, 0.0, 0.05, Color.BLACK, 0.0)
	if m is StandardMaterial3D:
		var sm: StandardMaterial3D = m
		sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		sm.albedo_color.a = 0.55
	return m

# Translucent water for the fountain (subtle aqua, see-through).
static func _water(col: Color) -> Material:
	var m: Material = _mk(col, 0.5, 0.9, 0.0, 0.02, Color(0.3, 0.6, 0.7), 0.35)
	if m is StandardMaterial3D:
		var sm: StandardMaterial3D = m
		sm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		sm.albedo_color.a = 0.6
	return m

static func _glow(col: Color, e: float) -> Material:
	return _mk(col, 0.20, 0.0, 0.0, 1.0, col, e)

# ---------------------------------------------------------------------------
# Primitive helpers — every node parented under a returned Node3D by the caller.
# ---------------------------------------------------------------------------
static func _box(parent: Node3D, size: Vector3, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var mesh: BoxMesh = BoxMesh.new()
	mesh.size = size
	mi.mesh = mesh
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var mesh: CylinderMesh = CylinderMesh.new()
	mesh.top_radius = r_top
	mesh.bottom_radius = r_bot
	mesh.height = h
	mesh.radial_segments = 18
	mi.mesh = mesh
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _ball(parent: Node3D, r: float, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var mesh: SphereMesh = SphereMesh.new()
	mesh.radius = r
	mesh.height = r * 2.0
	mesh.radial_segments = 20
	mesh.rings = 12
	mi.mesh = mesh
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

static func _torus(parent: Node3D, inner: float, outer: float, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var mesh: TorusMesh = TorusMesh.new()
	mesh.inner_radius = inner
	mesh.outer_radius = outer
	mesh.rings = 20
	mesh.ring_segments = 14
	mi.mesh = mesh
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

# Triangular prism (gable / roof slope). Extruded along +x by `width`,
# cross-section is the right-triangle/isoceles in the z-y plane.
static func _prism(parent: Node3D, size: Vector3, pos: Vector3, mat: Material) -> MeshInstance3D:
	var mi: MeshInstance3D = MeshInstance3D.new()
	var mesh: PrismMesh = PrismMesh.new()
	mesh.size = size
	mi.mesh = mesh
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi

# ---------------------------------------------------------------------------
# Palette — Tudor: dark oak timbers, white lime stucco, warm window glow,
# grey ashlar plinth, terracotta/slate roof, brass + gilded gold accents.
# ---------------------------------------------------------------------------
const C_TIMBER: Color = Color(0.18, 0.12, 0.08)      # dark stained oak
const C_TIMBER_HI: Color = Color(0.26, 0.18, 0.11)   # lighter oak (door/frames)
const C_STUCCO: Color = Color(0.95, 0.93, 0.86)      # warm lime-wash white
const C_PLINTH: Color = Color(0.52, 0.51, 0.49)      # grey ashlar stone
const C_STONE: Color = Color(0.60, 0.59, 0.57)
const C_MARBLE: Color = Color(0.90, 0.89, 0.85)      # pale honed marble
const C_ROOF: Color = Color(0.46, 0.27, 0.18)        # terracotta/slate brown
const C_ROOF_HI: Color = Color(0.55, 0.33, 0.22)
const C_CHIMNEY: Color = Color(0.55, 0.30, 0.24)     # warm brick
const C_GLASS: Color = Color(0.66, 0.78, 0.86)
const C_GLOW: Color = Color(1.0, 0.86, 0.55)         # warm interior glow
const C_BRASS: Color = Color(0.83, 0.66, 0.30)
const C_GOLD: Color = Color(0.95, 0.78, 0.36)        # bright gilded gold leaf
const C_WATER: Color = Color(0.55, 0.78, 0.82)       # fountain water
const C_FLOOR: Color = Color(0.42, 0.29, 0.18)       # warm board floor
const C_WALL_IN: Color = Color(0.90, 0.86, 0.78)     # interior plaster
const C_HEDGE: Color = Color(0.22, 0.40, 0.20)
const C_LEAF: Color = Color(0.26, 0.46, 0.24)        # brighter garden foliage
const C_BLOOM: Color = Color(0.86, 0.42, 0.55)       # flower-bed bloom
const C_PATH: Color = Color(0.62, 0.60, 0.56)

# ---------------------------------------------------------------------------
# Footprint constants (villa-ish, narrow town lot). Entrance faces +z.
# ---------------------------------------------------------------------------
const W: float = 8.0      # x width
const D: float = 7.0      # z depth
const H1: float = 3.0     # ground ceiling height
const H2: float = 3.0     # upper ceiling height
const JET: float = 0.45   # jetty overhang of the upper floor
const WALL: float = 0.18  # wall thickness

# ===========================================================================
# BUILD
# ===========================================================================
static func build() -> Node3D:
	var root: Node3D = Node3D.new()
	root.name = "TudorTownhouse"

	_build_ground_and_landscape(root)
	_build_fountain(root)
	_build_shell(root)
	_build_half_timbering(root)
	_build_windows(root)
	_build_entrance(root)
	_build_balcony(root)
	_build_roof(root)
	_build_dormers(root)
	_build_chimney(root)
	_build_interior(root)

	return root

# --- Ground slab, path, hedges, statues, lanterns, flower beds --------------
static func _build_ground_and_landscape(root: Node3D) -> void:
	var mat_stone: Material = _toon(C_STONE)
	var mat_path: Material = _gloss(C_PATH)
	var marble: Material = _marble(C_MARBLE)
	var gold: Material = _gold(C_GOLD)

	# Plinth slab the house sits on, with a gilded edge fillet for the tier read.
	_box(root, Vector3(W + 0.8, 0.3, D + 0.8), Vector3(0, 0.15, 0), _toon(C_PLINTH))
	_box(root, Vector3(W + 0.92, 0.06, D + 0.92), Vector3(0, 0.31, 0), gold)

	# Front cobble path leading to the door (+z), bordered by thin brass strips.
	for i: int in range(5):
		var z: float = D * 0.5 + 0.7 + float(i) * 0.85
		_box(root, Vector3(2.0, 0.08, 0.72), Vector3(0, 0.04, z), mat_path)
		for sb: float in [-1.0, 1.0]:
			_box(root, Vector3(0.06, 0.06, 0.72), Vector3(sb * 1.02, 0.06, z), gold)

	# Low clipped hedges flanking the path (wind sway).
	var hedge: Material = _mk_wind(C_HEDGE, 0.5, 0.7)
	for s: float in [-1.0, 1.0]:
		for j: int in range(4):
			var hz: float = D * 0.5 + 1.0 + float(j) * 0.95
			_box(root, Vector3(0.6, 0.7, 0.8), Vector3(s * 1.9, 0.35, hz), hedge)

	# Manicured flower beds outside the hedges (pops of colour, gentle sway).
	var bloom: Material = _mk_wind(C_BLOOM, 0.6, 1.2)
	var leaf: Material = _mk_wind(C_LEAF, 0.4, 0.9)
	for s_fb: float in [-1.0, 1.0]:
		for k: int in range(3):
			var fz: float = D * 0.5 + 1.3 + float(k) * 1.15
			_box(root, Vector3(0.55, 0.18, 0.85), Vector3(s_fb * 2.75, 0.18, fz), leaf)
			_ball(root, 0.16, Vector3(s_fb * 2.75, 0.4, fz), bloom)

	# A pair of topiary balls in stone pots by the door.
	for s2: float in [-1.0, 1.0]:
		var px: float = s2 * 1.45
		var pz: float = D * 0.5 + 0.55
		_cyl(root, 0.28, 0.34, 0.5, Vector3(px, 0.25, pz), mat_stone)
		_box(root, Vector3(0.7, 0.08, 0.7), Vector3(px, 0.5, pz), gold)
		_ball(root, 0.42, Vector3(px, 0.95, pz), _mk_wind(C_HEDGE, 0.35, 1.4))

	# CARVED MARBLE STATUES on gilded plinths flanking the front approach.
	for s_st: float in [-1.0, 1.0]:
		_build_statue(root, marble, gold, Vector3(s_st * 3.3, 0.3, D * 0.5 + 2.6))

	# Wrought-iron carriage lanterns on tall posts, with gilded caps (glowing).
	var iron: Material = _metal(Color(0.10, 0.10, 0.12))
	for s3: float in [-1.0, 1.0]:
		var lx: float = s3 * 2.5
		var lz: float = D * 0.5 + 0.6
		_cyl(root, 0.06, 0.07, 2.1, Vector3(lx, 1.05, lz), iron)
		_torus(root, 0.04, 0.1, Vector3(lx, 2.12, lz), gold)
		_box(root, Vector3(0.3, 0.42, 0.3), Vector3(lx, 2.35, lz), iron)
		_ball(root, 0.13, Vector3(lx, 2.35, lz), _glow(C_GLOW, 3.5))
		_prism(root, Vector3(0.34, 0.2, 0.34), Vector3(lx, 2.66, lz), gold)

# A small classical marble statue (robed figure) on a gilded plinth.
static func _build_statue(root: Node3D, marble: Material, gold: Material, base: Vector3) -> void:
	# Stepped plinth with a gilded band.
	_box(root, Vector3(0.78, 0.2, 0.78), base + Vector3(0, 0.1, 0), marble)
	_box(root, Vector3(0.66, 0.55, 0.66), base + Vector3(0, 0.475, 0), marble)
	_box(root, Vector3(0.7, 0.06, 0.7), base + Vector3(0, 0.78, 0), gold)
	# Figure: tapered robed body, shoulders, head.
	var fy: float = base.y + 0.78
	_cyl(root, 0.16, 0.28, 0.95, base + Vector3(0, fy + 0.475 - base.y, 0), marble)
	_box(root, Vector3(0.44, 0.18, 0.26), base + Vector3(0, fy + 0.95, 0), marble)
	_ball(root, 0.15, base + Vector3(0, fy + 1.22, 0), marble)
	# One raised arm holding a gilded laurel/orb.
	var arm: MeshInstance3D = _box(root, Vector3(0.5, 0.1, 0.1), base + Vector3(0.12, fy + 0.95, 0.05), marble)
	arm.rotation.z = 0.7
	_ball(root, 0.1, base + Vector3(0.3, fy + 1.25, 0.05), gold)

# --- Tiered marble + gold garden fountain in front of the path -------------
static func _build_fountain(root: Node3D) -> void:
	var marble: Material = _marble(C_MARBLE)
	var gold: Material = _gold(C_GOLD)
	var water: Material = _water(C_WATER)
	var fz: float = D * 0.5 + 5.1
	# Wide lower basin (octagon-ish via stacked stone rings).
	_cyl(root, 1.5, 1.6, 0.42, Vector3(0, 0.21, fz), marble)
	_torus(root, 1.3, 1.55, Vector3(0, 0.42, fz), gold)
	_cyl(root, 1.35, 1.35, 0.1, Vector3(0, 0.3, fz), water)
	# Central pedestal + mid bowl.
	_cyl(root, 0.22, 0.3, 0.8, Vector3(0, 0.7, fz), marble)
	_cyl(root, 0.62, 0.55, 0.18, Vector3(0, 1.05, fz), marble)
	_torus(root, 0.5, 0.62, Vector3(0, 1.14, fz), gold)
	_cyl(root, 0.5, 0.5, 0.06, Vector3(0, 1.1, fz), water)
	# Upper stem + small top bowl.
	_cyl(root, 0.12, 0.18, 0.5, Vector3(0, 1.4, fz), marble)
	_cyl(root, 0.3, 0.26, 0.12, Vector3(0, 1.66, fz), marble)
	_torus(root, 0.2, 0.3, Vector3(0, 1.72, fz), gold)
	# Gilded finial jet + arcing water spheres.
	_ball(root, 0.12, Vector3(0, 1.95, fz), gold)
	for i: int in range(6):
		var a: float = TAU * float(i) / 6.0
		_ball(root, 0.07, Vector3(cos(a) * 0.55, 1.55 - 0.1, fz + sin(a) * 0.55), water)
		_ball(root, 0.05, Vector3(cos(a) * 0.95, 0.95, fz + sin(a) * 0.95), water)

# --- Solid masonry shell (ground floor) + jettied upper box ----------------
static func _build_shell(root: Node3D) -> void:
	var stucco: Material = _toon(C_STUCCO)
	var plinth: Material = _toon(C_PLINTH)
	var floor_y0: float = 0.3

	# Stone plinth course around the ground floor base (low wall band).
	_box(root, Vector3(W, 0.5, WALL), Vector3(0, floor_y0 + 0.25, -D * 0.5 + WALL * 0.5), plinth)
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(WALL, 0.5, D), Vector3(s * (W * 0.5 - WALL * 0.5), floor_y0 + 0.25, 0), plinth)

	# GROUND FLOOR walls — back + two sides, FRONT (+z) OMITTED so camera looks in.
	var gy: float = floor_y0 + H1 * 0.5
	# Back wall (-z)
	_box(root, Vector3(W, H1, WALL), Vector3(0, gy, -D * 0.5 + WALL * 0.5), stucco)
	# Side walls
	for s2: float in [-1.0, 1.0]:
		_box(root, Vector3(WALL, H1, D), Vector3(s2 * (W * 0.5 - WALL * 0.5), gy, 0), stucco)
	# Front (+z): only short returns at the corners + a low threshold parapet,
	# leaving the middle open as a wide "looking-in" facade gap.
	for s3: float in [-1.0, 1.0]:
		_box(root, Vector3(1.3, H1, WALL), Vector3(s3 * (W * 0.5 - 0.65), gy, D * 0.5 - WALL * 0.5), stucco)
	# Low front threshold band (knee-high parapet).
	_box(root, Vector3(W - 2.4, 0.55, WALL), Vector3(0, floor_y0 + 0.275, D * 0.5 - WALL * 0.5), plinth)

	# Floor band between storeys (the jetty bressummer beam carries the overhang).
	var midy: float = floor_y0 + H1
	# UPPER FLOOR — jettied OUT on all four sides (the signature Tudor overhang).
	var uw: float = W + JET * 2.0
	var ud: float = D + JET * 2.0
	# Bressummer beams under the overhang (dark oak), all 4 sides.
	var beam: Material = _toon(C_TIMBER)
	var gold: Material = _gold(C_GOLD)
	_box(root, Vector3(uw, 0.22, 0.26), Vector3(0, midy + 0.05, ud * 0.5 - 0.13), beam)   # front
	_box(root, Vector3(uw, 0.22, 0.26), Vector3(0, midy + 0.05, -ud * 0.5 + 0.13), beam)  # back
	for s4: float in [-1.0, 1.0]:
		_box(root, Vector3(0.26, 0.22, ud), Vector3(s4 * (uw * 0.5 - 0.13), midy + 0.05, 0), beam)
	# Gilded bead running along the front bressummer (luxury cornice line).
	_box(root, Vector3(uw, 0.05, 0.05), Vector3(0, midy + 0.17, ud * 0.5 - 0.06), gold)
	# Carved corbel brackets supporting the jetty (front face accents).
	for s5: float in [-1.0, 1.0]:
		_prism(root, Vector3(0.22, 0.4, 0.5), Vector3(s5 * (W * 0.5 - 0.4), midy - 0.1, D * 0.5 - 0.1), beam)

	# Upper-floor stucco box (jettied). Front omitted too for interior view.
	var uyc: float = midy + 0.12 + H2 * 0.5
	_box(root, Vector3(uw, H2, WALL), Vector3(0, uyc, -ud * 0.5 + WALL * 0.5), stucco)  # back
	for s6: float in [-1.0, 1.0]:
		_box(root, Vector3(WALL, H2, ud), Vector3(s6 * (uw * 0.5 - WALL * 0.5), uyc, 0), stucco)  # sides
	# Upper front: corner returns only (open middle bay for the leaded bay window region).
	for s7: float in [-1.0, 1.0]:
		_box(root, Vector3(1.0, H2, WALL), Vector3(s7 * (uw * 0.5 - 0.5), uyc, ud * 0.5 - WALL * 0.5), stucco)
	# A spanning lintel across the open upper front so the gable has something to sit on.
	_box(root, Vector3(uw, 0.3, WALL), Vector3(0, uyc + H2 * 0.5 - 0.15, ud * 0.5 - WALL * 0.5), stucco)

# --- Decorative half-timbering grid on the stucco faces --------------------
static func _build_half_timbering(root: Node3D) -> void:
	var timber: Material = _toon(C_TIMBER)
	var floor_y0: float = 0.3
	var midy: float = floor_y0 + H1
	var uw: float = W + JET * 2.0
	var ud: float = D + JET * 2.0

	# GROUND-floor timbering on the two side walls + back (front mostly open).
	for s: float in [-1.0, 1.0]:
		var sx: float = s * (W * 0.5 - WALL)
		_timber_panel_side(root, timber, sx, floor_y0, H1, D, true)
	_timber_panel_back(root, timber, -D * 0.5 + WALL, floor_y0, H1, W, false)

	# UPPER-floor timbering — richer, with diagonal braces (the showy storey).
	var uy0: float = midy + 0.12
	for s2: float in [-1.0, 1.0]:
		var ux: float = s2 * (uw * 0.5 - WALL)
		_timber_panel_side(root, timber, ux, uy0, H2, ud, true)
	_timber_panel_back(root, timber, -ud * 0.5 + WALL, uy0, H2, uw, true)
	# Front upper corner returns get timbering too.
	for s3: float in [-1.0, 1.0]:
		_box(root, Vector3(0.16, H2 - 0.2, 0.06), Vector3(s3 * (uw * 0.5 - 0.5), uy0 + H2 * 0.5, ud * 0.5 - WALL + 0.04), timber)

# Vertical studs + horizontal rails on a SIDE wall (runs along z, faces ±x).
static func _timber_panel_side(root: Node3D, mat: Material, x: float, y0: float, h: float, depth: float, braces: bool) -> void:
	var face_x: float = x + (0.05 if x < 0.0 else -0.05)
	# Top + bottom rails + a mid rail.
	for ry: float in [0.15, h * 0.5, h - 0.15]:
		_box(root, Vector3(0.06, 0.14, depth - 0.1), Vector3(face_x, y0 + ry, 0), mat)
	# Vertical studs.
	var n: int = 5
	for i: int in range(n + 1):
		var z: float = -depth * 0.5 + 0.1 + (depth - 0.2) * float(i) / float(n)
		_box(root, Vector3(0.06, h - 0.2, 0.14), Vector3(face_x, y0 + h * 0.5, z), mat)
	# Diagonal braces in the upper corners (the iconic Tudor herringbone hint).
	if braces:
		for i2: int in range(n):
			var z2: float = -depth * 0.5 + 0.1 + (depth - 0.2) * (float(i2) + 0.5) / float(n)
			var brace: MeshInstance3D = _box(root, Vector3(0.06, 0.9, 0.1), Vector3(face_x, y0 + h * 0.72, z2), mat)
			brace.rotation.x = 0.6 * (1.0 if i2 % 2 == 0 else -1.0)

# Vertical studs + rails on the BACK wall (runs along x, faces -z).
static func _timber_panel_back(root: Node3D, mat: Material, z: float, y0: float, h: float, width: float, braces: bool) -> void:
	var face_z: float = z + 0.05
	for ry: float in [0.15, h * 0.5, h - 0.15]:
		_box(root, Vector3(width - 0.1, 0.14, 0.06), Vector3(0, y0 + ry, face_z), mat)
	var n: int = 6
	for i: int in range(n + 1):
		var x: float = -width * 0.5 + 0.1 + (width - 0.2) * float(i) / float(n)
		_box(root, Vector3(0.14, h - 0.2, 0.06), Vector3(x, y0 + h * 0.5, face_z), mat)
	if braces:
		for i2: int in range(n):
			var x2: float = -width * 0.5 + 0.1 + (width - 0.2) * (float(i2) + 0.5) / float(n)
			var brace: MeshInstance3D = _box(root, Vector3(0.1, 0.9, 0.06), Vector3(x2, y0 + h * 0.72, face_z), mat)
			brace.rotation.z = 0.6 * (1.0 if i2 % 2 == 0 else -1.0)

# --- Leaded windows (with the showpiece projecting oriel bay window) --------
static func _build_windows(root: Node3D) -> void:
	var frame: Material = _toon(C_TIMBER)
	var lead: Material = _metal(Color(0.25, 0.26, 0.28))
	var glass: Material = _glass(C_GLASS)
	var glow: Material = _glow(C_GLOW, 2.4)
	var floor_y0: float = 0.3
	var midy: float = floor_y0 + H1
	var ud: float = D + JET * 2.0

	# SIDE-wall windows (both floors, both sides) — simple leaded casements.
	for s: float in [-1.0, 1.0]:
		var wx: float = s * (W * 0.5 - 0.02)
		_leaded_window_side(root, frame, lead, glass, glow, wx, floor_y0 + 1.5, -1.4)
		_leaded_window_side(root, frame, lead, glass, glow, wx, floor_y0 + 1.5, 1.4)
		# upper side window (slightly jettied out)
		var uwx: float = s * ((W + JET * 2.0) * 0.5 - 0.02)
		_leaded_window_side(root, frame, lead, glass, glow, uwx, midy + 1.5, -1.0)
		_leaded_window_side(root, frame, lead, glass, glow, uwx, midy + 1.5, 1.0)

	# SHOWPIECE: projecting oriel BAY WINDOW on the upper front (jettied storey).
	_build_oriel_bay(root, frame, lead, glass, glow, midy + 0.12 + H2 * 0.5, ud * 0.5)

	# Two flanking ground-floor front windows beside the door.
	for s2: float in [-1.0, 1.0]:
		_leaded_window_front(root, frame, lead, glass, glow, s2 * 2.7, floor_y0 + 1.55, D * 0.5 - 0.02)

# A leaded casement window set INTO a side wall (faces ±x).
static func _leaded_window_side(root: Node3D, frame: Material, lead: Material, glass: Material, glow: Material, x: float, y: float, z: float) -> void:
	var fx: float = x + (0.06 if x < 0.0 else -0.06)
	# Glow plane behind the glass (warm interior light).
	_box(root, Vector3(0.05, 1.2, 0.95), Vector3(fx + (0.04 if x < 0.0 else -0.04), y, z), glow)
	# Glass.
	_box(root, Vector3(0.04, 1.25, 1.0), Vector3(fx, y, z), glass)
	# Oak frame.
	_box(root, Vector3(0.1, 1.45, 0.14), Vector3(fx, y + 0.62, z), frame)
	_box(root, Vector3(0.1, 0.14, 1.2), Vector3(fx, y - 0.65, z), frame)
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.1, 1.45, 0.14), Vector3(fx, y, z + s * 0.55), frame)
	# Diamond leadwork mullions.
	_box(root, Vector3(0.06, 1.2, 0.04), Vector3(fx, y, z), lead)
	_box(root, Vector3(0.06, 0.04, 1.0), Vector3(fx, y, z), lead)
	# A small drip-mould hood (timber) above.
	_box(root, Vector3(0.12, 0.1, 1.3), Vector3(fx, y + 0.78, z), frame)

# A leaded casement set into the FRONT wall (faces +z).
static func _leaded_window_front(root: Node3D, frame: Material, lead: Material, glass: Material, glow: Material, x: float, y: float, z: float) -> void:
	_box(root, Vector3(0.95, 1.2, 0.05), Vector3(x, y, z - 0.06), glow)
	_box(root, Vector3(1.0, 1.25, 0.04), Vector3(x, y, z), glass)
	_box(root, Vector3(1.2, 0.14, 0.1), Vector3(x, y + 0.62, z), frame)
	_box(root, Vector3(1.2, 0.14, 0.1), Vector3(x, y - 0.65, z), frame)
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.14, 1.45, 0.1), Vector3(x + s * 0.55, y, z), frame)
	_box(root, Vector3(0.04, 1.2, 0.06), Vector3(x, y, z), lead)
	_box(root, Vector3(1.0, 0.04, 0.06), Vector3(x, y, z), lead)
	_box(root, Vector3(1.3, 0.1, 0.12), Vector3(x, y + 0.78, z), frame)

# Projecting multi-light ORIEL bay window on the upper front (faceted, leaded).
static func _build_oriel_bay(root: Node3D, frame: Material, lead: Material, glass: Material, glow: Material, cy: float, front_z: float) -> void:
	var gold: Material = _gold(C_GOLD)
	var bw: float = 2.6
	var proj: float = 0.7
	var bz: float = front_z + proj * 0.5
	# Warm glow box inside the bay.
	_box(root, Vector3(bw - 0.3, 1.7, proj), Vector3(0, cy, front_z + proj * 0.35), glow)
	# Three glazed faces: center (faces +z) + two angled cheeks.
	_box(root, Vector3(bw - 0.7, 1.7, 0.05), Vector3(0, cy, front_z + proj), glass)
	for s: float in [-1.0, 1.0]:
		var cheek: MeshInstance3D = _box(root, Vector3(0.85, 1.7, 0.05), Vector3(s * (bw * 0.5 - 0.18), cy, front_z + proj * 0.5), glass)
		cheek.rotation.y = s * 0.7
	# Oak mullion posts (vertical) between lights.
	for mx: float in [-0.8, -0.27, 0.27, 0.8]:
		_box(root, Vector3(0.1, 1.8, 0.1), Vector3(mx, cy, front_z + proj - 0.02), frame)
	# Transom + sill rails.
	_box(root, Vector3(bw, 0.12, proj + 0.1), Vector3(0, cy + 0.88, bz), frame)
	_box(root, Vector3(bw, 0.16, proj + 0.1), Vector3(0, cy - 0.88, bz), frame)
	_box(root, Vector3(bw, 0.1, proj + 0.1), Vector3(0, cy, bz), frame)   # mid transom
	# Gilded sill bead under the oriel (catches the eye from the street).
	_box(root, Vector3(bw + 0.05, 0.05, proj + 0.12), Vector3(0, cy - 0.96, bz), gold)
	# Carved corbel + little lead roof over the oriel.
	_prism(root, Vector3(bw - 0.2, 0.35, proj * 0.7), Vector3(0, cy - 1.05, front_z + proj * 0.4), frame)
	_prism(root, Vector3(bw + 0.2, 0.4, proj + 0.3), Vector3(0, cy + 1.1, front_z + proj * 0.4), lead)
	# Gilded finial crowning the oriel roof.
	_ball(root, 0.09, Vector3(0, cy + 1.35, front_z + proj * 0.4), gold)

# --- Entrance: pointed-arch oak door, gilded columns, lantern, steps --------
static func _build_entrance(root: Node3D) -> void:
	var oak: Material = _gloss(C_TIMBER_HI)
	var stone: Material = _toon(C_PLINTH)
	var marble: Material = _marble(C_MARBLE)
	var brass: Material = _metal(C_BRASS)
	var gold: Material = _gold(C_GOLD)
	var floor_y0: float = 0.3
	var z: float = D * 0.5 - WALL * 0.5

	# Flanking GILDED-CAP MARBLE COLUMNS framing the porch (luxury portico).
	for s_col: float in [-1.0, 1.0]:
		var colx: float = s_col * 1.55
		var colz: float = z + 0.55
		# Square plinth + base.
		_box(root, Vector3(0.5, 0.22, 0.5), Vector3(colx, floor_y0 + 0.11, colz), stone)
		_torus(root, 0.12, 0.24, Vector3(colx, floor_y0 + 0.26, colz), gold)
		# Fluted marble shaft.
		_cyl(root, 0.16, 0.18, 2.7, Vector3(colx, floor_y0 + 1.6, colz), marble)
		# Gilded capital + abacus.
		_torus(root, 0.14, 0.26, Vector3(colx, floor_y0 + 2.95, colz), gold)
		_box(root, Vector3(0.46, 0.16, 0.46), Vector3(colx, floor_y0 + 3.08, colz), marble)
	# Gilded entablature spanning the two columns.
	_box(root, Vector3(3.7, 0.22, 0.46), Vector3(0, floor_y0 + 3.28, z + 0.55), gold)
	_box(root, Vector3(3.9, 0.12, 0.56), Vector3(0, floor_y0 + 3.42, z + 0.55), marble)

	# Stone door surround (jambs + Tudor four-centred arch head).
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.3, 2.5, 0.45), Vector3(s * 0.85, floor_y0 + 1.25, z), stone)
	# Flattened arch head (use a low prism + a lintel box) with a gilded keystone.
	_box(root, Vector3(2.3, 0.35, 0.45), Vector3(0, floor_y0 + 2.55, z), stone)
	_prism(root, Vector3(2.0, 0.45, 0.4), Vector3(0, floor_y0 + 2.85, z), stone)
	_box(root, Vector3(0.28, 0.5, 0.48), Vector3(0, floor_y0 + 2.65, z + 0.02), gold)

	# Studded oak double door (kept thin, slightly inset).
	for s2: float in [-1.0, 1.0]:
		_box(root, Vector3(0.62, 2.2, 0.12), Vector3(s2 * 0.34, floor_y0 + 1.1, z - 0.16), oak)
	# Vertical plank lines + iron strap hinges.
	for px: float in [-0.55, -0.2, 0.2, 0.55]:
		_box(root, Vector3(0.04, 2.1, 0.13), Vector3(px, floor_y0 + 1.1, z - 0.16), _toon(C_TIMBER))
	var iron: Material = _metal(Color(0.10, 0.10, 0.12))
	for hy: float in [floor_y0 + 0.55, floor_y0 + 1.65]:
		_box(root, Vector3(1.25, 0.08, 0.14), Vector3(0, hy, z - 0.16), brass)
	# Brass ring handles + central boss studs (gilded studs for the tier).
	for s3: float in [-1.0, 1.0]:
		_torus(root, 0.04, 0.09, Vector3(s3 * 0.5, floor_y0 + 1.1, z - 0.24), brass)
	for sx: float in [-0.5, 0.0, 0.5]:
		for sy: float in [0.6, 1.1, 1.6]:
			_ball(root, 0.045, Vector3(sx, floor_y0 + sy, z - 0.24), gold)

	# Marble entrance steps with a gilded nosing on the top tread.
	for i: int in range(3):
		var sw: float = 2.6 - float(i) * 0.3
		_box(root, Vector3(sw, 0.16, 0.4), Vector3(0, floor_y0 - 0.08 - float(i) * 0.16, z + 0.45 + float(i) * 0.4), marble)
	_box(root, Vector3(2.6, 0.04, 0.06), Vector3(0, floor_y0 + 0.0, z + 0.27), gold)

	# Hanging lantern over the door (glowing, gilded crown).
	_box(root, Vector3(0.12, 0.5, 0.12), Vector3(0, floor_y0 + 2.95, z - 0.1), iron)
	_box(root, Vector3(0.26, 0.34, 0.26), Vector3(0, floor_y0 + 2.6, z - 0.1), iron)
	_ball(root, 0.1, Vector3(0, floor_y0 + 2.6, z - 0.1), _glow(C_GLOW, 3.8))
	_prism(root, Vector3(0.3, 0.16, 0.3), Vector3(0, floor_y0 + 2.85, z - 0.1), gold)

# --- Wrought-iron + gilt front balcony beneath the oriel bay ---------------
static func _build_balcony(root: Node3D) -> void:
	var iron: Material = _metal(Color(0.10, 0.10, 0.12))
	var gold: Material = _gold(C_GOLD)
	var stone: Material = _toon(C_PLINTH)
	var floor_y0: float = 0.3
	var midy: float = floor_y0 + H1
	var ud: float = D + JET * 2.0
	var by: float = midy + 0.2
	var bz: float = ud * 0.5 + 0.55
	var bw: float = 3.2
	# Cantilevered stone deck slab on two corbels.
	_box(root, Vector3(bw, 0.16, 1.0), Vector3(0, by, bz), stone)
	for s_c: float in [-1.0, 1.0]:
		_prism(root, Vector3(0.22, 0.45, 0.6), Vector3(s_c * (bw * 0.5 - 0.3), by - 0.3, ud * 0.5 + 0.15), stone)
	# Iron balustrade: top + bottom rails with gilded caps.
	_box(root, Vector3(bw, 0.07, 0.07), Vector3(0, by + 0.62, bz + 0.46), iron)
	_box(root, Vector3(bw, 0.05, 0.05), Vector3(0, by + 0.66, bz + 0.46), gold)
	_box(root, Vector3(bw, 0.06, 0.06), Vector3(0, by + 0.12, bz + 0.46), iron)
	# Side rails.
	for s_s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.06, 0.62, 0.95), Vector3(s_s * bw * 0.5, by + 0.37, bz), iron)
	# Turned iron balusters along the front edge with gilded finials.
	var nb: int = 9
	for i: int in range(nb):
		var bx: float = -bw * 0.5 + 0.2 + (bw - 0.4) * float(i) / float(nb - 1)
		_cyl(root, 0.03, 0.03, 0.5, Vector3(bx, by + 0.37, bz + 0.46), iron)
		_ball(root, 0.04, Vector3(bx, by + 0.64, bz + 0.46), gold)

# --- Roof: steep gable + side slopes, ridge, finials ------------------------
static func _build_roof(root: Node3D) -> void:
	var roof: Material = _gloss(C_ROOF)
	var roof_hi: Material = _gloss(C_ROOF_HI)
	var timber: Material = _toon(C_TIMBER)
	var gold: Material = _gold(C_GOLD)
	var floor_y0: float = 0.3
	var uw: float = W + JET * 2.0
	var ud: float = D + JET * 2.0
	var eave_y: float = floor_y0 + H1 + 0.12 + H2

	# Main hipped/gabled roof — long prism ridge running along x.
	var roof_h: float = 2.6
	_prism(root, Vector3(uw + 0.4, roof_h, ud + 0.4), Vector3(0, eave_y + roof_h * 0.5, 0), roof)
	# Subtle shingle banding (rows of thin boxes on both slopes).
	for i: int in range(5):
		var t: float = float(i) / 5.0
		var ry: float = eave_y + roof_h * t + 0.1
		var rdepth: float = (ud + 0.4) * (1.0 - t)
		for s: float in [-1.0, 1.0]:
			var band: MeshInstance3D = _box(root, Vector3(uw + 0.5, 0.06, 0.5), Vector3(0, ry, s * rdepth * 0.25), roof_hi)
			band.rotation.x = s * 0.9

	# STEEP front cross-GABLE (the dominant Tudor silhouette) over the oriel.
	var gw: float = 3.4
	var gable_h: float = 2.9
	var gz: float = ud * 0.5 + 0.1
	# Gable wall (stucco triangle).
	_prism(root, Vector3(gw, gable_h, 0.3), Vector3(0, eave_y + gable_h * 0.5, gz), _toon(C_STUCCO))
	# Decorative gable bargeboards (oak verge timbers along the two slopes).
	for s2: float in [-1.0, 1.0]:
		var bb: MeshInstance3D = _box(root, Vector3(0.14, gable_h * 1.05, 0.16), Vector3(s2 * gw * 0.27, eave_y + gable_h * 0.55, gz + 0.18), timber)
		bb.rotation.z = s2 * 0.86
	# Gilded bead following the bargeboards.
	for s2b: float in [-1.0, 1.0]:
		var gb: MeshInstance3D = _box(root, Vector3(0.05, gable_h * 1.0, 0.05), Vector3(s2b * gw * 0.27, eave_y + gable_h * 0.55, gz + 0.27), gold)
		gb.rotation.z = s2b * 0.86
	# Decorative timber 'V' apex pattern in the gable.
	for s3: float in [-1.0, 1.0]:
		var vb: MeshInstance3D = _box(root, Vector3(0.1, 1.6, 0.1), Vector3(s3 * 0.5, eave_y + gable_h * 0.55, gz + 0.1), timber)
		vb.rotation.z = s3 * 0.5
	_box(root, Vector3(0.1, 1.3, 0.1), Vector3(0, eave_y + gable_h * 0.45, gz + 0.1), timber)
	# Gilded sunburst boss at the gable centre.
	_ball(root, 0.16, Vector3(0, eave_y + gable_h * 0.55, gz + 0.16), gold)
	# Gable roof cap (small prism over the cross-gable, faces +z, ridge along z).
	var cap: MeshInstance3D = _prism(root, Vector3(0.6, gw + 0.4, gable_h + 0.3), Vector3(0, eave_y + 1.0, gz - 1.4), roof)
	cap.rotation.z = PI * 0.5
	# Carved finial / pendant at the gable apex (gilded orb).
	_cyl(root, 0.04, 0.1, 0.55, Vector3(0, eave_y + gable_h + 0.25, gz + 0.1), timber)
	_ball(root, 0.13, Vector3(0, eave_y + gable_h + 0.55, gz + 0.1), gold)

	# Ridge crest tiles with gilded ridge finials at each end.
	_box(root, Vector3(uw + 0.5, 0.18, 0.22), Vector3(0, eave_y + roof_h + 0.02, 0), _gloss(C_CHIMNEY))
	for s_f: float in [-1.0, 1.0]:
		_cyl(root, 0.03, 0.08, 0.4, Vector3(s_f * (uw * 0.5 + 0.15), eave_y + roof_h + 0.22, 0), gold)
		_ball(root, 0.09, Vector3(s_f * (uw * 0.5 + 0.15), eave_y + roof_h + 0.46, 0), gold)

# --- Roof dormers (two leaded-glass dormers on the front slope) -------------
static func _build_dormers(root: Node3D) -> void:
	var roof: Material = _gloss(C_ROOF)
	var stucco: Material = _toon(C_STUCCO)
	var timber: Material = _toon(C_TIMBER)
	var glass: Material = _glass(C_GLASS)
	var glow: Material = _glow(C_GLOW, 2.2)
	var gold: Material = _gold(C_GOLD)
	var floor_y0: float = 0.3
	var ud: float = D + JET * 2.0
	var eave_y: float = floor_y0 + H1 + 0.12 + H2
	var dz: float = ud * 0.5 - 0.35
	var dy: float = eave_y + 0.85
	for s_d: float in [-1.0, 1.0]:
		var dx: float = s_d * 2.45
		# Dormer cheeks / box face.
		_box(root, Vector3(1.0, 1.0, 0.7), Vector3(dx, dy, dz), stucco)
		# Glowing leaded light.
		_box(root, Vector3(0.66, 0.66, 0.05), Vector3(dx, dy + 0.05, dz + 0.36), glow)
		_box(root, Vector3(0.7, 0.7, 0.04), Vector3(dx, dy + 0.05, dz + 0.37), glass)
		_box(root, Vector3(0.06, 0.7, 0.05), Vector3(dx, dy + 0.05, dz + 0.38), timber)
		_box(root, Vector3(0.7, 0.06, 0.05), Vector3(dx, dy + 0.05, dz + 0.38), timber)
		# Little hipped roof + gilded finial.
		_prism(root, Vector3(1.2, 0.7, 0.9), Vector3(dx, dy + 0.85, dz - 0.05), roof)
		_ball(root, 0.07, Vector3(dx, dy + 1.3, dz - 0.05), gold)

# --- Tall ornate Tudor brick chimney stack ---------------------------------
static func _build_chimney(root: Node3D) -> void:
	var brick: Material = _gloss(C_CHIMNEY)
	var stone: Material = _toon(C_STONE)
	var gold: Material = _gold(C_GOLD)
	var floor_y0: float = 0.3
	var ud: float = D + JET * 2.0
	var eave_y: float = floor_y0 + H1 + 0.12 + H2
	var cx: float = (W + JET * 2.0) * 0.5 - 1.2
	var cz: float = -ud * 0.25

	# Base shaft rising from the eave.
	_box(root, Vector3(1.0, 2.2, 1.0), Vector3(cx, eave_y + 1.1, cz), brick)
	# Decorative diagonal-set twin flues (classic Tudor barley-twist hint).
	for s: float in [-1.0, 1.0]:
		var flue: MeshInstance3D = _box(root, Vector3(0.42, 1.6, 0.42), Vector3(cx + s * 0.28, eave_y + 3.0, cz), brick)
		flue.rotation.y = PI * 0.25
	# Stone corbel cap + crown with a gilded band.
	_box(root, Vector3(1.2, 0.22, 1.2), Vector3(cx, eave_y + 3.9, cz), stone)
	_box(root, Vector3(1.26, 0.06, 1.26), Vector3(cx, eave_y + 4.02, cz), gold)
	_box(root, Vector3(1.3, 0.16, 1.3), Vector3(cx, eave_y + 4.05, cz), stone)
	# Two clay chimney pots.
	for s2: float in [-1.0, 1.0]:
		_cyl(root, 0.13, 0.16, 0.5, Vector3(cx + s2 * 0.28, eave_y + 4.35, cz), _gloss(Color(0.62, 0.33, 0.24)))

# ===========================================================================
# INTERIOR — walkable, open, 2 floors + a real staircase + showpieces.
# ===========================================================================
static func _build_interior(root: Node3D) -> void:
	var floor_mat: Material = _gloss(C_FLOOR)
	var plaster: Material = _toon(C_WALL_IN)
	var beam: Material = _toon(C_TIMBER)
	var gold: Material = _gold(C_GOLD)
	var marble: Material = _marble(C_MARBLE)
	var floor_y0: float = 0.3
	var midy: float = floor_y0 + H1

	# ---- Ground floor board floor ----
	_box(root, Vector3(W - WALL * 2.0, 0.1, D - WALL * 2.0), Vector3(0, floor_y0 + 0.05, 0), floor_mat)
	# Plank seams.
	for i: int in range(7):
		var px: float = -W * 0.5 + 1.0 + float(i) * (W - 2.0) / 6.0
		_box(root, Vector3(0.03, 0.11, D - 0.6), Vector3(px, floor_y0 + 0.06, 0), _gloss(Color(0.34, 0.22, 0.13)))
	# Inlaid marble medallion with a gilded ring (luxury entry rug, in stone).
	_cyl(root, 1.1, 1.1, 0.02, Vector3(0, floor_y0 + 0.11, 0.6), marble)
	_torus(root, 0.95, 1.08, Vector3(0, floor_y0 + 0.12, 0.6), gold)
	_torus(root, 0.4, 0.5, Vector3(0, floor_y0 + 0.12, 0.6), gold)

	# ---- Mid floor (ceiling of ground / floor of upper) with exposed joists ----
	var uw: float = W + JET * 2.0
	var ud: float = D + JET * 2.0
	_box(root, Vector3(uw - WALL * 2.0, 0.16, ud - WALL * 2.0), Vector3(0, midy + 0.06, 0), _toon(C_WALL_IN))
	# Exposed oak ceiling joists on the ground floor (premium beamed ceiling).
	for i2: int in range(6):
		var jz: float = -D * 0.5 + 0.8 + float(i2) * (D - 1.6) / 5.0
		_box(root, Vector3(W - 0.4, 0.16, 0.18), Vector3(0, midy - 0.12, jz), beam)
	# Upper-floor board floor over the joists (jettied footprint).
	_box(root, Vector3(uw - WALL * 2.0, 0.08, ud - WALL * 2.0), Vector3(0, midy + 0.16, 0), floor_mat)

	# ---- Top ceiling (under the roof) ----
	var topy: float = midy + 0.12 + H2
	_box(root, Vector3(uw - WALL * 2.0, 0.16, ud - WALL * 2.0), Vector3(0, topy - 0.08, 0), _toon(C_WALL_IN))
	# Exposed ridge collar beams on the upper ceiling.
	for i3: int in range(4):
		var jz3: float = -ud * 0.5 + 1.2 + float(i3) * (ud - 2.4) / 3.0
		_box(root, Vector3(uw - 1.0, 0.18, 0.2), Vector3(0, topy - 0.25, jz3), beam)

	# ---- Partial interior partition (defines a back room / hall) ground floor ----
	# A short partition wall, leaving a wide doorway — keeps the plan OPEN.
	_box(root, Vector3(0.14, H1 - 0.2, 2.4), Vector3(W * 0.5 - 1.8, floor_y0 + (H1 - 0.2) * 0.5, -D * 0.5 + 1.3), plaster)
	# Upper partition.
	_box(root, Vector3(0.14, H2 - 0.2, 2.6), Vector3(uw * 0.5 - 1.9, midy + 0.2 + (H2 - 0.2) * 0.5, -ud * 0.5 + 1.5), plaster)

	# ---- SHOWPIECE 1: Grand carved-oak staircase to the upper floor ----
	_build_staircase(root, floor_y0, midy, beam)

	# ---- SHOWPIECE 2: Inglenook fireplace with brick surround (back wall) ----
	_build_fireplace(root, floor_y0)

	# ---- SHOWPIECE 3: Crystal-and-brass chandelier in the main room ----
	_build_chandelier(root, floor_y0 + H1 - 0.5)

	# ---- A few subtle wall sconces (warm glow) on the interior plaster ----
	var sconce: Material = _metal(Color(0.12, 0.12, 0.14))
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.1, 0.3, 0.1), Vector3(s * (W * 0.5 - 0.3), floor_y0 + 1.9, -D * 0.5 + 0.6), sconce)
		_torus(root, 0.05, 0.1, Vector3(s * (W * 0.5 - 0.35), floor_y0 + 1.85, -D * 0.5 + 0.6), gold)
		_ball(root, 0.09, Vector3(s * (W * 0.5 - 0.4), floor_y0 + 2.0, -D * 0.5 + 0.6), _glow(C_GLOW, 3.0))

# Grand staircase along the -x interior wall, turning up to the mid floor.
static func _build_staircase(root: Node3D, floor_y0: float, midy: float, beam: Material) -> void:
	var tread: Material = _gloss(C_TIMBER_HI)
	var carpet: Material = _gloss(Color(0.45, 0.10, 0.12))   # deep red runner
	var gold: Material = _gold(C_GOLD)
	var steps: int = 11
	var rise: float = (midy + 0.16 - (floor_y0 + 0.1)) / float(steps)
	var run: float = 0.32
	var sx: float = -W * 0.5 + 1.2
	var z0: float = -D * 0.5 + 1.0
	for i: int in range(steps):
		var y: float = floor_y0 + 0.1 + rise * (float(i) + 0.5)
		var z: float = z0 + run * float(i)
		_box(root, Vector3(1.5, rise + 0.04, run + 0.02), Vector3(sx, y, z), tread)
		_box(root, Vector3(0.9, 0.02, run), Vector3(sx, y + rise * 0.5 + 0.02, z), carpet)
		# Gilded stair rods pinning the runner at each tread nosing.
		_cyl(root, 0.02, 0.02, 0.9, Vector3(sx, y + rise * 0.5 + 0.03, z + run * 0.5), gold)
	# Carved newel posts + handrail.
	for i2: int in range(steps + 1):
		if i2 % 3 != 0:
			continue
		var z2: float = z0 + run * float(i2) - run * 0.5
		var y2: float = floor_y0 + 0.1 + rise * float(i2) + 0.5
		_box(root, Vector3(0.08, 1.0, 0.08), Vector3(sx + 0.7, y2, z2), beam)
	# Sloped handrail with a thin gilded cap.
	var rail: MeshInstance3D = _box(root, Vector3(0.1, 0.1, run * float(steps) + 0.3), Vector3(sx + 0.7, floor_y0 + rise * float(steps) * 0.5 + 1.0, z0 + run * float(steps) * 0.5), beam)
	rail.rotation.x = -atan(rise / run)
	var rail_cap: MeshInstance3D = _box(root, Vector3(0.05, 0.04, run * float(steps) + 0.3), Vector3(sx + 0.7, floor_y0 + rise * float(steps) * 0.5 + 1.07, z0 + run * float(steps) * 0.5), gold)
	rail_cap.rotation.x = -atan(rise / run)
	# Newel finials at the foot (gilded orb crown).
	_box(root, Vector3(0.16, 1.2, 0.16), Vector3(sx + 0.7, floor_y0 + 0.7, z0 - 0.2), beam)
	_ball(root, 0.13, Vector3(sx + 0.7, floor_y0 + 1.35, z0 - 0.2), gold)

# Inglenook fireplace built into the back (-z) interior wall.
static func _build_fireplace(root: Node3D, floor_y0: float) -> void:
	var brick: Material = _gloss(C_CHIMNEY)
	var stone: Material = _toon(C_STONE)
	var marble: Material = _marble(C_MARBLE)
	var beam: Material = _toon(C_TIMBER)
	var gold: Material = _gold(C_GOLD)
	var z: float = -D * 0.5 + 0.5
	var cx: float = W * 0.5 - 2.2
	# Brick chimney breast.
	_box(root, Vector3(2.0, H1 - 0.2, 0.5), Vector3(cx, floor_y0 + (H1 - 0.2) * 0.5 + 0.1, z), brick)
	# Polished marble hearth.
	_box(root, Vector3(2.2, 0.18, 0.8), Vector3(cx, floor_y0 + 0.19, z + 0.3), marble)
	# Firebox opening (dark) + warm ember glow.
	_box(root, Vector3(1.2, 1.0, 0.4), Vector3(cx, floor_y0 + 0.75, z + 0.18), _toon(Color(0.05, 0.05, 0.06)))
	_box(root, Vector3(1.0, 0.4, 0.2), Vector3(cx, floor_y0 + 0.5, z + 0.25), _glow(Color(1.0, 0.55, 0.18), 3.5))
	# Massive oak mantel beam with a gilded fascia bead.
	_box(root, Vector3(2.1, 0.28, 0.6), Vector3(cx, floor_y0 + 1.4, z + 0.18), beam)
	_box(root, Vector3(2.12, 0.05, 0.62), Vector3(cx, floor_y0 + 1.56, z + 0.18), gold)
	# Stone jambs.
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.3, 1.3, 0.55), Vector3(cx + s * 0.75, floor_y0 + 0.75, z + 0.1), stone)
	# A gilded framed mirror over the mantel.
	_box(root, Vector3(1.1, 1.0, 0.05), Vector3(cx, floor_y0 + 2.15, z + 0.06), _glass(Color(0.7, 0.78, 0.82)))
	_torus(root, 0.0, 0.04, Vector3(cx, floor_y0 + 2.15, z + 0.06), gold)
	_box(root, Vector3(1.22, 0.08, 0.07), Vector3(cx, floor_y0 + 2.67, z + 0.06), gold)
	_box(root, Vector3(1.22, 0.08, 0.07), Vector3(cx, floor_y0 + 1.63, z + 0.06), gold)
	for s_m: float in [-1.0, 1.0]:
		_box(root, Vector3(0.08, 1.08, 0.07), Vector3(cx + s_m * 0.57, floor_y0 + 2.15, z + 0.06), gold)

# Crystal-and-brass candle chandelier hung from the ground-floor ceiling.
static func _build_chandelier(root: Node3D, y: float) -> void:
	var brass: Material = _metal(C_BRASS)
	var gold: Material = _gold(C_GOLD)
	var crystal: Material = _glass(Color(0.85, 0.9, 0.95))
	var flame: Material = _glow(C_GLOW, 4.0)
	var cx: float = -0.5
	var cz: float = 0.8
	# Chain + gilded hub.
	_cyl(root, 0.02, 0.02, 0.5, Vector3(cx, y + 0.35, cz), brass)
	_ball(root, 0.07, Vector3(cx, y + 0.12, cz), gold)
	_torus(root, 0.32, 0.42, Vector3(cx, y, cz), gold)
	_torus(root, 0.18, 0.26, Vector3(cx, y + 0.18, cz), brass)
	_ball(root, 0.1, Vector3(cx, y - 0.05, cz), gold)
	# Candle arms + flames around the ring, with hanging crystal drops.
	var arms: int = 8
	for i: int in range(arms):
		var a: float = TAU * float(i) / float(arms)
		var ax: float = cx + cos(a) * 0.42
		var az: float = cz + sin(a) * 0.42
		_box(root, Vector3(0.05, 0.18, 0.05), Vector3(ax, y + 0.12, az), _gloss(Color(0.92, 0.9, 0.82)))
		_ball(root, 0.05, Vector3(ax, y + 0.25, az), flame)
		# Crystal pendant drop just inside the arm.
		var dx: float = cx + cos(a) * 0.3
		var dz: float = cz + sin(a) * 0.3
		_ball(root, 0.05, Vector3(dx, y - 0.12, dz), crystal)
	# A bright crystal teardrop at the very bottom.
	_ball(root, 0.09, Vector3(cx, y - 0.22, cz), crystal)

# ===========================================================================
static func meta() -> Dictionary:
	return {
		"id": "tudor_townhouse",
		"name": "Blackthorn Tudor Townhouse",
		"tier": "Villa",
		"rarity": "Rare",
		"description": "A storybook Tudor townhouse elevated to estate grandeur: hand-pegged black oak half-timbering and a jettied upper storey crowned by a steep carved gable, a faceted leaded oriel bay glowing over a gilded balcony, marble portico columns, flanking carved statues, and a tiered marble-and-gold garden fountain. Within, two beamed floors hold a grand carved stair, an inglenook fireplace beneath a gilt mirror, and a crystal-and-brass chandelier.",
		"footprint": [8, 7],
		"floors": 2,
		"attributes": [
			["Style", "English Tudor"],
			["Material", "Oak, Lime Stucco, Marble & Gilt"],
			["Feature", "Oriel Bay, Gilded Balcony & Garden Fountain"],
			["Floors", "2"],
			["Vibe", "Storybook Heritage Luxury"]
		]
	}
