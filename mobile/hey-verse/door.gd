class_name VerseDoor
extends Node3D
## Sliding glass door: the two panels glide apart when the player (camera)
## comes near and glide shut again as they leave. No physics — pure distance
## check against the active camera, which always tracks the player.

var open_dist := 5.5
var slide := 1.1

var _left: Node3D
var _right: Node3D
var _base_l := Vector3.ZERO
var _base_r := Vector3.ZERO
var _open := false
var _tw: Tween


func setup(left: Node3D, right: Node3D) -> void:
	_left = left
	_right = right
	_base_l = left.position
	_base_r = right.position


func _process(_delta: float) -> void:
	if _left == null or _right == null:
		return
	var cam := get_viewport().get_camera_3d()
	if cam == null:
		return
	var near := global_position.distance_to(cam.global_position) < open_dist
	if near == _open:
		return
	_open = near
	if _tw:
		_tw.kill()
	_tw = create_tween().set_parallel()
	var off := slide if near else 0.0
	_tw.tween_property(_left, "position", _base_l + Vector3(-off, 0, 0), 0.38) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)
	_tw.tween_property(_right, "position", _base_r + Vector3(off, 0, 0), 0.38) \
		.set_trans(Tween.TRANS_SINE).set_ease(Tween.EASE_OUT)
