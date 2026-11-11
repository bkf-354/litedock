//! Interaksi dengan shell Windows: daftar jendela, ikon aplikasi, taskbar asli, AppBar, autostart.
use crate::gfx::{Icon, Image};
use windows::core::{w, Interface, BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Controls::IImageList;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[derive(Clone)]
pub struct WinInfo {
    pub hwnd: isize,
    pub key: String,
    pub path: String,
    pub title: String,
    pub minimized: bool,
    pub uwp: bool,
    pub aumid: String,
}

fn class_name(h: HWND) -> String {
    let mut buf = [0u16; 128];
    let n = unsafe { GetClassNameW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n.min(128)])
}

fn window_title(h: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(h, &mut buf) } as usize;
    String::from_utf16_lossy(&buf[..n.min(256)])
}

/// Nama program (tanpa .exe) pemilik jendela; dipakai sebagai label bila ikon tray tidak punya tooltip.
pub fn process_name_of_window(hwnd: isize) -> Option<String> {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid));
        let path = process_path(pid)?;
        std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string())
    }
}

pub fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_FORMAT(0), PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        ok.ok()?;
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn is_cloaked(h: HWND) -> bool {
    let mut v: u32 = 0;
    unsafe {
        let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut v as *mut u32 as *mut _, 4);
    }
    v != 0
}

struct EnumCtx {
    own: Vec<isize>,
    out: Vec<WinInfo>,
}

unsafe extern "system" fn enum_cb(h: HWND, lp: LPARAM) -> BOOL {
    let ctx = &mut *(lp.0 as *mut EnumCtx);
    if !IsWindowVisible(h).as_bool() || ctx.own.contains(&(h.0 as isize)) {
        return TRUE;
    }
    let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
    let owner = GetWindow(h, GW_OWNER).unwrap_or_default();
    if (ex & WS_EX_TOOLWINDOW.0) != 0 && (ex & WS_EX_APPWINDOW.0) == 0 {
        return TRUE;
    }
    if !owner.0.is_null() && (ex & WS_EX_APPWINDOW.0) == 0 {
        return TRUE;
    }
    if (ex & WS_EX_NOACTIVATE.0) != 0 && (ex & WS_EX_APPWINDOW.0) == 0 {
        return TRUE;
    }
    if is_cloaked(h) {
        return TRUE;
    }
    let title = window_title(h);
    if title.is_empty() {
        return TRUE;
    }
    let cls = class_name(h);
    if matches!(cls.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow" | "XamlExplorerHostIslandWindow") {
        return TRUE;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let mut uwp = false;
    let mut path = process_path(pid).unwrap_or_default();
    if cls == "ApplicationFrameWindow" {
        // cari proses aplikasi UWP sebenarnya (anak CoreWindow di proses lain)
        struct Child {
            frame_pid: u32,
            found: u32,
        }
        unsafe extern "system" fn child_cb(c: HWND, lp: LPARAM) -> BOOL {
            let ch = &mut *(lp.0 as *mut Child);
            let mut p = 0u32;
            GetWindowThreadProcessId(c, Some(&mut p));
            if p != ch.frame_pid && class_name(c) == "Windows.UI.Core.CoreWindow" {
                ch.found = p;
                return FALSE;
            }
            TRUE
        }
        let mut ch = Child { frame_pid: pid, found: 0 };
        let _ = EnumChildWindows(Some(h), Some(child_cb), LPARAM(&mut ch as *mut Child as isize));
        if ch.found != 0 {
            if let Some(p) = process_path(ch.found) {
                path = p;
            }
        }
        uwp = true;
    }
    // aplikasi paket (Store/MSIX): identitas & ikon diambil lewat AUMID, bukan berkas exe
    let packaged = uwp || path.to_lowercase().contains("\\windowsapps\\");
    let aumid = if packaged { window_aumid(h).unwrap_or_default() } else { String::new() };
    let key = if !aumid.is_empty() {
        format!("aumid:{}", aumid.to_lowercase())
    } else if path.is_empty() {
        format!("hwnd:{}", h.0 as isize)
    } else {
        path.to_lowercase()
    };
    ctx.out.push(WinInfo { hwnd: h.0 as isize, key, path, title, minimized: IsIconic(h).as_bool(), uwp, aumid });
    TRUE
}

/// Daftar jendela yang layak tampil di taskbar, urut dari paling atas (z-order).
pub fn enum_windows(own: &[isize]) -> Vec<WinInfo> {
    let mut ctx = EnumCtx { own: own.to_vec(), out: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut EnumCtx as isize));
    }
    ctx.out
}

// ------------------------------------------------------------------ Ikon

pub(crate) fn hicon_to_image(hicon: HICON) -> Option<Image> {
    hicon_to_image_ex(hicon, true)
}

/// `soften` = haluskan tepi ikon bermasker 1-bit (untuk ikon besar); false = piksel apa adanya (tajam, untuk ikon tray kecil).
pub(crate) fn hicon_to_image_ex(hicon: HICON, soften: bool) -> Option<Image> {
    unsafe {
        let mut ii = ICONINFO::default();
        GetIconInfo(hicon, &mut ii).ok()?;
        let mut bm = BITMAP::default();
        let n = GetObjectW(ii.hbmColor.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _));
        if n == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(ii.hbmColor.into());
            let _ = DeleteObject(ii.hbmMask.into());
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut px = vec![0u32; (w * h) as usize];
        let dc = GetDC(None);
        GetDIBits(dc, ii.hbmColor, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        let has_alpha = px.iter().any(|p| p >> 24 != 0);
        if !has_alpha {
            let mut mask = vec![0u32; (w * h) as usize];
            let mut bm2 = bi;
            GetDIBits(dc, ii.hbmMask, 0, h as u32, Some(mask.as_mut_ptr() as *mut _), &mut bm2, DIB_RGB_COLORS);
            for (p, m) in px.iter_mut().zip(mask.iter()) {
                *p = if m & 0x00FF_FFFF == 0 { *p | 0xFF00_0000 } else { 0 };
            }
            if soften {
                soften_edges(&mut px, w, h);
            }
        } else {
            // premultiply
            for p in px.iter_mut() {
                let a = *p >> 24;
                if a != 255 {
                    let r = ((*p >> 16) & 255) * a / 255;
                    let g = ((*p >> 8) & 255) * a / 255;
                    let b = (*p & 255) * a / 255;
                    *p = (a << 24) | (r << 16) | (g << 8) | b;
                }
            }
        }
        ReleaseDC(None, dc);
        let _ = DeleteObject(ii.hbmColor.into());
        let _ = DeleteObject(ii.hbmMask.into());
        Some(Image { w, h, px })
    }
}

/// Haluskan tepi bergerigi pada ikon bermasker 1-bit: rata-rata 3x3 hanya pada piksel tepi.
fn soften_edges(px: &mut [u32], w: i32, h: i32) {
    let src = px.to_vec();
    let at = |x: i32, y: i32| -> u32 {
        if x < 0 || y < 0 || x >= w || y >= h { 0 } else { src[(y * w + x) as usize] }
    };
    for y in 0..h {
        for x in 0..w {
            let c = at(x, y);
            let a = c >> 24;
            let mut edge = false;
            for (dx, dy) in [(-1, 0), (1, 0), (0, -1), (0, 1)] {
                if (at(x + dx, y + dy) >> 24) != a {
                    edge = true;
                    break;
                }
            }
            if !edge {
                continue;
            }
            let (mut sa, mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32, 0u32);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let n = at(x + dx, y + dy);
                    sa += n >> 24;
                    sr += (n >> 16) & 255;
                    sg += (n >> 8) & 255;
                    sb += n & 255;
                }
            }
            px[(y * w + x) as usize] = ((sa / 9) << 24) | ((sr / 9) << 16) | ((sg / 9) << 8) | (sb / 9);
        }
    }
}

/// Ikon besar (256 px bila ada) dari berkas exe lewat image list shell.
fn jumbo_icon(path: &str) -> Option<Image> {
    unsafe {
        let wp = wide(path);
        let mut sfi = SHFILEINFOW::default();
        let r = SHGetFileInfoW(PCWSTR(wp.as_ptr()), windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL, Some(&mut sfi), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_SYSICONINDEX);
        if r == 0 {
            return None;
        }
        for shil in [SHIL_JUMBO as i32, SHIL_EXTRALARGE as i32] {
            if let Ok(list) = SHGetImageList::<IImageList>(shil) {
                if let Ok(h) = list.GetIcon(sfi.iIcon, 1 /* ILD_TRANSPARENT */) {
                    let img = hicon_to_image(h);
                    let _ = DestroyIcon(h);
                    if let Some(i) = img {
                        // Windows kadang memperbesar ikon kecil dengan blok kasar -> kembalikan ke aslinya lalu perbesar halus
                        let i = i.crop_small_content().unwrap_or(i);
                        return Some(i.undo_nearest_upscale().unwrap_or(i));
                    }
                }
            }
        }
        None
    }
}

fn window_icon(hwnd: isize) -> Option<Image> {
    unsafe {
        let h = HWND(hwnd as *mut _);
        if IsHungAppWindow(h).as_bool() {
            return None;
        }
        let mut res = 0usize;
        for kind in [1usize /*ICON_BIG*/, 2 /*ICON_SMALL2*/] {
            let _ = SendMessageTimeoutW(h, WM_GETICON, WPARAM(kind), LPARAM(0), SMTO_ABORTIFHUNG, 100, Some(&mut res));
            if res != 0 {
                if let Some(i) = hicon_to_image(HICON(res as *mut _)) {
                    return Some(i);
                }
            }
        }
        let c = GetClassLongPtrW(h, GCLP_HICON);
        if c != 0 {
            return hicon_to_image(HICON(c as *mut _));
        }
        None
    }
}

fn generic_exe_index() -> i32 {
    use std::sync::OnceLock;
    static IDX: OnceLock<i32> = OnceLock::new();
    *IDX.get_or_init(|| unsafe {
        let wp = wide("x.exe");
        let mut sfi = SHFILEINFOW::default();
        let r = SHGetFileInfoW(PCWSTR(wp.as_ptr()), windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL, Some(&mut sfi), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES);
        if r == 0 { -1 } else { sfi.iIcon }
    })
}

/// Ikon exe asli; None bila hanya ikon generik "aplikasi tanpa ikon" yang tersedia.
fn real_exe_icon(path: &str) -> Option<Image> {
    unsafe {
        let wp = wide(path);
        let mut sfi = SHFILEINFOW::default();
        let r = SHGetFileInfoW(PCWSTR(wp.as_ptr()), windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL, Some(&mut sfi), std::mem::size_of::<SHFILEINFOW>() as u32, SHGFI_SYSICONINDEX);
        if r == 0 || sfi.iIcon == generic_exe_index() {
            return None;
        }
    }
    jumbo_icon(path)
}

/// AUMID jendela (aplikasi Store/MSIX), mis. "5319275A.WhatsAppDesktop_cv1g1gvanyjgm!App".
pub fn window_aumid(hwnd: HWND) -> Option<String> {
    use std::cell::RefCell;
    use std::collections::HashMap;
    thread_local! {
        static CACHE: RefCell<HashMap<isize, Option<String>>> = RefCell::new(HashMap::new());
    }
    let key = hwnd.0 as isize;
    if let Some(v) = CACHE.with(|c| c.borrow().get(&key).cloned()) {
        return v;
    }
    // jangan memanggil COM pada jendela yang sedang tidak responsif
    if unsafe { IsHungAppWindow(hwnd).as_bool() } {
        return None;
    }
    let v = window_aumid_uncached(hwnd);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.len() > 400 {
            c.clear();
        }
        c.insert(key, v.clone());
    });
    v
}

fn window_aumid_uncached(hwnd: HWND) -> Option<String> {
    use windows::Win32::Storage::EnhancedStorage::PKEY_AppUserModel_ID;
    use windows::Win32::System::Com::{CoTaskMemFree, StructuredStorage::PropVariantToStringAlloc};
    use windows::Win32::UI::Shell::PropertiesSystem::{IPropertyStore, SHGetPropertyStoreForWindow};
    unsafe {
        let store: IPropertyStore = SHGetPropertyStoreForWindow(hwnd).ok()?;
        let pv = store.GetValue(&PKEY_AppUserModel_ID).ok()?;
        let ps = PropVariantToStringAlloc(&pv).ok()?;
        let out = ps.to_string().ok();
        CoTaskMemFree(Some(ps.0 as *const _));
        out.filter(|x| !x.is_empty() && x.len() < 260)
    }
}

/// Nama tampilan dari AUMID, mis. "5319275A.WhatsAppDesktop_xxx!App" -> "WhatsAppDesktop".
pub fn aumid_display_name(aumid: &str) -> String {
    let fam = aumid.split('!').next().unwrap_or(aumid);
    let pkg = fam.split('_').next().unwrap_or(fam);
    let name = pkg.split_once('.').map(|x| x.1).unwrap_or(pkg);
    name.to_string()
}

/// Ikon tile aplikasi paket lewat shell (IShellItemImageFactory).
fn aumid_icon(aumid: &str) -> Option<Image> {
    use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY};
    unsafe {
        let name = wide(&format!("shell:AppsFolder\\{}", aumid));
        let f: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None).ok()?;
        let hbmp = f.GetImage(SIZE { cx: 256, cy: 256 }, SIIGBF_ICONONLY).ok()?;
        let mut bm = BITMAP::default();
        if GetObjectW(hbmp.into(), std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) == 0 || bm.bmWidth <= 0 || bm.bmHeight <= 0 {
            let _ = DeleteObject(hbmp.into());
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO::default();
        bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h;
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB.0;
        let mut px = vec![0u32; (w * h) as usize];
        let dc = GetDC(None);
        GetDIBits(dc, hbmp, 0, h as u32, Some(px.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        ReleaseDC(None, dc);
        let _ = DeleteObject(hbmp.into());
        if !px.iter().any(|p| p >> 24 != 0) {
            return None;
        }
        // pastikan premultiplied: bila ada kanal > alfa, anggap lurus lalu kalikan
        let straight = px.iter().any(|p| {
            let a = p >> 24;
            ((p >> 16) & 255) > a || ((p >> 8) & 255) > a || (p & 255) > a
        });
        if straight {
            for p in px.iter_mut() {
                let a = *p >> 24;
                let r = ((*p >> 16) & 255) * a / 255;
                let g = ((*p >> 8) & 255) * a / 255;
                let b = (*p & 255) * a / 255;
                *p = (a << 24) | (r << 16) | (g << 8) | b;
            }
        }
        Some(Image { w, h, px })
    }
}

fn hsl(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h / 60.0) % 6.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}

/// Ikon pengganti yang relevan: ubin berwarna (warna dari nama aplikasi) dengan huruf awalnya.
pub fn letter_icon(name: &str) -> Icon {
    use crate::gfx::{Canvas, Rgba, TextCache};
    let ch = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect::<String>()).unwrap_or_else(|| "?".into());
    let mut hsh: u32 = 2166136261;
    for b in name.to_lowercase().bytes() {
        hsh = (hsh ^ b as u32).wrapping_mul(16777619);
    }
    let hue = (hsh % 360) as f32;
    let (r1, g1, b1) = hsl(hue, 0.62, 0.58);
    let (r2, g2, b2) = hsl((hue + 24.0) % 360.0, 0.66, 0.40);
    let mut img = Image::new(128, 128);
    {
        let mut cv = Canvas { w: 128, h: 128, px: &mut img.px };
        cv.fill_rrect(6.0, 6.0, 116.0, 116.0, 28.0, Rgba(r1, g1, b1, 255), Rgba(r2, g2, b2, 255));
        cv.fill_rrect(6.0, 6.0, 116.0, 58.0, 28.0, Rgba(255, 255, 255, 38), Rgba(255, 255, 255, 0));
        let mut tc = TextCache::default();
        let m = tc.get(&ch, 74, true);
        cv.draw_mask(m, 64 - m.w / 2, 64 - m.h / 2 - 1, Rgba(255, 255, 255, 245));
    }
    Icon::from_image(img)
}

/// Ikon aplikasi: tile paket (AUMID) -> ikon exe asli -> ikon jendela -> ubin huruf.
pub fn load_icon(path: &str, uwp: bool, hwnd: Option<isize>, aumid: &str, name: &str) -> Icon {
    let mut img = None;
    if !aumid.is_empty() {
        img = aumid_icon(aumid);
    }
    if img.is_none() && !path.is_empty() && !uwp && !path.to_lowercase().starts_with("shell:") {
        img = real_exe_icon(path);
    }
    if img.is_none() {
        if let Some(h) = hwnd {
            img = window_icon(h);
        }
    }
    match img {
        Some(i) => {
            // ikon glif transparan -> ubin persegi membulat ala iOS/macOS
            let i = i.glyph_tile().unwrap_or(i);
            Icon::from_image(i)
        }
        None => letter_icon(name),
    }
}

// ------------------------------------------------------------------ Taskbar asli

pub fn taskbar_windows() -> Vec<HWND> {
    let mut v = Vec::new();
    let ours = crate::tray::host_hwnd();
    unsafe {
        // taskbar utama Windows (lewati jendela Shell_TrayWnd milik host tray LiteDock sendiri)
        let mut prev: Option<HWND> = None;
        loop {
            match FindWindowExW(None, prev, w!("Shell_TrayWnd"), PCWSTR::null()) {
                Ok(h) if !h.0.is_null() => {
                    if h.0 as isize != ours {
                        v.push(h);
                        break;
                    }
                    prev = Some(h);
                }
                _ => break,
            }
        }
        let mut prev: Option<HWND> = None;
        loop {
            match FindWindowExW(None, prev, w!("Shell_SecondaryTrayWnd"), PCWSTR::null()) {
                Ok(h) if !h.0.is_null() => {
                    v.push(h);
                    prev = Some(h);
                }
                _ => break,
            }
        }
    }
    v
}

fn appbar_state(main: HWND, set: Option<u32>) -> u32 {
    unsafe {
        let mut abd = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: main, ..Default::default() };
        match set {
            None => SHAppBarMessage(ABM_GETSTATE, &mut abd) as u32,
            Some(s) => {
                abd.lParam = LPARAM(s as isize);
                SHAppBarMessage(ABM_SETSTATE, &mut abd);
                s
            }
        }
    }
}

/// Sembunyikan taskbar asli. Mengembalikan status sebelumnya (untuk pemulihan).
pub fn hide_taskbar(prev_saved: i64) -> i64 {
    let tbs = taskbar_windows();
    let Some(main) = tbs.first().copied() else { return prev_saved };
    let prev = if prev_saved >= 0 { prev_saved as u32 } else { appbar_state(main, None) };
    appbar_state(main, Some(prev | ABS_AUTOHIDE));
    for t in &tbs {
        unsafe {
            let _ = ShowWindow(*t, SW_HIDE);
        }
    }
    prev as i64
}

pub fn restore_taskbar(prev: i64) {
    let tbs = taskbar_windows();
    let Some(main) = tbs.first().copied() else { return };
    let want = if prev >= 0 { prev as u32 } else { ABS_ALWAYSONTOP };
    appbar_state(main, Some(want));
    for t in &tbs {
        unsafe {
            let _ = ShowWindow(*t, SW_SHOWNA);
        }
    }
}

/// Naikkan taskbar asli ke atas dock (saat "intip tray").
pub fn raise_taskbar_topmost() {
    for t in taskbar_windows() {
        unsafe {
            let _ = SetWindowPos(t, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Jendela depan maksimal, atau menutupi area dock (sembunyi otomatis cerdas).
pub fn foreground_maximized_or_overlapping(own: &[isize], dock: RECT) -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() || own.contains(&(h.0 as isize)) || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
            return false;
        }
        let cls = class_name(h);
        if matches!(cls.as_str(), "#32769" | "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "ForegroundStaging" | "XamlExplorerHostIslandWindow" | "Windows.UI.Core.CoreWindow") || cls.starts_with("LiteDock") {
            return false;
        }
        if IsZoomed(h).as_bool() {
            return true;
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let v = ext_bounds(h, r);
        let iw = (v.right.min(dock.right) - v.left.max(dock.left)).max(0) as i64;
        let ih = (v.bottom.min(dock.bottom) - v.top.max(dock.top)).max(0) as i64;
        iw > 4 && ih > 4
    }
}

pub fn peek_taskbar(show: bool) {
    for t in taskbar_windows() {
        unsafe {
            let _ = ShowWindow(t, if show { SW_SHOWNA } else { SW_HIDE });
        }
    }
}

// ------------------------------------------------------------------ AppBar (sisakan ruang)

pub fn appbar_register(h: HWND, cb_msg: u32) {
    unsafe {
        let mut abd = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, uCallbackMessage: cb_msg, ..Default::default() };
        SHAppBarMessage(ABM_NEW, &mut abd);
    }
}

pub fn appbar_unregister(h: HWND) {
    unsafe {
        let mut abd = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, ..Default::default() };
        SHAppBarMessage(ABM_REMOVE, &mut abd);
    }
}

/// Pesan area di tepi bawah monitor (rect monitor `mon`, tinggi `height` px).
pub fn appbar_set_bottom(h: HWND, mon: RECT, height: i32) -> RECT {
    unsafe {
        let mut abd = APPBARDATA { cbSize: std::mem::size_of::<APPBARDATA>() as u32, hWnd: h, uEdge: ABE_BOTTOM, rc: mon, ..Default::default() };
        abd.rc.top = abd.rc.bottom - height;
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        abd.rc.top = abd.rc.bottom - height;
        SHAppBarMessage(ABM_SETPOS, &mut abd);
        let r = abd.rc;
        let _ = SetWindowPos(h, None, r.left, r.top, r.right - r.left, r.bottom - r.top, SWP_NOZORDER | SWP_NOACTIVATE);
        r
    }
}

pub fn work_area_bottom(mon: RECT) -> i32 {
    unsafe {
        let m = MonitorFromPoint(POINT { x: mon.left + 1, y: mon.top + 1 }, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        mi.rcWork.bottom
    }
}

// ------------------------------------------------------------------ Autostart & tema

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");

pub fn set_autostart(on: bool) {
    unsafe {
        let mut key = HKEY::default();
        if RegCreateKeyExW(HKEY_CURRENT_USER, RUN_KEY, None, PCWSTR::null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, None, &mut key, None).is_err() {
            return;
        }
        if on {
            if let Ok(exe) = std::env::current_exe() {
                let val = format!("\"{}\"", exe.display());
                let wv = wide(&val);
                let bytes = std::slice::from_raw_parts(wv.as_ptr() as *const u8, wv.len() * 2);
                let _ = RegSetValueExW(key, w!("LiteDock"), None, REG_SZ, Some(bytes));
            }
        } else {
            let _ = RegDeleteValueW(key, w!("LiteDock"));
        }
        let _ = RegCloseKey(key);
    }
}

pub fn autostart_enabled() -> bool {
    unsafe {
        let mut sz = 0u32;
        RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, w!("LiteDock"), RRF_RT_REG_SZ, None, None, Some(&mut sz)).is_ok() && sz > 2
    }
}

/// true = tema terang untuk elemen sistem (taskbar).
pub fn system_uses_light_theme() -> bool {
    unsafe {
        let mut v: u32 = 0;
        let mut sz = 4u32;
        let r = RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(&mut v as *mut u32 as *mut _),
            Some(&mut sz),
        );
        r.is_ok() && v == 1
    }
}

// ------------------------------------------------------------------ Status jaringan & baterai

pub fn battery() -> Option<(u8, bool)> {
    unsafe {
        let mut s = windows::Win32::System::Power::SYSTEM_POWER_STATUS::default();
        windows::Win32::System::Power::GetSystemPowerStatus(&mut s).ok()?;
        if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent == 255 {
            return None; // tidak ada baterai
        }
        Some((s.BatteryLifePercent, s.ACLineStatus == 1))
    }
}

/// Wi-Fi/Ethernet: true bila ada adapter nirkabel/ethernet yang aktif.
pub fn wifi_connected() -> bool {
    use windows::Win32::NetworkManagement::IpHelper::*;
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    unsafe {
        let mut size = 0u32;
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let _ = GetAdaptersAddresses(0, flags, None, None, &mut size);
        if size == 0 {
            return false;
        }
        let mut buf = vec![0u8; size as usize];
        let p = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
        if GetAdaptersAddresses(0, flags, None, Some(p), &mut size) != 0 {
            return false;
        }
        let mut cur = p;
        while !cur.is_null() {
            let a = &*cur;
            // 71 = IEEE80211 (Wi-Fi), 6 = Ethernet ; OperStatus 1 = Up
            if (a.IfType == 71 || a.IfType == 6) && a.OperStatus == IfOperStatusUp && !a.FirstUnicastAddress.is_null() {
                return true;
            }
            cur = a.Next;
        }
        false
    }
}

/// Status jaringan seperti yang dipakai ikon jaringan Windows (profil koneksi WinRT), tanpa tertipu adapter virtual.
#[derive(Clone)]
pub struct NetStatus {
    pub up: bool,
    /// 0 tidak ada, 1 Wi-Fi, 2 kabel/seluler
    pub kind: u8,
    /// busur Wi-Fi menyala 0..=3
    pub lit: u8,
    pub ssid: String,
}

static NET_SNAP: std::sync::Mutex<Option<NetStatus>> = std::sync::Mutex::new(None);

/// Status jaringan terakhir yang dihitung thread latar (tidak memblokir thread UI dengan panggilan WinRT).
pub fn net_status_cached() -> NetStatus {
    NET_SNAP.lock().ok().and_then(|g| g.clone()).unwrap_or(NetStatus { up: false, kind: 0, lit: 0, ssid: String::new() })
}

pub fn net_snap_ready() -> bool {
    NET_SNAP.lock().map(|g| g.is_some()).unwrap_or(false)
}

pub fn net_snap_store(st: NetStatus) {
    if let Ok(mut g) = NET_SNAP.lock() {
        *g = Some(st);
    }
}

pub fn net_status() -> NetStatus {
    use windows::Networking::Connectivity::*;
    let mut st = NetStatus { up: false, kind: 0, lit: 0, ssid: String::new() };
    let profile = NetworkInformation::GetInternetConnectionProfile().ok().or_else(|| {
        // tanpa internet tapi terhubung ke jaringan lokal: ambil profil pertama yang punya konektivitas
        NetworkInformation::GetConnectionProfiles().ok().and_then(|l| {
            l.into_iter().find(|p| p.GetNetworkConnectivityLevel().map(|c| c != NetworkConnectivityLevel::None).unwrap_or(false))
        })
    });
    match profile {
        Some(p) => {
            let lvl = p.GetNetworkConnectivityLevel().unwrap_or(NetworkConnectivityLevel::None);
            if lvl == NetworkConnectivityLevel::None {
                return st;
            }
            st.up = true;
            if p.IsWlanConnectionProfile().unwrap_or(false) {
                st.kind = 1;
                let bars = p.GetSignalBars().ok().and_then(|r| r.Value().ok()).unwrap_or(4);
                st.lit = match bars {
                    0 => 0,
                    1 | 2 => 1,
                    3 => 2,
                    _ => 3,
                };
                if let Ok(d) = p.WlanConnectionProfileDetails() {
                    if let Ok(h) = d.GetConnectedSsid() {
                        st.ssid = h.to_string();
                    }
                }
            } else {
                st.kind = 2;
            }
            st
        }
        None => {
            // WinRT tidak memberi profil (mis. mode pesawat/tanpa jaringan): terputus
            st
        }
    }
}

pub struct WifiInfo {
    pub connected: bool,
    pub kind: String,
    pub ssid: String,
    pub signal: Option<u32>,
    pub ip: String,
    pub speed_mbps: Option<u64>,
    pub adapter: String,
}

/// Detail jaringan aktif: adapter, IPv4, kecepatan; SSID & kekuatan sinyal bila Wi-Fi (WLAN API).
pub fn wifi_details() -> WifiInfo {
    use windows::Win32::NetworkManagement::IpHelper::*;
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    let mut info = WifiInfo { connected: false, kind: String::new(), ssid: String::new(), signal: None, ip: String::new(), speed_mbps: None, adapter: String::new() };
    unsafe {
        let mut size = 0u32;
        let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
        let _ = GetAdaptersAddresses(0, flags, None, None, &mut size);
        if size > 0 {
            let mut buf = vec![0u8; size as usize];
            let p = buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH;
            if GetAdaptersAddresses(0, flags, None, Some(p), &mut size) == 0 {
                let mut cur = p;
                while !cur.is_null() {
                    let a = &*cur;
                    if (a.IfType == 71 || a.IfType == 6) && a.OperStatus == IfOperStatusUp && !a.FirstUnicastAddress.is_null() {
                        // alamat IPv4
                        let mut ua = a.FirstUnicastAddress;
                        let mut ip = String::new();
                        while !ua.is_null() {
                            let sa = (*ua).Address.lpSockaddr;
                            if !sa.is_null() && (*sa).sa_family.0 == 2 {
                                let b = &(*sa).sa_data;
                                ip = format!("{}.{}.{}.{}", b[2] as u8, b[3] as u8, b[4] as u8, b[5] as u8);
                                break;
                            }
                            ua = (*ua).Next;
                        }
                        if !ip.is_empty() && !ip.starts_with("169.254") {
                            info.connected = true;
                            info.kind = if a.IfType == 71 { "Wi-Fi".into() } else { "Ethernet".into() };
                            info.ip = ip;
                            info.adapter = a.FriendlyName.to_string().unwrap_or_default();
                            let sp = a.TransmitLinkSpeed;
                            if sp > 0 && sp != u64::MAX {
                                info.speed_mbps = Some(sp / 1_000_000);
                            }
                            if a.IfType == 71 || info.kind == "Wi-Fi" {
                                break;
                            }
                        }
                    }
                    cur = a.Next;
                }
            }
        }
    }
    if let Some((ssid, sig)) = wlan_current() {
        info.ssid = ssid;
        info.signal = Some(sig);
    }
    info
}

fn wlan_current() -> Option<(String, u32)> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::NetworkManagement::WiFi::*;
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return None;
        }
        let mut out = None;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let n = (*list).dwNumberOfItems as usize;
            let base = (*list).InterfaceInfo.as_ptr();
            for i in 0..n {
                let it = &*base.add(i);
                if it.isState != wlan_interface_state_connected {
                    continue;
                }
                let mut sz = 0u32;
                let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
                if WlanQueryInterface(h, &it.InterfaceGuid, wlan_intf_opcode_current_connection, None, &mut sz, &mut data, None) == 0 && !data.is_null() {
                    let at = &*(data as *const WLAN_CONNECTION_ATTRIBUTES);
                    let ss = &at.wlanAssociationAttributes.dot11Ssid;
                    let len = (ss.uSSIDLength as usize).min(32);
                    let name = String::from_utf8_lossy(&ss.ucSSID[..len]).to_string();
                    out = Some((name, at.wlanAssociationAttributes.wlanSignalQuality));
                    WlanFreeMemory(data);
                    break;
                }
            }
            WlanFreeMemory(list as *const _);
        }
        let _ = WlanCloseHandle(h, None);
        out
    }
}

pub struct BatInfo {
    pub pct: u8,
    pub ac: bool,
    pub charging: bool,
    pub saver: bool,
    pub secs_left: Option<u32>,
}

pub fn battery_details() -> Option<BatInfo> {
    unsafe {
        let mut s = windows::Win32::System::Power::SYSTEM_POWER_STATUS::default();
        windows::Win32::System::Power::GetSystemPowerStatus(&mut s).ok()?;
        if s.BatteryFlag & 128 != 0 || s.BatteryLifePercent == 255 {
            return None;
        }
        Some(BatInfo {
            pct: s.BatteryLifePercent,
            ac: s.ACLineStatus == 1,
            charging: s.BatteryFlag & 8 != 0,
            saver: s.SystemStatusFlag == 1,
            secs_left: if s.BatteryLifeTime != u32::MAX { Some(s.BatteryLifeTime) } else { None },
        })
    }
}

pub fn trim_memory() {
    unsafe {
        let _ = windows::Win32::System::Memory::SetProcessWorkingSetSizeEx(GetCurrentProcess(), usize::MAX, usize::MAX, windows::Win32::System::Memory::SETPROCESSWORKINGSETSIZEEX_FLAGS(0));
    }
}

pub fn send_win_key() {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    unsafe {
        let mk = |up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VK_LWIN, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
        };
        let inputs = [mk(false), mk(true)];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn launch(path: &str) {
    unsafe {
        let wp = wide(path);
        ShellExecuteW(None, w!("open"), PCWSTR(wp.as_ptr()), PCWSTR::null(), PCWSTR::null(), SW_SHOWNORMAL);
    }
}

pub fn activate(hwnd: isize) {
    unsafe {
        let h = HWND(hwnd as *mut _);
        if IsIconic(h).as_bool() {
            let _ = ShowWindowAsync(h, SW_RESTORE);
        }
        let _ = SetForegroundWindow(h);
    }
}

pub fn minimize(hwnd: isize) {
    unsafe {
        let _ = ShowWindowAsync(HWND(hwnd as *mut _), SW_MINIMIZE);
    }
}

pub fn close_window(hwnd: isize) {
    unsafe {
        let _ = PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
}

pub fn set_transitions_disabled(hwnd: isize, off: bool) {
    unsafe {
        let v: BOOL = if off { TRUE } else { FALSE };
        let _ = DwmSetWindowAttribute(HWND(hwnd as *mut _), DWMWA_TRANSITIONS_FORCEDISABLED, &v as *const BOOL as *const _, 4);
    }
}

pub fn monitor_rect_primary() -> RECT {
    unsafe {
        let pt = POINT { x: 0, y: 0 };
        let m = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(m, &mut mi);
        mi.rcMonitor
    }
}

fn ext_bounds(h: HWND, fallback: RECT) -> RECT {
    let mut r = RECT::default();
    unsafe {
        if DwmGetWindowAttribute(h, DWMWA_EXTENDED_FRAME_BOUNDS, &mut r as *mut RECT as *mut _, std::mem::size_of::<RECT>() as u32).is_ok() && r.right > r.left {
            return r;
        }
    }
    fallback
}

fn os_fullscreen_state() -> bool {
    unsafe { matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_BUSY || s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE) }
}

fn covers(r: RECT, mon: RECT, tol: i32) -> bool {
    r.left <= mon.left + tol && r.top <= mon.top + tol && r.right >= mon.right - tol && r.bottom >= mon.bottom - tol
}

/// Ringkasan keputusan layar penuh (untuk diagnosa).
pub fn fullscreen_report(own: &[isize], mon: RECT) -> String {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() {
            return "tidak ada jendela depan".into();
        }
        let mut r = RECT::default();
        let _ = GetWindowRect(h, &mut r);
        let v = ext_bounds(h, r);
        let style = GetWindowLongW(h, GWL_STYLE) as u32;
        let ex = GetWindowLongW(h, GWL_EXSTYLE) as u32;
        format!(
            "kelas={} judul={:?}\nstyle={:#x} exstyle={:#x} zoomed={} iconic={}\nrect=({},{},{},{}) visual=({},{},{},{})\nmonitor=({},{},{},{})\nstatus OS layar-penuh={}\nkeputusan layar-penuh={}\n",
            class_name(h), window_title(h), style, ex, IsZoomed(h).as_bool(), IsIconic(h).as_bool(),
            r.left, r.top, r.right, r.bottom, v.left, v.top, v.right, v.bottom, mon.left, mon.top, mon.right, mon.bottom,
            os_fullscreen_state(), foreground_is_fullscreen(own, mon)
        )
    }
}

/// Deteksi aplikasi layar penuh di monitor `mon`.
pub fn foreground_is_fullscreen(own: &[isize], mon: RECT) -> bool {
    unsafe {
        let h = GetForegroundWindow();
        if h.0.is_null() || own.contains(&(h.0 as isize)) || !IsWindowVisible(h).as_bool() || IsIconic(h).as_bool() {
            return false;
        }
        let cls = class_name(h);
        if matches!(cls.as_str(), "#32769" | "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "ForegroundStaging" | "XamlExplorerHostIslandWindow" | "Windows.UI.Core.CoreWindow")
            || cls.starts_with("LiteDock")
        {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(h, &mut r).is_err() {
            return false;
        }
        let v = ext_bounds(h, r);
        let os_fs = os_fullscreen_state();
        let covers_any = covers(r, mon, 1) || covers(v, mon, 1);
        if !covers_any {
            // OS menyatakan layar penuh dan jendela menutupi >= 90% monitor
            if os_fs {
                let iw = (r.right.min(mon.right) - r.left.max(mon.left)).max(0) as i64;
                let ih = (r.bottom.min(mon.bottom) - r.top.max(mon.top)).max(0) as i64;
                let ma = ((mon.right - mon.left) as i64 * (mon.bottom - mon.top) as i64).max(1);
                return iw * ih * 10 >= ma * 9;
            }
            return false;
        }
        // persis sama dengan monitor (tanpa bingkai tak terlihat) = layar penuh sungguhan
        let exact = (r.left - mon.left).abs() <= 1 && (r.top - mon.top).abs() <= 1 && (r.right - mon.right).abs() <= 1 && (r.bottom - mon.bottom).abs() <= 1;
        if exact {
            return true;
        }
        let style = GetWindowLongW(h, GWL_STYLE) as u32;
        let has_caption = (style & WS_CAPTION.0) == WS_CAPTION.0;
        if !has_caption {
            // jendela maksimal berbingkai kustom (mis. Electron) bukan layar penuh
            if IsZoomed(h).as_bool() && (style & WS_THICKFRAME.0) != 0 {
                return os_fs;
            }
            return true;
        }
        os_fs
    }
}

/// Kirim kombinasi Win+<tombol> (mis. Win+A = Pengaturan Cepat, Win+N = Pusat Notifikasi).
pub fn send_win_combo(vk: u16) {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;
    unsafe {
        let key = |code: u16, up: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(code), wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
        };
        let inputs = [key(VK_LWIN.0, false), key(vk, false), key(vk, true), key(VK_LWIN.0, true)];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
}

pub fn icon_tray_handle() -> Option<HICON> {
    unsafe {
        let hi = windows::Win32::System::LibraryLoader::GetModuleHandleW(PCWSTR::null()).ok()?;
        LoadIconW(Some(hi.into()), PCWSTR(1 as *const u16)).ok()
    }
}

pub fn _unused() -> Option<windows::core::GUID> {
    let _ = IImageList::IID;
    None
}
