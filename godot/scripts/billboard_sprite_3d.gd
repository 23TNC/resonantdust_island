@tool
class_name BillboardSprite3D
extends Sprite3D

## A 2D sprite standing up in the 3D world. This is the whole trick the PoC is
## testing, so the rules live in one place rather than being re-set by hand on
## every node.
##
## Five settings do the work, and each of them is easy to get wrong:
##
## 1. `billboard = BILLBOARD_FIXED_Y` — the quad yaws to face the camera but
##    never pitches. Full billboarding (BILLBOARD_ENABLED) makes trees lie back
##    as the camera tilts down, which reads as fake immediately.
## 2. `alpha_cut = ALPHA_CUT_DISCARD` — draws in the opaque pass and discards
##    cut-out texels, so sprites depth-sort per pixel against each other and
##    against world geometry. Plain alpha blending sorts whole quads by their
##    origin, which pops when two trees overlap.
## 3. bottom-centre pivot — the origin sits where the sprite meets the ground,
##    so a node at y = 0 is standing on the ground rather than buried to the
##    waist. Done by lifting the (still centred) quad half its own height.
## 4. `alpha_antialiasing_mode = ALPHA_TO_COVERAGE` — the catch with the
##    discard in (2) is that it produces a hard, jagged cut-out edge that MSAA
##    cannot touch. Alpha-to-coverage feeds the cut-out into MSAA's coverage
##    mask instead, so edges smooth out while the sprite stays in the opaque
##    pass and keeps its per-pixel sorting. It needs MSAA on (see project.godot).
## 5. one project-wide pixel scale — see PIXELS_PER_METRE.

## Art scale for the whole project: 32 texture pixels is one metre. Everything
## derives its world size from its texture size, so a 96 px tree is 3 m tall
## with no per-asset tuning, and re-drawn art keeps its scale automatically.
const PIXELS_PER_METRE := 32.0

func _ready() -> void:
	texture_changed.connect(_apply)
	_apply()

func _apply() -> void:
	billboard = BaseMaterial3D.BILLBOARD_FIXED_Y
	alpha_cut = SpriteBase3D.ALPHA_CUT_DISCARD
	alpha_scissor_threshold = 0.5
	alpha_antialiasing_mode = BaseMaterial3D.ALPHA_ANTIALIASING_ALPHA_TO_COVERAGE
	alpha_antialiasing_edge = 0.3
	# Lit, so the directional light can carry a day/night cycle later.
	shaded = true
	double_sided = true
	texture_filter = BaseMaterial3D.TEXTURE_FILTER_LINEAR
	pixel_size = 1.0 / PIXELS_PER_METRE

	if texture != null:
		position.y = texture.get_height() * pixel_size * 0.5

## World-space height of this sprite, for callers that need to size a shadow or
## place something on top of it.
func world_height() -> float:
	if texture == null:
		return 0.0
	return texture.get_height() * pixel_size
