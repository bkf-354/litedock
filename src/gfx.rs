//! Renderer perangkat lunak ringan: piksel premultiplied BGRA (0xAARRGGBB), anti-alias via SDF.
//! Tidak memakai GPU/WebView -> memori kecil, CPU hanya terpakai saat animasi.
use std::collections::HashMap;
use windows::core::PCWSTR;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
pub use crate::gfxcore::*;

// ---------------------------------------------------------------- Surface (DIB section)

pub struct Surface {
    pub dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u32,
    pub w: i32,
    pub h: i32,
}

impl Surface {
    pub fn new(w: i32, h: i32) -> Option<Surface> {
        let (w, h) = (w.max(1), h.max(1));
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bi = BITMAPINFO::default();
            bi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bi.bmiHeader.biWidth = w;
            bi.bmiHeader.biHeight = -h;
            bi.bmiHeader.biPlanes = 1;
            bi.bmiHeader.biBitCount = 32;
            bi.bmiHeader.biCompression = BI_RGB.0;
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let bmp = CreateDIBSection(Some(dc), &bi, DIB_RGB_COLORS, &mut bits, None, 0);
            match bmp {
                Ok(bmp) if !bits.is_null() => {
                    let old = SelectObject(dc, bmp.into());
                    Some(Surface { dc, bmp, old, bits: bits as *mut u32, w, h })
                }
                _ => {
                    let _ = DeleteDC(dc);
                    None
                }
            }
        }
    }

    pub fn px(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
    }

    pub fn canvas(&mut self) -> Canvas<'_> {
        let (w, h) = (self.w, self.h);
        Canvas { w, h, px: self.px() }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(self.bmp.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

// ---------------------------------------------------------------- Canvas

// ---------------------------------------------------------------- Teks (GDI -> mask cakupan)


/// Jenis font global: 0 = Segoe UI, 1 = Roboto, 2 = Inter (gaya SF Pro; Roboto dan Inter disertakan di dalam aplikasi).
static FONT_FACE: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(2);

pub fn set_font_face(face: i32) {
    FONT_FACE.store(face.clamp(0, 2), std::sync::atomic::Ordering::Relaxed);
}

/// Ikon sistem: false = vektor bergaya macOS (default), true = glif Segoe Fluent Icons bawaan Windows.
static WIN_ICONS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_win_icons(on: bool) {
    WIN_ICONS.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Nama keluarga GDI + bobot untuk Inter. Inter statis dipecah per bobot (nama keluarga "Inter Medium", "Inter SemiBold", ...),
/// jadi bobot yang diminta dipetakan ke keluarga yang tepat dan GDI tidak membuat tebal buatan.
fn inter_face(weight: i32) -> (windows::core::PCWSTR, i32) {
    use windows::core::w;
    match weight {
        ..=360 => (w!("Inter Display Light"), 400),
        361..=450 => (w!("Inter"), 400),
        451..=550 => (w!("Inter Medium"), 400),
        551..=650 => (w!("Inter SemiBold"), 400),
        _ => (w!("Inter"), 700),
    }
}

/// Bobot untuk teks yang perlu sedikit menonjol: Segoe UI Semibold (600). Roboto hanya punya Regular dan Bold
/// (Bold terlalu tebal), jadi memakai Regular; hierarki tetap dibentuk oleh ukuran dan warna.
pub fn semi_weight() -> i32 {
    if FONT_FACE.load(std::sync::atomic::Ordering::Relaxed) == 1 {
        400
    } else {
        600
    }
}

/// Bobot teks dock (jam, tanggal, persen). Inter: Medium agar terbaca di atas kaca seperti menu bar macOS.
pub fn dock_weight(bold: bool) -> i32 {
    match (FONT_FACE.load(std::sync::atomic::Ordering::Relaxed), bold) {
        (2, true) => 600,
        (2, false) => 500,
        (_, true) => 700,
        (_, false) => 400,
    }
}

/// Bobot teks panel (flyout dan pengaturan): tebal hanya untuk judul dan angka besar, dan itu pun semibold (600) atau semilight
/// (350, angka besar agar anggun); teks kecil tetap regular. Roboto hanya punya Regular/Bold, jadi selalu Regular.
pub fn panel_weight(bold: bool, px: i32) -> i32 {
    let face = FONT_FACE.load(std::sync::atomic::Ordering::Relaxed);
    if face == 2 {
        // Inter: angka besar tipis (Display Light), judul semibold, label kecil medium, teks biasa regular
        return if !bold {
            400
        } else if px >= 26 {
            300
        } else if px >= 15 {
            600
        } else if px >= 12 {
            500
        } else {
            600
        };
    }
    if !bold || face == 1 {
        400
    } else if px >= 26 {
        350
    } else if px >= 15 || px <= 11 {
        600
    } else {
        400
    }
}

/// Font ikon bawaan Windows: Segoe Fluent Icons (Windows 11), cadangan Segoe MDL2 Assets (Windows 10).
/// None bila keduanya tidak terpasang; pemanggil lalu memakai ikon vektor.
pub fn icon_font_name() -> Option<&'static [u16]> {
    use std::sync::OnceLock;
    if !WIN_ICONS.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    static NAME: OnceLock<Option<Vec<u16>>> = OnceLock::new();
    NAME.get_or_init(|| unsafe {
        for (value, face) in [("Segoe Fluent Icons (TrueType)", "Segoe Fluent Icons"), ("Segoe MDL2 Assets (TrueType)", "Segoe MDL2 Assets")] {
            let v: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
            let mut sz = 0u32;
            let r = RegGetValueW(
                HKEY_LOCAL_MACHINE,
                windows::core::w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts"),
                PCWSTR(v.as_ptr()),
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut sz),
            );
            if r.is_ok() && sz > 2 {
                return Some(face.encode_utf16().chain(std::iter::once(0)).collect());
            }
        }
        None
    })
    .as_deref()
}


static ROBOTO_REGULAR: &[u8] = include_bytes!("../assets/fonts/Roboto-Regular.ttf");
static ROBOTO_BOLD: &[u8] = include_bytes!("../assets/fonts/Roboto-Bold.ttf");
static INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
static INTER_MEDIUM: &[u8] = include_bytes!("../assets/fonts/Inter-Medium.ttf");
static INTER_SEMIBOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");
static INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-Bold.ttf");
static INTER_DISPLAY_LIGHT: &[u8] = include_bytes!("../assets/fonts/InterDisplay-Light.ttf");

/// Daftarkan Roboto dan Inter sebagai font privat proses ini (tidak menginstal apa pun ke Windows).
pub fn register_fonts() {
    unsafe {
        for data in [ROBOTO_REGULAR, ROBOTO_BOLD, INTER_REGULAR, INTER_MEDIUM, INTER_SEMIBOLD, INTER_BOLD, INTER_DISPLAY_LIGHT] {
            let mut n = 0u32;
            let _ = AddFontMemResourceEx(data.as_ptr() as *const _, data.len() as u32, None, &mut n);
        }
    }
}

#[derive(Default)]
pub struct TextCache {
    map: HashMap<(String, i32, i32, i32), Mask>,
    fonts: HashMap<(i32, i32, i32), HFONT>,
}

impl TextCache {
    pub fn get(&mut self, text: &str, px: i32, bold: bool) -> &Mask {
        self.get_w(text, px, if bold { 700 } else { 400 })
    }

    /// Semi-bold (600) untuk teks jam/tanggal ala taskbar Windows.
    pub fn get_semi(&mut self, text: &str, px: i32) -> &Mask {
        self.get_w(text, px, 600)
    }

    /// Satu glif dari font ikon Windows (None bila font tidak ada).
    pub fn get_icon(&mut self, ch: char, px: i32) -> Option<&Mask> {
        let name = icon_font_name()?;
        let key = (ch.to_string(), px, -1, 100);
        if !self.map.contains_key(&key) {
            if self.map.len() > 1500 {
                self.map.clear();
            }
            let font = *self.fonts.entry((px, -1, 100)).or_insert_with(|| unsafe {
                CreateFontW(-px, 0, 0, 0, 400, 0, 0, 0, DEFAULT_CHARSET, OUT_TT_PRECIS, CLIP_DEFAULT_PRECIS, ANTIALIASED_QUALITY, 0, PCWSTR(name.as_ptr()))
            });
            let m = render_mask(&ch.to_string(), font);
            self.map.insert(key.clone(), m);
        }
        self.map.get(&key)
    }

    /// Gambar glif ikon Windows rata tengah pada (cx, cy). Posisi dihitung dari kotak tinta glif acuan `refch`
    /// (mis. Wi-Fi penuh), sehingga glif yang ditumpuk (redup + aktif) tetap sejajar. false bila font ikon tidak ada.
    pub fn draw_glyph(&mut self, cv: &mut Canvas, ch: char, refch: char, px: i32, cx: f32, cy: f32, col: Rgba) -> bool {
        let Some(ink) = self.get_icon(refch, px).and_then(|m| m.ink()) else { return false };
        let ox = cx - (ink.0 + ink.2 + 1) as f32 / 2.0;
        let oy = cy - (ink.1 + ink.3 + 1) as f32 / 2.0;
        match self.get_icon(ch, px) {
            Some(m) => {
                cv.draw_mask(m, ox.round() as i32, oy.round() as i32, col);
                true
            }
            None => false,
        }
    }

    pub fn get_w(&mut self, text: &str, px: i32, weight: i32) -> &Mask {
        let face = FONT_FACE.load(std::sync::atomic::Ordering::Relaxed);
        let key = (text.to_string(), px, weight, face);
        if !self.map.contains_key(&key) {
            // batas longgar: jendela pengaturan memakai ratusan string; membersihkan terlalu sering membuat setiap repaint merender ulang semua teks
            if self.map.len() > 1500 {
                self.map.clear();
            }
            let (fname, fweight) = match face {
                2 => inter_face(weight),
                1 => (windows::core::w!("Roboto"), weight),
                _ => (windows::core::w!("Segoe UI"), weight),
            };
            let font = *self.fonts.entry((px, weight, face)).or_insert_with(|| unsafe {
                CreateFontW(
                    -px,
                    0,
                    0,
                    0,
                    fweight,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET,
                    OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS,
                    ANTIALIASED_QUALITY,
                    0,
                    fname,
                )
            });
            let m = render_mask(text, font);
            self.map.insert(key.clone(), m);
        }
        &self.map[&key]
    }
}

impl Drop for TextCache {
    fn drop(&mut self) {
        for (_, f) in self.fonts.drain() {
            unsafe {
                let _ = DeleteObject(f.into());
            }
        }
    }
}

fn render_mask(text: &str, font: HFONT) -> Mask {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return Mask { w: 0, h: 0, a: Vec::new() };
    }
    unsafe {
        let probe = CreateCompatibleDC(None);
        let old_f = SelectObject(probe, font.into());
        let mut sz = SIZE::default();
        let _ = GetTextExtentPoint32W(probe, &wide, &mut sz);
        SelectObject(probe, old_f);
        let _ = DeleteDC(probe);
        let (w, h) = (sz.cx + 4, sz.cy + 2);
        let Some(mut s) = Surface::new(w, h) else {
            return Mask { w: 0, h: 0, a: Vec::new() };
        };
        let old_f = SelectObject(s.dc, font.into());
        SetBkMode(s.dc, TRANSPARENT);
        SetTextColor(s.dc, windows::Win32::Foundation::COLORREF(0x00FF_FFFF));
        let _ = TextOutW(s.dc, 2, 1, &wide);
        SelectObject(s.dc, old_f);
        let a: Vec<u8> = s.px().iter().map(|p| (*p & 0xFF) as u8).collect();
        let _ = PCWSTR::null();
        Mask { w, h, a }
    }
}
