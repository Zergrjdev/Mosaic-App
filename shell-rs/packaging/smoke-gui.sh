#!/bin/bash
# Starts the real window under a virtual display and checks the web view asks the embedded server for the app.
#   smoke-gui.sh <path-to-Mosaic-binary>
set -u
BIN="$1"; LOG="$(mktemp)"; HOME="$(mktemp -d)"; export HOME
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 MOSAIC_LOG=1
xvfb-run -a "$BIN" >"$LOG" 2>&1 &
PID=$!
for _ in $(seq 1 40); do grep -q "GET /app.html" "$LOG" && break; sleep 1; done
sleep 4
kill $PID 2>/dev/null; pkill -x "$(basename "$BIN")" 2>/dev/null; sleep 1
N=$(grep -c "^\[mosaic\] GET /js/" "$LOG")
if grep -q "GET /app.html" "$LOG" && [ "$N" -ge 10 ]; then echo "gui smoke: window loaded app.html and $N scripts"; exit 0; fi
echo "gui smoke: FAILED"; tail -30 "$LOG"; exit 1
