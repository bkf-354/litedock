//! Model, tata letak, animasi zoom, dan render dock.
use crate::config::{Config, Style};
use crate::gfx::*;
use crate::icons;
use std::rc::Rc;

pub struct AppEntry {
    pub key: String,
    pub path: String,
    pub title: String,
    pub hwnds: Vec<isize>,
    pub all_minimized: bool,
    pub pinned: bool,
    pub uwp: bool,
    pub name: String,
    pub icon: Rc<Icon>,
}

impl AppEntry {
    pub fn stem(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        let s = std::path::Path::new(&self.path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let mut c = s.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => "Aplikasi".into(),
        }
    }
    pub fn label(&self) -> String {
        match self.hwnds.len() {
            0 => self.stem(),
            1 => {
                if self.title.is_empty() { self.stem() } else { self.title.clone() }
            }
            n => format!("{} ({})", self.stem(), n),
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum SlotId {
    Start,
    /// satu modul salat: ikon dua warna, nama singkat, jam, titik tengah, hitung mundur
    Prayer,
    Sep(u8),
    App(String),
    Wifi,
    Battery,
    Clock,
    Tray,
    Bin,
    Explorer,
    Power,
    Control,
    Music,
    Weather,
}

pub struct Slot {
    pub id: SlotId,
    pub scale: f32,
    pub tscale: f32,
    pub appear: f32,
    /// kemajuan linear 0..1 (appear = easing dari prog)
    pub prog: f32,
    pub target: f32,
    pub base_w: f32,
    pub x: f32,
    pub w: f32,
    pub bounce: f32,
}

#[derive(Clone, Default)]
pub struct PrayerInfo {
    pub idx: usize,
    /// hitung mundur jj:mm:dd (diperbarui tiap detik)
    pub hms: String,
    /// jam-desimal (sejak tengah malam hari ini) waktu salat berikutnya / sebelumnya, untuk menghitung ulang tiap detik
    pub next_abs: f64,
    pub prev_abs: f64,
    pub name: String,
    pub time: String,
    pub left: String,
    pub soon: bool,
    pub progress: f32,
    pub loc: String,
    pub all: Vec<(String, String)>,
}

#[derive(Clone, Default, PartialEq)]
pub struct MusicInfo {
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Clone, Default)]
pub struct WeatherInfo {
    pub temp: i32,
    pub code: i32,
    pub day: bool,
}

#[derive(Clone, Default)]
pub struct Info {
    pub bin_full: bool,
    pub music: Option<MusicInfo>,
    pub weather: Option<WeatherInfo>,
    pub prayer: Option<PrayerInfo>,
    pub time: String,
    /// nama hari lengkap (Sabtu)
    pub day: String,
    /// tanggal (07 Nov 2026)
    pub date: String,
    pub wifi: bool,
    /// 0 tidak ada, 1 Wi-Fi, 2 Ethernet
    pub net_kind: u8,
    /// jumlah busur Wi-Fi yang menyala 0..=3
    pub wifi_lit: u8,
    pub battery: Option<(u8, bool)>,
    /// jumlah ikon tray yang di-host LiteDock
    pub tray_count: usize,
}

struct Theme {
    under: Rgba,
    top: Rgba,
    bot: Rgba,
    border: Rgba,
    hilite: Rgba,
    shadow_a: f32,
    fg: Rgba,
    dim: Rgba,
    accent: Rgba,
    sep: Rgba,
    text_shadow: bool,
    light: bool,
}

pub struct Dock {
    pub cfg: Config,
    pub ui: f32,
    pub slots: Vec<Slot>,
    pub apps: Vec<AppEntry>,
    pub info: Info,
    pub text: TextCache,
    pub mouse: Option<(f32, f32)>,
    pub shift: f32,
    tshift: f32,
    pub win_w: i32,
    pub win_h: i32,
    pub bg: (f32, f32, f32, f32),
    pub light_sys: bool,
    pub fg_hwnd: isize,
    scratch: Vec<u32>,
    /// latar statis (bayangan + kaca + tepi) yang sudah digambar; dipakai ulang bila geometri/tema tidak berubah
    bg_cache: Vec<u32>,
    pub bg_key: u64,
}

/// Kode singkat waktu salat untuk dock.
pub fn prayer_code(idx: usize) -> &'static str {
    match idx {
        0 => "SUB",
        1 => "TRB",
        2 => "DHU",
        3 => "ASH",
        4 => "MGH",
        _ => "ISY",
    }
}

/// Latar dock: bayangan konsentris + dasar + kaca bergradien + tepi + kilap.
fn paint_bg(cv: &mut Canvas, bx: f32, by: f32, bw: f32, bg_h: f32, r: f32, ui: f32, th: &Theme) {
    for i in (1..=5).rev() {
        let e = i as f32 * 3.0 * ui;
        let a = Rgba(0, 0, 0, (th.shadow_a * 255.0 / 2.0) as u8);
        cv.fill_rrect(bx - e, by - e + 5.0 * ui, bw + 2.0 * e, bg_h + 2.0 * e, r + e, a, a);
    }
    if th.under.3 > 0 {
        cv.fill_rrect(bx, by, bw, bg_h, r, th.under, th.under);
    }
    cv.fill_rrect(bx, by, bw, bg_h, r, th.top, th.bot);
    cv.stroke_rrect(bx, by, bw, bg_h, r, (1.0 * ui).max(1.0), th.border);
    cv.fill_rrect(bx + r * 0.6, by + (1.0 * ui).max(1.0), (bw - r * 1.2).max(0.0), 1.0, 0.5, th.hilite, th.hilite.mul_a(0.0));
}

fn smooth(cur: f32, target: f32, dt: f32, rate: f32) -> f32 {
    cur + (target - cur) * (1.0 - (-dt * rate).exp())
}

impl Dock {
    pub fn new(cfg: Config, ui: f32) -> Dock {
        Dock {
            cfg,
            ui,
            slots: Vec::new(),
            apps: Vec::new(),
            info: Info::default(),
            text: TextCache::default(),
            mouse: None,
            shift: 0.0,
            tshift: 0.0,
            win_w: 400,
            win_h: 200,
            bg: (0.0, 0.0, 0.0, 0.0),
            light_sys: false,
            fg_hwnd: 0,
            scratch: Vec::new(),
            bg_cache: Vec::new(),
            bg_key: 0,
        }
    }

    pub fn s(&self) -> f32 {
        self.cfg.icon_size as f32 * self.ui
    }
    /// padding vertikal latar dock
    fn pad(&self) -> f32 {
        self.s() * 0.17
    }
    /// padding kiri/kanan latar dock (dapat diatur)
    fn padx(&self) -> f32 {
        self.cfg.pad_x as f32 * self.ui
    }
    fn gap(&self) -> f32 {
        self.s() * 0.10
    }
    fn zoom(&self) -> f32 {
        if self.cfg.zoom_on { self.cfg.zoom } else { 1.0 }
    }
    /// Ukuran dasar modul sistem (Wi-Fi, baterai, tray, jam, salat): pengaturan sendiri, tidak ikut ukuran ikon aplikasi.
    fn m(&self) -> f32 {
        self.cfg.tray_size as f32 * self.ui
    }

    /// Garis dasar ikon aplikasi: ikon selalu di tengah vertikal latar dock (juga bila modul sistem lebih tinggi dari ikon).
    fn icon_baseline(&self, bg_top: f32) -> f32 {
        (bg_top + self.bg_h() / 2.0 + self.s() / 2.0).round()
    }

    pub fn bg_h(&self) -> f32 {
        self.s().max(self.m() * 1.04) + 2.0 * self.pad()
    }
    pub fn shadow_b(&self) -> f32 {
        16.0 * self.ui
    }
    fn speed(&self) -> f32 {
        self.cfg.anim_speed as f32 / 100.0
    }

    fn zoomable(id: &SlotId) -> bool {
        matches!(id, SlotId::Start | SlotId::App(_))
    }

    // ------------------------------------------------------------ slot sync

    /// Apakah modul sistem dengan kunci `key` aktif sesuai pengaturan.
    fn module_on(&self, key: &str) -> bool {
        let c = &self.cfg;
        match key {
            "prayer" => c.show_prayer && self.info.prayer.is_some(),
            "wifi" => c.show_wifi,
            "battery" => (c.show_battery || c.show_battery_pct) && self.info.battery.is_some(),
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

    fn slot_for(key: &str) -> Option<SlotId> {
        Some(match key {
            "prayer" => SlotId::Prayer,
            "wifi" => SlotId::Wifi,
            "battery" => SlotId::Battery,
            "clock" => SlotId::Clock,
            "tray" => SlotId::Tray,
            "bin" => SlotId::Bin,
            "explorer" => SlotId::Explorer,
            "power" => SlotId::Power,
            "control" => SlotId::Control,
            "music" => SlotId::Music,
            "weather" => SlotId::Weather,
            _ => return None,
        })
    }

    fn desired_ids(&self) -> Vec<SlotId> {
        let mut v = Vec::new();
        let c = &self.cfg;
        if c.show_start {
            v.push(SlotId::Start);
        }
        // modul salat berada di kiri (sebelum app), urutannya mengikuti daftar urutan modul
        let order: Vec<&str> = c.order.split(',').collect();
        for k in &order {
            if *k == "prayer" && self.module_on(k) {
                v.extend(Self::slot_for(k));
            }
        }
        if !v.is_empty() && !self.apps.is_empty() {
            v.push(SlotId::Sep(0));
        }
        for a in &self.apps {
            v.push(SlotId::App(a.key.clone()));
        }
        let mut tail = Vec::new();
        for k in &order {
            if *k != "prayer" && self.module_on(k) {
                tail.extend(Self::slot_for(k));
            }
        }
        if !tail.is_empty() && !v.is_empty() {
            v.push(SlotId::Sep(1));
        }
        v.extend(tail);
        v
    }

    /// Sinkronkan slot dengan daftar app/modul; slot app yang hilang memudar dulu.
    pub fn sync_slots(&mut self, instant: bool) {
        let want = self.desired_ids();
        let mut old: Vec<Slot> = std::mem::take(&mut self.slots);
        let mut new: Vec<Slot> = Vec::with_capacity(want.len());
        for id in &want {
            if let Some(p) = old.iter().position(|s| &s.id == id) {
                let mut s = old.remove(p);
                s.target = 1.0;
                new.push(s);
            } else {
                new.push(Slot { id: id.clone(), scale: 1.0, tscale: 1.0, appear: if instant { 1.0 } else { 0.0 }, prog: if instant { 1.0 } else { 0.0 }, target: 1.0, base_w: 0.0, x: 0.0, w: 0.0, bounce: -1.0 });
            }
        }
        // sisa slot lama: hanya app yang masih bisa memudar
        for (i, mut s) in old.into_iter().enumerate() {
            if matches!(s.id, SlotId::App(_)) && !instant && s.appear > 0.02 {
                s.target = 0.0;
                let at = i.min(new.len());
                new.insert(at, s);
            }
        }
        self.slots = new;
        self.refresh_base();
    }

    /// Satuan dasar modul sistem (px). Ikon, jarak, dan kapsul diturunkan dari sini sehingga selalu proporsional.
    fn u(&self) -> f32 {
        self.m() * 0.26
    }

    /// Ukuran huruf dasar (px) = satuan dasar x skala teks. Hanya teks yang ikut skala teks.
    fn fs(&self) -> f32 {
        self.u() * self.cfg.text_scale as f32 / 100.0
    }

    fn px(&self, f: f32) -> i32 {
        ((self.fs() * f).round() as i32).max(9)
    }

    /// Ketebalan teks dock sesuai pilihan Reguler/Tebal.
    fn wt(&self) -> i32 {
        crate::gfx::dock_weight(self.cfg.font_bold)
    }

    /// Lebar ikon Wi-Fi; baterai memakai 1.2x agar tinggi optiknya setara.
    fn ico(&self) -> f32 {
        self.u() * 1.45
    }

    /// Padding horizontal modul, jarak ikon-teks, jarak antar token teks, tinggi kapsul.
    fn mp(&self) -> f32 {
        self.u() * 0.55
    }
    fn ig(&self) -> f32 {
        self.u() * 0.40
    }
    fn ts(&self) -> f32 {
        self.fs() * 0.50
    }
    fn ph(&self) -> f32 {
        self.u() * 2.50
    }

    fn tsz(&mut self, t: &str, px: i32, wt: i32) -> (f32, f32) {
        let m = self.text.get_w(t, px, wt);
        ((m.w as f32 - 4.0).max(0.0), m.h as f32)
    }

    /// Gambar teks; x = tepi kiri tinta (bukan tepi bitmap).
    fn tdraw(&mut self, cv: &mut Canvas, t: &str, px: i32, wt: i32, x: f32, y: f32, col: Rgba) {
        let m = self.text.get_w(t, px, wt);
        cv.draw_mask(m, (x - 2.0).round() as i32, y.round() as i32, col);
    }

    fn tcol(&self, th: &Theme) -> Rgba {
        if self.cfg.text_color >= 0 {
            let c = self.cfg.text_color as u32;
            Rgba(((c >> 16) & 255) as u8, ((c >> 8) & 255) as u8, (c & 255) as u8, 255)
        } else {
            th.fg
        }
    }

    /// Jarak antar modul sistem (sama untuk semua modul, diatur di Pengaturan).
    fn sg(&self) -> f32 {
        self.cfg.sys_gap as f32 * self.ui
    }

    fn pill_pad(&self) -> f32 {
        if self.cfg.prayer_pill {
            self.sg().max(12.0 * self.ui)
        } else {
            self.sg()
        }
    }

    /// Ukuran glyph salat/cuaca berwarna.
    fn gs(&self) -> f32 {
        self.ph() * 0.66
    }

    pub fn music_label(&self) -> String {
        let Some(m) = &self.info.music else { return String::new() };
        let mut t = if m.artist.is_empty() { m.title.clone() } else { format!("{} \u{2013} {}", m.title, m.artist) };
        if t.chars().count() > 24 {
            t = t.chars().take(23).collect::<String>() + "\u{2026}";
        }
        t
    }

    fn weather_text(&self) -> String {
        match &self.info.weather {
            Some(w) => format!("{}\u{b0}", w.temp),
            None => "--\u{b0}".to_string(),
        }
    }

    /// Lebar titik tengah pemisah jam dan hitung mundur.
    fn dot_w(&self) -> f32 {
        (self.px(1.0) as f32 * 0.24).round().max(3.0)
    }

    /// Ukuran font (px) untuk glif ikon Windows 11 pada modul sistem.
    fn gpx(&self) -> i32 {
        ((self.ico() * 1.18).round() as i32).max(12)
    }

    /// (lebar, tinggi) tinta glif ikon Windows 11 acuan `refch` pada ukuran modul; None bila font ikon tidak ada.
    fn glyph_dims(&mut self, refch: char) -> Option<(f32, f32)> {
        let px = self.gpx();
        let ink = self.text.get_icon(refch, px)?.ink()?;
        Some(((ink.2 - ink.0 + 1) as f32, (ink.3 - ink.1 + 1) as f32))
    }

    /// Lebar ikon baterai (glif Windows 11 bila ada, selain itu ikon vektor).
    fn bat_icon_w(&mut self) -> f32 {
        match self.glyph_dims('\u{E83F}') {
            Some((w, _)) => w,
            None => self.ico() * 1.2 * 1.105,
        }
    }

    fn wifi_icon_w(&mut self) -> f32 {
        match self.glyph_dims('\u{E701}') {
            Some((w, _)) => w,
            None => self.ico(),
        }
    }

    /// Lebar isi (tanpa jarak antar modul) modul sistem, memakai teks yang sebenarnya.
    fn content_w(&mut self, id: &SlotId) -> f32 {
        let (ig, ts) = (self.ig(), self.ts());
        let (px, wt) = (self.px(1.0), self.wt());
        match id {
            SlotId::Prayer => {
                let Some(p) = self.info.prayer.clone() else { return 0.0 };
                let style = self.cfg.prayer_style;
                let mut total = self.gs();
                if style != 2 {
                    let (wc, _) = self.tsz(prayer_code(p.idx), px, wt);
                    let (wtm, _) = self.tsz(&p.time, px, wt);
                    total += ig + wc + ts + wtm;
                }
                if style == 0 {
                    let (wh, _) = self.tsz(&p.hms, px, wt);
                    total += ts + self.dot_w() + ts + wh;
                } else if style == 2 {
                    let (wh, _) = self.tsz(&p.hms, px, wt);
                    total += ig + wh;
                }
                total
            }
            SlotId::Wifi => self.wifi_icon_w(),
            SlotId::Bin | SlotId::Explorer | SlotId::Power | SlotId::Control => self.ico(),
            SlotId::Battery => {
                let Some((pct, _)) = self.info.battery else { return 0.0 };
                let mut total = 0.0;
                if self.cfg.show_battery {
                    total += self.bat_icon_w();
                }
                if self.cfg.show_battery_pct {
                    let (tw, _) = self.tsz(&format!("{}%", pct), px, wt);
                    total += if total > 0.0 { ig * 0.85 } else { 0.0 } + tw;
                }
                total
            }
            SlotId::Clock => {
                let (day, date, time) = (self.info.day.clone(), self.info.date.clone(), self.info.time.clone());
                let (wd, _) = self.tsz(&day, px, wt);
                let (wdt, _) = self.tsz(&date, px, wt);
                let (wtm, _) = self.tsz(&time, px, wt);
                wd + ts + wdt + ts + wtm
            }
            SlotId::Tray => self.ph() * 0.50,
            SlotId::Music => {
                let lab = self.music_label();
                let tw = if lab.is_empty() { 0.0 } else { ig + self.tsz(&lab, px, wt).0 };
                self.ico() * 0.95 + tw
            }
            SlotId::Weather => {
                let t = self.weather_text();
                let (tw, _) = self.tsz(&t, px, wt);
                self.gs() + ig + tw
            }
            _ => 0.0,
        }
    }

    fn measure(&mut self, id: &SlotId) -> f32 {
        let s = self.s();
        let gap = self.gap();
        match id {
            SlotId::Start | SlotId::App(_) => s + gap,
            SlotId::Sep(_) => s * 0.26,
            SlotId::Prayer => {
                if self.info.prayer.is_none() {
                    return 0.0;
                }
                self.content_w(id) + self.pill_pad() + gap
            }
            SlotId::Battery => {
                if self.info.battery.is_none() {
                    return 0.0;
                }
                self.content_w(id) + self.sg() + gap
            }
            SlotId::Tray => self.content_w(id) + self.sg().max(10.0 * self.ui) + gap,
            _ => self.content_w(id) + self.sg() + gap,
        }
    }

    /// Ukur ulang lebar tiap slot; true bila ada lebar yang berubah (perlu relayout jendela).
    pub fn refresh_base(&mut self) -> bool {
        let ids: Vec<SlotId> = self.slots.iter().map(|s| s.id.clone()).collect();
        let mut changed = false;
        for (i, id) in ids.iter().enumerate() {
            let w = self.measure(id);
            if (self.slots[i].base_w - w).abs() > 0.5 {
                changed = true;
            }
            self.slots[i].base_w = w;
        }
        changed
    }

    /// Ukuran jendela yang dibutuhkan (hasil akhir, tanpa memperhitungkan animasi muncul).
    pub fn desired_size(&self, screen_w: i32) -> (i32, i32) {
        let s = self.s();
        let pad = self.padx();
        let gap = self.gap();
        let (pl, pr) = self.edge_pads();
        let total: f32 = self.slots.iter().filter(|x| x.target > 0.5).map(|x| x.base_w).sum();
        let extra = (self.zoom() - 1.0) * s * 3.4 + 28.0 * self.ui;
        let w = (total - gap).max(s) + pl + pr + extra + 32.0 * self.ui;
        let overflow = ((self.zoom() - 1.0) * s - pad).max(0.5 * s - pad).max(0.0);
        let h = overflow + 0.35 * s + 24.0 * self.ui + self.cfg.label_gap as f32 * self.ui + self.bg_h() + self.shadow_b();
        ((w.ceil() as i32).min(screen_w.max(200)), h.ceil() as i32)
    }

    pub fn set_size(&mut self, w: i32, h: i32) {
        self.win_w = w;
        self.win_h = h;
    }

    // ------------------------------------------------------------ layout

    /// Seberapa jauh tinta yang terlihat dari sebuah modul berada di dalam slotnya (px): modul berkapsul (tray, salat berkapsul)
    /// terlihat sampai tepi slot, modul lain berisi ikon/teks yang diberi ruang sg/2 di kiri dan kanan.
    fn ink_inset(&self, id: &SlotId) -> f32 {
        match id {
            SlotId::Tray => 0.0,
            SlotId::Prayer if self.cfg.prayer_pill => 0.0,
            SlotId::Prayer => self.pill_pad() / 2.0,
            SlotId::Start | SlotId::App(_) | SlotId::Sep(_) => 0.0,
            SlotId::Battery | SlotId::Clock | SlotId::Wifi | SlotId::Bin | SlotId::Explorer | SlotId::Power | SlotId::Control | SlotId::Music | SlotId::Weather => self.sg() / 2.0,
        }
    }

    /// Padding kiri dan kanan latar dock yang diukur sampai tinta pertama/terakhir yang terlihat (bukan tepi slot),
    /// sehingga ikon paling kiri dan paling kanan berjarak sama dari tepi dock saat diam.
    fn edge_pads(&self) -> (f32, f32) {
        let pad = self.padx();
        let min = 3.0 * self.ui;
        let vis = |sl: &&Slot| sl.target > 0.5;
        let lead = self.slots.iter().find(vis).map(|sl| self.ink_inset(&sl.id)).unwrap_or(0.0);
        let tail = self.slots.iter().rev().find(vis).map(|sl| self.ink_inset(&sl.id)).unwrap_or(0.0);
        ((pad - lead).max(min), (pad - tail).max(min))
    }

    /// Hitung slot.x/slot.w untuk skala saat ini. Mengembalikan (kiri konten, lebar konten).
    fn layout(&mut self, shift: f32) -> (f32, f32) {
        let s = self.s();
        let gap = self.gap();
        let cx = self.win_w as f32 / 2.0;
        let (pl, pr) = self.edge_pads();
        let mut total = 0.0;
        for sl in self.slots.iter_mut() {
            sl.w = if Self::zoomable(&sl.id) { (s * sl.scale + gap) * sl.appear } else { sl.base_w * sl.appear };
            total += sl.w;
        }
        let content = (total - gap).max(0.0);
        // latar dock (konten + padding kiri/kanan) dipusatkan pada jendela; saat diam posisinya dibulatkan ke piksel utuh
        // agar ikon dan teks tidak jatuh di setengah piksel (buram dan terlihat bergeser)
        let mut left = cx - (content + pl + pr) / 2.0 + pl + shift;
        if shift.abs() < 0.01 {
            left = left.round();
        }
        let mut x = left;
        for sl in self.slots.iter_mut() {
            sl.x = x;
            x += sl.w;
        }
        (left, content)
    }

    fn compute_targets(&mut self) {
        let s = self.s();
        let gap = self.gap();
        let zoom = self.zoom();
        let cx = self.win_w as f32 / 2.0;
        let Some((mx, _)) = self.mouse else {
            for sl in self.slots.iter_mut() {
                sl.tscale = 1.0;
            }
            self.tshift = 0.0;
            return;
        };
        // posisi tanpa zoom (stabil) untuk menghitung jarak
        let total_u: f32 = self.slots.iter().map(|x| x.base_w * x.appear).sum();
        let (pl, pr) = self.edge_pads();
        let start_u = cx - ((total_u - gap).max(0.0) + pl + pr) / 2.0 + pl;
        let range = 2.6 * (s + gap);
        let mut ux = start_u;
        let mut anchor: Option<(usize, f32)> = None;
        for (i, sl) in self.slots.iter_mut().enumerate() {
            let bw = sl.base_w * sl.appear;
            if Self::zoomable(&sl.id) && zoom > 1.0 {
                let center = ux + (bw - gap * sl.appear) / 2.0;
                let t = ((mx - center).abs() / range).min(1.0);
                let bell = 0.5 * (1.0 + (std::f32::consts::PI * t).cos());
                sl.tscale = 1.0 + (zoom - 1.0) * bell;
            } else {
                sl.tscale = 1.0;
            }
            if mx >= ux && mx < ux + bw && bw > 0.0 {
                anchor = Some((i, (mx - ux) / bw));
            }
            ux += bw;
        }
        // jaga titik di bawah kursor tetap di bawah kursor setelah zoom
        if let Some((k, frac)) = anchor {
            let (left0, _) = self.layout(0.0);
            let _ = left0;
            let sl = &self.slots[k];
            let mapped = sl.x + frac * sl.w;
            let limit = (zoom - 1.0) * s * 1.7 + 14.0 * self.ui;
            self.tshift = (mx - mapped).clamp(-limit, limit);
        } else {
            self.tshift = 0.0;
        }
    }

    pub fn set_mouse(&mut self, m: Option<(f32, f32)>) {
        self.mouse = m;
    }

    /// Satu langkah animasi. true = masih bergerak (perlu frame berikutnya).
    pub fn step(&mut self, dt: f32) -> bool {
        let dt = dt.clamp(0.0, 0.05);
        let sp = self.speed();
        let anim = self.cfg.anim;
        self.compute_targets();
        let mut busy = false;
        let hovering = self.mouse.is_some();
        let rate = if hovering { 26.0 * sp } else { 16.0 * sp };
        for sl in self.slots.iter_mut() {
            if anim {
                sl.scale = smooth(sl.scale, sl.tscale, dt, rate);
                if (sl.scale - sl.tscale).abs() < 0.002 {
                    sl.scale = sl.tscale;
                } else {
                    busy = true;
                }
                // muncul/hilang: kemajuan linear -> easing halus (smootherstep), bukan gerak linear yang kaku
                let step = dt * 3.0 * sp;
                if sl.prog < sl.target {
                    sl.prog = (sl.prog + step).min(sl.target);
                    busy = true;
                } else if sl.prog > sl.target {
                    sl.prog = (sl.prog - step).max(sl.target);
                    busy = true;
                }
                let t = sl.prog.clamp(0.0, 1.0);
                sl.appear = t * t * t * (t * (t * 6.0 - 15.0) + 10.0);
                if sl.bounce >= 0.0 {
                    sl.bounce += dt;
                    if sl.bounce > 2.4 {
                        sl.bounce = -1.0;
                    }
                    busy = true;
                }
            } else {
                sl.scale = sl.tscale;
                sl.appear = sl.target;
                sl.prog = sl.target;
                sl.bounce = -1.0;
            }
        }
        if anim {
            self.shift = smooth(self.shift, self.tshift, dt, 22.0 * sp);
            if (self.shift - self.tshift).abs() < 0.2 {
                self.shift = self.tshift;
            } else {
                busy = true;
            }
        } else {
            self.shift = self.tshift;
        }
        self.slots.retain(|s| !(s.target == 0.0 && s.appear <= 0.001));
        busy
    }

    // ------------------------------------------------------------ hit-test

    fn bg_top(&self) -> f32 {
        self.win_h as f32 - self.shadow_b() - self.bg_h()
    }

    pub fn zone_top(&self) -> f32 {
        self.bg_top() - (self.zoom() - 1.0) * self.s() - self.pad() - 6.0 * self.ui
    }

    pub fn in_zone(&self, mx: f32, my: f32) -> bool {
        let s = self.s();
        let (bx, _, bw, _) = self.bg;
        mx >= bx - 0.6 * s && mx <= bx + bw + 0.6 * s && my >= self.zone_top() && my <= self.win_h as f32
    }

    pub fn hit_test(&self, mx: f32, my: f32) -> Option<usize> {
        if my < self.zone_top() {
            return None;
        }
        let gap = self.gap();
        for (i, sl) in self.slots.iter().enumerate() {
            if sl.appear < 0.5 || matches!(sl.id, SlotId::Sep(_)) {
                continue;
            }
            if mx >= sl.x - gap * 0.5 && mx < sl.x + sl.w - gap * 0.5 {
                return Some(i);
            }
        }
        None
    }

    /// Pusat horizontal slot (koordinat jendela dock).
    pub fn slot_center(&self, idx: usize) -> f32 {
        let sl = &self.slots[idx];
        sl.x + (sl.w - self.gap() * sl.appear) / 2.0
    }

    /// (pusat-x, atas, ukuran) ikon pada koordinat jendela dock.
    pub fn icon_rect(&self, idx: usize) -> (f32, f32, f32) {
        let s = self.s();
        let gap = self.gap();
        let sl = &self.slots[idx];
        let size = s * sl.scale;
        let cx = sl.x + (sl.w - gap * sl.appear) / 2.0;
        let baseline = self.icon_baseline(self.bg_top());
        (cx, baseline - size, size)
    }

    // ------------------------------------------------------------ render

    fn theme(&self) -> Theme {
        let op = self.cfg.opacity as f32 / 100.0;
        let white = Rgba(255, 255, 255, 255);
        let black = Rgba(0, 0, 0, 255);
        let none = Rgba(0, 0, 0, 0);
        let dark_theme = |top: Rgba, bot: Rgba| Theme {
            under: none,
            top: top.mul_a(op),
            bot: bot.mul_a(op),
            border: white.a(0.12),
            hilite: white.a(0.10),
            shadow_a: 0.06,
            fg: white,
            dim: white.a(0.62),
            accent: Rgba(112, 208, 255, 255),
            sep: white.a(0.18),
            text_shadow: false,
            light: false,
        };
        let light_theme = |top: Rgba, bot: Rgba| Theme {
            under: none,
            top: top.mul_a(op),
            bot: bot.mul_a(op),
            border: black.a(0.10),
            hilite: white.a(0.60),
            shadow_a: 0.045,
            fg: Rgba(28, 28, 32, 255),
            dim: Rgba(28, 28, 32, 150),
            accent: Rgba(0, 103, 192, 255),
            sep: black.a(0.14),
            text_shadow: false,
            light: true,
        };
        match self.cfg.style {
            Style::Mac => Theme {
                under: black.a(0.28 * op),
                top: white.a(0.36 * op),
                bot: white.a(0.20 * op),
                border: white.a(0.55 * op.max(0.5)),
                hilite: white.a(0.40),
                shadow_a: 0.08,
                fg: white,
                dim: white.a(0.78),
                accent: Rgba(150, 214, 255, 255),
                sep: white.a(0.35),
                text_shadow: true,
                light: false,
            },
            Style::Dark => dark_theme(Rgba(40, 40, 46, 214), Rgba(24, 24, 28, 224)),
            Style::Light => light_theme(Rgba(252, 252, 254, 224), Rgba(241, 241, 245, 230)),
            Style::Surface => {
                if self.light_sys {
                    light_theme(Rgba(243, 243, 243, 235), Rgba(238, 238, 238, 238))
                } else {
                    dark_theme(Rgba(36, 36, 36, 230), Rgba(30, 30, 30, 235))
                }
            }
            Style::Clear => Theme {
                under: black.a(0.10 * op),
                top: white.a(0.11 * op),
                bot: white.a(0.05 * op),
                border: white.a(0.32),
                hilite: white.a(0.18),
                shadow_a: 0.04,
                fg: white,
                dim: white.a(0.78),
                accent: Rgba(150, 214, 255, 255),
                sep: white.a(0.30),
                text_shadow: true,
                light: false,
            },
        }
    }

    fn text_at(&mut self, cv: &mut Canvas, t: &str, px: i32, bold: bool, x: f32, y: f32, col: Rgba, shadow: bool) {
        let m = self.text.get(t, px, bold);
        if shadow {
            cv.draw_mask(m, x.round() as i32, y.round() as i32 + 1, Rgba(0, 0, 0, 120));
        }
        cv.draw_mask(m, x.round() as i32, y.round() as i32, col);
    }

    pub fn render(&mut self, cv: &mut Canvas) {
        let th = self.theme();
        let s = self.s();
        let gap = self.gap();
        let ui = self.ui;
        let (pl, pr) = self.edge_pads();
        let (left, content) = self.layout(self.shift);
        let bg_h = self.bg_h();
        let bx = left - pl;
        let bw = content + pl + pr;
        let by = self.bg_top();
        self.bg = (bx, by, bw, bg_h);
        let r = (self.cfg.radius as f32 * ui).min(bg_h / 2.0);

        // latar (bayangan lembut + kaca + tepi + kilap) disusun dari potongan kiri | tengah | kanan.
        // Bagian tengah identik di sepanjang lebar, jadi saat dock melebar/menyempit (animasi) bayangan tidak digambar ulang.
        let (ix, fl) = (bx.floor(), bx - bx.floor());
        let rx = bx + bw;
        let (ir, fr) = (rx.floor(), rx - rx.floor());
        let m = (15.0 * ui + 3.0).ceil() as i32;
        let cap = m + r.ceil() as i32 + 3;
        let wr = 2 * cap + 4;
        let real_w = (ir - ix) as i32 + 2 * m;
        if real_w < wr || cv.h <= 0 {
            cv.clear();
            paint_bg(cv, bx, by, bw, bg_h, r, ui, &th);
        } else {
            let mut key: u64 = 0xcbf29ce484222325;
            for v in [cv.h as u32, by.to_bits(), bg_h.to_bits(), r.to_bits(), ui.to_bits(), th.shadow_a.to_bits(), (fl * 16.0).round() as u32, (fr * 16.0).round() as u32] {
                key = (key ^ v as u64).wrapping_mul(0x100000001b3);
            }
            for c in [th.top, th.bot, th.under, th.border, th.hilite] {
                key = (key ^ u32::from_le_bytes([c.0, c.1, c.2, c.3]) as u64).wrapping_mul(0x100000001b3);
            }
            let h = cv.h as usize;
            let capu = cap as usize;
            if self.bg_key != key || self.bg_cache.len() != 2 * capu * h + h {
                let mut tmp = vec![0u32; wr as usize * h];
                {
                    let mut rc = Canvas { w: wr, h: cv.h, px: &mut tmp };
                    let rbx = m as f32 + fl;
                    let rbw = (wr - m) as f32 + fr - rbx;
                    paint_bg(&mut rc, rbx, by, rbw, bg_h, r, ui, &th);
                }
                self.bg_cache.clear();
                self.bg_cache.resize(2 * capu * h + h, 0);
                for y in 0..h {
                    let row = &tmp[y * wr as usize..(y + 1) * wr as usize];
                    self.bg_cache[y * capu..(y + 1) * capu].copy_from_slice(&row[..capu]);
                    self.bg_cache[capu * h + y * capu..capu * h + (y + 1) * capu].copy_from_slice(&row[wr as usize - capu..]);
                    self.bg_cache[2 * capu * h + y] = row[capu + 1];
                }
                self.bg_key = key;
            }
            cv.clear();
            let (x_l, x_r) = (ix as i32 - m, ir as i32 + m);
            let cw = cv.w;
            for y in 0..h {
                let row = &mut cv.px[y * cw as usize..(y + 1) * cw as usize];
                let put = |row: &mut [u32], x0: i32, src: &[u32]| {
                    for (i, v) in src.iter().enumerate() {
                        let x = x0 + i as i32;
                        if x >= 0 && x < cw {
                            row[x as usize] = *v;
                        }
                    }
                };
                put(row, x_l, &self.bg_cache[y * capu..(y + 1) * capu]);
                put(row, x_r - cap, &self.bg_cache[capu * h + y * capu..capu * h + (y + 1) * capu]);
                let (a, b) = ((x_l + cap).clamp(0, cw), (x_r - cap).clamp(0, cw));
                if b > a {
                    row[a as usize..b as usize].fill(self.bg_cache[2 * capu * h + y]);
                }
            }
        }

        // area klik nyaris-transparan
        cv.fill_hit((bx - 4.0) as i32, (by - 2.0) as i32, (bw + 8.0) as i32, (bg_h + 4.0) as i32);
        if self.mouse.is_some() {
            let zt = self.zone_top();
            cv.fill_hit((bx - 0.6 * s) as i32, zt as i32, (bw + 1.2 * s) as i32, (self.win_h as f32 - zt) as i32);
        }

        let baseline = self.icon_baseline(by);
        let vpad = self.pad();
        let cy = by + bg_h / 2.0;
        let hover = self.mouse.and_then(|(mx, my)| self.hit_test(mx, my));
        let n = self.slots.len();
        let mut label: Option<(String, f32, f32)> = None;

        for i in 0..n {
            let (id, appear, sx, sw, scale, bounce) = {
                let sl = &self.slots[i];
                (sl.id.clone(), sl.appear, sl.x, sl.w, sl.scale, sl.bounce)
            };
            if appear <= 0.001 {
                continue;
            }
            let vis_w = sw - gap * appear;
            let mid = sx + vis_w / 2.0;
            let hovered = hover == Some(i);
            let size = s * scale * appear;
            let boff = if bounce >= 0.0 { -((bounce * 10.0).sin().abs() * (1.0 - bounce / 2.4)) * s * 0.5 } else { 0.0 };
            let icon_top = baseline - size + boff;
            match &id {
                SlotId::Start => {
                    // ubin sama persis dengan ikon aplikasi (ukuran penuh, sudut membulat, pas piksel): tepi kiri ikon Start
                    // sejajar dengan tepi kanan modul paling kanan, jadi dock terlihat simetris saat diam
                    let settled = (scale - 1.0).abs() < 0.004 && appear > 0.995 && bounce < 0.0;
                    let (tx, ty, ts_) = if settled {
                        let sp = s.round();
                        ((mid - sp / 2.0).round(), (baseline - sp).round(), sp)
                    } else {
                        (mid - size / 2.0, icon_top, size)
                    };
                    let inset = 0.5;
                    let (tw_, rr) = (ts_ - 2.0 * inset, ts_ * 0.215);
                    // warna latar ubin dipilih di Pengaturan (Terang / Gelap), terpisah dari tema dock
                    let dark_tile = self.cfg.start_bg == 1;
                    let (top, bot, edge, shine) = if dark_tile {
                        (Rgba(74, 76, 84, 255), Rgba(26, 27, 32, 255), Rgba(255, 255, 255, 44), Rgba(255, 255, 255, 80))
                    } else if th.light {
                        (Rgba(255, 255, 255, 255), Rgba(228, 236, 248, 255), Rgba(0, 0, 0, 26), Rgba(255, 255, 255, 190))
                    } else {
                        (Rgba(250, 252, 255, 255), Rgba(214, 226, 246, 255), Rgba(0, 0, 0, 30), Rgba(255, 255, 255, 190))
                    };
                    cv.fill_rrect(tx + inset, ty + inset, tw_, tw_, rr, top.a(appear), bot.a(appear));
                    cv.stroke_rrect(tx + inset, ty + inset, tw_, tw_, rr, (ts_ / 128.0).max(1.0), edge.a(appear));
                    cv.fill_rrect(tx + ts_ * 0.10, ty + inset + 1.0, ts_ * 0.80, 1.0, 0.5, shine, Rgba(shine.0, shine.1, shine.2, 0));
                    // logo Windows 11: empat persegi biru dengan celah sempit, tepat di tengah ubin
                    let logo = (ts_ * 0.46).round().max(8.0);
                    let g = (logo * 0.09).round().max(1.0);
                    let t = ((logo - g) / 2.0).floor();
                    let full = 2.0 * t + g;
                    let lx = (tx + (ts_ - full) / 2.0).round();
                    let ly = (ty + (ts_ - full) / 2.0).round();
                    let cols = if dark_tile {
                        [Rgba(96, 205, 255, 255), Rgba(64, 176, 255, 255), Rgba(41, 151, 255, 255), Rgba(10, 132, 255, 255)]
                    } else {
                        [Rgba(0, 164, 239, 255), Rgba(0, 144, 232, 255), Rgba(0, 120, 212, 255), Rgba(0, 96, 190, 255)]
                    };
                    let rr_ = (t * 0.10).max(0.6);
                    for k in 0..4 {
                        let (ci, cj) = ((k % 2) as f32, (k / 2) as f32);
                        let c = cols[k].a(appear);
                        cv.fill_rrect(lx + ci * (t + g), ly + cj * (t + g), t, t, rr_, c, c);
                    }
                    if hovered {
                        label = Some(("Start".into(), mid, icon_top));
                    }
                }
                SlotId::App(key) => {
                    if let Some(app) = self.apps.iter().find(|a| &a.key == key) {
                        // diam: ukuran & posisi bulat + mip tepat -> tepi ikon tajam (tidak bergerigi)
                        let settled = (scale - 1.0).abs() < 0.004 && appear > 0.995 && bounce < 0.0;
                        if settled {
                            let sp = s.round();
                            app.icon.set_rest(sp as i32);
                            cv.draw_icon(&app.icon, (mid - sp / 2.0).round(), (baseline - sp).round(), sp, 1.0);
                        } else {
                            cv.draw_icon(&app.icon, mid - size / 2.0, icon_top, size, appear);
                        }
                        if self.cfg.indicator && !app.hwnds.is_empty() && appear > 0.5 {
                            let rad = (s * 0.045).max(1.6 * ui);
                            let fg = app.hwnds.contains(&self.fg_hwnd);
                            let col = if fg { th.fg.a(0.95) } else { th.fg.a(0.55) };
                            cv.fill_circle(mid, by + bg_h - vpad * 0.46, rad, col);
                        }
                        if hovered {
                            label = Some((app.label(), mid, icon_top));
                        }
                    }
                }
                SlotId::Sep(_) => {
                    let h = s * 0.80;
                    cv.fill_rrect(mid - 0.5 * ui.max(1.0), cy - h / 2.0, ui.max(1.0), h, 0.5, th.sep, th.sep);
                }
                _ => {
                    // modul sistem (salat, jam, Wi-Fi, baterai, tray): bila sedang muncul/hilang gambar ke kanvas sementara,
                    // lalu potong (crop) simetris + pudar sesuai kemajuan -> tidak ada teks yang menjuntai keluar dari latar
                    let full = (self.slots[i].base_w - gap).max(1.0);
                    if appear >= 0.995 {
                        label = self.draw_module(cv, &th, &id, sx, cy, vis_w, hovered).map(|(t, off)| (t, mid, cy + off));
                    } else {
                        let (sw_i, sh_i) = (full.ceil() as i32 + 4, bg_h.ceil() as i32 + 2);
                        let mut buf = std::mem::take(&mut self.scratch);
                        buf.clear();
                        buf.resize((sw_i * sh_i) as usize, 0);
                        {
                            let mut scv = Canvas { w: sw_i, h: sh_i, px: &mut buf };
                            let _ = self.draw_module(&mut scv, &th, &id, 2.0, bg_h / 2.0, full, false);
                        }
                        let a8 = (appear.clamp(0.0, 1.0) * 255.0) as u32;
                        let crop = ((full - vis_w).max(0.0) / 2.0).round() as i32;
                        let dx0 = sx.round() as i32 - 2 - crop;
                        let dy0 = by.floor() as i32;
                        let vis_i = vis_w.round() as i32;
                        for yy in 0..sh_i {
                            let dy = dy0 + yy;
                            if dy < 0 || dy >= cv.h {
                                continue;
                            }
                            for xx in (2 + crop)..(2 + crop + vis_i).min(sw_i) {
                                let dx = dx0 + xx;
                                if dx < 0 || dx >= cv.w {
                                    continue;
                                }
                                let sp = buf[(yy * sw_i + xx) as usize];
                                if sp >> 24 != 0 {
                                    let di = (dy * cv.w + dx) as usize;
                                    cv.px[di] = blend_px(cv.px[di], scale_px(sp, a8));
                                }
                            }
                        }
                        self.scratch = buf;
                    }
                }
            }
        }

        // label di atas ikon
        if let Some((txt, mid, top)) = label {
            let mut t: String = txt.chars().take(52).collect();
            if txt.chars().count() > 52 {
                t.push('\u{2026}');
            }
            let px = ((s * 0.26) as i32).max(12);
            let (tw, tht) = {
                let m = self.text.get(&t, px, false);
                (m.w as f32, m.h as f32)
            };
            let (hp, vp) = (11.0 * ui, 4.0 * ui);
            let pw = tw + 2.0 * hp;
            let ph = tht + 2.0 * vp;
            let px_ = (mid - pw / 2.0).clamp(4.0, (self.win_w as f32 - pw - 4.0).max(4.0));
            let py_ = (top - self.cfg.label_gap as f32 * ui - ph).max(2.0);
            cv.fill_rrect(px_, py_, pw, ph, ph * 0.34, Rgba(52, 52, 56, 226), Rgba(40, 40, 44, 232));
            cv.stroke_rrect(px_, py_, pw, ph, ph * 0.34, 1.0, Rgba(255, 255, 255, 30));
            self.text_at(cv, &t, px, false, px_ + hp, py_ + vp - 1.0, Rgba(255, 255, 255, 255), false);
        }
    }

    // ------------------------------------------------------------ modul sistem

    /// Gambar satu modul sistem pada (sx, cy) selebar vis_w. Mengembalikan teks label hover dan offset-y label terhadap cy.
    fn draw_module(&mut self, cv: &mut Canvas, th: &Theme, id: &SlotId, sx: f32, cy: f32, vis_w: f32, hovered: bool) -> Option<(String, f32)> {
        let ui = self.ui;
        let (ig, ts, ph) = (self.ig(), self.ts(), self.ph());
        let (px, wt) = (self.px(1.0), self.wt());
        let fgc = self.tcol(th);
        let dimc = fgc.a(0.66);
        let bar_top = -(self.bg_h() / 2.0) - 4.0 * ui;
        let card_top = -ph / 2.0 - 3.0 * ui;
        let mid = sx + vis_w / 2.0;
        let ico = self.ico();
        let gp = self.gpx();
        let hover_pill = |cv: &mut Canvas, a: f32| {
            cv.fill_rrect(sx, cy - ph / 2.0, vis_w, ph, ph / 2.0, th.fg.a(a), th.fg.a(a));
        };
        // isi selalu rata tengah pada slot: jarak antar isi = jarak antar modul (sama untuk semua modul)
        let content = self.content_w(id);
        let x0 = sx + (vis_w - content) / 2.0;
        // modul satu ikon: glif Windows 11 bila font ada, selain itu ikon vektor
        let single = |this: &mut Dock, cv: &mut Canvas, glyph: char, gsz: i32, col: Rgba, fallback: &dyn Fn(&mut Canvas, f32, f32)| {
            if !this.text.draw_glyph(cv, glyph, glyph, gsz, mid, cy, col) {
                fallback(cv, mid, cy);
            }
        };
        match id {
            SlotId::Prayer => {
                let p = self.info.prayer.clone()?;
                let acc = icons::prayer_accent(p.idx);
                let style = self.cfg.prayer_style;
                let hot_col = if p.soon { Rgba(255, 149, 10, 255) } else { acc };
                if self.cfg.prayer_pill {
                    let hv = if hovered { 1.6 } else { 1.0 };
                    cv.fill_rrect(sx, cy - ph / 2.0, vis_w, ph, ph / 2.0, acc.a(0.20 * hv), acc.a(0.09 * hv));
                    cv.stroke_rrect(sx, cy - ph / 2.0, vis_w, ph, ph / 2.0, (1.0 * ui).max(1.0), if p.soon { hot_col.a(0.95) } else { acc.a(0.36) });
                } else if hovered {
                    hover_pill(cv, 0.13);
                }
                let gs = self.gs();
                icons::prayer_color(cv, p.idx, x0 + gs / 2.0, cy, gs, th.light);
                let mut x = x0 + gs;
                // (ikon) (nama singkat) (jam) (titik tengah) (hitung mundur), tanpa tanda kurung
                if style != 2 {
                    x += ig;
                    let code = prayer_code(p.idx);
                    let (wc, hc) = self.tsz(code, px, wt);
                    self.tdraw(cv, code, px, wt, x, cy - hc / 2.0, dimc);
                    x += wc + ts;
                    let (wtm, htm) = self.tsz(&p.time, px, wt);
                    self.tdraw(cv, &p.time, px, wt, x, cy - htm / 2.0, fgc);
                    x += wtm;
                }
                if style == 0 {
                    x += ts;
                    // titik tengah digambar sebagai lingkaran kecil agar selalu terlihat jelas di semua font dan warna teks
                    cv.fill_circle(x + self.dot_w() / 2.0, cy, self.dot_w() / 2.0, fgc.a(0.78));
                    x += self.dot_w() + ts;
                } else if style == 2 {
                    x += ig;
                }
                if style != 1 {
                    let (_, hh) = self.tsz(&p.hms, px, wt);
                    self.tdraw(cv, &p.hms, px, wt, x, cy - hh / 2.0, if p.soon { hot_col } else { fgc });
                }
                if hovered {
                    return Some((format!("{} {} \u{b7} {} lagi \u{b7} {}", p.name, p.time, p.hms, p.loc), card_top));
                }
                None
            }
            SlotId::Clock => {
                let (t, day, date) = (self.info.time.clone(), self.info.day.clone(), self.info.date.clone());
                let (wd, hd) = self.tsz(&day, px, wt);
                let (wdt, hdt) = self.tsz(&date, px, wt);
                let (_, ht) = self.tsz(&t, px, wt);
                self.tdraw(cv, &day, px, wt, x0, cy - hd / 2.0, dimc);
                self.tdraw(cv, &date, px, wt, x0 + wd + ts, cy - hdt / 2.0, fgc);
                self.tdraw(cv, &t, px, wt, x0 + wd + ts + wdt + ts, cy - ht / 2.0, fgc);
                if hovered {
                    return Some(("Kalender dan notifikasi (klik)".into(), bar_top));
                }
                None
            }
            SlotId::Wifi => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                let (kind, lit, up) = (self.info.net_kind, self.info.wifi_lit, self.info.wifi);
                let t = if !up { "Jaringan terputus (klik: detail)" } else if kind == 2 { "Ethernet terhubung (klik: detail)" } else { "Wi-Fi terhubung (klik: detail)" };
                let dim = fgc.a(0.30);
                let drawn = if !up {
                    self.text.draw_glyph(cv, '\u{F384}', '\u{E701}', gp, mid, cy, fgc.a(0.92))
                } else if kind == 2 {
                    self.text.draw_glyph(cv, '\u{E839}', '\u{E839}', gp, mid, cy, fgc)
                } else {
                    // Wi-Fi penuh yang redup di bawah, lalu glif kekuatan sinyal di atasnya (seperti taskbar Windows 11)
                    let base = self.text.draw_glyph(cv, '\u{E701}', '\u{E701}', gp, mid, cy, dim);
                    if base {
                        let ch = ['\u{E872}', '\u{E873}', '\u{E874}', '\u{E701}'][(lit as usize).min(3)];
                        self.text.draw_glyph(cv, ch, '\u{E701}', gp, mid, cy, fgc);
                    }
                    base
                };
                if !drawn {
                    if !up {
                        icons::wifi_off(cv, mid, cy, ico, fgc.a(0.85), dim);
                    } else if kind == 2 {
                        icons::ethernet(cv, mid, cy, ico, fgc);
                    } else {
                        icons::wifi(cv, mid, cy, ico, lit, fgc, dim);
                    }
                }
                if hovered {
                    return Some((t.to_string(), bar_top));
                }
                None
            }
            SlotId::Battery => {
                let (pct, chg) = self.info.battery?;
                if hovered {
                    hover_pill(cv, 0.13);
                }
                let txt = format!("{}%", pct);
                let (_, ht) = self.tsz(&txt, px, wt);
                // warna isyarat: hijau saat mengisi, merah <= 15%, oranye <= 30%, selain itu warna teks dock
                let col = if chg { Rgba(52, 211, 94, 255) } else if pct <= 15 { Rgba(255, 69, 58, 255) } else if pct <= 30 { Rgba(255, 159, 10, 255) } else { fgc };
                let mut x = x0;
                if self.cfg.show_battery {
                    let iw = self.bat_icon_w();
                    let tenth = ((pct as u32 + 5) / 10).min(10);
                    let ch = if tenth >= 10 {
                        if chg { '\u{EA93}' } else { '\u{E83F}' }
                    } else {
                        char::from_u32((if chg { 0xE85A } else { 0xE850 }) + tenth).unwrap_or('\u{E83F}')
                    };
                    if !self.text.draw_glyph(cv, ch, '\u{E83F}', gp, x + iw / 2.0, cy, col) {
                        let fill = if chg || pct >= 95 { Rgba(52, 211, 94, 255) } else if pct <= 15 { Rgba(255, 69, 58, 255) } else if pct <= 30 { Rgba(255, 159, 10, 255) } else { fgc };
                        icons::battery(cv, x + iw / 2.0, cy, ico * 1.2, pct, chg, false, fill, fgc);
                    }
                    x += iw + ig * 0.85;
                }
                if self.cfg.show_battery_pct {
                    self.tdraw(cv, &txt, px, wt, x, cy - ht / 2.0, fgc);
                }
                if hovered {
                    return Some((format!("Baterai {}%{} (klik: detail)", pct, if chg { ", mengisi" } else { "" }), bar_top));
                }
                None
            }
            SlotId::Tray => {
                let hv = if hovered { 0.20 } else { 0.10 };
                hover_pill(cv, hv);
                single(self, cv, '\u{E70E}', ((gp as f32 * 0.86).round() as i32).max(10), fgc, &|cv, x, y| icons::chevron_up(cv, x, y, ico * 0.80, fgc));
                if self.info.tray_count > 0 {
                    let rr = (ico * 0.10).max(1.8);
                    cv.fill_circle(sx + vis_w - ph * 0.20, cy - ph * 0.30, rr, th.accent);
                }
                if hovered {
                    let t = if self.info.tray_count > 0 { format!("Ikon tray ({}) - klik untuk membuka", self.info.tray_count) } else { "Ikon tray (klik)".to_string() };
                    return Some((t, bar_top));
                }
                None
            }
            SlotId::Bin => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                let full = self.info.bin_full;
                single(self, cv, '\u{E74D}', gp, fgc, &|cv, x, y| icons::trash(cv, x, y, ico * 1.05, fgc, full));
                if hovered { Some(("Recycle Bin (klik: buka, klik kanan: menu)".into(), bar_top)) } else { None }
            }
            SlotId::Explorer => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                icons::folder_color(cv, mid, cy, ico * 1.1);
                if hovered { Some(("File Explorer (klik kanan: folder cepat)".into(), bar_top)) } else { None }
            }
            SlotId::Power => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                single(self, cv, '\u{E7E8}', gp, fgc, &|cv, x, y| icons::power(cv, x, y, ico * 0.95, fgc));
                if hovered { Some(("Daya: kunci, tidur, mulai ulang, matikan".into(), bar_top)) } else { None }
            }
            SlotId::Control => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                single(self, cv, '\u{E9E9}', gp, fgc, &|cv, x, y| icons::sliders(cv, x, y, ico, fgc));
                if hovered { Some(("Pusat kontrol".into(), bar_top)) } else { None }
            }
            SlotId::Music => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                let playing = self.info.music.as_ref().map(|m| m.playing).unwrap_or(false);
                let has = self.info.music.is_some();
                let ic = if playing { th.accent } else if has { fgc } else { fgc.a(0.55) };
                let mw = ico * 0.95;
                if !self.text.draw_glyph(cv, '\u{E8D6}', '\u{E8D6}', gp, x0 + mw / 2.0, cy, ic) {
                    icons::music_note(cv, x0 + mw / 2.0, cy, mw, ic);
                }
                let lab = self.music_label();
                if !lab.is_empty() {
                    let (_, ht) = self.tsz(&lab, px, wt);
                    self.tdraw(cv, &lab, px, wt, x0 + mw + ig, cy - ht / 2.0, fgc);
                }
                if hovered {
                    let t = if has { if playing { "Musik: sedang diputar (klik: kontrol)" } else { "Musik: dijeda (klik: kontrol)" } } else { "Musik: tidak ada media (klik: kontrol)" };
                    return Some((t.into(), bar_top));
                }
                None
            }
            SlotId::Weather => {
                if hovered {
                    hover_pill(cv, 0.13);
                }
                let gs = self.gs();
                let (class, day) = match &self.info.weather {
                    Some(wi) => (icons::weather_class(wi.code), wi.day),
                    None => (2, true),
                };
                icons::weather_glyph(cv, class, day, x0 + gs / 2.0, cy, gs, th.light);
                let t = self.weather_text();
                let (_, ht) = self.tsz(&t, px, wt);
                self.tdraw(cv, &t, px, wt, x0 + gs + ig, cy - ht / 2.0, fgc);
                if hovered {
                    let lab = match &self.info.weather {
                        Some(wi) => format!("{}, {}\u{b0}C (klik: detail)", icons::weather_text(wi.code), wi.temp),
                        None => "Cuaca: memuat data (klik: detail)".to_string(),
                    };
                    return Some((lab, bar_top));
                }
                None
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------ banner (salat, Caps Lock, clipboard, aplikasi, baterai, jaringan)

    /// Ukuran banner menurut jenisnya: Caps/Num Lock, clipboard, baterai, dan jaringan dibuat kecil dan ringkas;
    /// salat dan notifikasi aplikasi sedikit lebih besar karena membawa dua baris teks.
    fn notif_spec(&self, kind: u8) -> NotifSpec {
        let u = self.ui;
        let px = |v: f32, min: i32| ((v * u).round() as i32).max(min);
        if matches!(kind, 0 | 3) {
            NotifSpec { h: 50.0 * u, chip: 30.0 * u, p1: px(13.0, 11), p2: px(11.0, 9), p3: px(11.0, 9), r: 15.0 * u, pad: 10.0 * u, min_w: 208.0 * u, max_w: 340.0 * u }
        } else {
            NotifSpec { h: 38.0 * u, chip: 22.0 * u, p1: px(12.0, 10), p2: px(10.0, 9), p3: px(10.0, 9), r: 12.0 * u, pad: 8.0 * u, min_w: 150.0 * u, max_w: 280.0 * u }
        }
    }

    /// Ukuran kartu banner (px). `pill` kosong = tanpa kapsul di kanan.
    pub fn notif_size(&mut self, kind: u8, title: &str, sub: &str, pill: &str) -> (f32, f32) {
        let u = self.ui;
        let sp = self.notif_spec(kind);
        let semi = semi_weight();
        let w1 = self.tsz(title, sp.p1, semi).0;
        let w2 = if sub.is_empty() { 0.0 } else { self.tsz(sub, sp.p2, 400).0 };
        let w3 = if pill.is_empty() { 0.0 } else { self.tsz(pill, sp.p3, semi).0 + 16.0 * u + 10.0 * u };
        let w = sp.pad + sp.chip + 10.0 * u + w1.max(w2) + w3 + 12.0 * u;
        (w.clamp(sp.min_w, sp.max_w), sp.h)
    }

    /// Gambar kartu banner di (x,y) berukuran (w,h); `kind`: 0 salat, 1 Caps/Num Lock, 2 clipboard, 3 notifikasi aplikasi, 4 baterai, 5 jaringan.
    pub fn render_notif(&mut self, cv: &mut Canvas, title: &str, sub: &str, pill: &str, kind: u8, idx: usize, x: f32, y: f32, w: f32, h: f32, alpha: f32) {
        let th = self.theme();
        let u = self.ui;
        let sp = self.notif_spec(kind);
        let semi = semi_weight();
        let (top, bot, fg, dim) = if th.light {
            (Rgba(252, 252, 254, 255), Rgba(240, 241, 246, 255), Rgba(24, 24, 28, 255), Rgba(24, 24, 28, 150))
        } else {
            (Rgba(54, 54, 59, 255), Rgba(42, 42, 46, 255), Rgba(255, 255, 255, 255), Rgba(235, 235, 245, 153))
        };
        let r = sp.r;
        for i in (1..=3).rev() {
            let e = i as f32 * 2.5 * u;
            let a = Rgba(0, 0, 0, if th.light { 12 } else { 22 });
            cv.fill_rrect(x - e, y - e + 4.0 * u, w + 2.0 * e, h + 2.0 * e, r + e, a, a);
        }
        cv.fill_rrect(x, y, w, h, r, top, bot);
        cv.stroke_rrect(x, y, w, h, r, (1.0 * u).max(1.0), fg.a(if th.light { 0.10 } else { 0.15 }));
        cv.fill_rrect(x + r * 0.7, y + (1.0 * u).max(1.0), w - r * 1.4, 1.0, 0.5, Rgba(255, 255, 255, if th.light { 120 } else { 36 }), Rgba(255, 255, 255, 0));
        let chip = sp.chip;
        let (ccx, ccy) = (x + sp.pad + chip / 2.0, y + h / 2.0);
        if kind == 0 {
            icons::prayer_color(cv, idx, ccx, ccy, chip * 1.0, th.light);
        } else {
            // ikon aplikasi ala macOS: squircle berwarna dengan glif putih
            let (c1, c2) = match kind {
                1 => (Rgba(152, 152, 158, 255), Rgba(110, 110, 116, 255)),
                2 => (Rgba(255, 170, 40, 255), Rgba(255, 140, 10, 255)),
                3 => (Rgba(255, 86, 76, 255), Rgba(240, 52, 44, 255)),
                4 => (Rgba(62, 218, 100, 255), Rgba(40, 190, 78, 255)),
                _ => (Rgba(40, 150, 255, 255), Rgba(10, 118, 240, 255)),
            };
            cv.fill_rrect(ccx - chip / 2.0, ccy - chip / 2.0, chip, chip, chip * 0.26, c1, c2);
            let wh = Rgba(255, 255, 255, 255);
            let g = chip * 0.62;
            match kind {
                1 => icons::keycap(cv, ccx, ccy, g, wh, title.contains("aktif")),
                2 => icons::clipboard(cv, ccx, ccy, g, wh),
                3 => icons::bell(cv, ccx, ccy, g, wh),
                4 => icons::battery_mono(cv, ccx, ccy, g * 1.1, 60, title.contains("terhubung"), wh),
                _ => {
                    if idx == 1 {
                        icons::wifi(cv, ccx, ccy, g, 3, wh, wh.mul_a(0.35));
                    } else {
                        icons::wifi_off(cv, ccx, ccy, g, wh, wh.mul_a(0.35));
                    }
                }
            }
        }
        let tx = x + sp.pad + chip + 10.0 * u;
        let (_, h1) = self.tsz(title, sp.p1, semi);
        let (_, h2) = self.tsz(sub, sp.p2, 400);
        let has_sub = !sub.is_empty();
        let block = if has_sub { h1 + h2 - 3.0 * u } else { h1 };
        let ty = y + (h - block) / 2.0;
        // batasi lebar teks agar tidak menabrak kapsul
        let pill_w = if pill.is_empty() { 0.0 } else { self.tsz(pill, sp.p3, semi).0 + 16.0 * u + 10.0 * u };
        let maxw = w - (tx - x) - pill_w - 12.0 * u;
        let clip = |this: &mut Dock, t: &str, px: i32, wt: i32| -> String {
            let mut s = t.to_string();
            while this.tsz(&s, px, wt).0 > maxw && s.chars().count() > 3 {
                s.pop();
            }
            if s != t {
                s.push('\u{2026}');
            }
            s
        };
        let title_c = clip(self, title, sp.p1, semi);
        self.tdraw(cv, &title_c, sp.p1, semi, tx, ty, fg);
        if has_sub {
            let sub_c = clip(self, sub, sp.p2, 400);
            self.tdraw(cv, &sub_c, sp.p2, 400, tx, ty + h1 - 3.0 * u, dim);
        }
        if !pill.is_empty() {
            // keterangan waktu/status di kanan: teks redup polos seperti banner notifikasi macOS
            let (w3, h3) = self.tsz(pill, sp.p3, 400);
            self.tdraw(cv, pill, sp.p3, 400, x + w - 12.0 * u - w3, y + (h - h3) / 2.0, dim);
        }
        // pudarkan seluruh area bila alpha < 1
        if alpha < 0.999 {
            let a8 = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
            for p in cv.px.iter_mut() {
                if *p != 0 {
                    *p = scale_px(*p, a8);
                }
            }
        }
    }
}

struct NotifSpec {
    h: f32,
    chip: f32,
    p1: i32,
    p2: i32,
    p3: i32,
    r: f32,
    pad: f32,
    min_w: f32,
    max_w: f32,
}
