extends SceneTree

func _init():
	var items := {
		"rug": VerseCatalogDecor.build_rug(),
		"vase": VerseCatalogDecor.build_vase(),
		"statue": VerseCatalogDecor.build_statue(),
		"fountain": VerseCatalogDecor.build_fountain(),
		"snowglobe": VerseCatalogDecor.build_snowglobe(),
		"gramophone": VerseCatalogDecor.build_gramophone(),
		"telescope": VerseCatalogDecor.build_telescope(),
		"aquarium": VerseCatalogDecor.build_aquarium(),
		"crystal": VerseCatalogDecor.build_crystal(),
		"balloons": VerseCatalogDecor.build_balloons(),
		"trophy": VerseCatalogDecor.build_trophy(),
	}
	var ok := 0
	for id in items.keys():
		var n: Node3D = items[id]
		if n == null:
			push_error("NULL from build_" + id)
			continue
		var cnt := _count(n)
		var mesh_cnt := _count_mesh(n)
		var box := _aabb(n, Transform3D.IDENTITY)
		print("%-12s nodes=%-4d meshes=%-4d  floorY=%+.3f  size=%s" % [
			id, cnt, mesh_cnt, box.position.y, str(box.size.snappedf(0.01))])
		ok += 1
	print("BUILT OK: %d / %d" % [ok, items.size()])
	quit()

func _count(n: Node) -> int:
	var c := 1
	for ch in n.get_children():
		c += _count(ch)
	return c

func _count_mesh(n: Node) -> int:
	var c := 0
	if n is MeshInstance3D and n.mesh != null:
		c += 1
	for ch in n.get_children():
		c += _count_mesh(ch)
	return c

var _box := AABB()
var _has := false
func _aabb(n: Node, xform: Transform3D) -> AABB:
	_box = AABB()
	_has = false
	_walk(n, xform)
	return _box

func _walk(n: Node, xform: Transform3D) -> void:
	var t := xform
	if n is Node3D:
		t = xform * n.transform
	if n is MeshInstance3D and n.mesh != null:
		var a: AABB = t * n.mesh.get_aabb()
		if not _has:
			_box = a
			_has = true
		else:
			_box = _box.merge(a)
	for ch in n.get_children():
		_walk(ch, t)
