extends CanvasLayer
## In-Verse CREATOR overlay — Dress your avatar (chassis finish + catalog
## headwear) and Build (grid-place catalog objects in your home). Self-contained
## 2D UI; home.gd drives the 3D side (ghost, raycast, grid snap, spawn, persist)
## through these signals. Matches the Hey HUD design tokens (hud.gd).
##
## Local-first TEST mode: every catalog item is available. Ownership gating
## (only equip/place ids you own) layers on later by filtering VerseCatalog.all()
## against an owned-id set — the UI here already routes through ids, so that is a
## one-line filter when the inventory/NFT layer lands.

signal finish_picked(id: String)
signal hat_picked(id: String)          # "" = bare head
signal item_picked(id: String)         # start placing this catalog item
signal rotate_pressed()
signal delete_pressed()
signal place_confirmed()
signal place_cancelled()
signal edit_mode_pressed()             # enter "move/delete placed" mode

const GOLD := Color(0.831, 0.722, 0.294)
const GOLD_HI := Color(0.910, 0.802, 0.380)
const NAVY := Color(0.035, 0.078, 0.153)
const INK := Color(0.918, 0.941, 0.980)
const MUTED := Color(0.553, 0.627, 0.745)
const SHEET := Color(0.047, 0.102, 0.200)
const GLASS_BORDER := Color(1, 1, 1, 0.10)

const FINISHES := [["gold", "Gold"], ["silver", "Silver"], ["obsidian", "Obsidian"], ["classic", "Classic"]]
const KINDS := [["", "All"], ["seating", "Seating"], ["table", "Tables"], ["lighting", "Lighting"], ["wallart", "Wall art"], ["plant", "Plants"], ["decor", "Decor"]]

var _fab: Button
var _panel: PanelContainer
var _body: VBoxContainer
var _actions: PanelContainer
var _act_row: HBoxContainer
var _hint: Label
var _tab := "dress"
var _kind := ""


func _ready() -> void:
	layer = 11   # above hud.gd (layer 10)

	_fab = Button.new()
	_fab.text = "✎ Create"
	_fab.custom_minimum_size = Vector2(122, 44)
	_fab.set_anchors_and_offsets_preset(Control.PRESET_TOP_LEFT)
	_fab.offset_left = 12.0
	_fab.offset_top = 122.0
	_solid(_fab)
	_fab.pressed.connect(func() -> void: open_panel(not _panel.visible))
	add_child(_fab)

	_panel = PanelContainer.new()
	_panel.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.97)))
	_panel.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	_panel.offset_left = 10.0
	_panel.offset_right = -10.0
	_panel.offset_top = -470.0
	_panel.offset_bottom = -188.0
	_panel.visible = false
	add_child(_panel)

	var col := VBoxContainer.new()
	col.add_theme_constant_override("separation", 8)
	_panel.add_child(col)

	var tabs := HBoxContainer.new()
	tabs.add_theme_constant_override("separation", 8)
	col.add_child(tabs)
	var t1 := _chip("Dress avatar")
	t1.pressed.connect(func() -> void: _show_tab("dress"))
	tabs.add_child(t1)
	var t2 := _chip("Build")
	t2.pressed.connect(func() -> void: _show_tab("build"))
	tabs.add_child(t2)
	var sp := Control.new()
	sp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	sp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	tabs.add_child(sp)
	var tc := _chip("✕")
	tc.pressed.connect(func() -> void: open_panel(false))
	tabs.add_child(tc)

	var scr := ScrollContainer.new()
	scr.custom_minimum_size = Vector2(0, 232)
	scr.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	col.add_child(scr)
	_body = VBoxContainer.new()
	_body.add_theme_constant_override("separation", 8)
	_body.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scr.add_child(_body)

	# action bar — shown by home.gd while placing/editing; sits above the chat bar
	_actions = PanelContainer.new()
	_actions.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.96), 24))
	_actions.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	_actions.offset_left = 10.0
	_actions.offset_right = -10.0
	_actions.offset_top = -250.0
	_actions.offset_bottom = -190.0
	_actions.visible = false
	add_child(_actions)
	var av := VBoxContainer.new()
	av.add_theme_constant_override("separation", 4)
	_actions.add_child(av)
	_hint = Label.new()
	_hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_hint.add_theme_font_size_override("font_size", 12)
	_hint.add_theme_color_override("font_color", MUTED)
	av.add_child(_hint)
	_act_row = HBoxContainer.new()
	_act_row.add_theme_constant_override("separation", 8)
	_act_row.alignment = BoxContainer.ALIGNMENT_CENTER
	av.add_child(_act_row)

	_build_tab_body()


# ── public API (home.gd) ───────────────────────────────────────────────────

## Available only in-world; hidden on the boot podium.
func active(on: bool) -> void:
	_fab.visible = on
	if not on:
		open_panel(false)
		hide_actions()


func open_panel(v: bool) -> void:
	_panel.visible = v
	_fab.text = "✕ Close" if v else "✎ Create"
	if v:
		hide_actions()
		place_cancelled.emit()   # leaving any placement when the menu reopens


func show_place_actions() -> void:
	_panel.visible = false
	_fab.text = "✎ Create"
	_hint.text = "drag on the floor to move · snaps to grid"
	_fill([["⟲ Rotate", rotate_pressed], ["✓ Place", place_confirmed], ["✕ Cancel", place_cancelled]])
	_actions.visible = true


func show_edit_actions() -> void:
	_panel.visible = false
	_fab.text = "✎ Create"
	_hint.text = "tap an object to pick it · drag to move · ⟲/🗑"
	_fill([["⟲ Rotate", rotate_pressed], ["🗑 Delete", delete_pressed], ["✓ Done", place_cancelled]])
	_actions.visible = true


func hide_actions() -> void:
	_actions.visible = false


# ── internals ───────────────────────────────────────────────────────────────

func _show_tab(t: String) -> void:
	_tab = t
	if not _panel.visible:
		open_panel(true)
	_build_tab_body()


func _build_tab_body() -> void:
	for c in _body.get_children():
		c.queue_free()
	if _tab == "dress":
		_build_dress()
	else:
		_build_build()


func _build_dress() -> void:
	_body.add_child(_section("Chassis finish"))
	var fr := _wrap()
	for f in FINISHES:
		var b := _chip(str(f[1]))
		var id := str(f[0])
		b.pressed.connect(func() -> void: finish_picked.emit(id))
		fr.add_child(b)
	_body.add_child(fr)
	_body.add_child(_section("Headwear"))
	var hr := _wrap()
	var none := _chip("None")
	none.pressed.connect(func() -> void: hat_picked.emit(""))
	hr.add_child(none)
	for it in VerseCatalog.all():
		if str(it.get("kind", "")) != "hat":
			continue
		var id := str(it["id"])
		var b := _chip(str(it.get("name", id)))
		b.add_theme_color_override("font_color", _rarity(str(it.get("rarity", ""))))
		b.pressed.connect(func() -> void: hat_picked.emit(id))
		hr.add_child(b)
	_body.add_child(hr)


func _build_build() -> void:
	var cr := _wrap()
	for k in KINDS:
		var b := _chip(str(k[1]))
		var key := str(k[0])
		b.pressed.connect(func() -> void:
			_kind = key
			_build_tab_body())
		cr.add_child(b)
	_body.add_child(cr)
	var ep := _chip("✎ Move / delete placed")
	ep.pressed.connect(func() -> void: edit_mode_pressed.emit())
	_body.add_child(ep)
	_body.add_child(_section("Tap an object to place it"))
	var grid := GridContainer.new()
	grid.columns = 2
	grid.add_theme_constant_override("h_separation", 8)
	grid.add_theme_constant_override("v_separation", 8)
	for it in VerseCatalog.all():
		var kind := str(it.get("kind", ""))
		if kind == "hat":
			continue   # headwear is worn, not placed
		if _kind != "" and kind != _kind:
			continue
		var id := str(it["id"])
		var b := _chip(str(it.get("name", id)))
		b.custom_minimum_size = Vector2(0, 50)
		b.clip_text = true
		b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		b.add_theme_color_override("font_color", _rarity(str(it.get("rarity", ""))))
		b.add_theme_font_size_override("font_size", 13)
		b.pressed.connect(func() -> void: item_picked.emit(id))
		grid.add_child(b)
	_body.add_child(grid)


func _fill(items: Array) -> void:
	for c in _act_row.get_children():
		c.queue_free()
	for it in items:
		var b := _chip(str(it[0]))
		b.custom_minimum_size = Vector2(96, 44)
		var sig: Signal = it[1]
		b.pressed.connect(func() -> void: sig.emit())
		_act_row.add_child(b)


func _rarity(r: String) -> Color:
	if r == "":
		return INK
	return VerseCatalog.rarity_color(r)


func _section(t: String) -> Label:
	var l := Label.new()
	l.text = t
	l.add_theme_font_size_override("font_size", 13)
	l.add_theme_color_override("font_color", GOLD)
	return l


func _wrap() -> HFlowContainer:
	var f := HFlowContainer.new()
	f.add_theme_constant_override("h_separation", 8)
	f.add_theme_constant_override("v_separation", 8)
	return f


func _chip(label: String) -> Button:
	var b := Button.new()
	b.text = label
	b.custom_minimum_size = Vector2(0, 40)
	b.add_theme_stylebox_override("normal", _box(Color(1, 1, 1, 0.06), 18))
	b.add_theme_stylebox_override("hover", _box(Color(1, 1, 1, 0.10), 18))
	b.add_theme_stylebox_override("pressed", _box(GOLD, 18))
	b.add_theme_stylebox_override("focus", _box(Color(1, 1, 1, 0.10), 18))
	b.add_theme_color_override("font_color", INK)
	b.add_theme_color_override("font_pressed_color", NAVY)
	b.add_theme_color_override("font_hover_color", INK)
	b.add_theme_font_size_override("font_size", 15)
	return b


func _solid(b: Button) -> void:
	b.add_theme_stylebox_override("normal", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.92), 20))
	b.add_theme_stylebox_override("hover", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.97), 20))
	b.add_theme_stylebox_override("pressed", _box(Color(0.07, 0.14, 0.26, 0.97), 20))
	b.add_theme_stylebox_override("focus", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.97), 20))
	b.add_theme_color_override("font_color", GOLD)
	b.add_theme_color_override("font_hover_color", GOLD_HI)
	b.add_theme_color_override("font_pressed_color", GOLD_HI)
	b.add_theme_font_size_override("font_size", 16)


func _box(bg: Color, radius := 18) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.set_corner_radius_all(radius)
	sb.border_width_bottom = 1
	sb.border_width_top = 1
	sb.border_width_left = 1
	sb.border_width_right = 1
	sb.border_color = GLASS_BORDER
	sb.content_margin_left = 14.0
	sb.content_margin_right = 14.0
	sb.content_margin_top = 8.0
	sb.content_margin_bottom = 8.0
	return sb
