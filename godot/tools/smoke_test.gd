# Headless smoke test: builds the world, runs a few physics frames and checks
# the things that are easy to break silently and hard to see in a screenshot.
#
#   scripts/godot.sh check
#
# Exits non-zero on failure, so it can gate a commit.
extends SceneTree

var _failures := 0

func _initialize() -> void:
	var world := (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(world)
	await process_frame
	await physics_frame
	await physics_frame

	var props: Node3D = world.get_node("Props")
	var player: CharacterBody3D = world.get_node("Player")
	var sprite: BillboardSprite3D = player.get_node("Sprite")

	_check("props were scattered", props.get_child_count() > 100,
		"%d props" % props.get_child_count())

	# Grass must not block the player and trees must, so the two populations
	# have to actually differ — a bug in prop.gd would collapse them into one.
	var solid := 0
	var radii := {}
	for prop in props.get_children():
		var collider: CollisionShape3D = prop.get_node("Collision")
		if collider.disabled:
			continue
		solid += 1
		radii[snappedf((collider.shape as CylinderShape3D).radius, 0.01)] = true
	_check("some props are solid and some are not",
		solid > 0 and solid < props.get_child_count(),
		"%d of %d solid" % [solid, props.get_child_count()])
	_check("colliders are per-instance, not one shared resource",
		radii.size() > 1, "%d distinct radii" % radii.size())

	# The pivot rule: a sprite's origin is its feet, and the body settles with
	# its origin on the ground. Both wrong-by-default; both invisible until
	# something is half-buried.
	_check("player is standing on the ground", player.is_on_floor(),
		"y = %.3f" % player.global_position.y)
	_check("player origin rests at ground level",
		absf(player.global_position.y) < 0.01,
		"y = %.3f" % player.global_position.y)
	_check("sprite pivot is bottom-centre",
		is_equal_approx(sprite.position.y, sprite.world_height() * 0.5),
		"pivot %.3f, height %.2f m" % [sprite.position.y, sprite.world_height()])
	_check("art scale holds: 48 px sprite is 1.5 m",
		is_equal_approx(sprite.world_height(), 1.5),
		"%.2f m" % sprite.world_height())

	# The four settings the whole look depends on.
	_check("billboard is fixed-Y",
		sprite.billboard == BaseMaterial3D.BILLBOARD_FIXED_Y, str(sprite.billboard))
	_check("sprites depth-sort per pixel (alpha cut, not blend)",
		sprite.alpha_cut == SpriteBase3D.ALPHA_CUT_DISCARD, str(sprite.alpha_cut))
	_check("alpha-to-coverage is on",
		sprite.alpha_antialiasing_mode == BaseMaterial3D.ALPHA_ANTIALIASING_ALPHA_TO_COVERAGE,
		str(sprite.alpha_antialiasing_mode))
	_check("MSAA is on, without which alpha-to-coverage does nothing",
		int(ProjectSettings.get_setting("rendering/anti_aliasing/quality/msaa_3d", 0)) > 0,
		str(ProjectSettings.get_setting("rendering/anti_aliasing/quality/msaa_3d", 0)))

	print("")
	if _failures == 0:
		print("smoke test passed")
	else:
		print("smoke test FAILED: %d check(s)" % _failures)
	quit(1 if _failures > 0 else 0)

func _check(what: String, ok: bool, detail: String) -> void:
	if not ok:
		_failures += 1
	print("%s  %s  (%s)" % ["ok  " if ok else "FAIL", what, detail])
