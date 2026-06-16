class_name VerseWalker
extends Node
## Calm stroll driver for custom figures (Sash, future characters): swings
## limb PIVOT nodes named LegL/LegR/ArmL/ArmR inside `rig` with a soft step
## bounce — and breathes: walks a while, then stands still a few seconds
## (pausing its orbit spinner too), then wanders on.

var speed := 4.8          # step cadence (rad/s of the swing phase)
var amp := 0.38           # leg swing amplitude
var orbit: Node3D = null  # optional spinner carrying the figure
var orbit_speed := 0.0    # restored when walking resumes

var _rig: Node3D
var _ll: Node3D
var _lr: Node3D
var _al: Node3D
var _ar: Node3D
var _base_y := 0.0
var _t := 0.0
var _walking := true
var _phase_t := 9.0


func setup(rig: Node3D) -> void:
	_rig = rig
	_ll = rig.get_node_or_null("LegL")
	_lr = rig.get_node_or_null("LegR")
	_al = rig.get_node_or_null("ArmL")
	_ar = rig.get_node_or_null("ArmR")
	_base_y = rig.position.y


func _process(delta: float) -> void:
	if _rig == null:
		return
	_phase_t -= delta
	if _phase_t <= 0.0:
		_walking = not _walking
		_phase_t = randf_range(8.0, 14.0) if _walking else randf_range(3.5, 7.0)
		if orbit != null:
			orbit.set("speed", orbit_speed if _walking else 0.0)
	if _walking:
		_t += delta
		var s := sin(_t * speed)
		if _ll:
			_ll.rotation.x = s * amp
		if _lr:
			_lr.rotation.x = -s * amp
		if _al:
			_al.rotation.x = -s * amp * 0.55
		if _ar:
			_ar.rotation.x = s * amp * 0.55
		_rig.position.y = _base_y + absf(s) * 0.03
	else:
		# settle into a relaxed stand
		if _ll:
			_ll.rotation.x = lerpf(_ll.rotation.x, 0.0, 6.0 * delta)
			_lr.rotation.x = lerpf(_lr.rotation.x, 0.0, 6.0 * delta)
		if _al:
			_al.rotation.x = lerpf(_al.rotation.x, 0.0, 6.0 * delta)
			_ar.rotation.x = lerpf(_ar.rotation.x, 0.0, 6.0 * delta)
		_rig.position.y = lerpf(_rig.position.y, _base_y, 6.0 * delta)
