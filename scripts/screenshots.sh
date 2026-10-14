#!/usr/bin/env bash
# Screenshots of Forge's main screens, light and dark, so "run it and look
# at it" is one command. Linux only (Xvfb, xdotool, ImageMagick's import).
#
#   cargo build && scripts/screenshots.sh [out-dir]
#
# It runs the debug build on a virtual display (:99, 1280x860) in demo mode
# (FORGE_DEMO=1: scripted replies, no model needed), with a throwaway config
# folder (FORGE_CONFIG_DIR, FORGE_HOME) and this repo as the project, then
# drives it with xdotool and saves PNGs to out-dir (default: screenshots/):
#
#   welcome, chat, map, split, projects, and every Settings section,
#   each as <name>-light.png and <name>-dark.png.
#
# Needs: Xvfb, xdotool, imagemagick, and a Vulkan driver (on a machine with
# no GPU, Mesa's lavapipe: apt install mesa-vulkan-drivers). The clicks on
# the Settings list are positions in a 1280x860 window; if the list changes,
# update SETTINGS below.

set -euo pipefail

cd "$(dirname "$0")/.."
OUT="${1:-screenshots}"
mkdir -p "$OUT"
BIN="target/debug/forge-gpui"
[ -x "$BIN" ] || { echo "Build first: cargo build" >&2; exit 1; }
for tool in Xvfb xdotool import; do
  command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 1; }
done

export DISPLAY=:99
TMP="$(mktemp -d)"
XVFB_PID=""
APP_PID=""
cleanup() {
  [ -n "$APP_PID" ] && kill "$APP_PID" 2>/dev/null || true
  [ -n "$XVFB_PID" ] && kill "$XVFB_PID" 2>/dev/null || true
  rm -rf "$TMP"
}
trap cleanup EXIT

if ! xdotool getdisplaygeometry >/dev/null 2>&1; then
  Xvfb :99 -screen 0 1280x860x24 >/dev/null 2>&1 &
  XVFB_PID=$!
  sleep 1
fi

# A name, so the app skips onboarding; lavapipe when there's no GPU.
mkdir -p "$TMP/config" "$TMP/home"
echo "Forge" > "$TMP/config/name.txt"
if [ -z "${VK_ICD_FILENAMES:-}" ] && [ -f /usr/share/vulkan/icd.d/lvp_icd.json ] && [ ! -e /dev/dri ]; then
  export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
fi

FORGE_DEMO=1 FORGE_CONFIG_DIR="$TMP/config" FORGE_HOME="$TMP/home" FORGE_PROJECT_DIR="$PWD" \
  "$BIN" >"$TMP/app.log" 2>&1 &
APP_PID=$!
sleep 6
WID="$(xdotool search --pid "$APP_PID" | tail -1)"
xdotool windowmove "$WID" 0 0 windowsize "$WID" 1280 860
sleep 1

key() { xdotool key --clearmodifiers "$@"; sleep 0.7; }
click() { xdotool mousemove "$1" "$2" click 1; sleep "${3:-0.8}"; }
scroll() { xdotool mousemove "$1" "$2"; for _ in $(seq "$3"); do xdotool click "${4:-5}"; done; sleep 0.5; }
shot() { import -window root "$OUT/$1-$THEME.png"; echo "  $OUT/$1-$THEME.png"; }

# Settings sections: name, then the nav row's position (after scrolling the
# nav down for the last two).
SETTINGS=(
  "general 296 233" "models 296 265" "profiles 296 297"
  "commands 324 362" "agents 296 394" "skills 290 426" "hooks 293 458"
  "mcp 311 490" "permissions 309 522" "sandbox 300 587" "network 300 619"
  "environments 314 651" "memory 327 683" "plugins 296 715"
)

screens() {
  # Welcome: a new chat (the sidebar's button also gives the composer focus).
  click 70 60 1.2
  shot welcome
  # A chat: the demo's scripted reply, its permission allowed.
  xdotool type --delay 15 "Add a greeting function and test it"
  key Return
  sleep 6
  click 1050 434 10
  shot chat
  key ctrl+shift+m
  sleep 1.5
  shot map
  key Escape
  # Split: the setup guide in the side panel, below the chat.
  key ctrl+comma
  click 296 265 1.2
  # "Setup guide", under the empty endpoint list.
  click 750 328 2
  key ctrl+backslash
  shot split
  key ctrl+backslash
  key Escape
  # Settings, every section.
  key ctrl+comma
  for s in "${SETTINGS[@]}"; do
    set -- $s
    click "$2" "$3" 1.2
    shot "settings-$1"
  done
  scroll 330 600 5
  click 292 669 1.2
  shot settings-guide
  click 292 701 1.2
  shot settings-about
  scroll 330 400 5 4
  key Escape
  key ctrl+shift+p
  sleep 1
  shot projects
}

THEME=light
echo "Light:"
screens
click 70 60 1.2
key ctrl+shift+l
THEME=dark
echo "Dark:"
screens
key ctrl+shift+l
echo "Done: $(ls "$OUT"/*.png | wc -l) screenshots in $OUT"
