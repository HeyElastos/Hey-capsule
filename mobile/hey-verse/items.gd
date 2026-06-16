class_name VerseItems
extends RefCounted
## The item contract — the shape every wearable/furniture piece uses, INCLUDING
## future marketplace purchases. A bought item is the same record with the ddrm
## fields filled in; nothing else in the game changes.
##
## Item record:
##   id          unique string ("cap", "tok:0xabc...")
##   kind        "hat" | "accent" | "furniture"
##   name        display name
##   builtin     builder id for primitive items ("cap", "cushion", ...)
##   glb_path    decrypted model on disk (set after ddrm unlock)  — optional
##   token_id    marketplace NFT id                               — optional
##   ddrm_cid    encrypted .ddrm blob CID (fetch from content)    — optional
##   preview_cid public preview mesh CID (visitors render this)   — optional
##
## Unlock flow (later, via the Rust bridge): own token_id -> fetch ddrm_cid ->
## key released against ownership proof -> decrypt to a temp .glb -> set
## glb_path -> load_item_mesh() picks it up. Until then builtins render.

const HATS: Array[String] = ["", "cap", "tophat", "crown", "sprout"]


## Resolve an item record to a renderable node. Encrypted .ddrm (decrypted
## on-device, from memory) wins; then a decrypted .glb on disk; then null and the
## owner builds a primitive (avatar.gd hats / home.gd furniture).
static func load_item_mesh(item: Dictionary) -> Node3D:
	# Encrypted .ddrm: fetch+decrypt on-device via the Rust bridge, render from
	# memory (no temp file, so non-owners never get the file). Local-first: the
	# content key `ck` rides the record for now; later it comes from HTKS release.
	if item.has("ddrm_cid") and item.has("ck"):
		var m := load_ddrm_mesh(str(item["ddrm_cid"]), str(item["ck"]))
		if m != null:
			return m
	if item.has("glb_path"):
		var path: String = item["glb_path"]
		if ResourceLoader.exists(path):
			var packed: PackedScene = load(path)
			return packed.instantiate()
	return null


## Build a Node3D from raw .glb BYTES at runtime (no temp file). Returns null on
## failure. Requires the engine's glTF module (standard Godot 4.6 ships it).
static func mesh_from_glb_bytes(bytes: PackedByteArray) -> Node3D:
	if bytes.is_empty():
		return null
	var doc := GLTFDocument.new()
	var state := GLTFState.new()
	var err := doc.append_from_buffer(bytes, "", state)
	if err != OK:
		push_warning("ddrm: glb parse failed (%d)" % err)
		return null
	return doc.generate_scene(state) as Node3D


## Owner-side unlock: ask the Hey Rust bridge to fetch+decrypt the .ddrm by cid
## (key released + decrypted ON-DEVICE), then build the mesh from the in-memory
## bytes. `ck` = content key (local-first; no chain yet). Null if unavailable.
static func load_ddrm_mesh(cid: String, ck: String) -> Node3D:
	if cid.is_empty() or not Engine.has_singleton("HeyVerse"):
		return null
	var hv = Engine.get_singleton("HeyVerse")
	var b64: String = hv.loadDdrm(cid, ck)
	if b64.is_empty():
		return null
	return mesh_from_glb_bytes(Marshalls.base64_to_raw(b64))


## TEST / creator helper: encrypt a res:// .glb with `ck`, store it via the Hey
## content provider, return its cid (""=fail). Lets a self-test do
## pack_ddrm_from_res(...) -> load_ddrm_mesh(cid, ck) end-to-end on-device.
static func pack_ddrm_from_res(res_path: String, ck: String) -> String:
	if not Engine.has_singleton("HeyVerse"):
		return ""
	var bytes := FileAccess.get_file_as_bytes(res_path)
	if bytes.is_empty():
		return ""
	var hv = Engine.get_singleton("HeyVerse")
	return hv.packDdrm(Marshalls.raw_to_base64(bytes), ck)


## ON-DEVICE PROOF of the whole .ddrm -> Godot path (LOCAL key, NO chain):
## generate a primitive .glb in-engine -> pack as .ddrm (encrypt + store via Hey)
## -> fetch + decrypt + parse from memory -> mount under `parent`. The mounted gold
## cube floating at y=2 == the full encrypt->store->fetch->decrypt->render path
## worked on real hardware. Returns a status string (also printed to logcat).
static func selftest(parent: Node3D) -> String:
	if not Engine.has_singleton("HeyVerse"):
		return "ddrm selftest: no HeyVerse singleton (not on device)"
	# 1. build a primitive cube + bake a .glb in-engine (no external asset needed)
	var root := Node3D.new()
	var box := MeshInstance3D.new()
	var mesh := BoxMesh.new()
	var mat := StandardMaterial3D.new()
	mat.albedo_color = Color(0.85, 0.7, 0.2)
	mesh.material = mat
	box.mesh = mesh
	root.add_child(box)
	box.owner = root
	var doc := GLTFDocument.new()
	var state := GLTFState.new()
	if doc.append_from_scene(root, state) != OK:
		root.queue_free()
		return "ddrm selftest: append_from_scene failed"
	var glb := doc.generate_buffer(state)
	root.queue_free()
	if glb.is_empty():
		return "ddrm selftest: generate_buffer empty"
	# 2. pack (encrypt + store) with a LOCAL 32-byte key (no chain), get cid
	var key := Marshalls.raw_to_base64("hey-ddrm-selftest-key-0123456789".to_utf8_buffer())
	var hv = Engine.get_singleton("HeyVerse")
	var cid: String = hv.packDdrm(Marshalls.raw_to_base64(glb), key)
	if cid.is_empty():
		return "ddrm selftest: packDdrm failed"
	print("ddrm selftest: packed %d-byte glb -> cid %s" % [glb.size(), cid])
	# 3. fetch + decrypt (on-device) + parse from memory -> mount
	var node := load_ddrm_mesh(cid, key)
	if node == null:
		return "ddrm selftest: load_ddrm_mesh returned null (cid %s)" % cid
	node.position = Vector3(0, 2, 0)
	parent.add_child(node)
	return "ddrm selftest: OK -- decrypted cube mounted (cid %s)" % cid
