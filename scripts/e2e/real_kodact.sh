#!/usr/bin/env bash
# The real app + the real KodAct CLI + a scripted model, with the browser on:
# a sub-agent tests a form with KodAct's browser tools, then the main agent
# opens and switches tabs and runs JavaScript. Linux, Xvfb, xdotool,
# ImageMagick, and a Chromium (Playwright's). Build KodAct CLI first and point
# KODACT_CLI_BIN at it. ALLOW=0 leaves the tools unallowed, so each step asks
# and the script answers Allow. Writes <out>-model.log and <out>-*.png.
cd /home/user/kodact-desktop
S=$(cd "$(dirname "$0")" && pwd); OUT=${1:-/tmp/kodact-e2e}
KODACT_CLI_BIN=${KODACT_CLI_BIN:-/home/user/kodact/target/debug/kodact}
export DISPLAY=:99
xdotool getdisplaygeometry >/dev/null 2>&1 || { Xvfb :99 -screen 0 1280x900x24 >/dev/null 2>&1 & sleep 1.5; }
pgrep -f "http.server 8765" >/dev/null || (cd $S/site && python3 -m http.server 8765 --bind 127.0.0.1 >/dev/null 2>&1 &)
: > $OUT-model.log
python3 $S/mock_model.py 4610 $OUT-model.log & MOCK=$!
TMP=$(mktemp -d); mkdir -p $TMP/config $TMP/home; echo KodAct > $TMP/config/name.txt
echo on > $TMP/config/browser.txt
cat > $TMP/config/endpoints.json <<J
{ "selected": "mock", "profiles": [ { "name": "mock", "baseUrl": "http://127.0.0.1:4610", "wire": "anthropic", "model": "mock-model", "displayName": "Mock model", "auth": { "type": "bearer", "token": "\${env:MOCK_KEY}" } } ] }
J
[ "${ALLOW:-1}" = 1 ] && echo '{ "permissions": { "allow": ["mcp__kodact-browser"] } }' > $TMP/home/settings.json
[ -e /dev/dri ] || export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
MOCK_KEY=test NO_PROXY=127.0.0.1,localhost no_proxy=127.0.0.1,localhost KODACT_CLI=$KODACT_CLI_BIN \
  KODACT_CONFIG_DIR=$TMP/config KODACT_HOME=$TMP/home KODACT_PROJECT_DIR=$PWD target/debug/kodact-desktop >$OUT-app.log 2>&1 &
PID=$!
sleep 7
WID=$(xdotool search --pid $PID | tail -1)
xdotool windowmove $WID 0 0 windowsize $WID 1280 900; sleep 1
xdotool mousemove 70 60 click 1; sleep 1.2
xdotool type --delay 15 "Use a sub-agent to test the greeting form in the browser"; xdotool key Return
sleep ${MID:-12}
import -window root $OUT-mid.png
if [ "${ALLOW:-1}" = 0 ]; then
  for i in $(seq 40); do
    import -window root $OUT-tick.png
    XY=$(python3 $S/find_allow.py $OUT-tick.png)
    [ -n "$XY" ] && { xdotool mousemove $XY click 1; echo "CLICKED $XY" >> $OUT-model.log; }
    grep -q '"text", "Checked' $OUT-model.log && break
    sleep 1.5
  done
fi
sleep ${END:-30}
import -window root $OUT-end.png
xdotool mousemove 986 60 click 1; sleep 2
import -window root $OUT-browser.png
kill $PID; sleep 3; kill $MOCK
echo "LEFT AFTER EXIT: chrome=$(ps -eo comm | grep -cE '^chrome') kodact=$(ps -eo comm | grep -cE '^(kodact|kodact-desktop)$')" >> $OUT-model.log
rm -rf $TMP
