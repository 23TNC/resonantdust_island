# 0002 — Tile World Generation

**Status:** `[~]` in progress — groups A–F complete; G (shell + smoke test) remains
**Depends on:** `0001-hello-world-entrypoint` (complete)
**Goal:** Generate a deterministic, seeded tile world with terrain elevation and
render it — replacing the hello triangle with something that is actually the
beginning of the game.

**Scope:** tiles only. Billboarded objects are unit `0003`.

---

## The decision that was handed to me: 3D world, not 2D

The question was whether to build a true 3D world with a fixed-pitch camera and
2D billboard assets (Mad Island, Don't Starve) or a flat 2D world (RimWorld).

**Building the 3D world.** Four reasons, in order of weight:

1. **A heightmap only means something in 3D.** RimWorld has no elevation at all
   — it is genuinely flat, and its whole renderer assumes that. Wanting a
   heightmap and wanting RimWorld's approach are in tension; you cannot have
   terrain height and also have the simplicity that comes from not having it.

2. **The depth buffer sorts for free.** This is the big one. In a 2D world,
   drawing a sprite that stands on sloped ground correctly means sorting it
   against the terrain with a painter's algorithm — and once elevation exists,
   "which is in front" stops having a per-sprite answer and starts needing
   per-pixel resolution. In 3D that is one depth attachment and zero thought.
   Getting this wrong in 2D is the classic sprite-sorting swamp that games sink
   months into.

3. **Billboards with x/y/z origins are the 3D formulation already.** The
   premise of this work — objects positioned in 3D space — is describing a 3D
   world with 2D art. That is exactly what Mad Island is.

4. **A fixed camera removes most of what makes 3D expensive.** No rotation
   means the billboard rotation is one constant matrix, not per-object work;
   the light direction never changes; and frustum culling reduces to a fixed
   region test. We get 3D's correctness without paying for 3D's generality.

**What this costs, stated plainly:** sprite art must be drawn for one specific
camera pitch, forever. Changing the pitch after art exists invalidates the art.
That makes the pitch angle the single most expensive decision in this unit, and
`A2` treats it accordingly.

---

## Answers carried in from planning

- **Camera:** fixed pitch, fixed yaw, no rotation, top-down. Orthographic.
- **Terrain:** heightmap. **No tunnels, caves, or overhangs** — one surface
  height per `(x, z)` column. This is a real simplification and the plan leans
  on it throughout.
- **Scope:** tiles only.

---

## Target file tree

```
crates/island_core/src/
├── lib.rs
├── renderer.rs              # extended: depth buffer, camera, terrain pipeline
├── camera.rs                # NEW  orthographic fixed-pitch camera
├── world/
│   ├── mod.rs               # NEW  TileMap, chunk indexing, coordinate helpers
│   ├── tile.rs              # NEW  TileKind and per-tile data
│   ├── noise.rs             # NEW  deterministic value noise + fBm
│   ├── generate.rs          # NEW  seeded heightmap and tile classification
│   └── mesh.rs              # NEW  chunk → vertex/index buffers
├── terrain.wgsl             # NEW
└── hello.wgsl               # retained until F6 decides its fate

docs/architecture/
├── frame-loop.md            # exists
└── coordinates.md           # NEW  axes, units, camera, tile origin
```

---

## Tasks

### A. Conventions, decided before any code

- [x] `A1` Write `docs/architecture/coordinates.md`: **X** right, **Y** up,
      **Z** toward the viewer, right-handed. One tile = `1.0` world unit. A
      tile's origin is its **centre** at ground height, so a billboard placed
      at a tile's origin stands in the middle of it rather than on a corner.
      Record the wgpu NDC convention (Y up, Z in `0..1`, unlike OpenGL's
      `-1..1`) because it is a standard source of an inverted or clipped scene.
- [~] `A2` **Pin the camera pitch, and pick it from evidence rather than
      taste.** *Convention fixed and constant defined; the committing choice
      is blocked on there being terrain to look at — see `issues.md` §7.* This is the one decision here that art will be locked to.
      Define it as *degrees above the horizontal ground plane* — state the
      convention explicitly, since "30 degree pitch" is ambiguous between
      degrees-from-horizontal (side-on) and degrees-from-vertical (top-down).
      Render the same seed at ~30°, ~45° and ~60° and compare the screenshots
      before committing. Higher shows more ground; lower shows more cliff face
      and more of each billboard's front. Mad Island reads as roughly 30°.
- [x] `A3` **Heights are integer steps, not a continuous surface.** Each tile
      is a flat quad at its own height with vertical walls between differing
      neighbours. This keeps tile identity crisp — a tile is one flat thing you
      can stand an object on — and matches the blocky reference look. A smooth
      interpolated surface would make "what height is this tile" ill-defined,
      which billboard placement in `0003` would then have to solve.
- [x] `A4` Add `glam` (0.33) to `[workspace.dependencies]` and to
      `island_core`. Hand-rolling matrix maths is not a good use of the time
      and is a reliable source of subtle transposition bugs.
- [x] `A5` *(added)* Pin glam's projection convention with tests rather than a
      comment. `orthographic_rh` vs `orthographic_rh_gl` differ only in depth
      range and only by a suffix; the wrong one looks like a depth-buffer bug.
      Verified empirically, not from memory.

**Verified:** `cargo test -p island_core` 5/5 green — `orthographic_rh` maps
near→0 and far→1 (wgpu's range), the GL variant maps near→-1, and +Y is up in
clip space. Also checked against the vendored source that `FrontFace::Ccw` is
wgpu's default but **`cull_mode` defaults to `None`**, so back-face culling has
to be requested explicitly in `F4`.

### B. World data model — `island_core::world`

- [x] `B1` `TileKind` enum: `Water`, `Sand`, `Grass`, `Rock`, `Snow` to start.
      `#[repr(u8)]`, exhaustive `match` on colour so adding a kind is a compile
      error rather than an invisible black tile.
- [x] `B2` Per-tile data: `height: i16` and `kind: TileKind`. Store as parallel
      arrays (struct-of-arrays), not `Vec<Tile>` — meshing reads all heights
      and then all kinds, and the neighbour lookups in `E2` are the hot path.
- [x] `B3` `TileMap`: fixed `width × depth` in tiles, addressed by
      `(x, z)`. Chunked at `CHUNK_SIZE` (start at 32×32 — see open questions).
      A chunk is the unit of mesh rebuild and the unit of culling.
- [x] `B4` Coordinate helpers, each with its own test: world position ↔ tile
      index ↔ chunk index, plus neighbour access that returns `Option` at the
      world edge rather than wrapping or panicking. Off-by-one and silent
      wrapping here would surface as baffling visual artefacts much later.
- [x] `B5` Tests: round-trip conversions, edge and corner neighbour queries,
      out-of-bounds behaviour.
- [x] `B6` *(added)* `TilePos` and `ChunkPos` as distinct types, so passing one
      where the other belongs is a compile error rather than output that is
      wrong by exactly one chunk.
- [x] `B7` *(added)* `chunk_bounds` — tight world-space AABB per chunk, for the
      frustum culling in `F5`. Y comes from the chunk's actual height range,
      extended one step down to cover walls hanging below the lowest top face.
- [x] `B8` *(added)* `HEIGHT_STEP` — world units per height step, so the
      vertical scale is retunable without touching stored data and a stored
      `5` is unambiguously five *steps*. See `issues.md` §11.

**Verified:** 31 tests green, `clippy --all-targets -- -D warnings` clean on
both host and wasm32. `TileMap::new` takes a size in **chunks**, making a
chunk-misaligned world unrepresentable rather than an error every downstream
consumer has to handle (`issues.md` §10).

### C. Deterministic noise — `island_core::world::noise`

- [x] `C1` Integer hash (wrapping multiply / xorshift on `u32`) → `f32` in
      `[0,1)`. Seeded.
- [x] `C2` 2D value noise with smoothstep interpolation.
- [x] `C3` fBm: several octaves with configurable lacunarity and gain.
- [x] `C4` **Use no `sin`, `cos`, `powf`, or any transcendental in the noise
      path.** IEEE-754 pins `+ - * /` exactly, but transcendental functions are
      free to differ between libm implementations — which here means **wasm and
      native could generate different worlds from the same seed**. A survival
      game with shareable seeds and saved worlds cannot tolerate that. Use
      integer hashing and polynomial interpolation only.
- [x] `C5` Tests: same seed → identical output; different seeds → different
      output; output stays within `[0,1]`; **golden values** for a handful of
      fixed inputs, so an accidental change to the hash is caught rather than
      silently reshaping every future world.
- [x] `C6` *(added)* `C4` is enforced by a test that scans this module's own
      source for transcendental **call syntax**, rather than being trusted to a
      comment. Matches `.sin(` and friends, not bare words, so the module docs
      can name them.
- [x] `C7` *(added)* Regression tests for two defects found by reading the
      golden values before baking them — see `issues.md` §12.

**Verified:** 46 tests green, clippy clean on both targets. Quintic fade rather
than cubic, so lighting does not pick out a grid of creases along the lattice
lines.

### D. Generation — `island_core::world::generate`

- [x] `D1` Heightmap from fBm, quantised to integer steps per `A3`.
- [x] `D2` Classify tiles into kinds by elevation band (water below sea level,
      sand at the margin, grass, rock, snow). Bands as named constants, not
      magic numbers scattered through the function.
- [x] `D3` `WorldParams { seed, width, depth, sea_level, ... }` so generation is
      one reproducible call with no hidden global state.
- [x] `D4` `world_hash()` — a cheap order-independent-free checksum over all
      heights and kinds. Its purpose is `G3`: proving the browser generates a
      **bit-identical** world to the native test run. That is the only way to
      actually catch the wasm/native divergence `C4` guards against; without it
      the guard is an untested assumption.
- [x] `D5` Tests: determinism, height range within bounds, sea level produces
      water, a fixed seed matches a golden `world_hash`.
- [x] `D6` *(added)* Generation is **two passes**. Land material bands scale
      from the world's *observed* peak rather than from `max_height`, because
      fBm never reaches its nominal ceiling. See `issues.md` §15 — the first
      version produced **zero snow tiles**.
- [x] `D7` *(added)* An `#[ignore]`d `inspect_world` diagnostic that prints the
      height range, material histogram and an ASCII map. Kept: it is how §15
      was caught, and it is what `HEIGHT_STEP` and the camera pitch will be
      tuned against in group F.

**Verified:** 62 tests green, clippy clean on both targets. Default world is
256×256, heights `-3..=18`, materials water 47.6% / sand 8.0% / grass 26.3% /
rock 16.0% / snow 2.0%, `world_hash = 0xd34bfa9b078f3806`. That hash is what
`G3` asserts the browser reproduces.

### E. Meshing — `island_core::world::mesh`

- [x] `E1` `Vertex { position: [f32;3], normal: [f32;3], color: [f32;3] }`,
      `Pod`/`Zeroable`. Per-vertex colour rather than a texture: the asset
      pipeline is out of scope, and colour is enough to read the terrain.
- [x] `E2` Top face per tile: one flat quad at the tile's height, normal `+Y`.
- [x] `E3` **Side walls where a neighbour is lower.** Without these the world
      is a set of floating plateaus with holes at every height change — the
      single most likely way for this to look broken. Walls are what make a
      heightmap read as terrain. Emit only the faces that are actually exposed;
      a wall between two equal-height tiles is wasted geometry.
- [x] `E4` World-edge walls dropped to a skirt height, so the world does not
      appear to float.
- [x] `E5` One vertex/index buffer per chunk, built on the CPU, uploaded once.
- [x] `E6` Tests: a flat chunk emits exactly `CHUNK_SIZE²` quads and no walls;
      a single raised tile emits four walls; a chunk at the world edge emits
      its skirt. Vertex and index counts asserted exactly — cheap tests that
      catch most meshing regressions.
- [x] `E7` *(added)* `every_triangle_winding_matches_its_normal` — recomputes
      the geometric normal of all 180k emitted triangles and checks it agrees
      with the stored one. The four wall corner orders are hand-derived, and a
      quad wound backwards is not subtly wrong but **invisible**, from exactly
      the side you were looking from. All four were correct first time; without
      this test that would have been luck rather than knowledge.
- [x] `E8` *(added)* `measure_mesh` diagnostic, answering `issues.md` §6 and
      `H3` with numbers instead of estimates.

**Measured** (default 256×256 world, release):

| | |
|---|---|
| generate | 7.0 ms |
| mesh | 11.3 ms |
| quads | 90,892 (**1.39 per tile**, vs 5 worst case) |
| triangles | 181,784 |
| vertices | 363,568 |
| vertex data | 12.48 MiB (36 B/vertex) |
| index data | 2.08 MiB (u32) |
| **total** | **14.56 MiB** |
| largest chunk | 6,192 vertices |

These are **startup** costs — `mesh_world` runs once and the buffers are then
just drawn. The recurring cost is re-meshing after a terrain edit, measured
separately at **38 µs per chunk** (77 µs for an edit on a chunk border, 154 µs
on a corner, against a 16,667 µs frame budget) — so editing can re-mesh
synchronously without a job system.

Comfortable. Walls cost far less than feared because real terrain is mostly
flat locally. Also note the largest chunk is well under `u16::MAX`, so 16-bit
indices are viable — but they would save only 1.04 MiB of 14.56, since vertex
data dominates. Not worth doing without a reason.

### F. Renderer — `island_core::renderer`

- [x] `F1` **Depth buffer.** `Depth32Float` texture, created with the surface
      and **recreated on resize**. Forgetting the recreate is the classic bug:
      it works until the window changes size, then renders through geometry.
      `Renderer::resize` currently only reconfigures the surface.
- [x] `F2` `camera.rs`: orthographic projection with fixed pitch and yaw, a
      settable focus point, and a settable vertical extent (zoom). Uploads a
      view-projection matrix as a uniform. Fixed orientation means the matrix
      only changes when the focus or zoom does.
- [x] `F3` `terrain.wgsl`: transform by view-projection, flat directional
      light against the vertex normal plus a small ambient term. Shading is
      what makes elevation legible — an unlit heightmap of flat colours reads
      as noise.
- [x] `F4` Terrain pipeline with depth testing enabled, `LessEqual`, and back
      face culling. Draw one call per visible chunk.
- [x] `F5` Per-chunk frustum culling. With a fixed camera this is a cheap
      AABB test, and it validates that the chunk decomposition in `B3` is
      actually good for something.
- [x] `F6` Decide the hello triangle's fate: keep it behind a debug flag as a
      known-good pipeline to fall back on, or delete it. Record which and why.

      **Deleted.** It existed to prove the Rust → wasm → worker → wgpu path,
      and the terrain pipeline now exercises strictly more of it — same
      surface, same device, plus vertex buffers, a depth buffer and culling.
      Keeping it would mean maintaining a shader, a uniform struct and a
      pipeline that nothing runs. Its WGSL-validation test pattern moved to
      `terrain.wgsl`, which is the part that was actually worth keeping. Git
      has it if it is ever wanted.

**`A2` closed — camera pitch is 30°.** Decided from rendered comparisons of
one seed at 30°, 45° and 60°, as the task asked. Cliff faces are clearly
legible at 30° and have nearly vanished by 60°, exactly as `cos θ` predicts —
and cliff faces are what make a stepped heightmap read as terrain at all. 30°
also matches the Mad Island reference and costs an upright billboard 13% of its
height rather than 50%.

**`issues.md` §11 closed — `HEIGHT_STEP` is 2.0**, from comparisons at 1.0,
2.0 and 3.0. At 1.0 the terrain reads as a coloured *map*: the cliffs are there
and correctly lit, just too shallow to see. At 3.0 they grow tall enough to
occlude the ground behind them, which hides playable space. 2.0 reads as
terraced landscape with almost nothing hidden. **This is a design commitment,
not only a visual one** — a two-unit terrace is taller than a person, so every
height change is a barrier to route around rather than a step up.

### G. Web integration

- [ ] `G1` `island_web` exports: `set_camera_focus(x, z)`, `set_camera_zoom(h)`,
      `world_hash()`, and world dimensions for display.
- [ ] `G2` Shell shows seed, world size, chunk count and `world_hash` next to
      the existing adapter report. Keep the adapter report — it is what makes a
      software-adapter fallback visible, per `0001` issue §6.
- [ ] `G3` Extend `scripts/test-chrome.mjs`: assert `world_hash` from the
      browser **equals the value the native test asserts**, move the camera and
      screenshot two distinct regions, and keep every `0001` assertion
      (no software adapter, frame count advancing, no `SEVERE` console output).
- [ ] `G4` Since the camera cannot be driven by input yet, have it pan slowly
      and automatically so the world is visibly larger than one screen. Removes
      the temptation to fold input plumbing into this unit.

### H. Wrap up

- [ ] `H1` `issues.md` — written during the work, not after.
- [ ] `H2` Finish `docs/architecture/coordinates.md` with what the work
      actually settled, including the chosen pitch and the reasoning.
- [ ] `H3` Record measured numbers: vertices and draw calls per frame, mesh
      build time, frame time at the chosen world size. Unit `0003` adds
      billboards on top of this budget and will need to know what is left.

---

## Acceptance criteria

1. `cargo test --workspace` passes on the host, including noise and
   `world_hash` golden-value tests.
2. `cargo clippy --workspace -- -D warnings` clean for both targets.
3. The page renders a recognisable tile world: water, beaches, grass, rock,
   snow by elevation, with **visible cliff faces** at height changes.
4. Terrain is lit well enough that elevation reads at a glance.
5. `world_hash` in the browser is **bit-identical** to the native test value.
6. The camera pans and the world extends beyond one screen.
7. No `SEVERE` console output; adapter still reports real hardware.
8. `scripts/test-chrome.mjs` exits 0 and captures screenshots of two regions.
9. `docs/architecture/coordinates.md` exists and matches the code.

---

## Explicitly out of scope

- **Billboards and any object rendering** — unit `0003`.
- Textures, atlases, any asset pipeline. Vertex colours only.
- Input handling and player-controlled camera.
- Collision, pathfinding, physics.
- Infinite or streaming worlds. Finite world, chunk-shaped so streaming can be
  added without redesign.
- Saving and loading.
- Water animation, shadows, ambient occlusion, any lighting beyond one
  directional term plus ambient.
- Biomes as a system (moisture, temperature). Elevation bands only.
- Caves, tunnels, overhangs — ruled out by the heightmap decision.

---

## Open questions to answer while doing the work

- **Chunk size.** Starting at 32×32. Too small wastes draw calls, too large
  wastes mesh rebuilds when one tile changes. Measure before settling; record
  the number in `H3`.
- **World size for now.** Large enough that panning is meaningful, small enough
  to generate and mesh instantly. Somewhere around 256×256 as a starting point.
- **Does `world_hash` actually match across wasm and native?** `C4` is
  reasoning, `G3` is the test. If they diverge, the noise path has a
  transcendental in it and must be found.
- **Is per-vertex colour enough to read the terrain,** or do tiles need a
  visible grid line to make individual tiles distinguishable? Cheap to add in
  the shader; decide from the screenshots.
- **How much of `Renderer` should become generic** now that it holds a second
  pipeline? Resist restructuring on two data points — note the friction and
  let unit `0003` decide with three.
