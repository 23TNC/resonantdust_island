#!/usr/bin/env bash
#
# Build island_web to wasm and generate the JS/TS bindings the web shell
# imports.
#
#   scripts/build-wasm.sh            release build (default)
#   scripts/build-wasm.sh --debug    debug build, keeps DWARF for stack traces
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CRATE=island_web
TARGET=wasm32-unknown-unknown
OUT_DIR="web/src/generated"

PROFILE=release
CARGO_FLAGS=(--release)
BINDGEN_FLAGS=()

for arg in "$@"; do
  case "$arg" in
    --debug)
      PROFILE=debug
      CARGO_FLAGS=()
      # Without this wasm-bindgen strips the debug sections, and a Rust panic
      # in the browser becomes an address instead of a stack trace.
      BINDGEN_FLAGS=(--keep-debug)
      ;;
    --release) ;;
    -h|--help)
      sed -n '2,8p' "${BASH_SOURCE[0]}" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

# The wasm-bindgen crate embeds a schema version that the CLI must recognise
# exactly. A mismatch produces a long, confusing error at the end of a slow
# build, so check it up front where the fix is obvious.
# See docs/work/0001-hello-world-entrypoint/issues.md §2.
if ! command -v wasm-bindgen >/dev/null 2>&1; then
  echo "error: wasm-bindgen CLI not found on PATH." >&2
  echo "       install it with: cargo install -f wasm-bindgen-cli --version <pinned>" >&2
  exit 1
fi

PINNED="$(grep -oE 'wasm-bindgen = "=[0-9]+\.[0-9]+\.[0-9]+"' Cargo.toml \
          | head -1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+')"
INSTALLED="$(wasm-bindgen --version | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"

if [[ -n "$PINNED" && "$PINNED" != "$INSTALLED" ]]; then
  echo "error: wasm-bindgen version mismatch." >&2
  echo "       Cargo.toml pins  =$PINNED" >&2
  echo "       CLI on PATH is    $INSTALLED" >&2
  echo "       fix with: cargo install -f wasm-bindgen-cli --version $PINNED" >&2
  exit 1
fi

echo "==> cargo build ($PROFILE) --target $TARGET"
cargo build -p "$CRATE" --target "$TARGET" "${CARGO_FLAGS[@]}"

WASM="target/$TARGET/$PROFILE/$CRATE.wasm"
[[ -f "$WASM" ]] || { echo "error: expected $WASM to exist" >&2; exit 1; }

echo "==> wasm-bindgen --target web --out-dir $OUT_DIR"
mkdir -p "$OUT_DIR"
wasm-bindgen --target web --out-dir "$OUT_DIR" "${BINDGEN_FLAGS[@]}" "$WASM"

SIZE="$(du -h "$OUT_DIR/${CRATE}_bg.wasm" | cut -f1)"
echo "==> done: $OUT_DIR/${CRATE}_bg.wasm ($SIZE, $PROFILE)"
