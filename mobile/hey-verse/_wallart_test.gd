extends SceneTree

func _init():
	var items := {
		"ornate_painting": VerseCatalogWallart.build_ornate_painting(),
		"neon": VerseCatalogWallart.build_neon(),
		"gilded_mirror": VerseCatalogWallart.build_gilded_mirror(),
		"grand_clock": VerseCatalogWallart.build_grand_clock(),
		"pixel_screen": VerseCatalogWallart.build_pixel_screen(),
		"pennant": VerseCatalogWallart.build_pennant(),
		"butterfly_display": VerseCatalogWallart.build_butterfly_display(),
		"vinyl_wall": VerseCatalogWallart.build_vinyl_wall(),
		"holo_poster": VerseCatalogWallart.build_holo_poster(),
	}
	var ok := 0
	for id in items.keys():
		var n: Node3D = items[id]
		if n == null:
			push_error("NULL from build_" + str(id))
			continue
		var cnt := _count(n)
		print("%-20s nodes=%d  aabb=%s" % [id, cnt, str(_aabb(n))])
		ok += 1
	print("BUILT OK: %d / %d" % [ok, items.size()])
	quit()

func _count(n: Node) -> int:
	var c := 1
	for ch in n.get_children():
		c += _count(ch)
	return c

# rough world-space bounds over all MeshInstance3D under n
func _aabb(n: Node) -> AABB:
	var box := AABB()
	var has := false
	for mi in _all_mesh(n):
		var a: AABB = mi.mesh.get_aabb()
		a.position += mi.position
		if not has:
			box = a
			has = true
		else:
			box = box.merge(a)
	return box

func _all_mesh(n: Node) -> Array:
	var out := []
	if n is MeshInstance3D and n.mesh != null:
		out.append(n)
	for ch in n.get_children():
		out += _all_mesh(ch)
	return out
