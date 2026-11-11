//! Pengaturan: UI kustom yang memakai gaya sama dengan dock (digambar dengan renderer perangkat lunak
//! yang sama, bukan kontrol Windows klasik). Dibuat saat dibuka dan dihancurkan saat ditutup.
use crate::config::*;
use crate::macui;
use crate::gfx::*;
use crate::prayer::METHODS;
use crate::shell::wide;
use std::collections::HashMap;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const W: f32 = 840.0;
const H: f32 = 640.0;
const M: f32 = 28.0;
const SIDEBAR: f32 = 196.0;
const PSTYLES: [&str; 3] = ["Lengkap: ikon, nama, jam \u{b7} hitung mundur", "Info waktu: ikon, nama, jam", "Hitung mundur: ikon dan sisa waktu"];
const HIDEMODES: [&str; 4] = ["Tidak pernah", "Saat aplikasi layar penuh", "Saat jendela maksimal, layar penuh, atau menutupi dock", "Selalu (muncul di tepi bawah layar)"];
const TABS: [&str; 9] = ["Tampilan", "Gerak", "Elemen", "Salat", "Sistem", "Teks", "Modul", "Banner", "Ukuran"];
const SWATCHES: [i32; 11] = [-1, 0xFFFFFF, 0x1C1C1E, 0xFFD60A, 0xFF9F0A, 0xFF453A, 0xFF6EB4, 0xBF5AF2, 0x0A84FF, 0x64D2FF, 0x30D158];
const FONTS: [&str; 3] = ["Segoe UI (bawaan Windows)", "Roboto (disertakan)", "Inter (gaya SF, disertakan)"];
const START_BGS: [&str; 2] = ["Terang", "Gelap"];
const WEIGHTS: [&str; 2] = ["Reguler", "Tebal"];
const WM_STATUS: u32 = WM_APP + 1;
const T_PAINT: usize = 1;
const T_ANIM: usize = 2;
const T_BLINK: usize = 3;

// id kontrol
const ZOOM_ON: u32 = 1;
const ANIM: u32 = 2;
const INDICATOR: u32 = 3;
const START: u32 = 4;
const PRAYER: u32 = 5;
const CLOCK: u32 = 6;
const WIFI: u32 = 7;
const BATT: u32 = 8;
const TRAY: u32 = 9;
const H24: u32 = 10;
const CLOCKBG: u32 = 19;
const TRAYHOST: u32 = 20;
const HIDETB: u32 = 11;
const RESERVE: u32 = 12;
const AUTOSTART: u32 = 13;
const HIDEFS: u32 = 14;
const AUTOLOC: u32 = 15;
const HANAFI: u32 = 16;
const NOTIFY: u32 = 17;
const NSOUND: u32 = 18;
const SIZE_: u32 = 30;
const ZOOM: u32 = 31;
const OPACITY: u32 = 32;
const RADIUS: u32 = 33;
const MARGIN: u32 = 34;
const LABELGAP: u32 = 35;
const SPEED: u32 = 36;
const TRAYSZ: u32 = 37;
const BATPCT: u32 = 80;
const TEXTSCALE: u32 = 38;
const CITY: u32 = 50;
const METHOD: u32 = 51;
const PSTYLE: u32 = 52;
const FONT: u32 = 60;
const STARTBG: u32 = 90;
const WINICONS: u32 = 81;
const PPILL: u32 = 70;
const BNCAPS: u32 = 71;
const BNCLIP: u32 = 72;
const BNAPPS: u32 = 73;
const BNSYS: u32 = 74;
const BNHIDE: u32 = 75;
const SYSGAP: u32 = 76;
const PADX: u32 = 77;
const TESTBANNER: u32 = 78;
const FSREVEAL: u32 = 79;
const FWEIGHT: u32 = 61;
const HIDEMODE: u32 = 53;
const LAT: u32 = 60;
const LON: u32 = 61;
const DETECT: u32 = 70;
const TEST: u32 = 71;
const RESTORE: u32 = 72;

static OPEN_HWND: AtomicIsize = AtomicIsize::new(0);
static STATUS: Mutex<String> = Mutex::new(String::new());
static mut UI: *mut Ui = std::ptr::null_mut();

fn ui() -> Option<&'static mut Ui> {
    unsafe { if UI.is_null() { None } else { Some(&mut *UI) } }
}

pub fn is_open() -> Option<HWND> {
    let h = OPEN_HWND.load(Ordering::SeqCst);
    if h == 0 { None } else { Some(HWND(h as *mut _)) }
}

pub fn set_status(t: &str) {
    if let Ok(mut g) = STATUS.lock() {
        *g = t.to_string();
    }
    if let Some(h) = is_open() {
        unsafe {
            let _ = PostMessageW(Some(h), WM_STATUS, WPARAM(0), LPARAM(0));
        }
    }
}

fn get_bool(c: &Config, id: u32) -> bool {
    match id {
        ZOOM_ON => c.zoom_on,
        ANIM => c.anim,
        INDICATOR => c.indicator,
        START => c.show_start,
        WINICONS => c.win_icons,
        PRAYER => c.show_prayer,
        CLOCK => c.show_clock,
        WIFI => c.show_wifi,
        BATT => c.show_battery,
        BATPCT => c.show_battery_pct,
        TRAY => c.show_tray_btn,
        H24 => c.clock_24h,
        CLOCKBG => c.clock_bg,
        TRAYHOST => c.tray_host,
        PPILL => c.prayer_pill,
        BNCAPS => c.banner_caps,
        BNCLIP => c.banner_clip,
        BNAPPS => c.banner_apps,
        BNSYS => c.banner_sys,
        BNHIDE => c.banner_hide_win,
        HIDETB => c.hide_taskbar,
        RESERVE => c.reserve_space,
        AUTOSTART => c.autostart,
        HIDEFS => c.hide_fullscreen,
        AUTOLOC => c.auto_loc,
        HANAFI => c.asr_hanafi,
        NOTIFY => c.notify,
        NSOUND => c.notify_sound,
        _ => false,
    }
}

fn set_bool(c: &mut Config, id: u32, v: bool) {
    match id {
        ZOOM_ON => c.zoom_on = v,
        ANIM => c.anim = v,
        INDICATOR => c.indicator = v,
        START => c.show_start = v,
        WINICONS => c.win_icons = v,
        PRAYER => c.show_prayer = v,
        CLOCK => c.show_clock = v,
        WIFI => c.show_wifi = v,
        BATT => c.show_battery = v,
        BATPCT => c.show_battery_pct = v,
        TRAY => c.show_tray_btn = v,
        H24 => c.clock_24h = v,
        CLOCKBG => c.clock_bg = v,
        TRAYHOST => c.tray_host = v,
        PPILL => c.prayer_pill = v,
        BNCAPS => c.banner_caps = v,
        BNCLIP => c.banner_clip = v,
        BNAPPS => c.banner_apps = v,
        BNSYS => c.banner_sys = v,
        BNHIDE => c.banner_hide_win = v,
        HIDETB => c.hide_taskbar = v,
        RESERVE => c.reserve_space = v,
        AUTOSTART => c.autostart = v,
        HIDEFS => c.hide_fullscreen = v,
        AUTOLOC => c.auto_loc = v,
        HANAFI => c.asr_hanafi = v,
        NOTIFY => c.notify = v,
        NSOUND => c.notify_sound = v,
        _ => {}
    }
}

fn get_int(c: &Config, id: u32) -> i32 {
    match id {
        SIZE_ => c.icon_size,
        ZOOM => (c.zoom * 100.0).round() as i32,
        OPACITY => c.opacity,
        RADIUS => c.radius,
        MARGIN => c.margin,
        LABELGAP => c.label_gap,
        SPEED => c.anim_speed,
        TRAYSZ => c.tray_size,
        TEXTSCALE => c.text_scale,
        SYSGAP => c.sys_gap,
        PADX => c.pad_x,
        FSREVEAL => c.fs_reveal_secs,
        _ => 0,
    }
}

fn set_int(c: &mut Config, id: u32, v: i32) {
    match id {
        SIZE_ => c.icon_size = v,
        ZOOM => c.zoom = v as f32 / 100.0,
        OPACITY => c.opacity = v,
        RADIUS => c.radius = v,
        MARGIN => c.margin = v,
        LABELGAP => c.label_gap = v,
        SPEED => c.anim_speed = v,
        TRAYSZ => c.tray_size = v,
        TEXTSCALE => c.text_scale = v,
        SYSGAP => c.sys_gap = v,
        PADX => c.pad_x = v,
        FSREVEAL => c.fs_reveal_secs = v,
        _ => {}
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Pal {
    pub(crate) win_a: Rgba,
    pub(crate) win_b: Rgba,
    pub(crate) border: Rgba,
    pub(crate) card: Rgba,
    pub(crate) text: Rgba,
    pub(crate) dim: Rgba,
    pub(crate) accent: Rgba,
    pub(crate) on_accent: Rgba,
    pub(crate) track: Rgba,
    pub(crate) hover: Rgba,
    pub(crate) shadow: u8,
}

pub(crate) fn palette(style: Style, light_sys: bool) -> Pal {
    let white = Rgba(255, 255, 255, 255);
    let black = Rgba(0, 0, 0, 255);
    let dark = |a: Rgba, b: Rgba, accent: Rgba, on: Rgba| Pal {
        win_a: a, win_b: b, border: white.a(0.11), card: white.a(0.065), text: white, dim: white.a(0.62), accent, on_accent: on, track: white.a(0.20), hover: white.a(0.09), shadow: 26,
    };
    let light = |a: Rgba, b: Rgba, accent: Rgba| Pal {
        win_a: a, win_b: b, border: black.a(0.10), card: black.a(0.045), text: Rgba(28, 28, 32, 255), dim: Rgba(28, 28, 32, 150), accent, on_accent: white, track: black.a(0.16), hover: black.a(0.06), shadow: 16,
    };
    match style {
        Style::Mac => Pal {
            // grafit macOS: permukaan netral (bukan kebiruan), teks sekunder ala label macOS, aksen biru sistem
            win_a: Rgba(62, 62, 68, 240), win_b: Rgba(44, 44, 49, 246), border: white.a(0.15), card: white.a(0.08), text: white, dim: Rgba(235, 235, 245, 153), accent: Rgba(10, 132, 255, 255), on_accent: white, track: white.a(0.20), hover: white.a(0.10), shadow: 30,
        },
        Style::Dark => dark(Rgba(38, 38, 44, 252), Rgba(26, 26, 30, 253), Rgba(96, 205, 255, 255), Rgba(8, 20, 30, 255)),
        Style::Light => light(Rgba(251, 251, 253, 253), Rgba(240, 241, 246, 253), Rgba(0, 103, 192, 255)),
        Style::Surface => {
            if light_sys {
                light(Rgba(245, 245, 246, 252), Rgba(236, 236, 238, 253), Rgba(0, 95, 184, 255))
            } else {
                dark(Rgba(36, 36, 36, 252), Rgba(28, 28, 28, 253), Rgba(96, 205, 255, 255), Rgba(8, 20, 30, 255))
            }
        }
        Style::Clear => Pal {
            win_a: Rgba(30, 36, 52, 214), win_b: Rgba(16, 20, 32, 226), border: white.a(0.32), card: white.a(0.08), text: white, dim: white.a(0.70), accent: Rgba(150, 214, 255, 255), on_accent: Rgba(8, 20, 30, 255), track: white.a(0.22), hover: white.a(0.10), shadow: 22,
        },
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Tab(usize),
    Toggle(u32),
    Slider(u32, i32, i32),
    Chip(usize),
    Dropdown(u32),
    Field(u32),
    Button(u32),
    Swatch(i32),
    Mod(usize, u8),
    Close,
    DdItem(usize),
    Block,
}

struct Hit {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    kind: Kind,
}

struct Dd {
    id: u32,
    x: f32,
    y: f32,
    w: f32,
    items: Vec<String>,
    sel: usize,
    scroll: usize,
}

struct Ui {
    hwnd: HWND,
    surf: Option<Surface>,
    text: TextCache,
    k: f32,
    cfg: Config,
    tab: usize,
    hits: Vec<Hit>,
    hover: Option<Kind>,
    drag: Option<(u32, f32, f32, i32, i32)>,
    anim: HashMap<u32, f32>,
    bg_cache: Vec<u32>,
    bg_key: u64,
    anim_target: HashMap<u32, f32>,
    dd: Option<Dd>,
    focus: Option<u32>,
    buf: String,
    caret: usize,
    blink: bool,
    paint_pending: bool,
    light_sys: bool,
    tracking: bool,
}

impl Ui {
    fn new(cfg: Config, k: f32, light_sys: bool) -> Ui {
        Ui {
            hwnd: HWND::default(),
            surf: None,
            text: TextCache::default(),
            k,
            cfg,
            tab: 0,
            hits: Vec::new(),
            hover: None,
            drag: None,
            anim: HashMap::new(),
            bg_cache: Vec::new(),
            bg_key: 0,
            anim_target: HashMap::new(),
            dd: None,
            focus: None,
            buf: String::new(),
            caret: 0,
            blink: true,
            paint_pending: false,
            light_sys,
            tracking: false,
        }
    }

    fn pal(&self) -> Pal {
        palette(self.cfg.style, self.light_sys)
    }

    fn t(&mut self, cv: &mut Canvas, s: &str, px: f32, bold: bool, x: f32, y: f32, col: Rgba) {
        let p = ((px * self.k).round() as i32).max(8);
        let m = self.text.get_w(s, p, panel_weight(bold, p));
        cv.draw_mask(m, x.round() as i32, y.round() as i32, col);
    }

    fn tw(&mut self, s: &str, px: f32, bold: bool) -> f32 {
        let p = ((px * self.k).round() as i32).max(8);
        self.text.get_w(s, p, panel_weight(bold, p)).w as f32
    }

    fn th(&mut self, px: f32, bold: bool) -> f32 {
        let p = ((px * self.k).round() as i32).max(8);
        self.text.get_w("Ag", p, panel_weight(bold, p)).h as f32
    }

    fn fit(&mut self, s: &str, px: f32, bold: bool, maxw: f32) -> String {
        if self.tw(s, px, bold) <= maxw {
            return s.to_string();
        }
        let mut v: Vec<char> = s.chars().collect();
        while v.len() > 1 {
            v.pop();
            let t: String = v.iter().collect::<String>() + "\u{2026}";
            if self.tw(&t, px, bold) <= maxw {
                return t;
            }
        }
        s.to_string()
    }

    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, kind: Kind) {
        self.hits.push(Hit { x, y, w, h, kind });
    }

    fn is_hover(&self, kind: Kind) -> bool {
        self.hover == Some(kind)
    }

    // ---------------------------------------------------------------- widget

    fn header(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, t: &str) -> f32 {
        let k = self.k;
        self.t(cv, &t.to_uppercase(), 11.5, true, x + 2.0 * k, y + 12.0 * k, p.dim);
        32.0 * k
    }

    fn toggle(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, id: u32, label: &str, sub: &str) -> f32 {
        let k = self.k;
        let h = 48.0 * k;
        let on = get_bool(&self.cfg, id);
        let target = if on { 1.0 } else { 0.0 };
        self.anim_target.insert(id, target);
        let t = *self.anim.entry(id).or_insert(target);
        if self.is_hover(Kind::Toggle(id)) {
            cv.fill_rrect(x, y + 2.0 * k, w, h - 4.0 * k, 12.0 * k, p.hover, p.hover);
        }
        let lh = self.th(14.5, false);
        if sub.is_empty() {
            self.t(cv, label, 14.5, false, x + 12.0 * k, y + (h - lh) / 2.0, p.text);
        } else {
            self.t(cv, label, 14.5, false, x + 12.0 * k, y + 5.0 * k, p.text);
            let s = self.fit(sub, 11.5, false, w - 100.0 * k);
            self.t(cv, &s, 11.5, false, x + 12.0 * k, y + 5.0 * k + lh + 1.0 * k, p.dim);
        }
        // switch hijau ala macOS
        let light = p.text.0 < 100;
        macui::switch(cv, x + w - 10.0 * k - 19.0 * k, y + h / 2.0, k, t, macui::green(light), p.track);
        self.hit(x, y, w, h, Kind::Toggle(id));
        h
    }

    fn slider(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, id: u32, label: &str, lo: i32, hi: i32, unit: &str) -> f32 {
        let k = self.k;
        let h = 54.0 * k;
        let v = get_int(&self.cfg, id).clamp(lo, hi);
        self.t(cv, label, 14.5, false, x + 12.0 * k, y + 5.0 * k, p.text);
        let vs = format!("{} {}", v, unit);
        let vw = self.tw(&vs, 13.0, true);
        self.t(cv, &vs, 13.0, true, x + w - vw - 12.0 * k, y + 6.0 * k, p.dim);
        let (tx, tw_) = (x + 16.0 * k, w - 32.0 * k);
        let ty = y + 36.0 * k;
        let f = (v - lo) as f32 / (hi - lo).max(1) as f32;
        cv.fill_rrect(tx, ty - 2.0 * k, tw_, 4.0 * k, 2.0 * k, p.track, p.track);
        cv.fill_rrect(tx, ty - 2.0 * k, (tw_ * f).max(4.0 * k), 4.0 * k, 2.0 * k, p.accent, p.accent);
        let hot = self.is_hover(Kind::Slider(id, lo, hi)) || self.drag.map(|d| d.0 == id).unwrap_or(false);
        let kr = if hot { 10.5 * k } else { 9.0 * k };
        cv.fill_circle(tx + tw_ * f, ty + 1.0 * k, kr + 1.5 * k, Rgba(0, 0, 0, 34));
        cv.fill_circle(tx + tw_ * f, ty, kr, Rgba(255, 255, 255, 255));
        self.hit(tx - 10.0 * k, y + 22.0 * k, tw_ + 20.0 * k, 28.0 * k, Kind::Slider(id, lo, hi));
        h
    }

    fn dropdown(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, id: u32, label: &str, value: &str, enabled: bool) -> f32 {
        let k = self.k;
        let h = 48.0 * k;
        let lh = self.th(14.5, false);
        let a = if enabled { 1.0 } else { 0.4 };
        self.t(cv, label, 14.5, false, x + 12.0 * k, y + (h - lh) / 2.0, p.text.mul_a(a));
        let bw = (w * 0.56).min(360.0 * k);
        let (bx, by, bh) = (x + w - bw - 10.0 * k, y + 6.0 * k, h - 12.0 * k);
        let hot = enabled && self.is_hover(Kind::Dropdown(id));
        let bg = if hot { p.hover } else { p.card };
        cv.fill_rrect(bx, by, bw, bh, 10.0 * k, bg, bg);
        cv.stroke_rrect(bx, by, bw, bh, 10.0 * k, 1.0_f32.max(k * 0.9), p.border);
        let s = self.fit(value, 13.5, false, bw - 44.0 * k);
        let vh = self.th(13.5, false);
        self.t(cv, &s, 13.5, false, bx + 12.0 * k, by + (bh - vh) / 2.0, p.text.mul_a(a));
        let (cx, cy) = (bx + bw - 18.0 * k, by + bh / 2.0);
        let th = 1.8 * k;
        cv.line(cx - 4.5 * k, cy - 2.0 * k, cx, cy + 2.5 * k, th, p.dim.mul_a(a));
        cv.line(cx, cy + 2.5 * k, cx + 4.5 * k, cy - 2.0 * k, th, p.dim.mul_a(a));
        if enabled {
            self.hit(bx, by, bw, bh, Kind::Dropdown(id));
        }
        h
    }

    fn fields(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, enabled: bool, lat_v: f64, lon_v: f64) -> f32 {
        let k = self.k;
        let h = 48.0 * k;
        let lh = self.th(14.5, false);
        let a = if enabled { 1.0 } else { 0.4 };
        self.t(cv, "Koordinat", 14.5, false, x + 12.0 * k, y + (h - lh) / 2.0, p.text.mul_a(a));
        let fw = ((w * 0.56).min(360.0 * k) - 10.0 * k) / 2.0;
        let (by, bh) = (y + 6.0 * k, h - 12.0 * k);
        let bx1 = x + w - 2.0 * fw - 10.0 * k - 10.0 * k;
        for (i, (id, name, val)) in [(LAT, "Lintang", lat_v), (LON, "Bujur", lon_v)].into_iter().enumerate() {
            let bx = bx1 + i as f32 * (fw + 10.0 * k);
            let focused = self.focus == Some(id) && enabled;
            let bg = if focused { p.card.mul_a(1.0) } else { p.card };
            cv.fill_rrect(bx, by, fw, bh, 10.0 * k, bg, bg);
            cv.stroke_rrect(bx, by, fw, bh, 10.0 * k, if focused { 1.6 * k } else { 1.0_f32.max(k * 0.9) }, if focused { p.accent } else { p.border });
            self.t(cv, name, 10.5, true, bx + 10.0 * k, by + 3.0 * k, p.dim.mul_a(a));
            let shown = if focused { self.buf.clone() } else { format!("{:.4}", val) };
            let vh = self.th(13.5, false);
            let ty = by + bh - vh - 3.0 * k;
            self.t(cv, &shown, 13.5, false, bx + 10.0 * k, ty, p.text.mul_a(a));
            if focused && self.blink {
                let pre: String = shown.chars().take(self.caret).collect();
                let cxp = bx + 10.0 * k + self.tw(&pre, 13.5, false);
                cv.fill_rrect(cxp + 0.5, ty + 1.0 * k, (1.4 * k).max(1.0), vh - 2.0 * k, 0.5, p.accent, p.accent);
            }
            if enabled {
                self.hit(bx, by, fw, bh, Kind::Field(id));
            }
        }
        h
    }

    fn buttons(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, list: &[(u32, &str, bool)], status: Option<String>) -> f32 {
        let k = self.k;
        let h = 52.0 * k;
        let bh = 36.0 * k;
        let by = y + (h - bh) / 2.0;
        let mut bx = x + 10.0 * k;
        for (id, label, primary) in list {
            let tw_ = self.tw(label, 13.5, true);
            let bw = tw_ + 36.0 * k;
            let hot = self.is_hover(Kind::Button(*id));
            let (bg, fg) = if *primary {
                (if hot { p.accent.lerp(Rgba(255, 255, 255, 255), 0.18) } else { p.accent }, p.on_accent)
            } else {
                (if hot { p.hover.mul_a(2.0) } else { p.card.mul_a(1.6) }, p.text)
            };
            cv.fill_rrect(bx, by, bw, bh, bh / 2.0, bg, bg);
            if !*primary {
                cv.stroke_rrect(bx, by, bw, bh, bh / 2.0, 1.0_f32.max(k * 0.9), p.border);
            }
            let th_ = self.th(13.5, true);
            self.t(cv, label, 13.5, true, bx + 18.0 * k, by + (bh - th_) / 2.0, fg);
            self.hit(bx, by, bw, bh, Kind::Button(*id));
            bx += bw + 12.0 * k;
        }
        if let Some(st) = status {
            let s = self.fit(&st, 12.0, false, x + w - bx - 8.0 * k);
            let sh = self.th(12.0, false);
            self.t(cv, &s, 12.0, false, bx + 2.0 * k, y + (h - sh) / 2.0, p.dim);
        }
        h
    }

    fn note(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, t: &str) -> f32 {
        let k = self.k;
        let s = self.fit(t, 11.5, false, w - 24.0 * k);
        self.t(cv, &s, 11.5, false, x + 12.0 * k, y + 6.0 * k, p.dim);
        26.0 * k
    }

    /// Baris pilihan warna teks dock: Otomatis + beberapa warna.
    fn swatches(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32) -> f32 {
        let k = self.k;
        let n = SWATCHES.len() as f32;
        let cell = (w - 12.0 * k) / n;
        let r = (cell * 0.36).min(13.0 * k);
        let cy = y + 24.0 * k;
        for (i, c) in SWATCHES.iter().enumerate() {
            let cx = x + 6.0 * k + cell * (i as f32 + 0.5);
            if *c < 0 {
                cv.fill_circle(cx, cy, r, Rgba(250, 250, 250, 255));
                cv.fill_pie(cx, cy, r, 270.0, 450.0, Rgba(40, 40, 44, 255));
            } else {
                let col = Rgba(((*c >> 16) & 255) as u8, ((*c >> 8) & 255) as u8, (*c & 255) as u8, 255);
                cv.fill_circle(cx, cy, r, col);
            }
            cv.stroke_rrect(cx - r, cy - r, 2.0 * r, 2.0 * r, r, 1.0_f32.max(k * 0.9), p.text.mul_a(0.25));
            let sel = self.cfg.text_color == *c;
            if sel {
                cv.stroke_rrect(cx - r - 3.0 * k, cy - r - 3.0 * k, 2.0 * (r + 3.0 * k), 2.0 * (r + 3.0 * k), r + 3.0 * k, 2.0 * k, p.accent);
            } else if self.is_hover(Kind::Swatch(*c)) {
                cv.stroke_rrect(cx - r - 3.0 * k, cy - r - 3.0 * k, 2.0 * (r + 3.0 * k), 2.0 * (r + 3.0 * k), r + 3.0 * k, 1.5 * k, p.text.mul_a(0.4));
            }
            self.hit(cx - cell / 2.0, y, cell, 48.0 * k, Kind::Swatch(*c));
        }
        let lab = if self.cfg.text_color < 0 { "Otomatis (mengikuti tema)".to_string() } else { format!("Warna #{:06X}", self.cfg.text_color) };
        self.t(cv, &lab, 11.5, false, x + 8.0 * k, y + 46.0 * k, p.dim);
        70.0 * k
    }

    /// Satu baris modul dock: nama, tombol naik/turun, dan saklar tampil.
    fn mod_row(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32, i: usize, key: &str, n: usize) -> f32 {
        let k = self.k;
        let h = 37.0 * k;
        let name = match key {
            "prayer" => "Waktu salat (ikon, nama, jam, hitung mundur)",
            "wifi" => "Wi-Fi / jaringan",
            "battery" => "Baterai (ikon dan persentase)",
            "clock" => "Hari, tanggal, dan jam",
            "tray" => "Tombol ikon tray",
            "bin" => "Recycle Bin",
            "explorer" => "File Explorer",
            "power" => "Tombol daya",
            "control" => "Pusat kontrol",
            "music" => "Pemutar musik",
            _ => "Cuaca",
        };
        let on = mod_enabled(&self.cfg, key);
        if self.is_hover(Kind::Mod(i, 0)) {
            cv.fill_rrect(x, y + 1.0 * k, w, h - 2.0 * k, 10.0 * k, p.hover, p.hover);
        }
        let lh = self.th(14.0, false);
        self.t(cv, name, 14.0, false, x + 12.0 * k, y + (h - lh) / 2.0, if on { p.text } else { p.dim });
        // saklar kecil
        let (sw, sh) = (36.0 * k, 20.0 * k);
        let (sx, sy) = (x + w - sw - 10.0 * k, y + (h - sh) / 2.0);
        let col = if on { p.accent } else { p.track };
        cv.fill_rrect(sx, sy, sw, sh, sh / 2.0, col, col);
        let kr = sh / 2.0 - 2.5 * k;
        let kx = sx + sh / 2.0 + (sw - sh) * if on { 1.0 } else { 0.0 };
        cv.fill_circle(kx, sy + sh / 2.0, kr, Rgba(255, 255, 255, 255));
        self.hit(x, y, w, h, Kind::Mod(i, 0));
        // naik / turun
        for (op, cx) in [(1u8, x + w - sw - 70.0 * k), (2u8, x + w - sw - 36.0 * k)] {
            let can = if op == 1 { i > 0 } else { i + 1 < n };
            let cy = y + h / 2.0;
            let hot = can && self.is_hover(Kind::Mod(i, op));
            if hot {
                cv.fill_rrect(cx - 13.0 * k, cy - 13.0 * k, 26.0 * k, 26.0 * k, 8.0 * k, p.hover.mul_a(2.0), p.hover.mul_a(2.0));
            }
            let c = if can { p.text } else { p.dim.mul_a(0.35) };
            let g = 13.0 * k;
            let th = (1.8 * k).max(1.4);
            if op == 1 {
                cv.line(cx - 0.38 * g, cy + 0.16 * g, cx, cy - 0.22 * g, th, c);
                cv.line(cx, cy - 0.22 * g, cx + 0.38 * g, cy + 0.16 * g, th, c);
            } else {
                cv.line(cx - 0.38 * g, cy - 0.16 * g, cx, cy + 0.22 * g, th, c);
                cv.line(cx, cy + 0.22 * g, cx + 0.38 * g, cy - 0.16 * g, th, c);
            }
            if can {
                self.hit(cx - 14.0 * k, y, 28.0 * k, h, Kind::Mod(i, op));
            }
        }
        h
    }

    fn chips(&mut self, cv: &mut Canvas, p: &Pal, x: f32, y: f32, w: f32) -> f32 {
        let k = self.k;
        let gap = 10.0 * k;
        let n = Style::ALL.len();
        let cw = (w - gap * (n as f32 - 1.0)) / n as f32;
        let ch = 88.0 * k;
        for (i, (st, _key, name)) in Style::ALL.iter().enumerate() {
            let cx = x + i as f32 * (cw + gap);
            let sel = self.cfg.style == *st;
            let hot = self.is_hover(Kind::Chip(i));
            // wallpaper mini
            let (wx, wy, ww, wh) = (cx + 6.0 * k, y + 6.0 * k, cw - 12.0 * k, 50.0 * k);
            cv.fill_rrect(wx, wy, ww, wh, 10.0 * k, Rgba(54, 84, 170, 255), Rgba(150, 92, 176, 255));
            let white = Rgba(255, 255, 255, 255);
            let (fill_a, fill_b, brd) = match st {
                Style::Mac => (white.a(0.40), white.a(0.24), white.a(0.55)),
                Style::Dark => (Rgba(40, 40, 46, 225), Rgba(26, 26, 30, 232), white.a(0.14)),
                Style::Surface => {
                    if self.light_sys { (Rgba(243, 243, 243, 238), Rgba(236, 236, 236, 240), Rgba(0, 0, 0, 24)) } else { (Rgba(36, 36, 36, 232), Rgba(30, 30, 30, 238), white.a(0.12)) }
                }
                Style::Light => (Rgba(252, 252, 254, 232), Rgba(240, 241, 246, 236), Rgba(0, 0, 0, 24)),
                Style::Clear => (white.a(0.12), white.a(0.06), white.a(0.40)),
            };
            let (dx, dy, dw, dh) = (wx + ww * 0.10, wy + wh - 22.0 * k, ww * 0.80, 15.0 * k);
            cv.fill_rrect(dx, dy, dw, dh, dh / 2.0, fill_a, fill_b);
            cv.stroke_rrect(dx, dy, dw, dh, dh / 2.0, 1.0, brd);
            for j in 0..4 {
                let c = [Rgba(255, 99, 71, 255), Rgba(80, 170, 255, 255), Rgba(120, 220, 120, 255), Rgba(255, 200, 60, 255)][j];
                cv.fill_circle(dx + dw * (0.22 + 0.19 * j as f32), dy + dh / 2.0, 3.4 * k, c);
            }
            if sel {
                cv.stroke_rrect(cx, y, cw, ch, 14.0 * k, 2.2 * k, p.accent);
            } else if hot {
                cv.stroke_rrect(cx, y, cw, ch, 14.0 * k, 1.4 * k, p.dim);
            }
            let short = name.split(" (").next().unwrap_or(name);
            let lw = self.tw(short, 12.0, sel);
            self.t(cv, short, 12.0, sel, cx + (cw - lw) / 2.0, y + 62.0 * k, if sel { p.accent } else { p.text });
            self.hit(cx, y, cw, ch, Kind::Chip(i));
        }
        ch + 10.0 * k
    }

    // ---------------------------------------------------------------- gambar utama

    fn draw(&mut self, cv: &mut Canvas) {
        let k = self.k;
        let p = self.pal();
        let (m, ww, wh) = (M * k, W * k, H * k);
        let r = 22.0 * k;
        // latar statis (bayangan + jendela + sidebar) digambar sekali, selanjutnya disalin
        let mut key: u64 = 0xcbf29ce484222325;
        for v in [cv.w as u32, cv.h as u32, k.to_bits(), p.shadow as u32] {
            key = (key ^ v as u64).wrapping_mul(0x100000001b3);
        }
        for c in [p.win_a, p.win_b, p.border, p.card] {
            key = (key ^ u32::from_le_bytes([c.0, c.1, c.2, c.3]) as u64).wrapping_mul(0x100000001b3);
        }
        let (sx, sy, sw, sh) = (m + 12.0 * k, m + 12.0 * k, SIDEBAR * k, wh - 24.0 * k);
        if self.bg_key == key && self.bg_cache.len() == cv.px.len() {
            cv.px.copy_from_slice(&self.bg_cache);
        } else {
            for i in (1..=6).rev() {
                let e = i as f32 * 4.0 * k;
                let a = Rgba(0, 0, 0, (p.shadow as f32 * 0.5) as u8);
                cv.fill_rrect(m - e, m - e + 8.0 * k, ww + 2.0 * e, wh + 2.0 * e, r + e, a, a);
            }
            cv.fill_rrect(m, m, ww, wh, r, p.win_a, p.win_b);
            cv.stroke_rrect(m, m, ww, wh, r, 1.0_f32.max(k * 0.9), p.border);
            cv.fill_rrect(m + r * 0.6, m + 1.0, ww - r * 1.2, 1.0, 0.5, Rgba(255, 255, 255, 36), Rgba(255, 255, 255, 0));
            cv.fill_rrect(sx, sy, sw, sh, 16.0 * k, p.card, p.card);
            self.bg_cache.clear();
            self.bg_cache.extend_from_slice(cv.px);
            self.bg_key = key;
        }
        // logo
        let (lx, ly, ls) = (sx + 20.0 * k, sy + 22.0 * k, 26.0 * k);
        let g = ls * 0.07;
        let t = (ls - g) / 2.0;
        let cols = [Rgba(120, 214, 255, 255), Rgba(70, 176, 255, 255), Rgba(64, 168, 252, 255), Rgba(22, 128, 236, 255)];
        for i in 0..4 {
            cv.fill_rrect(lx + (i % 2) as f32 * (t + g), ly + (i / 2) as f32 * (t + g), t, t, t * 0.14, cols[i], cols[i]);
        }
        self.t(cv, "LiteDock", 17.0, true, lx + ls + 10.0 * k, ly + 1.0 * k, p.text);
        // tab
        for (i, name) in TABS.iter().enumerate() {
            let ty = sy + 84.0 * k + i as f32 * 46.0 * k;
            let (tx, tw_, th_) = (sx + 10.0 * k, sw - 20.0 * k, 40.0 * k);
            let sel = self.tab == i;
            if sel {
                cv.fill_rrect(tx, ty, tw_, th_, 12.0 * k, p.accent.a(0.22), p.accent.a(0.22));
                cv.fill_rrect(tx + 3.0 * k, ty + 10.0 * k, 3.5 * k, th_ - 20.0 * k, 1.75 * k, p.accent, p.accent);
            } else if self.is_hover(Kind::Tab(i)) {
                cv.fill_rrect(tx, ty, tw_, th_, 12.0 * k, p.hover, p.hover);
            }
            let lh = self.th(14.5, sel);
            self.t(cv, name, 14.5, sel, tx + 20.0 * k, ty + (th_ - lh) / 2.0, p.text);
            self.hit(tx, ty, tw_, th_, Kind::Tab(i));
        }
        let ver = format!("v{}", env!("CARGO_PKG_VERSION"));
        self.t(cv, &ver, 11.5, false, sx + 20.0 * k, sy + sh - 28.0 * k, p.dim);

        // judul + tombol tutup
        let cx0 = sx + sw + 28.0 * k;
        let cw = m + ww - 28.0 * k - cx0;
        self.t(cv, TABS[self.tab], 26.0, true, cx0 + 2.0 * k, m + 24.0 * k, p.text);
        let (ccx, ccy, cr) = (m + ww - 38.0 * k, m + 38.0 * k, 14.0 * k);
        let ch = if self.is_hover(Kind::Close) { p.hover.mul_a(2.2) } else { p.card };
        cv.fill_circle(ccx, ccy, cr, ch);
        let d = 4.4 * k;
        cv.line(ccx - d, ccy - d, ccx + d, ccy + d, 1.8 * k, p.text);
        cv.line(ccx - d, ccy + d, ccx + d, ccy - d, 1.8 * k, p.text);
        self.hit(ccx - cr, ccy - cr, 2.0 * cr, 2.0 * cr, Kind::Close);

        // baris-baris
        let mut y = m + 86.0 * k;
        let c = self.cfg.clone();
        match self.tab {
            0 => {
                y += self.header(cv, &p, cx0, y, "Gaya latar dock");
                y += self.chips(cv, &p, cx0, y, cw);
                y += self.header(cv, &p, cx0, y + 6.0 * k, "Bentuk dan posisi") + 6.0 * k;
                y += self.slider(cv, &p, cx0, y, cw, OPACITY, "Kepekatan latar", 15, 100, "%");
                y += self.slider(cv, &p, cx0, y, cw, RADIUS, "Sudut membulat", 0, 36, "px");
                y += self.slider(cv, &p, cx0, y, cw, MARGIN, "Jarak dock dari bawah layar", 0, 48, "px");
                y += self.slider(cv, &p, cx0, y, cw, LABELGAP, "Jarak label nama aplikasi ke ikon", 0, 40, "px");
                self.note(cv, &p, cx0, y + 4.0 * k, cw, "Ukuran ikon aplikasi dan ikon tray diatur terpisah di tab Ukuran.");
            }
            1 => {
                y += self.header(cv, &p, cx0, y, "Zoom");
                y += self.toggle(cv, &p, cx0, y, cw, ZOOM_ON, "Zoom ikon saat kursor di atas dock", "Ikon membesar ala macOS dan keluar dari latar dock");
                y += self.slider(cv, &p, cx0, y, cw, ZOOM, "Besar zoom", 100, 250, "%");
                y += self.header(cv, &p, cx0, y + 6.0 * k, "Animasi") + 6.0 * k;
                y += self.toggle(cv, &p, cx0, y, cw, ANIM, "Animasi halus", "Zoom, genie minimize/maximize, pantulan, notifikasi");
                y += self.slider(cv, &p, cx0, y, cw, SPEED, "Kecepatan animasi", 40, 250, "%");
                self.toggle(cv, &p, cx0, y, cw, INDICATOR, "Titik indikator aplikasi berjalan", "");
            }
            2 => {
                y += self.header(cv, &p, cx0, y, "Elemen dock");
                y += self.toggle(cv, &p, cx0, y, cw, START, "Tombol Start", "");
                y += self.dropdown(cv, &p, cx0, y, cw, STARTBG, "Warna ubin ikon Windows (Start)", START_BGS[c.start_bg.clamp(0, 1) as usize], c.show_start);
                y += self.dropdown(cv, &p, cx0, y, cw, PSTYLE, "Isi modul salat", PSTYLES[c.prayer_style.min(2)], true);
                y += self.toggle(cv, &p, cx0, y, cw, PPILL, "Latar kapsul pada modul salat", "Mati: hanya ikon dua warna dan teks");
                y += self.toggle(cv, &p, cx0, y, cw, H24, "Format jam 24 jam", "");
                y += self.toggle(cv, &p, cx0, y, cw, WINICONS, "Ikon sistem gaya Windows", "Mati: ikon vektor ala macOS (Wi-Fi, baterai, dll.). Nyala: Segoe Fluent Icons");
                y += self.toggle(cv, &p, cx0, y, cw, TRAYHOST, "Ikon tray di panel LiteDock", "LiteDock menampung ikon tray sendiri (butuh taskbar asli disembunyikan)");
                y += self.note(cv, &p, cx0, y + 8.0 * k, cw, "Modul lain (Wi-Fi, baterai, jam, salat, cuaca, dll.) diatur di tab Modul.");
            }
            3 => {
                y += self.header(cv, &p, cx0, y, "Lokasi");
                y += self.toggle(cv, &p, cx0, y, cw, AUTOLOC, "Deteksi lokasi otomatis", "Lokasi Windows; cadangan lewat alamat IP bila tidak tersedia");
                let st = STATUS.lock().map(|g| g.clone()).unwrap_or_default();
                let st = if st.is_empty() {
                    if c.auto_ts > 0 { format!("Terakhir: {} ({:.3}, {:.3})", if c.auto_name.is_empty() { "-" } else { &c.auto_name }, c.auto_lat, c.auto_lon) } else { "Belum ada hasil deteksi".to_string() }
                } else {
                    st
                };
                y += self.buttons(cv, &p, cx0, y, cw, &[(DETECT, "Deteksi sekarang", false)], Some(st));
                let (city_txt, lat_v, lon_v) = if c.auto_loc && c.auto_ts > 0 {
                    (if c.auto_name.is_empty() { "Terdeteksi otomatis".to_string() } else { format!("{} (otomatis)", c.auto_name) }, c.auto_lat, c.auto_lon)
                } else {
                    (CITIES[c.city.min(CITIES.len() - 1)].0.to_string(), c.lat, c.lon)
                };
                y += self.dropdown(cv, &p, cx0, y, cw, CITY, "Kota", &city_txt, !c.auto_loc);
                y += self.fields(cv, &p, cx0, y, cw, !c.auto_loc, lat_v, lon_v);
                y += self.header(cv, &p, cx0, y + 4.0 * k, "Perhitungan") + 4.0 * k;
                y += self.dropdown(cv, &p, cx0, y, cw, METHOD, "Metode", METHODS[c.method.min(METHODS.len() - 1)].name, true);
                y += self.toggle(cv, &p, cx0, y, cw, HANAFI, "Ashar mazhab Hanafi", "Bayangan 2x tinggi benda (default Syafi'i)");
                y += self.header(cv, &p, cx0, y + 4.0 * k, "Notifikasi") + 4.0 * k;
                y += self.toggle(cv, &p, cx0, y, cw, NOTIFY, "Pop-up saat masuk waktu salat", "Muncul dari dock tepat pada waktunya");
                y += self.toggle(cv, &p, cx0, y, cw, NSOUND, "Bunyi notifikasi", "");
                self.buttons(cv, &p, cx0, y, cw, &[(TEST, "Tes notifikasi", false)], None);
            }
            5 => {
                y += self.header(cv, &p, cx0, y, "Font");
                y += self.dropdown(cv, &p, cx0, y, cw, FONT, "Jenis font", FONTS[c.font.clamp(0, 2) as usize], true);
                y += self.dropdown(cv, &p, cx0, y, cw, FWEIGHT, "Ketebalan teks dock", WEIGHTS[c.font_bold as usize], true);
                y += self.header(cv, &p, cx0, y + 6.0 * k, "Ukuran") + 6.0 * k;
                y += self.slider(cv, &p, cx0, y, cw, TEXTSCALE, "Ukuran teks dock (jam, tanggal, persen, salat)", 60, 160, "%");
                y += self.header(cv, &p, cx0, y + 4.0 * k, "Warna teks dock") + 4.0 * k;
                y += self.swatches(cv, &p, cx0, y, cw);
                y += self.header(cv, &p, cx0, y + 4.0 * k, "Jarak dan padding") + 4.0 * k;
                y += self.slider(cv, &p, cx0, y, cw, SYSGAP, "Jarak antar Wi-Fi, baterai, tanggal, jam, dll.", 0, 40, " px");
                self.slider(cv, &p, cx0, y, cw, PADX, "Padding kiri dan kanan dock", 0, 60, " px");
            }
            6 => {
                y += self.header(cv, &p, cx0, y, "Modul dock: tampil dan urutan");
                let keys: Vec<String> = c.order.split(',').map(|x| x.to_string()).collect();
                let n = keys.len();
                for (i, key) in keys.iter().enumerate() {
                    y += self.mod_row(cv, &p, cx0, y, cw, i, key, n);
                }
                self.note(cv, &p, cx0, y + 2.0 * k, cw, "Modul salat selalu di sisi kiri, modul lain di sisi kanan dock.");
            }
            7 => {
                y += self.header(cv, &p, cx0, y, "Banner pop-up");
                y += self.toggle(cv, &p, cx0, y, cw, BNCAPS, "Caps Lock dan Num Lock", "Banner saat tombol dihidupkan atau dimatikan");
                y += self.toggle(cv, &p, cx0, y, cw, BNCLIP, "Clipboard", "Banner saat teks, gambar, atau berkas disalin");
                y += self.toggle(cv, &p, cx0, y, cw, BNAPPS, "Notifikasi aplikasi (WhatsApp, dll.)", "Membaca banner Windows dan menampilkannya di dock");
                y += self.toggle(cv, &p, cx0, y, cw, BNSYS, "Baterai dan jaringan", "Pengisi daya dipasang/dilepas, baterai lemah, Wi-Fi tersambung/putus");
                y += self.toggle(cv, &p, cx0, y, cw, BNHIDE, "Sembunyikan banner bawaan Windows", "Percobaan: banner asli disembunyikan setelah terbaca");
                y += self.buttons(cv, &p, cx0, y + 8.0 * k, cw, &[(TESTBANNER, "Tampilkan contoh banner", true)], None) + 8.0 * k;
                self.note(cv, &p, cx0, y, cw, "Banner salat diatur di tab Salat.");
            }
            8 => {
                y += self.header(cv, &p, cx0, y, "Ikon aplikasi (Apps)");
                y += self.slider(cv, &p, cx0, y, cw, SIZE_, "Ukuran ikon aplikasi di dock", 28, 96, "px");
                y += self.header(cv, &p, cx0, y + 8.0 * k, "Ikon tray dan modul sistem") + 6.0 * k;
                y += self.slider(cv, &p, cx0, y, cw, TRAYSZ, "Ukuran ikon tray: Wi-Fi, baterai, cuaca, salat, tombol tray", 24, 120, "px");
                y += self.header(cv, &p, cx0, y + 8.0 * k, "Baterai") + 6.0 * k;
                y += self.toggle(cv, &p, cx0, y, cw, BATT, "Tampilkan ikon baterai", "Glif baterai Windows 11; hijau saat mengisi, merah saat lemah");
                y += self.toggle(cv, &p, cx0, y, cw, BATPCT, "Tampilkan persentase baterai", "Bisa dinyalakan tanpa ikonnya, atau sebaliknya");
                self.note(cv, &p, cx0, y + 6.0 * k, cw, "Ukuran ikon aplikasi dan ikon tray berdiri sendiri: mengubah satu tidak mengubah yang lain.");
            }
            _ => {
                y += self.header(cv, &p, cx0, y, "Taskbar Windows");
                y += self.toggle(cv, &p, cx0, y, cw, HIDETB, "Sembunyikan taskbar asli Windows", "Dikembalikan otomatis saat LiteDock ditutup");
                y += self.toggle(cv, &p, cx0, y, cw, RESERVE, "Sisakan ruang untuk dock", "Jendela maksimal tidak menutupi dock");
                y += self.dropdown(cv, &p, cx0, y, cw, HIDEMODE, "Sembunyikan dock otomatis", HIDEMODES[c.hide_mode.min(3)], true);
                y += self.slider(cv, &p, cx0, y, cw, FSREVEAL, "Lama dock tampil di atas aplikasi layar penuh", 5, 15, " dtk");
                y += self.header(cv, &p, cx0, y + 6.0 * k, "Umum") + 6.0 * k;
                y += self.toggle(cv, &p, cx0, y, cw, AUTOSTART, "Jalankan otomatis saat Windows mulai", "");
                y += self.header(cv, &p, cx0, y + 6.0 * k, "Pemulihan") + 6.0 * k;
                y += self.buttons(cv, &p, cx0, y, cw, &[(RESTORE, "Pulihkan taskbar dan keluar", true)], None);
                y += self.note(cv, &p, cx0, y, cw, "Taskbar tidak muncul? Jalankan \"LiteDock.exe --restore\".");
                self.note(cv, &p, cx0, y, cw, "Layar penuh tidak terdeteksi? Jalankan \"LiteDock.exe --diag\" lalu kirim hasilnya.");
            }
        }

        // daftar dropdown (di atas segalanya)
        if let Some(dd) = self.dd.as_ref().map(|d| (d.id, d.x, d.y, d.w, d.items.clone(), d.sel, d.scroll)) {
            let (_, bx, by, bw, items, sel, scroll) = dd;
            let rows = items.len().min(7);
            let rh = 34.0 * k;
            let lh = rows as f32 * rh + 12.0 * k;
            let mut ly = by + 6.0 * k;
            if ly + lh > m + wh - 10.0 * k {
                ly = (by - lh - 50.0 * k).max(m + 10.0 * k);
            }
            let bgc = Rgba(p.win_b.0, p.win_b.1, p.win_b.2, 255);
            for i in (1..=4).rev() {
                let e = i as f32 * 3.0 * k;
                cv.fill_rrect(bx - e, ly - e + 6.0 * k, bw + 2.0 * e, lh + 2.0 * e, 14.0 * k + e, Rgba(0, 0, 0, 22), Rgba(0, 0, 0, 22));
            }
            cv.fill_rrect(bx, ly, bw, lh, 14.0 * k, bgc.lerp(Rgba(255, 255, 255, 255), if p.text.0 > 128 { 0.07 } else { 0.0 }), bgc);
            cv.stroke_rrect(bx, ly, bw, lh, 14.0 * k, 1.0_f32.max(k * 0.9), p.border);
            self.hit(bx, ly, bw, lh, Kind::Block);
            for row in 0..rows {
                let idx = scroll + row;
                if idx >= items.len() {
                    break;
                }
                let ry = ly + 6.0 * k + row as f32 * rh;
                let hot = self.is_hover(Kind::DdItem(idx));
                if idx == sel {
                    cv.fill_rrect(bx + 6.0 * k, ry, bw - 12.0 * k, rh - 2.0 * k, 9.0 * k, p.accent.a(0.22), p.accent.a(0.22));
                } else if hot {
                    cv.fill_rrect(bx + 6.0 * k, ry, bw - 12.0 * k, rh - 2.0 * k, 9.0 * k, p.hover, p.hover);
                }
                let s = self.fit(&items[idx], 13.5, idx == sel, bw - 52.0 * k);
                let th_ = self.th(13.5, idx == sel);
                self.t(cv, &s, 13.5, idx == sel, bx + 16.0 * k, ry + (rh - 2.0 * k - th_) / 2.0, if idx == sel { p.accent } else { p.text });
                self.hit(bx + 6.0 * k, ry, bw - 12.0 * k, rh - 2.0 * k, Kind::DdItem(idx));
            }
            if items.len() > rows {
                let tr = lh - 16.0 * k;
                let th_ = tr * rows as f32 / items.len() as f32;
                let ty = ly + 8.0 * k + tr * scroll as f32 / items.len() as f32;
                cv.fill_rrect(bx + bw - 8.0 * k, ty, 3.0 * k, th_, 1.5 * k, p.dim, p.dim);
            }
        }
    }

    // ---------------------------------------------------------------- aksi

    fn apply(&mut self) {
        crate::apply_from_ui(self.cfg.clone());
        self.cfg = crate::current_config();
    }

    fn request_paint(&mut self) {
        if !self.paint_pending && !self.hwnd.0.is_null() {
            self.paint_pending = true;
            unsafe {
                SetTimer(Some(self.hwnd), T_PAINT, 8, None);
            }
        }
    }

    fn paint(&mut self) {
        self.paint_pending = false;
        let Some(mut surf) = self.surf.take() else { return };
        self.anim_target.clear();
        self.hits.clear();
        {
            let mut cv = surf.canvas();
            cv.clear();
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.draw(&mut cv)));
            if r.is_err() {
                dbg("panic di draw()".to_string());
            }
        }
        if !self.hwnd.0.is_null() {
            crate::genie::present_here(self.hwnd, &surf);
        }
        self.surf = Some(surf);
        // animasi saklar
        let mut busy = false;
        let targets: Vec<(u32, f32)> = self.anim_target.iter().map(|(a, b)| (*a, *b)).collect();
        for (id, t) in targets {
            let cur = self.anim.entry(id).or_insert(t);
            // dock sedang beranimasi: saklar langsung ke tujuan supaya repaint jendela ini tidak berebut waktu dengan animasi dock
            if crate::DOCK_ANIM.load(std::sync::atomic::Ordering::Relaxed) {
                *cur = t;
            } else if (*cur - t).abs() > 0.01 {
                *cur += (t - *cur) * 0.32;
                busy = true;
            } else {
                *cur = t;
            }
        }
        if !self.hwnd.0.is_null() {
            unsafe {
                if busy {
                    SetTimer(Some(self.hwnd), T_ANIM, 16, None);
                } else {
                    let _ = KillTimer(Some(self.hwnd), T_ANIM);
                }
            }
        }
    }

    fn commit_field(&mut self) {
        if let Some(id) = self.focus.take() {
            unsafe {
                let _ = KillTimer(Some(self.hwnd), T_BLINK);
            }
            if let Ok(v) = self.buf.trim().replace(',', ".").parse::<f64>() {
                if id == LAT {
                    self.cfg.lat = v;
                } else {
                    self.cfg.lon = v;
                }
                self.cfg.city = CITIES.len() - 1;
                self.apply();
            }
        }
    }

    fn open_dd(&mut self, id: u32, bx: f32, by: f32, bw: f32) {
        let k = self.k;
        let (items, sel): (Vec<String>, usize) = match id {
            CITY => (CITIES.iter().map(|c| c.0.to_string()).collect(), self.cfg.city),
            PSTYLE => (PSTYLES.iter().map(|m| m.to_string()).collect(), self.cfg.prayer_style),
            HIDEMODE => (HIDEMODES.iter().map(|m| m.to_string()).collect(), self.cfg.hide_mode),
            FONT => (FONTS.iter().map(|m| m.to_string()).collect(), self.cfg.font.clamp(0, 2) as usize),
            STARTBG => (START_BGS.iter().map(|m| m.to_string()).collect(), self.cfg.start_bg.clamp(0, 1) as usize),
            FWEIGHT => (WEIGHTS.iter().map(|m| m.to_string()).collect(), self.cfg.font_bold as usize),
            _ => (METHODS.iter().map(|m| m.name.to_string()).collect(), self.cfg.method),
        };
        let max_scroll = items.len().saturating_sub(7);
        let scroll = sel.saturating_sub(3).min(max_scroll);
        self.dd = Some(Dd { id, x: bx, y: by + 42.0 * k - 36.0 * k + 36.0 * k, w: bw, items, sel, scroll });
    }

    fn click(&mut self, x: f32, y: f32) {
        let idx = self.hits.iter().rposition(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h);
        let kind = idx.map(|i| self.hits[i].kind);
        let rect = idx.map(|i| (self.hits[i].x, self.hits[i].y, self.hits[i].w, self.hits[i].h));
        if self.dd.is_some() {
            if let Some(Kind::DdItem(i)) = kind {
                let id = self.dd.as_ref().unwrap().id;
                if id == CITY {
                    self.cfg.city = i;
                    if i < CITIES.len() - 1 {
                        self.cfg.lat = CITIES[i].1;
                        self.cfg.lon = CITIES[i].2;
                    }
                } else if id == PSTYLE {
                    self.cfg.prayer_style = i;
                } else if id == HIDEMODE {
                    self.cfg.hide_mode = i;
                } else if id == FONT {
                    self.cfg.font = i as i32;
                } else if id == STARTBG {
                    self.cfg.start_bg = i as i32;
                } else if id == FWEIGHT {
                    self.cfg.font_bold = i == 1;
                } else {
                    self.cfg.method = i;
                }
                self.dd = None;
                self.apply();
            } else if kind != Some(Kind::Block) {
                self.dd = None;
            }
            self.request_paint();
            return;
        }
        if !matches!(kind, Some(Kind::Field(_))) {
            self.commit_field();
        }
        match kind {
            Some(Kind::Tab(i)) => self.tab = i,
            Some(Kind::Toggle(id)) => {
                let v = !get_bool(&self.cfg, id);
                set_bool(&mut self.cfg, id, v);
                self.apply();
            }
            Some(Kind::Slider(id, lo, hi)) => {
                let (hx, _, hw, _) = rect.unwrap();
                let k = self.k;
                let (tx, tw_) = (hx + 10.0 * k, hw - 20.0 * k);
                self.drag = Some((id, tx, tw_, lo, hi));
                self.drag_to(x);
                unsafe {
                    SetCapture(self.hwnd);
                }
            }
            Some(Kind::Chip(i)) => {
                self.cfg.style = Style::ALL[i].0;
                self.apply();
            }
            Some(Kind::Dropdown(id)) => {
                let (bx, by, bw, bh) = rect.unwrap();
                self.open_dd(id, bx, by + bh - self.k * 36.0, bw);
                // posisi daftar tepat di bawah kotak
                if let Some(d) = self.dd.as_mut() {
                    d.y = by + bh - 6.0 * self.k;
                }
            }
            Some(Kind::Field(id)) => {
                if self.focus != Some(id) {
                    self.commit_field();
                    self.focus = Some(id);
                    self.buf = format!("{:.4}", if id == LAT { self.cfg.lat } else { self.cfg.lon });
                    self.caret = self.buf.chars().count();
                    self.blink = true;
                    unsafe {
                        SetTimer(Some(self.hwnd), T_BLINK, 530, None);
                    }
                }
            }
            Some(Kind::Swatch(c)) => {
                self.cfg.text_color = c;
                self.apply();
            }
            Some(Kind::Mod(i, op)) => {
                let mut keys: Vec<String> = self.cfg.order.split(',').map(|x| x.to_string()).collect();
                match op {
                    0 => {
                        let key = keys.get(i).cloned().unwrap_or_default();
                        let v = !mod_enabled(&self.cfg, &key);
                        set_mod_enabled(&mut self.cfg, &key, v);
                    }
                    1 if i > 0 => keys.swap(i - 1, i),
                    2 if i + 1 < keys.len() => keys.swap(i, i + 1),
                    _ => {}
                }
                self.cfg.order = keys.join(",");
                self.apply();
            }
            Some(Kind::Button(id)) => match id {
                TESTBANNER => crate::test_banner(),
                DETECT => crate::detect_now(),
                TEST => crate::test_notification(),
                RESTORE => crate::quit_app(),
                _ => {}
            },
            Some(Kind::Close) => unsafe {
                let _ = DestroyWindow(self.hwnd);
                return;
            },
            _ => {}
        }
        self.request_paint();
    }

    fn drag_to(&mut self, x: f32) {
        if let Some((id, tx, tw_, lo, hi)) = self.drag {
            let f = ((x - tx) / tw_.max(1.0)).clamp(0.0, 1.0);
            let v = lo + (f * (hi - lo) as f32).round() as i32;
            if v != get_int(&self.cfg, id) {
                set_int(&mut self.cfg, id, v);
                self.apply();
            }
            self.request_paint();
        }
    }

    fn key_char(&mut self, ch: u32) {
        if self.focus.is_none() {
            return;
        }
        let mut chars: Vec<char> = self.buf.chars().collect();
        match ch {
            8 => {
                if self.caret > 0 {
                    chars.remove(self.caret - 1);
                    self.caret -= 1;
                }
            }
            13 => {
                self.commit_field();
                self.request_paint();
                return;
            }
            27 => {
                self.focus = None;
                unsafe {
                    let _ = KillTimer(Some(self.hwnd), T_BLINK);
                }
                self.request_paint();
                return;
            }
            c => {
                if let Some(c) = char::from_u32(c) {
                    let c = if c == ',' { '.' } else { c };
                    if (c.is_ascii_digit() || c == '-' || c == '.') && chars.len() < 11 {
                        chars.insert(self.caret, c);
                        self.caret += 1;
                    }
                }
            }
        }
        self.buf = chars.into_iter().collect();
        self.blink = true;
        self.request_paint();
    }

    fn key_down(&mut self, vk: u16) -> bool {
        if self.focus.is_some() {
            let n = self.buf.chars().count();
            match VIRTUAL_KEY(vk) {
                VK_LEFT => self.caret = self.caret.saturating_sub(1),
                VK_RIGHT => self.caret = (self.caret + 1).min(n),
                VK_HOME => self.caret = 0,
                VK_END => self.caret = n,
                VK_DELETE => {
                    let mut chars: Vec<char> = self.buf.chars().collect();
                    if self.caret < n {
                        chars.remove(self.caret);
                        self.buf = chars.into_iter().collect();
                    }
                }
                _ => return false,
            }
            self.blink = true;
            self.request_paint();
            return true;
        }
        false
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        if self.drag.is_some() {
            self.drag_to(x);
            return;
        }
        let new = self.hits.iter().rev().find(|h| x >= h.x && x < h.x + h.w && y >= h.y && y < h.y + h.h).map(|h| h.kind);
        let changed = new != self.hover;
        self.hover = new;
        if changed {
            self.request_paint();
        }
    }
}

/// Render uji tanpa jendela (untuk pemeriksaan visual): tab, dan opsional daftar dropdown terbuka.
pub fn selftest_render(cfg: &Config, tab: usize, k: f32, light_sys: bool, dd_open: bool) -> Option<Surface> {
    let mut u = Ui::new(cfg.clone(), k, light_sys);
    u.tab = tab;
    let mut surf = Surface::new(((W + 2.0 * M) * k) as i32, ((H + 2.0 * M) * k) as i32)?;
    {
        let mut cv = surf.canvas();
        u.hits.clear();
        u.draw(&mut cv);
        if dd_open {
            let hit = u.hits.iter().find(|h| h.kind == Kind::Dropdown(CITY)).map(|h| (h.x, h.y, h.w, h.h));
            if let Some((bx, by, bw, bh)) = hit {
                u.open_dd(CITY, bx, by + bh, bw);
                if let Some(d) = u.dd.as_mut() {
                    d.y = by + bh - 6.0 * k;
                }
                cv.clear();
                u.hits.clear();
                u.draw(&mut cv);
            }
        }
    }
    Some(surf)
}

/// Ukur biaya gambar (ms): pertama (cache dingin) dan rata-rata gambar berikutnya (cache hangat).
pub fn selftest_time(cfg: &Config, tab: usize, k: f32) -> (f64, f64) {
    let mut u = Ui::new(cfg.clone(), k, false);
    u.tab = tab;
    let Some(mut surf) = Surface::new(((W + 2.0 * M) * k) as i32, ((H + 2.0 * M) * k) as i32) else { return (0.0, 0.0) };
    let mut cv = surf.canvas();
    let t0 = std::time::Instant::now();
    u.hits.clear();
    u.draw(&mut cv);
    let cold = t0.elapsed().as_secs_f64() * 1000.0;
    u.hits.clear();
    u.draw(&mut cv);
    let t1 = std::time::Instant::now();
    for _ in 0..20 {
        u.hits.clear();
        u.draw(&mut cv);
    }
    (cold, t1.elapsed().as_secs_f64() * 1000.0 / 20.0)
}

/// Buka pengaturan langsung di tab tertentu.
pub fn open_at(cfg: &Config, scale: f32, tab: usize) {
    open(cfg, scale);
    if let Some(u) = ui() {
        u.tab = tab.min(TABS.len() - 1);
        u.request_paint();
    }
}

pub fn open(cfg: &Config, scale: f32) {
    if let Some(h) = is_open() {
        unsafe {
            let _ = ShowWindow(h, SW_RESTORE);
            let _ = SetForegroundWindow(h);
        }
        return;
    }
    unsafe {
        let hi = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: hi.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("LiteDockSettings"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let k = scale;
        let (sw, sh) = (((W + 2.0 * M) * k) as i32, ((H + 2.0 * M) * k) as i32);
        let sx = GetSystemMetrics(SM_CXSCREEN);
        let sy = GetSystemMetrics(SM_CYSCREEN);
        let title = wide("Pengaturan LiteDock");
        let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_APPWINDOW, w!("LiteDockSettings"), PCWSTR(title.as_ptr()), WS_POPUP, ((sx - sw) / 2).max(0), ((sy - sh) / 2).max(0), sw, sh, None, None, Some(hi.into()), None);
        let Ok(hwnd) = hwnd else { return };
        let mut u = Box::new(Ui::new(cfg.clone(), k, crate::shell::system_uses_light_theme()));
        u.hwnd = hwnd;
        u.surf = Surface::new(sw, sh);
        UI = Box::into_raw(u);
        OPEN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        let _ = ShowWindow(hwnd, SW_SHOW);
        if let Some(u) = ui() {
            u.paint();
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| proc_inner(h, msg, wp, lp))) {
        Ok(r) => r,
        Err(_) => LRESULT(0),
    }
}

fn dbg(line: String) {
    if std::env::var_os("LITEDOCK_DEBUG").is_some() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::config::dir().join("debug.log")) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

unsafe fn proc_inner(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(u) = ui() else { return DefWindowProcW(h, msg, wp, lp) };
    let lx = (lp.0 & 0xFFFF) as i16 as f32;
    let ly = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
    match msg {
        WM_NCHITTEST => {
            // seret jendela lewat bilah judul (kecuali tombol tutup)
            let mut pt = POINT { x: (lp.0 & 0xFFFF) as i16 as i32, y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32 };
            let _ = windows::Win32::Graphics::Gdi::ScreenToClient(h, &mut pt);
            let (x, y) = (pt.x as f32, pt.y as f32);
            let k = u.k;
            let in_title = x >= M * k && x <= (M + W) * k && y >= M * k && y <= (M + 70.0) * k;
            let on_close = u.hits.iter().any(|ht| ht.kind == Kind::Close && x >= ht.x && x < ht.x + ht.w && y >= ht.y && y < ht.y + ht.h);
            let on_tab = u.hits.iter().any(|ht| matches!(ht.kind, Kind::Tab(_)) && x >= ht.x && x < ht.x + ht.w && y >= ht.y && y < ht.y + ht.h);
            if in_title && !on_close && !on_tab && u.dd.is_none() {
                return LRESULT(HTCAPTION as isize);
            }
            LRESULT(HTCLIENT as isize)
        }
        WM_MOUSEMOVE => {
            if !u.tracking {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                u.tracking = true;
            }
            u.mouse_move(lx, ly);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            u.tracking = false;
            if u.hover.is_some() {
                u.hover = None;
                u.request_paint();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            u.click(lx, ly);
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if u.drag.take().is_some() {
                let _ = ReleaseCapture();
                u.request_paint();
            }
            LRESULT(0)
        }
        WM_MOUSEWHEEL => {
            let delta = ((wp.0 >> 16) & 0xFFFF) as i16 as i32;
            if let Some(d) = u.dd.as_mut() {
                let max_scroll = d.items.len().saturating_sub(7);
                if delta > 0 {
                    d.scroll = d.scroll.saturating_sub(1);
                } else {
                    d.scroll = (d.scroll + 1).min(max_scroll);
                }
                u.request_paint();
            }
            LRESULT(0)
        }
        WM_CHAR => {
            u.key_char(wp.0 as u32);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wp.0 as u16 == VK_ESCAPE.0 && u.focus.is_none() {
                if u.dd.is_some() {
                    u.dd = None;
                    u.request_paint();
                } else {
                    let _ = DestroyWindow(h);
                }
                return LRESULT(0);
            }
            u.key_down(wp.0 as u16);
            LRESULT(0)
        }
        WM_TIMER => {
            match wp.0 {
                T_PAINT => {
                    let _ = KillTimer(Some(h), T_PAINT);
                    u.paint();
                }
                T_ANIM => u.paint(),
                T_BLINK => {
                    u.blink = !u.blink;
                    u.request_paint();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_STATUS => {
            u.cfg = crate::current_config();
            u.request_paint();
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(h);
            LRESULT(0)
        }
        WM_DESTROY => {
            OPEN_HWND.store(0, Ordering::SeqCst);
            if !UI.is_null() {
                drop(Box::from_raw(UI));
                UI = std::ptr::null_mut();
            }
            crate::shell::trim_memory();
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}


/// Apakah modul dock `key` aktif menurut pengaturan.
pub(crate) fn mod_enabled(c: &Config, key: &str) -> bool {
    match key {
        "prayer" => c.show_prayer,
        "wifi" => c.show_wifi,
        "battery" => c.show_battery || c.show_battery_pct,
        "clock" => c.show_clock,
        "tray" => c.show_tray_btn,
        "bin" => c.show_bin,
        "explorer" => c.show_explorer,
        "power" => c.show_power,
        "control" => c.show_control,
        "music" => c.show_music,
        "weather" => c.show_weather,
        _ => false,
    }
}

pub(crate) fn set_mod_enabled(c: &mut Config, key: &str, v: bool) {
    match key {
        "prayer" => c.show_prayer = v,
        "wifi" => c.show_wifi = v,
        "battery" => {
            // saklar modul menyalakan/mematikan keduanya; ikon dan persen diatur sendiri-sendiri di tab Ukuran
            c.show_battery = v;
            c.show_battery_pct = v;
        }
        "clock" => c.show_clock = v,
        "tray" => c.show_tray_btn = v,
        "bin" => c.show_bin = v,
        "explorer" => c.show_explorer = v,
        "power" => c.show_power = v,
        "control" => c.show_control = v,
        "music" => c.show_music = v,
        "weather" => c.show_weather = v,
        _ => {}
    }
}
