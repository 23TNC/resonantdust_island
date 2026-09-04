extends CharacterBody3D

## The player is a real 3D body — a capsule that collides with trees and rocks —
## wearing a 2D sprite. Keeping those two things separate is the point: the
## simulation never has to know the art is flat, and the art never has to know
## about physics.
##
## The two have to agree about one thing: where the ground is. The capsule in
## player.tscn sits half its own height above the body origin, so it comes to
## rest with the origin exactly at y = 0; BillboardSprite3D pivots the sprite
## from that same origin. Offset either and the character sinks or floats by a
## few centimetres — small enough to look like a lighting bug rather than a
## transform one. `scripts/godot.sh check` asserts both.

@export var speed := 4.5
@export var acceleration := 30.0
@export var friction := 22.0
@export var gravity := 24.0

## Cosmetic walk cycle, so a moving character does not look like a sliding
## decal. Real frame animation replaces this; the numbers are deliberately small.
@export var bob_height := 0.06
@export var bob_speed := 11.0
@export var squash := 0.05

@onready var _sprite: BillboardSprite3D = $Sprite

var _sprite_rest_y := 0.0
var _walk_phase := 0.0

func _ready() -> void:
	# BillboardSprite3D lifts itself to sit on the ground in its own _ready(),
	# which Godot runs before this one. Cache the result so the bob offsets from
	# it instead of overwriting it.
	_sprite_rest_y = _sprite.position.y

func _physics_process(delta: float) -> void:
	var camera := get_viewport().get_camera_3d()
	var input := Input.get_vector("move_left", "move_right", "move_up", "move_down")
	var wish := Vector3.ZERO
	var right := Vector3.RIGHT

	if camera != null:
		# Flatten the camera basis onto the ground plane so WASD is screen
		# relative: W walks up the screen no matter which way the camera is
		# turned. Without this, rotating the camera silently rebinds the keys.
		right = camera.global_basis.x
		right.y = 0.0
		right = right.normalized()
		var forward := -camera.global_basis.z
		forward.y = 0.0
		forward = forward.normalized()
		wish = (right * input.x - forward * input.y).limit_length(1.0)

	var planar := Vector3(velocity.x, 0.0, velocity.z)
	var rate := acceleration if wish != Vector3.ZERO else friction
	planar = planar.move_toward(wish * speed, rate * delta)
	velocity.x = planar.x
	velocity.z = planar.z

	if is_on_floor():
		velocity.y = minf(velocity.y, 0.0)
	else:
		velocity.y -= gravity * delta

	move_and_slide()
	_animate(planar, right, delta)

func _animate(planar: Vector3, right: Vector3, delta: float) -> void:
	var moving := planar.length() > 0.15

	# Face the way we are travelling *on screen*, not in world space — the
	# sprite has no idea where the camera is, so ask the camera's right vector.
	var lateral := right.dot(planar)
	if absf(lateral) > 0.25:
		_sprite.flip_h = lateral < 0.0

	if moving:
		_walk_phase += delta * bob_speed
	else:
		_walk_phase = move_toward(_walk_phase, round(_walk_phase / PI) * PI, delta * bob_speed)

	var bob: float = absf(sin(_walk_phase))
	_sprite.position.y = _sprite_rest_y + bob * bob_height
	_sprite.scale = Vector3(1.0 + squash * bob, 1.0 - squash * bob, 1.0)
