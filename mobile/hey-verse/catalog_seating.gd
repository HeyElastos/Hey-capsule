class_name VerseCatalogSeating
extends RefCounted
## Hey Verse — PREMIUM SEATING catalog (showroom set, 10 sellable items).
##
## These are mint-ready NFTs sold to users, so each piece is a RICH composite of
## many primitives (~24-60 parts) with a strong silhouette, careful proportions,
## and a cohesive premium palette. NO single-box laziness.
##
## Every item is a static `build_<id>() -> Node3D` returning ONE self-contained
## Node3D, built at the ORIGIN and resting on the floor plane y=0. Seat height is
## ~0.45 to suit the ~1.4-unit chibi-robot avatar (see avatar.gd).
##
## MATERIALS make these premium:
##  - matte toon surfaces (wood, fabric, ceramic) ride the shared cel shader
##    (toon.gdshader) with an inverted-hull outline (outline.gdshader) so they
##    read as "designed" — same trick the avatar uses.
##  - real metals (gold / brass / chrome) + glass use StandardMaterial3D with
##    proper metallic + roughness, so higher rarities glint.
##  - glow (gems, RGB, fireflies-feel) is unshaded emissive StandardMaterial3D.
##
## RARITY is readable at a glance: Common = humble materials; Legendary = gold
## trim, gemstones, emission, the works. The metadata manifest (rarity / name /
## description / attributes) lives with the build loop that consumes this file.
##
## Standalone: re-declares its own tiny material + primitive helpers, so it
## parses + runs with NO dependency on home.gd / avatar.gd internals.

const TOON_SHADER := preload("res://toon.gdshader")
const OUTLINE_SHADER := preload("res://outline.gdshader")

# One shared outline pass (same trick avatar.gd uses) — cheap + consistent.
static var _outline_mat: ShaderMaterial

# Typed mirror-pair, so `for sx in SIDES` gives a `float` (not Variant) and
# derived `var lx := sx * ...` infers cleanly under strict GDScript.
const SIDES: Array[float] = [-1.0, 1.0]


# ───────────────────────────── material helpers ────────────────────────────

## The cel material every matte surface uses (toon ramp + inverted-hull outline).
static func toon_mat(c: Color, rim := 0.32, outline := true, spec := 0.0) -> ShaderMaterial:
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


## Soft cloth look (cushions, sofa pads, beanbags) — low rim, gentle.
static func cloth_mat(c: Color) -> ShaderMaterial:
	return toon_mat(c, 0.20, true, 0.0)


## A real metal — gold / brass / chrome. PBR so it actually glints; the outline
## still wraps it so it stays in the toon family. `rough` low = mirror-bright.
static func metal_mat(c: Color, rough := 0.28, metallic := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.metallic_specular = 0.75
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	if _outline_mat == null:
		_outline_mat = ShaderMaterial.new()
		_outline_mat.shader = OUTLINE_SHADER
	m.next_pass = _outline_mat
	return m


## Glossy ceramic / lacquer — smooth dielectric with a hot highlight.
static func gloss_mat(c: Color, rough := 0.18) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.0
	m.roughness = rough
	m.metallic_specular = 0.85
	if _outline_mat == null:
		_outline_mat = ShaderMaterial.new()
		_outline_mat.shader = OUTLINE_SHADER
	m.next_pass = _outline_mat
	return m


## Translucent glass / gem shell (no outline — it would muddy the glass).
static func glass_mat(c: Color, alpha := 0.45) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.metallic = 0.1
	m.roughness = 0.05
	m.metallic_specular = 0.9
	return m


## A faceted gemstone material — translucent + a glowing inner core energy so
## set stones (throne / crown jewels) actually read as precious.
static func gem_mat(c: Color, alpha := 0.62) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.metallic = 0.2
	m.roughness = 0.04
	m.metallic_specular = 1.0
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = 0.7
	return m


## Unshaded glowing material — gems, RGB strips, neon, glow pucks.
static func glow_mat(c: Color, energy := 1.4) -> StandardMaterial3D:
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


static func _ball(parent: Node3D, r: float, mat: Material, pos: Vector3, s := Vector3.ONE, seg := 20, rings := 10) -> MeshInstance3D:
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


static func _capsule(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var cm := CapsuleMesh.new()
	cm.radius = r
	cm.height = h
	cm.radial_segments = 16
	cm.rings = 6
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 12) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 24
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


## A faceted gemstone: an octahedron-ish double cone, set into gold. The hero
## detail of high-rarity pieces. `r` = girdle radius, `h` = total height.
static func _gem(parent: Node3D, r: float, h: float, color: Color, pos: Vector3, rot := Vector3.ZERO, glow := true) -> Node3D:
	var n := Node3D.new()
	n.position = pos
	n.rotation = rot
	parent.add_child(n)
	var mat := gem_mat(color)
	# crown (table up) + pavilion (point down) — a cut brilliant silhouette
	_cyl(n, 0.0, r, h * 0.5, mat, Vector3(0, h * 0.25, 0), Vector3.ZERO, 8)
	_cyl(n, r, 0.0, h * 0.5, mat, Vector3(0, -h * 0.25, 0), Vector3.ZERO, 8)
	if glow:
		_ball(n, r * 0.35, glow_mat(color.lightened(0.3), 2.2), Vector3.ZERO, Vector3.ONE, 8, 4)
	return n


## A faint round contact shadow blob on the floor — grounds the piece.
static func _contact(parent: Node3D, r: float, pos := Vector3.ZERO) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = r
	disc.bottom_radius = r
	disc.height = 0.01
	disc.radial_segments = 24
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = Color(0, 0, 0, 0.12)
	var mi := MeshInstance3D.new()
	mi.mesh = disc
	mi.material_override = m
	mi.position = pos + Vector3(0, 0.011, 0)
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mi)


## A plush "pillow": a squashed sphere = a soft-stuffed cushion silhouette.
static func _pillow(parent: Node3D, size: Vector3, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> Node3D:
	var n := Node3D.new()
	n.position = pos
	n.rotation = rot
	parent.add_child(n)
	_ball(n, 0.5, mat, Vector3.ZERO, Vector3(size.x, size.y, size.z))
	return n


## A turned-wood leg: a stacked column of beads + a taper + a foot, the classic
## lathe-turned furniture silhouette. Built downward from `top`.
static func _turned_leg(parent: Node3D, top: Vector3, h: float, r: float, wood: Material, wood_d: Material, foot: Material) -> void:
	var n := Node3D.new()
	n.position = top
	parent.add_child(n)
	# upper square block (where it meets the seat rail)
	_box(n, Vector3(r * 2.4, r * 1.4, r * 2.4), wood_d, Vector3(0, -r * 0.7, 0))
	# tapered shaft
	_cyl(n, r * 0.55, r * 0.95, h - r * 2.4, wood, Vector3(0, -h * 0.5, 0), Vector3.ZERO, 10)
	# turned beads
	_ball(n, r * 1.05, wood_d, Vector3(0, -h * 0.32, 0), Vector3(1, 0.55, 1), 12, 6)
	_ball(n, r * 0.9, wood_d, Vector3(0, -h * 0.62, 0), Vector3(1, 0.5, 1), 12, 6)
	# foot pad
	_cyl(n, r * 0.9, r * 0.7, r * 1.2, foot, Vector3(0, -h + r * 0.6, 0), Vector3.ZERO, 10)


## A button-tufted panel: a grid of pressed-in dimple buttons over a fabric
## slab. The signature of velvet / chesterfield luxury. Drawn on the +Z face.
static func _tuft_panel(parent: Node3D, w: float, h: float, mat: Material, btn_mat: Material, pos: Vector3, rot := Vector3.ZERO, cols := 3, rows := 2, face_z := 0.06) -> void:
	var panel := Node3D.new()
	panel.position = pos
	panel.rotation = rot
	parent.add_child(panel)
	# puffed quilted squares
	var cw := w / float(cols)
	var ch := h / float(rows)
	for r in rows:
		for c in cols:
			var px := -w * 0.5 + cw * (float(c) + 0.5)
			var py := -h * 0.5 + ch * (float(r) + 0.5)
			_ball(panel, 0.5, mat, Vector3(px, py, face_z * 0.4),
				Vector3(cw * 0.92, ch * 0.92, face_z * 1.2), 12, 6)
	# pressed buttons at the seams (diamond tuft pattern)
	for r in rows + 1:
		for c in cols + 1:
			var bx := -w * 0.5 + cw * float(c)
			var by := -h * 0.5 + ch * float(r)
			_ball(panel, 0.022, btn_mat, Vector3(bx, by, face_z * 0.7), Vector3(1, 1, 0.5), 8, 4)


## A row of brass nailhead studs along a line (the leather-furniture tell).
static func _stud_row(parent: Node3D, from: Vector3, to: Vector3, count: int, mat: Material, r := 0.014) -> void:
	for k in count:
		var t := 0.0 if count <= 1 else float(k) / float(count - 1)
		var p := from.lerp(to, t)
		_ball(parent, r, mat, p, Vector3(1, 1, 0.6), 8, 4)


# ════════════════════════════════════════════════════════════════════ ITEMS


## 1 · VELVET ARMCHAIR — a plump button-tufted single-seater on tapered wood
##     legs with brass caps, nailhead trim and gentle wing arms. Cozy,
##     characterful, gently premium.                                  [Uncommon]
static func build_velvet_armchair() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.56)
	var velvet := cloth_mat(Color(0.36, 0.46, 0.86))       # rich periwinkle velvet
	var velvet_d := cloth_mat(Color(0.30, 0.39, 0.76))
	var velvet_l := cloth_mat(Color(0.46, 0.56, 0.92))
	var btn := metal_mat(Color(0.90, 0.74, 0.40), 0.30)    # brass tuft buttons
	var wood := toon_mat(Color(0.42, 0.28, 0.18), 0.2, true, 0.2)
	var wood_d := toon_mat(Color(0.34, 0.22, 0.14), 0.2)
	var brass := metal_mat(Color(0.86, 0.66, 0.30), 0.32)
	var piping := toon_mat(Color(0.94, 0.86, 0.58), 0.3)   # gold piping cord
	# base block + a deep plush seat cushion
	_box(root, Vector3(0.82, 0.24, 0.76), velvet_d, Vector3(0, 0.30, 0.04))
	_pillow(root, Vector3(0.76, 0.30, 0.70), velvet, Vector3(0, 0.47, 0.06))
	# piping cord around the seat cushion edge
	_torus(root, 0.30, 0.40, piping, Vector3(0, 0.45, 0.06), Vector3(PI / 2.0, 0, 0), 14)
	# brass nailhead trim along the seat front edge
	_stud_row(root, Vector3(-0.34, 0.20, 0.44), Vector3(0.34, 0.20, 0.44), 9, brass)
	# tall tufted back with a soft top roll + gentle wings
	_box(root, Vector3(0.80, 0.66, 0.16), velvet_d, Vector3(0, 0.72, -0.30))
	_tuft_panel(root, 0.62, 0.52, velvet, btn, Vector3(0, 0.74, -0.21), Vector3.ZERO, 3, 2, 0.07)
	_capsule(root, 0.13, 0.64, velvet_l, Vector3(0, 1.02, -0.27), Vector3(0, 0, PI / 2.0))
	# wings that curl forward at the top of the back
	for sx in SIDES:
		_capsule(root, 0.10, 0.40, velvet_d, Vector3(sx * 0.37, 0.92, -0.18), Vector3(0.5, sx * 0.4, 0))
	# rolled arms with piping + scroll caps
	for sx in SIDES:
		_box(root, Vector3(0.17, 0.32, 0.72), velvet_d, Vector3(sx * 0.40, 0.50, 0.04))
		_capsule(root, 0.115, 0.60, velvet, Vector3(sx * 0.40, 0.66, 0.06), Vector3(PI / 2.0, 0, 0))
		_ball(root, 0.12, velvet_l, Vector3(sx * 0.40, 0.66, 0.40), Vector3(1, 1, 1), 14, 8)  # arm cap
		_torus(root, 0.05, 0.11, piping, Vector3(sx * 0.40, 0.66, 0.40), Vector3.ZERO, 12)    # scroll trim
	# tapered turned-wood legs with brass feet
	for sx in SIDES:
		for sz in SIDES:
			var lx := sx * 0.34
			var lz := 0.04 + sz * 0.30
			_cyl(root, 0.035, 0.06, 0.20, wood, Vector3(lx, 0.11, lz), Vector3.ZERO, 10)
			_ball(root, 0.05, wood_d, Vector3(lx, 0.16, lz), Vector3(1, 0.6, 1), 10, 5)  # knee bead
			_cyl(root, 0.05, 0.045, 0.04, brass, Vector3(lx, 0.02, lz), Vector3.ZERO, 10)
	return root


## 2 · ROYAL THRONE — a regal high-back on a stepped dais: deep velvet, a gold
##     crown crest, faceted gemstone finials, lion-paw feet, a velvet drape
##     and a glowing center jewel. Maximum opulence.                 [Legendary]
static func build_royal_throne() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.74)
	var velvet := cloth_mat(Color(0.58, 0.10, 0.16))       # royal crimson
	var velvet_d := cloth_mat(Color(0.46, 0.07, 0.12))
	var velvet_l := cloth_mat(Color(0.70, 0.16, 0.22))
	var gold := metal_mat(Color(1.0, 0.80, 0.30), 0.20)
	var gold_d := metal_mat(Color(0.82, 0.62, 0.20), 0.30)
	var btn := metal_mat(Color(0.95, 0.78, 0.34), 0.25)
	# stepped marble-ish dais under the throne (the legendary footprint)
	_box(root, Vector3(1.10, 0.08, 1.04), gold_d, Vector3(0, 0.04, 0.02))
	_box(root, Vector3(0.92, 0.08, 0.88), gold, Vector3(0, 0.12, 0.02))
	_stud_row(root, Vector3(-0.50, 0.04, 0.52), Vector3(0.50, 0.04, 0.52), 9, glow_mat(Color(1.0, 0.85, 0.4), 1.2))
	# plinth + seat + plush cushion with gold piping
	_box(root, Vector3(0.74, 0.14, 0.70), gold_d, Vector3(0, 0.24, 0))
	_box(root, Vector3(0.66, 0.26, 0.62), velvet_d, Vector3(0, 0.42, 0))
	_pillow(root, Vector3(0.60, 0.26, 0.56), velvet, Vector3(0, 0.59, 0.02))
	_torus(root, 0.26, 0.34, gold, Vector3(0, 0.57, 0.02), Vector3(PI / 2.0, 0, 0), 14)
	# tall tufted back + a velvet drape framing it
	_box(root, Vector3(0.66, 1.06, 0.14), velvet, Vector3(0, 1.06, -0.24))
	_tuft_panel(root, 0.50, 0.80, velvet, btn, Vector3(0, 1.06, -0.16), Vector3.ZERO, 3, 3, 0.06)
	for sx in SIDES:
		_capsule(root, 0.07, 0.94, velvet_d, Vector3(sx * 0.30, 1.06, -0.28), Vector3(0, 0, sx * 0.06))
		_ball(root, 0.08, velvet_l, Vector3(sx * 0.30, 0.60, -0.26), Vector3(1, 1.2, 1), 12, 6)  # drape tassel
	# gold side rails + faceted gemstone finials
	for sx in SIDES:
		_cyl(root, 0.045, 0.05, 1.18, gold, Vector3(sx * 0.33, 1.18, -0.24), Vector3.ZERO, 12)
		_ball(root, 0.075, gold, Vector3(sx * 0.33, 1.80, -0.24), Vector3.ONE, 16, 8)
		_gem(root, 0.05, 0.16, Color(0.45, 0.85, 1.0), Vector3(sx * 0.33, 1.92, -0.20))
	# crown crest: an arched gold band of spikes with set gems across the top
	_torus(root, 0.20, 0.34, gold, Vector3(0, 1.72, -0.22), Vector3(0, 0, 0), 16)
	for k in 5:
		var t := (float(k) - 2.0) / 2.0
		_prism(root, Vector3(0.10, 0.18 + abs(t) * -0.04, 0.08), gold, Vector3(t * 0.22, 1.86, -0.22))
		_gem(root, 0.03, 0.10, Color(1.0, 0.35, 0.5), Vector3(t * 0.22, 1.92, -0.18))
	# central glowing crown jewel in a gold collet
	_gem(root, 0.09, 0.26, Color(0.55, 0.95, 1.0), Vector3(0, 1.70, -0.10))
	_torus(root, 0.09, 0.13, gold, Vector3(0, 1.62, -0.12), Vector3(PI / 2.0, 0, 0), 12)
	# gold armrests on turned columns with gem accents
	for sx in SIDES:
		_box(root, Vector3(0.10, 0.09, 0.52), gold, Vector3(sx * 0.34, 0.78, 0.02))
		_ball(root, 0.07, gold, Vector3(sx * 0.34, 0.78, 0.30), Vector3.ONE, 14, 8)  # scroll cap
		_gem(root, 0.028, 0.08, Color(0.9, 0.3, 0.45), Vector3(sx * 0.34, 0.78, 0.30))
		_cyl(root, 0.05, 0.055, 0.34, velvet_d, Vector3(sx * 0.34, 0.56, 0.02), Vector3.ZERO, 10)
	# lion-paw gold feet
	for sx in SIDES:
		for sz in SIDES:
			var fx := sx * 0.28
			var fz := sz * 0.26
			_ball(root, 0.075, gold, Vector3(fx, 0.19, fz), Vector3(1.0, 0.7, 1.3), 14, 8)
			for toe in [-1.0, 0.0, 1.0]:
				_ball(root, 0.026, gold, Vector3(fx + toe * 0.045, 0.17, fz + 0.07), Vector3(1, 0.7, 1), 8, 4)
	return root


## 3 · GAMING CHAIR — racer bucket seat, glossy carbon shell, neon trim, a
##     glowing winged logo, headrest + lumbar pillows, retractable footrest,
##     and a 5-star base with full RGB underglow.                        [Rare]
static func build_gaming_chair() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.46)
	var shell := gloss_mat(Color(0.08, 0.09, 0.13), 0.22)        # glossy carbon black
	var trim := toon_mat(Color(0.10, 0.85, 0.95), 0.45, true, 0.5)   # cyan neon trim
	var trim2 := toon_mat(Color(0.95, 0.18, 0.45), 0.45, true, 0.5)  # magenta accent
	var pad := cloth_mat(Color(0.14, 0.15, 0.20))
	var chrome := metal_mat(Color(0.78, 0.80, 0.86), 0.16)
	var cyan := glow_mat(Color(0.25, 0.95, 1.0), 2.2)
	var mag := glow_mat(Color(1.0, 0.25, 0.6), 2.2)
	# seat pan with raised side bolsters + glowing seam
	_box(root, Vector3(0.48, 0.11, 0.46), pad, Vector3(0, 0.45, 0))
	_box(root, Vector3(0.40, 0.015, 0.40), cyan, Vector3(0, 0.51, 0.0))   # seam glow strip
	for sx in SIDES:
		_capsule(root, 0.075, 0.42, shell, Vector3(sx * 0.22, 0.50, 0.0), Vector3(PI / 2.0, 0, 0))
		_capsule(root, 0.03, 0.40, trim, Vector3(sx * 0.235, 0.52, 0.02), Vector3(PI / 2.0, 0, 0))
	# tall racing backrest with wing bolsters + glowing seam stripes
	_box(root, Vector3(0.46, 0.82, 0.12), shell, Vector3(0, 0.90, -0.18))
	_box(root, Vector3(0.30, 0.78, 0.06), pad, Vector3(0, 0.90, -0.11))
	for sx in SIDES:
		_capsule(root, 0.06, 0.66, shell, Vector3(sx * 0.21, 0.96, -0.14), Vector3(0, 0, 0.07 * sx))
		_box(root, Vector3(0.025, 0.62, 0.04), trim, Vector3(sx * 0.15, 0.92, -0.07))
	# glowing winged logo emblem on the upper back (the hero brand tell)
	for sx in SIDES:
		_prism(root, Vector3(0.16, 0.10, 0.03), mag, Vector3(sx * 0.07, 1.16, -0.10), Vector3(0, 0, sx * -0.5))
	_ball(root, 0.04, cyan, Vector3(0, 1.18, -0.09), Vector3(1, 1, 0.5), 10, 5)
	# headrest + lumbar pillows on straps
	_pillow(root, Vector3(0.32, 0.18, 0.14), pad, Vector3(0, 1.34, -0.12))
	_box(root, Vector3(0.24, 0.04, 0.10), trim, Vector3(0, 1.34, -0.05))
	_pillow(root, Vector3(0.30, 0.16, 0.12), pad, Vector3(0, 0.72, -0.08))
	# armrests (4D-look) with chrome posts
	for sx in SIDES:
		_cyl(root, 0.025, 0.025, 0.18, chrome, Vector3(sx * 0.28, 0.58, 0.02), Vector3.ZERO, 8)
		_box(root, Vector3(0.10, 0.05, 0.26), shell, Vector3(sx * 0.28, 0.68, 0.02))
		_box(root, Vector3(0.08, 0.02, 0.22), trim, Vector3(sx * 0.28, 0.71, 0.02))
	# retractable footrest jutting out the front
	_box(root, Vector3(0.40, 0.04, 0.26), shell, Vector3(0, 0.40, 0.40))
	_box(root, Vector3(0.34, 0.02, 0.04), cyan, Vector3(0, 0.43, 0.50))
	# chrome gas-lift column + tilt mechanism
	_box(root, Vector3(0.18, 0.06, 0.22), shell, Vector3(0, 0.38, 0))
	_cyl(root, 0.05, 0.055, 0.28, chrome, Vector3(0, 0.24, 0), Vector3.ZERO, 12)
	# 5-star caster base with RGB glow pucks + wheels
	var rgb := [Color(1.0, 0.2, 0.35), Color(0.3, 0.65, 1.0), Color(0.4, 1.0, 0.55),
		Color(1.0, 0.82, 0.2), Color(0.85, 0.3, 1.0)]
	for k in 5:
		var ang := TAU * float(k) / 5.0
		var ex := cos(ang)
		var ez := sin(ang)
		_box(root, Vector3(0.34, 0.05, 0.08), shell, Vector3(ex * 0.16, 0.10, ez * 0.16), Vector3(0, -ang, 0))
		_box(root, Vector3(0.30, 0.02, 0.03), glow_mat(rgb[k], 2.2), Vector3(ex * 0.18, 0.075, ez * 0.18), Vector3(0, -ang, 0))
		_ball(root, 0.055, shell, Vector3(ex * 0.30, 0.055, ez * 0.30), Vector3.ONE, 12, 6)
		_ball(root, 0.03, glow_mat(rgb[k], 2.4), Vector3(ex * 0.30, 0.075, ez * 0.30))
	return root


## 4 · BEANBAG — a slouchy two-tone bean bag with a stitched seam, drawcord,
##     a leather brand patch and a sunken seat dimple. The comfiest, most
##     casual seat.                                                      [Common]
static func build_beanbag() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.56)
	var top := cloth_mat(Color(0.98, 0.78, 0.30))      # mustard top
	var bot := cloth_mat(Color(0.92, 0.50, 0.26))      # pumpkin base
	var seam := toon_mat(Color(0.84, 0.58, 0.22), 0.18)
	var patch := toon_mat(Color(0.56, 0.38, 0.22), 0.2, true, 0.15)   # leather brand tag
	# big squashed sphere base
	_ball(root, 0.5, bot, Vector3(0, 0.30, 0), Vector3(1.04, 0.80, 1.04), 24, 12)
	# a saggy top lobe (where you sink in)
	_ball(root, 0.5, top, Vector3(0, 0.50, 0.04), Vector3(0.80, 0.52, 0.84), 24, 12)
	# stitched seam ring around the middle
	_torus(root, 0.46, 0.51, seam, Vector3(0, 0.34, 0), Vector3(PI / 2.0, 0, 0), 14)
	# six panel seams running over the top (gores)
	for k in 6:
		var ang := TAU * float(k) / 6.0
		_capsule(root, 0.012, 0.34, seam, Vector3(cos(ang) * 0.26, 0.52, 0.04 + sin(ang) * 0.26),
			Vector3(PI / 2.0, ang, 0))
	# sunken seat dimple
	_cyl(root, 0.22, 0.26, 0.05, cloth_mat(Color(0.86, 0.62, 0.24)), Vector3(0, 0.55, 0.05), Vector3.ZERO, 18)
	# little leather brand patch on the front
	_box(root, Vector3(0.12, 0.07, 0.02), patch, Vector3(0.18, 0.34, 0.44), Vector3(0, 0, -0.2))
	_box(root, Vector3(0.08, 0.018, 0.01), seam, Vector3(0.18, 0.34, 0.45), Vector3(0, 0, -0.2))
	# top knot + drawcord
	_ball(root, 0.06, top, Vector3(0, 0.66, 0.06))
	for sx in SIDES:
		_capsule(root, 0.012, 0.10, seam, Vector3(sx * 0.05, 0.62, 0.18), Vector3(0.5, 0, sx * 0.3))
		_ball(root, 0.025, seam, Vector3(sx * 0.07, 0.56, 0.22))
	return root


## 5 · CARVED DINING CHAIR — a heritage chair: turned spindle back, a carved
##     crest with a center medallion, ribbon splat, a piped damask cushion,
##     fluted legs with stretchers.                                    [Uncommon]
static func build_carved_dining_chair() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.34)
	var wood := toon_mat(Color(0.62, 0.40, 0.24), 0.22, true, 0.2)   # warm walnut
	var wood_d := toon_mat(Color(0.50, 0.31, 0.18), 0.2)
	var cushion := cloth_mat(Color(0.85, 0.32, 0.34))               # red damask pad
	var piping := toon_mat(Color(0.96, 0.84, 0.50), 0.3)
	var gilt := metal_mat(Color(0.88, 0.70, 0.34), 0.30)            # gilt medallion
	# seat frame + piped cushion
	_box(root, Vector3(0.46, 0.06, 0.44), wood, Vector3(0, 0.45, 0))
	_pillow(root, Vector3(0.40, 0.10, 0.38), cushion, Vector3(0, 0.50, 0))
	_torus(root, 0.18, 0.26, piping, Vector3(0, 0.50, 0), Vector3(PI / 2.0, 0, 0), 12)
	# back posts (turned)
	for sx in SIDES:
		_cyl(root, 0.03, 0.035, 0.58, wood, Vector3(sx * 0.19, 0.76, -0.19), Vector3.ZERO, 10)
		_ball(root, 0.045, wood_d, Vector3(sx * 0.19, 1.06, -0.19))   # turned finial
		_ball(root, 0.04, wood_d, Vector3(sx * 0.19, 0.62, -0.19), Vector3(1, 0.6, 1))  # turned ring
	# carved crest rail with a gilt center medallion
	_capsule(root, 0.045, 0.42, wood, Vector3(0, 1.02, -0.18), Vector3(0, 0, PI / 2.0))
	_prism(root, Vector3(0.16, 0.10, 0.05), wood_d, Vector3(0, 1.07, -0.17))
	_torus(root, 0.03, 0.06, gilt, Vector3(0, 1.02, -0.13), Vector3(PI / 2.0, 0, 0), 10)
	_ball(root, 0.03, gilt, Vector3(0, 1.02, -0.13), Vector3(1, 1, 0.6))
	# a carved ribbon-back splat down the center
	_box(root, Vector3(0.10, 0.40, 0.03), wood, Vector3(0, 0.82, -0.205))
	for j in 3:
		_torus(root, 0.03, 0.055, wood_d, Vector3(0, 0.70 + j * 0.12, -0.20), Vector3(0, PI / 2.0, 0), 8)
	# two turned spindles flanking the splat
	for sx in SIDES:
		_cyl(root, 0.018, 0.022, 0.40, wood, Vector3(sx * 0.11, 0.80, -0.20), Vector3.ZERO, 8)
		_ball(root, 0.03, wood_d, Vector3(sx * 0.11, 0.80, -0.20), Vector3(1, 0.5, 1))   # spindle bead
	# lower back rail
	_capsule(root, 0.03, 0.40, wood, Vector3(0, 0.62, -0.20), Vector3(0, 0, PI / 2.0))
	# four fluted legs (front cabriole, back raked)
	for sx in SIDES:
		for sz in SIDES:
			_cyl(root, 0.028, 0.045, 0.45, wood, Vector3(sx * 0.18, 0.225, sz * 0.18), Vector3.ZERO, 8)
			_ball(root, 0.035, wood_d, Vector3(sx * 0.18, 0.30, sz * 0.18), Vector3(1, 0.5, 1))  # knee bead
			_cyl(root, 0.03, 0.02, 0.03, wood_d, Vector3(sx * 0.18, 0.02, sz * 0.18), Vector3.ZERO, 8)  # foot
	# stretchers between the legs
	_capsule(root, 0.018, 0.34, wood_d, Vector3(0, 0.14, 0.18), Vector3(0, 0, PI / 2.0))
	_capsule(root, 0.018, 0.34, wood_d, Vector3(0, 0.14, -0.18), Vector3(0, 0, PI / 2.0))
	_capsule(root, 0.018, 0.30, wood_d, Vector3(0.18, 0.14, 0), Vector3(PI / 2.0, 0, 0))
	return root


## 6 · MUSHROOM STOOL — a whimsical toadstool seat: glossy red cap with cream
##     spots, glowing gills underneath, a chubby stalk, a little ladybug rider
##     and a fairy-ring glow base.                                        [Rare]
static func build_mushroom_stool() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.38)
	var cap := gloss_mat(Color(0.86, 0.20, 0.24), 0.20)        # candy-red cap
	var spot := gloss_mat(Color(0.99, 0.96, 0.90), 0.25)       # cream spots
	var stalk := toon_mat(Color(0.96, 0.92, 0.82), 0.25)       # cream stalk
	var gill := toon_mat(Color(0.94, 0.80, 0.66), 0.2)         # warm gills
	var gill_glow := glow_mat(Color(1.0, 0.74, 0.5), 1.1)      # bioluminescent gills
	var moss := toon_mat(Color(0.40, 0.66, 0.34), 0.2)
	var bug := gloss_mat(Color(0.85, 0.16, 0.18), 0.2)
	var bug_dot := toon_mat(Color(0.05, 0.05, 0.07), 0.1)
	# chubby stalk (waisted)
	_cyl(root, 0.16, 0.20, 0.32, stalk, Vector3(0, 0.28, 0), Vector3.ZERO, 18)
	_torus(root, 0.13, 0.20, stalk, Vector3(0, 0.32, 0), Vector3(PI / 2.0, 0, 0), 14)  # skirt ring
	# domed cap = the seat (top ~0.45)
	_ball(root, 0.40, cap, Vector3(0, 0.45, 0), Vector3(1.0, 0.55, 1.0), 24, 12)
	_cyl(root, 0.40, 0.40, 0.02, cap, Vector3(0, 0.40, 0), Vector3.ZERO, 24)   # cap underside rim
	# radial gills under the cap (warm, with a soft glow undertone)
	for k in 14:
		var ang := TAU * float(k) / 14.0
		_box(root, Vector3(0.02, 0.04, 0.30), gill, Vector3(cos(ang) * 0.18, 0.41, sin(ang) * 0.18), Vector3(0, -ang, 0))
	_ball(root, 0.16, gill_glow, Vector3(0, 0.39, 0), Vector3(1, 0.3, 1), 16, 6)   # underglow disc
	# cream spots scattered on the dome
	var spots := [
		Vector3(0.0, 0.62, 0.0), Vector3(0.22, 0.53, 0.10), Vector3(-0.18, 0.54, 0.16),
		Vector3(0.10, 0.55, -0.24), Vector3(-0.24, 0.51, -0.06), Vector3(0.20, 0.50, -0.18),
	]
	var rad := [0.07, 0.055, 0.06, 0.05, 0.055, 0.045]
	for i in spots.size():
		_ball(root, rad[i], spot, spots[i], Vector3(1, 0.5, 1), 12, 6)
	# a tiny ladybug perched on the cap (the charm hero detail)
	_ball(root, 0.05, bug, Vector3(0.16, 0.56, 0.20), Vector3(1, 0.7, 1.2), 12, 6)
	_box(root, Vector3(0.005, 0.04, 0.10), bug_dot, Vector3(0.16, 0.58, 0.20))   # wing split
	_ball(root, 0.03, bug_dot, Vector3(0.16, 0.55, 0.26), Vector3(1, 0.7, 1), 8, 4)   # head
	for d in [-1.0, 1.0]:
		_ball(root, 0.012, bug_dot, Vector3(0.16 + d * 0.025, 0.59, 0.21), Vector3(1, 0.6, 1), 6, 3)  # spots
	# fairy-ring moss base with glowing dots
	_torus(root, 0.20, 0.30, moss, Vector3(0, 0.04, 0), Vector3(PI / 2.0, 0, 0), 14)
	for k in 5:
		var ang2 := TAU * float(k) / 5.0 + 0.3
		_ball(root, 0.03, glow_mat(Color(0.6, 1.0, 0.7), 1.6), Vector3(cos(ang2) * 0.25, 0.06, sin(ang2) * 0.25))
	return root


## 7 · SWING SEAT — a hanging rattan egg-chair on a stand: a woven dome shell,
##     deep cushion, chains, a tasselled throw pillow, trailing ivy and a
##     string of warm fairy lights.                                       [Epic]
static func build_swing_seat() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.7)
	var frame := metal_mat(Color(0.30, 0.33, 0.38), 0.30)       # matte gunmetal stand
	var rattan := toon_mat(Color(0.86, 0.70, 0.42), 0.28, true, 0.2)   # honey rattan
	var rattan_d := toon_mat(Color(0.74, 0.58, 0.32), 0.22)
	var cushion := cloth_mat(Color(0.46, 0.72, 0.66))           # seafoam cushion
	var pillow := cloth_mat(Color(0.96, 0.66, 0.48))            # coral throw pillow
	var chain := metal_mat(Color(0.74, 0.76, 0.80), 0.20)
	var ivy := toon_mat(Color(0.36, 0.60, 0.34), 0.2)
	var bulb := glow_mat(Color(1.0, 0.84, 0.5), 2.0)            # warm fairy lights
	# the woven egg shell — a big sphere, open at the front-top
	_ball(root, 0.46, rattan, Vector3(0, 0.78, -0.02), Vector3(1.0, 1.05, 0.95), 24, 12)
	# weave suggestion: hoops + verticals over the shell
	for i in 3:
		_torus(root, 0.40 - i * 0.05, 0.46 - i * 0.05, rattan_d, Vector3(0, 0.55 + i * 0.20, -0.02), Vector3(PI / 2.0, 0, 0), 18)
	for k in 8:
		var ang := PI * (float(k) / 7.0) - PI / 2.0
		_capsule(root, 0.012, 0.62, rattan_d, Vector3(cos(ang) * 0.44, 0.78, -0.02 + sin(ang) * 0.10),
			Vector3(0.0, 0, ang))
	# carve the seat opening with a darker recess + deep cushion
	_ball(root, 0.40, cushion, Vector3(0, 0.70, 0.16), Vector3(0.82, 0.55, 0.62), 20, 10)
	_box(root, Vector3(0.56, 0.16, 0.40), cushion, Vector3(0, 0.52, 0.14))
	_pillow(root, Vector3(0.30, 0.30, 0.14), pillow, Vector3(0, 0.78, 0.26))
	for sx in SIDES:   # pillow tassels
		_ball(root, 0.025, pillow, Vector3(sx * 0.16, 0.66, 0.30))
	# rim of the opening
	_torus(root, 0.28, 0.34, rattan, Vector3(0, 0.78, 0.22), Vector3(0.35, 0, 0), 16)
	# trailing ivy spilling over the rim (the cozy hero detail)
	for sx in SIDES:
		var bx := sx * 0.30
		_capsule(root, 0.01, 0.30, ivy, Vector3(bx, 0.86, 0.16), Vector3(0.3, 0, sx * 0.2))
		for j in 4:
			_ball(root, 0.035, ivy, Vector3(bx + sx * 0.02, 0.95 - j * 0.10, 0.18 + sin(j) * 0.03), Vector3(1.2, 0.5, 1), 8, 4)
	# the stand: a curved gooseneck arching over from a weighted base
	_cyl(root, 0.10, 0.14, 0.06, frame, Vector3(0, 0.04, -0.55), Vector3.ZERO, 18)        # base disc
	_torus(root, 0.16, 0.24, frame, Vector3(0, 0.03, -0.55), Vector3(PI / 2.0, 0, 0), 16)
	_cyl(root, 0.05, 0.06, 1.5, frame, Vector3(0, 0.78, -0.62), Vector3.ZERO, 14)         # upright
	_cyl(root, 0.05, 0.05, 0.62, frame, Vector3(0, 1.50, -0.34), Vector3(PI / 2.4, 0, 0)) # arch arm
	# chains from the arch to the shell
	for sx in SIDES:
		for j in 4:
			_torus(root, 0.014, 0.03, chain, Vector3(sx * 0.18, 1.34 - j * 0.10, -0.04),
				Vector3(0, 0, (j % 2) * PI / 2.0), 8)
	# a string of warm fairy lights draped along the arch
	for k in 6:
		var t := float(k) / 5.0
		_ball(root, 0.022, bulb, Vector3(lerpf(-0.30, 0.30, t), 1.46 - sin(t * PI) * 0.06, -0.30))
	return root


## 8 · CHESTERFIELD SOFA — the classic: deep button-tufted oxblood leather, low
##     rolled arms, brass stud trim, a folded throw blanket, two accent pillows,
##     and turned wood bun feet.                                          [Epic]
static func build_chesterfield_sofa() -> Node3D:
	var root := Node3D.new()
	_contact(root, 0.92)
	var leather := gloss_mat(Color(0.45, 0.13, 0.13), 0.30)        # oxblood leather
	var leather_d := gloss_mat(Color(0.37, 0.10, 0.10), 0.35)
	var leather_l := gloss_mat(Color(0.54, 0.18, 0.16), 0.28)
	var brass := metal_mat(Color(0.88, 0.68, 0.32), 0.30)
	var wood := toon_mat(Color(0.34, 0.22, 0.14), 0.2)
	var wood_d := toon_mat(Color(0.26, 0.16, 0.10), 0.2)
	var throw := cloth_mat(Color(0.86, 0.80, 0.66))                # cream throw blanket
	var accent := cloth_mat(Color(0.20, 0.32, 0.40))               # teal accent pillow
	var w := 1.7
	# base + deep tufted back
	_box(root, Vector3(w, 0.30, 0.80), leather_d, Vector3(0, 0.30, 0.04))
	_box(root, Vector3(w, 0.62, 0.18), leather, Vector3(0, 0.70, -0.31))
	_tuft_panel(root, w - 0.5, 0.52, leather, brass, Vector3(0, 0.72, -0.22), Vector3.ZERO, 6, 2, 0.07)
	# rolled top of the back
	_capsule(root, 0.13, w - 0.12, leather_l, Vector3(0, 1.00, -0.28), Vector3(0, 0, PI / 2.0))
	# two box seat cushions, lightly tufted
	for sx in SIDES:
		_pillow(root, Vector3(0.74, 0.26, 0.74), leather, Vector3(sx * 0.40, 0.47, 0.06))
		_ball(root, 0.022, brass, Vector3(sx * 0.40, 0.59, 0.06), Vector3(1, 1, 0.5), 8, 4)  # cushion button
	# low rolled arms (the chesterfield signature: arms level with the back)
	for sx in SIDES:
		_capsule(root, 0.17, 0.80, leather, Vector3(sx * (w / 2.0 - 0.13), 0.66, 0.02), Vector3(PI / 2.0, 0, 0))
		_ball(root, 0.17, leather_l, Vector3(sx * (w / 2.0 - 0.13), 0.66, 0.42), Vector3.ONE, 16, 8)
		# brass stud row along the arm front
		_stud_row(root, Vector3(sx * (w / 2.0 - 0.13), 0.48, 0.40), Vector3(sx * (w / 2.0 - 0.13), 0.84, 0.40), 5, brass, 0.015)
	# brass stud trim along the base front edge
	_stud_row(root, Vector3(-w / 2.0 + 0.12, 0.20, 0.44), Vector3(w / 2.0 - 0.12, 0.20, 0.44), 13, brass)
	# a folded cream throw blanket draped over one arm
	_box(root, Vector3(0.42, 0.06, 0.50), throw, Vector3(-0.42, 0.64, 0.10), Vector3(0.05, 0, 0.04))
	_box(root, Vector3(0.42, 0.05, 0.46), throw, Vector3(-0.42, 0.70, 0.06), Vector3(-0.02, 0, 0.02))
	# two teal accent pillows leaning into the corners
	for sx in SIDES:
		_pillow(root, Vector3(0.30, 0.30, 0.14), accent, Vector3(sx * 0.52, 0.66, -0.10), Vector3(0.3, 0, sx * 0.2))
		_ball(root, 0.02, brass, Vector3(sx * 0.52, 0.66, -0.03), Vector3(1, 1, 0.5), 8, 4)
	# turned wood bun feet
	for sx in SIDES:
		for sz in SIDES:
			_ball(root, 0.075, wood, Vector3(sx * (w / 2.0 - 0.16), 0.07, sz * 0.32), Vector3(1, 0.85, 1), 14, 8)
			_cyl(root, 0.05, 0.04, 0.03, wood_d, Vector3(sx * (w / 2.0 - 0.16), 0.015, sz * 0.32), Vector3.ZERO, 10)
	return root


## 9 · HAMMOCK — a striped canvas hammock slung between two A-frame wooden posts,
##     with rope fans, a fringed edge, a throw pillow and a string of warm
##     fairy lights between the apexes.                                    [Rare]
static func build_hammock() -> Node3D:
	var root := Node3D.new()
	_contact(root, 1.0)
	var wood := toon_mat(Color(0.66, 0.46, 0.28), 0.22, true, 0.2)
	var wood_d := toon_mat(Color(0.52, 0.35, 0.20), 0.2)
	var canvas := cloth_mat(Color(0.95, 0.93, 0.86))           # cream canvas
	var stripe := cloth_mat(Color(0.20, 0.62, 0.66))           # teal stripe
	var stripe2 := cloth_mat(Color(0.95, 0.56, 0.34))          # coral stripe
	var rope := toon_mat(Color(0.84, 0.74, 0.52), 0.2)
	var pillow := cloth_mat(Color(0.96, 0.80, 0.42))           # sunny pillow
	var hook_mat := metal_mat(Color(0.7, 0.72, 0.76), 0.2)
	var bulb := glow_mat(Color(1.0, 0.86, 0.52), 2.0)          # warm fairy lights
	var span := 1.9
	# two A-frame stands (splayed legs, top yoke, apex knob, hook)
	for sx in SIDES:
		var bx := sx * (span / 2.0)
		for dz in SIDES:
			# splay the legs outward in Z (rotate before placing — no get_child hacks)
			_cyl(root, 0.035, 0.05, 1.05, wood, Vector3(bx, 0.50, dz * 0.22), Vector3(dz * 0.22, 0, 0))
		# cross-tie + top yoke
		_cyl(root, 0.025, 0.025, 0.6, wood_d, Vector3(bx, 0.40, 0), Vector3(PI / 2.0, 0, 0), 8)
		_ball(root, 0.06, wood_d, Vector3(bx, 0.98, 0))   # apex knob
		# hook
		_torus(root, 0.025, 0.05, hook_mat, Vector3(bx + sx * -0.06, 0.92, 0), Vector3(0, 0, PI / 2.0), 10)
	# the slung bed — a shallow catenary U made of stacked, drooping slats
	var n := 11
	for i in n:
		var t := float(i) / float(n - 1)        # 0..1 across the span
		var x := lerpf(-span / 2.0 + 0.18, span / 2.0 - 0.18, t)
		var droop := 0.36 - cos((t - 0.5) * PI) * 0.30    # lowest in the middle (~0.50)
		var y := 0.30 + droop
		# alternate stripe colors
		var mat: Material = canvas
		if i % 3 == 1:
			mat = stripe
		elif i % 3 == 2:
			mat = stripe2
		# tilt the slats to follow the curve (set rotation directly)
		_box(root, Vector3(0.14, 0.05, 0.74), mat, Vector3(x, y, 0), Vector3(0, 0, (t - 0.5) * 0.9))
	# rope fans gathering the bed to each hook
	for sx in SIDES:
		for k in 5:
			var spread := (float(k) - 2.0) / 2.0
			_capsule(root, 0.008, 0.5, rope, Vector3(sx * (span / 2.0 - 0.34), 0.78, spread * 0.30),
				Vector3(0, 0, sx * 0.7))
	# throw pillow resting in the dip
	_pillow(root, Vector3(0.34, 0.16, 0.44), pillow, Vector3(0.0, 0.40, 0.0))
	# fringe along the long edges
	for sz in SIDES:
		for k in 9:
			var fx := lerpf(-span / 2.0 + 0.30, span / 2.0 - 0.30, float(k) / 8.0)
			_capsule(root, 0.006, 0.08, canvas, Vector3(fx, 0.30, sz * 0.36), Vector3.ZERO)
	# a string of warm fairy lights drooping between the two apexes
	for k in 7:
		var t2 := float(k) / 6.0
		var lx := lerpf(-span / 2.0 + 0.10, span / 2.0 - 0.10, t2)
		var ly := 0.98 - sin(t2 * PI) * 0.14   # catenary droop
		_ball(root, 0.02, bulb, Vector3(lx, ly, 0))
	return root


## 10 · CLOUD SOFA — a dreamy oversized modular sofa shaped from puffy cloud
##      lobes, a pastel sky gradient, a soft glow rim, a little rainbow arc and
##      drifting star sparkles. The crown jewel of comfort.          [Legendary]
static func build_cloud_sofa() -> Node3D:
	var root := Node3D.new()
	_contact(root, 1.0)
	var cloud := cloth_mat(Color(0.96, 0.97, 1.0))             # bright cloud white
	var cloud_s := cloth_mat(Color(0.86, 0.90, 0.99))          # cool shadow lobe
	var sky := cloth_mat(Color(0.74, 0.84, 0.99))              # sky-blue underside
	var blush := cloth_mat(Color(0.99, 0.88, 0.93))            # dawn-pink lobe
	var w := 1.8
	# big soft base slab (the seat platform)
	_box(root, Vector3(w, 0.30, 0.84), cloud_s, Vector3(0, 0.30, 0.04))
	# rolling cloud lobes across the FRONT base
	var front := [
		Vector3(-0.70, 0.32, 0.42), Vector3(-0.30, 0.30, 0.46), Vector3(0.12, 0.33, 0.45),
		Vector3(0.50, 0.30, 0.46), Vector3(0.80, 0.31, 0.42),
	]
	var fr := [0.30, 0.34, 0.32, 0.34, 0.28]
	for i in front.size():
		var c: Material = cloud if i % 2 == 0 else blush
		_ball(root, fr[i], c, front[i], Vector3(1.1, 0.9, 1.0), 18, 9)
	# two huge plush seat cushions (puffy lobes)
	for sx in SIDES:
		_ball(root, 0.40, cloud, Vector3(sx * 0.42, 0.52, 0.08), Vector3(1.05, 0.7, 1.0), 20, 10)
		_ball(root, 0.22, cloud, Vector3(sx * 0.42, 0.60, 0.30), Vector3(1.1, 0.6, 0.9), 16, 8)  # front roll
	# back: a row of tall cloud-puff back pillows
	var back := [-0.66, -0.22, 0.22, 0.66]
	for i in back.size():
		var c2: Material = cloud if i % 2 == 0 else cloud_s
		_ball(root, 0.34, c2, Vector3(back[i], 0.74, -0.30), Vector3(1.1, 1.2, 0.7), 18, 9)
		_ball(root, 0.20, blush if i % 2 == 0 else cloud, Vector3(back[i], 0.96, -0.28), Vector3(1.0, 0.9, 0.6), 14, 7)
	# soft cloud arms
	for sx in SIDES:
		_ball(root, 0.34, cloud, Vector3(sx * (w / 2.0 - 0.04), 0.56, 0.06), Vector3(0.9, 1.0, 1.2), 18, 9)
		_ball(root, 0.22, cloud_s, Vector3(sx * (w / 2.0 - 0.04), 0.56, 0.40), Vector3(0.8, 0.9, 0.9), 14, 7)
	# sky-tinted underside trim
	_box(root, Vector3(w - 0.1, 0.10, 0.80), sky, Vector3(0, 0.13, 0.04))
	# little floating feet = soft mist puffs
	for sx in SIDES:
		for sz in SIDES:
			_ball(root, 0.10, sky, Vector3(sx * (w / 2.0 - 0.18), 0.07, sz * 0.30), Vector3(1.4, 0.5, 1.4), 12, 6)
	# a soft glow rim along the top + tiny star sparkles (the legendary tell)
	var glow := glow_mat(Color(0.80, 0.90, 1.0), 0.9)
	_torus(root, 0.05, 0.10, glow, Vector3(0, 1.16, -0.28), Vector3(PI / 2.0, 0, 0), 16)
	# a little rainbow arc rising behind the back (pure legendary whimsy)
	var rainbow := [Color(1.0, 0.4, 0.45), Color(1.0, 0.72, 0.35), Color(1.0, 0.95, 0.5),
		Color(0.5, 0.9, 0.6), Color(0.45, 0.7, 1.0), Color(0.7, 0.5, 0.95)]
	for i in rainbow.size():
		var rr := 0.74 + i * 0.07
		_torus(root, rr - 0.035, rr, glow_mat(rainbow[i], 1.3), Vector3(0, 1.0, -0.36), Vector3.ZERO, 6)
	var stars := [
		Vector3(-0.5, 1.10, 0.1), Vector3(0.3, 1.22, -0.05), Vector3(0.7, 1.05, 0.15),
		Vector3(-0.2, 1.42, -0.1), Vector3(-0.8, 1.18, -0.05), Vector3(0.55, 1.50, -0.08),
	]
	for s in stars:
		_ball(root, 0.03, glow_mat(Color(1.0, 0.95, 0.7), 2.0), s, Vector3.ONE, 8, 4)
	return root
