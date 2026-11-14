#!/bin/bash
# Build + test + package for one 32-bit Linux target. Runs INSIDE a 32-bit Debian container in CI
# (see .github/workflows/build.yml); with CI_INSTALL unset it runs on the current machine instead.
#   env: DEB_ARCH=i386|armhf|amd64   RUST_HOST=<rust target triple>   VERSION=1.3.0
#        RUN_TESTS=1  GUI_SMOKE=1  OUT_DIR=/src/dist  CI_INSTALL=1
set -euxo pipefail
cd "$(dirname "$0")/.."                       # shell-rs/
: "${DEB_ARCH:?}"; OUT_DIR="${OUT_DIR:-$PWD/../dist}"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}"

if [ "${CI_INSTALL:-0}" = 1 ]; then
  apt-get update
  DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
    ca-certificates curl build-essential pkg-config file dpkg-dev xvfb xauth \
    libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-host "${RUST_HOST:?}"
  . "$HOME/.cargo/env"
fi

cargo build --release --locked
BIN=target/release/Mosaic

# prove it is the architecture we meant to build
INFO="$(file -L "$BIN")"; echo "$INFO"
case "$DEB_ARCH" in
  i386)  grep -q "Intel 80386" <<<"$INFO" ;;
  armhf) grep -q "ARM, EABI5" <<<"$INFO" && grep -q "32-bit" <<<"$INFO" ;;
  amd64) grep -q "x86-64" <<<"$INFO" ;;
esac

if [ "${RUN_TESTS:-0}" = 1 ]; then cargo test --release --locked; fi

# the server answers like the Qt app's does
"$BIN" --serve >/tmp/mosaic-serve.log 2>&1 &
SP=$!; trap 'kill $SP 2>/dev/null || true' EXIT
bash packaging/smoke.sh
kill $SP 2>/dev/null || true; wait $SP 2>/dev/null || true

if [ "${GUI_SMOKE:-0}" = 1 ]; then bash packaging/smoke-gui.sh "$PWD/$BIN"; fi

mkdir -p "$OUT_DIR"
bash packaging/make-deb.sh "$DEB_ARCH" "$VERSION" "$BIN" "$OUT_DIR/mosaic_${VERSION}_${DEB_ARCH}.deb"
