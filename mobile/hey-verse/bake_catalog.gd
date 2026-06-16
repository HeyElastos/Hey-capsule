extends SceneTree
## One-shot baker / verifier. Runs every VerseCatalog builder, exports each item to
## res://catalog_glb/<id>.glb (openable in Godot / any glTF viewer; also the .ddrm
## mint input), and writes catalog_glb/manifest.json (the full NFT-trait records).
## Run:  flatpak run org.godotengine.Godot --path mobile/hey-verse --script res://bake_catalog.gd
## (run WITHOUT --headless if .glb come out geometry-less — primitive meshes need a
## real rendering context to emit surface arrays.)

func _initialize() -> void:
	var items: Array = VerseCatalog.all()
	print("=== bake_catalog: %d items ===" % items.size())
	var out_abs := ProjectSettings.globalize_path("res://catalog_glb")
	DirAccess.make_dir_recursive_absolute(out_abs)

	var ok := 0
	var fail := 0
	var fails: Array = []
	var total_bytes := 0
	for it in items:
		var id: String = it.get("id", "")
		var node: Node3D = VerseCatalog.build(id)
		if node == null:
			fail += 1
			fails.append("%s: build() returned null" % id)
			continue
		var root := Node3D.new()
		root.name = id
		root.add_child(node)
		_own(root, root)
		var doc := GLTFDocument.new()
		var state := GLTFState.new()
		var err := doc.append_from_scene(root, state)
		if err != OK:
			fail += 1
			fails.append("%s: append_from_scene err %d" % [id, err])
			root.free()
			continue
		var glb: PackedByteArray = doc.generate_buffer(state)
		var meshes := _count_mesh(root)
		var f := FileAccess.open("res://catalog_glb/%s.glb" % id, FileAccess.WRITE)
		if f == null:
			fail += 1
			fails.append("%s: cannot open output file" % id)
			root.free()
			continue
		f.store_buffer(glb)
		f.close()
		total_bytes += glb.size()
		ok += 1
		print("  %-22s %2d parts  %6d bytes  [%s]" % [id, meshes, glb.size(), it.get("rarity", "?")])
		root.free()

	var mf := FileAccess.open("res://catalog_glb/manifest.json", FileAccess.WRITE)
	mf.store_string(JSON.stringify(items, "\t"))
	mf.close()

	print("=== BAKE DONE: ok=%d fail=%d  total=%d KB  -> res://catalog_glb/ ===" % [ok, fail, total_bytes / 1024])
	if fails.size() > 0:
		print("--- FAILURES ---")
		for s in fails:
			print("  " + s)
	quit()


func _own(n: Node, root: Node) -> void:
	for c in n.get_children():
		c.owner = root
		_own(c, root)


func _count_mesh(n: Node) -> int:
	var c := 0
	if n is MeshInstance3D:
		c += 1
	for ch in n.get_children():
		c += _count_mesh(ch)
	return c
