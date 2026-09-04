extends Node3D

## Scatters the scenery and drives the HUD.
##
## The scatter is seeded, so the island looks identical every launch. That is
## deliberate: when you are judging whether sprites sort and light correctly,
## you want to compare two runs of the same island, not two different ones.

const PROP_SCENE := preload("res://scenes/prop.tscn")

## Each entry: sprite, how often it appears, collider radius in metres (0 = you
## can walk through it), shadow width as a fraction of sprite width, and how
## much clear space it needs around itself.
const PROP_KINDS: Array[Dictionary] = [
	{
		"texture": "res://assets/sprites/tree.png",
		"weight": 4.5,
		"collision_radius": 0.55,
		"shadow_scale": 0.8,
		"spacing": 2.1,
	},
	{
		"texture": "res://assets/sprites/rock.png",
		"weight": 1.5,
		"collision_radius": 0.6,
		"shadow_scale": 0.85,
		"spacing": 2.0,
	},
	{
		"texture": "res://assets/sprites/grass.png",
		"weight": 4.0,
		"collision_radius": 0.0,
		"shadow_scale": 0.7,
		"spacing": 0.9,
	},
]

@export var world_seed := 20260904
@export var scatter_radius := 22.0
@export var prop_count := 340
## Keep the spawn point clear so the player is never wedged inside a tree.
@export var clearing_radius := 3.0

@onready var _props: Node3D = $Props
@onready var _player: CharacterBody3D = $Player
@onready var _stats: Label = $HUD/Stats

var _placed: Array[Vector2] = []

func _ready() -> void:
	_scatter()
	_stats.visible = false
	if OS.get_cmdline_user_args().has("--shot"):
		_capture_and_quit()

func _process(_delta: float) -> void:
	if Input.is_action_just_pressed("toggle_debug"):
		_stats.visible = not _stats.visible
	if _stats.visible:
		_stats.text = "%d fps\n%d props\n%d draw calls" % [
			Engine.get_frames_per_second(),
			_props.get_child_count(),
			RenderingServer.get_rendering_info(RenderingServer.RENDERING_INFO_TOTAL_DRAW_CALLS_IN_FRAME),
		]

func _scatter() -> void:
	var rng := RandomNumberGenerator.new()
	rng.seed = world_seed

	var total_weight := 0.0
	for kind in PROP_KINDS:
		total_weight += kind["weight"]

	# Dart throwing: pick a point, drop it if it crowds something already there.
	# Cheap, and good enough for a few hundred props — a real world generator
	# would use Poisson-disc sampling over the chunk grid instead.
	var attempts := prop_count * 12
	while attempts > 0 and _props.get_child_count() < prop_count:
		attempts -= 1
		var kind := _pick_kind(rng, total_weight)
		var point := _random_point(rng)
		if point.length() < clearing_radius:
			continue
		if _crowded(point, kind["spacing"]):
			continue
		_placed.append(point)
		_props.add_child(_make_prop(kind, point, rng))

func _pick_kind(rng: RandomNumberGenerator, total_weight: float) -> Dictionary:
	var roll := rng.randf() * total_weight
	for kind in PROP_KINDS:
		roll -= kind["weight"]
		if roll <= 0.0:
			return kind
	return PROP_KINDS[-1]

func _random_point(rng: RandomNumberGenerator) -> Vector2:
	# sqrt() keeps the distribution even across the disc; without it everything
	# bunches up in the middle.
	var angle := rng.randf() * TAU
	var radius := sqrt(rng.randf()) * scatter_radius
	return Vector2(cos(angle), sin(angle)) * radius

func _crowded(point: Vector2, spacing: float) -> bool:
	for other in _placed:
		if point.distance_to(other) < spacing:
			return true
	return false

func _make_prop(kind: Dictionary, point: Vector2, rng: RandomNumberGenerator) -> Node3D:
	var prop := PROP_SCENE.instantiate()
	prop.position = Vector3(point.x, 0.0, point.y)
	# Uniform only — a non-uniform scale on a physics body skews its collider.
	prop.scale = Vector3.ONE * rng.randf_range(0.85, 1.15)
	prop.texture = load(kind["texture"])
	prop.collision_radius = kind["collision_radius"]
	prop.shadow_scale = kind["shadow_scale"]
	# Mirror half of them so a stand of trees is not obviously one stamp.
	prop.get_node("Sprite").flip_h = rng.randf() < 0.5
	return prop

## Headless-ish capture, so the look can be checked from a script instead of by
## eye. Renders a few frames, writes a PNG, quits.
##
##   godot -- --shot                                    default framing
##   godot -- --shot --at=7,-3 --yaw=35 --out=turn.png  posed framing
func _capture_and_quit() -> void:
	var rig: Node3D = $CameraRig
	var here := Vector2(_player.position.x, _player.position.z)
	var at := _arg_vec2("--at=", here)
	_player.global_position = Vector3(at.x, 0.1, at.y)
	rig.rotation.y = deg_to_rad(_arg_float("--yaw=", rad_to_deg(rig.rotation.y)))
	# The rig eases toward its target; without a snap the capture would be taken
	# mid-glide from wherever the camera started.
	rig.global_position = _player.global_position + Vector3.UP * rig.height_offset

	for _i in 20:
		await get_tree().process_frame
	await RenderingServer.frame_post_draw

	# shots/ carries a .gdignore, so Godot leaves the PNGs alone instead of
	# importing each one as a game texture and littering .import files.
	var path: String = "res://shots/" + _arg_string("--out=", "screenshot.png")
	var err := get_viewport().get_texture().get_image().save_png(path)
	if err == OK:
		print("screenshot: ", ProjectSettings.globalize_path(path))
	else:
		push_error("screenshot failed (error %d)" % err)
	get_tree().quit(0 if err == OK else 1)

func _arg_string(prefix: String, fallback: String) -> String:
	for arg in OS.get_cmdline_user_args():
		if arg.begins_with(prefix):
			return arg.substr(prefix.length())
	return fallback

func _arg_float(prefix: String, fallback: float) -> float:
	var raw := _arg_string(prefix, "")
	return fallback if raw.is_empty() else raw.to_float()

func _arg_vec2(prefix: String, fallback: Vector2) -> Vector2:
	var parts := _arg_string(prefix, "").split(",")
	if parts.size() != 2:
		return fallback
	return Vector2(parts[0].to_float(), parts[1].to_float())
