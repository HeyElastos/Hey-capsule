extends Node
## Calm ambient soundscape — synthesized at runtime, zero assets, original.
## Slow breathing chord pads (C · Am · F · G), a soft sub drone, and a sparse
## gentle bell every little while. No squares, no chirps, nothing jumpy —
## just a warm home-metaverse atmosphere.

const RATE := 22050.0
const CHORD_DUR := 8.0
# soft voicings: root · fifth · octave · tenth (midi)
const CHORDS := [
	[48, 55, 60, 64],   # C
	[45, 52, 57, 60],   # Am
	[41, 48, 53, 57],   # F
	[43, 50, 55, 59],   # G
]
const BELL_NOTES := [72, 76, 79, 81, 84]

## "intro" (podium) is a touch brighter; "home" (in-world) is quieter, rarer bells.
var mode := "intro"

var _vol := -15.0
var _bell_min := 4.0
var _bell_max := 7.0

var _player: AudioStreamPlayer
var _pb: AudioStreamGeneratorPlayback
var _n := 0.0
var _bell_at := 3.0
var _bell_n := -1.0
var _bell_f := 523.25
var _pno_amp := 0.0      # >0 enables the soft piano voice (home mode)
var _pno_at := 2.5
var _pno_n := -1.0
var _pno_f := 261.63
var _echo: PackedFloat32Array
var _echo_i := 0


func _ready() -> void:
	if mode == "home":
		_vol = -18.0
		_bell_min = 7.0
		_bell_max = 12.0
		_pno_amp = 0.12
	_echo = PackedFloat32Array()
	_echo.resize(int(RATE * 0.38))
	_player = AudioStreamPlayer.new()
	var gen := AudioStreamGenerator.new()
	gen.mix_rate = RATE
	gen.buffer_length = 0.4
	_player.stream = gen
	_player.volume_db = _vol
	add_child(_player)
	_player.play()
	_pb = _player.get_stream_playback()


func fade_out() -> void:
	set_process(false)
	var tw := create_tween()
	tw.tween_property(_player, "volume_db", -44.0, 0.9)
	tw.tween_callback(func() -> void: queue_free())


static func _freq(m: int) -> float:
	return 440.0 * pow(2.0, (m - 69) / 12.0)


func _process(_delta: float) -> void:
	if _pb == null:
		return
	var frames := _pb.get_frames_available()
	if frames <= 0:
		return
	var buf := PackedVector2Array()
	buf.resize(frames)
	var echo_len := _echo.size()
	for i in frames:
		var t := _n / RATE
		# breathing pad: the chord swells and settles, never fully silent
		var ci := int(t / CHORD_DUR) % CHORDS.size()
		var frac := fposmod(t, CHORD_DUR) / CHORD_DUR
		var breathe := 0.3 + 0.7 * sin(PI * frac)
		var chord: Array = CHORDS[ci]
		var pad := 0.0
		pad += sin(TAU * _freq(int(chord[0])) * t) * 0.050
		pad += sin(TAU * _freq(int(chord[1])) * t) * 0.036
		pad += sin(TAU * _freq(int(chord[2])) * t) * 0.030
		pad += sin(TAU * _freq(int(chord[3])) * t) * 0.026
		pad *= breathe * (1.0 + 0.10 * sin(TAU * 0.11 * t))
		# soft sub drone on the chord root
		var drone := sin(TAU * _freq(int(chord[0]) - 12) * t) * 0.045 * breathe
		var s := pad + drone
		# a sparse, gentle bell — one clean tone with a long soft decay
		if _bell_n >= 0.0:
			var bt := (_n - _bell_n) / RATE
			if bt < 3.0:
				var bell := (sin(TAU * _bell_f * bt) + 0.35 * sin(TAU * _bell_f * 2.0 * bt)) \
					* exp(-bt / 1.1) * 0.085
				s += bell
			else:
				_bell_n = -1.0
		elif t >= _bell_at:
			_bell_at = t + randf_range(_bell_min, _bell_max)
			_bell_n = _n
			_bell_f = _freq(int(BELL_NOTES[randi() % BELL_NOTES.size()]))
		# calm piano: a soft note from the current chord every few seconds —
		# quick felt-hammer attack, long gentle decay, fading upper partials
		if _pno_amp > 0.0:
			if _pno_n >= 0.0:
				var pt := (_n - _pno_n) / RATE
				if pt < 2.6:
					var penv := clampf(pt / 0.004, 0.0, 1.0) * exp(-pt / 0.85)
					var pno := sin(TAU * _pno_f * pt) \
						+ 0.40 * sin(TAU * _pno_f * 2.003 * pt) * exp(-pt / 0.40) \
						+ 0.15 * sin(TAU * _pno_f * 3.0 * pt) * exp(-pt / 0.25)
					s += pno * penv * _pno_amp
				else:
					_pno_n = -1.0
			elif t >= _pno_at:
				_pno_at = t + randf_range(1.8, 3.6)
				_pno_n = _n
				var pch: Array = CHORDS[int(t / CHORD_DUR) % CHORDS.size()]
				_pno_f = _freq(int(pch[randi() % pch.size()]) + 12)
		# gentle echo wash
		var e := _echo[_echo_i]
		var out := s + e * 0.34
		_echo[_echo_i] = s + e * 0.24
		_echo_i = (_echo_i + 1) % echo_len
		var v := tanh(out)
		buf[i] = Vector2(v, v)
		_n += 1.0
	_pb.push_buffer(buf)
