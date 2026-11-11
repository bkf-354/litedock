#![windows_subsystem = "windows"]
//! LiteDock - dock ringan ala macOS untuk Windows 11 (Rust native, tanpa WebView/Electron).

mod config;
mod dock;
mod flyout;
mod genie;
mod gfx;
mod gfxcore;
mod icons;
mod macui;
mod media;
mod system;
mod volume;
mod weather;
mod location;
mod prayer;
mod settings;
mod shell;
mod sysctl;
mod toast;
mod tray;

use config::Config;
use dock::*;
use genie::Genie;
use gfx::{Icon, Image, Surface};
use shell::wide;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::*;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Controls::{InitCommonControlsEx, ICC_BAR_CLASSES, INITCOMMONCONTROLSEX, WM_MOUSELEAVE};
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

const WM_FRAME: u32 = WM_APP + 1;
const WM_SHOWSETTINGS: u32 = WM_APP + 2;
const WM_ABAR: u32 = WM_APP + 3;
const WM_TRAYICON: u32 = WM_APP + 4;

const T_REFRESH: usize = 1;
const T_MINUTE: usize = 2;
const T_PEEK: usize = 3;
const T_TRIM: usize = 4;
const T_ABAR: usize = 5;
const T_GENIE_END: usize = 6;
const T_FS: usize = 7;
const T_FSPOLL: usize = 8;
const T_LOC: usize = 9;
const T_NOTIF: usize = 10;
const T_ICON: usize = 13;
const T_NET: usize = 17;
const T_SAVE: usize = 18;
const T_TRAY: usize = 16;
const T_SEC: usize = 19;
const T_CAPS: usize = 21;
const T_TBGUARD: usize = 22;
const T_DEMO: usize = 23;
const WM_MEDIA: u32 = WM_APP + 7;
const WM_WEATHER: u32 = WM_APP + 8;
const WM_TOAST: u32 = WM_APP + 9;
static BIN_FULL: AtomicBool = AtomicBool::new(false);
const T_EDGE: usize = 15;
const T_DBG1: usize = 11;
const T_DBG2: usize = 12;
const T_DBG3: usize = 14;
const WM_LOCATION: u32 = WM_APP + 5;
const WM_NETSNAP: u32 = WM_APP + 6;
static NET_THREAD: std::sync::OnceLock<std::thread::Thread> = std::sync::OnceLock::new();

/// Bangunkan thread jaringan agar segera menghitung ulang (mis. setelah Wi-Fi dinyalakan).
fn net_kick() {
    if let Some(t) = NET_THREAD.get() {
        t.unpark();
    }
}

/// Thread latar: panggilan WinRT status jaringan (bisa puluhan ms) tidak boleh berjalan di thread UI.
fn start_net_thread(hwnd: isize) {
    let h = std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        loop {
            shell::net_snap_store(shell::net_status());
            BIN_FULL.store(system::bin_status().1, Ordering::Relaxed);
            unsafe {
                let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_NETSNAP, WPARAM(0), LPARAM(0));
            }
            std::thread::park_timeout(Duration::from_secs(5));
        }
    });
    let _ = NET_THREAD.set(h.thread().clone());
}

static FRAME_RUN: AtomicBool = AtomicBool::new(false);
/// dock sedang beranimasi (dibaca jendela pengaturan untuk menghindari repaint berat bersamaan)
pub static DOCK_ANIM: AtomicBool = AtomicBool::new(false);
static FRAME_PENDING: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicBool = AtomicBool::new(true);
static FRAME_HWND: AtomicIsize = AtomicIsize::new(0);
static FRAME_THREAD: OnceLock<std::thread::Thread> = OnceLock::new();
static DETECTING: AtomicBool = AtomicBool::new(false);
static TB_HIDDEN: AtomicBool = AtomicBool::new(false);
static TB_PREV: AtomicI64 = AtomicI64::new(-1);

static mut APP: *mut App = std::ptr::null_mut();

fn app() -> &'static mut App {
    unsafe { &mut *APP }
}

fn app_ready() -> bool {
    unsafe { !APP.is_null() }
}

pub fn current_config() -> Config {
    app().cfg.clone()
}
pub fn apply_from_ui(c: Config) {
    app().apply_config(c);
}
pub fn quit_app() {
    app().quit();
}
pub fn open_settings_tab(tab: usize) {
    let a = app();
    settings::open_at(&a.cfg, a.ui, tab);
}
pub fn detect_now() {
    app().detect_location();
}
pub fn tray_changed() {
    if app_ready() {
        app().on_tray_changed();
    }
}
pub fn test_banner() {
    if app_ready() {
        app().show_banner(3, 0, "WhatsApp \u{b7} Budi".to_string(), "Contoh pesan masuk dari aplikasi lain".to_string(), "WhatsApp".to_string(), 4500);
    }
}
pub fn test_notification() {
    app().test_notification();
}

fn request_frame() {
    DIRTY.store(true, Ordering::SeqCst);
    FRAME_RUN.store(true, Ordering::SeqCst);
    if let Some(t) = FRAME_THREAD.get() {
        t.unpark();
    }
}

static CRUMB: std::sync::Mutex<&'static str> = std::sync::Mutex::new("awal");

fn crumb(s: &'static str) {
    if let Ok(mut g) = CRUMB.try_lock() {
        *g = s;
    }
}

fn crumb_get() -> &'static str {
    CRUMB.try_lock().map(|g| *g).unwrap_or("?")
}

/// Catat kejadian fatal/panic ke %APPDATA%\LiteDock\crash.txt (untuk diagnosa).
fn crash_log(msg: &str) {
    use std::io::Write;
    let _ = std::fs::create_dir_all(config::dir());
    let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(config::dir().join("crash.txt")) {
        let _ = writeln!(f, "[{}] v{} {}", ts, env!("CARGO_PKG_VERSION"), msg);
    }
}

/// Jalankan `f`; bila panic, catat dan kembalikan `default` (aplikasi tetap hidup).
fn guarded<R>(default: R, name: &'static str, f: impl FnOnce() -> R) -> R {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => {
            crash_log(&format!("panic ditangkap di {} | terakhir: {}", name, crumb_get()));
            default
        }
    }
}

unsafe extern "system" fn seh_filter(ep: *const windows::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS) -> i32 {
    let (code, addr) = if !ep.is_null() && !(*ep).ExceptionRecord.is_null() { ((*(*ep).ExceptionRecord).ExceptionCode.0, (*(*ep).ExceptionRecord).ExceptionAddress as usize) } else { (0, 0) };
    crash_log(&format!("EXCEPTION {:#x} di alamat {:#x} | terakhir: {}", code, addr, crumb_get()));
    if TB_HIDDEN.load(Ordering::SeqCst) {
        shell::restore_taskbar(TB_PREV.load(Ordering::SeqCst));
    }
    1
}

const DAYS: [&str; 7] = ["Min", "Sen", "Sel", "Rab", "Kam", "Jum", "Sab"];
const DAYS_FULL: [&str; 7] = ["Minggu", "Senin", "Selasa", "Rabu", "Kamis", "Jumat", "Sabtu"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "Mei", "Jun", "Jul", "Agu", "Sep", "Okt", "Nov", "Des"];

struct Notif {
    win: HWND,
    surf: Surface,
    title: String,
    sub: String,
    time: String,
    idx: usize,
    /// 0 salat, 1 Caps/Num Lock, 2 clipboard, 3 notifikasi aplikasi, 4 baterai, 5 jaringan
    kind: u8,
    ms: u32,
    phase: u8,
    t0: Instant,
    x: i32,
    y: i32,
    cx: f32,
    cyf: f32,
    cw: f32,
    ch: f32,
    travel: f32,
}

fn ease_out_back(t: f32) -> f32 {
    let c1 = 1.0;
    let c3 = c1 + 1.0;
    1.0 + c3 * (t - 1.0).powi(3) + c1 * (t - 1.0).powi(2)
}

struct App {
    cfg: Config,
    dock: Dock,
    dock_hwnd: HWND,
    abar_hwnd: HWND,
    surf: Option<Surface>,
    icons: HashMap<String, Rc<Icon>>,
    run_order: Vec<String>,
    ui: f32,
    mon: RECT,
    win_x: i32,
    win_y: i32,
    anchor_bottom: i32,
    reserved: i32,
    abar_reg: bool,
    hidden_fs: bool,
    last_frame: Instant,
    genie: Option<Genie>,
    genie_cache: HashMap<isize, Image>,
    pending_overlay: Option<(HWND, isize)>,
    tracking: bool,
    modal: bool,
    down_slot: Option<usize>,
    tb_msg: u32,
    tb_hidden: bool,
    tb_wins: Vec<isize>,
    peeking: bool,
    refresh_pending: bool,
    known: Vec<isize>,
    hooks: Vec<HWINEVENTHOOK>,
    dock_was_busy: bool,
    notif: Option<Notif>,
    notified: Vec<(i64, usize)>,
    seeded: bool,
    wifi_prev: bool,
    in_refresh: bool,
    refresh_again: bool,
    icon_queue: Vec<(String, String, bool, Option<isize>, String, String)>,
    in_icon_job: bool,
    edge_reveal: bool,
    edge_since: Option<Instant>,
    leave_since: Option<Instant>,
    /// autohide: kemajuan linear 0..1 (0 tampil, 1 tersembunyi) dan versi easing-nya
    hide_p: f32,
    hide_t: f32,
    /// jendela dock sedang ditampilkan (false = SW_HIDE)
    hide_vis: bool,
    /// true bila yang menyembunyikan adalah aplikasi layar penuh (toast salat tidak boleh tampil)
    fs_active: bool,
    /// abaikan siaran TaskbarCreated buatan sendiri sampai waktu ini
    tb_self_until: Option<Instant>,
    /// jam-desimal pada tick detik terakhir (mendeteksi pergantian hari)
    tick_h: f64,
    /// status toggle tombol (bit0 Caps, bit1 Num) terakhir
    keys: u8,
    keys_init: bool,
    /// selama waktu ini taskbar asli yang muncul (mis. saat menu Start dibuka) langsung disembunyikan lagi
    tb_guard_until: Option<Instant>,
    last_bat: Option<(u8, bool)>,
    last_net: Option<bool>,
}

fn pin_key(p: &str) -> String {
    let l = p.to_lowercase();
    match l.strip_prefix("shell:appsfolder\\") {
        Some(r) => format!("aumid:{}", r),
        None => l,
    }
}

fn boost_priority(on: bool) {
    use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL, THREAD_PRIORITY_NORMAL};
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), if on { THREAD_PRIORITY_ABOVE_NORMAL } else { THREAD_PRIORITY_NORMAL });
    }
}

fn dpi_scale() -> f32 {
    unsafe {
        let m = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let (mut dx, mut dy) = (96u32, 96u32);
        if GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).is_ok() && dx > 0 {
            return dx as f32 / 96.0;
        }
        1.0
    }
}

/// Hitung mundur jj:mm:dd, status 'segera' (<= 15 menit), dan kemajuan antar salat; dihitung ulang tiap detik.
fn countdown_fields(next_abs: f64, prev_abs: f64, now_h: f64) -> (String, bool, f32) {
    let left = ((next_abs - now_h) * 3600.0).ceil().max(0.0) as i64;
    let hms = format!("{:02}:{:02}:{:02}", left / 3600, (left % 3600) / 60, left % 60);
    let progress = (((now_h - prev_abs) / (next_abs - prev_abs).max(0.01)) as f32).clamp(0.0, 1.0);
    (hms, left <= 15 * 60, progress)
}

/// Ringkasan isi clipboard: (judul, cuplikan). None bila tidak ada isi yang dikenali.
fn clipboard_summary(hwnd: HWND) -> Option<(String, String)> {
    use windows::Win32::System::DataExchange::*;
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    unsafe {
        let mut opened = false;
        for _ in 0..4 {
            if OpenClipboard(Some(hwnd)).is_ok() {
                opened = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(15));
        }
        if !opened {
            return None;
        }
        let mut out = None;
        if IsClipboardFormatAvailable(15).is_ok() {
            // CF_HDROP: berkas
            if let Ok(h) = GetClipboardData(15) {
                let n = windows::Win32::UI::Shell::DragQueryFileW(windows::Win32::UI::Shell::HDROP(h.0), 0xFFFF_FFFF, None);
                out = Some(("Berkas disalin".to_string(), format!("{} berkas", n)));
            }
        } else if IsClipboardFormatAvailable(13).is_ok() {
            // CF_UNICODETEXT
            if let Ok(h) = GetClipboardData(13) {
                let hg = windows::Win32::Foundation::HGLOBAL(h.0);
                let p = GlobalLock(hg) as *const u16;
                if !p.is_null() {
                    let mut v = Vec::new();
                    let mut i = 0;
                    while i < 200 && *p.add(i) != 0 {
                        v.push(*p.add(i));
                        i += 1;
                    }
                    let _ = GlobalUnlock(hg);
                    let t = String::from_utf16_lossy(&v).split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        let t: String = if t.chars().count() > 70 { t.chars().take(69).collect::<String>() + "\u{2026}" } else { t };
                        out = Some(("Teks disalin".to_string(), t));
                    }
                }
            }
        } else if IsClipboardFormatAvailable(8).is_ok() || IsClipboardFormatAvailable(2).is_ok() {
            out = Some(("Gambar disalin".to_string(), "Tersimpan di clipboard".to_string()));
        }
        let _ = CloseClipboard();
        out
    }
}

/// Jendela milik proses menu Start / Pencarian Windows.
fn is_start_host(hv: isize) -> bool {
    let p = shell::process_name_of_window(hv).unwrap_or_default().to_lowercase();
    p.contains("startmenuexperiencehost") || p.contains("searchhost") || p.contains("searchapp") || p.contains("shellhost")
}

fn tz_hours() -> f64 {
    unsafe {
        let mut tz = TIME_ZONE_INFORMATION::default();
        let id = GetTimeZoneInformation(&mut tz);
        let bias = tz.Bias + if id == 2 { tz.DaylightBias } else { tz.StandardBias };
        -(bias as f64) / 60.0
    }
}

impl App {
    fn own_windows(&self) -> Vec<isize> {
        let mut v = vec![self.dock_hwnd.0 as isize, self.abar_hwnd.0 as isize];
        if let Some(h) = settings::is_open() {
            v.push(h.0 as isize);
        }
        if let Some(g) = &self.genie {
            v.push(g.win.0 as isize);
        }
        if let Some(n) = &self.notif {
            v.push(n.win.0 as isize);
        }
        if let Some(h) = flyout::is_open() {
            v.push(h.0 as isize);
        }
        v
    }

    fn mon_w(&self) -> i32 {
        self.mon.right - self.mon.left
    }

    fn invalidate(&mut self) {
        request_frame();
    }

    fn recompute_monitor(&mut self) {
        self.mon = shell::monitor_rect_primary();
        self.ui = dpi_scale();
    }

    // ------------------------------------------------------------ daftar jendela

    fn schedule_refresh(&mut self) {
        if !self.refresh_pending {
            self.refresh_pending = true;
            unsafe {
                SetTimer(Some(self.dock_hwnd), T_REFRESH, 120, None);
            }
        }
    }

    fn icon_for(&mut self, key: &str, path: &str, uwp: bool, hwnd: Option<isize>, aumid: &str, name: &str) -> Rc<Icon> {
        if let Some(i) = self.icons.get(key) {
            return i.clone();
        }
        // aplikasi Store/UWP: ikon asli dimuat belakangan (COM bisa lambat); sementara ubin huruf yang relevan
        if uwp || !aumid.is_empty() {
            let ph = Rc::new(shell::letter_icon(name));
            self.icons.insert(key.to_string(), ph.clone());
            self.icon_queue.push((key.to_string(), path.to_string(), uwp, hwnd, aumid.to_string(), name.to_string()));
            unsafe {
                SetTimer(Some(self.dock_hwnd), T_ICON, 40, None);
            }
            return ph;
        }
        crumb("icon_for");
        let i = Rc::new(shell::load_icon(path, uwp, hwnd, aumid, name));
        self.icons.insert(key.to_string(), i.clone());
        i
    }

    /// Muat satu ikon dari antrean (dijalankan dari timer, di luar proses refresh).
    fn process_icon_job(&mut self) {
        if self.in_icon_job || self.icon_queue.is_empty() {
            return;
        }
        self.in_icon_job = true;
        crumb("icon_job");
        let (key, path, uwp, hwnd, aumid, name) = self.icon_queue.remove(0);
        let icon = Rc::new(shell::load_icon(&path, uwp, hwnd, &aumid, &name));
        self.icons.insert(key.clone(), icon.clone());
        for a in self.dock.apps.iter_mut() {
            if a.key == key {
                a.icon = icon.clone();
            }
        }
        self.in_icon_job = false;
        self.invalidate();
        if !self.icon_queue.is_empty() {
            unsafe {
                SetTimer(Some(self.dock_hwnd), T_ICON, 60, None);
            }
        }
    }

    fn refresh_windows(&mut self) {
        // COM (ikon aplikasi Store) bisa memompa pesan: cegah refresh bersarang sampai stack habis
        if self.in_refresh {
            self.refresh_again = true;
            return;
        }
        self.in_refresh = true;
        crumb("refresh_windows");
        self.refresh_windows_inner();
        self.in_refresh = false;
        if self.refresh_again {
            self.refresh_again = false;
            self.schedule_refresh();
        }
    }

    fn refresh_windows_inner(&mut self) {
        self.refresh_pending = false;
        let own = self.own_windows();
        let wins = shell::enum_windows(&own);
        let pinned_keys: Vec<String> = self.cfg.pinned.iter().map(|p| pin_key(p)).collect();
        let mut running: Vec<String> = Vec::new();
        for w in &wins {
            if !running.contains(&w.key) {
                running.push(w.key.clone());
            }
        }
        self.run_order.retain(|k| running.contains(k) && !pinned_keys.contains(k));
        for k in &running {
            if !pinned_keys.contains(k) && !self.run_order.contains(k) {
                self.run_order.push(k.clone());
            }
        }
        let mut order: Vec<String> = Vec::new();
        for k in &pinned_keys {
            if !order.contains(k) {
                order.push(k.clone());
            }
        }
        order.extend(self.run_order.iter().cloned());
        let mut apps: Vec<AppEntry> = Vec::new();
        for key in order {
            let group: Vec<&shell::WinInfo> = wins.iter().filter(|w| w.key == key).collect();
            let pinned_path = self.cfg.pinned.iter().find(|p| pin_key(p) == key).cloned();
            let pinned = pinned_path.is_some();
            let first = group.first();
            let aumid: String = first
                .map(|g| g.aumid.clone())
                .filter(|a| !a.is_empty())
                .or_else(|| pinned_path.as_ref().and_then(|p| p.get(17..).filter(|_| p.to_lowercase().starts_with("shell:appsfolder\\")).map(|x| x.to_string())))
                .unwrap_or_default();
            let exe = first.map(|g| g.path.clone()).unwrap_or_default();
            // jalur peluncuran: sematan, atau shell:AppsFolder untuk aplikasi paket, atau exe
            let path = pinned_path.clone().unwrap_or_else(|| if !aumid.is_empty() { format!("shell:AppsFolder\\{}", aumid) } else { exe.clone() });
            if group.is_empty() && !path.to_lowercase().starts_with("shell:") && !std::path::Path::new(&path).exists() {
                continue;
            }
            let uwp = first.map(|g| g.uwp).unwrap_or(false);
            let name = if !aumid.is_empty() {
                std::path::Path::new(&exe).file_stem().map(|x| x.to_string_lossy().to_string()).filter(|x| !x.is_empty() && !uwp).unwrap_or_else(|| shell::aumid_display_name(&aumid))
            } else {
                String::new()
            };
            let disp = if !name.is_empty() { name.clone() } else { std::path::Path::new(&path).file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default() };
            let icon_src = if exe.is_empty() { path.clone() } else { exe.clone() };
            let icon = self.icon_for(&key, &icon_src, uwp, first.map(|g| g.hwnd), &aumid, &disp);
            apps.push(AppEntry {
                key,
                path,
                title: first.map(|g| g.title.clone()).unwrap_or_default(),
                hwnds: group.iter().map(|g| g.hwnd).collect(),
                all_minimized: !group.is_empty() && group.iter().all(|g| g.minimized),
                pinned,
                uwp,
                name,
                icon,
            });
        }
        let changed = apps.iter().map(|a| &a.key).ne(self.dock.apps.iter().map(|a| &a.key));
        self.dock.apps = apps;
        self.dock.fg_hwnd = unsafe { GetForegroundWindow().0 as isize };
        self.known = wins.iter().map(|w| w.hwnd).collect();
        self.genie_cache.retain(|h, _| self.known.contains(h));
        if self.icons.len() > 48 {
            let keep: Vec<String> = self.dock.apps.iter().map(|a| a.key.clone()).collect();
            self.icons.retain(|k, _| keep.contains(k));
        }
        if changed {
            self.relayout(false);
        } else {
            self.invalidate();
        }
        self.check_fullscreen();
    }

    // ------------------------------------------------------------ info (jam, salat, baterai, wifi)

    fn prayer_info(&self, st: &windows::Win32::Foundation::SYSTEMTIME) -> PrayerInfo {
        let tz = tz_hours();
        let now_h = st.wHour as f64 + st.wMinute as f64 / 60.0 + st.wSecond as f64 / 3600.0;
        let m = &prayer::METHODS[self.cfg.method.min(prayer::METHODS.len() - 1)];
        let (lat, lon, loc) = self.cfg.eff_loc();
        let n = prayer::next_prayer(st.wYear as i32, st.wMonth as i32, st.wDay as i32, now_h, lat, lon, tz, m, self.cfg.asr_hanafi);
        let ml = n.minutes_left.max(0);
        let left = if ml >= 60 { format!("{}j {}m", ml / 60, ml % 60) } else { format!("{} mnt", ml) };
        // kemajuan dari salat sebelumnya menuju berikutnya
        let (y, mo, d) = (st.wYear as i32, st.wMonth as i32, st.wDay as i32);
        let prev = [0usize, 2, 3, 4, 5].iter().filter(|&&i| n.today[i].is_finite() && n.today[i] <= now_h).map(|&i| n.today[i]).last().unwrap_or_else(|| {
            let (py, pm, pd) = prayer::civil_from_days(prayer::days_from_civil(y, mo, d) - 1);
            prayer::compute(py, pm, pd, lat, lon, tz, m, self.cfg.asr_hanafi)[5] - 24.0
        });
        let next_abs = if n.time > now_h { n.time } else { n.time + 24.0 };
        let (hms, soon, progress) = countdown_fields(next_abs, prev, now_h);
        PrayerInfo {
            idx: n.idx.min(5),
            hms,
            next_abs,
            prev_abs: prev,
            name: prayer::NAMES[n.idx].to_string(),
            time: prayer::fmt_hm(n.time),
            left,
            soon,
            progress,
            loc,
            all: (0..6).map(|i| (prayer::NAMES[i].to_string(), prayer::fmt_hm(n.today[i]))).collect(),
        }
    }

    fn update_info(&mut self) {
        crumb("update_info");
        let st = unsafe { GetLocalTime() };
        let (h, m) = (st.wHour as u32, st.wMinute as u32);
        let time = if self.cfg.clock_24h {
            format!("{:02}:{:02}", h, m)
        } else {
            let h12 = if h % 12 == 0 { 12 } else { h % 12 };
            format!("{}:{:02} {}", h12, m, if h < 12 { "AM" } else { "PM" })
        };
        let day = DAYS[(st.wDayOfWeek as usize) % 7].to_string();
        let date = format!("{:02} {}", st.wDay, MONTHS[((st.wMonth as usize).max(1) - 1) % 12]);
        let net = if self.cfg.show_wifi || self.cfg.auto_loc { shell::net_status_cached() } else { shell::NetStatus { up: false, kind: 0, lit: 0, ssid: String::new() } };
        let wifi_now = net.up;
        if self.cfg.auto_loc && wifi_now && !self.wifi_prev {
            // jaringan baru tersambung: deteksi ulang lokasi sebentar lagi
            unsafe {
                SetTimer(Some(self.dock_hwnd), T_LOC, 6000, None);
            }
        }
        self.wifi_prev = wifi_now;
        let info = Info {
            time,
            day,
            date,
            wifi: wifi_now && self.cfg.show_wifi,
            net_kind: net.kind,
            wifi_lit: net.lit,
            battery: if self.cfg.show_battery || self.cfg.show_battery_pct { shell::battery() } else { None },
            prayer: if self.cfg.show_prayer { Some(self.prayer_info(&st)) } else { None },
            tray_count: tray::count(),
            bin_full: BIN_FULL.load(Ordering::Relaxed),
            music: self.dock.info.music.clone(),
            weather: self.dock.info.weather.clone(),
        };
        let (wlat, wlon, wname) = self.cfg.eff_loc();
        weather::set_coords(wlat, wlon, &wname);
        weather::set_enabled(self.cfg.show_weather);
        media::set_enabled(self.cfg.show_music);
        let layout_changed = info.battery.is_some() != self.dock.info.battery.is_some() || info.prayer.is_some() != self.dock.info.prayer.is_some();
        self.dock.info = info;
        let seed = !self.seeded;
        self.seeded = true;
        self.check_prayer_notify(&st, seed);
        if layout_changed || self.dock.refresh_base() {
            self.relayout(false);
        } else {
            self.invalidate();
        }
        unsafe {
            let ms = (60 - st.wSecond as u32) * 1000 - (st.wMilliseconds as u32).min(999) + 40;
            SetTimer(Some(self.dock_hwnd), T_MINUTE, ms, None);
        }
        self.tick_h = st.wHour as f64 + st.wMinute as f64 / 60.0 + st.wSecond as f64 / 3600.0;
        self.schedule_sec();
    }

    /// Selama beberapa detik, taskbar asli yang muncul (mis. karena menu Start) langsung disembunyikan lagi.
    fn start_tb_guard(&mut self) {
        if !(self.cfg.hide_taskbar && self.tb_hidden) {
            return;
        }
        self.tb_guard_until = Some(Instant::now() + Duration::from_millis(3000));
        unsafe {
            SetTimer(Some(self.dock_hwnd), T_TBGUARD, 20, None);
        }
        self.tb_guard_tick();
    }

    fn tb_guard_tick(&mut self) {
        let active = self.tb_guard_until.map(|t| Instant::now() < t).unwrap_or(false);
        if !active || !self.tb_hidden || self.peeking {
            unsafe {
                let _ = KillTimer(Some(self.dock_hwnd), T_TBGUARD);
            }
            return;
        }
        for t in shell::taskbar_windows() {
            unsafe {
                if IsWindowVisible(t).as_bool() {
                    let _ = ShowWindow(t, SW_HIDE);
                }
            }
        }
    }

    fn on_media(&mut self) {
        let m = media::snapshot().map(|s| dock::MusicInfo { title: s.title, artist: s.artist, playing: s.playing });
        if !self.cfg.show_music || m == self.dock.info.music {
            return;
        }
        self.dock.info.music = m;
        if self.dock.refresh_base() {
            self.relayout(false);
        }
        self.invalidate();
        flyout::data_changed();
    }

    fn on_weather(&mut self) {
        let w = weather::snapshot().map(|w| dock::WeatherInfo { temp: w.temp, code: w.code, day: w.day });
        if !self.cfg.show_weather {
            return;
        }
        self.dock.info.weather = w;
        if self.dock.refresh_base() {
            self.relayout(false);
        }
        self.invalidate();
        flyout::data_changed();
    }

    /// Caps Lock / Num Lock berubah -> banner.
    fn poll_keys(&mut self) {
        if !self.cfg.banner_caps {
            return;
        }
        let (caps, num) = unsafe { (GetKeyState(VK_CAPITAL.0 as i32) & 1 != 0, GetKeyState(VK_NUMLOCK.0 as i32) & 1 != 0) };
        let now = (caps as u8) | ((num as u8) << 1);
        if self.keys_init && now != self.keys {
            let diff = now ^ self.keys;
            if diff & 1 != 0 {
                let (t, sub) = if caps { ("Caps Lock aktif", "Huruf kapital") } else { ("Caps Lock mati", "Huruf kecil") };
                self.show_banner(1, 0, t.to_string(), sub.to_string(), if caps { "ON".into() } else { "OFF".into() }, 2200);
            } else if diff & 2 != 0 {
                let (t, sub) = if num { ("Num Lock aktif", "Angka tombol numerik") } else { ("Num Lock mati", "Navigasi tombol numerik") };
                self.show_banner(1, 0, t.to_string(), sub.to_string(), if num { "ON".into() } else { "OFF".into() }, 2200);
            }
        }
        self.keys = now;
        self.keys_init = true;
    }

    /// Clipboard berubah -> banner ringkas berisi cuplikan.
    fn on_clipboard(&mut self) {
        if !self.cfg.banner_clip {
            return;
        }
        let Some((title, sub)) = clipboard_summary(self.dock_hwnd) else { return };
        self.show_banner(2, 0, title, sub, String::new(), 2600);
    }

    /// Teks toast aplikasi lain (WhatsApp, dll.) hasil pembacaan UI Automation -> banner LiteDock.
    fn on_toast(&mut self, texts: Vec<String>) {
        if !self.cfg.banner_apps || texts.is_empty() {
            return;
        }
        let (app, title, body) = match texts.len() {
            1 => (String::new(), texts[0].clone(), String::new()),
            2 => (texts[0].clone(), texts[1].clone(), String::new()),
            _ => (texts[0].clone(), texts[1].clone(), texts[2..].join(" ")),
        };
        let pill: String = app.chars().take(12).collect();
        self.show_banner(3, 0, title, body, pill, 5500);
    }

    fn want_sec(&self) -> bool {
        self.cfg.show_prayer && self.cfg.prayer_style != 1 && self.dock.info.prayer.is_some()
    }

    /// Timer detik hanya berjalan bila modul hitung mundur tampil.
    fn schedule_sec(&self) {
        unsafe {
            if self.want_sec() {
                let ms = 1000 - GetLocalTime().wMilliseconds as u32 + 12;
                SetTimer(Some(self.dock_hwnd), T_SEC, ms, None);
            } else {
                let _ = KillTimer(Some(self.dock_hwnd), T_SEC);
            }
        }
    }

    fn tick_sec(&mut self) {
        let st = unsafe { GetLocalTime() };
        let now_h = st.wHour as f64 + st.wMinute as f64 / 60.0 + st.wSecond as f64 / 3600.0;
        let wrapped = now_h + 0.5 < self.tick_h;
        self.tick_h = now_h;
        let mut recalc = wrapped;
        let mut changed = false;
        if let Some(p) = self.dock.info.prayer.as_mut() {
            if p.next_abs <= now_h {
                recalc = true;
            } else {
                let (hms, soon, progress) = countdown_fields(p.next_abs, p.prev_abs, now_h);
                if hms != p.hms {
                    p.hms = hms;
                    p.soon = soon;
                    p.progress = progress;
                    changed = true;
                }
            }
        }
        if recalc {
            self.update_info();
            return;
        }
        if changed {
            self.invalidate();
        }
        self.schedule_sec();
    }

    /// Penyegaran ringan tiap beberapa detik: status jaringan (jenis + kekuatan sinyal) dan baterai berubah tanpa menunggu menit berikutnya.
    fn refresh_net(&mut self) {
        let mut changed = false;
        if self.cfg.show_wifi {
            let n = shell::net_status_cached();
            let up = n.up;
            let cur = &self.dock.info;
            if cur.wifi != up || cur.net_kind != n.kind || cur.wifi_lit != n.lit {
                self.dock.info.wifi = up;
                self.dock.info.net_kind = n.kind;
                self.dock.info.wifi_lit = n.lit;
                changed = true;
            }
            if self.cfg.auto_loc && up && !self.wifi_prev {
                unsafe {
                    SetTimer(Some(self.dock_hwnd), T_LOC, 6000, None);
                }
            }
            self.wifi_prev = up;
        }
        if self.cfg.show_battery || self.cfg.show_battery_pct {
            let b = shell::battery();
            if b != self.dock.info.battery {
                let layout = b.is_some() != self.dock.info.battery.is_some();
                self.dock.info.battery = b;
                if layout {
                    self.relayout(false);
                }
                changed = true;
            }
        }
        let tc = tray::count();
        if tc != self.dock.info.tray_count {
            self.dock.info.tray_count = tc;
            changed = true;
        }
        let bf = BIN_FULL.load(Ordering::Relaxed);
        if bf != self.dock.info.bin_full {
            self.dock.info.bin_full = bf;
            changed = true;
        }
        self.system_banners();
        if changed {
            if self.dock.refresh_base() {
                self.relayout(false);
            }
            self.invalidate();
        }
    }

    /// Banner otomatis untuk peristiwa sistem: pengisi daya dipasang/dilepas, baterai lemah, jaringan tersambung/terputus.
    fn system_banners(&mut self) {
        if !self.cfg.banner_sys {
            return;
        }
        if let Some((pct, chg)) = shell::battery() {
            if let Some((lp, lc)) = self.last_bat {
                if chg != lc {
                    let (t, sub) = if chg { ("Pengisi daya terhubung".to_string(), format!("Baterai {}%", pct)) } else { ("Pengisi daya dilepas".to_string(), format!("Baterai {}%", pct)) };
                    self.show_banner(4, 0, t, sub, String::new(), 3500);
                } else if !chg && pct <= 15 && lp > 15 {
                    self.show_banner(4, 0, "Baterai lemah".to_string(), format!("Tersisa {}%, segera isi daya", pct), format!("{}%", pct), 6000);
                }
            }
            self.last_bat = Some((pct, chg));
        }
        let n = shell::net_status_cached();
        if let Some(prev) = self.last_net {
            if prev != n.up {
                let (t, sub) = if n.up { ("Jaringan terhubung".to_string(), if n.ssid.is_empty() { String::new() } else { n.ssid.clone() }) } else { ("Jaringan terputus".to_string(), String::new()) };
                self.show_banner(5, if n.up { 1 } else { 0 }, t, sub, String::new(), 3500);
            }
        }
        // abaikan perubahan pertama (nilai awal belum diketahui)
        if shell::net_snap_ready() {
            self.last_net = Some(n.up);
        }
    }

    fn on_tray_changed(&mut self) {
        let tc = tray::count();
        if tc != self.dock.info.tray_count {
            self.dock.info.tray_count = tc;
            self.invalidate();
        }
        flyout::tray_refresh();
    }

    /// Hidupkan/matikan host tray sesuai pengaturan (hanya saat taskbar asli disembunyikan).
    fn update_tray_host(&mut self) {
        let want = self.cfg.tray_host && self.cfg.hide_taskbar && self.tb_hidden;
        unsafe {
            if want && !tray::is_active() {
                self.tb_self_until = Some(Instant::now() + Duration::from_millis(2500));
                if tray::start() {
                    SetTimer(Some(self.dock_hwnd), T_TRAY, 400, None);
                    self.add_tray_icon();
                }
            } else if !want && tray::is_active() {
                self.tb_self_until = Some(Instant::now() + Duration::from_millis(2500));
                let _ = KillTimer(Some(self.dock_hwnd), T_TRAY);
                tray::stop();
                self.add_tray_icon();
                self.dock.info.tray_count = 0;
            }
        }
        self.update_tray_anchor();
    }

    fn update_tray_anchor(&mut self) {
        if let Some(i) = self.dock.slots.iter().position(|s| s.id == SlotId::Tray) {
            let sl = &self.dock.slots[i];
            let (l, r) = (self.win_x as f32 + sl.x, self.win_x as f32 + sl.x + sl.w);
            let (t, b) = (self.win_y as f32 + self.dock.bg.1, self.win_y as f32 + self.dock.bg.1 + self.dock.bg.3);
            tray::set_anchor(l as i32, t as i32, r as i32, b as i32);
        }
    }

    // ------------------------------------------------------------ notifikasi salat

    fn check_prayer_notify(&mut self, st: &SYSTEMTIME, seed: bool) {
        let tz = tz_hours();
        let (lat, lon, loc) = self.cfg.eff_loc();
        let m = &prayer::METHODS[self.cfg.method.min(prayer::METHODS.len() - 1)];
        let (y, mo, d) = (st.wYear as i32, st.wMonth as i32, st.wDay as i32);
        let times = prayer::compute(y, mo, d, lat, lon, tz, m, self.cfg.asr_hanafi);
        let day = prayer::days_from_civil(y, mo, d);
        let now_min = (st.wHour as i64) * 60 + st.wMinute as i64;
        for &i in &[0usize, 2, 3, 4, 5] {
            if !times[i].is_finite() {
                continue;
            }
            // menit yang sama dengan yang ditampilkan di dock (dibulatkan)
            let pm = ((times[i] * 60.0).round() as i64).rem_euclid(1440);
            let key = (day, i);
            if self.notified.contains(&key) {
                continue;
            }
            if seed {
                if pm <= now_min {
                    self.notified.push(key);
                }
                continue;
            }
            if now_min >= pm && now_min - pm <= 2 {
                self.notified.push(key);
                let t = prayer::fmt_hm(times[i]);
                self.show_prayer_notice(i, &t, &loc, false);
            }
        }
        self.notified.retain(|k| k.0 >= day - 1);
    }

    fn test_notification(&mut self) {
        let st = unsafe { GetLocalTime() };
        let info = self.prayer_info(&st);
        let idx = prayer::NAMES.iter().position(|n| *n == info.name).unwrap_or(2);
        self.show_prayer_notice(idx, &info.time, &info.loc, true);
    }

    fn show_prayer_notice(&mut self, idx: usize, time: &str, loc: &str, test: bool) {
        unsafe {
            if self.cfg.notify_sound && (self.cfg.notify || test) {
                let _ = windows::Win32::System::Diagnostics::Debug::MessageBeep(MB_ICONASTERISK);
            }
        }
        if !self.cfg.notify && !test {
            return;
        }
        let title = format!("{}Waktu {}", if test { "Tes \u{b7} " } else { "" }, prayer::NAMES[idx]);
        let sub = if loc.is_empty() { "Waktunya salat".to_string() } else { loc.to_string() };
        self.show_banner(0, idx, title, sub, time.to_string(), 20000);
    }

    /// Banner serbaguna di atas dock: salat, Caps Lock, clipboard, notifikasi aplikasi, baterai, jaringan.
    fn show_banner(&mut self, kind: u8, idx: usize, title: String, sub: String, pill: String, ms: u32) {
        if self.fs_active {
            return;
        }
        self.destroy_notif();
        let (cw, ch) = self.dock.notif_size(kind, &title, &sub, &pill);
        let ui = self.ui;
        let mx = (24.0 * ui).ceil();
        let top_m = (18.0 * ui).ceil();
        let gap = 10.0 * ui;
        let (bgx, bgy, bgw, bgh) = self.dock.bg;
        let mut center = self.win_x as f32 + bgx + bgw / 2.0;
        if kind == 0 {
            if let Some(sl) = self.dock.slots.iter().find(|s| s.id == SlotId::Prayer) {
                center = self.win_x as f32 + sl.x + sl.w / 2.0;
            }
        }
        let ww = (cw + 2.0 * mx).ceil() as i32;
        let wh = (top_m + ch + gap + bgh * 0.5).ceil() as i32;
        let lo = self.mon.left + 4;
        let hi_ = (self.mon.right - ww - 4).max(lo);
        let wx = ((center - ww as f32 / 2.0) as i32).clamp(lo, hi_);
        let wy = (self.win_y as f32 + bgy - gap - ch - top_m) as i32;
        let Some(surf) = Surface::new(ww, wh) else { return };
        unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
            let win = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE, w!("LiteDockNotif"), w!(""), WS_POPUP, wx, wy, ww, wh, None, None, Some(hi.into()), None).unwrap_or_default();
            if win.0.is_null() {
                return;
            }
            let _ = ShowWindow(win, SW_SHOWNOACTIVATE);
            self.raise_dock();
            self.notif = Some(Notif { win, surf, title, sub, time: pill, idx, kind, ms, phase: 0, t0: Instant::now(), x: wx, y: wy, cx: mx, cyf: top_m, cw, ch, travel: wh as f32 - ch * 0.45 - top_m });
        }
        request_frame();
    }

    fn destroy_notif(&mut self) {
        if let Some(n) = self.notif.take() {
            unsafe {
                let _ = KillTimer(Some(self.dock_hwnd), T_NOTIF);
                let _ = DestroyWindow(n.win);
            }
        }
    }

    fn dismiss_notif(&mut self) {
        if let Some(n) = self.notif.as_mut() {
            if n.phase != 2 {
                n.phase = 2;
                n.t0 = Instant::now();
                unsafe {
                    let _ = KillTimer(Some(self.dock_hwnd), T_NOTIF);
                }
                request_frame();
            }
        }
    }

    /// Satu langkah animasi notifikasi; true bila masih bergerak.
    fn step_notif(&mut self) -> bool {
        let Some(mut n) = self.notif.take() else { return false };
        if n.phase == 1 {
            self.notif = Some(n);
            return false;
        }
        let el = n.t0.elapsed().as_secs_f32();
        let (slide, alpha, done) = if n.phase == 0 {
            let t = (el / 0.55).min(1.0);
            (ease_out_back(t), (t * 2.2).min(1.0), t >= 1.0)
        } else {
            let t = (el / 0.35).min(1.0);
            (1.0 - 0.9 * t * t, 1.0 - t, t >= 1.0)
        };
        {
            let y = n.cyf + (1.0 - slide) * n.travel;
            let mut cv = n.surf.canvas();
            cv.clear();
            self.dock.render_notif(&mut cv, &n.title, &n.sub, &n.time, n.kind, n.idx, n.cx, y, n.cw, n.ch, alpha);
        }
        genie::present(n.win, &n.surf, n.x, n.y);
        if done {
            if n.phase == 2 {
                unsafe {
                    let _ = DestroyWindow(n.win);
                }
                return false;
            }
            n.phase = 1;
            n.t0 = Instant::now();
            unsafe {
                SetTimer(Some(self.dock_hwnd), T_NOTIF, n.ms, None);
            }
            self.notif = Some(n);
            return false;
        }
        self.notif = Some(n);
        true
    }

    // ------------------------------------------------------------ deteksi lokasi otomatis

    fn detect_location(&mut self) {
        if DETECTING.swap(true, Ordering::SeqCst) {
            return;
        }
        settings::set_status("Mendeteksi lokasi\u{2026}");
        let hwnd = self.dock_hwnd.0 as isize;
        let spawned = std::thread::Builder::new().name("geoloc".into()).spawn(move || {
            let r = location::detect();
            let b = Box::into_raw(Box::new(r));
            unsafe {
                if PostMessageW(Some(HWND(hwnd as *mut _)), WM_LOCATION, WPARAM(0), LPARAM(b as isize)).is_err() {
                    drop(Box::from_raw(b));
                }
            }
        });
        if spawned.is_err() {
            DETECTING.store(false, Ordering::SeqCst);
        }
    }

    fn on_location(&mut self, r: Option<location::Detected>) {
        DETECTING.store(false, Ordering::SeqCst);
        match r {
            Some(d) => {
                self.cfg.auto_lat = d.lat;
                self.cfg.auto_lon = d.lon;
                self.cfg.auto_name = d.name.clone();
                self.cfg.auto_ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|x| x.as_secs() as i64).unwrap_or(1).max(1);
                self.cfg.save();
                settings::set_status(&format!("Terdeteksi via {}: {} ({:.3}, {:.3})", d.source, if d.name.is_empty() { "-" } else { &d.name }, d.lat, d.lon));
                if self.cfg.auto_loc {
                    self.update_info();
                }
            }
            None => settings::set_status("Gagal mendeteksi. Periksa koneksi internet atau izin Lokasi Windows."),
        }
        unsafe {
            if self.cfg.auto_loc {
                SetTimer(Some(self.dock_hwnd), T_LOC, 6 * 3600 * 1000, None);
            }
        }
    }

    // ------------------------------------------------------------ layout & render

    fn relayout(&mut self, instant: bool) {
        crumb("relayout");
        self.dock.cfg = self.cfg.clone();
        self.dock.ui = self.ui;
        self.dock.light_sys = shell::system_uses_light_theme();
        self.dock.sync_slots(instant);
        let (w, h) = self.dock.desired_size(self.mon_w());
        self.dock.set_size(w, h);
        self.update_reserve();
        let margin = (self.cfg.margin as f32 * self.ui).round() as i32;
        let sb = self.dock.shadow_b().round() as i32;
        self.win_x = self.mon.left + (self.mon_w() - w) / 2;
        self.win_y = self.anchor_bottom - margin + sb - h;
        self.invalidate();
    }

    fn update_reserve(&mut self) {
        let margin = (self.cfg.margin as f32 * self.ui).round() as i32;
        let want = if self.cfg.reserve_space && self.cfg.hide_mode < 2 { (self.dock.bg_h() + margin as f32 + 4.0 * self.ui).round() as i32 } else { 0 };
        if want != self.reserved {
            if want == 0 {
                if self.abar_reg {
                    shell::appbar_unregister(self.abar_hwnd);
                    self.abar_reg = false;
                }
            } else {
                if !self.abar_reg {
                    shell::appbar_register(self.abar_hwnd, WM_ABAR);
                    self.abar_reg = true;
                }
                let r = shell::appbar_set_bottom(self.abar_hwnd, self.mon, want);
                self.anchor_bottom = r.bottom;
            }
            self.reserved = want;
        }
        if want == 0 {
            // tanpa reservasi: bila taskbar asli tampil, jangan menimpanya
            self.anchor_bottom = if self.cfg.hide_taskbar { self.mon.bottom } else { shell::work_area_bottom(self.mon) };
        } else if !self.abar_reg {
            self.anchor_bottom = self.mon.bottom;
        }
    }

    fn render(&mut self) {
        crumb("render");
        if !self.hide_vis {
            return;
        }
        let (w, h) = (self.dock.win_w, self.dock.win_h);
        if self.surf.as_ref().map(|s| (s.w, s.h)) != Some((w, h)) {
            self.surf = Surface::new(w, h);
        }
        if let Some(s) = self.surf.as_mut() {
            {
                let mut cv = s.canvas();
                self.dock.render(&mut cv);
            }
            // sembunyi otomatis: dock meluncur turun melewati tepi bawah layar (bagian di luar layar dipotong)
            let travel = self.dock.bg_h() + (self.cfg.margin as f32 * self.ui) + 10.0 * self.ui;
            let off = (self.hide_t * travel).round() as i32;
            let top = self.win_y + off;
            let vis_h = (self.mon.bottom - top).clamp(1, h);
            genie::present_rect(self.dock_hwnd, s, (self.win_x, top), (0, 0), (w, vis_h));
        }
    }

    /// Gerak sembunyi/tampil dock (naik/turun dengan easing). true bila masih bergerak.
    fn step_hide(&mut self, dt: f32) -> bool {
        let target = if self.hidden_fs { 1.0 } else { 0.0 };
        if (self.hide_p - target).abs() < 0.0005 && (self.hide_t - target).abs() < 0.0005 {
            self.hide_p = target;
            self.hide_t = target;
            return false;
        }
        if self.cfg.anim {
            let dur = 0.34 / (self.cfg.anim_speed as f32 / 100.0).max(0.3);
            let step = dt / dur;
            if self.hide_p < target {
                self.hide_p = (self.hide_p + step).min(target);
            } else {
                self.hide_p = (self.hide_p - step).max(target);
            }
        } else {
            self.hide_p = target;
        }
        let t = self.hide_p;
        // sembunyi: percepat di akhir; tampil: lambat di akhir (ease in-out kubik)
        self.hide_t = if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 };
        if self.hide_p >= 1.0 && self.hidden_fs {
            self.hide_t = 1.0;
            self.hide_vis = false;
            unsafe {
                let _ = ShowWindow(self.dock_hwnd, SW_HIDE);
            }
            return false;
        }
        true
    }

    fn on_frame(&mut self) {
        crumb("on_frame");
        FRAME_PENDING.store(false, Ordering::SeqCst);
        let now = Instant::now();
        let raw = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        // frame pertama setelah diam (atau macet) memakai dt nominal 1/60 s: animasi mulai dari awal, tidak melompat ke akhir
        let dt = if raw > 0.12 { 1.0 / 60.0 } else { raw.min(0.05) };
        if self.dock.mouse.is_some() && !self.modal {
            unsafe {
                let mut pt = POINT::default();
                if GetCursorPos(&mut pt).is_ok() {
                    let (rx, ry) = ((pt.x - self.win_x) as f32, (pt.y - self.win_y) as f32);
                    if self.dock.in_zone(rx, ry) {
                        self.dock.set_mouse(Some((rx, ry)));
                    } else {
                        self.dock.set_mouse(None);
                    }
                }
            }
        }
        if self.hide_t > 0.001 || self.hidden_fs {
            self.dock.set_mouse(None);
        }
        let hide_busy = self.step_hide(dt);
        let dock_busy = self.dock.step(dt);
        let mut busy = dock_busy || hide_busy;
        DOCK_ANIM.store(busy, Ordering::Relaxed);
        let mut done = false;
        if let Some(g) = self.genie.as_mut() {
            busy = true;
            done = g.frame();
        }
        if done {
            self.finish_genie();
        }
        if self.step_notif() {
            busy = true;
        }
        if flyout::step() {
            busy = true;
        }
        // dock hanya digambar ulang bila berubah (selama genie, dock diam -> hemat CPU)
        let dirty = DIRTY.swap(false, Ordering::SeqCst);
        if dock_busy || hide_busy || self.dock_was_busy || dirty {
            self.render();
        }
        self.dock_was_busy = dock_busy;
        if !busy {
            FRAME_RUN.store(false, Ordering::SeqCst);
            unsafe {
                SetTimer(Some(self.dock_hwnd), T_TRIM, 4000, None);
            }
        }
    }

    fn schedule_fs_check(&self) {
        unsafe {
            SetTimer(Some(self.dock_hwnd), T_FS, 100, None);
        }
    }

    fn check_fullscreen(&mut self) {
        let own = self.own_windows();
        let mode = self.cfg.hide_mode;
        let fs = mode >= 1 && shell::foreground_is_fullscreen(&own, self.mon);
        self.fs_active = fs;
        let mut hide = fs;
        if fs && self.edge_reveal {
            // dimunculkan di atas aplikasi layar penuh: tampil selama `fs_reveal_secs` detik (diatur di Pengaturan > Sistem)
            hide = false;
        }
        if !fs {
            if mode >= 2 {
                let (bx, by, bw, bh) = self.dock.bg;
                let dock = RECT { left: (self.win_x as f32 + bx) as i32, top: (self.win_y as f32 + by) as i32, right: (self.win_x as f32 + bx + bw) as i32, bottom: (self.win_y as f32 + by + bh) as i32 };
                hide = shell::foreground_maximized_or_overlapping(&own, dock);
            }
            if mode == 3 {
                hide = true;
            }
            if hide && self.edge_reveal {
                hide = false;
            }
        }
        if hide != self.hidden_fs {
            self.hidden_fs = hide;
            unsafe {
                if hide {
                    flyout::close_now();
                    self.dock.set_mouse(None);
                } else if !self.hide_vis {
                    // tampilkan jendela dulu (posisi awal di bawah layar), lalu naik
                    self.hide_vis = true;
                    let _ = ShowWindow(self.dock_hwnd, SW_SHOWNA);
                    self.raise_dock();
                }
            }
            self.invalidate();
        }
    }

    /// Mode sembunyi 2/3: tampilkan dock saat kursor menyentuh tepi bawah layar, sembunyikan lagi setelah kursor menjauh.
    fn edge_poll(&mut self) {
        if self.cfg.hide_mode < 1 {
            return;
        }
        // mode 1 (hanya saat layar penuh): sentuhan tepi bawah hanya berlaku saat aplikasi layar penuh aktif
        if self.cfg.hide_mode < 2 && !self.fs_active && !self.edge_reveal {
            self.edge_since = None;
            return;
        }
        let mut pt = POINT::default();
        unsafe {
            if GetCursorPos(&mut pt).is_err() {
                return;
            }
        }
        // zona = jendela dock (berlaku juga saat dock belum pernah digambar karena tersembunyi sejak awal)
        let (l, t, r, b) = (self.win_x as f32 - 10.0 * self.ui, self.win_y as f32 - 10.0 * self.ui, (self.win_x + self.dock.win_w) as f32 + 10.0 * self.ui, (self.win_y + self.dock.win_h) as f32 + 30.0 * self.ui);
        let near = (pt.x as f32) >= l && (pt.x as f32) <= r && (pt.y as f32) >= t && (pt.y as f32) <= b;
        let at_edge = pt.y >= self.mon.bottom - 2 && (pt.x as f32) >= l && (pt.x as f32) <= r;
        if !self.edge_reveal {
            if at_edge {
                let since = *self.edge_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= Duration::from_millis(160) {
                    self.edge_reveal = true;
                    self.leave_since = None;
                    self.edge_since = None;
                    self.check_fullscreen();
                }
            } else {
                self.edge_since = None;
            }
        } else if near || at_edge || flyout::is_open().is_some() {
            self.leave_since = None;
        } else {
            let since = *self.leave_since.get_or_insert_with(Instant::now);
            let hold = if self.fs_active { Duration::from_secs(self.cfg.fs_reveal_secs.clamp(5, 15) as u64) } else { Duration::from_millis(700) };
            if since.elapsed() >= hold {
                self.edge_reveal = false;
                self.leave_since = None;
                self.check_fullscreen();
            }
        }
    }

    fn peek_taskbar_start(&mut self) {
        if self.peeking {
            self.peek_taskbar_end();
            return;
        }
        self.peeking = true;
        if self.tb_hidden {
            // kembalikan taskbar sebenarnya (bukan sekadar menampilkan jendela autohide)
            shell::restore_taskbar(self.cfg.tb_prev);
        } else {
            shell::peek_taskbar(true);
        }
        shell::raise_taskbar_topmost();
        unsafe {
            SetTimer(Some(self.dock_hwnd), T_PEEK, 9000, None);
        }
    }

    fn peek_taskbar_end(&mut self) {
        self.peeking = false;
        unsafe {
            let _ = KillTimer(Some(self.dock_hwnd), T_PEEK);
        }
        if self.cfg.hide_taskbar && self.tb_hidden {
            self.cfg.tb_prev = shell::hide_taskbar(self.cfg.tb_prev);
        }
    }

    // ------------------------------------------------------------ taskbar asli

    fn apply_taskbar_mode(&mut self) {
        self.tb_wins = shell::taskbar_windows().iter().map(|h| h.0 as isize).collect();
        if self.cfg.hide_taskbar && !self.tb_hidden {
            self.cfg.tb_prev = shell::hide_taskbar(self.cfg.tb_prev);
            self.tb_hidden = true;
            TB_PREV.store(self.cfg.tb_prev, Ordering::SeqCst);
            TB_HIDDEN.store(true, Ordering::SeqCst);
            self.cfg.save();
        } else if !self.cfg.hide_taskbar && self.tb_hidden {
            shell::restore_taskbar(self.cfg.tb_prev);
            self.tb_hidden = false;
            TB_HIDDEN.store(false, Ordering::SeqCst);
            self.cfg.tb_prev = -1;
            self.cfg.save();
        }
        unsafe {
            SetTimer(Some(self.dock_hwnd), T_ABAR, 700, None);
        }
    }

    fn apply_config(&mut self, mut c: Config) {
        c.sanitize();
        toast::set_flags(c.banner_apps, c.banner_hide_win, self.ui);
        c.tb_prev = self.cfg.tb_prev;
        c.auto_lat = self.cfg.auto_lat;
        c.auto_lon = self.cfg.auto_lon;
        c.auto_name = self.cfg.auto_name.clone();
        c.auto_ts = self.cfg.auto_ts;
        let old = std::mem::replace(&mut self.cfg, c);
        if old.autostart != self.cfg.autostart {
            shell::set_autostart(self.cfg.autostart);
        }
        let n = &self.cfg;
        unsafe {
            if old.hide_mode != n.hide_mode {
                if n.hide_mode >= 1 {
                    SetTimer(Some(self.dock_hwnd), T_FSPOLL, 1500, None);
                } else {
                    let _ = KillTimer(Some(self.dock_hwnd), T_FSPOLL);
                }
                if n.hide_mode >= 1 {
                    SetTimer(Some(self.dock_hwnd), T_EDGE, 150, None);
                } else {
                    let _ = KillTimer(Some(self.dock_hwnd), T_EDGE);
                    self.edge_reveal = false;
                }
            }
        }
        // kerja berat hanya bila pengaturan terkait benar-benar berubah: menggeser slider / menyalakan modul tetap mulus
        let info_changed = old.show_prayer != n.show_prayer
            || old.show_battery != n.show_battery
            || old.show_battery_pct != n.show_battery_pct
            || old.show_wifi != n.show_wifi
            || old.show_weather != n.show_weather
            || old.show_music != n.show_music
            || old.clock_24h != n.clock_24h
            || old.method != n.method
            || old.asr_hanafi != n.asr_hanafi
            || old.auto_loc != n.auto_loc
            || old.city != n.city
            || old.lat != n.lat
            || old.lon != n.lon;
        let reserve_changed = old.icon_size != n.icon_size
            || old.margin != n.margin
            || old.tray_size != n.tray_size
            || old.reserve_space != n.reserve_space
            || old.hide_mode != n.hide_mode
            || old.hide_taskbar != n.hide_taskbar;
        if old.hide_mode != n.hide_mode {
            self.check_fullscreen();
        }
        if !old.auto_loc && self.cfg.auto_loc {
            self.detect_location();
        } else if old.auto_loc && !self.cfg.auto_loc {
            unsafe {
                let _ = KillTimer(Some(self.dock_hwnd), T_LOC);
            }
        }
        self.save_soon();
        if old.hide_taskbar != self.cfg.hide_taskbar {
            self.apply_taskbar_mode();
        }
        if old.tray_host != self.cfg.tray_host || old.hide_taskbar != self.cfg.hide_taskbar {
            self.update_tray_host();
        }
        if reserve_changed {
            self.reserved = -1;
        }
        if old.font != self.cfg.font {
            gfx::set_font_face(self.cfg.font);
        }
        if old.win_icons != self.cfg.win_icons {
            gfx::set_win_icons(self.cfg.win_icons);
        }
        if old.show_wifi != self.cfg.show_wifi && self.cfg.show_wifi {
            net_kick();
        }
        if info_changed {
            self.update_info();
        }
        self.relayout(false);
        self.schedule_sec();
    }

    /// Simpan konfigurasi dengan jeda singkat (tidak menulis ke disk tiap geseran slider).
    fn save_soon(&mut self) {
        unsafe {
            SetTimer(Some(self.dock_hwnd), T_SAVE, 600, None);
        }
    }

    // ------------------------------------------------------------ interaksi

    fn icon_screen(&self, idx: usize) -> (f32, f32, f32) {
        let (cx, top, size) = self.dock.icon_rect(idx);
        (self.win_x as f32 + cx, self.win_y as f32 + top, size)
    }

    fn create_overlay(&self) -> HWND {
        unsafe {
            let hi = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
            let h = CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                w!("LiteDockGenie"),
                w!(""),
                WS_POPUP,
                0,
                0,
                1,
                1,
                None,
                None,
                Some(hi.into()),
                None,
            )
            .unwrap_or_default();
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
            h
        }
    }

    fn raise_dock(&self) {
        unsafe {
            let _ = SetWindowPos(self.dock_hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    fn start_minimize(&mut self, hwnd: isize, idx: usize) {
        if !self.cfg.anim || self.genie.is_some() {
            shell::minimize(hwnd);
            return;
        }
        let mut rc = RECT::default();
        let h = HWND(hwnd as *mut _);
        unsafe {
            if GetWindowRect(h, &mut rc).is_err() {
                shell::minimize(hwnd);
                return;
            }
        }
        let Some(img) = genie::capture_window(hwnd, rc) else {
            shell::minimize(hwnd);
            return;
        };
        if self.genie_cache.len() >= 4 {
            self.genie_cache.clear();
        }
        self.genie_cache.insert(hwnd, img.half());
        let tgt = self.icon_screen(idx);
        let ov = self.create_overlay();
        match Genie::start(ov, hwnd, true, rc, tgt, img, self.cfg.anim_speed as f32 / 100.0) {
            Some(mut g) => {
                shell::set_transitions_disabled(hwnd, true);
                boost_priority(true);
                g.frame();
                self.raise_dock();
                shell::minimize(hwnd);
                self.genie = Some(g);
                request_frame();
            }
            None => unsafe {
                let _ = DestroyWindow(ov);
                shell::minimize(hwnd);
            },
        }
    }

    fn start_restore(&mut self, hwnd: isize, idx: usize, icon: Rc<Icon>) {
        if !self.cfg.anim || self.genie.is_some() {
            shell::activate(hwnd);
            return;
        }
        let h = HWND(hwnd as *mut _);
        let mut wp = WINDOWPLACEMENT { length: std::mem::size_of::<WINDOWPLACEMENT>() as u32, ..Default::default() };
        unsafe {
            if GetWindowPlacement(h, &mut wp).is_err() {
                shell::activate(hwnd);
                return;
            }
        }
        let rc = if wp.flags.0 & WPF_RESTORETOMAXIMIZED.0 != 0 {
            unsafe {
                let m = MonitorFromWindow(h, MONITOR_DEFAULTTONEAREST);
                let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
                let _ = GetMonitorInfoW(m, &mut mi);
                mi.rcWork
            }
        } else {
            wp.rcNormalPosition
        };
        let (w_, h_) = (rc.right - rc.left, rc.bottom - rc.top);
        let img = self.genie_cache.remove(&hwnd).unwrap_or_else(|| genie::make_ghost(w_.min(1400), h_.min(900), &icon));
        let tgt = self.icon_screen(idx);
        let ov = self.create_overlay();
        match Genie::start(ov, hwnd, false, rc, tgt, img, self.cfg.anim_speed as f32 / 100.0) {
            Some(mut g) => {
                boost_priority(true);
                g.frame();
                self.raise_dock();
                self.genie = Some(g);
                request_frame();
            }
            None => unsafe {
                let _ = DestroyWindow(ov);
                shell::activate(hwnd);
            },
        }
    }

    fn finish_genie(&mut self) {
        let Some(g) = self.genie.take() else { return };
        boost_priority(false);
        unsafe {
            if g.minimize {
                shell::set_transitions_disabled(g.hwnd, false);
                let _ = DestroyWindow(g.win);
            } else {
                shell::set_transitions_disabled(g.hwnd, true);
                shell::activate(g.hwnd);
                self.pending_overlay = Some((g.win, g.hwnd));
                SetTimer(Some(self.dock_hwnd), T_GENIE_END, 130, None);
            }
        }
        self.schedule_refresh();
    }

    fn toggle_flyout(&mut self, idx: usize, kind: flyout::FlyKind) {
        if flyout::current() == Some(kind) {
            flyout::close();
            return;
        }
        let cx = self.win_x as f32 + self.dock.slot_center(idx);
        let top = self.win_y as f32 + self.dock.bg.1;
        let p = self.dock.info.prayer.clone();
        flyout::open(kind, cx, top, self.mon, self.ui, p, &self.cfg);
    }

    fn click(&mut self, idx: usize) {
        let Some(id) = self.dock.slots.get(idx).map(|s| s.id.clone()) else { return };
        if !matches!(id, SlotId::Prayer | SlotId::Wifi | SlotId::Battery | SlotId::Clock | SlotId::Tray | SlotId::Power | SlotId::Control | SlotId::Music | SlotId::Weather) {
            flyout::close();
        }
        match id {
            SlotId::Start => {
                self.start_tb_guard();
                shell::send_win_key()
            }
            SlotId::Prayer => self.toggle_flyout(idx, flyout::FlyKind::Prayer),
            SlotId::Tray => {
                if tray::is_active() {
                    self.update_tray_anchor();
                    self.toggle_flyout(idx, flyout::FlyKind::Tray)
                } else {
                    self.peek_taskbar_start()
                }
            }
            SlotId::Wifi => self.toggle_flyout(idx, flyout::FlyKind::Wifi),
            SlotId::Battery => self.toggle_flyout(idx, flyout::FlyKind::Battery),
            SlotId::Clock => self.toggle_flyout(idx, flyout::FlyKind::Calendar),
            SlotId::Bin => system::bin_open(),
            SlotId::Explorer => system::open_explorer(),
            SlotId::Power => self.toggle_flyout(idx, flyout::FlyKind::Power),
            SlotId::Control => self.toggle_flyout(idx, flyout::FlyKind::Control),
            SlotId::Music => self.toggle_flyout(idx, flyout::FlyKind::Music),
            SlotId::Weather => self.toggle_flyout(idx, flyout::FlyKind::Weather),
            SlotId::App(key) => self.activate_app(&key, idx),
            SlotId::Sep(_) => {}
        }
    }

    /// Menu popup sederhana di posisi kursor; mengembalikan id item yang dipilih (0 = batal).
    fn popup(&self, items: &[(u32, &str, bool)]) -> u32 {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else { return 0 };
            for (id, label, enabled) in items {
                if *id == 0 {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                } else {
                    let wl: Vec<u16> = label.encode_utf16().chain(std::iter::once(0)).collect();
                    let fl = if *enabled { MF_STRING } else { MF_STRING | MF_GRAYED };
                    let _ = AppendMenuW(menu, fl, *id as usize, PCWSTR(wl.as_ptr()));
                }
            }
            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);
            let _ = SetForegroundWindow(self.dock_hwnd);
            let r = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN | TPM_LEFTALIGN, pt.x, pt.y, Some(0), self.dock_hwnd, None);
            let _ = DestroyMenu(menu);
            r.0 as u32
        }
    }

    /// Klik kanan modul: Recycle Bin (buka/kosongkan) dan Explorer (folder cepat). true bila ditangani.
    fn module_menu(&mut self, id: &SlotId) -> bool {
        match id {
            SlotId::Bin => {
                let full = self.dock.info.bin_full;
                match self.popup(&[(1, "Buka Recycle Bin", true), (2, "Kosongkan Recycle Bin", full)]) {
                    1 => system::bin_open(),
                    2 => system::bin_empty(),
                    _ => {}
                }
                true
            }
            SlotId::Explorer => {
                let items = [
                    (1u32, "Beranda", true),
                    (2, "Dokumen", true),
                    (3, "Unduhan", true),
                    (4, "Gambar", true),
                    (5, "Desktop", true),
                    (6, "PC ini", true),
                ];
                let args = ["shell:::{679f85cb-0220-4080-b29b-5540cc05aab6}", "shell:Personal", "shell:Downloads", "shell:My Pictures", "shell:Desktop", "shell:MyComputerFolder"];
                let r = self.popup(&items);
                if r >= 1 && (r as usize) <= args.len() {
                    system::open_shell_folder(args[r as usize - 1]);
                }
                true
            }
            _ => false,
        }
    }

    fn activate_app(&mut self, key: &str, idx: usize) {
        let Some(a) = self.dock.apps.iter().find(|a| a.key == key) else { return };
        let hwnds = a.hwnds.clone();
        let path = a.path.clone();
        let icon = a.icon.clone();
        if hwnds.is_empty() {
            if !path.is_empty() {
                shell::launch(&path);
                if let Some(s) = self.dock.slots.get_mut(idx) {
                    s.bounce = 0.0;
                }
                request_frame();
                self.schedule_refresh();
            }
            return;
        }
        let fg = unsafe { GetForegroundWindow().0 as isize };
        if hwnds.len() == 1 {
            let h = hwnds[0];
            if unsafe { IsIconic(HWND(h as *mut _)).as_bool() } {
                self.start_restore(h, idx, icon);
            } else if fg == h {
                self.start_minimize(h, idx);
            } else {
                shell::activate(h);
            }
        } else if let Some(p) = hwnds.iter().position(|&x| x == fg) {
            shell::activate(hwnds[(p + 1) % hwnds.len()]);
        } else {
            shell::activate(hwnds[0]);
        }
    }

    fn menu_pos(&self) -> POINT {
        let mut pt = POINT::default();
        unsafe {
            let _ = GetCursorPos(&mut pt);
        }
        pt.y = self.win_y + self.dock.bg.1 as i32 - 6;
        pt
    }

    fn context_menu(&mut self, idx: Option<usize>) {
        const M_NEW: usize = 1;
        const M_PIN: usize = 2;
        const M_CLOSE: usize = 3;
        const M_SET: usize = 20;
        const M_PEEK: usize = 21;
        const M_QUIT: usize = 22;
        let entry = idx.and_then(|i| self.dock.slots.get(i)).and_then(|s| match &s.id {
            SlotId::App(k) => self.dock.apps.iter().find(|a| &a.key == k).map(|a| (a.key.clone(), a.path.clone(), a.pinned, a.uwp, a.hwnds.clone())),
            _ => None,
        });
        unsafe {
            let menu = CreatePopupMenu().unwrap_or_default();
            let add = |id: usize, t: &str, fl: MENU_ITEM_FLAGS| {
                let wt = wide(t);
                let _ = AppendMenuW(menu, MF_STRING | fl, id, PCWSTR(wt.as_ptr()));
            };
            if let Some((_, path, pinned, uwp, hw)) = &entry {
                if !path.is_empty() && !*uwp {
                    add(M_NEW, "Buka (jendela baru)", MENU_ITEM_FLAGS(0));
                    add(M_PIN, if *pinned { "Lepas dari dock" } else { "Sematkan di dock" }, MENU_ITEM_FLAGS(0));
                }
                if !hw.is_empty() {
                    add(M_CLOSE, if hw.len() > 1 { "Tutup semua jendela" } else { "Tutup jendela" }, MENU_ITEM_FLAGS(0));
                }
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            }
            add(M_SET, "Pengaturan LiteDock\u{2026}", MENU_ITEM_FLAGS(0));
            add(M_PEEK, "Intip taskbar Windows (8 detik)", MENU_ITEM_FLAGS(0));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            add(M_QUIT, "Keluar dari LiteDock (taskbar dipulihkan)", MENU_ITEM_FLAGS(0));
            let pt = self.menu_pos();
            self.modal = true;
            self.dock.set_mouse(None);
            let _ = SetForegroundWindow(self.dock_hwnd);
            let cmd = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_CENTERALIGN).0, pt.x, pt.y, self.dock_hwnd, None).0 as usize;
            let _ = PostMessageW(Some(self.dock_hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            self.modal = false;
            request_frame();
            match cmd {
                M_NEW => {
                    if let Some((_, path, ..)) = &entry {
                        shell::launch(path);
                        self.schedule_refresh();
                    }
                }
                M_PIN => {
                    if let Some((key, path, pinned, ..)) = &entry {
                        if *pinned {
                            self.cfg.pinned.retain(|p| p.to_lowercase() != *key);
                        } else if !self.cfg.pinned.iter().any(|p| p.to_lowercase() == *key) {
                            self.cfg.pinned.push(path.clone());
                        }
                        self.cfg.save();
                        self.refresh_windows();
                    }
                }
                M_CLOSE => {
                    if let Some((_, _, _, _, hw)) = &entry {
                        for h in hw {
                            shell::close_window(*h);
                        }
                    }
                }
                M_SET => settings::open(&self.cfg, self.ui),
                M_PEEK => self.peek_taskbar_start(),
                M_QUIT => self.quit(),
                _ => {}
            }
        }
    }

    fn prayer_menu(&mut self) {
        let Some(p) = self.dock.info.prayer.clone() else { return };
        unsafe {
            let menu = CreatePopupMenu().unwrap_or_default();
            if !p.loc.is_empty() {
                let t = wide(&format!("Lokasi: {}", p.loc));
                let _ = AppendMenuW(menu, MF_STRING | MF_GRAYED, 99, PCWSTR(t.as_ptr()));
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            }
            for (i, (name, time)) in p.all.iter().enumerate() {
                let t = wide(&format!("{}\t{}", name, time));
                let mut fl = MF_STRING | MF_GRAYED;
                if *name == p.name {
                    fl |= MF_CHECKED;
                }
                let _ = AppendMenuW(menu, fl, 100 + i, PCWSTR(t.as_ptr()));
            }
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let t = wide("Atur lokasi dan metode\u{2026}");
            let _ = AppendMenuW(menu, MF_STRING, 20, PCWSTR(t.as_ptr()));
            let pt = self.menu_pos();
            self.modal = true;
            self.dock.set_mouse(None);
            let _ = SetForegroundWindow(self.dock_hwnd);
            let cmd = TrackPopupMenuEx(menu, (TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_CENTERALIGN).0, pt.x, pt.y, self.dock_hwnd, None).0;
            let _ = PostMessageW(Some(self.dock_hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            let _ = DestroyMenu(menu);
            self.modal = false;
            request_frame();
            if cmd == 20 {
                settings::open(&self.cfg, self.ui);
            }
        }
    }

    fn add_tray_icon(&self) {
        unsafe {
            let mut nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.dock_hwnd, uID: 1, uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP, uCallbackMessage: WM_TRAYICON, ..Default::default() };
            if let Some(i) = shell::icon_tray_handle() {
                nid.hIcon = i;
            }
            for (i, c) in "LiteDock".encode_utf16().enumerate() {
                nid.szTip[i] = c;
            }
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }
    }

    fn quit(&mut self) {
        unsafe {
            for t in [T_REFRESH, T_MINUTE, T_PEEK, T_TRIM, T_ABAR, T_GENIE_END, T_FS, T_FSPOLL, T_LOC, T_NOTIF, T_EDGE, T_NET, T_TRAY, T_SAVE, T_SEC, T_CAPS, T_TBGUARD] {
                let _ = KillTimer(Some(self.dock_hwnd), t);
            }
            // lepas peran host tray lebih dulu: semua aplikasi memasang ulang ikonnya di Explorer
            self.tb_self_until = Some(Instant::now() + Duration::from_millis(2500));
            tray::stop();
            self.cfg.save();
            if self.tb_hidden {
                shell::restore_taskbar(self.cfg.tb_prev);
                self.tb_hidden = false;
                TB_HIDDEN.store(false, Ordering::SeqCst);
                self.cfg.tb_prev = -1;
                self.cfg.save();
            }
            if self.abar_reg {
                shell::appbar_unregister(self.abar_hwnd);
                self.abar_reg = false;
            }
            let nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.dock_hwnd, uID: 1, ..Default::default() };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            for h in self.hooks.drain(..) {
                let _ = UnhookWinEvent(h);
            }
            if let Some(g) = self.genie.take() {
                shell::set_transitions_disabled(g.hwnd, false);
                let _ = DestroyWindow(g.win);
            }
            if let Some(n) = self.notif.take() {
                let _ = DestroyWindow(n.win);
            }
            flyout::close_now();
            let _ = DestroyWindow(self.dock_hwnd);
        }
    }
}

// ---------------------------------------------------------------- WinEvent hook

unsafe extern "system" fn win_event(h: HWINEVENTHOOK, event: u32, hwnd: HWND, id_obj: i32, id_child: i32, t: u32, ms: u32) {
    guarded((), "win_event", || win_event_inner(h, event, hwnd, id_obj, id_child, t, ms));
}

unsafe fn win_event_inner(_h: HWINEVENTHOOK, event: u32, hwnd: HWND, id_obj: i32, id_child: i32, _t: u32, _ms: u32) {
    if !app_ready() || id_obj != OBJID_WINDOW.0 || id_child != 0 || hwnd.0.is_null() {
        return;
    }
    let a = app();
    let hv = hwnd.0 as isize;
    match event {
        EVENT_SYSTEM_FOREGROUND => {
            a.dock.fg_hwnd = hv;
            if a.cfg.hide_taskbar && a.tb_hidden && !a.peeking && is_start_host(hv) {
                // menu Start / Search dibuka (tombol Windows): jaga agar taskbar asli tidak ikut tampil
                a.start_tb_guard();
            }
            a.check_fullscreen();
            a.schedule_refresh();
            request_frame();
        }
        EVENT_SYSTEM_MINIMIZESTART | EVENT_SYSTEM_MINIMIZEEND => a.schedule_refresh(),
        EVENT_OBJECT_LOCATIONCHANGE => {
            if a.tb_wins.contains(&hv) {
                if a.cfg.hide_taskbar && a.tb_hidden && !a.peeking && IsWindowVisible(hwnd).as_bool() {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                }
                return;
            }
            if hv == a.dock.fg_hwnd {
                a.schedule_fs_check();
            }
        }
        EVENT_OBJECT_SHOW | EVENT_OBJECT_CREATE | EVENT_OBJECT_UNCLOAKED => {
            if a.tb_wins.contains(&hv) {
                if (event == EVENT_OBJECT_SHOW || event == EVENT_OBJECT_UNCLOAKED) && a.cfg.hide_taskbar && a.tb_hidden && !a.peeking {
                    let _ = ShowWindow(hwnd, SW_HIDE);
                    a.start_tb_guard();
                }
                return;
            }
            if GetAncestor(hwnd, GA_ROOT) != hwnd {
                return;
            }
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
            if ex & WS_EX_TOOLWINDOW.0 != 0 && ex & WS_EX_APPWINDOW.0 == 0 {
                return;
            }
            a.schedule_refresh();
        }
        EVENT_OBJECT_HIDE | EVENT_OBJECT_DESTROY | EVENT_OBJECT_CLOAKED => {
            if a.known.contains(&hv) {
                a.schedule_refresh();
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------- window procedures

fn lo(l: LPARAM) -> f32 {
    (l.0 & 0xFFFF) as i16 as f32
}
fn hi(l: LPARAM) -> f32 {
    ((l.0 >> 16) & 0xFFFF) as i16 as f32
}

unsafe extern "system" fn dock_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    guarded(LRESULT(0), "dock_proc", || dock_proc_inner(h, msg, wp, lp))
}

unsafe fn dock_proc_inner(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if !app_ready() {
        return DefWindowProcW(h, msg, wp, lp);
    }
    let a = app();
    if a.dock_hwnd.0.is_null() {
        return DefWindowProcW(h, msg, wp, lp);
    }
    if a.tb_msg != 0 && msg == a.tb_msg {
        if a.tb_self_until.map(|t| Instant::now() < t).unwrap_or(false) {
            // siaran TaskbarCreated buatan LiteDock sendiri (mulai/berhenti jadi host tray)
            return LRESULT(0);
        }
        // Explorer dimulai ulang
        a.tb_wins = shell::taskbar_windows().iter().map(|x| x.0 as isize).collect();
        if a.tb_hidden {
            a.cfg.tb_prev = shell::hide_taskbar(a.cfg.tb_prev);
        }
        if tray::is_active() {
            tray::raise_host();
        }
        a.add_tray_icon();
        a.abar_reg = false;
        a.reserved = -1;
        a.relayout(false);
        return LRESULT(0);
    }
    match msg {
        WM_FRAME => {
            a.on_frame();
            LRESULT(0)
        }
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_MOUSEMOVE => {
            if !a.modal {
                a.dock.set_mouse(Some((lo(lp), hi(lp))));
                if !a.tracking {
                    let mut t = TRACKMOUSEEVENT { cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32, dwFlags: TME_LEAVE, hwndTrack: h, dwHoverTime: 0 };
                    let _ = TrackMouseEvent(&mut t);
                    a.tracking = true;
                }
                request_frame();
            }
            LRESULT(0)
        }
        WM_MOUSELEAVE => {
            a.tracking = false;
            if !a.modal {
                a.dock.set_mouse(None);
                request_frame();
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            a.down_slot = a.dock.hit_test(lo(lp), hi(lp));
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let up = a.dock.hit_test(lo(lp), hi(lp));
            if up.is_some() && up == a.down_slot {
                a.click(up.unwrap());
            }
            a.down_slot = None;
            LRESULT(0)
        }
        WM_MBUTTONUP => {
            if let Some(i) = a.dock.hit_test(lo(lp), hi(lp)) {
                if let SlotId::App(k) = a.dock.slots[i].id.clone() {
                    if let Some(p) = a.dock.apps.iter().find(|x| x.key == k).map(|x| x.path.clone()) {
                        if !p.is_empty() {
                            shell::launch(&p);
                            a.schedule_refresh();
                        }
                    }
                }
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            let idx = a.dock.hit_test(lo(lp), hi(lp));
            let sid = idx.and_then(|i| a.dock.slots.get(i)).map(|s| s.id.clone());
            let handled = match sid {
                Some(ref id) => a.module_menu(id),
                None => false,
            };
            if !handled {
                a.context_menu(idx);
            }
            LRESULT(0)
        }
        WM_TRAYICON => {
            let ev = (lp.0 & 0xFFFF) as u32;
            if ev == WM_LBUTTONUP {
                settings::open(&a.cfg, a.ui);
            } else if ev == WM_RBUTTONUP {
                a.context_menu(None);
            }
            LRESULT(0)
        }
        WM_SHOWSETTINGS => {
            settings::open(&a.cfg, a.ui);
            LRESULT(0)
        }
        WM_NETSNAP => {
            a.refresh_net();
            LRESULT(0)
        }
        WM_MEDIA => {
            a.on_media();
            LRESULT(0)
        }
        WM_WEATHER => {
            a.on_weather();
            LRESULT(0)
        }
        WM_TOAST => {
            let b = Box::from_raw(lp.0 as *mut Vec<String>);
            a.on_toast(*b);
            LRESULT(0)
        }
        WM_CLIPBOARDUPDATE => {
            a.on_clipboard();
            LRESULT(0)
        }
        WM_LOCATION => {
            let b = Box::from_raw(lp.0 as *mut Option<location::Detected>);
            a.on_location(*b);
            LRESULT(0)
        }
        WM_TIMER => {
            let id = wp.0;
            match id {
                T_REFRESH => {
                    let _ = KillTimer(Some(h), T_REFRESH);
                    a.refresh_windows();
                }
                T_MINUTE => {
                    let _ = KillTimer(Some(h), T_MINUTE);
                    a.update_info();
                }
                T_PEEK => {
                    let _ = KillTimer(Some(h), T_PEEK);
                    a.peek_taskbar_end();
                }
                T_EDGE => a.edge_poll(),
                T_SEC => a.tick_sec(),
                T_CAPS => a.poll_keys(),
                T_DEMO => {
                    let _ = KillTimer(Some(h), T_DEMO);
                    test_banner();
                }
                T_TBGUARD => a.tb_guard_tick(),
                T_TRAY => {
                    if tray::is_active() {
                        tray::raise_host();
                        tray::prune();
                    }
                }
                T_SAVE => {
                    let _ = KillTimer(Some(h), T_SAVE);
                    a.cfg.save();
                }
                T_TRIM => {
                    let _ = KillTimer(Some(h), T_TRIM);
                    if !FRAME_RUN.load(Ordering::SeqCst) {
                        shell::trim_memory();
                    }
                }
                T_ABAR => {
                    let _ = KillTimer(Some(h), T_ABAR);
                    a.reserved = -1;
                    a.relayout(false);
                }
                T_LOC => {
                    let _ = KillTimer(Some(h), T_LOC);
                    if a.cfg.auto_loc {
                        a.detect_location();
                    }
                }
                T_NOTIF => {
                    let _ = KillTimer(Some(h), T_NOTIF);
                    a.dismiss_notif();
                }
                T_ICON => {
                    let _ = KillTimer(Some(h), T_ICON);
                    a.process_icon_job();
                }
                T_DBG1 => {
                    let _ = KillTimer(Some(h), T_DBG1);
                    settings::open(&a.cfg, a.ui);
                }
                T_DBG3 => {
                    let _ = KillTimer(Some(h), T_DBG3);
                    let args: Vec<String> = std::env::args().collect();
                    let want = if args.iter().any(|x| x == "--flyout-cal") { Some((SlotId::Clock, flyout::FlyKind::Calendar)) } else if args.iter().any(|x| x == "--flyout-prayer") { Some((SlotId::Prayer, flyout::FlyKind::Prayer)) } else if args.iter().any(|x| x == "--flyout-wifi") { Some((SlotId::Wifi, flyout::FlyKind::Wifi)) } else if args.iter().any(|x| x == "--flyout-bat") { Some((SlotId::Battery, flyout::FlyKind::Battery)) } else { None };
                    if let Some((sid, kind)) = want {
                        if let Some(i) = a.dock.slots.iter().position(|s| s.id == sid) {
                            a.toggle_flyout(i, kind);
                        }
                    }
                }
                T_DBG2 => {
                    let _ = KillTimer(Some(h), T_DBG2);
                    a.test_notification();
                }
                T_FS => {
                    let _ = KillTimer(Some(h), T_FS);
                    a.check_fullscreen();
                }
                T_FSPOLL => {
                    a.check_fullscreen();
                }
                T_GENIE_END => {
                    let _ = KillTimer(Some(h), T_GENIE_END);
                    if let Some((win, tw)) = a.pending_overlay.take() {
                        let _ = DestroyWindow(win);
                        shell::set_transitions_disabled(tw, false);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_DISPLAYCHANGE | WM_DPICHANGED => {
            a.recompute_monitor();
            a.tb_wins = shell::taskbar_windows().iter().map(|x| x.0 as isize).collect();
            a.reserved = -1;
            a.relayout(false);
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            a.dock.light_sys = shell::system_uses_light_theme();
            request_frame();
            DefWindowProcW(h, msg, wp, lp)
        }
        WM_POWERBROADCAST | WM_TIMECHANGE => {
            a.update_info();
            LRESULT(1)
        }
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            if wp.0 != 0 && a.tb_hidden {
                shell::restore_taskbar(a.cfg.tb_prev);
                a.cfg.tb_prev = -1;
                a.cfg.save();
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            a.quit();
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

unsafe extern "system" fn abar_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    guarded(LRESULT(0), "abar_proc", || abar_proc_inner(h, msg, wp, lp))
}

unsafe fn abar_proc_inner(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_ABAR && app_ready() {
        let a = app();
        if wp.0 as u32 == ABN_POSCHANGED && !a.dock_hwnd.0.is_null() {
            a.reserved = -1;
            a.relayout(false);
        } else if wp.0 as u32 == ABN_FULLSCREENAPP && !a.dock_hwnd.0.is_null() {
            a.schedule_fs_check();
        }
        return LRESULT(0);
    }
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    DefWindowProcW(h, msg, wp, lp)
}

unsafe extern "system" fn notif_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    guarded(LRESULT(0), "notif_proc", || notif_proc_inner(h, msg, wp, lp))
}

unsafe fn notif_proc_inner(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_LBUTTONUP | WM_RBUTTONUP => {
            if app_ready() {
                app().dismiss_notif();
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

unsafe extern "system" fn plain_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(h, msg, wp, lp)
}

unsafe fn register_class(name: PCWSTR, proc: WNDPROC) {
    let hi = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: proc,
        hInstance: hi.into(),
        hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
        lpszClassName: name,
        ..Default::default()
    };
    RegisterClassExW(&wc);
}


fn dump_raw(path: &str, s: &mut Surface) {
    let mut out = Vec::with_capacity(8 + (s.w * s.h * 4) as usize);
    out.extend_from_slice(&(s.w as u32).to_le_bytes());
    out.extend_from_slice(&(s.h as u32).to_le_bytes());
    for p in s.px().iter() {
        out.extend_from_slice(&p.to_le_bytes());
    }
    let _ = std::fs::write(path, out);
}

fn fake_icon(c1: gfx::Rgba, c2: gfx::Rgba) -> Rc<Icon> {
    let mut img = Image::new(128, 128);
    {
        let mut cv = gfx::Canvas { w: 128, h: 128, px: &mut img.px };
        cv.fill_rrect(6.0, 6.0, 116.0, 116.0, 28.0, c1, c2);
        cv.fill_circle(64.0, 64.0, 24.0, gfx::Rgba(255, 255, 255, 235));
    }
    Rc::new(Icon::from_image(img))
}

/// Uji render tanpa jendela: tulis dock ke berkas mentah untuk diperiksa visual.
fn selftest(prefix: &str) {
    use gfx::Rgba;
    let cols = [
        (Rgba(255, 99, 71, 255), Rgba(200, 40, 40, 255)),
        (Rgba(80, 170, 255, 255), Rgba(20, 90, 220, 255)),
        (Rgba(120, 220, 120, 255), Rgba(30, 150, 70, 255)),
        (Rgba(255, 200, 60, 255), Rgba(230, 140, 20, 255)),
        (Rgba(190, 120, 255, 255), Rgba(110, 50, 200, 255)),
        (Rgba(90, 220, 220, 255), Rgba(20, 150, 160, 255)),
    ];
    for (style, hover) in [(config::Style::Mac, false), (config::Style::Mac, true), (config::Style::Dark, true), (config::Style::Surface, true), (config::Style::Light, true), (config::Style::Clear, true)] {
        let mut cfg = Config::default();
        cfg.style = style;
        let mut d = Dock::new(cfg, 1.0);
        for (i, c) in cols.iter().enumerate() {
            d.apps.push(AppEntry { key: format!("app{}", i), path: format!("C:\\app{}.exe", i), title: format!("Aplikasi contoh {}", i), hwnds: if i % 2 == 0 { vec![i as isize + 1] } else { vec![] }, all_minimized: false, pinned: true, uwp: false, name: String::new(), icon: fake_icon(c.0, c.1) });
        }
        d.info = Info {
            time: "15:42".into(),
            day: "Sab".into(),
            date: "07 Nov".into(),
            wifi: true,
            net_kind: 1,
            wifi_lit: 3,
            tray_count: 3, bin_full: false, music: None, weather: None,
            battery: Some((78, false)),
            prayer: Some(PrayerInfo { idx: 3, hms: "02:14:33".into(), next_abs: 15.0, prev_abs: 12.0, name: "Ashar".into(), time: "14:54".into(), left: "2j 14m".into(), soon: false, progress: 0.55, loc: "Jakarta".into(), all: vec![] }),
        };
        d.sync_slots(true);
        let (w, h) = d.desired_size(1920);
        d.set_size(w, h);
        let Some(mut s) = Surface::new(w, h) else { return };
        {
            let mut cv = s.canvas();
            d.render(&mut cv);
        }
        if hover {
            let idx = d.slots.iter().position(|x| x.id == SlotId::App("app3".into())).unwrap_or(0);
            let (cx, _, _) = d.icon_rect(idx);
            let cy = d.bg.1 + d.bg.3 / 2.0;
            d.set_mouse(Some((cx, cy)));
            for _ in 0..150 {
                d.step(0.016);
            }
        }
        {
            let mut cv = s.canvas();
            d.render(&mut cv);
        }
        dump_raw(&format!("{}_{}_{}.raw", prefix, style.key(), if hover { "hover" } else { "idle" }), &mut s);
    }
    // genie
    let mut img = Image::new(800, 500);
    {
        let mut cv = gfx::Canvas { w: 800, h: 500, px: &mut img.px };
        cv.fill_rrect(0.0, 0.0, 800.0, 500.0, 10.0, Rgba(250, 250, 252, 255), Rgba(235, 238, 245, 255));
        cv.fill_rrect(0.0, 0.0, 800.0, 36.0, 10.0, Rgba(60, 90, 160, 255), Rgba(60, 90, 160, 255));
        cv.fill_circle(400.0, 270.0, 120.0, Rgba(255, 120, 80, 255));
    }
    let src = RECT { left: 300, top: 100, right: 1100, bottom: 600 };
    for (i, q) in [0.25f32, 0.5, 0.75].iter().enumerate() {
        if let Some(mut g) = Genie::start(HWND::default(), 0, true, src, (700.0, 860.0, 90.0), img.clone(), 1.0) {
            let s = g.debug_frame(*q);
            dump_raw(&format!("{}_genie{}.raw", prefix, i), s);
        }
    }
    // kualitas tepi ikon pada skala DPI 150% (ukuran 78 px dan zoom 117 px)
    {
        let mut img = Image::new(256, 256);
        {
            let mut cv = gfx::Canvas { w: 256, h: 256, px: &mut img.px };
            // tile penuh tanpa margin (seperti ikon Store) + bentuk diagonal tajam
            cv.fill_rrect(0.0, 0.0, 256.0, 256.0, 40.0, Rgba(37, 211, 102, 255), Rgba(18, 140, 126, 255));
            cv.line(40.0, 210.0, 210.0, 40.0, 26.0, Rgba(255, 255, 255, 255));
            cv.fill_circle(128.0, 128.0, 52.0, Rgba(255, 255, 255, 240));
        }
        let icon = Icon::from_image(img);
        if let Some(mut s) = Surface::new(420, 140) {
            {
                let mut cv = s.canvas();
                cv.clear();
                cv.fill_rrect(0.0, 0.0, 420.0, 140.0, 8.0, Rgba(60, 70, 110, 255), Rgba(60, 70, 110, 255));
                let mut x = 12.0;
                for sz in [48.0f32, 65.0, 78.0, 90.5, 117.0] {
                    cv.draw_icon(&icon, x, 10.0, sz, 1.0);
                    x += sz + 10.0;
                }
            }
            dump_raw(&format!("{}_iconq.raw", prefix), &mut s);
        }
    }
    // ikon hasil pembesaran blok kasar: bandingkan sebelum/sesudah perbaikan
    {
        let mut small = Image::new(48, 48);
        {
            let mut cv = gfx::Canvas { w: 48, h: 48, px: &mut small.px };
            cv.fill_rrect(3.0, 3.0, 42.0, 42.0, 9.0, Rgba(66, 133, 244, 255), Rgba(25, 80, 200, 255));
            cv.fill_circle(24.0, 24.0, 11.0, Rgba(255, 255, 255, 255));
            cv.line(10.0, 38.0, 38.0, 10.0, 4.0, Rgba(255, 220, 60, 255));
            for p in small.px.iter_mut() {
                if *p >> 24 < 128 { *p = 0 } else { *p |= 0xFF00_0000 }
            }
        }
        let mut big = Image::new(256, 256);
        for y in 0..256 {
            for x in 0..256 {
                big.px[y * 256 + x] = small.px[(y * 48 / 256) * 48 + (x * 48 / 256)];
            }
        }
        let detected = big.undo_nearest_upscale().map(|i| i.w).unwrap_or(0);
        let fixed = big.undo_nearest_upscale().unwrap_or_else(|| big.clone());
        let (ia, ib) = (Icon::from_image(big), Icon::from_image(fixed));
        if let Some(mut sf) = Surface::new(300, 120) {
            {
                let mut cv = sf.canvas();
                cv.clear();
                cv.fill_rrect(0.0, 0.0, 300.0, 120.0, 6.0, Rgba(60, 70, 110, 255), Rgba(60, 70, 110, 255));
                cv.draw_icon(&ia, 12.0, 18.0, 78.0, 1.0);
                cv.draw_icon(&ib, 108.0, 18.0, 78.0, 1.0);
                cv.draw_icon(&ib, 204.0, 6.0, 90.0, 1.0);
            }
            dump_raw(&format!("{}_unblock.raw", prefix), &mut sf);
        }
        let _ = std::fs::write(format!("{}_unblock.txt", prefix), format!("grid terdeteksi: {}", detected));
    }
    // panel detail (flyout)
    {
        let mut cfg = Config::default();
        cfg.style = config::Style::Mac;
        let pi = PrayerInfo {
            idx: 3,
            hms: "02:14:33".into(),
            next_abs: 15.0,
            prev_abs: 12.0, name: "Ashar".into(), time: "14:54".into(), left: "2j 14m".into(), soon: false, progress: 0.55, loc: "Jakarta".into(),
            all: vec![("Subuh".into(), "04:08".into()), ("Terbit".into(), "05:25".into()), ("Dzuhur".into(), "11:38".into()), ("Ashar".into(), "14:54".into()), ("Maghrib".into(), "17:48".into()), ("Isya".into(), "19:00".into())],
        };
        for (kind, name) in [(flyout::FlyKind::Calendar, "cal"), (flyout::FlyKind::Prayer, "prayer"), (flyout::FlyKind::Wifi, "wifi"), (flyout::FlyKind::Battery, "bat"), (flyout::FlyKind::Tray, "tray"), (flyout::FlyKind::Power, "power"), (flyout::FlyKind::Control, "control"), (flyout::FlyKind::Music, "music"), (flyout::FlyKind::Weather, "weather")] {
            if let Some(mut sf) = flyout::selftest_render(kind, &cfg, 1.0, Some(pi.clone())) {
                dump_raw(&format!("{}_fly_{}.raw", prefix, name), &mut sf);
            }
        }
    }
    // jendela pengaturan (uji visual)
    for (style, tab, dd, name) in [(config::Style::Mac, 0usize, false, "mac_t0"), (config::Style::Dark, 3, true, "dark_t3dd"), (config::Style::Light, 3, false, "light_t3"), (config::Style::Surface, 1, false, "surface_t1"), (config::Style::Dark, 2, false, "dark_t2"), (config::Style::Dark, 4, false, "dark_t4"), (config::Style::Dark, 5, false, "dark_t5"), (config::Style::Dark, 6, false, "dark_t6"), (config::Style::Dark, 7, false, "dark_t7"), (config::Style::Dark, 8, false, "dark_t8")] {
        let mut cfg = Config::default();
        cfg.style = style;
        cfg.auto_loc = name == "light_t3";
        if let Some(mut s) = settings::selftest_render(&cfg, tab, 1.0, false, dd) {
            dump_raw(&format!("{}_set_{}.raw", prefix, name), &mut s);
        }
    }
    // varian dock: ukuran ikon, gaya info salat, skala modul (uji visual)
    for (i, (size, ps, ms, ui_)) in [(52, 2, 52, 1.5f32), (52, 0, 52, 1.5), (36, 0, 52, 1.5), (36, 1, 40, 1.0), (52, 0, 72, 1.0)].into_iter().enumerate() {
        let mut cfg = Config::default();
        cfg.icon_size = size;
        cfg.prayer_style = ps;
        cfg.tray_size = ms;
        let mut d = Dock::new(cfg, ui_);
        for (j, c) in [(Rgba(255, 99, 71, 255), Rgba(200, 40, 40, 255)), (Rgba(80, 170, 255, 255), Rgba(20, 90, 220, 255)), (Rgba(120, 220, 120, 255), Rgba(30, 150, 70, 255))].iter().enumerate() {
            d.apps.push(AppEntry { key: format!("v{}", j), path: String::new(), title: format!("App {}", j), hwnds: if j == 0 { vec![1] } else { vec![] }, all_minimized: false, pinned: true, uwp: false, name: String::new(), icon: fake_icon(c.0, c.1) });
        }
        d.info = Info {
            time: "14:22".into(), day: "Rab".into(), date: "04 Nov".into(), wifi: true, net_kind: 1, wifi_lit: 2, tray_count: 0, bin_full: false, music: None, weather: None, battery: Some((100, true)),
            prayer: Some(PrayerInfo { idx: 3, hms: "02:14:33".into(), next_abs: 15.0, prev_abs: 12.0, name: "Ashar".into(), time: "14:54".into(), left: "2j 14m".into(), soon: false, progress: 0.55, loc: "Jakarta".into(), all: vec![] }),
        };
        d.sync_slots(true);
        let (w, h) = d.desired_size(1920);
        d.set_size(w, h);
        if let Some(mut sf) = Surface::new(w, h) {
            {
                let mut cv = sf.canvas();
                d.render(&mut cv);
            }
            dump_raw(&format!("{}_var{}.raw", prefix, i), &mut sf);
        }
    }
    // banner jenis lain: Caps Lock, clipboard, notifikasi aplikasi, baterai, jaringan
    for (k, (kind, title, sub, pill, idx)) in [(1u8, "Caps Lock aktif", "", "ON", 0usize), (2, "Disalin", "Teks - 24 karakter", "", 0), (3, "Budi Santoso", "Bro, jadi rapat jam 3 sore ini?", "WhatsApp", 0), (4, "Pengisi daya terhubung", "Baterai 78%", "", 0), (5, "Wi-Fi tersambung", "Kantor-5G", "", 1), (0, "Waktu Ashar", "Jakarta", "14:54", 3)].into_iter().enumerate() {
        let mut d = Dock::new(Config::default(), 1.0);
        let (cw, ch) = d.notif_size(kind, title, sub, pill);
        if let Some(mut s) = Surface::new((cw + 56.0) as i32, (ch + 60.0) as i32) {
            {
                let mut cv = s.canvas();
                d.render_notif(&mut cv, title, sub, pill, kind, idx, 28.0, 22.0, cw, ch, 1.0);
            }
            dump_raw(&format!("{}_banner{}.raw", prefix, k), &mut s);
        }
    }
    // kartu notifikasi salat
    for (style, name) in [(config::Style::Mac, "mac"), (config::Style::Light, "light"), (config::Style::Surface, "surface")] {
        let mut cfg = Config::default();
        cfg.style = style;
        let mut d = Dock::new(cfg, 1.0);
        let (cw, ch) = d.notif_size(0, "Waktu Ashar", "Jakarta", "14:54");
        if let Some(mut s) = Surface::new((cw + 56.0) as i32, (ch + 60.0) as i32) {
            {
                let mut cv = s.canvas();
                d.render_notif(&mut cv, "Waktu Ashar", "Jakarta", "14:54", 0, 3, 28.0, 22.0, cw, ch, 1.0);
            }
            dump_raw(&format!("{}_notif_{}.raw", prefix, name), &mut s);
        }
    }
    // ukur biaya render genie pada layar penuh 1900x1000 (ms/frame, satu thread bila CPU 1 inti)
    let mut big = Image::new(1900, 1000);
    for (i, p) in big.px.iter_mut().enumerate() {
        *p = 0xFF00_0000 | ((i as u32 * 2654435761) >> 8 & 0x00FF_FFFF);
    }
    let src = RECT { left: 10, top: 20, right: 1910, bottom: 1020 };
    let mut report = String::new();
    if let Some(mut g) = Genie::start(HWND::default(), 0, true, src, (960.0, 1040.0, 80.0), big, 1.0) {
        for q in [0.02f32, 0.15, 0.3, 0.5, 0.7, 0.9] {
            let t0 = Instant::now();
            for _ in 0..5 {
                g.debug_frame(q);
            }
            report.push_str(&format!("q={:.2} {:.2} ms/frame (fast_frames so far {})\n", q, t0.elapsed().as_secs_f64() * 1000.0 / 5.0, g.fast_frames));
        }
    }
    // biaya per frame: dock, pengaturan, panel (ms)
    {
        let mut cfg = Config::default();
        cfg.icon_size = 52;
        let mut d = Dock::new(cfg.clone(), 1.0);
        for (j, c) in [(Rgba(255, 99, 71, 255), Rgba(200, 40, 40, 255)), (Rgba(80, 170, 255, 255), Rgba(20, 90, 220, 255)), (Rgba(120, 220, 120, 255), Rgba(30, 150, 70, 255))].iter().enumerate() {
            d.apps.push(AppEntry { key: format!("t{}", j), path: String::new(), title: format!("App {}", j), hwnds: vec![], all_minimized: false, pinned: true, uwp: false, name: String::new(), icon: fake_icon(c.0, c.1) });
        }
        d.info = Info {
            time: "14:22".into(), day: "Rab".into(), date: "04 Nov".into(), wifi: true, net_kind: 1, wifi_lit: 2, tray_count: 2, bin_full: false, music: None, weather: None, battery: Some((78, false)),
            prayer: Some(PrayerInfo { idx: 3, hms: "02:14:33".into(), next_abs: 15.0, prev_abs: 12.0, name: "Ashar".into(), time: "14:54".into(), left: "2j 14m".into(), soon: false, progress: 0.55, loc: "Jakarta".into(), all: vec![] }),
        };
        d.sync_slots(true);
        let (w, h) = d.desired_size(1920);
        d.set_size(w, h);
        if let Some(mut sf) = Surface::new(w, h) {
            {
                let mut cv = sf.canvas();
                d.render(&mut cv);
            }
            let t0 = Instant::now();
            for _ in 0..40 {
                let mut cv = sf.canvas();
                d.render(&mut cv);
            }
            report.push_str(&format!("dock render {}x{}: {:.2} ms/frame\n", w, h, t0.elapsed().as_secs_f64() * 1000.0 / 40.0));
        }
        let t0 = Instant::now();
        for _ in 0..10 {
            let _ = settings::selftest_render(&cfg, 0, 1.0, false, false);
        }
        report.push_str(&format!("settings draw: {:.2} ms/paint\n", t0.elapsed().as_secs_f64() * 1000.0 / 10.0));
        for tab in [0usize, 2, 5] {
            let (cold, warm) = settings::selftest_time(&cfg, tab, 1.0);
            report.push_str(&format!("settings tab {}: cold {:.2} ms, warm {:.2} ms/paint\n", tab, cold, warm));
        }
        // semua modul aktif, warna teks merah muda, tanpa kapsul salat, jarak 20
        {
            let mut c3 = cfg.clone();
            c3.show_bin = true;
            c3.show_explorer = true;
            c3.show_power = true;
            c3.show_control = true;
            c3.show_music = true;
            c3.show_weather = true;
            c3.prayer_pill = false;
            c3.sys_gap = 20;
            c3.text_color = 0xFF6EB4;
            d.cfg = c3;
            d.info.bin_full = true;
            d.info.music = Some(dock::MusicInfo { title: "Judul Lagu".into(), artist: "Artis".into(), playing: true });
            d.info.weather = Some(dock::WeatherInfo { temp: 31, code: 2, day: true });
            d.refresh_base();
            d.sync_slots(true);
            let (w3, h3) = d.desired_size(1920);
            d.set_size(w3, h3);
            if let Some(mut sf) = Surface::new(w3, h3) {
                {
                    let mut cv = sf.canvas();
                    d.render(&mut cv);
                }
                dump_raw(&format!("{}_allmods.raw", prefix), &mut sf);
            }
            // normal lagi
            d.cfg.text_color = -1;
            d.cfg.prayer_pill = true;
            d.cfg.sys_gap = 14;
            d.cfg.show_bin = false;
            d.cfg.show_explorer = false;
            d.cfg.show_power = false;
            d.cfg.show_control = false;
            d.cfg.show_music = false;
            d.cfg.show_weather = false;
        }
        // ubin glif transparan -> ikon bergaya iOS/macOS
        for (name, col) in [("dark", Rgba(30, 60, 180, 255)), ("light", Rgba(250, 250, 250, 255)), ("red", Rgba(220, 40, 50, 255))] {
            let mut img = Image::new(256, 256);
            {
                let mut cv = gfx::Canvas { w: 256, h: 256, px: &mut img.px };
                cv.fill_poly(&[(70.0, 40.0), (210.0, 128.0), (70.0, 216.0)], col);
                cv.fill_rrect(20.0, 100.0, 70.0, 56.0, 12.0, col, col);
            }
            let tile = img.glyph_tile();
            let mut sf = Surface::new(256, 256).unwrap();
            {
                let mut cv = sf.canvas();
                cv.clear();
                cv.blit_image(tile.as_ref().unwrap_or(&img), 0, 0);
            }
            dump_raw(&format!("{}_glyph_{}.raw", prefix, name), &mut sf);
        }
        // uji font Roboto (reguler dan tebal)
        for (face, bold, name) in [(1, false, "roboto"), (1, true, "roboto_bold"), (0, true, "segoe_bold")] {
            gfx::set_font_face(face);
            let mut c2 = cfg.clone();
            c2.font = face;
            c2.font_bold = bold;
            d.cfg = c2;
            d.refresh_base();
            d.sync_slots(true);
            let (w2, h2) = d.desired_size(1920);
            d.set_size(w2, h2);
            if let Some(mut sf) = Surface::new(w2, h2) {
                {
                    let mut cv = sf.canvas();
                    d.render(&mut cv);
                }
                dump_raw(&format!("{}_{}.raw", prefix, name), &mut sf);
            }
        }
        gfx::set_font_face(0);
        // dock tanpa cache (geometri berubah tiap frame = saat animasi)
        if let Some(mut sf) = Surface::new(w, h) {
            let t0 = Instant::now();
            for i in 0..40 {
                d.bg_key = i;
                let mut cv = sf.canvas();
                d.render(&mut cv);
            }
            report.push_str(&format!("dock render tanpa cache: {:.2} ms/frame\n", t0.elapsed().as_secs_f64() * 1000.0 / 40.0));
        }
        let t0 = Instant::now();
        for _ in 0..10 {
            let _ = flyout::selftest_render(flyout::FlyKind::Wifi, &cfg, 1.0, None);
        }
        report.push_str(&format!("flyout wifi draw: {:.2} ms/paint\n", t0.elapsed().as_secs_f64() * 1000.0 / 10.0));
    }
    let _ = std::fs::write(format!("{}_timing.txt", prefix), report);
}

fn main() {
    std::panic::set_hook(Box::new(|info| {
        crash_log(&format!("PANIC: {} | terakhir: {}", info, crumb_get()));
    }));
    unsafe {
        windows::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter(Some(seh_filter));
    }
    if std::panic::catch_unwind(run).is_err() && TB_HIDDEN.load(Ordering::SeqCst) {
        shell::restore_taskbar(TB_PREV.load(Ordering::SeqCst));
    }
}

fn run() {
    unsafe {
        let args: Vec<String> = std::env::args().collect();
        gfx::register_fonts();
        if let Some(p) = args.iter().position(|a| a == "--selftest") {
            if let Some(prefix) = args.get(p + 1) {
                selftest(prefix);
            }
            return;
        }
        if args.iter().any(|a| a == "--diag") {
            // diagnosa layar penuh: arahkan fokus ke aplikasi yang bermasalah dalam 6 detik
            std::thread::sleep(Duration::from_secs(6));
            let report = shell::fullscreen_report(&[], shell::monitor_rect_primary());
            let _ = std::fs::create_dir_all(config::dir());
            let path = config::dir().join("diag.txt");
            let _ = std::fs::write(&path, report);
            shell::launch(&path.to_string_lossy());
            return;
        }
        if args.iter().any(|a| a == "--restore") {
            // pulihkan taskbar (dipakai installer/uninstaller & pintasan "Pulihkan Taskbar")
            if let Ok(h) = windows::Win32::UI::WindowsAndMessaging::FindWindowW(w!("LiteDockDock"), PCWSTR::null()) {
                let _ = PostMessageW(Some(h), WM_CLOSE, WPARAM(0), LPARAM(0));
                std::thread::sleep(Duration::from_millis(900));
            }
            let mut c = Config::load();
            shell::restore_taskbar(c.tb_prev);
            // bila LiteDock berhenti tiba-tiba saat jadi host tray: minta aplikasi memasang ulang ikon di Explorer
            tray::announce_to_explorer();
            c.tb_prev = -1;
            c.save();
            return;
        }

        // satu instance: bila sudah berjalan, buka jendela pengaturan miliknya lalu keluar
        let _mutex = CreateMutexW(None, true, w!("Local\\LiteDock.SingleInstance.v1"));
        if windows::Win32::Foundation::GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(h) = windows::Win32::UI::WindowsAndMessaging::FindWindowW(w!("LiteDockDock"), PCWSTR::null()) {
                let _ = PostMessageW(Some(h), WM_SHOWSETTINGS, WPARAM(0), LPARAM(0));
            }
            return;
        }

        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        let icc = INITCOMMONCONTROLSEX { dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32, dwICC: ICC_BAR_CLASSES };
        let _ = InitCommonControlsEx(&icc);

        register_class(w!("LiteDockDock"), Some(dock_proc));
        register_class(w!("LiteDockAbar"), Some(abar_proc));
        register_class(w!("LiteDockGenie"), Some(plain_proc));
        register_class(w!("LiteDockNotif"), Some(notif_proc));

        let mut cfg = Config::load();
        gfx::set_font_face(cfg.font);
        gfx::set_win_icons(cfg.win_icons);
        cfg.autostart = shell::autostart_enabled();
        let mon = shell::monitor_rect_primary();
        let ui = dpi_scale();
        let mut a = Box::new(App {
            dock: Dock::new(cfg.clone(), ui),
            cfg,
            dock_hwnd: HWND::default(),
            abar_hwnd: HWND::default(),
            surf: None,
            icons: HashMap::new(),
            run_order: Vec::new(),
            ui,
            mon,
            win_x: 0,
            win_y: 0,
            anchor_bottom: mon.bottom,
            reserved: -1,
            abar_reg: false,
            hidden_fs: false,
            last_frame: Instant::now(),
            genie: None,
            genie_cache: HashMap::new(),
            pending_overlay: None,
            tracking: false,
            modal: false,
            down_slot: None,
            tb_msg: RegisterWindowMessageW(w!("TaskbarCreated")),
            tb_hidden: false,
            tb_wins: Vec::new(),
            peeking: false,
            refresh_pending: false,
            known: Vec::new(),
            hooks: Vec::new(),
            dock_was_busy: false,
            notif: None,
            notified: Vec::new(),
            seeded: false,
            wifi_prev: false,
            in_refresh: false,
            refresh_again: false,
            icon_queue: Vec::new(),
            in_icon_job: false,
            edge_reveal: false,
            edge_since: None,
            leave_since: None,
            hide_p: 0.0,
            hide_t: 0.0,
            hide_vis: false,
            fs_active: false,
            tb_self_until: None,
            tick_h: 0.0,
            keys: 0,
            keys_init: false,
            tb_guard_until: None,
            last_bat: None,
            last_net: None,
        });
        // sisa dari crash sebelumnya: taskbar masih tersembunyi oleh kita
        if a.cfg.tb_prev >= 0 {
            a.tb_hidden = true;
            TB_HIDDEN.store(true, Ordering::SeqCst);
            TB_PREV.store(a.cfg.tb_prev, Ordering::SeqCst);
        }
        APP = Box::into_raw(a);
        let a = app();

        let hi_ = GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        a.abar_hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED | WS_EX_TRANSPARENT,
            w!("LiteDockAbar"),
            w!("LiteDockAbar"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(hi_.into()),
            None,
        )
        .unwrap_or_default();
        let _ = SetLayeredWindowAttributes(a.abar_hwnd, COLORREF(0), 1, LWA_ALPHA);
        let _ = ShowWindow(a.abar_hwnd, SW_SHOWNOACTIVATE);

        let dh = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("LiteDockDock"),
            w!("LiteDock"),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(hi_.into()),
            None,
        )
        .unwrap_or_default();
        a.dock_hwnd = dh;
        FRAME_HWND.store(dh.0 as isize, Ordering::SeqCst);

        // penggerak frame: hanya berjalan saat ada animasi (idle = 0% CPU)
        let th = std::thread::Builder::new().name("frame".into()).stack_size(64 * 1024).spawn(|| loop {
            if !FRAME_RUN.load(Ordering::SeqCst) {
                std::thread::park();
                continue;
            }
            // DwmFlush menunggu vsync; bila DWM nonaktif/sesi remote ia kembali instan -> batasi ~120 fps agar CPU tidak habis
            let t0 = Instant::now();
            let ok = DwmFlush().is_ok();
            let el = t0.elapsed();
            if !ok || el < Duration::from_millis(4) {
                std::thread::sleep(Duration::from_millis(8).saturating_sub(el));
            }
            if FRAME_RUN.load(Ordering::SeqCst) && !FRAME_PENDING.swap(true, Ordering::SeqCst) {
                let h = HWND(FRAME_HWND.load(Ordering::SeqCst) as *mut _);
                let _ = PostMessageW(Some(h), WM_FRAME, WPARAM(0), LPARAM(0));
            }
        });
        if let Ok(t) = th {
            let _ = FRAME_THREAD.set(t.thread().clone());
        }

        a.apply_taskbar_mode();
        a.update_tray_host();
        a.add_tray_icon();
        start_net_thread(a.dock_hwnd.0 as isize);
        if a.cfg.auto_loc {
            SetTimer(Some(a.dock_hwnd), T_LOC, 4000, None);
        }
        if a.cfg.hide_mode >= 1 {
            SetTimer(Some(a.dock_hwnd), T_FSPOLL, 1500, None);
        }
        if a.cfg.hide_mode >= 1 {
            SetTimer(Some(a.dock_hwnd), T_EDGE, 150, None);
        }
        SetTimer(Some(a.dock_hwnd), T_CAPS, 160, None);
        if args.iter().any(|x| x == "--demo-banner") {
            SetTimer(Some(a.dock_hwnd), T_DEMO, 2500, None);
        }
        let _ = windows::Win32::System::DataExchange::AddClipboardFormatListener(a.dock_hwnd);
        media::start(a.dock_hwnd.0 as isize, WM_MEDIA);
        weather::start(a.dock_hwnd.0 as isize, WM_WEATHER);
        if args.iter().any(|x| x == "--open-settings") {
            SetTimer(Some(a.dock_hwnd), T_DBG1, 2500, None);
        }
        if args.iter().any(|x| x.starts_with("--flyout-")) {
            SetTimer(Some(a.dock_hwnd), T_DBG3, 3500, None);
        }
        if args.iter().any(|x| x == "--test-notif") {
            SetTimer(Some(a.dock_hwnd), T_DBG2, 4200, None);
        }
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        for (lo_, hi_e) in [(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND), (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND), (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE), (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_LOCATIONCHANGE), (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED)] {
            let hk = SetWinEventHook(lo_, hi_e, None, Some(win_event), 0, 0, flags);
            if !hk.0.is_null() {
                a.hooks.push(hk);
            }
        }

        // pin awal bila belum ada: File Explorer
        if a.cfg.pinned.is_empty() {
            let win = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
            let ex = format!("{}\\explorer.exe", win);
            if std::path::Path::new(&ex).exists() {
                a.cfg.pinned.push(ex);
            }
        }
        toast::set_flags(a.cfg.banner_apps, a.cfg.banner_hide_win, a.ui);
        toast::start(a.dock_hwnd.0 as isize, WM_TOAST);
        a.update_info();
        a.refresh_windows();
        a.relayout(true);
        a.hide_vis = true;
        let _ = ShowWindow(a.dock_hwnd, SW_SHOWNA);
        a.raise_dock();
        a.check_fullscreen();
        request_frame();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            if let Some(sh) = settings::is_open() {
                if IsDialogMessageW(sh, &msg).as_bool() {
                    continue;
                }
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}
