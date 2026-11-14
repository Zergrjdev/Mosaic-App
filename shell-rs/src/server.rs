//! Loopback HTTP server. Same behaviour and endpoints as the Qt app's AssetServer:
//!   GET  /…                 the embedded web app (strict CSP, same as the site's _headers)
//!   GET  /__app             lets the page detect that it runs inside the app
//!   POST /__save?name=…     "save this file": native Save dialog (the page cannot use <a download>)
//!   GET/POST /__window      full screen and remembered window state
//!   GET  /user-plugins/…    plugins the person drops into the plugins folder
use crate::settings::Settings;
use percent_encoding::percent_decode_str;
use rust_embed::RustEmbed;
use serde_json::{json, Value};
use std::{
    fs, io::Read, path::{Path, PathBuf},
    sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex},
};
use tiny_http::{Header, Method, Request, Response, Server};

/// The whole web app, compiled into the executable. Nothing has to be installed next to it.
#[derive(RustEmbed)]
#[folder = "../web/"]
struct Web;

/// Bytes of one embedded web file (used for the window icon).
pub fn asset(path: &str) -> Option<Vec<u8>> { Web::get(path).map(|f| f.data.into_owned()) }

pub const CSP: &str = "default-src 'self'; script-src 'self' blob:; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; \
font-src 'self' https://fonts.gstatic.com data:; img-src 'self' data: blob: https:; media-src 'self' data: blob: https:; \
connect-src 'self' https://*.supabase.co wss://*.supabase.co https://discord.com https://discordapp.com https://fonts.googleapis.com http://127.0.0.1:* http://localhost:* https:; \
base-uri 'self'; form-action 'self'; object-src 'none'; frame-ancestors 'none'";

const MAX_BODY: u64 = 512 * 1024 * 1024;
const PREFERRED_PORT: u16 = 38417; // same as the Qt app, so the saved sign-in/localStorage origin stays stable

pub struct Shared {
    pub port: u16,
    pub app_version: String,
    pub plugins_dir: PathBuf,
    pub download_dir: PathBuf,
    pub settings: Settings,
    pub fullscreen: AtomicBool,
    /// Set by the window code; asks it to enter/leave full screen. None when there is no window.
    pub on_fullscreen: Mutex<Option<Box<dyn Fn(bool) + Send>>>,
}

/// Tries the preferred port first, then any free one.
pub fn bind() -> Result<(Server, u16), String> {
    bind_on(PREFERRED_PORT)
}
pub fn bind_on(preferred: u16) -> Result<(Server, u16), String> {
    let server = Server::http(("127.0.0.1", preferred))
        .or_else(|_| Server::http(("127.0.0.1", 0)))
        .map_err(|e| e.to_string())?;
    let port = server.server_addr().to_ip().ok_or("no ip address")?.port();
    Ok((server, port))
}

/// Starts answering requests on background threads.
pub fn serve(server: Server, shared: Arc<Shared>) {
    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let sh = shared.clone();
            std::thread::spawn(move || handle(req, &sh)); // a Save dialog may keep one request open for a while
        }
    });
}

fn header(req: &Request, name: &str) -> Option<String> {
    req.headers().iter().find(|h| h.field.as_str().as_str().eq_ignore_ascii_case(name)).map(|h| h.value.as_str().to_string())
}
fn hdr(name: &str, value: &str) -> Header { Header::from_bytes(name.as_bytes(), value.as_bytes()).unwrap() }

fn respond(req: Request, status: u16, content_type: &str, body: Vec<u8>) {
    let r = Response::from_data(body)
        .with_status_code(status)
        .with_header(hdr("Content-Type", content_type))
        .with_header(hdr("Cache-Control", "no-cache"))
        .with_header(hdr("X-Content-Type-Options", "nosniff"))
        .with_header(hdr("Referrer-Policy", "strict-origin-when-cross-origin"))
        .with_header(hdr("Content-Security-Policy", CSP));
    let _ = req.respond(r); // tiny_http leaves the body out for HEAD requests by itself
}
fn text(req: Request, status: u16, msg: &str) { respond(req, status, "text/plain", msg.as_bytes().to_vec()) }
fn json_resp(req: Request, v: Value) { respond(req, 200, "application/json", v.to_string().into_bytes()) }

pub fn pct_decode(s: &str) -> String { percent_decode_str(s).decode_utf8_lossy().into_owned() }

pub fn query_param(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (pct_decode(k) == key).then(|| pct_decode(v))
    })
}

pub fn safe_file_name(name: &str) -> String {
    let last = name.rsplit(['/', '\\']).next().unwrap_or("");
    let cleaned: String = last.chars()
        .map(|c| if (c as u32) < 0x20 || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    let n: String = cleaned.trim().chars().take(120).collect();
    if n.is_empty() { "mosaic-export".into() } else { n }
}

pub fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "webmanifest" => "application/json; charset=utf-8",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "webp" => "image/webp",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Must come from our own page: right Origin and a custom header, which other websites cannot add
/// without a CORS preflight that this server never answers.
fn from_our_page(req: &Request, port: u16) -> bool {
    let origin = header(req, "origin").unwrap_or_default();
    header(req, "x-mosaic").as_deref() == Some("1")
        && (origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}"))
}

fn handle(mut req: Request, sh: &Shared) {
    // Only answer requests addressed to the loopback origin (blocks DNS-rebinding tricks).
    let host = header(&req, "host").unwrap_or_default().to_lowercase();
    if host != format!("127.0.0.1:{}", sh.port) && host != format!("localhost:{}", sh.port) {
        return text(req, 403, "Forbidden");
    }
    let method = req.method().clone();
    let url = req.url().to_string();
    if std::env::var_os("MOSAIC_LOG").is_some() { eprintln!("[mosaic] {method} {url}"); }
    let (path_q, _) = url.split_once('#').unwrap_or((url.as_str(), ""));
    let (raw_path, query) = path_q.split_once('?').unwrap_or((path_q, ""));

    if method == Method::Post && raw_path == "/__save" {
        return handle_save(req, sh, query);
    }
    if raw_path == "/__window" {
        return handle_window(req, sh, &method, query);
    }
    if method != Method::Get && method != Method::Head {
        // drain nothing: the body of a rejected request is never read
        let _ = req.as_reader().read(&mut [0u8; 0]);
        return text(req, 405, "Method not allowed");
    }
    if raw_path == "/__app" {
        return json_resp(req, json!({"app": true, "version": sh.app_version, "pluginsDir": sh.plugins_dir.to_string_lossy()}));
    }
    if let Some(rel) = raw_path.strip_prefix("/user-plugins/") {
        return handle_plugins(req, &sh.plugins_dir, &pct_decode(rel));
    }

    let mut path = pct_decode(raw_path);
    if path.is_empty() || path == "/" { path = "/index.html".into(); }
    if path.contains("..") || path.contains('\\') || !path.starts_with('/') {
        return text(req, 400, "Bad request");
    }
    match Web::get(&path[1..]) {
        Some(f) => respond(req, 200, content_type_for(&path), f.data.into_owned()),
        None => text(req, 404, "Not found"),
    }
}

fn handle_plugins(req: Request, dir: &Path, rel: &str) {
    if rel == "index.json" { // every sub-folder that has a valid manifest.json
        let mut names: Vec<String> = fs::read_dir(dir).map(|rd| rd.flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !n.starts_with('.')).collect()).unwrap_or_default();
        names.sort();
        let mut list = Vec::new();
        for d in names {
            let Ok(t) = fs::read_to_string(dir.join(&d).join("manifest.json")) else { continue };
            let Ok(mut m) = serde_json::from_str::<Value>(&t) else { continue };
            let entry = m.get("entry").and_then(Value::as_str).unwrap_or("").to_string();
            if !m.is_object() || entry.is_empty() || entry.contains("..") || entry.starts_with('/') { continue; }
            m["entry"] = json!(format!("{d}/{entry}"));
            list.push(m);
        }
        return json_resp(req, Value::Array(list));
    }
    const OK: [&str; 7] = ["js", "json", "css", "png", "svg", "txt", "md"];
    let ext_ok = rel.rsplit_once('.').map(|(_, e)| OK.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false);
    let segs_ok = !rel.is_empty() && rel.split('/').all(|s| !s.is_empty() && !s.starts_with('.') && !s.contains('\\'));
    match (ext_ok && segs_ok).then(|| fs::read(dir.join(rel)).ok()).flatten() {
        Some(bytes) => respond(req, 200, content_type_for(rel), bytes),
        None => text(req, 404, "Not found"),
    }
}

fn handle_window(req: Request, sh: &Shared, method: &Method, query: &str) {
    if header(&req, "x-mosaic").as_deref() != Some("1") { return text(req, 403, "Forbidden"); }
    if *method == Method::Get {
        return json_resp(req, json!({
            "fullscreen": sh.fullscreen.load(Ordering::Relaxed),
            "startFullscreen": sh.settings.start_full_screen(),
            "remember": sh.settings.remember(),
        }));
    }
    if *method != Method::Post { return text(req, 405, "Forbidden"); }
    if !from_our_page(&req, sh.port) { return text(req, 403, "Forbidden"); }
    let on = query_param(query, "v").as_deref() == Some("1");
    match query_param(query, "cmd").as_deref() {
        Some("fullscreen") => {
            if let Some(f) = sh.on_fullscreen.lock().unwrap().as_ref() { f(on); }
            sh.fullscreen.store(on, Ordering::Relaxed);
        }
        Some("startFullscreen") => sh.settings.set_bool("startFullScreen", on),
        Some("remember") => sh.settings.set_bool("remember", on),
        _ => return text(req, 400, "Unknown command"),
    }
    text(req, 200, "ok")
}

fn handle_save(mut req: Request, sh: &Shared, query: &str) {
    if !from_our_page(&req, sh.port) { return text(req, 403, "Forbidden"); }
    let name = safe_file_name(&query_param(query, "name").unwrap_or_default());
    if req.body_length().map(|n| n as u64 > MAX_BODY).unwrap_or(false) { return text(req, 413, "Too large"); }
    let mut body = Vec::new();
    if req.as_reader().take(MAX_BODY + 1).read_to_end(&mut body).is_err() || body.len() as u64 > MAX_BODY {
        return text(req, 413, "Too large");
    }

    // Automation / testing: save straight into a folder instead of asking.
    if let Some(dir) = std::env::var_os("MOSAIC_AUTOSAVE_DIR").filter(|d| !d.is_empty()) {
        let ok = fs::create_dir_all(&dir).and_then(|_| fs::write(Path::new(&dir).join(&name), &body)).is_ok();
        return text(req, if ok { 200 } else { 500 }, if ok { "saved" } else { "failed" });
    }
    let picked = rfd::FileDialog::new().set_title("Save export")
        .set_directory(&sh.download_dir).set_file_name(&name).save_file();
    match picked {
        None => text(req, 409, "cancelled"),
        Some(p) => {
            let ok = fs::write(p, &body).is_ok();
            text(req, if ok { 200 } else { 500 }, if ok { "saved" } else { "failed" })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn file_names_are_made_safe() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name("a<b>:c|d?.png"), "a_b__c_d_.png");
        assert_eq!(safe_file_name("  "), "mosaic-export");
        assert_eq!(safe_file_name("C:\\x\\y.txt"), "y.txt");
        assert_eq!(safe_file_name(&"a".repeat(300)).len(), 120);
    }
    #[test]
    fn query_and_types() {
        assert_eq!(query_param("name=my%20file.png&v=1", "name").as_deref(), Some("my file.png"));
        assert_eq!(query_param("cmd=fullscreen", "v"), None);
        assert_eq!(content_type_for("/a/b.WOFF2"), "font/woff2");
        assert_eq!(content_type_for("/x.unknown"), "application/octet-stream");
    }
}
