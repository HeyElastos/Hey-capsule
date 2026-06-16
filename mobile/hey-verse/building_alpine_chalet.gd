class_name VerseBuildingAlpineChalet
extends RefCounted
## Hey Verse — PREMIUM procedural BUILDING: "Frosthaven Alpine Chalet" (Rare).
##
## A heavy-timber Swiss-alpine chalet on a veined-stone base, crowned by a wide
## snow-pitched gable roof with DEEP overhanging eaves and carved fascia. Two
## floors, carved fretwork balconies with turned balusters, glowing warm
## windows, a roaring stone hearth inside, and a walkable open interior (the
## front +z wall is OMITTED so the owner sees in and walks through to furnish).
##
## LUXURY PASS: stone-pillar gateposts with brass lanterns + carved eagle finial
## statues, a tiered brass-rimmed fountain on the approach, a colonnade of carved
## timber columns under the front balcony, two stone bear sentinel statues
## flanking the entrance, snow-dusted dormers breaking the roof, a brass ridge
## crest, a grand twin-flight staircase, a tiered antler-and-brass chandelier,
## and a marble-and-brass showpiece fireplace — gold/brass used sparingly so the
## timber-and-stone material story still reads as the hero.
##
## Sold as an NFT and PLACED on a player's land. Built at the ORIGIN, ground
## floor at y=0, entrance facing +z, scaled for the ~1.4-unit chibi-robot avatar
## (doors ~2.2, ceilings ~3.0, windows ~1.4). ~420 primitives.
##
## SELF-CONTAINED: loads the shared cel + outline shaders by RESOURCE PATH with
## ResourceLoader.exists() guards (StandardMaterial3D fallback), and re-declares
## its own material + primitive helpers, so it parses + runs with NO dependency
## on home.gd / avatar.gd / the catalog modules.

const TOON_PATH := "res://toon.gdshader"
const OUTLINE_PATH := "res://outline.gdshader"

# Lazily-built shared shaders (null = unavailable → fall back to StandardMaterial3D).
static var _toon_shader: Shader
static var _outline_mat: ShaderMaterial
static var _shaders_ready := false

# Typed mirror-pair so `for s: float in SIDES` infers cleanly under strict GDScript.
const SIDES: Array[float] = [-1.0, 1.0]

# ── palette (warm alpine luxury) ────────────────────────────────────────────
const C_TIMBER := Color(0.36, 0.23, 0.13)       # heavy dark-stained beams
const C_TIMBER_LT := Color(0.52, 0.36, 0.21)    # lighter plank infill
const C_STUCCO := Color(0.93, 0.90, 0.83)       # warm white render
const C_STONE := Color(0.49, 0.49, 0.52)        # grey veined base stone
const C_STONE_DK := Color(0.34, 0.34, 0.37)     # mortar / shadow stone
const C_MARBLE := Color(0.90, 0.88, 0.84)       # polished pale marble (showpieces)
const C_ROOF := Color(0.27, 0.20, 0.16)         # weathered timber shingle
const C_SNOW := Color(0.97, 0.98, 1.0)          # snow cap on the roof
const C_GOLD := Color(0.93, 0.74, 0.32)         # brass accent trim
const C_GOLD_DK := Color(0.70, 0.52, 0.20)      # deeper antique-brass shadow
const C_GLOW := Color(1.0, 0.80, 0.45)          # warm window glow
const C_FIRE := Color(1.0, 0.55, 0.18)          # hearth fire
const C_HEDGE := Color(0.20, 0.42, 0.22)        # landscaping green
const C_WATER := Color(0.55, 0.78, 0.88)        # fountain water
const C_FLOOR := Color(0.46, 0.31, 0.19)        # interior plank floor


# ───────────────────────────── shader setup ────────────────────────────────

static func _ensure_shaders() -> void:
	if _shaders_ready:
		return
	_shaders_ready = true
	if ResourceLoader.exists(TOON_PATH):
		var s := ResourceLoader.load(TOON_PATH)
		if s is Shader:
			_toon_shader = s
	if ResourceLoader.exists(OUTLINE_PATH):
		var o := ResourceLoader.load(OUTLINE_PATH)
		if o is Shader:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = o


# ───────────────────────────── material helpers ────────────────────────────

## Matte cel surface (timber, stucco, stone) + inverted-hull outline. Falls back
## to a plain StandardMaterial3D when the shaders are not present.
static func _toon(c: Color, rim := 0.30, outline := true, spec := 0.0) -> Material:
	_ensure_shaders()
	if _toon_shader == null:
		var f := StandardMaterial3D.new()
		f.albedo_color = c
		f.roughness = 0.95
		return f
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


## Real metal (brass / gold / chrome) — PBR so it glints; outline still wraps it.
static func _metal(c: Color, rough := 0.30, metallic := 1.0) -> StandardMaterial3D:
	_ensure_shaders()
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.metallic_specular = 0.78
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	if _outline_mat != null:
		m.next_pass = _outline_mat
	return m


## Glossy lacquer / polished stone / marble — smooth dielectric with a hot
## highlight.
static func _gloss(c: Color, rough := 0.20) -> StandardMaterial3D:
	_ensure_shaders()
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.0
	m.roughness = rough
	m.metallic_specular = 0.85
	if _outline_mat != null:
		m.next_pass = _outline_mat
	return m


## Translucent glass (window panes, fountain water) — no outline (it would muddy
## the glass).
static func _glass(c: Color, alpha := 0.40) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.metallic = 0.1
	m.roughness = 0.05
	m.metallic_specular = 0.9
	return m


## Unshaded glowing material — warm windows, hearth fire, lanterns.
static func _glow(c: Color, energy := 1.5) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


# ───────────────────────────── primitive helpers ───────────────────────────

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


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 14) -> MeshInstance3D:
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


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, s := Vector3.ONE, seg := 16, rings := 8) -> MeshInstance3D:
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


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 10) -> MeshInstance3D:
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


# ───────────────────────────── composite pieces ────────────────────────────

## A vertical turned baluster (slightly tapered post + a bead) for railings.
static func _baluster(parent: Node3D, h: float, mat: Material, pos: Vector3) -> void:
	_cyl(parent, 0.035, 0.05, h, mat, pos + Vector3(0, h * 0.5, 0), Vector3.ZERO, 8)
	_ball(parent, 0.055, mat, pos + Vector3(0, h * 0.55, 0), Vector3.ONE, 8, 4)


## A carved fretwork railing run along X at a given z, top rail + balusters, with
## a thin brass cap line for the luxury read.
static func _railing(parent: Node3D, x0: float, x1: float, z: float, y: float, mat: Material, cap: Material) -> void:
	var span := x1 - x0
	_box(parent, Vector3(absf(span) + 0.1, 0.07, 0.1), mat, Vector3((x0 + x1) * 0.5, y + 0.92, z))
	_box(parent, Vector3(absf(span) + 0.12, 0.03, 0.12), cap, Vector3((x0 + x1) * 0.5, y + 0.965, z))
	_box(parent, Vector3(absf(span) + 0.1, 0.06, 0.12), mat, Vector3((x0 + x1) * 0.5, y + 0.04, z))
	var n := maxi(2, int(absf(span) / 0.32))
	for i: int in range(n + 1):
		var t := float(i) / float(n)
		var bx: float = lerp(x0, x1, t)
		_baluster(parent, 0.86, mat, Vector3(bx, y + 0.06, z))


## A glowing multi-pane window set into a timber frame (faces +z by default),
## with a brass sill line and an alpine flower box.
static func _window(parent: Node3D, w: float, h: float, pos: Vector3, frame: Material, glow: Material, glass: Material, cap: Material) -> void:
	# outer timber frame
	_box(parent, Vector3(w + 0.18, 0.12, 0.16), frame, pos + Vector3(0, h * 0.5 + 0.06, 0))
	_box(parent, Vector3(w + 0.18, 0.12, 0.16), frame, pos + Vector3(0, -h * 0.5 - 0.06, 0))
	for s: float in SIDES:
		_box(parent, Vector3(0.1, h + 0.12, 0.16), frame, pos + Vector3(s * (w * 0.5 + 0.05), 0, 0))
	# warm glowing inner pane (the "lit from inside" read)
	_box(parent, Vector3(w, h, 0.05), glow, pos)
	# glass sheen over the glow
	_box(parent, Vector3(w + 0.02, h + 0.02, 0.02), glass, pos + Vector3(0, 0, 0.06))
	# muntins (cross dividers)
	_box(parent, Vector3(w, 0.05, 0.07), frame, pos + Vector3(0, 0, 0.05))
	_box(parent, Vector3(0.05, h, 0.07), frame, pos + Vector3(0, 0, 0.05))
	# brass sill cap (luxury accent)
	_box(parent, Vector3(w + 0.24, 0.04, 0.2), cap, pos + Vector3(0, -h * 0.5 - 0.12, 0.04))
	# a flower box under the sill (alpine charm)
	_box(parent, Vector3(w + 0.1, 0.16, 0.22), frame, pos + Vector3(0, -h * 0.5 - 0.24, 0.12))
	for k: int in range(int(w / 0.18)):
		var fx := -w * 0.5 + 0.12 + k * 0.18
		_ball(parent, 0.06, _toon(Color(0.85, 0.25, 0.30), 0.4), pos + Vector3(fx, -h * 0.5 - 0.12, 0.16), Vector3.ONE, 8, 4)


## A carved timber column on a stone plinth with a brass collar + capital — the
## colonnade under the front balcony.
static func _column(parent: Node3D, h: float, pos: Vector3, timber: Material, stone: Material, cap: Material) -> void:
	_box(parent, Vector3(0.5, 0.18, 0.5), stone, pos + Vector3(0, 0.09, 0))                  # plinth
	_box(parent, Vector3(0.44, 0.1, 0.44), cap, pos + Vector3(0, 0.22, 0))                   # brass base ring
	_cyl(parent, 0.18, 0.22, h, timber, pos + Vector3(0, 0.27 + h * 0.5, 0), Vector3.ZERO, 10)  # shaft
	# spiral-carved flutes hinted as 4 thin verticals
	for a: int in range(4):
		var ang := float(a) / 4.0 * TAU
		_cyl(parent, 0.02, 0.02, h - 0.2, timber, pos + Vector3(cos(ang) * 0.2, 0.27 + h * 0.5, sin(ang) * 0.2), Vector3.ZERO, 6)
	_cyl(parent, 0.26, 0.2, 0.16, cap, pos + Vector3(0, 0.27 + h + 0.08, 0), Vector3.ZERO, 10)  # brass capital
	_box(parent, Vector3(0.56, 0.14, 0.56), timber, pos + Vector3(0, 0.27 + h + 0.22, 0))      # abacus


## A small stone sentinel statue (a seated bear-ish guardian) on a pedestal.
static func _bear_statue(parent: Node3D, pos: Vector3, body: Material, plinth: Material) -> void:
	_box(parent, Vector3(0.9, 0.7, 0.9), plinth, pos + Vector3(0, 0.35, 0))                  # pedestal
	_box(parent, Vector3(0.96, 0.08, 0.96), plinth, pos + Vector3(0, 0.74, 0))               # pedestal cap
	_ball(parent, 0.36, body, pos + Vector3(0, 1.2, 0), Vector3(1.0, 1.15, 0.9), 12, 6)      # haunches
	_ball(parent, 0.3, body, pos + Vector3(0, 1.6, 0.05), Vector3(0.9, 1.1, 0.9), 12, 6)     # chest
	_ball(parent, 0.24, body, pos + Vector3(0, 2.0, 0.12), Vector3(1.0, 0.95, 0.95), 12, 6)  # head
	for e: float in SIDES:
		_ball(parent, 0.09, body, pos + Vector3(e * 0.16, 2.18, 0.12), Vector3.ONE, 8, 4)    # ears
		_cyl(parent, 0.08, 0.1, 0.55, body, pos + Vector3(e * 0.28, 1.55, 0.28), Vector3(0.4, 0, 0), 8)  # forelegs
	_ball(parent, 0.1, body, pos + Vector3(0, 1.95, 0.32), Vector3(1.2, 0.8, 1.0), 8, 4)     # snout


## A tiered brass-rimmed stone fountain with a glowing-water basin and a finial.
static func _fountain(parent: Node3D, pos: Vector3, stone: Material, cap: Material, water: Material) -> void:
	var f := Node3D.new()
	f.name = "Fountain"
	f.position = pos
	parent.add_child(f)
	# lower basin
	_cyl(f, 1.5, 1.6, 0.5, stone, Vector3(0, 0.25, 0), Vector3.ZERO, 18)
	_torus(f, 1.35, 1.6, cap, Vector3(0, 0.52, 0), Vector3(PI * 0.5, 0, 0), 16)              # brass rim
	_cyl(f, 1.32, 1.32, 0.12, water, Vector3(0, 0.46, 0), Vector3.ZERO, 18)                  # water sheet
	# central pedestal + upper bowl
	_cyl(f, 0.22, 0.3, 0.9, stone, Vector3(0, 0.9, 0), Vector3.ZERO, 12)
	_cyl(f, 0.7, 0.55, 0.28, stone, Vector3(0, 1.42, 0), Vector3.ZERO, 14)
	_torus(f, 0.55, 0.72, cap, Vector3(0, 1.56, 0), Vector3(PI * 0.5, 0, 0), 14)
	_cyl(f, 0.5, 0.5, 0.08, water, Vector3(0, 1.5, 0), Vector3.ZERO, 14)
	# spout finial + falling-water hint
	_cyl(f, 0.06, 0.12, 0.6, cap, Vector3(0, 1.95, 0), Vector3.ZERO, 8)
	_ball(f, 0.14, cap, Vector3(0, 2.3, 0), Vector3.ONE, 10, 5)
	for a: int in range(6):
		var ang := float(a) / 6.0 * TAU
		_cyl(f, 0.015, 0.03, 0.7, water, Vector3(cos(ang) * 0.35, 1.05, sin(ang) * 0.35), Vector3(0, 0, cos(ang) * 0.25), 6)


# ───────────────────────────── the build ───────────────────────────────────

static func build() -> Node3D:
	_ensure_shaders()
	var root := Node3D.new()
	root.name = "FrosthavenAlpineChalet"

	# materials
	var m_timber := _toon(C_TIMBER, 0.26)
	var m_timber_lt := _toon(C_TIMBER_LT, 0.30)
	var m_stucco := _toon(C_STUCCO, 0.22)
	var m_stone := _toon(C_STONE, 0.34, true, 0.12)
	var m_stone_dk := _toon(C_STONE_DK, 0.30)
	var m_marble := _gloss(C_MARBLE, 0.14)
	var m_roof := _toon(C_ROOF, 0.24)
	var m_snow := _gloss(C_SNOW, 0.55)
	var m_gold := _metal(C_GOLD, 0.26, 1.0)
	var m_gold_dk := _metal(C_GOLD_DK, 0.34, 1.0)
	var m_floor := _toon(C_FLOOR, 0.20)
	var glow_win := _glow(C_GLOW, 1.7)
	var glass := _glass(Color(0.85, 0.92, 1.0), 0.30)
	var water := _glass(C_WATER, 0.55)

	# footprint
	var W := 12.0          # building width  (along X)
	var D := 10.0          # building depth   (along Z)
	var hx := W * 0.5
	var hz := D * 0.5
	var floor_h := 3.0     # ground floor height
	var floor2_h := 2.8    # upper floor height

	_build_landscape(root, hx, hz, m_stone, m_stone_dk, m_gold, m_gold_dk, water)
	_build_stone_base(root, W, D, hx, hz, m_stone, m_stone_dk, m_gold, m_marble)
	_build_ground_floor(root, W, D, hx, hz, floor_h, m_timber, m_timber_lt, m_stucco, m_floor, m_gold, glow_win, glass, m_timber)
	_build_colonnade(root, W, D, hx, hz, floor_h, m_timber, m_stone, m_gold)
	_build_upper_floor(root, W, D, hx, hz, floor_h, floor2_h, m_timber, m_timber_lt, m_stucco, m_floor, m_gold, glow_win, glass)
	_build_balconies(root, W, D, hx, hz, floor_h, m_timber, m_timber_lt, m_gold)
	_build_roof(root, W, D, hx, hz, floor_h, floor2_h, m_roof, m_snow, m_timber, m_gold, glow_win, glass)
	_build_chimney(root, hx, hz, floor_h, floor2_h, m_stone, m_stone_dk, m_gold)
	_build_interior(root, W, D, hx, hz, floor_h, floor2_h, m_timber, m_stone, m_stone_dk, m_marble, m_floor, m_gold, m_gold_dk, glow_win)

	return root


## Grounds: flagstone approach with a centerpiece fountain, brass-capped stone
## gateposts crowned by carved eagle finials + lanterns, clipped hedges, a low
## alpine boulder cluster, and a pair of conifers.
static func _build_landscape(root: Node3D, hx: float, hz: float, stone: Material, stone_dk: Material, gold: Material, gold_dk: Material, water: Material) -> void:
	var g := Node3D.new()
	g.name = "Grounds"
	root.add_child(g)
	# wide flagstone forecourt apron
	_box(g, Vector3(7.5, 0.1, 8.0), stone_dk, Vector3(0, 0.05, hz + 4.0))
	# flagstone approach path (+z), split to flow around the fountain
	for i: int in range(7):
		var pz := hz + 0.6 + i * 1.15
		var jitter := 0.06 * sin(float(i) * 1.7)
		_box(g, Vector3(2.2, 0.12, 0.95), stone, Vector3(jitter, 0.11, pz))
		_box(g, Vector3(2.34, 0.06, 1.06), stone_dk, Vector3(jitter, 0.05, pz))
	# centerpiece fountain partway down the approach
	_fountain(g, Vector3(0, 0.1, hz + 6.0), stone, gold, water)
	# clipped hedges flanking the path
	var hedge := _toon(C_HEDGE, 0.34)
	for s: float in SIDES:
		for i: int in range(5):
			var hzp := hz + 1.0 + i * 1.15
			_box(g, Vector3(0.55, 0.7, 0.95), hedge, Vector3(s * 1.9, 0.4, hzp))
			_ball(g, 0.32, hedge, Vector3(s * 1.9, 0.78, hzp), Vector3(1.0, 0.7, 1.0), 10, 5)
	# stone gateposts at the path head: brass-capped pillars w/ lantern + eagle
	for s: float in SIDES:
		var lx := s * 2.6
		var lz := hz + 7.6
		_box(g, Vector3(0.7, 2.2, 0.7), stone, Vector3(lx, 1.1, lz))                       # pillar
		for row: int in range(5):
			_box(g, Vector3(0.74, 0.04, 0.74), stone_dk, Vector3(lx, 0.5 + row * 0.4, lz)) # coursing
		_box(g, Vector3(0.82, 0.12, 0.82), gold, Vector3(lx, 2.26, lz))                    # brass cap
		# glass lantern on the cap
		_box(g, Vector3(0.34, 0.42, 0.34), gold_dk, Vector3(lx, 2.6, lz))
		_box(g, Vector3(0.24, 0.32, 0.24), _glow(C_GLOW, 2.6), Vector3(lx, 2.6, lz))
		_prism(g, Vector3(0.42, 0.26, 0.42), gold, Vector3(lx, 2.95, lz))
		# carved eagle finial statue crowning the gatepost
		_ball(g, 0.16, stone, Vector3(lx, 3.2, lz), Vector3(1.0, 1.2, 1.0), 10, 5)           # body
		_ball(g, 0.11, stone, Vector3(lx, 3.42, lz + 0.04), Vector3.ONE, 8, 4)               # head
		for e: float in SIDES:
			_prism(g, Vector3(0.5, 0.18, 0.1), stone, Vector3(lx + e * 0.22, 3.26, lz), Vector3(0, 0, -e * 0.7))  # wings
	# alpine boulder cluster off to one side
	var rock := _toon(Color(0.45, 0.46, 0.48), 0.3)
	for i: int in range(4):
		var ang := float(i) * 1.6
		_ball(g, 0.45 + 0.12 * float(i % 2), rock, Vector3(-hx - 1.6 + cos(ang) * 0.6, 0.3, -hz + 0.4 + sin(ang) * 0.6), Vector3(1.0, 0.7, 1.1), 10, 5)
	# a couple of snow-tipped conifers framing the back corners
	var trunk := _toon(Color(0.32, 0.21, 0.12))
	var pine := _toon(Color(0.16, 0.38, 0.24), 0.34)
	var snow := _gloss(C_SNOW, 0.55)
	for s: float in SIDES:
		var tx := s * (hx + 1.8)
		var tz := -hz - 0.4
		_cyl(g, 0.1, 0.16, 0.9, trunk, Vector3(tx, 0.45, tz), Vector3.ZERO, 8)
		for k: int in range(3):
			var ky := 1.0 + k * 0.7
			var kr := 0.95 - k * 0.26
			_cyl(g, 0.0, kr, 1.0, pine, Vector3(tx, ky, tz), Vector3.ZERO, 10)
			_cyl(g, 0.0, kr * 0.55, 0.3, snow, Vector3(tx, ky + 0.35, tz), Vector3.ZERO, 10)


## Heavy veined-stone ground base the timber sits on — gives the chalet weight.
## Now with a polished-marble entry landing + brass nosing on the steps.
static func _build_stone_base(root: Node3D, W: float, D: float, hx: float, hz: float, stone: Material, stone_dk: Material, gold: Material, marble: Material) -> void:
	var b := Node3D.new()
	b.name = "StoneBase"
	root.add_child(b)
	var bh := 0.9
	# main plinth slab
	_box(b, Vector3(W + 0.6, bh, D + 0.6), stone, Vector3(0, bh * 0.5, 0))
	# coursed-stone band of irregular blocks around the plinth (3 sides; +z open)
	var courses := 3
	for c: int in range(courses):
		var cy := 0.18 + c * 0.26
		var off := 0.04 * float(c % 2)
		# back wall blocks
		for i: int in range(8):
			var bx := -hx + 0.5 + i * (W / 8.0) + off
			_box(b, Vector3(W / 8.0 - 0.06, 0.22, 0.18), stone_dk, Vector3(bx, cy, -hz - 0.18))
		# side wall blocks
		for s: float in SIDES:
			for i: int in range(7):
				var bz := -hz + 0.5 + i * (D / 7.0) + off
				_box(b, Vector3(0.18, 0.22, D / 7.0 - 0.06), stone_dk, Vector3(s * (hx + 0.18), cy, bz))
	# brass cap rail flush around the top edge (3 sides)
	_box(b, Vector3(W + 0.7, 0.06, 0.14), gold, Vector3(0, bh, -hz - 0.25))
	for s: float in SIDES:
		_box(b, Vector3(0.14, 0.06, D + 0.7), gold, Vector3(s * (hx + 0.25), bh, 0))
	# broad stone entrance steps at +z up to the timber floor (y≈0.9), brass nosing
	for i: int in range(3):
		var sy := 0.3 + i * 0.2
		var sw := 4.2 - i * 0.5
		_box(b, Vector3(sw, 0.2, 0.55), stone, Vector3(0, sy - 0.1, hz + 0.55 - i * 0.45))
		_box(b, Vector3(sw + 0.04, 0.04, 0.08), gold, Vector3(0, sy, hz + 0.55 - i * 0.45 + 0.26))
	# polished marble entry landing flush at the threshold
	_box(b, Vector3(3.2, 0.06, 0.9), marble, Vector3(0, bh + 0.03, hz - 0.2))


## Ground-floor timber shell on 3 sides (+z OMITTED for the walk-in view), the
## entrance with a real door + carved pediment, glowing windows, corner posts,
## flanking stone bear sentinel statues.
static func _build_ground_floor(root: Node3D, W: float, D: float, hx: float, hz: float, fh: float, timber: Material, timber_lt: Material, stucco: Material, floor_mat: Material, gold: Material, glow: Material, glass: Material, frame: Material) -> void:
	var g := Node3D.new()
	g.name = "GroundFloor"
	g.position.y = 0.9   # sits on the stone base
	root.add_child(g)
	var wall_t := 0.22

	# interior plank floor
	_box(g, Vector3(W - 0.2, 0.12, D - 0.2), floor_mat, Vector3(0, 0.06, 0))
	# subtle plank seams
	for i: int in range(9):
		var px := -hx + 1.0 + i * ((W - 2.0) / 8.0)
		_box(g, Vector3(0.04, 0.13, D - 0.4), timber, Vector3(px, 0.065, 0))

	# back wall (-z): stucco infill between corner posts, log-band base
	_box(g, Vector3(W, fh, wall_t), stucco, Vector3(0, fh * 0.5, -hz))
	# exposed horizontal log courses across the back (heavy timber read)
	for i: int in range(5):
		_cyl(g, 0.13, 0.13, W, timber, Vector3(0, 0.35 + i * 0.55, -hz - 0.02), Vector3(0, 0, PI * 0.5), 8)
	# side walls (±x): stucco with timber framing
	for s: float in SIDES:
		_box(g, Vector3(wall_t, fh, D), stucco, Vector3(s * hx, fh * 0.5, 0))
		# diagonal cross-braces (alpine fachwerk)
		_box(g, Vector3(0.16, fh * 1.1, 0.14), timber, Vector3(s * hx, fh * 0.5, hz * 0.45), Vector3(0, 0, 0.5))
		_box(g, Vector3(0.16, fh * 1.1, 0.14), timber, Vector3(s * hx, fh * 0.5, -hz * 0.45), Vector3(0, 0, -0.5))
		_box(g, Vector3(0.16, fh, 0.14), timber, Vector3(s * hx, fh * 0.5, 0))

	# heavy corner posts (4)
	for s: float in SIDES:
		for z: float in SIDES:
			_box(g, Vector3(0.28, fh + 0.1, 0.28), timber, Vector3(s * hx, fh * 0.5, z * hz))

	# low front threshold parapet at +z (the front wall is OMITTED so you see in)
	_box(g, Vector3(W, 0.5, wall_t), timber, Vector3(0, 0.25, hz))
	_box(g, Vector3(W, 0.08, 0.3), gold, Vector3(0, 0.5, hz))   # brass sill cap

	# entrance: a recessed timber door portal centered on +z, opening through
	# the parapet so the avatar can walk straight in.
	# door surround posts
	for s: float in SIDES:
		_box(g, Vector3(0.18, 2.5, 0.3), timber, Vector3(s * 0.95, 1.25, hz))
	_box(g, Vector3(2.1, 0.24, 0.32), timber, Vector3(0, 2.4, hz))   # lintel
	_box(g, Vector3(2.2, 0.05, 0.34), gold, Vector3(0, 2.55, hz))    # brass lintel line
	_prism(g, Vector3(2.2, 0.4, 0.34), timber, Vector3(0, 2.6, hz))  # carved pediment
	_ball(g, 0.1, gold, Vector3(0, 2.62, hz + 0.05), Vector3.ONE, 10, 5)  # pediment boss
	# the door leaf (slightly ajar, set back so the threshold is open)
	var door := _toon(Color(0.30, 0.18, 0.10), 0.26)
	_box(g, Vector3(1.5, 2.2, 0.1), door, Vector3(-0.35, 1.15, hz - 0.18), Vector3(0, -0.5, 0))
	# door plank seams + brass furniture
	for k: int in range(3):
		_box(g, Vector3(0.05, 2.1, 0.11), timber, Vector3(-0.35 + (k - 1) * 0.4, 1.15, hz - 0.18), Vector3(0, -0.5, 0))
	_ball(g, 0.07, gold, Vector3(0.18, 1.1, hz - 0.32), Vector3.ONE, 8, 4)
	_box(g, Vector3(0.5, 0.5, 0.05), _glow(C_GLOW, 2.0), Vector3(0, 2.35, hz - 0.06))  # transom glow

	# flanking stone bear sentinel statues at the threshold
	for s: float in SIDES:
		_bear_statue(g, Vector3(s * 2.5, -0.9, hz + 0.5), _gloss(C_STONE, 0.3), _toon(C_STONE_DK, 0.32))

	# glowing windows: two on the back wall, one per side wall
	_window(g, 1.3, 1.4, Vector3(-3.0, 1.55, -hz + 0.02), frame, glow, glass, gold)
	_window(g, 1.3, 1.4, Vector3(3.0, 1.55, -hz + 0.02), frame, glow, glass, gold)
	for s: float in SIDES:
		var wnode := Node3D.new()
		wnode.rotation.y = s * (PI * 0.5)
		wnode.position = Vector3(s * (hx - 0.02), 0, 0)
		g.add_child(wnode)
		_window(wnode, 1.2, 1.3, Vector3(0, 1.55, 0), frame, glow, glass, gold)


## A colonnade of carved timber columns standing on the stone base along the
## open +z face, carrying the front balcony deck above — instant grandeur.
static func _build_colonnade(root: Node3D, W: float, D: float, hx: float, hz: float, fh: float, timber: Material, stone: Material, gold: Material) -> void:
	var c := Node3D.new()
	c.name = "Colonnade"
	c.position.y = 0.9
	root.add_child(c)
	var col_h := fh - 0.45
	var bp := 1.4                       # must match the front balcony projection
	# four columns evenly spaced across the front, set just beyond the threshold
	for i: int in range(4):
		var t := float(i) / 3.0
		var cxp: float = lerp(-hx + 0.7, hx - 0.7, t)
		_column(c, col_h, Vector3(cxp, 0.0, hz + bp - 0.35), timber, stone, gold)
	# slim carved arch spandrels linking the column heads (decorative)
	for i: int in range(3):
		var t0 := float(i) / 3.0
		var t1 := float(i + 1) / 3.0
		var ax: float = lerp(-hx + 0.7, hx - 0.7, (t0 + t1) * 0.5)
		_prism(c, Vector3(1.4, 0.4, 0.16), timber, Vector3(ax, col_h + 0.05, hz + bp - 0.35), Vector3(PI, 0, 0))
		_box(c, Vector3(1.5, 0.06, 0.18), gold, Vector3(ax, col_h + 0.3, hz + bp - 0.35))


## Upper floor: a deck/floor slab over the ground floor, timber + stucco shell
## with glowing gable + side windows and a brass belt course.
static func _build_upper_floor(root: Node3D, W: float, D: float, hx: float, hz: float, fh: float, f2h: float, timber: Material, timber_lt: Material, stucco: Material, floor_mat: Material, gold: Material, glow: Material, glass: Material) -> void:
	var u := Node3D.new()
	u.name = "UpperFloor"
	var base_y := 0.9 + fh
	u.position.y = base_y
	root.add_child(u)
	var wall_t := 0.22

	# floor slab (with a stair opening cut by leaving a gap at +x/-z)
	# we model it as two L-pieces so the staircase well stays open
	_box(u, Vector3(W - 0.2, 0.22, D - 4.0), floor_mat, Vector3(0, 0.11, -2.0))
	_box(u, Vector3(W - 5.0, 0.22, 4.0), floor_mat, Vector3(-2.5, 0.11, hz - 2.0))
	# exposed joist edges along +z
	_box(u, Vector3(W, 0.18, 0.2), timber, Vector3(0, 0.05, hz - 0.1))

	# upper shell: timber-banded stucco, slightly inset for a tiered silhouette
	var inset := 0.15
	_box(u, Vector3(W - inset * 2.0, f2h, wall_t), stucco, Vector3(0, f2h * 0.5 + 0.2, -hz + inset))
	for s: float in SIDES:
		_box(u, Vector3(wall_t, f2h, D - inset * 2.0), stucco, Vector3(s * (hx - inset), f2h * 0.5 + 0.2, 0))
	# corner posts upstairs
	for s: float in SIDES:
		for z: float in SIDES:
			_box(u, Vector3(0.24, f2h + 0.1, 0.24), timber, Vector3(s * (hx - inset), f2h * 0.5 + 0.2, z * (hz - inset)))
	# horizontal timber band (the chalet "belt") + a brass pinstripe
	_box(u, Vector3(W, 0.18, 0.02), timber, Vector3(0, 0.32, -hz + inset + 0.01))
	_box(u, Vector3(W, 0.04, 0.03), gold, Vector3(0, 0.44, -hz + inset + 0.02))
	for s: float in SIDES:
		_box(u, Vector3(0.02, 0.18, D - inset * 2.0), timber, Vector3(s * (hx - inset - 0.12), 0.32, 0))

	# low front parapet upstairs (front omitted for the open balcony view)
	_box(u, Vector3(W - inset * 2.0, 0.6, wall_t), timber, Vector3(0, 0.5, hz - inset))

	# glowing upper windows: a central gable window on the back, side windows
	_window(u, 1.4, 1.5, Vector3(0, 1.6, -hz + inset + 0.02), timber, glow, glass, gold)
	for s: float in SIDES:
		var wnode := Node3D.new()
		wnode.rotation.y = s * (PI * 0.5)
		wnode.position = Vector3(s * (hx - inset - 0.02), 0, -1.0)
		u.add_child(wnode)
		_window(wnode, 1.2, 1.3, Vector3(0, 1.6, 0), timber, glow, glass, gold)


## Carved fretwork balconies: a wide front balcony upstairs + a back gable
## balcony, with turned balusters, brass cap rails, and scalloped brackets.
static func _build_balconies(root: Node3D, W: float, D: float, hx: float, hz: float, fh: float, timber: Material, timber_lt: Material, gold: Material) -> void:
	var b := Node3D.new()
	b.name = "Balconies"
	var deck_y := 0.9 + fh
	b.position.y = deck_y
	root.add_child(b)

	# FRONT balcony deck projecting beyond +z (carried by the colonnade below)
	var bp := 1.4
	_box(b, Vector3(W - 0.4, 0.16, bp), timber, Vector3(0, 0.05, hz + bp * 0.5))
	_box(b, Vector3(W - 0.3, 0.04, bp + 0.06), gold, Vector3(0, 0.14, hz + bp * 0.5))   # brass fascia line
	# decorative carved brackets under the deck
	for i: int in range(7):
		var bx := -hx + 0.6 + i * ((W - 1.2) / 6.0)
		_prism(b, Vector3(0.4, 0.5, 0.5), timber, Vector3(bx, -0.25, hz + 0.25), Vector3(PI, 0, 0))
	# railing around the 3 open sides of the front balcony
	_railing(b, -hx + 0.3, hx - 0.3, hz + bp - 0.06, 0.13, timber_lt, gold)
	for s: float in SIDES:
		var rn := Node3D.new()
		rn.rotation.y = s * (PI * 0.5)
		rn.position = Vector3(s * (hx - 0.05), 0.13, hz + bp * 0.5)
		b.add_child(rn)
		_railing(rn, -bp * 0.5 + 0.06, bp * 0.5 - 0.06, 0, 0, timber_lt, gold)

	# BACK gable balcony (smaller)
	var bz := -hz - 0.9
	_box(b, Vector3(W - 3.0, 0.16, 0.9), timber, Vector3(0, 0.05, bz + 0.45))
	for i: int in range(5):
		var bx2 := -(W - 3.0) * 0.5 + 0.4 + i * ((W - 3.8) / 4.0)
		_prism(b, Vector3(0.34, 0.45, 0.45), timber, Vector3(bx2, -0.22, bz + 0.2), Vector3(PI, 0, 0))
	_railing(b, -(W - 3.0) * 0.5 + 0.2, (W - 3.0) * 0.5 - 0.2, bz - 0.06, 0.13, timber_lt, gold)


## The hero: a WIDE snow-pitched gable roof with DEEP overhanging eaves, exposed
## ridge + purlin beams, timber-shingle slopes, carved bargeboards, snow caps,
## glowing snow-dusted dormers, a brass ridge crest, and a ridge cupola with a
## brass finial.
static func _build_roof(root: Node3D, W: float, D: float, hx: float, hz: float, fh: float, f2h: float, roof: Material, snow: Material, timber: Material, gold: Material, glow: Material, glass: Material) -> void:
	var r := Node3D.new()
	r.name = "Roof"
	var eave_y := 0.9 + fh + f2h + 0.2
	r.position.y = eave_y
	root.add_child(r)

	var over := 1.5                      # deep eave overhang
	var slope_w := hx + over             # half-span the slope must cover
	var ridge_h := 3.0                   # ridge above eave
	var roof_len := D + over * 2.0       # gable runs along Z, overhangs front/back
	var pitch := atan2(ridge_h, slope_w) # roof angle
	var slope_len := sqrt(slope_w * slope_w + ridge_h * ridge_h)
	var thick := 0.22

	# two big sloped shingle planes meeting at the ridge
	for s: float in SIDES:
		var plane := _box(r, Vector3(slope_len, thick, roof_len), roof, Vector3.ZERO)
		plane.position = Vector3(s * slope_w * 0.5, ridge_h * 0.5, 0)
		plane.rotation = Vector3(0, 0, -s * pitch)
		# snow cap riding the upper two-thirds of each slope
		var snowp := _box(r, Vector3(slope_len * 0.7, 0.1, roof_len * 0.98), snow, Vector3.ZERO)
		snowp.position = Vector3(s * slope_w * 0.32, ridge_h * 0.68, 0)
		snowp.rotation = Vector3(0, 0, -s * pitch)
		# exposed purlin beams under the eave
		_cyl(r, 0.12, 0.12, roof_len, timber, Vector3(s * (slope_w - 0.1), 0.1, 0), Vector3(PI * 0.5, 0, 0), 8)
		# brass drip-edge along the eave
		_box(r, Vector3(0.06, 0.05, roof_len), gold, Vector3(s * (slope_w + 0.02), 0.02, 0), Vector3(0, 0, -s * pitch))

	# ridge beam
	_cyl(r, 0.16, 0.16, roof_len + 0.2, timber, Vector3(0, ridge_h, 0), Vector3(PI * 0.5, 0, 0), 8)
	# brass ridge crest (a row of small finials marching the ridge line)
	for i: int in range(7):
		var crz := -roof_len * 0.4 + i * (roof_len * 0.8 / 6.0)
		_prism(r, Vector3(0.14, 0.3, 0.14), gold, Vector3(0, ridge_h + 0.18, crz))
		_ball(r, 0.05, gold, Vector3(0, ridge_h + 0.38, crz), Vector3.ONE, 6, 3)

	# snow-dusted dormers breaking each slope (two per side)
	for s: float in SIDES:
		for d: int in range(2):
			var dz := (float(d) - 0.5) * 3.4
			var dn := Node3D.new()
			dn.position = Vector3(s * slope_w * 0.42, ridge_h * 0.5, dz)
			r.add_child(dn)
			_box(dn, Vector3(1.1, 1.1, 1.1), roof, Vector3.ZERO)                                   # dormer box
			_box(dn, Vector3(0.6, 0.7, 0.06), glow, Vector3(s * 0.58, 0.05, 0), Vector3(0, s * PI * 0.5, 0))  # lit face
			_box(dn, Vector3(0.62, 0.72, 0.03), glass, Vector3(s * 0.62, 0.05, 0), Vector3(0, s * PI * 0.5, 0))
			_prism(dn, Vector3(1.2, 0.6, 1.2), roof, Vector3(0, 0.7, 0))                            # dormer gable
			_prism(dn, Vector3(1.0, 0.2, 1.0), snow, Vector3(0, 0.92, 0))                           # snow cap

	# gable infill triangles (front +z and back -z) so the ends read solid
	for z: float in SIDES:
		var gz := z * hz
		_prism(r, Vector3(slope_w * 2.0, ridge_h, thick), roof, Vector3(0, ridge_h * 0.5, gz), Vector3.ZERO)
		# carved scalloped bargeboard along both rake edges of this gable
		for s: float in SIDES:
			var bb := _box(r, Vector3(slope_len, 0.34, 0.1), timber, Vector3.ZERO)
			bb.position = Vector3(s * slope_w * 0.5, ridge_h * 0.5, gz + z * (over - 0.1))
			bb.rotation = Vector3(0, 0, -s * pitch)
		# scallop teeth hanging off the front bargeboard (decorative)
		if z > 0.0:
			for s2: float in SIDES:
				for k: int in range(6):
					var t := float(k) / 5.0
					var px: float = s2 * lerp(0.3, slope_w - 0.2, t)
					var py: float = lerp(ridge_h - 0.2, 0.2, t)
					_prism(r, Vector3(0.22, 0.3, 0.06), timber, Vector3(px, py, gz + over - 0.05), Vector3(PI, 0, 0))
			# carved sunburst ornament in the front gable peak
			_ball(r, 0.3, gold, Vector3(0, ridge_h - 0.6, gz + over - 0.12), Vector3(1.0, 1.0, 0.3), 12, 6)
			for a: int in range(8):
				var ang := float(a) / 8.0 * TAU
				_box(r, Vector3(0.08, 0.4, 0.05), gold, Vector3(cos(ang) * 0.45, ridge_h - 0.6 + sin(ang) * 0.45, gz + over - 0.1), Vector3(0, 0, ang))

	# exposed rafter tails poking out beyond the eaves along both long sides
	for s: float in SIDES:
		for i: int in range(11):
			var rz := -hz + 0.3 + i * ((D - 0.6) / 10.0)
			_box(r, Vector3(0.5, 0.1, 0.12), timber, Vector3(s * (slope_w - 0.1), 0.02, rz), Vector3(0, 0, -s * pitch))

	# ridge cupola with a brass finial (silhouette punctuation)
	_box(r, Vector3(1.0, 0.9, 1.0), roof, Vector3(0, ridge_h + 0.55, 1.5))
	_box(r, Vector3(0.55, 0.45, 0.08), _glow(C_GLOW, 2.0), Vector3(0, ridge_h + 0.55, 2.0))
	_prism(r, Vector3(1.2, 0.7, 1.2), roof, Vector3(0, ridge_h + 1.25, 1.5))
	_prism(r, Vector3(1.3, 0.22, 1.3), snow, Vector3(0, ridge_h + 1.5, 1.5))
	_cyl(r, 0.0, 0.06, 0.5, gold, Vector3(0, ridge_h + 1.9, 1.5), Vector3.ZERO, 8)
	_ball(r, 0.12, gold, Vector3(0, ridge_h + 2.1, 1.5), Vector3.ONE, 10, 5)


## A roaring stone chimney rising up the -x/-z corner, breaking the eave with a
## brass cap band and a little ember glow.
static func _build_chimney(root: Node3D, hx: float, hz: float, fh: float, f2h: float, stone: Material, stone_dk: Material, gold: Material) -> void:
	var c := Node3D.new()
	c.name = "Chimney"
	root.add_child(c)
	var cx := -hx + 1.0
	var cz := -hz + 0.8
	var top := 0.9 + fh + f2h + 0.2 + 3.0 + 1.0   # above the ridge
	# shaft
	_box(c, Vector3(1.1, top, 1.1), stone, Vector3(cx, top * 0.5, cz))
	# coursed-stone detailing on the shaft (front + side faces)
	for row: int in range(7):
		var ry := 1.0 + row * (top - 1.5) / 7.0
		for i: int in range(3):
			_box(c, Vector3(0.32, 0.18, 0.04), stone_dk, Vector3(cx - 0.36 + i * 0.36, ry, cz + 0.56))
			_box(c, Vector3(0.04, 0.18, 0.32), stone_dk, Vector3(cx + 0.56, ry, cz - 0.36 + i * 0.36))
	# brass cap band + cap + ember glow
	_box(c, Vector3(1.2, 0.12, 1.2), gold, Vector3(cx, top - 0.2, cz))
	_box(c, Vector3(1.35, 0.3, 1.35), stone_dk, Vector3(cx, top + 0.05, cz))
	_box(c, Vector3(0.7, 0.3, 0.7), _glow(C_FIRE, 1.6), Vector3(cx, top + 0.22, cz))


## Walkable open interior: a clear ground floor with a MARBLE-and-brass showpiece
## fireplace (the roaring fire), a partial room divider, a grand twin-flight
## staircase with brass-capped rails, a ceiling beam grid, and a tiered antler +
## brass chandelier. Rooms kept OPEN so the owner furnishes later.
static func _build_interior(root: Node3D, W: float, D: float, hx: float, hz: float, fh: float, f2h: float, timber: Material, stone: Material, stone_dk: Material, marble: Material, floor_mat: Material, gold: Material, gold_dk: Material, glow: Material) -> void:
	var n := Node3D.new()
	n.name = "Interior"
	n.position.y = 0.9
	root.add_child(n)

	# ── MARBLE & brass showpiece HEARTH against the back-left wall ──
	var hx_c := -hx + 1.8
	var hz_c := -hz + 0.45
	# fireplace mass (marble surround over a stone core)
	_box(n, Vector3(2.4, 2.2, 0.7), stone, Vector3(hx_c, 1.1, hz_c))
	_box(n, Vector3(2.5, 2.3, 0.2), marble, Vector3(hx_c, 1.15, hz_c + 0.3))                   # marble face slab
	# fluted marble pilasters flanking the firebox
	for e: float in SIDES:
		_cyl(n, 0.12, 0.12, 1.7, marble, Vector3(hx_c + e * 0.95, 0.95, hz_c + 0.38), Vector3.ZERO, 10)
		_box(n, Vector3(0.3, 0.1, 0.3), gold, Vector3(hx_c + e * 0.95, 1.85, hz_c + 0.38))     # brass capital
	# firebox opening + the FIRE itself
	_box(n, Vector3(1.3, 1.1, 0.3), _toon(Color(0.07, 0.05, 0.05), 0.1), Vector3(hx_c, 0.75, hz_c + 0.3))
	_box(n, Vector3(1.4, 0.08, 0.34), gold, Vector3(hx_c, 1.36, hz_c + 0.32))                  # brass firebox lintel
	_box(n, Vector3(1.0, 0.7, 0.2), _glow(C_FIRE, 2.6), Vector3(hx_c, 0.6, hz_c + 0.42))
	for k: int in range(5):
		var fx := hx_c - 0.4 + k * 0.2
		var fhh := 0.5 + 0.25 * sin(float(k) * 1.3)
		_cyl(n, 0.0, 0.1, fhh, _glow(Color(1.0, 0.7, 0.25), 2.8), Vector3(fx, 0.4 + fhh * 0.5, hz_c + 0.42), Vector3.ZERO, 6)
	# log stack in the hearth
	for k: int in range(3):
		_cyl(n, 0.08, 0.08, 1.0, timber, Vector3(hx_c - 0.2 + k * 0.2, 0.32 + k * 0.05, hz_c + 0.42), Vector3(0, 0, PI * 0.5), 6)
	# heavy timber mantel + a brass trim line + a pair of brass candlesticks
	_box(n, Vector3(2.7, 0.22, 0.85), timber, Vector3(hx_c, 1.55, hz_c + 0.05))
	_box(n, Vector3(2.7, 0.05, 0.88), gold, Vector3(hx_c, 1.68, hz_c + 0.05))
	for e: float in SIDES:
		_cyl(n, 0.04, 0.06, 0.4, gold_dk, Vector3(hx_c + e * 0.9, 1.86, hz_c + 0.1), Vector3.ZERO, 8)
		_ball(n, 0.06, _glow(C_GLOW, 2.4), Vector3(hx_c + e * 0.9, 2.12, hz_c + 0.1), Vector3.ONE, 8, 4)
	# a framed brass-rimmed mirror over the mantel (showpiece)
	_box(n, Vector3(1.2, 1.0, 0.05), gold, Vector3(hx_c, 2.5, hz_c + 0.02))
	_box(n, Vector3(1.0, 0.8, 0.06), _glow(Color(0.8, 0.86, 0.95), 0.6), Vector3(hx_c, 2.5, hz_c + 0.05))

	# ── partial room divider (does NOT block the walkway) ──
	_box(n, Vector3(0.2, fh, 2.4), timber, Vector3(1.4, fh * 0.5, -hz + 1.4))

	# ── grand TWIN-FLIGHT staircase up the right side with a half-landing ──
	var st := Node3D.new()
	st.name = "Staircase"
	n.add_child(st)
	var sx := hx - 1.6
	var flight := 6
	var rise := fh / float(flight * 2)
	# lower flight rising along +z
	for i: int in range(flight):
		var sy := rise * (i + 1)
		var sz := -hz + 1.0 + i * 0.34
		_box(st, Vector3(1.6, 0.14, 0.42), marble, Vector3(sx, sy - 0.07, sz))
		_box(st, Vector3(1.6, rise, 0.06), timber, Vector3(sx, sy - rise * 0.5, sz - 0.18))    # riser
		_box(st, Vector3(1.64, 0.03, 0.06), gold, Vector3(sx, sy, sz + 0.19))                  # brass nosing
	# half-landing
	var land_y := rise * flight
	var land_z := -hz + 1.0 + flight * 0.34
	_box(st, Vector3(1.6, 0.16, 1.0), marble, Vector3(sx, land_y - 0.08, land_z + 0.3))
	# upper flight rising back along -z, offset inward
	for i: int in range(flight):
		var sy2 := land_y + rise * (i + 1)
		var sz2 := land_z + 0.8 - i * 0.34
		_box(st, Vector3(1.6, 0.14, 0.42), marble, Vector3(sx - 0.05, sy2 - 0.07, sz2))
		_box(st, Vector3(1.6, rise, 0.06), timber, Vector3(sx - 0.05, sy2 - rise * 0.5, sz2 + 0.18))
		_box(st, Vector3(1.64, 0.03, 0.06), gold, Vector3(sx - 0.05, sy2, sz2 - 0.19))
	# stringers + turned newel posts + brass ball caps
	_box(st, Vector3(0.16, 0.5, flight * 0.34 + 0.4), timber, Vector3(sx - 0.84, rise * flight * 0.5, -hz + 1.0 + flight * 0.17))
	for p: int in range(2):
		var pz := -hz + 0.9 + float(p) * (flight * 0.34 + 0.4)
		_cyl(st, 0.09, 0.12, 1.3, timber, Vector3(sx - 0.84, rise * flight * 0.5, pz), Vector3.ZERO, 8)
		_ball(st, 0.14, gold, Vector3(sx - 0.84, rise * flight * 0.5 + 0.7, pz), Vector3.ONE, 10, 5)
	# raking brass handrail over the lower flight
	_box(st, Vector3(0.06, 0.06, flight * 0.4), gold, Vector3(sx - 0.84, rise * flight * 0.5 + 0.95, -hz + 1.0 + flight * 0.17), Vector3(atan2(fh * 0.5, flight * 0.34), 0, 0))

	# ── ceiling: plank deck + exposed beam grid over the ground floor ──
	_box(n, Vector3(W - 0.4, 0.1, D - 0.4), floor_mat, Vector3(0, fh - 0.05, 0))
	for i: int in range(5):
		var bx := -hx + 1.4 + i * ((W - 2.8) / 4.0)
		_cyl(n, 0.12, 0.12, D - 0.6, timber, Vector3(bx, fh - 0.22, 0), Vector3(PI * 0.5, 0, 0), 8)

	# ── tiered antler + brass chandelier hung from the ceiling center ──
	var ch := Node3D.new()
	ch.name = "Chandelier"
	ch.position = Vector3(-0.5, fh - 0.2, 0.3)
	n.add_child(ch)
	_cyl(ch, 0.025, 0.025, 0.6, gold, Vector3(0, -0.3, 0), Vector3.ZERO, 6)                    # chain rod
	_ball(ch, 0.08, gold, Vector3(0, -0.62, 0), Vector3.ONE, 8, 4)                             # boss
	# lower ring (6 candles)
	_torus(ch, 0.45, 0.6, timber, Vector3(0, -0.66, 0), Vector3(PI * 0.5, 0, 0), 8)
	_torus(ch, 0.5, 0.58, gold, Vector3(0, -0.66, 0), Vector3(PI * 0.5, 0, 0), 8)              # brass band
	for k: int in range(6):
		var a := float(k) / 6.0 * TAU
		_cyl(ch, 0.02, 0.04, 0.18, gold, Vector3(cos(a) * 0.55, -0.56, sin(a) * 0.55), Vector3.ZERO, 6)
		_ball(ch, 0.08, _glow(C_GLOW, 2.6), Vector3(cos(a) * 0.55, -0.44, sin(a) * 0.55), Vector3.ONE, 8, 4)
	# upper smaller ring (4 candles) for the tiered read
	_torus(ch, 0.22, 0.34, gold, Vector3(0, -0.36, 0), Vector3(PI * 0.5, 0, 0), 8)
	for k: int in range(4):
		var a2 := float(k) / 4.0 * TAU
		_cyl(ch, 0.02, 0.04, 0.16, gold, Vector3(cos(a2) * 0.3, -0.28, sin(a2) * 0.3), Vector3.ZERO, 6)
		_ball(ch, 0.07, _glow(C_GLOW, 2.6), Vector3(cos(a2) * 0.3, -0.16, sin(a2) * 0.3), Vector3.ONE, 8, 4)


# ───────────────────────────── metadata ────────────────────────────────────

static func meta() -> Dictionary:
	return {
		"id": "alpine_chalet",
		"name": "Frosthaven Alpine Chalet",
		"tier": "Villa",
		"rarity": "Rare",
		"description": "A heavy-timber Swiss alpine chalet on a veined-stone base, crowned by a wide snow-pitched roof with deep carved eaves, glowing dormers, fretwork balconies on a brass-trimmed colonnade, and a marble-and-brass showpiece hearth — guarded by stone sentinels and approached past a tiered fountain.",
		"footprint": [12, 10],
		"floors": 2,
		"attributes": [
			["Style", "Alpine Chalet"],
			["Material", "Heavy Timber, Veined Stone & Marble"],
			["Feature", "Marble & Brass Showpiece Hearth"],
			["Showpiece", "Grand Twin-Flight Staircase"],
			["Grounds", "Tiered Brass Fountain & Sentinels"],
			["Floors", "2"],
			["Vibe", "Cozy Mountain Luxury"],
		],
	}
