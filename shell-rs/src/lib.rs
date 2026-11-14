//! Mosaic desktop shell for systems the Qt build cannot target (32-bit Windows and Linux, 32-bit ARM).
//!
//! It does what `src/AssetServer.cpp` + `qml/Main.qml` do in the Qt app, with the same HTTP contract,
//! so the web app needs no changes: a loopback server serves the web app that is embedded in the
//! executable, and a system web view (WebView2 on Windows, WebKitGTK on Linux) shows it.
pub mod server;
pub mod settings;
