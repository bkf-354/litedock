//! Pembaca notifikasi (toast) Windows untuk banner aplikasi (WhatsApp, dll.).
//!
//! Jendela toast milik shell Windows (ShellExperienceHost / ShellHost) dipantau dengan polling ringan (4x per detik)
//! alih-alih bergantung pada satu peristiwa WinEvent: jendela toast di Windows 11 sering sudah ada lalu hanya
//! di-uncloak atau diubah ukurannya, sehingga peristiwa SHOW/UNCLOAKED bisa terlewat. Teks toast dibaca dengan
//! UI Automation (dengan beberapa kali percobaan karena isi toast terisi belakangan), lalu dikirim ke jendela dock
//! sebagai pesan WM_TOAST. Log diagnosis (tanpa isi pesan) ditulis ke %APPDATA%\LiteDock\toast.log.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::{Duration, Instant};
use windows::core::BOOL;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::System::Com::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static ON: AtomicBool = AtomicBool::new(true);
static HIDE: AtomicBool = AtomicBool::new(false);
static UI_X100: AtomicI32 = AtomicI32::new(100);

/// Sinkronkan pengaturan: banner aplikasi aktif, sembunyikan banner Windows, skala DPI.
pub fn set_flags(on: bool, hide: bool, ui: f32) {
    ON.store(on, Ordering::Relaxed);
    HIDE.store(hide, Ordering::Relaxed);
    UI_X100.store((ui * 100.0).round() as i32, Ordering::Relaxed);
}

struct Cand {
    hwnd: isize,
    h: i32,
    /// judul jendela jelas bertema notifikasi ("New notification", "Notifikasi baru", dll.)
    strong: bool,
}

const GENERIC: [&str; 6] = ["new notification", "notifikasi baru", "notification", "notifikasi", "notifications", "pemberitahuan"];

fn log(line: &str) {
    let p = crate::config::dir().join("toast.log");
    if std::fs::metadata(&p).map(|m| m.len() > 48 * 1024).unwrap_or(false) {
        let _ = std::fs::remove_file(&p);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p) {
        let _ = writeln!(f, "{}", line);
    }
}

fn win_text(hwnd: HWND, class: bool) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe {
        if class { GetClassNameW(hwnd, &mut buf) } else { GetWindowTextW(hwnd, &mut buf) }
    }
    .max(0) as usize;
    String::from_utf16_lossy(&buf[..n.min(buf.len())])
}

fn cloaked(h: HWND) -> bool {
    let mut v: u32 = 0;
    unsafe {
        let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut v as *mut u32 as *mut _, 4);
    }
    v != 0
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    let out = &mut *(lp.0 as *mut Vec<Cand>);
    if !IsWindowVisible(hwnd).as_bool() || cloaked(hwnd) {
        return BOOL(1);
    }
    let class = win_text(hwnd, true);
    let title = win_text(hwnd, false).to_lowercase();
    let notif_title = ["notif", "benachricht", "mitteilung", "notificac", "notificat", "pemberitahuan"].iter().any(|k| title.contains(k));
    if class != "Windows.UI.Core.CoreWindow" && !notif_title {
        return BOOL(1);
    }
    let proc_ = crate::shell::process_name_of_window(hwnd.0 as isize).unwrap_or_default().to_lowercase();
    if !(proc_.contains("shellexperiencehost") || proc_.contains("shellhost") || proc_.contains("notification")) {
        return BOOL(1);
    }
    let mut r = RECT::default();
    if GetWindowRect(hwnd, &mut r).is_err() {
        return BOOL(1);
    }
    let ui = UI_X100.load(Ordering::Relaxed).max(50) as f32 / 100.0;
    let (w, h) = ((r.right - r.left) as f32 / ui, (r.bottom - r.top) as f32 / ui);
    // toast: sekitar 360 x 100-200 DIP; Start, Pusat Notifikasi, dan Pengaturan Cepat jauh lebih besar
    if !(250.0..=700.0).contains(&w) || !(40.0..=380.0).contains(&h) {
        return BOOL(1);
    }
    out.push(Cand { hwnd: hwnd.0 as isize, h: (r.bottom - r.top), strong: notif_title });
    BOOL(1)
}

fn find_candidate() -> Option<Cand> {
    let mut v: Vec<Cand> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut v as *mut Vec<Cand> as isize));
    }
    // utamakan yang berjudul notifikasi, lalu yang tertinggi (tumpukan toast)
    v.sort_by(|a, b| b.strong.cmp(&a.strong).then(b.h.cmp(&a.h)));
    v.into_iter().next()
}

fn is_generic(t: &str) -> bool {
    let l = t.trim().to_lowercase();
    l.is_empty() || GENERIC.contains(&l.as_str())
}

/// Baca teks toast lewat UI Automation; mencoba beberapa kali sampai isi toast terisi.
fn scrape(hwnd: isize) -> Vec<String> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(ua) = CoCreateInstance::<_, IUIAutomation>(&CUIAutomation, None, CLSCTX_INPROC_SERVER) else { return Vec::new() };
        let Ok(cond) = ua.CreateTrueCondition() else { return Vec::new() };
        for attempt in 0..6 {
            std::thread::sleep(Duration::from_millis(if attempt == 0 { 160 } else { 140 }));
            let Ok(root) = ua.ElementFromHandle(HWND(hwnd as *mut _)) else { continue };
            let mut texts: Vec<String> = Vec::new();
            let mut loose: Vec<String> = Vec::new();
            if let Ok(arr) = root.FindAll(TreeScope_Descendants, &cond) {
                let n = arr.Length().unwrap_or(0);
                for i in 0..n.min(60) {
                    let Ok(e) = arr.GetElement(i) else { continue };
                    let ct = e.CurrentControlType().map(|c| c.0).unwrap_or(0);
                    let Ok(name) = e.CurrentName() else { continue };
                    let t = name.to_string();
                    if is_generic(&t) {
                        continue;
                    }
                    if ct == UIA_TextControlTypeId.0 {
                        if !texts.contains(&t) {
                            texts.push(t);
                        }
                    } else if ct != UIA_ButtonControlTypeId.0 && ct != UIA_ImageControlTypeId.0 && !loose.contains(&t) {
                        loose.push(t);
                    }
                }
            }
            if texts.is_empty() {
                texts = loose;
            }
            if texts.is_empty() {
                if let Ok(name) = root.CurrentName() {
                    let t = name.to_string();
                    if !is_generic(&t) {
                        texts.push(t);
                    }
                }
            }
            if !texts.is_empty() {
                return texts;
            }
        }
        Vec::new()
    }
}

/// Mulai thread pemantau. `dock` = HWND jendela dock, `msg` = pesan yang dikirim bersama Box<Vec<String>> di lParam.
pub fn start(dock: isize, msg: u32) {
    let _ = std::thread::Builder::new().name("toast-poll".into()).spawn(move || {
        let mut present: Option<(isize, i32)> = None;
        let mut last_texts: Vec<String> = Vec::new();
        let mut last_scrape = Instant::now();
        let mut gone_since: Option<Instant> = None;
        let mut logged: Option<(isize, i32)> = None;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            if !ON.load(Ordering::Relaxed) {
                present = None;
                continue;
            }
            match find_candidate() {
                Some(c) => {
                    gone_since = None;
                    let key = (c.hwnd, c.h);
                    let changed = present != Some(key);
                    if changed || last_scrape.elapsed() > Duration::from_millis(900) {
                        present = Some(key);
                        last_scrape = Instant::now();
                        let texts = scrape(c.hwnd);
                        if logged != Some(key) {
                            logged = Some(key);
                            let h = HWND(c.hwnd as *mut _);
                            log(&format!(
                                "toast terdeteksi: kelas={} proses={} judul={} tinggi={} jumlah_teks={}",
                                win_text(h, true),
                                crate::shell::process_name_of_window(c.hwnd).unwrap_or_default(),
                                win_text(h, false),
                                c.h,
                                texts.len()
                            ));
                        }
                        // jendela bukan-toast yang kebetulan berukuran mirip (mis. kilas volume) hanya diterima bila berisi >= 2 teks
                        let useful = texts.len() >= if c.strong { 1 } else { 2 };
                        if !useful {
                            // coba lagi lebih cepat bila isi belum terbaca
                            last_scrape = Instant::now() - Duration::from_millis(600);
                        } else if texts != last_texts {
                            last_texts = texts.clone();
                            if HIDE.load(Ordering::Relaxed) {
                                unsafe {
                                    let _ = ShowWindow(HWND(c.hwnd as *mut _), SW_HIDE);
                                }
                            }
                            let b = Box::into_raw(Box::new(texts));
                            unsafe {
                                if PostMessageW(Some(HWND(dock as *mut _)), msg, WPARAM(0), LPARAM(b as isize)).is_err() {
                                    drop(Box::from_raw(b));
                                }
                            }
                        }
                    }
                }
                None => {
                    present = None;
                    logged = None;
                    let t = *gone_since.get_or_insert_with(Instant::now);
                    if t.elapsed() > Duration::from_millis(1500) {
                        last_texts.clear();
                    }
                }
            }
        }
    });
}
