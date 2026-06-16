extends Node
## Net — the bridge between the game and the Hey mesh (autoload).
##
## If the Rust GDExtension class `HeyVerseBridge` is registered (built from
## mobile/hey-verse-bridge/), it is used for identity + contacts + transport.
## Otherwise a built-in simulation provides a small CONTACT LIST you can
## invite from, so the whole real flow — pick a friend's DID, they walk in,
## they leave, you must re-invite — is exercised with zero deps. The API is
## identical in both modes; home.gd never knows the difference.
##
## Session model (LIVE-ONLY, like a call):
##   - You invite a CONTACT (an existing Hey friend — added via hey invite
##     links, the only way contacts exist). The invite is a verse visit offer
##     sent over the sealed DM lane: {t:"verse-inv", ticket}.
##   - They accept -> their node dials yours (carrier) -> they're HERE.
##   - The grant lives exactly as long as the connection. Disconnect (either
##     side, or app close) voids it — nothing persists; re-invite to rejoin.
##
## Wire protocol (implemented by the Rust side, mirrored by the sim):
##   movement  — unreliable datagrams {"t":"mv","x":f32,"z":f32,"yw":f32,"m":bool} <=15 Hz
##   chat      — existing sealed Hey DM lane (chat_send)
##   presence  — join/leave on the live connection carrying {name, color, outfit}

signal peer_chat(did: String, text: String)
signal session_ended(did: String)
signal ui_cmd(cmd: String)               # dock-sheet commands from the app
signal me_updated(did: String, name: String)  # identity fills in async at boot
signal join_world(zone: String)          # accepted an invite → go to their world

const PALETTE: Array[Color] = [
	Color(0.5, 0.71, 1.0), Color(0.94, 0.78, 0.31), Color(1.0, 0.54, 0.81),
	Color(0.5, 0.89, 0.75), Color(0.76, 0.61, 1.0), Color(1.0, 0.64, 0.5),
	Color(0.54, 0.84, 1.0), Color(0.72, 0.91, 0.53),
]

const SIM_LOCAL_DID := "did:key:z6MkVerseLocalDev"
const SIM_CONTACTS: Array[Dictionary] = [
	{"did": "did:key:z6MkVerseFriendMira", "name": "Mira", "hat": "sprout"},
	{"did": "did:key:z6MkVerseFriendKai", "name": "Kai", "hat": "cap"},
	{"did": "did:key:z6MkVerseFriendSuki", "name": "Suki", "hat": "crown"},
]
const LINES: Array[String] = [
	"this place is so cozy",
	"love what you did here",
	"race you to the pond",
	"the light here is unreal",
	"ok this is really cute",
	"can i get a house like yours?",
]

var _bridge: Object = null
var _hv: Object = null              # HeyVersePlugin singleton (the real app)
var _hv_peers: Dictionary = {}
var _hv_accum := 0.0
var _me_did := ""
var _me_name := ""
var _local_pos := Vector3(0, 0, 1.5)
var _sim: Dictionary = {}   # did -> peer state


func _ready() -> void:
	# Tier 1: inside the Hey Android app — the Kotlin GodotPlugin carries real
	# identity, contacts, invites, presence and chat over the sealed DM lane.
	if Engine.has_singleton("HeyVerse"):
		_hv = Engine.get_singleton("HeyVerse")
		return
	if ClassDB.class_exists("HeyVerseBridge"):
		_bridge = ClassDB.instantiate("HeyVerseBridge")
		add_child(_bridge)
		print("[verse] HeyVerseBridge loaded (real bridge)")
		return
	print("[verse] no bridge — sim mode")
	for c in SIM_CONTACTS:
		var cd: Dictionary = c
		_sim[cd["did"]] = {
			"name": cd["name"], "hat": cd["hat"],
			"pos": Vector3.ZERO, "yaw": 0.0, "target": Vector3.ZERO,
			"moving": false, "retarget": 2.0, "ambient": 9.0,
			"arrived": false, "bye": false, "leave": 0.0,
		}


## Deterministic pastel per DID — same idea as Hey's chat-avatar palette.
static func did_color(did: String) -> Color:
	return PALETTE[absi(hash(did)) % PALETTE.size()]


# ── marketplace inventory ─────────────────────────────────────────────────────
## Proxied to the Rust bridge (JSON strings → Array), with a small sim catalog so
## the shop works with zero deps. Item records match VerseItems (id/kind/name/
## builtin/…); a .ddrm purchase later is the same record with token_id/ddrm_cid set.
const SIM_SHOP: Array = [
	{"id": "cozy_kitchen", "kind": "furniture", "name": "Cozy Kitchen", "builtin": "kitchen", "price_ela": "0.50", "pos": [1.7, 0.0, -1.3], "rot_y": 0.0},
	{"id": "reading_nook", "kind": "furniture", "name": "Reading Nook", "builtin": "cushion", "price_ela": "0.20", "pos": [-1.4, 0.0, 1.0], "rot_y": 0.4},
	{"id": "crate_stack", "kind": "furniture", "name": "Crate Stack", "builtin": "crate", "price_ela": "0.10", "pos": [2.1, 0.0, 0.9], "rot_y": 0.0},
]
var _sim_owned: Array = []

func shop_items() -> Array:
	if _bridge:
		var v: Variant = JSON.parse_string(str(_bridge.shop_items()))
		return v if v is Array else []
	return SIM_SHOP

func owned_items() -> Array:
	if _bridge:
		var v: Variant = JSON.parse_string(str(_bridge.owned_items()))
		return v if v is Array else []
	return _sim_owned.duplicate()

## Buy an item. Phase 0 grants it; Phase 1 pays via the wallet + checks an
## on-chain (ESC) license. Returns the bought record (or {} on failure).
func buy_item(id: String) -> Dictionary:
	for it in shop_items():
		if str((it as Dictionary).get("id", "")) == id:
			var ok := false
			if _bridge:
				ok = bool(_bridge.buy_item(id))
			else:
				if not _sim_owned.has(it):
					_sim_owned.append(it)
				ok = true
			return it if ok else {}
	return {}


## Pull the latest real-session state from the app plugin (~8 Hz): present
## peers (positions glide client-side), inbound chat bubbles, ended sessions.
func _drain_hv() -> void:
	var raw := str(_hv.pollJson())
	if raw.is_empty():
		return
	var parsed: Variant = JSON.parse_string(raw)
	if not (parsed is Dictionary):
		return
	var d: Dictionary = parsed
	var peers_in: Dictionary = d.get("peers", {})
	var out: Dictionary = {}
	for did in peers_in.keys():
		var p: Dictionary = peers_in[did]
		out[did] = {
			"pos": Vector3(float(p.get("x", 0.0)), 0.0, float(p.get("z", 2.0))),
			"yaw": float(p.get("yw", 0.0)),
			"moving": bool(p.get("m", false)),
			"sitting": bool(p.get("s", false)),
			"name": str(p.get("name", "friend")),
			"color": did_color(str(did)),
			"outfit": {},
			"zone": str(p.get("w", "")),
		}
	_hv_peers = out
	for c in d.get("chats", []):
		var ca: Array = c
		if ca.size() >= 2:
			peer_chat.emit(str(ca[0]), str(ca[1]))
	for e in d.get("ended", []):
		session_ended.emit(str(e))
	for u in d.get("ui", []):
		ui_cmd.emit(str(u))
	var me: Dictionary = d.get("me", {})
	var mdid := str(me.get("did", ""))
	var mname := str(me.get("name", ""))
	if mdid != "" and (mdid != _me_did or mname != _me_name):
		_me_did = mdid
		_me_name = mname
		me_updated.emit(mdid, mname)
	var jz := str(d.get("join", ""))
	if jz != "":
		join_world.emit(jz)


func is_sim() -> bool:
	return _bridge == null


## True when running inside the Hey app (the dock carries the game controls).
func is_app() -> bool:
	return _hv != null


## The game scene finished booting — lets the app hide its loading overlay.
func notify_ready() -> void:
	if _hv:
		_hv.gameReady()


## Ask the Hey app to open one of its popup sheets (tap-Sash FAQ etc).
func open_sheet(sheet: String) -> void:
	if _hv:
		_hv.openSheet(sheet)


func local_did() -> String:
	if _hv:
		return str(_hv.localDid())
	if _bridge:
		return str(_bridge.local_did())
	return SIM_LOCAL_DID


func local_name() -> String:
	if _hv:
		return str(_hv.localName())
	if _bridge:
		return str(_bridge.local_name())
	return "you"


func local_color() -> Color:
	if _bridge:
		var c: Color = _bridge.local_color()
		return c
	return did_color(local_did())


## Your Hey contacts — the only people who can ever be invited. Contacts are
## made via hey friend-invite links; there is no other way in.
func contacts() -> Array:
	var out: Array = []
	if _hv:
		var parsed: Variant = JSON.parse_string(str(_hv.contactsJson()))
		if parsed is Array:
			for c in parsed:
				var cd0: Dictionary = c
				out.append({"did": str(cd0.get("did", "")), "name": str(cd0.get("name", ""))})
		return out
	if _bridge:
		var d: Dictionary = _bridge.contacts()
		for did in d.keys():
			out.append({"did": str(did), "name": str(d[did])})
		return out
	for c in SIM_CONTACTS:
		var cd: Dictionary = c
		out.append({"did": cd["did"], "name": cd["name"]})
	return out


## Offer a live visit to one contact. Real mode: verse-inv over the sealed DM
## lane; they accept -> carrier session. Sim: they arrive after a moment and
## eventually say bye and leave (so the re-invite loop is exercised too).
func invite(did: String) -> void:
	if _hv:
		_hv.invite(did)
		return
	if _bridge:
		_bridge.invite(did)
		return
	if not _sim.has(did):
		return
	var st: Dictionary = _sim[did]
	if st["arrived"]:
		return
	var t := get_tree().create_timer(randf_range(1.4, 2.4))
	t.timeout.connect(func() -> void:
		st["arrived"] = true
		st["bye"] = false
		st["leave"] = randf_range(75.0, 130.0)
		st["pos"] = _local_pos + Vector3(randf_range(1.8, 3.0), 0.0, randf_range(1.5, 2.8))
		st["target"] = st["pos"]
		var t2 := get_tree().create_timer(1.1)
		t2.timeout.connect(func() -> void: peer_chat.emit(did, "hey hey, i'm here!")))


func send_move(pos: Vector3, yaw: float, moving: bool, sitting: bool = false) -> void:
	_local_pos = pos
	if _hv:
		_hv.sendMove(pos.x, pos.z, yaw, moving, sitting)
		return
	if _bridge:
		_bridge.send_move(pos.x, pos.z, yaw, moving)


func send_chat(text: String) -> void:
	if _hv:
		_hv.sendChat(text)
		return
	if _bridge:
		_bridge.send_chat(text)
		return
	# one random present friend replies
	var here: Array = []
	for did in _sim.keys():
		var st: Dictionary = _sim[did]
		if st["arrived"]:
			here.append(did)
	if here.is_empty():
		return
	var who: String = here[randi() % here.size()]
	var line: String = LINES[randi() % LINES.size()]
	var t := get_tree().create_timer(randf_range(1.4, 2.6))
	t.timeout.connect(func() -> void: peer_chat.emit(who, line))


## did -> {pos, yaw, moving, name, color, outfit} for everyone currently HERE.
func peers() -> Dictionary:
	if _hv:
		return _hv_peers
	if _bridge:
		var d: Dictionary = _bridge.poll()
		return d
	var out: Dictionary = {}
	for did in _sim.keys():
		var st: Dictionary = _sim[did]
		if not st["arrived"]:
			continue
		out[did] = {
			"pos": st["pos"], "yaw": st["yaw"], "moving": st["moving"],
			"name": st["name"], "color": did_color(str(did)),
			"outfit": {"hat": st["hat"]},
		}
	return out


func _process(delta: float) -> void:
	if _hv:
		_hv_accum -= delta
		if _hv_accum <= 0.0:
			_hv_accum = 0.05
			_drain_hv()
		return
	if _bridge:
		return
	for did in _sim.keys():
		var st: Dictionary = _sim[did]
		if not st["arrived"]:
			continue
		# live-only session: they eventually leave; re-invite to bring them back
		st["leave"] = float(st["leave"]) - delta
		if st["leave"] <= 2.5 and not st["bye"]:
			st["bye"] = true
			peer_chat.emit(str(did), "gtg — see ya!")
		if st["leave"] <= 0.0:
			st["arrived"] = false
			session_ended.emit(str(did))
			continue
		# wander around the player
		st["retarget"] = float(st["retarget"]) - delta
		if st["retarget"] <= 0.0:
			st["retarget"] = randf_range(3.0, 7.0)
			var ang := randf() * TAU
			var rad := randf_range(1.6, 5.5)
			var anchor: Vector3 = _local_pos if _local_pos.x < 30.0 else st["pos"]
			var tgt: Vector3 = anchor + Vector3(cos(ang) * rad, 0.0, sin(ang) * rad)
			# roam near the anchor (works in any world — yard or Ela City)
			tgt.x = clampf(tgt.x, anchor.x - 8.0, anchor.x + 8.0)
			tgt.z = clampf(tgt.z, anchor.z - 8.0, anchor.z + 8.0)
			st["target"] = tgt
		var to: Vector3 = st["target"] - st["pos"]
		to.y = 0.0
		var d := to.length()
		if d > 0.12:
			st["pos"] = st["pos"] + to.normalized() * minf(d, 3.0 * delta)
			st["yaw"] = atan2(to.x, to.z)
			st["moving"] = true
		else:
			st["moving"] = false
		# occasional ambient chatter
		st["ambient"] = float(st["ambient"]) - delta
		if st["ambient"] <= 0.0:
			st["ambient"] = randf_range(14.0, 26.0)
			peer_chat.emit(str(did), LINES[randi() % LINES.size()])
