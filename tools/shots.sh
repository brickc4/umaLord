#!/usr/bin/env bash
# Screenshots of the app on Linux without touching the desktop: each shot runs the release build
# inside its own headless sway (a compositor with no monitor or GPU, software rendering, its own
# socket in a private runtime folder), so the size is exact and nothing shows on screen.
#
# Usage: tools/shots.sh <out dir> <account folder> <name> <WxH> [VAR=value ...]
#   e.g. tools/shots.sh /tmp/shots accounts/mine home-dark 1600x1000 UMALORD_TAB=home UMALORD_THEME=dark
# Writes <out dir>/<name>-<WxH>.png. It builds the release binary first.
# Needs nix (it runs sway from nixpkgs) and the devShell's LD_LIBRARY_PATH, which it takes from
# `nix develop`. The output folder must be outside the repo.
set -euo pipefail
out=$1 root=$2 name=$3 size=$4; shift 4
repo=$(cd "$(dirname "$0")/.." && pwd)
case "$(realpath -m "$out")/" in "$repo"/*) echo "shots.sh: write outside the repo" >&2; exit 2 ;; esac
mkdir -p "$out"
png="$(realpath -m "$out")/$name-$size.png"
rm -f "$png"
# Build first: a stale binary makes a render that doesn't show the latest change.
(cd "$repo/app" && nix develop -c bash -c 'cargo build --release -q' >/dev/null 2>&1) || { echo "shots.sh: build failed" >&2; exit 1; }
ld=$(cd "$repo" && nix develop -c bash -c 'echo "$LD_LIBRARY_PATH"' 2>/dev/null | tail -1)
# A short private runtime folder: sway's socket path must fit in sun_path.
run=$(mktemp -d /tmp/umx.XXXXXX)
trap 'rm -rf "$run"' EXIT
chmod 700 "$run"
# The app runs from a launcher script, then tells sway to exit: sway's config splits commands at ";".
{
  echo '#!/bin/sh'
  printf 'env LD_LIBRARY_PATH=%q LIBGL_ALWAYS_SOFTWARE=1 UMALORD_ROOT=%q UMALORD_LIVE=off UMALORD_SCREENSHOT=%q' "$ld" "$(realpath "$root")" "$png"
  for v in "$@"; do printf ' %q' "$v"; done
  printf ' %q\n' "$repo/app/target/release/umalord"
  echo 'swaymsg exit'
} > "$run/run.sh"
chmod +x "$run/run.sh"
{
  echo "output HEADLESS-1 mode $size position 0 0"
  echo "default_border none"
  echo 'for_window [app_id=".*"] fullscreen enable'
  echo "exec $run/run.sh"
} > "$run/sway.conf"
timeout 240 nix shell nixpkgs#sway -c env -u WAYLAND_DISPLAY -u SWAYSOCK -u DISPLAY \
  XDG_RUNTIME_DIR="$run" WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
  sway -c "$run/sway.conf" > "$run/sway.log" 2>&1 || true
if [ -s "$png" ]; then echo "$png"; else echo "shots.sh: no screenshot for $name ($size)" >&2; tail -5 "$run/sway.log" >&2; exit 1; fi
