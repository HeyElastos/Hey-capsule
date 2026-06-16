class_name VerseGallery
extends RefCounted
## The Hey Verse showroom — lays every VerseCatalog item out on a tidy floor so
## a buyer can stroll the aisles and see each premium object slowly turning on
## its own pedestal, named and rarity-tinted.
##
## Layout: items are grouped into rows BY KIND (hats / seating / tables /
## lighting / wallart / plants / decor), one kind per row, centered on x and
## marching back along -z, ~1.4u apart. A small two-tier pedestal sits under
## each piece; a billboarded Label3D floats above it with the item NAME and
## RARITY, color-tinted by rarity. Each item is parented to a Turntable node so
## it slowly spins (~12 deg/s) and the buyer sees every side.
##
## A FRONT ROW of every hat floats at avatar head height (~1.6u, no pedestal)
## so a shopper can read the scale of headwear against their own robot.
##
## Mobile-cheap: pedestals are 2 primitive cylinders; the only per-frame cost is
## one tiny `rotate_y` per turntable in _process (no tweens, no particles added
## here). Call once: VerseGallery.show(parent).

const ROW_SPACING := 1.4          # spacing between items along a row (x)
const KIND_GAP := 2.6             # gap between one kind's row and the next (z)
const ROW_START_Z := -1.5         # the first (nearest) kind row sits here
const HAT_FLOAT_Y := 1.6          # avatar head height — front hat row floats here
const HAT_FRONT_Z := 2.6          # the floating hat scale row, in front of all rows
const SPIN_DEG_PER_SEC := 12.0    # gentle turntable speed
const LABEL_Y := 1.45             # name/rarity label height above each pedestal


## A self-contained turntable: rotates its own children slowly. One per item.
## Kept tiny so the whole showroom is just N cheap rotate_y calls per frame.
class Turntable extends Node3D:
	var speed_rad := deg_to_rad(12.0)

	func _process(delta: float) -> void:
		rotate_y(speed_rad * delta)


## Build the whole showroom under `parent`. Idempotent-ish: if a previous
## showroom root exists it is cleared first, so calling twice won't stack.
static func show(parent: Node3D) -> void:
	if parent == null:
		return
	var old := parent.get_node_or_null("VerseShowroom")
	if old != null:
		old.queue_free()

	var root := Node3D.new()
	root.name = "VerseShowroom"
	parent.add_child(root)

	var items: Array = VerseCatalog.all()

	# Bucket items by kind, preserving catalog order within each kind.
	var by_kind: Dictionary = {}
	for it in items:
		var k: String = str(it.get("kind", "decor"))
		if not by_kind.has(k):
			by_kind[k] = []
		by_kind[k].append(it)

	# One row per kind, marching back along -z in the canonical kind order.
	var z := ROW_START_Z
	for kind in VerseCatalog.KIND_ORDER:
		if not by_kind.has(kind):
			continue
		var row: Array = by_kind[kind]
		_lay_row(root, row, z, false)
		z -= KIND_GAP

	# Front scale row: every hat floating at avatar head height, no pedestal.
	if by_kind.has("hat"):
		_lay_row(root, by_kind["hat"], HAT_FRONT_Z, true)


## Lay one row of records centered on x at depth `z`. When `floating` is true
## the pieces hover at head height with no pedestal (the hat scale row);
## otherwise each gets a pedestal and sits on the floor.
static func _lay_row(root: Node3D, row: Array, z: float, floating: bool) -> void:
	var n := row.size()
	if n == 0:
		return
	# center the row on x: offsets are symmetric about 0
	var x0 := -ROW_SPACING * float(n - 1) * 0.5
	for i in range(n):
		var rec: Dictionary = row[i]
		var x := x0 + ROW_SPACING * float(i)
		_place_item(root, rec, Vector3(x, 0.0, z), floating)


## Place a single catalog record: pedestal (unless floating), the built model
## on a turntable, and the rarity-tinted name label.
static func _place_item(root: Node3D, rec: Dictionary, base_pos: Vector3, floating: bool) -> void:
	var id: String = str(rec.get("id", ""))
	var stand_y := 0.0

	if not floating:
		stand_y = _pedestal(root, base_pos)

	# the spinning model
	var turn := Turntable.new()
	turn.speed_rad = deg_to_rad(SPIN_DEG_PER_SEC)
	root.add_child(turn)
	if floating:
		turn.position = Vector3(base_pos.x, HAT_FLOAT_Y, base_pos.z)
	else:
		turn.position = Vector3(base_pos.x, stand_y, base_pos.z)

	var model := VerseCatalog.build(id)
	if model != null:
		turn.add_child(model)

	# rarity-tinted floating name label
	var label_y := (HAT_FLOAT_Y + 0.45) if floating else (stand_y + LABEL_Y)
	_label(root, rec, Vector3(base_pos.x, label_y, base_pos.z))


## A small two-tier pedestal (wide stone base + narrower cap). Returns the top
## surface y so the item rests on it.
static func _pedestal(root: Node3D, pos: Vector3) -> float:
	var base_h := 0.12
	var cap_h := 0.06

	var base := MeshInstance3D.new()
	var bm := CylinderMesh.new()
	bm.top_radius = 0.42
	bm.bottom_radius = 0.46
	bm.height = base_h
	bm.radial_segments = 16
	base.mesh = bm
	var bmat := StandardMaterial3D.new()
	bmat.albedo_color = Color(0.28, 0.30, 0.36)
	bmat.roughness = 0.9
	base.material_override = bmat
	base.position = Vector3(pos.x, base_h * 0.5, pos.z)
	root.add_child(base)

	var cap := MeshInstance3D.new()
	var cm := CylinderMesh.new()
	cm.top_radius = 0.34
	cm.bottom_radius = 0.40
	cm.height = cap_h
	cm.radial_segments = 16
	cap.mesh = cm
	var cmat := StandardMaterial3D.new()
	cmat.albedo_color = Color(0.46, 0.40, 0.30)
	cmat.metallic = 0.4
	cmat.roughness = 0.45
	cap.material_override = cmat
	cap.position = Vector3(pos.x, base_h + cap_h * 0.5, pos.z)
	root.add_child(cap)

	return base_h + cap_h


## A billboarded Label3D showing "Name" on one line and the rarity on the next,
## tinted by rarity (the name stays bright; rarity carries the color).
static func _label(root: Node3D, rec: Dictionary, pos: Vector3) -> void:
	var name_ := str(rec.get("name", ""))
	var rarity := str(rec.get("rarity", ""))
	var tint := VerseCatalog.rarity_color(rarity)

	var lbl := Label3D.new()
	lbl.text = "%s\n%s" % [name_, rarity]
	lbl.font_size = 34
	lbl.pixel_size = 0.0085
	lbl.outline_size = 10
	lbl.modulate = tint
	lbl.outline_modulate = Color(0.04, 0.07, 0.13, 0.92)
	lbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	lbl.no_depth_test = true
	lbl.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	lbl.position = pos
	root.add_child(lbl)
