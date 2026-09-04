# Godot proof of concept

A "hello world" for the Don't Starve look: **a real 3D world rendered with 2D
sprites**. Written in GDScript against Godot 4.6.

This is an evaluation branch. Nothing here shares code with the Rust/`wgpu`
renderer in `crates/`; the two are parallel answers to the same question and
only one of them survives. See
[`docs/work/0003-godot-proof-of-concept/todo.md`](../docs/work/0003-godot-proof-of-concept/todo.md)
for what it is meant to prove and what it deliberately skips.

## Running it

```bash
scripts/godot.sh          # play it
scripts/godot.sh edit     # open the Godot editor
scripts/godot.sh shot     # render godot/shots/screenshot.png and exit
scripts/godot.sh art      # regenerate the placeholder PNGs
scripts/godot.sh check    # headless smoke test
```

Godot runs as a **Windows** process even though the repo lives in WSL — Vulkan
cannot reach the GPU from inside WSL, and a software fallback would tell you
nothing about how this looks or how fast it draws. `scripts/godot.sh` handles
the path translation.

| Key | |
|---|---|
| `WASD` / arrows | move, relative to the screen |
| `Q` / `E` | turn the camera |
| Mouse wheel | zoom |
| `F1` | fps / prop count / draw calls |

## Layout

| Path | |
|---|---|
| `scenes/main.tscn` | ground, light, sky, camera rig, HUD |
| `scenes/player.tscn` | capsule body + billboard sprite + blob shadow |
| `scenes/prop.tscn` | one piece of scenery, configured by `world.gd` |
| `scripts/billboard_sprite_3d.gd` | **the interesting file** — every rule that makes a flat sprite sit convincingly in a 3D scene |
| `scripts/camera_rig.gd` | fixed-pitch yaw gimbal, narrow FOV |
| `scripts/player.gd` | camera-relative movement, sprite facing, walk bob |
| `scripts/world.gd` | seeded scatter, HUD, screenshot mode |
| `tools/gen_placeholder_art.gd` | draws the placeholder PNGs; not shipped |
| `tools/smoke_test.gd` | headless assertions for the things a screenshot hides |
| `assets/` | generated placeholder art — replace freely |

## Art scale

32 texture pixels is one metre, project-wide (`BillboardSprite3D.PIXELS_PER_METRE`).
Every sprite derives its world size from its own texture, so a 96 px tree is 3 m
tall with no per-asset tuning and re-drawn art keeps its scale automatically.
