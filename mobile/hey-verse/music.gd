extends Node
## Calm ambient soundscape — synthesized at runtime, zero assets, original.
## Slow breathing chord pads (C · Am · F · G), a soft sub drone, and a sparse
## gentle bell every little while. No squares, no chirps, nothing jumpy —
## just a warm home-metaverse atmosphere.

const RATE := 22050.0
# soft voicings: root · fifth · octave · tenth (midi)
const CHORDS := [
	[48, 55, 60, 64],   # C
	[45, 52, 57, 60],   # Am
	[41, 48, 53, 57],   # F
	[43, 50, 55, 59],   # G
]
const BELL_NOTES := [72, 76, 79, 81, 84]

## "intro" (podium) is a touch brighter; "home" (in-world) is quieter, rarer
## bells + soft piano; "city" is calm-futuristic — glassy chimes, spacier echo.
var mode := "intro"

# one shared level for every land/world — intro, home and city all play at
# the same volume so switching lands never feels louder or quieter
var _vol := -2.0
var _bell_min := 4.0
var _bell_max := 7.0
var _bell_metal := false
var _bell_p2 := 0.35
var _chord_dur := 8.0
var _chords: Array = CHORDS
var _echo_mix := 0.34
var _echo_fb := 0.24
# city groove (calm robot-electro): soft kick, whisper hats, smooth bass, arp
var _groove := false
const BEAT := 0.714              # ~84 BPM
const BPAT := [0, -1, 0, -1, 1, -1, 0, -1]   # eighth-note bass degrees (-1 = rest)
var _pad_amp := 1.0
# forest layer (home): sparse soft birdsong
var _forest := false
var _bird_at := 5.0
var _bird_n := -1.0
var _bird_f := 1900.0

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
		# calm forest home: slow pads + soft piano + sparse little birdsong
		_bell_min = 9.0
		_bell_max = 15.0
		_pno_amp = 0.12
		_chord_dur = 10.0
		_forest = true
	elif mode == "city":
		# calm robot-electro with a city pulse: soft beat + bass + sparse arp
		# under cooler minor-leaning pads; glassy chimes only now and then
		_bell_min = 1.0e9   # no bells in the city — the groove carries it
		_bell_max = 1.0e9
		_bell_at = 1.0e9    # …including the very first one
		_bell_metal = true
		_bell_p2 = 0.2
		_chord_dur = BEAT * 16.0   # one chord per 4 bars
		_chords = [CHORDS[1], CHORDS[2], CHORDS[0], CHORDS[3]]   # Am F C G
		_groove = true
		_pad_amp = 0.75
		_echo_mix = 0.4
		_echo_fb = 0.28
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
	tw.tween_property(_player, "volume_db", -44.0, 0.45)
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
		var ci := int(t / _chord_dur) % _chords.size()
		var frac := fposmod(t, _chord_dur) / _chord_dur
		var breathe := 0.3 + 0.7 * sin(PI * frac)
		var chord: Array = _chords[ci]
		var pad := 0.0
		pad += sin(TAU * _freq(int(chord[0])) * t) * 0.050
		pad += sin(TAU * _freq(int(chord[1])) * t) * 0.036
		pad += sin(TAU * _freq(int(chord[2])) * t) * 0.030
		pad += sin(TAU * _freq(int(chord[3])) * t) * 0.026
		pad *= breathe * (1.0 + 0.10 * sin(TAU * 0.11 * t)) * _pad_amp
		# soft sub drone on the chord root
		var drone := sin(TAU * _freq(int(chord[0]) - 12) * t) * 0.045 * breathe
		var s := pad + drone
		# the city pulse: calm robot-electro groove
		if _groove:
			var beat_t := fposmod(t, BEAT)
			# soft kick: a sine thump with a quick pitch drop
			if beat_t < 0.2:
				s += sin(TAU * (52.0 - 58.0 * beat_t) * beat_t) * exp(-beat_t / 0.08) * 0.12
			# whispered hat on the offbeat
			var off_t := fposmod(t - BEAT * 0.5, BEAT)
			if off_t < 0.04:
				s += (randf() * 2.0 - 1.0) * exp(-off_t / 0.011) * 0.028
			# smooth bass riding an eighth-note pattern
			var e8 := int(t / (BEAT * 0.5)) % BPAT.size()
			var deg: int = BPAT[e8]
			if deg >= 0:
				var bt8 := fposmod(t, BEAT * 0.5)
				s += sin(TAU * _freq(int(chord[deg]) - 12) * t) * exp(-bt8 / 0.2) * 0.08
			# sparse plucky arp, every other bar (the calm robot voice)
			var bar := int(t / (BEAT * 4.0))
			if bar % 2 == 1:
				var s16 := int(t / (BEAT * 0.25)) % 4
				var at := fposmod(t, BEAT * 0.25)
				s += sin(TAU * _freq(int(chord[s16]) + 12) * t) * exp(-at / 0.045) * 0.045
		# a sparse, gentle bell — one clean tone with a long soft decay
		if _bell_n >= 0.0:
			var bt := (_n - _bell_n) / RATE
			if bt < 3.0:
				# metal mode: inharmonic 2.76x partial = glassy sci-fi chime
				var p2 := 2.76 if _bell_metal else 2.0
				var bell := (sin(TAU * _bell_f * bt) \
					+ _bell_p2 * sin(TAU * _bell_f * p2 * bt) * exp(-bt / (0.5 if _bell_metal else 1.1))) \
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
				var pch: Array = CHORDS[int(t / _chord_dur) % CHORDS.size()]
				_pno_f = _freq(int(pch[randi() % pch.size()]) + 12)
		# little birds in the trees (home): 3-note rising warbles, far away
		if _forest:
			if _bird_n >= 0.0:
				var btt := (_n - _bird_n) / RATE
				if btt < 0.55:
					var ci2 := int(btt / 0.17)
					var ct := fposmod(btt, 0.17)
					if ct < 0.09 and ci2 < 3:
						var f2 := _bird_f * (1.0 + 0.06 * float(ci2))
						s += sin(TAU * f2 * ct * (1.0 + 6.0 * ct)) * exp(-ct / 0.03) * 0.032
				else:
					_bird_n = -1.0
			elif t >= _bird_at:
				_bird_at = t + randf_range(6.0, 13.0)
				_bird_n = _n
				_bird_f = randf_range(1650.0, 2350.0)
		# gentle echo wash
		var e := _echo[_echo_i]
		var out := s + e * _echo_mix
		_echo[_echo_i] = s + e * _echo_fb
		_echo_i = (_echo_i + 1) % echo_len
		# drive into the soft-clip: louder on phone speakers, tanh keeps it safe
		var v := tanh(out * 1.7)
		buf[i] = Vector2(v, v)
		_n += 1.0
	_pb.push_buffer(buf)
