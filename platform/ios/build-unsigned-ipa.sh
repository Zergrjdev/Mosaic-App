#!/bin/bash
# Builds Mosaic for iPhone/iPad WITHOUT code signing and wraps it into an unsigned .ipa.
# Needs a Mac with Xcode, Qt 6 for iOS and the matching Qt 6 desktop (host) kit:
#   QT_IOS=~/Qt/6.8.1/ios  QT_HOST=~/Qt/6.8.1/macos  platform/ios/build-unsigned-ipa.sh [output.ipa]
# The .ipa cannot be installed as it is: iOS only runs signed code. Re-sign it with your own Apple ID
# using AltStore, Sideloadly, TrollStore or Xcode ("Window > Devices and Simulators", drag the .ipa in).
set -euo pipefail
: "${QT_IOS:?set QT_IOS to the Qt for iOS folder, e.g. ~/Qt/6.8.1/ios}"
: "${QT_HOST:?set QT_HOST to the Qt desktop folder, e.g. ~/Qt/6.8.1/macos}"
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
OUT="${1:-$ROOT/Mosaic-ios-unsigned.ipa}"
BUILD="$ROOT/build-ios-unsigned"

"$QT_IOS/bin/qt-cmake" -S "$ROOT" -B "$BUILD" -G Xcode \
  -DQT_HOST_PATH="$QT_HOST" \
  -DMOSAIC_IOS_UNSIGNED=ON \
  -DCMAKE_OSX_DEPLOYMENT_TARGET=15.0 \
  -DCMAKE_BUILD_TYPE=Release

xcodebuild -project "$BUILD/Mosaic.xcodeproj" -scheme Mosaic -configuration Release \
  -sdk iphoneos -destination 'generic/platform=iOS' \
  CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO CODE_SIGN_IDENTITY="" DEVELOPMENT_TEAM="" \
  build

APP="$(find "$BUILD" -type d -name Mosaic.app -path '*Release-iphoneos*' | head -1)"
[ -n "$APP" ] || { echo "Mosaic.app was not produced"; exit 1; }

# sanity checks: a real arm64 device build, and really unsigned
lipo -info "$APP/Mosaic" | grep -q arm64 || { echo "not an arm64 build"; exit 1; }
[ ! -e "$APP/_CodeSignature" ] || { echo "app is unexpectedly signed"; exit 1; }

STAGE="$(mktemp -d)"; trap 'rm -rf "$STAGE"' EXIT
mkdir "$STAGE/Payload" && cp -R "$APP" "$STAGE/Payload/"
rm -f "$OUT"; (cd "$STAGE" && zip -qry "$OUT" Payload)
echo "built $OUT ($(du -h "$OUT" | cut -f1)) - unsigned, re-sign it to install"
