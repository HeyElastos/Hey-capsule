class_name VerseSpinner
extends Node3D
## Tiny motion driver for world dressing: spins its children around Y and/or
## bobs gently up and down. World packs load this from the main pack to make
## rings rotate, billboards float, trams orbit — life without physics.

var speed := 0.5    # rad/s around Y (0 = no spin)
var bob := 0.0      # meters of gentle vertical float (0 = none)

var _t := 0.0
var _base_y := 0.0


func _ready() -> void:
	_base_y = position.y
	_t = absf(position.x) * 0.7 + absf(position.z) * 0.3  # desync siblings


func _process(delta: float) -> void:
	if speed != 0.0:
		rotate_y(speed * delta)
	if bob > 0.0:
		_t += delta
		position.y = _base_y + sin(_t * 0.9) * bob
