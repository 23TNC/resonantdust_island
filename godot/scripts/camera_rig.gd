extends Node3D

## Don't Starve's camera: a fixed downward angle, a narrow field of view so the
## perspective stays nearly flat, and free yaw around the player. The narrow FOV
## matters — a wide one makes billboards visibly swivel as they cross the screen.
##
## Node layout is a gimbal, which keeps yaw and pitch from fighting:
##   CameraRig (this node, follows the player and yaws)
##     └── Pitch (tilts down)
##           └── Camera3D (pushed back along +Z by `distance`)

@export var target: Node3D
@export_range(4.0, 60.0) var distance := 19.0
@export_range(-89.0, 0.0) var pitch_degrees := -40.0
## Metres above the target's feet that the rig actually tracks, so the camera
## frames the character's chest rather than the ground under them.
@export var height_offset := 1.0
@export var rotate_speed_degrees := 110.0
@export var zoom_step := 2.0
@export var min_distance := 10.0
@export var max_distance := 40.0
## Higher is snappier. Exponential smoothing, so it is framerate independent.
@export var follow_smoothing := 9.0

@onready var _pitch: Node3D = $Pitch
@onready var _camera: Camera3D = $Pitch/Camera3D

func _ready() -> void:
	_pitch.rotation_degrees.x = pitch_degrees
	_camera.position = Vector3(0.0, 0.0, distance)
	if target != null:
		global_position = _desired_position()

func _process(delta: float) -> void:
	var turn := Input.get_axis("cam_rotate_right", "cam_rotate_left")
	if turn != 0.0:
		rotate_y(deg_to_rad(turn * rotate_speed_degrees * delta))

	if Input.is_action_just_pressed("cam_zoom_in"):
		distance = clampf(distance - zoom_step, min_distance, max_distance)
	if Input.is_action_just_pressed("cam_zoom_out"):
		distance = clampf(distance + zoom_step, min_distance, max_distance)
	_camera.position.z = lerpf(_camera.position.z, distance, 1.0 - exp(-12.0 * delta))

	if target != null:
		# exp() rather than a raw lerp so the smoothing does not change with
		# framerate — a plain `lerp(a, b, 0.1)` chases faster at 144 Hz than 60.
		var t := 1.0 - exp(-follow_smoothing * delta)
		global_position = global_position.lerp(_desired_position(), t)

func _desired_position() -> Vector3:
	return target.global_position + Vector3.UP * height_offset

## Yaw of the camera on the ground plane. The player uses this to turn WASD into
## camera-relative movement, so "up" is always up the screen.
func ground_yaw() -> float:
	return rotation.y
