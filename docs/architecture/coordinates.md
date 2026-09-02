# Coordinates, units and the camera

**Decided:** 2026-09-02, during work unit `0002-tile-world-generation`.
**Status:** in force. Terrain, billboards, placement and picking all depend on
it.

## Handedness and axes

**Right-handed. X right, Y up, Z toward the viewer.**

```
        Y (up)
        │
        │
        └───── X (right / east)
       ╱
      ╱
     Z (toward the viewer / south)
```

`X × Y = Z`. Y is the world's vertical axis: elevation, object height, and
gravity all act along it. The ground is the XZ plane.

On screen, with the camera below:

- increasing **X** moves **right**
- increasing **Y** moves **up**
- increasing **Z** moves **down** the screen (toward the viewer; -Z is farther
  away and appears higher up)

## Units

**One tile is `1.0` world unit on each side.** No pixels-per-unit constant, no
scale factor buried in the renderer. A character roughly 1.8 units tall is
roughly 1.8 tiles tall, and that is the whole conversion.

## Tile addressing

A tile is indexed by integers `(tx, tz)` and occupies:

```
world X ∈ [tx, tx + 1)
world Z ∈ [tz, tz + 1)
world Y  = its height
```

**A tile's origin is its horizontal centre, at ground height:**

```
origin = (tx + 0.5, height, tz + 0.5)
```

Centre rather than corner, because an object placed at a tile's origin should
stand in the middle of it. A corner origin makes every placement carry a `+0.5`
that is easy to forget in one place and not another.

## Terrain is stepped, not smooth

**Each tile is flat, at a single integer height.** Height changes between
neighbours are vertical walls, not slopes.

Chosen because a tile needs one unambiguous answer to "what height is this?" —
that is the number billboards are placed at, the number picking resolves to,
and the number pathing will use. A smoothly interpolated surface makes it a
function of position within the tile, and every system downstream then has to
care where in the tile it is standing.

Consequence: **height changes must be meshed as walls.** A heightmap that emits
only top faces renders as floating plateaus with gaps.

## No tunnels, caves or overhangs

One surface height per `(x, z)` column. This is assumed throughout: the world
is `width × depth` cells of `(height, kind)`, not a 3D grid. Lifting it later
is not a tweak — it changes the data model, the mesher, and picking.

## The camera never rotates

**Orthographic. Fixed pitch. Fixed yaw. No rotation, ever.**

Constants live in [`island_core::camera`](../../crates/island_core/src/camera.rs).

### Pitch: degrees above the horizontal ground plane

State the convention every time the number appears. "A 30 degree camera" is
ambiguous between 30° up from the ground (side-on) and 30° over from straight
down (nearly top-down) — a 60° difference. **Here it is always measured up from
the ground plane, so smaller is more side-on.**

Per world unit, at pitch θ, under orthographic projection:

| | screen height | 30° | 45° | 60° |
|---|---|---|---|---|
| 1 unit of ground depth | `sin θ` | 0.50 | 0.71 | 0.87 |
| 1 unit of cliff face | `cos θ` | 0.87 | 0.71 | 0.50 |
| upright billboard | `cos θ` | 0.87 | 0.71 | 0.50 |

A **low** pitch shows cliffs and standing objects near full height but
foreshortens the ground hard — tiles become thin slivers and less map fits on
screen. A **high** pitch reads the map clearly but squashes anything upright:
an upright billboard loses 13% of its height at 30° and **half** of it at 60°,
which artists must otherwise absorb by pre-stretching every sprite.

**Provisionally 30°**, matching the Mad Island reference and keeping billboard
squash low. Not final — the committing decision comes from rendered
comparisons of the same seed once terrain exists, because this is judged by eye.

### Why fixed

Not a limitation to lift later. It buys:

- billboard orientation is **one constant matrix**, not per-object work
- the light direction never changes, so terrain shading can be baked into
  vertex data if it ever needs to be
- frustum culling is a fixed-shape region test
- sprite art is drawn once, for one angle

A rotatable camera would need every sprite drawn from every angle, which is the
cost that made this style attractive in the first place.

### Yaw is zero

The camera looks along **-Z**, so the tile grid stays axis-aligned on screen: X
runs left-to-right, Z runs up and down. No isometric diamond skew.

Axis-aligned keeps tile-to-pixel mapping simple, makes mouse picking a division
rather than a matrix inverse, and means art is not drawn on a diagonal.

## Clip space

**wgpu puts depth in `0..1`**, not OpenGL's `-1..1`.

Use `glam::Mat4::orthographic_rh`. **Not** `orthographic_rh_gl` — one suffix
apart, and the wrong one gives a scene that is clipped or z-fighting for
reasons that look like a depth-buffer bug.

Pinned by tests in `camera.rs` (`orthographic_rh_matches_wgpu_depth_range`,
`orthographic_rh_gl_is_the_wrong_one`, `positive_y_is_up_in_clip_space`) so a
glam upgrade cannot silently swap the convention.

## Triangle winding

**Counter-clockwise is front-facing.** That *is* wgpu's default —
`FrontFace::Ccw` is `#[default]` on the enum.

**Back-face culling is not on by default** and must be asked for.
`PrimitiveState::cull_mode` is an `Option<Face>`, so `..Default::default()`
leaves it `None`, meaning nothing is culled. Set `cull_mode: Some(Face::Back)`
explicitly.

Terrain top faces and wall faces must be wound accordingly. A wall wound the
wrong way is invisible from the side you meant to see it from — and with
culling left off it is instead visible from *both* sides, which hides the
error until culling is switched on later.
