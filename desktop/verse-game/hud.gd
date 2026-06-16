extends CanvasLayer
## Minimal in-world HUD: a clean screen with ONE "···" button (top right) that
## opens the menu — peers info, lighting, hat, invite all live in there. The
## only other element is the chat bar at the bottom. The world stays the star.

signal chat_submitted(text: String)
signal preset_pressed
signal hat_pressed
signal avatar_pressed
signal travel_pressed
signal invite_pressed
signal invite_contact(did: String)

var _menu_btn: Button
var _menu: PanelContainer
var _picker: PanelContainer
var _picker_box: VBoxContainer
var _market: PanelContainer
var _sign: PanelContainer
var _peers_label: Label
var _preset_btn: Button
var _hat_btn: Button
var _invite_btn: Button
var _chat: LineEdit
var _fade: ColorRect
# desktop in-game dock (hidden in app mode, where the Compose dock drives it)
var _dock: PanelContainer
var _dock_light: Button
var _dock_hat: Button
var _dock_peers: Label

# Hey app design tokens (MainActivity.kt) — gold accent, navy ink-on-gold,
# frosted navy sheets. Verse chrome uses the same language as the app.
const GOLD := Color(0.831, 0.722, 0.294)        # #D4B84B
const GOLD_HI := Color(0.910, 0.802, 0.380)
const NAVY := Color(0.035, 0.078, 0.153)        # #091427
const INK := Color(0.918, 0.941, 0.980)         # #EAF0FA
const MUTED := Color(0.553, 0.627, 0.745)       # #8DA0BE
const SHEET := Color(0.047, 0.102, 0.200)       # #0C1A33
const GLASS_BORDER := Color(1, 1, 1, 0.10)


func _ready() -> void:
	layer = 10

	# the single "···" button, top right
	var top := HBoxContainer.new()
	top.set_anchors_and_offsets_preset(Control.PRESET_TOP_WIDE)
	top.offset_left = 12.0
	top.offset_right = -12.0
	top.offset_top = 122.0   # below the Hey app's floating top bar
	top.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(top)
	var sp := Control.new()
	sp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	sp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	top.add_child(sp)
	_menu_btn = Button.new()
	_menu_btn.text = "···"
	_menu_btn.custom_minimum_size = Vector2(54, 40)
	_btn_solid(_menu_btn)
	_menu_btn.pressed.connect(func() -> void: _menu.visible = not _menu.visible)
	top.add_child(_menu_btn)

	# the menu, hidden until "···" is tapped
	_menu = PanelContainer.new()
	_menu.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.96)))
	_menu.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	_menu.offset_top = 174.0
	_menu.offset_right = -12.0
	_menu.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	_menu.grow_vertical = Control.GROW_DIRECTION_END
	_menu.visible = false
	add_child(_menu)
	var mv := VBoxContainer.new()
	mv.add_theme_constant_override("separation", 8)
	_menu.add_child(mv)
	_peers_label = Label.new()
	_peers_label.text = "1 here"
	_peers_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_peers_label.add_theme_font_size_override("font_size", 14)
	_peers_label.add_theme_color_override("font_color", MUTED)
	mv.add_child(_peers_label)
	_preset_btn = Button.new()
	_preset_btn.text = "Light · Day"
	_style_button(_preset_btn)
	_preset_btn.pressed.connect(func() -> void: preset_pressed.emit())
	mv.add_child(_preset_btn)
	_hat_btn = Button.new()
	_hat_btn.text = "Hat"
	_style_button(_hat_btn)
	_hat_btn.pressed.connect(func() -> void: hat_pressed.emit())
	mv.add_child(_hat_btn)
	var avatar_btn := Button.new()
	avatar_btn.text = "Avatar"
	_style_button(avatar_btn)
	avatar_btn.pressed.connect(func() -> void:
		close_menu()
		avatar_pressed.emit())
	mv.add_child(avatar_btn)
	var travel_btn := Button.new()
	travel_btn.text = "Travel"
	_style_button(travel_btn)
	travel_btn.pressed.connect(func() -> void:
		close_menu()
		travel_pressed.emit())
	mv.add_child(travel_btn)
	var market_btn := Button.new()
	market_btn.text = "Marketplace"
	_style_button(market_btn)
	market_btn.pressed.connect(func() -> void:
		_menu.visible = false
		_market.visible = true)
	mv.add_child(market_btn)
	_invite_btn = Button.new()
	_invite_btn.text = "Invite friend"
	_btn_gold(_invite_btn)
	_invite_btn.pressed.connect(func() -> void: invite_pressed.emit())
	mv.add_child(_invite_btn)

	# the contact picker (filled by show_picker), same spot as the menu
	_picker = PanelContainer.new()
	_picker.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.96)))
	_picker.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	_picker.offset_top = 174.0
	_picker.offset_right = -12.0
	_picker.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	_picker.grow_vertical = Control.GROW_DIRECTION_END
	_picker.visible = false
	add_child(_picker)
	_picker_box = VBoxContainer.new()
	_picker_box.add_theme_constant_override("separation", 8)
	_picker.add_child(_picker_box)

	# Marketplace sheet — Elacity (rust capsule), coming soon: buy 3D models
	# as .ddrm, owned in your namespace on this device.
	_market = PanelContainer.new()
	_market.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.96)))
	_market.set_anchors_and_offsets_preset(Control.PRESET_TOP_RIGHT)
	_market.offset_top = 174.0
	_market.offset_right = -12.0
	_market.grow_horizontal = Control.GROW_DIRECTION_BEGIN
	_market.grow_vertical = Control.GROW_DIRECTION_END
	_market.visible = false
	add_child(_market)
	var mkv := VBoxContainer.new()
	mkv.add_theme_constant_override("separation", 8)
	_market.add_child(mkv)
	var mk_title := Label.new()
	mk_title.text = "Elacity Marketplace"
	mk_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	mk_title.add_theme_font_size_override("font_size", 16)
	mk_title.add_theme_color_override("font_color", GOLD)
	mkv.add_child(mk_title)
	var mk_soon := Label.new()
	mk_soon.text = "coming soon"
	mk_soon.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	mk_soon.add_theme_font_size_override("font_size", 12)
	mk_soon.add_theme_color_override("font_color", MUTED)
	mkv.add_child(mk_soon)
	var mk_body := Label.new()
	mk_body.text = "Truly buy 3D models — hats, furniture, decor — as .ddrm files. The token you own releases the key; the model lives in your namespace, on your device."
	mk_body.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	mk_body.custom_minimum_size = Vector2(252, 0)
	mk_body.add_theme_font_size_override("font_size", 13)
	mk_body.add_theme_color_override("font_color", INK)
	mkv.add_child(mk_body)
	var mk_back := Button.new()
	mk_back.text = "Back"
	_style_button(mk_back)
	mk_back.pressed.connect(func() -> void: _market.visible = false)
	mkv.add_child(mk_back)

	# the signpost's note (opened by tapping the sign in the yard)
	_sign = PanelContainer.new()
	_sign.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.97)))
	_sign.set_anchors_and_offsets_preset(Control.PRESET_CENTER)
	_sign.grow_horizontal = Control.GROW_DIRECTION_BOTH
	_sign.grow_vertical = Control.GROW_DIRECTION_BOTH
	_sign.visible = false
	add_child(_sign)
	var sgv := VBoxContainer.new()
	sgv.add_theme_constant_override("separation", 10)
	_sign.add_child(sgv)
	var sg_title := Label.new()
	sg_title.text = "Welcome to your own Heyverse"
	sg_title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	sg_title.add_theme_font_size_override("font_size", 18)
	sg_title.add_theme_color_override("font_color", GOLD)
	sgv.add_child(sg_title)
	var sg_sub := Label.new()
	sg_sub.text = "powered by ElastOS"
	sg_sub.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	sg_sub.add_theme_font_size_override("font_size", 14)
	sg_sub.add_theme_color_override("font_color", MUTED)
	sgv.add_child(sg_sub)
	var sg_close := Button.new()
	sg_close.text = "Close"
	_btn_gold(sg_close)
	sg_close.pressed.connect(func() -> void: _sign.visible = false)
	sgv.add_child(sg_close)

	# chat bar
	var bottom := HBoxContainer.new()
	bottom.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	bottom.offset_left = 12.0
	bottom.offset_right = -12.0
	# just above the Hey app's floating dock
	bottom.offset_top = -178.0
	bottom.offset_bottom = -126.0
	add_child(bottom)
	var spl := Control.new()
	spl.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spl.mouse_filter = Control.MOUSE_FILTER_IGNORE
	bottom.add_child(spl)
	var bar := HBoxContainer.new()
	bar.add_theme_constant_override("separation", 8)
	bottom.add_child(bar)
	_chat = LineEdit.new()
	_chat.placeholder_text = "say something..."
	_chat.custom_minimum_size = Vector2(300, 44)
	_chat.add_theme_stylebox_override("normal", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.88), 20))
	_chat.add_theme_stylebox_override("focus", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.96), 20))
	_chat.add_theme_color_override("font_color", INK)
	_chat.add_theme_color_override("font_placeholder_color", MUTED)
	_chat.add_theme_font_size_override("font_size", 16)
	_chat.text_submitted.connect(_submit)
	bar.add_child(_chat)
	var send := Button.new()
	send.text = "Send"
	_btn_gold(send)
	send.pressed.connect(func() -> void: _submit(_chat.text))
	bar.add_child(send)
	var spr := Control.new()
	spr.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	spr.mouse_filter = Control.MOUSE_FILTER_IGNORE
	bottom.add_child(spr)

	# ── desktop in-game dock: the Hey-styled bar with the same options as the
	# "···" menu, always visible + mouse-friendly. Replaces "···" on desktop and
	# is hidden in app mode (the Compose dock drives the game there).
	_menu_btn.visible = false
	var dock_row := HBoxContainer.new()
	dock_row.set_anchors_and_offsets_preset(Control.PRESET_BOTTOM_WIDE)
	dock_row.offset_top = -86.0
	dock_row.offset_bottom = -34.0
	add_child(dock_row)
	var dlsp := Control.new()
	dlsp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	dlsp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	dock_row.add_child(dlsp)
	_dock = PanelContainer.new()
	_dock.add_theme_stylebox_override("panel", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.92), 26))
	dock_row.add_child(_dock)
	var dh := HBoxContainer.new()
	dh.add_theme_constant_override("separation", 4)
	_dock.add_child(dh)
	_dock_peers = Label.new()
	_dock_peers.text = "1 here"
	_dock_peers.vertical_alignment = VERTICAL_ALIGNMENT_CENTER
	_dock_peers.add_theme_font_size_override("font_size", 13)
	_dock_peers.add_theme_color_override("font_color", MUTED)
	dh.add_child(_dock_peers)
	dh.add_child(_dock_gap())
	var d_avatar := _dock_btn("Avatar")
	d_avatar.pressed.connect(func() -> void:
		close_menu()
		avatar_pressed.emit())
	dh.add_child(d_avatar)
	_dock_light = _dock_btn("Day")
	_dock_light.pressed.connect(func() -> void: preset_pressed.emit())
	dh.add_child(_dock_light)
	_dock_hat = _dock_btn("Hat")
	_dock_hat.pressed.connect(func() -> void: hat_pressed.emit())
	dh.add_child(_dock_hat)
	var d_travel := _dock_btn("Travel")
	d_travel.pressed.connect(func() -> void:
		close_menu()
		travel_pressed.emit())
	dh.add_child(d_travel)
	var d_market := _dock_btn("Market")
	d_market.pressed.connect(func() -> void:
		_picker.visible = false
		_market.visible = not _market.visible)
	dh.add_child(d_market)
	dh.add_child(_dock_gap())
	var d_invite := Button.new()
	d_invite.text = "Invite"
	d_invite.custom_minimum_size = Vector2(0, 40)
	_btn_gold(d_invite)
	d_invite.pressed.connect(func() -> void: invite_pressed.emit())
	dh.add_child(d_invite)
	var drsp := Control.new()
	drsp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	drsp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	dock_row.add_child(drsp)

	# full-screen fade for door transitions
	_fade = ColorRect.new()
	_fade.color = Color(0, 0, 0, 0)
	_fade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_fade.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_fade)


func set_peer_count(n: int) -> void:
	_peers_label.text = "%d here" % n
	if _dock_peers:
		_dock_peers.text = "%d here" % n


## Quick black fade: out, run `mid` (the teleport), back in.
func fade(mid: Callable) -> void:
	var tw := create_tween()
	tw.tween_property(_fade, "color:a", 1.0, 0.22)
	tw.tween_callback(mid)
	tw.tween_interval(0.08)
	tw.tween_property(_fade, "color:a", 0.0, 0.28)


func close_menu() -> void:
	_menu.visible = false
	_picker.visible = false
	_market.visible = false


## Inside the Hey app the dock carries all controls — hide the in-game menu.
func set_app_mode() -> void:
	_menu_btn.visible = false
	if _dock:
		_dock.visible = false
	close_menu()


func show_sign() -> void:
	_sign.visible = true


## Tap-to-choose: list your contacts; the ones already here are marked.
func show_picker(contact_list: Array, present_dids: Array) -> void:
	_menu.visible = false
	for child in _picker_box.get_children():
		child.queue_free()
	var title := Label.new()
	title.text = "invite to your world"
	title.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	title.add_theme_font_size_override("font_size", 13)
	title.add_theme_color_override("font_color", MUTED)
	_picker_box.add_child(title)
	for c in contact_list:
		var cd: Dictionary = c
		var did := str(cd["did"])
		var b := Button.new()
		var here := present_dids.has(did)
		b.text = str(cd["name"]) + (" · here" if here else "")
		b.disabled = here
		_style_button(b)
		b.pressed.connect(func() -> void:
			invite_contact.emit(did)
			close_menu())
		_picker_box.add_child(b)
	var back := Button.new()
	back.text = "Back"
	_style_button(back)
	back.pressed.connect(func() -> void: _picker.visible = false)
	_picker_box.add_child(back)
	_picker.visible = true


func set_preset_name(s: String) -> void:
	_preset_btn.text = "Light · " + s
	if _dock_light:
		_dock_light.text = s


func set_hat_name(s: String) -> void:
	_hat_btn.text = "Hat · " + s if s != "" else "Hat"


func _submit(t: String) -> void:
	var s := t.strip_edges()
	_chat.text = ""
	_chat.release_focus()
	if s != "":
		chat_submitted.emit(s)


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


## In-panel button: subtle white-alpha glass row on the sheet.
func _style_button(b: Button) -> void:
	b.add_theme_stylebox_override("normal", _box(Color(1, 1, 1, 0.06), 20))
	b.add_theme_stylebox_override("hover", _box(Color(1, 1, 1, 0.10), 20))
	b.add_theme_stylebox_override("pressed", _box(Color(1, 1, 1, 0.15), 20))
	b.add_theme_stylebox_override("focus", _box(Color(1, 1, 1, 0.10), 20))
	b.add_theme_color_override("font_color", INK)
	b.add_theme_color_override("font_hover_color", INK)
	b.add_theme_color_override("font_pressed_color", INK)
	b.add_theme_font_size_override("font_size", 16)


## On-world button (floats over the 3D): solid frosted navy, like the dock.
func _btn_solid(b: Button) -> void:
	b.add_theme_stylebox_override("normal", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.92), 20))
	b.add_theme_stylebox_override("hover", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.97), 20))
	b.add_theme_stylebox_override("pressed", _box(Color(0.07, 0.14, 0.26, 0.97), 20))
	b.add_theme_stylebox_override("focus", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.97), 20))
	b.add_theme_color_override("font_color", GOLD)
	b.add_theme_color_override("font_hover_color", GOLD_HI)
	b.add_theme_color_override("font_pressed_color", GOLD_HI)
	b.add_theme_font_size_override("font_size", 16)


## Primary action: Hey's gold pill with navy ink.
func _btn_gold(b: Button) -> void:
	b.add_theme_stylebox_override("normal", _box(GOLD, 20))
	b.add_theme_stylebox_override("hover", _box(GOLD_HI, 20))
	b.add_theme_stylebox_override("pressed", _box(Color(0.72, 0.62, 0.25), 20))
	b.add_theme_stylebox_override("focus", _box(GOLD_HI, 20))
	b.add_theme_color_override("font_color", NAVY)
	b.add_theme_color_override("font_hover_color", NAVY)
	b.add_theme_color_override("font_pressed_color", NAVY)
	b.add_theme_font_size_override("font_size", 16)


## A dock button: transparent until hover, gold text — sits on the dock panel.
func _dock_btn(label: String) -> Button:
	var b := Button.new()
	b.text = label
	b.custom_minimum_size = Vector2(0, 40)
	var normal := StyleBoxFlat.new()
	normal.bg_color = Color(1, 1, 1, 0.0)
	normal.set_corner_radius_all(16)
	normal.content_margin_left = 14.0
	normal.content_margin_right = 14.0
	normal.content_margin_top = 8.0
	normal.content_margin_bottom = 8.0
	var hov := normal.duplicate() as StyleBoxFlat
	hov.bg_color = Color(1, 1, 1, 0.10)
	var pr := normal.duplicate() as StyleBoxFlat
	pr.bg_color = Color(1, 1, 1, 0.16)
	b.add_theme_stylebox_override("normal", normal)
	b.add_theme_stylebox_override("focus", normal)
	b.add_theme_stylebox_override("hover", hov)
	b.add_theme_stylebox_override("pressed", pr)
	b.add_theme_color_override("font_color", GOLD)
	b.add_theme_color_override("font_hover_color", GOLD_HI)
	b.add_theme_color_override("font_pressed_color", GOLD_HI)
	b.add_theme_font_size_override("font_size", 15)
	return b


## A small spacer between dock groups.
func _dock_gap() -> Control:
	var c := Control.new()
	c.custom_minimum_size = Vector2(8, 0)
	c.mouse_filter = Control.MOUSE_FILTER_IGNORE
	return c
