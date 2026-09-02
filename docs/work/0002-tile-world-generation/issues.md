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

## 8. `RESOLVED` — glam has two orthographic projections one suffix apart

**Symptom:** not a failure — caught while writing the conventions down rather
than after a scene rendered wrong.

**Cause:** `Mat4::orthographic_rh` and `Mat4::orthographic_rh_gl` differ only in
depth range. wgpu clip space is `0..1`; OpenGL's is `-1..1`. Picking the GL
variant gives a scene that is clipped or z-fights, and the symptom points at
the depth buffer rather than at the projection matrix.

**Resolution:** verified empirically instead of trusting the naming, and pinned
with three tests in `camera.rs` — near maps to 0 and far to 1 under
`orthographic_rh`, the GL variant maps near to -1, and +Y is up in clip space.
A glam upgrade that changed the convention would now fail the build rather than
change the picture.

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
