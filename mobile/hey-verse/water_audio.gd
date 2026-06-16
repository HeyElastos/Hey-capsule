class_name VerseWaterAudio
extends AudioStreamPlayer3D
## Fountain TINKLING, synthesized live — sparse little droplet plinks with a
## faint wet shimmer underneath (no river rush). Positional: audible when you
## walk up to the fountain, gone a few steps away. Listener = the camera, so
## ranges are tuned for the game's high camera.

const RATE := 11025.0
const VOICES := 3

var _pb: AudioStreamGeneratorPlayback
var _n := 0.0
var _lp := 0.0
var _next_drop := 0.3
var _drop_n: Array[float] = []
var _drop_f: Array[float] = []
var _drop_a: Array[float] = []
var _voice := 0


func _ready() -> void:
	for i in VOICES:
		_drop_n.append(-1.0)
		_drop_f.append(900.0)
		_drop_a.append(0.2)
	var gen := AudioStreamGenerator.new()
	gen.mix_rate = RATE
	gen.buffer_length = 0.3
	stream = gen
	volume_db = -10.0
	unit_size = 5.0
	max_distance = 12.0
	play()
	_pb = get_stream_playback()


func _process(_delta: float) -> void:
	if _pb == null:
		return
	var frames := _pb.get_frames_available()
	if frames <= 0:
		return
	var buf := PackedVector2Array()
	buf.resize(frames)
	for i in frames:
		var t := _n / RATE
		# the faintest wet shimmer (texture only, NOT a river)
		var noise := randf() * 2.0 - 1.0
		_lp += 0.3 * (noise - _lp)
		var s := _lp * 0.05
		# droplet plinks: short bright sines with a tiny downward bend
		if t >= _next_drop:
			_next_drop = t + randf_range(0.07, 0.34)
			_drop_n[_voice] = _n
			_drop_f[_voice] = randf_range(700.0, 1900.0)
			_drop_a[_voice] = randf_range(0.10, 0.30)
			_voice = (_voice + 1) % VOICES
		for v in VOICES:
			var dn: float = _drop_n[v]
			if dn < 0.0:
				continue
			var dt := (_n - dn) / RATE
			if dt > 0.12:
				_drop_n[v] = -1.0
				continue
			var f: float = _drop_f[v]
			s += sin(TAU * f * dt * (1.0 - 1.3 * dt)) * exp(-dt / 0.032) * _drop_a[v]
		var out := clampf(s, -1.0, 1.0)
		buf[i] = Vector2(out, out)
		_n += 1.0
	_pb.push_buffer(buf)
