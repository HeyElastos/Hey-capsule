class_name VerseBuildingModernVilla
extends RefCounted
## Hey Verse — premium procedural BUILDING: "Azure Skyline Villa" (Epic).
##
## A sleek modern villa sold as an NFT and placed on a player's land. Flat
## roofs, floor-to-ceiling glass curtain walls (emissive), a cantilevered
## upper floor over a glass entry, a long infinity pool out front, and a
## white + warm-wood + black-steel palette accented with tasteful brushed
## GOLD/BRASS, over an open-plan two-storey interior. The front (+z) wall is
## OMITTED so the follow camera reads the walkable interior, exactly like the
## existing Verse home.
##
## LUXURY pass: brass colonnade + portal columns, a grand floating stair, a
## crystal chandelier, a linear fireplace, a tiered entry fountain, bronze
## sculptures on plinths, glass balconies, roof dormers + lanterns, manicured
## landscaping and uplit facades — a clear Epic-tier silhouette and material
## story, kept clean and fully walkable on the open ground floor.
##
## Self-contained: it loads res://toon.gdshader + res://outline.gdshader by
## path (guarded by ResourceLoader.exists) and falls back to a plain
## StandardMaterial3D so the module parses + runs standalone. All material +
## primitive helpers live here; no preload of other .gd, no external assets.


const TOON_SHADER_PATH := "res://toon.gdshader"
const OUTLINE_SHADER_PATH := "res://outline.gdshader"

# Footprint (Luxury Villa band): a generous 12 x 10 with a wide front pool deck.
const W := 12.0          # x full-extent; W*0.5 = half-extent reference
const D := 10.0          # z depth
const FLOOR_H := 3.1     # clear ceiling per storey
const WALL_T := 0.16     # slab / wall thickness


# ---------------------------------------------------------------------------
# Material helpers — toon look with inverted-hull outline next_pass; graceful
# StandardMaterial3D fallback when the shaders aren't present.
# ---------------------------------------------------------------------------
static var _outline_mat: ShaderMaterial = null


static func _toon(c: Color, rim: float = 0.35, outline: bool = true, spec: float = 0.0) -> Material:
	if ResourceLoader.exists(TOON_SHADER_PATH):
		var sh := ResourceLoader.load(TOON_SHADER_PATH) as Shader
		if sh != null:
			var m := ShaderMaterial.new()
			m.shader = sh
			m.set_shader_parameter("albedo", c)
			m.set_shader_parameter("rim_strength", rim)
			m.set_shader_parameter("spec_strength", spec)
			m.set_shader_parameter("wind_strength", 0.0)
			m.set_shader_parameter("wind_height", 0.5)
			if outline:
				if _outline_mat == null and ResourceLoader.exists(OUTLINE_SHADER_PATH):
					var osh := ResourceLoader.load(OUTLINE_SHADER_PATH) as Shader
					if osh != null:
						_outline_mat = ShaderMaterial.new()
						_outline_mat.shader = osh
				if _outline_mat != null:
					m.next_pass = _outline_mat
			return m
	# Fallback: plain toon-ish standard material.
	var s := StandardMaterial3D.new()
	s.albedo_color = c
	s.roughness = 0.85
	s.specular = clampf(spec, 0.0, 1.0)
	return s


static func _metal(c: Color, rough: float = 0.35) -> Material:
	# Black steel / chrome / brushed gold — a glossy toon read with a touch of
	# spec; under the cel shader the spec dot does the metallic suggestion.
	if ResourceLoader.exists(TOON_SHADER_PATH):
		return _toon(c, 0.5, true, 0.85)
	var s := StandardMaterial3D.new()
	s.albedo_color = c
	s.metallic = 0.9
	s.roughness = rough
	return s


static func _brass(c: Color) -> Material:
	# Polished brass / gold with a faint self-glow so the trim reads "luxe" even
	# in dim interiors. Stays a StandardMaterial3D for the emissive sheen.
	var s := StandardMaterial3D.new()
	s.albedo_color = c
	s.metallic = 0.95
	s.roughness = 0.22
	s.emission_enabled = true
	s.emission = Color(c.r * 0.5, c.g * 0.42, c.b * 0.18)
	s.emission_energy_multiplier = 0.5
	return s


static func _gloss(c: Color) -> Material:
	# Polished marble / lacquer — bright rim, strong spec dot.
	return _toon(c, 0.55, true, 0.7)


static func _glass(tint: Color, energy: float = 1.0) -> StandardMaterial3D:
	# Glass stays a StandardMaterial3D so it can self-illuminate (warm interior
	# glow). Slightly transparent so the interior reads through the curtain wall.
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(tint.r, tint.g, tint.b, 0.34)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_PER_PIXEL
	m.roughness = 0.05
	m.metallic = 0.2
	m.emission_enabled = true
	m.emission = Color(1.0, 0.82, 0.52)
	m.emission_energy_multiplier = energy
	m.cull_mode = BaseMaterial3D.CULL_DISABLED
	return m


static func _glow(c: Color, energy: float = 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


static func _crystal(c: Color, energy: float = 2.0) -> StandardMaterial3D:
	# Faceted crystal — bright translucent emissive for the chandelier drops.
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, 0.55)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_PER_PIXEL
	m.roughness = 0.02
	m.metallic = 0.1
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


# ---------------------------------------------------------------------------
# Primitive helpers — every one returns the MeshInstance3D so callers can tweak.
# ---------------------------------------------------------------------------
static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot_y: float = 0.0) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	mi.rotation.y = rot_y
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, segs: int = 18) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = segs
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi


static func _ball(parent: Node3D, r: float, scale: Vector3, mat: Material, pos: Vector3) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 18
	sm.rings = 10
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = scale
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 24
	tm.ring_segments = 14
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _prism(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot_y: float = 0.0) -> MeshInstance3D:
	var pm := PrismMesh.new()
	pm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	mi.rotation.y = rot_y
	parent.add_child(mi)
	return mi


# A reusable fluted brass-capped column (plinth + shaft + capital). Used for the
# colonnade portal that frames the open front threshold.
static func _column(parent: Node3D, pos: Vector3, height: float, stone: Material, brass: Material) -> void:
	var r := 0.22
	# square base plinth
	_box(parent, Vector3(0.6, 0.2, 0.6), stone, pos + Vector3(0, 0.1, 0))
	# brass base ring
	_torus(parent, 0.18, 0.28, brass, pos + Vector3(0, 0.24, 0), Vector3(PI * 0.5, 0, 0))
	# tapered fluted shaft
	_cyl(parent, r * 0.86, r, height, stone, pos + Vector3(0, 0.24 + height * 0.5, 0), 16)
	# brass neck ring + capital block
	_torus(parent, 0.16, 0.26, brass, pos + Vector3(0, 0.24 + height - 0.06, 0), Vector3(PI * 0.5, 0, 0))
	_box(parent, Vector3(0.6, 0.16, 0.6), stone, pos + Vector3(0, 0.24 + height + 0.06, 0))


# A bronze figure on a marble plinth — abstract standing statue (a luxe accent).
static func _statue(parent: Node3D, pos: Vector3, plinth: Material, bronze: Material, facing: float = 0.0) -> void:
	# plinth
	_box(parent, Vector3(0.7, 0.7, 0.7), plinth, pos + Vector3(0, 0.35, 0))
	_box(parent, Vector3(0.8, 0.1, 0.8), plinth, pos + Vector3(0, 0.7, 0))
	var b := pos + Vector3(0, 0.75, 0)
	# legs / lower body
	_cyl(parent, 0.1, 0.16, 0.6, bronze, b + Vector3(0, 0.3, 0), 12)
	# torso
	_ball(parent, 0.18, Vector3(1.0, 1.4, 0.7), bronze, b + Vector3(0, 0.75, 0))
	# head
	_ball(parent, 0.12, Vector3.ONE, bronze, b + Vector3(0, 1.12, 0))
	# one raised arm (rot around facing)
	var a := Vector3(sin(facing) * 0.18, 0.85, cos(facing) * 0.18)
	_cyl(parent, 0.05, 0.05, 0.5, bronze, b + a + Vector3(0, 0.15, 0), 8)


# ---------------------------------------------------------------------------
# build() — one Node3D: exterior shell + walkable interior, at origin.
# ---------------------------------------------------------------------------
static func build() -> Node3D:
	var root := Node3D.new()
	root.name = "ModernVilla"

	# Palette
	var white := _gloss(Color(0.96, 0.96, 0.95))
	var white_soft := _toon(Color(0.93, 0.93, 0.91), 0.3)
	var stucco := _toon(Color(0.90, 0.90, 0.88), 0.28)
	var wood := _toon(Color(0.56, 0.39, 0.23), 0.22)
	var wood_dark := _toon(Color(0.40, 0.27, 0.15), 0.2)
	var steel := _metal(Color(0.10, 0.11, 0.13), 0.3)
	var steel_soft := _toon(Color(0.16, 0.17, 0.2), 0.3)
	var chrome := _metal(Color(0.78, 0.80, 0.83), 0.2)
	var gold := _brass(Color(0.92, 0.74, 0.34))
	var brass := _brass(Color(0.86, 0.66, 0.30))
	var bronze := _metal(Color(0.46, 0.33, 0.17), 0.35)
	var marble := _gloss(Color(0.92, 0.91, 0.89))
	var marble_grey := _gloss(Color(0.80, 0.81, 0.84))
	var concrete := _toon(Color(0.74, 0.73, 0.70), 0.25)
	var water := _glass(Color(0.30, 0.62, 0.74), 0.0)
	var water_lit := _glow(Color(0.22, 0.55, 0.70), 0.45)
	var greenery := _toon(Color(0.27, 0.45, 0.26), 0.25)
	var greenery2 := _toon(Color(0.22, 0.40, 0.24), 0.3)
	var glass := _glass(Color(0.62, 0.74, 0.78), 1.0)
	var glass_dim := _glass(Color(0.55, 0.66, 0.72), 0.55)

	var hw := W * 0.5
	var hd := D * 0.5

	# ---- Ground slab / plinth: a wide floating platform with a recessed base.
	_box(root, Vector3(W + 3.4, 0.4, D + 3.0), _toon(Color(0.66, 0.66, 0.63), 0.2), Vector3(0, -0.2, 0.4))
	_box(root, Vector3(W + 1.0, 0.3, D + 0.6), concrete, Vector3(0, 0.0, 0))   # main floor pad
	# A thin brass reveal line wrapping the plinth edge — luxe detail.
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(W + 1.04, 0.04, 0.06), gold, Vector3(0, 0.16, s * (hd + 0.3)))
		_box(root, Vector3(0.06, 0.04, D + 0.64), gold, Vector3(s * (hw + 0.5), 0.16, 0))

	# =====================================================================
	# GROUND FLOOR — open plan. Front (+z) wall OMITTED; a low glass parapet.
	# =====================================================================
	_build_ground_floor(root, white, white_soft, wood, wood_dark, steel, chrome, gold, brass, marble, marble_grey, glass, glass_dim, hw, hd)

	# =====================================================================
	# ENTRY COLONNADE — a brass-capped column portal framing the open front.
	# =====================================================================
	_build_colonnade(root, white, marble, gold, brass, hw, hd)

	# =====================================================================
	# UPPER FLOOR — CANTILEVERED over the entry, set back at the rear, all glass.
	# =====================================================================
	_build_upper_floor(root, white, white_soft, wood, steel, steel_soft, chrome, gold, brass, glass, glass_dim, hw, hd)

	# =====================================================================
	# ROOFLINE — twin flat roofs, parapet, pergola, dormers, lanterns, garden.
	# =====================================================================
	_build_roof(root, white, white_soft, steel, steel_soft, wood, greenery, greenery2, gold, brass, glass_dim, hw, hd)

	# =====================================================================
	# POOL + FRONT DECK — long infinity pool, deck, loungers, fire bowl, steps.
	# =====================================================================
	_build_pool_and_deck(root, white, wood, wood_dark, steel, chrome, gold, brass, water, water_lit, marble, concrete, hw, hd)

	# =====================================================================
	# FOUNTAIN + STATUES — a tiered brass fountain on axis + flanking bronzes.
	# =====================================================================
	_build_fountain_and_statues(root, white, marble, marble_grey, gold, brass, bronze, water, water_lit, hw, hd)

	# =====================================================================
	# LANDSCAPING — planters, hedges, path lights, accent trees, entry door.
	# =====================================================================
	_build_landscape(root, white, wood, steel, gold, greenery, greenery2, chrome, glass, hw, hd)

	return root


# ---------------------------------------------------------------------------
static func _build_ground_floor(root: Node3D, white: Material, white_soft: Material, wood: Material, wood_dark: Material, steel: Material, chrome: Material, gold: Material, brass: Material, marble: Material, marble_grey: Material, glass: StandardMaterial3D, glass_dim: StandardMaterial3D, hw: float, hd: float) -> void:
	# Polished marble interior floor (the walkable surface).
	_box(root, Vector3(W - 0.3, 0.1, D - 0.3), marble, Vector3(0, 0.21, 0))
	# A brass-inlaid marble medallion at the centre of the open plan.
	_torus(root, 1.4, 1.7, gold, Vector3(0, 0.27, 0.2), Vector3(PI * 0.5, 0, 0))
	_torus(root, 0.7, 0.86, gold, Vector3(0, 0.27, 0.2), Vector3(PI * 0.5, 0, 0))
	# A wood-inlay rug zone, centre-back, to anchor the lounge.
	_box(root, Vector3(5.4, 0.04, 3.6), wood, Vector3(-1.6, 0.27, -1.6))

	# REAR wall (-z) — solid white with a long window strip.
	_box(root, Vector3(W, FLOOR_H, WALL_T), white, Vector3(0, FLOOR_H * 0.5 + 0.26, -hd))
	# rear feature: backlit slot windows framed in brass
	for i: int in range(-2, 3):
		_box(root, Vector3(0.9, 1.0, WALL_T + 0.04), glass_dim, Vector3(float(i) * 2.0, 1.9, -hd + 0.01))
		_box(root, Vector3(1.0, 0.05, WALL_T + 0.06), gold, Vector3(float(i) * 2.0, 2.42, -hd + 0.005))
		_box(root, Vector3(1.0, 0.05, WALL_T + 0.06), gold, Vector3(float(i) * 2.0, 1.38, -hd + 0.005))

	# LEFT wall (-x) — full glass curtain wall in a black steel grid.
	_glass_wall(root, steel, glass, Vector3(-hw, FLOOR_H * 0.5 + 0.26, 0), Vector3(WALL_T, FLOOR_H, D - 0.4), true, 5)
	# RIGHT wall (+x) — half solid (kitchen) + half glass.
	_box(root, Vector3(WALL_T, FLOOR_H, D * 0.5 - 0.2), white, Vector3(hw, FLOOR_H * 0.5 + 0.26, -D * 0.25 - 0.1))
	_glass_wall(root, steel, glass, Vector3(hw, FLOOR_H * 0.5 + 0.26, D * 0.25 + 0.05), Vector3(WALL_T, FLOOR_H, D * 0.5 - 0.4), true, 2)

	# FRONT (+z): OMITTED wall. Low glass parapet + steel rail so the room reads
	# as open to the camera but still "contained". Brass cap-rail for luxe.
	_box(root, Vector3(W - 0.4, 0.5, WALL_T), white_soft, Vector3(0, 0.5, hd))
	_box(root, Vector3(W - 0.4, 0.05, 0.06), steel, Vector3(0, 0.92, hd))     # top rail
	_box(root, Vector3(W - 0.4, 0.03, 0.08), gold, Vector3(0, 0.95, hd))      # brass cap
	_cyl(root, 0.02, 0.02, 0.42, steel, Vector3(-hw + 0.8, 0.72, hd))
	_cyl(root, 0.02, 0.02, 0.42, steel, Vector3(hw - 0.8, 0.72, hd))
	# A wide open glass slider gap centred — the "indoor/outdoor" threshold.
	# (Left as open air for walking; framed by two steel mullions w/ brass feet.)
	_box(root, Vector3(0.1, FLOOR_H, 0.1), steel, Vector3(-2.2, FLOOR_H * 0.5 + 0.26, hd))
	_box(root, Vector3(0.1, FLOOR_H, 0.1), steel, Vector3(2.2, FLOOR_H * 0.5 + 0.26, hd))
	_torus(root, 0.07, 0.13, gold, Vector3(-2.2, 0.34, hd), Vector3(PI * 0.5, 0, 0))
	_torus(root, 0.07, 0.13, gold, Vector3(2.2, 0.34, hd), Vector3(PI * 0.5, 0, 0))

	# Ground-floor CEILING = the upper-floor slab. Recessed cove lighting strip.
	_box(root, Vector3(W, WALL_T, D), white_soft, Vector3(0, FLOOR_H + 0.26, 0))
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(W - 1.4, 0.06, 0.18), _glow(Color(1.0, 0.86, 0.6), 1.1), Vector3(0, FLOOR_H + 0.12, s * (hd - 1.0)))

	# --- Showpiece: GRAND floating stair (no risers) up the right-rear.
	_build_stair(root, wood, steel, chrome, gold, hw, hd)

	# --- Showpiece: CRYSTAL CHANDELIER hung over the central medallion.
	_build_chandelier(root, gold, brass, Vector3(0, FLOOR_H + 0.1, 0.2))

	# --- Showpiece: linear FIREPLACE set into a stone media wall (rear-left),
	# crowned by a slim brass mantle and flanked by sconces.
	_box(root, Vector3(3.2, FLOOR_H - 0.3, 0.3), marble_grey, Vector3(-hw + 2.2, (FLOOR_H - 0.3) * 0.5 + 0.26, -hd + 0.32))
	_box(root, Vector3(2.0, 0.32, 0.2), _glow(Color(1.0, 0.55, 0.2), 2.2), Vector3(-hw + 2.2, 1.0, -hd + 0.46))
	_box(root, Vector3(2.4, 0.08, 0.16), gold, Vector3(-hw + 2.2, 1.24, -hd + 0.5))    # brass surround
	_box(root, Vector3(2.6, 0.1, 0.5), wood_dark, Vector3(-hw + 2.2, 0.55, -hd + 0.4))   # hearth shelf
	_box(root, Vector3(2.9, 0.1, 0.6), gold, Vector3(-hw + 2.2, 2.4, -hd + 0.42))        # brass mantle
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.08, 0.5, 0.12), gold, Vector3(-hw + 2.2 + s * 1.7, 1.7, -hd + 0.5))
		_box(root, Vector3(0.16, 0.08, 0.2), _glow(Color(1.0, 0.86, 0.6), 1.6), Vector3(-hw + 2.2 + s * 1.7, 1.95, -hd + 0.55))

	# --- Showpiece: kitchen island (right side) — waterfall marble top + gold tap.
	_box(root, Vector3(2.6, 0.92, 1.3), white, Vector3(hw - 2.4, 0.46 + 0.26, -1.4))
	_box(root, Vector3(2.9, 0.12, 1.55), marble_grey, Vector3(hw - 2.4, 0.98 + 0.26, -1.4))  # counter top
	_box(root, Vector3(0.12, 0.92, 1.55), marble_grey, Vector3(hw - 2.4 - 1.45, 0.46 + 0.26, -1.4)) # waterfall leg
	_box(root, Vector3(2.9, 0.03, 1.58), gold, Vector3(hw - 2.4, 1.05 + 0.26, -1.4))        # brass edge band
	_cyl(root, 0.03, 0.03, 0.45, gold, Vector3(hw - 2.4, 1.26 + 0.26, -1.7))
	_torus(root, 0.03, 0.08, gold, Vector3(hw - 2.4, 1.46 + 0.26, -1.62), Vector3(PI * 0.5, 0, 0))
	# Backsplash run + tall pantry against the solid right wall.
	_box(root, Vector3(0.5, 1.0, 3.6), white_soft, Vector3(hw - 0.45, 0.5 + 0.26, -D * 0.25 - 0.1))
	_box(root, Vector3(0.5, 2.4, 1.2), wood, Vector3(hw - 0.45, 1.2 + 0.26, -hd + 1.0))

	# --- Lounge showpiece: a low sofa + brass-and-glass coffee table on the rug.
	_box(root, Vector3(3.0, 0.5, 1.0), white_soft, Vector3(-1.6, 0.5 + 0.26, -2.6))
	_box(root, Vector3(3.0, 0.4, 0.3), white, Vector3(-1.6, 0.7 + 0.26, -3.05))           # backrest
	_box(root, Vector3(1.4, 0.04, 0.8), glass, Vector3(-1.6, 0.5 + 0.26, -1.2))            # glass top
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_cyl(root, 0.02, 0.02, 0.45, gold, Vector3(-1.6 + sx * 0.6, 0.28 + 0.26, -1.2 + sz * 0.32), 8)


static func _glass_wall(parent: Node3D, frame_mat: Material, glass_mat: StandardMaterial3D, center: Vector3, size: Vector3, along_z: bool, bays: int) -> void:
	# A curtain wall: thin glass pane + a black steel mullion grid.
	_box(parent, size, glass_mat, center)
	var span: float = size.z if along_z else size.x
	var step: float = span / float(bays)
	for i: int in range(bays + 1):
		var off: float = -span * 0.5 + float(i) * step
		if along_z:
			_box(parent, Vector3(size.x + 0.04, size.y, 0.08), frame_mat, center + Vector3(0, 0, off))
		else:
			_box(parent, Vector3(0.08, size.y, size.z + 0.04), frame_mat, center + Vector3(off, 0, 0))
	# top + bottom rails
	var rail_x: float = size.x + 0.06 if not along_z else 0.1
	var rail_z: float = size.z + 0.06 if along_z else 0.1
	_box(parent, Vector3(rail_x, 0.1, rail_z), frame_mat, center + Vector3(0, size.y * 0.5, 0))
	_box(parent, Vector3(rail_x, 0.1, rail_z), frame_mat, center + Vector3(0, -size.y * 0.5, 0))


static func _build_stair(root: Node3D, wood: Material, steel: Material, chrome: Material, gold: Material, hw: float, hd: float) -> void:
	# GRAND floating wood treads on a single steel stringer, with a glass
	# balustrade + a brushed-brass handrail, rising along the rear-right toward
	# the cantilever void.
	var base_x: float = hw - 1.6
	var base_z: float = -hd + 1.0
	var n := 11
	for i: int in range(n):
		var t: float = float(i) / float(n - 1)
		var y: float = 0.4 + t * (FLOOR_H - 0.2)
		var z: float = base_z + t * 4.2
		_box(root, Vector3(1.4, 0.12, 0.6), wood, Vector3(base_x, y + 0.26, z))
		_box(root, Vector3(1.42, 0.03, 0.62), gold, Vector3(base_x, y + 0.33, z))     # brass nosing
		# thin steel support under each tread
		_box(root, Vector3(0.08, 0.5, 0.08), steel, Vector3(base_x + 0.5, y + 0.26 - 0.25, z))
	# glass balustrade (outboard side)
	_box(root, Vector3(0.05, 1.0, 4.8), _glass(Color(0.7, 0.8, 0.85), 0.0), Vector3(base_x - 0.6, FLOOR_H * 0.5 + 0.26, base_z + 2.1))
	_cyl(root, 0.03, 0.03, 4.9, gold, Vector3(base_x - 0.6, FLOOR_H * 0.5 + 0.78, base_z + 2.1))   # brass handrail
	# A landing platform at the top.
	_box(root, Vector3(1.8, 0.12, 1.2), wood, Vector3(base_x, FLOOR_H + 0.26, base_z + 4.4))


static func _build_chandelier(root: Node3D, gold: Material, brass: Material, anchor: Vector3) -> void:
	# A two-tier crystal chandelier: brass rings + radial crystal drops + a glow
	# core. Hangs from the ground-floor ceiling over the central medallion.
	var crystal := _crystal(Color(1.0, 0.92, 0.74), 2.2)
	# stem
	_cyl(root, 0.03, 0.03, 0.5, brass, anchor + Vector3(0, -0.25, 0), 8)
	# top + bottom brass rings
	_torus(root, 0.5, 0.58, gold, anchor + Vector3(0, -0.55, 0), Vector3(PI * 0.5, 0, 0))
	_torus(root, 0.32, 0.4, gold, anchor + Vector3(0, -0.85, 0), Vector3(PI * 0.5, 0, 0))
	# glowing core
	_ball(root, 0.16, Vector3.ONE, _glow(Color(1.0, 0.88, 0.62), 2.4), anchor + Vector3(0, -0.72, 0))
	# radial crystal drops on the outer ring
	for i: int in range(8):
		var a: float = float(i) / 8.0 * TAU
		var rx: float = cos(a) * 0.54
		var rz: float = sin(a) * 0.54
		_box(root, Vector3(0.07, 0.34, 0.07), crystal, anchor + Vector3(rx, -0.72, rz))
		_ball(root, 0.05, Vector3.ONE, crystal, anchor + Vector3(rx, -0.92, rz))
	# inner ring drops
	for i: int in range(6):
		var a2: float = float(i) / 6.0 * TAU
		_box(root, Vector3(0.06, 0.24, 0.06), crystal, anchor + Vector3(cos(a2) * 0.36, -0.98, sin(a2) * 0.36))


# ---------------------------------------------------------------------------
static func _build_colonnade(root: Node3D, white: Material, marble: Material, gold: Material, brass: Material, hw: float, hd: float) -> void:
	# A pair of brass-capped marble columns just outside the open front, carrying
	# a slim white lintel — a portal that frames the walkable threshold without
	# blocking it. Adds classical-luxe gravitas to the otherwise minimal facade.
	var col_z := hd + 0.9
	var col_h := FLOOR_H + 0.4
	for s: float in [-1.0, 1.0]:
		_column(root, Vector3(s * (hw - 0.6), 0.3, col_z), col_h, marble, gold)
	# slim lintel spanning the columns, set above head height (clear walk-under).
	_box(root, Vector3(W - 0.6, 0.24, 0.5), white, Vector3(0, 0.3 + col_h + 0.2, col_z))
	_box(root, Vector3(W - 0.5, 0.05, 0.56), gold, Vector3(0, 0.3 + col_h + 0.06, col_z))   # brass reveal
	# small brass pendant lanterns hanging under the lintel.
	for s: float in [-1.0, 1.0]:
		_cyl(root, 0.02, 0.02, 0.25, brass, Vector3(s * 1.6, 0.3 + col_h - 0.05, col_z), 6)
		_box(root, Vector3(0.18, 0.26, 0.18), gold, Vector3(s * 1.6, 0.3 + col_h - 0.32, col_z))
		_box(root, Vector3(0.1, 0.16, 0.1), _glow(Color(1.0, 0.84, 0.5), 1.8), Vector3(s * 1.6, 0.3 + col_h - 0.32, col_z))


# ---------------------------------------------------------------------------
static func _build_upper_floor(root: Node3D, white: Material, white_soft: Material, wood: Material, steel: Material, steel_soft: Material, chrome: Material, gold: Material, brass: Material, glass: StandardMaterial3D, glass_dim: StandardMaterial3D, hw: float, hd: float) -> void:
	var base_y := FLOOR_H + 0.26 + WALL_T   # sits on the ground-floor ceiling slab
	# CANTILEVER: the upper box pushes forward past the front parapet (+z) and is
	# wider than the ground floor, creating the signature floating overhang.
	var up_w := W + 1.2
	var up_d := D - 1.6
	var up_z := 1.2   # shifted toward +z to overhang the entry

	# Underside slab of the cantilever (wood-clad soffit, warm) + brass reveal.
	_box(root, Vector3(up_w, 0.2, up_d), wood, Vector3(0, base_y, up_z))
	_box(root, Vector3(up_w + 0.04, 0.04, 0.06), gold, Vector3(0, base_y - 0.1, up_z + up_d * 0.5))
	# Recessed downlights in the soffit overhang.
	for i: int in range(-2, 3):
		_cyl(root, 0.1, 0.1, 0.04, _glow(Color(1.0, 0.85, 0.55), 1.4), Vector3(float(i) * 2.2, base_y - 0.12, up_z + up_d * 0.5 - 0.4))

	var fy := base_y + FLOOR_H * 0.5 + 0.1

	# Upper interior floor.
	_box(root, Vector3(up_w - 0.4, 0.1, up_d - 0.4), white, Vector3(0, base_y + 0.16, up_z))

	# REAR wall (-z) upper — solid white.
	_box(root, Vector3(up_w, FLOOR_H, WALL_T), white, Vector3(0, fy, up_z - up_d * 0.5))
	# LEFT + RIGHT upper — full glass walls in steel.
	_glass_wall(root, steel, glass, Vector3(-up_w * 0.5, fy, up_z), Vector3(WALL_T, FLOOR_H, up_d - 0.3), true, 4)
	_glass_wall(root, steel, glass, Vector3(up_w * 0.5, fy, up_z), Vector3(WALL_T, FLOOR_H, up_d - 0.3), true, 4)
	# FRONT (+z) upper — the most glass: a panoramic curtain wall with a balcony.
	_glass_wall(root, steel, glass, Vector3(0, fy, up_z + up_d * 0.5), Vector3(up_w - 0.3, FLOOR_H, WALL_T), false, 6)

	# Upper-floor ceiling (the roof slab is added separately; this caps the room).
	_box(root, Vector3(up_w, WALL_T, up_d), white_soft, Vector3(0, base_y + FLOOR_H + 0.1, up_z))

	# --- Master-suite showpiece interior: a low platform bed plinth + headboard.
	_box(root, Vector3(3.2, 0.3, 2.4), wood, Vector3(-up_w * 0.25, base_y + 0.31, up_z - 0.4))
	_box(root, Vector3(3.4, 1.3, 0.2), white_soft, Vector3(-up_w * 0.25, base_y + 0.8, up_z - up_d * 0.5 + 0.3))  # headboard wall
	_box(root, Vector3(3.6, 0.05, 0.24), gold, Vector3(-up_w * 0.25, base_y + 1.42, up_z - up_d * 0.5 + 0.3))    # brass headboard cap
	_box(root, Vector3(3.0, 0.18, 2.0), white, Vector3(-up_w * 0.25, base_y + 0.55, up_z - 0.4))                 # mattress
	# Floating nightstand + a brass pendant.
	_box(root, Vector3(0.6, 0.4, 0.5), wood, Vector3(-up_w * 0.25 - 1.9, base_y + 0.6, up_z - 0.6))
	_cyl(root, 0.0, 0.16, 0.18, gold, Vector3(-up_w * 0.25 - 1.9, base_y + 1.7, up_z - 0.6), 14)
	_ball(root, 0.06, Vector3.ONE, _glow(Color(1.0, 0.86, 0.6), 1.6), Vector3(-up_w * 0.25 - 1.9, base_y + 1.6, up_z - 0.6))
	# A small brass-and-crystal vanity chandelier on the suite ceiling.
	_torus(root, 0.22, 0.28, gold, Vector3(up_w * 0.22, base_y + FLOOR_H - 0.4, up_z - 0.6), Vector3(PI * 0.5, 0, 0))
	_ball(root, 0.1, Vector3.ONE, _glow(Color(1.0, 0.88, 0.62), 2.0), Vector3(up_w * 0.22, base_y + FLOOR_H - 0.55, up_z - 0.6))

	# --- Glass-rail BALCONY cantilevered off the front curtain wall + brass rail.
	var bal_y := base_y + 0.16
	_box(root, Vector3(up_w - 1.0, 0.16, 1.6), wood, Vector3(0, bal_y, up_z + up_d * 0.5 + 0.8))
	_box(root, Vector3(up_w - 1.0, 1.1, 0.05), _glass(Color(0.7, 0.8, 0.85), 0.0), Vector3(0, bal_y + 0.6, up_z + up_d * 0.5 + 1.55))
	_cyl(root, 0.035, 0.035, up_w - 0.9, gold, Vector3(0, bal_y + 1.16, up_z + up_d * 0.5 + 1.55))   # brass top rail (horizontal)
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.05, 1.1, 1.5), _glass(Color(0.7, 0.8, 0.85), 0.0), Vector3(s * (up_w * 0.5 - 0.5), bal_y + 0.6, up_z + up_d * 0.5 + 0.8))

	# --- A SECOND smaller juliet balcony on the RIGHT glass wall (asymmetry/luxe).
	var jb_z := up_z - up_d * 0.25
	_box(root, Vector3(0.9, 0.12, 1.4), wood, Vector3(up_w * 0.5 + 0.45, bal_y, jb_z))
	_box(root, Vector3(0.05, 1.0, 1.4), _glass(Color(0.7, 0.8, 0.85), 0.0), Vector3(up_w * 0.5 + 0.9, bal_y + 0.55, jb_z))
	_cyl(root, 0.03, 0.03, 1.4, gold, Vector3(up_w * 0.5 + 0.9, bal_y + 1.06, jb_z))

	# A slim vertical accent fin running the full two-storey height (gold spine).
	_box(root, Vector3(0.14, FLOOR_H * 2.0 + 0.7, 0.14), gold, Vector3(hw + 0.1, FLOOR_H + 0.3, hd - 0.2))


# ---------------------------------------------------------------------------
static func _build_roof(root: Node3D, white: Material, white_soft: Material, steel: Material, steel_soft: Material, wood: Material, greenery: Material, greenery2: Material, gold: Material, brass: Material, glass_dim: StandardMaterial3D, hw: float, hd: float) -> void:
	var roof_y := (FLOOR_H + 0.26 + WALL_T) + FLOOR_H + 0.1 + WALL_T
	var up_w := W + 1.2
	var up_d := D - 1.6
	var up_z := 1.2

	# Main flat roof slab over the upper floor + a thin parapet upstand.
	_box(root, Vector3(up_w + 0.2, 0.18, up_d + 0.2), white, Vector3(0, roof_y, up_z))
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(up_w + 0.3, 0.4, 0.12), white_soft, Vector3(0, roof_y + 0.2, up_z + s * (up_d * 0.5 + 0.05)))
		_box(root, Vector3(0.12, 0.4, up_d + 0.3), white_soft, Vector3(s * (up_w * 0.5 + 0.05), roof_y + 0.2, up_z))
	# Brass coping line capping the parapet (luxe silhouette edge).
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(up_w + 0.34, 0.04, 0.06), gold, Vector3(0, roof_y + 0.4, up_z + s * (up_d * 0.5 + 0.05)))
		_box(root, Vector3(0.06, 0.04, up_d + 0.34), gold, Vector3(s * (up_w * 0.5 + 0.05), roof_y + 0.4, up_z))

	# A SECOND lower flat roof over the rear single-storey wing (steps the mass).
	_box(root, Vector3(W * 0.6, 0.16, 3.4), white, Vector3(-hw + W * 0.3, FLOOR_H + 0.26 + 0.05, -hd + 1.7))

	# DORMERS: three slim glass-faced clerestory dormers popping the rear roof
	# plane (skylight massing + a richer silhouette).
	for i: int in range(3):
		var dx: float = -up_w * 0.25 + float(i) * (up_w * 0.25)
		_prism(root, Vector3(1.2, 0.5, 0.8), white_soft, Vector3(dx, roof_y + 0.34, up_z - up_d * 0.5 + 0.6))
		_box(root, Vector3(0.9, 0.34, 0.05), glass_dim, Vector3(dx, roof_y + 0.42, up_z - up_d * 0.5 + 0.2))
		_box(root, Vector3(0.96, 0.04, 0.06), gold, Vector3(dx, roof_y + 0.6, up_z - up_d * 0.5 + 0.19))

	# ROOFTOP terrace deck (wood) + thin steel pergola — the silhouette topper.
	var ter_y := roof_y + 0.12
	_box(root, Vector3(up_w - 1.6, 0.1, up_d - 1.4), wood, Vector3(0.6, ter_y, up_z))
	# Pergola: 4 slim posts + a slatted steel roof.
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_box(root, Vector3(0.1, 2.0, 0.1), steel, Vector3(0.6 + sx * (up_w * 0.5 - 1.6), ter_y + 1.0, up_z + sz * (up_d * 0.5 - 1.4)))
	for i: int in range(9):
		var px: float = -(up_w * 0.5 - 1.6) + 0.6 + float(i) * ((up_w - 3.2) / 8.0)
		_box(root, Vector3(0.06, 0.1, up_d - 2.8), steel_soft, Vector3(px, ter_y + 2.05, up_z))
	# Brass finial caps on the pergola posts.
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_ball(root, 0.08, Vector3.ONE, gold, Vector3(0.6 + sx * (up_w * 0.5 - 1.6), ter_y + 2.06, up_z + sz * (up_d * 0.5 - 1.4)))
	# Rooftop garden planters with hedges.
	for i: int in range(3):
		var pz: float = up_z - (up_d * 0.5 - 1.8) + float(i) * (up_d - 3.6) * 0.5
		_box(root, Vector3(0.7, 0.4, 1.2), white_soft, Vector3(-(up_w * 0.5 - 1.0), ter_y + 0.2, pz))
		_ball(root, 0.45, Vector3(1.4, 0.8, 1.0), greenery, Vector3(-(up_w * 0.5 - 1.0), ter_y + 0.7, pz))
		_box(root, Vector3(0.7, 0.4, 1.2), white_soft, Vector3((up_w * 0.5 - 1.0), ter_y + 0.2, pz))
		_ball(root, 0.45, Vector3(1.4, 0.8, 1.0), greenery2, Vector3((up_w * 0.5 - 1.0), ter_y + 0.7, pz))

	# Rooftop glow-edge light strip (cool architectural uplight) + AC/skylight box.
	_box(root, Vector3(up_w - 0.4, 0.05, 0.12), _glow(Color(0.7, 0.85, 1.0), 1.0), Vector3(0, roof_y + 0.42, up_z - up_d * 0.5 + 0.1))
	_box(root, Vector3(1.6, 0.25, 1.6), steel_soft, Vector3(-up_w * 0.5 + 1.4, roof_y + 0.22, up_z - up_d * 0.5 + 1.3))
	# Flush skylight over the stair void (warm glow leaking up), brass-framed.
	_box(root, Vector3(2.0, 0.06, 2.0), glass_dim, Vector3(hw - 2.0, roof_y + 0.04, -hd + 3.0))
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(2.1, 0.05, 0.06), gold, Vector3(hw - 2.0, roof_y + 0.08, -hd + 3.0 + s))
		_box(root, Vector3(0.06, 0.05, 2.1), gold, Vector3(hw - 2.0 + s, roof_y + 0.08, -hd + 3.0))
	# Crowning gold cap on the vertical spine + a beacon glow.
	_box(root, Vector3(0.24, 0.24, 0.24), gold, Vector3(hw + 0.1, roof_y + 0.4, hd - 0.2))
	_ball(root, 0.09, Vector3.ONE, _glow(Color(1.0, 0.86, 0.5), 2.0), Vector3(hw + 0.1, roof_y + 0.6, hd - 0.2))


# ---------------------------------------------------------------------------
static func _build_pool_and_deck(root: Node3D, white: Material, wood: Material, wood_dark: Material, steel: Material, chrome: Material, gold: Material, brass: Material, water: StandardMaterial3D, water_lit: StandardMaterial3D, marble: Material, concrete: Material, hw: float, hd: float) -> void:
	var deck_z := hd + 4.6
	# Front deck — large travertine pad in front of the villa.
	_box(root, Vector3(W + 3.0, 0.16, 8.0), concrete, Vector3(0, 0.18, hd + 4.0))
	# Wood entry boardwalk leading to the omitted-front threshold, brass-banded.
	_box(root, Vector3(3.4, 0.18, 4.2), wood, Vector3(0, 0.2, hd + 2.1))
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.06, 0.04, 4.2), gold, Vector3(s * 1.7, 0.3, hd + 2.1))

	# INFINITY POOL — long rectangular basin to the left of the entry walk.
	var pool_x := -3.2
	var pool_z := deck_z
	# basin shell (sunken)
	_box(root, Vector3(5.0, 0.6, 6.2), marble, Vector3(pool_x, -0.1, pool_z))
	# water surface (slightly transparent, faint glow at dusk)
	_box(root, Vector3(4.6, 0.1, 5.8), water, Vector3(pool_x, 0.16, pool_z))
	_box(root, Vector3(4.4, 0.02, 5.6), water_lit, Vector3(pool_x, 0.10, pool_z))
	# coping lip (white stone) around the rim + a brass waterline trim
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(5.0, 0.1, 0.3), white, Vector3(pool_x, 0.2, pool_z + s * 3.1))
		_box(root, Vector3(0.3, 0.1, 6.2), white, Vector3(pool_x + s * 2.5, 0.2, pool_z))
		_box(root, Vector3(4.6, 0.02, 0.05), gold, Vector3(pool_x, 0.22, pool_z + s * 2.95))
	# infinity edge: a thin slot weir on the far (+z) side with a glow line
	_box(root, Vector3(4.6, 0.04, 0.1), _glow(Color(0.6, 0.85, 1.0), 0.8), Vector3(pool_x, 0.22, pool_z + 3.05))
	# stepping stones across the pool
	for i: int in range(3):
		_box(root, Vector3(0.7, 0.06, 0.7), white, Vector3(pool_x, 0.22, pool_z - 2.0 + float(i) * 2.0))

	# Sun loungers (two) on the deck, +x of the walk.
	for i: int in range(2):
		var lx: float = 3.4 + float(i) * 1.4
		_box(root, Vector3(0.7, 0.18, 1.9), wood, Vector3(lx, 0.4, deck_z))
		_box(root, Vector3(0.7, 0.14, 0.8), wood, Vector3(lx, 0.56, deck_z - 1.0), 0.0)
		_box(root, Vector3(0.66, 0.1, 1.8), white, Vector3(lx, 0.5, deck_z))   # cushion
		# slim chrome legs
		for sx: float in [-1.0, 1.0]:
			_cyl(root, 0.025, 0.025, 0.3, chrome, Vector3(lx + sx * 0.3, 0.15, deck_z - 0.7), 8)

	# A parasol with a brass pole between the loungers.
	_cyl(root, 0.03, 0.03, 2.2, brass, Vector3(4.8, 1.3, deck_z + 0.6), 8)
	_cyl(root, 0.0, 1.1, 0.5, white, Vector3(4.8, 2.45, deck_z + 0.6), 12)

	# Fire bowl feature between pool and walk.
	_cyl(root, 0.5, 0.55, 0.4, concrete, Vector3(pool_x + 3.0, 0.4, hd + 1.6), 16)
	_torus(root, 0.46, 0.56, gold, Vector3(pool_x + 3.0, 0.6, hd + 1.6), Vector3(PI * 0.5, 0, 0))   # brass rim
	_cyl(root, 0.42, 0.42, 0.1, _glow(Color(1.0, 0.5, 0.18), 2.4), Vector3(pool_x + 3.0, 0.62, hd + 1.6), 16)

	# Entry STEPS up to the threshold (wide, shallow, white stone) + brass nosing.
	for i: int in range(3):
		var sw: float = 4.0 - float(i) * 0.3
		_box(root, Vector3(sw, 0.12, 0.5), white, Vector3(0, 0.42 - float(i) * 0.12, hd + 0.4 + float(i) * 0.5))
		_box(root, Vector3(sw, 0.03, 0.06), gold, Vector3(0, 0.48 - float(i) * 0.12, hd + 0.65 + float(i) * 0.5))

	# Floating house numbers / brand plaque in gold by the entry.
	_box(root, Vector3(0.06, 0.6, 0.4), gold, Vector3(2.4, 0.9, hd + 0.1))


# ---------------------------------------------------------------------------
static func _build_fountain_and_statues(root: Node3D, white: Material, marble: Material, marble_grey: Material, gold: Material, brass: Material, bronze: Material, water: StandardMaterial3D, water_lit: StandardMaterial3D, hw: float, hd: float) -> void:
	# A tiered circular fountain on the entry axis, well in front of the steps so
	# it never blocks the walk-in. Marble basin + brass tiers + a glowing jet.
	var fz := hd + 7.2
	# lower basin ring
	_cyl(root, 1.5, 1.6, 0.4, marble, Vector3(0, 0.4, fz), 24)
	_torus(root, 1.4, 1.55, gold, Vector3(0, 0.6, fz), Vector3(PI * 0.5, 0, 0))      # brass rim
	_cyl(root, 1.35, 1.35, 0.06, water, Vector3(0, 0.56, fz), 24)                    # lower water
	_cyl(root, 1.3, 1.3, 0.02, water_lit, Vector3(0, 0.54, fz), 24)
	# pedestal + upper bowl
	_cyl(root, 0.18, 0.26, 0.7, marble_grey, Vector3(0, 0.95, fz), 16)
	_cyl(root, 0.7, 0.55, 0.22, marble, Vector3(0, 1.35, fz), 20)
	_torus(root, 0.62, 0.72, gold, Vector3(0, 1.46, fz), Vector3(PI * 0.5, 0, 0))    # upper brass rim
	_cyl(root, 0.55, 0.55, 0.04, water_lit, Vector3(0, 1.45, fz), 20)                # upper water
	# central jet (glowing column of "water")
	_cyl(root, 0.05, 0.12, 0.9, _glow(Color(0.7, 0.9, 1.0), 1.6), Vector3(0, 1.95, fz), 10)
	_ball(root, 0.16, Vector3(1.0, 0.7, 1.0), _glow(Color(0.8, 0.94, 1.0), 1.4), Vector3(0, 2.4, fz))

	# Flanking BRONZE statues on marble plinths either side of the fountain.
	_statue(root, Vector3(-3.0, 0.18, fz), marble, bronze, PI)
	_statue(root, Vector3(3.0, 0.18, fz), marble, bronze, 0.0)

	# A pair of brass urn ornaments at the foot of the entry steps.
	for s: float in [-1.0, 1.0]:
		var ux: float = s * 2.2
		_cyl(root, 0.22, 0.14, 0.5, gold, Vector3(ux, 0.45, hd + 0.2), 16)
		_torus(root, 0.16, 0.24, brass, Vector3(ux, 0.7, hd + 0.2), Vector3(PI * 0.5, 0, 0))
		_ball(root, 0.16, Vector3(1.0, 0.5, 1.0), _toon(Color(0.27, 0.45, 0.26), 0.25), Vector3(ux, 0.78, hd + 0.2))


# ---------------------------------------------------------------------------
static func _build_landscape(root: Node3D, white: Material, wood: Material, steel: Material, gold: Material, greenery: Material, greenery2: Material, chrome: Material, glass: StandardMaterial3D, hw: float, hd: float) -> void:
	# Pivot/sliding GLASS DOOR leaf parked open at the entry threshold (steel frame).
	_box(root, Vector3(1.1, 2.2, 0.06), glass, Vector3(2.6, 1.36, hd + 0.05))
	_box(root, Vector3(1.16, 2.26, 0.08), steel, Vector3(2.6, 1.36, hd + 0.02))
	_box(root, Vector3(1.1, 2.2, 0.07), glass, Vector3(2.6, 1.36, hd + 0.06))
	_cyl(root, 0.025, 0.025, 0.5, gold, Vector3(3.05, 1.36, hd + 0.1), 8)   # brass vertical pull handle

	# Hedge run along the rear and left property edge (toon, calm green).
	for i: int in range(7):
		var hz: float = -hd - 0.5 + float(i) * (D / 6.0)
		_box(root, Vector3(0.7, 0.9, 1.4), greenery, Vector3(-hw - 1.6, 0.55, hz))
	for i: int in range(8):
		var hx2: float = -hw - 1.2 + float(i) * (W / 7.0)
		_box(root, Vector3(1.4, 0.9, 0.7), greenery2, Vector3(hx2, 0.55, -hd - 1.4))

	# Two sculptural accent trees in white planters flanking the entry walk.
	for s: float in [-1.0, 1.0]:
		var tx: float = s * 2.6
		var tz: float = hd + 6.4
		_box(root, Vector3(1.0, 0.5, 1.0), white, Vector3(tx, 0.4, tz))
		_box(root, Vector3(1.04, 0.04, 1.04), gold, Vector3(tx, 0.66, tz))   # brass planter band
		_cyl(root, 0.12, 0.16, 1.8, wood, Vector3(tx, 1.3, tz), 10)
		_ball(root, 0.8, Vector3(1.0, 0.9, 1.0), greenery, Vector3(tx, 2.4, tz))
		_ball(root, 0.55, Vector3(1.0, 0.9, 1.0), greenery2, Vector3(tx + s * 0.3, 2.9, tz - 0.2))

	# Path bollard lights along the boardwalk (brass posts, warm glow caps).
	for i: int in range(4):
		var bz: float = hd + 1.0 + float(i) * 1.6
		for s2: float in [-1.0, 1.0]:
			_cyl(root, 0.045, 0.05, 0.5, gold, Vector3(s2 * 1.9, 0.45, bz), 8)
			_box(root, Vector3(0.14, 0.08, 0.14), _glow(Color(1.0, 0.84, 0.5), 1.6), Vector3(s2 * 1.9, 0.72, bz))

	# Low planter wall + integrated bench along the +x deck edge.
	_box(root, Vector3(0.4, 0.6, 6.0), white, Vector3(hw + 2.4, 0.5, hd + 4.0))
	_box(root, Vector3(0.5, 0.12, 6.0), wood, Vector3(hw + 2.0, 0.62, hd + 4.0))
	_box(root, Vector3(0.44, 0.03, 6.0), gold, Vector3(hw + 2.4, 0.8, hd + 4.0))   # brass cap line
	for i: int in range(4):
		_ball(root, 0.3, Vector3(1.3, 0.8, 1.0), greenery, Vector3(hw + 2.4, 0.85, hd + 1.5 + float(i) * 1.6))

	# A pair of slim uplight strips washing the white facade (architectural).
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.3, 0.05, 0.3), _glow(Color(0.8, 0.88, 1.0), 1.2), Vector3(s * (hw - 1.0), 0.25, hd + 0.6))


# ---------------------------------------------------------------------------
static func meta() -> Dictionary:
	return {
		"id": "modern_villa",
		"name": "Azure Skyline Villa",
		"tier": "Luxury Villa",
		"rarity": "Epic",
		"description": "A sleek two-storey modern villa with floor-to-ceiling glass, a dramatic cantilevered upper suite floating over a glowing entry, a brass colonnade portal, a tiered fountain flanked by bronze statues, and a long infinity pool — white stucco, warm wood, brushed brass and black steel under an open-plan, fully walkable interior crowned by a crystal chandelier and grand floating stair.",
		"footprint": [12, 10],
		"floors": 2,
		"attributes": [
			["Style", "Modern Minimalist Luxe"],
			["Material", "Glass, White Stucco, Wood, Brushed Brass & Black Steel"],
			["Feature", "Cantilever, Infinity Pool, Fountain, Colonnade, Rooftop Pergola"],
			["Showpiece", "Crystal Chandelier, Grand Floating Stair, Linear Fireplace"],
			["Floors", "2"],
			["Vibe", "Calm Sovereign Luxury"],
		],
	}
