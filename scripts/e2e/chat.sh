#!/usr/bin/env bash
# End to end: the real app, the real KodAct CLI and a scripted model
# (chat_model.py), one chat from the first message to a saved conversation.
#
#   cargo build && scripts/e2e/chat.sh [out-dir]
#
# What it drives, as a person would (xdotool on a virtual display):
#   1. a new chat in a project folder, in Manual mode, and a message;
#   2. each permission the reply asks for (two file writes, two commands),
#      answered Allow with Enter;
#   3. a follow-up message in the same chat.
# What it then checks, and fails on (exit 1, each failure named):
#   - the files the reply wrote are on disk with the right content;
#   - budget.xlsx, made by the office-documents skill's tool running on the
#     Python the app hands the session (`KODACT_PYTHON`), opens and holds
#     the rows;
#   - no tool call came back as an error, and none ran before it was allowed;
#   - the model was sent the follow-up in the same conversation;
#   - both replies are in the chat the app saved, and the app is still up.
# Screenshots of the end of each step land in out-dir (default:
# target/e2e-chat) with the model's log and the app's.
#
# Needs: Xvfb, xdotool, ImageMagick, a Vulkan driver (lavapipe will do),
# Python 3 with the document libraries (or the network, for the app to set
# them up), and KodAct CLI built: KODACT_CLI_BIN, default
# ../kodact/target/debug/kodact.
set -uo pipefail
here="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$here"
S="$here/scripts/e2e"
OUT="${1:-$here/target/e2e-chat}"
CLI="${KODACT_CLI_BIN:-$here/../kodact/target/debug/kodact}"
BIN="$here/target/debug/kodact-desktop"
PORT="${PORT:-4611}"
TOOLS="$here/assets/kodact-plugin/skills/xlsx/scripts"

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
cat > "$TMP/config/endpoints.json" <<J
{ "selected": "mock", "profiles": [ { "name": "mock", "baseUrl": "http://127.0.0.1:$PORT", "wire": "anthropic", "model": "mock-model", "displayName": "Mock model", "auth": { "type": "bearer", "token": "\${env:MOCK_KEY}" } } ] }
J

export DISPLAY="${DISPLAY:-:99}"
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

python3 "$S/chat_model.py" "$PORT" "$OUT/model.log" "$PROJECT" "$TOOLS" &
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
# The model's log has N requests' worth of lines (waits up to $2 seconds).
replies() { [ -f "$OUT/model.log" ] && grep -c "$1" "$OUT/model.log" || echo 0; }
wait_for() { # pattern, seconds
  for _ in $(seq "$(( $2 * 2 ))"); do
    grep -q "$1" "$OUT/model.log" 2>/dev/null && return 0
    sleep 0.5
  done
  return 1
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

echo "A new chat, and a message"
xdotool mousemove 70 96 click 1; sleep 1.5
xdotool type --delay 15 "Make a budget workbook with rent and food"
xdotool key Return
wait_for '"Write"' 40 || bad "the model was never asked (no request reached it)"
sleep 2; shot 1-asks
check "nothing is written until it is allowed" '[ ! -e "$PROJECT/notes.md" ]'

echo "Each permission, allowed with Enter"
# Four asks: notes.md, rows.csv, make the workbook, read it back. Enter
# answers the one showing; the next request to the model shows it ran.
for want in 'rows\.csv' 'xlsx_tool\.py.. create' 'xlsx_tool\.py.. read' 'Done: budget\.xlsx'; do
  got=0
  for _ in $(seq 12); do
    xdotool key Return
    for _ in 1 2 3 4 5 6; do
      grep -q "$want" "$OUT/model.log" && { got=1; break; }
      sleep 0.5
    done
    [ "$got" = 1 ] && break
  done
  [ "$got" = 1 ] || { bad "the turn stopped before: $want"; break; }
done
sleep 3; shot 2-done

echo "A follow-up in the same chat"
xdotool type --delay 15 "And the total?"
xdotool key Return
wait_for 'The total is 950' 40 || bad "the follow-up never reached the model"
sleep 3; shot 3-follow-up

echo "Checks"
check "notes.md is on disk as written" '[ "$(cat "$PROJECT/notes.md" 2>/dev/null)" = "$(printf "# Budget\n\nRent and food, for the end-to-end test.")" ]'
check "rows.csv is on disk" 'grep -q "Rent,700" "$PROJECT/rows.csv" 2>/dev/null'
check "budget.xlsx opens and holds the rows" 'python3 - "$PROJECT/budget.xlsx" <<P
import sys, openpyxl
ws = openpyxl.load_workbook(sys.argv[1])["Budget"]
rows = [[c.value for c in r] for r in ws.iter_rows()]
sys.exit(0 if rows == [["Item", "Amount"], ["Rent", 700], ["Food", 250]] else 1)
P'
check "no tool call came back as an error" '! grep -q "\", true\]" "$OUT/model.log"'
check "four tool calls ran, in order" 'python3 - "$OUT/model.log" <<P
import json, sys
last = [json.loads(l) for l in open(sys.argv[1]) if "Done: budget.xlsx" in l][-1]
sys.exit(0 if [r[0] for r in last["results"]] == ["Write", "Write", "Bash", "Bash"] else 1)
P'
check "the workbook read back through KODACT_PYTHON" 'grep -q "Done: budget.xlsx has Rent 700 and Food 250" "$OUT/model.log"'
check "the follow-up went to the same conversation" 'python3 - "$OUT/model.log" <<P
import json, sys
last = [json.loads(l) for l in open(sys.argv[1]) if "The total is 950" in l][-1]
asked = last["asked"]
sys.exit(0 if len(asked) == 2 and "budget workbook" in asked[0] and "total" in asked[1] and len(last["results"]) == 4 else 1)
P'
check "the app is still running" 'kill -0 "$APP" 2>/dev/null'
# The app writes a chat a moment after it changes.
sleep 2
check "both replies are in the saved chat" 'grep -rlq "Done: budget.xlsx has Rent 700 and Food 250" "$TMP/config/chats" 2>/dev/null && grep -rlq "The total is 950" "$TMP/config/chats" 2>/dev/null'
check "the app logged no panic" '! grep -qi "panicked" "$OUT/app.log"'

if [ "$FAILED" = 0 ]; then
  echo "PASS  ($OUT)"
else
  echo "FAIL  ($OUT: model.log, app.log, the screenshots)"
fi
exit "$FAILED"
