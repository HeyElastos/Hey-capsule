class_name VerseCatalogHats
extends RefCounted
## Hey Verse — PREMIUM HEADWEAR catalog (mint-ready showroom set, 12 items).
##
## These are SOLD to users as NFTs, so every piece is a rich composite of many
## primitives (~16–48 parts) with a strong silhouette, cohesive premium
## materials (brushed metals, matte felt, soft fabric, glossy ceramic, glass)
## and tasteful EMISSION for anything that glows (gems, halos, flame, screens,
## fireflies). Rarity is readable AT A GLANCE: higher tiers get more gold trim,
## more gemstones, glow and ornament.
##
## The set (rarity in parentheses):
##   build_top_hat        (Uncommon)   build_party_hat      (Common)
##   build_propeller_cap  (Uncommon)   build_headphones     (Rare)
##   build_cat_ears       (Uncommon)   build_flower_crown   (Rare)
##   build_viking_helmet  (Rare)       build_wizard_hat     (Epic)
##   build_crown          (Epic)       build_halo           (Epic)
##   build_astronaut_helmet (Legendary) build_flame_crown   (Legendary)
##
## SCALE / MOUNTING: the robot stands ~1.4 units tall; its head is a rounded
## ~0.94-wide TV head and avatar.gd anchors a `_hat_root` on the crown at
## y≈1.83. Every builder returns ONE self-contained Node3D built at the ORIGIN,
## sized ~0.3 across, with its underside resting at y≈0 (brims/bands model
## around y≈0.02–0.06, matching avatar.gd's classic hats) so it drops naturally
## onto the head when mounted on `_hat_root`.
##
## Pure procedural primitives — NO external assets, NO .glb, NO preload of art —
## so the module parses and runs standalone. Materials load by RESOURCE PATH
## (not preload of a .gd) so there is no compile-time dependency on avatar.gd;
## a StandardMaterial3D fallback keeps it from hard-failing if the shaders are
## absent (e.g. unit-testing outside the project). The inverted-hull outline is
## shared across every item (one cheap ShaderMaterial instance).

const TOON_SHADER_PATH := "res://toon.gdshader"
const OUTLINE_SHADER_PATH := "res://outline.gdshader"

static var _outline_mat: ShaderMaterial


# ── material helpers (self-contained; mirror avatar.gd's so this stands alone) ─

## Stylized cel material + shared inverted-hull outline as next_pass. `metal`
## and `rough` push it toward brushed metal / chrome / glossy ceramic; the toon
## shader stays flat, so `spec` carries the stylized highlight dot. Falls back
## to a plain toon-ish StandardMaterial3D when the shaders are missing.
static func _toon(c: Color, rim := 0.35, outline := true, spec := 0.0) -> Material:
	if ResourceLoader.exists(TOON_SHADER_PATH):
		var m := ShaderMaterial.new()
		m.shader = load(TOON_SHADER_PATH)
		m.set_shader_parameter("albedo", c)
		m.set_shader_parameter("rim_strength", rim)
		m.set_shader_parameter("spec_strength", spec)
		m.set_shader_parameter("wind_strength", 0.0)
		m.set_shader_parameter("wind_height", 0.5)
		if outline:
			if _outline_mat == null and ResourceLoader.exists(OUTLINE_SHADER_PATH):
				_outline_mat = ShaderMaterial.new()
				_outline_mat.shader = load(OUTLINE_SHADER_PATH)
			m.next_pass = _outline_mat
		return m
	var sm := StandardMaterial3D.new()
	sm.albedo_color = c
	sm.roughness = 0.85
	sm.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	sm.specular_mode = BaseMaterial3D.SPECULAR_DISABLED
	return sm


## Brushed/polished METAL — a glossy StandardMaterial3D (gold, brass, chrome,
## steel) with a real metallic+roughness response so premium trim reads richer
## than the flat cel surfaces around it. Keeps the toon outline as next_pass.
static func _metal(c: Color, metallic := 1.0, rough := 0.28) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = metallic
	m.roughness = rough
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	if ResourceLoader.exists(OUTLINE_SHADER_PATH):
		if _outline_mat == null:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = load(OUTLINE_SHADER_PATH)
		m.next_pass = _outline_mat
	return m


## Glossy CERAMIC / lacquer / enamel — smooth, low-roughness dielectric.
static func _gloss(c: Color, rough := 0.18) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.metallic = 0.0
	m.roughness = rough
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	if ResourceLoader.exists(OUTLINE_SHADER_PATH):
		if _outline_mat == null:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = load(OUTLINE_SHADER_PATH)
		m.next_pass = _outline_mat
	return m


## Soft matte FABRIC — felt / velvet / plush. High roughness, no spec, gentle
## rim so cloth reads soft next to the hard metals. (Outline as next_pass.)
static func _felt(c: Color, rim := 0.3) -> Material:
	return _toon(c, rim, true, 0.0)


## Tinted GLASS — translucent dome for the astronaut helmet (no outline; the
## outline would draw the back hull through it).
static func _glass(c: Color) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.roughness = 0.05
	m.metallic = 0.0
	m.specular_mode = BaseMaterial3D.SPECULAR_SCHLICK_GGX
	m.cull_mode = BaseMaterial3D.CULL_DISABLED
	return m


## Unshaded glowing material — gems, halos, flame, screens, sparkles. `alpha`<1
## reads as a soft radiance disc.
static func _glow(c: Color, energy := 1.4, alpha := 1.0) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(c.r, c.g, c.b, alpha)
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	if alpha < 1.0:
		m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	return m


# ── tiny primitive builders (kept local so the module is standalone) ──────────

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


static func _cyl(parent: Node3D, top_r: float, bot_r: float, h: float, mat: Material, pos: Vector3, rot := Vector3.ZERO, seg := 18) -> MeshInstance3D:
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


## Squashable sphere — the bread-and-butter cute primitive (domes, pompoms,
## gems, bobbles). `s` scales it into an egg / disc / squash.
static func _ball(parent: Node3D, r: float, s: Vector3, mat: Material, pos: Vector3, rot := Vector3.ZERO) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 22
	sm.rings = 11
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.scale = s
	mi.position = pos
	mi.rotation = rot
	parent.add_child(mi)
	return mi


static func _torus(parent: Node3D, inner: float, outer: float, mat: Material, pos: Vector3, lay_flat := true) -> MeshInstance3D:
	var tm := TorusMesh.new()
	tm.inner_radius = inner
	tm.outer_radius = outer
	tm.rings = 24
	tm.ring_segments = 12
	var mi := MeshInstance3D.new()
	mi.mesh = tm
	mi.material_override = mat
	mi.position = pos
	# TorusMesh stands up in the XY plane by default; lay it flat (around the head).
	if lay_flat:
		mi.rotation.x = PI / 2.0
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


## A faceted GEM — an octahedron-ish jewel (two short cones tip to tip), so
## higher rarities read as cut stones, not just glowing balls. Faces +Y.
static func _gem(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, facets := 6) -> Node3D:
	var n := Node3D.new()
	n.position = pos
	parent.add_child(n)
	_cyl(n, 0.0, r, h * 0.6, mat, Vector3(0, h * 0.3, 0), Vector3.ZERO, facets)          # crown
	_cyl(n, r, 0.0, h * 0.4, mat, Vector3(0, -h * 0.2, 0), Vector3.ZERO, facets)         # pavilion
	return n


## A 5-point star from two crossed flat prisms (cheap; reads as a star from the
## gameplay camera). Lies in the XY plane, faces +Z.
static func _star(parent: Node3D, span: float, depth: float, mat: Material, pos: Vector3) -> Node3D:
	var root := Node3D.new()
	root.position = pos
	for k in 5:
		var pm := PrismMesh.new()
		pm.size = Vector3(span * 0.5, span, depth)
		var mi := MeshInstance3D.new()
		mi.mesh = pm
		mi.material_override = mat
		mi.rotation.z = TAU * float(k) / 5.0
		root.add_child(mi)
	parent.add_child(root)
	return root


## A soft hovering RADIANCE disc — a flat unshaded glow billboard-ish quad,
## used under halos / behind gems so they feel lit. No shadow cast.
static func _radiance(parent: Node3D, r: float, mat: Material, pos: Vector3) -> MeshInstance3D:
	var d := _ball(parent, r, Vector3(1, 0.03, 1), mat, pos)
	d.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	return d


## A bezel-set CABOCHON — a glowing gem ringed by a metal collar, so set jewels
## read as "mounted" instead of floating. Used on bands and crowns.
static func _bezel_gem(parent: Node3D, r: float, gem_mat: Material, ring_mat: Material, pos: Vector3, rot := Vector3.ZERO) -> Node3D:
	var n := Node3D.new()
	n.position = pos
	n.rotation = rot
	parent.add_child(n)
	_torus(n, r * 0.9, r * 1.35, ring_mat, Vector3.ZERO)               # metal collar
	_ball(n, r, Vector3(1, 0.7, 1), gem_mat, Vector3(0, 0.01, 0))      # domed stone
	return n


# ════════════════════════════════════════════════════════════════════════════
#  THE CATALOG — one self-contained Node3D per item, built at the origin,
#  ~0.3 wide, resting so it drops onto the avatar's crown via _hat_root.
# ════════════════════════════════════════════════════════════════════════════


## PARTY HAT (Common) — bright candy-striped lacquer cone with a fuzzy pompom
## tip, a star topper, a chin elastic, polka dots, and a little confetti spray.
## The cheerful entry piece — but a RICH one.
static func build_party_hat() -> Node3D:
	var root := Node3D.new()
	var cone_c := _gloss(Color(0.30, 0.66, 0.95), 0.16)         # glossy sky-blue
	var stripe := _gloss(Color(1.0, 0.78, 0.26), 0.16)          # sunny yellow
	var stripe2 := _gloss(Color(0.99, 0.36, 0.55), 0.16)        # pink
	var dot := _gloss(Color(0.99, 0.97, 0.92), 0.18)            # cream polka dot
	var pom := _felt(Color(0.99, 0.95, 0.86), 0.45)             # cream fuzz
	var elastic := _felt(Color(0.30, 0.22, 0.20), 0.2)
	var star_m := _glow(Color(1.0, 0.88, 0.42), 1.6)            # glowing star topper
	# the cone body
	_cyl(root, 0.0, 0.185, 0.46, cone_c, Vector3(0, 0.23, 0), Vector3.ZERO, 22)
	# a crisp rolled rim at the base
	_torus(root, 0.175, 0.205, cone_c, Vector3(0, 0.02, 0))
	# spiralling candy stripes — thin rings climbing the cone (radius shrinks)
	for i in 6:
		var t := float(i) / 6.0
		var hy := 0.05 + t * 0.38
		var r := lerpf(0.18, 0.025, t)
		_cyl(root, r * 0.9, r * 1.02, 0.03, stripe if i % 2 == 0 else stripe2, Vector3(0, hy, 0), Vector3.ZERO, 20)
	# polka dots scattered between the stripes for richness
	for k in 8:
		var ang := TAU * float(k) / 8.0
		var t2 := fmod(float(k) * 0.37, 1.0)
		var hy2 := 0.10 + t2 * 0.26
		var rr := lerpf(0.17, 0.06, t2)
		_ball(root, 0.018, Vector3(1, 1, 0.5), dot, Vector3(cos(ang) * rr, hy2, sin(ang) * rr))
	# a fat fuzzy pompom on the tip (cluster of small balls)
	for k in 8:
		var ang := TAU * float(k) / 8.0
		_ball(root, 0.036, Vector3.ONE, pom, Vector3(cos(ang) * 0.032, 0.455 + sin(ang) * 0.02, sin(ang) * 0.032))
	_ball(root, 0.045, Vector3.ONE, pom, Vector3(0, 0.475, 0))
	# a tiny glowing star perched on the very tip
	var topper := _star(root, 0.06, 0.018, star_m, Vector3(0, 0.55, 0.02))
	topper.rotation.x = -0.2
	# chin elastic hugging the head sides
	for side: float in [-1.0, 1.0]:
		var e := _capsule(root, 0.012, 0.30, elastic, Vector3(side * 0.17, -0.06, 0), Vector3(0.1, 0, side * 0.5))
		e.rotation.z = side * 0.55
	# floating confetti dots (glow)
	_ball(root, 0.022, Vector3.ONE, _glow(Color(0.55, 1.0, 0.5), 1.3), Vector3(0.13, 0.40, 0.06))
	_ball(root, 0.020, Vector3.ONE, _glow(Color(1.0, 0.5, 0.9), 1.3), Vector3(-0.12, 0.30, -0.07))
	_ball(root, 0.018, Vector3.ONE, _glow(Color(0.5, 0.8, 1.0), 1.3), Vector3(0.10, 0.18, -0.10))
	_ball(root, 0.016, Vector3.ONE, _glow(Color(1.0, 0.95, 0.4), 1.3), Vector3(-0.10, 0.46, 0.05))
	return root


## TOP HAT (Uncommon) — tall midnight-felt crown, wide curled brim, glossy satin
## band with a gold buckle, a tucked silk rose, and a thin gold pin. Dapper + cute.
static func build_top_hat() -> Node3D:
	var root := Node3D.new()
	var felt := _felt(Color(0.11, 0.13, 0.19), 0.25)              # midnight felt
	var felt_hi := _felt(Color(0.17, 0.19, 0.27), 0.3)
	var band := _gloss(Color(0.72, 0.16, 0.32), 0.12)             # crimson satin
	var gold := _metal(Color(1.0, 0.83, 0.40), 1.0, 0.22)         # brass buckle
	var leaf := _felt(Color(0.32, 0.56, 0.32), 0.25)
	# wide brim, gently curled up at the sides (oval)
	var brim := _cyl(root, 0.30, 0.30, 0.035, felt, Vector3(0, 0.025, 0), Vector3.ZERO, 26)
	brim.scale = Vector3(1.0, 1.0, 1.08)
	_torus(root, 0.285, 0.315, felt_hi, Vector3(0, 0.04, 0))       # rolled brim edge
	# tall crown — slim, very gently flared toward the top for charm
	_cyl(root, 0.205, 0.188, 0.36, felt, Vector3(0, 0.22, 0), Vector3.ZERO, 26)
	_cyl(root, 0.21, 0.21, 0.02, felt_hi, Vector3(0, 0.40, 0), Vector3.ZERO, 26)   # crisp top rim
	_ball(root, 0.205, Vector3(1, 0.18, 1), felt_hi, Vector3(0, 0.41, 0))          # softly domed top
	# satin hatband + brass buckle
	_cyl(root, 0.195, 0.207, 0.07, band, Vector3(0, 0.085, 0), Vector3.ZERO, 26)
	_box(root, Vector3(0.07, 0.06, 0.02), gold, Vector3(0, 0.085, 0.205))
	_box(root, Vector3(0.035, 0.03, 0.025), band, Vector3(0, 0.085, 0.207))        # buckle window
	# a thin gold stick-pin through the band
	_cyl(root, 0.006, 0.006, 0.12, gold, Vector3(0.12, 0.085, 0.20), Vector3(0, 0, 0.4), 8)
	_ball(root, 0.018, Vector3.ONE, _glow(Color(0.55, 0.85, 1.0), 1.2), Vector3(0.14, 0.13, 0.21))   # pin pearl
	# a little silk rose tucked into the band
	var rose := _gloss(Color(0.95, 0.30, 0.42), 0.2)
	for k in 5:
		var a := TAU * float(k) / 5.0
		_ball(root, 0.028, Vector3(1, 0.5, 1.4), rose, Vector3(-0.14 + cos(a) * 0.022, 0.10, 0.16 + sin(a) * 0.022), Vector3(0.3, 0, a))
	_ball(root, 0.022, Vector3.ONE, _gloss(Color(0.99, 0.55, 0.55), 0.2), Vector3(-0.14, 0.10, 0.165))
	_ball(root, 0.02, Vector3(1.6, 0.4, 0.7), leaf, Vector3(-0.17, 0.085, 0.15))
	return root


## PROPELLER CAP (Uncommon) — the classic beanie rebuilt from clean stitched
## GORE panels in alternating lacquer colours, a white top button, a contrast
## tipped brim, and a real spinnable propeller on a chrome post (modelled
## mid-spin). Crisp panels now read like a sewn cap, not a blob.
static func build_propeller_cap() -> Node3D:
	var root := Node3D.new()
	var panel_a := _gloss(Color(0.92, 0.26, 0.30), 0.16)   # red
	var panel_b := _gloss(Color(0.98, 0.80, 0.24), 0.16)   # yellow
	var panel_c := _gloss(Color(0.30, 0.62, 0.95), 0.16)   # blue
	var panel_d := _gloss(Color(0.40, 0.78, 0.42), 0.16)   # green
	var brim_c := _gloss(Color(0.95, 0.95, 0.97), 0.18)    # white brim
	var post := _metal(Color(0.85, 0.86, 0.90), 1.0, 0.22) # chrome post
	var seam := _felt(Color(0.16, 0.16, 0.20), 0.2)
	var cols := [panel_a, panel_b, panel_c, panel_d, panel_a, panel_b]
	# the dome built as 6 GORE panels — tall thin squashed eggs leaning to the
	# crown, each on its own meridian, so the cap reads as distinct sewn sections
	for k in 6:
		var ang := TAU * float(k) / 6.0
		var px := cos(ang) * 0.115
		var pz := sin(ang) * 0.115
		var gore := _ball(root, 0.10, Vector3(0.85, 2.05, 1.7), cols[k], Vector3(px, 0.115, pz))
		gore.rotation.y = -ang
		gore.rotation.x = 0.0
	# a tight crown cap-ball to close the top of the gores cleanly
	_ball(root, 0.115, Vector3(1.0, 0.95, 1.0), panel_a, Vector3(0, 0.075, 0))
	# stitched panel seams sitting in the valleys between gores
	for k in 6:
		var ang := TAU * (float(k) + 0.5) / 6.0
		var rib := _capsule(root, 0.008, 0.30, seam, Vector3(cos(ang) * 0.10, 0.135, sin(ang) * 0.10), Vector3(0, 0, 0))
		rib.rotation.y = ang
		rib.rotation.x = 0.55
	# a beaded brow stitch ring around the base
	_torus(root, 0.205, 0.225, seam, Vector3(0, 0.025, 0))
	# contrast brim, tipped down a touch at the front
	var brim := _cyl(root, 0.20, 0.20, 0.028, brim_c, Vector3(0, 0.03, 0.20), Vector3(-0.18, 0, 0), 18)
	brim.scale = Vector3(1.1, 1.0, 1.25)
	_torus(root, 0.19, 0.21, seam, Vector3(0, 0.04, 0.20))   # brim welt
	# chrome post up top, with the propeller hub + blades (modelled mid-spin)
	_cyl(root, 0.018, 0.022, 0.10, post, Vector3(0, 0.25, 0), Vector3.ZERO, 10)
	_ball(root, 0.035, Vector3.ONE, post, Vector3(0, 0.305, 0))   # hub
	var blade_cols := [panel_b, panel_c, panel_a]
	for k in 3:
		var ang := TAU * float(k) / 3.0
		var blade := _box(root, Vector3(0.18, 0.012, 0.05), blade_cols[k], Vector3(cos(ang) * 0.09, 0.31, sin(ang) * 0.09))
		blade.rotation.y = ang
		blade.rotation.z = 0.18   # pitched, so it reads as spinning
	_ball(root, 0.022, Vector3.ONE, post, Vector3(0, 0.325, 0))   # cap nut
	# top button under the post
	_ball(root, 0.03, Vector3.ONE, brim_c, Vector3(0, 0.20, 0))
	return root


## CAT EARS (Uncommon) — a slim glossy headband with two plush triangular ears
## (gradient pink inner shells), fur tufts, dainty whiskers, a sparkle gem at the
## crest, and a pair of gold bell charms on tiny chains. Maximum kawaii.
static func build_cat_ears() -> Node3D:
	var root := Node3D.new()
	var band := _gloss(Color(0.16, 0.17, 0.22), 0.16)       # glossy black band
	var fur := _felt(Color(0.20, 0.21, 0.27), 0.32)         # plush dark-grey ear
	var fur_hi := _felt(Color(0.28, 0.29, 0.36), 0.34)      # lit plush highlight
	var inner := _felt(Color(0.98, 0.70, 0.78), 0.4)        # soft pink shell
	var inner_hi := _felt(Color(0.99, 0.84, 0.88), 0.42)    # lighter pink core
	var tuft := _felt(Color(0.96, 0.96, 0.98), 0.4)         # white fur tufts
	var bell := _metal(Color(1.0, 0.82, 0.36), 1.0, 0.24)   # gold bell
	var whisk := _felt(Color(0.95, 0.96, 0.99), 0.3)        # whiskers
	var spark := _glow(Color(1.0, 0.6, 0.85), 1.5)          # pink sparkle gem
	# slim headband arc (small boxes ear to ear over the crown)
	for k in 11:
		var t := float(k) / 10.0
		var ang := PI * t
		var x := cos(ang) * 0.235
		var y := 0.10 + sin(ang) * 0.14
		var seg := _box(root, Vector3(0.035, 0.035, 0.07), band, Vector3(x, y, 0))
		seg.rotation.z = ang - PI / 2.0
	# a sparkle bezel-gem set on the crest of the band
	_bezel_gem(root, 0.03, spark, bell, Vector3(0, 0.255, 0.02))
	# two ears — stacked tapering boxes for a soft triangle, leaned out
	for side: float in [-1.0, 1.0]:
		var ex: float = side * 0.13
		# outer plush ear (base shadow + lit highlight layer)
		_box(root, Vector3(0.18, 0.04, 0.11), fur, Vector3(ex, 0.20, 0), Vector3(0, 0, side * -0.25))
		_box(root, Vector3(0.13, 0.04, 0.085), fur, Vector3(ex + side * 0.015, 0.27, 0), Vector3(0, 0, side * -0.25))
		_box(root, Vector3(0.07, 0.04, 0.06), fur, Vector3(ex + side * 0.03, 0.33, 0), Vector3(0, 0, side * -0.25))
		# a thin lit highlight strip up the front edge of the ear
		_box(root, Vector3(0.03, 0.045, 0.10), fur_hi, Vector3(ex - side * 0.06, 0.21, 0.005), Vector3(0, 0, side * -0.25))
		# rounded plush tip
		_ball(root, 0.035, Vector3(1, 0.8, 0.9), fur, Vector3(ex + side * 0.042, 0.365, 0))
		# pink inner shell, slightly forward (two-tone)
		_box(root, Vector3(0.10, 0.03, 0.06), inner, Vector3(ex, 0.22, 0.025), Vector3(0, 0, side * -0.25))
		_box(root, Vector3(0.06, 0.03, 0.04), inner_hi, Vector3(ex + side * 0.02, 0.28, 0.028), Vector3(0, 0, side * -0.25))
		# little fur tufts poking from the base
		for j in 3:
			_ball(root, 0.014, Vector3(1, 1.6, 1), tuft, Vector3(ex - 0.05 + j * 0.05, 0.155, 0.04), Vector3(0, 0, side * 0.3))
		# dainty whiskers fanning forward from the lower band
		for w in 2:
			var wy := 0.04 + w * 0.03
			var wm := _capsule(root, 0.0035, 0.16 - w * 0.02, whisk, Vector3(side * 0.21, wy, 0.10), Vector3.ZERO)
			wm.rotation.y = side * (0.9 + w * 0.18)
			wm.rotation.z = side * (0.5 - w * 0.15)
		# gold bell charm on a tiny link, dangling at the band
		_cyl(root, 0.004, 0.004, 0.035, bell, Vector3(side * 0.235, 0.07, 0.04), Vector3.ZERO, 6)   # link
		_ball(root, 0.028, Vector3(1, 0.95, 1), bell, Vector3(side * 0.235, 0.045, 0.04))
		_box(root, Vector3(0.022, 0.006, 0.02), _felt(Color(0.6, 0.45, 0.2), 0.2), Vector3(side * 0.235, 0.03, 0.05))
	return root


## VIKING HELMET (Rare) — a brushed-iron dome with riveted brass brow band, a
## forged nose guard, two big curved bone horns with banded brass rings, a crest
## ridge, and a ruby finial gem. Heroic + chunky.
static func build_viking_helmet() -> Node3D:
	var root := Node3D.new()
	var iron := _metal(Color(0.62, 0.65, 0.70), 1.0, 0.34)       # brushed iron
	var iron_dk := _metal(Color(0.42, 0.45, 0.50), 1.0, 0.42)
	var brass := _metal(Color(0.86, 0.66, 0.30), 1.0, 0.30)      # brass band
	var horn := _felt(Color(0.95, 0.92, 0.84), 0.3)              # bone horn
	var horn_dk := _felt(Color(0.80, 0.75, 0.64), 0.25)
	var gem := _glow(Color(0.95, 0.30, 0.36), 1.4)               # ruby rivet
	# the dome — squashed iron half-sphere with a crisp seam ridge over the top
	_ball(root, 0.255, Vector3(1.0, 0.82, 1.0), iron, Vector3(0, 0.11, 0))
	_box(root, Vector3(0.03, 0.03, 0.5), iron_dk, Vector3(0, 0.30, 0))            # crest seam
	# top spike finial with a ruby
	_cyl(root, 0.0, 0.04, 0.10, iron_dk, Vector3(0, 0.33, 0), Vector3.ZERO, 8)
	_gem(root, 0.03, 0.06, gem, Vector3(0, 0.30, 0), 6)
	# riveted brass brow band around the base
	_torus(root, 0.215, 0.255, brass, Vector3(0, 0.045, 0))
	for k in 12:
		var ang := TAU * float(k) / 12.0
		_ball(root, 0.016, Vector3.ONE, iron_dk, Vector3(cos(ang) * 0.245, 0.05, sin(ang) * 0.245))   # rivets
	# forged nose guard down the front
	_box(root, Vector3(0.055, 0.18, 0.03), iron, Vector3(0, 0.0, 0.235), Vector3(-0.15, 0, 0))
	_ball(root, 0.04, Vector3(1, 0.7, 0.6), iron, Vector3(0, 0.06, 0.245))
	# two big curved horns sweeping up and out (stacked tapering cylinders)
	for side: float in [-1.0, 1.0]:
		var base := Vector3(side * 0.20, 0.07, 0.02)
		var prev := base
		var radii := [0.05, 0.042, 0.033, 0.022, 0.012]
		for i in radii.size():
			var seg_h := 0.085
			var top_r: float = radii[i + 1] if i + 1 < radii.size() else 0.005
			var curl := Vector3(side * 0.045 * float(i), 0.004 * float(i) * float(i), 0.0)
			var pos := prev + Vector3(0, seg_h * 0.5, 0) + curl
			var mat := horn_dk if i >= 3 else horn
			_cyl(root, top_r, radii[i], seg_h, mat, pos, Vector3(0, 0, -side * (0.35 + 0.12 * float(i))), 12)
			prev = pos + Vector3(0, seg_h * 0.5, 0) + curl
		# brass banding rings at the horn base
		for b in 2:
			_torus(root, 0.04 - b * 0.006, 0.058 - b * 0.006, brass, base + Vector3(side * (0.02 + b * 0.02), 0.05 + b * 0.05, 0))
	return root


## HEADPHONES (Rare) — premium studio cans: chrome-arm slider band, plush memory-
## foam ear cups with glowing RGB accent rings, a boom mic, and tiny lit level
## LEDs. Reads "audiophile".
static func build_headphones() -> Node3D:
	var root := Node3D.new()
	var shell := _gloss(Color(0.13, 0.14, 0.18), 0.16)          # piano-black shell
	var pad := _felt(Color(0.16, 0.17, 0.22), 0.3)              # memory foam
	var leather := _gloss(Color(0.20, 0.21, 0.27), 0.2)
	var chrome := _metal(Color(0.82, 0.84, 0.88), 1.0, 0.18)    # chrome slider
	var accent := _glow(Color(0.40, 0.85, 1.0), 1.6)            # cyan ring (matches LED eyes)
	var mic := _felt(Color(0.10, 0.10, 0.13), 0.2)
	# the over-the-head band — an arc of small shells from ear to ear
	for k in 13:
		var t := float(k) / 12.0
		var ang := PI * t
		var x := cos(ang) * 0.30
		var y := 0.16 + sin(ang) * 0.22
		var seg := _box(root, Vector3(0.055, 0.05, 0.085), shell, Vector3(x, y, 0))
		seg.rotation.z = ang - PI / 2.0
	# chrome slider rails set into the band
	for side: float in [-1.0, 1.0]:
		_box(root, Vector3(0.03, 0.16, 0.03), chrome, Vector3(side * 0.30, 0.14, 0), Vector3(0, 0, side * 0.2))
	# soft leather padding strip along the inner top
	_capsule(root, 0.03, 0.34, leather, Vector3(0, 0.37, 0), Vector3(0, 0, PI / 2.0))
	# the two ear cups — tiered: shell, foam ring, glowing accent ring, driver dot
	for side: float in [-1.0, 1.0]:
		_cyl(root, 0.12, 0.13, 0.07, shell, Vector3(side * 0.30, 0.02, 0), Vector3(0, 0, PI / 2.0), 20)
		_cyl(root, 0.105, 0.105, 0.05, leather, Vector3(side * 0.345, 0.02, 0), Vector3(0, 0, PI / 2.0), 18)
		_cyl(root, 0.075, 0.075, 0.05, pad, Vector3(side * 0.355, 0.02, 0), Vector3(0, 0, PI / 2.0), 18)
		var ring := _torus(root, 0.085, 0.108, accent, Vector3(side * 0.27, 0.02, 0))
		ring.rotation = Vector3(0, 0, PI / 2.0)
		# driver dot in the centre + a tiny lit logo
		_ball(root, 0.03, Vector3(0.4, 1, 1), shell, Vector3(side * 0.36, 0.02, 0))
		_ball(root, 0.014, Vector3.ONE, accent, Vector3(side * 0.375, 0.02, 0))
	# a boom mic on one side
	_capsule(root, 0.012, 0.22, mic, Vector3(0.30, -0.05, 0.18), Vector3(0.7, 0.4, 0))
	_ball(root, 0.03, Vector3.ONE, mic, Vector3(0.18, -0.14, 0.30))
	# three lit level LEDs on the cup
	for j in 3:
		var c: Color = [Color(0.4, 1.0, 0.5), Color(1.0, 0.85, 0.3), Color(1.0, 0.35, 0.4)][j]
		_ball(root, 0.012, Vector3.ONE, _glow(c, 1.8), Vector3(-0.34, 0.08 - j * 0.03, 0.06))
	return root


## FLOWER CROWN (Rare) — a woven leafy ring crowned with a ring of layered
## blossoms (multi-petal, glowing pollen centres), berries, and trailing ivy.
## Lush cottagecore — far richer than a simple daisy band.
static func build_flower_crown() -> Node3D:
	var root := Node3D.new()
	var vine := _felt(Color(0.36, 0.58, 0.32), 0.3)
	var vine_dk := _felt(Color(0.28, 0.48, 0.28), 0.25)
	var leaf := _felt(Color(0.44, 0.72, 0.38), 0.3)
	var berry := _gloss(Color(0.85, 0.20, 0.30), 0.18)
	# woven base — two braided vine tori, offset, plus leaf nubs all around
	_torus(root, 0.205, 0.25, vine, Vector3(0, 0.05, 0))
	_torus(root, 0.21, 0.248, vine_dk, Vector3(0, 0.07, 0))
	for k in 14:
		var ang := TAU * float(k) / 14.0
		var nub := _ball(root, 0.035, Vector3(1.7, 0.4, 0.8), leaf,
			Vector3(cos(ang) * 0.24, 0.07, sin(ang) * 0.24), Vector3(0.3, -ang, 0))
		nub.rotation.y = -ang
	# a ring of full blossoms — two petal layers + a glowing pollen centre
	var blooms := [
		Color(0.99, 0.97, 0.93), Color(0.99, 0.62, 0.74), Color(0.78, 0.66, 0.98),
		Color(0.99, 0.84, 0.42), Color(0.66, 0.86, 0.99), Color(0.99, 0.55, 0.45),
		Color(0.86, 0.99, 0.74),
	]
	for k in blooms.size():
		var ang := TAU * float(k) / float(blooms.size()) + 0.2
		var cx := cos(ang) * 0.225
		var cz := sin(ang) * 0.225
		var petal := _gloss(blooms[k], 0.22)
		var petal_in := _gloss(blooms[k].lightened(0.18), 0.22)
		var centre := _glow(Color(1.0, 0.82, 0.30), 0.9)
		# outer 5 petals
		for p in 5:
			var pa := TAU * float(p) / 5.0
			_ball(root, 0.04, Vector3(1, 0.35, 1.6), petal,
				Vector3(cx + cos(pa) * 0.045, 0.12, cz + sin(pa) * 0.045), Vector3(0.2, -ang, 0))
		# inner 5 petals (smaller, offset)
		for p in 5:
			var pa := TAU * (float(p) + 0.5) / 5.0
			_ball(root, 0.026, Vector3(1, 0.4, 1.4), petal_in,
				Vector3(cx + cos(pa) * 0.025, 0.135, cz + sin(pa) * 0.025))
		_ball(root, 0.028, Vector3(1, 0.6, 1), centre, Vector3(cx, 0.14, cz))
	# clusters of berries tucked between blooms
	for k in 5:
		var ang := TAU * (float(k) + 0.5) / 5.0
		var bx := cos(ang) * 0.235
		var bz := sin(ang) * 0.235
		for j in 3:
			_ball(root, 0.018, Vector3.ONE, berry, Vector3(bx + (j - 1) * 0.02, 0.10 + (j % 2) * 0.015, bz))
	# two trailing ivy strands dangling at the sides
	for side: float in [-1.0, 1.0]:
		var px: float = side * 0.24
		for j in 4:
			_ball(root, 0.025 - j * 0.003, Vector3(1.4, 0.5, 0.8), leaf,
				Vector3(px + side * j * 0.012, 0.05 - j * 0.05, 0.03), Vector3(0.3, 0, side * 0.4))
	return root


## WIZARD HAT (Epic) — a tall droopy starry cone with a curled bobble tip, a
## wide soft brim, a jewelled glowing band, scattered emissive stars + a crescent
## moon, and three little orbiting sparkles. Arcane and premium.
static func build_wizard_hat() -> Node3D:
	var root := Node3D.new()
	var cloth := _felt(Color(0.30, 0.20, 0.58), 0.3)              # deep indigo
	var cloth_dk := _felt(Color(0.22, 0.14, 0.46), 0.25)
	var brim_c := _felt(Color(0.25, 0.16, 0.50), 0.25)
	var band := _metal(Color(0.95, 0.80, 0.38), 1.0, 0.26)        # gold band
	var starm := _glow(Color(1.0, 0.92, 0.45), 1.6)
	var moonm := _glow(Color(0.88, 0.94, 1.0), 1.2)
	var gem := _glow(Color(0.55, 0.85, 1.0), 1.5)
	# wide soft brim with a curled-up edge
	var brim := _cyl(root, 0.30, 0.315, 0.035, brim_c, Vector3(0, 0.02, 0), Vector3.ZERO, 24)
	brim.scale = Vector3(1.0, 1.0, 1.06)
	_torus(root, 0.30, 0.33, cloth_dk, Vector3(0, 0.035, 0))
	# the cone — stackable segments that lean + narrow → a tall droopy tip
	var prev := Vector3(0, 0.05, 0)
	var lean := Vector3(0.05, 0, 0.02)
	var radii := [0.20, 0.16, 0.12, 0.085, 0.05, 0.028]
	for i in radii.size():
		var seg_h := 0.115
		var top_r: float = radii[i + 1] if i + 1 < radii.size() else 0.0
		var off := lean * (float(i) * float(i) * 0.16)
		var pos := prev + Vector3(0, seg_h * 0.5, 0) + off
		_cyl(root, top_r, radii[i], seg_h, cloth, pos, Vector3.ZERO, 20)
		prev = pos + Vector3(0, seg_h * 0.5, 0) + off
	# curled tip bobble
	_ball(root, 0.045, Vector3.ONE, starm, prev + Vector3(0.025, 0.02, 0.01))
	# jewelled gold band at the base with inlaid bezel-set gems
	_cyl(root, 0.205, 0.215, 0.06, band, Vector3(0, 0.085, 0), Vector3.ZERO, 24)
	for k in 6:
		var a := TAU * float(k) / 6.0
		_bezel_gem(root, 0.022, gem, band, Vector3(cos(a) * 0.215, 0.085, sin(a) * 0.215), Vector3(PI / 2.0, -a, 0))
	# emissive stars scattered up the cloth
	_star(root, 0.07, 0.02, starm, Vector3(0.0, 0.34, 0.18))
	_star(root, 0.05, 0.02, starm, Vector3(0.12, 0.18, 0.14))
	_star(root, 0.045, 0.02, starm, Vector3(-0.11, 0.26, 0.11))
	_star(root, 0.04, 0.02, starm, Vector3(0.04, 0.50, 0.10))
	# crescent moon = a glow ball with a cloth ball biting out of it
	_ball(root, 0.05, Vector3.ONE, moonm, Vector3(-0.14, 0.40, 0.0))
	_ball(root, 0.044, Vector3.ONE, cloth, Vector3(-0.165, 0.42, 0.01))
	# three little orbiting sparkles around the cone
	for k in 3:
		var a := TAU * float(k) / 3.0
		_ball(root, 0.018, Vector3.ONE, starm, Vector3(cos(a) * 0.26, 0.30 + sin(a) * 0.05, sin(a) * 0.26))
	return root


## CROWN (Epic) — a regal polished-gold band with five tipped fleur points, big
## faceted ruby/sapphire/emerald jewels on each point, a bezel-set gem frieze,
## pearl beading rails, and a soft radiance. Unmistakably royal.
static func build_crown() -> Node3D:
	var root := Node3D.new()
	var gold := _metal(Color(1.0, 0.82, 0.36), 1.0, 0.20)
	var gold_dk := _metal(Color(0.82, 0.62, 0.22), 1.0, 0.28)
	var ruby := _glow(Color(0.97, 0.24, 0.40), 1.6)
	var sapph := _glow(Color(0.36, 0.60, 1.0), 1.6)
	var emer := _glow(Color(0.35, 0.92, 0.55), 1.5)
	var pearl := _gloss(Color(0.98, 0.97, 0.95), 0.12)
	# the band — a flared gold ring with a beaded top + bottom rail
	_cyl(root, 0.21, 0.23, 0.12, gold, Vector3(0, 0.07, 0), Vector3.ZERO, 18)
	_torus(root, 0.215, 0.245, gold_dk, Vector3(0, 0.13, 0))
	_torus(root, 0.215, 0.245, gold_dk, Vector3(0, 0.02, 0))
	# five tipped fleur points, each a tapered point + a faceted jewel + collar
	for k in 5:
		var ang := TAU * float(k) / 5.0
		var px := cos(ang) * 0.205
		var pz := sin(ang) * 0.205
		var spike := _cyl(root, 0.0, 0.055, 0.17, gold, Vector3(px, 0.20, pz), Vector3.ZERO, 8)
		spike.look_at_from_position(spike.position, spike.position + Vector3(px, -2.0, pz), Vector3.UP)
		# little gold ball collar where the point meets the band
		_ball(root, 0.03, Vector3.ONE, gold_dk, Vector3(px, 0.135, pz))
		# big faceted tip jewel (cycle ruby / sapphire / emerald)
		var jewel: StandardMaterial3D = [ruby, sapph, emer][k % 3]
		_gem(root, 0.04, 0.10, jewel, Vector3(px, 0.30, pz), 6)
		# radiance behind it
		_radiance(root, 0.06, _glow(jewel.emission, 0.7, 0.3), Vector3(px, 0.30, pz))
	# bezel-set gem frieze around the band front + pearl dots between
	for k in 5:
		var ang2 := TAU * (float(k) + 0.5) / 5.0
		var gx := cos(ang2) * 0.225
		var gz := sin(ang2) * 0.225
		var fmat: StandardMaterial3D = [sapph, ruby, emer][k % 3]
		_bezel_gem(root, 0.028, fmat, gold_dk, Vector3(gx, 0.075, gz), Vector3(PI / 2.0, -ang2, 0))
	for k in 10:
		var ang3 := TAU * float(k) / 10.0
		_ball(root, 0.014, Vector3.ONE, pearl, Vector3(cos(ang3) * 0.235, 0.115, sin(ang3) * 0.235))
		_ball(root, 0.014, Vector3.ONE, pearl, Vector3(cos(ang3) * 0.235, 0.025, sin(ang3) * 0.235))
	return root


## HALO (Epic) — far more than a glowing ring: an ornate floating halo of solid
## gold filigree — a double scrollwork band studded with bezel-set sapphires, a
## ring of gold fleur points, a feathered celestial WING flaring at each side, a
## radiant core, orbiting twinkle-stars and drifting motes. Reads as a crafted
## angelic relic, not just light.
static func build_halo() -> Node3D:
	var root := Node3D.new()
	var gold := _metal(Color(1.0, 0.86, 0.42), 1.0, 0.18)       # solid polished gold
	var gold_dk := _metal(Color(0.82, 0.62, 0.24), 1.0, 0.26)
	var glow_g := _glow(Color(1.0, 0.90, 0.52), 2.0)            # gold emissive edge
	var glow_soft := _glow(Color(1.0, 0.94, 0.70), 1.2)
	var soft := _glow(Color(1.0, 0.97, 0.82), 0.6, 0.35)
	var feather := _gloss(Color(0.99, 0.98, 0.95), 0.14)        # pearly white feathers
	var feather_sh := _gloss(Color(0.90, 0.92, 0.98), 0.2)
	var sapph := _glow(Color(0.40, 0.66, 1.0), 1.6)             # set sapphires
	var hy := 0.36                                               # the halo hovers here
	# --- the SOLID halo: a thick polished-gold ring (real geometry) plus a thin
	#     glowing inner edge so it reads as a forged, lit relic
	var ring := _torus(root, 0.165, 0.215, gold, Vector3(0, hy, 0))
	ring.rotation.x = PI / 2.0 - 0.12
	var ring_dk := _torus(root, 0.20, 0.225, gold_dk, Vector3(0, hy, 0))
	ring_dk.rotation.x = PI / 2.0 - 0.12
	var ring_glow := _torus(root, 0.155, 0.168, glow_g, Vector3(0, hy, 0))
	ring_glow.rotation.x = PI / 2.0 - 0.12
	# --- gold filigree fleur points standing up around the band (crafted detail)
	for k in 8:
		var a := TAU * float(k) / 8.0
		var fx := cos(a) * 0.205
		var fz := sin(a) * 0.205
		var fp := _cyl(root, 0.0, 0.024, 0.07, gold, Vector3(fx, hy + 0.005, fz), Vector3.ZERO, 6)
		fp.look_at_from_position(fp.position, fp.position + Vector3(fx, 0.0, fz), Vector3.UP)
		fp.rotate_object_local(Vector3.RIGHT, PI / 2.0)
	# --- bezel-set sapphires inlaid around the band face
	for k in 6:
		var a := TAU * float(k) / 6.0 + 0.3
		_bezel_gem(root, 0.02, sapph, gold_dk, Vector3(cos(a) * 0.19, hy + 0.012, sin(a) * 0.19), Vector3(PI / 2.0 - 0.12, -a, 0))
	# --- two celestial WINGS flaring out from the sides (3 layered feather rows)
	for side: float in [-1.0, 1.0]:
		var wing := Node3D.new()
		wing.position = Vector3(side * 0.20, hy - 0.02, -0.02)
		wing.rotation = Vector3(0.0, side * 0.5, side * 0.25)
		root.add_child(wing)
		# three rows of overlapping feathers, each row longer than the last
		for row in 3:
			var n := 4 + row
			for j in n:
				var t := float(j) / float(n - 1)
				var fl := 0.06 + row * 0.045 + t * 0.05            # feather length grows outward
				var fx2 := side * (0.02 + t * (0.12 + row * 0.05))
				var fy2 := -0.01 + row * 0.03 + t * 0.05
				var fmat := feather if (j % 2 == 0) else feather_sh
				var fea := _ball(wing, 0.02, Vector3(0.6, 0.35, 1.0 + fl * 6.0), fmat, Vector3(fx2, fy2, -0.02 - row * 0.02))
				fea.rotation = Vector3(0.4, side * (-0.3 - t * 0.5), side * (0.3 + t * 0.4))
		# a gold covert-feather bar capping the wing root
		_capsule(wing, 0.012, 0.10, gold, Vector3(side * 0.03, 0.0, 0.0), Vector3(0, 0, side * 1.1))
	# --- radiance discs to read as light
	_radiance(root, 0.26, soft, Vector3(0, hy, 0))
	_radiance(root, 0.15, _glow(Color(1.0, 0.99, 0.90), 1.0, 0.5), Vector3(0, hy, 0))
	# --- a ring of little twinkle-stars riding the band
	for k in 6:
		var ang := TAU * float(k) / 6.0
		var st := _star(root, 0.04, 0.015, glow_soft, Vector3(cos(ang) * 0.19, hy + 0.01, sin(ang) * 0.19))
		st.rotation.x = PI / 2.0
	# --- a few gentle motes drifting just under the ring
	for k in 5:
		var ang := TAU * float(k) / 5.0 + 0.4
		_ball(root, 0.016, Vector3.ONE, glow_soft, Vector3(cos(ang) * 0.13, hy - 0.12 + sin(ang * 2.0) * 0.03, sin(ang) * 0.13))
	return root


## ASTRONAUT HELMET (Legendary) — a full glossy-white spacesuit helmet: a clear
## tinted glass dome over a glowing visor, a gold sun-shield rim, brushed-metal
## neck ring with hose ports, side lamps, antenna, and a lit HUD reticle inside.
## A premium hero piece — the glass dome reads instantly.
static func build_astronaut_helmet() -> Node3D:
	var root := Node3D.new()
	var shell := _gloss(Color(0.95, 0.96, 0.98), 0.14)            # glossy white shell
	var shell_sh := _gloss(Color(0.86, 0.88, 0.92), 0.18)
	var ring := _metal(Color(0.78, 0.80, 0.85), 1.0, 0.20)        # brushed neck ring
	var gold := _metal(Color(1.0, 0.84, 0.40), 1.0, 0.18)         # gold sun-shield
	var dome := _glass(Color(0.62, 0.82, 0.95, 0.28))             # clear blue-tinted glass
	var visor := _glow(Color(0.30, 0.78, 1.0), 1.5)               # glowing cyan visor
	var lamp := _glow(Color(1.0, 0.97, 0.88), 1.8)                # head lamps
	var hud := _glow(Color(0.4, 1.0, 0.7), 1.4)                   # HUD reticle
	# back/sides of the helmet shell — a white sphere cut by the dome at the front
	_ball(root, 0.26, Vector3(1.0, 1.0, 1.0), shell, Vector3(0, 0.22, -0.02))
	# a darker glowing visor face inside, set back so the glass reads over it
	_ball(root, 0.215, Vector3(1.0, 0.9, 0.7), visor, Vector3(0, 0.22, 0.10))
	# gold sun-shield rim arcing over the brow (segmented torus-ish band)
	for k in 9:
		var t := float(k) / 8.0
		var ang := lerpf(-0.5, PI + 0.5, t)
		var x := cos(ang) * 0.235
		var y := 0.22 + sin(ang) * 0.235
		var seg := _box(root, Vector3(0.05, 0.05, 0.28), gold, Vector3(x, y, 0.05))
		seg.rotation.z = ang - PI / 2.0
	# the CLEAR GLASS DOME over the front (drawn last-ish; double-sided)
	_ball(root, 0.255, Vector3(1.05, 1.0, 1.05), dome, Vector3(0, 0.22, 0.02))
	# lit HUD reticle floating on the glass
	_torus(root, 0.03, 0.04, hud, Vector3(0.08, 0.28, 0.255))
	_box(root, Vector3(0.06, 0.006, 0.006), hud, Vector3(0.08, 0.28, 0.255))
	_box(root, Vector3(0.006, 0.06, 0.006), hud, Vector3(0.08, 0.28, 0.255))
	# brushed-metal neck ring with bolts
	_cyl(root, 0.225, 0.235, 0.05, ring, Vector3(0, 0.015, 0), Vector3.ZERO, 24)
	_torus(root, 0.22, 0.25, shell_sh, Vector3(0, 0.05, 0))
	for k in 14:
		var a := TAU * float(k) / 14.0
		_ball(root, 0.012, Vector3.ONE, ring, Vector3(cos(a) * 0.235, 0.015, sin(a) * 0.235))
	# two hose-port nipples on the lower front
	for side: float in [-1.0, 1.0]:
		_cyl(root, 0.03, 0.035, 0.05, ring, Vector3(side * 0.13, 0.04, 0.20), Vector3(1.2, 0, 0), 12)
		_ball(root, 0.025, Vector3.ONE, _felt(Color(0.2, 0.22, 0.26), 0.2), Vector3(side * 0.13, 0.02, 0.235))
	# two head lamps on the sides + a short antenna with a glowing tip
	for side: float in [-1.0, 1.0]:
		_cyl(root, 0.04, 0.045, 0.05, shell_sh, Vector3(side * 0.245, 0.26, 0.06), Vector3(0, side * 0.6, 0), 12)
		_ball(root, 0.03, Vector3.ONE, lamp, Vector3(side * 0.27, 0.26, 0.10))
	_cyl(root, 0.008, 0.01, 0.12, ring, Vector3(0.14, 0.42, -0.10), Vector3(-0.3, 0, -0.3), 8)
	_ball(root, 0.025, Vector3.ONE, _glow(Color(1.0, 0.3, 0.3), 2.0), Vector3(0.17, 0.49, -0.13))
	return root


## FLAME CROWN (Legendary) — a blackened-gold ember crown wreathed in living
## fire: tapered flame tongues of stacked glowing tiers (red→orange→yellow→white
## core), drifting ember motes, a molten gem heart, and a heat-haze radiance.
## Everything emits — it blazes in the dark.
static func build_flame_crown() -> Node3D:
	var root := Node3D.new()
	var charcoal := _metal(Color(0.16, 0.14, 0.13), 1.0, 0.5)       # blackened metal
	var emberband := _glow(Color(1.0, 0.42, 0.12), 1.2)            # glowing seam
	var f_red := _glow(Color(1.0, 0.20, 0.08), 1.8)
	var f_org := _glow(Color(1.0, 0.45, 0.10), 2.0)
	var f_yel := _glow(Color(1.0, 0.80, 0.22), 2.2)
	var f_white := _glow(Color(1.0, 0.95, 0.70), 2.6)
	var molten := _glow(Color(1.0, 0.55, 0.15), 2.4)
	# the blackened-gold band with a molten ember seam glowing through cracks
	_cyl(root, 0.205, 0.225, 0.10, charcoal, Vector3(0, 0.06, 0), Vector3.ZERO, 20)
	_torus(root, 0.205, 0.235, emberband, Vector3(0, 0.085, 0))
	_torus(root, 0.205, 0.235, emberband, Vector3(0, 0.02, 0))
	for k in 10:
		var a := TAU * float(k) / 10.0
		_box(root, Vector3(0.012, 0.08, 0.01), emberband, Vector3(cos(a) * 0.222, 0.06, sin(a) * 0.222), Vector3(0, -a, 0))
	# five flame tongues — each a stack of shrinking tiers, layered red→white core,
	# leaning/twisting so they look alive
	for k in 5:
		var ang := TAU * float(k) / 5.0
		var bx := cos(ang) * 0.165
		var bz := sin(ang) * 0.165
		var lean := 0.04
		# outer red shell
		_cyl(root, 0.0, 0.075, 0.22, f_red, Vector3(bx, 0.20, bz), Vector3(bz * lean, 0, -bx * lean), 8)
		# orange mid
		_cyl(root, 0.0, 0.055, 0.20, f_org, Vector3(bx * 0.95, 0.21, bz * 0.95), Vector3(bz * lean, 0, -bx * lean), 8)
		# yellow inner
		_cyl(root, 0.0, 0.035, 0.17, f_yel, Vector3(bx * 0.9, 0.22, bz * 0.9), Vector3(bz * lean, 0, -bx * lean), 8)
		# white-hot core tip
		_cyl(root, 0.0, 0.018, 0.12, f_white, Vector3(bx * 0.85, 0.24, bz * 0.85), Vector3(bz * lean, 0, -bx * lean), 6)
		# a little teardrop of fire splitting off
		_ball(root, 0.03, Vector3(0.7, 1.5, 0.7), f_org, Vector3(bx * 1.15, 0.30, bz * 1.15))
	# a tall central flame tongue (the crown's peak)
	_cyl(root, 0.0, 0.09, 0.30, f_red, Vector3(0, 0.27, 0), Vector3.ZERO, 8)
	_cyl(root, 0.0, 0.06, 0.27, f_org, Vector3(0, 0.28, 0), Vector3.ZERO, 8)
	_cyl(root, 0.0, 0.035, 0.23, f_yel, Vector3(0, 0.29, 0), Vector3.ZERO, 8)
	_cyl(root, 0.0, 0.018, 0.16, f_white, Vector3(0, 0.31, 0), Vector3.ZERO, 6)
	# a molten gem heart at the brow + radiance
	_gem(root, 0.05, 0.11, molten, Vector3(0, 0.10, 0.21), 6)
	_radiance(root, 0.09, _glow(Color(1.0, 0.5, 0.15), 1.0, 0.35), Vector3(0, 0.10, 0.21))
	# drifting ember motes around the crown
	for k in 8:
		var a := TAU * float(k) / 8.0
		var em: StandardMaterial3D = [f_org, f_yel, f_red][k % 3]
		_ball(root, 0.014, Vector3.ONE, em, Vector3(cos(a) * 0.26, 0.34 + sin(a * 2.0) * 0.06, sin(a) * 0.26))
	# overall heat-haze radiance dome
	_radiance(root, 0.30, _glow(Color(1.0, 0.45, 0.15), 0.5, 0.18), Vector3(0, 0.24, 0))
	return root
