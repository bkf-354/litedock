//! Cuaca: Open-Meteo (tanpa kunci API) lewat WinHTTP di thread latar; diperbarui tiap 15 menit.
use crate::location::{http_get, json_num};
use std::sync::Mutex;
use std::time::Duration;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Default)]
pub struct Weather {
    pub temp: i32,
    pub feels: i32,
    pub code: i32,
    pub day: bool,
    pub humidity: i32,
    pub wind: i32,
    pub hi: i32,
    pub lo: i32,
}

static SNAP: Mutex<Option<Weather>> = Mutex::new(None);
static COORDS: Mutex<Option<(f64, f64, String)>> = Mutex::new(None);
static THREAD: std::sync::OnceLock<std::thread::Thread> = std::sync::OnceLock::new();
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Cuaca hanya diambil bila modul cuaca diaktifkan.
pub fn set_enabled(on: bool) {
    let was = ENABLED.swap(on, std::sync::atomic::Ordering::Relaxed);
    if on && !was {
        kick();
    }
}

pub fn snapshot() -> Option<Weather> {
    SNAP.lock().ok().and_then(|g| g.clone())
}

pub fn place() -> String {
    COORDS.lock().ok().and_then(|g| g.as_ref().map(|c| c.2.clone())).unwrap_or_default()
}

/// Koordinat lokasi dari pengaturan; bila berubah, cuaca diambil ulang segera.
pub fn set_coords(lat: f64, lon: f64, name: &str) {
    let changed = {
        let mut g = COORDS.lock().unwrap_or_else(|e| e.into_inner());
        let ch = g.as_ref().map(|c| (c.0 - lat).abs() > 0.01 || (c.1 - lon).abs() > 0.01).unwrap_or(true);
        *g = Some((lat, lon, name.to_string()));
        ch
    };
    if changed {
        kick();
    }
}

pub fn kick() {
    if let Some(t) = THREAD.get() {
        t.unpark();
    }
}

fn after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let i = s.find(key)? + key.len();
    Some(&s[i..])
}

fn fetch() -> Option<Weather> {
    let (lat, lon, _) = COORDS.lock().ok()?.clone()?;
    let path = format!(
        "/v1/forecast?latitude={:.3}&longitude={:.3}&current=temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code,wind_speed_10m&daily=temperature_2m_max,temperature_2m_min&timezone=auto&forecast_days=1",
        lat, lon
    );
    let body = http_get("api.open-meteo.com", &path)?;
    // "current_units" muncul lebih dulu dan memuat kunci yang sama: baca hanya bagian "current":{...}
    let cur = after(&body, "\"current\":{")?;
    let cur = &cur[..cur.find('}').unwrap_or(cur.len())];
    let daily = after(&body, "\"daily\":{").unwrap_or("");
    let first = |k: &str| -> Option<f64> {
        let r = after(daily, k)?.trim_start().strip_prefix('[')?;
        let end = r.find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '.')).unwrap_or(r.len());
        r[..end].parse().ok()
    };
    Some(Weather {
        temp: json_num(cur, "temperature_2m")?.round() as i32,
        feels: json_num(cur, "apparent_temperature").unwrap_or(0.0).round() as i32,
        code: json_num(cur, "weather_code").unwrap_or(2.0) as i32,
        day: json_num(cur, "is_day").unwrap_or(1.0) > 0.5,
        humidity: json_num(cur, "relative_humidity_2m").unwrap_or(0.0).round() as i32,
        wind: json_num(cur, "wind_speed_10m").unwrap_or(0.0).round() as i32,
        hi: first("\"temperature_2m_max\":").unwrap_or(0.0).round() as i32,
        lo: first("\"temperature_2m_min\":").unwrap_or(0.0).round() as i32,
    })
}

/// Thread pembaruan cuaca. `msg` dikirim ke `hwnd` setiap data baru berhasil diambil.
pub fn start(hwnd: isize, msg: u32) {
    let h = std::thread::spawn(move || {
        loop {
            if !ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_secs(30));
                continue;
            }
            if COORDS.lock().map(|g| g.is_some()).unwrap_or(false) {
                if let Some(w) = fetch() {
                    if let Ok(mut g) = SNAP.lock() {
                        *g = Some(w);
                    }
                    unsafe {
                        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0));
                    }
                    std::thread::park_timeout(Duration::from_secs(15 * 60));
                    continue;
                }
            }
            // gagal/belum ada koordinat: coba lagi 1 menit kemudian
            std::thread::park_timeout(Duration::from_secs(60));
        }
    });
    let _ = THREAD.set(h.thread().clone());
}
