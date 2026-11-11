//! Host ikon tray (area notifikasi) milik LiteDock, cara kerjanya seperti RetroBar/CairoShell:
//! LiteDock membuat jendela berkelas `Shell_TrayWnd` di atas taskbar Windows sehingga `Shell_NotifyIcon` dari aplikasi lain
//! sampai ke LiteDock. Ikon ditampilkan di panel tray LiteDock; pesan lain (appbar, dll.) diteruskan ke taskbar Windows asli.
//! Saat berhenti, LiteDock menyiarkan `TaskbarCreated` supaya semua aplikasi memasang ulang ikonnya di Explorer.
use crate::gfx::Image;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, Ordering};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

static HOST: AtomicIsize = AtomicIsize::new(0);
static NOTIFY: AtomicIsize = AtomicIsize::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(false);
static ANCH_L: AtomicI32 = AtomicI32::new(0);
static ANCH_T: AtomicI32 = AtomicI32::new(0);
static ANCH_R: AtomicI32 = AtomicI32::new(0);
static ANCH_B: AtomicI32 = AtomicI32::new(0);

pub fn host_hwnd() -> isize {
    HOST.load(Ordering::SeqCst)
}

pub fn is_active() -> bool {
    ACTIVE.load(Ordering::SeqCst)
}

/// Letak modul tray di layar (dipakai untuk menjawab `Shell_NotifyIconGetRect`).
pub fn set_anchor(l: i32, t: i32, r: i32, b: i32) {
    ANCH_L.store(l, Ordering::Relaxed);
    ANCH_T.store(t, Ordering::Relaxed);
    ANCH_R.store(r, Ordering::Relaxed);
    ANCH_B.store(b, Ordering::Relaxed);
}

const NIM_ADD: u32 = 0;
const NIM_MODIFY: u32 = 1;
const NIM_DELETE: u32 = 2;
const NIM_SETFOCUS: u32 = 3;
const NIM_SETVERSION: u32 = 4;

const NIF_MESSAGE: u32 = 0x01;
const NIF_ICON: u32 = 0x02;
const NIF_TIP: u32 = 0x04;
const NIF_STATE: u32 = 0x08;
const NIF_GUID: u32 = 0x20;
const NIS_HIDDEN: u32 = 0x01;

const NIN_SELECT: u32 = 0x400;

/// GUID ikon sistem Windows 11 (jaringan, daya, volume) -> tetap ditangani Explorer, LiteDock punya modul sendiri.
const SKIP_GUIDS: [u128; 3] = [
    0x7820ae74_23e3_4229_82c1_e41cb67d5b9c, // jaringan
    0x7820ae75_23e3_4229_82c1_e41cb67d5b9c, // daya
    0x7820ae73_23e3_4229_82c1_e41cb67d5b9c, // volume
];

pub struct TrayItem {
    pub hwnd: isize,
    pub uid: u32,
    pub guid: u128,
    pub cb: u32,
    pub version: u32,
    pub tip: String,
    pub hidden: bool,
    hicon: isize,
    /// (ukuran tampil, gambar siap pakai): dibuat lazy, tajam pada ukuran tampil tersebut
    icon: RefCell<Option<(i32, Rc<Image>)>>,
}

impl Drop for TrayItem {
    fn drop(&mut self) {
        if self.hicon != 0 {
            unsafe {
                let _ = DestroyIcon(HICON(self.hicon as *mut _));
            }
        }
    }
}

thread_local! {
    static ITEMS: RefCell<Vec<TrayItem>> = const { RefCell::new(Vec::new()) };
}

/// Salinan ringan untuk digambar panel.
pub struct ItemView {
    pub idx: usize,
    pub tip: String,
    pub icon: Option<Rc<Image>>,
}

pub fn count() -> usize {
    ITEMS.with(|v| v.borrow().iter().filter(|i| !i.hidden).count())
}

/// Siapkan ikon pada ukuran tampil `want` (px) tanpa blur: ikon besar dikecilkan dengan rata-rata area,
/// ikon kecil di layar DPI tinggi diperbesar bulat (2x) atau ditampilkan apa adanya.
fn prepare_icon(img: Image, want: i32) -> Image {
    let native = img.w.max(img.h);
    if native > want + want / 2 {
        img.box_resize(want, want)
    } else if native * 2 <= want + want / 8 {
        img.nearest2x()
    } else {
        img
    }
}

/// Daftar ikon yang tampil, dengan gambar tajam pada ukuran tampil `want` (px).
pub fn views(want: i32) -> Vec<ItemView> {
    prune();
    ITEMS.with(|v| {
        let v = v.borrow();
        v.iter()
            .enumerate()
            .filter(|(_, it)| !it.hidden)
            .map(|(idx, it)| {
                let mut slot = it.icon.borrow_mut();
                let stale = slot.as_ref().map(|(w, _)| *w != want).unwrap_or(true);
                if stale && it.hicon != 0 {
                    *slot = crate::shell::hicon_to_image_ex(HICON(it.hicon as *mut _), false).map(|img| (want, Rc::new(prepare_icon(img, want))));
                }
                ItemView { idx, tip: it.tip.clone(), icon: slot.as_ref().map(|(_, i)| i.clone()) }
            })
            .collect()
    })
}

/// Buang ikon yang jendela pemiliknya sudah tidak ada (aplikasi tertutup tanpa NIM_DELETE).
pub fn prune() {
    ITEMS.with(|v| {
        v.borrow_mut().retain(|i| unsafe { IsWindow(Some(HWND(i.hwnd as *mut _))).as_bool() });
    });
}

// ------------------------------------------------------------------ protokol NIM

fn rd32(b: &[u8], off: usize) -> u32 {
    if off + 4 <= b.len() {
        u32::from_le_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]])
    } else {
        0
    }
}

fn rd_wstr(b: &[u8], off: usize, max_chars: usize) -> String {
    let mut v: Vec<u16> = Vec::new();
    for i in 0..max_chars {
        let o = off + i * 2;
        if o + 2 > b.len() {
            break;
        }
        let c = u16::from_le_bytes([b[o], b[o + 1]]);
        if c == 0 {
            break;
        }
        v.push(c);
    }
    String::from_utf16_lossy(&v)
}

fn rd_guid(b: &[u8], off: usize) -> u128 {
    if off + 16 > b.len() {
        return 0;
    }
    let d1 = rd32(b, off) as u128;
    let d2 = u16::from_le_bytes([b[off + 4], b[off + 5]]) as u128;
    let d3 = u16::from_le_bytes([b[off + 6], b[off + 7]]) as u128;
    let mut tail: u128 = 0;
    for i in 0..8 {
        tail = (tail << 8) | b[off + 8 + i] as u128;
    }
    (d1 << 96) | (d2 << 80) | (d3 << 64) | tail
}

fn h32(v: u32) -> isize {
    v as i32 as isize
}

/// Tangani satu pesan NIM dari Shell_NotifyIcon. true = sudah diurus LiteDock; false = teruskan ke Explorer.
fn handle_nim(msg: u32, d: &[u8]) -> bool {
    // d = NOTIFYICONDATA (tanpa 8 byte kepala SHELLTRAYDATA)
    let hwnd = h32(rd32(d, 4));
    let uid = rd32(d, 8);
    let flags = rd32(d, 12);
    let guid = if flags & NIF_GUID != 0 { rd_guid(d, 936) } else { 0 };
    if guid != 0 && SKIP_GUIDS.contains(&guid) {
        return false;
    }
    if hwnd == 0 && (guid == 0 || msg == NIM_ADD) {
        return false;
    }
    let same = |i: &TrayItem| (i.hwnd == hwnd && i.uid == uid) || (guid != 0 && i.guid == guid);
    match msg {
        NIM_ADD | NIM_MODIFY => {
            if hwnd == 0 || unsafe { !IsWindow(Some(HWND(hwnd as *mut _))).as_bool() } {
                return false;
            }
            ITEMS.with(|v| {
                let mut v = v.borrow_mut();
                let pos = v.iter().position(|i| same(i));
                let existed = pos.is_some();
                if !existed {
                    v.push(TrayItem { hwnd, uid, guid, cb: 0, version: 0, tip: String::new(), hidden: false, hicon: 0, icon: RefCell::new(None) });
                }
                let it = match pos {
                    Some(p) => &mut v[p],
                    None => {
                        let n = v.len();
                        &mut v[n - 1]
                    }
                };
                it.hwnd = hwnd;
                it.uid = uid;
                if guid != 0 {
                    it.guid = guid;
                }
                if flags & NIF_MESSAGE != 0 {
                    it.cb = rd32(d, 16);
                }
                if flags & NIF_TIP != 0 {
                    let t = rd_wstr(d, 24, 128);
                    if !t.is_empty() {
                        it.tip = t;
                    }
                }
                if flags & NIF_STATE != 0 {
                    let (st, mask) = (rd32(d, 280), rd32(d, 284));
                    if mask & NIS_HIDDEN != 0 {
                        it.hidden = st & NIS_HIDDEN != 0;
                    }
                }
                let ver = rd32(d, 800);
                if msg == NIM_ADD && ver > 0 && ver <= 4 && it.version == 0 {
                    it.version = ver;
                }
                if flags & NIF_ICON != 0 {
                    let hi = h32(rd32(d, 20));
                    let copy = if hi != 0 { unsafe { CopyIcon(HICON(hi as *mut _)).map(|h| h.0 as isize).unwrap_or(0) } } else { 0 };
                    if it.hicon != 0 {
                        unsafe {
                            let _ = DestroyIcon(HICON(it.hicon as *mut _));
                        }
                    }
                    it.hicon = copy;
                    *it.icon.borrow_mut() = None;
                }
                if it.tip.is_empty() {
                    it.tip = crate::shell::process_name_of_window(hwnd).unwrap_or_default();
                }
                if !existed && msg == NIM_MODIFY && flags & NIF_ICON == 0 {
                    return true;
                }
                true
            })
        }
        NIM_DELETE => {
            ITEMS.with(|v| {
                let mut v = v.borrow_mut();
                let before = v.len();
                v.retain(|i| !same(i));
                before != v.len()
            });
            true
        }
        NIM_SETFOCUS => true,
        NIM_SETVERSION => {
            let ver = rd32(d, 800);
            if ver > 4 {
                return false;
            }
            ITEMS.with(|v| {
                for i in v.borrow_mut().iter_mut() {
                    if same(i) {
                        i.version = ver;
                    }
                }
            });
            true
        }
        _ => false,
    }
}

// ------------------------------------------------------------------ jendela host

fn real_tray() -> Option<HWND> {
    let ours = host_hwnd();
    unsafe {
        let mut prev: Option<HWND> = None;
        loop {
            match FindWindowExW(None, prev, w!("Shell_TrayWnd"), PCWSTR::null()) {
                Ok(h) if !h.0.is_null() => {
                    if h.0 as isize != ours {
                        return Some(h);
                    }
                    prev = Some(h);
                }
                _ => return None,
            }
        }
    }
}

fn forward(msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let Some(real) = real_tray() else { return LRESULT(0) };
    unsafe {
        if msg == WM_USER + 372 {
            let _ = PostMessageW(Some(real), msg, wp, lp);
            return LRESULT(0);
        }
        let mut res: usize = 0;
        let r = SendMessageTimeoutW(real, msg, wp, lp, SMTO_ABORTIFHUNG, 1500, Some(&mut res));
        if r.0 == 0 {
            return LRESULT(0);
        }
        LRESULT(res as isize)
    }
}

#[repr(C)]
struct CopyData {
    dw_data: usize,
    cb_data: u32,
    lp_data: *const u8,
}

unsafe extern "system" fn host_proc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| host_proc_inner(h, msg, wp, lp))) {
        Ok(r) => r,
        Err(_) => forward(msg, wp, lp),
    }
}

unsafe fn host_proc_inner(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COPYDATA => {
            if lp.0 == 0 {
                return LRESULT(0);
            }
            let cd = &*(lp.0 as *const CopyData);
            match cd.dw_data {
                1 if !cd.lp_data.is_null() && cd.cb_data >= 12 => {
                    let bytes = std::slice::from_raw_parts(cd.lp_data, cd.cb_data as usize);
                    let nim = rd32(bytes, 4);
                    if handle_nim(nim, &bytes[8.min(bytes.len())..]) {
                        crate::tray_changed();
                        return LRESULT(1);
                    }
                    return forward(msg, wp, lp);
                }
                3 if !cd.lp_data.is_null() && cd.cb_data >= 24 => {
                    // Shell_NotifyIconGetRect: beri posisi modul tray LiteDock
                    let bytes = std::slice::from_raw_parts(cd.lp_data, cd.cb_data as usize);
                    let which = rd32(bytes, 4);
                    let (hw, uid) = (h32(rd32(bytes, 16)), rd32(bytes, 20));
                    let known = ITEMS.with(|v| v.borrow().iter().any(|i| i.hwnd == hw && i.uid == uid));
                    if known {
                        let (x, y) = if which == 1 { (ANCH_L.load(Ordering::Relaxed), ANCH_T.load(Ordering::Relaxed)) } else { (ANCH_R.load(Ordering::Relaxed), ANCH_B.load(Ordering::Relaxed)) };
                        return LRESULT((((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF)) as i32 as isize);
                    }
                    return forward(msg, wp, lp);
                }
                _ => return forward(msg, wp, lp),
            }
        }
        WM_WINDOWPOSCHANGED => {
            // jendela host tidak boleh tampil
            let style = GetWindowLongW(h, GWL_STYLE) as u32;
            if style & WS_VISIBLE.0 != 0 {
                SetWindowLongW(h, GWL_STYLE, (style & !WS_VISIBLE.0) as i32);
            }
            DefWindowProcW(h, msg, wp, lp)
        }
        WM_ACTIVATEAPP | WM_COMMAND => forward(msg, wp, lp),
        m if m >= WM_USER => forward(m, wp, lp),
        _ => DefWindowProcW(h, msg, wp, lp),
    }
}

fn broadcast_taskbar_created() {
    unsafe {
        let m = RegisterWindowMessageW(w!("TaskbarCreated"));
        if m != 0 {
            let _ = SendNotifyMessageW(HWND_BROADCAST, m, WPARAM(0), LPARAM(0));
        }
    }
}

/// Mulai menjadi host tray. Mengembalikan true bila jendela host berhasil dibuat.
pub fn start() -> bool {
    if is_active() {
        return true;
    }
    unsafe {
        let hi = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()).unwrap_or_default();
        let reg = |name: PCWSTR| {
            let wc = WNDCLASSEXW { cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, style: CS_DBLCLKS, lpfnWndProc: Some(host_proc), hInstance: hi.into(), lpszClassName: name, ..Default::default() };
            RegisterClassExW(&wc)
        };
        reg(w!("Shell_TrayWnd"));
        reg(w!("TrayNotifyWnd"));
        let cx = GetSystemMetrics(SM_CXSCREEN);
        let Ok(host) = CreateWindowExW(WS_EX_TOPMOST | WS_EX_TOOLWINDOW, w!("Shell_TrayWnd"), w!(""), WS_POPUP | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, cx, 24, None, None, Some(hi.into()), None) else {
            return false;
        };
        let notify = CreateWindowExW(WINDOW_EX_STYLE(0), w!("TrayNotifyWnd"), PCWSTR::null(), WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS, 0, 0, cx, 24, Some(host), None, Some(hi.into()), None).unwrap_or_default();
        // aplikasi berhak admin juga boleh mengirim ikon
        let _ = ChangeWindowMessageFilterEx(host, WM_COPYDATA, MSGFLT_ALLOW, None);
        HOST.store(host.0 as isize, Ordering::SeqCst);
        NOTIFY.store(notify.0 as isize, Ordering::SeqCst);
        ACTIVE.store(true, Ordering::SeqCst);
        raise_host();
    }
    broadcast_taskbar_created();
    true
}

/// Jaga agar host berada di urutan pertama saat aplikasi mencari `Shell_TrayWnd`.
pub fn raise_host() {
    let h = host_hwnd();
    if h == 0 {
        return;
    }
    unsafe {
        if let Some(real) = real_tray() {
            // taskbar asli ke dasar z-order (tersembunyi) supaya FindWindow menemukan jendela host dulu
            let _ = SetWindowPos(real, Some(HWND_BOTTOM), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
        let first = FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()).map(|x| x.0 as isize).unwrap_or(0);
        if first != h {
            let _ = SetWindowPos(HWND(h as *mut _), Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Berhenti menjadi host: hancurkan jendela, kembalikan taskbar asli, minta semua aplikasi memasang ulang ikon di Explorer.
pub fn stop() {
    if !is_active() {
        return;
    }
    ACTIVE.store(false, Ordering::SeqCst);
    unsafe {
        let h = HOST.swap(0, Ordering::SeqCst);
        let n = NOTIFY.swap(0, Ordering::SeqCst);
        if n != 0 {
            let _ = DestroyWindow(HWND(n as *mut _));
        }
        if h != 0 {
            let _ = DestroyWindow(HWND(h as *mut _));
        }
        let _ = UnregisterClassW(w!("TrayNotifyWnd"), None);
        let _ = UnregisterClassW(w!("Shell_TrayWnd"), None);
        if let Some(real) = real_tray() {
            let _ = SetWindowPos(real, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
    ITEMS.with(|v| v.borrow_mut().clear());
    broadcast_taskbar_created();
}

/// Dipakai `--restore` dan pemulihan: minta aplikasi memasang ulang ikon di Explorer.
pub fn announce_to_explorer() {
    broadcast_taskbar_created();
}

// ------------------------------------------------------------------ klik ke ikon

fn mouse_param() -> u32 {
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        (((pt.y as u32) & 0xFFFF) << 16) | ((pt.x as u32) & 0xFFFF)
    }
}

fn send(it: &TrayItem, event: u32, mouse: u32) {
    if it.cb == 0 {
        return;
    }
    let (wp, lp) = if it.version > 3 { (mouse as usize, (event | (it.uid << 16)) as isize) } else { (it.uid as usize, event as isize) };
    unsafe {
        let _ = SendNotifyMessageW(HWND(it.hwnd as *mut _), it.cb, WPARAM(wp), LPARAM(lp));
    }
}

/// Klik ikon: `right` = klik kanan (menu konteks), selain itu klik kiri.
pub fn click(idx: usize, right: bool) {
    ITEMS.with(|v| {
        let v = v.borrow();
        let Some(it) = v.get(idx) else { return };
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(HWND(it.hwnd as *mut _), Some(&mut pid));
            let _ = AllowSetForegroundWindow(pid);
        }
        let mouse = mouse_param();
        if right {
            send(it, WM_RBUTTONDOWN, mouse);
            send(it, WM_RBUTTONUP, mouse);
            if it.version >= 3 {
                send(it, WM_CONTEXTMENU, mouse);
            }
        } else {
            send(it, WM_LBUTTONDOWN, mouse);
            send(it, WM_LBUTTONUP, mouse);
            if it.version >= 3 {
                send(it, NIN_SELECT, mouse);
            }
        }
    });
}
