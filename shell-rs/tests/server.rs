use mosaic_shell::{server, settings::Settings};
use std::{fs, io::{Read, Write}, net::TcpStream, path::PathBuf, sync::{atomic::AtomicBool, Arc, Mutex}};

struct Reply { status: u16, headers: String, body: Vec<u8> }
impl Reply { fn header(&self, name: &str) -> Option<String> {
    self.headers.lines().find_map(|l| { let (k, v) = l.split_once(':')?; k.eq_ignore_ascii_case(name).then(|| v.trim().to_string()) }) } }

fn tmp(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("mosaic-it-{}-{}", std::process::id(), name));
    let _ = fs::remove_dir_all(&p); fs::create_dir_all(&p).unwrap(); p
}
fn start(name: &str) -> (u16, PathBuf, Arc<server::Shared>) {
    let (srv, port) = server::bind_on(0).unwrap();
    let dir = tmp(name);
    let sh = Arc::new(server::Shared {
        port, app_version: "9.9.9".into(), plugins_dir: dir.join("plugins"), download_dir: dir.clone(),
        settings: Settings::load(dir.join("settings.json")),
        fullscreen: AtomicBool::new(false), on_fullscreen: Mutex::new(None),
    });
    fs::create_dir_all(&sh.plugins_dir).unwrap();
    server::serve(srv, sh.clone());
    (port, dir, sh)
}
fn raw(port: u16, req: &str, body: &[u8]) -> Reply {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.write_all(req.as_bytes()).unwrap(); s.write_all(body).unwrap();
    let mut out = Vec::new(); s.read_to_end(&mut out).unwrap();
    let split = out.windows(4).position(|w| w == b"\r\n\r\n").expect("no header end");
    let head = String::from_utf8_lossy(&out[..split]).to_string();
    let status = head.lines().next().unwrap().split(' ').nth(1).unwrap().parse().unwrap();
    let mut body = out[split + 4..].to_vec();
    if head.to_ascii_lowercase().contains("transfer-encoding: chunked") { body = dechunk(&body); }
    Reply { status, headers: head, body }
}
/// The server streams larger files with chunked transfer encoding (every web view decodes this).
fn dechunk(mut b: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let eol = b.windows(2).position(|w| w == b"\r\n").expect("chunk size line");
        let n = usize::from_str_radix(std::str::from_utf8(&b[..eol]).unwrap().split(';').next().unwrap().trim(), 16).expect("hex size");
        b = &b[eol + 2..];
        if n == 0 { return out; }
        out.extend_from_slice(&b[..n]);
        b = &b[n + 2..];
    }
}
fn get(port: u16, path: &str) -> Reply {
    raw(port, &format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"), b"")
}
fn post(port: u16, path: &str, extra: &str, body: &[u8]) -> Reply {
    raw(port, &format!("POST {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()), body)
}
fn ours(port: u16) -> String { format!("X-Mosaic: 1\r\nOrigin: http://127.0.0.1:{port}\r\n") }

#[test]
fn serves_the_embedded_web_app_with_csp() {
    let (port, _d, _s) = start("serve");
    for p in ["/", "/app.html", "/js/04.js", "/ui.css", "/sw.js", "/manifest.webmanifest"] {
        let r = get(port, p);
        assert_eq!(r.status, 200, "{p}");
        assert!(r.header("content-security-policy").unwrap().contains("default-src 'self'"), "{p}");
        assert_eq!(r.header("x-content-type-options").as_deref(), Some("nosniff"));
    }
    let on_disk = fs::read("../web/js/04.js").unwrap();
    let served = get(port, "/js/04.js").body;
    assert!(served == on_disk, "embedded bytes must equal web/js/04.js ({} served vs {} on disk)", served.len(), on_disk.len());
    assert!(get(port, "/app.html").header("content-type").unwrap().starts_with("text/html"));
    assert_eq!(get(port, "/nope.html").status, 404);
}

#[test]
fn rejects_bad_hosts_and_paths() {
    let (port, _d, _s) = start("bad");
    let r = raw(port, "GET /app.html HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n", b"");
    assert_eq!(r.status, 403, "DNS rebinding guard");
    assert_eq!(get(port, "/%2e%2e/secret").status, 400);
    assert_eq!(get(port, "/js/..%5Cx").status, 400);
    assert_eq!(get(port, "/user-plugins/../../etc/passwd").status, 404);
    assert_eq!(raw(port, &format!("DELETE /app.html HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"), b"").status, 405);
}

#[test]
fn app_endpoint_identifies_the_app() {
    let (port, _d, _s) = start("app");
    let v: serde_json::Value = serde_json::from_slice(&get(port, "/__app").body).unwrap();
    assert_eq!(v["app"], true);
    assert_eq!(v["version"], "9.9.9");
    assert!(v["pluginsDir"].as_str().unwrap().ends_with("plugins"));
}

#[test]
fn save_needs_our_origin_and_writes_the_file() {
    let (port, dir, _s) = start("save");
    let out = dir.join("saved");
    std::env::set_var("MOSAIC_AUTOSAVE_DIR", &out);
    assert_eq!(post(port, "/__save?name=a.png", "", b"x").status, 403, "no header/origin");
    assert_eq!(post(port, "/__save?name=a.png", "X-Mosaic: 1\r\nOrigin: http://evil.example\r\n", b"x").status, 403, "foreign origin");
    let r = post(port, "/__save?name=..%2F..%2Fhello%3Cworld%3E.txt", &ours(port), b"payload bytes");
    assert_eq!(r.status, 200);
    assert_eq!(fs::read(out.join("hello_world_.txt")).unwrap(), b"payload bytes");
    assert!(!dir.join("hello_world_.txt").exists());
}

#[test]
fn window_endpoint_state_and_commands() {
    let (port, _d, sh) = start("win");
    assert_eq!(get(port, "/__window").status, 403, "needs X-Mosaic");
    let st = raw(port, &format!("GET /__window HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Mosaic: 1\r\nConnection: close\r\n\r\n"), b"");
    let v: serde_json::Value = serde_json::from_slice(&st.body).unwrap();
    assert_eq!((v["fullscreen"].as_bool(), v["remember"].as_bool(), v["startFullscreen"].as_bool()), (Some(false), Some(true), Some(false)));

    let seen = Arc::new(Mutex::new(Vec::new()));
    let s2 = seen.clone();
    *sh.on_fullscreen.lock().unwrap() = Some(Box::new(move |on| s2.lock().unwrap().push(on)));
    assert_eq!(post(port, "/__window?cmd=fullscreen&v=1", "X-Mosaic: 1\r\n", b"").status, 403, "POST also needs the Origin");
    assert_eq!(post(port, "/__window?cmd=fullscreen&v=1", &ours(port), b"").status, 200);
    assert_eq!(post(port, "/__window?cmd=fullscreen&v=0", &ours(port), b"").status, 200);
    assert_eq!(*seen.lock().unwrap(), vec![true, false]);
    assert_eq!(post(port, "/__window?cmd=startFullscreen&v=1", &ours(port), b"").status, 200);
    assert!(sh.settings.start_full_screen());
    assert_eq!(post(port, "/__window?cmd=remember&v=0", &ours(port), b"").status, 200);
    assert!(!sh.settings.remember());
    assert_eq!(post(port, "/__window?cmd=format-disk&v=1", &ours(port), b"").status, 400);
}

#[test]
fn user_plugins_are_listed_and_served_safely() {
    let (port, _d, sh) = start("plug");
    let good = sh.plugins_dir.join("hello");
    fs::create_dir_all(&good).unwrap();
    fs::write(good.join("manifest.json"), r#"{"id":"hello","entry":"main.js"}"#).unwrap();
    fs::write(good.join("main.js"), "console.log(1)").unwrap();
    fs::write(good.join("secret.exe"), "x").unwrap();
    let bad = sh.plugins_dir.join("bad");
    fs::create_dir_all(&bad).unwrap();
    fs::write(bad.join("manifest.json"), r#"{"id":"bad","entry":"../x.js"}"#).unwrap();
    fs::create_dir_all(sh.plugins_dir.join(".hidden")).unwrap();

    let list: serde_json::Value = serde_json::from_slice(&get(port, "/user-plugins/index.json").body).unwrap();
    assert_eq!(list.as_array().unwrap().len(), 1);
    assert_eq!(list[0]["entry"], "hello/main.js");
    let js = get(port, "/user-plugins/hello/main.js");
    assert_eq!((js.status, js.body.as_slice()), (200, &b"console.log(1)"[..]));
    assert_eq!(get(port, "/user-plugins/hello/secret.exe").status, 404, "extension allow-list");
    assert_eq!(get(port, "/user-plugins/hello/..%2Fbad%2Fmanifest.json").status, 404);
}
