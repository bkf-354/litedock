//! Primitif UI bergaya macOS: warna sistem, switch hijau, slider pil tebal ala Pusat Kontrol.
//! Murni (hanya Canvas dan Rgba), sehingga dapat diuji tanpa Windows.
use crate::gfxcore::{Canvas, Rgba};

/// Warna sistem macOS (varian gelap / terang).
pub fn red(light: bool) -> Rgba {
    if light { Rgba(255, 59, 48, 255) } else { Rgba(255, 69, 58, 255) }
}
pub fn green(light: bool) -> Rgba {
    if light { Rgba(52, 199, 89, 255) } else { Rgba(48, 209, 88, 255) }
}

/// Switch ala macOS: jalur pil 38x22 (dikali `k`), kenop putih dengan bayangan tipis.
/// `t` = 0..1 posisi animasi (0 mati, 1 nyala). `(cx, cy)` = titik tengah switch.
pub fn switch(cv: &mut Canvas, cx: f32, cy: f32, k: f32, t: f32, on_col: Rgba, off_col: Rgba) {
    let (w, h) = (38.0 * k, 22.0 * k);
    let t = t.clamp(0.0, 1.0);
    let col = off_col.lerp(on_col, t);
    cv.fill_rrect(cx - w / 2.0, cy - h / 2.0, w, h, h / 2.0, col, col);
    let kr = h / 2.0 - 2.0 * k;
    let kx = cx - w / 2.0 + h / 2.0 + (w - h) * t;
    // bayangan kenop
    cv.fill_circle(kx, cy + 1.0 * k, kr + 0.6 * k, Rgba(0, 0, 0, 46));
    cv.fill_circle(kx, cy, kr, Rgba(255, 255, 255, 255));
}

/// Slider pil tebal ala Pusat Kontrol macOS: jalur redup, isi terang dari kiri, tanpa kenop.
/// Isi minimal selebar tinggi pil agar ikon di dalamnya selalu terlihat.
pub fn pill_slider(cv: &mut Canvas, x: f32, y: f32, w: f32, h: f32, frac: f32, track: Rgba, fill: Rgba) {
    cv.fill_rrect(x, y, w, h, h / 2.0, track, track);
    let fw = (w * frac.clamp(0.0, 1.0)).max(h);
    cv.fill_rrect(x, y, fw, h, h / 2.0, fill, fill);
}
