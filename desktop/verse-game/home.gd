extends Node3D
## Hey Verse — the home scene.
##
## Point-and-go controls: tap (or hold-drag) the ground and your avatar walks
## there. Fixed overhead-tilt follow camera. The world is built procedurally
## in _build_world(); tall grass and flowers are MultiMesh (one draw call
## each) and every material is a toon StandardMaterial3D with an inverted-
## hull outline, so the look is rich while staying phone-friendly. One
## directional shadow + light glow are the only "expensive" features, both
## standard on today's phones.
##
## Multiplayer flows through the Net autoload (net.gd): we send our movement
## at <=15 Hz and mirror whatever peers() reports as remote avatars. Chat from
## the HUD goes out via Net and shows as a bubble above heads both ways.

const TICK_MOVE := 1.0 / 15.0
const TICK_IDLE := 1.0
const WORLD_MODEL := "res://assets/models/world.glb"
const WATER_SHADER := preload("res://water.gdshader")
const SKY_SHADER := preload("res://sky.gdshader")
const VIGNETTE_SHADER := preload("res://vignette.gdshader")

# You start in the outside world; walking up to your house door fades you
# into the big home room, where ONLY the home is visible (dark calm surround,
# no outside). Stepping on the door mat fades you back out to the yard.
# HOME_ONLY=true would skip the yard and boot straight into the room.
const HOME_ONLY := false
const INTERIOR := Vector3(60, 0, 0)
const EXIT_MAT := Vector3(60, 0, 5.15)   # the door mat at the room's front edge

# The home manifest — outfit + placed furniture, persisted locally (the user's
# "Live Drive" record). Later this exact JSON is what gets DID-signed and
# served to visitors, and what marketplace (.ddrm) item records get added to.
const SAVE_PATH := "user://home.json"

# Avatar-first boot: your robot on a podium in the dark — edit it or enter.
const PODIUM := Vector3(-60, 0, 0)

# Wall slots for NFT paintings (left wall; two per floor). Hung via the app's
# Library sheet — any ESC NFT becomes a framed picture, saved in the manifest.
const PAINT_SLOTS := [
	{"pos": Vector3(-7.33, 1.85, 1.3), "up": false},
	{"pos": Vector3(-7.33, 1.85, 3.3), "up": false},
	{"pos": Vector3(-7.33, 4.45, -2.8), "up": true},
	{"pos": Vector3(-7.33, 4.45, -0.6), "up": true},
]

const PRESETS: Array[Dictionary] = [
	{
		"name": "Day",
		"sun_color": Color(1.0, 0.95, 0.86), "sun_energy": 0.6, "sun_rot": Vector3(-50, -42, 0),
		"sky_top": Color(0.28, 0.52, 0.8), "sky_hor": Color(0.65, 0.76, 0.72),
		"ground_hor": Color(0.65, 0.76, 0.72), "ground_bot": Color(0.42, 0.58, 0.44),
		"fog_color": Color(0.68, 0.78, 0.86), "fog_density": 0.008,
		"ambient": Color(0.64, 0.7, 0.8), "ambient_energy": 0.12,
		"lamp_energy": 0.0, "bulb_energy": 0.25,
		"glow": 0.08, "window_energy": 0.0, "fireflies": false, "exposure": 0.62,
		"cloud_col": Color(1.0, 1.0, 1.0), "cloud_amount": 0.42, "stars": 0.0,
	},
	{
		"name": "Sunset",
		"sun_color": Color(1.0, 0.58, 0.38), "sun_energy": 0.62, "sun_rot": Vector3(-20, -65, 0),
		"sky_top": Color(0.34, 0.38, 0.66), "sky_hor": Color(0.88, 0.58, 0.4),
		"ground_hor": Color(0.88, 0.58, 0.4), "ground_bot": Color(0.42, 0.34, 0.3),
		"fog_color": Color(0.88, 0.66, 0.5), "fog_density": 0.018,
		"ambient": Color(0.84, 0.64, 0.55), "ambient_energy": 0.18,
		"lamp_energy": 1.2, "bulb_energy": 1.5,
		"glow": 0.3, "window_energy": 1.3, "fireflies": true, "exposure": 0.75,
		"cloud_col": Color(1.0, 0.74, 0.58), "cloud_amount": 0.34, "stars": 0.25,
	},
	{
		"name": "Night",
		"sun_color": Color(0.5, 0.62, 1.0), "sun_energy": 0.2, "sun_rot": Vector3(-65, 30, 0),
		"sky_top": Color(0.04, 0.07, 0.16), "sky_hor": Color(0.12, 0.18, 0.32),
		"ground_hor": Color(0.12, 0.18, 0.32), "ground_bot": Color(0.05, 0.09, 0.15),
		"fog_color": Color(0.08, 0.12, 0.22), "fog_density": 0.022,
		"ambient": Color(0.3, 0.4, 0.6), "ambient_energy": 0.16,
		"lamp_energy": 2.6, "bulb_energy": 2.5,
		"glow": 0.7, "window_energy": 2.2, "fireflies": true, "exposure": 1.0,
		"cloud_col": Color(0.25, 0.30, 0.45), "cloud_amount": 0.16, "stars": 1.0,
	},
]

@export var move_speed := 3.5
@export var turn_speed := 9.0

var player: VerseAvatar
var _target: Vector3
var _moving := false
var _pressing := false
var _press_pos := Vector2.ZERO
var _dragged := false
var _t := 0.0
var _net_accum := 0.0
var _remotes: Dictionary = {}      # did -> VerseAvatar
var _lamps: Array = []             # {light: OmniLight3D, mat: StandardMaterial3D}
var _windows: Array = []           # StandardMaterial3D — glow at dusk/night
var _loft_root: Node3D             # everything on floor 2 (cutaway-hidden downstairs)
var _room: Node3D                  # the interior root (paintings hang here)
var _paintings: Array = []         # cached image paths, in slot order
var _water_hl: MeshInstance3D
var _water_mat: ShaderMaterial
var _pond_c := Vector3(7.5, 0, 5.5)
var _clouds: Node3D
var _fireflies: CPUParticles3D
var _hud: CanvasLayer
var _preset_idx := 0
# No physics engine (stripped from the slim build) — solid things are simple
# push-out shapes: circles (pond, trees) and boxes (houses).
var _obstacles: Array = []   # {pos: Vector3, r: float}
var _boxes: Array = []       # {pos: Vector3, half: Vector2}
var _manifest: Dictionary = {"outfit": {}, "furniture": []}
var _hat_idx := 0
var _accent_idx := -1
var _start: CanvasLayer
var _started := false
var _home_music: Node = null
var _spawn_accum := 5.0
var _emote_accum := 2.2
var _sign_pos := Vector3.ZERO
var _benches: Array = []           # {pos: Vector3, yaw: float} sittable seats
var _sit_bench: Dictionary = {}    # pending: sit down on arrival
var _inside := false
var _doors: Array = []             # Vector3 trigger points in front of doors
var _entry_door := Vector3.ZERO
var _door_cd := 0.0

@onready var pivot: Node3D = $CameraPivot
@onready var cam: Camera3D = $CameraPivot/Camera3D
@onready var marker: Node3D = $Marker
@onready var sun: DirectionalLight3D = $Sun
@onready var ground: MeshInstance3D = $Ground
@onready var env: Environment = $WorldEnvironment.environment
var sky_mat: ShaderMaterial


func _ready() -> void:
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.glow_enabled = true
	env.glow_intensity = 0.25
	# the stylized sky (sun disc + halo, drifting clouds, stars at night)
	sky_mat = ShaderMaterial.new()
	sky_mat.shader = SKY_SHADER
	env.sky.sky_material = sky_mat
	# Phones: bloom is several full-res passes on the Compatibility renderer —
	# a real frame-time hit on mobile GPUs. Emissive surfaces stay bright;
	# they just don't halo. (MSAA + shadow size are also lowered via the
	# .mobile overrides in project.godot.)
	if OS.has_feature("mobile"):
		env.glow_enabled = false
	cam.position = Vector3(0.0, 8.5, 10.5)
	cam.rotation_degrees = Vector3(-37.0, 0.0, 0.0)
	marker.visible = false
	marker.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# one real shadow — the single biggest "this looks like a game" upgrade
	sun.shadow_enabled = true
	sun.directional_shadow_mode = DirectionalLight3D.SHADOW_ORTHOGONAL
	sun.directional_shadow_max_distance = 55.0
	sun.shadow_blur = 1.4   # soft shadow edges (desktop PCF; mobile stays hard)

	_setup_ground()
	seed(20260609)
	_build_world()

	_inside = HOME_ONLY
	_load_manifest()
	player = VerseAvatar.new()
	add_child(player)
	player.position = Vector3(0, 0, 1.5)
	# manifest stores JSON-safe values; convert to engine types for setup()
	var saved: Dictionary = _manifest.get("outfit", {})
	var outfit: Dictionary = {}
	if saved.has("hat"):
		outfit["hat"] = str(saved["hat"])
	if saved.has("accent"):
		outfit["accent"] = Color(str(saved["accent"]))
	for k in saved.keys():
		if not outfit.has(k):
			outfit[k] = saved[k]
	player.setup(Net.local_color(), Net.local_name(), false,
		VerseAvatar.resolve_outfit(Net.local_did(), outfit))
	_hat_idx = maxi(0, VerseItems.HATS.find(str(outfit.get("hat", ""))))
	_target = player.position

	# subtle vignette under the UI — reads as color grading, costs nothing
	var vig := CanvasLayer.new()
	vig.layer = 5
	add_child(vig)
	var vrect := ColorRect.new()
	vrect.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	vrect.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var vsm := ShaderMaterial.new()
	vsm.shader = VIGNETTE_SHADER
	vrect.material = vsm
	vig.add_child(vrect)

	_hud = preload("res://hud.gd").new()
	add_child(_hud)
	_hud.chat_submitted.connect(_on_chat)
	_hud.preset_pressed.connect(func() -> void: _apply_preset(_preset_idx + 1))
	_hud.hat_pressed.connect(_on_hat)
	_hud.set_hat_name(VerseItems.HATS[_hat_idx])
	_hud.avatar_pressed.connect(_goto_podium)
	_hud.invite_pressed.connect(func() -> void:
		_hud.show_picker(Net.contacts(), _remotes.keys()))
	_hud.invite_contact.connect(func(did: String) -> void: Net.invite(did))
	Net.peer_chat.connect(_on_peer_chat)
	_apply_preset(0)

	# boot on the podium: avatar alone in the dark, HUD hidden until you enter
	_start = preload("res://start.gd").new()
	add_child(_start)
	_start.enter_world.connect(_on_enter_world)
	_start.hat_cycle.connect(func() -> void:
		_on_hat()
		_start.set_hat_label(VerseItems.HATS[_hat_idx]))
	_start.accent_cycle.connect(_on_accent)
	_start.body_cycle.connect(func() -> void: _cycle_trait("body", VerseAvatar.BODY_COLORS.size()))
	_start.eyes_cycle.connect(func() -> void: _cycle_trait("eyes", VerseAvatar.EYE_STYLES))
	_start.fins_cycle.connect(func() -> void: _cycle_trait("fins", VerseAvatar.FIN_STYLES))
	_start.set_hat_label(VerseItems.HATS[_hat_idx])

	# Boot straight into the world — at the saved spot if there is one (the
	# podium remains reachable via the desktop ··· Avatar editor; in the app
	# the dock's Avatar sheet does the editing).
	var sp: Dictionary = _manifest.get("spawn", {})
	_start.visible = false
	_start.stop_music()
	_started = true
	_hud.visible = true
	cam.position = Vector3(0.0, 8.5, 10.5)
	cam.rotation_degrees = Vector3(-37.0, 0.0, 0.0)
	var back := Vector3(float(sp.get("x", 0.0)), float(sp.get("y", 0.0)), float(sp.get("z", 1.5)))
	_teleport(back, float(sp.get("yaw", 0.0)))
	player.position.y = float(sp.get("y", 0.0))
	_ensure_home_music()

	# app integration: the dock sheets drive the game; identity binds in when
	# the runtime delivers it (async at boot)
	Net.ui_cmd.connect(_on_ui_cmd)
	Net.me_updated.connect(_on_me_updated)
	if Net.is_app():
		_hud.set_app_mode()
	Net.notify_ready()


## Mottled grass via a built-in noise texture — no asset files, kills the
## "flat green void" look. Generated threaded; pops in right after boot.
func _setup_ground() -> void:
	var noise := FastNoiseLite.new()
	noise.noise_type = FastNoiseLite.TYPE_SIMPLEX
	noise.frequency = 0.012
	var ramp := Gradient.new()
	ramp.set_color(0, Color(0.4, 0.66, 0.33))
	ramp.set_color(1, Color(0.5, 0.78, 0.42))
	var tex := NoiseTexture2D.new()
	tex.noise = noise
	tex.seamless = true
	tex.width = 256
	tex.height = 256
	tex.color_ramp = ramp
	var m := StandardMaterial3D.new()
	m.albedo_texture = tex
	m.uv1_scale = Vector3(26, 26, 1)
	m.roughness = 1.0
	m.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	m.specular_mode = BaseMaterial3D.SPECULAR_DISABLED
	ground.material_override = m


# ---------------------------------------------------------------- input + move

## Tap = walk to that spot. Hold-and-drag = follow the finger, and STOP the
## moment the finger lifts (no auto-walking to the last drag point).
func _unhandled_input(event: InputEvent) -> void:
	if not _started:
		return
	if event is InputEventMouseButton and event.button_index == MOUSE_BUTTON_LEFT:
		if event.pressed:
			_pressing = true
			_dragged = false
			_press_pos = event.position
			_walk_to_screen(event.position)
		else:
			_pressing = false
			if _dragged:
				_stop_walk()
	elif event is InputEventScreenTouch:
		if event.pressed:
			_pressing = true
			_dragged = false
			_press_pos = event.position
			_walk_to_screen(event.position)
		else:
			_pressing = false
			if _dragged:
				_stop_walk()
	elif event is InputEventScreenDrag and _pressing:
		if event.position.distance_to(_press_pos) > 14.0:
			_dragged = true
		_walk_to_screen(event.position)
	elif event is InputEventMouseMotion and _pressing and (event.button_mask & MOUSE_BUTTON_MASK_LEFT) != 0:
		if event.position.distance_to(_press_pos) > 14.0:
			_dragged = true
		_walk_to_screen(event.position)


func _stop_walk() -> void:
	_moving = false
	_target = player.position
	marker.visible = false


## Ray from the camera through the tap, intersected with the ground plane y=0.
func _walk_to_screen(screen_pos: Vector2) -> void:
	_hud.close_menu()
	var from := cam.project_ray_origin(screen_pos)
	var dir := cam.project_ray_normal(screen_pos)
	if absf(dir.y) < 0.0001:
		return
	var dist := -from.y / dir.y
	if dist <= 0.0:
		return
	_target = from + dir * dist
	if player.is_sitting():
		player.stand()
	_sit_bench = {}
	# Object picks happen in SCREEN space (the ground-plane target lands
	# BEHIND raised objects — tapping a seat back projected ~1m off):
	if not _dragged and not _inside:
		# the signpost: tap it to read it
		if _sign_pos != Vector3.ZERO:
			var ssp := cam.unproject_position(_sign_pos + Vector3(0, 1.0, 0))
			if ssp.distance_to(screen_pos) < 48.0 \
					or Vector2(_target.x - _sign_pos.x, _target.z - _sign_pos.z).length() < 0.9:
				_hud.show_sign()
				return
		# a bench: walk to its front edge, then sit down
		for b in _benches:
			var bd: Dictionary = b
			var bp: Vector3 = bd["pos"]
			var bsp := cam.unproject_position(bp + Vector3(0, 0.5, 0))
			if bsp.distance_to(screen_pos) < 55.0 \
					or Vector2(_target.x - bp.x, _target.z - bp.z).length() < 0.95:
				var ay: float = bd["yaw"]
				_sit_bench = bd
				_target = bp + Vector3(sin(ay), 0.0, cos(ay)) * 0.85
				_moving = true
				marker.global_position = _target + Vector3(0.0, 0.03, 0.0)
				marker.visible = true
				return
	_moving = true
	marker.global_position = _target + Vector3(0.0, 0.03, 0.0)
	marker.visible = true


func _process(delta: float) -> void:
	_t += delta

	# on the podium: the robot shows off — waves, hops, little spins
	if not _started and player:
		_emote_accum -= delta
		if _emote_accum <= 0.0:
			_emote_accum = randf_range(2.6, 5.0)
			player.emote(["wave", "hop", "spin"][randi() % 3])

	# local movement
	if _moving:
		var to := _target - player.position
		to.y = 0.0
		var d := to.length()
		if d < 0.08:
			_moving = false
			marker.visible = false
			# arrived at a bench: take a seat
			if not _sit_bench.is_empty():
				var bp2: Vector3 = _sit_bench["pos"]
				player.sit(bp2, float(_sit_bench["yaw"]))
				_sit_bench = {}
		else:
			var step := to.normalized() * move_speed * delta
			if step.length() >= d:
				player.position = Vector3(_target.x, 0.0, _target.z)
			else:
				player.position += step
			var want := atan2(to.x, to.z)
			player.rotation.y = lerp_angle(player.rotation.y, want, turn_speed * delta)
	player.moving = _moving

	# outdoors: keep the avatar out of houses, the pond and tree trunks
	# (paused while seated — the bench's own solid would push us off it)
	if not _inside and not player.is_sitting():
		player.position = _resolve_obstacles(player.position)

	# inside: solid walls + TWO full floors. The stairs strip ramps you up;
	# the upper floor is cutaway-hidden while you're downstairs, and the
	# camera rides up with you so only the active floor is in view.
	if _inside:
		if _loft_root:
			_loft_root.visible = player.position.y > 1.4
		player.position.x = clampf(player.position.x, INTERIOR.x - 7.0, INTERIOR.x + 7.0)
		var lp := player.position - INTERIOR
		var in_stairs := lp.x >= 5.35 and lp.z >= -1.75 and lp.z <= 2.1
		if in_stairs:
			player.position.y = clampf((1.8 - lp.z) / 3.2 * 2.9, 0.0, 2.9)
			player.position.z = clampf(player.position.z, INTERIOR.z - 5.05, INTERIOR.z + 5.2)
			# sticky: the staircase holds you in its strip until you're off
			# either end — diagonal walks can't slip out the side mid-ramp
			# (that's what snapped people back up to the loft)
			player.position.x = clampf(player.position.x, INTERIOR.x + 5.5, INTERIOR.x + 7.0)
		elif player.position.y > 1.45:
			# upstairs: the whole footprint, except the open stair corridor;
			# the railing has a gap at the top landing so the slab connects
			# straight onto the stairs
			player.position.y = 2.9
			player.position.z = clampf(player.position.z, INTERIOR.z - 5.05, INTERIOR.z + 5.2)
			if lp.z > -1.25:
				player.position.x = clampf(player.position.x, INTERIOR.x - 7.0, INTERIOR.x + 5.15)
		else:
			player.position.y = 0.0
			player.position.z = clampf(player.position.z, INTERIOR.z - 5.0, INTERIOR.z + 5.2)

	# doors: walk up to a door to step inside; stand on the mat to head out
	_door_cd = maxf(0.0, _door_cd - delta)
	if not HOME_ONLY and _door_cd == 0.0:
		if _inside:
			if player.position.distance_to(EXIT_MAT) < 0.7:
				_exit_house()
		else:
			for dpos in _doors:
				var d3: Vector3 = dpos
				if player.position.distance_to(d3) < 0.75:
					_enter_house(d3)
					break

	# movement lane: 15 Hz while walking, 1 Hz keepalive when idle
	_net_accum -= delta
	if _net_accum <= 0.0 and _started:
		_net_accum = TICK_MOVE if _moving else TICK_IDLE
		Net.send_move(player.position, player.rotation.y, _moving)

	# remember where you are (restored next time Verse opens)
	if _started:
		_spawn_accum -= delta
		if _spawn_accum <= 0.0:
			_spawn_accum = 5.0
			_save_spawn()

	# mirror peers as remote avatars
	var ps: Dictionary = Net.peers()
	for did in ps.keys():
		var p: Dictionary = ps[did]
		if not _remotes.has(did):
			var av := VerseAvatar.new()
			add_child(av)
			av.position = p["pos"]
			av.setup(p["color"], p["name"], true,
				VerseAvatar.resolve_outfit(str(did), p.get("outfit", {})))
			_remotes[did] = av
		var av2: VerseAvatar = _remotes[did]
		av2.set_remote_state(p["pos"], p["yaw"], p["moving"])
		if not _inside:
			av2.position = _resolve_obstacles(av2.position)
	var gone: Array = []
	for did in _remotes.keys():
		if not ps.has(did):
			gone.append(did)
	for did in gone:
		_remotes[did].queue_free()
		_remotes.erase(did)
	_hud.set_peer_count(ps.size() + 1)

	# camera trails the avatar (and rides up to the active floor)
	var follow := Vector3(player.position.x, player.position.y, player.position.z)
	pivot.global_position = pivot.global_position.lerp(follow, 0.12)

	# little life: marker pulse, pond sparkle, drifting clouds
	marker.scale = Vector3.ONE * (1.0 + sin(_t * 6.0) * 0.12)
	if _water_hl:
		_water_hl.position = _pond_c + Vector3(cos(_t * 0.45) * 0.7, 0.13, sin(_t * 0.45) * 0.45)
	if _clouds:
		_clouds.rotation.y += delta * 0.0045


# ---------------------------------------------------------------- chat + light

func _on_chat(text: String) -> void:
	player.say(text)
	Net.send_chat(text)


func _on_peer_chat(did: String, text: String) -> void:
	if _remotes.has(did):
		_remotes[did].say(text)


## Cycle headwear and persist it — the seed of the wardrobe. A marketplace
## (.ddrm) hat later is the same flow with an item record instead of an id.
func _on_hat() -> void:
	_hat_idx = (_hat_idx + 1) % VerseItems.HATS.size()
	var hat: String = VerseItems.HATS[_hat_idx]
	player.set_hat(hat)
	var outfit: Dictionary = _manifest.get("outfit", {})
	outfit["hat"] = hat
	_manifest["outfit"] = outfit
	_save_manifest()
	_hud.set_hat_name(hat)


func _load_manifest() -> void:
	if not FileAccess.file_exists(SAVE_PATH):
		return
	var raw := FileAccess.get_file_as_string(SAVE_PATH)
	var parsed: Variant = JSON.parse_string(raw)
	if parsed is Dictionary:
		_manifest = parsed


func _save_manifest() -> void:
	var f := FileAccess.open(SAVE_PATH, FileAccess.WRITE)
	if f:
		f.store_string(JSON.stringify(_manifest))


## Persist the current spot (position, facing, inside/outside) so the next
## Verse open resumes here instead of starting over.
func _save_spawn() -> void:
	if not _started:
		return
	_manifest["spawn"] = {
		"x": player.position.x, "y": player.position.y, "z": player.position.z,
		"yaw": player.rotation.y, "inside": _inside,
	}
	_save_manifest()


func _ensure_home_music() -> void:
	if _home_music != null:
		return
	_home_music = preload("res://music.gd").new()
	_home_music.mode = "home"
	add_child(_home_music)


## Commands queued by the Hey app's dock sheets (Avatar / Worlds / Exit).
func _on_ui_cmd(cmd: String) -> void:
	if cmd.begins_with("hang:"):
		_hang_painting(cmd.substr(5))
		return
	match cmd:
		"hat":
			_on_hat()
		"body":
			_cycle_trait("body", VerseAvatar.BODY_COLORS.size())
		"eyes":
			_cycle_trait("eyes", VerseAvatar.EYE_STYLES)
		"fins":
			_cycle_trait("fins", VerseAvatar.FIN_STYLES)
		"accent":
			_on_accent()
		"preset_day":
			_apply_preset(0)
		"preset_sunset":
			_apply_preset(1)
		"preset_night":
			_apply_preset(2)
		"save":
			_save_spawn()
		"sleep":
			_save_spawn()
			OS.low_processor_usage_mode = true
		"wake":
			OS.low_processor_usage_mode = false


## Hang an NFT image (downloaded by the app to a local file) as a framed
## painting on the next free wall slot; persisted in the manifest.
func _hang_painting(path: String) -> void:
	if _room == null:
		return
	if _add_painting_node(path, _paintings.size() % PAINT_SLOTS.size()):
		_paintings.append(path)
		_manifest["paintings"] = _paintings
		_save_manifest()


func _restore_paintings() -> void:
	_paintings = []
	for p in _manifest.get("paintings", []):
		var path := str(p)
		if _add_painting_node(path, _paintings.size() % PAINT_SLOTS.size()):
			_paintings.append(path)


func _add_painting_node(path: String, slot_i: int) -> bool:
	var bytes := FileAccess.get_file_as_bytes(path)
	if bytes.is_empty():
		return false
	var img := Image.new()
	if img.load_jpg_from_buffer(bytes) != OK \
			and img.load_png_from_buffer(bytes) != OK \
			and img.load_webp_from_buffer(bytes) != OK:
		return false
	var tex := ImageTexture.create_from_image(img)
	var slot: Dictionary = PAINT_SLOTS[slot_i]
	var parent: Node3D = _loft_root if bool(slot["up"]) else _room
	var root := Node3D.new()
	root.position = slot["pos"]
	root.rotation_degrees = Vector3(0, 90, 0)
	parent.add_child(root)
	var h := 1.05
	var w := clampf(h * float(img.get_width()) / float(maxi(img.get_height(), 1)), 0.6, 1.6)
	var frame := BoxMesh.new()
	frame.size = Vector3(w + 0.12, h + 0.12, 0.05)
	var fmi := MeshInstance3D.new()
	fmi.mesh = frame
	fmi.material_override = VerseAvatar.toon_mat(Color(0.45, 0.32, 0.2), 0.15)
	fmi.position.z = -0.02
	root.add_child(fmi)
	var quad := QuadMesh.new()
	quad.size = Vector2(w, h)
	var pm := StandardMaterial3D.new()
	pm.albedo_texture = tex
	pm.roughness = 0.9
	pm.diffuse_mode = BaseMaterial3D.DIFFUSE_TOON
	pm.specular_mode = BaseMaterial3D.SPECULAR_DISABLED
	var qmi := MeshInstance3D.new()
	qmi.mesh = quad
	qmi.material_override = pm
	qmi.position.z = 0.012
	root.add_child(qmi)
	return true


var _bound_did := ""
## The real Hey identity arrived: rebind the robot's look to the DID and wear
## the user's actual nickname.
func _on_me_updated(did: String, display: String) -> void:
	if did != _bound_did:
		_bound_did = did
		_rebuild_player_outfit()
	if display != "":
		player.set_display_name(display)


## Back to the podium (avatar editor) from the ··· menu.
func _goto_podium() -> void:
	if not _started:
		return
	_started = false
	player.stand()
	_moving = false
	marker.visible = false
	_hud.visible = false
	_hud.close_menu()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.02, 0.03, 0.06)
	env.fog_enabled = false
	player.position = PODIUM
	player.rotation.y = 0.0
	_target = PODIUM
	pivot.global_position = PODIUM
	cam.position = Vector3(0, 1.9, 3.4)
	cam.rotation_degrees = Vector3(-10, 0, 0)
	_start.visible = true


## Cycle a numbered avatar trait (body/eyes/fins), persist, rebuild in place.
func _cycle_trait(key: String, count: int) -> void:
	var saved: Dictionary = _manifest.get("outfit", {})
	var merged := VerseAvatar.resolve_outfit(Net.local_did(), saved)
	saved[key] = (int(merged.get(key, 0)) + 1) % count
	_manifest["outfit"] = saved
	_save_manifest()
	_rebuild_player_outfit()


func _rebuild_player_outfit() -> void:
	var saved: Dictionary = _manifest.get("outfit", {})
	var overlay: Dictionary = {}
	for k in saved.keys():
		overlay[k] = saved[k]
	if overlay.has("accent"):
		overlay["accent"] = Color(str(overlay["accent"]))
	player.rebuild(VerseAvatar.resolve_outfit(Net.local_did(), overlay))


## Cycle the chevron accent color and persist it.
func _on_accent() -> void:
	_accent_idx = (_accent_idx + 1) % Net.PALETTE.size()
	var c: Color = Net.PALETTE[_accent_idx]
	player.set_accent_color(c)
	var outfit: Dictionary = _manifest.get("outfit", {})
	outfit["accent"] = c.to_html(false)
	_manifest["outfit"] = outfit
	_save_manifest()


## Leave the podium: fade, restore the gameplay camera, spawn in the yard.
func _on_enter_world() -> void:
	if _started:
		return
	_start.stop_music()
	_start.fade(func() -> void:
		cam.position = Vector3(0.0, 8.5, 10.5)
		cam.rotation_degrees = Vector3(-37.0, 0.0, 0.0)
		_teleport(Vector3(0, 0, 1.5), 0.0)
		_started = true
		_hud.visible = true
		_start.visible = false
		# the warm home track takes over out in the world
		_ensure_home_music())


## The boot podium: a lit disc floating in the dark, far from the world.
func _build_podium(parent: Node3D) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = 2.2
	disc.bottom_radius = 2.6
	disc.height = 0.18
	disc.radial_segments = 24
	_mi(parent, disc, VerseAvatar.toon_mat(Color(0.16, 0.19, 0.27), 0.3, false), PODIUM + Vector3(0, -0.09, 0))
	var ring := CylinderMesh.new()
	ring.top_radius = 2.7
	ring.bottom_radius = 2.7
	ring.height = 0.04
	ring.radial_segments = 24
	var rmi := _mi(parent, ring, VerseAvatar.glow_mat(Color(0.5, 0.71, 1.0), 0.9), PODIUM + Vector3(0, -0.06, 0))
	rmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var key := OmniLight3D.new()
	key.position = PODIUM + Vector3(1.2, 2.4, 2.0)
	key.omni_range = 8.0
	key.light_color = Color(1.0, 0.92, 0.8)
	key.light_energy = 1.1
	parent.add_child(key)
	# gentle sparkles drifting up around the podium
	var sp := CPUParticles3D.new()
	sp.position = PODIUM + Vector3(0, 0.3, 0)
	sp.amount = 16
	sp.lifetime = 3.5
	sp.preprocess = 3.0
	sp.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	sp.emission_box_extents = Vector3(2.2, 0.2, 2.2)
	sp.direction = Vector3(0, 1, 0)
	sp.spread = 10.0
	sp.gravity = Vector3(0, 0.12, 0)
	sp.initial_velocity_min = 0.15
	sp.initial_velocity_max = 0.35
	sp.scale_amount_min = 0.5
	sp.scale_amount_max = 1.0
	var dot := SphereMesh.new()
	dot.radius = 0.03
	dot.height = 0.06
	dot.radial_segments = 6
	dot.rings = 3
	dot.material = VerseAvatar.glow_mat(Color(1.0, 0.86, 0.5), 1.6)
	sp.mesh = dot
	parent.add_child(sp)


## Spawn furniture recorded in the manifest into the room. Marketplace (.ddrm)
## items resolve through VerseItems (decrypted .glb); builtins are built here.
func _spawn_saved_furniture(room: Node3D) -> void:
	for entry in _manifest.get("furniture", []):
		var e: Dictionary = entry
		var node := VerseItems.load_item_mesh(e)
		if node == null:
			node = _build_builtin_furniture(e)
		if node == null:
			continue
		var p: Array = e.get("pos", [0.0, 0.0, 0.0])
		node.position = Vector3(float(p[0]), float(p[1]), float(p[2]))
		node.rotation.y = float(e.get("rot_y", 0.0))
		room.add_child(node)


func _build_builtin_furniture(e: Dictionary) -> Node3D:
	var kind := str(e.get("builtin", ""))
	var root := Node3D.new()
	match kind:
		"cushion":
			var col := Color(str(e.get("color", "f0c64e")))
			var cu := CylinderMesh.new()
			cu.top_radius = 0.3
			cu.bottom_radius = 0.34
			cu.height = 0.14
			cu.radial_segments = 14
			var mi := MeshInstance3D.new()
			mi.mesh = cu
			mi.material_override = VerseAvatar.toon_mat(col, 0.25)
			mi.position.y = 0.07
			root.add_child(mi)
		"crate":
			var bx := BoxMesh.new()
			bx.size = Vector3(0.5, 0.5, 0.5)
			var mi2 := MeshInstance3D.new()
			mi2.mesh = bx
			mi2.material_override = VerseAvatar.toon_mat(Color(0.7, 0.52, 0.33), 0.15)
			mi2.position.y = 0.25
			root.add_child(mi2)
		_:
			return null
	return root


func _apply_preset(i: int) -> void:
	_preset_idx = i % PRESETS.size()
	var p: Dictionary = PRESETS[_preset_idx]
	sun.light_color = p["sun_color"]
	sun.light_energy = p["sun_energy"]
	sun.rotation_degrees = p["sun_rot"]
	sky_mat.set_shader_parameter("top", p["sky_top"])
	sky_mat.set_shader_parameter("horizon", p["sky_hor"])
	sky_mat.set_shader_parameter("ground", p["ground_bot"])
	sky_mat.set_shader_parameter("cloud_col", p["cloud_col"])
	sky_mat.set_shader_parameter("cloud_amount", p["cloud_amount"])
	sky_mat.set_shader_parameter("stars", p["stars"])
	if _water_mat:
		_water_mat.set_shader_parameter("sky_col", p["sky_hor"])
	env.fog_light_color = p["fog_color"]
	env.fog_density = p["fog_density"]
	env.ambient_light_color = p["ambient"]
	env.ambient_light_energy = p["ambient_energy"]
	env.glow_intensity = p["glow"]
	env.tonemap_exposure = p["exposure"]
	for entry in _lamps:
		var d: Dictionary = entry
		var light: OmniLight3D = d["light"]
		light.light_energy = p["lamp_energy"]
		light.visible = p["lamp_energy"] > 0.01
		var mat: StandardMaterial3D = d["mat"]
		mat.emission_energy_multiplier = p["bulb_energy"]
	for wm in _windows:
		var mat2: StandardMaterial3D = wm
		mat2.emission_energy_multiplier = p["window_energy"]
	if _fireflies:
		_fireflies.emitting = p["fireflies"]
		_fireflies.visible = p["fireflies"]
	if _hud:
		_hud.set_preset_name(p["name"])


# ---------------------------------------------------------------- world build

func _toon(c: Color, rim := 0.35, outline := true, wind := 0.0, wind_h := 0.5, spec := 0.0) -> ShaderMaterial:
	return VerseAvatar.toon_mat(c, rim, outline, wind, wind_h, spec)


## Window glass stays a StandardMaterial3D so the lighting presets can drive
## its warm emission (the cel shader has no emission channel).
func _glass_mat() -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.albedo_color = Color(0.7, 0.88, 1.0)
	m.roughness = 0.4
	m.emission_enabled = true
	m.emission = Color(1.0, 0.78, 0.45)
	m.emission_energy_multiplier = 0.0
	return m


func _mi(parent: Node3D, m: Mesh, mat: Material, pos: Vector3) -> MeshInstance3D:
	var n := MeshInstance3D.new()
	n.mesh = m
	n.material_override = mat
	n.position = pos
	parent.add_child(n)
	return n


## Push the point out of all registered solids (circles + boxes). One pass is
## enough at walk speeds.
func _resolve_obstacles(p: Vector3, radius := 0.3) -> Vector3:
	for o in _obstacles:
		var od: Dictionary = o
		var c: Vector3 = od["pos"]
		var rr: float = od["r"] + radius
		var d := Vector2(p.x - c.x, p.z - c.z)
		var l := d.length()
		if l < rr and l > 0.001:
			var push := d.normalized() * (rr - l)
			p.x += push.x
			p.z += push.y
	for b in _boxes:
		var bd: Dictionary = b
		var c2: Vector3 = bd["pos"]
		var h: Vector2 = bd["half"]
		var ex: float = h.x + radius
		var ez: float = h.y + radius
		var dx: float = p.x - c2.x
		var dz: float = p.z - c2.z
		if absf(dx) < ex and absf(dz) < ez:
			if ex - absf(dx) < ez - absf(dz):
				p.x = c2.x + (ex if dx >= 0.0 else -ex)
			else:
				p.z = c2.z + (ez if dz >= 0.0 else -ez)
	return p


## Soft dark disc under big props — fake contact occlusion, grounds everything.
func _contact(parent: Node3D, r: float, pos: Vector3) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = r
	disc.bottom_radius = r
	disc.height = 0.015
	disc.radial_segments = 16
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = Color(0, 0, 0, 0.14)
	var mi := _mi(parent, disc, m, Vector3(pos.x, 0.012, pos.z))
	mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


func _build_world() -> void:
	var w: Node3D = $World
	# If a modeled world exists, use it and skip the procedural props —
	# the placeholder -> real-art swap needs zero code changes.
	_build_podium(w)
	if ResourceLoader.exists(WORLD_MODEL):
		var packed: PackedScene = load(WORLD_MODEL)
		w.add_child(packed.instantiate())
		return
	if HOME_ONLY:
		# Just your home on a lawn: the big room + scenery dressing around it.
		_add_hills(w)
		_tree(w, Vector3(-10.5, 0, -2.0), 1.15, 0)
		_tree(w, Vector3(10.0, 0, -4.5), 1.0, 2)
		_tree(w, Vector3(-9.5, 0, 4.5), 0.95, 1)
		_tree(w, Vector3(11.0, 0, 3.0), 1.2, 0)
		_tree(w, Vector3(2.0, 0, -9.5), 1.1, 2)
		_tree(w, Vector3(-3.5, 0, -10.0), 1.0, 0)
		_bush(w, Vector3(-8.6, 0, 1.0), 1.1)
		_bush(w, Vector3(8.8, 0, -1.5), 1.0)
		_bush(w, Vector3(9.2, 0, 5.0), 0.9)
		_bush(w, Vector3(-8.4, 0, -4.5), 1.0)
		_add_fireflies(w)
		_build_interior(w)
		return
	_add_hills(w)
	_add_patches(w)
	_add_path(w)
	_add_grass(w)
	_add_flowers(w)
	_add_trees(w)
	_add_bushes(w)
	_add_fences(w)
	_add_pond(w)
	_house(w, Vector3(-3.6, 0, -2.6), Color(0.86, 0.46, 0.36))
	_signpost(w, Vector3(1.3, 0, 0.4))
	_bench(w, Vector3(2.6, 0, 3.4), 28.0)
	_bench(w, Vector3(-5.4, 0, 4.2), -24.0)
	_lamp(w, Vector3(1.8, 0, 5.2))
	_lamp(w, Vector3(-2.6, 0, 1.0))
	_lamp(w, Vector3(6.2, 0, -2.6))
	_add_fireflies(w)
	_build_interior(w)


## Rolling hills ringing the playfield — depth and a real horizon.
func _add_hills(parent: Node3D) -> void:
	var spots := [
		[Vector3(0, 0, -34), 16.0, 0.3, Color(0.36, 0.6, 0.4)],
		[Vector3(-24, 0, -26), 12.0, 0.34, Color(0.4, 0.64, 0.42)],
		[Vector3(24, 0, -27), 13.0, 0.3, Color(0.34, 0.58, 0.4)],
		[Vector3(-34, 0, -6), 14.0, 0.26, Color(0.38, 0.6, 0.44)],
		[Vector3(35, 0, -4), 15.0, 0.28, Color(0.36, 0.58, 0.42)],
		[Vector3(-30, 0, 16), 12.0, 0.3, Color(0.42, 0.66, 0.44)],
		[Vector3(31, 0, 17), 12.0, 0.32, Color(0.4, 0.64, 0.42)],
		[Vector3(-14, 0, 30), 11.0, 0.26, Color(0.44, 0.68, 0.46)],
		[Vector3(15, 0, 31), 12.0, 0.28, Color(0.42, 0.66, 0.44)],
		[Vector3(0, 0, 38), 14.0, 0.24, Color(0.4, 0.64, 0.42)],
	]
	for s in spots:
		var sm := SphereMesh.new()
		sm.radius = s[1]
		sm.height = s[1] * 2.0
		sm.radial_segments = 14
		sm.rings = 8
		var mi := _mi(parent, sm, _toon(s[3], 0.15, false), s[0])
		mi.scale = Vector3(1.0, s[2], 1.0)
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## Puffy clouds slowly circling high above (whole ring rotates in _process).
func _add_clouds(parent: Node3D) -> void:
	_clouds = Node3D.new()
	parent.add_child(_clouds)
	var m := StandardMaterial3D.new()
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	m.albedo_color = Color(1, 1, 1, 0.92)
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	var spots := [
		[Vector3(18, 26, -20), 1.4], [Vector3(-25, 30, -10), 1.8],
		[Vector3(28, 28, 12), 1.5], [Vector3(-15, 25, 24), 1.2],
		[Vector3(2, 32, -32), 2.0], [Vector3(-32, 27, 5), 1.4],
	]
	for s in spots:
		var c := Node3D.new()
		c.position = s[0]
		c.scale = Vector3.ONE * s[1]
		_clouds.add_child(c)
		var puffs := [
			[Vector3.ZERO, 2.6], [Vector3(2.1, -0.3, 0.4), 1.9],
			[Vector3(-2.0, -0.4, -0.3), 1.7], [Vector3(0.6, 0.9, 0.2), 1.6],
		]
		for q in puffs:
			var sm := SphereMesh.new()
			sm.radius = q[1]
			sm.height = q[1] * 1.4
			sm.radial_segments = 10
			sm.rings = 5
			var mi := _mi(c, sm, m, q[0])
			mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


func _add_patches(parent: Node3D) -> void:
	var spots := [
		[Vector3(3, 0, 6), 4.2, Color(0.41, 0.69, 0.34)],
		[Vector3(-6, 0, -6), 3.4, Color(0.47, 0.75, 0.4)],
		[Vector3(-2, 0, 8), 2.8, Color(0.47, 0.75, 0.4)],
		[Vector3(8, 0, 0.5), 3.8, Color(0.41, 0.69, 0.34)],
		[Vector3(-9, 0, 6), 3.0, Color(0.41, 0.69, 0.34)],
		[Vector3(4, 0, -7), 3.2, Color(0.47, 0.75, 0.4)],
	]
	for s in spots:
		var disc := CylinderMesh.new()
		disc.top_radius = s[1]
		disc.bottom_radius = s[1]
		disc.height = 0.012
		disc.radial_segments = 20
		var mi := _mi(parent, disc, _toon(s[2], 0.0, false), Vector3(s[0].x, 0.008, s[0].z))
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


func _add_path(parent: Node3D) -> void:
	var disc := CylinderMesh.new()
	disc.top_radius = 0.9
	disc.bottom_radius = 0.9
	disc.height = 0.04
	disc.radial_segments = 14
	var mat := _toon(Color(0.80, 0.66, 0.42), 0.1, false)
	var pebble := SphereMesh.new()
	pebble.radius = 0.09
	pebble.height = 0.18
	pebble.radial_segments = 8
	pebble.rings = 4
	var pmat := _toon(Color(0.62, 0.6, 0.58), 0.1, false)
	var pts := [
		Vector3(0.4, 0, 8.5), Vector3(0.2, 0, 7.0), Vector3(-0.2, 0, 5.5),
		Vector3(-0.6, 0, 4.0), Vector3(-1.4, 0, 2.6), Vector3(-2.4, 0, 1.4),
		Vector3(-3.2, 0, 0.2), Vector3(-3.6, 0, -1.1),
	]
	for p in pts:
		var mi := _mi(parent, disc, mat, Vector3(p.x, 0.025, p.z))
		mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# a couple of pebbles at the rim of each step
		for k in 2:
			var ang := randf() * TAU
			var pb := _mi(parent, pebble, pmat, Vector3(p.x + cos(ang) * 0.95, 0.05, p.z + sin(ang) * 0.95))
			pb.scale = Vector3.ONE * randf_range(0.6, 1.1)
			pb.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


func _add_grass(parent: Node3D) -> void:
	# tall-grass patches: a dark tile + two MultiMeshes of blades (2 draw calls)
	var tile := CylinderMesh.new()
	tile.top_radius = 2.2
	tile.bottom_radius = 2.2
	tile.height = 0.03
	tile.radial_segments = 18
	var tile_mat := _toon(Color(0.24, 0.46, 0.22), 0.0, false)

	var centers := [
		Vector3(5.5, 0, 4.5), Vector3(-6.0, 0, 5.0),
		Vector3(7.0, 0, -1.5), Vector3(-7.0, 0, -2.0), Vector3(0.5, 0, 8.5),
	]
	var greens := [Color(0.31, 0.61, 0.27), Color(0.38, 0.69, 0.31)]
	for gi in 2:
		# real blades: thin tapered prisms (pointed tips), leaning naturally
		var blade := PrismMesh.new()
		blade.size = Vector3(0.035, 0.42, 0.014)
		var mm := MultiMesh.new()
		mm.transform_format = MultiMesh.TRANSFORM_3D
		mm.mesh = blade
		var xforms: Array[Transform3D] = []
		for c in centers:
			if gi == 0:
				var tmi := _mi(parent, tile, tile_mat, Vector3(c.x, 0.02, c.z))
				tmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			for i in 72:
				var off := Vector3(randf_range(-2.0, 2.0), 0.0, randf_range(-2.0, 2.0))
				var p: Vector3 = c + off
				p.y = 0.2
				var b := Basis(Vector3.UP, randf() * TAU)
				b = b.rotated(Vector3.RIGHT, randf_range(-0.25, 0.25))
				b = b.scaled(Vector3(1.0, randf_range(0.6, 1.3), 1.0))
				xforms.append(Transform3D(b, p))
		mm.instance_count = xforms.size()
		for i in xforms.size():
			mm.set_instance_transform(i, xforms[i])
		var mmi := MultiMeshInstance3D.new()
		mmi.multimesh = mm
		mmi.material_override = _toon(greens[gi], 0.15, false, 1.0, 0.55)
		mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		parent.add_child(mmi)


## Real little flowers: stem + a ring of FIVE petals + a warm center.
## Three MultiMeshes per color = 3 draw calls; petals read perfectly from
## the overhead camera.
func _flower_field(parent: Node3D, col: Color, n: int) -> void:
	var positions: Array[Vector3] = []
	for i in n:
		positions.append(Vector3(randf_range(-11.0, 11.0), 0.0, randf_range(-9.0, 11.0)))

	var stem := CylinderMesh.new()
	stem.top_radius = 0.014
	stem.bottom_radius = 0.018
	stem.height = 0.26
	stem.radial_segments = 6
	var mm_s := MultiMesh.new()
	mm_s.transform_format = MultiMesh.TRANSFORM_3D
	mm_s.mesh = stem
	mm_s.instance_count = n
	for i in n:
		mm_s.set_instance_transform(i, Transform3D(Basis.IDENTITY, positions[i] + Vector3(0, 0.13, 0)))
	var mmi_s := MultiMeshInstance3D.new()
	mmi_s.multimesh = mm_s
	mmi_s.material_override = _toon(Color(0.32, 0.58, 0.28), 0.1, false, 0.7, 0.28)
	mmi_s.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mmi_s)

	var petal := SphereMesh.new()
	petal.radius = 0.042
	petal.height = 0.084
	petal.radial_segments = 8
	petal.rings = 4
	var mm_p := MultiMesh.new()
	mm_p.transform_format = MultiMesh.TRANSFORM_3D
	mm_p.mesh = petal
	mm_p.instance_count = n * 5
	for i in n:
		var spin := randf() * TAU
		for k in 5:
			var ang := spin + TAU * float(k) / 5.0
			var b := Basis(Vector3.UP, ang)
			b = b.scaled(Vector3(1.6, 0.35, 0.95))
			var off := Vector3(cos(ang) * 0.062, 0.27, -sin(ang) * 0.062)
			mm_p.set_instance_transform(i * 5 + k, Transform3D(b, positions[i] + off))
	var mmi_p := MultiMeshInstance3D.new()
	mmi_p.multimesh = mm_p
	mmi_p.material_override = _toon(col, 0.3, false, 0.7, 0.3)
	mmi_p.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mmi_p)

	var center := SphereMesh.new()
	center.radius = 0.034
	center.height = 0.068
	center.radial_segments = 8
	center.rings = 4
	var mm_c := MultiMesh.new()
	mm_c.transform_format = MultiMesh.TRANSFORM_3D
	mm_c.mesh = center
	mm_c.instance_count = n
	for i in n:
		var bc := Basis.IDENTITY.scaled(Vector3(1.0, 0.6, 1.0))
		mm_c.set_instance_transform(i, Transform3D(bc, positions[i] + Vector3(0, 0.285, 0)))
	var mmi_c := MultiMeshInstance3D.new()
	mmi_c.multimesh = mm_c
	mmi_c.material_override = _toon(Color(1.0, 0.78, 0.25), 0.3, false, 0.7, 0.3)
	mmi_c.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	parent.add_child(mmi_c)


func _add_flowers(parent: Node3D) -> void:
	_flower_field(parent, Color(1.0, 0.55, 0.8), 45)
	_flower_field(parent, Color(1.0, 0.85, 0.4), 45)
	_flower_field(parent, Color(0.95, 0.95, 1.0), 35)


## kind 0 = round oak, 1 = tall pine-ish, 2 = pink blossom
func _tree(parent: Node3D, pos: Vector3, s: float, kind: int) -> void:
	var t := Node3D.new()
	t.position = pos
	t.scale = Vector3.ONE * s
	parent.add_child(t)
	_contact(parent, 1.1 * s, pos)
	_obstacles.append({"pos": pos, "r": 0.38 * s})
	var trunk := CylinderMesh.new()
	trunk.top_radius = 0.16
	trunk.bottom_radius = 0.26
	trunk.height = 1.4
	trunk.radial_segments = 8
	_mi(t, trunk, _toon(Color(0.5, 0.34, 0.2)), Vector3(0, 0.7, 0))

	var leaf := Color(0.34, 0.64, 0.3)
	if kind == 2:
		leaf = Color(0.95, 0.6, 0.75)
	var lumps: Array = []
	if kind == 1:
		lumps = [
			[Vector3(0, 1.7, 0), 0.85], [Vector3(0, 2.4, 0), 0.68],
			[Vector3(0, 3.0, 0), 0.5], [Vector3(0, 3.45, 0), 0.32],
		]
	else:
		lumps = [
			[Vector3(0, 1.9, 0), 1.0], [Vector3(-0.55, 1.65, 0.2), 0.62],
			[Vector3(0.55, 1.7, -0.15), 0.66], [Vector3(0.1, 2.55, 0.05), 0.7],
			[Vector3(-0.3, 2.3, -0.4), 0.5],
		]
	var li := 0
	for l in lumps:
		var sm := SphereMesh.new()
		sm.radius = l[1]
		sm.height = l[1] * 2.0
		sm.radial_segments = 10
		sm.rings = 6
		var c := leaf.lightened(0.05 * li)
		_mi(t, sm, _toon(c, 0.35, true, 0.18, 1.2), l[0])
		li += 1
	# a few leaves drift down and tumble on the wind
	var leaves := CPUParticles3D.new()
	leaves.amount = 3
	leaves.lifetime = 5.5
	leaves.preprocess = 4.0
	leaves.position = Vector3(0, 2.1, 0)
	leaves.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	leaves.emission_box_extents = Vector3(0.9, 0.5, 0.9)
	leaves.direction = Vector3(0.4, -1.0, 0.15)
	leaves.spread = 25.0
	leaves.gravity = Vector3(0.18, -0.45, 0.06)
	leaves.initial_velocity_min = 0.05
	leaves.initial_velocity_max = 0.22
	leaves.angular_velocity_min = -120.0
	leaves.angular_velocity_max = 120.0
	leaves.scale_amount_min = 0.7
	leaves.scale_amount_max = 1.15
	var lq := BoxMesh.new()
	lq.size = Vector3(0.09, 0.012, 0.07)
	lq.material = VerseAvatar.toon_mat(leaf.lightened(0.12), 0.2, false)
	leaves.mesh = lq
	t.add_child(leaves)
	# little fruits / blossom dots on round trees
	if kind == 0 and randf() > 0.5:
		var fruit := SphereMesh.new()
		fruit.radius = 0.09
		fruit.height = 0.18
		fruit.radial_segments = 8
		fruit.rings = 4
		var fmat := _toon(Color(0.9, 0.3, 0.3), 0.4)
		for k in 3:
			var ang := randf() * TAU
			_mi(t, fruit, fmat, Vector3(cos(ang) * 0.8, randf_range(1.6, 2.4), sin(ang) * 0.8))


func _add_trees(parent: Node3D) -> void:
	var spots := [
		[Vector3(8.0, 0, 3.0), 1.2, 0], [Vector3(-8.5, 0, 2.0), 1.0, 1],
		[Vector3(9.0, 0, -4.0), 1.3, 0], [Vector3(-9.0, 0, -4.5), 1.1, 2],
		[Vector3(2.0, 0, -6.5), 1.0, 1], [Vector3(-2.5, 0, -7.0), 1.15, 0],
		[Vector3(6.5, 0, 7.5), 0.9, 2], [Vector3(-6.5, 0, 8.0), 1.05, 0],
		[Vector3(11.0, 0, 0.0), 1.25, 1], [Vector3(-11.5, 0, 0.5), 1.2, 0],
		[Vector3(10.5, 0, 8.0), 1.0, 0], [Vector3(-10.0, 0, 9.0), 0.95, 2],
		[Vector3(7.5, 0, -7.5), 1.1, 1], [Vector3(-7.0, 0, -8.0), 1.0, 0],
	]
	for s in spots:
		_tree(parent, s[0], s[1], s[2])


func _add_bushes(parent: Node3D) -> void:
	var spots := [
		[Vector3(1.2, 0, 6.8), 1.0], [Vector3(-4.6, 0, 6.6), 1.2],
		[Vector3(5.8, 0, 0.8), 0.9], [Vector3(-6.8, 0, 0.2), 1.1],
		[Vector3(3.2, 0, -5.2), 1.0], [Vector3(-0.8, 0, -5.8), 0.95],
		[Vector3(9.5, 0, 5.5), 1.2], [Vector3(-9.5, 0, 4.0), 1.0],
	]
	for s in spots:
		_bush(parent, s[0], s[1])


func _bush(parent: Node3D, pos: Vector3, s: float) -> void:
	var b := Node3D.new()
	b.position = pos
	b.scale = Vector3.ONE * s
	parent.add_child(b)
	var m1 := SphereMesh.new()
	m1.radius = 0.5
	m1.height = 1.0
	m1.radial_segments = 10
	m1.rings = 5
	_mi(b, m1, _toon(Color(0.3, 0.58, 0.27), 0.3, true, 0.15, 0.8), Vector3(0, 0.32, 0)).scale = Vector3(1, 0.7, 1)
	var m2 := SphereMesh.new()
	m2.radius = 0.34
	m2.height = 0.68
	m2.radial_segments = 10
	m2.rings = 5
	_mi(b, m2, _toon(Color(0.38, 0.66, 0.32), 0.3, true, 0.15, 0.8), Vector3(0.3, 0.4, 0.12)).scale = Vector3(1, 0.75, 1)


func _add_fences(parent: Node3D) -> void:
	var mat := _toon(Color(0.80, 0.64, 0.42), 0.1)
	var post := BoxMesh.new()
	post.size = Vector3(0.12, 0.7, 0.12)
	var rail := BoxMesh.new()
	rail.size = Vector3(1.0, 0.07, 0.07)
	for side in [-1.0, 1.0]:
		var x0: float = side * 2.2
		for k in 5:
			var z := 6.0 - k * 1.0
			var p := _mi(parent, post, mat, Vector3(x0, 0.35, z))
			p.rotation.y = randf_range(-0.06, 0.06)
			if k < 4:
				_mi(parent, rail, mat, Vector3(x0, 0.52, z - 0.5))
				_mi(parent, rail, mat, Vector3(x0, 0.28, z - 0.5))


func _add_pond(parent: Node3D) -> void:
	_contact(parent, 2.45, _pond_c)
	var rim := CylinderMesh.new()
	rim.top_radius = 2.3
	rim.bottom_radius = 2.3
	rim.height = 0.05
	rim.radial_segments = 22
	var rmi := _mi(parent, rim, _toon(Color(0.36, 0.6, 0.32), 0.0, false), Vector3(_pond_c.x, 0.03, _pond_c.z))
	rmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_obstacles.append({"pos": _pond_c, "r": 2.3})

	var water := CylinderMesh.new()
	water.top_radius = 2.05
	water.bottom_radius = 2.05
	water.height = 0.08
	water.radial_segments = 48
	_water_mat = ShaderMaterial.new()
	_water_mat.shader = WATER_SHADER
	var wmi := _mi(parent, water, _water_mat, Vector3(_pond_c.x, 0.06, _pond_c.z))
	wmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# rocks around the rim
	var rock := SphereMesh.new()
	rock.radius = 0.24
	rock.height = 0.48
	rock.radial_segments = 8
	rock.rings = 5
	var rock_mat := _toon(Color(0.58, 0.58, 0.6), 0.15)
	for k in 6:
		var ang := randf() * TAU
		var rr := 2.35 + randf_range(-0.05, 0.15)
		var rmi2 := _mi(parent, rock, rock_mat, Vector3(_pond_c.x + cos(ang) * rr, 0.1, _pond_c.z + sin(ang) * rr))
		rmi2.scale = Vector3(randf_range(0.7, 1.3), randf_range(0.5, 0.8), randf_range(0.7, 1.3))

	# lily pads + one tiny bloom
	var pad := CylinderMesh.new()
	pad.top_radius = 0.26
	pad.bottom_radius = 0.26
	pad.height = 0.03
	pad.radial_segments = 12
	var pad_mat := _toon(Color(0.28, 0.55, 0.3), 0.2, false)
	var pads := [Vector3(-0.7, 0, 0.4), Vector3(0.5, 0, -0.6), Vector3(0.9, 0, 0.8)]
	for pp in pads:
		var pmi := _mi(parent, pad, pad_mat, _pond_c + pp + Vector3(0, 0.11, 0))
		pmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bloom := SphereMesh.new()
	bloom.radius = 0.09
	bloom.height = 0.18
	bloom.radial_segments = 8
	bloom.rings = 4
	_mi(parent, bloom, _toon(Color(1.0, 0.7, 0.85), 0.4, false), _pond_c + Vector3(0.9, 0.17, 0.8))

	# drifting sparkle (animated in _process)
	var hl := SphereMesh.new()
	hl.radius = 0.28
	hl.height = 0.56
	hl.radial_segments = 10
	hl.rings = 5
	_water_hl = _mi(parent, hl, VerseAvatar.glow_mat(Color(1, 1, 1), 0.6), _pond_c + Vector3(0, 0.13, 0))
	_water_hl.scale = Vector3(1.0, 0.1, 0.5)
	_water_hl.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


func _house(parent: Node3D, pos: Vector3, roof: Color) -> void:
	var h := Node3D.new()
	h.position = pos
	parent.add_child(h)
	_contact(parent, 2.7, pos)
	_doors.append(pos + Vector3(0, 0, 2.05))
	_boxes.append({"pos": pos, "half": Vector2(1.95, 1.95)})

	# foundation skirt + walls — YOUR house, big enough for two floors inside
	var skirt := BoxMesh.new()
	skirt.size = Vector3(3.7, 0.2, 3.7)
	_mi(h, skirt, _toon(Color(0.72, 0.68, 0.6)), Vector3(0, 0.1, 0))
	var wall := BoxMesh.new()
	wall.size = Vector3(3.5, 2.6, 3.5)
	_mi(h, wall, _toon(Color(0.96, 0.93, 0.84)), Vector3(0, 1.3, 0))

	# eaves + roof + chimney
	var eaves := BoxMesh.new()
	eaves.size = Vector3(4.35, 0.14, 4.35)
	_mi(h, eaves, _toon(roof.darkened(0.25)), Vector3(0, 2.66, 0))
	var roof_m := PrismMesh.new()
	roof_m.size = Vector3(4.25, 1.9, 4.25)
	_mi(h, roof_m, _toon(roof), Vector3(0, 3.62, 0))
	var chimney := BoxMesh.new()
	chimney.size = Vector3(0.4, 0.9, 0.4)
	_mi(h, chimney, _toon(Color(0.62, 0.5, 0.45)), Vector3(1.1, 4.15, -0.8))
	var cap := BoxMesh.new()
	cap.size = Vector3(0.52, 0.12, 0.52)
	_mi(h, cap, _toon(Color(0.5, 0.4, 0.36)), Vector3(1.1, 4.66, -0.8))

	# chimney smoke — tiny CPU particles, pure cosiness
	var smoke := CPUParticles3D.new()
	smoke.position = Vector3(1.1, 4.74, -0.8)
	smoke.amount = 7
	smoke.lifetime = 2.8
	smoke.preprocess = 2.0
	smoke.direction = Vector3(0, 1, 0)
	smoke.spread = 8.0
	smoke.gravity = Vector3(0.15, 0.5, 0)
	smoke.initial_velocity_min = 0.3
	smoke.initial_velocity_max = 0.55
	smoke.scale_amount_min = 0.6
	smoke.scale_amount_max = 1.4
	var puff := SphereMesh.new()
	puff.radius = 0.14
	puff.height = 0.28
	puff.radial_segments = 8
	puff.rings = 4
	smoke.mesh = puff
	var smat := StandardMaterial3D.new()
	smat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	smat.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	smat.albedo_color = Color(1, 1, 1, 0.4)
	smat.vertex_color_use_as_albedo = true
	smoke.mesh.material = smat
	var ramp := Gradient.new()
	ramp.set_color(0, Color(1, 1, 1, 0.5))
	ramp.set_color(1, Color(1, 1, 1, 0.0))
	smoke.color_ramp = ramp
	h.add_child(smoke)

	# door with frame, step and knob
	var frame := BoxMesh.new()
	frame.size = Vector3(1.0, 1.66, 0.1)
	_mi(h, frame, _toon(Color(0.85, 0.8, 0.7)), Vector3(0, 0.83, 1.73))
	var door := BoxMesh.new()
	door.size = Vector3(0.86, 1.5, 0.12)
	_mi(h, door, _toon(Color(0.5, 0.33, 0.2)), Vector3(0, 0.75, 1.76))
	var knob := SphereMesh.new()
	knob.radius = 0.05
	knob.height = 0.1
	knob.radial_segments = 8
	knob.rings = 4
	_mi(h, knob, _toon(Color(0.95, 0.8, 0.35), 0.5), Vector3(0.29, 0.78, 1.84))
	var step := BoxMesh.new()
	step.size = Vector3(1.05, 0.12, 0.55)
	_mi(h, step, _toon(Color(0.7, 0.66, 0.58)), Vector3(0, 0.06, 1.98))

	# windows: frame + sill + warm glass that glows at dusk/night
	var wframe := BoxMesh.new()
	wframe.size = Vector3(0.8, 0.8, 0.1)
	var sill := BoxMesh.new()
	sill.size = Vector3(0.88, 0.08, 0.18)
	var glass := BoxMesh.new()
	glass.size = Vector3(0.66, 0.66, 0.12)
	for wx in [-1.15, 1.15]:
		_mi(h, wframe, _toon(Color(0.85, 0.8, 0.7)), Vector3(wx, 1.55, 1.73))
		_mi(h, sill, _toon(Color(0.8, 0.75, 0.65)), Vector3(wx, 1.1, 1.79))
		var gmat := _glass_mat()
		_windows.append(gmat)
		_mi(h, glass, gmat, Vector3(wx, 1.55, 1.76))


func _bench(parent: Node3D, pos: Vector3, yaw_deg: float) -> void:
	var b := Node3D.new()
	b.position = pos
	b.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(b)
	# solid (two circles along the seat) + registered as sittable
	var a := deg_to_rad(yaw_deg)
	for sxo in [-0.33, 0.33]:
		var off: float = sxo
		_obstacles.append({"pos": pos + Vector3(cos(a) * off, 0.0, -sin(a) * off), "r": 0.38})
	_benches.append({"pos": pos, "yaw": a})
	var wood := _toon(Color(0.72, 0.52, 0.33), 0.1)
	var seat := BoxMesh.new()
	seat.size = Vector3(1.3, 0.1, 0.42)
	_mi(b, seat, wood, Vector3(0, 0.42, 0))
	var back := BoxMesh.new()
	back.size = Vector3(1.3, 0.46, 0.08)
	_mi(b, back, wood, Vector3(0, 0.72, -0.19))
	var leg := BoxMesh.new()
	leg.size = Vector3(0.1, 0.42, 0.36)
	_mi(b, leg, wood, Vector3(-0.52, 0.21, 0))
	_mi(b, leg, wood, Vector3(0.52, 0.21, 0))


func _lamp(parent: Node3D, pos: Vector3) -> void:
	var l := Node3D.new()
	l.position = pos
	parent.add_child(l)
	var base := CylinderMesh.new()
	base.top_radius = 0.12
	base.bottom_radius = 0.16
	base.height = 0.12
	base.radial_segments = 10
	_mi(l, base, _toon(Color(0.2, 0.23, 0.28), 0.15), Vector3(0, 0.06, 0))
	var post := CylinderMesh.new()
	post.top_radius = 0.05
	post.bottom_radius = 0.07
	post.height = 2.1
	post.radial_segments = 8
	_mi(l, post, _toon(Color(0.23, 0.26, 0.31), 0.15), Vector3(0, 1.05, 0))
	var bulb_mat := VerseAvatar.glow_mat(Color(1.0, 0.85, 0.55), 0.25)
	var bulb := SphereMesh.new()
	bulb.radius = 0.14
	bulb.height = 0.28
	bulb.radial_segments = 12
	bulb.rings = 6
	_mi(l, bulb, bulb_mat, Vector3(0, 2.18, 0))
	# glass housing around the bulb
	var glass := SphereMesh.new()
	glass.radius = 0.2
	glass.height = 0.4
	glass.radial_segments = 12
	glass.rings = 6
	var gm := StandardMaterial3D.new()
	gm.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	gm.albedo_color = Color(1, 1, 1, 0.18)
	gm.roughness = 0.1
	var gmi := _mi(l, glass, gm, Vector3(0, 2.18, 0))
	gmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var light := OmniLight3D.new()
	light.position = Vector3(0, 2.1, 0)
	light.omni_range = 7.0
	light.light_color = Color(1.0, 0.82, 0.55)
	light.light_energy = 0.0
	l.add_child(light)
	_lamps.append({"light": light, "mat": bulb_mat})


func _signpost(parent: Node3D, pos: Vector3) -> void:
	var s := Node3D.new()
	s.position = pos
	s.rotation_degrees = Vector3(0, 24, 0)
	parent.add_child(s)
	var post := CylinderMesh.new()
	post.top_radius = 0.07
	post.bottom_radius = 0.07
	post.height = 1.0
	post.radial_segments = 8
	_mi(s, post, _toon(Color(0.55, 0.38, 0.22)), Vector3(0, 0.5, 0))
	var board := BoxMesh.new()
	board.size = Vector3(0.8, 0.45, 0.08)
	_mi(s, board, _toon(Color(0.78, 0.6, 0.38)), Vector3(0, 1.05, 0))
	_sign_pos = pos   # tap the sign to read it (popup)


## Fireflies — only out at dusk and night (toggled by the lighting preset).
func _add_fireflies(parent: Node3D) -> void:
	_fireflies = CPUParticles3D.new()
	_fireflies.position = Vector3(0, 1.0, 1.0)
	_fireflies.amount = 26
	_fireflies.lifetime = 5.0
	_fireflies.preprocess = 4.0
	_fireflies.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	_fireflies.emission_box_extents = Vector3(11.0, 1.0, 9.0)
	_fireflies.direction = Vector3(0, 0, 0)
	_fireflies.spread = 180.0
	_fireflies.gravity = Vector3.ZERO
	_fireflies.initial_velocity_min = 0.15
	_fireflies.initial_velocity_max = 0.45
	_fireflies.scale_amount_min = 0.6
	_fireflies.scale_amount_max = 1.0
	var dot := SphereMesh.new()
	dot.radius = 0.035
	dot.height = 0.07
	dot.radial_segments = 6
	dot.rings = 3
	dot.material = VerseAvatar.glow_mat(Color(0.95, 1.0, 0.55), 2.2)
	_fireflies.mesh = dot
	_fireflies.emitting = false
	_fireflies.visible = false
	parent.add_child(_fireflies)


# ---------------------------------------------------------------- the house

func _enter_house(door: Vector3) -> void:
	_entry_door = door
	_door_cd = 1.2
	_hud.fade(func() -> void: _teleport(INTERIOR + Vector3(0, 0, 4.2), PI))


func _exit_house() -> void:
	_door_cd = 1.2
	_hud.fade(func() -> void: _teleport(_entry_door + Vector3(0, 0, 1.1), 0.0))


func _teleport(pos: Vector3, yaw: float) -> void:
	_inside = pos.x > 30.0
	# Inside, ONLY the home exists: calm dark surround, no sky, no fog.
	if _inside:
		env.background_mode = Environment.BG_COLOR
		env.background_color = Color(0.02, 0.03, 0.06)
		env.fog_enabled = false
	else:
		env.background_mode = Environment.BG_SKY
		env.fog_enabled = true
	player.position = pos
	player.rotation.y = yaw
	_target = pos
	_moving = false
	marker.visible = false
	pivot.global_position = Vector3(pos.x, 0.0, pos.z)
	if _started:
		_save_spawn()


## Your home: one BIG room (15 x 11 m) with furniture along the edges and a
## large open middle — the canvas we'll decorate later.
func _build_interior(parent: Node3D) -> void:
	var r := Node3D.new()
	r.position = INTERIOR
	parent.add_child(r)
	_room = r

	# dark surround disc — from inside, the home floats in calm darkness
	var void_disc := CylinderMesh.new()
	void_disc.top_radius = 30.0
	void_disc.bottom_radius = 30.0
	void_disc.height = 0.1
	void_disc.radial_segments = 32
	var void_mat := StandardMaterial3D.new()
	void_mat.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	void_mat.albedo_color = Color(0.02, 0.03, 0.06)
	var vmi := _mi(r, void_disc, void_mat, Vector3(0, -0.15, 0))
	vmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# plank floor (sits just above the lawn)
	var floor_m := BoxMesh.new()
	floor_m.size = Vector3(15.0, 0.12, 11.0)
	_mi(r, floor_m, _toon(Color(0.78, 0.62, 0.42), 0.1), Vector3(0, -0.04, 0))
	var strip := BoxMesh.new()
	strip.size = Vector3(14.9, 0.012, 0.03)
	var strip_mat := _toon(Color(0.66, 0.5, 0.33), 0.0, false)
	for k in 10:
		var smi := _mi(r, strip, strip_mat, Vector3(0, 0.028, -4.5 + k * 1.0))
		smi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# walls (no front wall — the camera looks in from there); two storeys tall
	var wall_mat := _toon(Color(0.93, 0.88, 0.78), 0.15)
	var back := BoxMesh.new()
	back.size = Vector3(15.0, 5.4, 0.18)
	_mi(r, back, wall_mat, Vector3(0, 2.64, -5.45))
	var side := BoxMesh.new()
	side.size = Vector3(0.18, 5.4, 11.0)
	_mi(r, side, wall_mat, Vector3(-7.45, 2.64, 0))
	_mi(r, side, wall_mat, Vector3(7.45, 2.64, 0))

	# ── second floor: FULL-size upper floor with the stair corridor cut out
	# along the right wall. It lives under _loft_root so it can vanish while
	# you're downstairs (cutaway) and appear as you climb.
	_loft_root = Node3D.new()
	r.add_child(_loft_root)
	var floor2_mat := _toon(Color(0.72, 0.56, 0.38), 0.1)
	var slab_a := BoxMesh.new()
	slab_a.size = Vector3(12.8, 0.24, 11.0)
	_mi(_loft_root, slab_a, floor2_mat, Vector3(-1.1, 2.78, 0))
	var slab_c := BoxMesh.new()
	slab_c.size = Vector3(2.2, 0.24, 3.7)
	_mi(_loft_root, slab_c, floor2_mat, Vector3(6.4, 2.78, -3.65))
	# railing guarding the stair corridor
	var rail_wood := _toon(Color(0.55, 0.4, 0.26), 0.1)
	var railing := BoxMesh.new()
	railing.size = Vector3(0.07, 0.07, 6.6)
	_mi(_loft_root, railing, rail_wood, Vector3(5.34, 3.32, 2.1))
	var rail_post := BoxMesh.new()
	rail_post.size = Vector3(0.07, 0.85, 0.07)
	for k in 6:
		_mi(_loft_root, rail_post, rail_wood, Vector3(5.34, 2.95, -1.05 + k * 1.15))
	var step_col := _toon(Color(0.68, 0.52, 0.35), 0.1)
	for i in 10:
		var sb := BoxMesh.new()
		sb.size = Vector3(1.5, 0.29 * (i + 1), 0.34)
		_mi(r, sb, step_col, Vector3(6.2, 0.145 * (i + 1), 1.64 - i * 0.32))
	var board := BoxMesh.new()
	board.size = Vector3(14.9, 0.16, 0.06)
	_mi(r, board, _toon(Color(0.6, 0.45, 0.3), 0.0, false), Vector3(0, 0.1, -5.34))
	var board_s := BoxMesh.new()
	board_s.size = Vector3(0.06, 0.16, 10.9)
	_mi(r, board_s, _toon(Color(0.6, 0.45, 0.3), 0.0, false), Vector3(-7.34, 0.1, 0))
	_mi(r, board_s, _toon(Color(0.6, 0.45, 0.3), 0.0, false), Vector3(7.34, 0.1, 0))

	# three windows on the back wall — warm preset glow like the old cottages
	for wx in [-4.8, 0.0, 4.8]:
		var wframe := BoxMesh.new()
		wframe.size = Vector3(1.1, 1.1, 0.1)
		_mi(r, wframe, _toon(Color(0.85, 0.8, 0.7)), Vector3(wx, 1.62, -5.38))
		var glass := BoxMesh.new()
		glass.size = Vector3(0.94, 0.94, 0.08)
		var gmat := _glass_mat()
		_windows.append(gmat)
		_mi(r, glass, gmat, Vector3(wx, 1.62, -5.36))
		var bar := BoxMesh.new()
		bar.size = Vector3(0.94, 0.05, 0.09)
		_mi(r, bar, _toon(Color(0.85, 0.8, 0.7), 0.0, false), Vector3(wx, 1.62, -5.35))
		var bar2 := BoxMesh.new()
		bar2.size = Vector3(0.05, 0.94, 0.09)
		_mi(r, bar2, _toon(Color(0.85, 0.8, 0.7), 0.0, false), Vector3(wx, 1.62, -5.35))

	# pictures on the wall (later: your Hey Feed photos)
	var pic_frame := BoxMesh.new()
	pic_frame.size = Vector3(0.7, 0.52, 0.06)
	var pic := BoxMesh.new()
	pic.size = Vector3(0.6, 0.42, 0.05)
	_mi(r, pic_frame, _toon(Color(0.55, 0.4, 0.25)), Vector3(-2.6, 1.8, -5.38))
	_mi(r, pic, _toon(Color(0.45, 0.75, 0.7), 0.3, false), Vector3(-2.6, 1.8, -5.35))
	_mi(r, pic_frame, _toon(Color(0.55, 0.4, 0.25)), Vector3(-1.7, 1.55, -5.38))
	_mi(r, pic, _toon(Color(0.95, 0.65, 0.75), 0.3, false), Vector3(-1.7, 1.55, -5.35))
	_mi(r, pic_frame, _toon(Color(0.55, 0.4, 0.25)), Vector3(2.5, 1.75, -5.38))
	_mi(r, pic, _toon(Color(0.94, 0.78, 0.31), 0.3, false), Vector3(2.5, 1.75, -5.35))

	# big rug anchoring the lounge area
	var rug := CylinderMesh.new()
	rug.top_radius = 2.6
	rug.bottom_radius = 2.6
	rug.height = 0.025
	rug.radial_segments = 26
	var rmi := _mi(r, rug, _toon(Color(0.85, 0.45, 0.4), 0.1, false), Vector3(0.2, 0.035, 0.7))
	rmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rug2 := CylinderMesh.new()
	rug2.top_radius = 1.8
	rug2.bottom_radius = 1.8
	rug2.height = 0.025
	rug2.radial_segments = 24
	var rmi2 := _mi(r, rug2, _toon(Color(0.95, 0.62, 0.5), 0.1, false), Vector3(0.2, 0.05, 0.7))
	rmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# bedroom — upstairs, parented to the cutaway root
	var bed_frame := BoxMesh.new()
	bed_frame.size = Vector3(1.5, 0.32, 2.5)
	_mi(_loft_root, bed_frame, _toon(Color(0.6, 0.42, 0.26)), Vector3(-6.3, 3.06, -3.9))
	var mattress := BoxMesh.new()
	mattress.size = Vector3(1.34, 0.18, 2.34)
	_mi(_loft_root, mattress, _toon(Color(0.96, 0.96, 0.94), 0.2), Vector3(-6.3, 3.32, -3.9))
	var pillow := BoxMesh.new()
	pillow.size = Vector3(1.0, 0.16, 0.5)
	_mi(_loft_root, pillow, _toon(Color(1.0, 1.0, 1.0), 0.3), Vector3(-6.3, 3.48, -4.75))
	var blanket := BoxMesh.new()
	blanket.size = Vector3(1.38, 0.1, 1.2)
	_mi(_loft_root, blanket, _toon(Color(0.5, 0.66, 0.95), 0.25), Vector3(-6.3, 3.42, -3.3))
	var stand := BoxMesh.new()
	stand.size = Vector3(0.5, 0.45, 0.5)
	_mi(_loft_root, stand, _toon(Color(0.62, 0.44, 0.28)), Vector3(-5.2, 3.125, -4.9))
	var orb := SphereMesh.new()
	orb.radius = 0.09
	orb.height = 0.18
	orb.radial_segments = 10
	orb.rings = 5
	_mi(_loft_root, orb, VerseAvatar.glow_mat(Color(1.0, 0.85, 0.6), 0.8), Vector3(-5.2, 3.44, -4.9))

	# lounge: low table + four cushions on the rug
	var ttop := CylinderMesh.new()
	ttop.top_radius = 0.7
	ttop.bottom_radius = 0.7
	ttop.height = 0.07
	ttop.radial_segments = 18
	_mi(r, ttop, _toon(Color(0.72, 0.52, 0.33), 0.15), Vector3(0.2, 0.38, 0.7))
	var tleg := CylinderMesh.new()
	tleg.top_radius = 0.09
	tleg.bottom_radius = 0.11
	tleg.height = 0.35
	tleg.radial_segments = 10
	_mi(r, tleg, _toon(Color(0.6, 0.42, 0.26)), Vector3(0.2, 0.17, 0.7))
	var cushion := CylinderMesh.new()
	cushion.top_radius = 0.3
	cushion.bottom_radius = 0.34
	cushion.height = 0.14
	cushion.radial_segments = 14
	_mi(r, cushion, _toon(Color(0.94, 0.78, 0.31), 0.25), Vector3(1.6, 0.07, 0.7))
	_mi(r, cushion, _toon(Color(1.0, 0.54, 0.81), 0.25), Vector3(-1.2, 0.07, 1.5))
	_mi(r, cushion, _toon(Color(0.5, 0.89, 0.75), 0.25), Vector3(0.2, 0.07, 2.1))
	_mi(r, cushion, _toon(Color(0.76, 0.61, 1.0), 0.25), Vector3(-0.9, 0.07, -0.4))

	# double bookshelf along the right wall
	var shelf := BoxMesh.new()
	shelf.size = Vector3(0.35, 1.5, 1.4)
	var shelf_board := BoxMesh.new()
	shelf_board.size = Vector3(0.3, 0.04, 1.3)
	var book := BoxMesh.new()
	book.size = Vector3(0.2, 0.3, 0.08)
	var book_cols := [Color(0.85, 0.4, 0.35), Color(0.4, 0.6, 0.85), Color(0.45, 0.75, 0.45), Color(0.94, 0.78, 0.31), Color(0.76, 0.61, 1.0)]
	for sz in [-3.2, -1.7]:
		_mi(r, shelf, _toon(Color(0.58, 0.4, 0.25)), Vector3(-7.1, 0.75, sz))
		_mi(r, shelf_board, _toon(Color(0.68, 0.5, 0.32), 0.0, false), Vector3(-7.1, 0.95, sz))
		_mi(r, shelf_board, _toon(Color(0.68, 0.5, 0.32), 0.0, false), Vector3(-7.1, 0.5, sz))
		for k in 5:
			var bc: Color = book_cols[(k + (1 if sz < -2.0 else 0)) % book_cols.size()]
			var bmi := _mi(r, book, _toon(bc, 0.2, false), Vector3(-7.1, 1.13 if k < 3 else 0.68, sz - 0.5 + k * 0.25))
			bmi.rotation.x = randf_range(-0.05, 0.05)

	# potted plants in the corners
	var pot := CylinderMesh.new()
	pot.top_radius = 0.17
	pot.bottom_radius = 0.13
	pot.height = 0.26
	pot.radial_segments = 10
	var leafb := SphereMesh.new()
	leafb.radius = 0.22
	leafb.height = 0.44
	leafb.radial_segments = 8
	leafb.rings = 4
	for ppos in [Vector3(6.9, 0, 4.6), Vector3(-6.9, 0, 4.6), Vector3(-6.9, 0, -0.5)]:
		var p3: Vector3 = ppos
		_mi(r, pot, _toon(Color(0.8, 0.5, 0.35), 0.15), Vector3(p3.x, 0.13, p3.z))
		_mi(r, leafb, _toon(Color(0.36, 0.64, 0.32), 0.3), Vector3(p3.x, 0.42, p3.z))
		_mi(r, leafb, _toon(Color(0.42, 0.7, 0.36), 0.3), Vector3(p3.x - 0.1, 0.58, p3.z - 0.1)).scale = Vector3.ONE * 0.7

	# two standing lamps — preset-driven like the street lamps: off in daylight,
	# warm at sunset, bright at night (they were stacking on the day sun)
	for lpos in [Vector3(-6.5, 0, 2.8), Vector3(5.6, 0, 4.6)]:
		var l3: Vector3 = lpos
		var lpost := CylinderMesh.new()
		lpost.top_radius = 0.03
		lpost.bottom_radius = 0.05
		lpost.height = 1.4
		lpost.radial_segments = 8
		_mi(r, lpost, _toon(Color(0.3, 0.32, 0.36)), Vector3(l3.x, 0.7, l3.z))
		var shade := CylinderMesh.new()
		shade.top_radius = 0.14
		shade.bottom_radius = 0.26
		shade.height = 0.3
		shade.radial_segments = 12
		var shade_mat := VerseAvatar.glow_mat(Color(1.0, 0.85, 0.6), 0.25)
		_mi(r, shade, shade_mat, Vector3(l3.x, 1.5, l3.z))
		var warm := OmniLight3D.new()
		warm.position = Vector3(l3.x, 1.6, l3.z)
		warm.omni_range = 9.0
		warm.light_color = Color(1.0, 0.85, 0.6)
		warm.light_energy = 0.0
		r.add_child(warm)
		_lamps.append({"light": warm, "mat": shade_mat})

	# door mat at the open front edge (the future front door)
	var mat_m := BoxMesh.new()
	mat_m.size = Vector3(1.1, 0.04, 0.65)
	var mmi := _mi(r, mat_m, _toon(Color(0.6, 0.45, 0.3), 0.1, false), Vector3(0, 0.04, 5.15))
	mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# anything you've placed (and, later, bought) comes from the manifest
	_spawn_saved_furniture(r)
	_restore_paintings()
