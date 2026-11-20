# Mosaic: C++ / Qt app

One C++ codebase that builds Mosaic for 64-bit Windows, macOS, Linux, Android and iOS, plus a small Rust shell (`shell-rs/`) for the
32-bit desktops Qt cannot target. Every build carries its own copy of the web app, so nobody needs the website to use Mosaic.

## How it works
- The whole web app (`web/`) is compiled into the executable as Qt resources, so it starts offline.
- `src/AssetServer.*` is a small read-only HTTP server on `127.0.0.1:38417` that serves those resources.
  This keeps your strict Content-Security-Policy, localStorage and Supabase auth working (they don't work from `file://`).
  It rejects path traversal and any `Host` header that isn't loopback.
- **Exports:** web views don't handle downloads consistently, so the site (`web/js/app.js`) sends exports to the app
  (`POST /__save`, accepted only from the app's own page), and `qml/Main.qml` opens a native "Save as" dialog.
- `src/main.cpp` starts the server and a Qt Quick window; `qml/Main.qml` is a full-window `WebView`.
- Qt WebView picks the engine per platform (system web view on Android, iOS and macOS; Qt WebEngine on Windows and Linux).
- Supabase, Google Fonts and your share links still use the internet exactly like the website does.
  Share links use your public site address (`siteBase()` already falls back to it when the page isn't https).

To update the app: replace the files in `web/` with your new site files and rebuild.

## Requirements
Qt 6.4+ (6.6 or newer recommended) with the modules **Qt WebView** and **Qt WebEngine** (desktop), plus CMake 3.21+ and Ninja.
Install through the Qt Online Installer or `aqtinstall`.

## Build

### Linux
```
sudo apt install cmake ninja-build qt6-base-dev qt6-declarative-dev qt6-webview-dev qt6-webengine-dev \
     qml6-module-qtwebview qml6-module-qtquick-window qml6-module-qtqml-workerscript \
     qml6-module-qtquick-templates qml6-module-qtquick-layouts qml6-module-qtquick-dialogs qml6-module-qtquick-controls
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build && ./build/Mosaic
```
Package it with linuxdeploy / AppImage, or a `.deb` via CPack.

### Windows (MSVC kit)
```
cmake -S . -B build -G Ninja -DCMAKE_PREFIX_PATH=C:/Qt/6.x.x/msvc2022_64 -DCMAKE_BUILD_TYPE=Release
cmake --build build
windeployqt --qmldir qml build/Mosaic.exe
```
Zip the folder, or wrap it with Inno Setup / WiX.

### macOS
```
cmake -S . -B build -G Ninja -DCMAKE_PREFIX_PATH=~/Qt/6.x.x/macos -DCMAKE_BUILD_TYPE=Release
cmake --build build
macdeployqt build/Mosaic.app -qmldir=qml -dmg
```
Sign and notarise to distribute outside the App Store.

### Android
Install Android Studio's SDK + NDK, then the Qt for Android kit (arm64-v8a). In Qt Creator choose that kit and press Run,
or from the command line:
```
~/Qt/6.x.x/android_arm64_v8a/bin/qt-cmake -S . -B build-android -G Ninja \
    -DANDROID_SDK_ROOT=$ANDROID_SDK_ROOT -DANDROID_NDK_ROOT=$ANDROID_NDK_ROOT
cmake --build build-android --target aab   # or "apk"
```
`platform/android/` allows plain http only to 127.0.0.1 (required for the local server).
If your Qt version's default manifest differs, copy Qt's and keep only the `networkSecurityConfig` attribute.

### iOS (needs a Mac with Xcode)

**Unsigned `.ipa` (no Apple account needed):**
```
QT_IOS=~/Qt/6.8.1/ios QT_HOST=~/Qt/6.8.1/macos platform/ios/build-unsigned-ipa.sh   # writes Mosaic-ios-unsigned.ipa
```
This configures with `-DMOSAIC_IOS_UNSIGNED=ON` (code signing off), builds for a real device with `xcodebuild`, checks the result
is arm64 and unsigned, and zips it into `Payload/Mosaic.app` inside an `.ipa`. iOS only runs signed code, so the file must be
re-signed with your own Apple ID before it installs: AltStore, Sideloadly, TrollStore, or Xcode (Devices and Simulators, drag the
`.ipa` in). Free Apple IDs work but the install expires after 7 days. GitHub Actions builds the same file (`ios-unsigned` job).

**Signed build (when you have a developer account):**
```
~/Qt/6.x.x/ios/bin/qt-cmake -S . -B build-ios -G Xcode
open build-ios/Mosaic.xcodeproj    # set your Team under Signing, pick a device, Run
```
`platform/ios/Info.plist` enables local networking for the in-app server.

## What the app opens
The app starts directly in the workspace (`app.html`). Home and Discover are workspace screens; nothing in the workspace links out to the website pages (they are still bundled for the About/legal links).

## Window and keyboard
`F11` toggles full screen (the page asks the app through `POST /__window`, which only accepts requests from the app's own page), `Shift+F11` is focus mode, `Ctrl+Shift+P` the command palette. Settings > Window & keys can start the app in full screen and remember its size and position (stored with QSettings).

## Phones, tablets and big screens
The workspace adapts to its window: a bottom tool bar and a "More" sheet on phones, a drawer sidebar on tablets, and larger panels on ultrawide displays. Touch: pinch to zoom, two-finger pan, plus a Pan tool for one-finger panning.

## Plugins in the app
Plugins work exactly as on the website (see `PLUGINS.md`). Bundled ones live in `web/plugins/`; the Plugin store (Settings > Plugins) installs approved plugins from the central Supabase. To add your own without rebuilding,
put a folder with a `manifest.json` and its scripts in the app's plugins folder; the exact path is shown in Settings > Plugins.

## Web + app together
Same account, same project list, same Supabase: people on the website and in the app can edit one project live.
Set your website address in `web/config.js` (`window.MOSAIC_SITE = 'https://YOUR-SITE.netlify.app/';`) so share links and
sign-up confirmation emails made inside the app point at the website.

## Known limits, test these on each platform
- The Qt builds were only compiled and run on Linux (exports tested end to end there). Windows, macOS, Android and iOS
  are untested; the native save dialog on Android/iOS in particular needs checking.
- The 32-bit shell (`shell-rs/`): its server, window, save dialog and full-screen command were compiled, tested and run on
  64-bit Linux (9 automated tests, a real WebKitGTK window under a virtual display). The 32-bit Windows, i386 and armhf
  builds use the same code but have not been built by the author; the CI jobs test each one and check the CPU type of the result.
- The iOS `.ipa` (script, CMake option and CI job) has not been run; no Mac was available.
- Image picking and JSON import use the system web view; behaviour differs by platform.
- The `/__window` endpoint, the QML window-state code and the widened CSP were added after the last compile; build and test F11 and window memory on each platform.
- Plugins from the Plugin store (for example the AI assistant) talk to Ollama/MCP servers directly from the page, so those must allow the app's origin (`OLLAMA_ORIGINS=http://127.0.0.1:38417`; CORS for MCP). Store plugins load from `blob:` URLs, so `script-src` in `kCsp` contains `blob:`.
- "Copy PNG to clipboard" depends on the web view's clipboard permission.
- Sign-up confirmation emails link to your public website, so the user confirms there, then signs in in the app.
- Keep port 38417 free; if it is taken the app picks another port, and the saved local data (sign-in, settings) will look reset because the web origin changed.
- App Store review (especially Apple's) can push back on apps that are only a website wrapper. Offline use and native packaging help, but consider adding native features before submitting.
- Windows/macOS/Linux icons: add `.ico` / `.icns` files if you want custom executable icons (the window icon is already set).

## Architectures (64-bit, 32-bit, ARM)

The download page detects the visitor's system and offers the matching build. `web/downloads/downloads.json` lists one entry per
platform and CPU (`x64`, `x86`, `arm64`, `arm`); the release workflow writes it for you. A build can carry its own `note`, which the
page shows under the button (used for the WebView2 hint and the iOS re-signing notice). An entry with `"native": false` still makes
the page send that kind of device to the web app instead.

| Platform | Builds | How |
|---|---|---|
| Windows | x64 zip, **32-bit x86 zip, ARM64 zip** | x64: Qt + Qt WebEngine. x86 and ARM64: Rust shell + the system WebView2 runtime (part of Windows 11 and up-to-date Windows 10). |
| Linux | x64 and arm64 AppImages, **32-bit i386 and armhf `.deb`** | AppImages: Qt + Qt WebEngine. 32-bit: Rust shell + system WebKitGTK 4.1 (`Depends:` is filled in, so `apt install ./file.deb` pulls it in). |
| macOS | one universal dmg | Qt + system web view. |
| Android | `arm64-v8a`, `armeabi-v7a` (32-bit ARM), `x86_64`, `x86` (32-bit) | Qt + system web view. One APK per ABI. |
| iOS | one **unsigned** `.ipa` | Qt + system web view (WKWebView). Must be re-signed to install, see iOS above. |

### Why 32-bit desktops use a different shell
Qt WebEngine (the browser Qt embeds on Windows and Linux) and Qt's downloadable kits are 64-bit only. `shell-rs/` replaces just
the shell: it does what `src/AssetServer.cpp` and `qml/Main.qml` do, with the same endpoints (`/__app`, `/__save`, `/__window`,
`/user-plugins/`), the same Content-Security-Policy and the same port (38417), so `web/` runs unchanged. The web app is compiled
into the executable (about 3 MB in total). See `shell-rs/README.md`.

### Is the `web/` folder still needed?
For *hosting*, no: every platform now has a native app that contains the whole web app, and nothing in the apps loads files from
a website (sign-in, sharing and Supabase still use the internet, like the site does). For *building*, yes: `web/` is the source of
the interface and is embedded into every app, so keep it in the repository. The site can stay up for people who prefer the browser.

## Build every platform with GitHub, and keep the download page current

Push this project to a GitHub repository. `.github/workflows/build.yml` builds, in parallel:

| Platform | Result |
|---|---|
| Linux | x64 and arm64 AppImages, plus 32-bit i386 and armhf `.deb` |
| Windows | x64 zip (Qt), 32-bit x86 zip and ARM64 zip (Rust shell) |
| macOS | one universal dmg for Apple silicon and Intel |
| Android | arm64-v8a, armeabi-v7a, x86_64 and x86 APKs |
| iOS | unsigned `.ipa` (re-sign to install, see iOS above) |

- **Just build:** open the **Actions** tab, pick **build**, press **Run workflow**. The files appear as artifacts on the run.
- **Release:** `git tag v1.4.0 && git push --tags`. When every build has finished, the `release` job
  1. attaches all files, a `SHA256SUMS.txt` and a ready-made `downloads.json` to a GitHub Release, then
  2. **commits that `downloads.json` to `web/downloads/downloads.json` on your default branch**, so the site (which redeploys on every
     push, e.g. Netlify or GitHub Pages) shows the new version and links every build. Nobody copies files by hand any more.
  The links point at the permanent release URLs. If the file did not change, nothing is committed.

### One-time setup
1. In the repository: **Settings > Actions > General > Workflow permissions**: choose *Read and write permissions* (the workflow
   already asks for `contents: write`; an organisation policy can override it).
2. If the default branch is protected, allow the `github-actions` bot to push to it (or the website step will fail after the
   release is already published; re-run just that job once fixed).
3. **Website in a different repository** (for example you host only the standalone `mosaic-web` folder): add a repository
   *variable* `SITE_REPO` (`owner/name`) and a *secret* `SITE_REPO_TOKEN` (a fine-grained token with *Contents: write* on that repo).
   Optionally set the variable `SITE_DOWNLOADS_PATH` (default `downloads/downloads.json`). The same job then updates that repo too.
4. Set your website address in `web/config.js` (`window.MOSAIC_SITE`), as above, so share links point at it.

### Notes
- Tags decide the file names of the `.deb` packages (`mosaic_<tag without v>_i386.deb`) and the links the page builds, so no manual
  version step is needed; the version in `shell-rs/Cargo.toml` is only what the app reports about itself, keep it roughly in step.
- The Linux AppImage jobs run `platform/linux/check-appimage.sh` on the finished file: it starts the app on a virtual screen and fails
  unless the server answers, the QML interface loads, Qt finds its web view plugin and the page is actually drawn. (A bundle that
  builds fine but is missing the QML modules or the web view plugin starts and then shows an empty window, so a build-only check
  cannot catch it.) A screenshot of what it drew is kept as the `appimage-screenshot-*` artifact of the run.
- Android APKs are signed in CI, because Android refuses unsigned apps. Without setup a throwaway key is made for each build (the APK
  installs, but a later build signed with a different key cannot update it). To keep one key, create a keystore once
  (`keytool -genkeypair -keystore mosaic.jks -alias mosaic -keyalg RSA -keysize 2048 -validity 10000`), then add the repository
  secrets `ANDROID_KEYSTORE_BASE64` (`base64 -w0 mosaic.jks`), `ANDROID_KEYSTORE_ALIAS` and `ANDROID_KEYSTORE_PASSWORD`
  (use the same password for the store and the key). Keep a safe copy of `mosaic.jks`: losing it means users must uninstall to update.
- The 32-bit ARM `.deb` is built under QEMU and takes about an hour; it targets ARMv7 (Raspberry Pi 2 and newer on a 32-bit OS,
  not the Pi Zero or Pi 1). Windows ARM64 is only cross-compiled (the runner is an Intel PC), so it is checked for CPU type, not run.
- After editing anything in `web/`, run `python3 .github/scripts/rehash.py web` so the `?v=` cache hashes match the files.
- If one platform fails, the release is not created (the `release` job needs all of them). Fix it and re-run, or tag again.

The workflow was written without being able to run the Qt, Windows, 32-bit and iOS jobs, so expect small fixes on the first run
(Qt version, package names or NDK version are the usual suspects). It passes `actionlint` (workflow syntax and shell checks).
`make-downloads.py`, `publish-downloads.sh` (tested against real git repositories, including a push race) and the 32-bit Linux
script (`shell-rs/packaging/ci-linux32.sh`) were run locally.
