# 0003 — Issues

Running log of problems hit while doing `todo.md`. Written during the work.

**Legend:** `OPEN` · `RESOLVED` · `ANTICIPATED` (predicted, not yet hit) ·
`WONTFIX`

---

## Carried forward, and still true in Godot

- **Rendering must be tested from a Windows process, not from WSL.** The
  constraint that forced Windows Chrome for the `wgpu` build applies unchanged
  here: WSL2 has no usable GPU driver path, so a Linux Godot falls back to
  software rendering. It still draws a frame, which is exactly what makes it
  dangerous — the thing under evaluation is how this looks and how fast it
  draws, and a software frame answers neither.

  Godot opens the project fine over the WSL share; `scripts/godot.sh` builds
  the UNC path (`\\wsl.localhost\$WSL_DISTRO_NAME\...`) from the repo location.
  Confirmed real: the banner reads
  `Vulkan 1.4.303 - Forward+ - Using Device #0: NVIDIA GeForce RTX 2080 Ti`.
  If that line ever says anything else, the run is not evidence.

---

## 1. `RESOLVED` — Tiled ground moired into diagonal stripes

**Symptom.** The first render's ground was crossed by soft diagonal bands that
had nothing to do with the noise being drawn into the texture.

**Cause.** Two things at once.

- Godot's PNG importer defaults to `mipmaps/generate=false`. The ground plane
  tiles its 128 px texture 24× across 80 m and is viewed at a 40° pitch, which
  is precisely the case mipmaps exist for. Without them the minified texels
  alias, and the aliasing beats against the tile repeat.
- The generator's own noise was weighted toward its lowest octave, so each tile
  was a few large soft blobs — and large soft blobs repeated 24× *are* a
  stripe pattern.

**Resolution.** `mipmaps/generate=true` on `ground.png`, noise reweighted
toward the higher octaves with a fine grain layer added, and `uv1_scale`
lowered 40 → 24 so a tile covers 3.3 m instead of 2 m.

**Also set:** `detect_3d/compress_to=0` on every texture. Left at its default
of `1`, Godot silently rewrites the `.import` file the first time the editor
opens a scene that uses the texture in 3D — switching it to VRAM compression
and turning mipmaps on. That is a helpful default and a terrible one to have
fire unannounced: it means the committed import settings are not the ones that
were tested, and the first editor session produces a diff nobody wrote. Pinned
explicitly instead.

## 2. `RESOLVED` — GDScript parse error from an unannotated return type

**Symptom.** `Parse Error: Cannot infer the type of "at" variable because the
value doesn't have a set type.` — the whole script failed to load.

**Cause.** `_arg_vec2()` had no declared return type, because it wanted to
return `null` when the argument was absent. `var at := _arg_vec2(...)` then had
nothing to infer from.

**Resolution.** Dropped the nullable return: the helpers now take a fallback
and always return a real `float` / `Vector2`. Better code anyway — no null
checks at the call site.

**Worth internalising.** GDScript's optional typing is checked at *parse* time,
so this class of mistake is caught before the game runs, not at the moment the
line executes. That is the main reason everything here is annotated. It is
still much weaker than Rust: a `Dictionary` field read is unchecked, and
`prop.texture = ...` on a `Node3D`-typed variable is resolved dynamically.

## 3. `RESOLVED` — Screenshots imported themselves as game assets

**Symptom.** `godot -- --shot` writing to `res://screenshot.png` produced a
`screenshot.png.import` beside it, and the file turned up as a texture in the
project.

**Cause.** Everything under `res://` is an asset as far as the importer is
concerned.

**Resolution.** Screenshots go to `res://shots/`, which carries a `.gdignore`
file. Godot skips the whole directory.

## 4. `RESOLVED` — Alpha-cut sprites have unantialiasable edges

**Symptom.** Sprite silhouettes were visibly jagged despite 4× MSAA being on.

**Cause.** `ALPHA_CUT_DISCARD` is what buys per-pixel depth sorting (see
`todo.md`, finding 2), but a `discard` is a binary decision — MSAA has no
gradient to resolve, so the cut-out edge stays hard.

**Resolution.** `alpha_antialiasing_mode = ALPHA_ANTIALIASING_ALPHA_TO_COVERAGE`
on every billboard. The cut-out feeds MSAA's coverage mask instead of being a
hard discard, so edges smooth out while the sprite stays in the opaque pass and
keeps its sorting. It only works with MSAA enabled — `project.godot` sets
`msaa_3d=2` (4×), and turning MSAA off silently un-fixes this.

FXAA (`screen_space_aa`) was removed at the same time: with MSAA plus
alpha-to-coverage it adds nothing and smears the sprite interiors.

## 5. `RESOLVED` — Billboards lean at the screen edges

**Symptom.** With the camera at 32° FOV, tree trunks toward the left and right
edges tilted outward rather than standing vertical.

**Cause.** Not a billboarding bug. A vertical line off the optical axis
projects as a converging line under any perspective camera; billboards make it
conspicuous because every object in the scene is a vertical line.

**Resolution.** FOV 32° → 26°, camera distance 15 m → 19 m to keep the framing.
Narrower is flatter. The end of that road is an orthographic camera, which
removes the lean entirely and also removes the parallax that makes the world
read as 3D at all, so this is a taste dial rather than a fix — `camera_rig.gd`
exports both ends of it.

## 6. `ANTICIPATED` — Being hidden behind a canopy is correct and unplayable

`shot-occlusion.png` is the acceptance evidence for per-pixel sorting and also
a preview of a design problem: the player is entirely behind a tree with only
their head visible, which is exactly right and no fun. Don't Starve fades
canopies to partial alpha when the player is under them.

The fix is not free. Fading means leaving the opaque pass for that object,
which means giving up the sorting from §4 for as long as the fade lasts. The
usual answer is a dither/hash fade (`ALPHA_CUT_HASH` in Godot) which stays in
the opaque pass by discarding a noise-thresholded subset of pixels. Untested
here.

## 7. `ANTICIPATED` — Nothing about this PoC survives contact with terrain height

The ground is one flat `PlaneMesh`. Sprites are placed at `y = 0` and rely on
their bottom-centre pivot landing on it. On a heightmap, every prop needs its
ground height sampled at placement, and a sprite standing on a slope has a
pivot line that no longer matches the terrain under it. `0002` was building
exactly that heightmap. If the Godot branch is taken forward, this is the first
real unknown, and it is a bigger one than anything in this unit.

## 8. `WONTFIX` — The placeholder art is placeholder art

`tools/gen_placeholder_art.gd` draws flat shapes with a silhouette outline pass
because Don't Starve's readability comes from silhouette first. It is not a
style proposal and it is not an argument that the pipeline handles real
hand-drawn art — 32 px-per-metre sprites at 4× the size, with soft edges and
interior detail, will stress the filtering and the alpha threshold in ways
these will not.
