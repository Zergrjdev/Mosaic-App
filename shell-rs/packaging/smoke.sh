#!/bin/bash
# Checks a running `Mosaic --serve` (or the full app) over HTTP, the same way the web view talks to it.
#   smoke.sh [port]
set -u
PORT="${1:-38417}"; B="http://127.0.0.1:$PORT"; FAIL=0
ok()   { echo "  ok   $1"; }
bad()  { echo "  FAIL $1"; FAIL=1; }
check(){ # name, expected-substring, curl args...
  local name="$1" want="$2"; shift 2
  local out; out="$(curl -s -m 10 -i "$@" 2>/dev/null | tr -d '\r')"
  if grep -qiF -- "$want" <<<"$out"; then ok "$name"; else bad "$name (wanted: $want)"; fi
}
# wait for the server
for _ in $(seq 1 30); do curl -s -m 2 -o /dev/null "$B/__app" && break; sleep 1; done
check "/__app says it is the app"           '"app":true'                  "$B/__app"
check "app.html is served"                  'HTTP/1.1 200'                "$B/app.html"
check "app.html content type"               'text/html'                   "$B/app.html"
check "CSP header is present"               "default-src 'self'"          "$B/app.html"
check "javascript is served"                'text/javascript'             "$B/js/04.js"
check "unknown file is 404"                 'HTTP/1.1 404'                "$B/does-not-exist"
check "wrong Host header is refused"        'HTTP/1.1 403'                -H "Host: evil.example" "$B/app.html"
check "save without the page header refused" 'HTTP/1.1 403'               -X POST --data x "$B/__save?name=a.txt"
check "window state needs the page header"  'HTTP/1.1 403'                "$B/__window"
check "window state with the header"        '"fullscreen"'                -H "X-Mosaic: 1" "$B/__window"
[ "$FAIL" = 0 ] && echo "smoke: all checks passed" || { echo "smoke: FAILED"; exit 1; }
