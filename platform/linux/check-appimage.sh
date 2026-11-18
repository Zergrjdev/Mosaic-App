#!/bin/bash
# Launches a built Mosaic AppImage on a virtual screen and fails unless it really works:
#   - its local server answers (same checks as the shell's smoke test)
#   - the QML interface loaded (no "module ... is not installed")
#   - Qt found its web view plugin and the app is still running
#   - the page was actually drawn (a window with no browser inside is almost one flat colour)
#   check-appimage.sh <file.AppImage> [screenshot.png]
set -u
IMG="$(realpath "$1")"; SHOT="$(realpath -m "${2:-appimage-screenshot.png}")"
HERE="$(cd "$(dirname "$0")" && pwd)"
if [ -z "${MOSAIC_CHECK_INNER:-}" ]; then      # first call: start a virtual screen and run ourselves inside it
  export MOSAIC_CHECK_INNER=1
  exec xvfb-run -a -s "-screen 0 1280x800x24" bash "$0" "$IMG" "$SHOT"
fi
LOG="$(mktemp)"; STATUS=0
HOME="$(mktemp -d)"; export HOME
export APPIMAGE_EXTRACT_AND_RUN=1 QTWEBENGINE_DISABLE_SANDBOX=1 QTWEBENGINE_CHROMIUM_FLAGS="--no-sandbox --disable-gpu"
"$IMG" >"$LOG" 2>&1 &
APP=$!
bash "$HERE/../../shell-rs/packaging/smoke.sh" 38417 || STATUS=1
sleep 15                                       # the server is up before the interface loads (or fails to)
kill -0 "$APP" 2>/dev/null || { echo "FAIL the app is no longer running"; STATUS=1; }
import -window root "$SHOT"
kill "$APP" 2>/dev/null; sleep 1; pkill -f QtWebEngineProcess 2>/dev/null

if grep -q -E "is not installed|failed to load component" "$LOG"; then
  echo "FAIL the QML interface did not load (modules missing from the AppImage?):"; sort -u "$LOG" | head -10; STATUS=1; fi
if grep -q "No WebView plug-in found" "$LOG"; then
  echo "FAIL Qt found no web view plugin (plugins/webview missing from the AppImage)"; STATUS=1; fi
COLORS="$(convert "$SHOT" -format %k info: 2>/dev/null || echo 0)"
if [ "$COLORS" -lt 40 ]; then echo "FAIL nothing was drawn: the screenshot has only $COLORS colours"; STATUS=1
else echo "  ok   the page was drawn ($COLORS colours on screen)"; fi
if [ "$STATUS" = 0 ]; then echo "appimage check: passed"
else echo "appimage check: FAILED. What the app printed:"; tail -40 "$LOG"; fi
exit $STATUS
