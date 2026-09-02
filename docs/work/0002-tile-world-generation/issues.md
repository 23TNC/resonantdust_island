# 0002 — Issues

Running log of problems hit while doing `todo.md`. Written during the work.

**Legend:** `OPEN` · `RESOLVED` · `ANTICIPATED` (predicted, not yet hit) ·
`WONTFIX`

---

## Carried forward from 0001

Still true, still load-bearing.

- **Read the vendored wgpu source, not recalled API shapes.**
  `~/.cargo/registry/src/*/wgpu-30.0.1/src/api/` is the reference for this
  version. Unit `0001` §10 lists nine places wgpu 30 differs from every example
  in circulation; this unit adds a depth buffer, a second pipeline, bind groups
  and vertex buffers, all of which are new API surface with the same hazard.
  `grep -n "pub struct <Name>" ` in that tree answers it in seconds.
- **`wasm-bindgen` stays pinned at `=0.2.127`** to match the installed CLI.
  `scripts/build-wasm.sh` checks this before building.
- **A software adapter is a hard failure.** Never add
  `--enable-unsafe-swiftshader`. `0001` §6 explains why the obvious check was
  nearly vacuous and what replaced it.
- **Test WSL↔Windows reachability with `curl.exe`, never PowerShell
  `Invoke-WebRequest`** — it times out on proxy autodetection and looks exactly
  like a network failure. `0001` §1.
- **Anything crossing `postMessage` must be structured-cloneable.** `0001` §14.
  The world data this unit produces is far larger than a report; if any of it
  is ever sent to the main thread, this constraint applies and the cost is
  real.
- **Watch for `*/` in TypeScript comments.** `0001` §15 — a path glob closed a
  block comment and broke the parser several lines later.

---

## 1. `ANTICIPATED` — heightmap terrain with no side walls looks broken

**Symptom (expected):** the world renders as disconnected floating plateaus
with the ground visible through gaps at every height change, or with the
terrain apparently stretched into cliffs that are see-through from one side.

**Cause:** a heightmap only produces top faces. Where two adjacent tiles differ
in height there is a vertical gap that nothing fills, and with back-face
culling on, the inside of the gap is invisible.

**Planned resolution:** `E3` — emit a vertical wall for each exposed side, only
where a neighbour is genuinely lower. `E4` adds a skirt at the world edge for
the same reason.

This is called out first because it is the most likely way for the whole unit
to look wrong while every test still passes: the meshing is *correct* for what
it was asked to build, it was just asked to build too little.

---

## 2. `ANTICIPATED` — wasm and native generating different worlds

**Symptom (expected):** `world_hash` from the browser does not match the value
the native test asserts. Same seed, different world.

**Cause:** IEEE-754 pins `+ - * /` and `sqrt` exactly, but leaves
transcendental functions — `sin`, `cos`, `exp`, `powf` — to the implementation.
Rust's `std` forwards these to the platform libm, and wasm's is not the same
code as glibc's. A single `sin` in the noise path is enough to make the browser
and the test disagree.

**Why it matters more than it looks:** shareable seeds and saved worlds both
assume a seed determines a world. If that is false, it is false silently, and
it surfaces as bug reports about worlds not matching between machines long
after the cause is buried.

**Planned resolution:** `C4` forbids transcendentals in the noise path —
integer hashing and polynomial interpolation only. `D4`/`G3` make that testable
rather than assumed, by asserting the browser's `world_hash` equals the native
one.

---

## 3. `ANTICIPATED` — depth buffer not recreated on resize

**Symptom (expected):** everything renders correctly until the window is
resized, after which geometry draws through other geometry, or the frame is
garbage, or validation errors appear about attachment dimensions not matching.

**Cause:** `Renderer::resize` currently only reconfigures the surface, which is
all that was needed with no depth attachment. A depth texture is a separate
resource of a fixed size and must be recreated to match.

**Planned resolution:** `F1` — create the depth texture alongside the surface
config and recreate it in `resize`. Worth testing deliberately by resizing the
browser window, since the automated check runs at one fixed size and would
never catch it.

---

## 4. `ANTICIPATED` — the camera pitch is cheap now and expensive later

**Symptom:** not a bug. A decision that is nearly free today and very costly
once sprite art exists.

**Cause:** billboard art is drawn for one specific viewing angle. Change the
pitch afterwards and every asset is subtly wrong — objects appear to lean, or
to float above or sink into the ground.

**Planned resolution:** `A2` — render one seed at ~30°, ~45° and ~60°, compare
screenshots, and commit to a number with the reasoning written down. Decide it
from evidence while the only thing that has to change is a constant.

**Also record the convention.** "30 degree pitch" is ambiguous:
degrees-from-horizontal (side-on) and degrees-from-vertical (top-down) differ by
60°. State which is meant wherever the number appears.

---

## 5. `ANTICIPATED` — wgpu 30 API surface that unit 0001 never touched

**Symptom (expected):** compile errors on first attempt at the depth buffer,
vertex buffers and bind group layouts, in the same style as `0001` §10.

**Cause:** unit `0001` used one pipeline, no vertex buffers, one uniform and no
depth attachment. This unit adds `DepthStencilState`, `VertexBufferLayout`,
`vertex_attr_array!`, texture creation with `TextureUsages::RENDER_ATTACHMENT`,
and multiple bind groups — all unexercised API.

**Planned resolution:** read the vendored source first, as above. Known
specifics already worth remembering from `0001`:
`VertexState.buffers` is `&[Option<VertexBufferLayout>]`, and
`RenderPipelineDescriptor` needs `multiview_mask` and `cache`.

---

## 6. `ANTICIPATED` — mesh upload cost at world scale

**Symptom (expected):** generation is instant but the first frame takes a
visible pause, or memory is far higher than expected.

**Cause:** a 256×256 world is 65,536 tiles. Blocky terrain cannot share
vertices between tiles — each needs its own normal and colour — so that is
~262k vertices for top faces alone, before walls. At 36 bytes per vertex that
is roughly 9 MB of vertex data, plus indices, plus walls.

**Planned resolution:** measure rather than guess (`H3`). Chunking already
bounds any single upload. If it is a problem, the levers are a smaller world, a
larger chunk size to cut per-buffer overhead, or packing the vertex format
(normals as a face index, colour as a palette index) — but none of that should
happen before there is a number showing it is needed.

---

## Issues actually encountered

---

## 7. `OPEN` — `A2` cannot be finished in group A; the plan ordered it wrong

**Symptom:** `A2` says to pick the camera pitch by rendering the same seed at
30°, 45° and 60° and comparing. There is no terrain renderer until group `F`,
so there is nothing to render and nothing to compare.

**Cause:** a planning mistake, not a code one. The task was placed in group A
because the *decision* is a convention, but its stated method depends on output
that groups B–F produce.

**What group A did instead:**

- Fixed the **convention** — degrees above the horizontal ground plane — which
  was the genuinely ambiguous part and needed settling before anything else
  used the number.
- Defined `CAMERA_PITCH_DEGREES` as a single constant with the trade-off table
  in its doc comment, so changing it stays a one-line edit.
- Set it provisionally to **30°** to match the Mad Island reference.

**Resolution:** the committing choice moves to a new task after `F3`, when
there is terrain to photograph. Tracked there rather than left implicit here.

**Worth noting for future plans:** a task whose acceptance depends on later
work does not belong in an early group, even when its subject matter does. The
tell was that `A2` said "render" while group A produces no renderer.

---

## 8. `RESOLVED` — glam's projection API, and the trap in its naming

**Symptom:** first noticed while writing the conventions down; then `cargo
test` in group B reported `Mat4::orthographic_rh` and `orthographic_rh_gl` are
**deprecated** in glam 0.33, which `clippy -D warnings` rejects outright.

**Cause and correction.** The replacement groups projections by graphics-API
convention, and the intuitive pick is wrong:

| glam module | NDC Z | NDC Y | right for wgpu? |
|---|---|---|---|
| `opengl` | `-1..1` | up | no — wrong depth range |
| `vulkan` | `0..1` | **down** | no — renders upside down |
| `directx` | `0..1` | up | **yes** |

`vulkan` is the trap. It shares wgpu's depth range, and "Vulkan" reads as the
modern choice next to "DirectX", but it flips Y — so the world renders upside
down while every depth test still passes and nothing errors. glam's own doc
comment on the `directx` one says *"for use with DirectX and WebGPU"*.

**Resolution:** `glam::camera::rh::proj::directx::orthographic`, aliased as
`island_core::orthographic_projection` so exactly one place makes the choice.
Pinned by four tests: the chosen projection maps near→0 and far→1, the OpenGL
one maps near→-1, the Vulkan one maps +Y **down** while ours maps it up, and
+Y is up in clip space.

**Worth noting:** group A's original two tests would have passed just as
happily with the `vulkan` projection, since both put depth in `0..1`. The test
that actually distinguishes them only got written because the deprecation
forced a look at the API. A test can be true and still not be the test you
needed.

---

## 9. `RESOLVED` — back-face culling is not on by default

**Symptom:** none yet; found by checking the vendored wgpu source while
documenting winding order, rather than by hitting it.

**Cause:** `FrontFace::Ccw` genuinely is wgpu's default, so it is tempting to
assume `PrimitiveState::default()` gives sensible culling too. It does not —
`cull_mode` is an `Option<Face>` and defaults to `None`, meaning **nothing is
culled**.

**Why it matters here:** the terrain mesher emits walls, and a wall wound the
wrong way is invisible from the side it should be seen from. With culling off,
it is instead visible from *both* sides — so the winding bug is hidden until
culling is switched on later, at which point walls start disappearing and the
cause looks like a mesher regression.

**Resolution:** `F4` must set `cull_mode: Some(Face::Back)` explicitly, and
should do so from the first version rather than adding it once terrain looks
right. Recorded in `docs/architecture/coordinates.md`.

---

## 10. `RESOLVED` — a world sized in tiles can be misaligned to chunks

**Symptom:** none — designed out rather than hit.

**Cause:** the obvious constructor is `TileMap::new(width_tiles, depth_tiles)`,
which permits sizes that are not multiples of `CHUNK_SIZE`. Every partial chunk
then becomes an edge case in the mesher, the culler, and any future save
format — each of which has to defend against it independently, and any one of
which can forget.

**Resolution:** `TileMap::new` takes the size in **chunks**, and derives tiles
from it. A misaligned world is now unrepresentable, so no downstream code has
to handle one.

Preferred over validating tile counts and returning an error: an error still
means every caller has a failure path to think about, whereas this removes the
state from the type.

---

## 11. `OPEN` — `HEIGHT_STEP` is 1.0 and probably too coarse

**Symptom:** not yet visible; recorded now because the value is easy to change
today and will be load-bearing once terrain is generated.

**Cause:** heights are stored as `i16` steps, and `HEIGHT_STEP` converts a step
to world Y. At `1.0` a one-step cliff is exactly as tall as a tile is wide,
which is a very blocky, Minecraft-like vertical scale. Real terrain usually
wants finer vertical granularity than horizontal.

**Why it is separate from tile size at all:** so the vertical scale can be
retuned without touching stored data, and so it is unambiguous whether a stored
`5` means five steps or five world units.

**Resolution:** decide from the rendered terrain in group F, alongside the
camera pitch — both are "looks right" judgements and both are single constants.
Note that the two interact: a lower camera pitch exaggerates cliff faces, so
the right `HEIGHT_STEP` depends on the pitch chosen.

---

## 12. `RESOLVED` — the seed was interchangeable with the X coordinate

**The most serious defect so far, and it would have shipped looking fine.**

**Symptom:** printing the golden values before baking them showed two things
wrong in six lines of output:

```
hash_2d(0, 0, 0) => 0x00000000
hash_2d(1, 0, 0) => 0x58f54975
hash_2d(0, 0, 1) => 0x58f54975     <-- identical
```

**Cause.** The construction was the obvious one:

```rust
let h = mix(seed ^ (x as u32));
mix(h ^ (z as u32))
```

Two separate problems fall out of it:

1. **Seed and X are symmetric.** `mix(seed ^ x)` cannot tell which operand was
   which, so `(x=1, seed=0)` and `(x=0, seed=1)` hash identically. The
   consequence is not a subtle statistical flaw — **changing the seed would
   have translated the world instead of regenerating it.** Every "new" seed
   would produce the same terrain shifted by a tile.
2. **Zero is a fixed point of the mixer.** Every operation in `mix` maps 0 to
   0: `0 ^ 0 == 0` and `0 * k == 0`. So the default seed had a hard artefact
   nailed to the world origin.

**Why it would not have been caught later.** The terrain from seed 1 and seed 2
would both have looked entirely plausible. Nothing errors, nothing is NaN, the
range is right, the noise is continuous. It would have surfaced as a vague
"seeds don't seem to do much" long after the cause was buried — if at all.

**Resolution:** three rounds, each folding in one input pre-multiplied by its
own large odd constant, plus a non-zero salt so all-zero input is not a fixed
point. Both defects now have named regression tests, and a third
(`changing_the_seed_does_not_merely_translate_the_world`) checks the property
on the actual noise field across every small offset, which is the form the bug
would have taken visually.

**The lesson worth keeping.** Golden values were about to be baked in by
copying whatever the implementation produced. Had they been pasted without
being read, the tests would have locked the bug in and defended it against
future correction. **A characterisation test records behaviour; it does not
check it.** Read the values before trusting them.

---

## 13. `RESOLVED` — an assertion that was wrong about correct code

**Symptom:** `all_zero_input_does_not_hash_to_zero` failed on
`assert_ne!(mix(0), 0)`.

**Cause:** the assertion was wrong, not the code. `mix(0) == 0` is exactly the
fixed point described in §12 — it is a property of xor-shift-multiply mixers,
not a defect in this one. The salt in `hash_2d` is what keeps that fixed point
out of the world.

**Resolution:** assert `mix(0) == 0` to document that the fixed point is real,
then assert `hash_2d(0, 0, 0) != 0` to show the salt handles it. The test now
explains why the salt exists instead of asserting it away.

Recorded because the failure looked at first glance like the §12 fix not
working, which is a misleading place to start debugging from.

---

## 14. `RESOLVED` — quintic fade rather than cubic

**Symptom:** would have appeared as a faint regular grid of creases across the
terrain under directional lighting — the usual tell of a hand-rolled value
noise.

**Cause:** the common `3t² - 2t³` smoothstep has a discontinuous *second*
derivative at 0 and 1. The surface is continuous and its slope is continuous,
but its curvature jumps at every lattice line, and shading is sensitive enough
to show it.

**Resolution:** Perlin's quintic fade, `6t⁵ - 15t⁴ + 10t³`, whose first and
second derivatives are both zero at the ends. Written as nested multiplication,
so it stays inside the arithmetic-only rule from §2.

Cheap to choose correctly now and annoying to diagnose later, since the
artefact only appears once terrain is lit — group `F3`.

---

## 15. `RESOLVED` — fBm never reaches its own range, so snow never existed

**Symptom:** the first generated world contained **zero snow tiles**. The
material histogram from the `inspect_world` diagnostic:

```
params    min=-8 max=24 sea=6
heights   actual range -3..=18
  water    31216   47.6%
  sand      5252    8.0%
  grass    25934   39.6%
  rock      3134    4.8%
  snow         0    0.0%
```

**Cause.** fBm is a sum of octaves normalised by total amplitude. Reaching
either extreme of `[0, 1)` requires *every* octave to agree at that point,
which effectively never happens — so the practical output occupies a much
narrower band than the nominal range. Mapped linearly onto `min_height..=max_height`,
the terrain topped out at **18 against a `max_height` of 24**.

The material bands were computed from `max_height`, which put the snow
threshold at height 20.6 — above the highest ground in the world. Snow was not
rare; it was *unreachable*.

**Resolution:** generation is now two passes. The first lays down heights; the
second classifies them using the world's **observed** peak. Water and beach
stay absolute, because sea level is a real elevation and should not drift with
the terrain, but the land bands above them are relative — so the highest ground
is always snow-capped whatever the noise produced. Still fully deterministic:
the peak is a function of the heights, which are a function of the params.

Result: water 47.6% / sand 8.0% / grass 26.3% / rock 16.0% / snow 2.0%.

**What this says about the parameters.** `min_height` and `max_height` are
*theoretical* bounds of the noise mapping, not observed bounds of the terrain,
and their doc comments now say so. Anything that reasons about the terrain's
actual extent must measure it rather than read the params.

**How it was caught:** the `inspect_world` diagnostic, printing a histogram
before anything was baked into a test. A determinism test, a range test, and a
"sea level is water" test all passed on the broken version — none of them can
see that a band is empty. Following §12's lesson, the output was read rather
than assumed.

---

## 16. `RESOLVED` — heights could exceed `max_height` when the range was degenerate

**Symptom:** not observed in practice; found while writing the test that
asserts heights lie within the configured range.

**Cause:** `height_span()` floors at 1 so the noise mapping never divides by
zero. With `min_height == max_height` that produces a span of 1 where the true
span is 0, letting a height land one step above `max_height`.

**Resolution:** clamp the result, with bounds ordered explicitly rather than
via `clamp` — which panics if its bounds are reversed, and inverted parameters
are exactly the kind of thing a save file or a tuning UI could deliver.
Confirmed a no-op on normal parameters: the default world's hash is unchanged
by the clamp.

Now unconditional: heights lie within `min..=max` for any parameters, equal or
inverted included, and both cases have tests.
