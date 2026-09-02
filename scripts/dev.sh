#!/usr/bin/env bash
#
# Build the wasm, then start the Vite dev server.
#
# The server binds 0.0.0.0 so Chrome on Windows can reach it. Load it at
# http://localhost:5173 — NOT the WSL IP. WebGPU requires a secure context,
# and a bare http:// IP is not one, so navigator.gpu would be undefined there.
# See docs/work/0001-hello-world-entrypoint/issues.md §1.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

"$REPO_ROOT/scripts/build-wasm.sh" "$@"

cd "$REPO_ROOT/web"
[[ -d node_modules ]] || { echo "==> npm install"; npm install; }

echo "==> vite dev server — open http://localhost:5173 in Windows Chrome"
exec npm run dev
