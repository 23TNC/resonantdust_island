# Generates the placeholder PNG art for the proof of concept.
#
# Run once from the repo root:
#   scripts/godot-gen-art.sh
#
# Everything it writes is a stand-in. The point of the PoC is the *pipeline* —
# 2D sprites standing up in a 3D world — so the art only has to be legible
# enough to prove sprites sort, billboard and light correctly. Replace the PNGs
# in assets/ with real art and nothing else has to change.
extends SceneTree

const OUTLINE := Color(0.09, 0.07, 0.11, 1.0)

func _initialize() -> void:
	var sprites := "res://assets/sprites/"
	var textures := "res://assets/textures/"
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(sprites))
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(textures))

	_save(_make_player(), sprites + "player.png")
	_save(_make_tree(), sprites + "tree.png")
	_save(_make_rock(), sprites + "rock.png")
	_save(_make_grass(), sprites + "grass.png")
	_save(_make_shadow(), sprites + "shadow.png")
	_save(_make_ground(), textures + "ground.png")

	print("placeholder art written")
	quit()

func _save(img: Image, path: String) -> void:
	var err := img.save_png(path)
	if err != OK:
		push_error("failed to write %s (error %d)" % [path, err])
	else:
		print("  %s  %dx%d" % [path, img.get_width(), img.get_height()])

# --- drawing helpers -------------------------------------------------------

func _blank(w: int, h: int) -> Image:
	return Image.create(w, h, false, Image.FORMAT_RGBA8)

func _put(img: Image, x: int, y: int, c: Color) -> void:
	if x < 0 or y < 0 or x >= img.get_width() or y >= img.get_height():
		return
	img.set_pixel(x, y, c)

func _rect(img: Image, x: int, y: int, w: int, h: int, c: Color) -> void:
	for oy in h:
		for ox in w:
			_put(img, x + ox, y + oy, c)

func _ellipse(img: Image, cx: float, cy: float, rx: float, ry: float, c: Color) -> void:
	var x0 := int(floor(cx - rx))
	var x1 := int(ceil(cx + rx))
	var y0 := int(floor(cy - ry))
	var y1 := int(ceil(cy + ry))
	for y in range(y0, y1 + 1):
		for x in range(x0, x1 + 1):
			var dx := (x + 0.5 - cx) / maxf(rx, 0.001)
			var dy := (y + 0.5 - cy) / maxf(ry, 0.001)
			if dx * dx + dy * dy <= 1.0:
				_put(img, x, y, c)

# Any transparent pixel touching an opaque one becomes outline. Don't Starve's
# whole silhouette-first look comes from a heavy dark keyline, and doing it as a
# post-pass means the shapes above stay dumb and readable.
func _outline(img: Image, thickness: int = 2) -> void:
	for _pass in thickness:
		var src := (img.duplicate() as Image)
		for y in img.get_height():
			for x in img.get_width():
				if src.get_pixel(x, y).a > 0.0:
					continue
				var touching := false
				for oy in range(-1, 2):
					for ox in range(-1, 2):
						var nx := x + ox
						var ny := y + oy
						if nx < 0 or ny < 0 or nx >= src.get_width() or ny >= src.get_height():
							continue
						if src.get_pixel(nx, ny).a > 0.0:
							touching = true
				if touching:
					img.set_pixel(x, y, OUTLINE)

# --- sprites ---------------------------------------------------------------

func _make_player() -> Image:
	var img := _blank(32, 48)
	var skin := Color(0.93, 0.76, 0.60)
	var shirt := Color(0.75, 0.28, 0.30)
	var pants := Color(0.28, 0.30, 0.42)
	var hair := Color(0.32, 0.20, 0.14)
	# legs
	_rect(img, 11, 36, 4, 8, pants)
	_rect(img, 17, 36, 4, 8, pants)
	# torso
	_rect(img, 10, 22, 12, 15, shirt)
	# arms
	_rect(img, 7, 24, 3, 10, skin)
	_rect(img, 22, 24, 3, 10, skin)
	# head
	_ellipse(img, 16.0, 15.0, 6.5, 7.0, skin)
	# hair sweep
	_ellipse(img, 16.0, 10.5, 7.0, 4.5, hair)
	_rect(img, 9, 11, 3, 6, hair)
	_outline(img, 2)
	return img

func _make_tree() -> Image:
	var img := _blank(64, 96)
	var bark := Color(0.36, 0.24, 0.17)
	var leaf_dark := Color(0.13, 0.34, 0.20)
	var leaf := Color(0.20, 0.48, 0.26)
	_rect(img, 27, 58, 10, 34, bark)
	_ellipse(img, 32.0, 52.0, 22.0, 16.0, leaf_dark)
	_ellipse(img, 32.0, 34.0, 24.0, 18.0, leaf_dark)
	_ellipse(img, 32.0, 20.0, 17.0, 14.0, leaf_dark)
	# lit side, offset toward the light
	_ellipse(img, 27.0, 30.0, 18.0, 14.0, leaf)
	_ellipse(img, 28.0, 17.0, 12.0, 10.0, leaf)
	_outline(img, 2)
	return img

func _make_rock() -> Image:
	var img := _blank(48, 40)
	var stone := Color(0.52, 0.53, 0.58)
	var stone_lit := Color(0.68, 0.69, 0.73)
	_ellipse(img, 24.0, 27.0, 20.0, 12.0, stone)
	_ellipse(img, 20.0, 18.0, 13.0, 11.0, stone)
	_ellipse(img, 33.0, 21.0, 10.0, 8.0, stone)
	_ellipse(img, 18.0, 15.0, 8.0, 6.0, stone_lit)
	_outline(img, 2)
	return img

func _make_grass() -> Image:
	var img := _blank(24, 22)
	var blade := Color(0.30, 0.55, 0.26)
	var blade_dark := Color(0.20, 0.40, 0.20)
	# a few blades fanning out from the base
	for i in 7:
		var lean := (i - 3) * 1.4
		var height := 12 + (i % 3) * 4
		var c := blade if i % 2 == 0 else blade_dark
		for t in height:
			var f := float(t) / float(height)
			var x := int(round(12.0 + lean * f * f))
			var y := 20 - t
			_put(img, x, y, c)
			_put(img, x + 1, y, c)
	_outline(img, 1)
	return img

# Soft radial blob. Sprites float without one — this is what visually sits them
# on the ground plane.
func _make_shadow() -> Image:
	var img := _blank(64, 64)
	for y in 64:
		for x in 64:
			var dx := (x + 0.5 - 32.0) / 31.0
			var dy := (y + 0.5 - 32.0) / 31.0
			var d := sqrt(dx * dx + dy * dy)
			if d >= 1.0:
				continue
			var a: float = pow(1.0 - d, 2.4) * 0.72
			img.set_pixel(x, y, Color(0.0, 0.0, 0.0, a))
	return img

# --- ground ----------------------------------------------------------------

# Value noise on a lattice that wraps at `period`, so the texture tiles
# seamlessly no matter how many times the ground plane repeats it.
func _hash2(x: int, y: int, period: int, seed_v: int) -> float:
	var hx := posmod(x, period)
	var hy := posmod(y, period)
	var n := hx * 374761393 + hy * 668265263 + seed_v * 1442695041
	n = (n ^ (n >> 13)) * 1274126177
	n = n ^ (n >> 16)
	return float(posmod(n, 65536)) / 65535.0

func _value_noise(u: float, v: float, period: int, seed_v: int) -> float:
	var fx := u * period
	var fy := v * period
	var x0 := int(floor(fx))
	var y0 := int(floor(fy))
	var tx := fx - x0
	var ty := fy - y0
	tx = tx * tx * (3.0 - 2.0 * tx)
	ty = ty * ty * (3.0 - 2.0 * ty)
	var a := _hash2(x0, y0, period, seed_v)
	var b := _hash2(x0 + 1, y0, period, seed_v)
	var c := _hash2(x0, y0 + 1, period, seed_v)
	var d := _hash2(x0 + 1, y0 + 1, period, seed_v)
	return lerpf(lerpf(a, b, tx), lerpf(c, d, tx), ty)

func _make_ground() -> Image:
	var size := 128
	var img := _blank(size, size)
	var grass_a := Color(0.24, 0.40, 0.22)
	var grass_b := Color(0.32, 0.50, 0.26)
	var dirt := Color(0.42, 0.35, 0.23)
	for y in size:
		for x in size:
			var u := float(x) / float(size)
			var v := float(y) / float(size)
			# Weighted toward the higher octaves: a low-frequency-dominant
			# ground reads as big soft blobs, and once it is tiled 20-odd
			# times across the plane those blobs moire into visible stripes.
			var n := _value_noise(u, v, 4, 1) * 0.25
			n += _value_noise(u, v, 8, 2) * 0.3
			n += _value_noise(u, v, 16, 3) * 0.25
			n += _value_noise(u, v, 32, 4) * 0.2
			var c := grass_a.lerp(grass_b, clampf(n * 1.7 - 0.35, 0.0, 1.0))
			var patch := _value_noise(u, v, 6, 7)
			if patch > 0.78:
				c = c.lerp(dirt, (patch - 0.78) / 0.22 * 0.55)
			# Fine grain, to give the mipmaps something to average into instead
			# of a flat colour at distance.
			c = c.lerp(Color(0.16, 0.26, 0.14), _value_noise(u, v, 64, 9) * 0.12)
			img.set_pixel(x, y, c)
	return img
