#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]
//! Mosaic desktop shell: a window with the system web view, pointed at the embedded loopback server.
//! `Mosaic --serve` runs only the server (for tests and CI smoke checks).
use mosaic_shell::{
    server::{self, Shared},
    settings::{Geometry, Settings},
};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};
use tao::{
    dpi::{LogicalSize, PhysicalPosition, PhysicalSize},
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder, EventLoopWindowTarget},
    window::{Fullscreen, Icon, WindowBuilder},
};
use wry::{NewWindowResponse, WebContext, WebViewBuilder};

#[derive(Debug, Clone, Copy)]
enum UserEvent { Fullscreen(bool) }

/// Shows a message the person can read (the Windows build has no console) and quits.
fn fatal(what: &str, hint: &str) -> ! {
    eprintln!("Mosaic: {what}\n{hint}");
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Mosaic could not start")
        .set_description(format!("{what}\n\n{hint}"))
        .show();
    std::process::exit(1)
}

#[cfg(windows)]
const WEBVIEW_HINT: &str = "Mosaic needs the Microsoft WebView2 Runtime. It is part of Windows 11 and of up-to-date Windows 10. \
Install it from https://go.microsoft.com/fwlink/p/?LinkId=2124703 and start Mosaic again.";
#[cfg(not(windows))]
const WEBVIEW_HINT: &str = "Mosaic needs WebKitGTK 4.1. On Debian, Ubuntu and Raspberry Pi OS install it with: sudo apt install libwebkit2gtk-4.1-0";

fn app_dirs() -> (PathBuf, PathBuf) {
    // Same folders the Qt app uses for plugins; settings live in the user's config folder.
    let data = dirs::data_dir().unwrap_or_else(std::env::temp_dir).join("Mosaic").join("Mosaic");
    let config = dirs::config_dir().unwrap_or_else(std::env::temp_dir).join("Mosaic");
    (data, config)
}

fn window_icon() -> Option<Icon> {
    let bytes = server::asset("icon-512.png")?;
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0u8; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let px = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => px.chunks(2).flat_map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Icon::from_rgba(rgba, info.width, info.height).ok()
}

/// Reuse the saved position only if a good part of the window is still on a connected screen.
fn on_screen<T>(g: Geometry, target: &EventLoopWindowTarget<T>) -> bool {
    target.available_monitors().any(|m| {
        let (p, s) = (m.position(), m.size());
        let w = (g.x + g.w as i32).min(p.x + s.width as i32) - g.x.max(p.x);
        let h = (g.y + g.h as i32).min(p.y + s.height as i32) - g.y.max(p.y);
        w >= 200 && h >= 120
    })
}

fn main() {
    let serve_only = std::env::args().any(|a| a == "--serve");
    let (data_dir, config_dir) = app_dirs();
    let plugins_dir = data_dir.join("plugins");
    let _ = std::fs::create_dir_all(&plugins_dir);

    let (srv, port) = match server::bind() {
        Ok(v) => v,
        Err(e) => fatal(&format!("Could not start the local server ({e})."), "Another program may be blocking local connections."),
    };
    let shared = Arc::new(Shared {
        port,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        plugins_dir,
        download_dir: dirs::download_dir().or_else(dirs::document_dir).unwrap_or_else(std::env::temp_dir),
        settings: Settings::load(config_dir.join("settings.json")),
        fullscreen: AtomicBool::new(false),
        on_fullscreen: Mutex::new(None),
    });
    server::serve(srv, shared.clone());
    let start_url = format!("http://127.0.0.1:{port}/app.html");

    if serve_only {
        println!("{start_url}");
        loop { std::thread::park(); }
    }

    // WebKitGTK 2.42+ shows a blank window on some GPU/driver combinations unless this is set.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let event_loop = EventLoopBuilder::<UserEvent>::with_user_event().build();
    let proxy = Mutex::new(event_loop.create_proxy());
    *shared.on_fullscreen.lock().unwrap() = Some(Box::new(move |on| {
        let _ = proxy.lock().unwrap().send_event(UserEvent::Fullscreen(on));
    }));

    let saved = shared.settings.geometry().filter(|g| on_screen(*g, &event_loop));
    let mut wb = WindowBuilder::new()
        .with_title("Mosaic")
        .with_window_icon(window_icon())
        .with_min_inner_size(LogicalSize::new(360.0, 480.0))
        .with_inner_size(LogicalSize::new(1280.0, 800.0));
    if let Some(g) = saved {
        wb = wb.with_inner_size(PhysicalSize::new(g.w, g.h)).with_position(PhysicalPosition::new(g.x, g.y));
    }
    if shared.settings.saved_maximized() { wb = wb.with_maximized(true); }
    let window = wb.build(&event_loop).unwrap_or_else(|e| fatal(&format!("Could not open a window ({e})."), "Is a display available?"));
    if shared.settings.start_full_screen() {
        window.set_fullscreen(Some(Fullscreen::Borderless(None)));
        shared.fullscreen.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    // Keep sign-in and settings between launches (and out of the folder the program sits in).
    let mut ctx = WebContext::new(Some(data_dir.join("webview")));
    let origins = [format!("http://127.0.0.1:{port}/"), format!("http://localhost:{port}/")];
    let builder = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_url(start_url)
        .with_devtools(cfg!(debug_assertions))
        // Only the app itself loads in this window; every other link opens in the system browser.
        .with_navigation_handler(move |url| {
            let ours = origins.iter().any(|o| url.starts_with(o.as_str()))
                || url.starts_with("blob:") || url.starts_with("data:") || url.starts_with("about:");
            if !ours && (url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:")) {
                let _ = opener::open(url);
            }
            ours
        })
        .with_new_window_req_handler(|url, _| {
            if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("mailto:") {
                let _ = opener::open(url);
            }
            NewWindowResponse::Deny
        });

    #[cfg(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android"))]
    let webview = builder.build(&window).unwrap_or_else(|e| fatal(&format!("Could not start the web view ({e})."), WEBVIEW_HINT));
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "ios", target_os = "android")))]
    let webview = {
        use tao::platform::unix::WindowExtUnix;
        use wry::WebViewBuilderExtUnix;
        let vbox = window.default_vbox().unwrap_or_else(|| fatal("Could not attach the web view to the window.", WEBVIEW_HINT));
        builder.build_gtk(vbox).unwrap_or_else(|e| fatal(&format!("Could not start the web view ({e})."), WEBVIEW_HINT))
    };

    let mut last_windowed: Option<Geometry> = saved;
    let mut was_maximized = shared.settings.saved_maximized();
    let remember = |window: &tao::window::Window, last: &mut Option<Geometry>, max: &mut bool| {
        if window.fullscreen().is_some() { return; }
        *max = window.is_maximized();
        if !*max {
            if let Ok(p) = window.outer_position() {
                let s = window.inner_size();
                *last = Some(Geometry { x: p.x, y: p.y, w: s.width, h: s.height });
            }
        }
    };

    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let _keep_alive = (&webview, &ctx);
        match event {
            Event::UserEvent(UserEvent::Fullscreen(on)) => {
                window.set_fullscreen(on.then_some(Fullscreen::Borderless(None)));
            }
            Event::WindowEvent { event: WindowEvent::Moved(_) | WindowEvent::Resized(_), .. } => {
                remember(&window, &mut last_windowed, &mut was_maximized);
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                shared.settings.save_window(last_windowed, was_maximized);
                *flow = ControlFlow::Exit;
            }
            _ => {}
        }
    });
}
