class_name VerseBuildingGrandMansion
extends RefCounted
## Hey Verse — "Belvedere Grand Mansion" (Legendary placeable building NFT).
##
## A symmetric three-storey limestone estate house at the pinnacle of the tier:
## a six-column Corinthian portico crowned by a sculpted pediment, a ribbed
## copper central DOME flanked by dormered mansard wings, gilded pilasters and
## string-courses, balustraded terraces and rooftop urns, marble guardian
## statues, a balconied piano-nobile, a tiered marble fountain, an allée of
## obelisks and clipped parterres, and lantern-lit approaches.
##
## The FRONT wall (+z, the camera side) is OMITTED so the marble entry hall,
## the sweeping double grand staircase, twin crystal chandeliers, the grand
## fireplace, the coffered domed ceiling and the upper galleries all read as a
## clean, walkable, furnishable interior.
##
## Self-contained: it re-declares its own toon / metal / gloss / glass / glow
## material helpers + box / cyl / ball / torus / prism primitive helpers, loads
## the shared res://toon.gdshader + res://outline.gdshader by PATH with
## ResourceLoader guards (StandardMaterial3D fallback), preloads NOTHING, builds
## at the origin with the entrance facing +z, and exposes the contract
## static build() -> Node3D / static meta() -> Dictionary.

# ───────────────────────────── shared state ────────────────────────────────

# One shared inverted-hull outline pass for the whole house (cheap + consistent).
static var _outline_mat: ShaderMaterial

# Typed mirror-pair so `for s: float in SIDES` yields a `float` (never Variant)
# and any derived `var x := s * ...` infers cleanly under strict GDScript.
const SIDES: Array[float] = [-1.0, 1.0]

# Footprint (mansion tier): a generous 16 wide x 13 deep estate.
const W: float = 16.0
const D: float = 13.0
const WALL: float = 0.34       # exterior wall thickness
const FLOOR_H: float = 3.2     # storey height (clear ~3.0 ceilings)

# Palette — cohesive luxury stone-and-gold scheme.
const C_STONE: Color = Color(0.86, 0.84, 0.78)        # warm limestone facade
const C_STONE_DK: Color = Color(0.74, 0.72, 0.66)     # shadowed stone / rustication
const C_MARBLE: Color = Color(0.93, 0.92, 0.90)       # pale marble (interior floor)
const C_MARBLE_DK: Color = Color(0.58, 0.57, 0.60)    # veined marble inlay
const C_GOLD: Color = Color(0.95, 0.74, 0.32)         # brushed gold / brass trim
const C_GOLD_DK: Color = Color(0.78, 0.56, 0.20)      # antique gold / shadowed gilt
const C_COPPER: Color = Color(0.46, 0.66, 0.55)       # patinated copper dome
const C_SLATE: Color = Color(0.30, 0.31, 0.36)        # slate mansard roof
const C_WOOD: Color = Color(0.45, 0.30, 0.20)         # warm walnut (stair / doors)
const C_WOOD_DK: Color = Color(0.32, 0.21, 0.14)
const C_RUNNER: Color = Color(0.62, 0.13, 0.16)       # crimson stair runner
const C_GLOW: Color = Color(1.0, 0.86, 0.55)          # warm window / lantern glow
const C_WATER: Color = Color(0.42, 0.74, 0.86)
const C_HEDGE: Color = Color(0.24, 0.46, 0.26)        # clipped topiary
const C_HEDGE_LT: Color = Color(0.30, 0.52, 0.30)
const C_FIRE: Color = Color(1.0, 0.55, 0.2)           # hearth flame


# ───────────────────────────── material helpers ────────────────────────────

static func _outline() -> ShaderMaterial:
	if _outline_mat == null:
		var sh: String = "res://outline.gdshader"
		if ResourceLoader.exists(sh):
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = load(sh)
	return _outline_mat


## The cel material every matte surface uses. Falls back to a plain
## StandardMaterial3D if the toon shader is unavailable (standalone parse).
static func _toon(c: Color, rim: float = 0.30, spec: float = 0.0, outline: bool = true) -> Material:
	var sh: String = "res://toon.gdshader"
	if ResourceLoader.exists(sh):
		var m: ShaderMaterial = ShaderMaterial.new()
		m.shader = load(sh)
		m.set_shader_parameter("albedo", c)
		m.set_shader_parameter("rim_strength", rim)
		m.set_shader_parameter("spec_strength", spec)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		if outline:
			var o: ShaderMaterial = _outline()
			if o != null:
				m.next_pass = o
		return m
	# Fallback — no shader present.
	var f: StandardMaterial3D = StandardMaterial3D.new()
	f.albedo_color = c
	f.roughness = 0.9
	return f


## A real metal — gold / brass / chrome / copper. PBR so it glints; the outline
## still wraps it so it stays in the toon family.
static func _metal(c: Color, rough: float = 0.30, metallic: float = 1.0) -> StandardMaterial3D:
	var m: StandardMaterial3D = StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.metallic_specular = 0.8
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	var o: ShaderMaterial = _outline()
	if o != null:
		m.next_pass = o
	return m


## Glossy dielectric — polished marble, lacquered wood, glazed tile.
static func _gloss(c: Color, rough: float = 0.16) -> StandardMaterial3D:
	var m: StandardMaterial3D = StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.0
	m.roughness = rough
	m.metallic_specular = 0.85
	var o: ShaderMaterial = _outline()
	if o != null:
		m.next_pass = o
	return m


## Translucent glass (window panes, fountain water) — no outline (muddies glass).
static func _glass(c: Color, alpha: float = 0.40) -> StandardMaterial3D:
	var m: StandardMaterial3D = StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.metallic = 0.1
	m.roughness = 0.05
	m.metallic_specular = 0.9
	return m


## Unshaded warm glow — window light, lanterns, hearth, chandelier candles.
static func _glow(c: Color, energy: float = 1.5) -> StandardMaterial3D:
	var m: StandardMaterial3D = StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


# ───────────────────────────── primitive helpers ───────────────────────────

static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO) -> MeshInstance3D:
	var bm: BoxMesh = BoxMesh.new()
	bm.size = size
	var mi: MeshInstance3D = MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO, seg: int = 18) -> MeshInstance3D:
	var cm: CylinderMesh = CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = seg
	var mi: MeshInstance3D = MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, s: Vector3 = Vector3.ONE, seg: int = 22, rings: int = 12) -> MeshInstance3D:
	var sm: SphereMesh = SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = seg
	sm.rings = rings
	var mi: MeshInstance3D = MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = s
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO, seg: int = 14) -> MeshInstance3D:
	var tm: TorusMesh = TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 28
	tm.ring_segments = seg
	var mi: MeshInstance3D = MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _prism(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot: Vector3 = Vector3.ZERO) -> MeshInstance3D:
	var pm: PrismMesh = PrismMesh.new()
	pm.size = size
	var mi: MeshInstance3D = MeshInstance3D.new()
	mi.mesh = pm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


# ───────────────────────────── compound helpers ────────────────────────────

## A fluted classical column with a moulded base and a Corinthian gold cap.
static func _column(parent: Node3D, base_pos: Vector3, height: float, radius: float, stone: Material, gold: Material) -> void:
	var n: Node3D = Node3D.new()
	n.position = base_pos
	parent.add_child(n)
	# stepped plinth
	_box(n, Vector3(radius * 2.7, 0.22, radius * 2.7), stone, Vector3(0, 0.11, 0))
	_box(n, Vector3(radius * 2.3, 0.14, radius * 2.3), stone, Vector3(0, 0.30, 0))
	# shaft (slightly tapered)
	var sh_h: float = height - 0.9
	_cyl(n, radius * 0.92, radius, sh_h, stone, Vector3(0, 0.37 + sh_h * 0.5, 0), Vector3.ZERO, 20)
	# flutes — a ring of thin vertical grooves read as fluting
	var grooves: int = 12
	for i: int in grooves:
		var a: float = TAU * float(i) / float(grooves)
		var gx: float = cos(a) * radius * 0.93
		var gz: float = sin(a) * radius * 0.93
		_box(n, Vector3(0.035, sh_h * 0.96, 0.05), stone, Vector3(gx, 0.37 + sh_h * 0.5, gz), Vector3(0, -a, 0))
	# gold Corinthian capital
	var cap_y: float = 0.37 + sh_h + 0.12
	_cyl(n, radius * 1.35, radius * 0.95, 0.22, gold, Vector3(0, cap_y, 0), Vector3.ZERO, 20)
	_box(n, Vector3(radius * 2.9, 0.14, radius * 2.9), gold, Vector3(0, cap_y + 0.18, 0))
	# acanthus corner curls
	for sx: float in SIDES:
		for sz: float in SIDES:
			_ball(n, radius * 0.34, gold, Vector3(sx * radius * 1.1, cap_y + 0.04, sz * radius * 1.1), Vector3(1.0, 0.7, 1.0), 10, 6)


## A slimmer engaged pilaster (half-column reading) for wall articulation.
static func _pilaster(parent: Node3D, pos: Vector3, height: float, gold: Material, stone: Material) -> void:
	var n: Node3D = Node3D.new()
	n.position = pos
	parent.add_child(n)
	_box(n, Vector3(0.5, 0.18, 0.22), stone, Vector3(0, 0.09, 0))            # base
	_box(n, Vector3(0.34, height - 0.5, 0.16), stone, Vector3(0, height * 0.5, 0))  # shaft
	_box(n, Vector3(0.06, height - 0.6, 0.03), gold, Vector3(0.1, height * 0.5, 0.09))  # gilt fillet
	_box(n, Vector3(0.06, height - 0.6, 0.03), gold, Vector3(-0.1, height * 0.5, 0.09))
	_box(n, Vector3(0.5, 0.16, 0.22), gold, Vector3(0, height - 0.1, 0))     # gilt capital


## A stone baluster (turned vase profile) for parapet / balcony rails.
static func _baluster(parent: Node3D, pos: Vector3, h: float, mat: Material) -> void:
	_box(parent, Vector3(0.22, 0.05, 0.22), mat, pos + Vector3(0, 0.025, 0))
	_cyl(parent, 0.05, 0.10, h * 0.45, mat, pos + Vector3(0, h * 0.27, 0), Vector3.ZERO, 10)
	_ball(parent, 0.11, mat, pos + Vector3(0, h * 0.55, 0), Vector3.ONE, 10, 6)
	_cyl(parent, 0.09, 0.05, h * 0.30, mat, pos + Vector3(0, h * 0.80, 0), Vector3.ZERO, 10)
	_box(parent, Vector3(0.20, 0.05, 0.20), mat, pos + Vector3(0, h - 0.02, 0))


## A run of balusters under a top rail, spanning `length` along the X (or Z) axis.
static func _balustrade(parent: Node3D, center: Vector3, length: float, h: float, along_x: bool, mat: Material) -> void:
	var n: int = int(length / 0.55)
	if n < 2:
		n = 2
	for i: int in n + 1:
		var t: float = -0.5 + float(i) / float(n)
		var off: float = t * length
		var p: Vector3 = center + (Vector3(off, 0, 0) if along_x else Vector3(0, 0, off))
		_baluster(parent, p, h, mat)
	# bottom plinth + top rail
	var rsize: Vector3 = (Vector3(length, 0.10, 0.30) if along_x else Vector3(0.30, 0.10, length))
	_box(parent, rsize, mat, center + Vector3(0, 0.05, 0))
	_box(parent, rsize, mat, center + Vector3(0, h, 0))


## A tall arched window with a glowing pane, gold mullions and a keystone.
static func _window(parent: Node3D, pos: Vector3, w: float, h: float, stone: Material, gold: Material, glow: Material) -> MeshInstance3D:
	var n: Node3D = Node3D.new()
	n.position = pos
	parent.add_child(n)
	# recessed glowing pane
	var pane: MeshInstance3D = _box(n, Vector3(w, h, 0.06), glow, Vector3(0, 0, 0.02))
	# stone surround
	_box(n, Vector3(w + 0.30, 0.16, 0.20), stone, Vector3(0, h * 0.5 + 0.08, 0))      # head
	_box(n, Vector3(w + 0.30, 0.16, 0.20), stone, Vector3(0, -h * 0.5 - 0.08, 0))     # sill
	for sx: float in SIDES:
		_box(n, Vector3(0.16, h + 0.32, 0.20), stone, Vector3(sx * (w * 0.5 + 0.08), 0, 0))  # jambs
	# gold mullion cross
	_box(n, Vector3(0.05, h, 0.10), gold, Vector3(0, 0, 0.05))
	_box(n, Vector3(w, 0.05, 0.10), gold, Vector3(0, 0, 0.05))
	# arched gold transom + keystone
	_torus(n, w * 0.32, w * 0.5, gold, Vector3(0, h * 0.5, 0.04), Vector3(PI * 0.5, 0, 0), 10)
	_prism(n, Vector3(0.24, 0.30, 0.18), stone, Vector3(0, h * 0.5 + 0.22, 0))
	return n


## A glowing wall lantern on a gold bracket.
static func _lantern(parent: Node3D, pos: Vector3, gold: Material) -> void:
	_box(parent, Vector3(0.06, 0.34, 0.06), gold, pos + Vector3(0, 0.17, 0))
	_cyl(parent, 0.0, 0.16, 0.10, gold, pos + Vector3(0, 0.40, 0), Vector3.ZERO, 8)
	_box(parent, Vector3(0.16, 0.22, 0.16), _glass(C_GLOW, 0.5), pos + Vector3(0, 0.27, 0))
	_ball(parent, 0.07, _glow(C_GLOW, 2.4), pos + Vector3(0, 0.27, 0), Vector3.ONE, 8, 5)


## A tall lamp-post / bollard lantern for the approach allée.
static func _lamppost(parent: Node3D, pos: Vector3, gold: Material, stone: Material) -> void:
	_box(parent, Vector3(0.42, 0.30, 0.42), stone, pos + Vector3(0, 0.15, 0))   # plinth
	_cyl(parent, 0.07, 0.11, 2.1, gold, pos + Vector3(0, 1.2, 0), Vector3.ZERO, 10)
	_cyl(parent, 0.0, 0.20, 0.16, gold, pos + Vector3(0, 2.34, 0), Vector3.ZERO, 8)
	_box(parent, Vector3(0.24, 0.30, 0.24), _glass(C_GLOW, 0.45), pos + Vector3(0, 2.55, 0))
	_ball(parent, 0.11, _glow(C_GLOW, 2.6), pos + Vector3(0, 2.55, 0), Vector3.ONE, 8, 5)


## A clipped topiary hedge — a green box with a slightly domed glossy top.
static func _hedge(parent: Node3D, pos: Vector3, size: Vector3) -> void:
	var leaf: Material = _toon(C_HEDGE, 0.22)
	_box(parent, size, leaf, pos + Vector3(0, size.y * 0.5, 0))
	_box(parent, Vector3(size.x * 1.04, 0.10, size.z * 1.04), _toon(C_HEDGE_LT, 0.20), pos + Vector3(0, size.y - 0.02, 0))


## A potted topiary ball in a gold-banded marble urn — flanks doors / stairs.
static func _topiary(parent: Node3D, pos: Vector3, marble: Material, gold: Material) -> void:
	_cyl(parent, 0.22, 0.30, 0.5, marble, pos + Vector3(0, 0.25, 0), Vector3.ZERO, 14)
	_box(parent, Vector3(0.62, 0.08, 0.62), gold, pos + Vector3(0, 0.52, 0))
	_ball(parent, 0.34, _toon(C_HEDGE, 0.22), pos + Vector3(0, 0.92, 0), Vector3.ONE, 14, 8)
	_ball(parent, 0.24, _toon(C_HEDGE_LT, 0.20), pos + Vector3(0, 1.36, 0), Vector3.ONE, 12, 8)


## A standing marble guardian statue on a gold-trimmed plinth (abstracted figure).
static func _statue(parent: Node3D, pos: Vector3, facing: float, marble: Material, gold: Material) -> void:
	var n: Node3D = Node3D.new()
	n.position = pos
	n.rotation.y = facing
	parent.add_child(n)
	# stepped plinth
	_box(n, Vector3(1.0, 0.24, 1.0), _gloss(C_MARBLE_DK, 0.2), Vector3(0, 0.12, 0))
	_box(n, Vector3(0.78, 0.7, 0.78), marble, Vector3(0, 0.6, 0))
	_box(n, Vector3(0.9, 0.1, 0.9), gold, Vector3(0, 1.0, 0))                  # gilt cap band
	# robed body (tapered cylinder), torso, head, raised arm
	_cyl(n, 0.20, 0.34, 1.2, marble, Vector3(0, 1.7, 0), Vector3.ZERO, 14)     # gown
	_ball(n, 0.26, marble, Vector3(0, 2.4, 0), Vector3(1.0, 1.1, 0.7), 14, 8)  # torso
	_ball(n, 0.17, marble, Vector3(0, 2.78, 0), Vector3.ONE, 12, 8)            # head
	_cyl(n, 0.06, 0.07, 0.7, marble, Vector3(0.28, 2.5, 0.05), Vector3(0, 0, -0.7), 8)  # raised arm
	_ball(n, 0.10, _glow(C_GLOW, 1.6), Vector3(0.56, 2.78, 0.05), Vector3.ONE, 8, 5)    # held light
	_cyl(n, 0.06, 0.07, 0.55, marble, Vector3(-0.22, 2.3, 0.06), Vector3(0, 0, 0.4), 8) # lowered arm


## A reclining lion guardian on a pedestal — flanks the grand stair / entry.
static func _lion(parent: Node3D, pos: Vector3, facing: float, marble: Material, gold: Material) -> void:
	var n: Node3D = Node3D.new()
	n.position = pos
	n.rotation.y = facing
	parent.add_child(n)
	_box(n, Vector3(1.5, 0.5, 0.8), _gloss(C_MARBLE_DK, 0.2), Vector3(0, 0.25, 0))   # pedestal
	_box(n, Vector3(1.4, 0.06, 0.74), gold, Vector3(0, 0.53, 0))                     # gilt band
	# body + haunch + forelegs + chest + maned head
	_ball(n, 0.45, marble, Vector3(0, 0.95, -0.1), Vector3(1.5, 0.85, 0.9), 14, 8)   # body
	_ball(n, 0.32, marble, Vector3(-0.55, 0.85, -0.1), Vector3(1.0, 1.0, 0.9), 12, 8) # haunch
	_cyl(n, 0.10, 0.12, 0.55, marble, Vector3(0.5, 0.7, 0.18), Vector3.ZERO, 8)      # foreleg
	_cyl(n, 0.10, 0.12, 0.55, marble, Vector3(0.5, 0.7, -0.18), Vector3.ZERO, 8)
	_box(n, Vector3(0.5, 0.1, 0.5), marble, Vector3(0.6, 0.45, 0))                   # paws
	_ball(n, 0.26, marble, Vector3(0.62, 1.25, 0), Vector3(0.9, 1.0, 1.0), 12, 8)    # mane
	_ball(n, 0.17, marble, Vector3(0.78, 1.28, 0), Vector3(1.0, 0.9, 0.9), 10, 6)    # muzzle
	_box(n, Vector3(0.3, 0.06, 0.06), marble, Vector3(-0.95, 0.95, 0), Vector3(0, 0, 0.4))  # tail


## A slender stone obelisk with a gilded apex — formal allée markers.
static func _obelisk(parent: Node3D, pos: Vector3, stone: Material, gold: Material) -> void:
	_box(parent, Vector3(0.7, 0.3, 0.7), _gloss(C_MARBLE_DK, 0.2), pos + Vector3(0, 0.15, 0))
	_box(parent, Vector3(0.5, 0.25, 0.5), stone, pos + Vector3(0, 0.42, 0))
	_prism(parent, Vector3(0.4, 2.6, 0.4), stone, pos + Vector3(0, 1.85, 0))
	_prism(parent, Vector3(0.4, 0.5, 0.4), gold, pos + Vector3(0, 3.4, 0))      # gilt pyramidion


# ═══════════════════════════════ BUILD ═════════════════════════════════════

static func build() -> Node3D:
	var root: Node3D = Node3D.new()
	root.name = "GrandMansion"

	var stone: Material = _toon(C_STONE, 0.26)
	var stone_dk: Material = _toon(C_STONE_DK, 0.24)
	var marble: Material = _gloss(C_MARBLE, 0.14)
	var marble_dk: Material = _gloss(C_MARBLE_DK, 0.18)
	var gold: Material = _metal(C_GOLD, 0.26, 1.0)
	var gold_dk: Material = _metal(C_GOLD_DK, 0.34, 1.0)
	var copper: Material = _toon(C_COPPER, 0.28, 0.1)
	var slate: Material = _toon(C_SLATE, 0.30)
	var wood: Material = _gloss(C_WOOD, 0.30)
	var wood_dk: Material = _gloss(C_WOOD_DK, 0.34)
	var runner: Material = _toon(C_RUNNER, 0.22)
	var glow: Material = _glow(C_GLOW, 1.6)

	_build_grounds(root, stone, stone_dk, marble, marble_dk, gold, glow)
	_build_shell(root, stone, stone_dk, marble, gold, gold_dk, glow)
	_build_portico(root, stone, stone_dk, marble, gold, glow)
	_build_roof(root, slate, copper, gold, stone)
	_build_interior(root, marble, marble_dk, wood, wood_dk, runner, gold, gold_dk, glow, stone)
	_build_windows_and_lanterns(root, stone, gold, glow)

	return root


# ─────────────────────────────── grounds ───────────────────────────────────

static func _build_grounds(root: Node3D, stone: Material, stone_dk: Material, marble: Material, marble_dk: Material, gold: Material, glow: Material) -> void:
	var g: Node3D = Node3D.new()
	g.name = "Grounds"
	root.add_child(g)

	# Estate terrace platform the house sits on (raises it for grandeur).
	_box(g, Vector3(W + 6.0, 0.30, D + 7.0), stone_dk, Vector3(0, -0.15, 1.5))
	_box(g, Vector3(W + 4.6, 0.20, D + 5.4), stone, Vector3(0, -0.05, 1.5))
	# gilt rim band around the terrace edge
	_box(g, Vector3(W + 4.7, 0.05, 0.18), gold, Vector3(0, 0.06, 1.5 + (D + 5.4) * 0.5))

	# Formal lawn parterre between terrace and forecourt (a green carpet).
	_box(g, Vector3(W + 1.0, 0.04, 6.0), _toon(C_HEDGE_LT, 0.18), Vector3(0, 0.02, D * 0.5 + 5.0))

	# Approach path of marble pavers leading to the steps (+z forecourt).
	for i: int in 8:
		var pz: float = D * 0.5 + 1.6 + float(i) * 1.4
		_box(g, Vector3(3.4, 0.06, 1.1), marble_dk, Vector3(0, 0.05, pz))
		_box(g, Vector3(2.6, 0.07, 0.9), marble, Vector3(0, 0.055, pz))

	# Clipped parterre hedges flanking the path + potted topiary rhythm.
	for sx: float in SIDES:
		for i: int in 4:
			var hz: float = D * 0.5 + 2.4 + float(i) * 1.5
			_hedge(g, Vector3(sx * 2.9, 0.05, hz), Vector3(0.9, 0.7, 0.9))
		# longer side run hugging the terrace edge
		_hedge(g, Vector3(sx * (W * 0.5 + 1.7), 0.05, 0.0), Vector3(0.8, 0.8, D * 0.7))
		# topiary urns marking the foot of the approach
		_topiary(g, Vector3(sx * 2.2, 0.05, D * 0.5 + 1.4), marble, gold)

	# Allée of obelisks + lamp-posts marking the formal approach.
	for sx: float in SIDES:
		for i: int in 3:
			var oz: float = D * 0.5 + 3.4 + float(i) * 2.2
			_obelisk(g, Vector3(sx * 4.7, 0.05, oz), stone, gold)
			_lamppost(g, Vector3(sx * 4.7, 0.05, oz + 1.1), gold, stone)

	# Grand tiered fountain centered on the forecourt.
	var fx: Vector3 = Vector3(0, 0, D * 0.5 + 7.4)
	_build_fountain(g, fx, stone, gold)

	# Reflecting pool flanking the far approach.
	_box(g, Vector3(2.0, 0.06, 4.0), _glass(C_WATER, 0.55), Vector3(0, 0.06, D * 0.5 + 11.6))
	_box(g, Vector3(2.4, 0.1, 4.4), stone, Vector3(0, 0.0, D * 0.5 + 11.6))

	# Guardian statues on tall pedestals flanking the foot of the grand steps.
	for sx: float in SIDES:
		_statue(g, Vector3(sx * 4.5, 0.05, D * 0.5 + 0.6), -sx * 0.4, marble, gold)

	# Entry gate posts with gold orbs + glowing finials, way out front.
	for sx: float in SIDES:
		var gp: Vector3 = Vector3(sx * 3.4, 0, D * 0.5 + 13.8)
		_box(g, Vector3(0.5, 1.9, 0.5), stone, gp + Vector3(0, 0.95, 0))
		_box(g, Vector3(0.66, 0.18, 0.66), stone_dk, gp + Vector3(0, 1.9, 0))
		_ball(g, 0.26, gold, gp + Vector3(0, 2.18, 0), Vector3.ONE, 14, 8)
		_ball(g, 0.09, _glow(C_GLOW, 2.6), gp + Vector3(0, 2.5, 0), Vector3.ONE, 8, 5)
	# gold gate scrollwork spanning the posts
	_box(g, Vector3(6.2, 0.1, 0.08), gold, Vector3(0, 2.0, D * 0.5 + 13.8))
	for i: int in 9:
		var tx: float = -2.6 + float(i) * 0.65
		_cyl(g, 0.03, 0.03, 1.4, gold, Vector3(tx, 1.3, D * 0.5 + 13.8), Vector3.ZERO, 6)


static func _build_fountain(parent: Node3D, center: Vector3, stone: Material, gold: Material) -> void:
	var f: Node3D = Node3D.new()
	f.position = center
	parent.add_child(f)
	var water: Material = _glass(C_WATER, 0.55)
	var waterglow: Material = _glow(C_WATER.lightened(0.2), 0.8)
	# lower basin
	_cyl(f, 2.2, 2.4, 0.5, stone, Vector3(0, 0.25, 0), Vector3.ZERO, 28)
	_cyl(f, 2.0, 2.0, 0.12, water, Vector3(0, 0.46, 0), Vector3.ZERO, 28)
	# baluster ring around basin rim
	var ring: int = 16
	for i: int in ring:
		var a: float = TAU * float(i) / float(ring)
		_baluster(f, Vector3(cos(a) * 2.3, 0.5, sin(a) * 2.3), 0.5, stone)
	# central pedestal + mid basin
	_cyl(f, 0.5, 0.7, 1.1, stone, Vector3(0, 1.05, 0), Vector3.ZERO, 18)
	_cyl(f, 1.1, 1.0, 0.34, stone, Vector3(0, 1.7, 0), Vector3.ZERO, 22)
	_cyl(f, 0.95, 0.95, 0.10, water, Vector3(0, 1.88, 0), Vector3.ZERO, 22)
	# top finial — gold urn with a glowing jet
	_cyl(f, 0.18, 0.34, 0.9, stone, Vector3(0, 2.35, 0), Vector3.ZERO, 14)
	_cyl(f, 0.30, 0.16, 0.30, gold, Vector3(0, 2.9, 0), Vector3.ZERO, 14)
	_ball(f, 0.14, gold, Vector3(0, 3.12, 0), Vector3.ONE, 12, 8)
	# water jet + falling arcs
	_cyl(f, 0.04, 0.06, 0.8, waterglow, Vector3(0, 3.5, 0), Vector3.ZERO, 8)
	for i: int in 8:
		var a: float = TAU * float(i) / 8.0
		_cyl(f, 0.025, 0.04, 0.7, water, Vector3(cos(a) * 0.5, 3.2, sin(a) * 0.5), Vector3(0.5, -a, 0), 6)


# ────────────────────────────── shell / walls ──────────────────────────────

static func _build_shell(root: Node3D, stone: Material, stone_dk: Material, marble: Material, gold: Material, gold_dk: Material, glow: Material) -> void:
	var s: Node3D = Node3D.new()
	s.name = "Shell"
	root.add_child(s)

	var total_h: float = FLOOR_H * 3.0
	var hw: float = W * 0.5
	var hd: float = D * 0.5

	# Interior marble floors per storey (kept clear/walkable).
	for fl: int in 3:
		var fy: float = float(fl) * FLOOR_H
		_box(s, Vector3(W - WALL * 1.6, 0.12, D - WALL * 1.6), marble, Vector3(0, fy + 0.06, 0))
		# subtle veined marble border inlay along the back
		_box(s, Vector3(W - WALL * 1.6, 0.13, 0.5), _gloss(C_MARBLE_DK, 0.16), Vector3(0, fy + 0.065, -hd + WALL + 0.4))

	# Rusticated ground-floor base course (banded stone) on the 3 solid walls.
	# Back wall (−z) solid full height.
	_box(s, Vector3(W, total_h, WALL), stone, Vector3(0, total_h * 0.5, -hd))
	# Side walls (±x) solid full height.
	for sx: float in SIDES:
		_box(s, Vector3(WALL, total_h, D), stone, Vector3(sx * hw, total_h * 0.5, 0))

	# Rustication bands across the ground storey of the side + back walls.
	for i: int in 5:
		var ry: float = 0.4 + float(i) * 0.5
		_box(s, Vector3(W + 0.04, 0.05, WALL + 0.04), stone_dk, Vector3(0, ry, -hd))
		for sx: float in SIDES:
			_box(s, Vector3(WALL + 0.04, 0.05, D + 0.04), stone_dk, Vector3(sx * hw, ry, 0))

	# FRONT (+z) is OMITTED for the camera. Keep only slim corner piers + a low
	# threshold parapet so the interior reads as open but framed.
	for sx: float in SIDES:
		_box(s, Vector3(WALL * 1.4, total_h, WALL * 1.4), stone, Vector3(sx * (hw - 0.1), total_h * 0.5, hd))
	_box(s, Vector3(W, 0.5, WALL), stone_dk, Vector3(0, 0.25, hd))   # low threshold sill

	# Quoins (corner stones) on the back corners for that estate look.
	for sx: float in SIDES:
		for i: int in 9:
			var qy: float = 0.5 + float(i) * (total_h - 1.0) / 9.0
			var qoff: float = 0.18 if (i % 2 == 0) else 0.34
			_box(s, Vector3(qoff * 2.0, 0.5, WALL + 0.06), stone_dk, Vector3(sx * (hw - qoff), qy, -hd))

	# Giant-order gilt pilasters articulating the side + back walls.
	for sx: float in SIDES:
		for pz: float in [-3.2, 3.2]:
			var pn: Node3D = Node3D.new()
			pn.position = Vector3(sx * (hw - 0.01), 0, pz)
			pn.rotation.y = sx * PI * 0.5
			s.add_child(pn)
			_pilaster(pn, Vector3.ZERO, FLOOR_H * 2.0 + 0.4, gold, stone)
	for bx: float in [-4.5, 4.5]:
		_pilaster(s, Vector3(bx, 0, -hd + WALL * 0.5 + 0.02), FLOOR_H * 2.0 + 0.4, gold, stone)

	# String courses (horizontal belt mouldings) between floors, wrapping sides+back.
	for fl: int in range(1, 4):
		var by: float = float(fl) * FLOOR_H - 0.05
		_box(s, Vector3(W + 0.12, 0.18, 0.06), gold, Vector3(0, by, -hd - 0.02))
		for sx: float in SIDES:
			_box(s, Vector3(0.06, 0.18, D + 0.12), gold, Vector3(sx * (hw + 0.03), by, 0))

	# Grand cornice crown along the roofline of the solid walls (dentil + ovolo).
	_box(s, Vector3(W + 0.7, 0.4, 0.5), stone_dk, Vector3(0, total_h + 0.1, -hd - 0.05))
	_box(s, Vector3(W + 0.74, 0.1, 0.55), gold_dk, Vector3(0, total_h - 0.12, -hd - 0.07))
	for sx: float in SIDES:
		_box(s, Vector3(0.5, 0.4, D + 0.7), stone_dk, Vector3(sx * (hw + 0.1), total_h + 0.1, 0))
		_box(s, Vector3(0.55, 0.1, D + 0.74), gold_dk, Vector3(sx * (hw + 0.12), total_h - 0.12, 0))

	# Roof parapet balustrade around the wings (read as a usable rooftop edge).
	var pr_y: float = total_h + 0.34
	_balustrade(s, Vector3(0, pr_y, -hd - 0.1), W + 0.2, 0.7, true, marble)
	for sx: float in SIDES:
		_balustrade(s, Vector3(sx * (hw + 0.15), pr_y, 0), D - 0.4, 0.7, false, marble)

	# Decorative urns + glowing finials punctuating the parapet corners.
	for sx: float in SIDES:
		for sz: float in SIDES:
			var up: Vector3 = Vector3(sx * (hw - 0.2), pr_y + 0.4, sz * (hd - 0.2))
			_cyl(s, 0.18, 0.30, 0.5, marble, up, Vector3.ZERO, 12)
			_ball(s, 0.20, gold, up + Vector3(0, 0.4, 0), Vector3(1.0, 1.2, 1.0), 12, 8)
			_ball(s, 0.07, _glow(C_GLOW, 2.2), up + Vector3(0, 0.7, 0), Vector3.ONE, 8, 5)


# ────────────────────────────── grand portico ──────────────────────────────

static func _build_portico(root: Node3D, stone: Material, stone_dk: Material, marble: Material, gold: Material, glow: Material) -> void:
	var p: Node3D = Node3D.new()
	p.name = "Portico"
	root.add_child(p)
	var hd: float = D * 0.5
	var pz: float = hd + 2.4          # portico projects in front of the omitted facade

	# Grand entrance steps up to the threshold (full width, several treads).
	for i: int in 5:
		var sw: float = 7.4 - float(i) * 0.5
		var tread_mat: Material = marble if (i % 2 == 0) else _gloss(C_MARBLE_DK, 0.18)
		_box(p, Vector3(sw, 0.18, 0.55), tread_mat, Vector3(0, 0.09 + float(i) * 0.18, hd + 1.6 - float(i) * 0.5))
	# cheek walls flanking the steps, with gold-banded plinths
	for sx: float in SIDES:
		_box(p, Vector3(0.4, 1.1, 2.6), stone, Vector3(sx * 3.7, 0.55, hd + 1.0))
		_box(p, Vector3(0.6, 0.14, 0.6), gold, Vector3(sx * 3.7, 1.1, hd + 2.2))
		_ball(p, 0.2, gold, Vector3(sx * 3.7, 1.4, hd + 2.2), Vector3.ONE, 12, 8)

	# Six-column Corinthian portico (4 across the front + 2 returns).
	var col_h: float = FLOOR_H * 2.0 + 0.2
	var col_xs: Array[float] = [-3.1, -1.05, 1.05, 3.1]
	for cx: float in col_xs:
		_column(p, Vector3(cx, 0.95, pz), col_h, 0.34, stone, gold)
	# return columns nearer the wall
	for cx2: float in [-3.1, 3.1]:
		_column(p, Vector3(cx2, 0.95, hd + 0.7), col_h, 0.30, stone, gold)

	# Entablature beam the columns carry, with a triglyph/gold frieze.
	var ent_y: float = 0.95 + col_h + 0.2
	_box(p, Vector3(8.0, 0.5, 4.0), stone, Vector3(0, ent_y, hd + 1.6))
	_box(p, Vector3(8.4, 0.2, 4.2), gold, Vector3(0, ent_y + 0.32, hd + 1.6))     # gold fascia band
	for i: int in 9:
		var gx: float = -3.6 + float(i) * 0.9
		_box(p, Vector3(0.14, 0.3, 0.06), gold, Vector3(gx, ent_y + 0.05, pz + 2.02))  # frieze rosettes

	# Triangular pediment with gold cornice + a glowing crest medallion.
	_prism(p, Vector3(8.4, 1.7, 4.2), stone_dk, Vector3(0, ent_y + 1.2, hd + 1.6))
	_prism(p, Vector3(7.2, 1.4, 0.2), stone, Vector3(0, ent_y + 1.15, pz + 1.9))   # tympanum face
	# pediment raking cornices in gold
	for sx: float in SIDES:
		_box(p, Vector3(0.18, 4.6, 0.18), gold, Vector3(sx * 2.0, ent_y + 1.2, pz + 1.95), Vector3(0, 0, sx * -0.62))
	# crest sunburst medallion + reclining tympanum figures
	_cyl(p, 0.0, 0.55, 0.18, gold, Vector3(0, ent_y + 1.0, pz + 2.0), Vector3(PI * 0.5, 0, 0), 16)
	_ball(p, 0.30, _glow(C_GLOW, 2.2), Vector3(0, ent_y + 1.0, pz + 2.12), Vector3.ONE, 16, 10)
	for sx: float in SIDES:
		_ball(p, 0.26, marble, Vector3(sx * 1.5, ent_y + 0.7, pz + 2.0), Vector3(1.4, 0.7, 0.5), 10, 6)
	# acroteria finials at the pediment apex + corners
	_ball(p, 0.22, gold, Vector3(0, ent_y + 2.05, hd + 1.6), Vector3.ONE, 12, 8)
	for sx: float in SIDES:
		_cyl(p, 0.0, 0.16, 0.5, gold, Vector3(sx * 4.2, ent_y + 0.6, hd + 1.6), Vector3.ZERO, 8)

	# Piano-nobile BALCONY over the entrance, with French doors + balustrade.
	var bal_y: float = FLOOR_H + 1.4
	_box(p, Vector3(5.4, 0.2, 1.6), marble, Vector3(0, bal_y, hd + 0.6))           # balcony slab
	# gold console brackets under the balcony
	for sx: float in SIDES:
		_prism(p, Vector3(0.4, 0.5, 0.5), gold, Vector3(sx * 2.0, bal_y - 0.4, hd + 0.5), Vector3(PI, 0, 0))
	_balustrade(p, Vector3(0, bal_y + 0.1, hd + 1.3), 5.0, 0.7, true, marble)
	# French doors with glowing panes onto the balcony (in the omitted facade gap)
	for sx: float in SIDES:
		_box(p, Vector3(0.9, 2.1, 0.08), _gloss(C_WOOD_DK, 0.3), Vector3(sx * 0.6, bal_y + 1.15, hd + 0.05))
		_box(p, Vector3(0.66, 1.8, 0.05), _glow(C_GLOW, 1.2), Vector3(sx * 0.6, bal_y + 1.2, hd + 0.1))
		_box(p, Vector3(0.7, 0.04, 0.06), gold, Vector3(sx * 0.6, bal_y + 1.2, hd + 0.12))
	_torus(p, 0.7, 1.1, gold, Vector3(0, bal_y + 2.1, hd + 0.08), Vector3(PI * 0.5, 0, 0), 12)

	# Grand double doors set in the threshold gap (open-ish, walnut + gold).
	_build_doors(p, stone, gold, Vector3(0, 0, hd - 0.05))


static func _build_doors(parent: Node3D, stone: Material, gold: Material, at: Vector3) -> void:
	var d: Node3D = Node3D.new()
	d.position = at
	parent.add_child(d)
	var wood: Material = _gloss(C_WOOD_DK, 0.30)
	# stone door frame + arched fanlight
	for sx: float in SIDES:
		_box(d, Vector3(0.28, 2.6, 0.4), stone, Vector3(sx * 1.35, 1.3, 0))
	_box(d, Vector3(3.0, 0.3, 0.4), stone, Vector3(0, 2.65, 0))
	_torus(d, 0.7, 1.2, gold, Vector3(0, 2.6, 0.1), Vector3(PI * 0.5, 0, 0), 12)
	_box(d, Vector3(2.0, 0.7, 0.06), _glow(C_GLOW, 1.4), Vector3(0, 2.95, 0.04))   # glowing fanlight
	# two leaves, slightly ajar (welcoming, doesn't block the walk-in)
	for sx: float in SIDES:
		var leaf: Node3D = Node3D.new()
		leaf.position = Vector3(sx * 0.62, 0, 0)
		leaf.rotation.y = sx * 0.30
		d.add_child(leaf)
		_box(leaf, Vector3(1.1, 2.2, 0.10), wood, Vector3(sx * -0.55, 1.1, 0))
		# raised panels
		for py: float in [0.6, 1.5]:
			_box(leaf, Vector3(0.7, 0.5, 0.04), _gloss(C_WOOD, 0.30), Vector3(sx * -0.55, py, 0.06))
		# gold ring handle
		_torus(leaf, 0.05, 0.12, gold, Vector3(sx * -0.05, 1.1, 0.1), Vector3(PI * 0.5, 0, 0), 10)


# ──────────────────────────── roof: dome + mansard ─────────────────────────

static func _build_roof(root: Node3D, slate: Material, copper: Material, gold: Material, stone: Material) -> void:
	var r: Node3D = Node3D.new()
	r.name = "Roof"
	root.add_child(r)
	var top: float = FLOOR_H * 3.0 + 0.5
	var hw: float = W * 0.5

	# Mansard roof over the two wings (steep slate slopes + dormers).
	for sx: float in SIDES:
		var cx: float = sx * (hw * 0.55)
		# steep lower mansard slope (a wide flat-topped band)
		_box(r, Vector3(hw * 0.78, 1.3, D - 0.4), slate, Vector3(cx, top + 0.65, 0))
		_box(r, Vector3(hw * 0.66, 0.2, D - 0.6), gold, Vector3(cx, top + 1.35, 0))   # ridge band
		# dormer windows on the slate (3 per wing, facing +z)
		for i: int in 3:
			var dz: float = -D * 0.28 + float(i) * D * 0.28
			_box(r, Vector3(0.8, 0.8, 0.45), slate, Vector3(cx, top + 0.5, dz + D * 0.18))
			_box(r, Vector3(0.5, 0.55, 0.1), _glow(C_GLOW, 1.4), Vector3(cx, top + 0.5, dz + D * 0.18 + 0.2))
			_box(r, Vector3(0.06, 0.55, 0.1), gold, Vector3(cx, top + 0.5, dz + D * 0.18 + 0.22))
			_prism(r, Vector3(0.9, 0.45, 0.55), copper, Vector3(cx, top + 1.05, dz + D * 0.18))
			_ball(r, 0.07, _glow(C_GLOW, 2.0), Vector3(cx, top + 1.4, dz + D * 0.18), Vector3.ONE, 8, 5)

	# Central drum + grand copper DOME crowning the entry hall.
	var drum_y: float = top + 0.4
	_cyl(r, 2.9, 3.1, 1.4, stone, Vector3(0, drum_y + 0.7, 0), Vector3.ZERO, 28)
	# colonnade of little columns around the drum (peristyle)
	var dn: int = 16
	for i: int in dn:
		var a: float = TAU * float(i) / float(dn)
		_cyl(r, 0.10, 0.10, 1.0, stone, Vector3(cos(a) * 2.95, drum_y + 0.7, sin(a) * 2.95), Vector3.ZERO, 8)
		_box(r, Vector3(0.2, 0.1, 0.2), gold, Vector3(cos(a) * 2.95, drum_y + 1.25, sin(a) * 2.95))  # caps
	# round-headed drum windows, glowing between the peristyle columns
	for i: int in 8:
		var a2: float = TAU * float(i) / 8.0 + TAU / 16.0
		_box(r, Vector3(0.4, 0.7, 0.1), _glow(C_GLOW, 1.2), Vector3(cos(a2) * 3.0, drum_y + 0.7, sin(a2) * 3.0), Vector3(0, -a2, 0))
	_box(r, Vector3(6.6, 0.2, 6.6), gold, Vector3(0, drum_y + 1.45, 0))
	# the dome — a half-ball, copper-patina with gold ribs
	var dome_base: float = drum_y + 1.5
	_ball(r, 2.95, copper, Vector3(0, dome_base, 0), Vector3(1.0, 0.78, 1.0), 28, 14)
	# gold meridian ribs (boxes leaning toward the apex)
	var ribs: int = 12
	for i: int in ribs:
		var a3: float = TAU * float(i) / float(ribs)
		_box(r, Vector3(0.06, 2.4, 0.06), gold, Vector3(cos(a3) * 1.9, dome_base + 0.7, sin(a3) * 1.9), Vector3(0, -a3, 0))
	# gold ring at the dome spring line
	_torus(r, 2.9, 3.05, gold, Vector3(0, dome_base + 0.05, 0), Vector3(PI * 0.5, 0, 0), 24)
	# gold lantern cupola + finial on top of the dome
	_cyl(r, 0.7, 0.8, 0.9, gold, Vector3(0, dome_base + 2.5, 0), Vector3.ZERO, 14)
	_box(r, Vector3(1.4, 0.7, 1.4), _glow(C_GLOW, 1.8), Vector3(0, dome_base + 2.5, 0))
	_cyl(r, 0.5, 0.7, 0.5, gold, Vector3(0, dome_base + 3.1, 0), Vector3.ZERO, 14)
	_cyl(r, 0.0, 0.4, 0.7, gold, Vector3(0, dome_base + 3.55, 0), Vector3.ZERO, 14)
	_ball(r, 0.22, gold, Vector3(0, dome_base + 4.05, 0), Vector3.ONE, 14, 8)
	_ball(r, 0.10, _glow(C_GLOW, 3.0), Vector3(0, dome_base + 4.3, 0), Vector3.ONE, 8, 5)

	# A pair of slender stone chimneys at the back corners.
	for sx: float in SIDES:
		var cp: Vector3 = Vector3(sx * (hw - 1.2), top + 1.4, -D * 0.5 + 0.9)
		_box(r, Vector3(0.8, 1.8, 0.8), stone, cp)
		_box(r, Vector3(1.0, 0.3, 1.0), gold, cp + Vector3(0, 1.0, 0))
		for px: float in SIDES:
			for pz: float in SIDES:
				_cyl(r, 0.09, 0.10, 0.4, slate, cp + Vector3(px * 0.22, 1.3, pz * 0.22), Vector3.ZERO, 8)


# ───────────────────────── walkable luxury interior ────────────────────────

static func _build_interior(root: Node3D, marble: Material, marble_dk: Material, wood: Material, wood_dk: Material, runner: Material, gold: Material, gold_dk: Material, glow: Material, stone: Material) -> void:
	var it: Node3D = Node3D.new()
	it.name = "Interior"
	root.add_child(it)
	var hd: float = D * 0.5
	var hw: float = W * 0.5

	# ── Ground-floor: a grand marble entry hall, kept OPEN to furnish. ──
	# Veined marble inlay rug pattern (octagon star) in the floor center.
	_box(it, Vector3(5.0, 0.13, 5.0), marble_dk, Vector3(0, 0.065, -0.5), Vector3(0, PI * 0.25, 0))
	_box(it, Vector3(4.0, 0.14, 4.0), marble, Vector3(0, 0.07, -0.5))
	_box(it, Vector3(2.2, 0.15, 2.2), _gloss(C_GOLD, 0.3), Vector3(0, 0.075, -0.5), Vector3(0, PI * 0.25, 0))
	_box(it, Vector3(0.9, 0.16, 0.9), marble_dk, Vector3(0, 0.08, -0.5), Vector3(0, PI * 0.25, 0))

	# Gilt wall pilasters articulating the interior of the back + side walls.
	for sx: float in SIDES:
		for iz: float in [-2.6, 1.6]:
			var ip: Node3D = Node3D.new()
			ip.position = Vector3(sx * (hw - WALL - 0.1), 0, iz)
			ip.rotation.y = sx * PI * 0.5
			it.add_child(ip)
			_pilaster(ip, Vector3.ZERO, FLOOR_H - 0.3, gold_dk, marble)

	# Partial interior partition walls (define rooms but stay open + walkable).
	# Two side walls that split the hall from side parlours, with wide arch openings.
	for sx: float in SIDES:
		var wx: float = sx * 3.6
		# wall above a tall arch opening (so you can walk through)
		_box(it, Vector3(0.18, FLOOR_H - 2.4, D - 2.0), marble, Vector3(wx, FLOOR_H - (FLOOR_H - 2.4) * 0.5, -0.8))
		# arch jambs + gold trim
		for jz: float in [-3.4, 2.0]:
			_box(it, Vector3(0.24, 2.3, 0.24), stone, Vector3(wx, 1.15, jz))
		_torus(it, 0.5, 0.9, gold, Vector3(wx, 2.3, -0.7), Vector3(0, PI * 0.5, PI * 0.5), 10)

	# Per-floor ceilings (coffer grid hints) + a coffered gold ceiling rose.
	for fl: int in range(1, 3):
		var cy: float = float(fl) * FLOOR_H - 0.02
		for gx: float in [-4.0, 0.0, 4.0]:
			_box(it, Vector3(0.12, 0.10, D - 2.0), _gloss(C_MARBLE_DK, 0.2), Vector3(gx, cy, -0.5))
		for gz: float in [-3.5, 0.0, 3.5]:
			_box(it, Vector3(W - 2.0, 0.10, 0.12), _gloss(C_MARBLE_DK, 0.2), Vector3(0, cy, gz))
	# domed ceiling rose over the hall (open oculus to the dome glow)
	_torus(it, 1.6, 2.4, gold, Vector3(0, FLOOR_H * 3.0 - 0.1, -0.5), Vector3(PI * 0.5, 0, 0), 16)
	_torus(it, 2.4, 2.9, gold_dk, Vector3(0, FLOOR_H * 3.0 - 0.12, -0.5), Vector3(PI * 0.5, 0, 0), 20)
	_ball(it, 1.5, _glow(C_GLOW, 0.7), Vector3(0, FLOOR_H * 3.0 + 0.4, -0.5), Vector3(1.0, 0.4, 1.0), 20, 10)

	# ── THE SWEEPING DOUBLE GRAND STAIRCASE ──
	# Two curved flights rising from the hall floor up to a first-floor gallery
	# landing at the back, meeting in the middle.
	_build_grand_stair(it, marble, runner, gold, wood_dk)

	# Guardian lions flanking the foot of the grand stair.
	for sx: float in SIDES:
		_lion(it, Vector3(sx * 4.0, 0.1, -0.6), -sx * PI * 0.5, marble, gold)

	# First-floor gallery balcony overlooking the hall (open void in the middle).
	var g1y: float = FLOOR_H
	# gallery floor is a U-ring: leave the center open above the hall
	_box(it, Vector3(W - WALL * 1.6, 0.14, 3.2), marble, Vector3(0, g1y + 0.07, -hd + WALL + 1.7))   # back run
	for sx: float in SIDES:
		_box(it, Vector3(3.2, 0.14, D - WALL * 1.6), marble, Vector3(sx * (hw - 1.9), g1y + 0.07, 0))  # side runs
	# gallery balustrade facing the open void
	_balustrade(it, Vector3(0, g1y + 0.14, -hd + WALL + 3.2), W - 6.6, 0.9, true, marble)
	for sx: float in SIDES:
		_balustrade(it, Vector3(sx * (hw - 3.4), g1y + 0.14, 0.5), D - 5.0, 0.9, false, marble)

	# Second-floor gallery (smaller ring) — reached by a return flight.
	var g2y: float = FLOOR_H * 2.0
	_box(it, Vector3(W - WALL * 1.6, 0.14, 2.6), marble, Vector3(0, g2y + 0.07, -hd + WALL + 1.4))
	for sx: float in SIDES:
		_box(it, Vector3(2.6, 0.14, D - WALL * 1.6), marble, Vector3(sx * (hw - 1.6), g2y + 0.07, 0))
	_balustrade(it, Vector3(0, g2y + 0.14, -hd + WALL + 2.7), W - 6.0, 0.9, true, gold)

	# Straight return stair from the back of floor-1 gallery up to floor-2.
	_build_return_stair(it, Vector3(0, FLOOR_H, -hd + WALL + 2.4), marble, runner, gold)

	# ── Showpiece built-in fixtures (kept against walls, hall stays open). ──
	# Grand marble fireplace on the back wall of the ground floor.
	_build_fireplace(it, Vector3(0, 0, -hd + WALL + 0.5), marble, gold, glow)

	# Twin glowing crystal chandeliers — one in the hall, one on the gallery.
	_build_chandelier(it, Vector3(0, FLOOR_H * 2.0 - 0.2, -0.5), gold, glow)
	_build_chandelier(it, Vector3(0, FLOOR_H - 0.3, -hd + WALL + 1.7), gold, glow)

	# Decorative gold-and-marble plinths with vase finials flanking the entry.
	for sx: float in SIDES:
		var pp: Vector3 = Vector3(sx * 2.4, 0, hd - 1.4)
		_box(it, Vector3(0.7, 1.0, 0.7), marble, pp + Vector3(0, 0.5, 0))
		_box(it, Vector3(0.84, 0.12, 0.84), gold, pp + Vector3(0, 1.0, 0))
		_cyl(it, 0.18, 0.32, 0.7, _gloss(C_MARBLE_DK, 0.2), pp + Vector3(0, 1.4, 0), Vector3.ZERO, 14)
		_ball(it, 0.26, _glow(C_GLOW, 1.2), pp + Vector3(0, 1.85, 0), Vector3.ONE, 14, 8)
		# potted topiary just inside the threshold for warmth
		_topiary(it, Vector3(sx * 5.0, 0.1, hd - 0.9), marble, gold)

	# Gilt console table with a glowing candelabrum against each side parlour wall.
	for sx: float in SIDES:
		var cp2: Vector3 = Vector3(sx * (hw - WALL - 0.5), 0, -3.0)
		_box(it, Vector3(0.5, 0.08, 1.6), _gloss(C_MARBLE_DK, 0.18), cp2 + Vector3(0, 0.92, 0))
		for lz: float in SIDES:
			_cyl(it, 0.05, 0.07, 0.6, gold, cp2 + Vector3(0, 1.26, lz * 0.5), Vector3.ZERO, 8)
			_ball(it, 0.06, _glow(C_GLOW, 2.2), cp2 + Vector3(0, 1.6, lz * 0.5), Vector3.ONE, 8, 5)
		# gilt-framed mirror above the console
		_box(it, Vector3(0.1, 1.4, 1.0), gold, cp2 + Vector3(0, 2.1, 0))
		_box(it, Vector3(0.05, 1.2, 0.8), _glass(Color(0.8, 0.85, 0.9), 0.4), cp2 + Vector3(sx * -0.02, 2.1, 0))


static func _build_grand_stair(parent: Node3D, marble: Material, runner: Material, gold: Material, wood: Material) -> void:
	# Two symmetric curved flights. Each flight: 14 treads swinging from the
	# hall floor (near +z, out to ±x) back and up to the floor-1 gallery (−z).
	var hd: float = D * 0.5
	var steps: int = 14
	for sx: float in SIDES:
		var fl: Node3D = Node3D.new()
		fl.name = "Flight"
		parent.add_child(fl)
		for i: int in steps:
			var t: float = float(i) / float(steps - 1)
			# sweep angle: start splayed to the side, curve inward as it climbs
			var ang: float = sx * (1.15 - t * 0.95)
			var radius: float = 4.2 - t * 1.2
			var x: float = sin(ang) * radius
			var z: float = -1.0 - t * (hd - 1.0)            # climb toward back wall
			var y: float = 0.18 + t * (FLOOR_H - 0.18)
			_box(fl, Vector3(1.7, 0.18, 0.7), marble, Vector3(x, y, z), Vector3(0, ang * 0.8, 0))
			# crimson runner inlay
			_box(fl, Vector3(0.9, 0.05, 0.66), runner, Vector3(x, y + 0.11, z), Vector3(0, ang * 0.8, 0))
			# riser face (supporting stringer fill)
			_box(fl, Vector3(1.6, max(0.05, FLOOR_H * 0.16), 0.12), _gloss(C_MARBLE_DK, 0.2), Vector3(x, y - 0.12, z + 0.3), Vector3(0, ang * 0.8, 0))
			# gold balusters + a flowing handrail (one per couple of steps)
			if i % 2 == 0:
				var bx: float = x + sx * 0.9 * cos(ang)
				var bz: float = z + sx * 0.9 * sin(ang) * -1.0
				_cyl(fl, 0.03, 0.04, 0.85, gold, Vector3(bx, y + 0.45, bz), Vector3.ZERO, 8)
				_ball(fl, 0.06, gold, Vector3(bx, y + 0.9, bz), Vector3.ONE, 8, 5)
				# rail segment
				_box(fl, Vector3(0.45, 0.08, 0.08), gold, Vector3(bx, y + 0.92, bz), Vector3(0, ang * 0.8, -0.18 * sx))
		# carved newel post at the foot of each flight, with a glowing torchère
		var nx: float = sin(sx * 1.15) * 4.2
		_box(fl, Vector3(0.4, 1.3, 0.4), wood, Vector3(nx, 0.65, -1.0))
		_cyl(fl, 0.16, 0.22, 0.2, gold, Vector3(nx, 1.35, -1.0), Vector3.ZERO, 12)
		_ball(fl, 0.18, _glow(C_GLOW, 2.2), Vector3(nx, 1.6, -1.0), Vector3(1.0, 1.3, 1.0), 12, 8)

	# Shared landing where the flights meet at the gallery level (back-center).
	_box(parent, Vector3(4.0, 0.2, 2.0), marble, Vector3(0, FLOOR_H - 0.1, -hd + 1.6))
	_box(parent, Vector3(2.4, 0.06, 1.6), runner, Vector3(0, FLOOR_H + 0.01, -hd + 1.6))
	# a grand arched alcove window behind the landing (glowing)
	_box(parent, Vector3(2.0, 2.4, 0.1), _glow(C_GLOW, 1.2), Vector3(0, FLOOR_H + 1.4, -hd + 0.6))
	_torus(parent, 0.8, 1.2, gold, Vector3(0, FLOOR_H + 2.4, -hd + 0.65), Vector3(PI * 0.5, 0, 0), 12)
	for sx: float in SIDES:
		_box(parent, Vector3(0.16, 2.6, 0.16), gold, Vector3(sx * 1.1, FLOOR_H + 1.4, -hd + 0.62))  # gilt mullions


static func _build_return_stair(parent: Node3D, foot: Vector3, marble: Material, runner: Material, gold: Material) -> void:
	# A straight, narrower flight rising from the floor-1 landing up to floor-2.
	var steps: int = 12
	for i: int in steps:
		var t: float = float(i) / float(steps - 1)
		var y: float = foot.y + 0.18 + t * (FLOOR_H - 0.18)
		var z: float = foot.z + t * 2.2
		_box(parent, Vector3(2.2, 0.18, 0.6), marble, Vector3(foot.x, y, z))
		_box(parent, Vector3(1.1, 0.05, 0.56), runner, Vector3(foot.x, y + 0.11, z))
		if i % 2 == 0:
			for sx: float in SIDES:
				_cyl(parent, 0.03, 0.04, 0.85, gold, Vector3(foot.x + sx * 1.0, y + 0.45, z), Vector3.ZERO, 8)
				_ball(parent, 0.05, gold, Vector3(foot.x + sx * 1.0, y + 0.9, z), Vector3.ONE, 8, 5)


static func _build_fireplace(parent: Node3D, at: Vector3, marble: Material, gold: Material, glow: Material) -> void:
	var f: Node3D = Node3D.new()
	f.position = at
	parent.add_child(f)
	# surround
	_box(f, Vector3(3.0, 2.4, 0.5), marble, Vector3(0, 1.2, 0))
	_box(f, Vector3(1.7, 1.5, 0.6), _gloss(C_MARBLE_DK, 0.2), Vector3(0, 0.85, 0.06))   # firebox recess
	# carved gold pilasters flanking the firebox
	for sx: float in SIDES:
		_box(f, Vector3(0.18, 1.7, 0.1), gold, Vector3(sx * 1.05, 0.95, 0.18))
	# fire glow + logs
	_box(f, Vector3(1.5, 1.2, 0.2), _glow(C_FIRE, 2.0), Vector3(0, 0.75, 0.18))
	for i: int in 3:
		_cyl(f, 0.09, 0.09, 1.0, _gloss(C_WOOD_DK, 0.3), Vector3(-0.3 + float(i) * 0.3, 0.4, 0.22), Vector3(0, 0, PI * 0.5), 8)
	# mantel shelf + gold frieze
	_box(f, Vector3(3.4, 0.22, 0.7), marble, Vector3(0, 2.45, 0.05))
	_box(f, Vector3(3.0, 0.12, 0.55), gold, Vector3(0, 2.28, 0.1))
	# gilt mirror / portrait above
	_box(f, Vector3(1.6, 1.8, 0.1), gold, Vector3(0, 3.5, -0.18))
	_box(f, Vector3(1.3, 1.5, 0.06), _glass(Color(0.8, 0.85, 0.9), 0.4), Vector3(0, 3.5, -0.12))
	# flanking candelabra on the mantel
	for sx: float in SIDES:
		_cyl(f, 0.05, 0.08, 0.5, gold, Vector3(sx * 1.2, 2.75, 0.1), Vector3.ZERO, 8)
		_ball(f, 0.07, _glow(C_GLOW, 2.2), Vector3(sx * 1.2, 3.05, 0.1), Vector3.ONE, 8, 5)


static func _build_chandelier(parent: Node3D, at: Vector3, gold: Material, glow: Material) -> void:
	var c: Node3D = Node3D.new()
	c.position = at
	parent.add_child(c)
	# chain + central boss
	_cyl(c, 0.03, 0.03, 0.6, gold, Vector3(0, 0.5, 0), Vector3.ZERO, 6)
	_ball(c, 0.16, gold, Vector3(0, 0.1, 0), Vector3.ONE, 12, 8)
	# two gold tiers of arms with candle flames + crystal drops
	for tier: int in 2:
		var ty: float = -0.1 - float(tier) * 0.35
		var rr: float = 0.55 - float(tier) * 0.18
		var arms: int = 8 - tier * 2
		_torus(c, rr * 0.85, rr, gold, Vector3(0, ty, 0), Vector3(PI * 0.5, 0, 0), 14)
		for i: int in arms:
			var a: float = TAU * float(i) / float(arms)
			var ax: float = cos(a) * rr
			var az: float = sin(a) * rr
			_cyl(c, 0.02, 0.03, 0.18, gold, Vector3(ax, ty + 0.12, az), Vector3.ZERO, 6)
			_ball(c, 0.05, glow, Vector3(ax, ty + 0.26, az), Vector3(1.0, 1.6, 1.0), 8, 5)   # candle flame
			# faceted crystal drop
			_cyl(c, 0.0, 0.05, 0.16, _glass(Color(0.95, 0.95, 1.0), 0.5), Vector3(ax, ty - 0.12, az), Vector3(PI, 0, 0), 6)
	# warm core glow
	_ball(c, 0.2, _glow(C_GLOW, 1.4), Vector3(0, -0.25, 0), Vector3.ONE, 10, 6)


# ───────────────────── exterior windows + wall lanterns ────────────────────

static func _build_windows_and_lanterns(root: Node3D, stone: Material, gold: Material, glow: Material) -> void:
	var w: Node3D = Node3D.new()
	w.name = "Windows"
	root.add_child(w)
	var hd: float = D * 0.5
	var hw: float = W * 0.5

	# Back wall (−z): a symmetric row of tall glowing windows per floor.
	for fl: int in 3:
		var fy: float = float(fl) * FLOOR_H + 1.5
		for col: int in [-5, -2, 2, 5]:
			_window(w, Vector3(float(col) * 1.0, fy, -hd - WALL * 0.5), 1.0, 1.7, stone, gold, glow)

	# Side walls (±x): windows facing out, per floor.
	for sx: float in SIDES:
		for fl: int in 3:
			var fy2: float = float(fl) * FLOOR_H + 1.5
			for cz: float in [-3.6, 0.0, 3.6]:
				var win: MeshInstance3D = _window(w, Vector3(sx * (hw + WALL * 0.5), fy2, cz), 1.0, 1.7, stone, gold, glow)
				win.rotation.y = sx * PI * 0.5   # face the window outward

	# Wall lanterns flanking the front threshold + along the terrace edge.
	for sx: float in SIDES:
		_lantern(w, Vector3(sx * 2.2, 0.1, hd + 0.4), gold)
		_lantern(w, Vector3(sx * (hw - 0.4), 0.1, hd + 0.2), gold)
		# upper-storey sconces beside the side windows
		for fl: int in range(1, 3):
			_lantern(w, Vector3(sx * (hw + WALL * 0.5 + 0.1), float(fl) * FLOOR_H + 0.8, 1.8), gold)


# ───────────────────────────────── meta ────────────────────────────────────

static func meta() -> Dictionary:
	return {
		"id": "grand_mansion",
		"name": "Belvedere Grand Mansion",
		"tier": "Mansion",
		"rarity": "Legendary",
		"description": "A symmetric three-storey limestone estate crowned by a ribbed copper dome and dormered mansard wings, fronted by a six-column Corinthian portico with a balconied piano nobile, gilt pilasters, balustraded terraces topped by urns, marble guardian statues, lion sentinels, an obelisk allée and a tiered marble fountain — inside, a marble entry hall opens beneath twin crystal chandeliers and a coffered golden dome to a sweeping double grand staircase climbing three furnishable floors.",
		"footprint": [16, 13],
		"floors": 3,
		"attributes": [
			["Style", "Neoclassical Beaux-Arts Estate"],
			["Material", "Limestone, Marble, Copper & Brushed Gold"],
			["Feature", "Double Grand Staircase, Copper Dome & Fountain"],
			["Showpiece", "Twin Chandeliers, Grand Fireplace & Balcony"],
			["Grounds", "Statues, Obelisk Allée & Tiered Fountain"],
			["Floors", "3"],
			["Vibe", "Old-Money Grandeur"],
		],
	}
