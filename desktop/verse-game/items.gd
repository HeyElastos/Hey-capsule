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


## Resolve an item record to a renderable node. Decrypted .glb wins; builtins
## are built by their owner (avatar.gd hats / home.gd furniture); null = none.
static func load_item_mesh(item: Dictionary) -> Node3D:
	if item.has("glb_path"):
		var path: String = item["glb_path"]
		if ResourceLoader.exists(path):
			var packed: PackedScene = load(path)
			return packed.instantiate()
	return null
