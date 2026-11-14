//! Small JSON settings file (window size/position, full screen on start). Stored per user.
use serde_json::{json, Value};
use std::{fs, path::PathBuf, sync::Mutex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry { pub x: i32, pub y: i32, pub w: u32, pub h: u32 }

pub struct Settings { path: PathBuf, data: Mutex<Value> }

impl Settings {
    pub fn load(path: PathBuf) -> Self {
        let data = fs::read_to_string(&path).ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(Value::is_object)
            .unwrap_or_else(|| json!({}));
        Settings { path, data: Mutex::new(data) }
    }
    fn flush(&self, d: &Value) {
        if let Some(dir) = self.path.parent() { let _ = fs::create_dir_all(dir); }
        let _ = fs::write(&self.path, serde_json::to_vec_pretty(d).unwrap_or_default());
    }
    pub fn get_bool(&self, key: &str, default: bool) -> bool {
        self.data.lock().unwrap().get(key).and_then(Value::as_bool).unwrap_or(default)
    }
    pub fn set_bool(&self, key: &str, v: bool) {
        let mut d = self.data.lock().unwrap();
        d[key] = json!(v);
        self.flush(&d);
    }
    pub fn start_full_screen(&self) -> bool { self.get_bool("startFullScreen", false) }
    pub fn remember(&self) -> bool { self.get_bool("remember", true) }
    pub fn saved_maximized(&self) -> bool { self.remember() && self.get_bool("maximized", false) }

    /// Saved window rectangle, or None when nothing is saved, remembering is off or it is too small.
    /// The caller still has to check the spot is on a connected screen.
    pub fn geometry(&self) -> Option<Geometry> {
        if !self.remember() { return None; }
        let d = self.data.lock().unwrap();
        let n = |k: &str| d.get(k).and_then(Value::as_i64);
        let g = Geometry { x: n("x")? as i32, y: n("y")? as i32, w: n("w")? as u32, h: n("h")? as u32 };
        (g.w >= 360 && g.h >= 480).then_some(g)
    }
    pub fn save_window(&self, g: Option<Geometry>, maximized: bool) {
        if !self.remember() { return; }
        let mut d = self.data.lock().unwrap();
        if let Some(g) = g.filter(|g| g.w >= 360 && g.h >= 480) {
            d["x"] = json!(g.x); d["y"] = json!(g.y); d["w"] = json!(g.w); d["h"] = json!(g.h);
        }
        d["maximized"] = json!(maximized);
        self.flush(&d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip_and_limits() {
        let p = std::env::temp_dir().join(format!("mosaic-settings-{}.json", std::process::id()));
        let _ = fs::remove_file(&p);
        let s = Settings::load(p.clone());
        assert!(s.geometry().is_none());
        assert!(s.remember() && !s.start_full_screen());
        s.save_window(Some(Geometry { x: 10, y: 20, w: 1000, h: 700 }), true);
        s.set_bool("startFullScreen", true);
        let s2 = Settings::load(p.clone());
        assert_eq!(s2.geometry(), Some(Geometry { x: 10, y: 20, w: 1000, h: 700 }));
        assert!(s2.saved_maximized() && s2.start_full_screen());
        s2.save_window(Some(Geometry { x: 0, y: 0, w: 100, h: 100 }), false); // too small: keeps the old rect
        assert_eq!(s2.geometry().unwrap().w, 1000);
        s2.set_bool("remember", false);
        assert!(s2.geometry().is_none() && !s2.saved_maximized());
        let _ = fs::remove_file(p);
    }
}
