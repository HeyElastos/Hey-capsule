extends Node3D
## Standalone catalog showroom — walk every VerseCatalog item on its turntable with
## the REAL toon shaders (no runtime / HeyVerse singleton needed). Run with:
##   flatpak run org.godotengine.Godot --path mobile/hey-verse res://showroom.tscn
## Controls: WASD move · Q/E down/up · hold RIGHT-MOUSE to look · Shift = faster · Esc quit

var _cam: Camera3D
var _yaw := 0.0
var _pitch := -0.12
var _speed := 5.0
var _look := false


func _ready() -> void:
	# soft studio environment so the toon/outline materials read well
	var we := WorldEnvironment.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.10, 0.11, 0.15)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.55, 0.57, 0.64)
	env.ambient_light_energy = 0.7
	env.fog_enabled = true
	env.fog_light_color = Color(0.10, 0.11, 0.15)
	env.fog_density = 0.012
	we.environment = env
	add_child(we)

	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-52, -40, 0)
	sun.light_energy = 1.25
	add_child(sun)
	var fill := DirectionalLight3D.new()
	fill.rotation_degrees = Vector3(-18, 130, 0)
	fill.light_energy = 0.45
	fill.light_color = Color(0.8, 0.85, 1.0)
	add_child(fill)

	var ground := MeshInstance3D.new()
	var pm := PlaneMesh.new()
	pm.size = Vector2(80, 120)
	ground.mesh = pm
	var gm := StandardMaterial3D.new()
	gm.albedo_color = Color(0.14, 0.15, 0.19)
	gm.roughness = 0.95
	ground.material_override = gm
	ground.position = Vector3(0, -0.06, -28)
	add_child(ground)

	_cam = Camera3D.new()
	_cam.position = Vector3(0, 1.7, 5.0)
	_cam.fov = 62
	_cam.far = 400.0
	add_child(_cam)
	_apply_look()

	# the catalog itself — one row per kind, every item on a rotating pedestal
	VerseGallery.show(self)

	var hud := CanvasLayer.new()
	var lbl := Label.new()
	lbl.text = "Hey Verse — Catalog Showroom · 72 items\nWASD move · Q/E down·up · hold RIGHT-MOUSE to look · Shift faster · Esc quit"
	lbl.position = Vector2(14, 10)
	lbl.add_theme_color_override("font_color", Color(0.95, 0.95, 1.0))
	lbl.add_theme_color_override("font_outline_color", Color(0, 0, 0))
	lbl.add_theme_constant_override("outline_size", 6)
	hud.add_child(lbl)
	add_child(hud)


func _apply_look() -> void:
	_cam.rotation = Vector3(_pitch, _yaw, 0.0)


func _input(e: InputEvent) -> void:
	if e is InputEventMouseButton and e.button_index == MOUSE_BUTTON_RIGHT:
		_look = e.pressed
		Input.mouse_mode = Input.MOUSE_MODE_CAPTURED if _look else Input.MOUSE_MODE_VISIBLE
	elif e is InputEventMouseMotion and _look:
		_yaw -= e.relative.x * 0.005
		_pitch = clampf(_pitch - e.relative.y * 0.005, -1.4, 1.4)
		_apply_look()
	elif e is InputEventKey and e.pressed and e.keycode == KEY_ESCAPE:
		get_tree().quit()


func _process(delta: float) -> void:
	var sp := _speed * (3.0 if Input.is_key_pressed(KEY_SHIFT) else 1.0)
	var dir := Vector3.ZERO
	var b := _cam.global_transform.basis
	if Input.is_key_pressed(KEY_W): dir -= b.z
	if Input.is_key_pressed(KEY_S): dir += b.z
	if Input.is_key_pressed(KEY_A): dir -= b.x
	if Input.is_key_pressed(KEY_D): dir += b.x
	if Input.is_key_pressed(KEY_E): dir += Vector3.UP
	if Input.is_key_pressed(KEY_Q): dir -= Vector3.UP
	if dir != Vector3.ZERO:
		_cam.position += dir.normalized() * sp * delta
