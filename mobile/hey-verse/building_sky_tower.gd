class_name VerseBuildingSkyTower
extends RefCounted
## Hey Verse — PREMIUM PROCEDURAL BUILDING · "Aurelia Sky Tower" (Legendary).
##
## A luxury sky high-rise sold as an NFT and placed on a player's land. Built
## entirely from primitives (~520 parts) at the ORIGIN, ground floor at y=0,
## entrance facing +z. A ~1.4-unit chibi-robot avatar walks INTO the glowing
## marble lobby, past the fountain court and grand stair to the sky-lounge
## mezzanine, then the tower tapers skyward through planted setback terraces with
## balconies and statues to a glowing faceted gold crown.
##
## Silhouette (the wealth read): a tiered marble podium ringed by a balustrade and
## guardian statues, a colonnade lobby with a central fountain, a tall tapered
## glass shaft stepped by three setback garden terraces (hedges, trees, planters,
## belvederes), full-height gold pilaster fins + corner statues, projecting
## balconies, dormer-glazed crown floor, a gold-framed sky-lounge penthouse, and a
## faceted gold coronet + spire with a beacon — all in marble + glass + brushed
## gold, with emissive warm windows banding the shaft.
##
## SELF-CONTAINED: pulls only the shared res://toon.gdshader + res://outline.gdshader
## (guarded by ResourceLoader.exists, with a StandardMaterial3D fallback) and
## re-declares its own material (_toon/_metal/_gloss/_glass/_glow) + primitive
## (_box/_cyl/_ball/_torus/_prism) helpers — no preload of other .gd, no assets.
## Parses + runs standalone.
##
## Walkable interior: the FRONT (+z) ground-floor wall is OMITTED (a low marble
## threshold/parapet only) so the camera looks straight into the lobby; rooms are
## defined by partial interior walls + a real ceiling, kept OPEN to furnish later;
## a real staircase climbs to the sky-lounge mezzanine.

const TOON_SHADER_PATH := "res://toon.gdshader"
const OUTLINE_SHADER_PATH := "res://outline.gdshader"

static var _toon_shader: Shader
static var _outline_mat: ShaderMaterial


# ───────────────────────────── material helpers (self-contained) ────────────

## Cel material + inverted-hull outline — the Verse "designed" look on solids.
## Falls back to a plain toon-ish StandardMaterial3D if the shaders are missing.
static func _toon(c: Color, rim := 0.3, outline := true, spec := 0.0) -> Material:
	if _toon_shader == null and ResourceLoader.exists(TOON_SHADER_PATH):
		_toon_shader = load(TOON_SHADER_PATH)
	if _toon_shader == null:
		var sm := StandardMaterial3D.new()
		sm.albedo_color = c
		sm.roughness = 0.9
		sm.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
		sm.specular_mode = BaseMaterial3D.SPECULAR_DISABLED
		return sm
	var m := ShaderMaterial.new()
	m.shader = _toon_shader
	m.set_shader_parameter("albedo", c)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", spec)
	m.set_shader_parameter("wind_strength", 0.0)
	m.set_shader_parameter("wind_height", 0.5)
	if outline:
		if _outline_mat == null and ResourceLoader.exists(OUTLINE_SHADER_PATH):
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = load(OUTLINE_SHADER_PATH)
		if _outline_mat != null:
			m.next_pass = _outline_mat
	return m


## A real metal — brushed GOLD / brass / chrome with PBR metallic + low roughness
## so it catches a bright spec highlight (the premium trim). Toon-diffuse so it
## sits beside the cel surfaces happily, plus a faint warm self-tint.
static func _metal(c: Color, rough := 0.18, metallic := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	m.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.05
	return m


## Polished stone — marble / veined stucco with a soft sheen (low rough, slight
## metallic) so floors and columns read as buffed luxury stone, not flat plaster.
static func _gloss(c: Color, rough := 0.32) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.0
	m.roughness = rough
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	m.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	return m


## Translucent architectural GLASS — the tapered curtain wall, terrace rails and
## penthouse. Lightly tinted, faintly emissive (so the shaft glows), casts no
## shadow (it would punch dark holes through the tower).
static func _glass(c: Color, alpha := 0.30, glow := 0.35) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.metallic = 0.35
	m.roughness = 0.06
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = glow
	return m


## Unshaded emissive — the warm window bands, lobby glow, crown beacon. The only
## surfaces that should HALO. Casts no shadow.
static func _glow(c: Color, energy := 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


# ───────────────────────────── primitive helpers (self-contained) ───────────

static func _box(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, no_shadow := false) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _cyl(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, seg := 16, no_shadow := false) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, sc := Vector3.ONE, seg := 16, rings := 8, no_shadow := false) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = seg
	sm.rings = rings
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = sc
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, seg := 20, no_shadow := false) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = seg
	tm.ring_segments = 8
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


## A tapered pyramid / spire cut from a cone (r_top → r_bot). Used for the crown
## spire, terrace finials and the entry-canopy bevels.
static func _prism(parent: Node3D, r_top: float, r_bot: float, h: float, mat: Material, pos: Vector3, seg := 4, no_shadow := false) -> MeshInstance3D:
	var cm := CylinderMesh.new()
	cm.top_radius = r_top
	cm.bottom_radius = r_bot
	cm.height = h
	cm.radial_segments = seg
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	if no_shadow:
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)
	return mi


## A warm point light tucked inside a glow source. Cheap: modest range, no shadow.
static func _light(parent: Node3D, pos: Vector3, color: Color, energy: float, rng: float) -> OmniLight3D:
	var o := OmniLight3D.new()
	o.position = pos
	o.light_color = color
	o.light_energy = energy
	o.omni_range = rng
	o.shadow_enabled = false
	parent.add_child(o)
	return o


# ───────────────────────────── small composite fixtures ─────────────────────

## A fluted marble column with a gold base + capital — the lobby colonnade.
static func _column(parent: Node3D, pos: Vector3, h: float, r: float, stone: Material, gold: Material) -> void:
	# gold plinth + ring base
	_box(parent, Vector3(r * 2.7, 0.10, r * 2.7), gold, pos + Vector3(0, 0.05, 0))
	_torus(parent, r * 1.05, r * 1.3, gold, pos + Vector3(0, 0.12, 0), 18)
	# fluted shaft (core cylinder + reeded ribs)
	_cyl(parent, r * 0.92, r, h, stone, pos + Vector3(0, 0.12 + h * 0.5, 0), 18)
	for k: int in range(10):
		var ang: float = TAU * float(k) / 10.0
		_cyl(parent, r * 0.07, r * 0.07, h * 0.96, _gloss(Color(0.80, 0.80, 0.84), 0.4),
			pos + Vector3(cos(ang) * r * 0.92, 0.12 + h * 0.5, sin(ang) * r * 0.92), 5)
	# gold capital + neck ring
	_torus(parent, r * 1.0, r * 1.28, gold, pos + Vector3(0, 0.12 + h - 0.04, 0), 18)
	_box(parent, Vector3(r * 2.7, 0.12, r * 2.7), gold, pos + Vector3(0, 0.12 + h + 0.05, 0))


## A planter box with clipped hedge + a slim conical tree — the setback gardens.
static func _planter(parent: Node3D, pos: Vector3, w: float, d: float, gold: Material) -> void:
	var stone := _gloss(Color(0.90, 0.89, 0.86), 0.4)
	var hedge := _toon(Color(0.24, 0.46, 0.26), 0.34)
	var hedge2 := _toon(Color(0.30, 0.54, 0.30), 0.34)
	var tree := _toon(Color(0.20, 0.42, 0.24), 0.34)
	var trunk := _toon(Color(0.42, 0.30, 0.18), 0.25)
	# the marble planter box with a gold rim
	_box(parent, Vector3(w, 0.22, d), stone, pos + Vector3(0, 0.11, 0))
	_box(parent, Vector3(w + 0.05, 0.05, d + 0.05), gold, pos + Vector3(0, 0.24, 0))
	# clipped hedge mass (a couple of rounded boxes)
	_box(parent, Vector3(w - 0.10, 0.16, d - 0.10), hedge, pos + Vector3(0, 0.30, 0))
	_box(parent, Vector3(w - 0.30, 0.12, d - 0.16), hedge2, pos + Vector3(0, 0.40, 0))
	# a slim conifer rising from the planter
	_cyl(parent, 0.025, 0.04, 0.20, trunk, pos + Vector3(0, 0.42, 0), 8)
	_prism(parent, 0.0, 0.20, 0.50, tree, pos + Vector3(0, 0.72, 0), 8)
	_prism(parent, 0.0, 0.14, 0.36, hedge2, pos + Vector3(0, 0.95, 0), 8)


## A topiary ball on a slim trunk in a gold pot — flanks the entrance path.
static func _topiary(parent: Node3D, pos: Vector3, gold: Material) -> void:
	_cyl(parent, 0.14, 0.18, 0.22, gold, pos + Vector3(0, 0.11, 0), 14)
	_torus(parent, 0.13, 0.17, gold, pos + Vector3(0, 0.22, 0), 16)
	_cyl(parent, 0.03, 0.035, 0.20, _toon(Color(0.42, 0.30, 0.18), 0.25), pos + Vector3(0, 0.34, 0), 8)
	_ball(parent, 0.18, _toon(Color(0.26, 0.50, 0.28), 0.34), pos + Vector3(0, 0.56, 0), Vector3.ONE, 12, 6)
	_ball(parent, 0.10, _toon(Color(0.32, 0.56, 0.32), 0.34), pos + Vector3(0.05, 0.66, 0.04), Vector3.ONE, 8, 4)


## A short balustered rail run (stone posts + a gold cap) — podium edge + balconies.
## Length runs along the local axis given by `horizontal` (true = along x).
static func _balustrade(parent: Node3D, pos: Vector3, length: float, posts: int, horizontal: bool, stone: Material, gold: Material) -> void:
	var half := length * 0.5
	# bottom kick rail + top gold cap
	if horizontal:
		_box(parent, Vector3(length, 0.06, 0.10), stone, pos + Vector3(0, 0.03, 0))
		_box(parent, Vector3(length, 0.06, 0.14), gold, pos + Vector3(0, 0.52, 0))
	else:
		_box(parent, Vector3(0.10, 0.06, length), stone, pos + Vector3(0, 0.03, 0))
		_box(parent, Vector3(0.14, 0.06, length), gold, pos + Vector3(0, 0.52, 0))
	# the bellied balusters
	for k: int in range(posts):
		var f: float = -half + (length / float(max(posts - 1, 1))) * float(k)
		var bp: Vector3 = pos + (Vector3(f, 0.0, 0.0) if horizontal else Vector3(0.0, 0.0, f))
		_cyl(parent, 0.035, 0.05, 0.46, stone, bp + Vector3(0, 0.27, 0), 8)
		_ball(parent, 0.06, stone, bp + Vector3(0, 0.30, 0), Vector3(1, 0.8, 1), 8, 5)


## A small classical guardian statue on a gold-trimmed marble plinth — podium +
## terrace corners + crown. Abstracted (robed figure on a base) but unmistakable.
static func _statue(parent: Node3D, pos: Vector3, scl: float, stone: Material, gold: Material) -> void:
	# tiered plinth
	_box(parent, Vector3(0.62 * scl, 0.18 * scl, 0.62 * scl), stone, pos + Vector3(0, 0.09 * scl, 0))
	_box(parent, Vector3(0.70 * scl, 0.06 * scl, 0.70 * scl), gold, pos + Vector3(0, 0.20 * scl, 0))
	_box(parent, Vector3(0.46 * scl, 0.34 * scl, 0.46 * scl), stone, pos + Vector3(0, 0.40 * scl, 0))
	# robed body (a tapered drape), shoulders, head
	var fig := _gloss(Color(0.96, 0.95, 0.93), 0.26)
	_prism(parent, 0.10 * scl, 0.22 * scl, 0.70 * scl, fig, pos + Vector3(0, 0.92 * scl, 0), 10)
	_ball(parent, 0.16 * scl, fig, pos + Vector3(0, 1.22 * scl, 0), Vector3(1.0, 0.7, 1.0), 10, 6)
	_ball(parent, 0.11 * scl, fig, pos + Vector3(0, 1.46 * scl, 0), Vector3.ONE, 10, 6)
	# a raised gold arm holding an orb (the heroic gesture)
	_cyl(parent, 0.03 * scl, 0.035 * scl, 0.42 * scl, fig, pos + Vector3(0.14 * scl, 1.30 * scl, 0), 6)
	_ball(parent, 0.08 * scl, gold, pos + Vector3(0.22 * scl, 1.54 * scl, 0), Vector3.ONE, 8, 5)


## A two-tier fountain — bowl + jet on a stepped marble basin, glowing water.
## Returns nothing; drops a soft light + a glow disc for the "water".
static func _fountain(parent: Node3D, pos: Vector3, r: float, stone: Material, gold: Material) -> void:
	var water := _glass(Color(0.60, 0.84, 0.96), 0.42, 0.5)
	# stepped octagonal basin
	_cyl(parent, r * 1.06, r * 1.12, 0.20, stone, pos + Vector3(0, 0.10, 0), 8)
	_cyl(parent, r, r * 1.02, 0.30, stone, pos + Vector3(0, 0.30, 0), 8)
	_torus(parent, r * 0.92, r, gold, pos + Vector3(0, 0.42, 0), 8)
	# lower water surface
	_cyl(parent, r * 0.86, r * 0.86, 0.04, water, pos + Vector3(0, 0.40, 0), 24, true)
	# central pedestal + upper bowl
	_cyl(parent, 0.10, 0.16, 0.55, stone, pos + Vector3(0, 0.70, 0), 12)
	_cyl(parent, r * 0.42, r * 0.30, 0.14, stone, pos + Vector3(0, 0.98, 0), 16)
	_torus(parent, r * 0.36, r * 0.42, gold, pos + Vector3(0, 1.05, 0), 18)
	_cyl(parent, r * 0.30, r * 0.30, 0.03, water, pos + Vector3(0, 1.04, 0), 18, true)
	# the jet + falling-water plume (a slim glowing column)
	_cyl(parent, 0.03, 0.05, 0.9, water, pos + Vector3(0, 1.55, 0), 8, true)
	_ball(parent, 0.10, _glow(Color(0.8, 0.94, 1.0), 1.4), pos + Vector3(0, 2.05, 0), Vector3.ONE, 10, 5, true)
	_light(parent, pos + Vector3(0, 0.8, 0), Color(0.7, 0.9, 1.0), 1.4, 4.5)


# ════════════════════════════════════════════════════════════════════════════
#  BUILD  —  one Node3D: exterior shell + walkable interior, at the origin.
#  Convention: ground floor at y=0, entrance faces +z, footprint ~18 (x) × 16 (z)
#  at the podium, tapering to ~6 at the crown. Total height ~28 units.
# ════════════════════════════════════════════════════════════════════════════
static func build() -> Node3D:
	var root := Node3D.new()
	root.name = "AureliaSkyTower"

	# ── palette ──────────────────────────────────────────────────────────────
	var marble := _gloss(Color(0.94, 0.93, 0.90), 0.28)          # warm white marble
	var marble_dk := _gloss(Color(0.80, 0.79, 0.76), 0.34)       # veined grey marble
	var floor_mat := _gloss(Color(0.86, 0.84, 0.80), 0.30)       # polished lobby floor
	var floor_inlay := _gloss(Color(0.30, 0.26, 0.34), 0.22)     # dark inlay stone
	var gold := _metal(Color(0.94, 0.76, 0.34), 0.14)            # brushed gold trim
	var gold_dk := _metal(Color(0.74, 0.56, 0.24), 0.22)         # deep brass
	var chrome := _metal(Color(0.82, 0.84, 0.90), 0.10)          # mullions / handrail
	var glass := _glass(Color(0.62, 0.80, 0.92), 0.26, 0.30)     # cool curtain wall
	var glass_warm := _glass(Color(0.96, 0.86, 0.62), 0.30, 0.45)  # warm penthouse glass
	var win_glow := _glow(Color(1.0, 0.86, 0.56), 1.5)           # warm window band
	var lobby_glow := _glow(Color(1.0, 0.90, 0.66), 1.3)

	# tower taper geometry (shared by shaft, fins, windows)
	var base_w := 7.6     # full width of the glass shaft at the podium
	var base_d := 6.4
	var podium_top := 4.2
	var shaft_top := 22.0
	var taper := 0.62     # crown is taper× the base footprint

	# ───────────────────── 1 · GROUND: terrace plaza + paths ─────────────────
	# a broad tiered marble plaza the tower stands on (a low lip, ringed in gold)
	_box(root, Vector3(17.6, 0.30, 15.6), marble_dk, Vector3(0, 0.15, 0))
	_box(root, Vector3(17.0, 0.10, 15.0), marble, Vector3(0, 0.35, 0))
	# gold inlay border line around the plaza
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(16.4, 0.04, 0.10), gold, Vector3(0, 0.41, s * 7.0))
		_box(root, Vector3(0.10, 0.04, 14.4), gold, Vector3(s * 8.0, 0.41, 0))
	# a perimeter balustrade ringing the plaza on the three non-entry sides
	_balustrade(root, Vector3(0, 0.40, -7.4), 16.4, 19, true, marble, gold)
	for s: float in [-1.0, 1.0]:
		_balustrade(root, Vector3(s * 8.4, 0.40, -1.4), 11.6, 13, false, marble, gold)
	# guardian statues at the plaza's front corners (the gateway figures)
	_statue(root, Vector3(-7.6, 0.40, 6.6), 1.15, marble, gold)
	_statue(root, Vector3(7.6, 0.40, 6.6), 1.15, marble, gold)

	# approach path of marble pavers running out the +z entrance
	for i: int in range(5):
		_box(root, Vector3(3.2, 0.06, 1.0), marble, Vector3(0, 0.43, 8.2 + float(i) * 1.1))
		_box(root, Vector3(3.2, 0.02, 0.06), gold, Vector3(0, 0.46, 7.75 + float(i) * 1.1))
	# manicured lawn beds + low hedges flanking the approach path
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(2.6, 0.06, 5.0), _toon(Color(0.26, 0.48, 0.28), 0.34), Vector3(s * 4.4, 0.42, 10.6))
		_box(root, Vector3(2.6, 0.34, 0.30), _toon(Color(0.22, 0.44, 0.26), 0.34), Vector3(s * 4.4, 0.56, 8.2))
		_topiary(root, Vector3(s * 4.4, 0.42, 9.5), gold)
		_topiary(root, Vector3(s * 4.4, 0.42, 11.7), gold)

	# ───────────────────── 2 · PODIUM + LOBBY (ground floor) ─────────────────
	# the flared marble podium block the lobby sits in. Front (+z) is OPEN.
	var pw := 9.4   # podium full width
	var pd := 8.2   # podium full depth
	var wall_t := 0.4
	var floor_y := 0.4
	# lobby floor slab (polished, inside the podium)
	_box(root, Vector3(pw, 0.10, pd), floor_mat, Vector3(0, floor_y + 0.05, 0))
	# a dark-marble inlay frame banding the lobby floor (the luxury rug-line)
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(pw - 1.0, 0.02, 0.14), floor_inlay, Vector3(0, floor_y + 0.11, s * (pd * 0.5 - 1.0)), true)
		_box(root, Vector3(0.14, 0.02, pd - 1.6), floor_inlay, Vector3(s * (pw * 0.5 - 1.0), floor_y + 0.11, 0), true)
	# a gold compass medallion inlaid in the lobby floor
	_torus(root, 1.5, 1.7, gold, Vector3(0, floor_y + 0.12, -0.4), 28, true)
	_torus(root, 1.0, 1.12, gold_dk, Vector3(0, floor_y + 0.12, -0.4), 24, true)
	for k: int in range(8):
		var ang: float = TAU * float(k) / 8.0
		_box(root, Vector3(0.10, 0.02, 1.4), gold, Vector3(sin(ang) * 0.7, floor_y + 0.12, -0.4 + cos(ang) * 0.7), true).rotation.y = -ang

	# podium side + back walls (front omitted so the camera sees in)
	var lobby_h := 3.0
	# back wall
	_box(root, Vector3(pw, lobby_h, wall_t), marble, Vector3(0, floor_y + lobby_h * 0.5, -pd * 0.5 + wall_t * 0.5))
	# side walls
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(wall_t, lobby_h, pd), marble, Vector3(s * (pw * 0.5 - wall_t * 0.5), floor_y + lobby_h * 0.5, 0))
		# gold pilaster bands on the side wall fronts
		_box(root, Vector3(0.18, lobby_h, 0.18), gold, Vector3(s * (pw * 0.5 - 0.2), floor_y + lobby_h * 0.5, pd * 0.5 - 0.3))
		# a glowing wall sconce midway down each side wall
		_box(root, Vector3(0.08, 0.5, 0.16), win_glow, Vector3(s * (pw * 0.5 - wall_t - 0.05), floor_y + 1.7, -0.6), true)
		_torus(root, 0.10, 0.16, gold, Vector3(s * (pw * 0.5 - wall_t - 0.02), floor_y + 1.7, -0.6), 14)
	# low marble threshold parapet across the OPEN front (knee height, walkable over)
	_box(root, Vector3(pw, 0.5, wall_t), marble, Vector3(0, floor_y + 0.25, pd * 0.5 - wall_t * 0.5))
	_box(root, Vector3(pw, 0.06, wall_t + 0.06), gold, Vector3(0, floor_y + 0.53, pd * 0.5 - wall_t * 0.5))

	# lobby ceiling slab (= podium top) with a coffered gold rosette
	_box(root, Vector3(pw + 0.6, 0.40, pd + 0.6), marble_dk, Vector3(0, floor_y + lobby_h + 0.2, 0))
	_torus(root, 1.4, 1.6, gold, Vector3(0, floor_y + lobby_h - 0.02, 0.6), 26, true)
	_torus(root, 0.9, 1.02, gold_dk, Vector3(0, floor_y + lobby_h - 0.02, 0.6), 22, true)
	# warm cove glow ringing the lobby ceiling
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(pw - 0.6, 0.08, 0.12), lobby_glow, Vector3(0, floor_y + lobby_h - 0.1, s * (pd * 0.5 - 0.5)), true)
		_box(root, Vector3(0.12, 0.08, pd - 0.6), lobby_glow, Vector3(s * (pw * 0.5 - 0.5), floor_y + lobby_h - 0.1, 0), true)
	_light(root, Vector3(0, floor_y + 2.4, 0.5), Color(1.0, 0.9, 0.66), 3.0, 9.0)

	# the flared podium skirt (steps up from the plaza to the lobby, all sides
	# except the front so the entrance reads as the way in)
	for i: int in range(3):
		var sw: float = pw + 1.6 - float(i) * 0.5
		var sd: float = pd + 1.6 - float(i) * 0.5
		_box(root, Vector3(sw, 0.18, sd), marble if i % 2 == 0 else marble_dk, Vector3(0, 0.49, 0))
	# proper entrance steps out the front (+z), three risers
	for i: int in range(3):
		_box(root, Vector3(4.2, 0.14, 0.7), marble, Vector3(0, 0.32 - float(i) * 0.10, pd * 0.5 + 0.4 + float(i) * 0.7))

	# ── LOBBY COLONNADE: four fluted marble + gold columns inside, kept to the
	# sides/back so the centre stays clear and walkable ──
	_column(root, Vector3(-pw * 0.5 + 1.3, floor_y, -pd * 0.5 + 1.3), lobby_h - 0.3, 0.30, marble, gold)
	_column(root, Vector3(pw * 0.5 - 1.3, floor_y, -pd * 0.5 + 1.3), lobby_h - 0.3, 0.30, marble, gold)
	_column(root, Vector3(-pw * 0.5 + 1.3, floor_y, pd * 0.5 - 1.6), lobby_h - 0.3, 0.30, marble, gold)
	_column(root, Vector3(pw * 0.5 - 1.3, floor_y, pd * 0.5 - 1.6), lobby_h - 0.3, 0.30, marble, gold)

	# ── grand entrance portal: a tall gold-framed double door under a portico ──
	# portico canopy projecting out the front over the steps
	_box(root, Vector3(4.6, 0.30, 1.6), marble, Vector3(0, floor_y + 2.5, pd * 0.5 + 0.6))
	_box(root, Vector3(4.6, 0.10, 1.6), gold, Vector3(0, floor_y + 2.68, pd * 0.5 + 0.6))
	_prism(root, 0.0, 0.5, 0.5, gold, Vector3(0, floor_y + 2.9, pd * 0.5 + 0.6), 4)
	# a crowning gold finial + flanking acroteria on the portico pediment
	_ball(root, 0.14, gold, Vector3(0, floor_y + 3.18, pd * 0.5 + 0.6), Vector3.ONE, 10, 6)
	for s: float in [-1.0, 1.0]:
		_prism(root, 0.0, 0.12, 0.26, gold, Vector3(s * 2.1, floor_y + 2.78, pd * 0.5 + 0.6), 4)
	# two portico posts
	for s: float in [-1.0, 1.0]:
		_cyl(root, 0.16, 0.18, 2.4, marble, Vector3(s * 1.9, floor_y + 1.2, pd * 0.5 + 1.2), 16)
		_torus(root, 0.16, 0.22, gold, Vector3(s * 1.9, floor_y + 2.4, pd * 0.5 + 1.2), 16)
		_torus(root, 0.16, 0.20, gold, Vector3(s * 1.9, floor_y + 0.06, pd * 0.5 + 1.2), 16)
	# door surround set into the back of the portico (in the front parapet gap)
	_box(root, Vector3(2.6, 2.5, 0.16), gold, Vector3(0, floor_y + 1.25, pd * 0.5 - wall_t - 0.1))
	# the two glass-and-gold door leaves (entrance is ~2.2 tall, faces +z)
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(1.05, 2.2, 0.10), glass_warm, Vector3(s * 0.55, floor_y + 1.1, pd * 0.5 - wall_t - 0.02), true)
		_box(root, Vector3(0.08, 2.2, 0.12), gold, Vector3(s * 1.05, floor_y + 1.1, pd * 0.5 - wall_t - 0.02))
		_ball(root, 0.07, gold, Vector3(s * 0.15, floor_y + 1.1, pd * 0.5 - wall_t - 0.1), Vector3.ONE, 10, 5)
	# door transom glow above
	_box(root, Vector3(2.4, 0.4, 0.06), win_glow, Vector3(0, floor_y + 2.45, pd * 0.5 - wall_t - 0.05), true)

	# entrance landscaping: topiary pots flanking the steps + path lanterns
	_topiary(root, Vector3(-2.6, 0.4, pd * 0.5 + 1.6), gold)
	_topiary(root, Vector3(2.6, 0.4, pd * 0.5 + 1.6), gold)
	for i: int in range(3):
		for s: float in [-1.0, 1.0]:
			var lz: float = pd * 0.5 + 2.2 + float(i) * 1.6
			_cyl(root, 0.06, 0.08, 0.9, gold_dk, Vector3(s * 2.4, 0.85, lz), 10)
			_ball(root, 0.13, _glow(Color(1.0, 0.84, 0.5), 1.6), Vector3(s * 2.4, 1.4, lz), Vector3.ONE, 10, 5, true)
			_light(root, Vector3(s * 2.4, 1.4, lz), Color(1.0, 0.82, 0.5), 1.0, 3.0)

	# ── CENTRAL LOBBY FOUNTAIN: a showpiece on the open centre line, kept low so
	# it never blocks the walk-in sightline ──
	_fountain(root, Vector3(0, floor_y + 0.1, 1.0), 1.05, marble, gold)

	# ── LOBBY INTERIOR SHOWPIECES (kept to the back so the floor stays open) ──
	# reception desk along the back wall
	var desk := _gloss(Color(0.16, 0.17, 0.22), 0.3)
	_box(root, Vector3(3.4, 1.0, 0.8), desk, Vector3(0, floor_y + 0.5, -pd * 0.5 + 1.1))
	_box(root, Vector3(3.6, 0.08, 1.0), gold, Vector3(0, floor_y + 1.0, -pd * 0.5 + 1.1))
	_box(root, Vector3(3.0, 0.5, 0.06), win_glow, Vector3(0, floor_y + 0.5, -pd * 0.5 + 1.46), true)
	# a glowing gold "A" monogram crest on the back wall over the desk
	_torus(root, 0.6, 0.74, gold, Vector3(0, floor_y + 2.0, -pd * 0.5 + wall_t + 0.05), 24)
	_box(root, Vector3(0.16, 0.9, 0.06), gold, Vector3(-0.2, floor_y + 2.0, -pd * 0.5 + wall_t + 0.08)).rotation.z = 0.32
	_box(root, Vector3(0.16, 0.9, 0.06), gold, Vector3(0.2, floor_y + 2.0, -pd * 0.5 + wall_t + 0.08)).rotation.z = -0.32

	# ── GRAND STAIR up to the sky-lounge mezzanine (back-left, leaving centre clear) ──
	var stair := _gloss(Color(0.88, 0.86, 0.82), 0.3)
	var mez_y := floor_y + lobby_h + 0.4    # mezzanine floor sits on the podium top
	var n_steps := 12
	var step_x := -pw * 0.5 + 1.0
	for i: int in range(n_steps):
		var sy: float = floor_y + 0.1 + (mez_y - floor_y) * (float(i) + 1.0) / float(n_steps)
		var sz: float = -pd * 0.5 + 1.0 + float(i) * 0.52
		_box(root, Vector3(1.5, 0.16, 0.6), stair, Vector3(step_x, sy - 0.08, sz))
		_box(root, Vector3(1.5, 0.03, 0.6), gold, Vector3(step_x, sy + 0.005, sz), true)
	# a red carpet runner down the centre of the grand stair
	for i: int in range(n_steps):
		var ry: float = floor_y + 0.1 + (mez_y - floor_y) * (float(i) + 1.0) / float(n_steps)
		var rz: float = -pd * 0.5 + 1.0 + float(i) * 0.52
		_box(root, Vector3(0.7, 0.02, 0.6), _toon(Color(0.55, 0.10, 0.12), 0.5), Vector3(step_x, ry + 0.025, rz), true)
	# stair stringer + a gold-baluster handrail
	_box(root, Vector3(0.14, mez_y - floor_y, n_steps * 0.52), marble, Vector3(step_x + 0.78, floor_y + (mez_y - floor_y) * 0.5, -pd * 0.5 + 1.0 + n_steps * 0.26))
	for i: int in range(6):
		var hy: float = floor_y + 0.5 + (mez_y - floor_y) * float(i) / 6.0
		var hz: float = -pd * 0.5 + 1.2 + float(i) * 0.95
		_cyl(root, 0.025, 0.03, 0.9, gold, Vector3(step_x + 0.78, hy + 0.2, hz), 8)
		_ball(root, 0.05, gold, Vector3(step_x + 0.78, hy + 0.65, hz), Vector3.ONE, 8, 4)

	# ── SKY-LOUNGE MEZZANINE (a partial second-floor deck over the back half) ──
	var mez_d := pd * 0.5
	_box(root, Vector3(pw - 0.8, 0.20, mez_d), marble, Vector3(0, mez_y, -pd * 0.25))
	# mezzanine glass balustrade facing the open lobby (+z edge of the deck)
	_box(root, Vector3(pw - 0.8, 0.7, 0.08), glass, Vector3(0, mez_y + 0.45, -pd * 0.25 + mez_d * 0.5 - 0.04), true)
	_box(root, Vector3(pw - 0.8, 0.06, 0.12), gold, Vector3(0, mez_y + 0.8, -pd * 0.25 + mez_d * 0.5 - 0.04))
	# a GRAND CHANDELIER hung over the lobby void from the mezzanine ceiling
	_cyl(root, 0.04, 0.04, 0.8, gold, Vector3(0, floor_y + lobby_h - 0.4, 0.6), 8)
	_torus(root, 0.5, 0.6, gold, Vector3(0, floor_y + lobby_h - 0.9, 0.6), 24)
	_torus(root, 0.32, 0.4, gold, Vector3(0, floor_y + lobby_h - 1.25, 0.6), 20)
	for k: int in range(8):
		var ca: float = TAU * float(k) / 8.0
		_ball(root, 0.07, _glow(Color(1.0, 0.9, 0.66), 1.8), Vector3(cos(ca) * 0.55, floor_y + lobby_h - 1.1, 0.6 + sin(ca) * 0.55), Vector3.ONE, 8, 4, true)
		# a dripping inner ring of smaller crystal beads
		_ball(root, 0.045, _glow(Color(1.0, 0.94, 0.72), 1.6), Vector3(cos(ca) * 0.34, floor_y + lobby_h - 1.45, 0.6 + sin(ca) * 0.34), Vector3.ONE, 6, 4, true)
	_ball(root, 0.16, _glow(Color(1.0, 0.92, 0.72), 1.6), Vector3(0, floor_y + lobby_h - 1.0, 0.6), Vector3.ONE, 12, 6, true)
	_light(root, Vector3(0, floor_y + lobby_h - 1.0, 0.6), Color(1.0, 0.9, 0.68), 2.4, 8.0)
	# a built-in lounge fireplace feature on the mezzanine back wall
	_box(root, Vector3(2.0, 1.6, 0.3), marble_dk, Vector3(0, mez_y + 0.9, -pd * 0.5 + wall_t + 0.15))
	_box(root, Vector3(2.2, 0.1, 0.4), gold, Vector3(0, mez_y + 1.7, -pd * 0.5 + wall_t + 0.2))
	_box(root, Vector3(1.2, 0.7, 0.1), _glow(Color(1.0, 0.55, 0.22), 1.8), Vector3(0, mez_y + 0.6, -pd * 0.5 + wall_t + 0.22), true)
	# a gilt-framed artwork over the mantel
	_box(root, Vector3(1.0, 0.8, 0.06), _glow(Color(0.9, 0.7, 0.4), 0.8), Vector3(0, mez_y + 1.9, -pd * 0.5 + wall_t + 0.2), true)
	_box(root, Vector3(1.16, 0.06, 0.08), gold, Vector3(0, mez_y + 2.32, -pd * 0.5 + wall_t + 0.22))
	_box(root, Vector3(1.16, 0.06, 0.08), gold, Vector3(0, mez_y + 1.48, -pd * 0.5 + wall_t + 0.22))
	for s: float in [-1.0, 1.0]:
		_box(root, Vector3(0.06, 0.9, 0.08), gold, Vector3(s * 0.55, mez_y + 1.9, -pd * 0.5 + wall_t + 0.22))
	_light(root, Vector3(0, mez_y + 0.7, -pd * 0.5 + 1.0), Color(1.0, 0.55, 0.25), 1.6, 5.0)

	# ───────────────────── 3 · THE TAPERED GLASS SHAFT ──────────────────────
	# The shaft is a stack of glass "tube" segments that narrow toward the crown,
	# wrapped by full-height gold pilaster fins and banded by warm window glows.
	# It rises from the podium top (y=podium_top) to shaft_top.
	var seg_n := 5
	for i: int in range(seg_n):
		var t0: float = float(i) / float(seg_n)
		var t1: float = float(i + 1) / float(seg_n)
		var y0: float = lerp(podium_top, shaft_top, t0)
		var y1: float = lerp(podium_top, shaft_top, t1)
		var w0: float = base_w * lerp(1.0, taper, t0)
		var d0: float = base_d * lerp(1.0, taper, t0)
		var w1: float = base_w * lerp(1.0, taper, t1)
		var d1: float = base_d * lerp(1.0, taper, t1)
		var yc: float = (y0 + y1) * 0.5
		var wc: float = (w0 + w1) * 0.5
		var dc: float = (d0 + d1) * 0.5
		var h: float = y1 - y0
		# the four glass curtain walls of this segment (slightly chamfered corners)
		_box(root, Vector3(wc, h, 0.12), glass, Vector3(0, yc, dc * 0.5), true)        # +z
		_box(root, Vector3(wc, h, 0.12), glass, Vector3(0, yc, -dc * 0.5), true)       # -z
		_box(root, Vector3(0.12, h, dc), glass, Vector3(wc * 0.5, yc, 0), true)        # +x
		_box(root, Vector3(0.12, h, dc), glass, Vector3(-wc * 0.5, yc, 0), true)       # -x
		# a horizontal gold spandrel band at each floor split
		_box(root, Vector3(w1 + 0.2, 0.18, d1 + 0.2), gold, Vector3(0, y1, 0))
		# warm window glow bands at two levels within the segment
		for f: int in range(2):
			var fy: float = lerp(y0, y1, 0.28 + float(f) * 0.44)
			var fw: float = lerp(w0, w1, 0.28 + float(f) * 0.44)
			var fd: float = lerp(d0, d1, 0.28 + float(f) * 0.44)
			_box(root, Vector3(fw - 0.6, 0.5, 0.04), win_glow, Vector3(0, fy, fd * 0.5 + 0.05), true)
			_box(root, Vector3(fw - 0.6, 0.5, 0.04), win_glow, Vector3(0, fy, -fd * 0.5 - 0.05), true)
			_box(root, Vector3(0.04, 0.5, fd - 0.6), win_glow, Vector3(fw * 0.5 + 0.05, fy, 0), true)
			_box(root, Vector3(0.04, 0.5, fd - 0.6), win_glow, Vector3(-fw * 0.5 - 0.05, fy, 0), true)

	# projecting glass-and-gold BALCONIES on the front face between terraces — the
	# little cantilevers that catch the eye and say "private outdoor space".
	for by: float in [7.0, 12.5, 18.0]:
		var bt: float = (by - podium_top) / (shaft_top - podium_top)
		var bw: float = base_w * lerp(1.0, taper, bt)
		var bd: float = base_d * lerp(1.0, taper, bt)
		# slab + gold soffit, glass parapet, gold cap
		_box(root, Vector3(bw * 0.5, 0.14, 1.0), marble, Vector3(0, by, bd * 0.5 + 0.5))
		_box(root, Vector3(bw * 0.5 + 0.1, 0.06, 1.05), gold, Vector3(0, by - 0.10, bd * 0.5 + 0.5))
		_box(root, Vector3(bw * 0.5, 0.5, 0.05), glass, Vector3(0, by + 0.32, bd * 0.5 + 1.0), true)
		_box(root, Vector3(bw * 0.5, 0.05, 0.09), gold, Vector3(0, by + 0.58, bd * 0.5 + 1.0))
		for s: float in [-1.0, 1.0]:
			_box(root, Vector3(0.05, 0.5, 1.0), glass, Vector3(s * bw * 0.25, by + 0.32, bd * 0.5 + 0.5), true)

	# full-height gold pilaster fins at the four corners (the vertical accent that
	# stretches the silhouette and reads "designed")
	for cx: float in [-1.0, 1.0]:
		for cz: float in [-1.0, 1.0]:
			# a tapered fin tracking the shaft corner from podium to crown
			var fin := _prism(root,
				base_w * taper * 0.5 * 0.10,   # near-vanishing at top
				base_w * 0.5 * 0.13,           # thicker at base
				shaft_top - podium_top,
				gold,
				Vector3(cx * base_w * 0.5 * 0.9, (podium_top + shaft_top) * 0.5, cz * base_d * 0.5 * 0.9),
				4)
			fin.rotation.y = PI / 4.0
	# slim mullion fins on each face (vertical chrome lines on the glass)
	for i: int in range(7):
		var mx: float = lerp(-base_w * 0.5, base_w * 0.5, float(i) / 6.0) * 0.86
		_cyl(root, 0.05, 0.06, shaft_top - podium_top, chrome, Vector3(mx, (podium_top + shaft_top) * 0.5, base_d * 0.5 * 0.82), 6, true)
		_cyl(root, 0.05, 0.06, shaft_top - podium_top, chrome, Vector3(mx, (podium_top + shaft_top) * 0.5, -base_d * 0.5 * 0.82), 6, true)

	# ───────────────────── 4 · SETBACK GARDEN TERRACES ──────────────────────
	# Three planted setbacks where the shaft steps in — the wealth signature.
	var terr_levels: Array[float] = [0.22, 0.52, 0.80]   # fractional heights up the shaft
	for ti: int in range(terr_levels.size()):
		var t: float = terr_levels[ti]
		var ty: float = lerp(podium_top, shaft_top, t)
		var tw: float = base_w * lerp(1.0, taper, t) + 1.8 - float(ti) * 0.4
		var td: float = base_d * lerp(1.0, taper, t) + 1.6 - float(ti) * 0.4
		# the cantilevered terrace slab + gold soffit
		_box(root, Vector3(tw, 0.30, td), marble, Vector3(0, ty, 0))
		_box(root, Vector3(tw + 0.1, 0.08, td + 0.1), gold, Vector3(0, ty - 0.18, 0))
		# glass + gold terrace balustrade around the perimeter
		_box(root, Vector3(tw, 0.6, 0.06), glass, Vector3(0, ty + 0.45, td * 0.5 - 0.03), true)
		_box(root, Vector3(tw, 0.6, 0.06), glass, Vector3(0, ty + 0.45, -td * 0.5 + 0.03), true)
		_box(root, Vector3(0.06, 0.6, td), glass, Vector3(tw * 0.5 - 0.03, ty + 0.45, 0), true)
		_box(root, Vector3(0.06, 0.6, td), glass, Vector3(-tw * 0.5 + 0.03, ty + 0.45, 0), true)
		_box(root, Vector3(tw, 0.05, 0.10), gold, Vector3(0, ty + 0.75, td * 0.5 - 0.03))
		_box(root, Vector3(tw, 0.05, 0.10), gold, Vector3(0, ty + 0.75, -td * 0.5 + 0.03))
		# the gardens: planters in the front corners + a couple of topiary balls
		_planter(root, Vector3(-tw * 0.5 + 0.7, ty + 0.15, td * 0.5 - 0.7), 1.0, 0.9, gold)
		_planter(root, Vector3(tw * 0.5 - 0.7, ty + 0.15, td * 0.5 - 0.7), 1.0, 0.9, gold)
		_topiary(root, Vector3(-tw * 0.5 + 0.7, ty + 0.15, -td * 0.5 + 0.7), gold)
		_topiary(root, Vector3(tw * 0.5 - 0.7, ty + 0.15, -td * 0.5 + 0.7), gold)
		# a small classical statue centred at the back of each garden terrace
		_statue(root, Vector3(0, ty + 0.15, -td * 0.5 + 0.8), 0.7, marble, gold)
		# a warm uplight wash on each terrace garden
		_light(root, Vector3(0, ty + 0.6, td * 0.4), Color(1.0, 0.86, 0.6), 1.4, 5.0)

	# ───────────────────── 5 · SKY-LOUNGE PENTHOUSE (top floor) ──────────────
	var pen_y := shaft_top
	var pw2 := base_w * taper + 0.6
	var pd2 := base_d * taper + 0.6
	var pen_h := 3.4
	# penthouse floor slab
	_box(root, Vector3(pw2 + 0.4, 0.3, pd2 + 0.4), marble, Vector3(0, pen_y + 0.15, 0))
	# warm floor-to-ceiling glass on all four sides, gold-framed
	_box(root, Vector3(pw2, pen_h, 0.12), glass_warm, Vector3(0, pen_y + pen_h * 0.5, pd2 * 0.5), true)
	_box(root, Vector3(pw2, pen_h, 0.12), glass_warm, Vector3(0, pen_y + pen_h * 0.5, -pd2 * 0.5), true)
	_box(root, Vector3(0.12, pen_h, pd2), glass_warm, Vector3(pw2 * 0.5, pen_y + pen_h * 0.5, 0), true)
	_box(root, Vector3(0.12, pen_h, pd2), glass_warm, Vector3(-pw2 * 0.5, pen_y + pen_h * 0.5, 0), true)
	# gold corner posts + top & bottom frame rails
	for cx: float in [-1.0, 1.0]:
		for cz: float in [-1.0, 1.0]:
			_box(root, Vector3(0.22, pen_h, 0.22), gold, Vector3(cx * pw2 * 0.5, pen_y + pen_h * 0.5, cz * pd2 * 0.5))
	_box(root, Vector3(pw2 + 0.3, 0.22, pd2 + 0.3), gold, Vector3(0, pen_y + pen_h, 0))
	_box(root, Vector3(pw2 + 0.2, 0.18, pd2 + 0.2), gold, Vector3(0, pen_y + 0.2, 0))
	# pitched gold DORMER gables peaking out of each penthouse face — the crown floor
	for s: float in [-1.0, 1.0]:
		_prism(root, 0.0, 0.55, 0.7, gold, Vector3(0, pen_y + pen_h + 0.45, s * (pd2 * 0.5 + 0.05)), 3).rotation.x = PI * 0.5 * s
		_box(root, Vector3(0.7, 0.5, 0.04), win_glow, Vector3(0, pen_y + pen_h * 0.6, s * (pd2 * 0.5 + 0.07)), true)
	# the warm interior glow of the sky lounge + a real light
	_box(root, Vector3(pw2 - 0.6, 0.1, pd2 - 0.6), lobby_glow, Vector3(0, pen_y + pen_h - 0.2, 0), true)
	_light(root, Vector3(0, pen_y + pen_h * 0.5, 0), Color(1.0, 0.88, 0.6), 3.4, 11.0)
	# a sky-lounge centrepiece: a gold-ringed firepit table glowing in the lounge
	_cyl(root, 0.7, 0.8, 0.5, marble_dk, Vector3(0, pen_y + 0.55, 0), 20)
	_torus(root, 0.7, 0.82, gold, Vector3(0, pen_y + 0.8, 0), 24)
	_cyl(root, 0.5, 0.5, 0.1, _glow(Color(1.0, 0.55, 0.22), 1.8), Vector3(0, pen_y + 0.82, 0), 18, true)

	# ───────────────────── 6 · THE GOLD CROWN ────────────────────────────────
	var crown_y := pen_y + pen_h + 0.2
	# a stepped gold cornice cap
	_box(root, Vector3(pw2 + 0.6, 0.4, pd2 + 0.6), gold, Vector3(0, crown_y + 0.2, 0))
	_box(root, Vector3(pw2 - 0.2, 0.4, pd2 - 0.2), gold_dk, Vector3(0, crown_y + 0.6, 0))
	# guardian statues standing at the four crown-cornice corners (the apex figures)
	for cx: float in [-1.0, 1.0]:
		for cz: float in [-1.0, 1.0]:
			_statue(root, Vector3(cx * (pw2 * 0.5 + 0.1), crown_y + 0.4, cz * (pd2 * 0.5 + 0.1)), 0.5, marble, gold)
	# a ring of gold crown "flame" fins flaring up (the coronet)
	var cn := 12
	for k: int in range(cn):
		var ang: float = TAU * float(k) / float(cn)
		var rr: float = (pw2 + pd2) * 0.25 - 0.3
		var fin := _prism(root, 0.0, 0.22, 1.3, gold, Vector3(cos(ang) * rr, crown_y + 1.3, sin(ang) * rr), 4)
		fin.rotation.y = -ang
		# a small glow gem at each fin tip
		_ball(root, 0.10, _glow(Color(1.0, 0.84, 0.5), 1.8), Vector3(cos(ang) * rr * 1.05, crown_y + 1.95, sin(ang) * rr * 1.05), Vector3.ONE, 8, 4, true)
	# central faceted gold spire + glowing beacon
	_prism(root, 0.0, 0.7, 3.0, gold, Vector3(0, crown_y + 2.3, 0), 6)
	_torus(root, 0.4, 0.55, gold_dk, Vector3(0, crown_y + 1.4, 0), 18)
	_ball(root, 0.35, _glow(Color(1.0, 0.92, 0.7), 2.2), Vector3(0, crown_y + 4.0, 0), Vector3.ONE, 14, 7, true)
	_ball(root, 0.55, _glass(Color(1.0, 0.9, 0.6), 0.22, 1.4), Vector3(0, crown_y + 4.0, 0), Vector3.ONE, 14, 7, true)
	_prism(root, 0.0, 0.10, 0.9, gold, Vector3(0, crown_y + 4.7, 0), 4)   # antenna finial
	_ball(root, 0.10, _glow(Color(1.0, 0.4, 0.4), 2.4), Vector3(0, crown_y + 5.2, 0), Vector3.ONE, 8, 4, true)  # aviation beacon
	_light(root, Vector3(0, crown_y + 4.0, 0), Color(1.0, 0.9, 0.66), 4.0, 14.0)

	return root


# ════════════════════════════════════════════════════════════════════════════
static func meta() -> Dictionary:
	return {
		"id": "sky_tower",
		"name": "Aurelia Sky Tower",
		"tier": "Penthouse",
		"rarity": "Legendary",
		"description": "A tapered glass high-rise crowned in a faceted gold coronet, its setback garden terraces and projecting balconies stepping skyward past guardian statues to a glowing sky-lounge penthouse above a fountain-court marble colonnade lobby — the rarest address in the Verse.",
		"footprint": [17.6, 15.6],
		"floors": 14,
		"attributes": [
			["Style", "Modern Luxury High-Rise"],
			["Material", "Marble · Glass · Brushed Gold"],
			["Feature", "Gold Coronet · Sky-Lounge · Garden Terraces"],
			["Showpiece", "Lobby Fountain · Grand Chandelier · Statues"],
			["Floors", "14"],
			["Vibe", "Skyline Opulence"],
		],
	}
