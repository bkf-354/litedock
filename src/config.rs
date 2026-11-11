//! Konfigurasi: file INI sederhana di %APPDATA%\LiteDock\config.ini (tanpa dependensi serde).
use std::{fs, path::PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Mac,
    Dark,
    Surface,
    Light,
    Clear,
}

impl Style {
    pub const ALL: [(Style, &'static str, &'static str); 5] = [
        (Style::Mac, "mac", "Mac (kaca terang)"),
        (Style::Dark, "dark", "Gelap"),
        (Style::Surface, "surface", "Surface (ikut tema Windows)"),
        (Style::Light, "light", "Terang"),
        (Style::Clear, "clear", "Bening"),
    ];
    pub fn key(self) -> &'static str {
        Self::ALL.iter().find(|s| s.0 == self).map(|s| s.1).unwrap_or("mac")
    }
    pub fn from_key(k: &str) -> Style {
        Self::ALL.iter().find(|s| s.1 == k).map(|s| s.0).unwrap_or(Style::Mac)
    }
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| s.0 == self).unwrap_or(0)
    }
}

/// Kota preset (nama, lintang, bujur). Entri terakhir = kustom.
pub const CITIES: &[(&str, f64, f64)] = &[
    ("Jakarta", -6.2088, 106.8456),
    ("Bandung", -6.9175, 107.6191),
    ("Bogor", -6.5950, 106.8166),
    ("Semarang", -6.9667, 110.4167),
    ("Yogyakarta", -7.7956, 110.3695),
    ("Surabaya", -7.2575, 112.7521),
    ("Denpasar", -8.6500, 115.2167),
    ("Medan", 3.5952, 98.6722),
    ("Banda Aceh", 5.5483, 95.3238),
    ("Padang", -0.9471, 100.4172),
    ("Palembang", -2.9761, 104.7754),
    ("Pontianak", -0.0263, 109.3425),
    ("Banjarmasin", -3.3194, 114.5908),
    ("Balikpapan", -1.2379, 116.8529),
    ("Makassar", -5.1477, 119.4327),
    ("Manado", 1.4748, 124.8421),
    ("Mataram", -8.5833, 116.1167),
    ("Kupang", -10.1772, 123.6070),
    ("Jayapura", -2.5337, 140.7181),
    ("Kustom (isi lintang/bujur)", 0.0, 0.0),
];

#[derive(Clone)]
pub struct Config {
    pub icon_size: i32,
    pub zoom: f32,
    pub zoom_on: bool,
    pub style: Style,
    pub opacity: i32,
    pub radius: i32,
    pub margin: i32,
    pub label_gap: i32,
    pub anim: bool,
    pub anim_speed: i32,
    pub indicator: bool,
    pub hide_taskbar: bool,
    pub reserve_space: bool,
    pub show_start: bool,
    pub show_prayer: bool,
    pub show_clock: bool,
    pub show_wifi: bool,
    /// Ikon baterai (glif Windows 11) tampil.
    pub show_battery: bool,
    /// Persentase baterai tampil (terpisah dari ikonnya).
    pub show_battery_pct: bool,
    pub show_tray_btn: bool,
    pub clock_24h: bool,
    pub autostart: bool,
    pub hide_fullscreen: bool,
    /// 0 tidak pernah, 1 layar penuh, 2 jendela maksimal/layar penuh, 3 selalu (muncul di tepi bawah)
    pub hide_mode: usize,
    /// Ukuran ikon tray / modul sistem (Wi-Fi, baterai, tray, jam, salat, cuaca) dalam px; terpisah dari ukuran ikon aplikasi.
    pub tray_size: i32,
    pub clock_bg: bool,
    /// Skala teks dock (%), memengaruhi jam, tanggal, persen baterai, kartu salat.
    pub text_scale: i32,
    /// 0 = Segoe UI (bawaan Windows), 1 = Roboto (disertakan)
    pub font: i32,
    /// Warna latar ubin Start: 0 = terang, 1 = gelap.
    pub start_bg: i32,
    /// Ikon sistem (Wi-Fi, baterai, dll): false = vektor bergaya macOS, true = glif Segoe Fluent Icons.
    pub win_icons: bool,
    /// false = Reguler, true = Tebal (teks dock dan info salat)
    pub font_bold: bool,
    /// LiteDock menjadi host ikon tray (ikon tampil di panel LiteDock, bukan taskbar Windows).
    pub tray_host: bool,
    /// lama dock tampil (detik) saat dimunculkan di atas aplikasi layar penuh sebelum disembunyikan lagi
    pub fs_reveal_secs: i32,
    /// jarak antar modul sistem (px pada skala 1.0)
    pub sys_gap: i32,
    /// padding kiri/kanan latar dock (px pada skala 1.0)
    pub pad_x: i32,
    /// warna teks dock 0xRRGGBB; -1 = otomatis (mengikuti tema)
    pub text_color: i32,
    /// latar kapsul pada info dan hitung mundur salat
    pub prayer_pill: bool,
    /// urutan modul sistem di dock (daftar kunci dipisah koma)
    pub order: String,
    pub show_bin: bool,
    pub show_explorer: bool,
    pub show_power: bool,
    pub show_control: bool,
    pub show_music: bool,
    pub show_weather: bool,
    pub banner_caps: bool,
    pub banner_clip: bool,
    pub banner_apps: bool,
    pub banner_sys: bool,
    pub banner_hide_win: bool,
    /// 0 lengkap (ikon, nama, jam, titik, hitung mundur), 1 hanya info waktu (ikon, nama, jam), 2 hanya hitung mundur (ikon, hitung mundur)
    pub prayer_style: usize,
    pub cfg_ver: i32,
    pub notify: bool,
    pub notify_sound: bool,
    pub auto_loc: bool,
    pub auto_lat: f64,
    pub auto_lon: f64,
    pub auto_name: String,
    pub auto_ts: i64,
    pub city: usize,
    pub lat: f64,
    pub lon: f64,
    pub method: usize,
    pub asr_hanafi: bool,
    pub pinned: Vec<String>,
    /// Status taskbar Windows sebelum disembunyikan (untuk pemulihan setelah crash); -1 = tidak ada.
    pub tb_prev: i64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icon_size: 52,
            zoom: 1.75,
            zoom_on: true,
            style: Style::Mac,
            opacity: 100,
            radius: 20,
            margin: 8,
            label_gap: 9,
            anim: true,
            anim_speed: 100,
            indicator: true,
            hide_taskbar: true,
            reserve_space: true,
            show_start: true,
            show_prayer: true,
            show_clock: true,
            show_wifi: true,
            show_battery: true,
            show_battery_pct: true,
            show_tray_btn: true,
            clock_24h: true,
            autostart: false,
            hide_fullscreen: true,
            hide_mode: 2,
            tray_size: 52,
            clock_bg: false,
            text_scale: 100,
            font: 2,
            start_bg: 0,
            win_icons: false,
            font_bold: false,
            tray_host: true,
            fs_reveal_secs: 8,
            sys_gap: 14,
            pad_x: 9,
            text_color: -1,
            prayer_pill: true,
            order: DEFAULT_ORDER.to_string(),
            show_bin: false,
            show_explorer: false,
            show_power: false,
            show_control: false,
            show_music: false,
            show_weather: false,
            banner_caps: true,
            banner_clip: true,
            banner_apps: true,
            banner_sys: true,
            banner_hide_win: false,
            prayer_style: 0,
            cfg_ver: 6,
            notify: true,
            notify_sound: true,
            auto_loc: false,
            auto_lat: 0.0,
            auto_lon: 0.0,
            auto_name: String::new(),
            auto_ts: 0,
            city: 0,
            lat: CITIES[0].1,
            lon: CITIES[0].2,
            method: 0,
            asr_hanafi: false,
            pinned: Vec::new(),
            tb_prev: -1,
        }
    }
}

pub fn dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("LiteDock")
}

fn path() -> PathBuf {
    dir().join("config.ini")
}

fn pb(v: &str) -> bool {
    v == "1" || v.eq_ignore_ascii_case("true")
}

impl Config {
    pub fn load() -> Config {
        let mut c = Config::default();
        let Ok(text) = fs::read_to_string(path()) else {
            return c;
        };
        let mut pins = Vec::new();
        let mut legacy_scale = 100;
        let mut has_tray_size = false;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            match k {
                "icon_size" => c.icon_size = v.parse().unwrap_or(c.icon_size),
                "zoom" => c.zoom = v.parse().unwrap_or(c.zoom),
                "zoom_on" => c.zoom_on = pb(v),
                "style" => c.style = Style::from_key(v),
                "opacity" => c.opacity = v.parse().unwrap_or(c.opacity),
                "radius" => c.radius = v.parse().unwrap_or(c.radius),
                "margin" => c.margin = v.parse().unwrap_or(c.margin),
                "label_gap" => c.label_gap = v.parse().unwrap_or(c.label_gap),
                "anim" => c.anim = pb(v),
                "anim_speed" => c.anim_speed = v.parse().unwrap_or(c.anim_speed),
                "indicator" => c.indicator = pb(v),
                "hide_taskbar" => c.hide_taskbar = pb(v),
                "reserve_space" => c.reserve_space = pb(v),
                "show_start" => c.show_start = pb(v),
                "show_prayer" => c.show_prayer = pb(v),
                "show_clock" => c.show_clock = pb(v),
                "show_wifi" => c.show_wifi = pb(v),
                "show_battery" => c.show_battery = pb(v),
                "show_battery_pct" => c.show_battery_pct = pb(v),
                "show_tray_btn" => c.show_tray_btn = pb(v),
                "clock_24h" => c.clock_24h = pb(v),
                "autostart" => c.autostart = pb(v),
                "hide_fullscreen" => c.hide_fullscreen = pb(v),
                "hide_mode" => c.hide_mode = v.parse().unwrap_or(2),
                "module_scale" => legacy_scale = v.parse().unwrap_or(100),
                "tray_size" => {
                    c.tray_size = v.parse().unwrap_or(c.tray_size);
                    has_tray_size = true;
                }
                "clock_bg" => c.clock_bg = pb(v),
                "text_scale" => c.text_scale = v.parse().unwrap_or(100),
                "font" => c.font = v.parse().unwrap_or(2),
                "start_bg" => c.start_bg = v.parse().unwrap_or(0),
                "win_icons" => c.win_icons = pb(v),
                "font_bold" => c.font_bold = pb(v),
                "tray_host" => c.tray_host = pb(v),
                "fs_reveal_secs" => c.fs_reveal_secs = v.parse().unwrap_or(8),
                "sys_gap" => c.sys_gap = v.parse().unwrap_or(14),
                "pad_x" => c.pad_x = v.parse().unwrap_or(9),
                "text_color" => c.text_color = v.parse().unwrap_or(-1),
                "prayer_pill" => c.prayer_pill = pb(v),
                "order" => c.order = v.to_string(),
                "show_bin" => c.show_bin = pb(v),
                "show_explorer" => c.show_explorer = pb(v),
                "show_power" => c.show_power = pb(v),
                "show_control" => c.show_control = pb(v),
                "show_music" => c.show_music = pb(v),
                "show_weather" => c.show_weather = pb(v),
                "banner_caps" => c.banner_caps = pb(v),
                "banner_clip" => c.banner_clip = pb(v),
                "banner_apps" => c.banner_apps = pb(v),
                "banner_sys" => c.banner_sys = pb(v),
                "banner_hide_win" => c.banner_hide_win = pb(v),
                "prayer_style" => c.prayer_style = v.parse().unwrap_or(2),
                "cfg_ver" => c.cfg_ver = v.parse().unwrap_or(0),
                "notify" => c.notify = pb(v),
                "notify_sound" => c.notify_sound = pb(v),
                "auto_loc" => c.auto_loc = pb(v),
                "auto_lat" => c.auto_lat = v.parse().unwrap_or(0.0),
                "auto_lon" => c.auto_lon = v.parse().unwrap_or(0.0),
                "auto_name" => c.auto_name = v.chars().filter(|ch| !ch.is_control()).take(60).collect(),
                "auto_ts" => c.auto_ts = v.parse().unwrap_or(0),
                "city" => c.city = v.parse().unwrap_or(0),
                "lat" => c.lat = v.parse().unwrap_or(c.lat),
                "lon" => c.lon = v.parse().unwrap_or(c.lon),
                "method" => c.method = v.parse().unwrap_or(0),
                "asr_hanafi" => c.asr_hanafi = pb(v),
                "tb_prev" => c.tb_prev = v.parse().unwrap_or(-1),
                "pin" => {
                    if !v.is_empty() {
                        pins.push(v.to_string())
                    }
                }
                _ => {}
            }
        }
        c.pinned = pins;
        if c.cfg_ver < 2 {
            // migrasi: deteksi lokasi otomatis aktif, mode sembunyi mengikuti pengaturan lama, info salat ringkas
            c.auto_loc = true;
            c.hide_mode = if c.hide_fullscreen { 2 } else { 0 };
            c.prayer_style = 2;
            c.cfg_ver = 2;
        }
        if c.cfg_ver < 3 {
            // jam & tanggal satu baris tanpa kapsul (mengikuti referensi taskbar Windows)
            c.clock_bg = false;
            c.cfg_ver = 3;
        }
        if c.cfg_ver < 4 {
            // v1.5: info waktu salat dan hitung mundur menjadi dua modul terpisah; jam tanpa kapsul
            c.prayer_style = 0;
            c.clock_bg = false;
            c.text_scale = 100;
            c.tray_host = true;
            c.cfg_ver = 4;
        }
        if c.cfg_ver < 5 {
            // v1.8: ukuran ikon tray dipisah dari ukuran ikon aplikasi (nilai lama = ukuran ikon x skala modul);
            // info salat + hitung mundur digabung dalam satu modul; ikon baterai dan persentasenya terpisah
            if !has_tray_size {
                c.tray_size = (c.icon_size as f32 * legacy_scale as f32 / 100.0).round() as i32;
            }
            c.show_battery_pct = true;
            c.cfg_ver = 5;
        }
        if c.cfg_ver < 6 {
            // v1.9: tampilan ala macOS: font Inter (pengganti SF Pro), ikon sistem vektor, ubin Start terang
            c.font = 2;
            c.win_icons = false;
            c.start_bg = 0;
            c.cfg_ver = 6;
        }
        c.sanitize();
        c
    }

    /// Lokasi efektif untuk jadwal salat: hasil deteksi otomatis (bila aktif & ada) atau manual.
    pub fn eff_loc(&self) -> (f64, f64, String) {
        if self.auto_loc && self.auto_ts > 0 {
            let name = if self.auto_name.is_empty() { format!("{:.2}, {:.2}", self.auto_lat, self.auto_lon) } else { self.auto_name.clone() };
            (self.auto_lat, self.auto_lon, name)
        } else {
            let name = if self.city < CITIES.len() - 1 { CITIES[self.city].0.to_string() } else { format!("{:.2}, {:.2}", self.lat, self.lon) };
            (self.lat, self.lon, name)
        }
    }

    pub fn sanitize(&mut self) {
        self.icon_size = self.icon_size.clamp(28, 96);
        self.zoom = self.zoom.clamp(1.0, 2.5);
        self.opacity = self.opacity.clamp(15, 100);
        self.radius = self.radius.clamp(0, 36);
        self.margin = self.margin.clamp(0, 48);
        self.label_gap = self.label_gap.clamp(0, 40);
        self.tray_size = self.tray_size.clamp(24, 120);
        self.hide_mode = self.hide_mode.min(3);
        self.prayer_style = self.prayer_style.min(2);
        self.text_scale = self.text_scale.clamp(60, 160);
        self.font = self.font.clamp(0, 2);
        self.start_bg = self.start_bg.clamp(0, 1);
        self.fs_reveal_secs = self.fs_reveal_secs.clamp(5, 15);
        self.sys_gap = self.sys_gap.clamp(0, 40);
        self.pad_x = self.pad_x.clamp(0, 60);
        if self.text_color > 0xFFFFFF || self.text_color < -1 {
            self.text_color = -1;
        }
        self.order = normalize_order(&self.order);
        self.hide_fullscreen = self.hide_mode > 0;
        self.anim_speed = self.anim_speed.clamp(40, 250);
        self.city = self.city.min(CITIES.len() - 1);
        self.lat = self.lat.clamp(-66.0, 66.0);
        self.lon = self.lon.clamp(-180.0, 180.0);
        self.auto_lat = self.auto_lat.clamp(-66.0, 66.0);
        self.auto_lon = self.auto_lon.clamp(-180.0, 180.0);
        self.method = self.method.min(crate::prayer::METHODS.len() - 1);
        if !self.zoom_on {
            self.zoom = self.zoom.max(1.0);
        }
    }

    pub fn save(&self) {
        let mut s = String::with_capacity(1024);
        let b = |v: bool| if v { "1" } else { "0" };
        let mut line = |k: &str, v: String| {
            s.push_str(k);
            s.push('=');
            s.push_str(&v);
            s.push('\n');
        };
        line("icon_size", self.icon_size.to_string());
        line("zoom", format!("{:.2}", self.zoom));
        line("zoom_on", b(self.zoom_on).into());
        line("style", self.style.key().into());
        line("opacity", self.opacity.to_string());
        line("radius", self.radius.to_string());
        line("margin", self.margin.to_string());
        line("label_gap", self.label_gap.to_string());
        line("anim", b(self.anim).into());
        line("anim_speed", self.anim_speed.to_string());
        line("indicator", b(self.indicator).into());
        line("hide_taskbar", b(self.hide_taskbar).into());
        line("reserve_space", b(self.reserve_space).into());
        line("show_start", b(self.show_start).into());
        line("show_prayer", b(self.show_prayer).into());
        line("show_clock", b(self.show_clock).into());
        line("show_wifi", b(self.show_wifi).into());
        line("show_battery", b(self.show_battery).into());
        line("show_battery_pct", b(self.show_battery_pct).into());
        line("show_tray_btn", b(self.show_tray_btn).into());
        line("clock_24h", b(self.clock_24h).into());
        line("autostart", b(self.autostart).into());
        line("hide_fullscreen", b(self.hide_fullscreen).into());
        line("hide_mode", self.hide_mode.to_string());
        line("tray_size", self.tray_size.to_string());
        line("clock_bg", b(self.clock_bg).into());
        line("text_scale", self.text_scale.to_string());
        line("font", self.font.to_string());
        line("start_bg", self.start_bg.to_string());
        line("win_icons", b(self.win_icons).into());
        line("font_bold", b(self.font_bold).into());
        line("fs_reveal_secs", self.fs_reveal_secs.to_string());
        line("sys_gap", self.sys_gap.to_string());
        line("pad_x", self.pad_x.to_string());
        line("text_color", self.text_color.to_string());
        line("prayer_pill", b(self.prayer_pill).into());
        line("order", self.order.clone());
        line("show_bin", b(self.show_bin).into());
        line("show_explorer", b(self.show_explorer).into());
        line("show_power", b(self.show_power).into());
        line("show_control", b(self.show_control).into());
        line("show_music", b(self.show_music).into());
        line("show_weather", b(self.show_weather).into());
        line("banner_caps", b(self.banner_caps).into());
        line("banner_clip", b(self.banner_clip).into());
        line("banner_apps", b(self.banner_apps).into());
        line("banner_sys", b(self.banner_sys).into());
        line("banner_hide_win", b(self.banner_hide_win).into());
        line("tray_host", b(self.tray_host).into());
        line("prayer_style", self.prayer_style.to_string());
        line("cfg_ver", self.cfg_ver.to_string());
        line("notify", b(self.notify).into());
        line("notify_sound", b(self.notify_sound).into());
        line("auto_loc", b(self.auto_loc).into());
        line("auto_lat", format!("{:.5}", self.auto_lat));
        line("auto_lon", format!("{:.5}", self.auto_lon));
        line("auto_name", self.auto_name.chars().filter(|ch| !ch.is_control()).collect());
        line("auto_ts", self.auto_ts.to_string());
        line("city", self.city.to_string());
        line("lat", format!("{:.5}", self.lat));
        line("lon", format!("{:.5}", self.lon));
        line("method", self.method.to_string());
        line("asr_hanafi", b(self.asr_hanafi).into());
        line("tb_prev", self.tb_prev.to_string());
        for p in &self.pinned {
            line("pin", p.clone());
        }
        let _ = fs::create_dir_all(dir());
        let tmp = dir().join("config.tmp");
        if fs::write(&tmp, s).is_ok() {
            let _ = fs::rename(&tmp, path());
        }
    }
}

/// Kunci modul sistem yang bisa diurutkan di dock.
pub const MODULE_KEYS: [&str; 11] = ["prayer", "wifi", "battery", "clock", "tray", "bin", "explorer", "power", "control", "music", "weather"];
pub const DEFAULT_ORDER: &str = "prayer,wifi,battery,clock,tray,bin,explorer,power,control,music,weather";

/// Rapikan daftar urutan: buang kunci tak dikenal/ganda, tambahkan yang hilang di akhir.
pub fn normalize_order(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for k in s.split(',') {
        let k = k.trim();
        if let Some(m) = MODULE_KEYS.iter().find(|m| **m == k) {
            if !out.contains(m) {
                out.push(m);
            }
        }
    }
    for m in MODULE_KEYS {
        if !out.contains(&m) {
            out.push(m);
        }
    }
    out.join(",")
}
