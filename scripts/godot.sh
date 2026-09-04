#!/usr/bin/env bash
#
# Drive the Godot proof of concept in godot/.
#
#   scripts/godot.sh                    run the game
#   scripts/godot.sh edit               open the Godot editor
#   scripts/godot.sh import             (re)import assets, headless
#   scripts/godot.sh art                regenerate the placeholder PNGs
#   scripts/godot.sh check              headless smoke test (exits non-zero on failure)
#   scripts/godot.sh shot [args]        render a PNG and exit
#     e.g. scripts/godot.sh shot --at=9,6 --yaw=55 --out=turn.png
#
# Godot has to run as a *Windows* process. Vulkan cannot reach the GPU from
# inside WSL, so a Linux build silently falls back to software rendering — the
# frame still appears, which is exactly what makes it dangerous, because the
# thing being evaluated here is how it looks and how fast it draws. The Windows
# binary opens the project straight off the \\wsl.localhost share.
#
# Override the binary with GODOT_BIN=/mnt/c/path/to/Godot_console.exe.
#
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_DIR="$REPO_ROOT/godot"

if [[ -z "${WSL_DISTRO_NAME:-}" ]]; then
	echo "godot.sh: expected to be run from WSL (WSL_DISTRO_NAME is unset)." >&2
	exit 1
fi

# WSL path -> UNC path Windows can open.
PROJECT_UNC="\\\\wsl.localhost\\${WSL_DISTRO_NAME}${PROJECT_DIR//\//\\}"

find_godot() {
	if [[ -n "${GODOT_BIN:-}" ]]; then
		printf '%s' "$GODOT_BIN"
		return
	fi
	# Prefer the console build: the plain .exe detaches from the terminal on
	# Windows, so print() and errors never make it back here.
	local found
	found=$(compgen -G "/mnt/c/Users/*/Downloads/Godot_v4.*/Godot_v4.*_console.exe" || true)
	if [[ -z "$found" ]]; then
		found=$(compgen -G "/mnt/c/Users/*/Downloads/Godot_v4.*_console.exe" || true)
	fi
	if [[ -z "$found" ]]; then
		found=$(compgen -G "/mnt/c/Program Files/Godot*/Godot*_console.exe" || true)
	fi
	# Newest version last, alphabetically.
	printf '%s' "$(printf '%s\n' $found | sort -V | tail -1)"
}

GODOT="$(find_godot)"
if [[ -z "$GODOT" || ! -x "$GODOT" ]]; then
	echo "godot.sh: no Godot 4 binary found. Set GODOT_BIN to the .exe." >&2
	exit 1
fi

# Godot writes CRLF through the WSL interop pipe; strip it so grep and diff
# behave.
run() { "$GODOT" --path "$PROJECT_UNC" "$@" 2>&1 | tr -d '\r'; }

case "${1:-run}" in
	run)    run ;;
	edit)   shift; run --editor "$@" ;;
	import) run --headless --import ;;
	art)    run --headless --script res://tools/gen_placeholder_art.gd ;;
	check)  run --headless --script res://tools/smoke_test.gd ;;
	shot)   shift; run -- --shot "$@" ;;
	*)      run "$@" ;;
esac
