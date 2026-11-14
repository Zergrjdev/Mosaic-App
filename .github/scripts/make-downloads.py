#!/usr/bin/env python3
"""Writes downloads.json for the website from the files attached to a GitHub release.
usage: make-downloads.py <version> <base-url> <dir-with-files> > downloads.json"""
import json, os, sys
ver, base, d = sys.argv[1], sys.argv[2].rstrip('/'), sys.argv[3]
files = {f: os.path.getsize(os.path.join(d, f)) for f in os.listdir(d)} if os.path.isdir(d) else {}

def entry(name, note=None):
    out = {"url": "", "size": ""}
    if name in files:
        out = {"url": f"{base}/{name}", "size": f"{files[name] / 1048576:.0f} MB"}
    if note:
        out["note"] = note
    return out

WEBVIEW2 = ("Uses the Microsoft WebView2 Runtime, which is part of Windows 11 and of up-to-date Windows 10. "
            "If Mosaic asks for it, install it from Microsoft and start Mosaic again.")
DEB = "A .deb package. Install it with: sudo apt install ./<downloaded file>"
UNSIGNED = ("This build is not signed by Apple, so iOS will not run it as it is. Re-sign it with your own Apple ID "
            "using AltStore, Sideloadly, TrollStore or Xcode, then install it. It is not on the App Store yet.")

out = {"version": ver, "platforms": {
  "windows": {"label": "Windows", "note": "Windows 10 or newer", "builds": {
      "x64": entry("Mosaic-windows-x64.zip"),
      "arm64": entry("Mosaic-windows-arm64.zip", WEBVIEW2),
      "x86": entry("Mosaic-windows-x86.zip", WEBVIEW2)}},
  "macos": {"label": "macOS", "note": "macOS 12 or newer", "builds": {
      "arm64": entry("Mosaic-macos-universal.dmg"), "x64": entry("Mosaic-macos-universal.dmg")}},
  "linux": {"label": "Linux", "note": "AppImage (.deb for 32-bit)", "builds": {
      "x64": entry("Mosaic-linux-x64.AppImage"),
      "arm64": entry("Mosaic-linux-arm64.AppImage"),
      "arm": entry(f"mosaic_{ver}_armhf.deb", DEB + " Works on Raspberry Pi OS 32-bit for the Pi 2 and newer."),
      "x86": entry(f"mosaic_{ver}_i386.deb", DEB)}},
  "android": {"label": "Android", "note": "Android 8 or newer \u00b7 APK", "builds": {
      "arm64": entry("Mosaic-android-arm64-v8a.apk"), "arm": entry("Mosaic-android-armeabi-v7a.apk"),
      "x64": entry("Mosaic-android-x86_64.apk"), "x86": entry("Mosaic-android-x86.apk")}},
  "ios": {"label": "iPhone & iPad", "note": "iOS 15 or newer \u00b7 unsigned build", "builds": {
      "universal": entry("Mosaic-ios-unsigned.ipa", UNSIGNED)}}}}
json.dump(out, sys.stdout, indent=2)
