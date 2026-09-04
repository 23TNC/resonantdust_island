@tool
extends StaticBody3D

## One scattered piece of scenery: a billboard sprite, a blob shadow sized to
## match it, and an optional cylinder of collision. World.gd stamps these out
## from a small table rather than there being a scene per bush.

@export var texture: Texture2D:
	set(value):
		texture = value
		_apply()

## Radius of the collider in metres. Zero means "walk straight through it" —
## grass should not stop the player.
@export var collision_radius := 0.0:
	set(value):
		collision_radius = value
		_apply()

## Shadow width as a fraction of the sprite's own width.
@export var shadow_scale := 0.7:
	set(value):
		shadow_scale = value
		_apply()

func _ready() -> void:
	_apply()

func _apply() -> void:
	if not is_node_ready():
		return
	var sprite: BillboardSprite3D = $Sprite
	var shadow: Sprite3D = $Shadow
	var collider: CollisionShape3D = $Collision

	sprite.texture = texture

	if texture != null:
		var width := texture.get_width() * sprite.pixel_size
		shadow.pixel_size = width * shadow_scale / maxf(shadow.texture.get_width(), 1.0)

	var solid := collision_radius > 0.0
	collider.disabled = not solid
	var shape := collider.shape as CylinderShape3D
	if solid and shape != null:
		# The shape is marked resource_local_to_scene in prop.tscn, so each
		# instance owns its own copy. Without that, resizing one prop's collider
		# would resize every prop's.
		shape.radius = collision_radius
		shape.height = sprite.world_height()
		collider.position.y = shape.height * 0.5
