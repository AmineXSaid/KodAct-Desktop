#!/usr/bin/env bash
# End to end: a deck made the way the office-documents skill says to, in the
# real app with the real KodAct CLI and a scripted model (office_model.py).
# It is the test for what went wrong on 2026-11-24 ("Bypass permissions"
# still asking; no file to watch until the reply ended; slides shown as text).
#
#   cargo build && scripts/e2e/office_live.sh [out-dir]
#
# What it drives (xdotool on a virtual display):
#   1. a new chat in Auto (Shift+Tab twice) and a message;
#   2. nothing else while the deck is made, filled and drawn: Auto must not ask;
#   3. the one question Auto does ask (`rm -rf pages`) is answered by
#      switching to Bypass permissions (Shift+Tab twice more), not by Allow.
# What it checks, and fails on (exit 1, each failure named):
#   - no question before the removal, and the removal ran once in Bypass;
#   - the deck's card is in the chat while the turn still runs (screenshot
#     2-live shows it, and its panel);
#   - the deck on disk is valid and has its 4 slides;
#   - the reply ended, the chat was saved with the deck's card, no panic.
#
# Needs: Xvfb, xdotool, ImageMagick, a Vulkan driver (lavapipe will do),
# Python 3 with the document libraries, LibreOffice (for the panel's slides
# on Linux), and KodAct CLI built: KODACT_CLI_BIN, default
# ../kodact/target/debug/kodact.
set -uo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$here"
S="$here/scripts/e2e"
OUT="${1:-$here/target/e2e-office}"
CLI="${KODACT_CLI_BIN:-$here/../kodact/target/debug/kodact}"
BIN="$here/target/debug/kodact-desktop"
PORT="${PORT:-4612}"
TOOLS="$here/assets/kodact-plugin/skills/pptx/scripts"

[ -x "$BIN" ] || { echo "Build first: cargo build" >&2; exit 2; }
[ -x "$CLI" ] || { echo "No KodAct CLI at $CLI: build it, or set KODACT_CLI_BIN" >&2; exit 2; }
for tool in Xvfb xdotool import python3; do
  command -v "$tool" >/dev/null || { echo "Missing $tool" >&2; exit 2; }
done

rm -rf "$OUT" && mkdir -p "$OUT"
TMP="$(mktemp -d)"
PROJECT="$TMP/project"
mkdir -p "$TMP/config" "$TMP/home" "$PROJECT"
echo "KodAct" > "$TMP/config/name.txt"
# Bypass was allowed on an earlier day: it is in the Shift+Tab round.
echo '{"bypass_allowed": true}' > "$TMP/config/window.json"
cat > "$TMP/config/endpoints.json" <<J
{ "selected": "mock", "profiles": [ { "name": "mock", "baseUrl": "http://127.0.0.1:$PORT", "wire": "anthropic", "model": "mock-model", "displayName": "Mock model", "auth": { "type": "bearer", "token": "\${env:MOCK_KEY}" } } ] }
J

export DISPLAY="${DISPLAY:-:98}"
XVFB=""; APP=""; MOCK=""
cleanup() {
  [ -n "$APP" ] && kill "$APP" 2>/dev/null
  [ -n "$MOCK" ] && kill "$MOCK" 2>/dev/null
  [ -n "$XVFB" ] && kill "$XVFB" 2>/dev/null
  rm -rf "$TMP"
}
trap cleanup EXIT
if ! xdotool getdisplaygeometry >/dev/null 2>&1; then
  Xvfb "$DISPLAY" -screen 0 1280x860x24 >/dev/null 2>&1 &
  XVFB=$!
  sleep 1.5
fi
if [ -z "${VK_ICD_FILENAMES:-}" ] && [ -f /usr/share/vulkan/icd.d/lvp_icd.json ] && [ ! -e /dev/dri ]; then
  export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
fi

PAUSE=10 python3 "$S/office_model.py" "$PORT" "$OUT/model.log" "$PROJECT" "$TOOLS" &
MOCK=$!
MOCK_KEY=test NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost \
  KODACT_CLI="$CLI" KODACT_CONFIG_DIR="$TMP/config" KODACT_HOME="$TMP/home" \
  KODACT_PROJECT_DIR="$PROJECT" "$BIN" >"$OUT/app.log" 2>&1 &
APP=$!

FAILED=0
ok() { echo "  ok    $1"; }
bad() { echo "  FAIL  $1"; FAILED=1; }
check() { if eval "$2"; then ok "$1"; else bad "$1"; fi; }
shot() { import -window root "$OUT/$1.png"; }
wait_for() { # pattern, seconds
  for _ in $(seq "$(( $2 * 2 ))"); do
    grep -q -- "$1" "$OUT/model.log" 2>/dev/null && return 0
    sleep 0.5
  done
  return 1
}
steps() { [ -f "$OUT/model.log" ] && python3 - "$OUT/model.log" <<'P' || echo 0
import json, sys
print(max([len(json.loads(l)["results"]) for l in open(sys.argv[1]) if l.strip()] or [0]))
P
}

WID=""
for _ in $(seq 60); do
  WID="$(xdotool search --pid "$APP" 2>/dev/null | tail -1)"
  [ -n "$WID" ] && break
  sleep 0.5
done
[ -n "$WID" ] || { echo "The app's window never opened (see $OUT/app.log)" >&2; exit 1; }
xdotool windowmove "$WID" 0 0 windowsize "$WID" 1280 860
sleep 4

echo "A new chat in Auto, and a message"
xdotool mousemove 70 96 click 1; sleep 1.5
xdotool key shift+Tab; sleep 0.4; xdotool key shift+Tab; sleep 0.6
shot 0-auto
xdotool type --delay 15 "Make a PowerPoint about cancer for kids"
xdotool key Return
wait_for 'init' 40 || bad "the model was never asked"

echo "While the deck is made: the card and the panel, live"
# The model waits PAUSE seconds after `init` ran: the turn is still going.
for _ in $(seq 40); do [ "$(steps)" -ge 1 ] && break; sleep 0.5; done
sleep 5; shot 2-live
check "the deck exists while the turn runs" '[ -s "$PROJECT/deck.pptx" ]'

echo "Auto runs the build and the render without asking"
wait_for 'rm -rf' 60 || bad "the turn stopped before the removal (an unwanted question?)"
check "no question before the removal: four steps ran unanswered" '[ "$(steps)" -ge 4 ]'
sleep 2; shot 3-asks
check "the removal waits for an answer in Auto" '[ -d "$PROJECT/pages" ]'

echo "Switch to Bypass permissions: the waiting question is answered by it"
xdotool key shift+Tab; sleep 0.4; xdotool key shift+Tab
wait_for 'Done: the deck has 4 slides' 40 || bad "Bypass did not let the removal run"
sleep 3; shot 4-done

echo "Checks"
check "the removal ran" '[ ! -e "$PROJECT/pages" ]'
check "no tool call came back as an error" 'python3 - "$OUT/model.log" <<P
import json, sys
last = [json.loads(l) for l in open(sys.argv[1]) if l.strip()][-1]
sys.exit(0 if len(last["results"]) == 5 and not any(r[2] for r in last["results"]) else 1)
P'
check "the deck is valid and has 4 slides" 'python3 - "$PROJECT/deck.pptx" "$TOOLS" <<P
import sys
sys.path.insert(0, sys.argv[2])
import check_tool
from pptx import Presentation
sys.exit(0 if not check_tool.validate(sys.argv[1]) and len(Presentation(sys.argv[1]).slides) == 4 else 1)
P'
check "the render used the route the app named, without searching" '! grep -q "which \|Get-Command\|where.exe" "$OUT/model.log"'
check "the app is still running" 'kill -0 "$APP" 2>/dev/null'
sleep 2
check "the chat was saved with the deck's card" 'grep -rlq "\"deck.pptx\"" "$TMP/config/chats" 2>/dev/null'
check "the app logged no panic" '! grep -qi "panicked" "$OUT/app.log"'

if [ "$FAILED" = 0 ]; then
  echo "PASS  ($OUT)"
else
  echo "FAIL  ($OUT: model.log, app.log, the screenshots)"
fi
exit "$FAILED"
