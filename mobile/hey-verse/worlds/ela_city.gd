## Ela City — a SEPARATE WORLD PACK.
##
## This script ships as its own `ela_city.pck`, not inside the main verse.pck:
## home.gd mounts the pack and calls build() the first time you travel here
## (Worlds → Ela City → Visit). Isolated land — there is no walking between
## worlds; the only way in or out is the Worlds tab. That makes this file the
## template for every future world pack (community lands, bought spaces).
##
## Contents: a stone plaza set in a green park, neon main street, six vendor
## shops, a futuristic skyline (needles, spires, the twisting leaf-glass
## landmark) — and a big modern MALL you can actually walk into: glass
## curtain facade outside, a bright two-storey hall with shopfronts inside.
extends RefCounted

const ORIGIN := Vector3(0, 0, -200)
const CYAN := Color(0.35, 0.85, 1.0)


func build(parent: Node3D, host: Node) -> void:
	var c := ORIGIN
	# ── ground: stone slab in a round green park (skyline-behind-park look) ──
	var park := CylinderMesh.new()
	park.top_radius = 47.0
	park.bottom_radius = 47.0
	park.height = 0.06
	park.radial_segments = 30
	var pkmi: MeshInstance3D = host._mi(parent, park, host._toon(Color(0.40, 0.66, 0.36), 0.05, false), c + Vector3(0, -0.05, 0))
	pkmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var slab := BoxMesh.new()
	slab.size = Vector3(62, 0.3, 62)
	host._mi(parent, slab, host._toon(Color(0.56, 0.58, 0.61), 0.05, false), c + Vector3(0, -0.15, 0))
	# each ground layer gets its OWN height band — coplanar surfaces flicker:
	# slab top 0.00 · street 0.01-0.04 · plaza 0.03-0.07 · neon 0.075+
	var street := BoxMesh.new()
	street.size = Vector3(7, 0.03, 62)
	var smi: MeshInstance3D = host._mi(parent, street, host._toon(Color(0.45, 0.47, 0.5), 0.05, false), c + Vector3(0, 0.025, 0))
	smi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var plaza := CylinderMesh.new()
	plaza.top_radius = 9.0
	plaza.bottom_radius = 9.0
	plaza.height = 0.04
	plaza.radial_segments = 28
	var pmi: MeshInstance3D = host._mi(parent, plaza, host._toon(Color(0.72, 0.70, 0.66), 0.05, false), c + Vector3(0, 0.05, 4))
	pmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# neon guide lines along the street (above every paving layer)
	var lane := BoxMesh.new()
	lane.size = Vector3(0.12, 0.028, 58)
	for lx in [-3.6, 3.6]:
		var lxx: float = lx
		var lmi: MeshInstance3D = host._mi(parent, lane, VerseAvatar.glow_mat(CYAN, 1.3), c + Vector3(lxx, 0.092, 0))
		lmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# ── the fountain: modern white tiers, LIVING water + jets + babble ───────
	_fountain(parent, host, c + Vector3(0, 0, 4))
	var holo := TorusMesh.new()
	holo.inner_radius = 1.0
	holo.outer_radius = 1.15
	holo.rings = 32
	holo.ring_segments = 10
	var hspin: Node3D = _spinner(parent, c + Vector3(0, 3.4, 4), 0.55, 0.12)
	var hmi: MeshInstance3D = host._mi(hspin, holo, VerseAvatar.glow_mat(CYAN, 1.6), Vector3.ZERO)
	hmi.rotation_degrees = Vector3(12, 0, 0)
	hmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# ── ALIVE: hover trams orbiting the plaza (above the rooftops/signs) ─────
	# four of them, evenly spaced, each FACING its direction of travel (the
	# body rides the orbit's tangent, not the radius)
	var tram_orbit: Node3D = _spinner(parent, c + Vector3(0, 0, 4), 0.075, 0.0)
	_tram(tram_orbit, host, Vector3(14.5, 3.7, 0), 90.0)
	_tram(tram_orbit, host, Vector3(-14.5, 4.1, 0), 270.0)
	_tram(tram_orbit, host, Vector3(0, 3.9, 14.5), 0.0)
	_tram(tram_orbit, host, Vector3(0, 4.3, -14.5), 180.0)
	# small robot birds on the wing — circling at different heights
	_bird(parent, host, c + Vector3(0, 0, 4), 11.0, 5.2, 0.22)
	_bird(parent, host, c + Vector3(-8, 0, -10), 7.0, 4.2, -0.27)
	_bird(parent, host, c + Vector3(8, 0, 8), 6.0, 6.0, 0.3)
	_bird(parent, host, c + Vector3(0, 0, -16), 9.0, 7.2, -0.18)
	_bird(parent, host, c + Vector3(-10, 0, 14), 6.5, 5.8, 0.26)
	_bird(parent, host, c + Vector3(12, 0, -2), 7.5, 6.6, -0.22)
	_bird(parent, host, c + Vector3(0, 0, 28), 6.0, 4.8, 0.2)
	# drifting light motes over the plaza (day fireflies of the robot city)
	var mote := SphereMesh.new()
	mote.radius = 0.035
	mote.height = 0.07
	mote.radial_segments = 6
	mote.rings = 3
	mote.material = VerseAvatar.glow_mat(Color(0.7, 0.95, 1.0), 0.8)
	var motes := CPUParticles3D.new()
	motes.amount = 34
	motes.lifetime = 7.0
	motes.mesh = mote
	motes.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	motes.emission_box_extents = Vector3(13, 2.4, 11)
	motes.direction = Vector3(0, 1, 0)
	motes.spread = 180.0
	motes.gravity = Vector3.ZERO
	motes.initial_velocity_min = 0.06
	motes.initial_velocity_max = 0.22
	motes.position = c + Vector3(0, 2.6, 2)
	parent.add_child(motes)
	# zebra crossings where the street meets the plaza
	var zebra := BoxMesh.new()
	zebra.size = Vector3(0.55, 0.022, 1.7)
	for zz in [13.9, -5.8]:
		var zzz: float = zz
		for k in 5:
			var zmi: MeshInstance3D = host._mi(parent, zebra, host._toon(Color(0.9, 0.9, 0.88), 0.05, false), c + Vector3(-1.45 + k * 0.72, 0.06, zzz))
			zmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# white obelisk by the plaza
	var obk := CylinderMesh.new()
	obk.top_radius = 0.10
	obk.bottom_radius = 0.42
	obk.height = 3.4
	obk.radial_segments = 4
	var omi: MeshInstance3D = host._mi(parent, obk, host._toon(Color(0.93, 0.93, 0.90), 0.15), c + Vector3(2.6, 1.7, 8.5))
	omi.rotation_degrees = Vector3(0, 45, 0)
	host._obstacles.append({"pos": c + Vector3(2.6, 0, 8.5), "r": 0.6})

	# ── market carts, flower beds, string lights, banners ────────────────────
	_cart(parent, host, c + Vector3(-6.2, 0, 5.8), 24.0, Color(0.86, 0.42, 0.38))
	_cart(parent, host, c + Vector3(5.6, 0, 7.6), -38.0, Color(0.42, 0.60, 0.86))
	_flower_bed(parent, host, c + Vector3(-7.6, 0, 11.5), Color(0.92, 0.5, 0.62))
	_flower_bed(parent, host, c + Vector3(6.9, 0, 11.8), Color(0.95, 0.72, 0.3))
	_flower_bed(parent, host, c + Vector3(-7.4, 0, -1.6), Color(0.62, 0.55, 0.95))
	_flower_bed(parent, host, c + Vector3(7.6, 0, -3.2), Color(0.88, 0.42, 0.4))
	_string_lights(parent, host, c + Vector3(3.2, 2.55, 10.5), c + Vector3(-3.2, 2.55, -2.5))
	_string_lights(parent, host, c + Vector3(-3.2, 2.55, -2.5), c + Vector3(3.2, 2.55, -12.5))
	_banners(parent, host, c + Vector3(-3.9, 2.5, -8.0), c + Vector3(3.9, 2.5, -8.0))
	_banners(parent, host, c + Vector3(-3.9, 2.6, 14.0), c + Vector3(3.9, 2.6, 14.0))

	# ── the vendor street: six small shops ───────────────────────────────────
	var shop_cols := [
		Color(0.86, 0.42, 0.38), Color(0.42, 0.60, 0.86), Color(0.45, 0.72, 0.45),
		Color(0.92, 0.74, 0.34), Color(0.88, 0.52, 0.72), Color(0.42, 0.74, 0.72),
	]
	for i in 6:
		var side := -1.0 if i % 2 == 0 else 1.0
		var sz := -2.0 - float(i / 2) * 7.5
		_shop(parent, host, c + Vector3(side * 9.5, 0, sz), -side * 90.0, shop_cols[i])

	# ── the mall (exterior + walk-in interior) ───────────────────────────────
	_mall(parent, host, c + Vector3(0, 0, -24))
	_mall_interior(parent, host)
	_mall_l2(parent, host)
	_mall_l3(parent, host)
	_wf_cinema(parent, host)
	_wf_her(parent, host)
	_wf_him(parent, host)
	_wf_hall_fun(parent, host)
	_lift_stop(parent, host, Vector3.ZERO, host.MALL_IN + Vector3(235.7, 0, 4.8), 180.0, [
		{"label": "Floor 1", "to": host.MALL_IN + Vector3(-5.2, 0, 0), "yaw": PI * 0.5},
		{"label": "Level 2 — for Her", "to": host.MALL_IN + Vector3(35.7, 0, 2.6), "yaw": PI},
		{"label": "Lounge — for Him", "to": host.MALL_IN + Vector3(75.7, 0, 2.6), "yaw": PI},
	])

	# ── the dense sides: robo-townhouses, mini gardens, strolling bots ───────
	_townhouse(parent, host, c + Vector3(-18.0, 0, 9.0), 90.0, Color(0.85, 0.74, 0.62), 2)
	_townhouse(parent, host, c + Vector3(-21.0, 0, 1.0), 90.0, Color(0.64, 0.74, 0.86), 3)
	_townhouse(parent, host, c + Vector3(-18.5, 0, -7.0), 90.0, Color(0.78, 0.68, 0.84), 2)
	_townhouse(parent, host, c + Vector3(18.0, 0, 6.0), -90.0, Color(0.72, 0.82, 0.7), 3)
	_townhouse(parent, host, c + Vector3(21.0, 0, 1.0), -90.0, Color(0.88, 0.78, 0.6), 2)
	_townhouse(parent, host, c + Vector3(18.5, 0, -7.5), -90.0, Color(0.66, 0.78, 0.82), 2)
	_garden(parent, host, c + Vector3(-15.5, 0, -14.0))
	_garden(parent, host, c + Vector3(-12.0, 0, 18.0))
	_garden(parent, host, c + Vector3(12.5, 0, -16.0))
	host._bench(parent, c + Vector3(-11.0, 0, 16.5), 142.0)
	host._bench(parent, c + Vector3(11.8, 0, -14.5), -120.0)
	# the big fenced PARK: flowers, water, playground (north-east quarter)
	_park(parent, host, c + Vector3(18.5, 0, 14.5))

	# ── the SOUTH quarter: gate, pond park, food corner, statue ──────────────
	_gate(parent, host, c + Vector3(0, 0, 24))
	# bush rows lining the south street
	for k in 3:
		_bush_clump(parent, host, c + Vector3(-4.7, 0, 16.0 + float(k) * 2.6), 1.0)
		_bush_clump(parent, host, c + Vector3(4.7, 0, 16.6 + float(k) * 2.6), 1.0)
	# pond park (south-east): mirror water, trees, a bench to rest on
	var pond := CylinderMesh.new()
	pond.top_radius = 2.3
	pond.bottom_radius = 2.3
	pond.height = 0.07
	pond.radial_segments = 22
	var pwm := ShaderMaterial.new()
	pwm.shader = host.WATER_SHADER
	var pdmi: MeshInstance3D = host._mi(parent, pond, pwm, c + Vector3(10, 0.05, 21))
	pdmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var prim := TorusMesh.new()
	prim.inner_radius = 2.25
	prim.outer_radius = 2.5
	prim.rings = 26
	prim.ring_segments = 6
	host._mi(parent, prim, host._toon(Color(0.72, 0.7, 0.66), 0.1), c + Vector3(10, 0.1, 21))
	host._obstacles.append({"pos": c + Vector3(10, 0, 21), "r": 2.65})
	host._tree(parent, c + Vector3(13.2, 0, 18.6), 1.0, 1)
	host._tree(parent, c + Vector3(7.4, 0, 24.2), 0.95, 0)
	host._bench(parent, c + Vector3(6.6, 0, 20.0), 64.0)
	_flower_bed(parent, host, c + Vector3(13.4, 0, 23.6), Color(0.7, 0.5, 0.92))
	# food corner (south-west): two more carts under string lights
	_cart(parent, host, c + Vector3(-9.5, 0, 20.0), 14.0, Color(0.42, 0.74, 0.72))
	_cart(parent, host, c + Vector3(-9.8, 0, 24.2), -28.0, Color(0.92, 0.74, 0.34))
	host._lamp(parent, c + Vector3(-7.4, 0, 18.2))
	host._lamp(parent, c + Vector3(-9.4, 0, 27.0))
	_string_lights(parent, host, c + Vector3(-7.4, 2.55, 18.2), c + Vector3(-9.4, 2.55, 27.0))
	# a little robot statue greets arrivals from the south
	_statue(parent, host, c + Vector3(3.4, 0, 17.0))
	# corner townhouses fill the far edges
	_townhouse(parent, host, c + Vector3(-24.0, 0, 21.0), 125.0, Color(0.82, 0.7, 0.78), 2)
	_townhouse(parent, host, c + Vector3(24.0, 0, 20.5), -125.0, Color(0.68, 0.8, 0.74), 3)
	_townhouse(parent, host, c + Vector3(-24.5, 0, -17.0), 90.0, Color(0.86, 0.8, 0.64), 2)
	_townhouse(parent, host, c + Vector3(23.5, 0, -18.0), -90.0, Color(0.7, 0.74, 0.88), 2)
	# south residents
	_npc(parent, host, c + Vector3(10, 0, 21), 3.6, 0.05, "Mochi", "did:verse:npc-mochi")
	_npc(parent, host, c + Vector3(-11, 0, 21.5), 2.9, -0.06, "Zip", "did:verse:npc-zip")
	_bird(parent, host, c + Vector3(0, 0, 20), 8.0, 5.5, 0.24)
	# resident bots out for a stroll (slow orbits, walk anim on)
	_npc(parent, host, c + Vector3(0, 0, 4), 6.4, 0.05, "Nova", "did:verse:npc-nova")
	_npc(parent, host, c + Vector3(-15.5, 0, -14.0), 2.6, 0.12, "Bolt", "did:verse:npc-bolt")
	_npc(parent, host, c + Vector3(17.5, 0, 3.0), 3.4, -0.09, "Pixel", "did:verse:npc-pixel")
	_npc(parent, host, c + Vector3(0, 0, -8.0), 8.3, -0.04, "Echo", "did:verse:npc-echo")
	_npc(parent, host, c + Vector3(0, 0, -13.0), 3.8, 0.06, "Gizmo", "did:verse:npc-gizmo")
	# a robot family strolling the park ring: two grown-ups + a little one
	_npc(parent, host, c + Vector3(0, 0, 4), 10.6, 0.03, "Tek", "did:verse:npc-tek")
	_npc(parent, host, c + Vector3(0, 0, 4), 11.2, 0.03, "Lumi", "did:verse:npc-lumi", 1.0, 0.06)
	_npc(parent, host, c + Vector3(0, 0, 4), 10.9, 0.03, "Bitty", "did:verse:npc-bitty", 0.55, 0.13)
	# and another little family near the west houses
	_npc(parent, host, c + Vector3(-18.0, 0, 3.0), 3.6, 0.07, "Juno", "did:verse:npc-juno")
	_npc(parent, host, c + Vector3(-18.0, 0, 3.0), 3.2, 0.07, "Dot", "did:verse:npc-dot", 0.55, 0.3)
	_npc(parent, host, c + Vector3(-18.0, 0, 3.0), 4.0, 0.07, "Chip", "did:verse:npc-chip", 0.55, 0.55)
	# browsers inside the mall hall
	_npc(parent, host, host.MALL_IN + Vector3(0, 0, 0), 4.8, 0.05, "Vee", "did:verse:npc-vee")
	_npc(parent, host, host.MALL_IN + Vector3(0, 0, 0), 6.2, -0.04, "Rune", "did:verse:npc-rune")
	# and Sash himself — the creator of Elacity, out for his plaza walk
	_sash(parent, host, c + Vector3(0, 0, 4), 7.9, -0.028)
	# one of the Elacity devs, resting on a bench by the east garden
	_devbot(parent, host, c + Vector3(13.6, 0, 11.2), -58.0)

	# ── dressing: lamps, benches, trees ──────────────────────────────────────
	host._lamp(parent, c + Vector3(3.2, 0, 10.5))
	host._lamp(parent, c + Vector3(-3.2, 0, -2.5))
	host._lamp(parent, c + Vector3(3.2, 0, -12.5))
	host._bench(parent, c + Vector3(-5.2, 0, 9.0), 38.0)
	host._bench(parent, c + Vector3(5.4, 0, 1.0), -32.0)
	host._tree(parent, c + Vector3(-13.0, 0, 12.0), 1.1, 0)
	host._tree(parent, c + Vector3(13.5, 0, 10.0), 1.0, 2)
	host._tree(parent, c + Vector3(-14.0, 0, -12.0), 1.05, 1)
	host._tree(parent, c + Vector3(14.0, 0, -14.0), 1.1, 0)

	# ── skyline (backdrop beyond the walkable edge) ──────────────────────────
	_tower(parent, host, c + Vector3(-16, 0, -33), 30.0, 3)   # the leaf hero
	_tower(parent, host, c + Vector3(-27, 0, 16), 16.0, 0)
	_tower(parent, host, c + Vector3(-29, 0, 2), 22.0, 1)
	_tower(parent, host, c + Vector3(-27, 0, -14), 13.0, 2)
	_tower(parent, host, c + Vector3(27, 0, 18), 14.0, 2)
	_tower(parent, host, c + Vector3(29, 0, 4), 19.0, 0)
	_tower(parent, host, c + Vector3(28, 0, -12), 24.0, 1)
	_tower(parent, host, c + Vector3(-23, 0, -26), 18.0, 0)
	_tower(parent, host, c + Vector3(15, 0, -29), 21.0, 2)
	host._tree(parent, c + Vector3(-20, 0, -21), 1.2, 1)
	host._tree(parent, c + Vector3(22, 0, -22), 1.15, 0)
	host._tree(parent, c + Vector3(-24, 0, 24), 1.2, 2)
	host._tree(parent, c + Vector3(24, 0, 24), 1.1, 1)

	# ── ALIVE v2: traffic, vendors, crowds, pets, billboards, far skyline ────
	_alive(parent, host, c)
	# ── DENSE v2: rail ring, packed houses, fences, paths, stalls, greenery ──
	_dense(parent, host, c)


## A motion node from the main pack (spin + bob), or a plain Node3D fallback.
func _spinner(parent: Node3D, pos: Vector3, speed: float, bob: float) -> Node3D:
	var s_script: GDScript = load("res://spinner.gd")
	var n: Node3D = s_script.new() if s_script else Node3D.new()
	n.position = pos
	if s_script:
		n.speed = speed
		n.bob = bob
	parent.add_child(n)
	return n


## A little hover tram that rides the orbit its parent spinner provides.
func _tram(parent: Node3D, host: Node, offset: Vector3, yaw_deg: float) -> void:
	var t := Node3D.new()
	t.position = offset
	t.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(t)
	var body := BoxMesh.new()
	body.size = Vector3(2.4, 0.65, 1.05)
	host._mi(t, body, host._toon(Color(0.93, 0.94, 0.97), 0.2, false, 0.0, 0.5, 0.45), Vector3.ZERO)
	var canopy := BoxMesh.new()
	canopy.size = Vector3(1.5, 0.42, 0.95)
	var cmat: StandardMaterial3D = host._glass_mat()
	host._windows.append(cmat)
	host._mi(t, canopy, cmat, Vector3(0.1, 0.5, 0))
	var strip := BoxMesh.new()
	strip.size = Vector3(2.4, 0.07, 0.07)
	for sz in [-0.56, 0.56]:
		var szz: float = sz
		var smi6: MeshInstance3D = host._mi(t, strip, VerseAvatar.glow_mat(CYAN, 1.3), Vector3(0, -0.3, szz))
		smi6.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var skid := BoxMesh.new()
	skid.size = Vector3(1.6, 0.08, 0.16)
	for sz2 in [-0.4, 0.4]:
		var szz2: float = sz2
		host._mi(t, skid, host._toon(Color(0.5, 0.55, 0.62), 0.1, false), Vector3(0, -0.42, szz2))


## A robo-townhouse: pastel block, flat roof, warm window grid, neon underline.
func _townhouse(parent: Node3D, host: Node, pos: Vector3, yaw_deg: float, col: Color, floors: int) -> void:
	var s := Node3D.new()
	s.position = pos
	s.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(s)
	var hgt := 2.4 * floors
	host._boxes.append({"pos": pos, "half": Vector2(2.4, 2.5)})
	var body := BoxMesh.new()
	body.size = Vector3(4.2, hgt, 4.4)
	host._mi(s, body, host._toon(col), Vector3(0, hgt * 0.5, 0))
	var roof := BoxMesh.new()
	roof.size = Vector3(4.5, 0.22, 4.7)
	host._mi(s, roof, host._toon(col.darkened(0.35)), Vector3(0, hgt + 0.11, 0))
	var neon := BoxMesh.new()
	neon.size = Vector3(4.3, 0.05, 0.05)
	var nmi3: MeshInstance3D = host._mi(s, neon, VerseAvatar.glow_mat(CYAN, 1.1), Vector3(0, hgt - 0.12, 2.24))
	nmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var door := BoxMesh.new()
	door.size = Vector3(0.85, 1.7, 0.1)
	host._mi(s, door, host._toon(Color(0.4, 0.3, 0.24)), Vector3(1.2, 0.85, 2.23))
	var win := BoxMesh.new()
	win.size = Vector3(0.62, 0.5, 0.06)
	var wmat: StandardMaterial3D = VerseAvatar.glow_mat(Color(1.0, 0.9, 0.66), 0.5)
	for fy in floors:
		for wx in 3:
			if fy == 0 and wx == 2:
				continue   # the door takes this spot
			var wmi3: MeshInstance3D = host._mi(s, win, wmat, Vector3(-1.3 + wx * 1.1, 1.45 + fy * 2.4, 2.25))
			wmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## A real bush: a clump of overlapping wind-swayed spheres (no green boxes).
func _bush_clump(parent: Node3D, host: Node, pos: Vector3, s: float = 1.0) -> void:
	host._obstacles.append({"pos": pos, "r": 0.5 * s})
	var bcol := Color(0.34, 0.58, 0.32)
	var offs := [
		Vector3(0, 0.34, 0), Vector3(0.3, 0.26, 0.12), Vector3(-0.26, 0.24, -0.1),
		Vector3(0.05, 0.3, -0.26), Vector3(-0.08, 0.27, 0.25),
	]
	var sizes := [0.42, 0.3, 0.28, 0.26, 0.27]
	for k in offs.size():
		var sph := SphereMesh.new()
		var rr: float = sizes[k] * s
		sph.radius = rr
		sph.height = rr * 2.0
		sph.radial_segments = 10
		sph.rings = 5
		var off: Vector3 = offs[k]
		host._mi(parent, sph, host._toon(bcol.lightened(0.05 * float(k % 3)), 0.25, true, 0.14, 0.75),
			pos + off * s)


## A mini garden: bush corners, flower bed, young tree.
func _garden(parent: Node3D, host: Node, pos: Vector3) -> void:
	_flower_bed(parent, host, pos, Color(0.92, 0.6, 0.4))
	host._tree(parent, pos + Vector3(1.9, 0, 1.3), 0.8, 2)
	for k in 4:
		var ang := TAU * float(k) / 4.0 + 0.4
		_bush_clump(parent, host, pos + Vector3(cos(ang) * 2.6, 0, sin(ang) * 2.6), 0.9)


## A resident bot out for a stroll — orbits its patch with the walk anim on.
## scale 0.55 ≈ a robot kid; phase offsets members of a family so they walk
## in formation on the same circle.
func _npc(parent: Node3D, host: Node, center: Vector3, r: float, speed: float, bot_name: String, did: String, bot_scale: float = 1.0, phase: float = 0.0) -> void:
	var orbit: Node3D = _spinner(parent, center, speed, 0.0)
	if phase != 0.0:
		orbit.rotate_y(phase)
	var bot: VerseAvatar = VerseAvatar.new()
	bot.display_name = bot_name
	bot.base_color = Net.did_color(did)
	bot.outfit = VerseAvatar.resolve_outfit(did, {})
	bot.moving = true
	bot.position = Vector3(r, 0, 0)
	bot.rotation.y = PI if speed > 0.0 else 0.0
	bot.scale = Vector3.ONE * bot_scale
	orbit.add_child(bot)
## Sash — the creator of Elacity, as a human figure (not a robot): red cap,
## red track jacket with the gold chevron + medal, jeans, sneakers, a real
## face — properly WALKING (limb pivots driven by the main pack's walker),
## and you can tap him to chat.
func _sash(parent: Node3D, host: Node, center: Vector3, r: float, speed: float) -> void:
	var orbit: Node3D = _spinner(parent, center, speed, 0.0)
	var s := Node3D.new()
	s.position = Vector3(r, 0, 0)
	s.rotation.y = PI if speed > 0.0 else 0.0
	orbit.add_child(s)
	var rig := Node3D.new()
	rig.name = "Rig"
	s.add_child(rig)
	var skin := Color(0.93, 0.77, 0.62)
	var blazer := Color(0.72, 0.16, 0.18)        # the red suit
	var tee := Color(0.10, 0.10, 0.12)
	var trousers := Color(0.62, 0.13, 0.15)      # suit trousers, same red family
	var beard_col := Color(0.72, 0.55, 0.34)
	var gold := Color(0.87, 0.72, 0.32)
	# legs = hip pivots the walker swings; dark trousers + shoes
	var leg := CylinderMesh.new()
	leg.top_radius = 0.085
	leg.bottom_radius = 0.095
	leg.height = 0.5
	leg.radial_segments = 10
	var shoe := BoxMesh.new()
	shoe.size = Vector3(0.15, 0.09, 0.26)
	for lx in [-0.11, 0.11]:
		var lxx: float = lx
		var piv := Node3D.new()
		piv.name = "LegL" if lxx < 0.0 else "LegR"
		piv.position = Vector3(lxx, 0.55, 0)
		rig.add_child(piv)
		host._mi(piv, leg, VerseAvatar.toon_mat(trousers, 0.25), Vector3(0, -0.275, 0))
		host._mi(piv, shoe, VerseAvatar.toon_mat(Color(0.12, 0.12, 0.14), 0.2), Vector3(0, -0.51, 0.04))
	# charcoal blazer over a black tee
	var torso := CapsuleMesh.new()
	torso.radius = 0.235
	torso.height = 0.64
	var tmi: MeshInstance3D = host._mi(rig, torso, VerseAvatar.toon_mat(blazer, 0.25), Vector3(0, 0.82, 0))
	tmi.scale = Vector3(1.0, 1.0, 0.84)
	var shirt := BoxMesh.new()
	shirt.size = Vector3(0.17, 0.34, 0.03)
	host._mi(rig, shirt, VerseAvatar.toon_mat(tee, 0.15, false), Vector3(0, 0.92, 0.195))
	var lapel := BoxMesh.new()
	lapel.size = Vector3(0.055, 0.3, 0.02)
	for k in 2:
		var lmi2: MeshInstance3D = host._mi(rig, lapel, VerseAvatar.toon_mat(blazer.darkened(0.18), 0.2, false),
			Vector3(-0.1 if k == 0 else 0.1, 0.96, 0.212))
		lmi2.rotation_degrees = Vector3(0, 0, -16.0 if k == 0 else 16.0)
	# arms = shoulder pivots (walker swings them opposite the legs)
	var arm := CapsuleMesh.new()
	arm.radius = 0.065
	arm.height = 0.42
	var hand := SphereMesh.new()
	hand.radius = 0.055
	hand.height = 0.11
	hand.radial_segments = 8
	hand.rings = 4
	for ax in [-0.30, 0.30]:
		var axx: float = ax
		var apiv := Node3D.new()
		apiv.name = "ArmL" if axx < 0.0 else "ArmR"
		apiv.position = Vector3(axx, 1.06, 0)
		rig.add_child(apiv)
		var ami: MeshInstance3D = host._mi(apiv, arm, VerseAvatar.toon_mat(blazer, 0.25), Vector3(0, -0.22, 0))
		ami.rotation_degrees = Vector3(0, 0, 10.0 if axx > 0.0 else -10.0)
		host._mi(apiv, hand, VerseAvatar.toon_mat(skin, 0.3), Vector3(0, -0.45, 0.02))
	# head — bald, with a full short blond beard + mustache (the real Sash)
	var head := SphereMesh.new()
	head.radius = 0.205
	head.height = 0.41
	head.radial_segments = 14
	head.rings = 8
	host._mi(rig, head, VerseAvatar.toon_mat(skin, 0.3), Vector3(0, 1.38, 0.02))
	var beard := BoxMesh.new()
	beard.size = Vector3(0.21, 0.105, 0.06)
	host._mi(rig, beard, VerseAvatar.toon_mat(beard_col, 0.25), Vector3(0, 1.245, 0.15))
	var stache := BoxMesh.new()
	stache.size = Vector3(0.105, 0.026, 0.03)
	host._mi(rig, stache, VerseAvatar.toon_mat(beard_col.darkened(0.08), 0.25), Vector3(0, 1.318, 0.196))
	var sclera := SphereMesh.new()
	sclera.radius = 0.034
	sclera.height = 0.068
	sclera.radial_segments = 8
	sclera.rings = 4
	var pupil := SphereMesh.new()
	pupil.radius = 0.017
	pupil.height = 0.034
	pupil.radial_segments = 6
	pupil.rings = 3
	var brow := BoxMesh.new()
	brow.size = Vector3(0.06, 0.014, 0.014)
	for ex in [-0.07, 0.07]:
		var exx: float = ex
		host._mi(rig, sclera, VerseAvatar.toon_mat(Color(0.97, 0.97, 0.97), 0.1, false), Vector3(exx, 1.4, 0.175))
		host._mi(rig, pupil, VerseAvatar.toon_mat(Color(0.18, 0.13, 0.1), 0.1, false), Vector3(exx, 1.4, 0.205))
		var bmi4: MeshInstance3D = host._mi(rig, brow, VerseAvatar.toon_mat(beard_col.darkened(0.1), 0.1, false), Vector3(exx, 1.465, 0.185))
		bmi4.rotation_degrees = Vector3(0, 0, -8.0 if exx < 0.0 else 8.0)
	var nose := SphereMesh.new()
	nose.radius = 0.026
	nose.height = 0.052
	nose.radial_segments = 6
	nose.rings = 3
	host._mi(rig, nose, VerseAvatar.toon_mat(skin.darkened(0.06), 0.2, false), Vector3(0, 1.36, 0.21))
	var smile := BoxMesh.new()
	smile.size = Vector3(0.07, 0.013, 0.012)
	host._mi(rig, smile, VerseAvatar.toon_mat(Color(0.45, 0.25, 0.2), 0.1, false), Vector3(0.005, 1.318, 0.2))
	# the red hat to match the suit
	var brim := BoxMesh.new()
	brim.size = Vector3(0.27, 0.025, 0.17)
	host._mi(rig, brim, VerseAvatar.toon_mat(blazer.darkened(0.06), 0.25), Vector3(0, 1.515, 0.17))
	var dome := SphereMesh.new()
	dome.radius = 0.205
	dome.height = 0.41
	dome.radial_segments = 12
	dome.rings = 6
	var dmi: MeshInstance3D = host._mi(rig, dome, VerseAvatar.toon_mat(blazer.darkened(0.06), 0.25), Vector3(0, 1.52, -0.01))
	dmi.scale = Vector3(1.04, 0.55, 1.02)
	# natural walk: the main pack's walker swings the limb pivots
	var wk_s: GDScript = load("res://walker.gd")
	if wk_s:
		var wk: Node = wk_s.new()
		s.add_child(wk)
		wk.setup(rig)
		wk.orbit = orbit
		wk.orbit_speed = speed
	# no floating text on Sash — tapping him opens the FAQ popup instead
	# (the in-world bubble only appears outside the app, e.g. desktop)
	host._talkers.append({
		"node": s, "i": 0, "sheet": "sash_faq", "silent": true,
		"lines": [
			"welcome to Ela City!",
			"I built this place for creators.",
			"everything here is truly yours.",
			"the mall shops open soon — stay tuned!",
			"nice robot — very you.",
		],
	})
	if gold.a > 2.0:
		pass # (keeps `gold` referenced; tag removed by design)


## An Elacity dev taking a well-earned break: brown hair, rectangular
## glasses, stubble, red shirt — seated on a bench. Tap him to chat.
func _devbot(parent: Node3D, host: Node, pos: Vector3, yaw_deg: float) -> void:
	host._bench(parent, pos, yaw_deg)
	var a := deg_to_rad(yaw_deg)
	var s := Node3D.new()
	s.position = pos + Vector3(cos(a) * 0.3, 0.44, -sin(a) * 0.3)
	s.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(s)
	var skin := Color(0.93, 0.78, 0.64)
	var hair := Color(0.36, 0.26, 0.17)
	var stub := Color(0.58, 0.45, 0.32)
	var shirt := Color(0.62, 0.82, 0.58)   # light green tee
	var pants := Color(0.55, 0.74, 0.52)   # matching shorts
	# seated: hips on the bench, thighs forward, shins down to the ground
	var hips := BoxMesh.new()
	hips.size = Vector3(0.32, 0.14, 0.24)
	host._mi(s, hips, VerseAvatar.toon_mat(pants, 0.25), Vector3(0, 0.05, 0.02))
	var thigh := CylinderMesh.new()
	thigh.top_radius = 0.075
	thigh.bottom_radius = 0.075
	thigh.height = 0.32
	thigh.radial_segments = 10
	var shin := CylinderMesh.new()
	shin.top_radius = 0.062
	shin.bottom_radius = 0.062
	shin.height = 0.34
	shin.radial_segments = 10
	var shoe := BoxMesh.new()
	shoe.size = Vector3(0.14, 0.08, 0.24)
	for lx in [-0.09, 0.09]:
		var lxx: float = lx
		# shorts cover the thighs; the shins are bare (summer dev energy)
		var tmi2: MeshInstance3D = host._mi(s, thigh, VerseAvatar.toon_mat(pants, 0.25), Vector3(lxx, 0.08, 0.18))
		tmi2.rotation_degrees = Vector3(90, 0, 0)
		host._mi(s, shin, VerseAvatar.toon_mat(skin, 0.25), Vector3(lxx, -0.13, 0.32))
		host._mi(s, shoe, VerseAvatar.toon_mat(Color(0.15, 0.15, 0.17), 0.2), Vector3(lxx, -0.31, 0.38))
	# red shirt torso, leaned back a touch
	var torso := CapsuleMesh.new()
	torso.radius = 0.21
	torso.height = 0.56
	var tomi: MeshInstance3D = host._mi(s, torso, VerseAvatar.toon_mat(shirt, 0.3), Vector3(0, 0.34, -0.02))
	tomi.scale = Vector3(1.0, 1.0, 0.8)
	tomi.rotation_degrees = Vector3(-7, 0, 0)
	# arms resting toward the knees
	var arm := CapsuleMesh.new()
	arm.radius = 0.06
	arm.height = 0.36
	var hand := SphereMesh.new()
	hand.radius = 0.05
	hand.height = 0.1
	hand.radial_segments = 8
	hand.rings = 4
	for ax in [-0.24, 0.24]:
		var axx: float = ax
		var ami2: MeshInstance3D = host._mi(s, arm, VerseAvatar.toon_mat(shirt.darkened(0.06), 0.3), Vector3(axx, 0.3, 0.1))
		ami2.rotation_degrees = Vector3(-38, 0, 8.0 if axx > 0.0 else -8.0)
		host._mi(s, hand, VerseAvatar.toon_mat(skin, 0.3), Vector3(axx * 0.7, 0.12, 0.26))
	# head: brown hair, stubble, rectangular glasses
	var head := SphereMesh.new()
	head.radius = 0.185
	head.height = 0.37
	head.radial_segments = 14
	head.rings = 8
	host._mi(s, head, VerseAvatar.toon_mat(skin, 0.3), Vector3(0, 0.78, 0.02))
	var hairm := SphereMesh.new()
	hairm.radius = 0.19
	hairm.height = 0.38
	hairm.radial_segments = 12
	hairm.rings = 6
	var hmi3: MeshInstance3D = host._mi(s, hairm, VerseAvatar.toon_mat(hair, 0.25), Vector3(0, 0.83, -0.04))
	hmi3.scale = Vector3(1.02, 0.9, 0.98)
	var stubble := BoxMesh.new()
	stubble.size = Vector3(0.18, 0.08, 0.05)
	host._mi(s, stubble, VerseAvatar.toon_mat(stub, 0.25), Vector3(0, 0.665, 0.13))
	# glasses: dark rims with pale lenses + bridge + temples
	var rim := BoxMesh.new()
	rim.size = Vector3(0.085, 0.06, 0.014)
	var lens := BoxMesh.new()
	lens.size = Vector3(0.068, 0.044, 0.016)
	for ex in [-0.062, 0.062]:
		var exx: float = ex
		host._mi(s, rim, VerseAvatar.toon_mat(Color(0.13, 0.12, 0.12), 0.1, false), Vector3(exx, 0.795, 0.165))
		host._mi(s, lens, VerseAvatar.toon_mat(Color(0.78, 0.84, 0.9), 0.1, false), Vector3(exx, 0.795, 0.169))
	var bridge := BoxMesh.new()
	bridge.size = Vector3(0.045, 0.012, 0.012)
	host._mi(s, bridge, VerseAvatar.toon_mat(Color(0.13, 0.12, 0.12), 0.1, false), Vector3(0, 0.8, 0.168))
	var temple := BoxMesh.new()
	temple.size = Vector3(0.012, 0.012, 0.14)
	for tx in [-0.105, 0.105]:
		var txx: float = tx
		host._mi(s, temple, VerseAvatar.toon_mat(Color(0.13, 0.12, 0.12), 0.1, false), Vector3(txx, 0.795, 0.095))
	var nose2 := SphereMesh.new()
	nose2.radius = 0.024
	nose2.height = 0.048
	nose2.radial_segments = 6
	nose2.rings = 3
	host._mi(s, nose2, VerseAvatar.toon_mat(skin.darkened(0.06), 0.2, false), Vector3(0, 0.765, 0.185))
	var mouth := BoxMesh.new()
	mouth.size = Vector3(0.06, 0.012, 0.012)
	host._mi(s, mouth, VerseAvatar.toon_mat(Color(0.45, 0.25, 0.2), 0.1, false), Vector3(0, 0.71, 0.175))
	# name tag + tap-to-talk
	var tag := Label3D.new()
	tag.text = "Dev"
	tag.font_size = 48
	tag.pixel_size = 0.004
	tag.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	tag.modulate = Color(0.8, 0.87, 1.0)
	tag.outline_size = 8
	tag.position = Vector3(0, 1.25, 0)
	s.add_child(tag)
	host._talkers.append({
		"node": s, "i": 0,
		"lines": [
			"I must rest… all that coding of ElastOS.",
			"shipping the world computer is heavy work!",
			"ok — five more minutes, then back to the code.",
		],
	})


## The big square park: wooden fence with a west entrance, lawns, flowers,
## a little pond, and a PLAYGROUND — slide, swaying swings, a seesaw — with
## a robot kid making the most of it.
func _park(parent: Node3D, host: Node, p: Vector3) -> void:
	# lawn (own height band over the slab)
	var lawn := BoxMesh.new()
	lawn.size = Vector3(13.0, 0.06, 9.0)
	var lmi3: MeshInstance3D = host._mi(parent, lawn, host._toon(Color(0.42, 0.67, 0.37), 0.05, false), p + Vector3(0, 0.035, 0))
	lmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# wooden fence: posts + two rails, entrance gap mid-west
	var wood: ShaderMaterial = host._toon(Color(0.62, 0.45, 0.28), 0.15)
	var post := BoxMesh.new()
	post.size = Vector3(0.12, 0.85, 0.12)
	var hx := 6.5
	var hz := 4.5
	for k in 9:   # north + south sides
		var fx := -hx + float(k) * (hx * 2.0 / 8.0)
		for sz in [-hz, hz]:
			var szz: float = sz
			host._mi(parent, post, wood, p + Vector3(fx, 0.42, szz))
	for k in 7:   # east side + west side (west keeps the entrance gap)
		var fz := -hz + float(k) * (hz * 2.0 / 6.0)
		host._mi(parent, post, wood, p + Vector3(hx, 0.42, fz))
		if absf(fz) > 1.1:
			host._mi(parent, post, wood, p + Vector3(-hx, 0.42, fz))
	var rail_n := BoxMesh.new()
	rail_n.size = Vector3(13.0, 0.07, 0.07)
	for sz2 in [-hz, hz]:
		var szz2: float = sz2
		for ry in [0.35, 0.66]:
			var ryy: float = ry
			host._mi(parent, rail_n, wood, p + Vector3(0, ryy, szz2))
	var rail_e := BoxMesh.new()
	rail_e.size = Vector3(0.07, 0.07, 9.0)
	for ry2 in [0.35, 0.66]:
		var ryy2: float = ry2
		host._mi(parent, rail_e, wood, p + Vector3(hx, ryy2, 0))
	var rail_w := BoxMesh.new()
	rail_w.size = Vector3(0.07, 0.07, 3.2)
	for ry3 in [0.35, 0.66]:
		var ryy3: float = ry3
		host._mi(parent, rail_w, wood, p + Vector3(-hx, ryy3, -2.85))
		host._mi(parent, rail_w, wood, p + Vector3(-hx, ryy3, 2.85))
	# fence solids (the west side leaves the entrance walkable)
	host._boxes.append({"pos": p + Vector3(0, 0, hz), "half": Vector2(6.5, 0.25)})
	host._boxes.append({"pos": p + Vector3(0, 0, -hz), "half": Vector2(6.5, 0.25)})
	host._boxes.append({"pos": p + Vector3(hx, 0, 0), "half": Vector2(0.25, 4.5)})
	host._boxes.append({"pos": p + Vector3(-hx, 0, -2.85), "half": Vector2(0.25, 1.65)})
	host._boxes.append({"pos": p + Vector3(-hx, 0, 2.85), "half": Vector2(0.25, 1.65)})
	# flowers + bushes + a tree
	_flower_bed(parent, host, p + Vector3(-3.6, 0, 2.6), Color(0.92, 0.5, 0.62))
	_flower_bed(parent, host, p + Vector3(-4.2, 0, -2.4), Color(0.95, 0.72, 0.3))
	_bush_clump(parent, host, p + Vector3(5.6, 0, 3.6), 0.9)
	_bush_clump(parent, host, p + Vector3(5.7, 0, -3.5), 0.9)
	host._tree(parent, p + Vector3(-1.2, 0, 3.2), 0.9, 1)
	host._bench(parent, p + Vector3(-2.0, 0, -3.4), 14.0)
	# the pond (north-east corner of the lawn)
	var pond := CylinderMesh.new()
	pond.top_radius = 1.6
	pond.bottom_radius = 1.6
	pond.height = 0.06
	pond.radial_segments = 20
	var pwm2 := ShaderMaterial.new()
	pwm2.shader = host.WATER_SHADER
	var pmi2: MeshInstance3D = host._mi(parent, pond, pwm2, p + Vector3(4.2, 0.09, 2.4))
	pmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var prim2 := TorusMesh.new()
	prim2.inner_radius = 1.55
	prim2.outer_radius = 1.75
	prim2.rings = 22
	prim2.ring_segments = 6
	host._mi(parent, prim2, host._toon(Color(0.72, 0.7, 0.66), 0.1), p + Vector3(4.2, 0.12, 2.4))
	host._obstacles.append({"pos": p + Vector3(4.2, 0, 2.4), "r": 1.9})
	# ── the playground ───────────────────────────────────────────────────────
	# slide: ladder up, platform, bright ramp down
	var sl := Node3D.new()
	sl.position = p + Vector3(1.6, 0, -1.8)
	parent.add_child(sl)
	host._obstacles.append({"pos": sl.position, "r": 1.3})
	var plat := BoxMesh.new()
	plat.size = Vector3(0.8, 0.1, 0.8)
	host._mi(sl, plat, host._toon(Color(0.95, 0.77, 0.32), 0.2), Vector3(0, 1.1, 0))
	var ramp := BoxMesh.new()
	ramp.size = Vector3(0.62, 0.08, 2.0)
	var rampmi: MeshInstance3D = host._mi(sl, ramp, host._toon(Color(0.86, 0.34, 0.3), 0.25), Vector3(0, 0.62, 1.32))
	rampmi.rotation_degrees = Vector3(-29, 0, 0)
	var lpost := BoxMesh.new()
	lpost.size = Vector3(0.07, 1.1, 0.07)
	for cx in [-0.34, 0.34]:
		var cxx: float = cx
		host._mi(sl, lpost, wood, Vector3(cxx, 0.55, -0.34))
		host._mi(sl, lpost, wood, Vector3(cxx, 0.55, 0.34))
	var rung := BoxMesh.new()
	rung.size = Vector3(0.6, 0.05, 0.05)
	for k2 in 4:
		host._mi(sl, rung, wood, Vector3(0, 0.25 + float(k2) * 0.26, -0.36))
	# swing set: frame + two gently swaying swings (bob spinners)
	var sw := Node3D.new()
	sw.position = p + Vector3(-1.4, 0, 0.6)
	parent.add_child(sw)
	host._obstacles.append({"pos": sw.position + Vector3(-1.1, 0, 0), "r": 0.3})
	host._obstacles.append({"pos": sw.position + Vector3(1.1, 0, 0), "r": 0.3})
	var spost := BoxMesh.new()
	spost.size = Vector3(0.1, 1.7, 0.1)
	host._mi(sw, spost, wood, Vector3(-1.1, 0.85, 0))
	host._mi(sw, spost, wood, Vector3(1.1, 0.85, 0))
	var sbar := BoxMesh.new()
	sbar.size = Vector3(2.4, 0.09, 0.09)
	host._mi(sw, sbar, wood, Vector3(0, 1.72, 0))
	var chain := BoxMesh.new()
	chain.size = Vector3(0.03, 0.85, 0.03)
	var seat := BoxMesh.new()
	seat.size = Vector3(0.4, 0.05, 0.2)
	for sxo in [-0.55, 0.55]:
		var sxx2: float = sxo
		var swing: Node3D = _spinner(sw, Vector3(sxx2, 1.68, 0), 0.0, 0.07)
		host._mi(swing, chain, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), Vector3(-0.15, -0.43, 0))
		host._mi(swing, chain, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), Vector3(0.15, -0.43, 0))
		host._mi(swing, seat, host._toon(Color(0.35, 0.55, 0.85), 0.25), Vector3(0, -0.86, 0))
	# seesaw
	var ss := Node3D.new()
	ss.position = p + Vector3(2.6, 0, 0.8)
	ss.rotation_degrees = Vector3(0, 24, 0)
	parent.add_child(ss)
	host._obstacles.append({"pos": ss.position, "r": 0.9})
	var pivot2 := CylinderMesh.new()
	pivot2.top_radius = 0.12
	pivot2.bottom_radius = 0.16
	pivot2.height = 0.36
	pivot2.radial_segments = 10
	host._mi(ss, pivot2, wood, Vector3(0, 0.18, 0))
	var plank := BoxMesh.new()
	plank.size = Vector3(2.4, 0.07, 0.34)
	var plmi: MeshInstance3D = host._mi(ss, plank, host._toon(Color(0.95, 0.77, 0.32), 0.25), Vector3(0, 0.38, 0))
	plmi.rotation_degrees = Vector3(0, 0, 11)
	# a robot kid having the time of their life
	_npc(parent, host, p + Vector3(0.4, 0, -0.4), 2.1, 0.16, "Pip", "did:verse:npc-pip", 0.55)


## The south gate: two pylons + a glowing beam over the street — the city's
## front door, seen as you arrive.
func _gate(parent: Node3D, host: Node, pos: Vector3) -> void:
	var pylon := BoxMesh.new()
	pylon.size = Vector3(0.8, 5.2, 0.8)
	for px in [-4.4, 4.4]:
		var pxx: float = px
		host._mi(parent, pylon, host._toon(Color(0.66, 0.7, 0.76), 0.15), pos + Vector3(pxx, 2.6, 0))
		host._obstacles.append({"pos": pos + Vector3(pxx, 0, 0), "r": 0.75})
	var beam := BoxMesh.new()
	beam.size = Vector3(9.6, 0.5, 0.9)
	host._mi(parent, beam, host._toon(Color(0.72, 0.76, 0.82), 0.15), pos + Vector3(0, 5.35, 0))
	var glowline := BoxMesh.new()
	glowline.size = Vector3(8.8, 0.07, 0.07)
	var gmi: MeshInstance3D = host._mi(parent, glowline, VerseAvatar.glow_mat(CYAN, 1.4), pos + Vector3(0, 5.02, 0.2))
	gmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var tip := SphereMesh.new()
	tip.radius = 0.14
	tip.height = 0.28
	tip.radial_segments = 8
	tip.rings = 4
	for px2 in [-4.4, 4.4]:
		var pxx2: float = px2
		var tmi3: MeshInstance3D = host._mi(parent, tip, VerseAvatar.glow_mat(Color(0.95, 0.85, 0.5), 1.2), pos + Vector3(pxx2, 5.32, 0))
		tmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## A friendly robot statue on a pedestal (it blinks — a "living statue").
func _statue(parent: Node3D, host: Node, pos: Vector3) -> void:
	host._obstacles.append({"pos": pos, "r": 1.0})
	var ped := CylinderMesh.new()
	ped.top_radius = 0.7
	ped.bottom_radius = 0.85
	ped.height = 0.55
	ped.radial_segments = 14
	host._mi(parent, ped, host._toon(Color(0.74, 0.72, 0.68), 0.1), pos + Vector3(0, 0.275, 0))
	var ring := TorusMesh.new()
	ring.inner_radius = 0.62
	ring.outer_radius = 0.72
	ring.rings = 20
	ring.ring_segments = 6
	var rmi2: MeshInstance3D = host._mi(parent, ring, VerseAvatar.glow_mat(Color(0.95, 0.85, 0.5), 0.9), pos + Vector3(0, 0.56, 0))
	rmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bot: VerseAvatar = VerseAvatar.new()
	bot.display_name = ""
	bot.base_color = Color(0.78, 0.8, 0.84)
	bot.outfit = VerseAvatar.resolve_outfit("did:verse:statue", {"hat": ""})
	bot.position = pos + Vector3(0, 0.55, 0)
	bot.rotation.y = PI
	bot.scale = Vector3.ONE * 0.85
	parent.add_child(bot)


## A small robot bird on the wing.
func _bird(parent: Node3D, host: Node, center: Vector3, r: float, h: float, speed: float) -> void:
	var orbit: Node3D = _spinner(parent, center, speed, 0.25)
	var b := Node3D.new()
	b.position = Vector3(r, h, 0)
	b.rotation.y = PI if speed > 0.0 else 0.0
	orbit.add_child(b)
	var body := CapsuleMesh.new()
	body.radius = 0.09
	body.height = 0.34
	var bmi2: MeshInstance3D = host._mi(b, body, VerseAvatar.toon_mat(Color(0.95, 0.96, 0.98), 0.3, false), Vector3.ZERO)
	bmi2.rotation_degrees = Vector3(90, 0, 0)
	var head := SphereMesh.new()
	head.radius = 0.075
	head.height = 0.15
	head.radial_segments = 8
	head.rings = 4
	host._mi(b, head, VerseAvatar.toon_mat(Color(0.10, 0.16, 0.3), 0.3, false), Vector3(0, 0.05, 0.2))
	var eye := SphereMesh.new()
	eye.radius = 0.018
	eye.height = 0.036
	eye.radial_segments = 6
	eye.rings = 3
	for ex in [-0.032, 0.032]:
		var exx: float = ex
		var emi2: MeshInstance3D = host._mi(b, eye, VerseAvatar.glow_mat(Color(0.45, 0.9, 1.0), 1.6), Vector3(exx, 0.06, 0.26))
		emi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var beak := CylinderMesh.new()
	beak.top_radius = 0.005
	beak.bottom_radius = 0.025
	beak.height = 0.07
	beak.radial_segments = 6
	var kmi: MeshInstance3D = host._mi(b, beak, VerseAvatar.toon_mat(Color(0.95, 0.78, 0.35), 0.2, false), Vector3(0, 0.03, 0.3))
	kmi.rotation_degrees = Vector3(90, 0, 0)
	# wings that actually FLY: root-pivoted, beating in mirrored strokes —
	# each bird gets its own wingbeat so the flock never moves in lockstep
	var wing := BoxMesh.new()
	wing.size = Vector3(0.26, 0.02, 0.12)
	var beat := randf_range(0.16, 0.22)
	for wx in [-1.0, 1.0]:
		var wxx: float = wx
		var piv := Node3D.new()
		piv.position = Vector3(wxx * 0.05, 0.03, 0.02)
		piv.rotation_degrees = Vector3(0, 0, wxx * 28.0)
		b.add_child(piv)
		host._mi(piv, wing, VerseAvatar.toon_mat(Color(0.88, 0.91, 0.96), 0.3, false), Vector3(wxx * 0.13, 0, 0))
		var tw := piv.create_tween()
		tw.set_loops()
		tw.tween_property(piv, "rotation_degrees:z", wxx * -24.0, beat) \
			.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
		tw.tween_property(piv, "rotation_degrees:z", wxx * 34.0, beat) \
			.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	var tail := BoxMesh.new()
	tail.size = Vector3(0.1, 0.02, 0.14)
	host._mi(b, tail, VerseAvatar.toon_mat(Color(0.88, 0.91, 0.96), 0.3, false), Vector3(0, 0.01, -0.2))


## A lovely modern fountain: white round basin, slim column with two catch
## dishes, the animated water surface, a tall center jet + four arc jets
## (real moving water via particles), warm rim studs — and a positional
## babble source so you HEAR the water when you walk up to it.
func _fountain(parent: Node3D, host: Node, pos: Vector3) -> void:
	var fn := Node3D.new()
	fn.position = pos
	parent.add_child(fn)
	host._obstacles.append({"pos": pos, "r": 2.7})
	var basin := CylinderMesh.new()
	basin.top_radius = 2.5
	basin.bottom_radius = 2.65
	basin.height = 0.55
	basin.radial_segments = 26
	host._mi(fn, basin, host._toon(Color(0.93, 0.93, 0.95), 0.15), Vector3(0, 0.275, 0))
	var lip := TorusMesh.new()
	lip.inner_radius = 2.42
	lip.outer_radius = 2.62
	lip.rings = 30
	lip.ring_segments = 6
	host._mi(fn, lip, host._toon(Color(0.97, 0.97, 0.99), 0.2), Vector3(0, 0.56, 0))
	# the living water surface (animated mirror shader)
	var fw := CylinderMesh.new()
	fw.top_radius = 2.3
	fw.bottom_radius = 2.3
	fw.height = 0.08
	fw.radial_segments = 26
	var fwm := ShaderMaterial.new()
	fwm.shader = host.WATER_SHADER
	fwm.set_shader_parameter("rings", 1.0)   # the jet sends out ripple rings
	# clear of the basin's top face — coplanar surfaces flicker
	var fmi: MeshInstance3D = host._mi(fn, fw, fwm, Vector3(0, 0.6, 0))
	fmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# slim column with two catch dishes
	var colm := CylinderMesh.new()
	colm.top_radius = 0.16
	colm.bottom_radius = 0.22
	colm.height = 1.7
	colm.radial_segments = 12
	host._mi(fn, colm, host._toon(Color(0.93, 0.93, 0.95), 0.15), Vector3(0, 1.35, 0))
	var dish1 := CylinderMesh.new()
	dish1.top_radius = 1.05
	dish1.bottom_radius = 0.75
	dish1.height = 0.16
	dish1.radial_segments = 20
	host._mi(fn, dish1, host._toon(Color(0.95, 0.95, 0.97), 0.2), Vector3(0, 1.45, 0))
	var dish2 := CylinderMesh.new()
	dish2.top_radius = 0.62
	dish2.bottom_radius = 0.42
	dish2.height = 0.14
	dish2.radial_segments = 16
	host._mi(fn, dish2, host._toon(Color(0.95, 0.95, 0.97), 0.2), Vector3(0, 2.12, 0))
	# warm glow studs on the rim
	var stud := SphereMesh.new()
	stud.radius = 0.06
	stud.height = 0.12
	stud.radial_segments = 8
	stud.rings = 4
	for k in 6:
		var ang := TAU * float(k) / 6.0
		var stmi: MeshInstance3D = host._mi(fn, stud, VerseAvatar.glow_mat(Color(1.0, 0.9, 0.65), 1.0),
			Vector3(cos(ang) * 2.52, 0.62, sin(ang) * 2.52))
		stmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# MOVING WATER: tall center jet falling back into the dishes…
	var drop := SphereMesh.new()
	drop.radius = 0.05
	drop.height = 0.1
	drop.radial_segments = 6
	drop.rings = 3
	drop.material = VerseAvatar.glow_mat(Color(0.78, 0.93, 1.0), 0.45)
	var jet := CPUParticles3D.new()
	jet.amount = 42
	jet.lifetime = 1.15
	jet.mesh = drop
	jet.direction = Vector3(0, 1, 0)
	jet.spread = 6.0
	jet.initial_velocity_min = 4.4
	jet.initial_velocity_max = 5.0
	jet.gravity = Vector3(0, -9.6, 0)
	jet.scale_amount_min = 0.7
	jet.scale_amount_max = 1.2
	jet.position = Vector3(0, 2.25, 0)
	fn.add_child(jet)
	# …and four low arcs from the column into the basin
	for k in 4:
		var ang2 := TAU * float(k) / 4.0 + 0.4
		var arc := CPUParticles3D.new()
		arc.amount = 16
		arc.lifetime = 0.8
		arc.mesh = drop
		arc.direction = Vector3(cos(ang2) * 0.8, 1.0, sin(ang2) * 0.8).normalized()
		arc.spread = 4.0
		arc.initial_velocity_min = 2.3
		arc.initial_velocity_max = 2.7
		arc.gravity = Vector3(0, -9.6, 0)
		arc.scale_amount_min = 0.6
		arc.scale_amount_max = 1.0
		arc.position = Vector3(cos(ang2) * 0.3, 1.5, sin(ang2) * 0.3)
		fn.add_child(arc)
	# the sound of water, positional — audible when you're near
	var ws_script: GDScript = load("res://water_audio.gd")
	if ws_script:
		var ws: AudioStreamPlayer3D = ws_script.new()
		ws.position = Vector3(0, 1.0, 0)
		fn.add_child(ws)


## A short span of warm string lights (sagging line of little glow bulbs).
func _string_lights(parent: Node3D, host: Node, a: Vector3, b: Vector3) -> void:
	var bulb := SphereMesh.new()
	bulb.radius = 0.055
	bulb.height = 0.11
	bulb.radial_segments = 8
	bulb.rings = 4
	var n := 9
	for k in n:
		var t := float(k) / float(n - 1)
		var p := a.lerp(b, t)
		p.y -= sin(t * PI) * 0.7
		var bmi: MeshInstance3D = host._mi(parent, bulb, VerseAvatar.glow_mat(Color(1.0, 0.88, 0.6), 1.1), p)
		bmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## Pennant banners strung across the street.
func _banners(parent: Node3D, host: Node, a: Vector3, b: Vector3) -> void:
	var cols := [
		Color(0.86, 0.42, 0.38), Color(0.92, 0.74, 0.34), Color(0.42, 0.74, 0.72),
		Color(0.42, 0.60, 0.86), Color(0.88, 0.52, 0.72),
	]
	var flag := BoxMesh.new()
	flag.size = Vector3(0.18, 0.26, 0.02)
	var n := 9
	for k in n:
		var t := float(k) / float(n - 1)
		var p := a.lerp(b, t)
		p.y -= sin(t * PI) * 0.45 + 0.13
		var col: Color = cols[k % cols.size()]
		var fmi2: MeshInstance3D = host._mi(parent, flag, host._toon(col, 0.2, false), p)
		fmi2.rotation_degrees = Vector3(0, 0, 8.0 if k % 2 == 0 else -8.0)


## A round flower bed: stone ring, soil, a cluster of bright blooms.
func _flower_bed(parent: Node3D, host: Node, pos: Vector3, col: Color) -> void:
	host._obstacles.append({"pos": pos, "r": 1.0})
	var ring := CylinderMesh.new()
	ring.top_radius = 0.95
	ring.bottom_radius = 1.0
	ring.height = 0.22
	ring.radial_segments = 16
	host._mi(parent, ring, host._toon(Color(0.8, 0.78, 0.74), 0.1), pos + Vector3(0, 0.11, 0))
	var soil := CylinderMesh.new()
	soil.top_radius = 0.85
	soil.bottom_radius = 0.85
	soil.height = 0.06
	soil.radial_segments = 14
	var smi2: MeshInstance3D = host._mi(parent, soil, host._toon(Color(0.34, 0.45, 0.28), 0.05, false), pos + Vector3(0, 0.22, 0))
	smi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bloom := SphereMesh.new()
	bloom.radius = 0.09
	bloom.height = 0.18
	bloom.radial_segments = 8
	bloom.rings = 4
	for k in 7:
		var ang := TAU * float(k) / 7.0
		var r := 0.25 + 0.3 * float(k % 2)
		var bcol := col if k % 3 != 2 else col.lightened(0.3)
		host._mi(parent, bloom, host._toon(bcol, 0.3, true, 0.2, 0.9),
			pos + Vector3(cos(ang) * r, 0.32 + 0.05 * float(k % 2), sin(ang) * r))


## A little market cart with a parasol.
func _cart(parent: Node3D, host: Node, pos: Vector3, yaw_deg: float, col: Color) -> void:
	var s := Node3D.new()
	s.position = pos
	s.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(s)
	host._obstacles.append({"pos": pos, "r": 0.8})
	var body := BoxMesh.new()
	body.size = Vector3(1.5, 0.8, 0.9)
	host._mi(s, body, host._toon(Color(0.88, 0.84, 0.76)), Vector3(0, 0.65, 0))
	var wheel := CylinderMesh.new()
	wheel.top_radius = 0.22
	wheel.bottom_radius = 0.22
	wheel.height = 0.08
	wheel.radial_segments = 10
	for wx in [-0.55, 0.55]:
		var wxx: float = wx
		var wmi: MeshInstance3D = host._mi(s, wheel, host._toon(Color(0.35, 0.28, 0.22)), Vector3(wxx, 0.22, 0))
		wmi.rotation_degrees = Vector3(0, 0, 90)
	var pole := CylinderMesh.new()
	pole.top_radius = 0.03
	pole.bottom_radius = 0.03
	pole.height = 1.5
	pole.radial_segments = 8
	host._mi(s, pole, host._toon(Color(0.5, 0.4, 0.3)), Vector3(0, 1.7, 0))
	var para := CylinderMesh.new()
	para.top_radius = 0.05
	para.bottom_radius = 1.1
	para.height = 0.45
	para.radial_segments = 10
	host._mi(s, para, host._toon(col, 0.25), Vector3(0, 2.5, 0))


## A small vendor shop: cream box, glowing shop window, striped awning, neon
## sign — a stage for future .ddrm storefronts.
func _shop(parent: Node3D, host: Node, pos: Vector3, yaw_deg: float, col: Color) -> void:
	var s := Node3D.new()
	s.position = pos
	s.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(s)
	host._contact(parent, 2.4, pos)
	host._boxes.append({"pos": pos, "half": Vector2(1.9, 1.9)})
	var body := BoxMesh.new()
	body.size = Vector3(3.4, 2.5, 3.0)
	host._mi(s, body, host._toon(Color(0.94, 0.90, 0.82)), Vector3(0, 1.25, 0))
	var roof := BoxMesh.new()
	roof.size = Vector3(3.7, 0.18, 3.3)
	host._mi(s, roof, host._toon(col.darkened(0.25)), Vector3(0, 2.6, 0))
	var win := BoxMesh.new()
	win.size = Vector3(1.7, 1.1, 0.1)
	var gmat: StandardMaterial3D = host._glass_mat()
	host._windows.append(gmat)
	host._mi(s, win, gmat, Vector3(-0.5, 1.15, 1.53))
	var door := BoxMesh.new()
	door.size = Vector3(0.8, 1.6, 0.1)
	host._mi(s, door, host._toon(Color(0.5, 0.33, 0.2)), Vector3(1.15, 0.8, 1.53))
	for k in 4:
		var strip := BoxMesh.new()
		strip.size = Vector3(0.82, 0.05, 1.0)
		var smat: ShaderMaterial = host._toon(col if k % 2 == 0 else Color(0.96, 0.95, 0.9), 0.15)
		var mi2: MeshInstance3D = host._mi(s, strip, smat, Vector3(-1.23 + k * 0.82, 2.18, 1.85))
		mi2.rotation_degrees = Vector3(22, 0, 0)
	var sign := BoxMesh.new()
	sign.size = Vector3(2.2, 0.5, 0.08)
	host._mi(s, sign, host._toon(col, 0.25), Vector3(0, 2.95, 1.45))
	var neon := BoxMesh.new()
	neon.size = Vector3(2.2, 0.06, 0.06)
	var nmi: MeshInstance3D = host._mi(s, neon, VerseAvatar.glow_mat(col.lightened(0.3), 1.5), Vector3(0, 2.66, 1.5))
	nmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var crate := BoxMesh.new()
	crate.size = Vector3(0.5, 0.5, 0.5)
	host._mi(s, crate, host._toon(Color(0.7, 0.52, 0.33), 0.15), Vector3(-1.9, 0.25, 1.2))
	host._mi(s, crate, host._toon(Color(0.62, 0.45, 0.28), 0.15), Vector3(-1.9, 0.75, 1.05))



## Skyline tower: 0 slim neon block · 1 needle+disc · 2 stacked spire ·
## 3 the twisting leaf-glass landmark. Backdrop only.
func _tower(parent: Node3D, host: Node, pos: Vector3, h: float, style: int) -> void:
	var t := Node3D.new()
	t.position = pos
	parent.add_child(t)
	var body_col := Color(0.62, 0.68, 0.76)
	var glow: StandardMaterial3D = VerseAvatar.glow_mat(CYAN, 1.2)
	if style == 1:
		var shaft := CylinderMesh.new()
		shaft.top_radius = 0.5
		shaft.bottom_radius = 1.3
		shaft.height = h
		shaft.radial_segments = 12
		host._mi(t, shaft, host._toon(body_col, 0.1, false), Vector3(0, h * 0.5, 0))
		var disc := CylinderMesh.new()
		disc.top_radius = 2.4
		disc.bottom_radius = 1.8
		disc.height = 1.4
		disc.radial_segments = 16
		host._mi(t, disc, host._toon(Color(0.72, 0.78, 0.85), 0.15, false), Vector3(0, h * 0.82, 0))
		var ring := TorusMesh.new()
		ring.inner_radius = 2.3
		ring.outer_radius = 2.5
		ring.rings = 24
		ring.ring_segments = 8
		var rmi: MeshInstance3D = host._mi(t, ring, glow, Vector3(0, h * 0.82, 0))
		rmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var tip := CylinderMesh.new()
		tip.top_radius = 0.02
		tip.bottom_radius = 0.12
		tip.height = h * 0.22
		tip.radial_segments = 8
		host._mi(t, tip, host._toon(body_col.darkened(0.2), 0.1, false), Vector3(0, h * 1.05, 0))
	elif style == 2:
		var w := 4.6
		var y := 0.0
		for k in 3:
			var seg := BoxMesh.new()
			var sh := h * (0.45 - k * 0.1)
			seg.size = Vector3(w, sh, w)
			host._mi(t, seg, host._toon(body_col.lightened(0.04 * k), 0.1, false), Vector3(0, y + sh * 0.5, 0))
			y += sh
			w *= 0.68
		var cap := BoxMesh.new()
		cap.size = Vector3(w + 0.4, 0.3, w + 0.4)
		var cmi: MeshInstance3D = host._mi(t, cap, glow, Vector3(0, y + 0.15, 0))
		cmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	elif style == 3:
		# the leaf landmark: twisting lens-shaped glass tower (the art's hero)
		var glass_col := Color(0.55, 0.72, 0.68)
		var segs := 9
		for k in segs:
			var f := float(k) / float(segs - 1)
			var wprof := sin((0.12 + 0.88 * f) * PI)
			var seg := BoxMesh.new()
			seg.size = Vector3(0.8 + 4.8 * wprof, h / segs + 0.1, 0.6 + 1.7 * wprof)
			var smi5: MeshInstance3D = host._mi(t, seg, host._toon(glass_col.lightened(0.05 * (k % 2)), 0.15, false),
				Vector3(0, f * h + h / (segs * 2.0), 0))
			smi5.rotation_degrees = Vector3(0, f * 38.0, 0)
	else:
		var blk := BoxMesh.new()
		blk.size = Vector3(4.0, h, 4.0)
		host._mi(t, blk, host._toon(body_col, 0.1, false), Vector3(0, h * 0.5, 0))
		var strip := BoxMesh.new()
		strip.size = Vector3(0.18, h * 0.86, 0.06)
		for sxo in [-1.2, 1.2]:
			var sx2: float = sxo
			var smi4: MeshInstance3D = host._mi(t, strip, glow, Vector3(sx2, h * 0.48, 2.04))
			smi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# lit window grid (homes and offices of the robot city)
		var winb := BoxMesh.new()
		winb.size = Vector3(0.5, 0.34, 0.05)
		var wmat: StandardMaterial3D = VerseAvatar.glow_mat(Color(1.0, 0.9, 0.66), 0.55)
		var rows := int(h / 2.4)
		for ry in rows:
			for wx in 3:
				if (ry * 3 + wx) % 4 == 1:
					continue   # some windows dark — feels inhabited
				var wmi2: MeshInstance3D = host._mi(t, winb, wmat,
					Vector3(-0.62 + 0.62 * wx, 1.6 + float(ry) * 2.4, 2.05))
				wmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## ───────────────────────── ALIVE v2: the density pass ──────────────────────
## Everything that turns "a nice set" into "a city that's busy living":
## sky-lane hover traffic, a keeper in every shop doorway, street crowds and
## queues, a busker with visible music, a sweeper on its rounds, a robot
## puppy, parked pods at the curb, holo billboards, a corner cafe terrace,
## a second market corner — and a far skyline ring so the city doesn't end
## at the first row of towers.
func _alive(parent: Node3D, host: Node, c: Vector3) -> void:
	# sky lanes: two rings of hover cars at different heights and directions
	var lane_a: Node3D = _spinner(parent, c + Vector3(0, 0, 4), 0.085, 0.0)
	var cols_a := [Color(0.88, 0.62, 0.4), Color(0.55, 0.72, 0.9), Color(0.92, 0.88, 0.85)]
	for k in 3:
		var aa := TAU * float(k) / 3.0 + 0.5
		_car(lane_a, host, Vector3(cos(aa) * 19.0, 9.2 + 0.4 * float(k), sin(aa) * 19.0),
			90.0 - rad_to_deg(aa), cols_a[k])
	var lane_b: Node3D = _spinner(parent, c + Vector3(0, 0, 4), -0.11, 0.0)
	var cols_b := [Color(0.72, 0.88, 0.6), Color(0.95, 0.75, 0.5), Color(0.6, 0.85, 0.9)]
	for k in 3:
		var ab := TAU * float(k) / 3.0
		_car(lane_b, host, Vector3(cos(ab) * 13.5, 6.0 + 0.3 * float(k), sin(ab) * 13.5),
			-90.0 - rad_to_deg(ab), cols_b[k])
	# delivery drones zipping above the plaza and the south street
	_drone(parent, host, c + Vector3(0, 0, 4), 8.5, 6.8, 0.4)
	_drone(parent, host, c + Vector3(0, 0, 18), 5.0, 5.0, -0.5)

	# every shop has its keeper at the door (tap them — they'll chat)
	var vend := [
		["Pixelina", ["fresh pixels, hot off the chain!", "everything here is truly yours."]],
		["Marrow", ["best parts in the city.", "trade you for a story?"]],
		["Tinker", ["repairs while you wait!", "nice hat, by the way."]],
		["Suki", ["come in, browse a while!", "new stock from the mainland today."]],
		["Cog", ["gears! springs! dreams!", "careful — the springs bite."]],
		["Lumen", ["lights for every home.", "take a lantern for the road."]],
	]
	for i in 6:
		var side := -1.0 if i % 2 == 0 else 1.0
		var sz := -2.0 - float(i / 2) * 7.5
		var v: Array = vend[i]
		_stander(parent, host, c + Vector3(side * 7.2, 0, sz + side * 1.15),
			c + Vector3(0, 0, sz), v[0], "did:verse:npc-" + str(v[0]).to_lower(), 1.0, v[1])

	# the busker by the fountain — music you can SEE — plus a little audience
	_busker(parent, host, c + Vector3(-3.0, 0, 0.6))
	_stander(parent, host, c + Vector3(-4.3, 0, 1.8), c + Vector3(-3.0, 0, 0.6),
		"Fan", "did:verse:npc-fan", 1.0, ["shh — this is the best part!"])
	_stander(parent, host, c + Vector3(-4.0, 0, -0.6), c + Vector3(-3.0, 0, 0.6),
		"Bea", "did:verse:npc-bea", 0.55)
	# a little queue at the noodle cart
	_stander(parent, host, c + Vector3(-5.0, 0, 6.4), c + Vector3(-6.2, 0, 5.8),
		"Pepper", "did:verse:npc-pepper", 1.0, ["the noodle pods here are legendary."])
	_stander(parent, host, c + Vector3(-4.1, 0, 6.9), c + Vector3(-5.0, 0, 6.4),
		"Niblet", "did:verse:npc-niblet", 0.55, ["dad says I can get TWO."])
	# two old friends catching up by the flower bed
	_stander(parent, host, c + Vector3(3.6, 0, 12.4), c + Vector3(4.6, 0, 12.9),
		"Vex", "did:verse:npc-vex", 1.0, ["…and that's when the relay came back up."])
	_stander(parent, host, c + Vector3(4.6, 0, 12.9), c + Vector3(3.6, 0, 12.4),
		"Moss", "did:verse:npc-moss", 1.0, ["no way. NO way."])

	# more residents out and about, each on their own stroll
	_npc(parent, host, c + Vector3(0, 0, 4), 7.2, -0.075, "Wren", "did:verse:npc-wren")
	_npc(parent, host, c + Vector3(0, 0, 4), 12.6, 0.04, "Tau", "did:verse:npc-tau")
	_npc(parent, host, c + Vector3(-9, 0, 8), 3.2, -0.1, "Koda", "did:verse:npc-koda")
	_npc(parent, host, c + Vector3(8, 0, 16), 3.0, 0.09, "Miko", "did:verse:npc-miko")
	_npc(parent, host, c + Vector3(0, 0, 4), 4.6, 0.21, "Juju", "did:verse:npc-juju", 0.55)
	# a robot puppy trotting after the west-side family
	_pet(parent, host, c + Vector3(-18.0, 0, 3.0), 3.4, 0.07, 0.85)
	# the sweeper bot on its plaza rounds
	_sweeper(parent, host, c)

	# mall: a greeter at the door + window shoppers
	_stander(parent, host, host.MALL_IN + Vector3(1.2, 0, 4.4), host.MALL_IN + Vector3(0, 0, 5.9),
		"Suri", "did:verse:npc-suri", 1.0,
		["welcome to the mall!", "ride the pads up — the sky lounge is open."])
	_stander(parent, host, host.MALL_IN + Vector3(-4.2, 0, -3.9), host.MALL_IN + Vector3(-5.15, 0, -4.7),
		"Quill", "did:verse:npc-quill")
	_stander(parent, host, host.MALL_IN + Vector3(7.0, 0, 4.9), host.MALL_IN,
		"Tam", "did:verse:npc-tam")

	# the corner cafe terrace by the west townhouses
	_terrace(parent, host, c + Vector3(-13.5, 0, 1.5))
	# a second market corner (east) under its own string lights
	_cart(parent, host, c + Vector3(12.0, 0, -6.8), 64.0, Color(0.88, 0.52, 0.72))
	_cart(parent, host, c + Vector3(13.6, 0, -10.2), 8.0, Color(0.45, 0.72, 0.45))
	_string_lights(parent, host, c + Vector3(12.0, 2.5, -6.8), c + Vector3(13.6, 2.5, -10.2))
	_stander(parent, host, c + Vector3(12.3, 0, -8.6), c + Vector3(13.6, 0, -10.2),
		"Rye", "did:verse:npc-rye", 1.0, ["one of everything, please."])

	# parked pods along the curb
	_parked(parent, host, c + Vector3(-5.5, 0, 2.4), 8.0, Color(0.62, 0.7, 0.85))
	_parked(parent, host, c + Vector3(-5.5, 0, -8.8), -6.0, Color(0.9, 0.7, 0.45))
	_parked(parent, host, c + Vector3(5.8, 0, 10.4), 184.0, Color(0.75, 0.85, 0.7))

	# holo billboards: the city talks back
	_billboard(parent, host, c + Vector3(0, 12.6, -15.8), 0.0, "ELA CITY", CYAN)
	_billboard(parent, host, c + Vector3(-16.2, 6.2, 1.0), 90.0, "HEY", Color(0.95, 0.75, 0.4))
	_billboard(parent, host, c + Vector3(16.4, 6.4, 0.0), -90.0, "DDRM", Color(0.7, 0.6, 1.0))

	# the far skyline ring — the city keeps going past the first towers
	var far := [
		[Vector3(-6, 0, -42), 22.0, 4.5], [Vector3(9, 0, -44), 17.0, 4.0],
		[Vector3(22, 0, -37), 25.0, 5.0], [Vector3(-20, 0, -40), 15.0, 3.6],
		[Vector3(-34, 0, -30), 20.0, 4.4], [Vector3(34, 0, -27), 14.0, 3.8],
		[Vector3(-40, 0, -12), 17.0, 4.2], [Vector3(40, 0, -6), 21.0, 4.6],
		[Vector3(-38, 0, 14), 13.0, 3.6], [Vector3(38, 0, 12), 16.0, 4.0],
	]
	for f in far:
		var fa: Array = f
		_far_tower(parent, host, c + fa[0], fa[1], fa[2])

	# street dressing: more light, more places to sit
	host._lamp(parent, c + Vector3(-3.2, 0, -15.0))
	host._lamp(parent, c + Vector3(3.2, 0, 18.5))
	host._bench(parent, c + Vector3(-5.0, 0, -6.2), -38.0)
	host._bench(parent, c + Vector3(8.4, 0, 4.8), -96.0)
	_string_lights(parent, host, c + Vector3(3.2, 2.55, -12.5), c + Vector3(-3.2, 2.55, -15.0))
	_banners(parent, host, c + Vector3(-3.9, 2.7, 20.0), c + Vector3(3.9, 2.7, 20.0))


## A resident standing still, facing `look_at_p` — vendors, queues, little
## chat circles. Cheap life: no orbit driver, idle anim only; optional
## tap-to-chat lines.
func _stander(parent: Node3D, host: Node, pos: Vector3, look_at_p: Vector3, bot_name: String, did: String, bot_scale: float = 1.0, lines: Array = []) -> void:
	var bot: VerseAvatar = VerseAvatar.new()
	bot.display_name = bot_name
	bot.base_color = Net.did_color(did)
	bot.outfit = VerseAvatar.resolve_outfit(did, {})
	bot.position = pos
	var d := look_at_p - pos
	bot.rotation.y = atan2(-d.x, -d.z)
	bot.scale = Vector3.ONE * bot_scale
	parent.add_child(bot)
	if not lines.is_empty():
		host._talkers.append({"node": bot, "i": 0, "lines": lines})


## A hover car for the sky lanes: pastel pod, glass canopy, cyan underglow,
## warm tail light. Rides its parent orbit like the trams do.
func _car(parent: Node3D, host: Node, offset: Vector3, yaw_deg: float, col: Color) -> void:
	var t := Node3D.new()
	t.position = offset
	t.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(t)
	var body := BoxMesh.new()
	body.size = Vector3(1.5, 0.45, 0.8)
	host._mi(t, body, host._toon(col, 0.2, false, 0.0, 0.5, 0.4), Vector3.ZERO)
	var canopy := BoxMesh.new()
	canopy.size = Vector3(0.8, 0.3, 0.66)
	var cmat: StandardMaterial3D = host._glass_mat()
	host._windows.append(cmat)
	host._mi(t, canopy, cmat, Vector3(0.15, 0.35, 0))
	var strip := BoxMesh.new()
	strip.size = Vector3(1.5, 0.06, 0.06)
	var smi7: MeshInstance3D = host._mi(t, strip, VerseAvatar.glow_mat(CYAN, 1.2), Vector3(0, -0.27, 0))
	smi7.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var tail := BoxMesh.new()
	tail.size = Vector3(0.1, 0.12, 0.5)
	var tlmi: MeshInstance3D = host._mi(t, tail, VerseAvatar.glow_mat(Color(1.0, 0.45, 0.4), 1.3), Vector3(-0.78, 0.05, 0))
	tlmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## A parked hover pod resting on its skids by the curb.
func _parked(parent: Node3D, host: Node, pos: Vector3, yaw_deg: float, col: Color) -> void:
	var t := Node3D.new()
	t.position = pos + Vector3(0, 0.45, 0)
	t.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(t)
	host._obstacles.append({"pos": pos, "r": 1.0})
	var body := BoxMesh.new()
	body.size = Vector3(1.5, 0.45, 0.8)
	host._mi(t, body, host._toon(col, 0.2, false, 0.0, 0.5, 0.4), Vector3.ZERO)
	var canopy := BoxMesh.new()
	canopy.size = Vector3(0.8, 0.3, 0.66)
	var cmat: StandardMaterial3D = host._glass_mat()
	host._windows.append(cmat)
	host._mi(t, canopy, cmat, Vector3(0.15, 0.35, 0))
	var skid := BoxMesh.new()
	skid.size = Vector3(1.1, 0.07, 0.12)
	for sz in [-0.3, 0.3]:
		var szz: float = sz
		host._mi(t, skid, host._toon(Color(0.5, 0.55, 0.62), 0.1, false), Vector3(0, -0.38, szz))


## A little delivery drone: round body, spinning rotors, warm belly light.
func _drone(parent: Node3D, host: Node, center: Vector3, r: float, h: float, speed: float) -> void:
	var orbit: Node3D = _spinner(parent, center, speed, 0.3)
	var d := Node3D.new()
	d.position = Vector3(r, h, 0)
	orbit.add_child(d)
	var body := SphereMesh.new()
	body.radius = 0.16
	body.height = 0.26
	body.radial_segments = 10
	body.rings = 5
	host._mi(d, body, host._toon(Color(0.85, 0.88, 0.92), 0.2, false), Vector3.ZERO)
	var rotor := BoxMesh.new()
	rotor.size = Vector3(0.34, 0.025, 0.07)
	for k in 2:
		var rspin: Node3D = _spinner(d, Vector3(-0.22 + 0.44 * float(k), 0.14, 0), 14.0, 0.0)
		host._mi(rspin, rotor, host._toon(Color(0.45, 0.5, 0.58), 0.1, false), Vector3.ZERO)
	var lampm := SphereMesh.new()
	lampm.radius = 0.045
	lampm.height = 0.09
	lampm.radial_segments = 6
	lampm.rings = 3
	var dlmi: MeshInstance3D = host._mi(d, lampm, VerseAvatar.glow_mat(Color(1.0, 0.5, 0.45), 1.6), Vector3(0, -0.12, 0))
	dlmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## A robot puppy trotting behind its family — same circle, a few steps back.
func _pet(parent: Node3D, host: Node, center: Vector3, r: float, speed: float, phase: float) -> void:
	var orbit: Node3D = _spinner(parent, center, speed, 0.0)
	orbit.rotate_y(phase)
	var d := Node3D.new()
	d.position = Vector3(r, 0, 0)
	d.rotation.y = PI if speed > 0.0 else 0.0
	orbit.add_child(d)
	var grey := Color(0.92, 0.93, 0.96)
	var body := CapsuleMesh.new()
	body.radius = 0.13
	body.height = 0.52
	var pbmi: MeshInstance3D = host._mi(d, body, VerseAvatar.toon_mat(grey, 0.3), Vector3(0, 0.3, 0))
	pbmi.rotation_degrees = Vector3(90, 0, 0)
	var head := SphereMesh.new()
	head.radius = 0.11
	head.height = 0.22
	head.radial_segments = 10
	head.rings = 5
	host._mi(d, head, VerseAvatar.toon_mat(Color(0.2, 0.26, 0.4), 0.3), Vector3(0, 0.42, 0.26))
	var eye := SphereMesh.new()
	eye.radius = 0.022
	eye.height = 0.044
	eye.radial_segments = 6
	eye.rings = 3
	for ex in [-0.045, 0.045]:
		var exx: float = ex
		var pemi: MeshInstance3D = host._mi(d, eye, VerseAvatar.glow_mat(Color(0.45, 0.9, 1.0), 1.6), Vector3(exx, 0.45, 0.36))
		pemi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var ear := BoxMesh.new()
	ear.size = Vector3(0.05, 0.12, 0.04)
	for ex2 in [-0.07, 0.07]:
		var exx2: float = ex2
		var earm: MeshInstance3D = host._mi(d, ear, VerseAvatar.toon_mat(grey, 0.3), Vector3(exx2, 0.55, 0.22))
		earm.rotation_degrees = Vector3(0, 0, -14.0 if exx2 < 0.0 else 14.0)
	var leg := CylinderMesh.new()
	leg.top_radius = 0.035
	leg.bottom_radius = 0.035
	leg.height = 0.18
	leg.radial_segments = 8
	for lx in [-0.08, 0.08]:
		for lz in [-0.15, 0.15]:
			var lxx: float = lx
			var lzz: float = lz
			host._mi(d, leg, VerseAvatar.toon_mat(grey.darkened(0.15), 0.2), Vector3(lxx, 0.09, lzz))
	# tail antenna with a glow tip, wagging
	var tailp := Node3D.new()
	tailp.position = Vector3(0, 0.38, -0.26)
	d.add_child(tailp)
	var tail := CylinderMesh.new()
	tail.top_radius = 0.012
	tail.bottom_radius = 0.02
	tail.height = 0.2
	tail.radial_segments = 6
	var ptmi2: MeshInstance3D = host._mi(tailp, tail, VerseAvatar.toon_mat(grey.darkened(0.1), 0.2), Vector3(0, 0.1, 0))
	ptmi2.rotation_degrees = Vector3(-30, 0, 0)
	var tip := SphereMesh.new()
	tip.radius = 0.03
	tip.height = 0.06
	tip.radial_segments = 6
	tip.rings = 3
	var ttmi: MeshInstance3D = host._mi(tailp, tip, VerseAvatar.glow_mat(Color(0.95, 0.85, 0.5), 1.4), Vector3(0, 0.2, 0.06))
	ttmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var wag := tailp.create_tween()
	wag.set_loops()
	wag.tween_property(tailp, "rotation_degrees:y", 28.0, 0.22) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	wag.tween_property(tailp, "rotation_degrees:y", -28.0, 0.22) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)


## The plaza sweeper bot on its tireless rounds, brushes spinning.
func _sweeper(parent: Node3D, host: Node, c: Vector3) -> void:
	var orbit: Node3D = _spinner(parent, c + Vector3(0, 0, 4), 0.045, 0.0)
	var s := Node3D.new()
	s.position = Vector3(11.9, 0, 0)
	s.rotation.y = PI
	orbit.add_child(s)
	var body := CylinderMesh.new()
	body.top_radius = 0.3
	body.bottom_radius = 0.38
	body.height = 0.55
	body.radial_segments = 12
	host._mi(s, body, host._toon(Color(0.95, 0.78, 0.35), 0.2), Vector3(0, 0.35, 0))
	var dome := SphereMesh.new()
	dome.radius = 0.22
	dome.height = 0.3
	dome.radial_segments = 10
	dome.rings = 5
	host._mi(s, dome, host._toon(Color(0.85, 0.87, 0.9), 0.2), Vector3(0, 0.66, 0))
	var eye := SphereMesh.new()
	eye.radius = 0.045
	eye.height = 0.09
	eye.radial_segments = 6
	eye.rings = 3
	var semi: MeshInstance3D = host._mi(s, eye, VerseAvatar.glow_mat(Color(0.45, 0.9, 1.0), 1.6), Vector3(0, 0.7, 0.19))
	semi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var beacon := SphereMesh.new()
	beacon.radius = 0.05
	beacon.height = 0.1
	beacon.radial_segments = 6
	beacon.rings = 3
	var sbmi: MeshInstance3D = host._mi(s, beacon, VerseAvatar.glow_mat(Color(1.0, 0.6, 0.3), 1.4), Vector3(0, 0.86, 0))
	sbmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bspin: Node3D = _spinner(s, Vector3(0, 0.06, 0.42), 6.0, 0.0)
	var brush := CylinderMesh.new()
	brush.top_radius = 0.15
	brush.bottom_radius = 0.17
	brush.height = 0.09
	brush.radial_segments = 10
	host._mi(bspin, brush, host._toon(Color(0.42, 0.6, 0.86), 0.2), Vector3.ZERO)


## A street performer on a little stage — bouncing to the beat, glowing notes
## drifting up over the crowd.
func _busker(parent: Node3D, host: Node, pos: Vector3) -> void:
	host._obstacles.append({"pos": pos, "r": 0.9})
	var stage := CylinderMesh.new()
	stage.top_radius = 0.85
	stage.bottom_radius = 0.95
	stage.height = 0.16
	stage.radial_segments = 14
	host._mi(parent, stage, host._toon(Color(0.78, 0.74, 0.7), 0.1), pos + Vector3(0, 0.08, 0))
	var bounce: Node3D = _spinner(parent, pos + Vector3(0, 0.16, 0), 0.0, 0.05)
	var bot: VerseAvatar = VerseAvatar.new()
	bot.display_name = "Strum"
	bot.base_color = Net.did_color("did:verse:npc-strum")
	bot.outfit = VerseAvatar.resolve_outfit("did:verse:npc-strum", {})
	bot.rotation.y = PI * 0.5   # facing the audience
	bounce.add_child(bot)
	host._talkers.append({"node": bounce, "i": 0, "lines": [
		"any requests?",
		"this next one's about the mainchain…",
		"thank you, thank you — I'm here all week.",
	]})
	var note := SphereMesh.new()
	note.radius = 0.05
	note.height = 0.1
	note.radial_segments = 6
	note.rings = 3
	note.material = VerseAvatar.glow_mat(Color(1.0, 0.85, 0.5), 1.2)
	var notes := CPUParticles3D.new()
	notes.amount = 8
	notes.lifetime = 2.4
	notes.mesh = note
	notes.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	notes.emission_sphere_radius = 0.4
	notes.direction = Vector3(0, 1, 0)
	notes.spread = 25.0
	notes.gravity = Vector3.ZERO
	notes.initial_velocity_min = 0.5
	notes.initial_velocity_max = 0.9
	notes.position = pos + Vector3(0, 1.7, 0)
	parent.add_child(notes)


## The corner cafe terrace: parasol tables and robots on a coffee break.
func _terrace(parent: Node3D, host: Node, pos: Vector3) -> void:
	var tcols := [Color(0.86, 0.42, 0.38), Color(0.42, 0.6, 0.86)]
	for k in 2:
		var tp := pos + Vector3(float(k) * 2.8 - 1.4, 0, float(k) * 1.6 - 0.8)
		host._obstacles.append({"pos": tp, "r": 0.75})
		var polem := CylinderMesh.new()
		polem.top_radius = 0.04
		polem.bottom_radius = 0.04
		polem.height = 2.0
		polem.radial_segments = 8
		host._mi(parent, polem, host._toon(Color(0.55, 0.45, 0.35), 0.15), tp + Vector3(0, 1.0, 0))
		var top := CylinderMesh.new()
		top.top_radius = 0.55
		top.bottom_radius = 0.5
		top.height = 0.06
		top.radial_segments = 12
		host._mi(parent, top, host._toon(Color(0.93, 0.91, 0.86), 0.15), tp + Vector3(0, 0.78, 0))
		var para := CylinderMesh.new()
		para.top_radius = 0.05
		para.bottom_radius = 0.95
		para.height = 0.4
		para.radial_segments = 10
		host._mi(parent, para, host._toon(tcols[k], 0.25), tp + Vector3(0, 2.1, 0))
	host._lamp(parent, pos + Vector3(2.6, 0, -1.6))
	_string_lights(parent, host, pos + Vector3(2.6, 2.55, -1.6), pos + Vector3(-1.4, 2.3, -0.8))
	_stander(parent, host, pos + Vector3(-0.7, 0, 0.1), pos + Vector3(-1.4, 0, -0.8),
		"Latte", "did:verse:npc-latte", 1.0, ["they brew it with real photons."])
	_stander(parent, host, pos + Vector3(1.9, 0, 1.2), pos + Vector3(1.4, 0, 0.8),
		"Mocha", "did:verse:npc-mocha")


## A floating holo billboard: dark screen, glowing text, soft underline —
## bobbing gently like everything else the city projects.
func _billboard(parent: Node3D, host: Node, pos: Vector3, yaw_deg: float, text: String, col: Color) -> void:
	var b: Node3D = _spinner(parent, pos, 0.0, 0.1)
	b.rotation_degrees = Vector3(0, yaw_deg, 0)
	var panel := BoxMesh.new()
	panel.size = Vector3(3.6, 1.5, 0.08)
	var pnmi: MeshInstance3D = host._mi(b, panel, host._toon(Color(0.08, 0.12, 0.2), 0.05, false), Vector3.ZERO)
	pnmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var under := BoxMesh.new()
	under.size = Vector3(3.6, 0.06, 0.06)
	var unmi: MeshInstance3D = host._mi(b, under, VerseAvatar.glow_mat(col, 1.4), Vector3(0, -0.82, 0))
	unmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var lbl := Label3D.new()
	lbl.text = text
	lbl.font_size = 120
	lbl.pixel_size = 0.008
	lbl.modulate = col.lightened(0.25)
	lbl.outline_size = 12
	lbl.position = Vector3(0, 0, 0.06)
	b.add_child(lbl)


## A distant silhouette tower — cheap (3 meshes), fills the far skyline.
func _far_tower(parent: Node3D, host: Node, pos: Vector3, h: float, w: float) -> void:
	var t := Node3D.new()
	t.position = pos
	parent.add_child(t)
	var blk := BoxMesh.new()
	blk.size = Vector3(w, h, w)
	host._mi(t, blk, host._toon(Color(0.55, 0.61, 0.7), 0.08, false), Vector3(0, h * 0.5, 0))
	var cap := BoxMesh.new()
	cap.size = Vector3(w + 0.3, 0.22, w + 0.3)
	var fcmi: MeshInstance3D = host._mi(t, cap, VerseAvatar.glow_mat(CYAN, 0.9), Vector3(0, h + 0.11, 0))
	fcmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var strip := BoxMesh.new()
	strip.size = Vector3(0.14, h * 0.8, 0.05)
	var fsmi: MeshInstance3D = host._mi(t, strip, VerseAvatar.glow_mat(Color(1.0, 0.9, 0.66), 0.5),
		Vector3(0, h * 0.45, w * 0.5 + 0.03))
	fsmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## ───────────────────────── THE MALL (v3 — the landmark) ────────────────────
## A real metaverse shopping mall: glass-curtain exterior with a domed roof and
## SLIDING glass doors; inside, a grand luxury atrium — black-grid glass dome,
## three white gallery levels ringed in flowing gold light, a glowing
## sculpture-tree under a canopy of golden orbs, organic gold ribbon seating,
## crossing glass escalators, a panoramic atrium lift, a moon-gate flagship —
## and two more WALKABLE floors: ride the lift pads up to Level 2's gallery
## ring and the Level 3 sky lounge with a view over Ela City.

const MALL_WHITE := Color(0.95, 0.94, 0.91)
const MALL_CREAM := Color(0.9, 0.88, 0.84)
const MALL_GOLD := Color(0.85, 0.72, 0.34)
const MALL_GOLD_GLOW := Color(1.0, 0.84, 0.45)
const MALL_DARK := Color(0.10, 0.11, 0.13)
const MALL_TEAL := Color(0.35, 0.95, 0.85)


## Exterior: taller glass-curtain volume (three storeys), gold roofline, domed
## skylight, recessed atrium entrance with REAL sliding glass doors.
func _mall(parent: Node3D, host: Node, pos: Vector3) -> void:
	var m := Node3D.new()
	m.position = pos
	parent.add_child(m)
	host._boxes.append({"pos": pos, "half": Vector2(12.5, 7.5)})
	var body := BoxMesh.new()
	body.size = Vector3(24, 13.5, 14)
	host._mi(m, body, host._toon(Color(0.82, 0.86, 0.90), 0.15, true, 0.0, 0.5, 0.4), Vector3(0, 6.75, 0))
	# gold roofline + thin glow accent
	var band := BoxMesh.new()
	band.size = Vector3(24.5, 0.45, 14.5)
	host._mi(m, band, host._toon(MALL_GOLD, 0.3, true, 0.0, 0.5, 0.5), Vector3(0, 13.7, 0))
	var band2 := BoxMesh.new()
	band2.size = Vector3(24.6, 0.1, 14.6)
	var b2mi: MeshInstance3D = host._mi(m, band2, VerseAvatar.glow_mat(Color(0.93, 0.84, 0.5), 0.7), Vector3(0, 13.32, 0))
	b2mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# three-storey glass curtain with white fin mullions (front face)
	var pane := BoxMesh.new()
	pane.size = Vector3(3.2, 3.4, 0.12)
	var fin := BoxMesh.new()
	fin.size = Vector3(0.22, 12.2, 0.5)
	for gx in 6:
		var px := -8.75 + gx * 3.5
		for gy in 3:
			var gmat: StandardMaterial3D = host._glass_mat()
			host._windows.append(gmat)
			host._mi(m, pane, gmat, Vector3(px, 2.2 + gy * 3.7, 7.06))
		if gx < 5:
			host._mi(m, fin, host._toon(Color(0.95, 0.96, 0.98), 0.2), Vector3(px + 1.75, 6.3, 7.1))
	# the domed skylight, visible from the plaza: squashed glass dome + dark ribs
	var domem := SphereMesh.new()
	domem.radius = 6.0
	domem.height = 4.6
	domem.radial_segments = 18
	domem.rings = 8
	var dmat: StandardMaterial3D = host._glass_mat()
	host._windows.append(dmat)
	host._mi(m, domem, dmat, Vector3(0, 13.6, -0.5))
	var drib := TorusMesh.new()
	drib.inner_radius = 5.0
	drib.outer_radius = 5.18
	drib.rings = 24
	drib.ring_segments = 6
	var drmi: MeshInstance3D = host._mi(m, drib, host._toon(MALL_DARK, 0.1, false), Vector3(0, 14.4, -0.5))
	drmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# recessed glass atrium entrance + gold canopy on slim poles
	var atrium := BoxMesh.new()
	atrium.size = Vector3(4.6, 5.6, 0.16)
	var amat: StandardMaterial3D = host._glass_mat()
	host._windows.append(amat)
	host._mi(m, atrium, amat, Vector3(0, 4.2, 7.3))
	var aframe := BoxMesh.new()
	aframe.size = Vector3(5.2, 0.25, 0.7)
	host._mi(m, aframe, host._toon(Color(0.95, 0.96, 0.98), 0.2), Vector3(0, 5.75, 7.35))
	var canopy := BoxMesh.new()
	canopy.size = Vector3(6.4, 0.18, 2.4)
	host._mi(m, canopy, host._toon(MALL_GOLD, 0.3), Vector3(0, 5.1, 8.2))
	var cpole := CylinderMesh.new()
	cpole.top_radius = 0.07
	cpole.bottom_radius = 0.07
	cpole.height = 5.0
	cpole.radial_segments = 8
	for cx in [-2.8, 2.8]:
		var cxx: float = cx
		host._mi(m, cpole, host._toon(Color(0.9, 0.91, 0.94), 0.2), Vector3(cxx, 2.5, 8.9))
	var steps := BoxMesh.new()
	steps.size = Vector3(6.2, 0.18, 1.6)
	host._mi(m, steps, host._toon(Color(0.7, 0.7, 0.72), 0.1), Vector3(0, 0.09, 8.0))
	# THE SLIDING DOORS — glide open as you walk up, shut behind you
	_slide_doors(m, host, Vector3(0, 0, 7.55))
	# planters flanking the steps (solid — no walking through the pots)
	for sx in [-4.0, 4.0]:
		var sxx: float = sx
		host._obstacles.append({"pos": pos + Vector3(sxx, 0, 7.8), "r": 0.6})
		var pot := CylinderMesh.new()
		pot.top_radius = 0.5
		pot.bottom_radius = 0.42
		pot.height = 0.5
		pot.radial_segments = 12
		host._mi(m, pot, host._toon(Color(0.62, 0.6, 0.58), 0.1), Vector3(sxx, 0.25, 7.8))
		var bush := SphereMesh.new()
		bush.radius = 0.55
		bush.height = 1.1
		bush.radial_segments = 10
		bush.rings = 5
		host._mi(m, bush, host._toon(Color(0.36, 0.62, 0.34), 0.3, true, 0.15, 0.8), Vector3(sxx, 0.85, 7.8))
	# the REAL doorway: step through the open doors and you're inside
	host._portals.append({
		"at": pos + Vector3(0, 0, 7.9), "to": host.MALL_IN + Vector3(0, 0, 4.6), "yaw": PI,
	})


## A pair of sliding glass door panels driven by door.gd (camera-proximity).
## Falls back to static glass if the main pack predates door.gd.
func _slide_doors(parent: Node3D, host: Node, pos: Vector3) -> void:
	var d_script: GDScript = load("res://door.gd")
	var d: Node3D = d_script.new() if d_script else Node3D.new()
	d.position = pos
	parent.add_child(d)
	var frame := BoxMesh.new()
	frame.size = Vector3(2.6, 0.12, 0.2)
	host._mi(d, frame, host._toon(MALL_GOLD, 0.3), Vector3(0, 2.76, 0))
	var panel := BoxMesh.new()
	panel.size = Vector3(1.18, 2.7, 0.07)
	var pl := Node3D.new()
	pl.position = Vector3(-0.59, 1.35, 0)
	d.add_child(pl)
	var pr := Node3D.new()
	pr.position = Vector3(0.59, 1.35, 0)
	d.add_child(pr)
	for p in [pl, pr]:
		var pn: Node3D = p
		var gmat: StandardMaterial3D = host._glass_mat()
		host._windows.append(gmat)
		host._mi(pn, panel, gmat, Vector3.ZERO)
		var hbar := BoxMesh.new()
		hbar.size = Vector3(1.18, 0.07, 0.09)
		host._mi(pn, hbar, host._toon(MALL_GOLD, 0.3), Vector3(0, 0, 0.01))
	if d_script:
		d.call("setup", pl, pr)


## The grand hall: dome grid, glow tree + orb canopy, ribbon benches, three
## gold-lit gallery levels, escalators, atrium lift, shopfronts, moon gate.
func _mall_interior(parent: Node3D, host: Node) -> void:
	var q := Node3D.new()
	q.position = host.MALL_IN
	parent.add_child(q)
	# ── floor: polished cream, gold medallion, radial inlays, glow path ──────
	var fl := BoxMesh.new()
	fl.size = Vector3(24, 0.3, 14.5)
	host._mi(q, fl, host._toon(MALL_CREAM, 0.1, false, 0.0, 0.5, 0.4), Vector3(0, -0.15, 0))
	var path := BoxMesh.new()
	path.size = Vector3(2.6, 0.02, 13.5)
	var ptmi: MeshInstance3D = host._mi(q, path, host._toon(Color(0.84, 0.81, 0.76), 0.05, false), Vector3(0, 0.012, 0))
	ptmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var ring1 := TorusMesh.new()
	ring1.inner_radius = 2.55
	ring1.outer_radius = 2.7
	ring1.rings = 32
	ring1.ring_segments = 4
	var r1mi: MeshInstance3D = host._mi(q, ring1, host._toon(MALL_GOLD, 0.25, false), Vector3(0, 0.022, 0))
	r1mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var ring2 := TorusMesh.new()
	ring2.inner_radius = 3.25
	ring2.outer_radius = 3.34
	ring2.rings = 32
	ring2.ring_segments = 4
	var r2mi: MeshInstance3D = host._mi(q, ring2, host._toon(MALL_GOLD, 0.25, false), Vector3(0, 0.028, 0))
	r2mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var inlay := BoxMesh.new()
	inlay.size = Vector3(0.08, 0.016, 4.6)
	for k in 6:
		var imi: MeshInstance3D = host._mi(q, inlay, host._toon(MALL_GOLD, 0.2, false), Vector3(0, 0.018, 0))
		imi.rotation_degrees = Vector3(0, 30.0 + 60.0 * float(k), 0)
		imi.position += imi.basis.z * -6.4
		imi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# ── perimeter walls (solid — no walking into the void) ───────────────────
	var wall_mat: ShaderMaterial = host._toon(Color(0.74, 0.82, 0.9), 0.1, false)
	var wn := BoxMesh.new()
	wn.size = Vector3(24, 13.6, 0.3)
	# dollhouse cutaway: the FRONT (z+) wall is never drawn — the camera sits
	# behind the avatar, so a drawn front wall hides the player. The collision
	# box below still seals the room.
	host._mi(q, wn, wall_mat, Vector3(0, 6.8, -7.25))
	var we := BoxMesh.new()
	we.size = Vector3(0.3, 13.6, 14.5)
	host._mi(q, we, wall_mat, Vector3(-12.05, 6.8, 0))
	host._mi(q, we, wall_mat, Vector3(12.05, 6.8, 0))
	host._boxes.append({"pos": host.MALL_IN + Vector3(0, 0, -7.25), "half": Vector2(12.2, 0.3)})
	host._boxes.append({"pos": host.MALL_IN + Vector3(0, 0, 7.25), "half": Vector2(12.2, 0.3)})
	host._boxes.append({"pos": host.MALL_IN + Vector3(-12.05, 0, 0), "half": Vector2(0.3, 7.4)})
	host._boxes.append({"pos": host.MALL_IN + Vector3(12.05, 0, 0), "half": Vector2(0.3, 7.4)})
	# ── the black-grid glass dome (ceiling) ──────────────────────────────────
	_dome_grid(q, host, Vector3(0, 13.5, 0), 8.6)
	# slim white columns
	var col := CylinderMesh.new()
	col.top_radius = 0.28
	col.bottom_radius = 0.34
	col.height = 13.2
	col.radial_segments = 12
	for cx in [-8.0, 8.0]:
		for cz in [-4.0, 4.0]:
			var cxx: float = cx
			var czz: float = cz
			host._mi(q, col, host._toon(Color(0.94, 0.95, 0.97), 0.15, false), Vector3(cxx, 6.6, czz))
			host._obstacles.append({"pos": host.MALL_IN + Vector3(cxx, 0, czz), "r": 0.55})
	# ── three gallery levels, white + flowing gold ───────────────────────────
	var lvl_cols := [
		[Color(0.86, 0.42, 0.38), Color(0.92, 0.74, 0.34), Color(0.88, 0.52, 0.72)],
		[Color(0.42, 0.74, 0.86), Color(0.45, 0.55, 0.92), Color(0.6, 0.5, 0.92)],
		[Color(0.92, 0.6, 0.3), Color(0.55, 0.78, 0.45), Color(0.93, 0.8, 0.4)],
	]
	var lvl_tags := ["LEVEL 2 · FOR HER", "LEVEL 3 · FOR HIM", "LEVEL 4 · CINEMA"]
	var lvl_names := [["ROSE", "PEARL"], ["GENTS", "WATCH"], ["FILM", "SNACKS"]]
	for li in 3:
		_gallery_level(q, host, 3.4 + 3.4 * float(li), lvl_cols[li], lvl_tags[li], lvl_names[li])
	# window-shopper silhouettes up on the galleries (life on every floor)
	_stander(q, host, Vector3(-4.5, 3.42, -5.5), Vector3(-4.5, 3.42, -7.0), "Ada", "did:verse:npc-ada")
	_stander(q, host, Vector3(5.5, 6.82, -5.5), Vector3(5.5, 6.82, -7.0), "Niko", "did:verse:npc-niko")
	_stander(q, host, Vector3(2.5, 10.22, -5.5), Vector3(2.5, 10.22, -7.0), "Page", "did:verse:npc-page", 0.9)
	# ── THE GLOW TREE — sculptural centerpiece under a canopy of gold orbs ───
	_glow_tree(q, host)
	host._obstacles.append({"pos": host.MALL_IN, "r": 2.9})
	# organic gold ribbon benches sweeping around the tree
	_ribbon_bench(q, host, Vector3.ZERO, 3.7, 15.0, 125.0)
	_ribbon_bench(q, host, Vector3.ZERO, 3.7, 195.0, 305.0)
	for ang in [70.0, 250.0]:
		var aa: float = deg_to_rad(ang)
		host._obstacles.append({"pos": host.MALL_IN + Vector3(cos(aa) * 3.7, 0, sin(aa) * 3.7), "r": 0.55})
	# ── crossing glass escalators up to the first gallery ───────────────────
	# ONE escalator — the second one sat right in front of CIRCUIT & CO
	_escalator(q, host, Vector3(5.2, 0, 2.6), -12.0)
	for ex in [6.4, 8.4, 10.4]:
		var exx: float = ex
		host._obstacles.append({"pos": host.MALL_IN + Vector3(exx, 0, 2.6), "r": 0.8})
	# ── the panoramic atrium lift (glass cab riding up and down) ─────────────
	_glass_lift(q, host, Vector3(-7.5, 0, 0))
	host._obstacles.append({"pos": host.MALL_IN + Vector3(-7.5, 0, 0), "r": 1.2})
	# ── ground floor: REAL walk-in stores (lit, stocked, staffed) + the
	# moon-gate flagship at center north ─────────────────────────────────────
	_store(q, host, host.MALL_IN, Vector3(-7.0, 0, -5.9), 0.0, Color(0.86, 0.42, 0.38), "ROBO THREADS", 2.4, "Wira")
	_store(q, host, host.MALL_IN, Vector3(7.0, 0, -5.9), 0.0, Color(0.42, 0.74, 0.72), "CIRCUIT & CO", 2.4, "Volt")
	_store(q, host, host.MALL_IN, Vector3(-10.7, 0, 2.5), 90.0, Color(0.92, 0.74, 0.34), "GOLDSMITH", 2.4, "Auria")
	_moon_gate(q, host, Vector3(0, 0, -6.6))
	host._boxes.append({"pos": host.MALL_IN + Vector3(0, 0, -6.6), "half": Vector2(2.7, 0.5)})
	# ── de-cement: wainscot + gold trim + pilasters, dressed columns, carpet
	# runners, lit art panels, planters ──────────────────────────────────────
	_wall_dress(q, host, Vector3(0, 0, -7.1), 0.0, 23.6, 13.4)
	_wall_dress(q, host, Vector3(11.88, 0, 0), -90.0, 14.2, 13.4)
	_wall_dress(q, host, Vector3(-11.88, 0, 0), 90.0, 14.2, 13.4)
	var cbase := CylinderMesh.new()
	cbase.top_radius = 0.4
	cbase.bottom_radius = 0.46
	cbase.height = 0.28
	cbase.radial_segments = 12
	var ccap := CylinderMesh.new()
	ccap.top_radius = 0.46
	ccap.bottom_radius = 0.4
	ccap.height = 0.24
	ccap.radial_segments = 12
	var cneon := BoxMesh.new()
	cneon.size = Vector3(0.05, 10.8, 0.05)
	for cx2 in [-8.0, 8.0]:
		for cz2 in [-4.0, 4.0]:
			var cxx2: float = cx2
			var czz2: float = cz2
			host._mi(q, cbase, host._toon(MALL_GOLD, 0.3), Vector3(cxx2, 0.14, czz2))
			host._mi(q, ccap, host._toon(MALL_GOLD, 0.3), Vector3(cxx2, 12.95, czz2))
			var cnmi: MeshInstance3D = host._mi(q, cneon, VerseAvatar.glow_mat(MALL_TEAL, 0.4),
				Vector3(cxx2 + (0.36 if cxx2 < 0.0 else -0.36), 6.6, czz2))
			cnmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rug := BoxMesh.new()
	rug.size = Vector3(2.4, 0.025, 2.8)
	var rugmi: MeshInstance3D = host._mi(q, rug, host._toon(Color(0.66, 0.28, 0.26), 0.1, false), Vector3(0, 0.03, 3.7))
	rugmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rug2 := BoxMesh.new()
	rug2.size = Vector3(2.4, 0.025, 4.0)
	var rug2mi: MeshInstance3D = host._mi(q, rug2, host._toon(Color(0.66, 0.28, 0.26), 0.1, false), Vector3(0, 0.03, -4.4))
	rug2mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_art_panel(q, host, Vector3(11.8, 6.2, 3.6), -90.0, MALL_TEAL, "ELA")
	_art_panel(q, host, Vector3(11.8, 6.2, -3.6), -90.0, Color(0.95, 0.75, 0.4), "VERSE")
	_art_panel(q, host, Vector3(-11.8, 8.6, 2.5), 90.0, Color(0.7, 0.6, 1.0), "HEY")
	_planter_bush(q, host, host.MALL_IN, Vector3(10.8, 0, 4.8))
	_planter_bush(q, host, host.MALL_IN, Vector3(10.8, 0, -4.8))
	_planter_bush(q, host, host.MALL_IN, Vector3(-11.0, 0, -3.5))
	_npc(parent, host, host.MALL_IN, 5.5, -0.06, "Mira", "did:verse:npc-mira2")
	_npc(parent, host, host.MALL_IN + Vector3(-4, 0, -2.6), 2.4, 0.09, "Bo", "did:verse:npc-bo")
	# ── kiosk, blossom tree, dome orb cluster ────────────────────────────────
	_kiosk(q, host, Vector3(4.0, 0, -3.6), Color(0.42, 0.74, 0.72))
	host._obstacles.append({"pos": host.MALL_IN + Vector3(4.0, 0, -3.6), "r": 1.1})
	_blossom_tree(q, host, Vector3(-6.0, 0, 4.6))
	host._obstacles.append({"pos": host.MALL_IN + Vector3(-6.0, 0, 4.6), "r": 0.9})
	var orb := SphereMesh.new()
	orb.radius = 0.22
	orb.height = 0.44
	orb.radial_segments = 8
	orb.rings = 4
	for k in 5:
		var oa := TAU * float(k) / 5.0
		var omi2: MeshInstance3D = host._mi(q, orb, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.0),
			Vector3(cos(oa) * (0.8 + 0.4 * float(k % 2)), 12.1 + 0.35 * float(k % 3), sin(oa) * (0.8 + 0.4 * float(k % 2))))
		omi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# ── exits: glowing pads (the entrance doors live on the facade outside) ──
	_lift_pad(q, host, Vector3(0, 0, 5.9), "⬇ Plaza", Color(0.93, 0.84, 0.5))
	host._portals.append({
		"at": host.MALL_IN + Vector3(0, 0, 5.9), "to": ORIGIN + Vector3(0, 0, -14.6), "yaw": 0.0,
	})
	# the escalator IS the way up: step on the lower end and get carried to
	# Level 2 (the matching well up there rides you back down)
	host._portals.append({
		"at": host.MALL_IN + Vector3(5.0, 0, 2.6),
		"ride": host.MALL_IN + Vector3(11.3, 3.32, 3.85),
		"to": host.MALL_IN + Vector3(48.6, 0, 1.0), "yaw": -PI * 0.5,
	})
	var bglow := BoxMesh.new()
	bglow.size = Vector3(1.3, 0.02, 0.5)
	var bgmi: MeshInstance3D = host._mi(q, bglow, VerseAvatar.glow_mat(MALL_TEAL, 0.8), Vector3(5.0, 0.03, 2.6))
	bgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var uplbl := Label3D.new()
	uplbl.text = "Her Level ▲"
	uplbl.font_size = 48
	uplbl.pixel_size = 0.005
	uplbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	uplbl.modulate = MALL_TEAL.lightened(0.2)
	uplbl.outline_size = 8
	uplbl.position = Vector3(5.0, 1.9, 2.6)
	q.add_child(uplbl)
	# floor 1's elevator IS the big panoramic lift: walk up to its glowing
	# pad and the Hey floor sheet opens (upper floors keep their gold stops)
	_lift_pad(q, host, Vector3(-6.05, 0, 0), "Elevator", MALL_TEAL)
	host._portals.append({
		"at": host.MALL_IN + Vector3(-6.05, 0, 0),
		"lift": [
			{"label": "Level 2 — for Her", "to": host.MALL_IN + Vector3(35.7, 0, 2.6), "yaw": PI},
			{"label": "Lounge — for Him", "to": host.MALL_IN + Vector3(75.7, 0, 2.6), "yaw": PI},
			{"label": "Cinema", "to": host.MALL_IN + Vector3(235.7, 0, 2.6), "yaw": PI},
		],
	})
	# step INSIDE a store: through the doorway into its own full room
	host._portals.append({
		"at": host.MALL_IN + Vector3(-7.0, 0, -5.9), "to": host.MALL_IN + Vector3(120, 0, 3.4), "yaw": PI,
	})
	host._portals.append({
		"at": host.MALL_IN + Vector3(7.0, 0, -5.9), "to": host.MALL_IN + Vector3(160, 0, 3.4), "yaw": PI,
	})
	host._portals.append({
		"at": host.MALL_IN + Vector3(-10.7, 0, 2.5), "to": host.MALL_IN + Vector3(200, 0, 3.4), "yaw": PI,
	})
	_store_room(parent, host, host.MALL_IN + Vector3(120, 0, 0), Color(0.86, 0.42, 0.38), "ROBO THREADS", 0,
		host.MALL_IN + Vector3(-7.0, 0, -4.1), 0.0, "Stitch", "Fern")
	_store_room(parent, host, host.MALL_IN + Vector3(160, 0, 0), Color(0.42, 0.74, 0.72), "CIRCUIT & CO", 1,
		host.MALL_IN + Vector3(7.0, 0, -4.1), 0.0, "Ohm", "Dax")
	_store_room(parent, host, host.MALL_IN + Vector3(200, 0, 0), Color(0.92, 0.74, 0.34), "GOLDSMITH", 2,
		host.MALL_IN + Vector3(-9.0, 0, 2.5), PI * 0.5, "Carat", "Vance")


## One white gallery level: slabs + end caps, gold fascia lines, cove light,
## glass balustrade, and a lit shopfront strip on the wall behind.
func _gallery_level(q: Node3D, host: Node, lvl: float, cols: Array, tag: String, names: Array) -> void:
	var slab := BoxMesh.new()
	slab.size = Vector3(23.6, 0.24, 2.2)
	var cap := BoxMesh.new()
	cap.size = Vector3(2.2, 0.24, 14.5)
	var fascia := BoxMesh.new()
	fascia.size = Vector3(23.6, 0.5, 0.1)
	var gold1 := BoxMesh.new()
	gold1.size = Vector3(23.6, 0.05, 0.05)
	var gold2 := BoxMesh.new()
	gold2.size = Vector3(19.0, 0.04, 0.05)
	var cove := BoxMesh.new()
	cove.size = Vector3(23.6, 0.04, 0.04)
	var rail_g := BoxMesh.new()
	rail_g.size = Vector3(23.6, 0.05, 0.05)
	var bal := BoxMesh.new()
	bal.size = Vector3(23.6, 0.62, 0.05)
	var post := BoxMesh.new()
	post.size = Vector3(0.06, 0.74, 0.06)
	# galleries dress the BACK (z-) side only — the front stays open so the
	# camera always sees the avatar (the end caps still close the ring look)
	var deck_mat: Material = _roof_mat(host) if lvl < 4.0 else host._toon(MALL_WHITE, 0.1, false)
	for gz in [-1.0]:
		var gzz: float = gz
		var dmi2: MeshInstance3D = host._mi(q, slab, deck_mat, Vector3(0, lvl, gzz * 6.0))
		if lvl < 4.0:
			dmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# fascia band + the flowing gold lines (the pic-2 signature)
		host._mi(q, fascia, host._toon(MALL_WHITE, 0.1, false), Vector3(0, lvl - 0.3, gzz * 4.86))
		var g1: MeshInstance3D = host._mi(q, gold1, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.7), Vector3(0, lvl - 0.16, gzz * 4.83))
		g1.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var g2: MeshInstance3D = host._mi(q, gold2, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.5), Vector3(0.8, lvl - 0.4, gzz * 4.83))
		g2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# warm cove light washing the floor below
		var cv: MeshInstance3D = host._mi(q, cove, VerseAvatar.glow_mat(Color(1.0, 0.92, 0.75), 0.45), Vector3(0, lvl - 0.52, gzz * 5.6))
		cv.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		# glass balustrade + gold cap rail + posts
		var bmat: StandardMaterial3D = host._glass_mat()
		host._windows.append(bmat)
		host._mi(q, bal, bmat, Vector3(0, lvl + 0.43, gzz * 4.95))
		var rg: MeshInstance3D = host._mi(q, rail_g, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.55), Vector3(0, lvl + 0.78, gzz * 4.95))
		rg.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		for pk in 5:
			host._mi(q, post, host._toon(MALL_WHITE, 0.15, false), Vector3(-9.4 + 4.7 * float(pk), lvl + 0.43, gzz * 4.95))
		# OPEN mini-stores on the gallery — lit interiors, stocked shelves
		_store_lite(q, host, Vector3(-7.0, lvl + 0.12, gzz * 6.6), cols[0], names[0])
		_store_lite(q, host, Vector3(7.0, lvl + 0.12, gzz * 6.6), cols[1], names[1])
		# the level's name, center fascia
		var tagl := Label3D.new()
		tagl.text = tag
		tagl.font_size = 44
		tagl.pixel_size = 0.0055
		tagl.modulate = MALL_GOLD_GLOW
		tagl.outline_size = 8
		tagl.position = Vector3(0, lvl + 1.6, gzz * 6.9)
		q.add_child(tagl)
		# pennant banners hanging from the fascia
		var flag := BoxMesh.new()
		flag.size = Vector3(0.45, 0.7, 0.03)
		for fk in 4:
			var fcol2: Color = cols[fk % cols.size()]
			var fmi3: MeshInstance3D = host._mi(q, flag, host._toon(fcol2, 0.2, false),
				Vector3(-9.0 + 6.0 * float(fk), lvl - 0.85, gzz * 4.8))
			fmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# end caps close the ring
	for gx in [-1.0, 1.0]:
		var gxx: float = gx
		var cmi4: MeshInstance3D = host._mi(q, cap, deck_mat, Vector3(gxx * 10.9, lvl, 0))
		if lvl < 4.0:
			cmi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var cfas := BoxMesh.new()
		cfas.size = Vector3(0.1, 0.5, 14.5)
		host._mi(q, cfas, host._toon(MALL_WHITE, 0.1, false), Vector3(gxx * 9.84, lvl - 0.3, 0))
		var cgold := BoxMesh.new()
		cgold.size = Vector3(0.05, 0.05, 14.0)
		var cgmi: MeshInstance3D = host._mi(q, cgold, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.7), Vector3(gxx * 9.81, lvl - 0.16, 0))
		cgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## The dome: a glass disk under a black mullion grid — radial ribs, rings, a
## gold rim. Reads as the grand geodesic skylight from below.
func _dome_grid(q: Node3D, host: Node, pos: Vector3, r: float) -> void:
	var disk := CylinderMesh.new()
	disk.top_radius = r
	disk.bottom_radius = r
	disk.height = 0.12
	disk.radial_segments = 26
	var dmi: MeshInstance3D = host._mi(q, disk, _roof_mat(host), pos)
	dmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rib := BoxMesh.new()
	rib.size = Vector3(0.1, 0.06, r * 2.0)
	for k in 4:
		var rmi3: MeshInstance3D = host._mi(q, rib, host._toon(MALL_DARK, 0.1, false), pos + Vector3(0, -0.08, 0))
		rmi3.rotation_degrees = Vector3(0, 45.0 * float(k), 0)
		rmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	for rr in [r * 0.38, r * 0.7, r * 0.97]:
		var rrr: float = rr
		var tor := TorusMesh.new()
		tor.inner_radius = rrr - 0.05
		tor.outer_radius = rrr + 0.05
		tor.rings = 30
		tor.ring_segments = 4
		var tmi4: MeshInstance3D = host._mi(q, tor, host._toon(MALL_DARK, 0.1, false), pos + Vector3(0, -0.08, 0))
		tmi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rim := TorusMesh.new()
	rim.inner_radius = r - 0.02
	rim.outer_radius = r + 0.12
	rim.rings = 30
	rim.ring_segments = 4
	var rimmi: MeshInstance3D = host._mi(q, rim, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.6), pos + Vector3(0, -0.12, 0))
	rimmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## The glowing sculpture-tree: white flared trunk rising from a teal light
## pool, slender branches, and a floating canopy of golden orbs (a few bob).
func _glow_tree(q: Node3D, host: Node) -> void:
	# teal-lit base pool + white planter ring
	var pool := CylinderMesh.new()
	pool.top_radius = 1.7
	pool.bottom_radius = 1.7
	pool.height = 0.1
	pool.radial_segments = 22
	var plmi2: MeshInstance3D = host._mi(q, pool, VerseAvatar.glow_mat(MALL_TEAL, 0.8), Vector3(0, 0.06, 0))
	plmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rim2 := TorusMesh.new()
	rim2.inner_radius = 1.78
	rim2.outer_radius = 2.05
	rim2.rings = 26
	rim2.ring_segments = 6
	host._mi(q, rim2, host._toon(MALL_WHITE, 0.15), Vector3(0, 0.16, 0))
	# flared trunk in three sweeps
	var t1 := CylinderMesh.new()
	t1.top_radius = 0.34
	t1.bottom_radius = 0.95
	t1.height = 2.4
	t1.radial_segments = 12
	host._mi(q, t1, host._toon(MALL_WHITE, 0.2), Vector3(0, 1.3, 0))
	var t2 := CylinderMesh.new()
	t2.top_radius = 0.26
	t2.bottom_radius = 0.36
	t2.height = 2.6
	t2.radial_segments = 10
	host._mi(q, t2, host._toon(MALL_WHITE, 0.2), Vector3(0, 3.7, 0))
	var t3 := CylinderMesh.new()
	t3.top_radius = 0.14
	t3.bottom_radius = 0.26
	t3.height = 2.4
	t3.radial_segments = 10
	host._mi(q, t3, host._toon(MALL_WHITE, 0.2), Vector3(0, 6.1, 0))
	# slender branches reaching for the dome
	var br := CylinderMesh.new()
	br.top_radius = 0.05
	br.bottom_radius = 0.11
	br.height = 2.6
	br.radial_segments = 8
	for k in 5:
		var ba := TAU * float(k) / 5.0 + 0.35
		var bmi5: MeshInstance3D = host._mi(q, br, host._toon(MALL_WHITE, 0.2),
			Vector3(cos(ba) * 0.95, 7.6, sin(ba) * 0.95))
		bmi5.rotation_degrees = Vector3(cos(ba) * 0.0 + 26.0, -rad_to_deg(ba) + 90.0, 0)
	# the orb canopy: golden glass bubbles, a few teal — some drift gently
	var sizes := [0.3, 0.24, 0.2, 0.27, 0.18, 0.22, 0.26, 0.17, 0.21, 0.24, 0.19, 0.28, 0.16, 0.23]
	for k in sizes.size():
		var oa := TAU * float(k) / float(sizes.size()) * 2.6
		var orad := 0.7 + 2.1 * fposmod(float(k) * 0.61, 1.0)
		var oy := 7.4 + 3.2 * fposmod(float(k) * 0.37, 1.0)
		var os2: float = sizes[k]
		var orbm := SphereMesh.new()
		orbm.radius = os2
		orbm.height = os2 * 2.0
		orbm.radial_segments = 8
		orbm.rings = 4
		var col := MALL_GOLD_GLOW if k % 4 != 3 else MALL_TEAL
		var energy := 0.9 if k % 4 != 3 else 0.7
		var p := Vector3(cos(oa) * orad, oy, sin(oa) * orad)
		if k % 3 == 0:
			var sp: Node3D = _spinner(q, p, 0.0, 0.16)
			var omi3: MeshInstance3D = host._mi(sp, orbm, VerseAvatar.glow_mat(col, energy), Vector3.ZERO)
			omi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		else:
			var omi4: MeshInstance3D = host._mi(q, orbm, VerseAvatar.glow_mat(col, energy), p)
			omi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## An organic gold ribbon bench sweeping an arc — slatted segments riding the
## curve (the pic-2 swirl seating).
func _ribbon_bench(q: Node3D, host: Node, center: Vector3, r: float, a0_deg: float, a1_deg: float) -> void:
	var seg := BoxMesh.new()
	seg.size = Vector3(0.95, 0.42, 0.4)
	var back := BoxMesh.new()
	back.size = Vector3(0.95, 0.6, 0.08)
	var n := 7
	for k in n:
		var t := float(k) / float(n - 1)
		var a := deg_to_rad(lerpf(a0_deg, a1_deg, t))
		var p := center + Vector3(cos(a) * r, 0.21, sin(a) * r)
		var smi8: MeshInstance3D = host._mi(q, seg, host._toon(MALL_GOLD, 0.25), p)
		smi8.rotation_degrees = Vector3(0, -rad_to_deg(a) + 90.0, 0)
		# the back swells through the middle of the arc, like a rising ribbon
		var bh := sin(t * PI)
		if bh > 0.3:
			var bmi6: MeshInstance3D = host._mi(q, back, host._toon(MALL_GOLD.darkened(0.08), 0.25),
				p + Vector3(cos(a) * 0.18, 0.3 + 0.18 * bh, sin(a) * 0.18))
			bmi6.rotation_degrees = Vector3(0, -rad_to_deg(a) + 90.0, -8.0)


## A crossing glass escalator: inclined ramp, glass sides with gold caps, and
## glowing steps that ride the slope on a loop.
func _escalator(q: Node3D, host: Node, base: Vector3, yaw_deg: float) -> void:
	var e := Node3D.new()
	e.position = base
	e.rotation_degrees = Vector3(0, yaw_deg, 0)
	q.add_child(e)
	var run := 6.6
	var rise := 3.4
	var ang := atan2(rise, run)
	var ramp := BoxMesh.new()
	ramp.size = Vector3(sqrt(run * run + rise * rise), 0.16, 1.3)
	var rmi4: MeshInstance3D = host._mi(e, ramp, host._toon(Color(0.82, 0.84, 0.88), 0.15, false), Vector3(run * 0.5, rise * 0.5, 0))
	rmi4.rotation_degrees = Vector3(0, 0, rad_to_deg(ang))
	var side := BoxMesh.new()
	side.size = Vector3(sqrt(run * run + rise * rise), 0.9, 0.06)
	for sz in [-0.65, 0.65]:
		var szz: float = sz
		var smat: StandardMaterial3D = host._glass_mat()
		host._windows.append(smat)
		var smi9: MeshInstance3D = host._mi(e, side, smat, Vector3(run * 0.5, rise * 0.5 + 0.5, szz))
		smi9.rotation_degrees = Vector3(0, 0, rad_to_deg(ang))
		var cap2 := BoxMesh.new()
		cap2.size = Vector3(sqrt(run * run + rise * rise), 0.06, 0.08)
		var cmi2: MeshInstance3D = host._mi(e, cap2, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.6), Vector3(run * 0.5, rise * 0.5 + 0.96, szz))
		cmi2.rotation_degrees = Vector3(0, 0, rad_to_deg(ang))
		cmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# moving steps: three glow strips, each riding its third of the slope on a
	# loop — together they read as the escalator belt climbing forever
	var step := BoxMesh.new()
	step.size = Vector3(0.5, 0.06, 1.1)
	var t0 := Vector3(0.6, 0.4, 0)
	var t1 := Vector3(run - 0.4, rise + 0.05, 0)
	for k in 3:
		var s0 := t0.lerp(t1, float(k) / 3.0)
		var s1 := t0.lerp(t1, float(k + 1) / 3.0)
		var stmi2: MeshInstance3D = host._mi(e, step, VerseAvatar.glow_mat(Color(0.85, 0.95, 1.0), 0.55), s0)
		stmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var tw := stmi2.create_tween()
		tw.set_loops()
		tw.tween_property(stmi2, "position", s1, 1.2)
		tw.tween_callback(func() -> void: stmi2.position = s0)


## The panoramic lift: four gold rails and a glass cab gliding up and down.
func _glass_lift(q: Node3D, host: Node, base: Vector3) -> void:
	var l := Node3D.new()
	l.position = base
	q.add_child(l)
	var rail := BoxMesh.new()
	rail.size = Vector3(0.08, 12.8, 0.08)
	for rx in [-0.75, 0.75]:
		for rz in [-0.75, 0.75]:
			var rxx: float = rx
			var rzz: float = rz
			host._mi(l, rail, host._toon(MALL_GOLD, 0.3), Vector3(rxx, 6.4, rzz))
	var basep := CylinderMesh.new()
	basep.top_radius = 1.05
	basep.bottom_radius = 1.15
	basep.height = 0.12
	basep.radial_segments = 14
	var bpmi: MeshInstance3D = host._mi(l, basep, VerseAvatar.glow_mat(MALL_TEAL, 0.5), Vector3(0, 0.07, 0))
	bpmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var capb := BoxMesh.new()
	capb.size = Vector3(1.9, 0.14, 1.9)
	host._mi(l, capb, host._toon(MALL_GOLD, 0.3), Vector3(0, 12.85, 0))
	# the cab: glass box on a slow bob spinner — forever gliding between floors
	var cabspin: Node3D = _spinner(l, Vector3(0, 6.2, 0), 0.0, 4.6)
	var cab := BoxMesh.new()
	cab.size = Vector3(1.35, 2.0, 1.35)
	var cmat2: StandardMaterial3D = host._glass_mat()
	host._windows.append(cmat2)
	host._mi(cabspin, cab, cmat2, Vector3.ZERO)
	var cfloor := BoxMesh.new()
	cfloor.size = Vector3(1.35, 0.1, 1.35)
	host._mi(cabspin, cfloor, host._toon(MALL_GOLD, 0.3), Vector3(0, -1.0, 0))
	var ctop := BoxMesh.new()
	ctop.size = Vector3(1.45, 0.1, 1.45)
	host._mi(cabspin, ctop, host._toon(MALL_GOLD, 0.3), Vector3(0, 1.05, 0))


## A round boutique kiosk: white drum, gold flared roof, lit counter band.
func _kiosk(q: Node3D, host: Node, pos: Vector3, col: Color) -> void:
	var drum := CylinderMesh.new()
	drum.top_radius = 0.95
	drum.bottom_radius = 0.95
	drum.height = 1.15
	drum.radial_segments = 12
	host._mi(q, drum, host._toon(MALL_WHITE, 0.15), pos + Vector3(0, 0.58, 0))
	var bandk := CylinderMesh.new()
	bandk.top_radius = 0.97
	bandk.bottom_radius = 0.97
	bandk.height = 0.1
	bandk.radial_segments = 12
	var bkmi: MeshInstance3D = host._mi(q, bandk, VerseAvatar.glow_mat(col.lightened(0.2), 0.8), pos + Vector3(0, 0.95, 0))
	bkmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var roof := CylinderMesh.new()
	roof.top_radius = 1.45
	roof.bottom_radius = 0.25
	roof.height = 0.5
	roof.radial_segments = 12
	host._mi(q, roof, host._toon(MALL_GOLD, 0.3), pos + Vector3(0, 2.0, 0))
	var polek := CylinderMesh.new()
	polek.top_radius = 0.07
	polek.bottom_radius = 0.07
	polek.height = 0.7
	polek.radial_segments = 8
	host._mi(q, polek, host._toon(MALL_WHITE, 0.15), pos + Vector3(0, 1.5, 0))


## The moon gate — a glowing circular flagship entrance (pic-1's round arch).
func _moon_gate(q: Node3D, host: Node, pos: Vector3) -> void:
	var wallp := BoxMesh.new()
	wallp.size = Vector3(5.2, 4.6, 0.5)
	host._mi(q, wallp, host._toon(MALL_WHITE, 0.1, false), pos + Vector3(0, 2.3, 0))
	var arch := TorusMesh.new()
	arch.inner_radius = 1.45
	arch.outer_radius = 1.8
	arch.rings = 26
	arch.ring_segments = 8
	var ami3: MeshInstance3D = host._mi(q, arch, host._toon(MALL_GOLD, 0.3), pos + Vector3(0, 1.85, 0.3))
	ami3.rotation_degrees = Vector3(90, 0, 0)
	var glow := TorusMesh.new()
	glow.inner_radius = 1.38
	glow.outer_radius = 1.48
	glow.rings = 26
	glow.ring_segments = 6
	var gmi2: MeshInstance3D = host._mi(q, glow, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.1), pos + Vector3(0, 1.85, 0.34))
	gmi2.rotation_degrees = Vector3(90, 0, 0)
	gmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var inner := CylinderMesh.new()
	inner.top_radius = 1.36
	inner.bottom_radius = 1.36
	inner.height = 0.1
	inner.radial_segments = 22
	var imi2: MeshInstance3D = host._mi(q, inner, VerseAvatar.glow_mat(Color(1.0, 0.95, 0.8), 0.35), pos + Vector3(0, 1.85, 0.28))
	imi2.rotation_degrees = Vector3(90, 0, 0)
	imi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## An indoor blossom tree in a gold planter (the pic-1 pink tree).
func _blossom_tree(q: Node3D, host: Node, pos: Vector3) -> void:
	var pot := CylinderMesh.new()
	pot.top_radius = 0.62
	pot.bottom_radius = 0.5
	pot.height = 0.55
	pot.radial_segments = 12
	host._mi(q, pot, host._toon(MALL_GOLD, 0.3), pos + Vector3(0, 0.28, 0))
	var trunk := CylinderMesh.new()
	trunk.top_radius = 0.07
	trunk.bottom_radius = 0.13
	trunk.height = 1.6
	trunk.radial_segments = 8
	host._mi(q, trunk, host._toon(Color(0.45, 0.33, 0.26), 0.2), pos + Vector3(0, 1.3, 0))
	var puffs := [
		[Vector3(0, 2.25, 0), 0.62], [Vector3(0.45, 2.0, 0.2), 0.45],
		[Vector3(-0.4, 2.05, -0.15), 0.42], [Vector3(0.1, 1.85, -0.4), 0.36],
	]
	for p in puffs:
		var pa: Array = p
		var puff := SphereMesh.new()
		var pr: float = pa[1]
		puff.radius = pr
		puff.height = pr * 2.0
		puff.radial_segments = 10
		puff.rings = 5
		host._mi(q, puff, host._toon(Color(0.95, 0.7, 0.78), 0.3, true, 0.12, 0.85), pos + pa[0])


## A glowing teleport pad with a floating label — the mall's lifts.
func _lift_pad(q: Node3D, host: Node, pos: Vector3, text: String, col: Color) -> void:
	var pad := CylinderMesh.new()
	pad.top_radius = 0.85
	pad.bottom_radius = 0.85
	pad.height = 0.03
	pad.radial_segments = 18
	var pmi3: MeshInstance3D = host._mi(q, pad, VerseAvatar.glow_mat(col, 0.8), pos + Vector3(0, 0.025, 0))
	pmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var ringp := TorusMesh.new()
	ringp.inner_radius = 0.86
	ringp.outer_radius = 0.96
	ringp.rings = 22
	ringp.ring_segments = 4
	var rpmi: MeshInstance3D = host._mi(q, ringp, host._toon(MALL_GOLD, 0.3), pos + Vector3(0, 0.04, 0))
	rpmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var lbl := Label3D.new()
	lbl.text = text
	lbl.font_size = 64
	lbl.pixel_size = 0.006
	lbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	lbl.modulate = col.lightened(0.2)
	lbl.outline_size = 10
	lbl.position = pos + Vector3(0, 1.7, 0)
	q.add_child(lbl)


## LEVEL 2 — a walkable gallery ring around the open atrium: look down on the
## glow tree, shop the ring, ride on up to the sky lounge.
func _mall_l2(parent: Node3D, host: Node) -> void:
	var l2: Vector3 = host.MALL_IN + Vector3(40, 0, 0)
	var q := Node3D.new()
	q.position = l2
	parent.add_child(q)
	# ONE BIG OPEN FLOOR — Level 2 is a full deck now (no atrium hole), so
	# the whole storey is usable space, like the home's upper floor
	var fl2 := BoxMesh.new()
	fl2.size = Vector3(24, 0.3, 14.5)
	host._mi(q, fl2, host._toon(MALL_CREAM, 0.1, false, 0.0, 0.5, 0.4), Vector3(0, -0.15, 0))
	# center medallion, carpet and ribbon seating — furnished, not empty
	var ring4 := TorusMesh.new()
	ring4.inner_radius = 2.1
	ring4.outer_radius = 2.24
	ring4.rings = 30
	ring4.ring_segments = 4
	var r4mi: MeshInstance3D = host._mi(q, ring4, host._toon(MALL_GOLD, 0.25, false), Vector3(0, 0.022, 0))
	r4mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rug3 := BoxMesh.new()
	rug3.size = Vector3(2.4, 0.025, 8.0)
	var rg3mi: MeshInstance3D = host._mi(q, rug3, host._toon(Color(0.3, 0.42, 0.6), 0.1, false), Vector3(0, 0.03, 0))
	rg3mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_ribbon_bench(q, host, Vector3.ZERO, 3.1, 30.0, 140.0)
	_ribbon_bench(q, host, Vector3.ZERO, 3.1, 210.0, 320.0)
	for ang3 in [85.0, 265.0]:
		var aa3 := deg_to_rad(ang3)
		host._obstacles.append({"pos": l2 + Vector3(cos(aa3) * 3.1, 0, sin(aa3) * 3.1), "r": 0.55})
	# the escalator WELL (east edge): the hall's moving stair arrives here,
	# and stepping back onto it rides you down
	var wellm := BoxMesh.new()
	wellm.size = Vector3(2.8, 0.04, 1.15)
	var wmi6: MeshInstance3D = host._mi(q, wellm, host._toon(Color(0.06, 0.07, 0.1), 0.02, false), Vector3(10.6, 0.012, 2.42))
	wmi6.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var wramp := BoxMesh.new()
	wramp.size = Vector3(3.4, 0.14, 1.05)
	var wrmi: MeshInstance3D = host._mi(q, wramp, host._toon(Color(0.82, 0.84, 0.88), 0.15, false), Vector3(10.3, -0.75, 2.42))
	wrmi.rotation_degrees = Vector3(0, 0, 27)
	var wcap := BoxMesh.new()
	wcap.size = Vector3(3.4, 0.05, 0.07)
	for cz3 in [-0.5, 0.5]:
		var czz3: float = cz3
		var wcmi: MeshInstance3D = host._mi(q, wcap, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.6), Vector3(10.3, -0.42, 2.42 + czz3))
		wcmi.rotation_degrees = Vector3(0, 0, 27)
		wcmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var wrail := BoxMesh.new()
	wrail.size = Vector3(2.9, 0.55, 0.05)
	var wgold := BoxMesh.new()
	wgold.size = Vector3(2.9, 0.05, 0.05)
	for wz2 in [1.82, 3.02]:
		var wzz2: float = wz2
		var wmat4: StandardMaterial3D = host._glass_mat()
		host._windows.append(wmat4)
		host._mi(q, wrail, wmat4, Vector3(10.6, 0.4, wzz2))
		var wgmi: MeshInstance3D = host._mi(q, wgold, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.55), Vector3(10.6, 0.7, wzz2))
		wgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	host._boxes.append({"pos": l2 + Vector3(10.6, 0, 1.8), "half": Vector2(1.5, 0.12)})
	host._boxes.append({"pos": l2 + Vector3(10.6, 0, 3.05), "half": Vector2(1.5, 0.1)})
	# outer walls + a decorative upper fascia (more floors above, implied)
	var wall_mat: ShaderMaterial = host._toon(Color(0.74, 0.82, 0.9), 0.1, false)
	var wn2 := BoxMesh.new()
	wn2.size = Vector3(24, 9.6, 0.3)
	# front (z+) wall left undrawn — dollhouse view, collision box still seals
	host._mi(q, wn2, wall_mat, Vector3(0, 4.8, -7.25))
	var we2 := BoxMesh.new()
	we2.size = Vector3(0.3, 9.6, 14.5)
	host._mi(q, we2, wall_mat, Vector3(-12.05, 4.8, 0))
	host._mi(q, we2, wall_mat, Vector3(12.05, 4.8, 0))
	host._boxes.append({"pos": l2 + Vector3(0, 0, -7.25), "half": Vector2(12.2, 0.3)})
	host._boxes.append({"pos": l2 + Vector3(0, 0, 7.25), "half": Vector2(12.2, 0.3)})
	host._boxes.append({"pos": l2 + Vector3(-12.05, 0, 0), "half": Vector2(0.3, 7.4)})
	host._boxes.append({"pos": l2 + Vector3(12.05, 0, 0), "half": Vector2(0.3, 7.4)})
	var fas2 := BoxMesh.new()
	fas2.size = Vector3(23.6, 0.5, 0.1)
	var fgold := BoxMesh.new()
	fgold.size = Vector3(23.6, 0.05, 0.05)
	for gz3 in [-7.05]:
		var gzz3: float = gz3
		host._mi(q, fas2, host._toon(MALL_WHITE, 0.1, false), Vector3(0, 4.7, gzz3))
		var fgmi: MeshInstance3D = host._mi(q, fgold, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.7), Vector3(0, 4.84, gzz3 * 0.997))
		fgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# the dome again, closer now
	_dome_grid(q, host, Vector3(0, 9.5, 0), 7.4)
	# REAL walk-in stores — Level 2 is the tech floor (shallower units so the
	# ring lane stays wide) + two flagship corners on the end caps
	_store(q, host, l2, Vector3(-7.0, 0, -6.2), 0.0, Color(0.95, 0.62, 0.75), "LUNA BEAUTY", 1.8, "Indi")
	_store(q, host, l2, Vector3(0.0, 0, -6.2), 0.0, Color(0.88, 0.45, 0.6), "VELVET ROSE", 1.8)
	_store(q, host, l2, Vector3(7.0, 0, -6.2), 0.0, Color(0.92, 0.78, 0.86), "PEARL", 1.8, "Nyx")
	_store(q, host, l2, Vector3(-11.0, 0, -2.2), 90.0, Color(0.85, 0.5, 0.7), "CHIC SHOES", 1.8)
	_store(q, host, l2, Vector3(7.0, 0, 6.35), 180.0, Color(0.55, 0.78, 0.45), "BLOOM", 1.8)
	_store(q, host, l2, Vector3(-7.0, 0, 6.35), 180.0, Color(0.78, 0.55, 0.9), "SILK & STAR", 1.8, "Vio")
	_wall_dress(q, host, Vector3(0, 0, -7.1), 0.0, 23.6, 9.4)
	_wall_dress(q, host, Vector3(11.88, 0, 0), -90.0, 14.2, 9.4)
	_wall_dress(q, host, Vector3(-11.88, 0, 0), 90.0, 14.2, 9.4)
	_art_panel(q, host, Vector3(3.5, 6.8, -7.05), 0.0, Color(0.95, 0.6, 0.75), "FOR HER")
	_planter_bush(q, host, l2, Vector3(-3.6, 0, -5.8))
	_planter_bush(q, host, l2, Vector3(10.6, 0, -5.6))
	# life on the ring
	_blossom_tree(q, host, Vector3(8.6, 0, -4.6))
	host._obstacles.append({"pos": l2 + Vector3(8.6, 0, -4.6), "r": 0.9})
	_kiosk(q, host, Vector3(-2.6, 0, 1.0), Color(0.45, 0.55, 0.92))
	host._obstacles.append({"pos": l2 + Vector3(-2.6, 0, 1.0), "r": 1.1})
	_ribbon_bench(q, host, Vector3(0, 0, 0), 6.2, 230.0, 310.0)
	_npc(parent, host, l2, 6.5, 0.05, "Faye", "did:verse:npc-faye")
	_npc(parent, host, l2, 6.5, -0.045, "Orin", "did:verse:npc-orin", 1.0, 2.4)
	_npc(parent, host, l2 + Vector3(0, 0, -3.8), 1.2, 0.2, "Lulu", "did:verse:npc-lulu", 0.55)
	_stander(q, host, Vector3(-7.0, 0, -5.2), Vector3(-7.0, 0, -6.55), "Lyra", "did:verse:npc-lyra", 1.0,
		["her level has the BEST lighting in the verse."])
	_stander(q, host, Vector3(3.4, 0, -3.9), Vector3(0, 0, 0), "Pix", "did:verse:npc-pix", 0.55,
		["this whole floor is OURS!"])
	# down to the hall: ride the escalator well; the elevator serves the rest
	host._portals.append({
		"at": l2 + Vector3(8.9, 0, 2.42),
		"ride": l2 + Vector3(11.0, -1.7, 2.42),
		"to": host.MALL_IN + Vector3(4.2, 0, 1.2), "yaw": -PI * 0.5,
	})
	var dnlbl := Label3D.new()
	dnlbl.text = "⬇ Floor 1"
	dnlbl.font_size = 44
	dnlbl.pixel_size = 0.005
	dnlbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	dnlbl.modulate = Color(0.93, 0.84, 0.5)
	dnlbl.outline_size = 8
	dnlbl.position = Vector3(8.9, 1.7, 2.42)
	q.add_child(dnlbl)
	_lift_stop(q, host, l2, Vector3(-4.3, 0, 4.8), 180.0, [
		{"label": "Floor 1", "to": host.MALL_IN + Vector3(-5.2, 0, 0), "yaw": PI * 0.5},
		{"label": "Lounge — for Him", "to": host.MALL_IN + Vector3(75.7, 0, 2.6), "yaw": PI},
		{"label": "Cinema", "to": host.MALL_IN + Vector3(235.7, 0, 2.6), "yaw": PI},
	])


## LEVEL 3 — the sky lounge: cafe tables under a small dome, golden chandelier
## orbs, and a glass wall with the Ela City skyline glittering beyond.
func _mall_l3(parent: Node3D, host: Node) -> void:
	var l3: Vector3 = host.MALL_IN + Vector3(80, 0, 0)
	var q := Node3D.new()
	q.position = l3
	parent.add_child(q)
	var fl3 := BoxMesh.new()
	fl3.size = Vector3(18, 0.3, 11)
	host._mi(q, fl3, host._toon(MALL_CREAM, 0.1, false, 0.0, 0.5, 0.4), Vector3(0, -0.15, 0))
	# walls: solid on three sides, GLASS on the north — the view side
	var wall_mat: ShaderMaterial = host._toon(Color(0.74, 0.82, 0.9), 0.1, false)
	# front (z+) wall undrawn — the lounge reads like a stage set, the avatar
	# always visible; its collision box below still seals the room
	var we3 := BoxMesh.new()
	we3.size = Vector3(0.3, 6.8, 11)
	host._mi(q, we3, wall_mat, Vector3(-9.05, 3.4, 0))
	host._mi(q, we3, wall_mat, Vector3(9.05, 3.4, 0))
	host._boxes.append({"pos": l3 + Vector3(0, 0, 5.5), "half": Vector2(9.2, 0.3)})
	host._boxes.append({"pos": l3 + Vector3(-9.05, 0, 0), "half": Vector2(0.3, 5.65)})
	host._boxes.append({"pos": l3 + Vector3(9.05, 0, 0), "half": Vector2(0.3, 5.65)})
	host._boxes.append({"pos": l3 + Vector3(0, 0, -5.5), "half": Vector2(9.2, 0.3)})
	var gpane := BoxMesh.new()
	gpane.size = Vector3(3.4, 5.6, 0.12)
	var gfin := BoxMesh.new()
	gfin.size = Vector3(0.14, 5.6, 0.3)
	for k in 5:
		var px2 := -7.2 + 3.6 * float(k)
		var gm3: StandardMaterial3D = host._glass_mat()
		host._windows.append(gm3)
		host._mi(q, gpane, gm3, Vector3(px2, 2.9, -5.42))
		if k < 4:
			host._mi(q, gfin, host._toon(MALL_WHITE, 0.15, false), Vector3(px2 + 1.8, 2.9, -5.44))
	# the skyline beyond the glass: glowing towers + a moon in the dark
	_far_tower(parent, host, l3 + Vector3(-6.5, 0, -9.5), 5.5, 1.3)
	_far_tower(parent, host, l3 + Vector3(-2.5, 0, -11.0), 4.2, 1.1)
	_far_tower(parent, host, l3 + Vector3(1.5, 0, -9.8), 6.2, 1.4)
	_far_tower(parent, host, l3 + Vector3(5.5, 0, -10.8), 3.8, 1.0)
	_far_tower(parent, host, l3 + Vector3(8.5, 0, -9.2), 5.0, 1.2)
	var moon := SphereMesh.new()
	moon.radius = 0.7
	moon.height = 1.4
	moon.radial_segments = 12
	moon.rings = 6
	var mmi: MeshInstance3D = host._mi(q, moon, VerseAvatar.glow_mat(Color(0.95, 0.93, 0.85), 0.8), Vector3(3.0, 5.6, -10.5))
	mmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var star := SphereMesh.new()
	star.radius = 0.05
	star.height = 0.1
	star.radial_segments = 6
	star.rings = 3
	for k in 7:
		var smi10: MeshInstance3D = host._mi(q, star, VerseAvatar.glow_mat(Color(0.9, 0.95, 1.0), 0.9),
			Vector3(-7.0 + 2.3 * float(k), 5.0 + 1.4 * fposmod(float(k) * 0.7, 1.0), -11.5))
		smi10.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# small dome + chandelier of golden orbs
	_dome_grid(q, host, Vector3(0, 6.7, 0), 5.6)
	var corb := SphereMesh.new()
	corb.radius = 0.16
	corb.height = 0.32
	corb.radial_segments = 8
	corb.rings = 4
	for k in 7:
		var ca2 := TAU * float(k) / 7.0
		var comi: MeshInstance3D = host._mi(q, corb, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.0),
			Vector3(cos(ca2) * (0.5 + 0.35 * float(k % 3)), 5.6 - 0.45 * float(k % 3), sin(ca2) * (0.5 + 0.35 * float(k % 3))))
		comi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# gold fascia ring + cove
	# the lounge: cafe tables, food kiosks, a ribbon bench, the blossom tree
	for tp in [Vector3(-5.5, 0, -1.5), Vector3(-2.0, 0, 1.8), Vector3(4.5, 0, -1.0)]:
		var tpp: Vector3 = tp
		host._obstacles.append({"pos": l3 + tpp, "r": 0.7})
		var polet := CylinderMesh.new()
		polet.top_radius = 0.05
		polet.bottom_radius = 0.05
		polet.height = 0.75
		polet.radial_segments = 8
		host._mi(q, polet, host._toon(MALL_GOLD, 0.3), tpp + Vector3(0, 0.38, 0))
		var topt := CylinderMesh.new()
		topt.top_radius = 0.55
		topt.bottom_radius = 0.5
		topt.height = 0.06
		topt.radial_segments = 12
		host._mi(q, topt, host._toon(MALL_WHITE, 0.15), tpp + Vector3(0, 0.78, 0))
	_store(q, host, l3, Vector3(8.0, 0, 0.2), -90.0, Color(0.42, 0.6, 0.86), "BARBER BOT", 1.8, "Fade")
	_store(q, host, l3, Vector3(-8.0, 0, 0.2), 90.0, Color(0.45, 0.55, 0.7), "GENT TECH", 1.8)
	_planter_bush(q, host, l3, Vector3(4.0, 0, 4.6))
	_kiosk(q, host, Vector3(6.8, 0, 3.2), Color(0.85, 0.72, 0.34))
	host._obstacles.append({"pos": l3 + Vector3(6.8, 0, 3.2), "r": 1.1})
	_kiosk(q, host, Vector3(-6.8, 0, 3.2), Color(0.35, 0.45, 0.6))
	host._obstacles.append({"pos": l3 + Vector3(-6.8, 0, 3.2), "r": 1.1})
	_ribbon_bench(q, host, Vector3(0, 0, -2.8), 4.4, 200.0, 340.0)
	_blossom_tree(q, host, Vector3(8.0, 0, -3.8))
	host._obstacles.append({"pos": l3 + Vector3(8.0, 0, -3.8), "r": 0.9})
	# lounge life
	_npc(parent, host, l3 + Vector3(0, 0, 0.5), 4.0, 0.05, "Juniper", "did:verse:npc-juniper")
	_stander(q, host, Vector3(-1.4, 0, -4.4), Vector3(-1.4, 0, -5.42), "Sol", "did:verse:npc-sol", 1.0,
		["best view in the whole verse.", "they say the moon up here is hand-made."])
	_stander(q, host, Vector3(-5.0, 0, -0.6), Vector3(-5.5, 0, -1.5), "Remy", "did:verse:npc-remy")
	_stander(q, host, Vector3(7.0, 0, 1.8), Vector3(6.8, 0, 3.2), "Tofu", "did:verse:npc-tofu", 0.55,
		["dad said I can watch the haircut!"])
	# the elevator home: floor 1 or the gallery ring
	_lift_stop(q, host, l3, Vector3(-4.3, 0, 4.8), 180.0, [
		{"label": "Floor 1", "to": host.MALL_IN + Vector3(-5.2, 0, 0), "yaw": PI * 0.5},
		{"label": "Level 2 — for Her", "to": host.MALL_IN + Vector3(35.7, 0, 2.6), "yaw": PI},
		{"label": "Cinema", "to": host.MALL_IN + Vector3(235.7, 0, 2.6), "yaw": PI},
	])


## ───────────────────── REAL OPEN STORES + mall dressing ────────────────────

## A real OPEN store you can walk into: lit interior, stocked shelves, a
## counter with a register, display windows flanking an open doorway, sign +
## neon + name — protruding from the wall line (no wall holes needed).
## yaw 0 faces +z. `zone` = the zone's global origin (collision is global).
func _store(q: Node3D, host: Node, zone: Vector3, pos: Vector3, yaw_deg: float, col: Color, sname: String, depth: float = 2.4, clerk: String = "") -> void:
	var s := Node3D.new()
	s.position = pos
	s.rotation_degrees = Vector3(0, yaw_deg, 0)
	q.add_child(s)
	var hd := depth * 0.5
	var inner := col.lightened(0.5)
	# shell: tinted back wall, white sides, lit ceiling
	var back := BoxMesh.new()
	back.size = Vector3(5.4, 3.1, 0.12)
	host._mi(s, back, host._toon(inner, 0.1, false), Vector3(0, 1.55, -hd + 0.06))
	var sidew := BoxMesh.new()
	sidew.size = Vector3(0.12, 3.1, depth)
	host._mi(s, sidew, host._toon(MALL_WHITE, 0.1, false), Vector3(-2.64, 1.55, 0))
	host._mi(s, sidew, host._toon(MALL_WHITE, 0.1, false), Vector3(2.64, 1.55, 0))
	var ceil := BoxMesh.new()
	ceil.size = Vector3(5.4, 0.12, depth)
	host._mi(s, ceil, host._toon(MALL_WHITE, 0.1, false), Vector3(0, 3.16, 0))
	var lamp := BoxMesh.new()
	lamp.size = Vector3(1.7, 0.05, 0.4)
	for lx in [-1.2, 1.2]:
		var lxx: float = lx
		var lmi4: MeshInstance3D = host._mi(s, lamp, VerseAvatar.glow_mat(Color(1.0, 0.95, 0.82), 0.85), Vector3(lxx, 3.08, 0))
		lmi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var mat2 := BoxMesh.new()
	mat2.size = Vector3(5.0, 0.04, depth - 0.4)
	var matmi: MeshInstance3D = host._mi(s, mat2, host._toon(col.darkened(0.05), 0.1, false), Vector3(0, 0.025, 0))
	matmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# stocked shelves on the back wall
	var shelf := BoxMesh.new()
	shelf.size = Vector3(4.6, 0.07, 0.45)
	var prod := BoxMesh.new()
	prod.size = Vector3(0.38, 0.4, 0.3)
	var pcols := [col, col.lightened(0.25), Color(0.92, 0.74, 0.34), Color(0.42, 0.74, 0.72), Color(0.88, 0.52, 0.72)]
	for sy in [1.15, 1.95]:
		var syy: float = sy
		host._mi(s, shelf, host._toon(MALL_WHITE.darkened(0.06), 0.15, false), Vector3(0, syy, -hd + 0.34))
		for k in 5:
			var pc: Color = pcols[(k + int(syy * 2.0)) % pcols.size()]
			host._mi(s, prod, host._toon(pc, 0.25, false), Vector3(-1.8 + 0.9 * float(k), syy + 0.24, -hd + 0.34))
	# counter + register + glow pad
	var cnt := BoxMesh.new()
	cnt.size = Vector3(1.7, 0.95, 0.6)
	host._mi(s, cnt, host._toon(MALL_WHITE, 0.15, false), Vector3(1.25, 0.48, -hd + 1.0))
	var cntt := BoxMesh.new()
	cntt.size = Vector3(1.75, 0.05, 0.65)
	host._mi(s, cntt, host._toon(MALL_GOLD, 0.3), Vector3(1.25, 0.98, -hd + 1.0))
	var reg := BoxMesh.new()
	reg.size = Vector3(0.3, 0.26, 0.2)
	host._mi(s, reg, host._toon(Color(0.18, 0.2, 0.24), 0.15, false), Vector3(1.0, 1.14, -hd + 1.0))
	var rp := BoxMesh.new()
	rp.size = Vector3(0.26, 0.02, 0.16)
	var rpmi2: MeshInstance3D = host._mi(s, rp, VerseAvatar.glow_mat(MALL_TEAL, 0.8), Vector3(1.55, 1.02, -hd + 1.0))
	rpmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# front: display windows + risers flanking the OPEN doorway
	var riser := BoxMesh.new()
	riser.size = Vector3(1.6, 0.5, 0.12)
	var disp := BoxMesh.new()
	disp.size = Vector3(1.6, 1.9, 0.08)
	for wx in [-1.85, 1.85]:
		var wxx: float = wx
		host._mi(s, riser, host._toon(MALL_WHITE, 0.15, false), Vector3(wxx, 0.25, hd - 0.06))
		var dgm: StandardMaterial3D = host._glass_mat()
		host._windows.append(dgm)
		host._mi(s, disp, dgm, Vector3(wxx, 1.45, hd - 0.04))
		# a mannequin block in each display window
		var mq := BoxMesh.new()
		mq.size = Vector3(0.34, 0.9, 0.3)
		host._mi(s, mq, host._toon(col.lightened(0.15), 0.25, false), Vector3(wxx, 0.95, hd - 0.34))
	# fascia + neon + the store's name in lights
	var fas := BoxMesh.new()
	fas.size = Vector3(5.4, 0.55, 0.14)
	host._mi(s, fas, host._toon(col, 0.25), Vector3(0, 2.85, hd - 0.02))
	var neon := BoxMesh.new()
	neon.size = Vector3(5.0, 0.05, 0.05)
	var nmi4: MeshInstance3D = host._mi(s, neon, VerseAvatar.glow_mat(col.lightened(0.3), 1.4), Vector3(0, 2.52, hd + 0.02))
	nmi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var lbl := Label3D.new()
	lbl.text = sname
	lbl.font_size = 52
	lbl.pixel_size = 0.006
	lbl.modulate = Color(1.0, 0.97, 0.9)
	lbl.outline_size = 8
	lbl.position = Vector3(0, 2.85, hd + 0.08)
	s.add_child(lbl)
	# warm doorway glow line on the floor
	var dg := BoxMesh.new()
	dg.size = Vector3(1.5, 0.02, 0.08)
	var dgmi: MeshInstance3D = host._mi(s, dg, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.7), Vector3(0, 0.03, hd))
	dgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# collision: side walls + window risers (doorway stays open), axis-aligned
	var rot := deg_to_rad(yaw_deg)
	var swap := absf(sin(rot)) > 0.5
	for off in [[-2.64, 0.0, 0.15, hd], [2.64, 0.0, 0.15, hd], [-1.85, hd - 0.06, 0.85, 0.15], [1.85, hd - 0.06, 0.85, 0.15], [1.25, -hd + 1.0, 0.9, 0.35]]:
		var o: Array = off
		var wx2 := float(o[0]) * cos(rot) + float(o[1]) * sin(rot)
		var wz2 := -float(o[0]) * sin(rot) + float(o[1]) * cos(rot)
		var hx := float(o[3]) if swap else float(o[2])
		var hz := float(o[2]) if swap else float(o[3])
		host._boxes.append({"pos": zone + pos + Vector3(wx2, 0, wz2), "half": Vector2(hx, hz)})
	# the keeper, behind the counter
	if clerk != "":
		var cx := 1.25 * cos(rot) + (-hd + 1.6) * sin(rot)
		var cz := -1.25 * sin(rot) + (-hd + 1.6) * cos(rot)
		var fx2 := 1.25 * cos(rot) + hd * sin(rot)
		var fz2 := -1.25 * sin(rot) + hd * cos(rot)
		_stander(q, host, pos + Vector3(cx, 0, cz), pos + Vector3(fx2, 0, fz2),
			clerk, "did:verse:npc-" + clerk.to_lower(), 1.0,
			["welcome in — everything's sovereign-made.", "have a look around!"])


## A gallery mini-store (visual floors): glowing lit interior, shelf hints,
## sign — sells "open and busy" from a distance for a dozen meshes.
func _store_lite(q: Node3D, host: Node, pos: Vector3, col: Color, sname: String) -> void:
	var inner := col.lightened(0.5)
	var back := BoxMesh.new()
	back.size = Vector3(4.6, 2.3, 0.08)
	host._mi(q, back, host._toon(inner, 0.1, false), pos + Vector3(0, 1.15, -0.3))
	var jamb := BoxMesh.new()
	jamb.size = Vector3(0.12, 2.3, 0.6)
	host._mi(q, jamb, host._toon(MALL_WHITE, 0.1, false), pos + Vector3(-2.3, 1.15, 0))
	host._mi(q, jamb, host._toon(MALL_WHITE, 0.1, false), pos + Vector3(2.3, 1.15, 0))
	var glowc := BoxMesh.new()
	glowc.size = Vector3(4.4, 0.06, 0.4)
	var gcmi: MeshInstance3D = host._mi(q, glowc, VerseAvatar.glow_mat(Color(1.0, 0.95, 0.82), 0.8), pos + Vector3(0, 2.24, -0.1))
	gcmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var shelf := BoxMesh.new()
	shelf.size = Vector3(3.8, 0.06, 0.3)
	var prod := BoxMesh.new()
	prod.size = Vector3(0.32, 0.34, 0.24)
	var pcols := [col, col.lightened(0.3), Color(0.92, 0.74, 0.34), Color(0.42, 0.74, 0.72)]
	for sy in [0.85, 1.55]:
		var syy: float = sy
		host._mi(q, shelf, host._toon(MALL_WHITE.darkened(0.06), 0.15, false), pos + Vector3(0, syy, -0.22))
		for k in 4:
			var pc: Color = pcols[(k + int(syy)) % pcols.size()]
			host._mi(q, prod, host._toon(pc, 0.25, false), pos + Vector3(-1.4 + 0.95 * float(k), syy + 0.2, -0.22))
	var fas := BoxMesh.new()
	fas.size = Vector3(4.6, 0.4, 0.1)
	host._mi(q, fas, host._toon(col, 0.25), pos + Vector3(0, 2.55, 0.02))
	var neon := BoxMesh.new()
	neon.size = Vector3(4.3, 0.04, 0.04)
	var nmi5: MeshInstance3D = host._mi(q, neon, VerseAvatar.glow_mat(col.lightened(0.3), 1.2), pos + Vector3(0, 2.32, 0.06))
	nmi5.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var lbl := Label3D.new()
	lbl.text = sname
	lbl.font_size = 40
	lbl.pixel_size = 0.0055
	lbl.modulate = Color(1.0, 0.97, 0.9)
	lbl.outline_size = 7
	lbl.position = pos + Vector3(0, 2.55, 0.1)
	q.add_child(lbl)


## A gold-potted bush — the mall's planter dressing.
func _planter_bush(q: Node3D, host: Node, zone: Vector3, pos: Vector3) -> void:
	host._obstacles.append({"pos": zone + pos, "r": 0.55})
	var pot := CylinderMesh.new()
	pot.top_radius = 0.5
	pot.bottom_radius = 0.4
	pot.height = 0.5
	pot.radial_segments = 12
	host._mi(q, pot, host._toon(MALL_GOLD, 0.3), pos + Vector3(0, 0.25, 0))
	var bush := SphereMesh.new()
	bush.radius = 0.5
	bush.height = 1.0
	bush.radial_segments = 10
	bush.rings = 5
	host._mi(q, bush, host._toon(Color(0.36, 0.62, 0.34), 0.3, true, 0.15, 0.8), pos + Vector3(0, 0.82, 0))


## A lit wall art panel: dark screen, glow border, a big bright word.
func _art_panel(q: Node3D, host: Node, pos: Vector3, yaw_deg: float, col: Color, text: String) -> void:
	var a := Node3D.new()
	a.position = pos
	a.rotation_degrees = Vector3(0, yaw_deg, 0)
	q.add_child(a)
	var panel := BoxMesh.new()
	panel.size = Vector3(4.6, 2.2, 0.1)
	var pmi4: MeshInstance3D = host._mi(a, panel, host._toon(Color(0.1, 0.13, 0.2), 0.05, false), Vector3.ZERO)
	pmi4.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bord := BoxMesh.new()
	bord.size = Vector3(4.7, 0.05, 0.05)
	for by in [-1.12, 1.12]:
		var byy: float = by
		var bmi7: MeshInstance3D = host._mi(a, bord, VerseAvatar.glow_mat(col, 1.0), Vector3(0, byy, 0.02))
		bmi7.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var lbl := Label3D.new()
	lbl.text = text
	lbl.font_size = 110
	lbl.pixel_size = 0.008
	lbl.modulate = col.lightened(0.25)
	lbl.outline_size = 12
	lbl.position = Vector3(0, 0, 0.07)
	a.add_child(lbl)


## Wainscot + gold trim + pilasters along a wall run — kills the cement look.
## yaw 0 = a wall facing +z (run along x).
func _wall_dress(q: Node3D, host: Node, pos: Vector3, yaw_deg: float, length: float, height: float) -> void:
	var w := Node3D.new()
	w.position = pos
	w.rotation_degrees = Vector3(0, yaw_deg, 0)
	q.add_child(w)
	var wains := BoxMesh.new()
	wains.size = Vector3(length, 1.1, 0.08)
	host._mi(w, wains, host._toon(Color(0.88, 0.85, 0.78), 0.12, false), Vector3(0, 0.55, 0.06))
	var trim := BoxMesh.new()
	trim.size = Vector3(length, 0.05, 0.05)
	var tmi5: MeshInstance3D = host._mi(w, trim, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.55), Vector3(0, 1.14, 0.08))
	tmi5.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var toptrim := BoxMesh.new()
	toptrim.size = Vector3(length, 0.05, 0.05)
	var ttmi2: MeshInstance3D = host._mi(w, toptrim, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.45), Vector3(0, height - 0.6, 0.08))
	ttmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var pil := BoxMesh.new()
	pil.size = Vector3(0.35, height - 0.8, 0.14)
	var n := int(length / 5.8)
	for k in n + 1:
		var px3 := -length * 0.5 + length * float(k) / float(n)
		host._mi(w, pil, host._toon(MALL_WHITE.lightened(0.02), 0.12, false), Vector3(px3, (height - 0.8) * 0.5, 0.05))


## ───────────────────── DENSE v2: the packed-town pass ──────────────────────
## The town-map feel: an elevated rail ring the trams ride, more houses packed
## into every gap, fenced side lanes off the zebra crossings, rows of trees
## ringing the town, market stalls at the park gate, street signs, bollards,
## flower patches — and more residents out living in it.
func _dense(parent: Node3D, host: Node, c: Vector3) -> void:
	# ── the elevated rail ring (the trams ride just above it) ────────────────
	var rail := TorusMesh.new()
	rail.inner_radius = 14.32
	rail.outer_radius = 14.62
	rail.rings = 48
	rail.ring_segments = 6
	var rlmi2: MeshInstance3D = host._mi(parent, rail, host._toon(Color(0.78, 0.8, 0.84), 0.15, false), c + Vector3(0, 3.15, 4))
	rlmi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var glow := TorusMesh.new()
	glow.inner_radius = 14.4
	glow.outer_radius = 14.54
	glow.rings = 48
	glow.ring_segments = 4
	var glmi: MeshInstance3D = host._mi(parent, glow, VerseAvatar.glow_mat(CYAN, 0.5), c + Vector3(0, 3.28, 4))
	glmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var pylon := CylinderMesh.new()
	pylon.top_radius = 0.18
	pylon.bottom_radius = 0.26
	pylon.height = 3.15
	pylon.radial_segments = 8
	for ang in [18.0, 64.0, 116.0, 154.0, 198.0, 244.0, 296.0, 334.0]:
		var aa := deg_to_rad(ang)
		var pp := c + Vector3(cos(aa) * 14.5, 0, 4.0 + sin(aa) * 14.5)
		host._mi(parent, pylon, host._toon(Color(0.7, 0.73, 0.78), 0.12, false), pp + Vector3(0, 1.58, 0))
		host._obstacles.append({"pos": pp, "r": 0.35})

	# ── more houses packed into the gaps ─────────────────────────────────────
	_townhouse(parent, host, c + Vector3(-20.0, 0, 13.5), 100.0, Color(0.8, 0.72, 0.6), 2)
	_townhouse(parent, host, c + Vector3(24.2, 0, 6.5), -95.0, Color(0.66, 0.76, 0.84), 3)
	_townhouse(parent, host, c + Vector3(-21.0, 0, -12.5), 90.0, Color(0.74, 0.82, 0.68), 2)
	_townhouse(parent, host, c + Vector3(21.5, 0, -13.0), -90.0, Color(0.86, 0.7, 0.64), 3)
	# two plaza-corner shops flanking the mall approach
	_shop(parent, host, c + Vector3(-16.0, 0, -21.0), 70.0, Color(0.88, 0.52, 0.72))
	_shop(parent, host, c + Vector3(16.0, 0, -21.0), -70.0, Color(0.45, 0.72, 0.45))

	# ── paved side lanes off the zebra crossings, fenced like a real town ────
	var lane := BoxMesh.new()
	lane.size = Vector3(10.0, 0.024, 2.2)
	for lz in [13.9, -5.8]:
		var lzz: float = lz
		for lx in [-8.5, 8.5]:
			var lxx: float = lx
			var lmi5: MeshInstance3D = host._mi(parent, lane, host._toon(Color(0.62, 0.6, 0.56), 0.05, false), c + Vector3(lxx, 0.018, lzz))
			lmi5.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_fence_run(parent, host, c + Vector3(-13.5, 0, 12.6), c + Vector3(-4.8, 0, 12.6))
	_fence_run(parent, host, c + Vector3(-13.5, 0, 15.2), c + Vector3(-4.8, 0, 15.2))
	_fence_run(parent, host, c + Vector3(4.8, 0, 12.6), c + Vector3(13.5, 0, 12.6))
	_fence_run(parent, host, c + Vector3(4.8, 0, -4.5), c + Vector3(13.5, 0, -4.5))
	_fence_run(parent, host, c + Vector3(-13.5, 0, -4.5), c + Vector3(-4.8, 0, -4.5))
	_fence_run(parent, host, c + Vector3(-13.5, 0, -7.1), c + Vector3(-4.8, 0, -7.1))
	_fence_run(parent, host, c + Vector3(4.8, 0, -7.1), c + Vector3(10.2, 0, -7.1))
	# the gate approach, fenced like the town entrance it is
	_fence_run(parent, host, c + Vector3(-4.7, 0, 25.5), c + Vector3(-4.7, 0, 30.5))
	_fence_run(parent, host, c + Vector3(4.7, 0, 25.5), c + Vector3(4.7, 0, 30.5))

	# ── the tree ring: woods hugging the town on every side ─────────────────
	var ring_trees := [
		Vector3(34, 0, 6), Vector3(27.5, 0, 24), Vector3(10.5, 0, 36), Vector3(-10.5, 0, 36),
		Vector3(-27.5, 0, 24), Vector3(-34, 0, 4), Vector3(-29, 0, -18), Vector3(-14.6, 0, -30),
		Vector3(17.5, 0, -33), Vector3(27.5, 0, -16),
	]
	for k in ring_trees.size():
		var tp: Vector3 = ring_trees[k]
		host._tree(parent, c + tp, 1.0 + 0.15 * float(k % 3), k % 3)
	_bush_clump(parent, host, c + Vector3(30.5, 0, 14), 1.1)
	_bush_clump(parent, host, c + Vector3(-31, 0, 12), 1.0)
	_bush_clump(parent, host, c + Vector3(-31, 0, -6), 1.1)
	_bush_clump(parent, host, c + Vector3(31, 0, -5), 1.0)

	# ── market stalls at the park gate ───────────────────────────────────────
	_cart(parent, host, c + Vector3(10.2, 0, 16.8), -30.0, Color(0.92, 0.74, 0.34))
	_cart(parent, host, c + Vector3(10.4, 0, 12.2), 20.0, Color(0.42, 0.60, 0.86))
	_stander(parent, host, c + Vector3(9.6, 0, 14.6), c + Vector3(10.2, 0, 16.8),
		"Plum", "did:verse:npc-plum", 1.0, ["fresh from the park gardens!"])

	# ── street furniture: signs, bollards, flower patches, benches ──────────
	var spole := CylinderMesh.new()
	spole.top_radius = 0.04
	spole.bottom_radius = 0.05
	spole.height = 2.3
	spole.radial_segments = 6
	var spanel := BoxMesh.new()
	spanel.size = Vector3(0.7, 0.35, 0.05)
	var scols := [Color(0.42, 0.74, 0.72), Color(0.92, 0.74, 0.34), Color(0.86, 0.42, 0.38), Color(0.42, 0.6, 0.86)]
	var sps := [Vector3(-4.4, 0, -5.0), Vector3(4.4, 0, -5.0), Vector3(-4.4, 0, 13.1), Vector3(4.4, 0, 13.1)]
	for k in 4:
		var sp2: Vector3 = sps[k]
		host._mi(parent, spole, host._toon(Color(0.55, 0.58, 0.63), 0.1, false), c + sp2 + Vector3(0, 1.15, 0))
		var pnl: MeshInstance3D = host._mi(parent, spanel, host._toon(scols[k], 0.25), c + sp2 + Vector3(0, 2.05, 0))
		pnl.rotation_degrees = Vector3(0, 35.0 + 90.0 * float(k), 0)
	var boll := CylinderMesh.new()
	boll.top_radius = 0.09
	boll.bottom_radius = 0.11
	boll.height = 0.75
	boll.radial_segments = 8
	var btip := SphereMesh.new()
	btip.radius = 0.07
	btip.height = 0.14
	btip.radial_segments = 6
	btip.rings = 3
	for ang2 in [30.0, 150.0, 210.0, 330.0]:
		var ba2 := deg_to_rad(ang2)
		var bp2 := c + Vector3(cos(ba2) * 10.0, 0, 4.0 + sin(ba2) * 10.0)
		host._mi(parent, boll, host._toon(Color(0.7, 0.72, 0.76), 0.12, false), bp2 + Vector3(0, 0.38, 0))
		var btmi: MeshInstance3D = host._mi(parent, btip, VerseAvatar.glow_mat(Color(1.0, 0.88, 0.6), 0.9), bp2 + Vector3(0, 0.8, 0))
		btmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var patch := CylinderMesh.new()
	patch.top_radius = 0.55
	patch.bottom_radius = 0.55
	patch.height = 0.05
	patch.radial_segments = 10
	var bloom := SphereMesh.new()
	bloom.radius = 0.08
	bloom.height = 0.16
	bloom.radial_segments = 6
	bloom.rings = 3
	var patches := [
		Vector3(2.2, 0, 16.8), Vector3(-2.4, 0, 18.4), Vector3(10.5, 0, 13.5),
		Vector3(-10.8, 0, 13.0), Vector3(5.0, 0, -12.0), Vector3(-5.2, 0, -12.4),
	]
	var pcols2 := [Color(0.92, 0.5, 0.62), Color(0.95, 0.72, 0.3), Color(0.62, 0.55, 0.95)]
	for k in patches.size():
		var pp2: Vector3 = patches[k]
		var pmi5: MeshInstance3D = host._mi(parent, patch, host._toon(Color(0.36, 0.5, 0.3), 0.05, false), c + pp2 + Vector3(0, 0.05, 0))
		pmi5.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		for j in 3:
			var ja := TAU * float(j) / 3.0 + float(k)
			host._mi(parent, bloom, host._toon(pcols2[(k + j) % 3], 0.3, true, 0.2, 0.9),
				c + pp2 + Vector3(cos(ja) * 0.26, 0.14, sin(ja) * 0.26))
	host._bench(parent, c + Vector3(-2.6, 0, 22.0), 160.0)
	host._bench(parent, c + Vector3(2.8, 0, -14.2), 20.0)

	# ── and more townsfolk out in it ─────────────────────────────────────────
	_npc(parent, host, c + Vector3(0, 0, 4), 13.8, -0.035, "Vito", "did:verse:npc-vito")
	_npc(parent, host, c + Vector3(-9, 0, -6), 2.6, 0.11, "Nia", "did:verse:npc-nia")
	_npc(parent, host, c + Vector3(9, 0, 12), 2.4, -0.1, "Ash", "did:verse:npc-ash")
	_npc(parent, host, c + Vector3(0, 0, 18), 2.8, 0.18, "Momo", "did:verse:npc-momo", 0.55)
	_npc(parent, host, c + Vector3(-10.5, 0, 17.5), 2.2, -0.16, "Kiki", "did:verse:npc-kiki", 0.55)
	# the big west park: ponds, bridges, a second playground full of robot kids
	_west_park(parent, host, c)


## A short wooden fence run between two points: posts + two rails.
func _fence_run(parent: Node3D, host: Node, a: Vector3, b: Vector3) -> void:
	var wood: ShaderMaterial = host._toon(Color(0.62, 0.45, 0.28), 0.15)
	var d := b - a
	var l := d.length()
	if l < 0.5:
		return
	var dir := d / l
	var post := BoxMesh.new()
	post.size = Vector3(0.12, 0.8, 0.12)
	var n := maxi(int(l / 1.4), 1)
	for k in n + 1:
		host._mi(parent, post, wood, a + dir * (l * float(k) / float(n)) + Vector3(0, 0.4, 0))
	var railm := BoxMesh.new()
	railm.size = Vector3(l, 0.07, 0.07)
	for ry in [0.32, 0.62]:
		var ryy: float = ry
		var rmi5: MeshInstance3D = host._mi(parent, railm, wood, (a + b) * 0.5 + Vector3(0, ryy, 0))
		rmi5.rotation.y = atan2(-dir.z, dir.x)


## An elevator stop (Levels 2/3): gold frame, glass shaft hint, glowing call
## pad — step onto it and the Hey floor sheet pops up.
func _lift_stop(q: Node3D, host: Node, zone: Vector3, pos: Vector3, yaw_deg: float, options: Array) -> void:
	var s := Node3D.new()
	s.position = pos
	s.rotation_degrees = Vector3(0, yaw_deg, 0)
	q.add_child(s)
	var rail := BoxMesh.new()
	rail.size = Vector3(0.1, 3.2, 0.1)
	for rx in [-0.85, 0.85]:
		var rxx: float = rx
		host._mi(s, rail, host._toon(MALL_GOLD, 0.3), Vector3(rxx, 1.6, 0))
	var top := BoxMesh.new()
	top.size = Vector3(1.9, 0.16, 0.7)
	host._mi(s, top, host._toon(MALL_GOLD, 0.3), Vector3(0, 3.25, 0))
	var backg := BoxMesh.new()
	backg.size = Vector3(1.6, 2.9, 0.08)
	var gm4: StandardMaterial3D = host._glass_mat()
	host._windows.append(gm4)
	host._mi(s, backg, gm4, Vector3(0, 1.55, -0.25))
	var padm := CylinderMesh.new()
	padm.top_radius = 0.6
	padm.bottom_radius = 0.6
	padm.height = 0.03
	padm.radial_segments = 14
	var pmi7: MeshInstance3D = host._mi(s, padm, VerseAvatar.glow_mat(MALL_TEAL, 0.7), Vector3(0, 0.025, 0.55))
	pmi7.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var lbl := Label3D.new()
	lbl.text = "Elevator"
	lbl.font_size = 44
	lbl.pixel_size = 0.005
	lbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	lbl.modulate = MALL_TEAL.lightened(0.2)
	lbl.outline_size = 8
	lbl.position = Vector3(0, 2.5, 0.4)
	s.add_child(lbl)
	var rot := deg_to_rad(yaw_deg)
	host._portals.append({"at": zone + pos + Vector3(sin(rot), 0, cos(rot)) * 0.9, "lift": options})


## A FULL store interior — its own room, entered through the storefront door
## exactly like entering the mall from the plaza. Themed to the shop:
## 0 = fashion (racks, mannequins, fitting booth) · 1 = tech (parts wall,
## workbench, holo screen) · 2 = jewelry (glass cases, chandelier, vault).
func _store_room(parent: Node3D, host: Node, zone: Vector3, col: Color, sname: String, theme: int, back_to: Vector3, back_yaw: float, clerk: String, browser: String) -> void:
	var q := Node3D.new()
	q.position = zone
	parent.add_child(q)
	var inner := col.lightened(0.55)
	# floor + walls (front z+ stays open for the camera; a box still seals it)
	var fl := BoxMesh.new()
	fl.size = Vector3(16, 0.3, 11)
	var fcol := Color(0.72, 0.6, 0.46) if theme == 0 else (Color(0.85, 0.87, 0.9) if theme == 1 else Color(0.93, 0.91, 0.87))
	host._mi(q, fl, host._toon(fcol, 0.1, false, 0.0, 0.5, 0.35), Vector3(0, -0.15, 0))
	var wback := BoxMesh.new()
	wback.size = Vector3(16, 5.2, 0.3)
	host._mi(q, wback, host._toon(inner, 0.1, false), Vector3(0, 2.6, -5.5))
	var wside := BoxMesh.new()
	wside.size = Vector3(0.3, 5.2, 11)
	host._mi(q, wside, host._toon(inner.darkened(0.06), 0.1, false), Vector3(-8.0, 2.6, 0))
	host._mi(q, wside, host._toon(inner.darkened(0.06), 0.1, false), Vector3(8.0, 2.6, 0))
	host._boxes.append({"pos": zone + Vector3(0, 0, -5.5), "half": Vector2(8.2, 0.3)})
	host._boxes.append({"pos": zone + Vector3(-8.0, 0, 0), "half": Vector2(0.3, 5.7)})
	host._boxes.append({"pos": zone + Vector3(8.0, 0, 0), "half": Vector2(0.3, 5.7)})
	host._boxes.append({"pos": zone + Vector3(0, 0, 5.5), "half": Vector2(8.2, 0.3)})
	_wall_dress(q, host, Vector3(0, 0, -5.35), 0.0, 15.6, 5.0)
	# ceiling: slab, warm light strips, hanging pendants
	var ceil2 := BoxMesh.new()
	ceil2.size = Vector3(16, 0.16, 11)
	var c2mi: MeshInstance3D = host._mi(q, ceil2, _roof_mat(host), Vector3(0, 5.28, 0))
	c2mi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var strip2 := BoxMesh.new()
	strip2.size = Vector3(11, 0.05, 0.5)
	for sz3 in [-2.6, 0.0, 2.6]:
		var szz3: float = sz3
		var smi11: MeshInstance3D = host._mi(q, strip2, VerseAvatar.glow_mat(Color(1.0, 0.95, 0.82), 0.8), Vector3(0, 5.18, szz3))
		smi11.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var cord := BoxMesh.new()
	cord.size = Vector3(0.03, 1.2, 0.03)
	var bulb := SphereMesh.new()
	bulb.radius = 0.14
	bulb.height = 0.28
	bulb.radial_segments = 8
	bulb.rings = 4
	for px4 in [-4.5, 0.0, 4.5]:
		var pxx4: float = px4
		host._mi(q, cord, host._toon(Color(0.2, 0.2, 0.22), 0.1, false), Vector3(pxx4, 4.6, 0.8))
		var blmi: MeshInstance3D = host._mi(q, bulb, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.0), Vector3(pxx4, 3.95, 0.8))
		blmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# the shop's name on the back wall
	var nlbl := Label3D.new()
	nlbl.text = sname
	nlbl.font_size = 96
	nlbl.pixel_size = 0.007
	nlbl.modulate = col.lightened(0.2)
	nlbl.outline_size = 12
	nlbl.position = Vector3(0, 4.1, -5.32)
	q.add_child(nlbl)
	# counter island + register + pay glow
	var cnt2 := BoxMesh.new()
	cnt2.size = Vector3(2.4, 0.95, 0.7)
	host._mi(q, cnt2, host._toon(MALL_WHITE, 0.15, false), Vector3(-4.6, 0.48, -3.3))
	var cnt2t := BoxMesh.new()
	cnt2t.size = Vector3(2.5, 0.05, 0.75)
	host._mi(q, cnt2t, host._toon(MALL_GOLD, 0.3), Vector3(-4.6, 0.98, -3.3))
	var reg2 := BoxMesh.new()
	reg2.size = Vector3(0.3, 0.26, 0.2)
	host._mi(q, reg2, host._toon(Color(0.18, 0.2, 0.24), 0.15, false), Vector3(-5.1, 1.14, -3.3))
	var pay := BoxMesh.new()
	pay.size = Vector3(0.26, 0.02, 0.16)
	var pymi: MeshInstance3D = host._mi(q, pay, VerseAvatar.glow_mat(MALL_TEAL, 0.8), Vector3(-4.2, 1.02, -3.3))
	pymi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	host._boxes.append({"pos": zone + Vector3(-4.6, 0, -3.3), "half": Vector2(1.25, 0.4)})
	# staff + a browsing customer
	_stander(q, host, Vector3(-4.6, 0, -4.2), Vector3(-4.6, 0, 0),
		clerk, "did:verse:npc-" + clerk.to_lower(), 1.0,
		["welcome to " + sname.capitalize() + "!", "take your time — it's all yours to try."])
	_stander(q, host, Vector3(2.5, 0, -0.5), Vector3(4.0, 0, -2.0),
		browser, "did:verse:npc-" + browser.to_lower())
	# the way back out
	_lift_pad(q, host, Vector3(0, 0, 4.6), "⬇ Mall", Color(0.93, 0.84, 0.5))
	host._portals.append({"at": zone + Vector3(0, 0, 4.6), "to": back_to, "yaw": back_yaw})
	_planter_bush(q, host, zone, Vector3(7.0, 0, 4.4))
	_planter_bush(q, host, zone, Vector3(-7.0, 0, 4.4))

	if theme == 0:
		# FASHION: hanging racks, folded-stack table, mannequins, fitting booth
		var rod := BoxMesh.new()
		rod.size = Vector3(5.6, 0.06, 0.06)
		var hang := BoxMesh.new()
		hang.size = Vector3(0.52, 0.85, 0.08)
		var hcols := [col, col.lightened(0.3), Color(0.42, 0.6, 0.86), Color(0.92, 0.74, 0.34), Color(0.45, 0.72, 0.45)]
		for side in [-1.0, 1.0]:
			var sdd: float = side
			host._mi(q, rod, host._toon(MALL_GOLD, 0.3), Vector3(sdd * 4.6, 1.9, -5.1))
			for k in 7:
				host._mi(q, hang, host._toon(hcols[(k + int(sdd)) % hcols.size()], 0.2, false),
					Vector3(sdd * 4.6 - 2.4 + 0.8 * float(k), 1.45, -5.05))
		var tab := BoxMesh.new()
		tab.size = Vector3(2.6, 0.8, 1.3)
		host._mi(q, tab, host._toon(Color(0.6, 0.46, 0.34), 0.15), Vector3(2.5, 0.4, -2.2))
		host._boxes.append({"pos": zone + Vector3(2.5, 0, -2.2), "half": Vector2(1.4, 0.75)})
		var stack := BoxMesh.new()
		stack.size = Vector3(0.55, 0.14, 0.45)
		for k in 6:
			host._mi(q, stack, host._toon(hcols[k % hcols.size()], 0.2, false),
				Vector3(1.7 + 0.8 * float(k % 3), 0.87 + 0.15 * float(k / 3), -2.2))
		var ped := CylinderMesh.new()
		ped.top_radius = 0.45
		ped.bottom_radius = 0.5
		ped.height = 0.3
		ped.radial_segments = 12
		var mq2 := BoxMesh.new()
		mq2.size = Vector3(0.4, 1.15, 0.32)
		var mqh := SphereMesh.new()
		mqh.radius = 0.14
		mqh.height = 0.28
		mqh.radial_segments = 8
		mqh.rings = 4
		for k in 3:
			var mx := -1.5 + 2.2 * float(k)
			host._obstacles.append({"pos": zone + Vector3(mx, 0, 1.8), "r": 0.6})
			host._mi(q, ped, host._toon(MALL_WHITE, 0.15), Vector3(mx, 0.15, 1.8))
			host._mi(q, mq2, host._toon(hcols[k % hcols.size()], 0.25, false), Vector3(mx, 0.9, 1.8))
			host._mi(q, mqh, host._toon(Color(0.9, 0.9, 0.92), 0.2, false), Vector3(mx, 1.65, 1.8))
		var booth := BoxMesh.new()
		booth.size = Vector3(1.6, 2.6, 0.1)
		host._mi(q, booth, host._toon(MALL_WHITE, 0.1, false), Vector3(-7.0, 1.3, -1.0))
		var curt := BoxMesh.new()
		curt.size = Vector3(1.5, 2.2, 0.06)
		host._mi(q, curt, host._toon(col.lightened(0.15), 0.2, false, 0.06, 0.7), Vector3(-7.2, 1.25, -0.2))
		host._boxes.append({"pos": zone + Vector3(-7.1, 0, -0.7), "half": Vector2(0.85, 0.5)})
		var mir := BoxMesh.new()
		mir.size = Vector3(0.9, 2.0, 0.06)
		var mmat: StandardMaterial3D = host._glass_mat()
		host._windows.append(mmat)
		host._mi(q, mir, mmat, Vector3(-7.85, 1.5, 1.6))
	elif theme == 1:
		# TECH: parts wall, workbench, gadget tables, a live holo screen
		var bin := BoxMesh.new()
		bin.size = Vector3(0.8, 0.55, 0.5)
		var bcols := [col, Color(0.45, 0.55, 0.92), Color(0.6, 0.5, 0.92), Color(0.42, 0.6, 0.86)]
		for gy in 3:
			for gx in 5:
				host._mi(q, bin, host._toon(bcols[(gx + gy) % bcols.size()].darkened(0.1), 0.2, false),
					Vector3(2.2 + 0.95 * float(gx), 0.75 + 0.72 * float(gy), -5.1))
		var bench := BoxMesh.new()
		bench.size = Vector3(3.2, 0.9, 1.2)
		host._mi(q, bench, host._toon(Color(0.45, 0.48, 0.54), 0.2), Vector3(-1.5, 0.45, -4.3))
		host._boxes.append({"pos": zone + Vector3(-1.5, 0, -4.3), "half": Vector2(1.7, 0.7)})
		var tool := BoxMesh.new()
		tool.size = Vector3(0.3, 0.12, 0.18)
		for k in 4:
			host._mi(q, tool, host._toon(Color(0.85, 0.55, 0.25).lightened(0.1 * float(k % 2)), 0.25, false),
				Vector3(-2.6 + 0.75 * float(k), 0.97, -4.3))
		var sparkm: MeshInstance3D = host._mi(q, tool, VerseAvatar.glow_mat(MALL_TEAL, 1.4), Vector3(-1.2, 0.99, -4.0))
		sparkm.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var gtab := BoxMesh.new()
		gtab.size = Vector3(2.2, 0.85, 1.1)
		var gad := BoxMesh.new()
		gad.size = Vector3(0.45, 0.3, 0.35)
		for k in 2:
			var gx2 := -0.5 + 4.0 * float(k)
			host._mi(q, gtab, host._toon(MALL_WHITE, 0.15), Vector3(gx2, 0.42, 0.8))
			host._boxes.append({"pos": zone + Vector3(gx2, 0, 0.8), "half": Vector2(1.2, 0.65)})
			for j in 3:
				host._mi(q, gad, host._toon(bcols[(j + k) % bcols.size()], 0.25, false),
					Vector3(gx2 - 0.7 + 0.7 * float(j), 0.99, 0.8))
				var gl2: MeshInstance3D = host._mi(q, pay, VerseAvatar.glow_mat(bcols[(j + k) % bcols.size()].lightened(0.3), 0.9),
					Vector3(gx2 - 0.7 + 0.7 * float(j), 1.16, 0.8))
				gl2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		_art_panel(q, host, Vector3(-5.5, 2.9, -5.2), 0.0, MALL_TEAL, "« CODE »")
	else:
		# JEWELRY: glass cases with glowing pieces, chandelier, the vault door
		var base := BoxMesh.new()
		base.size = Vector3(1.3, 0.95, 1.3)
		var caseg := BoxMesh.new()
		caseg.size = Vector3(1.1, 0.7, 1.1)
		var gem := SphereMesh.new()
		gem.radius = 0.09
		gem.height = 0.18
		gem.radial_segments = 8
		gem.rings = 4
		var gcols := [MALL_GOLD_GLOW, Color(0.7, 0.9, 1.0), Color(0.95, 0.6, 0.7), MALL_GOLD_GLOW]
		for k in 4:
			var cx3 := -4.5 + 3.0 * float(k)
			host._obstacles.append({"pos": zone + Vector3(cx3, 0, -0.6), "r": 0.95})
			host._mi(q, base, host._toon(MALL_WHITE, 0.15), Vector3(cx3, 0.48, -0.6))
			var cmat3: StandardMaterial3D = host._glass_mat()
			host._windows.append(cmat3)
			host._mi(q, caseg, cmat3, Vector3(cx3, 1.3, -0.6))
			for j in 2:
				var gmi3: MeshInstance3D = host._mi(q, gem, VerseAvatar.glow_mat(gcols[(k + j) % gcols.size()], 1.3),
					Vector3(cx3 - 0.2 + 0.4 * float(j), 1.12, -0.6))
				gmi3.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var vault := CylinderMesh.new()
		vault.top_radius = 1.5
		vault.bottom_radius = 1.5
		vault.height = 0.35
		vault.radial_segments = 18
		var vmi: MeshInstance3D = host._mi(q, vault, host._toon(Color(0.62, 0.64, 0.7), 0.35, true, 0.0, 0.5, 0.6), Vector3(4.8, 1.8, -5.25))
		vmi.rotation_degrees = Vector3(90, 0, 0)
		var spoke := BoxMesh.new()
		spoke.size = Vector3(1.6, 0.12, 0.12)
		for k in 3:
			var skmi2: MeshInstance3D = host._mi(q, spoke, host._toon(MALL_GOLD, 0.35), Vector3(4.8, 1.8, -5.0))
			skmi2.rotation_degrees = Vector3(0, 0, 60.0 * float(k))
		var ring3 := TorusMesh.new()
		ring3.inner_radius = 1.42
		ring3.outer_radius = 1.55
		ring3.rings = 24
		ring3.ring_segments = 6
		var vrmi: MeshInstance3D = host._mi(q, ring3, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.8), Vector3(4.8, 1.8, -5.18))
		vrmi.rotation_degrees = Vector3(90, 0, 0)
		vrmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var runner := BoxMesh.new()
		runner.size = Vector3(1.8, 0.025, 9.5)
		var rnmi: MeshInstance3D = host._mi(q, runner, host._toon(Color(0.55, 0.16, 0.2), 0.1, false), Vector3(0, 0.03, 0))
		rnmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		for k in 9:
			var ca3 := TAU * float(k) / 9.0
			var comi2: MeshInstance3D = host._mi(q, gem, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.1),
				Vector3(cos(ca3) * (0.4 + 0.3 * float(k % 3)), 4.4 - 0.3 * float(k % 3), 0.5 + sin(ca3) * (0.4 + 0.3 * float(k % 3))))
			comi2.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## ──────────────── the WEST PARK: water, bridges, robot kids ────────────────
## The big green SOUTH-WEST corner the city was missing: lawns, THREE ponds with little
## plank bridges over the water, and a second playground — merry-go-round,
## swings, slide, spring riders — busy with robot kids.
func _west_park(parent: Node3D, host: Node, c: Vector3) -> void:
	var p := c + Vector3(-15.5, 0, 24.5)
	# lawn
	var lawn := BoxMesh.new()
	lawn.size = Vector3(9.0, 0.06, 10.5)
	var lmi6: MeshInstance3D = host._mi(parent, lawn, host._toon(Color(0.42, 0.67, 0.37), 0.05, false), p + Vector3(0, 0.035, 0))
	lmi6.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# fence along the north edge, gap for the entrance
	_fence_run(parent, host, p + Vector3(-4.5, 0, -5.25), p + Vector3(-0.7, 0, -5.25))
	_fence_run(parent, host, p + Vector3(1.3, 0, -5.25), p + Vector3(4.5, 0, -5.25))
	# park sign at the gap
	var sgn := BoxMesh.new()
	sgn.size = Vector3(1.6, 0.5, 0.08)
	host._mi(parent, sgn, host._toon(Color(0.62, 0.45, 0.28), 0.15), p + Vector3(1.9, 1.15, -5.25))
	var spost2 := BoxMesh.new()
	spost2.size = Vector3(0.1, 1.0, 0.1)
	host._mi(parent, spost2, host._toon(Color(0.5, 0.36, 0.22), 0.15), p + Vector3(1.9, 0.5, -5.25))
	var slbl := Label3D.new()
	slbl.text = "ROBO PARK"
	slbl.font_size = 40
	slbl.pixel_size = 0.005
	slbl.modulate = Color(1.0, 0.95, 0.8)
	slbl.outline_size = 8
	slbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	slbl.position = p + Vector3(1.9, 1.6, -5.25)
	parent.add_child(slbl)
	# ── THREE ponds (live water) + plank bridges across two of them ─────────
	_pond(parent, host, p + Vector3(-2.5, 0, -2.75), 2.0)
	_pond(parent, host, p + Vector3(2.5, 0, 2.25), 1.6)
	_pond(parent, host, p + Vector3(-3.6, 0, 4.35), 1.1)
	# bridge over the big pond (east-west): water solids leave the deck free
	host._obstacles.append({"pos": p + Vector3(-2.5, 0, -3.95), "r": 1.15})
	host._obstacles.append({"pos": p + Vector3(-2.5, 0, -1.55), "r": 1.15})
	_bridge(parent, host, p + Vector3(-2.5, 0, -2.75), 0.0, 4.6)
	# bridge over the middle pond (north-south)
	host._obstacles.append({"pos": p + Vector3(1.25, 0, 2.25), "r": 0.85})
	host._obstacles.append({"pos": p + Vector3(3.75, 0, 2.25), "r": 0.85})
	_bridge(parent, host, p + Vector3(2.5, 0, 2.25), 90.0, 3.8)
	# the small pond is for the ducks — full solid
	host._obstacles.append({"pos": p + Vector3(-3.6, 0, 4.35), "r": 1.3})
	# ── playground #2 ────────────────────────────────────────────────────────
	# merry-go-round: a spinning platform with robot kids riding it
	var mgr: Node3D = _spinner(parent, p + Vector3(3.0, 0, -3.25), 0.8, 0.0)
	host._obstacles.append({"pos": p + Vector3(3.0, 0, -3.25), "r": 1.6})
	var plat2 := CylinderMesh.new()
	plat2.top_radius = 1.3
	plat2.bottom_radius = 1.35
	plat2.height = 0.16
	plat2.radial_segments = 16
	host._mi(mgr, plat2, host._toon(Color(0.86, 0.34, 0.3), 0.25), Vector3(0, 0.1, 0))
	var hub := CylinderMesh.new()
	hub.top_radius = 0.09
	hub.bottom_radius = 0.09
	hub.height = 0.9
	hub.radial_segments = 8
	host._mi(mgr, hub, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), Vector3(0, 0.6, 0))
	var bar2 := BoxMesh.new()
	bar2.size = Vector3(0.06, 0.06, 2.4)
	for k in 2:
		var bmi8: MeshInstance3D = host._mi(mgr, bar2, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), Vector3(0, 1.0, 0))
		bmi8.rotation_degrees = Vector3(0, 90.0 * float(k), 0)
	for k in 2:
		var ka := PI * float(k)
		var kid: VerseAvatar = VerseAvatar.new()
		kid.display_name = ["Twix", "Pop"][k]
		kid.base_color = Net.did_color("did:verse:npc-" + ["twix", "pop"][k])
		kid.outfit = VerseAvatar.resolve_outfit("did:verse:npc-" + ["twix", "pop"][k], {})
		kid.position = Vector3(cos(ka) * 0.85, 0.18, sin(ka) * 0.85)
		kid.rotation.y = ka + PI * 0.5
		kid.scale = Vector3.ONE * 0.5
		mgr.add_child(kid)
	# swings: one robot kid mid-air
	var sw2 := Node3D.new()
	sw2.position = p + Vector3(-0.4, 0, -4.6)
	sw2.rotation_degrees = Vector3(0, 90, 0)
	parent.add_child(sw2)
	host._obstacles.append({"pos": sw2.position + Vector3(0, 0, -1.1), "r": 0.3})
	host._obstacles.append({"pos": sw2.position + Vector3(0, 0, 1.1), "r": 0.3})
	var wood2: ShaderMaterial = host._toon(Color(0.62, 0.45, 0.28), 0.15)
	var spost3 := BoxMesh.new()
	spost3.size = Vector3(0.1, 1.7, 0.1)
	host._mi(sw2, spost3, wood2, Vector3(-1.1, 0.85, 0))
	host._mi(sw2, spost3, wood2, Vector3(1.1, 0.85, 0))
	var sbar2 := BoxMesh.new()
	sbar2.size = Vector3(2.4, 0.09, 0.09)
	host._mi(sw2, sbar2, wood2, Vector3(0, 1.72, 0))
	var chain2 := BoxMesh.new()
	chain2.size = Vector3(0.03, 0.85, 0.03)
	var seat2 := BoxMesh.new()
	seat2.size = Vector3(0.4, 0.05, 0.2)
	for sxo in [-0.55, 0.55]:
		var sxx3: float = sxo
		var swing2: Node3D = _spinner(sw2, Vector3(sxx3, 1.68, 0), 0.0, 0.09)
		host._mi(swing2, chain2, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), Vector3(-0.15, -0.43, 0))
		host._mi(swing2, chain2, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), Vector3(0.15, -0.43, 0))
		host._mi(swing2, seat2, host._toon(Color(0.35, 0.55, 0.85), 0.25), Vector3(0, -0.86, 0))
		if sxx3 > 0.0:
			var skid: VerseAvatar = VerseAvatar.new()
			skid.display_name = "Mo"
			skid.base_color = Net.did_color("did:verse:npc-mo")
			skid.outfit = VerseAvatar.resolve_outfit("did:verse:npc-mo", {})
			skid.position = Vector3(0, -0.84, 0)
			skid.scale = Vector3.ONE * 0.5
			swing2.add_child(skid)
	# slide
	var sl2 := Node3D.new()
	sl2.position = p + Vector3(0.8, 0, -0.4)
	sl2.rotation_degrees = Vector3(0, -35, 0)
	parent.add_child(sl2)
	host._obstacles.append({"pos": sl2.position, "r": 1.3})
	var plat3 := BoxMesh.new()
	plat3.size = Vector3(0.8, 0.1, 0.8)
	host._mi(sl2, plat3, host._toon(Color(0.95, 0.77, 0.32), 0.2), Vector3(0, 1.1, 0))
	var ramp3 := BoxMesh.new()
	ramp3.size = Vector3(0.62, 0.08, 2.0)
	var rampmi2: MeshInstance3D = host._mi(sl2, ramp3, host._toon(Color(0.42, 0.74, 0.72), 0.25), Vector3(0, 0.62, 1.32))
	rampmi2.rotation_degrees = Vector3(-29, 0, 0)
	var lpost2 := BoxMesh.new()
	lpost2.size = Vector3(0.07, 1.1, 0.07)
	for cx in [-0.34, 0.34]:
		var cxx3: float = cx
		host._mi(sl2, lpost2, wood2, Vector3(cxx3, 0.55, -0.34))
		host._mi(sl2, lpost2, wood2, Vector3(cxx3, 0.55, 0.34))
	var rung2 := BoxMesh.new()
	rung2.size = Vector3(0.6, 0.05, 0.05)
	for k2 in 4:
		host._mi(sl2, rung2, wood2, Vector3(0, 0.25 + float(k2) * 0.26, -0.36))
	# spring riders: bouncy robo-animals
	for k in 2:
		var rp2 := p + Vector3(-0.6 + 1.4 * float(k), 0, 1.0)
		host._obstacles.append({"pos": rp2, "r": 0.35})
		var spring := CylinderMesh.new()
		spring.top_radius = 0.07
		spring.bottom_radius = 0.1
		spring.height = 0.35
		spring.radial_segments = 8
		host._mi(parent, spring, host._toon(Color(0.6, 0.62, 0.66), 0.1, false), rp2 + Vector3(0, 0.18, 0))
		var rider: Node3D = _spinner(parent, rp2 + Vector3(0, 0.5, 0), 0.0, 0.08)
		var animal := BoxMesh.new()
		animal.size = Vector3(0.5, 0.3, 0.7)
		host._mi(rider, animal, host._toon([Color(0.92, 0.74, 0.34), Color(0.42, 0.6, 0.86)][k], 0.3), Vector3.ZERO)
		var ahead := SphereMesh.new()
		ahead.radius = 0.14
		ahead.height = 0.28
		ahead.radial_segments = 8
		ahead.rings = 4
		host._mi(rider, ahead, host._toon([Color(0.92, 0.74, 0.34), Color(0.42, 0.6, 0.86)][k].lightened(0.2), 0.3),
			Vector3(0, 0.18, 0.38))
	# kids running loose + a watchful parent
	_npc(parent, host, p + Vector3(0.5, 0, -2.0), 2.2, 0.22, "Zuzu", "did:verse:npc-zuzu", 0.55)
	_npc(parent, host, p + Vector3(-1.5, 0, 3.2), 1.8, -0.2, "Bibi", "did:verse:npc-bibi", 0.55)
	_stander(parent, host, p + Vector3(1.6, 0, -1.8), p + Vector3(3.0, 0, -3.25),
		"Mama Bolt", "did:verse:npc-mamabolt", 1.0, ["careful on the spinny one, Twix!"])
	# dressing: trees, bushes, flowers, benches, lamps
	host._tree(parent, p + Vector3(-4.0, 0, -4.6), 0.9, 1)
	host._tree(parent, p + Vector3(4.2, 0, 4.6), 0.85, 2)
	_bush_clump(parent, host, p + Vector3(-4.3, 0, 0.6), 0.9)
	_bush_clump(parent, host, p + Vector3(4.4, 0, -1.4), 0.85)
	_flower_bed(parent, host, p + Vector3(0.2, 0, 4.3), Color(0.92, 0.5, 0.62))
	host._bench(parent, p + Vector3(-0.6, 0, -6.2), 4.0)
	host._bench(parent, p + Vector3(4.3, 0, 0.6), -90.0)
	host._lamp(parent, p + Vector3(-4.4, 0, 2.6))
	host._lamp(parent, p + Vector3(4.4, 0, -4.4))


## A see-through roof material: reads as a ceiling from below, but the camera
## (and you) always see the avatar through it from above.
func _roof_mat(host: Node) -> StandardMaterial3D:
	var m := StandardMaterial3D.new()
	m.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	m.albedo_color = Color(0.82, 0.86, 0.92, 0.16)
	m.cull_mode = BaseMaterial3D.CULL_DISABLED
	m.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	if host:
		pass
	return m


## A pond with LIVE water (animated shader) and a stone rim.
func _pond(parent: Node3D, host: Node, pos: Vector3, r: float) -> void:
	var pond := CylinderMesh.new()
	pond.top_radius = r
	pond.bottom_radius = r
	pond.height = 0.06
	pond.radial_segments = 22
	var pwm := ShaderMaterial.new()
	pwm.shader = host.WATER_SHADER
	var pdmi: MeshInstance3D = host._mi(parent, pond, pwm, pos + Vector3(0, 0.05, 0))
	pdmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var rim := TorusMesh.new()
	rim.inner_radius = r - 0.05
	rim.outer_radius = r + 0.2
	rim.rings = 24
	rim.ring_segments = 6
	host._mi(parent, rim, host._toon(Color(0.72, 0.7, 0.66), 0.1), pos + Vector3(0, 0.08, 0))


## A little plank bridge over the water: arched deck, side rails — the rails
## are solid, so crossings funnel over the planks.
func _bridge(parent: Node3D, host: Node, mid: Vector3, yaw_deg: float, span: float) -> void:
	var b := Node3D.new()
	b.position = mid
	b.rotation_degrees = Vector3(0, yaw_deg, 0)
	parent.add_child(b)
	var wood: ShaderMaterial = host._toon(Color(0.66, 0.48, 0.3), 0.15)
	var plank := BoxMesh.new()
	plank.size = Vector3(span / 5.0 - 0.04, 0.05, 1.15)
	for k in 5:
		var t := float(k) / 4.0 - 0.5
		var arc := (0.25 - t * t) * 0.55
		var pmi8: MeshInstance3D = host._mi(b, plank, wood, Vector3(t * span, 0.05 + arc, 0))
		pmi8.rotation_degrees = Vector3(0, 0, -t * 16.0)
	var railp := BoxMesh.new()
	railp.size = Vector3(span, 0.06, 0.06)
	var rpost := BoxMesh.new()
	rpost.size = Vector3(0.07, 0.45, 0.07)
	for sz in [-0.62, 0.62]:
		var szz: float = sz
		host._mi(b, railp, wood, Vector3(0, 0.55, szz))
		for k in 3:
			host._mi(b, rpost, wood, Vector3(-span * 0.4 + span * 0.4 * float(k), 0.3, szz))
	# rail solids: cross between them, not through them (axis-aware)
	var rot := deg_to_rad(yaw_deg)
	var swap := absf(sin(rot)) > 0.5
	for szb in [-0.62, 0.62]:
		var szz2: float = szb
		var wx2 := szz2 * sin(rot)
		var wz2 := szz2 * cos(rot)
		var hx := 0.1 if swap else span * 0.5
		var hz := span * 0.5 if swap else 0.1
		host._boxes.append({"pos": mid + Vector3(wx2, 0, wz2), "half": Vector2(hx, hz)})


## HEY CINEMA — the mall's dark plush movie theater (dollhouse: the front
## z+5.5 wall is never drawn, the camera looks in; its collision box still
## seals the room). A big glow screen color-cycles like a movie playing,
## three rows of red seats face it down a step-lit aisle, Butters runs the
## popcorn counter in the west corner, marquee bulbs string across the open
## front, and tiny stars glow on the dark ceiling. The east front lane
## (z > 3.4) stays clear for the elevator arrival at +(6.3..8.4, 0, 3.8).
func _wf_cinema(parent: Node3D, host: Node) -> void:
	var zo: Vector3 = host.MALL_IN + Vector3(240, 0, 0)
	var q := Node3D.new()
	q.position = zo
	parent.add_child(q)
	# ── shell: floor, back + side walls, dark ceiling (front stays open) ────
	var fl := BoxMesh.new()
	fl.size = Vector3(19, 0.3, 11)
	host._mi(q, fl, host._toon(Color(0.14, 0.11, 0.14), 0.08, false), Vector3(0, -0.15, 0))
	var wall_mat: Material = host._toon(Color(0.12, 0.12, 0.17), 0.08, false)
	var wback := BoxMesh.new()
	wback.size = Vector3(19, 6.5, 0.3)
	host._mi(q, wback, wall_mat, Vector3(0, 3.25, -5.5))
	var wside := BoxMesh.new()
	wside.size = Vector3(0.3, 6.5, 11)
	host._mi(q, wside, wall_mat, Vector3(-9.5, 3.25, 0))
	host._mi(q, wside, wall_mat, Vector3(9.5, 3.25, 0))
	var slab := BoxMesh.new()
	slab.size = Vector3(19, 0.25, 11)
	var slmi: MeshInstance3D = host._mi(q, slab, _roof_mat(host), Vector3(0, 6.4, 0))
	slmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# collision seals ALL four walls — the undrawn front one included
	host._boxes.append({"pos": zo + Vector3(0, 0, -5.5), "half": Vector2(9.7, 0.3)})
	host._boxes.append({"pos": zo + Vector3(0, 0, 5.5), "half": Vector2(9.7, 0.3)})
	host._boxes.append({"pos": zo + Vector3(-9.5, 0, 0), "half": Vector2(0.3, 5.7)})
	host._boxes.append({"pos": zo + Vector3(9.5, 0, 0), "half": Vector2(0.3, 5.7)})
	# tiny glow stars under the dark ceiling
	var star := SphereMesh.new()
	star.radius = 0.05
	star.height = 0.1
	star.radial_segments = 6
	star.rings = 3
	var star_mat: StandardMaterial3D = VerseAvatar.glow_mat(Color(0.82, 0.86, 1.0), 1.1)
	for sp in [Vector3(-6.5, 6.22, -3.2), Vector3(4.6, 6.22, -4.1), Vector3(7.4, 6.22, 1.6), Vector3(-3.4, 6.22, 2.7), Vector3(1.2, 6.22, -1.6), Vector3(-7.6, 6.22, 3.9), Vector3(3.2, 6.22, 3.4), Vector3(-1.8, 6.22, -4.4)]:
		var spp: Vector3 = sp
		var stmi: MeshInstance3D = host._mi(q, star, star_mat, spp)
		stmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# faint gold cornice along both side walls — keeps the luxury thread
	var strim := BoxMesh.new()
	strim.size = Vector3(0.06, 0.06, 10.8)
	var strim_mat: StandardMaterial3D = VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.4)
	for tx in [-9.31, 9.31]:
		var txx: float = tx
		var tmi: MeshInstance3D = host._mi(q, strim, strim_mat, Vector3(txx, 5.7, 0))
		tmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# ── THE SCREEN: glow panel whose emission color-cycles (a movie on) ─────
	var scr_mat: StandardMaterial3D = VerseAvatar.glow_mat(Color(0.92, 0.5, 0.28), 1.5)
	scr_mat.albedo_color = Color(0.07, 0.07, 0.09)
	var scr := BoxMesh.new()
	scr.size = Vector3(11, 4.5, 0.08)
	var smi: MeshInstance3D = host._mi(q, scr, scr_mat, Vector3(0, 3.0, -5.27))
	smi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var movie := smi.create_tween()
	movie.set_loops()
	for mc in [Color(0.25, 0.55, 0.95), Color(0.95, 0.42, 0.6), Color(0.95, 0.78, 0.32), Color(0.3, 0.85, 0.72), Color(0.92, 0.5, 0.28)]:
		var mcc: Color = mc
		movie.tween_property(scr_mat, "emission", mcc, 2.0).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	var frame_mat: Material = host._toon(Color(0.05, 0.05, 0.07), 0.05, false)
	var fh := BoxMesh.new()
	fh.size = Vector3(11.5, 0.2, 0.14)
	host._mi(q, fh, frame_mat, Vector3(0, 5.35, -5.28))
	host._mi(q, fh, frame_mat, Vector3(0, 0.65, -5.28))
	var fv := BoxMesh.new()
	fv.size = Vector3(0.2, 4.9, 0.14)
	host._mi(q, fv, frame_mat, Vector3(-5.65, 3.0, -5.28))
	host._mi(q, fv, frame_mat, Vector3(5.65, 3.0, -5.28))
	# ── three rows of plush red seats facing the screen ─────────────────────
	var seat_mat: Material = host._toon(Color(0.46, 0.12, 0.16), 0.2)
	var seat := BoxMesh.new()
	seat.size = Vector3(0.7, 0.45, 0.62)
	var sback := BoxMesh.new()
	sback.size = Vector3(0.7, 0.78, 0.16)
	for rz in [-1.8, 0.0, 1.8]:
		var rzz: float = rz
		for sx in [-3.2, -2.4, -1.6, 1.6, 2.4, 3.2]:
			var sxx: float = sx
			host._mi(q, seat, seat_mat, Vector3(sxx, 0.225, rzz))
			var bkmi: MeshInstance3D = host._mi(q, sback, seat_mat, Vector3(sxx, 0.79, rzz + 0.33))
			bkmi.rotation_degrees = Vector3(7, 0, 0)
		host._boxes.append({"pos": zo + Vector3(0, 0, rzz), "half": Vector2(3.7, 0.55)})
	# ── center aisle: dark red carpet + pulsing glow step dots ──────────────
	var rug := BoxMesh.new()
	rug.size = Vector3(2.0, 0.03, 8.2)
	var rgmi: MeshInstance3D = host._mi(q, rug, host._toon(Color(0.34, 0.09, 0.11), 0.08, false), Vector3(0, 0.015, -0.4))
	rgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dot := CylinderMesh.new()
	dot.top_radius = 0.07
	dot.bottom_radius = 0.07
	dot.height = 0.04
	dot.radial_segments = 8
	var dot_mat: StandardMaterial3D = VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.9)
	var first_dot: MeshInstance3D = null
	for dz in [-3.9, -2.5, -1.1, 0.3, 1.7, 3.1]:
		var dzz: float = dz
		for dx in [-1.05, 1.05]:
			var dxx: float = dx
			var dmi: MeshInstance3D = host._mi(q, dot, dot_mat, Vector3(dxx, 0.05, dzz))
			dmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			if first_dot == null:
				first_dot = dmi
	var pulse := first_dot.create_tween()
	pulse.set_loops()
	pulse.tween_property(dot_mat, "emission_energy_multiplier", 1.6, 1.4).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	pulse.tween_property(dot_mat, "emission_energy_multiplier", 0.5, 1.4).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	# ── snacks corner (west): counter, popcorn machine, Butters ─────────────
	var cbody := BoxMesh.new()
	cbody.size = Vector3(1.0, 1.1, 2.7)
	host._mi(q, cbody, host._toon(Color(0.33, 0.1, 0.13), 0.15), Vector3(-6.9, 0.55, 2.0))
	var ctop := BoxMesh.new()
	ctop.size = Vector3(1.16, 0.07, 2.86)
	host._mi(q, ctop, host._toon(MALL_GOLD, 0.3), Vector3(-6.9, 1.14, 2.0))
	var band := BoxMesh.new()
	band.size = Vector3(0.05, 0.12, 2.6)
	var bdmi: MeshInstance3D = host._mi(q, band, VerseAvatar.glow_mat(Color(1.0, 0.85, 0.55), 0.9), Vector3(-6.36, 0.92, 2.0))
	bdmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	host._boxes.append({"pos": zo + Vector3(-6.9, 0, 2.0), "half": Vector2(0.58, 1.43)})
	var pbox := BoxMesh.new()
	pbox.size = Vector3(0.16, 0.22, 0.16)
	host._mi(q, pbox, host._toon(Color(0.85, 0.2, 0.22), 0.25), Vector3(-6.75, 1.28, 1.3))
	host._mi(q, pbox, host._toon(MALL_CREAM, 0.2), Vector3(-6.95, 1.28, 2.6))
	var slbl := Label3D.new()
	slbl.text = "SNACKS"
	slbl.font_size = 52
	slbl.pixel_size = 0.006
	slbl.modulate = Color(1.0, 0.88, 0.6)
	slbl.outline_size = 9
	slbl.position = Vector3(-7.6, 2.3, 2.0)
	slbl.rotation_degrees = Vector3(0, 90, 0)
	q.add_child(slbl)
	# popcorn machine: red base, glass box, 6 yellow kernels, soft pops
	var mred: Material = host._toon(Color(0.55, 0.14, 0.16), 0.2)
	var mbase := BoxMesh.new()
	mbase.size = Vector3(0.8, 0.55, 0.8)
	host._mi(q, mbase, mred, Vector3(-8.5, 0.275, 0.6))
	var mglass := BoxMesh.new()
	mglass.size = Vector3(0.72, 0.75, 0.72)
	var gmat: Material = host._glass_mat()
	host._windows.append(gmat)
	host._mi(q, mglass, gmat, Vector3(-8.5, 0.95, 0.6))
	var mcap := BoxMesh.new()
	mcap.size = Vector3(0.86, 0.14, 0.86)
	host._mi(q, mcap, mred, Vector3(-8.5, 1.4, 0.6))
	var kern := SphereMesh.new()
	kern.radius = 0.07
	kern.height = 0.14
	kern.radial_segments = 6
	kern.rings = 3
	var kmat: StandardMaterial3D = VerseAvatar.glow_mat(Color(1.0, 0.92, 0.62), 0.5)
	kern.material = kmat
	for ko in [Vector3(-0.18, 0.7, -0.1), Vector3(0.12, 0.68, 0.15), Vector3(0.02, 0.72, -0.18), Vector3(-0.1, 0.66, 0.2), Vector3(0.2, 0.7, -0.04), Vector3(-0.02, 0.78, 0.04)]:
		var koo: Vector3 = ko
		var kmi: MeshInstance3D = host._mi(q, kern, kmat, Vector3(-8.5, 0, 0.6) + koo)
		kmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	host._obstacles.append({"pos": zo + Vector3(-8.5, 0, 0.6), "r": 0.55})
	var pop := CPUParticles3D.new()
	pop.amount = 10
	pop.lifetime = 1.0
	pop.mesh = kern
	pop.position = Vector3(-8.5, 0.75, 0.6)
	pop.direction = Vector3(0, 1, 0)
	pop.spread = 25.0
	pop.initial_velocity_min = 0.5
	pop.initial_velocity_max = 0.9
	pop.gravity = Vector3(0, -1.8, 0)
	pop.scale_amount_min = 0.5
	pop.scale_amount_max = 0.9
	pop.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	pop.emission_box_extents = Vector3(0.18, 0.04, 0.18)
	pop.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	q.add_child(pop)
	_stander(q, host, Vector3(-8.2, 0, 2.2), Vector3(0, 0, 2.0), "Butters", "did:verse:npc-butters", 1.0,
		["extra butter? always extra butter.", "shh — eat QUIETLY, the robots are kissing."])
	# ── movie posters on the side walls ─────────────────────────────────────
	_art_panel(q, host, Vector3(-9.28, 2.6, -1.5), 90.0, CYAN, "VERSE WARS")
	_art_panel(q, host, Vector3(9.28, 2.6, -1.5), -90.0, Color(0.95, 0.5, 0.7), "ROBO LOVE")
	# ── velvet ropes guiding the entry (west of the open front) ─────────────
	var post := CylinderMesh.new()
	post.top_radius = 0.06
	post.bottom_radius = 0.06
	post.height = 1.0
	post.radial_segments = 8
	var ball := SphereMesh.new()
	ball.radius = 0.1
	ball.height = 0.2
	ball.radial_segments = 8
	ball.rings = 4
	var gold_mat: Material = host._toon(MALL_GOLD, 0.3, false, 0.0, 0.5, 0.5)
	for px in [-4.5, -2.5]:
		var pxx: float = px
		host._mi(q, post, gold_mat, Vector3(pxx, 0.5, 4.3))
		host._mi(q, ball, gold_mat, Vector3(pxx, 1.04, 4.3))
		host._obstacles.append({"pos": zo + Vector3(pxx, 0, 4.3), "r": 0.2})
	var rope := BoxMesh.new()
	rope.size = Vector3(1.9, 0.05, 0.05)
	host._mi(q, rope, host._toon(Color(0.55, 0.12, 0.18), 0.2, false), Vector3(-3.5, 0.86, 4.3))
	host._boxes.append({"pos": zo + Vector3(-3.5, 0, 4.3), "half": Vector2(1.15, 0.12)})
	# ── marquee: bulb string across the open front + gold fascia ────────────
	var wire := BoxMesh.new()
	wire.size = Vector3(18.6, 0.02, 0.02)
	var wrmi: MeshInstance3D = host._mi(q, wire, host._toon(Color(0.2, 0.2, 0.24), 0.05, false), Vector3(0, 3.3, 5.05))
	wrmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var bulb := SphereMesh.new()
	bulb.radius = 0.07
	bulb.height = 0.14
	bulb.radial_segments = 8
	bulb.rings = 4
	var bulb_warm: StandardMaterial3D = VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.1)
	var bulb_cool: StandardMaterial3D = VerseAvatar.glow_mat(MALL_TEAL, 1.0)
	for bi in 9:
		var bx := -8.0 + 2.0 * float(bi)
		var bmat: StandardMaterial3D = bulb_warm if bi % 2 == 0 else bulb_cool
		var blmi: MeshInstance3D = host._mi(q, bulb, bmat, Vector3(bx, 3.22, 5.05))
		blmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var fas := BoxMesh.new()
	fas.size = Vector3(19, 0.55, 0.22)
	host._mi(q, fas, host._toon(Color(0.09, 0.09, 0.12), 0.08, false), Vector3(0, 6.25, 5.38))
	var fglow := BoxMesh.new()
	fglow.size = Vector3(18.8, 0.06, 0.06)
	var fgmi: MeshInstance3D = host._mi(q, fglow, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.8), Vector3(0, 5.95, 5.45))
	fgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var mlbl := Label3D.new()
	mlbl.text = "HEY CINEMA"
	mlbl.font_size = 96
	mlbl.pixel_size = 0.007
	mlbl.modulate = MALL_GOLD_GLOW
	mlbl.outline_size = 12
	mlbl.position = Vector3(0, 6.24, 5.52)
	q.add_child(mlbl)
	# ── life: two watchers in the back aisle + a lobby walker ───────────────
	_stander(q, host, Vector3(-2.2, 0, 2.75), Vector3(-2.2, 0, -5.5), "Juno", "did:verse:npc-juno", 1.0,
		["shh... this is the good bit."])
	_stander(q, host, Vector3(2.3, 0, 2.8), Vector3(2.3, 0, -5.5), "Nia", "did:verse:npc-nia", 0.55,
		["the best part is coming!"])
	_npc(q, host, Vector3(0, 0, 3.6), 1.6, 0.15, "Reel", "did:verse:npc-reel", 1.0, 1.3)


## WOMEN'S FLOOR (Level 2) — life + glam decor layer only (floor/walls/stores exist).
## Adds: a white/rose-gold RUNWAY with glowing edge strips + 2 strutting model bots,
## a GLAM BAR (counter + tall mirror + stylist Coco), a misting perfume vitrine (west),
## two jewelry vitrines with spinning gems (east promenade), a gold flower arch
## (south center, EAST of the rug — the suggested west band collides with the kiosk)
## and one gossip kid. 4 avatars total. Every case/counter/runway/post registers a
## solid in GLOBAL coords (zone origin + local). All solids recomputed clear of the
## reserved spots and of the NPC orbit annulus r 6.1..6.9.
func _wf_her(parent: Node3D, host: Node) -> void:
	var zo: Vector3 = host.MALL_IN + Vector3(40, 0, 0)
	var q := Node3D.new()
	q.position = zo
	parent.add_child(q)
	var rose := Color(0.95, 0.62, 0.75)
	var rose_deep := Color(0.88, 0.45, 0.6)
	var white_m: Material = host._toon(MALL_WHITE, 0.32, true)
	var rose_m: Material = host._toon(rose, 0.4, true)
	var rose_deep_m: Material = host._toon(rose_deep, 0.4, true)
	var gold_m: Material = host._toon(MALL_GOLD, 0.45, true, 0.0, 0.5, 0.6)
	var gold_glow: Material = VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.6)
	var pink_glow: Material = VerseAvatar.glow_mat(Color(1.0, 0.62, 0.78), 1.4)
	var rose_glow: Material = VerseAvatar.glow_mat(rose_deep, 1.3)
	var glass_m: Material = host._glass_mat()
	host._windows.append(glass_m)
	var off := GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# ---------- RUNWAY (deck + strips entirely inside r 5.8; east solid edge -2.45 keeps 1.25m off the rug) ----------
	var rc := Vector3(-4.1, 0, 0.3)
	var deck := BoxMesh.new()
	deck.size = Vector3(3.2, 0.25, 0.9)
	host._mi(q, deck, white_m, rc + Vector3(0, 0.125, 0))
	var strip_side := BoxMesh.new()
	strip_side.size = Vector3(3.2, 0.05, 0.07)
	var strip_end := BoxMesh.new()
	strip_end.size = Vector3(0.07, 0.05, 0.76)
	for side in [-1.0, 1.0]:
		var ss: MeshInstance3D = host._mi(q, strip_side, gold_glow, rc + Vector3(0, 0.265, 0.415 * side))
		ss.cast_shadow = off
		var se: MeshInstance3D = host._mi(q, strip_end, gold_glow, rc + Vector3(1.565 * side, 0.265, 0))
		se.cast_shadow = off
	var spark := SphereMesh.new()
	spark.radius = 0.07
	spark.height = 0.14
	spark.radial_segments = 8
	spark.rings = 4
	var ci := 0
	for sx in [-1.53, 1.53]:
		for sz in [-0.38, 0.38]:
			var s: MeshInstance3D = host._mi(q, spark, gold_glow, rc + Vector3(sx, 0.31, sz))
			s.cast_shadow = off
			var dur := 0.8 + 0.12 * float(ci)
			var tw := s.create_tween()
			tw.set_loops()
			tw.tween_property(s, "scale", Vector3(1.5, 1.5, 1.5), dur).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
			tw.tween_property(s, "scale", Vector3.ONE, dur).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
			ci += 1
	var disc := CylinderMesh.new()
	disc.top_radius = 0.09
	disc.bottom_radius = 0.09
	disc.height = 0.04
	disc.radial_segments = 10
	for dx in [-5.0, -3.2]:
		for dz in [-0.7, 1.3]:
			var d: MeshInstance3D = host._mi(q, disc, gold_glow, Vector3(dx, 0.02, dz))
			d.cast_shadow = off
	host._boxes.append({"pos": zo + rc, "half": Vector2(1.65, 0.48)})
	_npc(q, host, rc, 1.5, 0.25, "Vogue", "did:hey:her-model-vogue", 1.0, 0.0)
	_npc(q, host, rc, 1.5, -0.25, "Saskia", "did:hey:her-model-saskia", 1.0, PI)
	var rl := Label3D.new()
	rl.text = "HER RUNWAY"
	rl.font_size = 84
	rl.pixel_size = 0.0065
	rl.modulate = MALL_GOLD_GLOW
	rl.outline_size = 8
	rl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	rl.position = rc + Vector3(0, 2.9, 0)
	q.add_child(rl)

	# ---------- GLAM BAR (inside r 6.1, all solids z <= 3.78, outside bench clearance r 4.3) ----------
	var gb := Vector3(3.95, 0, 3.4)
	var cbody := BoxMesh.new()
	cbody.size = Vector3(1.3, 0.95, 0.5)
	host._mi(q, cbody, white_m, gb + Vector3(0, 0.475, 0))
	var ctop := BoxMesh.new()
	ctop.size = Vector3(1.42, 0.06, 0.6)
	host._mi(q, ctop, rose_m, gb + Vector3(0, 0.98, 0))
	var ckick := BoxMesh.new()
	ckick.size = Vector3(1.34, 0.06, 0.54)
	var ck: MeshInstance3D = host._mi(q, ckick, gold_glow, gb + Vector3(0, 0.05, 0))
	ck.cast_shadow = off
	var mframe := BoxMesh.new()
	mframe.size = Vector3(1.0, 2.1, 0.08)
	host._mi(q, mframe, gold_m, Vector3(3.95, 1.05, 3.72))
	var mglass := BoxMesh.new()
	mglass.size = Vector3(0.9, 2.0, 0.04)
	host._mi(q, mglass, glass_m, Vector3(3.95, 1.03, 3.67))
	var tray := BoxMesh.new()
	tray.size = Vector3(0.3, 0.04, 0.2)
	host._mi(q, tray, gold_m, gb + Vector3(0.35, 1.03, 0))
	var lip := CylinderMesh.new()
	lip.top_radius = 0.04
	lip.bottom_radius = 0.045
	lip.height = 0.12
	lip.radial_segments = 8
	var b1: MeshInstance3D = host._mi(q, lip, pink_glow, gb + Vector3(0.3, 1.11, 0.02))
	b1.cast_shadow = off
	var b2: MeshInstance3D = host._mi(q, lip, rose_glow, gb + Vector3(-0.32, 1.07, -0.04))
	b2.cast_shadow = off
	host._boxes.append({"pos": zo + Vector3(3.95, 0, 3.4), "half": Vector2(0.72, 0.32)})
	host._boxes.append({"pos": zo + Vector3(3.95, 0, 3.7), "half": Vector2(0.54, 0.08)})
	_stander(q, host, Vector3(3.0, 0, 3.6), Vector3(4.9, 0, 2.3), "Coco", "did:hey:her-coco", 1.0, ["Darling, posture! This whole floor is a runway.", "Rose gold is not a color, it is an attitude."])
	var gl := Label3D.new()
	gl.text = "GLAM BAR"
	gl.font_size = 58
	gl.pixel_size = 0.006
	gl.modulate = rose
	gl.outline_size = 7
	gl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	gl.position = Vector3(3.95, 2.45, 3.6)
	q.add_child(gl)

	# ---------- PERFUME VITRINE (west, fully outside the npc orbit band) ----------
	var pv := Vector3(-9.0, 0, -0.6)
	var pbase := BoxMesh.new()
	pbase.size = Vector3(0.72, 0.85, 0.72)
	host._mi(q, pbase, white_m, pv + Vector3(0, 0.425, 0))
	var ptrim := BoxMesh.new()
	ptrim.size = Vector3(0.8, 0.06, 0.8)
	host._mi(q, ptrim, gold_m, pv + Vector3(0, 0.88, 0))
	var pcase := BoxMesh.new()
	pcase.size = Vector3(0.6, 0.55, 0.6)
	host._mi(q, pcase, glass_m, pv + Vector3(0, 1.185, 0))
	var pcap := BoxMesh.new()
	pcap.size = Vector3(0.66, 0.05, 0.66)
	host._mi(q, pcap, gold_m, pv + Vector3(0, 1.485, 0))
	var capm := SphereMesh.new()
	capm.radius = 0.03
	capm.height = 0.06
	capm.radial_segments = 6
	capm.rings = 3
	var bots := [Vector3(-0.14, 0, 0.05), Vector3(0.13, 0, 0.11), Vector3(0.0, 0, -0.14)]
	var bmats := [pink_glow, gold_glow, rose_glow]
	for i in 3:
		var bb: MeshInstance3D = host._mi(q, lip, bmats[i], pv + bots[i] + Vector3(0, 0.97, 0))
		bb.cast_shadow = off
		host._mi(q, capm, gold_m, pv + bots[i] + Vector3(0, 1.05, 0))
	var mist := CPUParticles3D.new()
	mist.amount = 9
	mist.lifetime = 2.8
	var pm := SphereMesh.new()
	pm.radius = 0.035
	pm.height = 0.07
	pm.radial_segments = 6
	pm.rings = 3
	pm.material = VerseAvatar.glow_mat(rose, 1.1)
	mist.mesh = pm
	mist.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	mist.emission_box_extents = Vector3(0.2, 0.03, 0.2)
	mist.direction = Vector3(0, 1, 0)
	mist.spread = 18.0
	mist.gravity = Vector3.ZERO
	mist.initial_velocity_min = 0.08
	mist.initial_velocity_max = 0.2
	mist.scale_amount_min = 0.5
	mist.scale_amount_max = 1.3
	mist.position = pv + Vector3(0, 1.62, 0)
	mist.cast_shadow = off
	q.add_child(mist)
	var pl := Label3D.new()
	pl.text = "PARFUM"
	pl.font_size = 52
	pl.pixel_size = 0.006
	pl.modulate = rose
	pl.outline_size = 7
	pl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	pl.position = pv + Vector3(0, 2.2, 0)
	q.add_child(pl)
	host._boxes.append({"pos": zo + pv, "half": Vector2(0.42, 0.42)})

	# ---------- JEWELRY VITRINES (east promenade, clear of tree/arrival/escalator) ----------
	var jbase := BoxMesh.new()
	jbase.size = Vector3(0.66, 0.9, 0.66)
	var jtrim := BoxMesh.new()
	jtrim.size = Vector3(0.72, 0.05, 0.72)
	var jcase := BoxMesh.new()
	jcase.size = Vector3(0.55, 0.5, 0.55)
	var jcap := BoxMesh.new()
	jcap.size = Vector3(0.6, 0.05, 0.6)
	var jped := CylinderMesh.new()
	jped.top_radius = 0.07
	jped.bottom_radius = 0.09
	jped.height = 0.1
	jped.radial_segments = 8
	var jzs := [-0.7, -2.1]
	for i in 2:
		var jp := Vector3(10.2, 0, jzs[i])
		host._mi(q, jbase, white_m, jp + Vector3(0, 0.45, 0))
		host._mi(q, jtrim, gold_m, jp + Vector3(0, 0.925, 0))
		host._mi(q, jcase, glass_m, jp + Vector3(0, 1.2, 0))
		host._mi(q, jcap, gold_m, jp + Vector3(0, 1.475, 0))
		host._mi(q, jped, gold_m, jp + Vector3(0, 1.0, 0))
		var spn := _spinner(q, jp + Vector3(0, 1.24, 0), 1.4, 0.03)
		var gem: MeshInstance3D
		if i == 0:
			var ring := TorusMesh.new()
			ring.inner_radius = 0.05
			ring.outer_radius = 0.11
			ring.rings = 10
			ring.ring_segments = 6
			gem = host._mi(spn, ring, gold_glow, Vector3.ZERO)
			gem.rotation_degrees = Vector3(24, 0, 0)
		else:
			var jewel := SphereMesh.new()
			jewel.radius = 0.085
			jewel.height = 0.17
			jewel.radial_segments = 8
			jewel.rings = 4
			gem = host._mi(spn, jewel, pink_glow, Vector3.ZERO)
		gem.cast_shadow = off
		host._boxes.append({"pos": zo + jp, "half": Vector2(0.38, 0.38)})
	var jl := Label3D.new()
	jl.text = "BIJOUX"
	jl.font_size = 52
	jl.pixel_size = 0.006
	jl.modulate = MALL_GOLD_GLOW
	jl.outline_size = 7
	jl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	jl.position = Vector3(10.2, 2.0, -1.4)
	q.add_child(jl)

	# ---------- FLOWER ARCH (south center, EAST of the rug; kiosk ~5m away; walk-through 0.63m) ----------
	var post := CylinderMesh.new()
	post.top_radius = 0.06
	post.bottom_radius = 0.07
	post.height = 2.3
	post.radial_segments = 8
	var pcapm := SphereMesh.new()
	pcapm.radius = 0.08
	pcapm.height = 0.16
	pcapm.radial_segments = 6
	pcapm.rings = 3
	for px in [1.45, 2.4]:
		host._mi(q, post, gold_m, Vector3(px, 1.15, 4.45))
		var pc: MeshInstance3D = host._mi(q, pcapm, gold_glow, Vector3(px, 2.32, 4.45))
		pc.cast_shadow = off
		host._obstacles.append({"pos": zo + Vector3(px, 0, 4.45), "r": 0.16})
	var bloom_big := SphereMesh.new()
	bloom_big.radius = 0.17
	bloom_big.height = 0.34
	bloom_big.radial_segments = 8
	bloom_big.rings = 4
	var bloom_small := SphereMesh.new()
	bloom_small.radius = 0.13
	bloom_small.height = 0.26
	bloom_small.radial_segments = 8
	bloom_small.rings = 4
	var angs := [168.0, 142.0, 116.0, 90.0, 64.0, 38.0, 12.0]
	for k in angs.size():
		var a: float = deg_to_rad(angs[k])
		var bp := Vector3(1.925 + 0.475 * cos(a), 2.32 + 0.475 * sin(a), 4.45)
		var bm: Material = rose_m
		if k % 3 == 1:
			bm = rose_deep_m
		elif k % 3 == 2:
			bm = white_m
		if k % 2 == 0:
			host._mi(q, bloom_big, bm, bp)
		else:
			host._mi(q, bloom_small, bm, bp)
	var tops := [132.0, 90.0, 48.0]
	for k in tops.size():
		var a2: float = deg_to_rad(tops[k])
		var sp2 := Vector3(1.925 + 0.61 * cos(a2), 2.32 + 0.61 * sin(a2), 4.45)
		var g2: MeshInstance3D = host._mi(q, spark, gold_glow, sp2)
		g2.cast_shadow = off
		if k == 1:
			var tw2 := g2.create_tween()
			tw2.set_loops()
			tw2.tween_property(g2, "scale", Vector3(1.6, 1.6, 1.6), 1.1).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
			tw2.tween_property(g2, "scale", Vector3.ONE, 1.1).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	var petal := CylinderMesh.new()
	petal.top_radius = 0.06
	petal.bottom_radius = 0.06
	petal.height = 0.02
	petal.radial_segments = 6
	for pp in [Vector3(2.15, 0.01, 4.05), Vector3(1.65, 0.01, 4.0), Vector3(1.95, 0.01, 3.7)]:
		var pe: MeshInstance3D = host._mi(q, petal, rose_deep_m, pp)
		pe.cast_shadow = off

	# ---------- GOSSIP KID (chats with Coco across the glam bar; outside bench buffer r 4.3) ----------
	_stander(q, host, Vector3(4.9, 0, 2.3), Vector3(3.0, 0, 3.6), "Mimi", "did:hey:her-mimi", 0.55, ["Psst... the runway ladies get FREE perfume. I counted three bottles!"])


## ── Level 3 · MEN'S SKY LOUNGE — life + lounge decor pass ───────────────────
## Games corner (pool table mid-match, dart board, spinning barber pole), a
## leather reading nook with floor lamp, a rotating watch vitrine, twin sconces
## and a SKY LOUNGE art panel. Floor/walls/stores already exist. Every reserved
## spot honoured; the 3.6–4.4 walker ring around (0,0,0.5) stays solid-free.
## Palette: deep navy / charcoal + amber-gold glow + leather brown. 4 avatars.
func _wf_him(parent: Node3D, host: Node) -> void:
	var zo: Vector3 = host.MALL_IN + Vector3(80, 0, 0)
	var q := Node3D.new()
	q.position = zo
	parent.add_child(q)
	var navy := Color(0.13, 0.16, 0.24)
	var charcoal := Color(0.16, 0.17, 0.2)
	var leather := Color(0.36, 0.23, 0.15)
	var walnut := Color(0.24, 0.17, 0.12)
	var felt_green := Color(0.16, 0.42, 0.26)

	# ── POOL TABLE mid-game at (1.3, 2.55) — ring-safe, bench-clear ─────────
	var prug := BoxMesh.new()
	prug.size = Vector3(2.5, 0.024, 1.6)
	var prugmi: MeshInstance3D = host._mi(q, prug, host._toon(charcoal.darkened(0.25), 0.05, false), Vector3(1.35, 0.012, 2.72))
	prugmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var pbody := BoxMesh.new()
	pbody.size = Vector3(2.0, 0.55, 1.1)
	host._mi(q, pbody, host._toon(walnut, 0.2, true, 0.0, 0.5, 0.25), Vector3(1.3, 0.45, 2.55))
	var pfelt := BoxMesh.new()
	pfelt.size = Vector3(1.8, 0.05, 0.92)
	host._mi(q, pfelt, host._toon(felt_green, 0.25, false), Vector3(1.3, 0.75, 2.55))
	var pleg := BoxMesh.new()
	pleg.size = Vector3(0.14, 0.45, 0.14)
	for plx in [-0.85, 0.85]:
		for plz in [-0.4, 0.4]:
			var plxx: float = plx
			var plzz: float = plz
			host._mi(q, pleg, host._toon(charcoal, 0.15, false), Vector3(1.3 + plxx, 0.22, 2.55 + plzz))
	var ptriml := BoxMesh.new()
	ptriml.size = Vector3(2.04, 0.045, 0.06)
	for ptz in [-0.575, 0.575]:
		var ptzz: float = ptz
		host._mi(q, ptriml, host._toon(MALL_GOLD, 0.3), Vector3(1.3, 0.71, 2.55 + ptzz))
	var ptrims := BoxMesh.new()
	ptrims.size = Vector3(0.06, 0.045, 1.14)
	for ptx in [-1.0, 1.0]:
		var ptxx: float = ptx
		host._mi(q, ptrims, host._toon(MALL_GOLD, 0.3), Vector3(1.3 + ptxx, 0.71, 2.55))
	var pock := CylinderMesh.new()
	pock.top_radius = 0.07
	pock.bottom_radius = 0.07
	pock.height = 0.03
	pock.radial_segments = 8
	for pkx in [-0.84, 0.84]:
		for pkz in [-0.41, 0.41]:
			var pkxx: float = pkx
			var pkzz: float = pkz
			host._mi(q, pock, host._toon(MALL_DARK, 0.1, false), Vector3(1.3 + pkxx, 0.745, 2.55 + pkzz))
	var ball := SphereMesh.new()
	ball.radius = 0.05
	ball.height = 0.1
	ball.radial_segments = 8
	ball.rings = 4
	var cueball: MeshInstance3D = host._mi(q, ball, VerseAvatar.glow_mat(Color(0.98, 0.97, 0.92), 0.5), Vector3(0.85, 0.825, 2.4))
	cueball.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var btw := cueball.create_tween()
	btw.set_loops()
	btw.tween_property(cueball, "position", Vector3(1.5, 0.825, 2.75), 2.6).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	btw.tween_property(cueball, "position", Vector3(0.85, 0.825, 2.4), 2.6).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	var rball: MeshInstance3D = host._mi(q, ball, VerseAvatar.glow_mat(Color(0.85, 0.25, 0.2), 0.5), Vector3(1.72, 0.825, 2.72))
	rball.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var gball: MeshInstance3D = host._mi(q, ball, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.5), Vector3(1.58, 0.825, 2.34))
	gball.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var cue := BoxMesh.new()
	cue.size = Vector3(0.04, 1.5, 0.04)
	var cuemi: MeshInstance3D = host._mi(q, cue, host._toon(Color(0.62, 0.45, 0.28), 0.2, false), Vector3(0.6, 0.65, 3.09))
	cuemi.rotation_degrees = Vector3(-26, 0, -14)
	host._boxes.append({"pos": zo + Vector3(1.3, 0, 2.55), "half": Vector2(1.02, 0.6)})
	_stander(q, host, Vector3(2.2, 0, 1.75), Vector3(1.3, 0, 2.55), "Cassius", "did:verse:npc-him-cassius", 1.0,
		["Eight ball, corner pocket. Watch closely."])
	_stander(q, host, Vector3(1.3, 0, 3.55), Vector3(1.3, 0, 2.55), "Flint", "did:verse:npc-him-flint", 1.0,
		["You called that same pocket three shots ago, friend."])

	# ── BARBER corner: classic spinning pole (kiosk keepout honoured) ───────
	var bbase := CylinderMesh.new()
	bbase.top_radius = 0.2
	bbase.bottom_radius = 0.24
	bbase.height = 0.08
	bbase.radial_segments = 12
	host._mi(q, bbase, host._toon(charcoal, 0.15, false), Vector3(5.0, 0.04, 4.55))
	var bpost := CylinderMesh.new()
	bpost.top_radius = 0.05
	bpost.bottom_radius = 0.06
	bpost.height = 1.0
	bpost.radial_segments = 10
	host._mi(q, bpost, host._toon(navy, 0.2), Vector3(5.0, 0.55, 4.55))
	var bdrum := CylinderMesh.new()
	bdrum.top_radius = 0.11
	bdrum.bottom_radius = 0.11
	bdrum.height = 0.8
	bdrum.radial_segments = 12
	host._mi(q, bdrum, host._toon(MALL_WHITE, 0.2, false), Vector3(5.0, 1.45, 4.55))
	var bspin: Node3D = _spinner(q, Vector3(5.0, 1.45, 4.55), 2.0, 0.0)
	var stripe := TorusMesh.new()
	stripe.inner_radius = 0.12
	stripe.outer_radius = 0.2
	stripe.rings = 12
	stripe.ring_segments = 6
	var sdata := [[0.24, Color(0.8, 0.2, 0.2), 16.0], [0.0, Color(0.24, 0.36, 0.72), -16.0], [-0.24, Color(0.8, 0.2, 0.2), 16.0]]
	for sd in sdata:
		var sda: Array = sd
		var sy: float = sda[0]
		var scol: Color = sda[1]
		var stilt: float = sda[2]
		var stmi: MeshInstance3D = host._mi(bspin, stripe, host._toon(scol, 0.25, false), Vector3(0, sy, 0))
		stmi.rotation_degrees = Vector3(stilt, 0, 0)
	var bcap := SphereMesh.new()
	bcap.radius = 0.1
	bcap.height = 0.2
	bcap.radial_segments = 10
	bcap.rings = 5
	var bcmi: MeshInstance3D = host._mi(q, bcap, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.9), Vector3(5.0, 1.95, 4.55))
	bcmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var blbl := Label3D.new()
	blbl.text = "BARBER"
	blbl.font_size = 46
	blbl.pixel_size = 0.0055
	blbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	blbl.modulate = MALL_GOLD_GLOW
	blbl.outline_size = 8
	blbl.position = Vector3(5.0, 2.3, 4.55)
	q.add_child(blbl)
	host._obstacles.append({"pos": zo + Vector3(5.0, 0, 4.55), "r": 0.32})

	# ── leather reading nook by the south wall ──────────────────────────────
	var lrug := BoxMesh.new()
	lrug.size = Vector3(2.3, 0.024, 1.55)
	var lrmi: MeshInstance3D = host._mi(q, lrug, host._toon(navy.darkened(0.2), 0.05, false), Vector3(-0.95, 0.012, 4.1))
	lrmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var cseat := BoxMesh.new()
	cseat.size = Vector3(0.78, 0.55, 0.72)
	var ccush := BoxMesh.new()
	ccush.size = Vector3(0.64, 0.1, 0.58)
	var cback := BoxMesh.new()
	cback.size = Vector3(0.78, 0.7, 0.2)
	var carm := BoxMesh.new()
	carm.size = Vector3(0.16, 0.76, 0.72)
	var chdata := [[Vector3(-1.6, 0, 4.4), -45.0], [Vector3(-0.35, 0, 4.4), 43.0]]
	for cd in chdata:
		var cda: Array = cd
		var cpos: Vector3 = cda[0]
		var cyaw: float = cda[1]
		var ch := Node3D.new()
		ch.position = cpos
		ch.rotation_degrees = Vector3(0, cyaw, 0)
		q.add_child(ch)
		host._mi(ch, cseat, host._toon(leather, 0.25, true, 0.0, 0.5, 0.3), Vector3(0, 0.275, 0))
		host._mi(ch, ccush, host._toon(leather.lightened(0.12), 0.25, false, 0.0, 0.5, 0.3), Vector3(0, 0.57, 0.02))
		var cbmi: MeshInstance3D = host._mi(ch, cback, host._toon(leather, 0.25, true, 0.0, 0.5, 0.3), Vector3(0, 0.78, 0.34))
		cbmi.rotation_degrees = Vector3(8, 0, 0)
		for cax in [-0.47, 0.47]:
			var caxx: float = cax
			host._mi(ch, carm, host._toon(leather.darkened(0.1), 0.25, true, 0.0, 0.5, 0.3), Vector3(caxx, 0.38, 0))
		host._obstacles.append({"pos": zo + cpos, "r": 0.55})
	var ltbase := CylinderMesh.new()
	ltbase.top_radius = 0.26
	ltbase.bottom_radius = 0.3
	ltbase.height = 0.05
	ltbase.radial_segments = 12
	host._mi(q, ltbase, host._toon(charcoal, 0.15, false), Vector3(-0.95, 0.025, 3.75))
	var ltpole := CylinderMesh.new()
	ltpole.top_radius = 0.06
	ltpole.bottom_radius = 0.06
	ltpole.height = 0.38
	ltpole.radial_segments = 10
	host._mi(q, ltpole, host._toon(charcoal, 0.15, false), Vector3(-0.95, 0.22, 3.75))
	var lttop := CylinderMesh.new()
	lttop.top_radius = 0.46
	lttop.bottom_radius = 0.46
	lttop.height = 0.05
	lttop.radial_segments = 14
	host._mi(q, lttop, host._toon(walnut, 0.2, true, 0.0, 0.5, 0.3), Vector3(-0.95, 0.43, 3.75))
	var glass2 := CylinderMesh.new()
	glass2.top_radius = 0.05
	glass2.bottom_radius = 0.04
	glass2.height = 0.09
	glass2.radial_segments = 8
	for gp in [Vector3(-0.8, 0.5, 3.86), Vector3(-1.12, 0.5, 3.64)]:
		var gpp: Vector3 = gp
		var gpmi: MeshInstance3D = host._mi(q, glass2, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.7), gpp)
		gpmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var book1 := BoxMesh.new()
	book1.size = Vector3(0.28, 0.045, 0.2)
	host._mi(q, book1, host._toon(navy.lightened(0.08), 0.2, false), Vector3(-0.98, 0.478, 3.78))
	var book2 := BoxMesh.new()
	book2.size = Vector3(0.24, 0.04, 0.17)
	var bk2mi: MeshInstance3D = host._mi(q, book2, host._toon(Color(0.85, 0.8, 0.7), 0.15, false), Vector3(-0.98, 0.52, 3.78))
	bk2mi.rotation_degrees = Vector3(0, 14, 0)
	host._obstacles.append({"pos": zo + Vector3(-0.95, 0, 3.75), "r": 0.55})
	_stander(q, host, Vector3(-1.1, 0, 3.2), Vector3(-0.95, 0, 4.4), "Hawthorne", "did:verse:npc-him-hawthorne", 1.0,
		["Quiet floor, strong coffee, the whole skyline. I may never leave."])
	var paper := BoxMesh.new()
	paper.size = Vector3(0.34, 0.42, 0.02)
	var papmi: MeshInstance3D = host._mi(q, paper, host._toon(Color(0.92, 0.91, 0.86), 0.1, false), Vector3(-1.07, 1.08, 3.5))
	papmi.rotation_degrees = Vector3(-32, 0, 0)
	papmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF

	# ── two slim floor lamps (reading nook + games corner) ──────────────────
	var lbase := CylinderMesh.new()
	lbase.top_radius = 0.16
	lbase.bottom_radius = 0.18
	lbase.height = 0.05
	lbase.radial_segments = 12
	var lpole := CylinderMesh.new()
	lpole.top_radius = 0.035
	lpole.bottom_radius = 0.035
	lpole.height = 1.72
	lpole.radial_segments = 8
	var lshade := CylinderMesh.new()
	lshade.top_radius = 0.15
	lshade.bottom_radius = 0.23
	lshade.height = 0.22
	lshade.radial_segments = 12
	var lbulb := SphereMesh.new()
	lbulb.radius = 0.08
	lbulb.height = 0.16
	lbulb.radial_segments = 10
	lbulb.rings = 5
	var lampdata := [Vector3(-1.0, 0, 4.75), Vector3(4.55, 0, 2.15)]
	for li in lampdata.size():
		var lp: Vector3 = lampdata[li]
		host._mi(q, lbase, host._toon(charcoal, 0.15, false), lp + Vector3(0, 0.025, 0))
		host._mi(q, lpole, host._toon(MALL_GOLD, 0.3), lp + Vector3(0, 0.91, 0))
		host._mi(q, lshade, host._toon(charcoal, 0.15, false), lp + Vector3(0, 1.86, 0))
		var lbmi: MeshInstance3D = host._mi(q, lbulb, VerseAvatar.glow_mat(Color(1.0, 0.88, 0.6), 1.1), lp + Vector3(0, 1.72, 0))
		lbmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		var ltw := lbmi.create_tween()
		ltw.set_loops()
		ltw.tween_property(lbmi, "scale", Vector3.ONE * 1.16, 1.6 + 0.5 * float(li)).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
		ltw.tween_property(lbmi, "scale", Vector3.ONE, 1.6 + 0.5 * float(li)).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
		host._obstacles.append({"pos": zo + lp, "r": 0.3})

	# ── WATCH vitrine (pulled to (-1,-2.3): the walker ring must stay clear) ─
	var vped := BoxMesh.new()
	vped.size = Vector3(0.7, 1.0, 0.7)
	host._mi(q, vped, host._toon(navy, 0.2, true, 0.0, 0.5, 0.2), Vector3(-1.0, 0.5, -2.3))
	var vtrim := BoxMesh.new()
	vtrim.size = Vector3(0.74, 0.05, 0.74)
	host._mi(q, vtrim, host._toon(MALL_GOLD, 0.3), Vector3(-1.0, 0.985, -2.3))
	var vglow := BoxMesh.new()
	vglow.size = Vector3(0.6, 0.02, 0.6)
	var vgmi: MeshInstance3D = host._mi(q, vglow, VerseAvatar.glow_mat(Color(1.0, 0.95, 0.8), 0.6), Vector3(-1.0, 1.015, -2.3))
	vgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var vglass := BoxMesh.new()
	vglass.size = Vector3(0.62, 0.55, 0.62)
	var vmat = host._glass_mat()
	host._windows.append(vmat)
	host._mi(q, vglass, vmat, Vector3(-1.0, 1.3, -2.3))
	var vpost := BoxMesh.new()
	vpost.size = Vector3(0.04, 0.57, 0.04)
	for vpx in [-0.31, 0.31]:
		for vpz in [-0.31, 0.31]:
			var vpxx: float = vpx
			var vpzz: float = vpz
			host._mi(q, vpost, host._toon(MALL_GOLD, 0.3), Vector3(-1.0 + vpxx, 1.3, -2.3 + vpzz))
	var vspin: Node3D = _spinner(q, Vector3(-1.0, 1.26, -2.3), 0.6, 0.0)
	var wring := TorusMesh.new()
	wring.inner_radius = 0.045
	wring.outer_radius = 0.075
	wring.rings = 10
	wring.ring_segments = 6
	var wpos := [Vector3(-0.16, 0, 0), Vector3(0, 0.05, 0), Vector3(0.16, 0, 0)]
	for wi in wpos.size():
		var wp: Vector3 = wpos[wi]
		var wmi: MeshInstance3D = host._mi(vspin, wring, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.1), wp)
		wmi.rotation_degrees = Vector3(90, -20.0 + 20.0 * float(wi), 0)
		wmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var vlbl := Label3D.new()
	vlbl.text = "FINE TIMEPIECES"
	vlbl.font_size = 44
	vlbl.pixel_size = 0.005
	vlbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	vlbl.modulate = MALL_GOLD_GLOW
	vlbl.outline_size = 8
	vlbl.position = Vector3(-1.0, 2.05, -2.3)
	q.add_child(vlbl)
	var motes := CPUParticles3D.new()
	motes.position = Vector3(-1.0, 1.75, -2.3)
	motes.amount = 8
	motes.lifetime = 2.4
	motes.preprocess = 1.0
	var mmesh := SphereMesh.new()
	mmesh.radius = 0.015
	mmesh.height = 0.03
	mmesh.radial_segments = 6
	mmesh.rings = 3
	mmesh.material = VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.4)
	motes.mesh = mmesh
	motes.emission_shape = CPUParticles3D.EMISSION_SHAPE_BOX
	motes.emission_box_extents = Vector3(0.28, 0.04, 0.28)
	motes.direction = Vector3(0, 1, 0)
	motes.spread = 8.0
	motes.gravity = Vector3.ZERO
	motes.initial_velocity_min = 0.12
	motes.initial_velocity_max = 0.25
	q.add_child(motes)
	host._obstacles.append({"pos": zo + Vector3(-1.0, 0, -2.3), "r": 0.55})
	_stander(q, host, Vector3(-1.0, 0, -1.35), Vector3(-1.0, 0, -2.3), "Sterling", "did:verse:npc-him-sterling", 1.0,
		["That gold chronograph... one day."])

	# ── dart board on a slim pillar, south edge ─────────────────────────────
	var dpil := BoxMesh.new()
	dpil.size = Vector3(0.16, 1.7, 0.16)
	host._mi(q, dpil, host._toon(charcoal, 0.15), Vector3(3.0, 0.85, 4.6))
	var dface := CylinderMesh.new()
	dface.top_radius = 0.34
	dface.bottom_radius = 0.34
	dface.height = 0.07
	dface.radial_segments = 14
	var dfmi: MeshInstance3D = host._mi(q, dface, host._toon(navy.darkened(0.15), 0.1, false), Vector3(3.0, 1.45, 4.5))
	dfmi.rotation_degrees = Vector3(90, 0, 0)
	var dring := TorusMesh.new()
	dring.inner_radius = 0.2
	dring.outer_radius = 0.3
	dring.rings = 14
	dring.ring_segments = 6
	var drmi: MeshInstance3D = host._mi(q, dring, host._toon(MALL_CREAM, 0.2, false), Vector3(3.0, 1.45, 4.45))
	drmi.rotation_degrees = Vector3(90, 0, 0)
	var dbull := SphereMesh.new()
	dbull.radius = 0.045
	dbull.height = 0.09
	dbull.radial_segments = 8
	dbull.rings = 4
	var dbmi: MeshInstance3D = host._mi(q, dbull, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.0), Vector3(3.0, 1.45, 4.44))
	dbmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dart := BoxMesh.new()
	dart.size = Vector3(0.024, 0.024, 0.16)
	var ddata := [[Vector3(2.9, 1.54, 4.38), Color(0.8, 0.2, 0.2)], [Vector3(3.1, 1.38, 4.38), MALL_GOLD], [Vector3(3.04, 1.56, 4.38), MALL_CREAM]]
	for dd in ddata:
		var dda: Array = dd
		var dpos: Vector3 = dda[0]
		var dcol: Color = dda[1]
		var dmi: MeshInstance3D = host._mi(q, dart, host._toon(dcol, 0.2, false), dpos)
		dmi.rotation_degrees = Vector3(-8 + randf() * 16.0, randf() * 8.0, 0)
	var oche := BoxMesh.new()
	oche.size = Vector3(0.55, 0.02, 0.3)
	var ocmi: MeshInstance3D = host._mi(q, oche, host._toon(charcoal.darkened(0.2), 0.05, false), Vector3(3.0, 0.01, 3.05))
	ocmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var oline := BoxMesh.new()
	oline.size = Vector3(0.55, 0.024, 0.05)
	var olmi: MeshInstance3D = host._mi(q, oline, host._toon(MALL_CREAM, 0.15, false), Vector3(3.0, 0.012, 2.96))
	olmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	host._obstacles.append({"pos": zo + Vector3(3.0, 0, 4.6), "r": 0.3})

	# ── wall sconces + art panel high on the south wall (above heads) ───────
	var splate := BoxMesh.new()
	splate.size = Vector3(0.3, 0.5, 0.07)
	var sstrip := BoxMesh.new()
	sstrip.size = Vector3(0.08, 0.36, 0.05)
	for scx in [-3.9, 3.9]:
		var scxx: float = scx
		host._mi(q, splate, host._toon(charcoal, 0.15, false), Vector3(scxx, 2.4, 4.97))
		var ssmi: MeshInstance3D = host._mi(q, sstrip, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.0), Vector3(scxx, 2.4, 4.92))
		ssmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_art_panel(q, host, Vector3(0, 6.1, -5.25), 0.0, MALL_GOLD_GLOW, "SKY LOUNGE")

	# ── greenery inside the walker ring's clear middle ──────────────────────
	_planter_bush(q, host, zo, Vector3(2.2, 0, -1.0))
	_planter_bush(q, host, zo, Vector3(-2.45, 0, -0.5))


## Grand-hall FUN layer — three attractions on the pre-cleared anchors:
## a candy-red claw machine by the stores (A), DJ Volt's twirling dance-bot
## stage against the east gallery (B), Fluff's striped candy-floss cart in
## the south-west corner (C), plus Bobbin the balloon bot working the south
## strip with three glow balloons and one runaway. 4 avatars, 3 solids; the
## balloon bot deliberately gets no solid so the NPC orbit ring stays clean.
func _wf_hall_fun(parent: Node3D, host: Node) -> void:
	var q := Node3D.new()
	q.position = host.MALL_IN
	parent.add_child(q)

	# ── ANCHOR A: the claw machine (max r 1.1) ───────────────────────────────
	var ax := Vector3(-3.2, 0, -4.35)
	host._obstacles.append({"pos": host.MALL_IN + ax, "r": 0.75})
	var cbase := BoxMesh.new()
	cbase.size = Vector3(0.95, 0.5, 0.95)
	host._mi(q, cbase, host._toon(MALL_DARK, 0.2), ax + Vector3(0, 0.25, 0))
	var cglass := BoxMesh.new()
	cglass.size = Vector3(0.9, 1.8, 0.9)
	var cgm: Material = host._glass_mat()
	host._windows.append(cgm)
	var cgmi: MeshInstance3D = host._mi(q, cglass, cgm, ax + Vector3(0, 1.4, 0))
	cgmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var post := BoxMesh.new()
	post.size = Vector3(0.07, 1.8, 0.07)
	for px in [-0.44, 0.44]:
		for pz in [-0.44, 0.44]:
			var pxx: float = px
			var pzz: float = pz
			host._mi(q, post, host._toon(MALL_GOLD, 0.3), ax + Vector3(pxx, 1.4, pzz))
	var ccap := BoxMesh.new()
	ccap.size = Vector3(1.04, 0.22, 1.04)
	host._mi(q, ccap, host._toon(MALL_DARK, 0.2), ax + Vector3(0, 2.41, 0))
	var cband := BoxMesh.new()
	cband.size = Vector3(1.08, 0.06, 1.08)
	var cbmi: MeshInstance3D = host._mi(q, cband, VerseAvatar.glow_mat(Color(1.0, 0.45, 0.6), 1.1), ax + Vector3(0, 2.31, 0))
	cbmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# six prizes piled on the base floor, one stacked on top of the heap
	var ball := SphereMesh.new()
	ball.radius = 0.12
	ball.height = 0.24
	ball.radial_segments = 8
	ball.rings = 4
	var prizes := [
		[Vector3(-0.22, 0.62, -0.08), Color(0.9, 0.32, 0.34)],
		[Vector3(0.06, 0.62, 0.2), Color(0.36, 0.78, 0.75)],
		[Vector3(0.25, 0.62, -0.16), Color(0.97, 0.78, 0.35)],
		[Vector3(-0.04, 0.62, -0.27), Color(0.66, 0.45, 0.92)],
		[Vector3(-0.29, 0.62, 0.16), Color(0.45, 0.78, 0.45)],
		[Vector3(-0.06, 0.84, 0.0), Color(0.95, 0.55, 0.72)],
	]
	for pr in prizes:
		var pa: Array = pr
		host._mi(q, ball, host._toon(pa[1], 0.35, false, 0.0, 0.5, 0.3), ax + pa[0])
	# the claw itself, forever hunting on a slow bob inside the glass
	var claw: Node3D = _spinner(q, ax + Vector3(0, 1.98, 0), 0.4, 0.1)
	var shaft := CylinderMesh.new()
	shaft.top_radius = 0.022
	shaft.bottom_radius = 0.022
	shaft.height = 0.55
	shaft.radial_segments = 6
	host._mi(claw, shaft, host._toon(Color(0.55, 0.58, 0.64), 0.15, false), Vector3(0, -0.28, 0))
	var knuckle := SphereMesh.new()
	knuckle.radius = 0.06
	knuckle.height = 0.12
	knuckle.radial_segments = 8
	knuckle.rings = 4
	host._mi(claw, knuckle, host._toon(MALL_GOLD, 0.3), Vector3(0, -0.56, 0))
	var finger := BoxMesh.new()
	finger.size = Vector3(0.035, 0.17, 0.035)
	for k in 3:
		var fa := TAU * float(k) / 3.0
		var fmi: MeshInstance3D = host._mi(claw, finger, host._toon(MALL_GOLD, 0.3),
			Vector3(cos(fa) * 0.075, -0.66, sin(fa) * 0.075))
		fmi.rotation_degrees = Vector3(0, -rad_to_deg(fa), 18)
	# coin panel + prize chute on the front of the base
	var panel := BoxMesh.new()
	panel.size = Vector3(0.22, 0.3, 0.06)
	host._mi(q, panel, host._toon(MALL_GOLD.darkened(0.2), 0.25), ax + Vector3(0.26, 0.3, 0.46))
	var pdot := SphereMesh.new()
	pdot.radius = 0.03
	pdot.height = 0.06
	pdot.radial_segments = 6
	pdot.rings = 3
	var pdmi: MeshInstance3D = host._mi(q, pdot, VerseAvatar.glow_mat(MALL_TEAL, 1.3), ax + Vector3(0.26, 0.39, 0.49))
	pdmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var chute := BoxMesh.new()
	chute.size = Vector3(0.3, 0.24, 0.06)
	host._mi(q, chute, host._toon(Color(0.06, 0.07, 0.09), 0.1, false), ax + Vector3(-0.2, 0.18, 0.46))
	var lip := BoxMesh.new()
	lip.size = Vector3(0.34, 0.035, 0.05)
	var lipmi: MeshInstance3D = host._mi(q, lip, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 0.8), ax + Vector3(-0.2, 0.33, 0.47))
	lipmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var wlbl := Label3D.new()
	wlbl.text = "WIN!"
	wlbl.font_size = 78
	wlbl.pixel_size = 0.006
	wlbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	wlbl.modulate = Color(1.0, 0.55, 0.7)
	wlbl.outline_size = 12
	wlbl.position = ax + Vector3(0, 2.75, 0)
	q.add_child(wlbl)
	# a robot kid pressed at the glass, willing the claw toward the gold one
	_stander(q, host, ax + Vector3(-0.1, 0, 0.82), ax, "Pip", "did:verse:npc-pip", 0.55,
		["the gold one!! it was SO close that time.", "one more try. just one more."])

	# ── ANCHOR B: DJ Volt's dance-bot stage (max r 1.3) ──────────────────────
	var bx := Vector3(9.9, 0, 0.2)
	host._obstacles.append({"pos": host.MALL_IN + bx, "r": 1.2})
	var pod := CylinderMesh.new()
	pod.top_radius = 1.0
	pod.bottom_radius = 1.1
	pod.height = 0.22
	pod.radial_segments = 14
	host._mi(q, pod, host._toon(MALL_DARK, 0.2), bx + Vector3(0, 0.11, 0))
	var rimt := TorusMesh.new()
	rimt.inner_radius = 0.92
	rimt.outer_radius = 1.02
	rimt.rings = 14
	rimt.ring_segments = 6
	var rmi: MeshInstance3D = host._mi(q, rimt, VerseAvatar.glow_mat(CYAN, 0.9), bx + Vector3(0, 0.22, 0))
	rmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dsc := CylinderMesh.new()
	dsc.top_radius = 0.92
	dsc.bottom_radius = 0.92
	dsc.height = 0.04
	dsc.radial_segments = 14
	var dscmi: MeshInstance3D = host._mi(q, dsc, VerseAvatar.glow_mat(Color(0.95, 0.35, 0.75), 0.7), bx + Vector3(0, 0.24, 0))
	dscmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dtw := dscmi.create_tween()
	dtw.set_loops()
	dtw.tween_property(dscmi, "scale", Vector3(1.06, 1.0, 1.06), 0.6).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	dtw.tween_property(dscmi, "scale", Vector3.ONE, 0.6).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	var stud := SphereMesh.new()
	stud.radius = 0.05
	stud.height = 0.1
	stud.radial_segments = 6
	stud.rings = 3
	for k2 in 6:
		var sa2 := TAU * float(k2) / 6.0
		var stmi: MeshInstance3D = host._mi(q, stud, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.0),
			bx + Vector3(cos(sa2) * 1.06, 0.08, sin(sa2) * 1.06))
		stmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# the dancer: a stylized bot built straight onto the spinner so it twirls
	var dancer: Node3D = _spinner(q, bx + Vector3(0, 0.38, 0), 1.6, 0.12)
	var dleg := BoxMesh.new()
	dleg.size = Vector3(0.09, 0.34, 0.09)
	for lx in [-0.09, 0.09]:
		var lxx: float = lx
		host._mi(dancer, dleg, host._toon(MALL_DARK, 0.2), Vector3(lxx, 0.17, 0))
	var dbody := CapsuleMesh.new()
	dbody.radius = 0.16
	dbody.height = 0.6
	dbody.radial_segments = 10
	dbody.rings = 5
	host._mi(dancer, dbody, host._toon(Color(0.62, 0.32, 0.95), 0.35), Vector3(0, 0.64, 0))
	var dchest := BoxMesh.new()
	dchest.size = Vector3(0.16, 0.1, 0.05)
	var dchmi: MeshInstance3D = host._mi(dancer, dchest, VerseAvatar.glow_mat(CYAN, 1.2), Vector3(0, 0.7, 0.13))
	dchmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dhead := SphereMesh.new()
	dhead.radius = 0.14
	dhead.height = 0.28
	dhead.radial_segments = 10
	dhead.rings = 5
	host._mi(dancer, dhead, host._toon(Color(0.92, 0.93, 0.96), 0.3), Vector3(0, 1.08, 0))
	var dvisor := BoxMesh.new()
	dvisor.size = Vector3(0.2, 0.06, 0.06)
	var dvmi: MeshInstance3D = host._mi(dancer, dvisor, VerseAvatar.glow_mat(MALL_TEAL, 1.4), Vector3(0, 1.09, 0.1))
	dvmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var darm := BoxMesh.new()
	darm.size = Vector3(0.4, 0.075, 0.075)
	var lamr: MeshInstance3D = host._mi(dancer, darm, host._toon(Color(0.62, 0.32, 0.95), 0.35), Vector3(-0.27, 0.86, 0))
	lamr.rotation_degrees = Vector3(0, 0, -40)
	var ramr: MeshInstance3D = host._mi(dancer, darm, host._toon(Color(0.62, 0.32, 0.95), 0.35), Vector3(0.27, 0.64, 0))
	ramr.rotation_degrees = Vector3(0, 0, -30)
	var dhand := SphereMesh.new()
	dhand.radius = 0.05
	dhand.height = 0.1
	dhand.radial_segments = 6
	dhand.rings = 3
	for hp in [Vector3(-0.42, 0.99, 0), Vector3(0.44, 0.54, 0)]:
		var hpp: Vector3 = hp
		var dhmi: MeshInstance3D = host._mi(dancer, dhand, VerseAvatar.glow_mat(MALL_GOLD_GLOW, 1.2), hpp)
		dhmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dant := CylinderMesh.new()
	dant.top_radius = 0.015
	dant.bottom_radius = 0.015
	dant.height = 0.12
	dant.radial_segments = 6
	host._mi(dancer, dant, host._toon(MALL_DARK, 0.2), Vector3(0, 1.26, 0))
	var dab: MeshInstance3D = host._mi(dancer, dhand, VerseAvatar.glow_mat(Color(1.0, 0.45, 0.85), 1.5), Vector3(0, 1.34, 0))
	dab.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# two angled spotlights mounted on the stage disc, flanking the dancer
	var sbase := CylinderMesh.new()
	sbase.top_radius = 0.12
	sbase.bottom_radius = 0.16
	sbase.height = 0.08
	sbase.radial_segments = 10
	var spole := CylinderMesh.new()
	spole.top_radius = 0.035
	spole.bottom_radius = 0.035
	spole.height = 1.45
	spole.radial_segments = 8
	var shead := BoxMesh.new()
	shead.size = Vector3(0.16, 0.13, 0.13)
	var beam := CylinderMesh.new()
	beam.top_radius = 0.05
	beam.bottom_radius = 0.28
	beam.height = 1.3
	beam.radial_segments = 10
	var spots := [
		[bx + Vector3(-0.45, 0.26, 0.58), bx + Vector3(-0.17, 1.36, 0.14), Vector3(-38, 0, -42), CYAN],
		[bx + Vector3(-0.45, 0.26, -0.58), bx + Vector3(-0.17, 1.36, -0.14), Vector3(38, 0, -42), Color(1.0, 0.45, 0.85)],
	]
	for sp in spots:
		var spa: Array = sp
		host._mi(q, sbase, host._toon(MALL_DARK, 0.2), spa[0] + Vector3(0, 0.04, 0))
		host._mi(q, spole, host._toon(MALL_DARK.lightened(0.15), 0.2), spa[0] + Vector3(0, 0.76, 0))
		var hdmi: MeshInstance3D = host._mi(q, shead, host._toon(MALL_GOLD, 0.3), spa[0] + Vector3(0, 1.52, 0))
		hdmi.rotation_degrees = spa[2]
		var bmmi: MeshInstance3D = host._mi(q, beam, VerseAvatar.glow_mat(spa[3], 0.55), spa[1])
		bmmi.rotation_degrees = spa[2]
		bmmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dlbl := Label3D.new()
	dlbl.text = "DJ VOLT"
	dlbl.font_size = 64
	dlbl.pixel_size = 0.006
	dlbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	dlbl.modulate = Color(1.0, 0.5, 0.88)
	dlbl.outline_size = 10
	dlbl.position = bx + Vector3(0, 2.9, 0)
	q.add_child(dlbl)
	var ltw := dlbl.create_tween()
	ltw.set_loops()
	ltw.tween_property(dlbl, "position:y", 3.1, 1.4).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	ltw.tween_property(dlbl, "position:y", 2.9, 1.4).set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_IN_OUT)
	# a thin drift of stage sparkles over the dancer
	var spark := CPUParticles3D.new()
	spark.amount = 10
	spark.lifetime = 1.8
	spark.emission_shape = CPUParticles3D.EMISSION_SHAPE_SPHERE
	spark.emission_sphere_radius = 0.75
	spark.direction = Vector3(0, 1, 0)
	spark.spread = 20.0
	spark.gravity = Vector3(0, 0.2, 0)
	spark.initial_velocity_min = 0.15
	spark.initial_velocity_max = 0.35
	spark.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var smesh := SphereMesh.new()
	smesh.radius = 0.03
	smesh.height = 0.06
	smesh.radial_segments = 6
	smesh.rings = 3
	smesh.material = VerseAvatar.glow_mat(Color(1.0, 0.6, 0.9), 1.6)
	spark.mesh = smesh
	spark.position = bx + Vector3(0, 2.0, 0)
	q.add_child(spark)
	_stander(q, host, Vector3(8.0, 0, 0.9), bx, "Jive", "did:verse:npc-jive", 1.0,
		["DJ Volt hasn't stopped spinning since launch day.", "wait for the arm move. it's coming."])

	# ── ANCHOR C: Fluff's candy-floss cart (max r 1.2) ───────────────────────
	var cx := Vector3(-10.4, 0, 6.0)
	host._obstacles.append({"pos": host.MALL_IN + cx, "r": 0.8})
	var cartb := BoxMesh.new()
	cartb.size = Vector3(1.1, 0.7, 0.72)
	host._mi(q, cartb, host._toon(Color(0.95, 0.62, 0.75), 0.3), cx + Vector3(0, 0.62, 0))
	var stripe := BoxMesh.new()
	stripe.size = Vector3(0.16, 0.7, 0.74)
	for sx in [-0.33, 0.0, 0.33]:
		var sxx: float = sx
		host._mi(q, stripe, host._toon(MALL_WHITE, 0.2), cx + Vector3(sxx, 0.62, 0))
	var counter := BoxMesh.new()
	counter.size = Vector3(1.24, 0.07, 0.84)
	host._mi(q, counter, host._toon(MALL_CREAM, 0.2), cx + Vector3(0, 1.0, 0))
	var trimm := BoxMesh.new()
	trimm.size = Vector3(1.2, 0.05, 0.05)
	var trmi: MeshInstance3D = host._mi(q, trimm, VerseAvatar.glow_mat(Color(1.0, 0.6, 0.75), 1.0), cx + Vector3(0, 1.0, -0.44))
	trmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var wheel := CylinderMesh.new()
	wheel.top_radius = 0.17
	wheel.bottom_radius = 0.17
	wheel.height = 0.07
	wheel.radial_segments = 10
	for wx in [-0.36, 0.36]:
		var wxx: float = wx
		var whmi: MeshInstance3D = host._mi(q, wheel, host._toon(MALL_DARK, 0.2), cx + Vector3(wxx, 0.17, -0.4))
		whmi.rotation_degrees = Vector3(90, 0, 0)
	var cleg := CylinderMesh.new()
	cleg.top_radius = 0.035
	cleg.bottom_radius = 0.035
	cleg.height = 0.28
	cleg.radial_segments = 6
	for gx in [-0.36, 0.36]:
		var gxx: float = gx
		host._mi(q, cleg, host._toon(MALL_DARK, 0.2), cx + Vector3(gxx, 0.14, 0.32))
	var handle := CylinderMesh.new()
	handle.top_radius = 0.03
	handle.bottom_radius = 0.03
	handle.height = 0.5
	handle.radial_segments = 8
	var hnmi: MeshInstance3D = host._mi(q, handle, host._toon(MALL_GOLD, 0.3), cx + Vector3(0.66, 0.85, 0))
	hnmi.rotation_degrees = Vector3(90, 0, 0)
	var hrod := CylinderMesh.new()
	hrod.top_radius = 0.02
	hrod.bottom_radius = 0.02
	hrod.height = 0.16
	hrod.radial_segments = 6
	for rz in [-0.18, 0.18]:
		var rzz: float = rz
		var hrmi: MeshInstance3D = host._mi(q, hrod, host._toon(MALL_GOLD, 0.3), cx + Vector3(0.6, 0.85, rzz))
		hrmi.rotation_degrees = Vector3(0, 0, 90)
	var ppole := CylinderMesh.new()
	ppole.top_radius = 0.04
	ppole.bottom_radius = 0.04
	ppole.height = 1.45
	ppole.radial_segments = 8
	host._mi(q, ppole, host._toon(MALL_WHITE, 0.2), cx + Vector3(0, 1.72, 0))
	var parasol := CylinderMesh.new()
	parasol.top_radius = 0.06
	parasol.bottom_radius = 0.95
	parasol.height = 0.5
	parasol.radial_segments = 12
	host._mi(q, parasol, host._toon(Color(0.93, 0.5, 0.66), 0.3), cx + Vector3(0, 2.5, 0))
	var scal := SphereMesh.new()
	scal.radius = 0.09
	scal.height = 0.18
	scal.radial_segments = 6
	scal.rings = 3
	for k3 in 6:
		var ca := TAU * float(k3) / 6.0
		host._mi(q, scal, host._toon(MALL_WHITE, 0.2), cx + Vector3(cos(ca) * 0.92, 2.27, sin(ca) * 0.92))
	var finial := SphereMesh.new()
	finial.radius = 0.07
	finial.height = 0.14
	finial.radial_segments = 6
	finial.rings = 3
	host._mi(q, finial, host._toon(MALL_GOLD, 0.3), cx + Vector3(0, 2.82, 0))
	# two floss clouds on sticks, plus the sugar jar
	var stick := CylinderMesh.new()
	stick.top_radius = 0.018
	stick.bottom_radius = 0.018
	stick.height = 0.3
	stick.radial_segments = 6
	var cloud := SphereMesh.new()
	cloud.radius = 0.18
	cloud.height = 0.36
	cloud.radial_segments = 10
	cloud.rings = 5
	host._mi(q, stick, host._toon(MALL_CREAM, 0.2), cx + Vector3(-0.22, 1.18, -0.18))
	host._mi(q, cloud, host._toon(Color(0.98, 0.72, 0.82), 0.35, true, 0.12, 0.85), cx + Vector3(-0.22, 1.45, -0.18))
	host._mi(q, stick, host._toon(MALL_CREAM, 0.2), cx + Vector3(0.18, 1.18, 0.14))
	host._mi(q, cloud, host._toon(Color(0.95, 0.6, 0.74), 0.35, true, 0.12, 0.85), cx + Vector3(0.18, 1.43, 0.14))
	var jar := CylinderMesh.new()
	jar.top_radius = 0.09
	jar.bottom_radius = 0.09
	jar.height = 0.18
	jar.radial_segments = 8
	host._mi(q, jar, host._toon(MALL_WHITE, 0.2), cx + Vector3(0.42, 1.13, -0.1))
	var clbl := Label3D.new()
	clbl.text = "FLUFF'S FLOSS"
	clbl.font_size = 56
	clbl.pixel_size = 0.006
	clbl.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	clbl.modulate = Color(1.0, 0.62, 0.76)
	clbl.outline_size = 10
	clbl.position = cx + Vector3(0, 3.1, 0)
	q.add_child(clbl)
	_stander(q, host, Vector3(-11.3, 0, 5.55), Vector3(-9.2, 0, 5.0), "Fluff", "did:verse:npc-fluff", 1.0,
		["spun fresh — clouds you can eat!", "pink or extra pink. those are the flavours."])

	# ── the balloon bot, working the south strip (no solid: thin strings) ────
	var bb := Vector3(-3.6, 0, 5.7)
	_stander(q, host, bb, Vector3(-1.2, 0, 4.4), "Bobbin", "did:verse:npc-bobbin", 1.0,
		["balloons! they glow, they float, they're free."])
	var bnode := Node3D.new()
	bnode.position = bb
	q.add_child(bnode)
	var bstring := BoxMesh.new()
	bstring.size = Vector3(0.016, 1.55, 0.016)
	var bball := SphereMesh.new()
	bball.radius = 0.17
	bball.height = 0.34
	bball.radial_segments = 10
	bball.rings = 5
	var bdefs := [
		[Vector3(0.26, 1.82, 0.1), Vector3(0, 0, -7), Vector3(0.36, 2.6, 0.1), CYAN],
		[Vector3(0.08, 1.86, -0.14), Vector3(-6, 0, 0), Vector3(0.08, 2.66, -0.22), MALL_GOLD_GLOW],
		[Vector3(-0.12, 1.78, 0.16), Vector3(0, 0, 8), Vector3(-0.23, 2.56, 0.16), Color(1.0, 0.55, 0.75)],
	]
	for bd in bdefs:
		var bda: Array = bd
		var bsmi: MeshInstance3D = host._mi(bnode, bstring, host._toon(MALL_DARK, 0.1, false), bda[0])
		bsmi.rotation_degrees = bda[1]
		var bbmi: MeshInstance3D = host._mi(bnode, bball, VerseAvatar.glow_mat(bda[3], 1.1), bda[2])
		bbmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	# one that got away, drifting on a lazy bob above the strip
	var drift: Node3D = _spinner(q, bb + Vector3(-0.55, 3.3, 0.35), 0.25, 0.35)
	var drmi: MeshInstance3D = host._mi(drift, bball, VerseAvatar.glow_mat(Color(0.62, 0.45, 0.95), 1.2), Vector3.ZERO)
	drmi.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	var dstr := BoxMesh.new()
	dstr.size = Vector3(0.014, 0.5, 0.014)
	host._mi(drift, dstr, host._toon(MALL_DARK, 0.1, false), Vector3(0, -0.42, 0))
