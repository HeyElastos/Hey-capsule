class_name VerseAvatar
extends Node3D
## A cute chibi-robot avatar built from primitives (no model files needed).
## If res://assets/models/avatar.glb exists it is loaded instead — same node,
## same API — so swapping in real art later needs zero code changes.
##
## API: setup(color, name, remote) · say(text) · set_remote_state(pos, yaw, moving)
## Local avatars are moved by their controller (home.gd); remote ones glide
## toward the last state received from the network.

const MODEL_PATH := "res://assets/models/avatar.glb"
const OUTLINE_SHADER := preload("res://outline.gdshader")
const TOON_SHADER := preload("res://toon.gdshader")
const SPEED := 3.5

# ── avatar traits (collection-style, but live 3D) ───────────────────────────
# Every DID derives a unique default combination; each slot is editable and,
# later, mintable as marketplace items. body × eyes × fins × visor × accent.
const BODY_COLORS: Array[Color] = [
	Color(0.95, 0.95, 0.97),   # arctic white
	Color(0.99, 0.92, 0.84),   # warm cream
	Color(0.86, 0.96, 0.90),   # mint
	Color(0.99, 0.88, 0.92),   # blush
	Color(0.87, 0.92, 0.99),   # sky
]
const VISOR_COLORS: Array[Color] = [
	Color(0.05, 0.06, 0.10),   # midnight
	Color(0.10, 0.05, 0.12),   # plum
	Color(0.03, 0.10, 0.11),   # deep teal
	Color(0.10, 0.07, 0.05),   # cocoa
]
const EYE_STYLES := 3          # 0 oval · 1 round · 2 happy
const FIN_STYLES := 4          # 0 blade · 1 round ears · 2 tall · 3 none

static var _outline_mat: ShaderMaterial


## Deterministic unique look per DID — no two robots default the same.
static func derive_traits(did: String) -> Dictionary:
	var h := absi(hash(did))
	return {
		# everyone defaults to the classic white suit (other body tones are
		# an editor choice); eyes/ears/visor + accent vary per DID
		"body": 0,
		"eyes": (h / 37) % EYE_STYLES,
		"fins": (h / 151) % 2,   # standard or tall ears by default, never bare
		"visor": (h / 631) % VISOR_COLORS.size(),
	}


## DID defaults with the user's saved/sent choices layered on top.
static func resolve_outfit(did: String, overlay: Dictionary) -> Dictionary:
	var t := derive_traits(did)
	for k in overlay.keys():
		t[k] = overlay[k]
	return t

var base_color := Color(0.5, 0.71, 1.0)
var display_name := ""
var moving := false
# Dress-up: {"hat": "cap"|"tophat"|"crown"|"sprout", "accent": Color}. Applied
# in setup(); later this rides the presence gossip so visitors see your look.
var outfit: Dictionary = {}

var _remote := false
var _t := 0.0
var _blink_t := 0.0
var _next_blink := 3.0
var _bubble_until_ms := 0
var _target_pos := Vector3.ZERO
var _target_yaw := 0.0

var _mesh: Node3D
var _arm_l: Node3D
var _arm_r: Node3D
var _leg_l: Node3D
var _leg_r: Node3D
var _antenna: Node3D
var _eye_l: MeshInstance3D
var _eye_r: MeshInstance3D
var _eye_base_y := 1.3
var _bubble: Label3D
var _name_label: Label3D
var _hat_root: Node3D
var _shadow: MeshInstance3D
var _accent_mat: ShaderMaterial
var _accent_mat_light: ShaderMaterial   # the faded upper logo layer


## Re-dress in place: tear down the body and rebuild with new traits
## (used by the avatar editor; name tag, bubble and position survive).
func rebuild(new_outfit: Dictionary) -> void:
	outfit = new_outfit
	if _mesh:
		_mesh.queue_free()
	_mesh = null
	_arm_l = null
	_arm_r = null
	_leg_l = null
	_leg_r = null
	_antenna = null
	_eye_l = null
	_eye_r = null
	_hat_root = null
	_accent_mat = null
	_accent_mat_light = null
	_build_primitive()
	if outfit.has("accent"):
		set_accent_color(outfit["accent"])
	if outfit.has("hat"):
		set_hat(str(outfit["hat"]))


## One stylized cel material — the whole cartoon look comes from here.
## Backed by toon.gdshader: soft two-band ramp with warm-light/cool-shadow
## tinting, height gradient, rim, optional spec dot and wind sway; plus an
## inverted-hull outline as next_pass. Base pass only — phone-cheap.
static func toon_mat(c: Color, rim := 0.35, outline := true, wind := 0.0, wind_h := 0.5, spec := 0.0) -> ShaderMaterial:
	var m := ShaderMaterial.new()
	m.shader = TOON_SHADER
	m.set_shader_parameter("albedo", c)
	m.set_shader_parameter("rim_strength", rim)
	m.set_shader_parameter("spec_strength", spec)
	m.set_shader_parameter("wind_strength", wind)
	m.set_shader_parameter("wind_height", wind_h)
	if outline:
		if _outline_mat == null:
			_outline_mat = ShaderMaterial.new()
			_outline_mat.shader = OUTLINE_SHADER
		m.next_pass = _outline_mat
	return m


static func glow_mat(c: Color, energy := 1.2) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = c
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.emission_enabled = true
	m.emission = c
	m.emission_energy_multiplier = energy
	return m


func setup(color: Color, name_: String, remote := false, outfit_: Dictionary = {}) -> void:
	base_color = color
	display_name = name_
	_remote = remote
	outfit = outfit_
	_target_pos = position
	_target_yaw = rotation.y
	if ResourceLoader.exists(MODEL_PATH):
		_mesh = Node3D.new()
		add_child(_mesh)
		var packed: PackedScene = load(MODEL_PATH)
		_mesh.add_child(packed.instantiate())
	else:
		_build_primitive()
	_build_labels()
	if outfit.has("accent"):
		set_accent_color(outfit["accent"])
	if outfit.has("hat"):
		set_hat(outfit["hat"])


## Live recolor of the chevron markings (the DID/identity accent).
func set_accent_color(c: Color) -> void:
	if _accent_mat:
		_accent_mat.set_shader_parameter("albedo", c)
	if _accent_mat_light:
		_accent_mat_light.set_shader_parameter("albedo", c.lightened(0.55))


## Swap headwear. Cheap primitive hats for now; real .glb hats slot in later.
func set_hat(id: String) -> void:
	if _hat_root == null:
		return
	for child in _hat_root.get_children():
		child.queue_free()
	match id:
		"cap":
			_ball(_hat_root, 0.26, Vector3(1, 0.5, 1), toon_mat(base_color, 0.3), Vector3(0, 0.05, 0))
			var brim := CylinderMesh.new()
			brim.top_radius = 0.2
			brim.bottom_radius = 0.2
			brim.height = 0.03
			brim.radial_segments = 12
			var bmi := MeshInstance3D.new()
			bmi.mesh = brim
			bmi.material_override = toon_mat(base_color.darkened(0.2), 0.2)
			bmi.position = Vector3(0, 0.02, 0.22)
			_hat_root.add_child(bmi)
		"tophat":
			var navy := toon_mat(Color(0.13, 0.15, 0.21), 0.25)
			var brim2 := CylinderMesh.new()
			brim2.top_radius = 0.3
			brim2.bottom_radius = 0.3
			brim2.height = 0.04
			brim2.radial_segments = 14
			var b2 := MeshInstance3D.new()
			b2.mesh = brim2
			b2.material_override = navy
			_hat_root.add_child(b2)
			var crown_c := CylinderMesh.new()
			crown_c.top_radius = 0.18
			crown_c.bottom_radius = 0.18
			crown_c.height = 0.3
			crown_c.radial_segments = 14
			var c2 := MeshInstance3D.new()
			c2.mesh = crown_c
			c2.material_override = navy
			c2.position = Vector3(0, 0.17, 0)
			_hat_root.add_child(c2)
			var band := CylinderMesh.new()
			band.top_radius = 0.185
			band.bottom_radius = 0.185
			band.height = 0.06
			band.radial_segments = 14
			var band_mi := MeshInstance3D.new()
			band_mi.mesh = band
			band_mi.material_override = toon_mat(base_color, 0.3)
			band_mi.position = Vector3(0, 0.07, 0)
			_hat_root.add_child(band_mi)
		"crown":
			var gold := glow_mat(Color(1.0, 0.84, 0.42), 0.7)
			var ring := CylinderMesh.new()
			ring.top_radius = 0.19
			ring.bottom_radius = 0.21
			ring.height = 0.1
			ring.radial_segments = 12
			var rmi := MeshInstance3D.new()
			rmi.mesh = ring
			rmi.material_override = gold
			_hat_root.add_child(rmi)
			var spike := CylinderMesh.new()
			spike.top_radius = 0.0
			spike.bottom_radius = 0.04
			spike.height = 0.12
			spike.radial_segments = 6
			for k in 5:
				var ang := TAU * k / 5.0
				var smi2 := MeshInstance3D.new()
				smi2.mesh = spike
				smi2.material_override = gold
				smi2.position = Vector3(cos(ang) * 0.18, 0.1, sin(ang) * 0.18)
				_hat_root.add_child(smi2)
		"sprout":
			var stem := CylinderMesh.new()
			stem.top_radius = 0.02
			stem.bottom_radius = 0.02
			stem.height = 0.14
			stem.radial_segments = 6
			var st := MeshInstance3D.new()
			st.mesh = stem
			st.material_override = toon_mat(Color(0.36, 0.64, 0.32), 0.2)
			st.position = Vector3(0, 0.07, 0)
			_hat_root.add_child(st)
			_ball(_hat_root, 0.07, Vector3(1.4, 0.5, 0.8), toon_mat(Color(0.42, 0.7, 0.36), 0.3), Vector3(-0.06, 0.16, 0))
			_ball(_hat_root, 0.07, Vector3(1.4, 0.5, 0.8), toon_mat(Color(0.42, 0.7, 0.36), 0.3), Vector3(0.06, 0.16, 0))
		_:
			pass


func say(text: String) -> void:
	_bubble.text = text
	_bubble.visible = true
	_bubble_until_ms = Time.get_ticks_msec() + 4200


func set_remote_state(pos: Vector3, yaw: float, is_moving: bool) -> void:
	_target_pos = pos
	_target_yaw = yaw
	moving = is_moving


## Cute one-shots for the start podium (and later, an emote wheel).
var _emote := ""
var _emote_t := 0.0
func emote(kind: String) -> void:
	_emote = kind
	_emote_t = 1.3 if kind == "wave" else (0.7 if kind == "spin" else 0.6)


## Sitting (benches): legs fold forward, gentle breathing, no walk anim.
var _sitting := false
func is_sitting() -> bool:
	return _sitting


func sit(seat_pos: Vector3, yaw: float) -> void:
	_sitting = true
	moving = false
	position = Vector3(seat_pos.x, 0.06, seat_pos.z)
	rotation.y = yaw


func stand() -> void:
	if not _sitting:
		return
	_sitting = false
	position.y = 0.0


# ------------------------------------------------------------------ building

func _box_part(parent: Node3D, size: Vector3, mat: Material, pos: Vector3) -> MeshInstance3D:
	var bm := BoxMesh.new()
	bm.size = size
	var mi := MeshInstance3D.new()
	mi.mesh = bm
	mi.material_override = mat
	mi.position = pos
	parent.add_child(mi)
	return mi


func _cyl_part(parent: Node3D, r: float, h: float, mat: Material, pos: Vector3, rot_x := 0.0) -> MeshInstance3D:
	var cm2 := CylinderMesh.new()
	cm2.top_radius = r
	cm2.bottom_radius = r
	cm2.height = h
	cm2.radial_segments = 16
	var mi := MeshInstance3D.new()
	mi.mesh = cm2
	mi.material_override = mat
	mi.position = pos
	mi.rotation.x = rot_x
	parent.add_child(mi)
	return mi


func _ball(parent: Node3D, r: float, s: Vector3, mat: Material, pos: Vector3) -> MeshInstance3D:
	var sm := SphereMesh.new()
	sm.radius = r
	sm.height = r * 2.0
	sm.radial_segments = 24
	sm.rings = 12
	var mi := MeshInstance3D.new()
	mi.mesh = sm
	mi.material_override = mat
	mi.scale = s
	mi.position = pos
	parent.add_child(mi)
	return mi


func _build_primitive() -> void:
	# faint contact blob (real sun shadows do the rest) — once, survives rebuilds
	if _shadow == null:
		var disc := CylinderMesh.new()
		disc.top_radius = 0.4
		disc.bottom_radius = 0.4
		disc.height = 0.02
		disc.radial_segments = 18
		var sh_mat := StandardMaterial3D.new()
		sh_mat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
		sh_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
		sh_mat.albedo_color = Color(0, 0, 0, 0.13)
		_shadow = MeshInstance3D.new()
		_shadow.mesh = disc
		_shadow.material_override = sh_mat
		_shadow.position.y = 0.02
		_shadow.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		add_child(_shadow)

	_mesh = Node3D.new()
	add_child(_mesh)
	# White mascot robot with navy accents; the chevrons carry your DID color
	# so everyone's robot is the same cute species but recognisably theirs.
	var body_col: Color = BODY_COLORS[int(outfit.get("body", 0)) % BODY_COLORS.size()]
	var visor_col: Color = VISOR_COLORS[int(outfit.get("visor", 0)) % VISOR_COLORS.size()]
	# ELAnaut finish: glossy white suit, deep navy details, glowing cyan face
	var white := toon_mat(body_col, 0.45, true, 0.0, 0.5, 0.7)
	var navy := toon_mat(Color(0.10, 0.12, 0.17), 0.3, true, 0.0, 0.5, 0.5)
	var accent := toon_mat(base_color, 0.4, true, 0.0, 0.5, 0.6)
	var visor_mat := toon_mat(visor_col, 0.2, true, 0.0, 0.5, 0.75)
	_accent_mat = accent

	# --- torso: a smooth rounded chest (capsule), like the sheets — wide at
	# the chest, tucking in at the waist, small dark pelvis under it
	var chest := CapsuleMesh.new()
	chest.radius = 0.30
	chest.height = 0.74
	chest.radial_segments = 24
	chest.rings = 12
	var tmi := MeshInstance3D.new()
	tmi.mesh = chest
	tmi.material_override = white
	tmi.scale = Vector3(1, 1, 0.88)
	tmi.position = Vector3(0, 0.84, 0)
	_mesh.add_child(tmi)
	_ball(_mesh, 0.21, Vector3(1, 0.6, 1), navy, Vector3(0, 0.50, 0))         # pelvis

	# --- REAL legs: hip pivots that swing in a walk cycle (white thigh, navy boot)
	_leg_l = _make_leg(navy, white, -1.0)
	_leg_r = _make_leg(navy, white, 1.0)

	# --- neck joint
	var neck := CylinderMesh.new()
	neck.top_radius = 0.12
	neck.bottom_radius = 0.14
	neck.height = 0.16
	neck.radial_segments = 16
	var nmi := MeshInstance3D.new()
	nmi.mesh = neck
	nmi.material_override = navy
	nmi.position = Vector3(0, 1.18, 0)
	_mesh.add_child(nmi)

	# --- head: a rounded-square TV head, like the sheets — a box core with
	# rounded vertical edges (no more pill)
	var hw := 0.94
	var hh := 0.66
	var hd := 0.64
	var cr := 0.15
	_box_part(_mesh, Vector3(hw - 2.0 * cr, hh, hd), white, Vector3(0, 1.48, 0))
	_box_part(_mesh, Vector3(hw, hh, hd - 2.0 * cr), white, Vector3(0, 1.48, 0))
	for cx in [-1.0, 1.0]:
		for cz in [-1.0, 1.0]:
			var px: float = cx
			var pz: float = cz
			_cyl_part(_mesh, cr, hh, white, Vector3(px * (hw * 0.5 - cr), 1.48, pz * (hd * 0.5 - cr)))

	# --- the face screen: a dark rounded-RECTANGLE panel on the front
	var sw := 0.84
	var sh2 := 0.56
	var sr := 0.13
	var sz := 0.335
	_box_part(_mesh, Vector3(sw - 2.0 * sr, sh2, 0.05), visor_mat, Vector3(0, 1.46, sz))
	_box_part(_mesh, Vector3(sw, sh2 - 2.0 * sr, 0.05), visor_mat, Vector3(0, 1.46, sz))
	for cx in [-1.0, 1.0]:
		for cy in [-1.0, 1.0]:
			var px2: float = cx
			var py2: float = cy
			_cyl_part(_mesh, sr, 0.05, visor_mat,
				Vector3(px2 * (sw * 0.5 - sr), 1.46 + py2 * (sh2 * 0.5 - sr), sz), PI / 2.0)


	# eyes (they blink) + a little smile on the screen — style is a trait
	var eye_style := int(outfit.get("eyes", 0)) % EYE_STYLES
	var esc := Vector3(1.1, 1.0, 0.45)      # 0 · round (the ELAnaut default)
	var ey := 1.47
	if eye_style == 1:                      # 1 · tall oval
		esc = Vector3(1, 1.3, 0.45)
	elif eye_style == 2:                    # 2 · happy squint
		esc = Vector3(1.3, 0.5, 0.45)
		ey = 1.50
	_eye_base_y = esc.y
	# LED eyes — glowing cyan on the dark screen, whatever the suit color
	var led := Color(0.42, 0.83, 1.0)
	_eye_l = _ball(_mesh, 0.10, esc, glow_mat(led, 1.9), Vector3(-0.16, ey, 0.355))
	_eye_r = _ball(_mesh, 0.10, esc, glow_mat(led, 1.9), Vector3(0.16, ey, 0.355))
	var smile := BoxMesh.new()
	smile.size = Vector3(0.17, 0.028, 0.02)
	var smi := MeshInstance3D.new()
	smi.mesh = smile
	smi.material_override = glow_mat(Color(0.42, 0.83, 1.0), 1.0)
	smi.position = Vector3(0, 1.30, 0.355)
	_mesh.add_child(smi)

	# --- the marks: ONE chevron on the forehead; the CHEST wears the layered
	# wide-chevron mark — a faded upper layer over a solid lower one, drawn in
	# your accent (the Elastos-style layered V)
	_accent_mat_light = toon_mat(base_color.lightened(0.55), 0.3, true, 0.0, 0.5, 0.5)
	var chev := BoxMesh.new()
	chev.size = Vector3(0.16, 0.045, 0.025)
	for side in [-1.0, 1.0]:
		var s2: float = side
		var c := MeshInstance3D.new()
		c.mesh = chev
		c.material_override = accent
		c.position = Vector3(s2 * 0.066, 1.775, 0.325)
		c.rotation.z = s2 * 0.42
		_mesh.add_child(c)
	var chev_b := BoxMesh.new()
	chev_b.size = Vector3(0.20, 0.055, 0.025)
	for row in 2:
		for side in [-1.0, 1.0]:
			var s3: float = side
			var c2 := MeshInstance3D.new()
			c2.mesh = chev_b
			c2.material_override = _accent_mat_light if row == 0 else accent
			c2.position = Vector3(s3 * 0.082, 0.99 - row * 0.10, 0.265 + row * 0.008)
			c2.rotation.z = s3 * 0.42
			_mesh.add_child(c2)

	# --- dark arms + big mitten hands
	_arm_l = _make_arm(navy, navy, -1.0)
	_arm_r = _make_arm(navy, navy, 1.0)

	# --- hat anchor on the crown (dress-up system)
	_hat_root = Node3D.new()
	_hat_root.position = Vector3(0, 1.83, 0)
	_mesh.add_child(_hat_root)

	# --- ear fins: the style is a trait (blade / round ears / tall / none);
	# they wag with the antenna sway either way
	_antenna = Node3D.new()
	_antenna.position = Vector3(0, 1.42, 0)
	_mesh.add_child(_antenna)
	# antenna in the middle of the head: 0 zigzag · 1 straight · 2 tall zigzag
	# · 3 none — a thin dark stick with a small glowing ball on top
	var ant_style := int(outfit.get("fins", 0)) % FIN_STYLES
	if ant_style != 3:
		var glow_tip := glow_mat(base_color.lightened(0.3), 1.6)
		if ant_style == 1:
			var rod2 := CylinderMesh.new()
			rod2.top_radius = 0.018
			rod2.bottom_radius = 0.018
			rod2.height = 0.26
			rod2.radial_segments = 8
			var st := MeshInstance3D.new()
			st.mesh = rod2
			st.material_override = navy
			st.position = Vector3(0, 0.52, 0)
			_antenna.add_child(st)
			_ball(_antenna, 0.05, Vector3.ONE, glow_tip, Vector3(0, 0.69, 0))
		else:
			var seg := CylinderMesh.new()
			seg.top_radius = 0.018
			seg.bottom_radius = 0.018
			seg.height = 0.14
			seg.radial_segments = 8
			var n_seg := 2 if ant_style == 0 else 3
			var bx := 0.0
			var by := 0.39
			for i in n_seg:
				var sgn := 1.0 if (i % 2 == 0) else -1.0
				var dx := sin(0.6) * 0.14 * sgn
				var dy := cos(0.6) * 0.14
				var sm2 := MeshInstance3D.new()
				sm2.mesh = seg
				sm2.material_override = navy
				sm2.position = Vector3(bx + dx * 0.5, by + dy * 0.5, 0)
				sm2.rotation.z = -0.6 * sgn
				_antenna.add_child(sm2)
				bx += dx
				by += dy
			_ball(_antenna, 0.05, Vector3.ONE, glow_tip, Vector3(bx, by + 0.04, 0))


func _make_arm(mat: Material, hand_mat: Material, side: float) -> Node3D:
	var arm_pivot := Node3D.new()
	arm_pivot.position = Vector3(side * 0.40, 1.0, 0)
	_mesh.add_child(arm_pivot)
	# white shoulder section over the dark arm, like the sheets
	var sj := SphereMesh.new()
	sj.radius = 0.095
	sj.height = 0.19
	sj.radial_segments = 16
	sj.rings = 8
	var sjm := MeshInstance3D.new()
	sjm.mesh = sj
	sjm.material_override = toon_mat(Color(0.95, 0.95, 0.97), 0.4, true, 0.0, 0.5, 0.7)
	arm_pivot.add_child(sjm)
	var cm := CapsuleMesh.new()
	cm.radius = 0.085
	cm.height = 0.38
	cm.radial_segments = 16
	cm.rings = 6
	var mi := MeshInstance3D.new()
	mi.mesh = cm
	mi.material_override = mat
	mi.position.y = -0.15
	arm_pivot.add_child(mi)
	var hand := SphereMesh.new()
	hand.radius = 0.12
	hand.height = 0.24
	hand.radial_segments = 10
	hand.rings = 5
	var hm := MeshInstance3D.new()
	hm.mesh = hand
	hm.material_override = hand_mat
	hm.position.y = -0.4
	arm_pivot.add_child(hm)
	return arm_pivot


func _make_leg(boot_mat: Material, thigh_mat: Material, side: float) -> Node3D:
	var hip := Node3D.new()
	hip.position = Vector3(side * 0.14, 0.56, 0)
	_mesh.add_child(hip)
	var th := CapsuleMesh.new()
	th.radius = 0.09
	th.height = 0.34
	th.radial_segments = 14
	th.rings = 4
	var tmi2 := MeshInstance3D.new()
	tmi2.mesh = th
	tmi2.material_override = thigh_mat
	tmi2.position.y = -0.22
	hip.add_child(tmi2)
	var boot := SphereMesh.new()
	boot.radius = 0.125
	boot.height = 0.25
	boot.radial_segments = 20
	boot.rings = 10
	var bmi := MeshInstance3D.new()
	bmi.mesh = boot
	bmi.material_override = boot_mat
	bmi.scale = Vector3(1.05, 0.6, 1.4)
	bmi.position = Vector3(0, -0.49, 0.03)
	hip.add_child(bmi)
	return hip


## Live rename (the Hey nickname arrives async at boot).
func set_display_name(n: String) -> void:
	display_name = n
	if _name_label:
		_name_label.text = n


func _build_labels() -> void:
	var name_l := Label3D.new()
	_name_label = name_l
	name_l.text = display_name
	name_l.position.y = 2.18
	name_l.font_size = 40
	name_l.pixel_size = 0.011
	name_l.outline_size = 10
	name_l.modulate = Color(1, 1, 1)
	name_l.outline_modulate = Color(0.04, 0.07, 0.13, 0.9)
	name_l.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	name_l.no_depth_test = true
	add_child(name_l)

	_bubble = Label3D.new()
	_bubble.position.y = 2.6
	_bubble.font_size = 44
	_bubble.pixel_size = 0.011
	_bubble.outline_size = 14
	_bubble.modulate = Color(1, 1, 1)
	_bubble.outline_modulate = Color(0.04, 0.07, 0.13, 1.0)
	_bubble.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	_bubble.no_depth_test = true
	_bubble.autowrap_mode = TextServer.AUTOWRAP_WORD
	_bubble.width = 300.0
	_bubble.visible = false
	add_child(_bubble)


# ------------------------------------------------------------------ animation

func _process(delta: float) -> void:
	_t += delta
	if _remote:
		var to := _target_pos - position
		to.y = 0.0
		var d := to.length()
		if d > 0.04:
			position += to.normalized() * minf(d, SPEED * delta * 1.15)
		rotation.y = lerp_angle(rotation.y, _target_yaw, 10.0 * delta)
	if _mesh == null:
		return
	if _sitting:
		_mesh.position.y = sin(_t * 1.6) * 0.012
		_mesh.scale.y = lerpf(_mesh.scale.y, 1.0, 8.0 * delta)
		if _leg_l:
			_leg_l.rotation.x = lerpf(_leg_l.rotation.x, -1.5, 10.0 * delta)
			_leg_r.rotation.x = lerpf(_leg_r.rotation.x, -1.5, 10.0 * delta)
		if _arm_l:
			_arm_l.rotation.x = lerpf(_arm_l.rotation.x, -0.25, 8.0 * delta)
			_arm_r.rotation.x = lerpf(_arm_r.rotation.x, -0.25, 8.0 * delta)
	elif moving:
		_mesh.position.y = absf(sin(_t * 7.0)) * 0.045
		_mesh.scale.y = 1.0 + sin(_t * 14.0) * 0.03
		if _arm_l:
			_arm_l.rotation.x = sin(_t * 9.0) * 0.65
			_arm_r.rotation.x = -sin(_t * 9.0) * 0.65
			_antenna.rotation.z = sin(_t * 9.0) * 0.16
		if _leg_l:
			_leg_l.rotation.x = -sin(_t * 9.0) * 0.5
			_leg_r.rotation.x = sin(_t * 9.0) * 0.5
	else:
		_mesh.position.y = sin(_t * 2.4) * 0.035
		_mesh.scale.y = lerpf(_mesh.scale.y, 1.0, 8.0 * delta)
		if _arm_l:
			_arm_l.rotation.x = lerpf(_arm_l.rotation.x, 0.0, 8.0 * delta)
			_arm_r.rotation.x = lerpf(_arm_r.rotation.x, 0.0, 8.0 * delta)
			_antenna.rotation.z = sin(_t * 1.8) * 0.05
		if _leg_l:
			_leg_l.rotation.x = lerpf(_leg_l.rotation.x, 0.0, 8.0 * delta)
			_leg_r.rotation.x = lerpf(_leg_r.rotation.x, 0.0, 8.0 * delta)
	# emotes (wave / hop / spin) override the idle pose briefly
	if _emote != "":
		_emote_t -= delta
		match _emote:
			"wave":
				if _arm_r:
					_arm_r.rotation.x = -2.4
					_arm_r.rotation.z = sin(_t * 14.0) * 0.45
			"hop":
				_mesh.position.y = absf(sin((1.0 - _emote_t / 0.6) * PI)) * 0.22
			"spin":
				rotation.y += delta * TAU / 0.7
		if _emote_t <= 0.0:
			if _arm_r:
				_arm_r.rotation.z = 0.0
			_emote = ""
	# blink
	if _eye_l:
		_blink_t += delta
		if _blink_t > _next_blink + 0.12:
			_blink_t = 0.0
			_next_blink = randf_range(2.2, 4.8)
			_eye_l.scale.y = _eye_base_y
			_eye_r.scale.y = _eye_base_y
		elif _blink_t > _next_blink:
			_eye_l.scale.y = 0.15
			_eye_r.scale.y = 0.15
	if _bubble and _bubble.visible and Time.get_ticks_msec() > _bubble_until_ms:
		_bubble.visible = false
