extends Node3D
## Standalone PREMIUM BUILDINGS showroom — walk through all VerseBuildings on a
## lawn, each on its own plot with a name/tier/rarity sign. Real toon shaders, no
## runtime needed. Run:
##   flatpak run org.godotengine.Godot --path mobile/hey-verse res://building_showroom.tscn
## Controls: WASD move · Q/E down·up · hold RIGHT-MOUSE to look · Shift faster · Esc quit

const GOLD := Color(0.831, 0.722, 0.294)

var _cam: Camera3D
var _yaw := 0.0
var _pitch := -0.18
var _speed := 9.0
var _look := false


func _ready() -> void:
	var we := WorldEnvironment.new()
	var env := Environment.new()
	env.background_mode = Environment.BG_COLOR
	env.background_color = Color(0.55, 0.70, 0.92)
	env.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	env.ambient_light_color = Color(0.6, 0.64, 0.72)
	env.ambient_light_energy = 0.8
	we.environment = env
	add_child(we)

	var sun := DirectionalLight3D.new()
	sun.rotation_degrees = Vector3(-48, -52, 0)
	sun.light_energy = 1.3
	sun.shadow_enabled = true
	add_child(sun)

	# big grass lawn
	var ground := MeshInstance3D.new()
	var pm := PlaneMesh.new()
	pm.size = Vector2(400, 400)
	ground.mesh = pm
	var gm := StandardMaterial3D.new()
	gm.albedo_color = Color(0.36, 0.52, 0.30)
	gm.roughness = 1.0
	ground.material_override = gm
	ground.position.y = -0.06
	add_child(ground)

	# lay the buildings out on a wide grid (they're big), spaced 34 m
	var items: Array = VerseBuildings.all()
	var cols := 4
	var gap := 34.0
	for i in items.size():
		var rec: Dictionary = items[i]
		var id := str(rec.get("id", ""))
		var node: Node3D = VerseBuildings.build(id)
		if node == null:
			continue
		var col := i % cols
		var row := i / cols
		var x := (float(col) - float(cols - 1) * 0.5) * gap
		var z := -8.0 - float(row) * gap
		node.position = Vector3(x, 0, z)
		add_child(node)
		_sign(rec, Vector3(x, 0.1, z + 9.0))

	_cam = Camera3D.new()
	_cam.position = Vector3(0, 6.0, 26.0)
	_cam.fov = 66
	_cam.far = 800.0
	add_child(_cam)
	_apply_look()

	var hud := CanvasLayer.new()
	var lbl := Label.new()
	lbl.text = "Hey Verse — Premium Buildings (%d)\nWASD move · Q/E down·up · hold RIGHT-MOUSE to look · Shift faster · Esc quit" % items.size()
	lbl.position = Vector2(14, 10)
	lbl.add_theme_color_override("font_color", Color(1, 1, 1))
	lbl.add_theme_color_override("font_outline_color", Color(0, 0, 0))
	lbl.add_theme_constant_override("outline_size", 6)
	hud.add_child(lbl)
	add_child(hud)


func _sign(rec: Dictionary, pos: Vector3) -> void:
	var l := Label3D.new()
	l.text = "%s\n%s · %s" % [str(rec.get("name", "")), str(rec.get("tier", "")), str(rec.get("rarity", ""))]
	l.position = pos + Vector3(0, 2.0, 0)
	l.font_size = 48
	l.pixel_size = 0.01
	l.outline_size = 12
	l.modulate = GOLD
	l.outline_modulate = Color(0.04, 0.07, 0.13)
	l.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	l.no_depth_test = true
	add_child(l)


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
