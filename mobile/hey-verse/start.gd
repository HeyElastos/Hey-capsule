extends CanvasLayer
## Avatar-first start screen: your robot stands on a podium in the dark.
## Choose "Edit avatar" (cycle hat / accent color) or "Enter world".

signal enter_world
signal hat_cycle
signal accent_cycle
signal body_cycle
signal eyes_cycle
signal fins_cycle

var _main: VBoxContainer
var _edit: VBoxContainer
var _hat_btn: Button
var _fade: ColorRect
var _music: Node
var _status: Label
var _quip_t := 0.0
var _quip_i := 0

const QUIPS: Array[String] = [
	"polishing the visor…",
	"charging happy circuits…",
	"tuning the antenna…",
	"counting clouds…",
	"watering the pixels…",
	"warming up the lawn…",
	"teaching boots to walk…",
	"practicing the wave…",
]

# Hey app design tokens — same language as the app chrome.
const GOLD := Color(0.831, 0.722, 0.294)
const GOLD_HI := Color(0.910, 0.802, 0.380)
const NAVY := Color(0.035, 0.078, 0.153)
const INK := Color(0.918, 0.941, 0.980)
const MUTED := Color(0.553, 0.627, 0.745)
const SHEET := Color(0.047, 0.102, 0.200)
const GLASS_BORDER := Color(1, 1, 1, 0.10)


func _ready() -> void:
	layer = 12
	var root := MarginContainer.new()
	root.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	root.add_theme_constant_override("margin_left", 18)
	root.add_theme_constant_override("margin_right", 18)
	root.add_theme_constant_override("margin_top", 26)
	# clear the Hey app's floating dock at the bottom of the screen
	root.add_theme_constant_override("margin_bottom", 170)
	root.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(root)

	var col := VBoxContainer.new()
	col.mouse_filter = Control.MOUSE_FILTER_IGNORE
	root.add_child(col)

	# (no in-game title — the Hey app's top bar already says "Hey Verse")
	var sp := Control.new()
	sp.size_flags_vertical = Control.SIZE_EXPAND_FILL
	sp.mouse_filter = Control.MOUSE_FILTER_IGNORE
	col.add_child(sp)

	# the cute status line sits just above the buttons
	_status = Label.new()
	_status.text = QUIPS[0]
	_status.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	_status.add_theme_font_size_override("font_size", 13)
	_status.add_theme_color_override("font_color", Color(MUTED.r, MUTED.g, MUTED.b, 0.85))
	col.add_child(_status)

	_main = VBoxContainer.new()
	_main.add_theme_constant_override("separation", 12)
	col.add_child(_main)
	_main.add_child(_row(_button("Edit avatar", func() -> void: _set_mode(true))))
	_main.add_child(_row(_gold_button("Enter world", func() -> void: enter_world.emit())))

	_edit = VBoxContainer.new()
	_edit.add_theme_constant_override("separation", 12)
	_edit.visible = false
	col.add_child(_edit)
	_hat_btn = _button("Hat", func() -> void: hat_cycle.emit())
	_edit.add_child(_row(_hat_btn))
	_edit.add_child(_row(_button("Body", func() -> void: body_cycle.emit())))
	_edit.add_child(_row(_button("Eyes", func() -> void: eyes_cycle.emit())))
	_edit.add_child(_row(_button("Antenna", func() -> void: fins_cycle.emit())))
	_edit.add_child(_row(_button("Color", func() -> void: accent_cycle.emit())))
	_edit.add_child(_row(_gold_button("Done", func() -> void: _set_mode(false))))

	_fade = ColorRect.new()
	_fade.color = Color(0, 0, 0, 0)
	_fade.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	_fade.mouse_filter = Control.MOUSE_FILTER_IGNORE
	add_child(_fade)

	# fuzzy robot intro tune (procedural — see music.gd)
	_music = preload("res://music.gd").new()
	add_child(_music)

	# buttons pop in with a soft stagger
	_main.modulate = Color(1, 1, 1, 0)
	var tw := create_tween()
	tw.tween_interval(0.25)
	tw.tween_property(_main, "modulate:a", 1.0, 0.45)


func _process(delta: float) -> void:
	_quip_t -= delta
	if _quip_t <= 0.0:
		_quip_t = 2.4
		_quip_i = (_quip_i + 1) % QUIPS.size()
		_status.text = QUIPS[_quip_i]


func stop_music() -> void:
	if _music:
		_music.fade_out()
		_music = null


func set_hat_label(s: String) -> void:
	_hat_btn.text = "Hat · " + s if s != "" else "Hat"


## Black fade used for the enter-world transition (the HUD is hidden here).
func fade(mid: Callable) -> void:
	var tw := create_tween()
	tw.tween_property(_fade, "color:a", 1.0, 0.22)
	tw.tween_callback(mid)
	tw.tween_interval(0.08)
	tw.tween_property(_fade, "color:a", 0.0, 0.28)


func _set_mode(editing: bool) -> void:
	_main.visible = not editing
	_edit.visible = editing


func _row(b: Button) -> HBoxContainer:
	var h := HBoxContainer.new()
	h.mouse_filter = Control.MOUSE_FILTER_IGNORE
	var l := Control.new()
	l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	l.mouse_filter = Control.MOUSE_FILTER_IGNORE
	h.add_child(l)
	h.add_child(b)
	var r := Control.new()
	r.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	r.mouse_filter = Control.MOUSE_FILTER_IGNORE
	h.add_child(r)
	return h


func _box(bg: Color) -> StyleBoxFlat:
	var sb := StyleBoxFlat.new()
	sb.bg_color = bg
	sb.set_corner_radius_all(20)
	sb.border_width_bottom = 1
	sb.border_width_top = 1
	sb.border_width_left = 1
	sb.border_width_right = 1
	sb.border_color = GLASS_BORDER
	sb.content_margin_left = 26.0
	sb.content_margin_right = 26.0
	sb.content_margin_top = 13.0
	sb.content_margin_bottom = 13.0
	return sb


## Secondary: frosted navy sheet, ink text.
func _button(text: String, on_press: Callable) -> Button:
	var b := Button.new()
	b.text = text
	b.custom_minimum_size = Vector2(230, 52)
	b.add_theme_stylebox_override("normal", _box(Color(SHEET.r, SHEET.g, SHEET.b, 0.92)))
	b.add_theme_stylebox_override("hover", _box(Color(0.07, 0.14, 0.26, 0.96)))
	b.add_theme_stylebox_override("pressed", _box(Color(0.09, 0.17, 0.30, 0.96)))
	b.add_theme_stylebox_override("focus", _box(Color(0.07, 0.14, 0.26, 0.96)))
	b.add_theme_color_override("font_color", INK)
	b.add_theme_color_override("font_hover_color", INK)
	b.add_theme_color_override("font_pressed_color", INK)
	b.add_theme_font_size_override("font_size", 18)
	b.pressed.connect(on_press)
	return b


## Primary: Hey's gold pill with navy ink.
func _gold_button(text: String, on_press: Callable) -> Button:
	var b := Button.new()
	b.text = text
	b.custom_minimum_size = Vector2(230, 52)
	b.add_theme_stylebox_override("normal", _box(GOLD))
	b.add_theme_stylebox_override("hover", _box(GOLD_HI))
	b.add_theme_stylebox_override("pressed", _box(Color(0.72, 0.62, 0.25)))
	b.add_theme_stylebox_override("focus", _box(GOLD_HI))
	b.add_theme_color_override("font_color", NAVY)
	b.add_theme_color_override("font_hover_color", NAVY)
	b.add_theme_color_override("font_pressed_color", NAVY)
	b.add_theme_font_size_override("font_size", 18)
	b.pressed.connect(on_press)
	return b
