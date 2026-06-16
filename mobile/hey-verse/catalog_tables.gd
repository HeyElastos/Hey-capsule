class_name VerseCatalogTables
extends RefCounted
## Hey Verse — PREMIUM showroom catalog: theme "tables" (tables + storage).
##
## These are SELLABLE NFT items. Each one is a rich composite of many primitives
## (real legs, panels, hardware, trim) with a strong silhouette, careful
## proportions, premium materials (brushed metal, matte wood, glossy ceramic,
## glass, velvet) and tasteful EMISSION where something should glow. Rarity is
## expressed VISUALLY — higher tiers get gold trim, gemstones, glow and richer
## finishes you can read at a glance.
##
## Each item is a `static func build_<id>() -> Node3D` returning ONE
## self-contained Node3D, built at the ORIGIN and resting on the floor plane
## y=0, sized for the ~1.4-unit chibi-robot avatar (coffee top ~0.40,
## dining/desk top ~0.74, shelves up to ~1.8 tall).
##
## Style matches avatar.gd / home.gd — flat bright toon colors, soft rounded
## shapes, the shared cel shader (toon.gdshader) with an inverted-hull outline
## (outline.gdshader) as a next_pass. Mobile-cheap: the cel shader is the base
## look; StandardMaterial3D is used only for metal/glass/glow finishes that the
## toon shader can't express. Primitives only — no .glb, no art assets.
##
## Standalone: re-declares its own tiny material/mesh helpers so it parses + runs
## with NO dependency on home.gd or avatar.gd internals.

const TOON_SHADER := preload("res://toon.gdshader")
const OUTLINE_SHADER := preload("res://outline.gdshader")

# One shared outline pass (same trick avatar.gd uses) — cheap and consistent.
static var _outline_mat: ShaderMaterial


# ───────────────────────────── material helpers ────────────────────────────
# Re-declared here so the module parses + runs standalone.

## Stylized cel material with the inverted-hull outline as next_pass.
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


## Unshaded glowing material (lamp bulbs, gems, neon, screens, fireflies).
static func _glow(c: Color, energy := 1.2) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


## Polished metal — gold/brass/chrome. Lit (so it catches the toon light) with a
## tight spec highlight + an outline next_pass for the cohesive cartoon edge.
static func _metal(c: Color, rough := 0.18, metallic := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.metallic_specular = 0.9
	m.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	if _outline_mat == null:
		_outline_mat = ShaderMaterial.new()
		_outline_mat.shader = OUTLINE_SHADER
	m.next_pass = _outline_mat
	return m


## Coloured glass / crystal — translucent, glossy, faintly self-lit so gems and
## crystal tops read as premium on mobile (no refraction; cheap).
static func _glass(c: Color, alpha := 0.45, emit := 0.18) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.roughness = 0.05
	m.metallic = 0.0
	m.metallic_specular = 1.0
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = emit
	return m


## Soft cloth / velvet — toon, low rim, gentle outline (runners, felt liners).
static func _cloth(c: Color) -> ShaderMaterial:
	return _toon(c, 0.2, true, 0.0)


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


static func _cyl(parent: Node3D, top_r: float, bot_r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 16) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = top_r
	cm.bottom_radius = bot_r
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _sphere(parent: Node3D, r: float, scl: Vector3, mat: Material, pos: Vector3, seg := 16, rings := 8) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = seg
	sm.rings = rings
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.scale = scl
	mi.position = pos
	parent.add_child(mi)
	return mi


## A tapered, rounded leg made of a capsule (warm wooden default).
static func _leg(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var cm := CapsuleMesh.new()
	cm.radius = r
	cm.height = h
	cm.radial_segments = 12
	cm.rings = 4
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, ring_seg := 18, rings := 8) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = rings
	tm.ring_segments = ring_seg
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


## A rounded rectangular SLAB (box core + corner cylinders) — soft tabletops,
## chest bodies, drawer fronts. Returns the slab's own Node3D (centered at pos).
static func _slab(parent: Node3D, w: float, h: float, d: float, cr: float, mat: Material, pos: Vector3) -> Node3D:
	var n := Node3D.new()
	n.position = pos
	parent.add_child(n)
	_box(n, Vector3(w - 2.0 * cr, h, d), mat, Vector3.ZERO)
	_box(n, Vector3(w, h, d - 2.0 * cr), mat, Vector3.ZERO)
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_cyl(n, cr, cr, h, mat, Vector3(sx * (w * 0.5 - cr), 0, sz * (d * 0.5 - cr)), Vector3.ZERO, 10)
	return n


## A small flat faceted gem (an octahedron-ish double cone) — rarity sparkle.
static func _gem(parent: Node3D, r: float, mat: Material, pos: Vector3) -> void:
	var top := _cyl(parent, 0.0, r, r * 1.1, mat, pos + Vector3(0, r * 0.55, 0), Vector3.ZERO, 6)
	top.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bot := _cyl(parent, r, 0.0, r * 0.7, mat, pos + Vector3(0, -r * 0.35, 0), Vector3.ZERO, 6)
	bot.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## Faint round contact shadow (matches the avatar's blob) — sits at y≈0.011.
static func _shadow(parent: Node3D, r: float, pos := Vector3.ZERO) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = r
	disc.bottom_radius = r
	disc.height = 0.01
	disc.radial_segments = 20
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = Color(0, 0, 0, 0.11)
	var mi := MeshInstance3D.new()
	mi.mesh = disc
	mi.material_override = m
	mi.position = pos + Vector3(0, 0.011, 0)
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)


# Shared palette — warm woods + bright toy accents, tuned to the world look.
const WOOD := Color(0.72, 0.52, 0.33)
const WOOD_DARK := Color(0.50, 0.34, 0.21)
const WOOD_LIGHT := Color(0.84, 0.66, 0.44)
const OAK := Color(0.80, 0.60, 0.36)
const OAK_DARK := Color(0.60, 0.43, 0.25)
const BRASS := Color(0.96, 0.78, 0.38)
const GOLD := Color(1.0, 0.84, 0.40)
const CHROME := Color(0.78, 0.82, 0.88)
const BOOK_COLS: Array[Color] = [
	Color(0.85, 0.40, 0.35), Color(0.40, 0.60, 0.85), Color(0.45, 0.75, 0.45),
	Color(0.94, 0.78, 0.31), Color(0.76, 0.61, 1.0), Color(0.50, 0.89, 0.75),
]


# ════════════════════════════════ ITEMS ════════════════════════════════════


## 1 · MARBLE COFFEE TABLE — Rare. A low oval slab of veined white marble with a
## beveled gold-inlay edge, set on a sculptural brass cross-frame with brass leg
## collars + sabot feet, a smoked-glass under-shelf, and a styled tray (turned
## brass candlestick + glowing flame, a fruit dish, a faceted gem). Top ~0.40.
static func build_marble_coffee_table() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.66)
	var marble := _toon(Color(0.95, 0.95, 0.93), 0.4, true, 0.6)        # glossy white stone
	var marble_d := _toon(Color(0.88, 0.88, 0.90), 0.3, true, 0.4)      # bevel underside
	var vein := _toon(Color(0.62, 0.66, 0.72), 0.2)                      # grey veining
	var vein_gold := _glow(Color(0.95, 0.8, 0.45), 0.5)                  # gilded hairline vein
	var brass := _metal(BRASS, 0.16)
	var brass_dark := _metal(Color(0.74, 0.58, 0.26), 0.22)
	# rounded marble top (a wide soft slab) over a chamfered underslab (a real bevel)
	var top := _slab(root, 1.12, 0.07, 0.66, 0.18, marble, Vector3(0, 0.41, 0))
	_slab(root, 1.04, 0.04, 0.58, 0.16, marble_d, Vector3(0, 0.355, 0))  # chamfer step
	# inlaid veins across the top surface — two grey + one gilded hairline
	_box(top, Vector3(0.5, 0.012, 0.02), vein, Vector3(-0.1, 0.04, 0.08), Vector3(0, 0.5, 0))
	_box(top, Vector3(0.36, 0.012, 0.018), vein, Vector3(0.18, 0.04, -0.1), Vector3(0, -0.7, 0))
	_box(top, Vector3(0.24, 0.012, 0.015), vein, Vector3(-0.22, 0.04, -0.12), Vector3(0, 0.3, 0))
	_box(top, Vector3(0.44, 0.008, 0.01), vein_gold, Vector3(0.05, 0.041, -0.02), Vector3(0, 0.25, 0))
	_slab(root, 1.10, 0.016, 0.64, 0.17, brass, Vector3(0, 0.372, 0))    # gold inlay edge band
	# brass cross-frame legs (an X seen from above) + a turned center hub
	for diag in [1.0, -1.0]:
		var ang: float = diag * 0.62
		var beam := _box(root, Vector3(1.0, 0.05, 0.07), brass, Vector3(0, 0.34, 0), Vector3(0, ang, 0))
		beam.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_ON
		# the slanted leg dropping to the floor at each end + a brass collar + sabot
		for sx in [-1.0, 1.0]:
			var lx: float = cos(ang) * 0.46 * sx
			var lz: float = sin(ang) * 0.46 * sx
			var lg := _cyl(root, 0.03, 0.045, 0.34, brass, Vector3(lx, 0.18, lz), Vector3.ZERO, 10)
			lg.rotation.x = -lz * 0.12
			lg.rotation.z = lx * 0.12
			_torus(root, 0.03, 0.052, brass_dark, Vector3(lx, 0.30, lz), Vector3(PI / 2.0, 0, 0), 12, 4)  # collar
			_cyl(root, 0.05, 0.04, 0.04, brass_dark, Vector3(lx * 1.02, 0.02, lz * 1.02), Vector3.ZERO, 10)  # sabot foot
	_cyl(root, 0.05, 0.08, 0.05, brass_dark, Vector3(0, 0.34, 0), Vector3.ZERO, 12)   # center hub
	_sphere(root, 0.05, Vector3(1, 0.7, 1), brass, Vector3(0, 0.30, 0), 12, 6)        # hub finial
	# smoked-glass under-shelf slung in the frame + brass rim
	_slab(root, 0.6, 0.025, 0.4, 0.1, _glass(Color(0.78, 0.84, 0.92), 0.5, 0.1), Vector3(0, 0.18, 0))
	_torus(root, 0.19, 0.23, brass, Vector3(0, 0.166, 0), Vector3.ZERO, 22, 4)
	# styled tray on top: a brass dish with fruit + a turned candlestick + a gem
	_cyl(root, 0.13, 0.11, 0.02, brass, Vector3(0.22, 0.455, 0.06), Vector3.ZERO, 16)
	var dish_fruit := [Color(0.95, 0.45, 0.35), Color(0.55, 0.82, 0.4), Color(0.98, 0.78, 0.3)]
	for i in 3:
		var fa := TAU * i / 3.0
		_sphere(root, 0.035, Vector3.ONE, _toon(dish_fruit[i], 0.35, true, 0.3),
			Vector3(0.22 + cos(fa) * 0.045, 0.49, 0.06 + sin(fa) * 0.045), 10, 5)
	# a proper turned brass candlestick: foot + stem knop + cup + cream candle + flame
	_cyl(root, 0.05, 0.055, 0.02, brass, Vector3(-0.2, 0.455, -0.04), Vector3.ZERO, 14)
	_cyl(root, 0.018, 0.022, 0.07, brass, Vector3(-0.2, 0.50, -0.04), Vector3.ZERO, 10)
	_sphere(root, 0.028, Vector3.ONE, brass, Vector3(-0.2, 0.52, -0.04), 10, 5)        # knop
	_cyl(root, 0.035, 0.028, 0.02, brass, Vector3(-0.2, 0.555, -0.04), Vector3.ZERO, 12)  # cup
	_cyl(root, 0.026, 0.03, 0.1, _toon(Color(0.95, 0.92, 0.86), 0.2), Vector3(-0.2, 0.615, -0.04), Vector3.ZERO, 10)
	_sphere(root, 0.028, Vector3(1, 1.5, 1), _glow(Color(1.0, 0.78, 0.4), 1.9), Vector3(-0.2, 0.69, -0.04), 8, 4)
	_gem(root, 0.035, _glow(Color(0.6, 0.85, 1.0), 1.4), Vector3(0.0, 0.46, -0.16))    # a single rare gem accent
	return root


## 2 · OAK DINING TABLE — Uncommon. A big honest farmhouse table: thick planked
## oak top with grain lines + breadboard ends, chunky chamfered legs with turned
## beads + forged-iron corner brackets + foot caps, an H-stretcher, and a styled
## centerpiece (linen runner, ceramic fruit bowl, a candle pair). Top ~0.74.
static func build_oak_dining_table() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.95)
	var oak := _toon(OAK, 0.18, true, 0.16)
	var oak_d := _toon(OAK_DARK, 0.14)
	var grain := _toon(OAK_DARK.darkened(0.1), 0.1, false)
	var iron := _metal(Color(0.30, 0.31, 0.34), 0.4, 0.8)
	var w := 1.7
	var d := 0.92
	var h := 0.74
	# planked top: five boards with thin gaps + a faint grain line down each board
	var nb := 5
	for k in nb:
		var bz := -d * 0.5 + d / float(nb) * (k + 0.5)
		var bc := OAK if k % 2 == 0 else OAK.lightened(0.05)
		_box(root, Vector3(w - 0.14, 0.07, d / float(nb) - 0.012), _toon(bc, 0.18, true, 0.16),
			Vector3(0, h, bz))
		_box(root, Vector3(w - 0.3, 0.002, 0.004), grain, Vector3(0, h + 0.036, bz + 0.02))   # grain line
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.1, 0.075, d), oak_d, Vector3(sx * (w * 0.5 - 0.05), h, 0))   # breadboard end
		_cyl(root, 0.012, 0.012, d - 0.04, iron, Vector3(sx * (w * 0.5 - 0.05), h - 0.025, 0), Vector3(PI / 2.0, 0, 0), 6)  # peg line
	# apron rails under the top
	_box(root, Vector3(w - 0.3, 0.1, 0.05), oak_d, Vector3(0, h - 0.09, d * 0.5 - 0.1))
	_box(root, Vector3(w - 0.3, 0.1, 0.05), oak_d, Vector3(0, h - 0.09, -(d * 0.5 - 0.1)))
	# four chunky chamfered legs + turned beads + iron corner brackets + foot caps
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			var lx: float = sx * (w * 0.5 - 0.16)
			var lz: float = sz * (d * 0.5 - 0.16)
			_cyl(root, 0.07, 0.11, h - 0.1, oak_d, Vector3(lx, (h - 0.1) * 0.5, lz), Vector3.ZERO, 8)
			_cyl(root, 0.105, 0.105, 0.04, oak, Vector3(lx, h - 0.18, lz), Vector3.ZERO, 8)        # turned bead under apron
			_torus(root, 0.07, 0.1, iron, Vector3(lx, h - 0.26, lz), Vector3(PI / 2.0, 0, 0), 8, 4)  # iron strap
			_cyl(root, 0.085, 0.095, 0.05, iron, Vector3(lx, 0.025, lz), Vector3.ZERO, 10)         # foot cap
	# H-stretcher tying the legs + a center turned baluster
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.06, 0.06, d - 0.34), oak_d, Vector3(sx * (w * 0.5 - 0.16), 0.22, 0))
	_box(root, Vector3(w - 0.34, 0.06, 0.06), oak_d, Vector3(0, 0.22, 0))
	_sphere(root, 0.06, Vector3(1, 1.3, 1), oak, Vector3(0, 0.22, 0), 12, 6)
	# styled top: a linen runner + a ceramic fruit bowl with fruit + candle pair
	_box(root, Vector3(0.42, 0.012, d - 0.16), _cloth(Color(0.92, 0.88, 0.78)), Vector3(0, h + 0.043, 0))
	_box(root, Vector3(0.42, 0.004, 0.02), _cloth(Color(0.78, 0.55, 0.42)), Vector3(0, h + 0.05, d * 0.5 - 0.1))   # runner stripe
	_box(root, Vector3(0.42, 0.004, 0.02), _cloth(Color(0.78, 0.55, 0.42)), Vector3(0, h + 0.05, -(d * 0.5 - 0.1)))
	_sphere(root, 0.17, Vector3(1, 0.45, 1), _toon(Color(0.95, 0.95, 0.97), 0.32, true, 0.45), Vector3(0, h + 0.07, 0), 18, 8)
	var fruit := [Color(0.95, 0.45, 0.35), Color(0.55, 0.82, 0.4), Color(0.98, 0.78, 0.3), Color(0.85, 0.4, 0.55)]
	for i in 4:
		var ang := TAU * i / 4.0
		_sphere(root, 0.05, Vector3.ONE, _toon(fruit[i], 0.35, true, 0.3),
			Vector3(cos(ang) * 0.07, h + 0.10, sin(ang) * 0.07), 12, 6)
	for cx in [-0.5, 0.5]:
		_cyl(root, 0.026, 0.03, 0.13, _toon(Color(0.92, 0.5, 0.42), 0.2), Vector3(cx, h + 0.11, -0.02), Vector3.ZERO, 10)
		_sphere(root, 0.022, Vector3(1, 1.5, 1), _glow(Color(1.0, 0.78, 0.4), 1.8), Vector3(cx, h + 0.19, -0.02), 8, 4)
	return root


## 3 · GLOWING DESK — Epic. A sleek cyber workstation: matte slab top with an
## inset RGB light groove, a monitor showing a glowing screen, a keyboard, a
## little tower with vent slits + status LEDs, cable-managed metal legs and an
## under-desk neon underglow. Top ~0.74 high. Lights up the room.
static func build_glowing_desk() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.85)
	var top_mat := _toon(Color(0.14, 0.15, 0.20), 0.3, true, 0.4)       # matte graphite
	var metal := _metal(Color(0.55, 0.58, 0.64), 0.28, 0.9)
	var dark := _toon(Color(0.10, 0.11, 0.14), 0.2)
	var neon := Color(0.30, 0.85, 1.0)
	var h := 0.74
	var w := 1.5
	# top slab with a glowing inlaid groove tracing the front edge
	_slab(root, w, 0.06, 0.7, 0.06, top_mat, Vector3(0, h, 0))
	_box(root, Vector3(w - 0.16, 0.012, 0.02), _glow(neon, 2.2), Vector3(0, h + 0.035, 0.33))
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.02, 0.012, 0.62), _glow(neon, 2.0), Vector3(sx * (w * 0.5 - 0.08), h + 0.035, 0))
	# A-frame metal legs joined by a cable tray, glowing cable runs underneath
	for sx in [-1.0, 1.0]:
		var lx: float = sx * (w * 0.5 - 0.12)
		_box(root, Vector3(0.06, h - 0.06, 0.5), metal, Vector3(lx, (h - 0.06) * 0.5, 0))
		_box(root, Vector3(0.1, 0.05, 0.6), dark, Vector3(lx, 0.04, 0))           # foot rail
	_box(root, Vector3(w - 0.3, 0.05, 0.12), dark, Vector3(0, 0.22, -0.2))         # cable tray
	_box(root, Vector3(w - 0.34, 0.015, 0.02), _glow(neon, 1.4), Vector3(0, 0.205, -0.14))
	# under-desk underglow strip (downlight feel)
	_box(root, Vector3(w - 0.2, 0.02, 0.45), _glow(neon, 1.0), Vector3(0, h - 0.05, 0))
	# monitor: arm + bezel + a bright pictured screen, on a weighted base
	_cyl(root, 0.1, 0.12, 0.02, dark, Vector3(0, h + 0.04, -0.24), Vector3.ZERO, 14)
	_box(root, Vector3(0.04, 0.26, 0.04), metal, Vector3(0, h + 0.18, -0.25))
	_box(root, Vector3(0.62, 0.36, 0.03), dark, Vector3(0, h + 0.34, -0.25))
	_box(root, Vector3(0.56, 0.3, 0.012), _glow(Color(0.5, 0.78, 1.0), 1.8), Vector3(0, h + 0.34, -0.235))
	# a couple of UI bars glowing on the screen
	_box(root, Vector3(0.5, 0.03, 0.005), _glow(Color(1.0, 0.6, 0.85), 1.6), Vector3(0, h + 0.44, -0.232))
	_box(root, Vector3(0.32, 0.02, 0.005), _glow(Color(0.6, 1.0, 0.8), 1.6), Vector3(-0.1, h + 0.30, -0.232))
	# keyboard with a faint backlight + a mouse
	_box(root, Vector3(0.5, 0.025, 0.16), dark, Vector3(0, h + 0.04, 0.12))
	_box(root, Vector3(0.46, 0.006, 0.13), _glow(Color(0.5, 0.4, 1.0), 0.7), Vector3(0, h + 0.055, 0.12))
	_sphere(root, 0.05, Vector3(1, 0.5, 1.5), dark, Vector3(0.33, h + 0.05, 0.14), 12, 6)
	# a small PC tower beside the legs with vent slits, a glowing fan ring + LEDs
	_box(root, Vector3(0.22, 0.5, 0.42), dark, Vector3(0.6, 0.26, -0.05))
	for k in 5:
		_box(root, Vector3(0.16, 0.012, 0.01), _glow(neon, 1.4), Vector3(0.6, 0.42 - k * 0.05, 0.165))
	_torus(root, 0.05, 0.07, _glow(Color(0.7, 0.4, 1.0), 1.6), Vector3(0.6, 0.16, 0.165), Vector3.ZERO, 16, 4)  # glowing fan ring
	_sphere(root, 0.018, Vector3.ONE, _glow(Color(0.4, 1.0, 0.5), 2.0), Vector3(0.66, 0.46, 0.17), 8, 4)
	_sphere(root, 0.018, Vector3.ONE, _glow(Color(1.0, 0.5, 0.3), 2.0), Vector3(0.55, 0.46, 0.17), 8, 4)
	# a headphone stand: a slim post + a glowing band hung over the top
	_cyl(root, 0.018, 0.02, 0.22, metal, Vector3(-0.52, h + 0.15, -0.18), Vector3.ZERO, 8)
	_cyl(root, 0.05, 0.05, 0.04, dark, Vector3(-0.52, h + 0.025, -0.18), Vector3.ZERO, 12)
	_torus(root, 0.05, 0.075, _toon(Color(0.18, 0.19, 0.25), 0.2), Vector3(-0.52, h + 0.26, -0.18), Vector3(0, 0, 0), 14, 5)
	for hx in [-1.0, 1.0]:
		_sphere(root, 0.045, Vector3(1, 1.1, 0.8), _toon(Color(0.14, 0.15, 0.2), 0.2), Vector3(-0.52 + hx * 0.075, h + 0.21, -0.18), 12, 6)
		_torus(root, 0.02, 0.04, _glow(neon, 1.0), Vector3(-0.52 + hx * 0.095, h + 0.21, -0.18), Vector3(0, PI / 2.0, 0), 10, 4)
	# a tiny glowing desk plant in a pot beside the keyboard
	_cyl(root, 0.05, 0.04, 0.06, _toon(Color(0.85, 0.45, 0.4), 0.2), Vector3(-0.5, h + 0.06, 0.18), Vector3.ZERO, 10)
	for i in 4:
		var pa := TAU * i / 4.0
		var lf := _sphere(root, 0.035, Vector3(0.5, 1.6, 0.5), _glow(Color(0.4, 0.95, 0.6), 0.6), Vector3(-0.5 + cos(pa) * 0.025, h + 0.13, 0.18 + sin(pa) * 0.025), 8, 5)
		lf.rotation = Vector3(sin(pa) * 0.4, 0, -cos(pa) * 0.4)
	return root


## 4 · ORNATE BOOKSHELF — Legendary. A grand carved library case: fluted columns,
## an arched gold pediment with a glowing centre gem, scrolled feet, glass-fronted
## upper shelves of colourful tomes, busts and a softly lit lantern. ~1.9 tall.
static func build_ornate_bookshelf() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.78)
	var mahog := _toon(Color(0.42, 0.20, 0.16), 0.18, true, 0.2)        # rich mahogany
	var mahog_l := _toon(Color(0.54, 0.28, 0.22), 0.16)
	var gold := _metal(GOLD, 0.14)
	var back := _toon(Color(0.32, 0.15, 0.12), 0.1, false)
	var h := 1.9
	var w := 1.2
	var d := 0.38
	# carcass: sides, top, bottom, back
	_box(root, Vector3(0.09, h - 0.2, d), mahog, Vector3(-w * 0.5, (h - 0.2) * 0.5 + 0.1, 0))
	_box(root, Vector3(0.09, h - 0.2, d), mahog, Vector3(w * 0.5, (h - 0.2) * 0.5 + 0.1, 0))
	_box(root, Vector3(w, 0.08, d), mahog, Vector3(0, h - 0.24, 0))
	_box(root, Vector3(w, 0.1, d), mahog, Vector3(0, 0.12, 0))
	_box(root, Vector3(w - 0.16, h - 0.4, 0.03), back, Vector3(0, h * 0.5, -d * 0.5 + 0.02))
	# fluted columns flanking the front
	for sx in [-1.0, 1.0]:
		_cyl(root, 0.05, 0.05, h - 0.4, gold, Vector3(sx * (w * 0.5 - 0.04), h * 0.5 - 0.02, d * 0.5 - 0.04), Vector3.ZERO, 8)
		_cyl(root, 0.07, 0.08, 0.07, gold, Vector3(sx * (w * 0.5 - 0.04), h - 0.3, d * 0.5 - 0.04), Vector3.ZERO, 10)   # capital
	# arched gold pediment with a glowing crest gem
	_box(root, Vector3(w + 0.12, 0.1, d + 0.06), mahog_l, Vector3(0, h - 0.13, 0))
	_torus(root, 0.18, 0.34, gold, Vector3(0, h - 0.02, d * 0.4), Vector3(PI / 2.0, 0, 0), 20, 4)   # vertical arch crest
	_gem(root, 0.07, _glow(Color(0.5, 0.85, 1.0), 1.8), Vector3(0, h + 0.03, d * 0.42))
	for sx in [-1.0, 1.0]:
		_sphere(root, 0.05, Vector3.ONE, gold, Vector3(sx * (w * 0.5 + 0.02), h - 0.05, d * 0.4), 10, 5)   # corner finials
	# 4 shelves of leaning colourful tomes + a glass front rail
	var book := BoxMesh.new()
	book.size = Vector3(0.08, 0.30, 0.22)
	for s in 4:
		var sy := 0.3 + s * 0.36
		_box(root, Vector3(w - 0.18, 0.04, d - 0.06), mahog_l, Vector3(0, sy, 0))
		var n := 8
		for k in n:
			var bx := -0.44 + k * 0.115
			var bc: Color = BOOK_COLS[(k + s) % BOOK_COLS.size()]
			var bmi := MeshInstance3D.new()
			bmi.mesh = book
			bmi.material_override = _toon(bc, 0.2, false)
			bmi.position = Vector3(bx, sy + 0.17, -0.02)
			if k == n - 1:
				bmi.rotation.z = 0.3
				bmi.position.x -= 0.04
			root.add_child(bmi)
		# slim glass guard rail in front of each shelf
		_box(root, Vector3(w - 0.2, 0.22, 0.012), _glass(Color(0.8, 0.9, 1.0), 0.22, 0.05), Vector3(0, sy + 0.16, d * 0.5 - 0.02))
	# treasures: a marble bust + a glowing lantern + a golden globe on a stand
	_cyl(root, 0.05, 0.06, 0.06, _toon(Color(0.9, 0.9, 0.92), 0.3), Vector3(-0.36, 1.42, 0.02), Vector3.ZERO, 12)
	_sphere(root, 0.06, Vector3(1, 1.2, 1), _toon(Color(0.92, 0.92, 0.94), 0.35, true, 0.5), Vector3(-0.36, 1.52, 0.02), 12, 7)
	_box(root, Vector3(0.13, 0.16, 0.13), gold, Vector3(0.34, 1.46, 0.02))
	_sphere(root, 0.05, Vector3.ONE, _glow(Color(1.0, 0.86, 0.5), 1.4), Vector3(0.34, 1.46, 0.02), 10, 5)
	# a golden mantel clock on the lowest tier with a glowing face
	_cyl(root, 0.07, 0.07, 0.04, gold, Vector3(0.0, 0.5, 0.04), Vector3(PI / 2.0, 0, 0), 16)
	_cyl(root, 0.055, 0.055, 0.012, _glow(Color(0.98, 0.95, 0.8), 0.8), Vector3(0.0, 0.5, 0.07), Vector3(PI / 2.0, 0, 0), 16)
	_box(root, Vector3(0.006, 0.04, 0.006), _toon(Color(0.2, 0.15, 0.1), 0.1, false), Vector3(0.0, 0.515, 0.077))
	_box(root, Vector3(0.03, 0.006, 0.006), _toon(Color(0.2, 0.15, 0.1), 0.1, false), Vector3(0.012, 0.5, 0.077))
	_sphere(root, 0.018, Vector3.ONE, gold, Vector3(0.0, 0.575, 0.04), 8, 4)   # clock finial
	# a crowning center finial + extra mid finials for that Legendary read
	_sphere(root, 0.05, Vector3(1, 1.4, 1), gold, Vector3(0, h + 0.1, d * 0.42), 12, 6)
	_gem(root, 0.03, _glow(Color(1.0, 0.55, 0.75), 1.4), Vector3(0, h + 0.12, d * 0.42))
	# scrolled gold feet (paw-style: ball + claw cap)
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_sphere(root, 0.06, Vector3(1, 0.7, 1), gold, Vector3(sx * (w * 0.5 - 0.06), 0.05, sz * (d * 0.5 - 0.06)), 12, 6)
			_cyl(root, 0.045, 0.06, 0.03, _metal(GOLD.darkened(0.15), 0.2), Vector3(sx * (w * 0.5 - 0.06), 0.015, sz * (d * 0.5 - 0.06)), Vector3.ZERO, 10)
	return root


## 5 · TREASURE CHEST — Epic. A fat pirate hoard chest: domed wood lid cracked
## open on gold hinges, heavy gold straps + studs, a big lock plate, and a
## glowing pile of gold coins and gems spilling out. ~0.55 tall, ~0.8 wide.
static func build_treasure_chest() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.55)
	var wood := _toon(Color(0.50, 0.30, 0.18), 0.18, true, 0.15)
	var wood_d := _toon(Color(0.38, 0.22, 0.13), 0.12)
	var gold := _metal(GOLD, 0.14)
	var gold_glow := _glow(Color(1.0, 0.85, 0.45), 0.9)
	var w := 0.8
	var d := 0.5
	# base box (planked) + a wood foot rail
	_box(root, Vector3(w, 0.34, d), wood, Vector3(0, 0.19, 0))
	for k in 4:
		_box(root, Vector3(w + 0.005, 0.005, d + 0.005), wood_d, Vector3(0, 0.07 + k * 0.07, 0))   # plank lines
	_box(root, Vector3(w + 0.04, 0.06, d + 0.04), wood_d, Vector3(0, 0.04, 0))
	# vertical gold straps + corner studs on the base
	for bx in [-w * 0.32, 0.0, w * 0.32]:
		_box(root, Vector3(0.05, 0.36, d + 0.015), gold, Vector3(bx, 0.19, 0))
	for sx in [-1.0, 1.0]:
		for sy in [0.08, 0.3]:
			_sphere(root, 0.022, Vector3.ONE, gold, Vector3(sx * (w * 0.5 - 0.03), sy, d * 0.5 + 0.01), 8, 4)
	# heavy lock plate + keyhole on the front
	_box(root, Vector3(0.16, 0.16, 0.02), gold, Vector3(0, 0.26, d * 0.5 + 0.01))
	_cyl(root, 0.02, 0.02, 0.02, wood_d, Vector3(0, 0.27, d * 0.5 + 0.025), Vector3(PI / 2.0, 0, 0), 8)
	# domed lid, hinged open at the back
	var lid := Node3D.new()
	lid.position = Vector3(0, 0.36, -d * 0.5)
	lid.rotation.x = -1.15   # cracked well open so the glowing hoard shows
	root.add_child(lid)
	var dome := _cyl(lid, w * 0.5, w * 0.5, d, wood, Vector3(0, 0, d * 0.5), Vector3(0, 0, PI / 2.0), 16)
	dome.scale = Vector3(1.0, 1.0, 0.55)
	for bx in [-w * 0.32, 0.0, w * 0.32]:
		var st := _cyl(lid, w * 0.5 + 0.006, w * 0.5 + 0.006, 0.05, gold, Vector3(bx, 0, d * 0.5), Vector3(0, 0, PI / 2.0), 16)
		st.scale = Vector3(1.0, 1.0, 0.55)
	# gold hinges at the back
	for sx in [-1.0, 1.0]:
		_cyl(lid, 0.025, 0.025, 0.08, gold, Vector3(sx * 0.28, 0, 0.02), Vector3(0, 0, PI / 2.0), 8)
	# glowing hoard spilling out: coin pile + scattered coins + a coin stack + gems
	_sphere(root, 0.18, Vector3(1.2, 0.45, 1.0), gold_glow, Vector3(0, 0.36, 0.02), 16, 8)
	for k in 7:
		var ang := TAU * k / 7.0
		_cyl(root, 0.035, 0.035, 0.012, gold, Vector3(cos(ang) * 0.18, 0.38 + (k % 3) * 0.02, sin(ang) * 0.1 + 0.04), Vector3(randf() * 0.3, 0, randf() * 0.3), 10)
	for s in 4:
		_cyl(root, 0.034, 0.034, 0.012, gold, Vector3(0.24, 0.4 + s * 0.014, -0.04), Vector3.ZERO, 10)   # neat coin stack
	# a rolled treasure map scroll tucked in the hoard
	_cyl(root, 0.022, 0.022, 0.18, _toon(Color(0.92, 0.84, 0.64), 0.15), Vector3(-0.22, 0.4, -0.02), Vector3(0, 0, PI / 2.0), 10)
	_cyl(root, 0.026, 0.026, 0.02, _toon(Color(0.55, 0.18, 0.18), 0.2), Vector3(-0.31, 0.4, -0.02), Vector3(0, 0, PI / 2.0), 10)  # ribbon
	# a crown jewel centerpiece on a tiny gold band + scattered gems
	_torus(root, 0.04, 0.06, gold, Vector3(0.0, 0.41, 0.1), Vector3(PI / 2.0, 0, 0), 14, 4)
	_gem(root, 0.06, _glow(Color(0.95, 0.35, 0.55), 1.6), Vector3(0.0, 0.45, 0.1))
	var gem_cols := [Color(0.4, 0.85, 0.95), Color(0.95, 0.4, 0.6), Color(0.6, 0.95, 0.5), Color(0.7, 0.5, 1.0)]
	for i in 4:
		_gem(root, 0.045, _glow(gem_cols[i], 1.3), Vector3(-0.2 + i * 0.13, 0.42, 0.08))
	return root


## 6 · APOTHECARY CABINET — Rare. A wall of tiny labelled drawers (a herbalist's
## chest): a grid of pull drawers with brass cup-handles + label cards, a glass
## display top holding glowing potion bottles, crown moulding and bracket feet.
## ~1.45 tall (a display top to brew at). Cottagecore-meets-alchemy.
static func build_apothecary_cabinet() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.7)
	var body := _toon(Color(0.36, 0.46, 0.44), 0.18, true, 0.15)        # sage-teal cabinet
	var draw := _toon(Color(0.86, 0.82, 0.70), 0.16)                    # cream drawer fronts
	var brass := _metal(BRASS, 0.2)
	var w := 1.1
	var drawers_h := 0.92
	var base_y := 0.16
	# carcass behind the drawer grid + crown moulding on top
	_box(root, Vector3(w, drawers_h + 0.08, 0.5), body, Vector3(0, base_y + (drawers_h + 0.08) * 0.5, 0))
	_box(root, Vector3(w + 0.1, 0.06, 0.56), _toon(Color(0.30, 0.40, 0.38), 0.16), Vector3(0, base_y + drawers_h + 0.1, 0))
	# a 4×3 grid of little drawers, each with a brass cup handle + a label card
	var cols := 4
	var rows := 3
	var cw := (w - 0.1) / float(cols)
	var ch := (drawers_h - 0.06) / float(rows)
	for r in rows:
		for c in cols:
			var dx := -w * 0.5 + 0.05 + cw * (c + 0.5)
			var dy := base_y + 0.05 + ch * (r + 0.5)
			_box(root, Vector3(cw - 0.02, ch - 0.02, 0.02), draw, Vector3(dx, dy, 0.25))
			_torus(root, 0.012, 0.028, brass, Vector3(dx, dy - 0.01, 0.27), Vector3(PI / 2.0, 0, 0), 10, 5)   # cup handle
			_box(root, Vector3(cw * 0.55, ch * 0.32, 0.005), _toon(Color(0.95, 0.93, 0.86), 0.1, false), Vector3(dx, dy + ch * 0.22, 0.265))   # label card
	# glass display top: a clear case with glowing potion bottles
	var top_y := base_y + drawers_h + 0.13
	_box(root, Vector3(w, 0.03, 0.5), _toon(Color(0.30, 0.40, 0.38), 0.16), Vector3(0, top_y, 0))
	_box(root, Vector3(w - 0.04, 0.26, 0.46), _glass(Color(0.82, 0.92, 0.95), 0.18, 0.05), Vector3(0, top_y + 0.15, 0))
	var potion_cols := [Color(0.4, 0.95, 0.7), Color(0.95, 0.5, 0.8), Color(0.5, 0.7, 1.0), Color(1.0, 0.8, 0.4)]
	for i in 4:
		var px := -0.36 + i * 0.24
		_cyl(root, 0.045, 0.06, 0.14, _glass(potion_cols[i], 0.6, 0.6), Vector3(px, top_y + 0.1, 0), Vector3.ZERO, 12)
		_cyl(root, 0.02, 0.025, 0.04, _toon(Color(0.7, 0.6, 0.45), 0.2), Vector3(px, top_y + 0.19, 0), Vector3.ZERO, 8)   # cork
		_sphere(root, 0.035, Vector3.ONE, _glow(potion_cols[i], 1.4), Vector3(px, top_y + 0.1, 0), 10, 5)   # inner glow
	# a stone mortar & pestle sitting on the display top
	_cyl(root, 0.06, 0.045, 0.06, _toon(Color(0.78, 0.78, 0.82), 0.3, true, 0.4), Vector3(0.4, top_y + 0.07, 0.0), Vector3.ZERO, 14)
	_cyl(root, 0.04, 0.04, 0.012, _toon(Color(0.62, 0.62, 0.66), 0.2), Vector3(0.4, top_y + 0.045, 0.0), Vector3.ZERO, 12)
	_cyl(root, 0.012, 0.018, 0.08, _toon(Color(0.7, 0.7, 0.74), 0.3, true, 0.4), Vector3(0.42, top_y + 0.12, 0.0), Vector3(0, 0, -0.5), 10)
	# a hanging dried-herb bundle off the crown moulding (cottagecore touch)
	_cyl(root, 0.006, 0.006, 0.05, _toon(Color(0.6, 0.45, 0.3), 0.1, false), Vector3(-0.42, base_y + drawers_h + 0.05, 0.27), Vector3.ZERO, 6)
	var herb_mat := _toon(Color(0.45, 0.6, 0.35), 0.2)
	for hb in 3:
		var ha := -0.3 + hb * 0.3
		var sprig := _sphere(root, 0.025, Vector3(0.6, 1.4, 0.6), herb_mat, Vector3(-0.42 + (hb - 1) * 0.012, base_y + drawers_h - 0.02, 0.27), 8, 5)
		sprig.rotation = Vector3(0, 0, ha)
	_cyl(root, 0.01, 0.01, 0.03, _toon(Color(0.7, 0.35, 0.35), 0.2), Vector3(-0.42, base_y + drawers_h + 0.025, 0.27), Vector3.ZERO, 6)  # tie
	# a brass apothecary name-plate across the crown
	_box(root, Vector3(0.46, 0.05, 0.012), brass, Vector3(0, base_y + drawers_h + 0.1, 0.28))
	for ec in [-1.0, 1.0]:
		_sphere(root, 0.012, Vector3.ONE, brass, Vector3(ec * 0.2, base_y + drawers_h + 0.1, 0.29), 8, 4)
	# bracket feet with a scrolled toe
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.12, 0.16, 0.12), body, Vector3(sx * (w * 0.5 - 0.1), 0.08, 0.18))
		_box(root, Vector3(0.12, 0.16, 0.12), body, Vector3(sx * (w * 0.5 - 0.1), 0.08, -0.18))
		_sphere(root, 0.05, Vector3(1, 0.6, 1), _toon(Color(0.30, 0.40, 0.38), 0.16), Vector3(sx * (w * 0.5 - 0.1), 0.02, 0.22), 10, 5)
		_sphere(root, 0.05, Vector3(1, 0.6, 1), _toon(Color(0.30, 0.40, 0.38), 0.16), Vector3(sx * (w * 0.5 - 0.1), 0.02, -0.22), 10, 5)
	return root


## 7 · CRYSTAL SIDE TABLE — Legendary. A faceted amethyst-crystal slab top
## floating on a cluster of glowing geode crystals rising from a polished gold
## base ring, with motes of light. Top ~0.55 high. Pure fantasy-luxe accent.
static func build_crystal_side_table() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.42)
	var crystal := _glass(Color(0.62, 0.45, 0.95), 0.5, 0.5)            # amethyst
	var crystal_pink := _glass(Color(0.95, 0.55, 0.85), 0.55, 0.6)
	var crystal_cyan := _glass(Color(0.5, 0.9, 1.0), 0.5, 0.55)
	var gold := _metal(GOLD, 0.12)
	var h := 0.55
	# polished gold base ring + a soft glowing core
	_torus(root, 0.16, 0.24, gold, Vector3(0, 0.05, 0), Vector3.ZERO, 22, 6)   # flat base ring
	_cyl(root, 0.2, 0.22, 0.02, gold, Vector3(0, 0.02, 0), Vector3.ZERO, 22)
	_sphere(root, 0.1, Vector3(1, 0.5, 1), _glow(Color(0.7, 0.5, 1.0), 0.9), Vector3(0, 0.06, 0), 14, 7)
	# a cluster of tall faceted crystals forming the pedestal — the central
	# pillar reaches the underside of the slab so the top doesn't float
	var shards := [
		[0.0, 0.0, 0.5, crystal, 2.0],
		[0.11, 0.06, 0.36, crystal_pink, 1.0],
		[-0.1, -0.05, 0.4, crystal_cyan, 1.1],
		[0.06, -0.11, 0.28, crystal_pink, 0.8],
		[-0.08, 0.1, 0.24, crystal_cyan, 0.7],
	]
	for s in shards:
		var sx: float = s[0]
		var sz: float = s[1]
		var sh: float = s[2]
		var mat: Material = s[3]
		var lean: float = s[4]
		# pivot at the shard's BASE so leaning swings the TIP out (not below floor)
		var pivot := Node3D.new()
		pivot.position = Vector3(sx, 0.04, sz)
		pivot.rotation = Vector3(sz * 0.5, 0, -sx * 0.5)
		root.add_child(pivot)
		var shard := _cyl(pivot, 0.0, 0.05 * lean, sh, mat, Vector3(0, sh * 0.5, 0), Vector3.ZERO, 6)
		shard.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# a tapered collar where the cluster fuses into the slab (hides the join)
	_cyl(root, 0.2, 0.08, 0.16, crystal, Vector3(0, h - 0.13, 0), Vector3.ZERO, 6)
	# faceted amethyst top: a wide low hexagonal slab (two stacked 6-gon cones)
	_cyl(root, 0.34, 0.32, 0.05, crystal, Vector3(0, h, 0), Vector3.ZERO, 6)
	_cyl(root, 0.32, 0.22, 0.05, crystal, Vector3(0, h - 0.05, 0), Vector3.ZERO, 6)
	_torus(root, 0.30, 0.36, gold, Vector3(0, h, 0), Vector3.ZERO, 22, 5)   # flat gold rim around the top
	# a faceted crown gem rising from the top center (the hero sparkle)
	var crown_gem := _cyl(root, 0.0, 0.09, 0.16, crystal_pink, Vector3(0, h + 0.1, 0), Vector3.ZERO, 6)
	crown_gem.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_torus(root, 0.08, 0.11, gold, Vector3(0, h + 0.03, 0), Vector3.ZERO, 18, 4)
	_sphere(root, 0.03, Vector3.ONE, _glow(Color(1.0, 0.85, 0.6), 2.0), Vector3(0, h + 0.2, 0), 8, 4)  # tip spark
	# floating light motes orbiting the cluster (more, for the Legendary read)
	for k in 9:
		var ang := TAU * k / 9.0
		_sphere(root, 0.016, Vector3.ONE, _glow(Color(0.85, 0.7, 1.0), 1.8),
			Vector3(cos(ang) * 0.28, 0.2 + sin(ang * 2.0) * 0.16, sin(ang) * 0.28), 8, 4)
	return root


## 8 · ARCADE CABINET — Epic. A retro upright arcade machine: angled marquee with
## a glowing logo, a bright game screen behind a bezel, a control deck with a
## joystick + coloured buttons, a coin door, side art and floor light spill.
## ~1.7 tall. Pure 80s neon fun.
static func build_arcade_cabinet() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.5)
	var cab := _toon(Color(0.16, 0.17, 0.26), 0.25, true, 0.3)          # deep blue-black shell
	var side := _toon(Color(0.55, 0.20, 0.65), 0.2)                     # purple side art
	var trim := _metal(Color(0.7, 0.72, 0.78), 0.3, 0.8)
	var w := 0.62
	var d := 0.62
	# main body: lower deck box + a back slab rising to the marquee
	_box(root, Vector3(w, 1.0, d), cab, Vector3(0, 0.5, -0.05))
	# purple side-art panels
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.012, 0.96, d - 0.04), side, Vector3(sx * (w * 0.5 + 0.002), 0.55, -0.05))
		_box(root, Vector3(0.014, 0.4, 0.3), _glow(Color(0.9, 0.3, 0.8), 0.7), Vector3(sx * (w * 0.5 + 0.004), 0.95, -0.05))   # neon stripe
	# angled control deck jutting toward the player
	_box(root, Vector3(w, 0.08, 0.34), cab, Vector3(0, 0.92, 0.22), Vector3(-0.5, 0, 0))
	# joystick + 4 coloured buttons on the deck
	_sphere(root, 0.045, Vector3.ONE, _toon(Color(0.95, 0.2, 0.3), 0.3, true, 0.4), Vector3(-0.16, 1.02, 0.24), 12, 6)
	_cyl(root, 0.012, 0.012, 0.07, trim, Vector3(-0.16, 0.97, 0.23), Vector3(-0.5, 0, 0), 8)
	var btn_cols := [Color(0.95, 0.8, 0.2), Color(0.3, 0.9, 0.4), Color(0.3, 0.6, 1.0), Color(0.95, 0.3, 0.6)]
	for i in 4:
		_cyl(root, 0.028, 0.028, 0.02, _glow(btn_cols[i], 1.2), Vector3(0.02 + i * 0.085, 1.0 + (i % 2) * 0.01, 0.25), Vector3(-0.5, 0, 0), 12)
	# screen bezel + a glowing game screen tilted back slightly
	_box(root, Vector3(w - 0.04, 0.5, 0.05), cab, Vector3(0, 1.28, 0.02), Vector3(0.12, 0, 0))
	_box(root, Vector3(w - 0.16, 0.4, 0.012), _glow(Color(0.2, 0.5, 1.0), 1.6), Vector3(0, 1.28, 0.05), Vector3(0.12, 0, 0))
	# pixel-art blips glowing on the screen
	for k in 5:
		_box(root, Vector3(0.05, 0.05, 0.005), _glow(btn_cols[k % 4], 1.8), Vector3(-0.18 + k * 0.09, 1.18 + (k % 3) * 0.08, 0.058), Vector3(0.12, 0, 0))
	# speaker grilles flanking the marquee
	for sx in [-1.0, 1.0]:
		for gk in 3:
			_box(root, Vector3(0.14, 0.008, 0.008), trim, Vector3(sx * 0.18, 1.46 + gk * 0.02, 0.085), Vector3(0.12, 0, 0))
	# marquee header with a glowing logo bar + a glowing diamond emblem
	_box(root, Vector3(w + 0.06, 0.2, 0.16), cab, Vector3(0, 1.62, 0.04), Vector3(-0.2, 0, 0))
	_box(root, Vector3(w - 0.06, 0.13, 0.012), _glow(Color(1.0, 0.4, 0.7), 1.8), Vector3(0, 1.62, 0.12), Vector3(-0.2, 0, 0))
	var emblem := _box(root, Vector3(0.07, 0.07, 0.006), _glow(Color(0.4, 0.9, 1.0), 2.2), Vector3(0, 1.62, 0.127), Vector3(-0.2, 0, PI / 4.0))
	emblem.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# coin door + a glowing coin slot near the bottom + a kick plate
	_box(root, Vector3(0.2, 0.16, 0.02), trim, Vector3(0, 0.4, d * 0.5 - 0.04))
	_box(root, Vector3(0.06, 0.012, 0.01), _glow(Color(1.0, 0.9, 0.5), 1.5), Vector3(0, 0.45, d * 0.5 - 0.03))
	_sphere(root, 0.02, Vector3(1, 1, 0.6), trim, Vector3(0, 0.34, d * 0.5 - 0.03), 10, 5)   # coin return cup
	_box(root, Vector3(w - 0.04, 0.1, 0.012), trim, Vector3(0, 0.06, d * 0.5 - 0.02))         # kick plate
	# floor light-spill puck (the cabinet glows onto the floor)
	var spill := _cyl(root, 0.45, 0.45, 0.006, _glow(Color(0.4, 0.3, 0.9), 0.5), Vector3(0, 0.02, 0.1), Vector3.ZERO, 24)
	spill.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# stubby feet
	for sx in [-1.0, 1.0]:
		_cyl(root, 0.04, 0.05, 0.06, trim, Vector3(sx * (w * 0.5 - 0.06), 0.03, d * 0.5 - 0.1), Vector3.ZERO, 8)
		_cyl(root, 0.04, 0.05, 0.06, trim, Vector3(sx * (w * 0.5 - 0.06), 0.03, -d * 0.5 + 0.1), Vector3.ZERO, 8)
	return root


## 9 · TERRARIUM TABLE — Rare. A glass-box coffee table that's a living
## micro-world: a glass tank top over a brass frame, with soil, layered mossy
## plants, a tiny tree, glowing mushrooms and drifting fireflies inside; warm
## wood legs. Top ~0.42 high. A whole biome you can set your cup on.
static func build_terrarium_table() -> Node3D:
	var root := Node3D.new()
	_shadow(root, 0.6)
	var wood := _toon(WOOD, 0.18, true, 0.15)
	var brass := _metal(BRASS, 0.18)
	var glass := _glass(Color(0.8, 0.92, 0.95), 0.16, 0.04)
	var soil := _toon(Color(0.34, 0.24, 0.16), 0.1, false)
	var w := 1.0
	var d := 0.62
	var tank_y := 0.30
	var tank_h := 0.26
	# wooden base tray that holds the tank + warm legs
	_slab(root, w, 0.06, d, 0.08, wood, Vector3(0, tank_y - tank_h * 0.5 - 0.03, 0))
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			var lg := _leg(root, 0.045, 0.32, _toon(WOOD_DARK, 0.14), Vector3(sx * (w * 0.5 - 0.1), 0.16, sz * (d * 0.5 - 0.1)))
			lg.rotation.x = sz * 0.05
			lg.rotation.z = -sx * 0.05
	# soil bed + the living scene INSIDE
	_box(root, Vector3(w - 0.12, 0.05, d - 0.12), soil, Vector3(0, tank_y - tank_h * 0.5 + 0.03, 0))
	# layered mossy mounds
	_sphere(root, 0.14, Vector3(1.4, 0.6, 1.2), _toon(Color(0.36, 0.6, 0.32), 0.25), Vector3(-0.2, tank_y - 0.05, 0.05), 12, 6)
	_sphere(root, 0.1, Vector3(1.3, 0.7, 1.2), _toon(Color(0.46, 0.72, 0.4), 0.25), Vector3(0.18, tank_y - 0.06, -0.08), 12, 6)
	# a tiny tree: trunk + a leafy crown
	_cyl(root, 0.015, 0.022, 0.14, _toon(Color(0.5, 0.36, 0.24), 0.15), Vector3(0.12, tank_y + 0.02, 0.06), Vector3.ZERO, 8)
	_sphere(root, 0.09, Vector3.ONE, _toon(Color(0.42, 0.68, 0.36), 0.3), Vector3(0.12, tank_y + 0.12, 0.06), 12, 6)
	_sphere(root, 0.06, Vector3.ONE, _toon(Color(0.52, 0.78, 0.44), 0.3), Vector3(0.05, tank_y + 0.14, 0.1), 10, 5)
	# glowing mushrooms (caps + a faint stem glow)
	for i in 3:
		var mx := -0.28 + i * 0.16
		_cyl(root, 0.012, 0.016, 0.05, _toon(Color(0.92, 0.9, 0.84), 0.2), Vector3(mx, tank_y - 0.02, -0.12), Vector3.ZERO, 8)
		_sphere(root, 0.03, Vector3(1, 0.7, 1), _glow(Color(0.5, 0.85, 1.0), 1.4), Vector3(mx, tank_y + 0.01, -0.12), 10, 5)
		_sphere(root, 0.006, Vector3.ONE, _glow(Color(0.85, 0.95, 1.0), 1.2), Vector3(mx, tank_y - 0.01, -0.105), 6, 3)
	# a stacked-stone cairn beside the tree
	var stone := _toon(Color(0.6, 0.62, 0.66), 0.2, true, 0.3)
	for sk in 3:
		_sphere(root, 0.03 - sk * 0.006, Vector3(1.3, 0.6, 1.1), stone, Vector3(-0.05, tank_y - 0.04 + sk * 0.03, -0.06), 10, 5)
	# a tiny glowing pond (a thin glass disc with a soft underlight)
	var pond := _cyl(root, 0.08, 0.08, 0.008, _glass(Color(0.5, 0.85, 1.0), 0.45, 0.4), Vector3(0.22, tank_y - 0.045, 0.06), Vector3.ZERO, 16)
	pond.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# drifting firefly motes (more, dancing at two heights)
	for k in 6:
		var ang := TAU * k / 6.0
		_sphere(root, 0.012, Vector3.ONE, _glow(Color(0.9, 1.0, 0.5), 1.8), Vector3(cos(ang) * 0.22, tank_y + 0.08 + sin(ang * 1.5) * 0.06, sin(ang) * 0.14), 8, 4)
	# the glass tank: brass corner posts + thin glass walls + a glass lid
	for sx in [-1.0, 1.0]:
		for sz in [-1.0, 1.0]:
			_cyl(root, 0.014, 0.014, tank_h, brass, Vector3(sx * (w * 0.5 - 0.04), tank_y, sz * (d * 0.5 - 0.04)), Vector3.ZERO, 6)
	_box(root, Vector3(w - 0.08, tank_h, 0.01), glass, Vector3(0, tank_y, d * 0.5 - 0.04))
	_box(root, Vector3(w - 0.08, tank_h, 0.01), glass, Vector3(0, tank_y, -(d * 0.5 - 0.04)))
	_box(root, Vector3(0.01, tank_h, d - 0.08), glass, Vector3(w * 0.5 - 0.04, tank_y, 0))
	_box(root, Vector3(0.01, tank_h, d - 0.08), glass, Vector3(-(w * 0.5 - 0.04), tank_y, 0))
	# brass-framed glass lid (the tabletop you set things on)
	_slab(root, w - 0.04, 0.02, d - 0.04, 0.05, glass, Vector3(0, tank_y + tank_h * 0.5 + 0.01, 0))
	_torus(root, (d - 0.04) * 0.5 - 0.02, (d - 0.04) * 0.5, brass, Vector3(0, tank_y + tank_h * 0.5 + 0.02, 0), Vector3.ZERO, 24, 4)   # flat brass rim around lid
	return root


## 10 · FLOATING SHELF — Uncommon. A wall-mounted hidden-bracket shelf: a thick
## live-edge plank with a brushed-brass front lip on chunky metal hidden brackets,
## a warm LED underglow + wall-spill, and a styled vignette (a brass-framed lit
## picture, a row of books with a brass bookend, a glazed potted succulent, a
## little brass clock, a glowing candle). Sits ~1.3 high. Centered to mount at -Z.
static func build_floating_shelf() -> Node3D:
	var root := Node3D.new()
	# This piece hangs on a wall: the wall is behind at -Z, props sit on top (+Y).
	var wood := _toon(WOOD_LIGHT, 0.2, true, 0.18)
	var wood_d := _toon(WOOD_DARK, 0.14)
	var brass := _metal(BRASS, 0.2)
	var metal := _metal(Color(0.42, 0.44, 0.5), 0.35, 0.85)
	var w := 1.1
	var d := 0.26
	# flush backplate (against the wall) + two chunky metal hidden brackets
	_box(root, Vector3(w, 0.16, 0.02), wood_d, Vector3(0, -0.02, -d * 0.5 - 0.01))
	for sx in [-1.0, 1.0]:
		_box(root, Vector3(0.05, 0.04, d - 0.06), metal, Vector3(sx * 0.36, -0.04, -0.01))   # bracket arm
		_box(root, Vector3(0.05, 0.1, 0.02), metal, Vector3(sx * 0.36, -0.06, -d * 0.5 - 0.005))   # bracket back
	# the thick live-edge floating plank + a brushed-brass front lip
	_slab(root, w, 0.07, d, 0.05, wood, Vector3(0, 0, 0))
	_cyl(root, 0.035, 0.035, w - 0.1, wood, Vector3(0, -0.035, d * 0.5 - 0.035), Vector3(0, 0, PI / 2.0), 14)   # bullnose front edge
	_box(root, Vector3(w - 0.04, 0.012, 0.02), brass, Vector3(0, 0.025, d * 0.5 - 0.012))   # brass front lip
	# soft LED underglow strip + a faint light-spill bar on the wall below
	_box(root, Vector3(w - 0.12, 0.012, 0.02), _glow(Color(1.0, 0.86, 0.6), 1.4), Vector3(0, -0.05, d * 0.5 - 0.07))
	var spill := _box(root, Vector3(w - 0.2, 0.18, 0.005), _glow(Color(1.0, 0.86, 0.6), 0.35), Vector3(0, -0.22, -d * 0.5 + 0.01))
	spill.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# styled props on top
	# 1) a brass-framed lit picture leaning against the wall (frame + lit canvas + mat)
	_box(root, Vector3(0.3, 0.34, 0.02), brass, Vector3(-0.32, 0.21, -0.04), Vector3(0.12, 0, 0))
	_box(root, Vector3(0.26, 0.3, 0.012), wood_d, Vector3(-0.32, 0.21, -0.032), Vector3(0.12, 0, 0))
	_box(root, Vector3(0.2, 0.24, 0.01), _glow(Color(0.75, 0.55, 0.85), 0.45), Vector3(-0.32, 0.21, -0.025), Vector3(0.12, 0, 0))
	# 2) a short row of books held by a brass L-bookend
	var book := BoxMesh.new()
	book.size = Vector3(0.05, 0.22, 0.16)
	for k in 4:
		var bmi := MeshInstance3D.new()
		bmi.mesh = book
		bmi.material_override = _toon(BOOK_COLS[k], 0.2, false)
		bmi.position = Vector3(-0.02 + k * 0.06, 0.145, 0.0)
		if k == 3:
			bmi.rotation.z = 0.28
			bmi.position.x += 0.02
		root.add_child(bmi)
	_box(root, Vector3(0.012, 0.24, 0.16), brass, Vector3(-0.06, 0.155, 0.0))          # bookend upright
	_box(root, Vector3(0.06, 0.012, 0.16), brass, Vector3(-0.04, 0.04, 0.0))           # bookend foot
	# 3) a glazed potted succulent (rolled rim pot + layered rosette)
	_cyl(root, 0.06, 0.05, 0.08, _toon(Color(0.9, 0.55, 0.42), 0.25, true, 0.4), Vector3(0.3, 0.075, 0.02), Vector3.ZERO, 12)
	_torus(root, 0.05, 0.065, _toon(Color(0.78, 0.42, 0.32), 0.2), Vector3(0.3, 0.115, 0.02), Vector3.ZERO, 14, 4)   # rolled rim
	for i in 5:
		var ang := TAU * i / 5.0
		var leaf := _sphere(root, 0.04, Vector3(0.6, 1.5, 0.6), _toon(Color(0.42, 0.7, 0.4), 0.3), Vector3(0.3 + cos(ang) * 0.03, 0.16, 0.02 + sin(ang) * 0.03), 8, 5)
		leaf.rotation = Vector3(sin(ang) * 0.4, 0, -cos(ang) * 0.4)
	_sphere(root, 0.035, Vector3(0.7, 1.4, 0.7), _toon(Color(0.5, 0.78, 0.46), 0.3), Vector3(0.3, 0.2, 0.02), 8, 5)
	# 4) a small brass desk clock with a softly lit face
	_cyl(root, 0.045, 0.045, 0.025, brass, Vector3(0.62, 0.1, -0.02), Vector3(PI / 2.0, 0, 0), 16)
	_cyl(root, 0.034, 0.034, 0.01, _glow(Color(0.98, 0.95, 0.82), 0.7), Vector3(0.62, 0.1, -0.002), Vector3(PI / 2.0, 0, 0), 16)
	_box(root, Vector3(0.004, 0.025, 0.004), wood_d, Vector3(0.62, 0.11, 0.006))
	_cyl(root, 0.012, 0.014, 0.02, brass, Vector3(0.62, 0.04, -0.02), Vector3.ZERO, 10)   # clock foot
	# 5) a slim candle with a glowing flame
	_cyl(root, 0.025, 0.03, 0.12, _toon(Color(0.95, 0.92, 0.86), 0.2), Vector3(0.46, 0.095, -0.02), Vector3.ZERO, 10)
	_sphere(root, 0.022, Vector3(1, 1.5, 1), _glow(Color(1.0, 0.78, 0.4), 1.9), Vector3(0.46, 0.18, -0.02), 8, 4)
	return root
