//! Panel detail yang mengembang dari dock saat modul diklik: kalender, jadwal salat, Wi-Fi, baterai.
//! Digambar dengan renderer yang sama dan palet yang sama dengan jendela Pengaturan/dock.
use crate::config::*;
use crate::dock::PrayerInfo;
use crate::gfx::*;
use std::time::Duration;
use crate::icons;
use crate::macui;
use crate::prayer;
use crate::tray;
use crate::settings::{palette, Pal};
use crate::shell;
use crate::sysctl::{self, NetState};
use std::sync::atomic::{AtomicIsize, AtomicU8, Ordering};
use std::time::Instant;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FlyKind {
    Calendar,
    Prayer,
    Wifi,
    Battery,
    Tray,
    Power,
    Control,
    Music,
    Weather,
}

/// Panel dibuat lebih ringkas: semua ukuran panel dikalikan faktor ini.
const PANEL_SCALE: f32 = 0.86;
const MG: f32 = 26.0; // margin bayangan (logis)
const T_TICK: usize = 2;

static OPEN_HWND: AtomicIsize = AtomicIsize::new(0);
static OPEN_KIND: AtomicU8 = AtomicU8::new(255);
static mut FLY: *mut Fly = std::ptr::null_mut();

fn fly() -> Option<&'static mut Fly> {
    unsafe { if FLY.is_null() { None } else { Some(&mut *FLY) } }
}

pub fn current() -> Option<FlyKind> {
    match OPEN_KIND.load(Ordering::SeqCst) {
        0 => Some(FlyKind::Calendar),
        1 => Some(FlyKind::Prayer),
        2 => Some(FlyKind::Wifi),
        3 => Some(FlyKind::Battery),
        4 => Some(FlyKind::Tray),
        5 => Some(FlyKind::Power),
        6 => Some(FlyKind::Control),
        7 => Some(FlyKind::Music),
        8 => Some(FlyKind::Weather),
        _ => None,
    }
}

pub fn is_open() -> Option<HWND> {
    let h = OPEN_HWND.load(Ordering::SeqCst);
    if h == 0 { None } else { Some(HWND(h as *mut _)) }
}

/// Langkah animasi panel (dipanggil dari loop frame yang tersinkron vsync). true bila masih bergerak.
pub fn step() -> bool {
    match fly() {
        Some(f) => f.step(),
        None => false,
    }
}

/// Tutup dengan animasi (aman dipanggil kapan saja).
pub fn close() {
    if let Some(f) = fly() {
        f.begin_close();
    }
}

/// Tutup seketika tanpa animasi.
/// Data musik/cuaca berubah: segarkan panel bila sedang terbuka.
pub fn data_changed() {
    if let Some(f) = fly() {
        if matches!(f.kind, FlyKind::Music | FlyKind::Weather) {
            f.refresh_data();
            f.request_paint();
        }
    }
}

/// Daftar ikon tray berubah: gambar ulang panel tray bila sedang terbuka.
pub fn tray_refresh() {
    if let Some(f) = fly() {
        if f.kind == FlyKind::Tray {
            f.request_paint();
        }
    }
}

pub fn close_now() {
    if let Some(h) = is_open() {
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Hit {
    Prev,
    Next,
    Today,
    Day(i64),
    Settings,
    Btn(u8),
    TileWifi,
    TileAir,
    TileHot,
    Net(usize),
    Refresh,
    TileSaver,
    Mode(u8),
    TrayIcon(usize),
    PowerAct(u8),
    Media(u8),
    VolMute,
    VolBar,
    TileBt,
    WxRefresh,
}

const WM_NET: u32 = WM_APP + 20;
const WM_POWER: u32 = WM_APP + 21;

enum NetAction {
    /// daftar cepat dari cache Windows, lalu pindai ulang
    Scan,
    Wifi(bool),
    Airplane(bool),
    Hotspot(bool),
    Bt(bool),
    Connect(String),
    Disconnect,
}

struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hit: Hit,
}

struct Fly {
    hwnd: HWND,
    surf: Option<Surface>,
    text: TextCache,
    k: f32,
    kind: FlyKind,
    pal: Pal,
    style_cfg: Config,
    cw: f32,
    ch: f32,
    t0: Instant,
    closing: bool,
    hits: Vec<Rect>,
    hover: Option<Hit>,
    cal_y: i32,
    cal_m: i32,
    sel_day: Option<i64>,
    prayer: Option<PrayerInfo>,
    method: String,
    wifi: Option<shell::WifiInfo>,
    bat: Option<shell::BatInfo>,
    tick: u32,
    tracking: bool,
    paint_pending: bool,
    cache: Option<Image>,
    animating: bool,
    net: Option<NetState>,
    st: Option<shell::NetStatus>,
    tray_n: usize,
    net_busy: bool,
    busy_hit: Option<Hit>,
    toast: String,
    toast_until: Option<Instant>,
    mode: Option<u8>,
    saver_busy: bool,
    frames: u32,
    compose_ms: f32,
    threads: usize,
    vol: Option<(f32, bool)>,
    confirm: Option<u8>,
    music: Option<crate::media::MediaSnap>,
    wx: Option<crate::weather::Weather>,
    /// panel tetap terbuka sementara menu klik-kanan aplikasi lain tampil
    menu_mode: Option<Instant>,
    menu_seen: bool,
    drag_vol: bool,
}

const DAYS_FULL: [&str; 7] = ["Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu"];
const MONTHS_FULL: [&str; 12] = ["Januari", "Februari", "Maret", "April", "Mei", "Juni", "Juli", "Agustus", "September", "Oktober", "November", "Desember"];
const WD_SHORT: [&str; 7] = ["Sen", "Sel", "Rab", "Kam", "Jum", "Sab", "Min"];

fn days_in_month(y: i32, m: i32) -> i32 {
    let (ny, nm) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    (prayer::days_from_civil(ny, nm, 1) - prayer::days_from_civil(y, m, 1)) as i32
}

fn dow_sun0(days: i64) -> usize {
    (days + 4).rem_euclid(7) as usize
}

impl Fly {
    fn t(&mut self, cv: &mut Canvas, s: &str, px: f32, bold: bool, x: f32, y: f32, col: Rgba) {
        let p = ((px * self.k).round() as i32).max(10);
        let m = self.text.get_w(s, p, panel_weight(bold, p));
        cv.draw_mask(m, x.round() as i32, y.round() as i32, col);
    }
    fn tw(&mut self, s: &str, px: f32, bold: bool) -> f32 {
        let p = ((px * self.k).round() as i32).max(10);
        self.text.get_w(s, p, panel_weight(bold, p)).w as f32
    }
    fn th(&mut self, px: f32, bold: bool) -> f32 {
        let p = ((px * self.k).round() as i32).max(10);
        self.text.get_w("Ag", p, panel_weight(bold, p)).h as f32
    }
    fn hit(&mut self, x: f32, y: f32, w: f32, h: f32, hit: Hit) {
        self.hits.push(Rect { x, y, w, h, hit });
    }
    fn hov(&self, h: Hit) -> bool {
        self.hover == Some(h)
    }

    fn size_for(kind: FlyKind, tray_n: usize) -> (f32, f32) {
        match kind {
            FlyKind::Calendar => (336.0, 404.0),
            FlyKind::Prayer => (350.0, 492.0),
            FlyKind::Wifi => (368.0, 588.0),
            FlyKind::Battery => (340.0, 492.0),
            FlyKind::Power => (292.0, 316.0),
            FlyKind::Control => (336.0, 292.0),
            FlyKind::Music => (336.0, 176.0),
            FlyKind::Weather => (320.0, 334.0),
            FlyKind::Tray => {
                let rows = tray_n.div_ceil(6).clamp(1, 5) as f32;
                (312.0, 150.0 + rows * 44.0)
            }
        }
    }

    fn begin_close(&mut self) {
        if !self.closing {
            self.closing = true;
            self.t0 = Instant::now();
            self.build_cache();
            self.animating = true;
            crate::request_frame();
        }
    }

    fn refresh_data(&mut self) {
        match self.kind {
            FlyKind::Control => {
                self.vol = crate::volume::get();
                if self.net.is_none() && !self.net_busy {
                    self.spawn_net(NetAction::Scan, None);
                }
            }
            FlyKind::Music => self.music = crate::media::snapshot(),
            FlyKind::Weather => self.wx = crate::weather::snapshot(),
            FlyKind::Wifi => {
                self.wifi = Some(shell::wifi_details());
                self.st = Some(shell::net_status_cached());
            }
            FlyKind::Battery => {
                self.bat = shell::battery_details();
                self.mode = sysctl::power_mode_get();
            }
            _ => {}
        }
    }

    fn request_paint(&mut self) {
        if !self.paint_pending && !self.animating {
            self.paint_pending = true;
            unsafe {
                SetTimer(Some(self.hwnd), 3, 8, None);
            }
        }
    }

    /// kemajuan animasi 0..1 (membuka) / 1..0 (menutup); true bila animasi selesai
    fn progress(&self) -> (f32, bool) {
        let el = self.t0.elapsed().as_secs_f32();
        if self.closing {
            let t = (el / 0.20).min(1.0);
            (1.0 - t * t * (3.0 - 2.0 * t), t >= 1.0)
        } else {
            let t = (el / 0.34).min(1.0);
            // ease-out quint: cepat di awal, mendarat lembut
            (1.0 - (1.0 - t).powi(5), t >= 1.0)
        }
    }

    /// Gambar kartu penuh sekali ke bitmap (dipakai animasi: hanya transformasi per frame).
    fn build_cache(&mut self) {
        let Some(surf) = self.surf.as_ref() else { return };
        let (w, h) = (surf.w, surf.h);
        let mut img = Image::new(w, h);
        self.hits.clear();
        {
            let mut cv = Canvas { w, h, px: &mut img.px };
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.draw(&mut cv)));
        }
        self.cache = Some(img);
    }

    /// Satu langkah animasi (dipanggil dari loop frame yang tersinkron vsync). true bila masih bergerak.
    fn step(&mut self) -> bool {
        if !self.animating {
            return false;
        }
        let (e, done) = self.progress();
        let t0 = Instant::now();
        self.compose(e);
        self.frames += 1;
        self.compose_ms += t0.elapsed().as_secs_f32() * 1000.0;
        if done {
            dbgf(format!("flyout anim: {} frame, rata-rata compose {:.2} ms", self.frames, self.compose_ms / self.frames.max(1) as f32));
            self.animating = false;
            if self.closing {
                unsafe {
                    let _ = DestroyWindow(self.hwnd);
                }
                return false;
            }
            self.cache = None;
            self.paint();
            return false;
        }
        true
    }

    /// Komposisi frame animasi dari bitmap cache: skala dari tepi dock + geser + pudar.
    /// Titik-tetap + banyak thread (baris dibagi) agar tetap mulus di layar 4K/DPI tinggi.
    fn compose(&mut self, e: f32) {
        let Some(mut surf) = self.surf.take() else { return };
        if let Some(cache) = self.cache.as_ref() {
            let k = self.k;
            let (w, h) = (surf.w, surf.h);
            let sc = 0.86 + 0.14 * e;
            let (ax, ay) = (w as f32 / 2.0, (MG + self.ch) * k);
            let ty = (1.0 - e) * 10.0 * k;
            let a8 = ((e * 1.5).min(1.0) * 255.0) as u32;
            let y0 = ((ay - ay * sc + ty).floor().max(0.0)) as i32;
            let y1 = (((ay + (h as f32 - ay) * sc) + ty).ceil().min(h as f32)) as i32;
            let x0 = ((ax - ax * sc).floor().max(0.0)) as i32;
            let x1 = ((ax + (w as f32 - ax) * sc).ceil().min(w as f32)) as i32;
            let px = surf.px();
            px.fill(0);
            if y1 > y0 && x1 > x0 {
                let rows = (y1 - y0) as usize;
                let area = rows * (x1 - x0) as usize;
                let nthreads = if area < 120_000 { 1 } else { self.threads };
                let chunk_rows = rows.div_ceil(nthreads).max(1);
                let region = &mut px[(y0 * w) as usize..(y1 * w) as usize];
                let params = (w, x0, x1, ax, ay, ty, 1.0 / sc, a8);
                if nthreads == 1 {
                    compose_rows(region, y0, cache, params);
                } else {
                    std::thread::scope(|sc_| {
                        for (ci, chunk) in region.chunks_mut(chunk_rows * w as usize).enumerate() {
                            let row0 = y0 + (ci * chunk_rows) as i32;
                            sc_.spawn(move || compose_rows(chunk, row0, cache, params));
                        }
                    });
                }
            }
        }
        shell_present(self.hwnd, &surf);
        self.surf = Some(surf);
    }

    /// Gambar statis (kualitas penuh) langsung ke jendela.
    fn paint(&mut self) {
        self.paint_pending = false;
        if self.animating {
            return;
        }
        let Some(mut surf) = self.surf.take() else { return };
        self.hits.clear();
        {
            let mut cv = surf.canvas();
            cv.clear();
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.draw(&mut cv)));
        }
        shell_present(self.hwnd, &surf);
        self.surf = Some(surf);
    }

    fn spawn_net(&mut self, action: NetAction, busy: Option<Hit>) {
        if self.net_busy {
            return;
        }
        self.net_busy = true;
        self.busy_hit = busy;
        let h = self.hwnd.0 as isize;
        let _ = std::thread::Builder::new().name("net".into()).spawn(move || {
            if matches!(action, NetAction::Scan) {
                // 1) daftar cepat dari cache Windows (langsung tampil), 2) pindai ulang lalu perbarui
                let quick = sysctl::scan_state(false);
                let b = Box::into_raw(Box::new(quick));
                unsafe {
                    if PostMessageW(Some(HWND(h as *mut _)), WM_NET, WPARAM(1 | 2), LPARAM(b as isize)).is_err() {
                        drop(Box::from_raw(b));
                    }
                }
            }
            let ok = match &action {
                NetAction::Scan => true,
                NetAction::Wifi(on) => sysctl::set_radios(true, *on),
                NetAction::Bt(on) => sysctl::set_bluetooth(*on),
                NetAction::Airplane(on) => sysctl::set_radios(false, !*on),
                NetAction::Hotspot(on) => sysctl::set_hotspot(*on),
                NetAction::Connect(p) => sysctl::wlan_connect(p),
                NetAction::Disconnect => sysctl::wlan_disconnect(),
            };
            if matches!(action, NetAction::Connect(_) | NetAction::Disconnect) {
                std::thread::sleep(std::time::Duration::from_millis(1800));
            }
            let st = sysctl::scan_state(true);
            let b = Box::into_raw(Box::new(st));
            unsafe {
                if PostMessageW(Some(HWND(h as *mut _)), WM_NET, WPARAM(ok as usize), LPARAM(b as isize)).is_err() {
                    drop(Box::from_raw(b));
                }
            }
        });
    }

    fn spawn_power(&mut self, saver: Option<bool>, mode: Option<u8>) {
        if self.saver_busy {
            return;
        }
        self.saver_busy = true;
        let h = self.hwnd.0 as isize;
        let _ = std::thread::Builder::new().name("power".into()).spawn(move || {
            let ok = match (saver, mode) {
                (Some(on), _) => sysctl::energy_saver_set(on),
                (_, Some(m)) => sysctl::power_mode_set(m),
                _ => false,
            };
            unsafe {
                let _ = PostMessageW(Some(HWND(h as *mut _)), WM_POWER, WPARAM(ok as usize), LPARAM(0));
            }
        });
    }

    fn say(&mut self, t: &str) {
        self.toast = t.to_string();
        self.toast_until = Some(Instant::now() + std::time::Duration::from_secs(6));
    }

    // ------------------------------------------------------------ gambar

    fn card_bg(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32) {
        let k = self.k;
        let p = self.pal;
        let r = 20.0 * k;
        for i in (1..=6).rev() {
            let e = i as f32 * 4.0 * k;
            let a = Rgba(0, 0, 0, (p.shadow as f32 * 0.55) as u8);
            cv.fill_rrect(x - e, y - e + 7.0 * k, w + 2.0 * e, h + 2.0 * e, r + e, a, a);
        }
        // kartu hampir opak agar teks selalu terbaca
        let a = Rgba(p.win_a.0, p.win_a.1, p.win_a.2, p.win_a.3.max(244));
        let b = Rgba(p.win_b.0, p.win_b.1, p.win_b.2, p.win_b.3.max(246));
        cv.fill_rrect(x, y, w, h, r, a, b);
        cv.stroke_rrect(x, y, w, h, r, 1.0_f32.max(k * 0.9), p.border);
        cv.fill_rrect(x + r * 0.6, y + 1.0, w - r * 1.2, 1.0, 0.5, Rgba(255, 255, 255, 34), Rgba(255, 255, 255, 0));
    }

    /// Judul panel ala macOS: judul semibold rata kiri + satu baris keterangan redup (tanpa kotak ikon).
    fn head(&mut self, cv: &mut Canvas, x: f32, y: f32, title: &str, sub: &str, sub_col: Rgba) {
        let k = self.k;
        let p = self.pal;
        self.t(cv, title, 17.0, true, x, y + 20.0 * k, p.text);
        if !sub.is_empty() {
            self.t(cv, sub, 12.0, false, x, y + 44.0 * k, sub_col);
        }
    }

    fn button(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, label: &str, hit: Hit, primary: bool) {
        let k = self.k;
        let p = self.pal;
        let h = 34.0 * k;
        let hot = self.hov(hit);
        let (bg, fg) = if primary {
            (if hot { p.accent.lerp(Rgba(255, 255, 255, 255), 0.18) } else { p.accent }, p.on_accent)
        } else {
            (if hot { p.hover.mul_a(2.6) } else { p.hover.mul_a(1.7) }, p.text)
        };
        cv.fill_rrect(x, y, w, h, h / 2.0, bg, bg);
        let tw_ = self.tw(label, 13.0, true);
        let th_ = self.th(13.0, true);
        self.t(cv, label, 13.0, true, x + (w - tw_) / 2.0, y + (h - th_) / 2.0, fg);
        self.hit(x, y, w, h, hit);
    }

    fn row(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, label: &str, value: &str) {
        let k = self.k;
        let p = self.pal;
        let h = 34.0 * k;
        cv.fill_rrect(x, y + 1.0, w, 1.0, 0.5, p.border.mul_a(0.6), p.border.mul_a(0.6));
        let lh = self.th(13.0, false);
        self.t(cv, label, 13.0, false, x, y + (h - lh) / 2.0 + 1.0, p.dim);
        let vw = self.tw(value, 13.5, true);
        self.t(cv, value, 13.5, true, x + w - vw, y + (h - lh) / 2.0 + 1.0, p.text);
    }

    fn draw(&mut self, cv: &mut Canvas) {
        let k = self.k;
        let (cw, ch) = (self.cw * k, self.ch * k);
        let mg = MG * k;
        let (x, y) = (mg, mg);
        self.card_bg(cv, x, y, cw, ch);
        match self.kind {
            FlyKind::Calendar => self.draw_calendar(cv, x, y, cw),
            FlyKind::Prayer => self.draw_prayer(cv, x, y, cw),
            FlyKind::Wifi => self.draw_wifi(cv, x, y, cw),
            FlyKind::Battery => self.draw_battery(cv, x, y, cw),
            FlyKind::Tray => self.draw_tray(cv, x, y, cw),
            FlyKind::Power => self.draw_power(cv, x, y, cw),
            FlyKind::Control => self.draw_control(cv, x, y, cw),
            FlyKind::Music => self.draw_music(cv, x, y, cw),
            FlyKind::Weather => self.draw_weather(cv, x, y, cw),
        }
    }

    fn draw_calendar(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let st = unsafe { GetLocalTime() };
        let today = prayer::days_from_civil(st.wYear as i32, st.wMonth as i32, st.wDay as i32);
        let pad = 22.0 * k;
        // header ala widget Kalender macOS: nama hari kecil berwarna merah, tanggal besar, jam di kanan atas
        let light = p.text.0 < 100;
        let red = macui::red(light);
        let dow = DAYS_FULL[(st.wDayOfWeek as usize) % 7];
        self.t(cv, &dow.to_uppercase(), 11.5, true, x + pad, y + 20.0 * k, red);
        let ds = format!("{} {} {}", st.wDay, MONTHS_FULL[((st.wMonth as usize).max(1) - 1) % 12], st.wYear);
        self.t(cv, &ds, 22.0, true, x + pad, y + 38.0 * k, p.text);
        let clock = if self.style_cfg.clock_24h { format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond) } else {
            let h12 = if st.wHour % 12 == 0 { 12 } else { st.wHour % 12 };
            format!("{}:{:02}:{:02} {}", h12, st.wMinute, st.wSecond, if st.wHour < 12 { "AM" } else { "PM" })
        };
        let tw_ = self.tw(&clock, 13.0, false);
        self.t(cv, &clock, 13.0, false, x + cw - pad - tw_, y + 22.0 * k, p.dim);
        // navigasi bulan
        let ny = y + 84.0 * k;
        let label = format!("{} {}", MONTHS_FULL[(self.cal_m as usize - 1) % 12], self.cal_y);
        let lw = self.tw(&label, 15.0, true);
        let lh = self.th(15.0, true);
        let _ = lw;
        self.t(cv, &label, 15.0, true, x + pad, ny + (30.0 * k - lh) / 2.0, p.text);
        for (i, hit) in [Hit::Prev, Hit::Next].into_iter().enumerate() {
            let bx = if i == 0 { x + cw - pad - 60.0 * k } else { x + cw - pad - 30.0 * k };
            let hot = self.hov(hit);
            if hot {
                cv.fill_circle(bx + 15.0 * k, ny + 15.0 * k, 15.0 * k, p.hover.mul_a(2.0));
            }
            let d = if i == 0 { -1.0 } else { 1.0 };
            icons::chevron(cv, bx + 15.0 * k, ny + 15.0 * k, 20.0 * k, d, p.text);
            self.hit(bx, ny, 30.0 * k, 30.0 * k, hit);
        }
        // tombol "Hari ini" bila bukan bulan berjalan
        if self.cal_y != st.wYear as i32 || self.cal_m != st.wMonth as i32 {
            let bw = 64.0 * k;
            let bx = x + cw - pad - 60.0 * k - 6.0 * k - bw;
            let by = ny + 5.0 * k;
            let hot = self.hov(Hit::Today);
            let bg = if hot { p.hover.mul_a(2.6) } else { p.hover.mul_a(1.7) };
            cv.fill_rrect(bx, by, bw, 20.0 * k, 10.0 * k, bg, bg);
            let tw2 = self.tw("Hari ini", 11.5, true);
            let th2 = self.th(11.5, true);
            self.t(cv, "Hari ini", 11.5, true, bx + (bw - tw2) / 2.0, by + (20.0 * k - th2) / 2.0, p.text);
            self.hit(bx, by, bw, 20.0 * k, Hit::Today);
        }
        // judul hari
        let gx = x + pad;
        let gw = cw - 2.0 * pad;
        let cell = gw / 7.0;
        let wy = y + 144.0 * k;
        for (i, n) in WD_SHORT.iter().enumerate() {
            let w_ = self.tw(n, 11.5, true);
            let col = p.dim;
            self.t(cv, n, 11.5, true, gx + cell * i as f32 + (cell - w_) / 2.0, wy, col);
        }
        // petak tanggal
        let first = prayer::days_from_civil(self.cal_y, self.cal_m, 1);
        let off = (dow_sun0(first) + 6) % 7;
        let start = first - off as i64;
        let ry0 = y + 168.0 * k;
        let rh = 36.0 * k;
        for i in 0..42i64 {
            let day = start + i;
            let (yy, mm, dd) = prayer::civil_from_days(day);
            let (r, c) = ((i / 7) as f32, (i % 7) as f32);
            let cx = gx + cell * c + cell / 2.0;
            let cy = ry0 + rh * r + rh / 2.0;
            let cur = mm == self.cal_m && yy == self.cal_y;
            let is_today = day == today;
            let is_sel = self.sel_day == Some(day);
            let hot = self.hov(Hit::Day(day));
            let rad = 15.0 * k;
            if is_today {
                cv.fill_circle(cx, cy, rad, red);
            } else if is_sel {
                cv.fill_circle(cx, cy, rad, p.text.a(0.16));
            } else if hot {
                cv.fill_circle(cx, cy, rad, p.hover.mul_a(2.0));
            }
            let s = dd.to_string();
            let w_ = self.tw(&s, 13.5, is_today);
            let h_ = self.th(13.5, is_today);
            let col = if is_today { Rgba(255, 255, 255, 255) } else if !cur { p.dim.mul_a(0.45) } else { p.text };
            self.t(cv, &s, 13.5, is_today, cx - w_ / 2.0, cy - h_ / 2.0, col);
            self.hit(cx - cell / 2.0, cy - rh / 2.0, cell, rh, Hit::Day(day));
        }
    }

    fn draw_prayer(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 22.0 * k;
        let Some(info) = self.prayer.clone() else {
            self.t(cv, "Jadwal salat tidak tersedia", 14.0, false, x + pad, y + 40.0 * k, p.dim);
            return;
        };
        let st = unsafe { GetLocalTime() };
        let now_min = st.wHour as i32 * 60 + st.wMinute as i32;
        let sub = format!("{} \u{b7} {} {} {}", if info.loc.is_empty() { "-" } else { &info.loc }, st.wDay, MONTHS_FULL[((st.wMonth as usize).max(1) - 1) % 12], st.wYear);
        self.head(cv, x + pad, y, "Jadwal Salat", &sub, p.dim);
        // kartu "berikutnya": netral, satu warna aksen
        let (bx, by, bw, bh) = (x + pad, y + 84.0 * k, cw - 2.0 * pad, 90.0 * k);
        cv.fill_rrect(bx, by, bw, bh, 16.0 * k, p.card.mul_a(1.8), p.card.mul_a(1.8));
        icons::prayer_color(cv, info.idx, bx + 16.0 * k + 22.0 * k, by + 38.0 * k, 44.0 * k, p.text.0 < 100);
        let tx0 = bx + 16.0 * k + 44.0 * k + 8.0 * k;
        self.t(cv, "BERIKUTNYA", 10.0, true, tx0, by + 12.0 * k, p.dim);
        self.t(cv, &info.name, 21.0, true, tx0, by + 27.0 * k, p.text);
        let tw_ = self.tw(&info.time, 30.0, true);
        self.t(cv, &info.time, 30.0, true, bx + bw - 16.0 * k - tw_, by + 12.0 * k, p.text);
        let left = format!("{} lagi", info.left);
        let lw = self.tw(&left, 12.0, true);
        self.t(cv, &left, 12.0, true, bx + bw - 16.0 * k - lw, by + 50.0 * k, p.dim);
        let (px0, pw, ph) = (bx + 16.0 * k, bw - 32.0 * k, 5.0 * k);
        cv.fill_rrect(px0, by + bh - 15.0 * k, pw, ph, ph / 2.0, p.track, p.track);
        cv.fill_rrect(px0, by + bh - 15.0 * k, (pw * info.progress.clamp(0.0, 1.0)).max(ph), ph, ph / 2.0, p.accent, p.accent);
        // daftar waktu dengan ikon berwarna
        let rows_y = y + 190.0 * k;
        let rh = 40.0 * k;
        let veil = Rgba(p.win_a.0, p.win_a.1, p.win_a.2, 150);
        for (i, (name, time)) in info.all.iter().enumerate() {
            let ry = rows_y + i as f32 * rh;
            let is_next = *name == info.name;
            let pidx = prayer::NAMES.iter().position(|n| n == name).unwrap_or(2).min(5);
            let mins = time.split_once(':').and_then(|(h, m)| Some(h.parse::<i32>().ok()? * 60 + m.parse::<i32>().ok()?)).unwrap_or(0);
            let passed = !is_next && mins <= now_min;
            let informational = name == "Terbit";
            if is_next {
                cv.fill_rrect(x + pad - 8.0 * k, ry + 2.0 * k, cw - 2.0 * pad + 16.0 * k, rh - 4.0 * k, 13.0 * k, p.hover.mul_a(2.2), p.hover.mul_a(2.2));
            }
            let (cx, cy) = (x + pad + 14.0 * k, ry + rh / 2.0);
            // ikon selalu berwarna dua warna; waktu yang sudah lewat cukup dipudarkan lewat teksnya
            icons::prayer_color(cv, pidx, cx, cy, 28.0 * k, p.text.0 < 100);
            let _ = veil;
            let lh = self.th(14.5, is_next);
            let col = if is_next { p.text } else if passed || informational { p.dim } else { p.text };
            let label = if informational { "Terbit (bukan waktu salat)".to_string() } else { name.clone() };
            self.t(cv, &label, 14.5, is_next, x + pad + 36.0 * k, ry + (rh - lh) / 2.0, col);
            let tw2 = self.tw(time, 15.5, true);
            self.t(cv, time, 15.5, true, x + cw - pad - tw2, ry + (rh - lh) / 2.0, col);
        }
        // kaki
        let fy = y + self.ch * k - 52.0 * k;
        let m = self.tw(&self.method.clone(), 11.0, false);
        let _ = m;
        let method = self.method.clone();
        self.t(cv, &method, 11.0, false, x + pad, fy + 10.0 * k, p.dim);
        let bw2 = 84.0 * k;
        self.button(cv, x + cw - pad - bw2, fy + 2.0 * k, bw2, "Atur", Hit::Settings, false);
    }

    /// Ubin koneksi ala Pusat Kontrol macOS: lingkaran ikon (biru saat aktif), nama, dan status di bawahnya.
    fn tile(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, glyph: u8, label: &str, active: bool, enabled: bool, busy: bool, hit: Hit) {
        let k = self.k;
        let p = self.pal;
        let hot = enabled && self.hov(hit);
        let dim = if enabled { 1.0 } else { 0.45 };
        let bg = if hot { p.hover.mul_a(2.4) } else { p.card.mul_a(1.8) };
        cv.fill_rrect(x, y, w, h, 16.0 * k, bg.mul_a(dim), bg.mul_a(dim));
        let d = 28.0 * k;
        let (cx, cy) = (x + 12.0 * k + d / 2.0, y + 9.0 * k + d / 2.0);
        let (cbg, fg) = if active { (p.accent, p.on_accent) } else { (p.text.a(0.16), p.text) };
        cv.fill_circle(cx, cy, d / 2.0, cbg.mul_a(dim));
        self.glyph_in_circle(cv, glyph, cx, cy, d, fg.mul_a(dim));
        let name = self.fit(label, 12.0, true, w - 20.0 * k);
        let nh = self.th(12.0, true);
        let ny = y + 9.0 * k + d + 4.0 * k;
        self.t(cv, &name, 12.0, true, x + 12.0 * k, ny, p.text.mul_a(dim));
        let status = if !enabled { "Tidak tersedia" } else if busy { "Menunggu\u{2026}" } else if active { "Nyala" } else { "Mati" };
        self.t(cv, status, 10.5, false, x + 12.0 * k, ny + nh, p.dim.mul_a(dim));
        if enabled {
            self.hit(x, y, w, h, hit);
        }
    }

    /// Ubin lebar ala Pusat Kontrol macOS: lingkaran ikon di kiri, nama dan status di kanan.
    fn tile_wide(&mut self, cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, glyph: u8, label: &str, active: bool, enabled: bool, busy: bool, hit: Hit) {
        let k = self.k;
        let p = self.pal;
        let hot = enabled && self.hov(hit);
        let dim = if enabled { 1.0 } else { 0.45 };
        let bg = if hot { p.hover.mul_a(2.4) } else { p.card.mul_a(1.8) };
        cv.fill_rrect(x, y, w, h, 16.0 * k, bg.mul_a(dim), bg.mul_a(dim));
        let d = (h - 20.0 * k).min(36.0 * k);
        let (cx, cy) = (x + 12.0 * k + d / 2.0, y + h / 2.0);
        let (cbg, fg) = if active { (p.accent, p.on_accent) } else { (p.text.a(0.16), p.text) };
        cv.fill_circle(cx, cy, d / 2.0, cbg.mul_a(dim));
        self.glyph_in_circle(cv, glyph, cx, cy, d, fg.mul_a(dim));
        let tx = x + 12.0 * k + d + 10.0 * k;
        let avail = w - (tx - x) - 8.0 * k;
        let name = self.fit(label, 13.0, true, avail);
        let (nh, sh) = (self.th(13.0, true), self.th(11.5, false));
        let top = y + (h - nh - sh) / 2.0;
        self.t(cv, &name, 13.0, true, tx, top, p.text.mul_a(dim));
        let status = if !enabled { "Tidak tersedia" } else if busy { "Menunggu\u{2026}" } else if active { "Nyala" } else { "Mati" };
        self.t(cv, status, 11.5, false, tx, top + nh, p.dim.mul_a(dim));
        if enabled {
            self.hit(x, y, w, h, hit);
        }
    }

    /// Glif ubin koneksi di dalam lingkaran berdiameter `d`.
    fn glyph_in_circle(&mut self, cv: &mut Canvas, glyph: u8, cx: f32, cy: f32, d: f32, c: Rgba) {
        let g = d * 0.60;
        match glyph {
            0 => icons::wifi(cv, cx, cy, g * 1.08, 3, c, c.mul_a(0.35)),
            1 => icons::airplane(cv, cx, cy, g, c),
            2 => icons::hotspot(cv, cx, cy, g * 1.05, c),
            4 => icons::bluetooth(cv, cx, cy, g, c),
            _ => icons::battery_saver(cv, cx, cy, g, c),
        }
    }

    fn fit(&mut self, s: &str, px: f32, bold: bool, max_w: f32) -> String {
        let mut st = s.to_string();
        while self.tw(&st, px, bold) > max_w && st.chars().count() > 3 {
            st.pop();
        }
        if st != s {
            st.push('\u{2026}');
        }
        st
    }

    fn draw_wifi(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 22.0 * k;
        let info = self.wifi.take();
        // status gabungan: profil koneksi WinRT (paling akurat) + detail adapter
        let st = self.st.clone();
        let up = st.as_ref().map(|s| s.up).unwrap_or_else(|| info.as_ref().map(|i| i.connected).unwrap_or(false));
        let wifi_kind = st.as_ref().map(|s| s.kind == 1).unwrap_or_else(|| info.as_ref().map(|i| i.kind.contains("Wi")).unwrap_or(false));
        let mut ssid = st.as_ref().map(|s| s.ssid.clone()).unwrap_or_default();
        if ssid.is_empty() {
            ssid = info.as_ref().map(|i| i.ssid.clone()).unwrap_or_default();
        }
        // pastikan jaringan tersambung selalu muncul di daftar walau Windows tidak menyertakannya (izin lokasi/privasi SSID)
        if let Some(n) = self.net.as_mut() {
            if up && wifi_kind && !ssid.is_empty() && !n.nets.iter().any(|a| a.connected || a.ssid == ssid) {
                let sig = info.as_ref().and_then(|i| i.signal).unwrap_or_else(|| st.as_ref().map(|s| s.lit as u32 * 30 + 10).unwrap_or(60));
                n.nets.insert(0, crate::sysctl::AvNet { ssid: ssid.clone(), signal: sig, secured: true, profile: ssid.clone(), connected: true });
            }
            for a in n.nets.iter_mut() {
                if up && wifi_kind && !ssid.is_empty() && a.ssid == ssid {
                    a.connected = true;
                }
            }
        }
        self.head(cv, x + pad, y, "Jaringan", "", p.dim);
        let kind_label = if !up { "Tidak terhubung" } else if wifi_kind { "Wi-Fi" } else { "Ethernet" };
        let col = if up { p.accent } else { p.dim };
        cv.fill_circle(x + pad + 4.0 * k, y + 52.0 * k, 3.5 * k, col);
        self.t(cv, kind_label, 12.0, true, x + pad + 14.0 * k, y + 44.0 * k, col);

        // kartu jaringan terhubung
        let (cx0, cy0, cwid, chei) = (x + pad, y + 84.0 * k, cw - 2.0 * pad, 62.0 * k);
        let lit = st.as_ref().map(|s| s.lit).unwrap_or(2);
        cv.fill_rrect(cx0, cy0, cwid, chei, 16.0 * k, p.card.mul_a(1.8), p.card.mul_a(1.8));
        let (gx, gy, gs) = (cx0 + 14.0 * k + 21.0 * k, cy0 + chei / 2.0, 42.0 * k);
        let white = Rgba(255, 255, 255, 255);
        if up {
            cv.fill_circle(gx, gy, gs / 2.0, p.accent);
            if wifi_kind {
                icons::wifi(cv, gx, gy, gs * 0.58, lit, white, white.a(0.4));
            } else {
                icons::ethernet(cv, gx, gy, gs * 0.58, white);
            }
        } else {
            cv.fill_circle(gx, gy, gs / 2.0, p.track);
            icons::wifi_off(cv, gx, gy, gs * 0.58, p.dim, p.dim.mul_a(0.5));
        }
        let tx = gx + gs / 2.0 + 12.0 * k;
        let (ip, speed) = info.as_ref().map(|i| (i.ip.clone(), i.speed_mbps)).unwrap_or_default();
        let avail = cwid - (tx - cx0) - 14.0 * k;
        if up {
            let title = if wifi_kind && !ssid.is_empty() { ssid.clone() } else if wifi_kind { "Wi-Fi".to_string() } else { "Ethernet".to_string() };
            let title = self.fit(&title, 15.5, true, avail);
            self.t(cv, &title, 15.5, true, tx, cy0 + 11.0 * k, p.text);
            let sub = format!("Terhubung{}{}", speed.map(|s| format!("  \u{b7}  {} Mbps", s)).unwrap_or_default(), if ip.is_empty() { String::new() } else { format!("  \u{b7}  {}", ip) });
            let sub = self.fit(&sub, 11.5, false, avail);
            let th_ = self.th(15.5, true);
            self.t(cv, &sub, 11.5, false, tx, cy0 + 13.0 * k + th_, p.dim);
        } else {
            self.t(cv, "Tidak terhubung", 15.5, true, tx, cy0 + 11.0 * k, p.text);
            let th_ = self.th(15.5, true);
            self.t(cv, "Pilih jaringan di bawah", 11.5, false, tx, cy0 + 13.0 * k + th_, p.dim);
        }

        // ubin: Wi-Fi, Mode pesawat, Hotspot seluler
        let net = self.net.clone();
        let gap = 10.0 * k;
        let tw_ = (cw - 2.0 * pad - 2.0 * gap) / 3.0;
        let ty = y + 158.0 * k;
        let th_ = 78.0 * k;
        let wifi_on = net.as_ref().and_then(|n| n.wifi_on).unwrap_or(wifi_kind && up);
        let air = net.as_ref().and_then(|n| n.airplane).unwrap_or(false);
        let hot = net.as_ref().and_then(|n| n.hotspot).unwrap_or(false);
        let hot_ok = net.as_ref().map(|n| n.hotspot_ok).unwrap_or(true);
        let b = self.busy_hit;
        self.tile(cv, x + pad, ty, tw_, th_, 0, "Wi-Fi", wifi_on, true, b == Some(Hit::TileWifi), Hit::TileWifi);
        self.tile(cv, x + pad + tw_ + gap, ty, tw_, th_, 1, "Pesawat", air, true, b == Some(Hit::TileAir), Hit::TileAir);
        self.tile(cv, x + pad + 2.0 * (tw_ + gap), ty, tw_, th_, 2, "Hotspot", hot, hot_ok, b == Some(Hit::TileHot), Hit::TileHot);

        // daftar jaringan tersedia
        let ly = ty + th_ + 16.0 * k;
        self.t(cv, "JARINGAN TERSEDIA", 10.5, true, x + pad + 2.0 * k, ly, p.dim);
        let scanning = self.net_busy && self.busy_hit.is_none();
        let rl = if scanning { "Memindai\u{2026}" } else { "Segarkan" };
        let rw = self.tw(rl, 11.5, true);
        let rhot = self.hov(Hit::Refresh);
        self.t(cv, rl, 11.5, true, x + cw - pad - rw, ly - 1.0 * k, if rhot { p.text } else { p.dim });
        self.hit(x + cw - pad - rw - 8.0 * k, ly - 6.0 * k, rw + 16.0 * k, 24.0 * k, Hit::Refresh);
        let list_y = ly + 22.0 * k;
        let rh = 46.0 * k;
        let rows = 5usize;
        let net = self.net.clone();
        match net.as_ref() {
            None => {
                self.t(cv, "Mencari jaringan\u{2026}", 13.0, false, x + pad + 4.0 * k, list_y + 14.0 * k, p.dim);
            }
            Some(n) if n.wifi_on == Some(false) => {
                self.t(cv, "Wi-Fi mati. Hidupkan untuk melihat jaringan.", 12.5, false, x + pad + 4.0 * k, list_y + 14.0 * k, p.dim);
            }
            Some(n) if n.nets.is_empty() => {
                let msg = if scanning { "Mencari jaringan\u{2026}" } else { "Tidak ada jaringan ditemukan" };
                self.t(cv, msg, 13.0, false, x + pad + 4.0 * k, list_y + 14.0 * k, p.dim);
            }
            Some(n) => {
                for (i, a) in n.nets.iter().take(rows).enumerate() {
                    let ry = list_y + i as f32 * rh;
                    let hot = self.hov(Hit::Net(i));
                    if hot || a.connected {
                        let bg = if a.connected { p.accent.a(0.16) } else { p.hover.mul_a(1.6) };
                        cv.fill_rrect(x + pad - 6.0 * k, ry + 2.0 * k, cw - 2.0 * pad + 12.0 * k, rh - 4.0 * k, 13.0 * k, bg, bg);
                    }
                    // ikon sinyal Wi-Fi bergaya Windows 11
                    let arcs = if a.signal >= 70 { 3 } else if a.signal >= 45 { 2 } else if a.signal >= 20 { 1 } else { 0 };
                    let ic = if a.connected { p.accent } else { p.text };
                    icons::wifi(cv, x + pad + 14.0 * k, ry + rh / 2.0, 22.0 * k, arcs, ic, ic.mul_a(0.28));
                    let nx = x + pad + 38.0 * k;
                    let name = self.fit(&a.ssid, 14.0, a.connected, cw - 2.0 * pad - 112.0 * k);
                    let nh = self.th(14.0, a.connected);
                    let sub = if a.connected { "Tersambung" } else if !a.profile.is_empty() { "Tersimpan" } else if a.secured { "Aman" } else { "Terbuka" };
                    self.t(cv, &name, 14.0, a.connected, nx, ry + 6.0 * k, if a.connected { p.accent } else { p.text });
                    self.t(cv, sub, 11.0, false, nx, ry + 8.0 * k + nh, p.dim);
                    let ax = x + cw - pad - 10.0 * k;
                    if hot {
                        let t = if a.connected { "Putuskan" } else if a.profile.is_empty() { "Buka panel" } else { "Hubungkan" };
                        let tw2 = self.tw(t, 11.5, true);
                        let ah = self.th(11.5, true);
                        self.t(cv, t, 11.5, true, ax - tw2, ry + (rh - ah) / 2.0, p.accent);
                    } else if a.secured && !a.connected {
                        icons::lock(cv, ax - 6.0 * k, ry + rh / 2.0 - 1.0 * k, 15.0 * k, p.dim);
                    } else if a.connected {
                        icons::check(cv, ax - 6.0 * k, ry + rh / 2.0, 18.0 * k, p.accent);
                    }
                    self.hit(x + pad - 6.0 * k, ry, cw - 2.0 * pad + 12.0 * k, rh, Hit::Net(i));
                }
            }
        }
        // pesan singkat + tombol
        let fy = y + self.ch * k - 52.0 * k;
        if !self.toast.is_empty() && self.toast_until.map(|t| t > Instant::now()).unwrap_or(false) {
            let line = self.toast.clone();
            self.t(cv, &line, 11.5, false, x + pad + 2.0 * k, fy - 24.0 * k, p.dim);
        }
        self.wifi = info;
        let bw = (cw - 2.0 * pad - 10.0 * k) / 2.0;
        self.button(cv, x + pad, fy, bw, "Pengaturan Wi-Fi", Hit::Btn(0), false);
        self.button(cv, x + pad + bw + 10.0 * k, fy, bw, "Pengaturan Cepat", Hit::Btn(1), false);
    }


    /// Daya: kunci, tidur, mulai ulang, matikan, keluar. Tindakan merusak butuh klik kedua untuk konfirmasi.
    fn draw_power(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 20.0 * k;
        self.head(cv, x + pad, y, "Daya", "Pilih tindakan", p.dim);
        let items = [("Kunci layar", 0u8), ("Tidur", 1), ("Mulai ulang", 2), ("Matikan", 3), ("Keluar akun", 4)];
        let (ry0, rh) = (y + 76.0 * k, 44.0 * k);
        for (i, (label, act)) in items.iter().enumerate() {
            let ry = ry0 + i as f32 * rh;
            let hot = self.hov(Hit::PowerAct(*act));
            let asking = self.confirm == Some(*act);
            if hot || asking {
                let bg = if asking { Rgba(255, 90, 80, 70) } else { p.hover.mul_a(2.0) };
                cv.fill_rrect(x + pad - 6.0 * k, ry + 2.0 * k, cw - 2.0 * pad + 12.0 * k, rh - 4.0 * k, 12.0 * k, bg, bg);
            }
            let ic = if asking { Rgba(255, 120, 110, 255) } else { p.text };
            let (cx, cy) = (x + pad + 14.0 * k, ry + rh / 2.0);
            match act {
                0 => icons::lock(cv, cx, cy - 1.0 * k, 22.0 * k, ic),
                1 => icons::moon_sleep(cv, cx, cy, 20.0 * k, ic),
                2 => icons::restart(cv, cx, cy, 20.0 * k, ic),
                3 => icons::power(cv, cx, cy, 20.0 * k, ic),
                _ => icons::signout(cv, cx, cy, 20.0 * k, ic),
            }
            let lh = self.th(14.0, false);
            self.t(cv, label, 14.0, false, x + pad + 38.0 * k, ry + (rh - lh) / 2.0, ic);
            if asking {
                let t = "Klik lagi untuk konfirmasi";
                let tw_ = self.tw(t, 11.0, true);
                let th_ = self.th(11.0, true);
                self.t(cv, t, 11.0, true, x + cw - pad - tw_, ry + (rh - th_) / 2.0, ic);
            }
            self.hit(x + pad - 6.0 * k, ry, cw - 2.0 * pad + 12.0 * k, rh, Hit::PowerAct(*act));
        }
    }

    /// Pusat Kontrol ala macOS: ubin koneksi 2x2, slider suara pil tebal, dan dua tombol pintas.
    fn draw_control(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let light = p.text.0 < 100;
        let pad = 18.0 * k;
        // ubin koneksi
        let net = self.net.clone();
        let st = self.st.clone();
        let wifi_on = net.as_ref().and_then(|n| n.wifi_on).unwrap_or(st.as_ref().map(|s| s.up && s.kind == 1).unwrap_or(false));
        let bt_on = net.as_ref().and_then(|n| n.bt_on);
        let air = net.as_ref().and_then(|n| n.airplane).unwrap_or(false);
        let hot = net.as_ref().and_then(|n| n.hotspot).unwrap_or(false);
        let hot_ok = net.as_ref().map(|n| n.hotspot_ok).unwrap_or(false);
        let gap = 10.0 * k;
        let tw2 = (cw - 2.0 * pad - gap) / 2.0;
        let th2 = 58.0 * k;
        let ty = y + pad;
        let b = self.busy_hit;
        self.tile_wide(cv, x + pad, ty, tw2, th2, 0, "Wi-Fi", wifi_on, true, b == Some(Hit::TileWifi), Hit::TileWifi);
        self.tile_wide(cv, x + pad + tw2 + gap, ty, tw2, th2, 4, "Bluetooth", bt_on.unwrap_or(false), bt_on.is_some(), b == Some(Hit::TileBt), Hit::TileBt);
        self.tile_wide(cv, x + pad, ty + th2 + gap, tw2, th2, 1, "Pesawat", air, true, b == Some(Hit::TileAir), Hit::TileAir);
        self.tile_wide(cv, x + pad + tw2 + gap, ty + th2 + gap, tw2, th2, 2, "Hotspot", hot, hot_ok, b == Some(Hit::TileHot), Hit::TileHot);
        // suara: slider pil tebal, ikon speaker di dalam isian (ketuk ikon = bisukan)
        let (lvl, muted) = self.vol.unwrap_or((0.0, false));
        let sy = ty + 2.0 * th2 + gap + 18.0 * k;
        self.t(cv, "Suara", 13.0, true, x + pad + 2.0 * k, sy, p.text);
        let pct = format!("{}%", (lvl * 100.0).round() as i32);
        let pw_ = self.tw(&pct, 12.0, false);
        self.t(cv, &pct, 12.0, false, x + cw - pad - 2.0 * k - pw_, sy + 1.0 * k, p.dim);
        let bar_y = sy + self.th(13.0, true) + 8.0 * k;
        let bh = 28.0 * k;
        let (bx, bw_) = (x + pad, cw - 2.0 * pad);
        let shown = if muted { 0.0 } else { lvl };
        let fill = if light { Rgba(255, 255, 255, 255) } else { Rgba(255, 255, 255, 238) };
        macui::pill_slider(cv, bx, bar_y, bw_, bh, shown, p.track, fill);
        icons::volume(cv, bx + bh / 2.0 + 1.0 * k, bar_y + bh / 2.0, 22.0 * k, Rgba(58, 58, 64, 255), muted || lvl <= 0.001, lvl);
        // hit bar lebih dulu, lalu ikon speaker (yang terakhir didaftarkan menang saat tumpang tindih)
        self.hit(bx - 6.0 * k, bar_y - 4.0 * k, bw_ + 12.0 * k, bh + 8.0 * k, Hit::VolBar);
        self.hit(bx, bar_y, bh, bh, Hit::VolMute);
        let fy = y + self.ch * k - pad - 34.0 * k;
        let bw2 = (cw - 2.0 * pad - 10.0 * k) / 2.0;
        self.button(cv, x + pad, fy, bw2, "Pengaturan suara", Hit::Btn(2), false);
        self.button(cv, x + pad + bw2 + 10.0 * k, fy, bw2, "Pengaturan Windows", Hit::Btn(3), false);
    }

    /// Pemutar musik ala widget Now Playing macOS: sampul membulat, judul/artis, kontrol polos tanpa latar.
    fn draw_music(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 20.0 * k;
        let m = self.music.clone();
        let white = Rgba(255, 255, 255, 255);
        // sampul (placeholder berwarna; media Windows tidak selalu menyertakan gambar)
        let a = 72.0 * k;
        let (ax, ay) = (x + pad, y + pad);
        if m.is_some() {
            cv.fill_rrect(ax, ay, a, a, 15.0 * k, Rgba(255, 108, 124, 255), Rgba(238, 38, 76, 255));
            icons::music_note(cv, ax + a / 2.0, ay + a / 2.0, 34.0 * k, white);
        } else {
            let c = p.text.a(0.10);
            cv.fill_rrect(ax, ay, a, a, 15.0 * k, c, c);
            icons::music_note(cv, ax + a / 2.0, ay + a / 2.0, 34.0 * k, p.dim);
        }
        let tx = ax + a + 14.0 * k;
        let avail = x + cw - pad - tx;
        let (h1, h2, h3) = (self.th(15.0, true), self.th(13.0, false), self.th(11.5, false));
        let top = ay + (a - (h1 + h2 + h3 + 2.0 * k)) / 2.0;
        match &m {
            Some(m) => {
                let title = self.fit(&m.title, 15.0, true, avail);
                self.t(cv, &title, 15.0, true, tx, top, p.text);
                let artist = self.fit(if m.artist.is_empty() { &m.app } else { &m.artist }, 13.0, false, avail);
                self.t(cv, &artist, 13.0, false, tx, top + h1, p.dim);
                let app = self.fit(if m.playing { "Sedang diputar" } else { "Dijeda" }, 11.5, false, avail);
                self.t(cv, &app, 11.5, false, tx, top + h1 + h2 + 2.0 * k, p.dim.mul_a(0.8));
            }
            None => {
                self.t(cv, "Tidak ada media", 15.0, true, tx, top, p.text);
                let l1 = self.fit("Putar musik atau video di", 12.0, false, avail);
                let l2 = self.fit("aplikasi apa pun.", 12.0, false, avail);
                self.t(cv, &l1, 12.0, false, tx, top + h1, p.dim);
                self.t(cv, &l2, 12.0, false, tx, top + h1 + h2, p.dim);
            }
        }
        // kontrol polos: sebelumnya | putar/jeda | berikutnya
        let cy = ay + a + 36.0 * k;
        let mid = x + cw / 2.0;
        let playing = m.as_ref().map(|m| m.playing).unwrap_or(false);
        for (i, dx) in [-80.0f32, 0.0, 80.0].iter().enumerate() {
            let (cx, r) = (mid + dx * k, 26.0 * k);
            let hit = Hit::Media(i as u8);
            if self.hov(hit) {
                cv.fill_circle(cx, cy, r, p.hover.mul_a(2.2));
            }
            if i == 1 {
                if playing {
                    icons::pause(cv, cx, cy, 36.0 * k, p.text);
                } else {
                    icons::play(cv, cx + 1.5 * k, cy, 36.0 * k, p.text);
                }
            } else {
                icons::skip(cv, cx, cy, 26.0 * k, if i == 0 { -1.0 } else { 1.0 }, p.text);
            }
            self.hit(cx - r, cy - r, 2.0 * r, 2.0 * r, hit);
        }
    }

    /// Cuaca saat ini dari Open-Meteo.
    fn draw_weather(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 20.0 * k;
        let place = crate::weather::place();
        let title = if place.is_empty() { "Cuaca".to_string() } else { self.fit(&place, 17.0, true, cw - 2.0 * pad - 80.0 * k) };
        self.head(cv, x + pad, y, &title, "Kondisi saat ini", p.dim);
        match self.wx.clone() {
            Some(w) => {
                let t = format!("{}\u{b0}", w.temp);
                // suhu besar tipis ala widget Cuaca macOS
                self.t(cv, &t, 58.0, true, x + pad - 2.0 * k, y + 66.0 * k, p.text);
                self.t(cv, icons::weather_text(w.code), 14.0, true, x + pad, y + 138.0 * k, p.text);
                let (gx, gy) = (x + cw - pad - 40.0 * k, y + 108.0 * k);
                icons::weather_glyph(cv, icons::weather_class(w.code), w.day, gx, gy, 74.0 * k, (self.pal.text.0 < 100));
                let rows = [
                    ("Terasa seperti", format!("{}\u{b0}C", w.feels)),
                    ("Tertinggi / terendah", format!("{}\u{b0} / {}\u{b0}", w.hi, w.lo)),
                    ("Kelembapan", format!("{}%", w.humidity)),
                    ("Angin", format!("{} km/jam", w.wind)),
                ];
                let mut ry = y + 164.0 * k;
                for (a, b) in rows.iter() {
                    self.row(cv, x + pad, ry, cw - 2.0 * pad, a, b);
                    ry += 28.0 * k;
                }
            }
            None => {
                self.t(cv, "Memuat data cuaca\u{2026}", 13.0, false, x + pad + 2.0 * k, y + 90.0 * k, p.dim);
                self.t(cv, "Butuh koneksi internet dan lokasi di Pengaturan > Salat.", 11.5, false, x + pad + 2.0 * k, y + 114.0 * k, p.dim);
            }
        }
        let fy = y + self.ch * k - 40.0 * k;
        let bw2 = 96.0 * k;
        self.button(cv, x + cw - pad - bw2, fy, bw2, "Segarkan", Hit::WxRefresh, false);
    }

    /// Panel ikon tray: ikon kecil dan tajam dalam grid ringkas.
    fn draw_tray(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 20.0 * k;
        let want = ((16.0 * k / PANEL_SCALE).round() as i32).max(14);
        let views = tray::views(want);
        let sub = if views.is_empty() { "Belum ada ikon".to_string() } else { format!("{} aplikasi", views.len()) };
        self.head(cv, x + pad, y, "Ikon tray", &sub, p.dim);
        let cols = 6usize;
        let cell = (cw - 2.0 * pad) / cols as f32;
        let rows = ((Fly::size_for(FlyKind::Tray, self.tray_n).1 - 150.0) / 44.0) as usize;
        let cap = cols * rows.max(1);
        let gy = y + 82.0 * k;
        let rh = 44.0 * k;
        if views.is_empty() {
            self.t(cv, "Ikon aplikasi latar belakang muncul di sini.", 12.0, false, x + pad + 2.0 * k, gy + 8.0 * k, p.dim);
        }
        let mut tip = String::new();
        for (n, v) in views.iter().take(cap).enumerate() {
            let (r, c) = (n / cols, n % cols);
            let (cx0, cy0) = (x + pad + c as f32 * cell, gy + r as f32 * rh);
            let hot = self.hov(Hit::TrayIcon(v.idx));
            let (bw, bh) = (cell - 4.0 * k, rh - 4.0 * k);
            if hot {
                cv.fill_rrect(cx0 + 2.0 * k, cy0, bw, bh, 10.0 * k, p.hover.mul_a(2.2), p.hover.mul_a(2.2));
                tip = v.tip.clone();
            }
            let (icx, icy) = (cx0 + cell / 2.0, cy0 + bh / 2.0);
            match &v.icon {
                Some(img) => cv.blit_image(img, (icx - img.w as f32 / 2.0).round() as i32, (icy - img.h as f32 / 2.0).round() as i32),
                None => cv.fill_circle(icx, icy, 5.0 * k, p.dim.mul_a(0.5)),
            }
            self.hit(cx0 + 2.0 * k, cy0, bw, bh, Hit::TrayIcon(v.idx));
        }
        let fy = y + self.ch * k - 36.0 * k;
        let hint = if tip.is_empty() { "Klik kiri: buka  \u{b7}  Klik kanan: menu".to_string() } else { tip };
        let hint = self.fit(&hint, 12.0, !views.is_empty() && self.hover.is_some(), cw - 2.0 * pad);
        self.t(cv, &hint, 12.0, self.hover.is_some(), x + pad + 2.0 * k, fy, if self.hover.is_some() { p.text } else { p.dim });
    }

    fn draw_battery(&mut self, cv: &mut Canvas, x: f32, y: f32, cw: f32) {
        let k = self.k;
        let p = self.pal;
        let pad = 22.0 * k;
        let light = p.text.0 < 100;
        self.head(cv, x + pad, y, "Baterai", "", p.dim);
        let Some(b) = self.bat.as_ref().map(|b| (b.pct, b.ac, b.charging, b.saver, b.secs_left)) else {
            self.t(cv, "Perangkat ini tidak memiliki baterai", 13.0, false, x + pad, y + 100.0 * k, p.dim);
            return;
        };
        let (pct, ac, charging, saver, secs) = b;
        let status = if charging { "Mengisi daya" } else if ac && pct >= 95 { "Terisi penuh" } else if ac { "Tersambung ke listrik" } else { "Menggunakan baterai" };
        // seperti baterai macOS: putih biasa, hijau saat mengisi, merah saat hampir habis
        let col = if pct <= 15 && !charging { macui::red(light) } else if charging { macui::green(light) } else { p.text };
        self.t(cv, status, 12.0, false, x + pad, y + 44.0 * k, p.dim);
        // cincin persen
        let (rcx, rcy, rr) = (x + cw / 2.0, y + 140.0 * k, 42.0 * k);
        cv.arc(rcx, rcy, rr, 7.0 * k, 0.0, 360.0, p.track);
        let sweep = 360.0 * pct as f32 / 100.0;
        let a0 = 270.0;
        let a1 = (a0 + sweep).min(360.0);
        cv.arc_cap(rcx, rcy, rr, 7.0 * k, a0, a1.max(a0 + 1.0), col);
        if a0 + sweep > 360.0 {
            cv.arc_cap(rcx, rcy, rr, 7.0 * k, 0.0, a0 + sweep - 360.0, col);
        }
        let ps = format!("{}%", pct);
        let pw = self.tw(&ps, 26.0, true);
        let ph = self.th(26.0, true);
        self.t(cv, &ps, 26.0, true, rcx - pw / 2.0, rcy - ph / 2.0, p.text);
        let time_txt = match secs {
            Some(s) if !charging && !ac => format!("{} j {} m", s / 3600, (s % 3600) / 60),
            _ if charging => "Sedang mengisi".to_string(),
            _ => "-".to_string(),
        };
        let rw = cw - 2.0 * pad;
        let mut yy = y + 200.0 * k;
        for (l, v) in [("Perkiraan sisa waktu", time_txt), ("Sumber daya", if ac { "Listrik AC".to_string() } else { "Baterai".to_string() })] {
            self.row(cv, x + pad, yy, rw, l, &v);
            yy += 34.0 * k;
        }
        // penghemat baterai: baris kartu dengan switch hijau ala macOS
        let sy = yy + 12.0 * k;
        let sh = 56.0 * k;
        let sv = saver;
        let hot = self.hov(Hit::TileSaver);
        let bg = if hot { p.hover.mul_a(2.4) } else { p.card.mul_a(1.8) };
        cv.fill_rrect(x + pad, sy, rw, sh, 14.0 * k, bg, bg);
        let d = 32.0 * k;
        let (gcx, gcy) = (x + pad + 14.0 * k + d / 2.0, sy + sh / 2.0);
        let (cbg, gfg) = if sv { (macui::green(light), Rgba(255, 255, 255, 255)) } else { (p.text.a(0.16), p.text) };
        cv.fill_circle(gcx, gcy, d / 2.0, cbg);
        icons::battery_saver(cv, gcx, gcy, 19.0 * k, gfg);
        let nx = x + pad + 14.0 * k + d + 12.0 * k;
        self.t(cv, "Penghemat baterai", 14.0, true, nx, sy + 10.0 * k, p.text);
        let sub = if self.saver_busy { "Mengubah\u{2026}" } else if sv { "Nyala" } else { "Mati" };
        let th14 = self.th(14.0, true);
        self.t(cv, sub, 11.5, false, nx, sy + 10.0 * k + th14, p.dim);
        macui::switch(cv, x + pad + rw - 14.0 * k - 19.0 * k, sy + sh / 2.0, k, if sv { 1.0 } else { 0.0 }, macui::green(light), p.text.a(0.22));
        self.hit(x + pad, sy, rw, sh, Hit::TileSaver);
        // mode daya
        let my = sy + sh + 14.0 * k;
        self.t(cv, "MODE DAYA", 10.5, true, x + pad + 2.0 * k, my, p.dim);
        let (seg_y, seg_h) = (my + 20.0 * k, 36.0 * k);
        cv.fill_rrect(x + pad, seg_y, rw, seg_h, seg_h / 2.0, p.card.mul_a(1.8), p.card.mul_a(1.8));
        let cur = self.mode.unwrap_or(1);
        let segw = rw / 3.0;
        for (i, name) in ["Hemat daya", "Seimbang", "Performa"].iter().enumerate() {
            let sx = x + pad + i as f32 * segw;
            let sel = cur as usize == i;
            let hot = self.hov(Hit::Mode(i as u8));
            if sel {
                let c = if light { Rgba(255, 255, 255, 255) } else { Rgba(255, 255, 255, 56) };
                cv.fill_rrect(sx + 3.0 * k, seg_y + 3.0 * k, segw - 6.0 * k, seg_h - 6.0 * k, (seg_h - 6.0 * k) / 2.0, c, c);
            } else if hot {
                cv.fill_rrect(sx + 3.0 * k, seg_y + 3.0 * k, segw - 6.0 * k, seg_h - 6.0 * k, (seg_h - 6.0 * k) / 2.0, p.hover.mul_a(1.6), p.hover.mul_a(1.6));
            }
            let tw2 = self.tw(name, 12.0, true);
            let th2 = self.th(12.0, true);
            self.t(cv, name, 12.0, true, sx + (segw - tw2) / 2.0, seg_y + (seg_h - th2) / 2.0, if sel { p.text } else { p.dim });
            self.hit(sx, seg_y, segw, seg_h, Hit::Mode(i as u8));
        }
        let bw = (rw - 10.0 * k) / 2.0;
        let fy = y + self.ch * k - 52.0 * k;
        self.button(cv, x + pad, fy, bw, "Pengaturan baterai", Hit::Btn(0), false);
        self.button(cv, x + pad + bw + 10.0 * k, fy, bw, "Pengaturan Cepat", Hit::Btn(1), false);
    }

    // ------------------------------------------------------------ aksi

    fn click(&mut self, x: f32, y: f32) {
        let hit = self.hits.iter().rev().find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|r| r.hit);
        match hit {
            Some(Hit::Prev) => {
                self.cal_m -= 1;
                if self.cal_m < 1 {
                    self.cal_m = 12;
                    self.cal_y -= 1;
                }
            }
            Some(Hit::Next) => {
                self.cal_m += 1;
                if self.cal_m > 12 {
                    self.cal_m = 1;
                    self.cal_y += 1;
                }
            }
            Some(Hit::Today) => {
                let st = unsafe { GetLocalTime() };
                self.cal_y = st.wYear as i32;
                self.cal_m = st.wMonth as i32;
                self.sel_day = None;
            }
            Some(Hit::Day(d)) => self.sel_day = if self.sel_day == Some(d) { None } else { Some(d) },
            Some(Hit::Settings) => {
                self.begin_close();
                crate::open_settings_tab(3);
            }
            Some(Hit::Btn(0)) => {
                self.begin_close();
                match self.kind {
                    FlyKind::Wifi => shell::launch("ms-settings:network-wifi"),
                    FlyKind::Battery => shell::launch("ms-settings:batterysaver"),
                    _ => {}
                }
            }
            Some(Hit::Btn(2)) => {
                self.begin_close();
                shell::launch("ms-settings:sound");
            }
            Some(Hit::Btn(3)) => {
                self.begin_close();
                shell::launch("ms-settings:");
            }
            Some(Hit::Btn(_)) => {
                self.begin_close();
                shell::send_win_combo(0x41); // Win+A
            }
            Some(Hit::TileWifi) => {
                let on = self.net.as_ref().and_then(|n| n.wifi_on).unwrap_or(true);
                self.spawn_net(NetAction::Wifi(!on), Some(Hit::TileWifi));
            }
            Some(Hit::TileAir) => {
                let on = self.net.as_ref().and_then(|n| n.airplane).unwrap_or(false);
                self.spawn_net(NetAction::Airplane(!on), Some(Hit::TileAir));
            }
            Some(Hit::TileHot) => {
                let on = self.net.as_ref().and_then(|n| n.hotspot).unwrap_or(false);
                self.spawn_net(NetAction::Hotspot(!on), Some(Hit::TileHot));
            }
            Some(Hit::TileBt) => {
                let on = self.net.as_ref().and_then(|n| n.bt_on).unwrap_or(false);
                self.spawn_net(NetAction::Bt(!on), Some(Hit::TileBt));
            }
            Some(Hit::PowerAct(a)) => {
                if matches!(a, 2 | 3 | 4) && self.confirm != Some(a) {
                    self.confirm = Some(a);
                } else {
                    self.begin_close();
                    match a {
                        0 => crate::system::lock(),
                        1 => crate::system::sleep(),
                        2 => crate::system::restart(),
                        3 => crate::system::shutdown(),
                        _ => crate::system::signout(),
                    }
                }
            }
            Some(Hit::Media(i)) => {
                use crate::media::{command, Cmd};
                command(match i {
                    0 => Cmd::Prev,
                    1 => Cmd::PlayPause,
                    _ => Cmd::Next,
                });
                if i == 1 {
                    if let Some(m) = self.music.as_mut() {
                        m.playing = !m.playing;
                    }
                }
            }
            Some(Hit::VolMute) => {
                let (l, m) = self.vol.unwrap_or((0.0, false));
                crate::volume::set_mute(!m);
                self.vol = Some((l, !m));
            }
            Some(Hit::WxRefresh) => crate::weather::kick(),
            Some(Hit::VolBar) => {}
            Some(Hit::Refresh) => self.spawn_net(NetAction::Scan, None),
            Some(Hit::Net(i)) => {
                let a = self.net.as_ref().and_then(|n| n.nets.get(i).cloned());
                if let Some(a) = a {
                    if a.connected {
                        self.spawn_net(NetAction::Disconnect, None);
                    } else if !a.profile.is_empty() {
                        self.say(&format!("Menghubungkan ke {}\u{2026}", a.ssid));
                        self.spawn_net(NetAction::Connect(a.profile.clone()), None);
                    } else {
                        // jaringan baru butuh kata sandi: buka panel Wi-Fi bawaan Windows
                        self.begin_close();
                        shell::launch("ms-availablenetworks:");
                    }
                }
            }
            Some(Hit::TileSaver) => {
                let on = self.bat.as_ref().map(|b| b.saver).unwrap_or(false);
                self.spawn_power(Some(!on), None);
            }
            Some(Hit::Mode(m)) => {
                self.mode = Some(m);
                self.spawn_power(None, Some(m));
            }
            Some(Hit::TrayIcon(i)) => {
                self.begin_close();
                tray::click(i, false);
            }
            None => {}
        }
        self.request_paint();
    }

    fn click_right(&mut self, x: f32, y: f32) {
        let hit = self.hits.iter().rev().find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|r| r.hit);
        if let Some(Hit::TrayIcon(i)) = hit {
            // panel tetap terbuka sampai menu klik-kanan aplikasi selesai dipakai
            self.menu_mode = Some(Instant::now());
            self.menu_seen = false;
            unsafe {
                SetTimer(Some(self.hwnd), 4, 150, None);
            }
            tray::click(i, true);
        }
    }

    /// Atur volume dari posisi x pada batang (rect klik = batang + 6 piksel tiap sisi).
    fn set_vol_from(&mut self, x: f32, rx: f32, rw: f32) {
        let k = self.k;
        let frac = ((x - (rx + 6.0 * k)) / (rw - 12.0 * k).max(1.0)).clamp(0.0, 1.0);
        crate::volume::set(frac);
        self.vol = Some((frac, false));
        self.request_paint();
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        let new = self.hits.iter().rev().find(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h).map(|r| r.hit);
        if new != self.hover {
            self.hover = new;
            self.request_paint();
        }
    }
}

fn dbgf(line: String) {
    if std::env::var_os("LITEDOCK_DEBUG").is_some() {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(crate::config::dir().join("debug.log")) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

fn compose_rows(chunk: &mut [u32], row0: i32, cache: &Image, (w, x0, x1, ax, ay, ty, inv, a8): (i32, i32, i32, f32, f32, f32, f32, u32)) {
    let nrows = chunk.len() / w as usize;
    let (cw, ch) = (cache.w, cache.h);
    for r in 0..nrows {
        let yd = row0 + r as i32;
        let sy = ay + (yd as f32 + 0.5 - ty - ay) * inv - 0.5;
        let yf = sy.floor();
        let fy = ((sy - yf) * 256.0) as u32;
        let yi = yf as i32;
        let row = &mut chunk[r * w as usize..(r + 1) * w as usize];
        let sx0 = ax + (x0 as f32 + 0.5 - ax) * inv - 0.5;
        let mut u_fp: i64 = (sx0 * 65536.0) as i64;
        let du = (inv * 65536.0) as i64;
        if yi < 0 || yi + 1 >= ch {
            for xd in x0..x1 {
                let sxf = ax + (xd as f32 + 0.5 - ax) * inv - 0.5;
                let p = cache.sample_t(sxf, sy);
                if p >> 24 != 0 {
                    row[xd as usize] = scale_px(p, a8);
                }
            }
            continue;
        }
        let r0 = &cache.px[(yi * cw) as usize..((yi + 1) * cw) as usize];
        let r1 = &cache.px[((yi + 1) * cw) as usize..((yi + 2) * cw) as usize];
        for xd in x0..x1 {
            let xi = (u_fp >> 16) as i32;
            let fx = ((u_fp >> 8) & 255) as u32;
            u_fp += du;
            let p = if xi >= 0 && xi + 1 < cw {
                let (a, b) = (r0[xi as usize], r0[xi as usize + 1]);
                let (c, d) = (r1[xi as usize], r1[xi as usize + 1]);
                if (a | b | c | d) == 0 {
                    continue;
                }
                lerp_px(lerp_px(a, b, fx), lerp_px(c, d, fx), fy)
            } else {
                cache.sample_t((u_fp - du) as f32 / 65536.0, sy)
            };
            if p >> 24 != 0 {
                row[xd as usize] = scale_px(p, a8);
            }
        }
    }
}

fn shell_present(hwnd: HWND, surf: &Surface) {
    crate::genie::present_here(hwnd, surf);
}

/// Buka panel. `anchor_cx`: tengah modul (layar); `dock_top`: sisi atas latar dock (layar).
pub fn open(kind: FlyKind, anchor_cx: f32, dock_top: f32, mon: RECT, ui: f32, prayer: Option<PrayerInfo>, cfg: &Config) {
    close_now();
    unsafe {
        let hi = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(proc),
            hInstance: hi.into(),
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            lpszClassName: w!("LiteDockFly"),
            ..Default::default()
        };
        RegisterClassExW(&wc);
        let tray_n_init = if kind == FlyKind::Tray { tray::count() + 1 } else { 0 };
        let (cw, ch) = Fly::size_for(kind, tray_n_init);
        let k = ui * PANEL_SCALE;
        let (ww, wh) = (((cw + 2.0 * MG) * k).ceil() as i32, ((ch + 2.0 * MG) * k).ceil() as i32);
        let gap = 10.0 * k;
        let lo = mon.left + 4;
        let hi_ = (mon.right - ww - 4).max(lo);
        let wx = ((anchor_cx - ww as f32 / 2.0) as i32).clamp(lo, hi_);
        // sisi bawah kartu tepat di atas dock
        let wy = ((dock_top - gap - (ch + MG) * k).round() as i32).max(mon.top + 4);
        let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_TOPMOST, w!("LiteDockFly"), w!(""), WS_POPUP, wx, wy, ww, wh, None, None, Some(hi.into()), None);
        let Ok(hwnd) = hwnd else { return };
        let st = GetLocalTime();
        let mut f = Box::new(Fly {
            hwnd,
            surf: Surface::new(ww, wh),
            text: TextCache::default(),
            k,
            kind,
            pal: palette(cfg.style, shell::system_uses_light_theme()),
            style_cfg: cfg.clone(),
            cw,
            ch,
            t0: Instant::now(),
            closing: false,
            hits: Vec::new(),
            hover: None,
            cal_y: st.wYear as i32,
            cal_m: st.wMonth as i32,
            sel_day: None,
            prayer,
            method: crate::prayer::METHODS[cfg.method.min(crate::prayer::METHODS.len() - 1)].name.to_string(),
            wifi: None,
            bat: None,
            tick: 0,
            tracking: false,
            paint_pending: false,
            cache: None,
            animating: false,
            net: None,
            st: None,
            tray_n: tray_n_init,
            net_busy: false,
            busy_hit: None,
            toast: String::new(),
            toast_until: None,
            mode: None,
            saver_busy: false,
            frames: 0,
            compose_ms: 0.0,
            threads: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 6),
            vol: None,
            confirm: None,
            music: None,
            wx: None,
            menu_mode: None,
            menu_seen: false,
            drag_vol: false,
        });
        f.refresh_data();
        FLY = Box::into_raw(f);
        OPEN_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
        OPEN_KIND.store(
            match kind {
                FlyKind::Calendar => 0,
                FlyKind::Prayer => 1,
                FlyKind::Wifi => 2,
                FlyKind::Battery => 3,
                FlyKind::Tray => 4,
                FlyKind::Power => 5,
                FlyKind::Control => 6,
                FlyKind::Music => 7,
                FlyKind::Weather => 8,
            },
            Ordering::SeqCst,
        );
        let _ = ShowWindow(hwnd, SW_SHOW);
        if let Some(f) = fly() {
            f.build_cache();
            f.t0 = Instant::now();
            f.animating = true;
            f.compose(0.0);
            if f.kind == FlyKind::Wifi {
                f.spawn_net(NetAction::Scan, None);
            }
            crate::request_frame();
        }
        SetTimer(Some(hwnd), T_TICK, 1000, None);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// Render uji tanpa jendela.
pub fn selftest_render(kind: FlyKind, cfg: &Config, k: f32, prayer: Option<PrayerInfo>) -> Option<Surface> {
    let k = k * PANEL_SCALE;
    let (cw, ch) = Fly::size_for(kind, 4);
    let mut surf = Surface::new(((cw + 2.0 * MG) * k) as i32, ((ch + 2.0 * MG) * k) as i32)?;
    let st = unsafe { GetLocalTime() };
    let mut f = Fly {
        hwnd: HWND::default(),
        surf: None,
        text: TextCache::default(),
        k,
        kind,
        pal: palette(cfg.style, false),
        style_cfg: cfg.clone(),
        cw,
        ch,
        t0: Instant::now(),
        closing: false,
        hits: Vec::new(),
        hover: None,
        cal_y: st.wYear as i32,
        cal_m: st.wMonth as i32,
        sel_day: None,
        prayer,
        method: crate::prayer::METHODS[0].name.to_string(),
        wifi: Some(shell::WifiInfo { connected: true, kind: "Wi-Fi".into(), ssid: "Rumah_5G".into(), signal: Some(82), ip: "192.168.1.24".into(), speed_mbps: Some(866), adapter: "Wi-Fi".into() }),
        bat: Some(shell::BatInfo { pct: 78, ac: false, charging: false, saver: false, secs_left: Some(3 * 3600 + 41 * 60) }),
        tick: 0,
        tracking: false,
        paint_pending: false,
        cache: None,
        animating: false,
        st: Some(shell::NetStatus { up: true, kind: 1, lit: 3, ssid: "Rumah_5G".into() }),
        tray_n: 4,
        net: Some(NetState {
            bt_on: Some(true),
            wifi_on: Some(true),
            airplane: Some(false),
            hotspot: Some(false),
            hotspot_ok: true,
            nets: vec![
                sysctl::AvNet { ssid: "Rumah_5G".into(), signal: 82, secured: true, profile: "Rumah_5G".into(), connected: true },
                sysctl::AvNet { ssid: "Kantor-Guest".into(), signal: 61, secured: true, profile: String::new(), connected: false },
                sysctl::AvNet { ssid: "WiFi.id".into(), signal: 40, secured: false, profile: String::new(), connected: false },
                sysctl::AvNet { ssid: "Tetangga_2.4G".into(), signal: 22, secured: true, profile: "Tetangga_2.4G".into(), connected: false },
            ],
        }),
        net_busy: false,
        busy_hit: None,
        toast: String::new(),
        toast_until: None,
        mode: Some(1),
        saver_busy: false,
        frames: 0,
        compose_ms: 0.0,
        vol: Some((0.6, false)),
        confirm: None,
        music: Some(crate::media::MediaSnap { title: "Judul Lagu".into(), artist: "Nama Artis".into(), app: String::new(), playing: true }),
        wx: Some(crate::weather::Weather { temp: 31, feels: 35, code: 2, day: true, humidity: 74, wind: 12, hi: 33, lo: 25 }),
        menu_mode: None,
        menu_seen: false,
        drag_vol: false,
        threads: 1,
    };
    {
        let mut cv = surf.canvas();
        f.draw(&mut cv);
    }
    Some(surf)
}

unsafe extern "system" fn proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| proc_inner(h, msg, wp, lp))) {
        Ok(r) => r,
        Err(_) => LRESULT(0),
    }
}

unsafe fn proc_inner(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(f) = fly() else {
        return DefWindowProcW(h, msg, wp, lp);
    };
    let lx = (lp.0 & 0xFFFF) as i16 as f32;
    let ly = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
    match msg {
        WM_MOUSEMOVE => {
            if !f.tracking {
                let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                let _ = TrackMouseEvent(&mut t);
                f.tracking = true;
            }
            if f.drag_vol {
                if let Some((rx, rw)) = f.hits.iter().find(|r| r.hit == Hit::VolBar).map(|r| (r.x, r.w)) {
                    f.set_vol_from(lx, rx, rw);
                }
                return LRESULT(0);
            }
            f.mouse_move(lx, ly);
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            f.tracking = false;
            if f.hover.is_some() {
                f.hover = None;
                f.request_paint();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            let r = f.hits.iter().rev().find(|r| lx >= r.x && lx < r.x + r.w && ly >= r.y && ly < r.y + r.h).map(|r| (r.hit, r.x, r.w));
            if let Some((Hit::VolBar, rx, rw)) = r {
                f.drag_vol = true;
                SetCapture(h);
                f.set_vol_from(lx, rx, rw);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            if f.drag_vol {
                f.drag_vol = false;
                let _ = ReleaseCapture();
                return LRESULT(0);
            }
            f.click(lx, ly);
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            f.click_right(lx, ly);
            LRESULT(0)
        }
        WM_KEYDOWN => {
            match VIRTUAL_KEY(wp.0 as u16) {
                VK_ESCAPE => f.begin_close(),
                VK_LEFT if f.kind == FlyKind::Calendar => {
                    f.cal_m -= 1;
                    if f.cal_m < 1 {
                        f.cal_m = 12;
                        f.cal_y -= 1;
                    }
                    f.request_paint();
                }
                VK_RIGHT if f.kind == FlyKind::Calendar => {
                    f.cal_m += 1;
                    if f.cal_m > 12 {
                        f.cal_m = 1;
                        f.cal_y += 1;
                    }
                    f.request_paint();
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            // klik di luar panel (fokus pindah) -> tutup
            if (wp.0 & 0xFFFF) == 0 && f.menu_mode.is_none() {
                f.begin_close();
            }
            LRESULT(0)
        }
        WM_TIMER => {
            match wp.0 {
                T_TICK => {
                    f.tick += 1;
                    match f.kind {
                        FlyKind::Wifi => {
                            if f.tick % 3 == 0 {
                                f.wifi = Some(shell::wifi_details());
                                f.st = Some(shell::net_status_cached());
                            }
                            if f.tick % 10 == 0 && !f.net_busy && !f.animating {
                                f.spawn_net(NetAction::Scan, None);
                            }
                            f.request_paint();
                        }
                        FlyKind::Battery => {
                            if f.tick % 3 == 0 {
                                f.refresh_data();
                            }
                            f.request_paint();
                        }
                        _ => f.request_paint(),
                    }
                }
                3 => {
                    let _ = KillTimer(Some(h), 3);
                    f.paint();
                }
                4 => {
                    // pantau menu konteks aplikasi (kelas jendela menu "#32768"); tutup panel setelah menu hilang
                    let open = FindWindowW(w!("#32768"), PCWSTR::null()).map(|w| !w.0.is_null() && IsWindowVisible(w).as_bool()).unwrap_or(false);
                    if let Some(t0) = f.menu_mode {
                        if open {
                            f.menu_seen = true;
                        } else if f.menu_seen || t0.elapsed() > Duration::from_millis(2500) {
                            f.menu_mode = None;
                            let _ = KillTimer(Some(h), 4);
                            if GetForegroundWindow() != h {
                                f.begin_close();
                            }
                        }
                    } else {
                        let _ = KillTimer(Some(h), 4);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_NET => {
            let st = Box::from_raw(lp.0 as *mut NetState);
            let ok = wp.0 & 1 != 0;
            let partial = wp.0 & 2 != 0;
            let was = f.busy_hit;
            f.net = Some(*st);
            f.st = Some(shell::net_status_cached());
            f.wifi = Some(shell::wifi_details());
            if partial {
                // hasil cepat dari cache; pemindaian penuh masih berjalan
                f.request_paint();
                return LRESULT(0);
            }
            f.net_busy = false;
            f.busy_hit = None;
            if !ok {
                match was {
                    Some(Hit::TileAir) => shell::launch("ms-settings:network-airplanemode"),
                    Some(Hit::TileHot) => shell::launch("ms-settings:network-mobilehotspot"),
                    Some(Hit::TileWifi) => shell::launch("ms-settings:network-wifi"),
                    Some(Hit::TileBt) => shell::launch("ms-settings:bluetooth"),
                    _ => {}
                }
            }
            f.request_paint();
            LRESULT(0)
        }
        WM_POWER => {
            f.saver_busy = false;
            if wp.0 == 0 {
                shell::launch("ms-settings:batterysaver");
            }
            f.refresh_data();
            f.request_paint();
            LRESULT(0)
        }
        WM_CLOSE => {
            f.begin_close();
            LRESULT(0)
        }
        WM_DESTROY => {
            OPEN_HWND.store(0, Ordering::SeqCst);
            OPEN_KIND.store(255, Ordering::SeqCst);
            if !FLY.is_null() {
                drop(Box::from_raw(FLY));
                FLY = std::ptr::null_mut();
            }
            shell::trim_memory();
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}
