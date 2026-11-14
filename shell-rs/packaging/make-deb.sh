#!/bin/bash
# Builds a .deb from an already-built Mosaic binary.
#   make-deb.sh <deb-arch: i386|armhf|amd64|arm64> <version> <path-to-Mosaic-binary> <output.deb>
set -euo pipefail
ARCH="$1"; VERSION="$2"; BIN="$3"; OUT="$4"
HERE="$(cd "$(dirname "$0")" && pwd)"
APP="$HERE/../.."          # mosaic-app/
STAGE="$(mktemp -d)"; trap 'rm -rf "$STAGE"' EXIT

install -Dm755 "$BIN" "$STAGE/usr/bin/mosaic"
install -Dm644 "$APP/platform/linux/mosaic.desktop" "$STAGE/usr/share/applications/mosaic.desktop"
sed -i 's/^Exec=.*/Exec=mosaic/' "$STAGE/usr/share/applications/mosaic.desktop"
install -Dm644 "$APP/web/icon-512.png" "$STAGE/usr/share/icons/hicolor/512x512/apps/mosaic.png"

SIZE_KB=$(du -sk "$STAGE" | cut -f1)
mkdir -p "$STAGE/DEBIAN"
cat > "$STAGE/DEBIAN/control" <<CTL
Package: mosaic
Version: $VERSION
Architecture: $ARCH
Maintainer: Mosaic <noreply@users.noreply.github.com>
Section: graphics
Priority: optional
Installed-Size: $SIZE_KB
Depends: libwebkit2gtk-4.1-0, libgtk-3-0
Description: Mosaic - design your entire game in one place
 Boards, documents, whiteboards and designs in one app. Everything is stored
 on this computer unless you sign in. This is the lightweight build that uses
 the system web view (WebKitGTK), used for 32-bit systems.
CTL
mkdir -p "$(dirname "$OUT")"
dpkg-deb --build --root-owner-group "$STAGE" "$OUT" >/dev/null
echo "built $OUT ($(du -h "$OUT" | cut -f1))"
