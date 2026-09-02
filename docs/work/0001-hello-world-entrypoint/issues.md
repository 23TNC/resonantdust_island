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

**Status:** open until `E5` is done.

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
