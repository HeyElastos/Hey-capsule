class_name VerseBuildingBeachHouse
extends RefCounted
## Hey Verse — PREMIUM procedural BUILDING: "Azure Tide Beach House" (Rare).
##
## A LUXURY stilted coastal retreat raised on engineered timber pilings over a
## reflecting tide pool in pale sand: a deep wraparound sun-deck with a brass-capped
## sea-glass balustrade, a cantilevered upper balcony, fluted columns, dormer
## windows, louvered turquoise shutters, deep shading eaves and a brass ridge.
## The wide front opens (NO front wall) onto a breezy, walkable great-room with
## showpiece fixtures — a sweeping driftwood grand stair to a mezzanine, a tiered
## driftwood-and-brass chandelier, a glowing sea-stone fireplace, a marble-topped
## island, a reflecting fountain on the deck, a leaning palm and a propped surfboard.
##
## Light driftwood-toned wood + white stucco + turquoise sea-glass + brushed-brass
## accents used tastefully, with warm glowing windows. Sold as an NFT, placed on a
## player's land.
##
## Built at the ORIGIN, sand pad at y=0, entrance (open glass wall) faces +z so the
## camera looks straight in. A ~1.4-unit chibi-robot climbs the deck stair, walks
## the deck and through the open front into a clear, furnishable great-room.
##
## Pure procedural primitives — no .glb, no preload of other .gd. The shared toon +
## outline shaders are loaded by RESOURCE PATH with ResourceLoader.exists() guards
## and a StandardMaterial3D fallback, so this module parses and runs standalone.

const TOON_PATH := "res://toon.gdshader"
const OUTLINE_PATH := "res://outline.gdshader"

static var _outline_mat: ShaderMaterial


# ───────────────────────────── shared material helpers ──────────────────────

## Cel material + inverted-hull outline — the Verse "designed" look on solids.
## Falls back to a plain StandardMaterial3D if the shaders aren't on disk, so the
## class always parses + runs standalone.
static func _toon(c: Color, rim := 0.3, outline := true, spec := 0.0, wind := 0.0, wind_h := 1.0) -> Material:
	if ResourceLoader.exists(TOON_PATH):
		var sh := ResourceLoader.load(TOON_PATH)
		if sh is Shader:
			var m := ShaderMaterial.new()
			m.shader = sh
			m.set_shader_parameter("albedo", c)
			m.set_shader_parameter("rim_strength", rim)
			m.set_shader_parameter("spec_strength", spec)
			m.set_shader_parameter("wind_strength", wind)
			m.set_shader_parameter("wind_height", wind_h)
			if outline:
				if _outline_mat == null and ResourceLoader.exists(OUTLINE_PATH):
					var osh := ResourceLoader.load(OUTLINE_PATH)
					if osh is Shader:
						_outline_mat = ShaderMaterial.new()
						_outline_mat.shader = osh
						_outline_mat.set_shader_parameter("thickness", 0.016)
						_outline_mat.set_shader_parameter("line_color", Color(0.07, 0.09, 0.13, 1.0))
				if _outline_mat != null:
					m.next_pass = _outline_mat
			return m
	# Fallback — keeps colour + a soft roughness if shaders are missing.
	var fm := StandardMaterial3D.new()
	fm.albedo_color = c
	fm.roughness = 0.85
	fm.metallic = 0.0
	return fm


## Polished metal — brushed brass / chrome (strong rim + spec dot).
static func _metal(c: Color, spec := 0.7) -> Material:
	return _toon(c, 0.5, true, spec)


## Soft satin gloss — painted woodwork, lacquered trim (a touch of spec, no harsh shine).
static func _gloss(c: Color, spec := 0.25) -> Material:
	return _toon(c, 0.4, true, spec)


## Translucent sea-glass / turquoise pane — alpha, lightly emissive, no shadow.
static func _glass(c: Color, alpha := 0.34) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.08
	m.metallic = 0.1
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.16
	m.cull_mode = BaseMaterial3D.CULL_DISABLED
	return m


## A mirror-bright reflecting water surface for the fountain / tide pool.
static func _water(c: Color, alpha := 0.7) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.04
	m.metallic = 0.6
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.22
	return m


## Unshaded warm emissive — glowing windows, lantern bulbs.
static func _glow(c: Color, energy := 1.5) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


# ───────────────────────────── primitive helpers ────────────────────────────

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


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, s := Vector3.ONE) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 18
	sm.rings = 9
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = s
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, ring_seg := 12) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 18
	tm.ring_segments = ring_seg
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


# ───────────────────────────── sub-assemblies ───────────────────────────────

## One timber piling: a thick post on a stubby concrete pad footer.
static func _piling(parent: Node3D, pos: Vector3, h: float, wood: Material, footer: Material) -> void:
	_box(parent, Vector3(0.42, 0.22, 0.42), footer, Vector3(pos.x, 0.11, pos.z))
	_cyl(parent, 0.16, 0.18, h, wood, Vector3(pos.x, 0.22 + h * 0.5, pos.z), Vector3.ZERO, 10)
	# diagonal cross-brace stub for that "engineered stilt" read
	_box(parent, Vector3(0.10, 0.10, 0.9), wood, Vector3(pos.x, 0.22 + h * 0.35, pos.z), Vector3(0.5, 0, 0))


## A slender fluted column with a brass base ring + capital — the luxury upgrade
## on the deck-facing pilings; reads as architecture, not just a post.
static func _column(parent: Node3D, pos: Vector3, h: float, shaft: Material, brass: Material) -> void:
	# concrete-style pad footer + brass base ring
	_box(parent, Vector3(0.5, 0.18, 0.5), shaft, Vector3(pos.x, pos.y + 0.09, pos.z))
	_torus(parent, 0.04, 0.26, brass, Vector3(pos.x, pos.y + 0.22, pos.z), Vector3.ZERO, 12)
	# fluted shaft (slight taper)
	_cyl(parent, 0.18, 0.22, h, shaft, Vector3(pos.x, pos.y + 0.22 + h * 0.5, pos.z), Vector3.ZERO, 14)
	for i: int in range(8):
		var a: float = TAU * float(i) / 8.0
		_box(parent, Vector3(0.03, h * 0.92, 0.03), brass, Vector3(pos.x + cos(a) * 0.21, pos.y + 0.22 + h * 0.5, pos.z + sin(a) * 0.21))
	# brass collar + capital block under the deck
	_torus(parent, 0.04, 0.27, brass, Vector3(pos.x, pos.y + 0.22 + h, pos.z), Vector3.ZERO, 12)
	_box(parent, Vector3(0.5, 0.14, 0.5), shaft, Vector3(pos.x, pos.y + 0.22 + h + 0.08, pos.z))


## A louvered shutter (turquoise slats in a wood frame), hinged open-ish on a wall.
static func _shutter(parent: Node3D, pos: Vector3, w: float, h: float, frame: Material, slat: Material, rot_y: float) -> void:
	var g := Node3D.new()
	g.position = pos
	g.rotation = Vector3(0, rot_y, 0)
	parent.add_child(g)
	_box(g, Vector3(w, h, 0.05), frame, Vector3.ZERO)
	var n := 6
	for i: int in range(n):
		var yy: float = -h * 0.5 + h * 0.5 / n + h * float(i) / n
		_box(g, Vector3(w * 0.84, h * 0.7 / n, 0.07), slat, Vector3(0.0, yy, 0.02), Vector3(0.35, 0, 0))


## A hanging deck lantern: brass cage + warm glowing bulb.
static func _lantern(parent: Node3D, pos: Vector3, brass: Material) -> void:
	_cyl(parent, 0.02, 0.02, 0.5, brass, Vector3(pos.x, pos.y + 0.45, pos.z), Vector3.ZERO, 6)
	_box(parent, Vector3(0.20, 0.04, 0.20), brass, Vector3(pos.x, pos.y + 0.20, pos.z))
	_box(parent, Vector3(0.20, 0.04, 0.20), brass, Vector3(pos.x, pos.y - 0.16, pos.z))
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_cyl(parent, 0.012, 0.012, 0.34, brass, Vector3(pos.x + sx * 0.09, pos.y + 0.02, pos.z + sz * 0.09), Vector3.ZERO, 5)
	_ball(parent, 0.09, _glow(Color(1.0, 0.82, 0.5), 2.2), Vector3(pos.x, pos.y + 0.02, pos.z))


## A small carved sea-bird statue on a brass plinth — a tasteful luxury showpiece
## used to flank the grand stair / entrance.
static func _heron_statue(parent: Node3D, pos: Vector3, stone: Material, brass: Material) -> void:
	var g := Node3D.new()
	g.position = pos
	parent.add_child(g)
	# plinth: brass-banded stone pedestal
	_box(g, Vector3(0.5, 0.12, 0.5), stone, Vector3(0, 0.06, 0))
	_box(g, Vector3(0.42, 0.5, 0.42), stone, Vector3(0, 0.37, 0))
	_torus(g, 0.03, 0.24, brass, Vector3(0, 0.62, 0), Vector3.ZERO, 12)
	# stylised standing heron in pale stone
	_cyl(g, 0.04, 0.05, 0.55, stone, Vector3(0.0, 0.95, 0.0), Vector3.ZERO, 8)            # leg
	_ball(g, 0.18, stone, Vector3(0.0, 1.28, 0.02), Vector3(0.8, 1.3, 1.0))               # body
	_cyl(g, 0.03, 0.05, 0.4, stone, Vector3(0.0, 1.55, 0.08), Vector3(-0.5, 0, 0), 8)     # neck
	_ball(g, 0.07, stone, Vector3(0.0, 1.72, 0.22))                                       # head
	_prism(g, Vector3(0.05, 0.05, 0.22), brass, Vector3(0.0, 1.72, 0.36), Vector3(PI * 0.5, 0, 0))  # beak
	# folded wing hint
	_prism(g, Vector3(0.04, 0.3, 0.34), stone, Vector3(0.13, 1.3, -0.02), Vector3(0, 0, -0.4))


## A tiered reflecting fountain: stone basin, brass bowl, water disc + jet plume.
static func _fountain(parent: Node3D, pos: Vector3, stone: Material, brass: Material, water: Material) -> void:
	var g := Node3D.new()
	g.position = pos
	parent.add_child(g)
	# lower basin ring + water
	_cyl(g, 0.95, 1.0, 0.34, stone, Vector3(0, 0.17, 0), Vector3.ZERO, 26)
	_torus(g, 0.06, 1.0, brass, Vector3(0, 0.34, 0), Vector3.ZERO, 16)
	_cyl(g, 0.86, 0.86, 0.06, water, Vector3(0, 0.31, 0), Vector3.ZERO, 26)
	# central pedestal + upper bowl + upper water
	_cyl(g, 0.14, 0.18, 0.5, stone, Vector3(0, 0.55, 0), Vector3.ZERO, 12)
	_cyl(g, 0.42, 0.30, 0.16, brass, Vector3(0, 0.82, 0), Vector3.ZERO, 18)
	_cyl(g, 0.36, 0.36, 0.05, water, Vector3(0, 0.91, 0), Vector3.ZERO, 18)
	# jet plume + droplets, glowing faintly turquoise
	var jet := _glow(Color(0.7, 0.95, 0.95), 0.8)
	_cyl(g, 0.02, 0.05, 0.5, jet, Vector3(0, 1.18, 0), Vector3.ZERO, 8)
	for i: int in range(5):
		var a: float = TAU * float(i) / 5.0
		_ball(g, 0.04, jet, Vector3(cos(a) * 0.22, 1.1 + (i % 2) * 0.12, sin(a) * 0.22))


## The leaning beach palm: curved driftwood trunk + drooping fronds + coconuts.
static func _palm(parent: Node3D, pos: Vector3) -> void:
	var bark := _toon(Color(0.55, 0.43, 0.30), 0.3, true, 0.0, 0.2, 5.0)
	var frond := _toon(Color(0.32, 0.62, 0.34), 0.45, true, 0.1, 0.9, 6.0)
	var nut := _gloss(Color(0.40, 0.27, 0.18), 0.2)
	var g := Node3D.new()
	g.position = pos
	parent.add_child(g)
	# curved trunk — a stack of segments leaning toward +x
	var ty := 0.0
	var tx := 0.0
	var n := 9
	for i: int in range(n):
		var t: float = float(i) / float(n - 1)
		var r: float = lerp(0.20, 0.11, t)
		var seg_h := 0.62
		var lean: float = 0.16 + t * 0.10
		_cyl(g, r * 0.9, r, seg_h, bark, Vector3(tx + lean * 0.5, ty + seg_h * 0.5, 0.0), Vector3(0, 0, -lean), 9)
		tx += lean
		ty += seg_h * cos(lean)
	var crown := Vector3(tx, ty, 0.0)
	# crown ring of fronds
	var fcount := 8
	for i: int in range(fcount):
		var ang: float = TAU * float(i) / float(fcount)
		var fr := Node3D.new()
		fr.position = crown
		fr.rotation = Vector3(0.55, ang, 0)
		g.add_child(fr)
		# each frond = a few tapering blades sweeping out + drooping
		for j: int in range(3):
			var blade_len: float = 1.5 - j * 0.22
			var off: float = 0.4 + j * 0.42
			_prism(fr, Vector3(0.34 - j * 0.06, 0.04, blade_len), frond, Vector3(0, -off * 0.35, off), Vector3(-0.45 - j * 0.12, 0, 0))
	# coconuts
	for k: int in range(3):
		var a: float = TAU * float(k) / 3.0
		_ball(g, 0.11, nut, crown + Vector3(cos(a) * 0.16, -0.12, sin(a) * 0.16))


## A clipped topiary ball on a driftwood stem in a brass-banded planter pot —
## crisp landscaping for the deck corners.
static func _topiary(parent: Node3D, pos: Vector3, pot: Material, brass: Material, leaf: Material) -> void:
	var g := Node3D.new()
	g.position = pos
	parent.add_child(g)
	_cyl(g, 0.26, 0.20, 0.36, pot, Vector3(0, 0.18, 0), Vector3.ZERO, 14)
	_torus(g, 0.03, 0.27, brass, Vector3(0, 0.34, 0), Vector3.ZERO, 14)
	_cyl(g, 0.05, 0.06, 0.4, _gloss(Color(0.45, 0.35, 0.24), 0.1), Vector3(0, 0.55, 0), Vector3.ZERO, 8)
	_ball(g, 0.32, leaf, Vector3(0, 0.86, 0), Vector3(1.0, 0.9, 1.0))
	_ball(g, 0.2, leaf, Vector3(0.16, 1.06, 0.05), Vector3(1.0, 0.9, 1.0))


# ───────────────────────────── main build ───────────────────────────────────

static func build() -> Node3D:
	var root := Node3D.new()
	root.name = "BeachHouse"

	# ── palette ───────────────────────────────────────────────────────────────
	var wood := _gloss(Color(0.78, 0.66, 0.48), 0.22)        # warm driftwood cladding
	var wood_dk := _gloss(Color(0.55, 0.43, 0.30), 0.18)     # darker frame timber
	var white := _toon(Color(0.95, 0.95, 0.92), 0.32)        # white stucco / trim
	var deck := _gloss(Color(0.72, 0.60, 0.43), 0.18)        # deck planking
	var turq := _toon(Color(0.20, 0.70, 0.68), 0.4, true, 0.2)  # turquoise accent
	var brass := _metal(Color(0.86, 0.70, 0.32), 0.85)       # brushed brass trim
	var brass_dk := _metal(Color(0.66, 0.50, 0.20), 0.7)     # darker brass for depth
	var footer := _toon(Color(0.62, 0.62, 0.60), 0.25)       # concrete footer
	var stone := _toon(Color(0.90, 0.88, 0.82), 0.28)        # pale cast stone (statues/fountain)
	var roof := _gloss(Color(0.90, 0.91, 0.90), 0.2)         # pale standing-seam roof
	var glass := _glass(Color(0.40, 0.82, 0.80), 0.30)       # turquoise sea-glass
	var win_glow := _glow(Color(1.0, 0.86, 0.58), 1.6)       # warm interior light
	var sand := _toon(Color(0.91, 0.85, 0.66), 0.2)          # pale beach sand
	var floor_mat := _gloss(Color(0.83, 0.73, 0.56), 0.16)   # light interior floor
	var water := _water(Color(0.30, 0.74, 0.78), 0.7)        # reflecting water
	var topiary_leaf := _toon(Color(0.32, 0.58, 0.36), 0.4, true, 0.05, 0.4, 5.0)

	# footprint of the house box itself (deck extends beyond on the +z side)
	var W := 9.0      # width  (x)
	var D := 7.0      # depth  (z), house body
	var STILT := 1.4  # piling height (raised over sand)
	var FLOOR := STILT + 0.22          # interior floor top ≈ y 1.62
	var WALL := 3.0   # interior ceiling height
	var ceil_y := FLOOR + WALL

	# ── 0. SAND PAD + reflecting tide pool ─────────────────────────────────────
	_cyl(root, 8.8, 9.2, 0.10, sand, Vector3(0, 0.05, 0.6), Vector3.ZERO, 30)
	# raised stone curb ring + a calm reflecting tide pool the house stands over
	_torus(root, 0.18, 5.4, stone, Vector3(0, 0.12, -3.2), Vector3.ZERO, 18)
	_cyl(root, 5.2, 5.2, 0.05, water, Vector3(0, 0.10, -3.2), Vector3.ZERO, 28)
	# a hint of wet/turquoise shoreline behind (−z)
	_cyl(root, 5.0, 5.0, 0.04, _glass(Color(0.30, 0.74, 0.78), 0.45), Vector3(0, 0.06, -6.4), Vector3.ZERO, 24)

	# ── 1. PILINGS (raise the house over the sand) ─────────────────────────────
	var px := W * 0.5 - 0.6
	var pz0 := -D * 0.5 + 0.6
	var pz1 := D * 0.5 - 0.6
	for sx: float in [-1.0, -0.34, 0.34, 1.0]:
		for pz: float in [pz0, 0.0, pz1]:
			_piling(root, Vector3(sx * px, 0, pz), STILT, wood_dk, footer)
	# the FRONT-facing pilings become fluted brass-trimmed columns (luxury read)
	for sx: float in [-1.0, 1.0]:
		_column(root, Vector3(sx * px, 0, pz1), STILT - 0.22, stone, brass)
	# under-floor beams (ledger) tying the pilings
	for pz: float in [pz0, pz1]:
		_box(root, Vector3(W - 0.6, 0.22, 0.30), wood_dk, Vector3(0, STILT + 0.08, pz))
	_box(root, Vector3(0.30, 0.22, D - 0.6), wood_dk, Vector3(-px, STILT + 0.08, 0))
	_box(root, Vector3(0.30, 0.22, D - 0.6), wood_dk, Vector3(px, STILT + 0.08, 0))

	# ── 2. FLOOR PLATE (sub-deck + interior floor) ─────────────────────────────
	_box(root, Vector3(W, 0.22, D), wood, Vector3(0, STILT + 0.11, 0))
	_box(root, Vector3(W - 0.4, 0.06, D - 0.4), floor_mat, Vector3(0, FLOOR + 0.01, 0))
	# subtle plank seams on the interior floor
	for i: int in range(5):
		var z: float = -D * 0.5 + 0.9 + i * (D - 1.8) / 4.0
		_box(root, Vector3(W - 0.6, 0.012, 0.03), wood_dk, Vector3(0, FLOOR + 0.045, z))
	# a brass-inlaid threshold band where deck meets interior (luxury detail)
	_box(root, Vector3(W - 0.6, 0.02, 0.08), brass, Vector3(0, FLOOR + 0.05, D * 0.5 - 0.2))

	# ── 3. WRAP DECK (extends past the house on +z; the camera-facing side) ────
	var DECK_Z := D * 0.5 + 3.6   # outer edge of front deck
	_box(root, Vector3(W + 0.6, 0.18, 3.8), deck, Vector3(0, FLOOR - 0.04, D * 0.5 + 1.9))
	# deck plank seams
	for i: int in range(7):
		var dx: float = -W * 0.5 + 0.7 + i * (W) / 6.0
		_box(root, Vector3(0.03, 0.02, 3.7), wood_dk, Vector3(dx, FLOOR + 0.005, D * 0.5 + 1.9))
	# deck support pilings out front
	for sx: float in [-1.0, 1.0]:
		_piling(root, Vector3(sx * (W * 0.5 - 0.3), 0, DECK_Z - 0.4), STILT, wood_dk, footer)

	# ── 3a. DECK RAILING (posts + caps + turquoise glass infill), open at stair ─
	var rail_top := FLOOR + 1.0
	# side rails of the deck (with brass post finials)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.10, 1.0, 3.8), wood_dk, Vector3(sx * (W * 0.5 + 0.25), FLOOR + 0.5, D * 0.5 + 1.9))
		_box(root, Vector3(0.16, 0.10, 3.8), brass, Vector3(sx * (W * 0.5 + 0.25), rail_top, D * 0.5 + 1.9))
		_box(root, Vector3(0.10, 0.86, 3.6), glass, Vector3(sx * (W * 0.5 + 0.20), FLOOR + 0.5, D * 0.5 + 1.9))
		_ball(root, 0.09, brass, Vector3(sx * (W * 0.5 + 0.25), rail_top + 0.08, D * 0.5 + 0.1))
		_ball(root, 0.09, brass, Vector3(sx * (W * 0.5 + 0.25), rail_top + 0.08, DECK_Z - 0.05))
	# front rail segments (left + right of central stair gap)
	for side: float in [-1.0, 1.0]:
		var cx: float = side * (W * 0.25 + 0.5)
		_box(root, Vector3(W * 0.5 - 1.0, 0.86, 0.10), glass, Vector3(cx, FLOOR + 0.5, DECK_Z - 0.05))
		_box(root, Vector3(W * 0.5 - 0.8, 0.10, 0.16), brass, Vector3(cx, rail_top, DECK_Z - 0.05))
		# corner posts
		_box(root, Vector3(0.12, 1.0, 0.12), wood_dk, Vector3(side * (W * 0.5 + 0.2), FLOOR + 0.5, DECK_Z - 0.05))

	# ── 3b. GRAND STAIR down to the sand (front-center, wider + flanked statues) ─
	var steps := 6
	for i: int in range(steps):
		var sy: float = FLOOR - (i + 1) * (FLOOR / float(steps + 1))
		var sz: float = DECK_Z + 0.25 + i * 0.46
		_box(root, Vector3(2.8, 0.10, 0.54), deck, Vector3(0, sy + 0.05, sz))
		_box(root, Vector3(2.8, 0.36, 0.12), wood_dk, Vector3(0, sy - 0.14, sz - 0.22))
		_box(root, Vector3(2.6, 0.02, 0.5), brass, Vector3(0, sy + 0.11, sz))  # brass nosing
	# stair stringers / mini-rails with brass caps
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.12, 0.95, 3.2), wood_dk, Vector3(sx * 1.45, FLOOR * 0.45, DECK_Z + 1.6), Vector3(-0.52, 0, 0))
		_box(root, Vector3(0.12, 0.10, 3.2), brass, Vector3(sx * 1.45, FLOOR * 0.45 + 0.55, DECK_Z + 1.6), Vector3(-0.52, 0, 0))
	# carved heron statues flanking the foot of the grand stair (luxury showpiece)
	_heron_statue(root, Vector3(-1.9, 0.1, DECK_Z + 2.9), stone, brass)
	_heron_statue(root, Vector3(1.9, 0.1, DECK_Z + 2.9), stone, brass)

	# ── 4. WALLS (FRONT OMITTED — open glass wall facing +z) ────────────────────
	var wall_t := 0.18
	# back wall (−z), solid stucco with two glowing windows
	_box(root, Vector3(W, WALL, wall_t), white, Vector3(0, FLOOR + WALL * 0.5, -D * 0.5))
	# side walls (±x)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(wall_t, WALL, D), wood, Vector3(sx * W * 0.5, FLOOR + WALL * 0.5, 0))
	# a slim brass belt course banding the side + back walls at sill height
	_box(root, Vector3(W + 0.04, 0.05, wall_t + 0.02), brass_dk, Vector3(0, FLOOR + 0.9, -D * 0.5))
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(wall_t + 0.02, 0.05, D), brass_dk, Vector3(sx * W * 0.5, FLOOR + 0.9, 0))

	# ── 4a. low FRONT threshold parapet (so the interior reads as a room) ──────
	_box(root, Vector3(W, 0.45, wall_t), wood, Vector3(0, FLOOR + 0.22, D * 0.5))
	_box(root, Vector3(W, 0.06, 0.22), brass, Vector3(0, FLOOR + 0.45, D * 0.5))
	# slim front corner posts framing the open wall, with a top header beam
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.20, WALL, 0.20), wood_dk, Vector3(sx * (W * 0.5 - 0.1), FLOOR + WALL * 0.5, D * 0.5 - 0.1))
		# brass-banded base + capital on the entry posts
		_box(root, Vector3(0.26, 0.1, 0.26), brass, Vector3(sx * (W * 0.5 - 0.1), FLOOR + 0.12, D * 0.5 - 0.1))
		_box(root, Vector3(0.26, 0.1, 0.26), brass, Vector3(sx * (W * 0.5 - 0.1), ceil_y - 0.4, D * 0.5 - 0.1))
	_box(root, Vector3(W, 0.30, 0.22), wood_dk, Vector3(0, ceil_y - 0.15, D * 0.5 - 0.1))
	# a central front mullion (open glass great-room wall split into bays)
	_box(root, Vector3(0.14, WALL - 0.7, 0.10), wood_dk, Vector3(0, FLOOR + 0.45 + (WALL - 0.7) * 0.5, D * 0.5))
	# clerestory glass bands above the open front (turquoise tint)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(W * 0.46, 0.7, 0.05), glass, Vector3(sx * W * 0.24, ceil_y - 0.55, D * 0.5))

	# ── 5. WINDOWS + glow on the back & side walls ─────────────────────────────
	# back wall windows
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(1.7, 1.4, 0.06), win_glow, Vector3(sx * 2.1, FLOOR + 1.5, -D * 0.5 + 0.04))
		_box(root, Vector3(1.9, 1.6, 0.10), white, Vector3(sx * 2.1, FLOOR + 1.5, -D * 0.5 - 0.01))  # trim frame
		_box(root, Vector3(1.95, 0.06, 0.12), brass, Vector3(sx * 2.1, FLOOR + 2.32, -D * 0.5 - 0.02))  # brass window head
		# louvered turquoise shutters flanking
		_shutter(root, Vector3(sx * 3.15, FLOOR + 1.5, -D * 0.5 + 0.10), 0.5, 1.5, wood_dk, turq, sx * -0.35)
	# side wall windows + shutters
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.06, 1.3, 1.8), win_glow, Vector3(sx * (W * 0.5 - 0.04), FLOOR + 1.5, -0.6))
		_shutter(root, Vector3(sx * (W * 0.5 - 0.12), FLOOR + 1.5, 0.5), 0.5, 1.4, wood_dk, turq, sx * 1.2 + PI * 0.5)

	# ── 5a. CANTILEVERED UPPER BALCONY (back, over the tide pool) ──────────────
	var bal_y := ceil_y + 0.1
	_box(root, Vector3(W - 1.4, 0.16, 1.6), deck, Vector3(0, bal_y, -D * 0.5 - 0.7))
	# balcony sea-glass balustrade + brass cap + finials
	_box(root, Vector3(W - 1.4, 0.7, 0.08), glass, Vector3(0, bal_y + 0.42, -D * 0.5 - 1.45))
	_box(root, Vector3(W - 1.3, 0.08, 0.14), brass, Vector3(0, bal_y + 0.78, -D * 0.5 - 1.45))
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.08, 0.7, 1.6), glass, Vector3(sx * (W * 0.5 - 0.74), bal_y + 0.42, -D * 0.5 - 0.7))
		_box(root, Vector3(0.12, 0.92, 0.12), wood_dk, Vector3(sx * (W * 0.5 - 0.7), bal_y + 0.5, -D * 0.5 - 1.4))
		_ball(root, 0.08, brass, Vector3(sx * (W * 0.5 - 0.7), bal_y + 1.0, -D * 0.5 - 1.4))
	# french door + glow opening onto the balcony
	_box(root, Vector3(1.5, 1.9, 0.06), win_glow, Vector3(0, FLOOR + 1.7, -D * 0.5 + 0.04))
	_box(root, Vector3(1.7, 2.05, 0.10), white, Vector3(0, FLOOR + 1.7, -D * 0.5 - 0.02))

	# ── 6. ROOF (low gabled "beach" pitch + deep shading eaves + dormers) ──────
	var roof_w := W + 1.2
	var roof_d := D + 1.0
	var eave_y := ceil_y + 0.05
	# eave fascia board all round
	_box(root, Vector3(roof_w, 0.22, 0.18), wood_dk, Vector3(0, eave_y, roof_d * 0.5 - 0.3))
	_box(root, Vector3(roof_w, 0.22, 0.18), wood_dk, Vector3(0, eave_y, -roof_d * 0.5 + 0.3))
	# brass drip-edge under the fascia (luxury sparkle)
	_box(root, Vector3(roof_w, 0.04, 0.06), brass, Vector3(0, eave_y - 0.12, roof_d * 0.5 - 0.22))
	# two roof slopes meeting at a ridge running along x
	var pitch := 1.4
	var slope_len: float = sqrt(pow(roof_d * 0.5, 2.0) + pow(pitch, 2.0))
	var ang := atan2(pitch, roof_d * 0.5)
	_box(root, Vector3(roof_w, 0.12, slope_len + 0.2), roof, Vector3(0, eave_y + pitch * 0.5, roof_d * 0.25), Vector3(ang, 0, 0))
	_box(root, Vector3(roof_w, 0.12, slope_len + 0.2), roof, Vector3(0, eave_y + pitch * 0.5, -roof_d * 0.25), Vector3(-ang, 0, 0))
	# ridge cap (brass) + ridge finials
	_box(root, Vector3(roof_w + 0.1, 0.12, 0.16), brass, Vector3(0, eave_y + pitch + 0.04, 0))
	for sx: float in [-1.0, 1.0]:
		_ball(root, 0.12, brass, Vector3(sx * roof_w * 0.48, eave_y + pitch + 0.16, 0))
	# standing-seam ridge lines for texture
	for i: int in range(9):
		var lx: float = -roof_w * 0.5 + 0.6 + i * (roof_w - 1.2) / 8.0
		_box(root, Vector3(0.04, 0.04, slope_len), wood_dk, Vector3(lx, eave_y + pitch * 0.5 + 0.06, roof_d * 0.25), Vector3(ang, 0, 0))
		_box(root, Vector3(0.04, 0.04, slope_len), wood_dk, Vector3(lx, eave_y + pitch * 0.5 + 0.06, -roof_d * 0.25), Vector3(-ang, 0, 0))
	# little roof gable triangles closing the ends
	for sx: float in [-1.0, 1.0]:
		_prism(root, Vector3(0.16, pitch, roof_d), white, Vector3(sx * roof_w * 0.5, eave_y + pitch * 0.5, 0), Vector3(PI * 0.5, 0, PI * 0.5 * sx))
	# ── 6a. DORMER WINDOWS on the front roof slope (glowing) ───────────────────
	for sx: float in [-1.0, 1.0]:
		var dz := roof_d * 0.22
		var dy := eave_y + pitch * 0.42
		var dg := Node3D.new()
		dg.position = Vector3(sx * roof_w * 0.24, dy, dz)
		root.add_child(dg)
		_box(dg, Vector3(0.9, 0.7, 0.7), white, Vector3.ZERO)
		_box(dg, Vector3(0.6, 0.5, 0.06), win_glow, Vector3(0, 0.02, 0.36))
		_box(dg, Vector3(0.68, 0.06, 0.1), brass, Vector3(0, 0.28, 0.36))
		_prism(dg, Vector3(1.0, 0.45, 0.7), roof, Vector3(0, 0.5, 0), Vector3(0, 0, 0))

	# ── 7. INTERIOR — open, walkable great-room with showpiece fixtures ────────
	# ceiling underside (so it reads enclosed from inside)
	_box(root, Vector3(W - 0.4, 0.10, D - 0.4), white, Vector3(0, ceil_y - 0.05, 0))
	# exposed driftwood ceiling beams
	for i: int in range(4):
		var bz: float = -D * 0.5 + 1.2 + i * (D - 2.4) / 3.0
		_box(root, Vector3(W - 0.6, 0.16, 0.16), wood_dk, Vector3(0, ceil_y - 0.18, bz))

	# 7a. partial interior wall splitting off a small back room (kitchen nook),
	#     kept LOW/partial so the great-room stays open + furnishable.
	_box(root, Vector3(0.16, WALL - 1.0, D * 0.42), white, Vector3(-W * 0.18, FLOOR + (WALL - 1.0) * 0.5, -D * 0.5 + D * 0.21))

	# 7b. KITCHEN ISLAND (back-left) — wood base + white stone top + brass tap
	var island := Node3D.new()
	root.add_child(island)
	_box(island, Vector3(2.0, 0.9, 1.0), wood, Vector3(-W * 0.28, FLOOR + 0.45, -D * 0.5 + 1.3))
	_box(island, Vector3(2.2, 0.12, 1.2), white, Vector3(-W * 0.28, FLOOR + 0.96, -D * 0.5 + 1.3))
	_box(island, Vector3(2.1, 0.02, 1.1), brass, Vector3(-W * 0.28, FLOOR + 1.03, -D * 0.5 + 1.3))  # brass counter trim
	_cyl(island, 0.05, 0.05, 0.45, brass, Vector3(-W * 0.28 + 0.6, FLOOR + 1.2, -D * 0.5 + 1.3), Vector3.ZERO, 8)
	_torus(island, 0.03, 0.10, brass, Vector3(-W * 0.28 + 0.6, FLOOR + 1.42, -D * 0.5 + 1.3), Vector3(PI * 0.5, 0, 0), 8)

	# 7c. FIREPLACE / built-in shelving niche (back-right) — sea-stone surround,
	#     warm glowing hearth, brass mantel.
	var fp := Node3D.new()
	root.add_child(fp)
	_box(fp, Vector3(2.0, WALL - 0.4, 0.4), white, Vector3(W * 0.26, FLOOR + (WALL - 0.4) * 0.5, -D * 0.5 + 0.25))
	_box(fp, Vector3(2.1, 0.06, 0.42), brass, Vector3(W * 0.26, FLOOR + WALL - 0.5, -D * 0.5 + 0.25))  # brass top band
	_box(fp, Vector3(1.2, 0.9, 0.3), _toon(Color(0.2, 0.2, 0.22), 0.2), Vector3(W * 0.26, FLOOR + 0.55, -D * 0.5 + 0.35))
	_torus(fp, 0.04, 0.62, brass, Vector3(W * 0.26, FLOOR + 0.55, -D * 0.5 + 0.5), Vector3.ZERO, 14)  # brass hearth ring
	_box(fp, Vector3(1.0, 0.6, 0.2), _glow(Color(1.0, 0.55, 0.22), 2.0), Vector3(W * 0.26, FLOOR + 0.45, -D * 0.5 + 0.42))
	_box(fp, Vector3(2.2, 0.16, 0.5), wood_dk, Vector3(W * 0.26, FLOOR + 1.15, -D * 0.5 + 0.3))  # mantel

	# 7d. GRAND STAIR to a mezzanine landing (sweeping driftwood treads, the hero
	#     interior showpiece). Runs up the right-hand wall to a small loft shelf.
	var stair := Node3D.new()
	root.add_child(stair)
	var gsteps := 8
	for i: int in range(gsteps):
		var t: float = float(i) / float(gsteps)
		var ty: float = FLOOR + 0.18 + t * (WALL - 1.0)
		var tz: float = -D * 0.5 + 1.4 + t * (D - 2.6)
		_box(stair, Vector3(1.4, 0.12, 0.5), wood, Vector3(W * 0.32, ty, tz))
		_box(stair, Vector3(1.3, 0.02, 0.46), brass, Vector3(W * 0.32, ty + 0.07, tz))  # brass nosing
		_box(stair, Vector3(0.06, 0.7, 0.06), wood_dk, Vector3(W * 0.32 - 0.65, ty + 0.42, tz))  # baluster
		_ball(stair, 0.05, brass, Vector3(W * 0.32 - 0.65, ty + 0.78, tz))               # baluster cap
	# sweeping brass handrail rail line + a mezzanine landing shelf
	_box(stair, Vector3(0.06, 0.06, D - 2.4), brass, Vector3(W * 0.32 - 0.65, FLOOR + 1.5, -0.1), Vector3(-0.5, 0, 0))
	_box(stair, Vector3(1.5, 0.14, 1.6), wood, Vector3(W * 0.32, FLOOR + WALL - 0.85, D * 0.5 - 1.4))
	_box(stair, Vector3(1.5, 0.7, 0.08), glass, Vector3(W * 0.32, FLOOR + WALL - 0.45, D * 0.5 - 0.65))  # loft glass rail
	_box(stair, Vector3(1.5, 0.06, 0.12), brass, Vector3(W * 0.32, FLOOR + WALL - 0.12, D * 0.5 - 0.65))

	# 7e. CHANDELIER — tiered driftwood-ring fixture with warm bulbs over centre
	var chand := Node3D.new()
	root.add_child(chand)
	_cyl(chand, 0.03, 0.03, 0.5, wood_dk, Vector3(-0.4, ceil_y - 0.35, 0.3), Vector3.ZERO, 6)
	_torus(chand, 0.06, 0.5, wood_dk, Vector3(-0.4, ceil_y - 0.65, 0.3), Vector3.ZERO, 12)
	_torus(chand, 0.05, 0.3, brass, Vector3(-0.4, ceil_y - 0.85, 0.3), Vector3.ZERO, 12)
	for i: int in range(6):
		var a: float = TAU * float(i) / 6.0
		_ball(chand, 0.08, _glow(Color(1.0, 0.84, 0.55), 2.0), Vector3(-0.4 + cos(a) * 0.46, ceil_y - 0.72, 0.3 + sin(a) * 0.46))
	for i: int in range(3):
		var a2: float = TAU * float(i) / 3.0 + 0.5
		_ball(chand, 0.06, _glow(Color(1.0, 0.84, 0.55), 2.0), Vector3(-0.4 + cos(a2) * 0.26, ceil_y - 0.92, 0.3 + sin(a2) * 0.26))

	# 7f. a built-in window bench along the back, under the windows (furnishable)
	_box(root, Vector3(W - 1.2, 0.45, 0.6), wood, Vector3(0, FLOOR + 0.22, -D * 0.5 + 0.45))
	_box(root, Vector3(W - 1.4, 0.12, 0.5), _gloss(Color(0.40, 0.78, 0.74), 0.2), Vector3(0, FLOOR + 0.5, -D * 0.5 + 0.45))  # turquoise cushion

	# ── 8. EXTERIOR DRESSING — fountain, palm, surfboard, lanterns, landscaping ─
	# reflecting fountain showpiece on the deck, off to one side of the entrance
	_fountain(root, Vector3(W * 0.5 - 1.0, FLOOR + 0.02, D * 0.5 + 2.6), stone, brass, water)

	# leaning palm at the front-left of the deck
	_palm(root, Vector3(-W * 0.5 - 1.6, 0.1, D * 0.5 + 2.4))

	# clipped topiary planters dressing the deck corners + stair head (landscaping)
	_topiary(root, Vector3(-W * 0.5 - 0.1, FLOOR, DECK_Z - 0.5), wood_dk, brass, topiary_leaf)
	_topiary(root, Vector3(-1.7, FLOOR, DECK_Z - 0.4), wood_dk, brass, topiary_leaf)
	_topiary(root, Vector3(1.7, FLOOR, DECK_Z - 0.4), wood_dk, brass, topiary_leaf)

	# surfboard propped against the front-right rail post
	var board := Node3D.new()
	board.position = Vector3(W * 0.5 + 0.05, FLOOR + 1.0, DECK_Z - 0.2)
	board.rotation = Vector3(0, 0.15, 1.18)
	root.add_child(board)
	var board_mat := _gloss(Color(0.95, 0.93, 0.88), 0.3)
	_ball(board, 0.5, board_mat, Vector3.ZERO, Vector3(0.55, 2.6, 0.12))
	_box(board, Vector3(0.06, 1.5, 0.02), turq, Vector3(0, 0, 0.02))           # centre stripe
	_box(board, Vector3(0.18, 0.18, 0.06), turq, Vector3(0, -1.1, -0.05), Vector3(0.4, 0, 0))  # fin

	# hanging lanterns at the front deck corners (off the header)
	_lantern(root, Vector3(-W * 0.5 + 0.4, ceil_y - 0.2, D * 0.5 + 0.2), brass)
	_lantern(root, Vector3(W * 0.5 - 0.4, ceil_y - 0.2, D * 0.5 + 0.2), brass)

	# a planter of beachgrass at the deck edge (wind-swayed toon blades)
	var planter := Node3D.new()
	planter.position = Vector3(-W * 0.5 - 0.1, FLOOR, DECK_Z - 1.4)
	root.add_child(planter)
	_box(planter, Vector3(0.7, 0.4, 0.7), wood_dk, Vector3(0, 0.2, 0))
	_torus(planter, 0.03, 0.38, brass, Vector3(0, 0.4, 0), Vector3.ZERO, 14)
	var grass := _toon(Color(0.55, 0.72, 0.40), 0.4, true, 0.1, 0.7, 7.0)
	for i: int in range(10):
		var a: float = TAU * float(i) / 10.0
		var rr: float = 0.12 + (i % 3) * 0.06
		_prism(planter, Vector3(0.06, 0.6 + (i % 4) * 0.12, 0.04), grass, Vector3(cos(a) * rr, 0.7, sin(a) * rr), Vector3((i % 3 - 1) * 0.2, a, 0))

	# a couple of beach rocks + a starfish on the sand for life
	for r: Array in [[-2.4, -3.4, 0.4], [2.8, -2.6, 0.5], [3.4, 1.2, 0.3]]:
		var rx: float = r[0]
		var rz: float = r[1]
		var rr2: float = r[2]
		_ball(root, rr2, _toon(Color(0.6, 0.58, 0.55), 0.2), Vector3(rx, 0.12 + rr2 * 0.4, rz), Vector3(1.1, 0.7, 1.0))

	return root


static func meta() -> Dictionary:
	return {
		"id": "beach_house",
		"name": "Azure Tide Beach House",
		"tier": "Beach House",
		"rarity": "Rare",
		"description": "A luxury sun-bleached driftwood retreat raised on fluted brass-trimmed columns over a glassy reflecting tide pool. A deep wraparound sea-glass deck with a tiered fountain and a grand stair flanked by carved heron statues, a cantilevered upper balcony, dormer windows, louvered turquoise shutters, a leaning palm and a propped surfboard. The open glass front breezes straight into a light, airy great-room with a sweeping driftwood grand stair, a tiered chandelier and a glowing sea-stone fireplace — ready to furnish.",
		"footprint": [9.0, 7.0],
		"floors": 1,
		"attributes": [
			["Style", "Stilted Coastal Modern"],
			["Material", "Driftwood, White Stucco, Sea-Glass & Brass"],
			["Feature", "Grand Stair, Fountain, Balcony & Statues"],
			["Floors", 1],
			["Vibe", "Breezy Sun-Drenched Luxury Escape"]
		]
	}
