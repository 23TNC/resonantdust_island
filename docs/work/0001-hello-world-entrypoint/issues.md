# 0001 — Issues

Running log of problems hit while doing `todo.md`. Entries are added as they
happen. Resolved issues stay in the file with the resolution attached.

**Legend:** `OPEN` · `RESOLVED` · `ANTICIPATED` (predicted from the environment
survey, not yet actually hit) · `WONTFIX`

---

## Environment survey (verified before starting)

Recorded so that later failures can be attributed correctly.

| Thing | Value | How checked |
|---|---|---|
| Host | WSL2, Linux 6.6.87.2-microsoft-standard-WSL2 | `uname` |
| `cargo` / `rustc` | 1.97.1 | `cargo --version` |
| Rust targets installed | `wasm32-unknown-unknown`, `x86_64-unknown-linux-gnu` | `rustup target list --installed` |
| `wasm-bindgen` CLI | **0.2.127** | `wasm-bindgen --version` |
| `wasm-pack` | **not installed** | `which wasm-pack` |
| node / npm | v22.20.0 / 10.9.3 | `node --version` |
| crates.io reachable | yes | `cargo search wgpu` |
| latest `wgpu` | 30.0.1 | `cargo search wgpu` |
| WSL IP | `172.16.15.60` | `hostname -I` |
| Windows host (from WSL) | `10.255.255.254` (resolv.conf nameserver) | `cat /etc/resolv.conf` |
| Chrome | `/mnt/c/Program Files/Google/Chrome/152.0.7977.42/chrome-win64/chrome.exe` | `ls` |
| chromedriver | `/mnt/c/Program Files/Google/Chrome/152.0.7977.42/chromedriver-win64/chromedriver.exe` | `find` |

Notes:
- `wasm-pack` is absent, and the plan does not use it — `cargo build` plus the
  `wasm-bindgen` CLI directly is fewer moving parts and gives exact control
  over the output directory. No action needed.
- The `wasm32-unknown-unknown` target is already installed, so `rust-toolchain.toml`
  declaring it is belt-and-braces for a fresh checkout, not a fix for this machine.

---

## 1. `ANTICIPATED` — WebGPU requires a secure context; the WSL IP is not one

**Symptom (expected):** loading the page from Windows at
`http://172.16.15.60:5173` serves fine, but `navigator.gpu` is `undefined` in
both the page and the worker, and `wgpu` fails to find an adapter with a
confusing "no suitable adapter" error rather than "insecure context".

**Cause:** WebGPU is a powerful feature gated on secure contexts. `https://` and
`http://localhost` (and `127.0.0.1`) qualify; a plain-`http` LAN IP does not.
`172.16.15.60` is a plain-http origin, so the API is simply not exposed.

**Planned resolution, in order of preference:**
1. Use `http://localhost:5173` from Windows. WSL2 forwards ports listening
   inside the VM to the Windows loopback, so this normally just works provided
   Vite binds `0.0.0.0` (`server.host = true`) rather than only the WSL-internal
   `127.0.0.1`.
2. If forwarding does not work, launch Chrome with
   `--unsafely-treat-insecure-origin-as-secure=http://172.16.15.60:5173`
   together with `--user-data-dir` (the flag is ignored without a separate
   profile).
3. Last resort: serve the dev server over HTTPS with a self-signed certificate
   and accept the warning.

**Status:** open until `F2` is done.

---

## 2. `RESOLVED` — `wasm-bindgen` crate/CLI version mismatch (did not occur)

**Symptom (expected):** `wasm-bindgen` CLI aborts with a message about the
schema version of the `.wasm` not matching the CLI, naming two `0.2.x`
versions.

**Cause:** the `wasm-bindgen` crate embeds a schema version in the produced
module, and the CLI refuses anything it does not recognise. The crate version
in `Cargo.toml` and the installed CLI must match exactly.

**Planned resolution:** pin `wasm-bindgen = "=0.2.127"` in
`[workspace.dependencies]` to match the installed CLI. If `wgpu` 30 transitively
requires a newer `wasm-bindgen`, upgrade the CLI instead
(`cargo install -f wasm-bindgen-cli --version <x>`) and update the pin — do not
downgrade `wgpu` to preserve a CLI version.

**Outcome (group A):** did not occur. `wgpu 30.0.1` resolves against
`wasm-bindgen =0.2.127` with no conflict, and the pin is in
`[workspace.dependencies]` with a comment explaining why it is exact. The CLI
did not need upgrading. The pin is load-bearing — see §8.

---

## 3. `ANTICIPATED` — dedicated workers have no `requestAnimationFrame`

**Symptom (expected):** the obvious implementation — run the frame loop inside
the worker with `requestAnimationFrame` — fails, because `rAF` is not defined
on `DedicatedWorkerGlobalScope`.

**Cause:** `requestAnimationFrame` is tied to the document's rendering
lifecycle, which workers do not have. Only the main thread can observe vsync.

**Planned resolution:** the main thread owns the clock. `boot.ts` runs the `rAF`
loop and posts `{ type: 'tick', t }` to the worker; the worker renders on
receipt. `setTimeout`/`setInterval` inside the worker is the alternative but is
not vsync-aligned and will tear or stutter.

This is a real architectural decision, not a workaround — it is written up in
`todo.md` §E4 and `H2` so later systems inherit it deliberately.

**Status:** open until `E4` is done.

---

## 4. `ANTICIPATED` — Vite may not resolve the `.wasm` URL inside a worker bundle

**Symptom (expected):** the page loads, the worker starts, then `init()` throws
a 404 or a `WebAssembly.instantiate` failure because the glue asked for
`island_web_bg.wasm` at a path Vite did not emit.

**Cause:** `wasm-bindgen --target web` glue resolves its payload with
`new URL('island_web_bg.wasm', import.meta.url)`. Vite normally rewrites that
into a hashed asset URL, but the rewrite has to survive being pulled into a
separate worker bundle.

**Planned resolution:** if it breaks, import the wasm explicitly on the worker
side and pass the URL to `init()`:
```ts
import wasmUrl from './generated/island_web_bg.wasm?url'
await init({ module_or_path: wasmUrl })
```
This is deterministic and does not depend on the rewrite.

**Status:** open until `E5` is done. **Premise confirmed (group C):** the
generated glue does exactly this — `island_web.js` line ~1579 reads
`module_or_path = new URL('island_web_bg.wasm', import.meta.url)` when called
with no argument, and accepts an explicit URL otherwise. So the documented
fallback is available and known to work.

---

## 5. `ANTICIPATED` — reaching a Windows-side chromedriver from WSL

**Symptom (expected):** `scripts/test-chrome.mjs` running under WSL cannot
connect to `http://localhost:9515` after starting `chromedriver.exe`, because
that `localhost` is the WSL VM, not Windows.

**Cause:** port forwarding is one-directional by default. WSL→Windows requires
addressing the Windows host explicitly; only Windows→WSL gets the automatic
`localhost` mapping.

**Planned resolution:** resolve the Windows host IP at runtime
(`/etc/resolv.conf` nameserver, `10.255.255.254` on this machine — it is *not*
stable across reboots, so read it rather than hard-coding) and point the
WebDriver client at `http://<host>:9515`. Also pass
`--allowed-ips=<wsl-ip>` to `chromedriver.exe`, which by default only accepts
connections from the local machine and will otherwise reject WSL as a remote
client.

**Status:** open until `G2` is done.

---

## 6. `ANTICIPATED` — a software adapter would make the whole test meaningless

**Symptom (expected):** everything "works" — triangle renders, no errors — but
the adapter is SwiftShader / a warp device, which is exactly the failure mode
the Windows-side testing requirement exists to avoid.

**Cause:** Chrome falls back to a software WebGPU implementation when it cannot
get a hardware adapter, and the fallback is silent. Under headless in
particular, `--enable-unsafe-swiftshader` (or a driver problem) turns a real
failure into a passing test.

**Planned resolution:** the adapter name and backend are surfaced to the page
(`HelloReport`) and asserted in `test-chrome.mjs`. A software adapter is a test
failure, not a pass. Do **not** add `--enable-unsafe-swiftshader` to make a
headless run go green. Cross-check `chrome://gpu` on first run.

**Status:** open until `F4`/`G1` are done.

---

## 7. `RESOLVED` — the working directory is not a git repository

**Symptom:** no version control; nothing here is recoverable if it is
overwritten.

**Resolution:** `git init`, first commit, pushed to
`git@github.com:23TNC/resonantdust_island.git` on `main`.

**Incidental finding:** the global git identity was
`user.name = PurpleDimension`, `user.email = None` — the literal string
`None`, not an address. Commits made with it would author as
`PurpleDimension <None>` and GitHub would not attribute them to the account.
Set `user.email` **repo-locally** to the account address; global config left
untouched. Worth checking before the first commit on any future clone.

---


---

## 8. `RESOLVED` — dependency versions verified for wasm32

Not a failure — recorded because these versions are now load-bearing and the
next unit of work should not casually bump them.

Resolved and **compiled** with `cargo check --target wasm32-unknown-unknown`
(125 packages, 48s clean):

| Crate | Version | Note |
|---|---|---|
| `wgpu` | 30.0.1 | latest published |
| `wasm-bindgen` | **0.2.127** | pinned `=`, must match the installed CLI |
| `wasm-bindgen-futures` | 0.4.77 | |
| `web-sys` | 0.3.104 | |
| `js-sys` | 0.3.104 | |
| `bytemuck` | 1.25.2 | `derive` feature on |
| `thiserror` | 2.0.20 | |
| `log` | 0.4.x | |
| `console_log` | 1.1.0 | |
| `console_error_panic_hook` | 0.1.7 | |

The verification was done with a throwaway `crates/_validate` member that
depended on all ten, then deleted. This proves the whole graph builds for the
real target *before* any of our own code exists, so a compile failure in group
B or C is unambiguously our code rather than a bad version pin.

`Cargo.lock` was deliberately **not** committed at this stage: the only lock
that existed named the deleted `_validate` crate. It gets generated for real
in `B1` and committed then — it is an application workspace, so the lock does
belong in version control.

---

## 9. `RESOLVED` — an empty workspace does not parse

**Symptom:** with `members = ["crates/*"]` and no crates yet, every cargo
command fails:

```
error: failed to load manifest for workspace member `.../crates/*`
Caused by: failed to read `.../crates/*/Cargo.toml`
```

**Cause:** cargo does not glob-expand a members pattern to zero matches — it
falls back to treating the literal `crates/*` string as a member path. Creating
an empty `crates/` directory does not help; the glob needs at least one real
member.

**Resolution:** none needed beyond sequencing. The workspace manifest is
correct; it simply cannot be exercised until `B1` creates `island_core`.
Verified in the meantime via the throwaway member described in §8.

**Consequence for anyone picking this up mid-stream:** between the end of group
A and the start of group B, `cargo` commands at the repo root will fail with
the error above. This is expected, not a broken checkout.

---

## 10. `RESOLVED` — wgpu 30 differs substantially from the documented wgpu API

**Symptom:** the renderer, written against the `wgpu` API as it is described in
essentially all available examples and tutorials, failed to compile with 10
errors on the first `cargo check`.

**Cause:** almost every wgpu example in circulation targets roughly 0.20–25.
Version 30 renamed and restructured a lot of the core surface. None of it is
subtle once seen, but all of it is invisible until the compiler objects.

**The actual differences, for reference:**

| Written from memory / docs | wgpu 30.0.1 |
|---|---|
| `Instance::new(&InstanceDescriptor::default())` | `Instance::new(InstanceDescriptor::new_without_display_handle())` — by value, and there is **no** `Default` impl |
| `surface.get_current_texture() -> Result<_, SurfaceError>` | returns a `CurrentSurfaceTexture` **enum**; `wgpu::SurfaceError` no longer exists |
| `frame.present()` | `queue.present(frame)` — moved onto `Queue` |
| `PipelineLayoutDescriptor { push_constant_ranges }` | `{ immediate_size: u32 }` — push constants are now "immediate data" |
| `bind_group_layouts: &[&BindGroupLayout]` | `&[Option<&BindGroupLayout>]` |
| `RenderPipelineDescriptor { multiview }` | `{ multiview_mask }` |
| `RenderPassDescriptor` (5 fields) | also requires `multiview_mask` |
| `RenderPassColorAttachment { view, resolve_target, ops }` | also requires `depth_slice` |
| `VertexState { buffers: &[VertexBufferLayout] }` | `&[Option<VertexBufferLayout>]` |

`CurrentSurfaceTexture` has **seven** variants — `Success`, `Suboptimal`,
`Timeout`, `Occluded`, `Outdated`, `Lost`, `Validation` — and the compiler
enforces handling all of them. This is an improvement on the old `Result`:
none of these are conditions the caller can meaningfully propagate, so
modelling them as errors was always slightly wrong. `Renderer::render` returns
a `FrameStatus` (`Presented` / `Skipped`) accordingly.

**Resolution:** read the vendored source in
`~/.cargo/registry/src/*/wgpu-30.0.1/src/api/` rather than trusting recalled or
online API shapes. That is the authoritative reference for this version, and
grepping it for `pub struct <Name>` resolves any of these in seconds.

**Carry forward:** when `island_web` (group C) or any later renderer work is
written, check struct fields against the vendored source first. Expect the same
class of mismatch in any wgpu example copied from the internet.

---

## 11. `RESOLVED` — WGSL errors were only discoverable in a browser

**Symptom:** not a failure, a gap. `device.create_shader_module` compiles WGSL
at runtime, so a shader typo would first appear as a console error after a full
rebuild, redeploy and page load — and on the far side of the WSL/Windows
boundary at that.

**Resolution:** added `naga` as a direct dev-dependency (it is already in the
tree via `wgpu`, so this costs nothing) and a `cargo test` that runs the same
WGSL frontend wgpu uses, parsing and validating `hello.wgsl`. A second test
asserts the Rust `Uniforms` struct stays 16 bytes, since it is shared with the
shader by memory layout alone and a size change on either side would silently
corrupt every field.

Verified the test is not vacuous by deliberately breaking the shader: it fails
with `error: invalid field accessor ...` and names the line.

**Note:** `wgpu::naga` is re-exported but behind `#[cfg(all(not(wgpu_core),
naga))]`, so it is not reliably reachable across both targets. Depending on
`naga` directly avoids the cfg entirely.

---

## 12. `RESOLVED` — `OffscreenCanvas` has no `Into<SurfaceTarget>`

**Symptom:** `island_web` failed to compile with six errors, all variations of
`the trait bound OffscreenCanvas: Into<wgpu::SurfaceTarget<'static>> is not
satisfied`, with the compiler unhelpfully suggesting `HasWindowHandle` impls.

**Cause:** `SurfaceTarget` does have an `OffscreenCanvas` variant, but wgpu's
only `From` impl for it is a blanket `impl<'a, T> From<T> for SurfaceTarget<'a>
where T: DisplayAndWindowHandle` — i.e. raw window handles. A web canvas is not
a window handle, so the variant has to be named explicitly.

**Resolution:** construct `SurfaceTarget::OffscreenCanvas(canvas)` in
`island_web`, which is the right place for it — building a platform-specific
surface target is precisely the platform adapter's job, and `island_core` stays
free of browser types.

That requires wgpu types in `island_web`. Rather than declare wgpu as a second
direct dependency (where a version skew between the two crates would surface as
a baffling trait-mismatch error), `island_core` now does `pub use wgpu;` and
`island_web` goes through `island_core::wgpu`. The shared version becomes
structural rather than a convention two manifests have to keep agreeing on.

---

## 13. `RESOLVED` — `island_web` cannot compile natively, breaking `cargo test`

**Symptom:** `cargo test --workspace` failed:

```
error[E0599]: no variant, associated function, or constant named
`OffscreenCanvas` found for enum `SurfaceTarget<'window>`
```

**Cause:** the `OffscreenCanvas` variant is `#[cfg]`-gated to wasm targets, so
`island_web` is inherently wasm-only. Nothing wrong with that — but the default
`cargo test` and `cargo check` build for the host, so a wasm-only member makes
both fail at the workspace root.

**Resolution:** `#![cfg(target_arch = "wasm32")]` on the crate root, so
`island_web` compiles to an empty crate off wasm. `cargo test --workspace` and
`cargo check --workspace` now both pass on the host, and the wasm32 target is
unaffected — verified by re-running `wasm-bindgen` afterwards and confirming all
five exports survive.

Everything worth unit-testing lives in `island_core`, which builds on both
targets, so nothing is lost by making `island_web` inert on the host.

---

## 14. `RESOLVED` — the adapter report cannot be a `#[wasm_bindgen]` struct

**Symptom:** not a failure — caught while designing `start`'s return type,
before writing code that would have failed confusingly at runtime.

**Cause:** the natural way to return structured data from Rust to JS is an
exported `#[wasm_bindgen]` struct, which gives real TypeScript types. But the
worker immediately forwards this report to the main thread via `postMessage`,
which **structured-clones** its argument. A wasm-bindgen struct is a JS object
wrapping a pointer into wasm linear memory: it is not structured-cloneable, and
would either throw `DataCloneError` or arrive as something meaningless.

**Resolution:** build a plain object with `js_sys::Object` and `Reflect::set`.

**Cost, carried into group E:** the generated `.d.ts` types `start` as
`Promise<any>`, so `worker.ts` must declare its own interface describing the
report's shape. That interface and `report_to_js` are two places that have to
agree by hand — worth a comment on both sides.

**General rule for later work:** anything crossing a `postMessage` boundary
must be structured-cloneable. This will come up again the moment game state is
shared between the worker and the main thread.
