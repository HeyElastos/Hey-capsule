class_name VerseBuildingStoneCastle
extends RefCounted
## Hey Verse — PREMIUM placeable BUILDING: "Ravenspire Keep", a Legendary STONE
## CASTLE sold as an NFT and dropped on a player's land. This is the top trophy in
## the catalogue and is dressed to read instantly as Legendary at any distance.
##
## A real fortress silhouette: a crenellated curtain wall on a moulded plinth, four
## round corner towers with conical slate roofs, gold finials + swallow-tail
## pennants, a twin-bastion gatehouse with a pointed arch, gold portcullis and a
## draped banner, arrow-slit windows that glow at dusk, statue-topped wing turrets,
## projecting oriel balconies, brass-trimmed dormers, and — inside — a walkable
## great hall on a clear ground floor with a colonnade, hanging banners, a fire-lit
## hearth, a CRYSTAL CHANDELIER, a GRAND DOUBLE STAIR to the battlement gallery, and
## a raised GOLD THRONE DAIS at the back. ~3 levels (hall floor, gallery / battlement
## walk, tower tops).
##
## SCALE: built at the ORIGIN, ground at y=0, entrance facing +z. Palace footprint
## (~24 x 19). Doors ~2.2 tall, ceilings ~3.5, arrow-slits at eye height. The FRONT
## WALL is OMITTED (the camera looks in from +z) — a low parapet + the gatehouse
## arch frame the opening so the owner can walk straight in and furnish the hall.
##
## STANDALONE: re-declares its own tiny toon/metal/gloss/glass/glow material set
## and _box/_cyl/_ball/_torus/_prism primitive helpers, and loads the shared
## shaders BY PATH with ResourceLoader.exists() guards + a StandardMaterial3D
## fallback, so the module parses + runs with NO dependency on home.gd / avatar.gd.

const TOON_PATH := "res://toon.gdshader"
const OUTLINE_PATH := "res://outline.gdshader"

# Cached shared passes (resolved once via the guarded loaders below).
static var _outline_mat: ShaderMaterial
static var _toon_shader: Shader
static var _shaders_tried := false

# Typed mirror-pair so `for s in SIDES` yields a `float` (strict-GDScript safe).
const SIDES: Array[float] = [-1.0, 1.0]

# ── premium stone-castle palette ──
const STONE := Color(0.62, 0.63, 0.66)        # cool grey ashlar
const STONE_DK := Color(0.46, 0.47, 0.51)     # shadowed / lower courses
const STONE_LT := Color(0.74, 0.75, 0.77)     # sun-bleached merlon caps
const MARBLE := Color(0.86, 0.86, 0.88)       # polished pale stair / statues
const MORTAR := Color(0.40, 0.41, 0.45)
const SLATE := Color(0.27, 0.30, 0.38)        # conical tower roofs
const SLATE_LT := Color(0.36, 0.40, 0.50)
const WOOD := Color(0.34, 0.22, 0.13)         # gate timbers / beams
const WOOD_DK := Color(0.24, 0.15, 0.09)
const GOLD := Color(1.00, 0.80, 0.30)
const GOLD_DK := Color(0.78, 0.58, 0.20)
const BRASS := Color(0.83, 0.66, 0.28)
const IRON := Color(0.20, 0.21, 0.24)         # portcullis / studs
const BANNER_RED := Color(0.66, 0.12, 0.16)
const BANNER_BLUE := Color(0.14, 0.22, 0.52)
const VELVET := Color(0.42, 0.08, 0.12)       # throne cushion
const FIRE := Color(1.00, 0.55, 0.16)
const WINDOW_GLOW := Color(1.00, 0.80, 0.45)
const CRYSTAL := Color(0.80, 0.90, 1.00)      # chandelier crystals
const WATER := Color(0.55, 0.78, 0.95)


# ───────────────────────────── shader plumbing ─────────────────────────────

## Resolve the shared toon shader + outline pass ONCE, guarded so the module is
## safe to parse/run even if the shaders are missing (fallback = plain material).
static func _ensure_shaders() -> void:
	if _shaders_tried:
		return
	_shaders_tried = true
	if ResourceLoader.exists(TOON_PATH):
		var s: Resource = load(TOON_PATH)
		if s is Shader:
			_toon_shader = s
	if ResourceLoader.exists(OUTLINE_PATH):
		var o: Resource = load(OUTLINE_PATH)
		if o is Shader:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = o


# ───────────────────────────── material helpers ────────────────────────────

## The cel material every matte surface uses (toon ramp + inverted-hull outline).
## Falls back to a toon-shaded StandardMaterial3D if the shader is unavailable.
static func _toon(c: Color, rim: float = 0.30, outline: bool = true, spec: float = 0.0) -> Material:
	_ensure_shaders()
	if _toon_shader != null:
		var m := ShaderMaterial.new()
		m.shader = _toon_shader
		m.set_shader_parameter("albedo", c)
		m.set_shader_parameter("rim_strength", rim)
		m.set_shader_parameter("spec_strength", spec)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		if outline and _outline_mat != null:
			m.next_pass = _outline_mat
		return m
	# fallback — still toon-diffuse, no shader needed
	var sm := StandardMaterial3D.new()
	sm.albedo_color = c
	sm.roughness = 1.0
	sm.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	sm.specular_mode = BaseMaterial3D.SPECULAR_DISABLED
	if outline and _outline_mat != null:
		sm.next_pass = _outline_mat
	return sm


## A real metal — gold / brass / iron. PBR so it glints; outline keeps it on-style.
static func _metal(c: Color, rough: float = 0.30, metallic: float = 1.0) -> StandardMaterial3D:
	_ensure_shaders()
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.metallic_specular = 0.75
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	if _outline_mat != null:
		m.next_pass = _outline_mat
	return m


## Glossy dielectric — lacquered timber, banner-pole caps, polished marble.
static func _gloss(c: Color, rough: float = 0.20) -> StandardMaterial3D:
	_ensure_shaders()
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.0
	m.roughness = rough
	m.metallic_specular = 0.85
	if _outline_mat != null:
		m.next_pass = _outline_mat
	return m


## Translucent glass / lantern shell / crystal (no outline — it would muddy it).
static func _glass(c: Color, alpha: float = 0.42) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.metallic = 0.1
	m.roughness = 0.06
	m.metallic_specular = 0.9
	return m


## Unshaded glowing material — arrow-slit glow, hearth fire, lanterns, gold finials.
static func _glow(c: Color, energy: float = 1.6) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


# ───────────────────────────── primitive helpers ───────────────────────────

static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO, seg: int = 18) -> MeshInstance3D:
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


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, s: Vector3 = Vector3.ONE, seg: int = 18, rings: int = 9) -> MeshInstance3D:
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


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO, seg: int = 12) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 20
	tm.ring_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _prism(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO) -> MeshInstance3D:
	var pm := PrismMesh.new()
	pm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


## A point of warm light (hearth / lantern). Cheap OmniLight; kept low-range so a
## yard full of these castles doesn't drown the scene.
static func _lamp(parent: Node3D, c: Color, energy: float, rng: float, pos: Vector3) -> OmniLight3D:
	var l := OmniLight3D.new()
	l.light_color = c
	l.light_energy = energy
	l.omni_range = rng
	l.shadow_enabled = false
	l.position = pos
	parent.add_child(l)
	return l


# ══════════════════════════════════ BUILD ══════════════════════════════════

static func build() -> Node3D:
	_shaders_tried = false   # re-resolve per build so a fresh load picks shaders up
	_ensure_shaders()
	var root := Node3D.new()
	root.name = "RavenspireKeep"

	# Footprint (palace tier). Outer curtain wall spans HALF_W x HALF_D; corner
	# towers sit just outside the corners. +z is the open / entrance face.
	var hw := 11.0    # half width  (x)
	var hd := 9.0     # half depth  (z)
	var wall_h := 5.2
	var wall_t := 0.8

	_ground(root, hw, hd)
	_curtain_walls(root, hw, hd, wall_h, wall_t)
	_oriel_balconies(root, hw, hd, wall_h)
	_corner_towers(root, hw, hd, wall_h)
	_wing_turrets(root, hw, hd, wall_h)
	_gatehouse(root, hw, hd, wall_h, wall_t)
	_great_hall_interior(root, hw, hd, wall_h, wall_t)
	_chandelier(root, hw, hd)
	_throne_dais(root, hw, hd)
	_grand_stair(root, hw, hd, wall_h)
	_landscaping(root, hw, hd)

	return root


# ─────────────────────────────── foundation ────────────────────────────────

## A moulded stone plinth the whole keep sits on + the hall floor flagstones, so
## the fortress reads as planted on bedrock, not floating. A gold-edged red runner
## leads the eye from the gate straight to the dais.
static func _ground(root: Node3D, hw: float, hd: float) -> void:
	var base := Node3D.new()
	base.name = "Plinth"
	root.add_child(base)
	# stepped, moulded plinth (three courses) under the footprint
	_box(base, Vector3(hw * 2.0 + 3.2, 0.5, hd * 2.0 + 3.2), _toon(STONE_DK), Vector3(0, -0.25, 0))
	_box(base, Vector3(hw * 2.0 + 2.2, 0.4, hd * 2.0 + 2.2), _toon(STONE), Vector3(0, 0.1, 0))
	# chamfered cap course (a gold pinstripe reveal between courses for richness)
	_box(base, Vector3(hw * 2.0 + 1.5, 0.08, hd * 2.0 + 1.5), _metal(GOLD_DK, 0.5), Vector3(0, 0.33, 0))
	_box(base, Vector3(hw * 2.0 + 1.2, 0.28, hd * 2.0 + 1.2), _toon(STONE_LT), Vector3(0, 0.45, 0))
	# interior flagstone floor (slight warm grey), kept CLEAR for the owner
	var floor_mat := _toon(Color(0.56, 0.56, 0.58))
	_box(base, Vector3(hw * 2.0 - 1.0, 0.2, hd * 2.0 - 1.0), floor_mat, Vector3(0, 0.55, 0))
	# inlaid marble compass medallion in the floor centre — a luxury detail
	_cyl(base, 2.0, 2.0, 0.06, _toon(MARBLE, 0.18), Vector3(0, 0.66, 1.0), Vector3.ZERO, 28)
	_cyl(base, 1.4, 1.4, 0.07, _metal(BRASS, 0.4), Vector3(0, 0.665, 1.0), Vector3.ZERO, 28)
	for i: int in range(8):
		var a := TAU * float(i) / 8.0
		_box(base, Vector3(0.16, 0.08, 1.7), _metal(GOLD, 0.35), Vector3(sin(a) * 0.0, 0.67, 1.0), Vector3(0, a, 0))
	# a runner of red carpet from the gate to the dais
	_box(base, Vector3(2.6, 0.04, hd * 2.0 - 1.6), _toon(BANNER_RED, 0.22), Vector3(0, 0.67, 0.0))
	# carpet gold edge trim
	for s in SIDES:
		_box(base, Vector3(0.16, 0.05, hd * 2.0 - 1.6), _metal(GOLD, 0.4), Vector3(s * 1.38, 0.675, 0.0))


# ─────────────────────────────── curtain wall ──────────────────────────────

## The three solid curtain walls (back + two sides) with a crenellated parapet
## and a wall-walk. The FRONT (+z) is left open save for a low threshold parapet
## and the gatehouse, so the camera sees into the hall.
static func _curtain_walls(root: Node3D, hw: float, hd: float, h: float, t: float) -> void:
	var walls := Node3D.new()
	walls.name = "CurtainWalls"
	root.add_child(walls)
	var sm := _toon(STONE)
	var smd := _toon(STONE_DK)
	var floor_y := 0.6   # interior floor top, walls sit on it

	# back wall (-z)
	_box(walls, Vector3(hw * 2.0, h, t), sm, Vector3(0, floor_y + h * 0.5, -hd))
	# side walls (±x) — leave a gap at the front for the open face
	for s in SIDES:
		_box(walls, Vector3(t, h, hd * 2.0), sm, Vector3(s * hw, floor_y + h * 0.5, 0))
	# darker lower course band (visual ashlar weathering)
	_box(walls, Vector3(hw * 2.0 + 0.06, 1.2, t + 0.08), smd, Vector3(0, floor_y + 0.6, -hd))
	for s in SIDES:
		_box(walls, Vector3(t + 0.08, 1.2, hd * 2.0 + 0.06), smd, Vector3(s * hw, floor_y + 0.6, 0))
	# carved gold string-course mid-band (luxury reveal) along back + sides
	_box(walls, Vector3(hw * 2.0 + 0.12, 0.14, t + 0.1), _metal(GOLD_DK, 0.5), Vector3(0, floor_y + h * 0.62, -hd))
	for s in SIDES:
		_box(walls, Vector3(t + 0.1, 0.14, hd * 2.0 + 0.12), _metal(GOLD_DK, 0.5), Vector3(s * hw, floor_y + h * 0.62, 0))

	# low front threshold parapet (so you read an enclosure, but walk straight in)
	for s in SIDES:
		_box(walls, Vector3(hw - 2.2, 0.9, t), sm, Vector3(s * (hw * 0.5 + 1.1), floor_y + 0.45, hd))
		# gold ball finials on the threshold parapet ends
		_ball(walls, 0.22, _metal(GOLD, 0.3), Vector3(s * (hw - 1.4), floor_y + 1.05, hd))

	# crenellations (merlons) along back + sides + the wall-walk underneath
	_crenellate_run(walls, Vector3(-hw + 0.6, floor_y + h, -hd), Vector3(1, 0, 0), (hw * 2.0) - 1.2, t)
	for s in SIDES:
		_crenellate_run(walls, Vector3(s * hw, floor_y + h, -hd + 0.6), Vector3(0, 0, 1), (hd * 2.0) - 1.2, t)
	# wall-walk slab (battlement floor) — the "level 2" you reach by the stair
	_box(walls, Vector3(hw * 2.0 + 0.4, 0.25, 1.0), smd, Vector3(0, floor_y + h - 0.12, -hd + 0.5))
	for s in SIDES:
		_box(walls, Vector3(1.0, 0.25, hd * 2.0 + 0.4), smd, Vector3(s * (hw - 0.5), floor_y + h - 0.12, 0))

	# arrow-slit windows (glowing) set into the back + side walls
	var slit := _toon(STONE_DK)
	var glow := _glow(WINDOW_GLOW, 1.7)
	for i: int in range(-1, 2):
		_arrow_slit(walls, Vector3(float(i) * 4.6, floor_y + 2.4, -hd + t * 0.5 + 0.02), 0.0, slit, glow)
	for s in SIDES:
		for j: int in range(-1, 2):
			_arrow_slit(walls, Vector3(s * (hw - t * 0.5 - 0.02), floor_y + 2.4, float(j) * 4.4), PI * 0.5, slit, glow)


## A run of crenellations (merlon / crenel pattern) along `dir` from `start`.
## Each merlon is gold-capped at its top edge for a richer skyline.
static func _crenellate_run(parent: Node3D, start: Vector3, dir: Vector3, length: float, t: float) -> void:
	var merlon_w := 0.7
	var gap := 0.55
	var step := merlon_w + gap
	var n := int(length / step)
	var cap := _toon(STONE_LT)
	var along_x := absf(dir.x) > 0.5
	var pos := start
	for i: int in range(n):
		var msz := Vector3(merlon_w if along_x else t + 0.12, 0.8, t + 0.12 if along_x else merlon_w)
		_box(parent, msz, cap, pos + Vector3(0, 0.4, 0))
		# thin gold cap reveal on top of each merlon
		_box(parent, Vector3(msz.x * 0.9, 0.06, msz.z * 0.9), _metal(GOLD_DK, 0.5), pos + Vector3(0, 0.83, 0))
		pos += dir * step


## An arrow-slit: a tall narrow recess with a cruciform notch + an inner glow,
## framed by chamfered stone. `yaw` faces it along ±x (PI*0.5) or ±z (0).
static func _arrow_slit(parent: Node3D, pos: Vector3, yaw: float, frame: Material, glow: Material) -> void:
	var n := Node3D.new()
	n.position = pos
	n.rotation.y = yaw
	parent.add_child(n)
	# recessed dark frame
	_box(n, Vector3(0.5, 1.5, 0.12), frame, Vector3.ZERO)
	# the lit slit (vertical) + the cross-bar (so it reads as a fighting loop)
	_box(n, Vector3(0.14, 1.2, 0.16), glow, Vector3(0, 0, 0.02))
	_box(n, Vector3(0.42, 0.16, 0.16), glow, Vector3(0, 0.18, 0.02))
	# chamfered stone surround
	_box(n, Vector3(0.66, 0.12, 0.1), _toon(STONE_LT), Vector3(0, 0.82, 0.04))


# ─────────────────────────────── oriel balconies ───────────────────────────

## Projecting oriel balconies (carved corbel + railing + glowing leaded window)
## mid-way up the two side walls — instant aristocratic detail on the silhouette.
static func _oriel_balconies(root: Node3D, hw: float, hd: float, wall_h: float) -> void:
	var ob := Node3D.new()
	ob.name = "Oriels"
	root.add_child(ob)
	var floor_y := 0.6
	for s in SIDES:
		var o := Node3D.new()
		o.position = Vector3(s * (hw - 0.4), floor_y + wall_h * 0.62, 0.0)
		o.rotation.y = -s * PI * 0.5
		ob.add_child(o)
		# corbel bracket (stacked prisms) supporting the projection
		_prism(o, Vector3(2.0, 1.0, 1.4), _toon(STONE_DK), Vector3(0, -1.1, 0), Vector3(PI, 0, 0))
		# the oriel box (bay) projecting outward (+local x)
		_box(o, Vector3(1.0, 1.8, 2.6), _toon(STONE_LT), Vector3(0.55, 0, 0))
		# glowing leaded glass face + brass mullions
		_box(o, Vector3(0.12, 1.4, 2.2), _glow(WINDOW_GLOW, 1.5), Vector3(1.07, 0, 0))
		for my: float in [-0.45, 0.45]:
			_box(o, Vector3(0.16, 0.08, 2.2), _metal(BRASS, 0.4), Vector3(1.09, my, 0))
		for mz: float in [-0.7, 0.0, 0.7]:
			_box(o, Vector3(0.16, 1.4, 0.08), _metal(BRASS, 0.4), Vector3(1.09, 0, mz))
		# little gold-balustraded balcony shelf below the window
		_box(o, Vector3(0.5, 0.1, 2.8), _toon(STONE_LT), Vector3(0.8, -0.95, 0))
		for bz: float in [-1.1, -0.55, 0.0, 0.55, 1.1]:
			_cyl(o, 0.05, 0.06, 0.55, _metal(GOLD, 0.35), Vector3(1.05, -0.7, bz), Vector3.ZERO, 8)
		_box(o, Vector3(0.12, 0.1, 2.8), _metal(GOLD, 0.35), Vector3(1.05, -0.43, 0))
		# tiny conical lead roof cap over the oriel
		_cyl(o, 0.0, 1.0, 0.9, _toon(SLATE), Vector3(0.6, 1.35, 0), Vector3.ZERO, 4)
		_ball(o, 0.12, _glow(GOLD, 2.2), Vector3(0.6, 1.9, 0))


# ─────────────────────────────── corner towers ─────────────────────────────

## Four round corner towers, each taller than the wall, crenellated, with a carved
## gold string-course, brass-trimmed dormers, a conical slate roof and a gold pennant
## on a pole. The two FRONT towers also carry hanging banners — the strongest part
## of the silhouette.
static func _corner_towers(root: Node3D, hw: float, hd: float, wall_h: float) -> void:
	var towers := Node3D.new()
	towers.name = "CornerTowers"
	root.add_child(towers)
	var th := wall_h + 3.6     # tower body height
	var tr := 2.0              # tower radius
	var corners: Array[Vector2] = [
		Vector2(-hw, -hd), Vector2(hw, -hd),   # back pair
		Vector2(-hw, hd), Vector2(hw, hd),     # front pair
	]
	var front_flags: Array[Color] = [BANNER_BLUE, BANNER_RED, BANNER_RED, BANNER_BLUE]
	for ci: int in range(corners.size()):
		var c: Vector2 = corners[ci]
		_one_tower(towers, Vector3(c.x, 0, c.y), tr, th, front_flags[ci], c.y > 0.0)


static func _one_tower(parent: Node3D, base_pos: Vector3, r: float, h: float, flag_col: Color, is_front: bool) -> void:
	var t := Node3D.new()
	t.position = base_pos
	parent.add_child(t)
	var sm := _toon(STONE)
	var smd := _toon(STONE_DK)
	# tapered body (very slight) on a flared base
	_cyl(t, r * 0.92, r * 1.12, 0.7, smd, Vector3(0, 0.35, 0), Vector3.ZERO, 22)
	_cyl(t, r, r * 0.96, h, sm, Vector3(0, h * 0.5 + 0.6, 0), Vector3.ZERO, 22)
	# stone string-courses (horizontal bands), the middle one gold-leafed
	for yb: float in [1.8, 3.4, 5.0]:
		var band_mat: Material = _metal(GOLD_DK, 0.5) if absf(yb - 3.4) < 0.01 else smd
		_cyl(t, r * 1.03, r * 1.03, 0.16 if absf(yb - 3.4) < 0.01 else 0.18, band_mat, Vector3(0, yb, 0), Vector3.ZERO, 22)
	# machicolation / corbel ring under the parapet
	_cyl(t, r * 1.18, r * 1.06, 0.5, smd, Vector3(0, h + 0.55, 0), Vector3.ZERO, 22)
	# crenellated parapet ring (ring of merlons, gold-capped)
	var cap := _toon(STONE_LT)
	var merlons := 12
	for i: int in range(merlons):
		var a := TAU * float(i) / float(merlons)
		_box(t, Vector3(0.5, 0.8, 0.34), cap,
			Vector3(cos(a) * (r * 1.05), h + 1.2, sin(a) * (r * 1.05)),
			Vector3(0, -a, 0))
		_box(t, Vector3(0.46, 0.06, 0.3), _metal(GOLD_DK, 0.5),
			Vector3(cos(a) * (r * 1.05), h + 1.63, sin(a) * (r * 1.05)),
			Vector3(0, -a, 0))
	# arrow slits round the tower (4 cardinal)
	var glow := _glow(WINDOW_GLOW, 1.6)
	for k: int in range(4):
		var a2 := TAU * float(k) / 4.0
		var rp := r * 0.99
		_arrow_slit(t, Vector3(cos(a2) * rp, h * 0.55, sin(a2) * rp), -a2 + PI * 0.5, smd, glow)
	# conical slate roof + ridge lines
	var roof_h := 3.2
	_cyl(t, 0.0, r * 1.16, roof_h, _toon(SLATE), Vector3(0, h + 1.6 + roof_h * 0.5, 0), Vector3.ZERO, 22)
	# lighter slate scales (a couple of ring courses) for richness
	for sc: float in [0.3, 0.55, 0.8]:
		var rr: float = r * 1.16 * (1.0 - sc)
		_torus(t, rr * 0.9, rr, _toon(SLATE_LT), Vector3(0, h + 1.6 + roof_h * sc, 0), Vector3(PI * 0.5, 0, 0), 16)
	# brass-trimmed dormers around the roof skirt (4 cardinal) — luxury roofline
	for dk: int in range(4):
		var da := TAU * float(dk) / 4.0 + PI * 0.25
		var dn := Node3D.new()
		dn.position = Vector3(cos(da) * r * 0.95, h + 1.9, sin(da) * r * 0.95)
		dn.rotation.y = -da + PI * 0.5
		t.add_child(dn)
		_box(dn, Vector3(0.6, 0.6, 0.4), _toon(STONE_LT), Vector3(0.0, 0, 0))
		_prism(dn, Vector3(0.7, 0.4, 0.5), _toon(SLATE_LT), Vector3(0, 0.5, 0))
		_box(dn, Vector3(0.06, 0.4, 0.28), _glow(WINDOW_GLOW, 1.4), Vector3(0.24, -0.02, 0))
		_box(dn, Vector3(0.08, 0.5, 0.06), _metal(BRASS, 0.4), Vector3(0.24, 0, 0))
	# gold finial ball + pennant pole + glowing finial
	_ball(t, 0.3, _metal(GOLD, 0.25), Vector3(0, h + 1.6 + roof_h + 0.1, 0))
	_cyl(t, 0.05, 0.05, 1.7, _metal(BRASS, 0.4), Vector3(0, h + 1.6 + roof_h + 1.0, 0))
	_ball(t, 0.13, _glow(GOLD, 2.4), Vector3(0, h + 1.6 + roof_h + 1.9, 0))
	# triangular gold-edged pennant flag (prism reads as a swallow-tail flag)
	var flag := Node3D.new()
	flag.position = Vector3(0, h + 1.6 + roof_h + 1.5, 0)
	t.add_child(flag)
	_box(flag, Vector3(1.3, 0.6, 0.04), _toon(flag_col, 0.18), Vector3(0.7, 0, 0))
	_prism(flag, Vector3(0.7, 0.6, 0.04), _toon(flag_col, 0.18), Vector3(1.7, 0, 0), Vector3(0, 0, -PI * 0.5))
	_box(flag, Vector3(1.3, 0.07, 0.05), _metal(GOLD, 0.4), Vector3(0.7, 0.3, 0))
	_box(flag, Vector3(1.3, 0.07, 0.05), _metal(GOLD, 0.4), Vector3(0.7, -0.3, 0))

	# front towers fly a long hanging house banner down the wall face
	if is_front:
		var ban := Node3D.new()
		ban.position = Vector3(0, h - 0.5, r * 0.9)
		t.add_child(ban)
		_box(ban, Vector3(1.1, 3.2, 0.06), _toon(flag_col, 0.18), Vector3.ZERO)
		# gold device (diamond) + trim
		_box(ban, Vector3(1.16, 0.12, 0.07), _metal(GOLD, 0.4), Vector3(0, 1.55, 0))
		_box(ban, Vector3(0.5, 0.5, 0.08), _metal(GOLD, 0.35), Vector3(0, 0.2, 0.01), Vector3(0, 0, PI * 0.25))
		# swallow-tail bottom
		for s in SIDES:
			_prism(ban, Vector3(0.55, 0.7, 0.06), _toon(flag_col, 0.18),
				Vector3(s * 0.275, -1.95, 0), Vector3(PI, 0, 0))


# ─────────────────────────────── wing turrets / statues ────────────────────

## Two slender statue-topped wing turrets flanking the gatehouse front (just inside
## the corner towers) carrying heraldic LION STATUES on plinths — a regal sentinel
## pair that frames the entrance and lifts the front silhouette.
static func _wing_turrets(root: Node3D, hw: float, hd: float, wall_h: float) -> void:
	var wt := Node3D.new()
	wt.name = "WingTurrets"
	root.add_child(wt)
	for s in SIDES:
		var n := Node3D.new()
		n.position = Vector3(s * (hw - 3.2), 0, hd - 0.2)
		wt.add_child(n)
		# round plinth + slim shaft
		_cyl(n, 1.0, 1.2, 0.6, _toon(STONE_DK), Vector3(0, 0.3, 0), Vector3.ZERO, 16)
		_cyl(n, 0.7, 0.78, wall_h * 0.7, _toon(STONE), Vector3(0, 0.6 + wall_h * 0.35, 0), Vector3.ZERO, 16)
		# gold capital ring
		_cyl(n, 0.92, 0.78, 0.26, _metal(GOLD_DK, 0.5), Vector3(0, 0.6 + wall_h * 0.7, 0), Vector3.ZERO, 16)
		# square cap pedestal for the statue
		var top := 0.6 + wall_h * 0.7 + 0.3
		_box(n, Vector3(1.2, 0.4, 1.2), _toon(STONE_LT), Vector3(0, top, 0))
		_statue_lion(n, Vector3(0, top + 0.2, 0), s)


## A stylized seated heraldic lion in pale marble — a luxury showpiece statue.
static func _statue_lion(parent: Node3D, pos: Vector3, facing: float) -> void:
	var n := Node3D.new()
	n.position = pos
	n.rotation.y = facing * 0.25   # turned slightly toward the path
	parent.add_child(n)
	var marble := _toon(MARBLE, 0.16)
	# haunches / seated body
	_box(n, Vector3(0.7, 0.7, 1.1), marble, Vector3(0, 0.55, -0.15))
	_ball(n, 0.42, marble, Vector3(0, 0.6, -0.2), Vector3(1.0, 1.0, 1.1))
	# upright chest + front legs
	_box(n, Vector3(0.55, 1.0, 0.5), marble, Vector3(0, 1.0, 0.35))
	for s in SIDES:
		_cyl(n, 0.14, 0.16, 1.0, marble, Vector3(s * 0.18, 0.5, 0.55), Vector3.ZERO, 8)
		_ball(n, 0.16, marble, Vector3(s * 0.18, 0.05, 0.7))   # paw
	# maned head
	_ball(n, 0.34, marble, Vector3(0, 1.7, 0.4))
	_torus(n, 0.18, 0.42, _toon(GOLD_DK, 0.2), Vector3(0, 1.7, 0.32), Vector3(0, 0, 0), 12)  # gilt mane
	_box(n, Vector3(0.22, 0.2, 0.3), marble, Vector3(0, 1.62, 0.62))  # muzzle
	for s in SIDES:
		_prism(n, Vector3(0.12, 0.16, 0.1), marble, Vector3(s * 0.16, 1.96, 0.4))  # ears
	# the lion holds a small gold heraldic shield
	_box(n, Vector3(0.5, 0.6, 0.08), _metal(GOLD, 0.3), Vector3(0, 0.95, 0.62))
	_prism(n, Vector3(0.5, 0.28, 0.08), _metal(GOLD, 0.3), Vector3(0, 0.55, 0.62), Vector3(PI, 0, 0))


# ───────────────────────────────── gatehouse ───────────────────────────────

## The gatehouse straddles the front opening: twin half-bastions, a pointed stone
## arch, a raised GOLD-banded PORTCULLIS in its slot, heavy timber doors thrown open,
## a banner over the arch, and lit braziers either side of the threshold.
static func _gatehouse(root: Node3D, hw: float, hd: float, wall_h: float, t: float) -> void:
	var g := Node3D.new()
	g.name = "Gatehouse"
	g.position = Vector3(0, 0.6, hd)
	root.add_child(g)
	var sm := _toon(STONE)
	var smd := _toon(STONE_DK)
	var gate_w := 3.4          # clear opening width
	var gate_h := 4.6          # arch springline height
	var gh_h := wall_h + 2.6   # gatehouse roof height
	var bastion_w := 2.6

	# twin bastion blocks flanking the gate
	for s in SIDES:
		var bx: float = s * (gate_w * 0.5 + bastion_w * 0.5)
		_box(g, Vector3(bastion_w, gh_h, 2.2), sm, Vector3(bx, gh_h * 0.5, 0))
		_box(g, Vector3(bastion_w + 0.08, 1.4, 2.3), smd, Vector3(bx, 0.7, 0))
		# gold string-course on the bastions
		_box(g, Vector3(bastion_w + 0.1, 0.14, 2.3), _metal(GOLD_DK, 0.5), Vector3(bx, gh_h * 0.62, 0))
		# bastion crenellations
		for k: int in range(3):
			_box(g, Vector3(0.6, 0.8, 0.5), _toon(STONE_LT),
				Vector3(bx - bastion_w * 0.5 + 0.45 + float(k) * 0.85, gh_h + 0.4, s * 0.85))
		# brazier of fire at the threshold
		_brazier(g, Vector3(bx * 0.55, 0, 1.5))

	# the spanning lintel block + pointed arch over the opening
	_box(g, Vector3(gate_w + bastion_w * 2.0 + 0.2, gh_h - gate_h - 0.4, 2.2),
		sm, Vector3(0, gate_h + 0.4 + (gh_h - gate_h - 0.4) * 0.5, 0))
	# pointed arch voussoirs (two leaning prisms meeting at an apex)
	for s in SIDES:
		_prism(g, Vector3(gate_w * 0.62, 1.5, 2.0), smd,
			Vector3(s * gate_w * 0.25, gate_h + 0.05, 0), Vector3(0, 0, s * 0.5))
	# arch keystone (gold-trimmed) with a glowing gem
	_box(g, Vector3(0.7, 0.9, 2.1), _toon(STONE_LT), Vector3(0, gate_h + 0.55, 0))
	_box(g, Vector3(0.4, 0.4, 2.2), _metal(GOLD, 0.4), Vector3(0, gate_h + 0.55, 0), Vector3(0, 0, PI * 0.25))
	_ball(g, 0.14, _glow(Color(0.5, 0.85, 1.0), 2.4), Vector3(0, gate_h + 0.55, 1.12))

	# arch ring of voussoir stones for richness
	var voussoirs := 9
	for i: int in range(voussoirs):
		var a: float = lerp(PI * 0.92, PI * 0.08, float(i) / float(voussoirs - 1))
		var rad := gate_w * 0.5 + 0.45
		_box(g, Vector3(0.55, 0.7, 2.05), smd,
			Vector3(cos(a) * rad, gate_h * 0.62 + sin(a) * 1.7, 0), Vector3(0, 0, a - PI * 0.5))

	# the PORTCULLIS — an iron grid (gold-banded) raised up into its slot under the arch
	var port := Node3D.new()
	port.position = Vector3(0, gate_h - 0.6, 0.55)
	g.add_child(port)
	var iron := _metal(IRON, 0.5)
	var bars := 6
	for i: int in range(bars):
		var x: float = lerp(-gate_w * 0.5 + 0.25, gate_w * 0.5 - 0.25, float(i) / float(bars - 1))
		_box(port, Vector3(0.12, 2.4, 0.12), iron, Vector3(x, 0, 0))
	for j: int in range(3):
		_box(port, Vector3(gate_w - 0.2, 0.12, 0.12), _metal(GOLD_DK, 0.5), Vector3(0, -0.9 + float(j) * 0.9, 0))
	# spike tips on the bottom rail
	for i: int in range(bars):
		var x2: float = lerp(-gate_w * 0.5 + 0.25, gate_w * 0.5 - 0.25, float(i) / float(bars - 1))
		_prism(port, Vector3(0.2, 0.3, 0.2), iron, Vector3(x2, -1.35, 0), Vector3(PI, 0, 0))

	# heavy timber doors thrown OPEN (so you walk straight in), iron-studded
	for s in SIDES:
		var leaf := Node3D.new()
		leaf.position = Vector3(s * (gate_w * 0.5 - 0.08), gate_h * 0.5 - 0.3, -0.5)
		leaf.rotation.y = s * 1.95   # swung inward/open
		g.add_child(leaf)
		_box(leaf, Vector3(gate_w * 0.5, gate_h - 0.6, 0.18), _toon(WOOD), Vector3(s * gate_w * 0.25, 0, 0))
		# plank lines + iron straps + studs
		for p: int in range(3):
			_box(leaf, Vector3(0.04, gate_h - 0.7, 0.2), _toon(WOOD_DK),
				Vector3(s * (0.3 + float(p) * 0.55), 0, 0))
		for sy: float in [-1.0, 1.0]:
			_box(leaf, Vector3(gate_w * 0.5, 0.14, 0.22), _metal(IRON, 0.55),
				Vector3(s * gate_w * 0.25, sy * (gate_h * 0.5 - 0.7), 0))
		# ring handle
		_torus(leaf, 0.08, 0.16, _metal(IRON, 0.5), Vector3(s * gate_w * 0.42, 0, 0.14), Vector3(PI * 0.5, 0, 0))

	# banner draped over the arch (house colors + gold device)
	var ban := Node3D.new()
	ban.position = Vector3(0, gate_h + 0.5, 1.18)
	g.add_child(ban)
	_box(ban, Vector3(1.5, 2.4, 0.06), _toon(BANNER_RED, 0.18), Vector3.ZERO)
	_box(ban, Vector3(1.56, 0.14, 0.07), _metal(GOLD, 0.4), Vector3(0, 1.1, 0))
	_ball(ban, 0.34, _metal(GOLD, 0.3), Vector3(0, 0.2, 0.05), Vector3(1, 1, 0.4))
	for s in SIDES:
		_prism(ban, Vector3(0.75, 0.7, 0.06), _toon(BANNER_RED, 0.18),
			Vector3(s * 0.375, -1.55, 0), Vector3(PI, 0, 0))


## A lit iron brazier on a tripod — warm fire glow framing the gate / dais.
static func _brazier(parent: Node3D, pos: Vector3) -> void:
	var b := Node3D.new()
	b.position = pos
	parent.add_child(b)
	var iron := _metal(IRON, 0.5)
	# tripod legs
	for k: int in range(3):
		var a := TAU * float(k) / 3.0
		_cyl(b, 0.04, 0.06, 1.2, iron, Vector3(cos(a) * 0.22, 0.6, sin(a) * 0.22), Vector3(cos(a) * 0.18, 0, -sin(a) * 0.18))
	# bowl
	_cyl(b, 0.42, 0.26, 0.4, iron, Vector3(0, 1.3, 0), Vector3.ZERO, 14)
	# coals + flame
	_ball(b, 0.3, _glow(FIRE, 2.0), Vector3(0, 1.42, 0), Vector3(1, 0.5, 1), 12, 6)
	for fk: int in range(3):
		var fa := TAU * float(fk) / 3.0 + 0.4
		_ball(b, 0.18, _glow(Color(1.0, 0.78, 0.3), 2.6),
			Vector3(cos(fa) * 0.12, 1.62 + float(fk) * 0.06, sin(fa) * 0.12), Vector3(0.8, 1.6, 0.8), 10, 5)
	_lamp(b, FIRE, 1.4, 6.0, Vector3(0, 1.6, 0))


# ───────────────────────────── great hall interior ─────────────────────────

## The walkable great hall: a high beamed ceiling held off the side walls (front
## kept OPEN), a brass-capitalled colonnade defining bays, hanging banners, wall
## sconces, and a great fire-lit stone hearth on the side wall. Ground stays CLEAR
## for the owner to furnish.
static func _great_hall_interior(root: Node3D, hw: float, hd: float, wall_h: float, t: float) -> void:
	var hall := Node3D.new()
	hall.name = "GreatHall"
	hall.position = Vector3(0, 0.6, 0)   # sits on the interior floor top
	root.add_child(hall)
	var ceil_y := 3.5

	# free-standing colonnade columns just inboard of the side walls — define bays
	var col_mat := _toon(STONE_LT)
	for s in SIDES:
		for j: int in range(-2, 3):
			var cx: float = s * (hw - t - 1.0)
			var cz: float = float(j) * 3.4
			# fluted shaft on a moulded base + brass capital
			_cyl(hall, 0.34, 0.42, 0.3, _toon(STONE_DK), Vector3(cx, 0.15, cz), Vector3.ZERO, 14)
			_cyl(hall, 0.3, 0.34, ceil_y - 0.7, col_mat, Vector3(cx, ceil_y * 0.5, cz), Vector3.ZERO, 16)
			_cyl(hall, 0.46, 0.34, 0.3, _metal(BRASS, 0.45), Vector3(cx, ceil_y - 0.35, cz), Vector3.ZERO, 16)
			_box(hall, Vector3(0.7, 0.18, 0.7), col_mat, Vector3(cx, ceil_y - 0.1, cz))
	# engaged pilasters up the back wall
	for j: int in range(-2, 3):
		_box(hall, Vector3(0.5, ceil_y, 0.5), col_mat, Vector3(float(j) * 3.4, ceil_y * 0.5, -hd + t * 0.5 + 0.3))
		_box(hall, Vector3(0.7, 0.35, 0.7), _metal(BRASS, 0.45), Vector3(float(j) * 3.4, ceil_y - 0.1, -hd + t * 0.5 + 0.3))

	# timber roof beams across the hall (open-truss feel), held above the columns
	var beam := _toon(WOOD)
	for j: int in range(-2, 3):
		_box(hall, Vector3(hw * 2.0 - 1.4, 0.3, 0.32), beam, Vector3(0, ceil_y, float(j) * 3.4))
	# ridge + a coffered stone ceiling slab so upstairs reads as enclosed
	_box(hall, Vector3(0.4, 0.4, hd * 2.0 - 1.2), beam, Vector3(0, ceil_y + 0.2, 0))
	_box(hall, Vector3(hw * 2.0 - 1.2, 0.2, hd * 2.0 - 1.2), _toon(STONE_DK), Vector3(0, ceil_y + 0.45, 0))
	# gilt coffer ribs across the ceiling for a richer soffit
	for j: int in range(-3, 4):
		_box(hall, Vector3(hw * 2.0 - 1.4, 0.08, 0.12), _metal(GOLD_DK, 0.5), Vector3(0, ceil_y + 0.34, float(j) * 2.4))

	# hanging interior banners between bays (alternating house colors)
	for s in SIDES:
		for j: int in range(-1, 2):
			var col: Color = BANNER_BLUE if (j % 2 == 0) else BANNER_RED
			var bn := Node3D.new()
			bn.position = Vector3(s * (hw - t - 0.4), ceil_y - 1.4, float(j) * 3.4 + 1.7)
			hall.add_child(bn)
			_box(bn, Vector3(0.05, 2.0, 0.9), _toon(col, 0.18), Vector3.ZERO)
			_box(bn, Vector3(0.06, 0.12, 0.96), _metal(GOLD, 0.4), Vector3(0, 0.85, 0))
			_ball(bn, 0.18, _metal(GOLD, 0.35), Vector3(0.02, 0.1, 0), Vector3(0.3, 1, 1))

	# wall sconces (glowing) up the side walls
	for s in SIDES:
		for j: int in range(-1, 2):
			_sconce(hall, Vector3(s * (hw - t - 0.05), 2.6, float(j) * 4.2), s)

	# suits of armour standing sentinel along the back wall (a collector flex)
	for s in SIDES:
		_armour(hall, Vector3(s * 2.4, 0, -hd + t + 0.8))

	# the GREAT HEARTH — a grand fireplace on the back-left wall
	_hearth(hall, Vector3(-hw + t + 0.05, 0, -hd * 0.45), wall_h)


## A wall sconce: a brass bracket + a glowing flame + a pool of warm light.
static func _sconce(parent: Node3D, pos: Vector3, facing: float) -> void:
	var n := Node3D.new()
	n.position = pos
	parent.add_child(n)
	var br := _metal(BRASS, 0.4)
	_box(n, Vector3(0.1, 0.5, 0.2), br, Vector3(-facing * 0.1, 0, 0))
	_cyl(n, 0.14, 0.1, 0.22, br, Vector3(-facing * 0.28, 0.18, 0), Vector3.ZERO, 10)
	_ball(n, 0.13, _glow(Color(1.0, 0.78, 0.36), 2.4), Vector3(-facing * 0.28, 0.36, 0), Vector3(0.8, 1.5, 0.8), 10, 5)
	_lamp(n, WINDOW_GLOW, 0.9, 5.5, Vector3(-facing * 0.4, 0.4, 0))


## A polished suit of plate armour on a stand — a luxury hall showpiece.
static func _armour(parent: Node3D, pos: Vector3) -> void:
	var n := Node3D.new()
	n.position = pos
	parent.add_child(n)
	var steel := _metal(Color(0.78, 0.80, 0.85), 0.18)
	var gold := _metal(GOLD, 0.3)
	# plinth
	_cyl(n, 0.4, 0.46, 0.2, _toon(STONE_DK), Vector3(0, 0.1, 0), Vector3.ZERO, 12)
	# legs + torso + pauldrons
	for s in SIDES:
		_cyl(n, 0.1, 0.12, 1.0, steel, Vector3(s * 0.16, 0.7, 0), Vector3.ZERO, 8)
		_ball(n, 0.18, steel, Vector3(s * 0.34, 1.55, 0))   # pauldron
		_cyl(n, 0.08, 0.09, 0.8, steel, Vector3(s * 0.34, 1.2, 0.0), Vector3.ZERO, 8)  # arm
	_box(n, Vector3(0.55, 0.8, 0.35), steel, Vector3(0, 1.55, 0))
	_box(n, Vector3(0.58, 0.12, 0.37), gold, Vector3(0, 1.2, 0))   # gilt belt
	# helm with plume
	_ball(n, 0.22, steel, Vector3(0, 2.1, 0))
	_box(n, Vector3(0.18, 0.1, 0.26), steel, Vector3(0, 2.06, 0.12))  # visor
	_prism(n, Vector3(0.16, 0.5, 0.16), _toon(BANNER_RED, 0.2), Vector3(0, 2.45, -0.05))  # plume
	# upright sword + shield
	_box(n, Vector3(0.06, 1.6, 0.04), steel, Vector3(0.5, 1.0, 0.05))
	_box(n, Vector3(0.22, 0.06, 0.04), gold, Vector3(0.5, 1.7, 0.05))  # crossguard
	_box(n, Vector3(0.5, 0.7, 0.06), _toon(BANNER_BLUE, 0.2), Vector3(-0.45, 1.4, 0.1))
	_box(n, Vector3(0.55, 0.08, 0.07), gold, Vector3(-0.45, 1.7, 0.1))


## The hearth: a deep stone fireplace with a carved mantel, a roaring fire, log
## pile, and a heraldic shield above. A built-in showpiece, left against the wall.
static func _hearth(parent: Node3D, pos: Vector3, wall_h: float) -> void:
	var h := Node3D.new()
	h.position = pos
	h.rotation.y = PI * 0.5   # faces into the hall (+x)
	parent.add_child(h)
	var sm := _toon(STONE)
	var smd := _toon(STONE_DK)
	# surround
	for s in SIDES:
		_box(h, Vector3(0.6, 3.0, 1.0), sm, Vector3(s * 1.5, 1.5, 0))
	_box(h, Vector3(3.6, 0.9, 1.0), sm, Vector3(0, 3.0, 0))         # lintel
	_box(h, Vector3(4.2, 0.4, 1.2), _toon(MARBLE, 0.18), Vector3(0, 2.0, 0.1))  # marble mantel shelf
	_box(h, Vector3(4.2, 0.08, 1.24), _metal(GOLD_DK, 0.5), Vector3(0, 2.22, 0.1))  # gold reveal under shelf
	# firebox back (sooted) + opening
	_box(h, Vector3(2.4, 2.0, 0.3), smd, Vector3(0, 1.1, -0.35))
	# fire + logs
	for k: int in range(3):
		_cyl(h, 0.14, 0.14, 1.6, _toon(WOOD_DK), Vector3(-0.5 + float(k) * 0.5, 0.3, 0.1), Vector3(0, 0, PI * 0.5))
	_ball(h, 0.5, _glow(FIRE, 2.2), Vector3(0, 0.5, 0.0), Vector3(1.4, 0.7, 0.6), 12, 6)
	for fk: int in range(4):
		var fa := TAU * float(fk) / 4.0
		_ball(h, 0.22, _glow(Color(1.0, 0.8, 0.32), 2.8),
			Vector3(cos(fa) * 0.4, 0.7 + float(fk) * 0.05, 0.05), Vector3(0.8, 1.8, 0.7), 10, 5)
	_lamp(h, FIRE, 2.2, 8.0, Vector3(0, 0.7, 0.6))
	# heraldic shield over the mantel (gold-bordered, gem-set)
	var sh := Node3D.new()
	sh.position = Vector3(0, 3.0, 0.4)
	h.add_child(sh)
	_box(sh, Vector3(1.0, 1.2, 0.12), _toon(BANNER_RED, 0.2), Vector3.ZERO)
	_prism(sh, Vector3(1.0, 0.5, 0.12), _toon(BANNER_RED, 0.2), Vector3(0, -0.85, 0), Vector3(PI, 0, 0))
	_box(sh, Vector3(1.1, 0.1, 0.14), _metal(GOLD, 0.4), Vector3(0, 0.5, 0))
	_ball(sh, 0.22, _metal(GOLD, 0.3), Vector3(0, 0, 0.06), Vector3(1, 1, 0.5))
	# crossed swords behind the shield
	for s in SIDES:
		_box(sh, Vector3(0.07, 1.7, 0.05), _metal(Color(0.8, 0.82, 0.86), 0.2),
			Vector3(0, 0.2, -0.05), Vector3(0, 0, s * 0.6))


# ──────────────────────────────── chandelier ───────────────────────────────

## A grand iron-and-gold ring CHANDELIER with hanging crystals + glowing candles,
## suspended over the hall centre — the headline interior luxury showpiece.
static func _chandelier(root: Node3D, hw: float, hd: float) -> void:
	var floor_y := 0.6
	var ceil_y := 3.5
	var ch := Node3D.new()
	ch.name = "Chandelier"
	ch.position = Vector3(0, floor_y + ceil_y - 1.1, 0.6)
	root.add_child(ch)
	# chain up to the ceiling
	_cyl(ch, 0.04, 0.04, 1.0, _metal(IRON, 0.5), Vector3(0, 1.15, 0), Vector3.ZERO, 6)
	# twin concentric gold rings
	_torus(ch, 1.5, 1.7, _metal(GOLD, 0.28), Vector3(0, 0, 0), Vector3(PI * 0.5, 0, 0), 20)
	_torus(ch, 0.85, 1.0, _metal(GOLD, 0.28), Vector3(0, 0.35, 0), Vector3(PI * 0.5, 0, 0), 18)
	# spokes
	for i: int in range(6):
		var a := TAU * float(i) / 6.0
		_cyl(ch, 0.03, 0.03, 1.7, _metal(BRASS, 0.4), Vector3(cos(a) * 0.85, 0.2, sin(a) * 0.85), Vector3(0, -a, 0.45))
	# candles + flames around the outer ring
	for i: int in range(12):
		var a := TAU * float(i) / 12.0
		var cx := cos(a) * 1.6
		var cz := sin(a) * 1.6
		_cyl(ch, 0.07, 0.08, 0.3, _toon(Color(0.95, 0.93, 0.85), 0.2), Vector3(cx, 0.2, cz), Vector3.ZERO, 8)
		_ball(ch, 0.1, _glow(Color(1.0, 0.82, 0.4), 2.6), Vector3(cx, 0.42, cz), Vector3(0.8, 1.5, 0.8), 8, 4)
	# inner candle ring
	for i: int in range(6):
		var a := TAU * float(i) / 6.0 + 0.5
		_ball(ch, 0.09, _glow(Color(1.0, 0.82, 0.4), 2.6), Vector3(cos(a) * 0.92, 0.55, sin(a) * 0.92), Vector3(0.8, 1.5, 0.8), 8, 4)
	# hanging crystal teardrops (glass) for the sparkle
	for i: int in range(12):
		var a := TAU * float(i) / 12.0 + 0.25
		_ball(ch, 0.12, _glass(CRYSTAL, 0.5), Vector3(cos(a) * 1.6, -0.35, sin(a) * 1.6), Vector3(0.7, 1.4, 0.7), 8, 5)
	# central drop crystal + finial
	_ball(ch, 0.2, _glass(CRYSTAL, 0.55), Vector3(0, -0.55, 0), Vector3(0.8, 1.6, 0.8), 10, 6)
	_ball(ch, 0.12, _metal(GOLD, 0.25), Vector3(0, -0.95, 0))
	# warm fill light from the chandelier
	_lamp(ch, WINDOW_GLOW, 1.6, 11.0, Vector3(0, 0.2, 0))


# ──────────────────────────────── throne dais ──────────────────────────────

## The raised THRONE DAIS at the back of the hall: stepped stone platform, a gold
## throne with a velvet seat and a tall crested back, flanking pillars with
## glowing finials, and a pair of braziers. The wealth-read centerpiece.
static func _throne_dais(root: Node3D, hw: float, hd: float) -> void:
	var d := Node3D.new()
	d.name = "ThroneDais"
	d.position = Vector3(0, 0.6, -hd + 1.6)
	root.add_child(d)
	var sm := _toon(STONE)
	var smd := _toon(STONE_DK)
	# two stone steps up to the platform (red-carpet topped)
	_box(d, Vector3(6.4, 0.3, 2.6), smd, Vector3(0, 0.15, 1.4))
	_box(d, Vector3(5.4, 0.3, 1.8), sm, Vector3(0, 0.45, 0.9))
	_box(d, Vector3(4.6, 0.4, 3.4), sm, Vector3(0, 0.7, -0.3))
	_box(d, Vector3(4.7, 0.06, 3.5), _metal(GOLD_DK, 0.5), Vector3(0, 0.91, -0.3))  # gold edge band on platform
	_box(d, Vector3(2.4, 0.06, 2.8), _toon(BANNER_RED, 0.22), Vector3(0, 0.95, 0.2))  # dais carpet

	# flanking honor pillars with glowing gem finials
	for s in SIDES:
		var px: float = s * 2.1
		_cyl(d, 0.32, 0.4, 3.4, sm, Vector3(px, 2.0, -0.3), Vector3.ZERO, 14)
		_cyl(d, 0.5, 0.34, 0.3, _metal(BRASS, 0.45), Vector3(px, 3.85, -0.3), Vector3.ZERO, 14)  # capital
		_ball(d, 0.28, _metal(GOLD, 0.25), Vector3(px, 4.1, -0.3))
		_ball(d, 0.16, _glow(Color(0.6, 0.85, 1.0), 2.4), Vector3(px, 4.42, -0.3))
		_brazier(d, Vector3(px * 1.15, 0.9, 1.2))

	# a sweeping velvet canopy / baldachin over the throne
	_box(d, Vector3(3.6, 0.3, 2.4), _toon(VELVET, 0.18), Vector3(0, 5.0, -0.6))
	_box(d, Vector3(3.7, 0.1, 2.5), _metal(GOLD, 0.35), Vector3(0, 5.17, -0.6))
	for s in SIDES:
		_prism(d, Vector3(1.8, 0.6, 0.1), _toon(VELVET, 0.18), Vector3(0, 4.7, s * 1.2 - 0.6), Vector3(PI, 0, s * 0.0))
	_ball(d, 0.2, _metal(GOLD, 0.25), Vector3(0, 5.35, -0.6))

	# THE THRONE — gold frame, velvet seat, crested high back
	var th := Node3D.new()
	th.position = Vector3(0, 0.9, -0.6)
	d.add_child(th)
	var gold := _metal(GOLD, 0.26)
	var velvet := _toon(VELVET, 0.2)
	# seat box + base
	_box(th, Vector3(1.6, 0.3, 1.4), gold, Vector3(0, 0.6, 0))
	_box(th, Vector3(1.7, 0.2, 1.5), velvet, Vector3(0, 0.78, 0))   # cushion
	_box(th, Vector3(1.5, 0.5, 1.4), gold, Vector3(0, 0.25, 0))     # plinth
	# armrests
	for s in SIDES:
		_box(th, Vector3(0.22, 0.7, 1.3), gold, Vector3(s * 0.78, 0.75, 0))
		_ball(th, 0.16, gold, Vector3(s * 0.78, 1.1, 0.6))
	# tall back + velvet panel
	_box(th, Vector3(1.7, 3.0, 0.22), gold, Vector3(0, 2.0, -0.6))
	_box(th, Vector3(1.3, 2.4, 0.1), velvet, Vector3(0, 2.0, -0.48))
	# crest — fleur points + a central crown jewel
	for i: int in range(-2, 3):
		var hgt: float = 0.7 - absf(float(i)) * 0.12
		_prism(th, Vector3(0.3, hgt, 0.18), gold, Vector3(float(i) * 0.35, 3.6, -0.6))
	_ball(th, 0.3, _metal(GOLD, 0.22), Vector3(0, 3.7, -0.55))
	_ball(th, 0.18, _glow(Color(1.0, 0.3, 0.36), 2.6), Vector3(0, 3.9, -0.5))  # ruby crown jewel
	# bezel gems down the back rails
	for s in SIDES:
		for k: int in range(3):
			_ball(th, 0.1, _glow(Color(0.5, 0.8, 1.0), 2.0), Vector3(s * 0.72, 1.2 + float(k) * 0.7, -0.45))


# ─────────────────────────────── grand staircase ───────────────────────────

## A GRAND DOUBLE staircase from the hall floor up to the battlement wall-walk
## (level 2): two symmetric polished-marble flights hugging the back corners climb
## to a shared landing, with carved newels, gold rails + balusters. Kept tight to
## the corners so the hall stays open and walkable.
static func _grand_stair(root: Node3D, hw: float, hd: float, wall_h: float) -> void:
	var floor_y := 0.6
	var stairs := Node3D.new()
	stairs.name = "GrandStair"
	stairs.position = Vector3(0, floor_y, 0)
	root.add_child(stairs)
	# a mirrored flight on each back corner
	for s in SIDES:
		_stair_flight(stairs, Vector3(s * (hw - 2.6), 0, -hd + 2.0), wall_h, s)


## One marble flight + gold balustrade, mirrored by `side` (-1 / +1).
static func _stair_flight(root: Node3D, base_pos: Vector3, wall_h: float, side: float) -> void:
	var st := Node3D.new()
	st.position = base_pos
	root.add_child(st)
	var marble := _toon(MARBLE, 0.18)
	var marble_dk := _toon(Color(0.72, 0.72, 0.75), 0.18)
	var smd := _toon(STONE_DK)
	var steps := 14
	var rise := wall_h / float(steps)
	var run := 0.42
	var sw := 2.2   # stair width
	for i: int in range(steps):
		var y := rise * (float(i) + 0.5)
		var z := -run * float(i)
		_box(st, Vector3(sw, rise, run + 0.05), marble if i % 2 == 0 else marble_dk, Vector3(0, y, z))
		# thin gold nosing on each tread
		_box(st, Vector3(sw, 0.04, 0.06), _metal(GOLD_DK, 0.5), Vector3(0, y + rise * 0.5, z + run * 0.5))
	# carriage / stringer
	_box(st, Vector3(sw + 0.2, 0.4, run * float(steps) + 0.4),
		smd, Vector3(0, wall_h * 0.5, -run * float(steps) * 0.5), Vector3(-atan2(wall_h, run * float(steps)), 0, 0))
	# newel posts + gold rail along the open (inboard) side
	var gold := _metal(GOLD, 0.32)
	var rail_x := side * (-sw * 0.5)   # inboard edge faces the hall centre
	for i: int in [0, steps]:
		var z2 := -run * float(i)
		var y2 := rise * float(i)
		_cyl(st, 0.12, 0.14, 1.2, _metal(BRASS, 0.4), Vector3(rail_x, y2 + 0.6, z2), Vector3.ZERO, 10)
		_ball(st, 0.13, gold, Vector3(rail_x, y2 + 1.25, z2))
	# the rail itself (a leaning box following the slope)
	_box(st, Vector3(0.1, 0.1, run * float(steps) + 0.2), gold,
		Vector3(rail_x, wall_h * 0.5 + 1.0, -run * float(steps) * 0.5),
		Vector3(-atan2(wall_h, run * float(steps)), 0, 0))
	# baluster pickets
	var picks := 7
	for p: int in range(picks):
		var f := float(p) / float(picks - 1)
		_cyl(st, 0.04, 0.04, 1.0, gold,
			Vector3(rail_x, wall_h * f + 0.5, -run * float(steps) * f), Vector3.ZERO, 6)
	# top landing slab connecting onto the wall-walk
	_box(st, Vector3(sw + 0.4, 0.25, 1.6), smd, Vector3(0, wall_h + 0.0, -run * float(steps) - 0.7))


# ─────────────────────────────── landscaping ───────────────────────────────

## Approach grounds: a stone path to the gate, flanking hedges + topiary, standing
## torches, a grand tiered fountain on the lawn, manicured flowerbeds, and pennant
## poles — sells "estate".
static func _landscaping(root: Node3D, hw: float, hd: float) -> void:
	var land := Node3D.new()
	land.name = "Grounds"
	root.add_child(land)
	var path := _toon(Color(0.58, 0.57, 0.55))
	# cobbled approach path from the gate out into +z
	for i: int in range(6):
		var z := hd + 1.2 + float(i) * 1.5
		_box(land, Vector3(3.0, 0.08, 1.3), path, Vector3(0, 0.06, z))
		# cobble seams
		for s in SIDES:
			_box(land, Vector3(0.1, 0.1, 1.3), _toon(MORTAR), Vector3(s * 1.5, 0.07, z))

	# flanking hedges + clipped topiary balls along the path
	var hedge := _toon(Color(0.20, 0.42, 0.22), 0.25)
	var topiary := _toon(Color(0.24, 0.46, 0.24), 0.25)
	for s in SIDES:
		for i: int in range(5):
			var z := hd + 2.0 + float(i) * 1.7
			_box(land, Vector3(0.9, 1.0, 1.4), hedge, Vector3(s * 2.6, 0.5, z))
			_ball(land, 0.55, topiary, Vector3(s * 2.6, 1.0, z), Vector3(1, 0.6, 1), 10, 6)
		# manicured flowerbeds (low colored mounds) hugging the front wall base
		_box(land, Vector3(hw * 0.7, 0.3, 0.7), _toon(Color(0.22, 0.4, 0.22), 0.25), Vector3(s * hw * 0.5, 0.15, hd + 0.7))
		for fb: int in range(4):
			var fcol := Color(0.85, 0.3, 0.4) if fb % 2 == 0 else Color(0.9, 0.78, 0.3)
			_ball(land, 0.2, _toon(fcol, 0.3), Vector3(s * (hw * 0.5 - 1.4 + float(fb) * 0.9), 0.35, hd + 0.7), Vector3(1, 0.7, 1), 8, 5)

	# tall potted topiary cones flanking the gate threshold (formal entrance)
	for s in SIDES:
		_cyl(land, 0.32, 0.4, 0.5, _metal(BRASS, 0.45), Vector3(s * 2.4, 0.25, hd + 0.8), Vector3.ZERO, 12)
		_cyl(land, 0.0, 0.5, 1.6, topiary, Vector3(s * 2.4, 1.3, hd + 0.8), Vector3.ZERO, 10)
		_ball(land, 0.12, _glow(Color(1.0, 0.85, 0.5), 1.6), Vector3(s * 2.4, 2.2, hd + 0.8))

	# standing torches lining the approach
	for s in SIDES:
		for i: int in range(3):
			var z := hd + 2.5 + float(i) * 2.8
			_torch(land, Vector3(s * 3.4, 0, z))

	# a grand ornamental fountain on the front lawn
	_fountain(land, Vector3(0, 0, hd + 9.5))

	# pennant poles flanking the path mouth
	for s in SIDES:
		var col: Color = BANNER_RED if s < 0 else BANNER_BLUE
		_cyl(land, 0.08, 0.1, 5.0, _toon(WOOD), Vector3(s * 4.6, 2.5, hd + 1.0), Vector3.ZERO, 10)
		_ball(land, 0.16, _metal(GOLD, 0.3), Vector3(s * 4.6, 5.1, hd + 1.0))
		var fl := Node3D.new()
		fl.position = Vector3(s * 4.6, 4.4, hd + 1.0)
		land.add_child(fl)
		_box(fl, Vector3(0.04, 0.9, 1.2), _toon(col, 0.18), Vector3(0, 0, -s * 0.65))


## A standing torch on a stone foot, with a glowing flame + warm light.
static func _torch(parent: Node3D, pos: Vector3) -> void:
	var t := Node3D.new()
	t.position = pos
	parent.add_child(t)
	_box(t, Vector3(0.5, 0.3, 0.5), _toon(STONE_DK), Vector3(0, 0.15, 0))
	_cyl(t, 0.08, 0.1, 1.8, _toon(WOOD_DK), Vector3(0, 1.1, 0), Vector3.ZERO, 8)
	_cyl(t, 0.18, 0.1, 0.3, _metal(IRON, 0.5), Vector3(0, 2.05, 0), Vector3.ZERO, 10)
	_ball(t, 0.18, _glow(FIRE, 2.4), Vector3(0, 2.3, 0), Vector3(0.8, 1.5, 0.8), 10, 5)
	_lamp(t, FIRE, 1.0, 5.5, Vector3(0, 2.4, 0))


## A grand circular stone fountain with a statue-topped tiered center + glowing
## water and cascading bowls — the centrepiece of the approach lawn.
static func _fountain(parent: Node3D, pos: Vector3) -> void:
	var f := Node3D.new()
	f.position = pos
	parent.add_child(f)
	var sm := _toon(STONE)
	var smd := _toon(STONE_DK)
	# basin ring + carved rim
	_cyl(f, 1.8, 1.9, 0.7, sm, Vector3(0, 0.35, 0), Vector3.ZERO, 28)
	_torus(f, 1.65, 1.85, _toon(STONE_LT), Vector3(0, 0.68, 0), Vector3(PI * 0.5, 0, 0), 24)
	# scalloped gold rim reveal
	_torus(f, 1.62, 1.7, _metal(GOLD_DK, 0.5), Vector3(0, 0.72, 0), Vector3(PI * 0.5, 0, 0), 24)
	# water surface (glowing pale)
	_cyl(f, 1.55, 1.55, 0.1, _glow(WATER, 1.0), Vector3(0, 0.6, 0), Vector3.ZERO, 28)
	# tiered pedestal + two cascading bowls
	_cyl(f, 0.34, 0.46, 1.0, smd, Vector3(0, 1.0, 0), Vector3.ZERO, 16)
	_cyl(f, 0.95, 0.7, 0.2, sm, Vector3(0, 1.55, 0), Vector3.ZERO, 22)
	_cyl(f, 0.9, 0.9, 0.08, _glow(WATER, 0.9), Vector3(0, 1.66, 0), Vector3.ZERO, 22)
	_cyl(f, 0.2, 0.28, 0.8, smd, Vector3(0, 2.05, 0), Vector3.ZERO, 14)
	_cyl(f, 0.55, 0.4, 0.16, sm, Vector3(0, 2.5, 0), Vector3.ZERO, 18)
	_cyl(f, 0.5, 0.5, 0.06, _glow(WATER, 0.9), Vector3(0, 2.6, 0), Vector3.ZERO, 18)
	# a small heraldic statue spout on top (gold dolphin-esque finial)
	_cyl(f, 0.14, 0.2, 0.6, smd, Vector3(0, 2.9, 0), Vector3.ZERO, 10)
	_ball(f, 0.26, _metal(GOLD, 0.26), Vector3(0, 3.25, 0))
	_prism(f, Vector3(0.2, 0.5, 0.16), _metal(GOLD, 0.26), Vector3(0, 3.6, 0))
	# water jets cascading off the top
	_cyl(f, 0.04, 0.1, 0.7, _glow(Color(0.7, 0.88, 1.0), 1.6), Vector3(0, 3.95, 0), Vector3.ZERO, 8)
	for s in SIDES:
		_cyl(f, 0.03, 0.06, 0.9, _glow(Color(0.7, 0.88, 1.0), 1.4), Vector3(s * 0.5, 3.0, 0), Vector3(0, 0, s * 0.5), 8)
	_lamp(f, Color(0.6, 0.82, 1.0), 0.9, 7.5, Vector3(0, 1.4, 0))


# ─────────────────────────────────── meta ──────────────────────────────────

static func meta() -> Dictionary:
	return {
		"id": "stone_castle",
		"name": "Ravenspire Keep",
		"tier": "Castle",
		"rarity": "Legendary",
		"description": "A storybook stone fortress crowned with four conical-roofed towers flying gold pennants, a portcullis gatehouse guarded by marble lion statues, and a banner-hung great hall built around a crystal chandelier, a fire-lit hearth, a grand double stair and a canopied gold throne dais. The rarest land trophy in the Verse.",
		"footprint": [24, 19],
		"floors": 3,
		"attributes": [
			["Style", "Medieval Royal Fortress"],
			["Material", "Grey Ashlar, Marble, Slate & Gold"],
			["Feature", "Crenellated Towers, Portcullis Gatehouse, Lion Statues, Crystal Chandelier & Throne Dais"],
			["Floors", "3 (Great Hall, Battlement Walk, Tower Tops)"],
			["Vibe", "Legendary Royalty"],
		],
	}
