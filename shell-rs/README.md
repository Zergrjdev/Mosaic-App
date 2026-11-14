# Mosaic shell for 32-bit systems (`shell-rs`)

A small Rust program that does what the Qt app does (`src/AssetServer.cpp` + `qml/Main.qml`) on systems Qt cannot target:
32-bit Windows (x86), 32-bit Linux (i386) and 32-bit ARM Linux (armhf, e.g. Raspberry Pi OS 32-bit). It also builds on 64-bit
Linux, which is how it is developed and tested.

- `src/server.rs`: loopback HTTP server on `127.0.0.1:38417` (any free port if taken) that serves the web app **compiled into the
  executable** (`rust-embed` of `../web/`). Same endpoints, headers and security checks as the Qt app: `/__app`, `POST /__save`
  (native Save dialog), `/__window` (full screen, remembered window), `/user-plugins/`, strict CSP, loopback-only `Host`,
  Origin + `X-Mosaic` header on anything that writes.
- `src/main.rs`: window (`tao`) with the system web view (`wry`: WebView2 on Windows, WebKitGTK on Linux). Remembers size, position and
  maximised state, supports F11 from the page, opens every external link in the system browser, shows a readable message when
  WebView2 / WebKitGTK is missing.
- `src/settings.rs`: the JSON settings file (`%APPDATA%\Mosaic\settings.json`, `~/.config/Mosaic/settings.json`).
- Plugins folder: same place as the Qt app (`<data dir>/Mosaic/Mosaic/plugins`); the page shows the exact path in Settings > Plugins.

## Build and test
```
sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev libsoup-3.0-dev   # Linux only
cargo test                       # unit tests + 6 integration tests against the real server
cargo build --release            # target/release/Mosaic (about 3 MB, the web app is inside)
target/release/Mosaic --serve    # server only, no window: for tests (packaging/smoke.sh checks it over HTTP)
MOSAIC_LOG=1 target/release/Mosaic   # log every request to stderr
```
Windows 32-bit: `rustup target add i686-pc-windows-msvc` then `cargo build --release --target i686-pc-windows-msvc`.
Linux 32-bit: build inside a 32-bit Debian (see `packaging/ci-linux32.sh` and the `linux-32` job), because it needs the 32-bit
WebKitGTK development packages.

`packaging/make-deb.sh <arch> <version> <binary> <out.deb>` builds the Debian package. Runtime needs: WebKitGTK 4.1
(`libwebkit2gtk-4.1-0`) and GTK 3 on Linux, the WebView2 runtime on Windows 10/11 (Windows 7/8 are not supported by WebView2).

## Testing without a file dialog
`MOSAIC_AUTOSAVE_DIR=/some/folder` makes `/__save` write straight into that folder (used by the tests).

## Known limits
- Linux sets `WEBKIT_DISABLE_DMABUF_RENDERER=1` (unless you set it yourself) because WebKitGTK 2.42+ shows a blank window on some GPU/driver
  combinations without it. Remove that line in `main.rs` if you do not want it.
- The sign-in and settings stored in this shell are separate from the Qt app's (different web view profile).
- Wayland does not let apps read or set their own window position, so only size is remembered there.
