# Hey Verse — premium procedural BUILDING: "Thistledown Manor Cottage" (Uncommon).
#
# A warm storybook timber-frame cottage, ELEVATED to an heirloom showpiece you
# OWN as an NFT and drop on your land. Whitewashed stucco panels laced with dark
# oak Tudor framing now meet brass-banded stone columns, a steep multi-pitch
# slate roof crowned with copper finials, twin gabled dormers with glowing
# leaded glass, a wrought-brass loft balcony, a carved round oak door under a
# gilded arch, overflowing flower boxes, formal box-hedge parterres, a tiered
# stone fountain, lantern-lit stepping stones, and a pair of guardian statues.
# Inside: a grand curved oak stair to the sleeping loft, a brass chandelier, and
# a carved stone hearth whose chimney trails a soft curl of smoke. The whole
# thing reads cosy yet unmistakably high-end and hand-built.
#
# Self-contained: it loads res://toon.gdshader + res://outline.gdshader by path
# (guarded by ResourceLoader.exists) and falls back to StandardMaterial3D so the
# module parses + runs standalone with no other Verse scripts present.
#
# Scale: ground floor at y=0, entrance faces +z, the FRONT WALL IS OMITTED so the
# camera looks straight into the walkable room (a low threshold sill remains).
# Door ~2.2 tall, ceiling ~2.95, windows ~1.4. Footprint ~9 x 8.
extends RefCounted
class_name VerseBuildingCozyCottage


# ───────────────────────────── palette ──────────────────────────────────────
const STUCCO     := Color(0.95, 0.93, 0.86)   # warm whitewash
const STUCCO_W   := Color(0.88, 0.85, 0.76)   # weathered panel
const OAK        := Color(0.34, 0.22, 0.13)   # dark timber framing
const OAK_LIGHT  := Color(0.46, 0.31, 0.18)   # door / shutters
const SLATE      := Color(0.30, 0.33, 0.40)   # roof slate
const SLATE_DK   := Color(0.22, 0.24, 0.30)
const STONE      := Color(0.62, 0.60, 0.55)   # chimney + base course (lifted)
const STONE_DK   := Color(0.48, 0.46, 0.42)
const STONE_LT   := Color(0.78, 0.76, 0.70)   # dressed ashlar columns
const BRASS      := Color(0.86, 0.69, 0.33)   # door ring / lantern / accents
const GOLD       := Color(0.95, 0.80, 0.42)   # finials / gilt trim (sparingly)
const GOLD_DK    := Color(0.72, 0.55, 0.24)
const COPPER     := Color(0.55, 0.78, 0.66)   # verdigris copper roof caps
const FLOOR_WOOD := Color(0.50, 0.34, 0.20)   # plank floor
const FLOOR_DK   := Color(0.40, 0.26, 0.15)   # inlaid border
const BEAM_WOOD  := Color(0.30, 0.20, 0.12)   # ceiling beams + loft
const GLOW_WARM  := Color(1.0, 0.84, 0.52)    # window / lantern light
const GLOW_GOLD  := Color(1.0, 0.90, 0.66)    # chandelier glow
const LEAF       := Color(0.32, 0.50, 0.26)   # hedges / box greenery
const LEAF_DK    := Color(0.24, 0.40, 0.20)
const PETAL_RED  := Color(0.86, 0.30, 0.30)
const PETAL_YEL  := Color(0.95, 0.80, 0.32)
const PETAL_PNK  := Color(0.92, 0.58, 0.70)
const PETAL_PUR  := Color(0.62, 0.44, 0.78)
const PATH_STONE := Color(0.70, 0.67, 0.62)
const WATER      := Color(0.46, 0.70, 0.80)
const SMOKE      := Color(0.82, 0.82, 0.84)
const MARBLE     := Color(0.90, 0.89, 0.86)   # statue / fountain stone


# ───────────────────────────── shared material cache ────────────────────────
static var _outline_mat: ShaderMaterial
static var _have_toon := false
static var _have_outline := false
static var _checked := false


static func _ensure_shaders() -> void:
	if _checked:
		return
	_checked = true
	_have_toon = ResourceLoader.exists("res://toon.gdshader")
	_have_outline = ResourceLoader.exists("res://outline.gdshader")


# ───────────────────────────── material helpers ─────────────────────────────

## Cel material + inverted-hull outline — the Verse "designed" look on solids.
## Falls back to a plain StandardMaterial3D when the shaders aren't present.
static func _toon(c: Color, rim := 0.3, outline := true, spec := 0.0) -> Material:
	_ensure_shaders()
	if not _have_toon:
		var sm := StandardMaterial3D.new()
		sm.albedo_color = c
		sm.roughness = 0.85
		sm.metallic = 0.0
		return sm
	var m := ShaderMaterial.new()
	m.shader = ResourceLoader.load("res://toon.gdshader")
	m.set_shader_parameter("albedo", c)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", spec)
	m.set_shader_parameter("wind_strength", 0.0)
	m.set_shader_parameter("wind_height", 0.5)
	if outline and _have_outline:
		if _outline_mat == null:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = ResourceLoader.load("res://outline.gdshader")
		m.next_pass = _outline_mat
	return m


## Polished metal — strong rim + spec dot reads as brass / chrome / gold.
static func _metal(c: Color, spec := 0.7) -> Material:
	return _toon(c, 0.5, true, spec)


## Soft sheen for slate / stone — slightly glossy toon.
static func _gloss(c: Color, spec := 0.25) -> Material:
	return _toon(c, 0.4, true, spec)


## Translucent glass for window panes — no shadow, faint glow.
static func _glass(c: Color, alpha := 0.30) -> Material:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.12
	m.metallic = 0.1
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.20
	return m


## Unshaded emissive — glowing windows, lantern flame, hearth embers.
static func _glow(c: Color, energy := 1.4) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


## Faintly translucent water for the fountain basin.
static func _water(c: Color, alpha := 0.55) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.05
	m.metallic = 0.3
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.10
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
	sm.radial_segments = 16
	sm.rings = 8
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.position = pos
	mi.scale = s
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, ring_seg := 10) -> MeshInstance3D:
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


static func _light(parent: Node3D, c: Color, energy: float, rng: float, pos: Vector3) -> OmniLight3D:
	var l := OmniLight3D.new()
	l.light_color = c
	l.light_energy = energy
	l.omni_range = rng
	l.position = pos
	l.shadow_enabled = false
	parent.add_child(l)
	return l


## A dressed stone column with brass base + gilt capital ring — the luxury motif.
static func _column(parent: Node3D, pos: Vector3, h: float, r := 0.22) -> void:
	var shaft := _gloss(STONE_LT, 0.2)
	var brass := _metal(BRASS, 0.85)
	var gold := _metal(GOLD, 0.9)
	# Stepped plinth.
	_box(parent, Vector3(r * 3.4, 0.18, r * 3.4), _gloss(STONE_DK, 0.15), pos + Vector3(0, 0.09, 0))
	_box(parent, Vector3(r * 2.8, 0.14, r * 2.8), _gloss(STONE, 0.18), pos + Vector3(0, 0.25, 0))
	# Brass base torus.
	_torus(parent, r * 0.7, r * 1.25, brass, pos + Vector3(0, 0.36, 0), Vector3(PI * 0.5, 0, 0), 12)
	# Fluted shaft (slim taper).
	_cyl(parent, r * 0.92, r, h, shaft, pos + Vector3(0, 0.4 + h * 0.5, 0), Vector3.ZERO, 14)
	# Gilt capital ring + abacus block.
	_torus(parent, r * 0.78, r * 1.35, gold, pos + Vector3(0, 0.4 + h, 0), Vector3(PI * 0.5, 0, 0), 12)
	_box(parent, Vector3(r * 2.9, 0.16, r * 2.9), _gloss(STONE_LT, 0.2), pos + Vector3(0, 0.5 + h, 0))


# ═══════════════════════════════ BUILD ══════════════════════════════════════

static func build() -> Node3D:
	var root := Node3D.new()
	root.name = "ThistledownManorCottage"

	_build_base(root)        # plinth + inlaid plank floor + ashlar foundation
	_build_walls(root)       # stucco shell, omitted front, oak Tudor framing
	_build_columns(root)     # entrance + corner stone columns (gilt capitals)
	_build_windows(root)     # glowing leaded windows + shutters + flower boxes
	_build_door(root)        # carved round oak door + gilded arch + lanterns
	_build_roof(root)        # multi-pitch slate + dormers + copper finials
	_build_chimney(root)     # carved stone chimney + smoke curl
	_build_balcony(root)     # wrought-brass loft balcony over the entrance
	_build_interior(root)    # beams, grand hearth, grand stair, chandelier, loft
	_build_landscape(root)   # parterre hedges, fountain, statues, path, lanterns
	_build_lighting(root)    # warm interior + dusk key

	return root


# ───────────────────────────── base / floor ─────────────────────────────────
static func _build_base(root: Node3D) -> void:
	var W := 9.0
	var D := 8.0
	# Wide dressed-ashlar terrace the whole manor sits on (reads expensive).
	_box(root, Vector3(W + 2.4, 0.3, D + 2.4), _gloss(STONE_LT, 0.2), Vector3(0, 0.15, 0.4))
	_box(root, Vector3(W + 2.0, 0.12, D + 2.0), _gloss(STONE, 0.18), Vector3(0, 0.34, 0.4))
	# Low stone foundation course wrapping the footprint.
	var fmat := _gloss(STONE, 0.15)
	_box(root, Vector3(W + 0.5, 0.4, D + 0.5), fmat, Vector3(0, 0.4, 0))
	# Darker inset ashlar band.
	_box(root, Vector3(W + 0.2, 0.18, D + 0.2), _gloss(STONE_DK, 0.1), Vector3(0, 0.51, 0))
	# Warm plank floor (the walkable interior surface).
	var floor_mat := _toon(FLOOR_WOOD, 0.18)
	_box(root, Vector3(W - 0.4, 0.12, D - 0.4), floor_mat, Vector3(0, 0.46, 0))
	# Inlaid dark border framing the floor — a touch of craftsmanship.
	var inlay := _toon(FLOOR_DK, 0.16)
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.18, 0.13, D - 0.4), inlay, Vector3(sx * (W * 0.5 - 0.45), 0.465, 0))
	for sz: float in [-1.0, 1.0]:
		_box(root, Vector3(W - 0.4, 0.13, 0.18), inlay, Vector3(0, 0.465, sz * (D * 0.5 - 0.45)))
	# Plank seams for texture — thin grooves running front-to-back.
	var seam := _toon(BEAM_WOOD, 0.1)
	for i: int in range(-3, 4):
		var x := float(i) * 1.05
		_box(root, Vector3(0.05, 0.13, D - 0.7), seam, Vector3(x, 0.465, 0))
	# A small brass medallion inlaid at floor centre — the house mark.
	_torus(root, 0.28, 0.42, _metal(GOLD, 0.9), Vector3(0, 0.53, 0), Vector3(PI * 0.5, 0, 0), 16)
	_cyl(root, 0.26, 0.26, 0.04, _metal(BRASS, 0.85), Vector3(0, 0.53, 0), Vector3.ZERO, 18)
	# A low front threshold sill (front wall omitted; this defines the edge).
	_box(root, Vector3(W - 0.4, 0.22, 0.3), _gloss(STONE_LT, 0.2), Vector3(0, 0.6, D * 0.5 - 0.25))
	# Gilt nosing strip along the threshold lip.
	_box(root, Vector3(W - 0.4, 0.05, 0.08), _metal(GOLD, 0.9), Vector3(0, 0.71, D * 0.5 - 0.13))


# ───────────────────────────── walls + framing ──────────────────────────────
static func _build_walls(root: Node3D) -> void:
	var W := 9.0
	var D := 8.0
	var H := 2.95               # eave wall height
	var t := 0.3                # wall thickness
	var stucco := _toon(STUCCO, 0.22)
	var stucco_w := _toon(STUCCO_W, 0.2)
	var oak := _toon(OAK, 0.2)
	var base_y := 0.52

	# Back wall (solid, with a small high window gap left for the back glow).
	_box(root, Vector3(W, H, t), stucco, Vector3(0, base_y + H * 0.5, -D * 0.5 + t * 0.5))
	# Left + right side walls.
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(t, H, D - t), stucco_w if sx < 0.0 else stucco,
			Vector3(sx * (W * 0.5 - t * 0.5), base_y + H * 0.5, 0))
	# Two short return wings on the FRONT so the room still reads enclosed while
	# the centre is open to the camera (entrance + view gap between them).
	var wing_w := 2.3
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(wing_w, H, t), stucco,
			Vector3(sx * (W * 0.5 - wing_w * 0.5), base_y + H * 0.5, D * 0.5 - t * 0.5))

	# ── Tudor timber framing: dark oak beams laid over the stucco. ──
	var fy := base_y
	# Sill beam + top plate around the visible shell (back + sides + front wings).
	_box(root, Vector3(W + 0.05, 0.22, 0.34), oak, Vector3(0, fy + 0.11, -D * 0.5 + t * 0.5))
	_box(root, Vector3(W + 0.05, 0.22, 0.34), oak, Vector3(0, fy + H - 0.11, -D * 0.5 + t * 0.5))
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.34, 0.22, D), oak, Vector3(sx * (W * 0.5 - t * 0.5), fy + 0.11, 0))
		_box(root, Vector3(0.34, 0.22, D), oak, Vector3(sx * (W * 0.5 - t * 0.5), fy + H - 0.11, 0))
		# Corner posts.
		_box(root, Vector3(0.3, H, 0.34), oak, Vector3(sx * (W * 0.5 - 0.15), fy + H * 0.5, -D * 0.5 + t))
		_box(root, Vector3(0.34, H, 0.3), oak, Vector3(sx * (W * 0.5 - t), fy + H * 0.5, D * 0.5 - 0.15))

	# Vertical studs across the back wall.
	for i: int in range(-2, 3):
		var x := float(i) * 1.7
		_box(root, Vector3(0.2, H - 0.4, 0.33), oak, Vector3(x, fy + H * 0.5, -D * 0.5 + t * 0.5))
	# Decorative diagonal braces on the back gable corners — storybook X's.
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.2, 2.0, 0.33), oak,
			Vector3(sx * 2.9, fy + H * 0.5, -D * 0.5 + t * 0.5), Vector3(0, 0, sx * 0.6))
		_box(root, Vector3(0.2, 2.0, 0.33), oak,
			Vector3(sx * 2.9, fy + H * 0.5, -D * 0.5 + t * 0.5), Vector3(0, 0, -sx * 0.6))
	# Studs on the two front wings too.
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.2, H - 0.4, 0.33), oak,
			Vector3(sx * (W * 0.5 - 1.1), fy + H * 0.5, D * 0.5 - t * 0.5))

	# Gable triangles (front + back) filling under the roof pitch.
	for sz: float in [-1.0, 1.0]:
		_prism(root, Vector3(W, 2.4, t), stucco_w,
			Vector3(0, base_y + H + 1.2, sz * (D * 0.5 - t * 0.5)))
		# Vertical gable beam.
		_box(root, Vector3(0.22, 2.4, 0.32), oak,
			Vector3(0, base_y + H + 1.2, sz * (D * 0.5 - t * 0.45)))


# ───────────────────────────── stone columns ────────────────────────────────
static func _build_columns(root: Node3D) -> void:
	var W := 9.0
	var D := 8.0
	var base_y := 0.52
	var H := 2.95
	# Two tall entrance columns flanking the open front (carry the balcony band).
	for sx: float in [-1.0, 1.0]:
		_column(root, Vector3(sx * (W * 0.5 - 0.55), base_y - 0.06, D * 0.5 - 0.3), H - 0.5, 0.26)
	# Four shorter corner columns on the terrace corners (frame the silhouette).
	for sx: float in [-1.0, 1.0]:
		for sz: float in [-1.0, 1.0]:
			_column(root, Vector3(sx * (W * 0.5 + 0.85), 0.34, sz * (D * 0.5 + 0.85) + 0.4), 1.7, 0.18)


# ───────────────────────────── windows + boxes ──────────────────────────────
static func _build_windows(root: Node3D) -> void:
	var oak := _toon(OAK, 0.2)
	var shutter := _toon(OAK_LIGHT, 0.2)
	var pane := _glass(GLOW_WARM, 0.45)
	var brass := _metal(BRASS, 0.85)
	var box_mat := _toon(BEAM_WOOD, 0.15)
	var base_y := 0.52

	# Side-wall windows (left + right), each a leaded casement with shutters,
	# a brass cresting, and an overflowing flower box.
	for sx: float in [-1.0, 1.0]:
		var wx := sx * (4.5 - 0.18)
		var wy := base_y + 1.55
		for wz: float in [-1.4, 1.4]:
			# Glowing pane.
			_box(root, Vector3(0.1, 1.2, 1.0), pane, Vector3(wx, wy, wz))
			# Oak frame.
			_box(root, Vector3(0.16, 1.4, 0.12), oak, Vector3(wx, wy, wz - 0.56))
			_box(root, Vector3(0.16, 1.4, 0.12), oak, Vector3(wx, wy, wz + 0.56))
			_box(root, Vector3(0.16, 0.12, 1.2), oak, Vector3(wx, wy + 0.64, wz))
			_box(root, Vector3(0.16, 0.12, 1.2), oak, Vector3(wx, wy - 0.64, wz))
			# Leaded muntins (cross + diamond hint).
			_box(root, Vector3(0.13, 1.3, 0.06), oak, Vector3(wx, wy, wz))
			_box(root, Vector3(0.13, 0.06, 1.05), oak, Vector3(wx, wy, wz))
			# Brass cresting bar above the window head.
			_box(root, Vector3(0.18, 0.05, 1.1), brass, Vector3(wx, wy + 0.74, wz))
			# Shutters flanking the casement.
			for ss: float in [-1.0, 1.0]:
				_box(root, Vector3(0.1, 1.35, 0.5), shutter,
					Vector3(wx + sx * 0.04, wy, wz + ss * 0.78))
			# Flower box under the sill (with brass strap).
			var bx := wx
			_box(root, Vector3(0.34, 0.28, 1.25), box_mat, Vector3(bx, wy - 0.78, wz))
			_box(root, Vector3(0.36, 0.05, 1.27), brass, Vector3(bx, wy - 0.66, wz))
			_flowers(root, Vector3(bx, wy - 0.55, wz), sx, wz)

	# A high glowing back-wall window for inner warmth seen through the door.
	_box(root, Vector3(1.4, 0.9, 0.1), pane, Vector3(0, base_y + 1.9, -4.0 + 0.05))
	_box(root, Vector3(1.6, 0.14, 0.16), oak, Vector3(0, base_y + 2.4, -3.96))
	_box(root, Vector3(0.1, 0.9, 0.14), oak, Vector3(0, base_y + 1.9, -3.96))
	# Gilt sunburst keystone over the back window.
	_torus(root, 0.12, 0.24, _metal(GOLD, 0.9), Vector3(0, base_y + 2.55, -3.94), Vector3.ZERO, 12)


## A clump of toon flowers spilling from a window box.
static func _flowers(root: Node3D, base: Vector3, side: float, wz: float) -> void:
	var stem := _toon(LEAF_DK, 0.2)
	var petals := [PETAL_RED, PETAL_YEL, PETAL_PNK, PETAL_PUR, PETAL_YEL]
	var n := petals.size()
	for i: int in range(n):
		var fz := wz - 0.5 + float(i) * (1.0 / float(n - 1))
		var lift := 0.18 + 0.06 * float(i % 2)
		_cyl(root, 0.02, 0.03, 0.3, stem, base + Vector3(side * 0.05, lift, fz - wz), Vector3(0, 0, side * 0.1))
		var pc: Color = petals[i]
		_ball(root, 0.13, _toon(pc, 0.35), base + Vector3(side * 0.07, lift + 0.16, fz - wz), Vector3(1, 0.8, 1))
		_ball(root, 0.05, _glow(PETAL_YEL, 0.6), base + Vector3(side * 0.1, lift + 0.18, fz - wz))
	# A little trailing greenery over the box lip.
	for j: int in range(3):
		_ball(root, 0.1, _toon(LEAF, 0.25), base + Vector3(side * 0.12, -0.02, -0.4 + float(j) * 0.4), Vector3(1, 0.7, 1))


# ───────────────────────────── round door + porch ───────────────────────────
static func _build_door(root: Node3D) -> void:
	var base_y := 0.52
	var oak := _toon(OAK_LIGHT, 0.25)
	var oak_dk := _toon(OAK, 0.2)
	var brass := _metal(BRASS, 0.85)
	var gold := _metal(GOLD, 0.9)
	var d := 8.0 * 0.5 - 0.15   # front-plane z

	# Dressed stone surround framing the entrance gap between the wings.
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.45, 2.7, 0.5), _gloss(STONE_LT, 0.2),
			Vector3(sx * 1.1, base_y + 1.35, d))
	# Round-topped arch over the doorway (beveled stones) with a gilt keystone.
	_box(root, Vector3(2.6, 0.4, 0.5), _gloss(STONE_LT, 0.2), Vector3(0, base_y + 2.55, d))
	_cyl(root, 1.0, 1.0, 0.45, _gloss(STONE, 0.15), Vector3(0, base_y + 2.55, d), Vector3(PI * 0.5, 0, 0), 16)
	# Gilded arch band tracing the round top.
	_torus(root, 0.92, 1.05, gold, Vector3(0, base_y + 2.55, d + 0.04), Vector3.ZERO, 16)
	_prism(root, Vector3(0.4, 0.5, 0.2), gold, Vector3(0, base_y + 3.7, d + 0.02))

	# The round oak door — plank slab with a curved top.
	var dy := base_y + 1.1
	_box(root, Vector3(1.7, 2.1, 0.18), oak, Vector3(0, dy, d - 0.06))
	_cyl(root, 0.85, 0.85, 0.18, oak, Vector3(0, dy + 1.05, d - 0.06), Vector3(PI * 0.5, 0, 0), 16)
	# Vertical plank lines + iron straps.
	for i: int in range(-2, 3):
		_box(root, Vector3(0.05, 2.0, 0.2), oak_dk, Vector3(float(i) * 0.4, dy, d - 0.04))
	for sy: float in [-0.6, 0.55]:
		_box(root, Vector3(1.7, 0.12, 0.2), brass, Vector3(0, dy + sy, d - 0.03))
	# Brass ring knocker + gilt studs.
	_torus(root, 0.08, 0.16, brass, Vector3(0.35, dy + 0.1, d + 0.04), Vector3(PI * 0.5, 0, 0))
	for sy: float in [-0.6, 0.0, 0.55]:
		for sx: float in [-0.7, 0.7]:
			_ball(root, 0.05, gold, Vector3(sx, dy + sy, d + 0.04))

	# Carved stone door step with a brass nosing.
	_box(root, Vector3(2.2, 0.2, 0.7), _gloss(STONE_LT, 0.2), Vector3(0, base_y + 0.05, d + 0.3))
	_box(root, Vector3(2.2, 0.05, 0.1), brass, Vector3(0, base_y + 0.16, d + 0.62))

	# Two matching porch lanterns flanking the door (was one — now a pair).
	for sx: float in [-1.0, 1.0]:
		var lx := sx * 1.5
		var lz := d + 0.1
		_box(root, Vector3(0.08, 0.5, 0.08), oak_dk, Vector3(lx, base_y + 2.6, lz))
		_cyl(root, 0.0, 0.04, 0.3, oak_dk, Vector3(lx, base_y + 2.35, lz - 0.2), Vector3(PI * 0.5, 0, 0), 8)
		_box(root, Vector3(0.22, 0.3, 0.22), brass, Vector3(lx, base_y + 2.05, lz - 0.2))
		_box(root, Vector3(0.16, 0.22, 0.16), _glow(GLOW_WARM, 1.8), Vector3(lx, base_y + 2.05, lz - 0.2))
		_light(root, GLOW_WARM, 1.1, 4.0, Vector3(lx, base_y + 2.05, lz - 0.2))


# ───────────────────────────── steep slate roof ─────────────────────────────
static func _build_roof(root: Node3D) -> void:
	var W := 9.0
	var D := 8.0
	var base_y := 0.52
	var H := 2.95
	var eave_y := base_y + H
	var ridge := 2.8           # height of ridge above eave (steeper, grander)
	var overhang := 0.6
	var slate := _gloss(SLATE, 0.3)
	var slate_dk := _gloss(SLATE_DK, 0.25)
	var rmat := _toon(BEAM_WOOD, 0.15)
	var gold := _metal(GOLD, 0.9)
	var copper := _metal(COPPER, 0.6)

	# Two big sloped slabs (a clean pitched plane each side).
	var slope_len := sqrt(pow(W * 0.5 + overhang, 2.0) + ridge * ridge)
	var ang := atan2(ridge, W * 0.5 + overhang)
	for sx: float in [-1.0, 1.0]:
		var slab := _box(root, Vector3(slope_len, 0.18, D + overhang * 2.0), slate,
			Vector3(sx * (W * 0.25), eave_y + ridge * 0.5 + 0.1, 0),
			Vector3(0, 0, -sx * ang))
		slab.scale = Vector3(1, 1, 1)
	# Slate course lines (rows of tiles) on each slope for richness.
	for sx: float in [-1.0, 1.0]:
		for c: int in range(1, 5):
			var f := float(c) / 5.0
			var px := sx * (W * 0.5 + overhang) * (1.0 - f) * 0.5
			var py := eave_y + ridge * f + 0.12
			_box(root, Vector3(0.05, 0.06, D + overhang * 2.0 - 0.1), slate_dk,
				Vector3(px, py, 0), Vector3(0, 0, -sx * ang))

	# Ridge beam capping the peak + verdigris copper ridge cap.
	_box(root, Vector3(0.3, 0.3, D + overhang * 2.0), slate_dk, Vector3(0, eave_y + ridge + 0.1, 0))
	_cyl(root, 0.13, 0.13, D + overhang * 2.0, copper, Vector3(0, eave_y + ridge + 0.22, 0), Vector3(PI * 0.5, 0, 0), 10)

	# Gilt finials at each ridge end — the crown of the silhouette.
	for sz: float in [-1.0, 1.0]:
		var fz := sz * (D * 0.5 + overhang - 0.05)
		_cyl(root, 0.04, 0.1, 0.45, copper, Vector3(0, eave_y + ridge + 0.42, fz), Vector3.ZERO, 10)
		_ball(root, 0.16, gold, Vector3(0, eave_y + ridge + 0.72, fz))
		_cyl(root, 0.0, 0.04, 0.3, gold, Vector3(0, eave_y + ridge + 1.0, fz), Vector3.ZERO, 8)

	# Eave fascia boards + exposed rafter tails.
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.16, 0.2, D + overhang * 2.0), rmat,
			Vector3(sx * (W * 0.5 + overhang - 0.05), eave_y + 0.05, 0))
		for i: int in range(-3, 4):
			_box(root, Vector3(overhang, 0.1, 0.12), rmat,
				Vector3(sx * (W * 0.5 + overhang * 0.5), eave_y + 0.02, float(i) * 1.1))

	# Bargeboards on the front + back gables (storybook scalloped trim).
	for sz: float in [-1.0, 1.0]:
		for sx: float in [-1.0, 1.0]:
			_box(root, Vector3(slope_len, 0.16, 0.18), rmat,
				Vector3(sx * (W * 0.25), eave_y + ridge * 0.5 + 0.12, sz * (D * 0.5 + overhang)),
				Vector3(0, 0, -sx * ang))

	# ── Twin gabled DORMERS poking from each roof slope (glowing leaded glass). ──
	var pane := _glass(GLOW_WARM, 0.5)
	var oak := _toon(OAK, 0.2)
	for sx: float in [-1.0, 1.0]:
		for dz: float in [-1.6, 1.6]:
			var f := 0.42
			var dxp := sx * (W * 0.5 + overhang) * (1.0 - f) * 0.55
			var dyp := eave_y + ridge * f + 0.35
			# Dormer body + little roof.
			_box(root, Vector3(0.8, 1.0, 1.1), _toon(STUCCO, 0.22), Vector3(dxp, dyp, dz))
			_prism(root, Vector3(1.2, 0.7, 1.3), slate, Vector3(dxp + sx * 0.1, dyp + 0.7, dz), Vector3(0, 0, sx * PI * 0.5))
			# Glowing dormer pane + oak frame + brass cresting.
			_box(root, Vector3(0.1, 0.7, 0.7), pane, Vector3(dxp + sx * 0.42, dyp + 0.05, dz))
			_box(root, Vector3(0.14, 0.12, 0.82), oak, Vector3(dxp + sx * 0.42, dyp + 0.42, dz))
			_box(root, Vector3(0.14, 0.6, 0.1), oak, Vector3(dxp + sx * 0.42, dyp + 0.05, dz))
			_ball(root, 0.07, gold, Vector3(dxp + sx * 0.42, dyp + 0.78, dz))


# ───────────────────────────── stone chimney + smoke ────────────────────────
static func _build_chimney(root: Node3D) -> void:
	var base_y := 0.52
	var H := 2.95
	var cx := -2.6
	var cz := -1.2
	var stone := _gloss(STONE, 0.18)
	var stone_dk := _gloss(STONE_DK, 0.12)
	var brass := _metal(BRASS, 0.85)
	var top := base_y + H + 3.6

	# Stack rising through the roof.
	_box(root, Vector3(1.0, 4.8, 0.9), stone, Vector3(cx, base_y + H + 0.3, cz))
	# Random stacked-stone look — offset blocks.
	for r: int in range(7):
		var y := base_y + H - 0.4 + float(r) * 0.72
		var off := 0.06 if r % 2 == 0 else -0.06
		_box(root, Vector3(1.05, 0.18, 0.95), stone_dk, Vector3(cx + off, y, cz))
	# Brass banding ring partway up (luxury accent).
	_box(root, Vector3(1.08, 0.08, 0.98), brass, Vector3(cx, base_y + H + 1.7, cz))
	# Corbelled cap with a brass coping.
	_box(root, Vector3(1.25, 0.25, 1.15), stone_dk, Vector3(cx, top - 0.2, cz))
	_box(root, Vector3(1.05, 0.2, 0.95), stone, Vector3(cx, top, cz))
	_box(root, Vector3(1.1, 0.05, 1.0), brass, Vector3(cx, top + 0.12, cz))
	# Two clay pots on top.
	for po: float in [-0.22, 0.22]:
		_cyl(root, 0.16, 0.18, 0.4, _toon(Color(0.62, 0.35, 0.24), 0.25),
			Vector3(cx + po, top + 0.25, cz), Vector3.ZERO, 12)

	# A soft curl of rising smoke (cheap CPU particles).
	var smoke := CPUParticles3D.new()
	smoke.position = Vector3(cx, top + 0.5, cz)
	smoke.amount = 14
	smoke.lifetime = 4.0
	smoke.preprocess = 3.0
	smoke.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	smoke.emission_sphere_radius = 0.12
	smoke.direction = Vector3(0.2, 1.0, 0.0)
	smoke.spread = 12.0
	smoke.gravity = Vector3(0.15, 0.35, 0.0)
	smoke.initial_velocity_min = 0.2
	smoke.initial_velocity_max = 0.45
	smoke.scale_amount_min = 1.0
	smoke.scale_amount_max = 2.6
	smoke.damping_min = 0.1
	smoke.damping_max = 0.3
	var puff := SphereMesh.new()
	puff.radius = 0.18
	puff.height = 0.36
	puff.radial_segments = 8
	puff.rings = 4
	var pm := StandardMaterial3D.new()
	pm.albedo_color = Color(SMOKE.r, SMOKE.g, SMOKE.b, 0.5)
	pm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	pm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	puff.material = pm
	smoke.mesh = puff
	smoke.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(smoke)


# ───────────────────────────── loft balcony (over entrance) ──────────────────
static func _build_balcony(root: Node3D) -> void:
	var W := 9.0
	var D := 8.0
	var base_y := 0.52
	var H := 2.95
	var d := D * 0.5 - 0.2
	var deck := _toon(FLOOR_WOOD, 0.18)
	var brass := _metal(BRASS, 0.85)
	var gold := _metal(GOLD, 0.9)
	var stone := _gloss(STONE_LT, 0.2)

	# A small projecting balcony deck carried over the open entrance.
	var by := base_y + H - 0.55
	_box(root, Vector3(4.0, 0.18, 0.9), deck, Vector3(0, by, d + 0.2))
	_box(root, Vector3(4.2, 0.12, 0.18), stone, Vector3(0, by - 0.1, d + 0.6))
	# Carved corbels under the deck (support look).
	for sx: float in [-1.0, 1.0]:
		_prism(root, Vector3(0.4, 0.5, 0.6), stone, Vector3(sx * 1.6, by - 0.45, d + 0.3), Vector3(PI, 0, 0))
	# Wrought-brass balustrade: top rail + turned balusters + gilt finials.
	_box(root, Vector3(4.0, 0.08, 0.08), brass, Vector3(0, by + 0.62, d + 0.6))
	_box(root, Vector3(4.0, 0.06, 0.06), gold, Vector3(0, by + 0.66, d + 0.6))
	for i: int in range(-6, 7):
		var x := float(i) * 0.32
		_cyl(root, 0.03, 0.03, 0.6, brass, Vector3(x, by + 0.3, d + 0.6), Vector3.ZERO, 6)
		if absi(i) == 6:
			_ball(root, 0.07, gold, Vector3(x, by + 0.7, d + 0.6))
	# A pair of potted topiary on the balcony for charm.
	for sx: float in [-1.0, 1.0]:
		_cyl(root, 0.14, 0.18, 0.22, _toon(Color(0.62, 0.35, 0.24), 0.25), Vector3(sx * 1.7, by + 0.2, d + 0.3), Vector3.ZERO, 10)
		_ball(root, 0.24, _toon(LEAF, 0.25), Vector3(sx * 1.7, by + 0.5, d + 0.3), Vector3(1, 0.9, 1))
		_ball(root, 0.12, _toon(PETAL_PNK, 0.3), Vector3(sx * 1.7, by + 0.6, d + 0.38))


# ───────────────────────────── interior: grand showpieces ───────────────────
static func _build_interior(root: Node3D) -> void:
	var W := 9.0
	var D := 8.0
	var base_y := 0.52
	var H := 2.95
	var beam := _toon(BEAM_WOOD, 0.18)
	var ceil_mat := _toon(STUCCO_W, 0.18)
	var brass := _metal(BRASS, 0.85)
	var gold := _metal(GOLD, 0.9)

	# ── Exposed ceiling beams running side to side (storybook rafters). ──
	for i: int in range(-3, 4):
		var z := float(i) * 1.0
		_box(root, Vector3(W - 0.6, 0.2, 0.22), beam, Vector3(0, base_y + H - 0.15, z))
	# A central tie-beam ridge piece.
	_box(root, Vector3(0.24, 0.26, D - 0.6), beam, Vector3(0, base_y + H - 0.05, 0))
	# Plaster ceiling above the beams (partial — leaves loft area open).
	_box(root, Vector3(W - 0.5, 0.1, D * 0.55), ceil_mat,
		Vector3(0, base_y + H, D * 0.22))

	# ── Grand stone hearth + carved fireplace in the back-left corner. ──
	var hx := -2.6
	var hz := -3.4
	var stone := _gloss(STONE_LT, 0.2)
	_box(root, Vector3(2.4, 2.5, 0.95), stone, Vector3(hx, base_y + 1.25, hz))      # surround
	# Carved pilasters flanking the firebox.
	for sx: float in [-1.0, 1.0]:
		_cyl(root, 0.1, 0.12, 1.6, _gloss(STONE, 0.18), Vector3(hx + sx * 0.95, base_y + 0.95, hz + 0.35), Vector3.ZERO, 10)
		_torus(root, 0.1, 0.18, gold, Vector3(hx + sx * 0.95, base_y + 1.78, hz + 0.35), Vector3(PI * 0.5, 0, 0), 10)
	_box(root, Vector3(1.3, 1.4, 0.5), _toon(Color(0.12, 0.10, 0.10), 0.1),
		Vector3(hx, base_y + 0.7, hz + 0.3))                                       # firebox void
	# Glowing embers + flames.
	_box(root, Vector3(1.0, 0.2, 0.4), _glow(Color(1.0, 0.45, 0.15), 2.0),
		Vector3(hx, base_y + 0.16, hz + 0.35))
	for i: int in range(3):
		var fx := hx - 0.3 + float(i) * 0.3
		_prism(root, Vector3(0.3, 0.6, 0.2), _glow(Color(1.0, 0.6, 0.2), 1.8),
			Vector3(fx, base_y + 0.45, hz + 0.35))
		_prism(root, Vector3(0.18, 0.4, 0.14), _glow(GLOW_WARM, 2.2),
			Vector3(fx, base_y + 0.6, hz + 0.36))
	# Carved oak mantel with a gilt frieze + a couple of logs.
	_box(root, Vector3(2.6, 0.22, 1.05), beam, Vector3(hx, base_y + 1.75, hz + 0.05))
	_box(root, Vector3(2.5, 0.06, 1.0), gold, Vector3(hx, base_y + 1.88, hz + 0.05))
	for lg: int in range(2):
		_cyl(root, 0.1, 0.1, 0.7, beam, Vector3(hx - 0.2 + float(lg) * 0.4, base_y + 0.12, hz + 0.4),
			Vector3(0, 0, PI * 0.5), 8)
	# A pair of brass candlesticks on the mantel (a showpiece touch).
	for cs: float in [-0.7, 0.7]:
		_cyl(root, 0.05, 0.08, 0.3, brass, Vector3(hx + cs, base_y + 2.0, hz + 0.05), Vector3.ZERO, 8)
		_box(root, Vector3(0.05, 0.18, 0.05), _glow(GLOW_WARM, 1.6), Vector3(hx + cs, base_y + 2.24, hz + 0.05))
	_light(root, Color(1.0, 0.55, 0.2), 1.9, 5.5, Vector3(hx, base_y + 0.7, hz + 0.6))

	# ── Sleeping loft over the back half, with a balustrade + access. ──
	var loft_y := base_y + 1.9
	var loft := _toon(FLOOR_WOOD, 0.18)
	_box(root, Vector3(W - 0.8, 0.16, D * 0.45), loft, Vector3(0, loft_y, -D * 0.27))
	# Loft floor planks.
	for i: int in range(-3, 4):
		_box(root, Vector3(0.04, 0.17, D * 0.45), beam, Vector3(float(i) * 1.05, loft_y + 0.01, -D * 0.27))
	# Loft balustrade facing the room (turned posts + gilt rail).
	var rail := _toon(BEAM_WOOD, 0.2)
	var rz := -D * 0.27 + D * 0.225
	_box(root, Vector3(W - 0.8, 0.1, 0.1), rail, Vector3(0, loft_y + 0.55, rz))
	_box(root, Vector3(W - 0.8, 0.05, 0.06), gold, Vector3(0, loft_y + 0.62, rz))
	for i: int in range(-4, 5):
		_cyl(root, 0.04, 0.04, 0.55, rail, Vector3(float(i) * 0.9, loft_y + 0.28, rz), Vector3.ZERO, 8)
	# A cosy bedroll + pillow on the loft.
	_box(root, Vector3(2.0, 0.18, 1.1), _toon(Color(0.78, 0.4, 0.4), 0.25), Vector3(0.6, loft_y + 0.18, -D * 0.32))
	_box(root, Vector3(0.7, 0.22, 0.9), _toon(STUCCO, 0.3), Vector3(-0.3, loft_y + 0.2, -D * 0.32))

	# ── GRAND CURVED STAIR up to the loft (against the right wall). ──
	# Stepped treads sweeping in an arc — a real showpiece versus a ladder.
	var sx0 := W * 0.5 - 1.0
	var steps := 7
	for s: int in range(steps):
		var t := float(s) / float(steps - 1)
		var sy := base_y + 0.2 + t * (loft_y - base_y - 0.1)
		var arc := -0.9 + t * 0.9
		var szp := rz + 0.4 + arc
		_box(root, Vector3(1.0, 0.12, 0.5), loft, Vector3(sx0, sy, szp))
		_box(root, Vector3(1.0, 0.4, 0.06), _toon(FLOOR_DK, 0.15), Vector3(sx0, sy - 0.22, szp + 0.25))
		# Brass-topped newel baluster on the open side of each tread.
		_cyl(root, 0.025, 0.025, 0.5, _toon(BEAM_WOOD, 0.2), Vector3(sx0 - 0.45, sy + 0.3, szp), Vector3.ZERO, 6)
		if s == 0 or s == steps - 1:
			_ball(root, 0.06, gold, Vector3(sx0 - 0.45, sy + 0.58, szp))
	# Curved handrail riding the newels.
	_cyl(root, 0.035, 0.035, 2.4, brass, Vector3(sx0 - 0.45, base_y + 1.1, rz + 0.4 - 0.1), Vector3(0.55, 0, 0.0), 8)

	# ── A BRASS CHANDELIER hung from the central beam (the room's crown). ──
	var chy := base_y + H - 0.45
	_cyl(root, 0.0, 0.03, 0.5, brass, Vector3(0, chy + 0.25, 0.4), Vector3.ZERO, 6)
	_torus(root, 0.32, 0.46, brass, Vector3(0, chy, 0.4), Vector3(PI * 0.5, 0, 0), 14)
	_torus(root, 0.16, 0.26, gold, Vector3(0, chy + 0.12, 0.4), Vector3(PI * 0.5, 0, 0), 12)
	# Candle arms + flames around the ring.
	for a: int in range(6):
		var th := float(a) * TAU / 6.0
		var ax := cos(th) * 0.4
		var az := 0.4 + sin(th) * 0.4
		_cyl(root, 0.02, 0.02, 0.16, _toon(STUCCO, 0.3), Vector3(ax, chy + 0.1, az), Vector3.ZERO, 6)
		_box(root, Vector3(0.05, 0.1, 0.05), _glow(GLOW_GOLD, 2.0), Vector3(ax, chy + 0.24, az))
	# Hanging crystal drops (gilt teardrops) for sparkle.
	for a: int in range(6):
		var th := float(a) * TAU / 6.0 + 0.5
		_ball(root, 0.04, _glow(GLOW_GOLD, 1.2), Vector3(cos(th) * 0.46, chy - 0.18, 0.4 + sin(th) * 0.46))
	_light(root, GLOW_GOLD, 1.6, 6.0, Vector3(0, chy - 0.1, 0.4))


# ───────────────────────────── landscaping / garden ─────────────────────────
static func _build_landscape(root: Node3D) -> void:
	var path := _gloss(PATH_STONE, 0.12)
	var hedge := _toon(LEAF, 0.25)
	var hedge_dk := _toon(LEAF_DK, 0.2)
	var oak := _toon(OAK_LIGHT, 0.2)
	var brass := _metal(BRASS, 0.85)
	var gold := _metal(GOLD, 0.9)
	var marble := _gloss(MARBLE, 0.3)

	# Stepping-stone path leading from the door out toward +z, framed by a
	# pale paved border (reads like a formal approach, not a footpath).
	for i: int in range(7):
		var pz := 4.4 + float(i) * 0.95
		var ofx := 0.18 * sin(float(i) * 1.3)
		_cyl(root, 0.62, 0.66, 0.1, path, Vector3(ofx, 0.06, pz), Vector3.ZERO, 8)
		_torus(root, 0.6, 0.72, _gloss(STONE_LT, 0.2), Vector3(ofx, 0.07, pz), Vector3(PI * 0.5, 0, 0), 10)

	# ── Formal box-hedge parterres hugging the front wings + sides. ──
	for sx: float in [-1.0, 1.0]:
		for i: int in range(4):
			var z := -2.6 + float(i) * 1.7
			_ball(root, 0.55, hedge, Vector3(sx * 5.0, 0.5, z), Vector3(1.1, 0.8, 1.1))
			_ball(root, 0.3, hedge_dk, Vector3(sx * 5.0, 0.75, z + 0.2), Vector3(1, 0.8, 1))
		# Clipped cone topiary flanking the path entrance (manor formality).
		_cyl(root, 0.0, 0.5, 1.4, hedge, Vector3(sx * 1.9, 1.1, 5.4), Vector3.ZERO, 10)
		_ball(root, 0.18, _toon(PETAL_YEL, 0.3), Vector3(sx * 1.9, 1.85, 5.4))
		# Low parterre edging boxes between the cones and the terrace.
		for j: int in range(3):
			_box(root, Vector3(0.7, 0.4, 0.7), hedge, Vector3(sx * 3.2, 0.3, 4.6 + float(j) * 1.0))

	# ── A tiered stone FOUNTAIN centred on the approach axis. ──
	var fx := 0.0
	var fz := 8.4
	_cyl(root, 1.5, 1.6, 0.4, marble, Vector3(fx, 0.26, fz), Vector3.ZERO, 24)        # basin wall
	_cyl(root, 1.35, 1.35, 0.12, _water(WATER, 0.6), Vector3(fx, 0.42, fz), Vector3.ZERO, 24) # water
	_torus(root, 1.45, 1.62, gold, Vector3(fx, 0.46, fz), Vector3(PI * 0.5, 0, 0), 24)  # gilt rim
	_cyl(root, 0.22, 0.32, 0.9, marble, Vector3(fx, 0.85, fz), Vector3.ZERO, 16)      # pedestal
	_cyl(root, 0.7, 0.78, 0.18, marble, Vector3(fx, 1.3, fz), Vector3.ZERO, 20)       # upper bowl
	_cyl(root, 0.55, 0.55, 0.08, _water(WATER, 0.6), Vector3(fx, 1.4, fz), Vector3.ZERO, 20)
	_ball(root, 0.18, gold, Vector3(fx, 1.55, fz))                                    # gilt finial
	# Fountain spray (cheap CPU particles).
	var spray := CPUParticles3D.new()
	spray.position = Vector3(fx, 1.6, fz)
	spray.amount = 22
	spray.lifetime = 1.4
	spray.preprocess = 1.0
	spray.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	spray.emission_sphere_radius = 0.06
	spray.direction = Vector3(0, 1, 0)
	spray.spread = 22.0
	spray.gravity = Vector3(0, -3.6, 0)
	spray.initial_velocity_min = 1.8
	spray.initial_velocity_max = 2.6
	spray.scale_amount_min = 0.4
	spray.scale_amount_max = 0.9
	var drop := SphereMesh.new()
	drop.radius = 0.06
	drop.height = 0.12
	drop.radial_segments = 6
	drop.rings = 3
	var dm := StandardMaterial3D.new()
	dm.albedo_color = Color(WATER.r, WATER.g, WATER.b, 0.7)
	dm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	dm.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	drop.material = dm
	spray.mesh = drop
	spray.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	root.add_child(spray)
	_light(root, WATER, 0.7, 3.5, Vector3(fx, 0.7, fz))

	# ── A pair of guardian STATUES on pedestals flanking the fountain. ──
	for sx: float in [-1.0, 1.0]:
		var stx := sx * 3.2
		var stz := 8.0
		# Pedestal with gilt cap.
		_box(root, Vector3(0.9, 0.9, 0.9), _gloss(STONE_LT, 0.2), Vector3(stx, 0.55, stz))
		_box(root, Vector3(1.0, 0.1, 1.0), gold, Vector3(stx, 1.05, stz))
		# A simple sculpted figure (robed sentinel) in pale marble.
		_cyl(root, 0.18, 0.3, 1.1, marble, Vector3(stx, 1.65, stz), Vector3.ZERO, 14)   # robed body
		_ball(root, 0.22, marble, Vector3(stx, 2.32, stz))                              # head
		_box(root, Vector3(0.5, 0.1, 0.2), marble, Vector3(stx, 1.9, stz + 0.15), Vector3(0, 0, sx * 0.4)) # arm
		_ball(root, 0.1, gold, Vector3(stx + sx * 0.28, 1.78, stz + 0.2))               # held orb (gilt)
		_light(root, GLOW_WARM, 0.5, 3.0, Vector3(stx, 2.0, stz))

	# ── A formal garden gate + low fence at the front boundary. ──
	var gz := 10.4
	for i: int in range(-4, 5):
		if absi(i) <= 1:
			continue
		_box(root, Vector3(0.12, 0.9, 0.12), oak, Vector3(float(i) * 0.9, 0.45, gz))
		_ball(root, 0.06, brass, Vector3(float(i) * 0.9, 0.95, gz))
	_box(root, Vector3(7.4, 0.1, 0.1), oak, Vector3(0, 0.7, gz))
	_box(root, Vector3(7.4, 0.1, 0.1), oak, Vector3(0, 0.3, gz))
	# Gate posts with brass lamps.
	for sx: float in [-1.0, 1.0]:
		_box(root, Vector3(0.26, 1.4, 0.26), oak, Vector3(sx * 1.1, 0.7, gz))
		_box(root, Vector3(0.3, 0.3, 0.3), brass, Vector3(sx * 1.1, 1.5, gz))
		_box(root, Vector3(0.2, 0.22, 0.2), _glow(GLOW_WARM, 1.6), Vector3(sx * 1.1, 1.5, gz))
		_ball(root, 0.1, gold, Vector3(sx * 1.1, 1.72, gz))
		_light(root, GLOW_WARM, 0.7, 3.0, Vector3(sx * 1.1, 1.5, gz))

	# ── Garden lanterns along the path (taller, brass, on stone bases). ──
	for sx: float in [-1.0, 1.0]:
		var lz := 6.6
		_cyl(root, 0.18, 0.24, 0.3, _gloss(STONE, 0.18), Vector3(sx * 1.7, 0.2, lz), Vector3.ZERO, 10)
		_cyl(root, 0.08, 0.1, 1.0, oak, Vector3(sx * 1.7, 0.85, lz), Vector3.ZERO, 8)
		_box(root, Vector3(0.26, 0.34, 0.26), brass, Vector3(sx * 1.7, 1.5, lz))
		_box(root, Vector3(0.18, 0.24, 0.18), _glow(GLOW_WARM, 1.6), Vector3(sx * 1.7, 1.5, lz))
		_ball(root, 0.07, gold, Vector3(sx * 1.7, 1.72, lz))
		_light(root, GLOW_WARM, 0.9, 3.8, Vector3(sx * 1.7, 1.5, lz))

	# A storybook flower patch + a tiny mushroom by the door.
	for i: int in range(7):
		var fang := float(i) * 0.9
		var pfx := 3.6 + cos(fang) * 1.4
		var pfz := 5.6 + sin(fang) * 1.2
		var pc: Color = [PETAL_RED, PETAL_YEL, PETAL_PNK, PETAL_PUR][i % 4]
		_cyl(root, 0.02, 0.03, 0.4, hedge_dk, Vector3(pfx, 0.2, pfz))
		_ball(root, 0.14, _toon(pc, 0.35), Vector3(pfx, 0.42, pfz), Vector3(1, 0.7, 1))
		_ball(root, 0.05, _glow(PETAL_YEL, 0.5), Vector3(pfx, 0.46, pfz))
	# Toadstool cluster.
	for m: int in range(3):
		var mx := -3.6 + float(m) * 0.3
		_cyl(root, 0.06, 0.08, 0.25, _toon(STUCCO, 0.2), Vector3(mx, 0.18, 5.3))
		_ball(root, 0.16, _toon(PETAL_RED, 0.35), Vector3(mx, 0.34, 5.3), Vector3(1.1, 0.6, 1.1))
		_ball(root, 0.03, _toon(STUCCO, 0.3), Vector3(mx + 0.06, 0.4, 5.36))

	# Contact-shadow ground disc to ground the whole manor.
	var sh := StandardMaterial3D.new()
	sh.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	sh.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	sh.albedo_color = Color(0, 0, 0, 0.14)
	_cyl(root, 7.6, 7.6, 0.02, sh, Vector3(0, 0.02, 1.4), Vector3.ZERO, 30)


# ───────────────────────────── lighting ─────────────────────────────────────
static func _build_lighting(root: Node3D) -> void:
	# Warm fill inside the room so the open interior reads cosy.
	_light(root, GLOW_WARM, 1.4, 7.0, Vector3(0, 2.2, 0))
	_light(root, Color(1.0, 0.78, 0.5), 1.0, 6.0, Vector3(0, 3.4, -1.5))
	# A soft cool fill out front to lift the facade at dusk.
	_light(root, Color(0.7, 0.8, 1.0), 0.5, 6.0, Vector3(0, 2.5, 4.0))
	# A gentle gold uplight on the columns + balcony for the high-end glow.
	_light(root, GLOW_GOLD, 0.6, 5.0, Vector3(0, 0.8, 3.6))


# ═══════════════════════════════ META ═══════════════════════════════════════

static func meta() -> Dictionary:
	return {
		"id": "cozy_cottage",
		"name": "Thistledown Manor Cottage",
		"tier": "Cottage",
		"rarity": "Uncommon",
		"description": "An heirloom storybook cottage elevated to a showpiece: oak Tudor framing meets brass-capped stone columns, a steep slate roof crowned with gilt finials and twin glowing dormers, a wrought-brass loft balcony, and a gilded round-oak door. Inside, a grand curved stair sweeps up to a sleeping loft beneath a brass chandelier, while a carved stone hearth's chimney trails a curl of smoke. Out front: formal box-hedge parterres, guardian statues, and a tiered gilt fountain.",
		"footprint": [9, 8],
		"floors": 1,
		"attributes": [
			["Style", "Storybook Tudor Manor Cottage"],
			["Material", "Whitewash Stucco, Oak, Slate, Brass & Marble"],
			["Feature", "Grand Stair, Chandelier, Carved Hearth & Loft Balcony"],
			["Grounds", "Tiered Fountain, Guardian Statues & Parterre Garden"],
			["Floors", "1 + Loft"],
			["Vibe", "Cosy, Heirloom, Unmistakably High-End"]
		]
	}
