//! Ikon vektor bergaya Fluent / Windows 11. Semua ukuran relatif terhadap `s` (lebar kotak ikon, piksel),
//! sehingga ikon Wi-Fi, baterai, tray, dan panel selalu proporsional satu sama lain.
use crate::gfx::{Canvas, Rgba};

fn stroke_w(s: f32, k: f32) -> f32 {
    (s * k).max(1.25)
}

// ------------------------------------------------------------------ jaringan

/// Wi-Fi: titik + 3 busur. `lit` = jumlah busur menyala (0..=3), sisanya diredupkan.
pub fn wifi(cv: &mut Canvas, cx: f32, cy: f32, s: f32, lit: u8, col: Rgba, dim: Rgba) {
    let th = stroke_w(s, 0.125);
    let (ox, oy) = (cx, cy + 0.36 * s);
    for k in (1..=3u8).rev() {
        let r = 0.205 * s * k as f32;
        let c = if k <= lit { col } else { dim };
        cv.arc_cap(ox, oy, r, th, 226.0, 314.0, c);
    }
    cv.fill_circle(ox, oy, (th * 0.78).max(1.2), col);
}

/// Wi-Fi terputus: kipas redup + garis coret.
pub fn wifi_off(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba, dim: Rgba) {
    wifi(cv, cx, cy, s, 0, dim, dim);
    let th = stroke_w(s, 0.11);
    cv.line(cx - 0.44 * s, cy - 0.40 * s, cx + 0.44 * s, cy + 0.46 * s, th, col);
}

/// Kabel jaringan (Ethernet).
pub fn ethernet(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.10);
    let (w, h) = (0.86 * s, 0.50 * s);
    let (x, y) = (cx - w / 2.0, cy - 0.40 * s);
    cv.stroke_rrect(x, y, w, h, 0.12 * s, th, col);
    for i in 1..=3 {
        let tx = x + w * i as f32 / 4.0;
        cv.line(tx, y + h - th * 0.4, tx, y + h * 0.42, th * 0.85, col);
    }
    cv.line(cx, y + h, cx, cy + 0.36 * s, th, col);
    cv.line(cx - 0.26 * s, cy + 0.38 * s, cx + 0.26 * s, cy + 0.38 * s, th, col);
}

/// Mode pesawat.
pub fn airplane(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let u = s / 24.0;
    let p = |x: f32, y: f32| (cx + (x - 12.0) * u, cy + (y - 12.0) * u);
    let pts = [
        p(12.0, 1.5), p(13.6, 4.2), p(13.6, 9.0), p(22.0, 14.0), p(22.0, 16.4), p(13.6, 13.4), p(13.6, 18.2), p(16.4, 20.4),
        p(16.4, 22.0), p(12.0, 20.8), p(7.6, 22.0), p(7.6, 20.4), p(10.4, 18.2), p(10.4, 13.4), p(2.0, 16.4), p(2.0, 14.0),
        p(10.4, 9.0), p(10.4, 4.2),
    ];
    cv.fill_poly(&pts, col);
}

/// Hotspot seluler: titik + gelombang kiri/kanan.
pub fn hotspot(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.10);
    cv.fill_circle(cx, cy, 0.10 * s, col);
    for r in [0.24f32, 0.40] {
        cv.arc_cap(cx, cy, r * s, th, 322.0, 360.0, col);
        cv.arc_cap(cx, cy, r * s, th, 0.0, 38.0, col);
        cv.arc_cap(cx, cy, r * s, th, 142.0, 218.0, col);
    }
}

/// Gembok kecil (jaringan aman).
pub fn lock(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.13);
    let (bw, bh) = (0.64 * s, 0.46 * s);
    cv.fill_rrect(cx - bw / 2.0, cy - 0.02 * s, bw, bh, 0.10 * s, col, col);
    cv.arc_cap(cx, cy - 0.02 * s, 0.19 * s, th, 180.0, 360.0, col);
    cv.line(cx - 0.19 * s, cy - 0.02 * s, cx - 0.19 * s, cy + 0.08 * s, th, col);
    cv.line(cx + 0.19 * s, cy - 0.02 * s, cx + 0.19 * s, cy + 0.08 * s, th, col);
}

// ------------------------------------------------------------------ daya

pub fn bolt(cv: &mut Canvas, cx: f32, cy: f32, h: f32, col: Rgba) {
    let u = h / 2.0;
    let p = |x: f32, y: f32| (cx + x * u * 0.62, cy + y * u);
    let pts = [p(0.30, -1.0), p(-0.62, 0.18), p(-0.04, 0.18), p(-0.30, 1.0), p(0.62, -0.22), p(0.04, -0.22)];
    cv.fill_poly(&pts, col);
}

/// Baterai mendatar cadangan (dipakai bila font ikon Windows 11 tidak ada): garis tepi tebal berwarna `line`,
/// isi level berwarna `fill`, tonjolan kutub di kanan. Semua ukuran dibulatkan ke piksel utuh agar tegas. `s` = lebar badan.
pub fn battery(cv: &mut Canvas, cx: f32, cy: f32, s: f32, pct: u8, charging: bool, _saver: bool, fill: Rgba, line: Rgba) {
    let th = (s * 0.10).round().max(2.0);
    let w = s.round();
    let h = (s * 0.58).round().max(10.0);
    let nub_w = (s * 0.09).round().max(2.0);
    let total = w + nub_w + 1.0;
    let x = (cx - total / 2.0).round();
    let y = (cy - h / 2.0).round();
    let r = (h * 0.28).max(2.0);
    cv.stroke_rrect(x, y, w, h, r, th, line);
    let nh = (h * 0.38).round().max(3.0);
    let ny = (cy - nh / 2.0).round();
    cv.fill_rrect(x + w + 1.0, ny, nub_w, nh, nub_w * 0.5, line, line);
    let inset = th + 1.0;
    let (ix, iy, iw, ih) = (x + inset, y + inset, w - 2.0 * inset, h - 2.0 * inset);
    let fw = (iw * (pct as f32 / 100.0)).clamp(2.0, iw).round();
    cv.fill_rrect(ix, iy, fw, ih, (ih * 0.22).max(1.0), fill, fill);
    if charging {
        // petir dengan tepi kontras agar terbaca di atas isi maupun bagian kosong
        bolt(cv, x + w * 0.5, cy, h * 0.78, Rgba(255, 255, 255, 235));
        bolt(cv, x + w * 0.5, cy, h * 0.62, Rgba(20, 20, 24, 235));
    }
}

// ------------------------------------------------------------------ umum

pub fn chevron_up(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.14);
    cv.line(cx - 0.30 * s, cy + 0.12 * s, cx, cy - 0.16 * s, th, col);
    cv.line(cx, cy - 0.16 * s, cx + 0.30 * s, cy + 0.12 * s, th, col);
}

pub fn chevron(cv: &mut Canvas, cx: f32, cy: f32, s: f32, dir: f32, col: Rgba) {
    let th = stroke_w(s, 0.14);
    cv.line(cx - dir * 0.12 * s, cy - 0.28 * s, cx + dir * 0.14 * s, cy, th, col);
    cv.line(cx + dir * 0.14 * s, cy, cx - dir * 0.12 * s, cy + 0.28 * s, th, col);
}

pub fn calendar(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.085);
    let (w, h) = (0.82 * s, 0.78 * s);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0 + 0.04 * s);
    cv.stroke_rrect(x, y, w, h, 0.14 * s, th, col);
    cv.fill_rrect(x, y, w, h * 0.30, 0.14 * s, col, col);
    cv.line(x + w * 0.28, y - 0.07 * s, x + w * 0.28, y + 0.06 * s, th, col);
    cv.line(x + w * 0.72, y - 0.07 * s, x + w * 0.72, y + 0.06 * s, th, col);
    for i in 0..3 {
        cv.fill_circle(x + w * (0.25 + 0.25 * i as f32), y + h * 0.66, (0.045 * s).max(0.9), col);
    }
}

pub fn check(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.13);
    cv.line(cx - 0.26 * s, cy + 0.02 * s, cx - 0.07 * s, cy + 0.22 * s, th, col);
    cv.line(cx - 0.07 * s, cy + 0.22 * s, cx + 0.28 * s, cy - 0.20 * s, th, col);
}

/// Daun/penghemat baterai: baterai kecil dengan tanda plus.
pub fn battery_saver(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.09);
    let (w, h) = (0.80 * s, 0.50 * s);
    cv.stroke_rrect(cx - w / 2.0 - 0.04 * s, cy - h / 2.0, w, h, 0.14 * s, th, col);
    cv.fill_rrect(cx + w / 2.0 - 0.01 * s, cy - 0.10 * s, 0.07 * s, 0.20 * s, 0.03 * s, col, col);
    cv.line(cx - 0.18 * s, cy, cx + 0.10 * s, cy, th, col);
    cv.line(cx - 0.04 * s, cy - 0.14 * s, cx - 0.04 * s, cy + 0.14 * s, th, col);
}

// ------------------------------------------------------------------ waktu salat

fn star(cv: &mut Canvas, cx: f32, cy: f32, r: f32, col: Rgba) {
    let k = 0.28;
    let pts = [(cx, cy - r), (cx + r * k, cy - r * k), (cx + r, cy), (cx + r * k, cy + r * k), (cx, cy + r), (cx - r * k, cy + r * k), (cx - r, cy), (cx - r * k, cy - r * k)];
    cv.fill_poly(&pts, col);
}

fn rays(cv: &mut Canvas, cx: f32, cy: f32, r0: f32, r1: f32, a0: f32, a1: f32, n: usize, th: f32, col: Rgba) {
    for i in 0..n {
        let a = (a0 + (a1 - a0) * if n > 1 { i as f32 / (n - 1) as f32 } else { 0.5 }).to_radians();
        cv.line(cx + r0 * a.cos(), cy + r0 * a.sin(), cx + r1 * a.cos(), cy + r1 * a.sin(), th, col);
    }
}

/// Dua warna ikon salat (emas dan oranye untuk siang; emas dan biru untuk malam), sedikit lebih pekat di latar terang.
fn duo(light_bg: bool) -> (Rgba, Rgba, Rgba) {
    if light_bg {
        (Rgba(244, 170, 22, 255), Rgba(238, 104, 28, 255), Rgba(84, 112, 224, 255))
    } else {
        (Rgba(255, 200, 58, 255), Rgba(255, 134, 52, 255), Rgba(132, 164, 255, 255))
    }
}

/// Warna aksen per waktu salat (kapsul dock, bilah kemajuan, kartu banner): 0 Subuh, 1 Terbit, 2 Dzuhur, 3 Ashar, 4 Maghrib, 5 Isya.
pub fn prayer_accent(idx: usize) -> Rgba {
    match idx {
        0 => Rgba(128, 156, 255, 255),
        1 => Rgba(255, 172, 64, 255),
        2 => Rgba(255, 200, 58, 255),
        3 => Rgba(255, 150, 58, 255),
        4 => Rgba(255, 116, 86, 255),
        _ => Rgba(122, 132, 238, 255),
    }
}

/// Ikon waktu salat dua warna, tanpa kotak latar dan tanpa gradien: dipakai di dock, panel jadwal, dan banner.
/// Tiap ikon hanya memakai dua warna dari `duo` agar tetap bersih dan konsisten di semua ukuran.
pub fn prayer_color(cv: &mut Canvas, idx: usize, cx: f32, cy: f32, s: f32, light_bg: bool) {
    let th = stroke_w(s, 0.085);
    let (gold, orange, blue) = duo(light_bg);
    match idx {
        0 => {
            // Subuh: matahari baru mengintip di ufuk (oranye), bintang fajar (biru)
            cv.fill_pie(cx, cy + 0.17 * s, 0.17 * s, 180.0, 360.0, orange);
            cv.line(cx - 0.36 * s, cy + 0.19 * s, cx + 0.36 * s, cy + 0.19 * s, th, orange);
            star(cv, cx - 0.15 * s, cy - 0.22 * s, 0.14 * s, blue);
            star(cv, cx + 0.20 * s, cy - 0.08 * s, 0.09 * s, blue);
        }
        1 => {
            // Terbit: setengah matahari (emas) + sinar dan ufuk (oranye)
            cv.fill_pie(cx, cy + 0.14 * s, 0.21 * s, 180.0, 360.0, gold);
            cv.line(cx - 0.38 * s, cy + 0.16 * s, cx + 0.38 * s, cy + 0.16 * s, th, orange);
            rays(cv, cx, cy + 0.14 * s, 0.31 * s, 0.44 * s, 205.0, 335.0, 5, th, orange);
        }
        2 => {
            // Dzuhur: matahari penuh (emas) + sinar (oranye)
            cv.fill_circle(cx, cy, 0.19 * s, gold);
            rays(cv, cx, cy, 0.29 * s, 0.42 * s, 0.0, 315.0, 8, th, orange);
        }
        3 => {
            // Ashar: matahari rendah (emas) di balik awan (biru)
            cv.fill_circle(cx - 0.10 * s, cy - 0.11 * s, 0.15 * s, gold);
            rays(cv, cx - 0.10 * s, cy - 0.11 * s, 0.24 * s, 0.34 * s, 180.0, 300.0, 4, th, gold);
            cloud(cv, cx + 0.06 * s, cy + 0.15 * s, 0.64 * s, blue);
        }
        4 => {
            // Maghrib: matahari terbenam (oranye) + garis pantulan (emas)
            cv.fill_pie(cx, cy + 0.06 * s, 0.22 * s, 180.0, 360.0, orange);
            cv.line(cx - 0.38 * s, cy + 0.08 * s, cx + 0.38 * s, cy + 0.08 * s, th, gold);
            cv.line(cx - 0.22 * s, cy + 0.20 * s, cx + 0.22 * s, cy + 0.20 * s, th, gold.mul_a(0.8));
            cv.line(cx - 0.11 * s, cy + 0.31 * s, cx + 0.11 * s, cy + 0.31 * s, th, gold.mul_a(0.6));
            rays(cv, cx, cy + 0.06 * s, 0.32 * s, 0.43 * s, 215.0, 325.0, 3, th, orange);
        }
        _ => {
            // Isya: bulan sabit (emas) + bintang (biru)
            cv.crescent(cx - 0.05 * s, cy + 0.01 * s, 0.27 * s, 0.14 * s, -0.10 * s, 0.23 * s, gold);
            star(cv, cx + 0.23 * s, cy - 0.17 * s, 0.12 * s, blue);
            star(cv, cx + 0.13 * s, cy + 0.16 * s, 0.07 * s, blue);
        }
    }
}

fn cloud(cv: &mut Canvas, cx: f32, cy: f32, w: f32, col: Rgba) {
    let u = w / 10.0;
    cv.fill_circle(cx - 2.6 * u, cy + 0.6 * u, 2.2 * u, col);
    cv.fill_circle(cx - 0.2 * u, cy - 0.8 * u, 3.0 * u, col);
    cv.fill_circle(cx + 2.8 * u, cy + 0.4 * u, 2.4 * u, col);
    cv.fill_rrect(cx - 4.8 * u, cy + 0.2 * u, 9.6 * u, 2.7 * u, 1.35 * u, col, col);
}

// ------------------------------------------------------------------ ikon modul tambahan

/// Tempat sampah (Recycle Bin); `full` menampilkan kertas menyembul.
pub fn trash(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba, full: bool) {
    let th = stroke_w(s, 0.085);
    let (w, h) = (0.52 * s, 0.56 * s);
    let (x, y) = (cx - w / 2.0, cy - 0.20 * s);
    // badan menyempit ke bawah
    let pts = [(x, y), (x + w, y), (x + w * 0.88, y + h), (x + w * 0.12, y + h)];
    for i in 0..4 {
        let (a, b) = (pts[i], pts[(i + 1) % 4]);
        if i != 0 {
            cv.line(a.0, a.1, b.0, b.1, th, col);
        }
    }
    cv.line(x - 0.08 * s, y, x + w + 0.08 * s, y, th, col);
    cv.line(cx - 0.10 * s, y - 0.07 * s, cx + 0.10 * s, y - 0.07 * s, th, col);
    for k in [-0.14f32, 0.0, 0.14] {
        cv.line(cx + k * s, y + 0.12 * s, cx + k * s * 0.85, y + h - 0.10 * s, th * 0.85, col.mul_a(0.8));
    }
    if full {
        cv.line(cx - 0.12 * s, y - 0.07 * s, cx - 0.02 * s, y - 0.22 * s, th, col);
        cv.line(cx + 0.02 * s, y - 0.07 * s, cx + 0.16 * s, y - 0.18 * s, th, col);
    }
}

/// Folder File Explorer berwarna.
pub fn folder_color(cv: &mut Canvas, cx: f32, cy: f32, s: f32) {
    let (w, h) = (0.84 * s, 0.60 * s);
    let (x, y) = (cx - w / 2.0, cy - h / 2.0 + 0.02 * s);
    cv.fill_rrect(x, y - 0.07 * s, w * 0.42, 0.16 * s, 0.05 * s, Rgba(224, 160, 28, 255), Rgba(224, 160, 28, 255));
    cv.fill_rrect(x, y, w, h, 0.09 * s, Rgba(255, 206, 70, 255), Rgba(247, 176, 36, 255));
    cv.fill_rrect(x, y + h * 0.28, w, h * 0.72, 0.09 * s, Rgba(255, 224, 120, 255), Rgba(250, 190, 56, 255));
}

pub fn power(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.11);
    cv.arc_cap(cx, cy + 0.02 * s, 0.30 * s, th, 300.0, 360.0, col);
    cv.arc_cap(cx, cy + 0.02 * s, 0.30 * s, th, 0.0, 240.0, col);
    cv.line(cx, cy - 0.38 * s, cx, cy - 0.06 * s, th, col);
}

/// Pusat kontrol: dua saklar bertumpuk.
pub fn sliders(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.085);
    let (w, h) = (0.70 * s, 0.26 * s);
    let x = cx - w / 2.0;
    cv.stroke_rrect(x, cy - 0.34 * s, w, h, h / 2.0, th, col);
    cv.fill_circle(x + w - h / 2.0, cy - 0.34 * s + h / 2.0, h * 0.30, col);
    cv.stroke_rrect(x, cy + 0.06 * s, w, h, h / 2.0, th, col);
    cv.fill_circle(x + h / 2.0, cy + 0.06 * s + h / 2.0, h * 0.30, col);
}

pub fn music_note(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.10);
    cv.fill_circle(cx - 0.15 * s, cy + 0.26 * s, 0.13 * s, col);
    cv.fill_circle(cx + 0.22 * s, cy + 0.16 * s, 0.13 * s, col);
    cv.line(cx - 0.03 * s, cy + 0.24 * s, cx - 0.03 * s, cy - 0.30 * s, th, col);
    cv.line(cx + 0.34 * s, cy + 0.14 * s, cx + 0.34 * s, cy - 0.38 * s, th, col);
    cv.line(cx - 0.03 * s, cy - 0.30 * s, cx + 0.34 * s, cy - 0.38 * s, th * 1.5, col);
}

pub fn play(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    cv.fill_poly(&[(cx - 0.24 * s, cy - 0.34 * s), (cx + 0.34 * s, cy), (cx - 0.24 * s, cy + 0.34 * s)], col);
}

pub fn pause(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let w = 0.20 * s;
    cv.fill_rrect(cx - 0.30 * s, cy - 0.32 * s, w, 0.64 * s, w * 0.3, col, col);
    cv.fill_rrect(cx + 0.10 * s, cy - 0.32 * s, w, 0.64 * s, w * 0.3, col, col);
}

pub fn skip(cv: &mut Canvas, cx: f32, cy: f32, s: f32, dir: f32, col: Rgba) {
    let d = dir;
    cv.fill_poly(&[(cx - d * 0.30 * s, cy - 0.26 * s), (cx + d * 0.10 * s, cy), (cx - d * 0.30 * s, cy + 0.26 * s)], col);
    cv.fill_rrect(cx + d * 0.16 * s - 0.05 * s, cy - 0.27 * s, 0.10 * s, 0.54 * s, 0.03 * s, col, col);
}

/// Tombol Caps Lock.
pub fn keycap(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba, on: bool) {
    let th = stroke_w(s, 0.09);
    let (w, h) = (0.80 * s, 0.74 * s);
    if on {
        cv.fill_rrect(cx - w / 2.0, cy - h / 2.0, w, h, 0.16 * s, col.mul_a(0.22), col.mul_a(0.22));
    }
    cv.stroke_rrect(cx - w / 2.0, cy - h / 2.0, w, h, 0.16 * s, th, col);
    cv.fill_poly(&[(cx, cy - 0.24 * s), (cx + 0.20 * s, cy + 0.02 * s), (cx + 0.08 * s, cy + 0.02 * s), (cx + 0.08 * s, cy + 0.16 * s), (cx - 0.08 * s, cy + 0.16 * s), (cx - 0.08 * s, cy + 0.02 * s), (cx - 0.20 * s, cy + 0.02 * s)], col);
    cv.line(cx - 0.12 * s, cy + 0.27 * s, cx + 0.12 * s, cy + 0.27 * s, th, col);
}

pub fn clipboard(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.09);
    let (w, h) = (0.62 * s, 0.76 * s);
    cv.stroke_rrect(cx - w / 2.0, cy - h / 2.0 + 0.05 * s, w, h, 0.10 * s, th, col);
    cv.fill_rrect(cx - 0.17 * s, cy - h / 2.0 - 0.02 * s, 0.34 * s, 0.14 * s, 0.05 * s, col, col);
    cv.line(cx - 0.16 * s, cy + 0.06 * s, cx + 0.16 * s, cy + 0.06 * s, th * 0.85, col);
    cv.line(cx - 0.16 * s, cy + 0.20 * s, cx + 0.08 * s, cy + 0.20 * s, th * 0.85, col);
}

pub fn bell(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.09);
    cv.arc_cap(cx, cy - 0.04 * s, 0.24 * s, th, 180.0, 360.0, col);
    cv.line(cx - 0.24 * s, cy - 0.04 * s, cx - 0.30 * s, cy + 0.22 * s, th, col);
    cv.line(cx + 0.24 * s, cy - 0.04 * s, cx + 0.30 * s, cy + 0.22 * s, th, col);
    cv.line(cx - 0.34 * s, cy + 0.24 * s, cx + 0.34 * s, cy + 0.24 * s, th, col);
    cv.fill_circle(cx, cy + 0.34 * s, 0.07 * s, col);
    cv.line(cx, cy - 0.36 * s, cx, cy - 0.30 * s, th, col);
}

pub fn restart(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.10);
    cv.arc_cap(cx, cy, 0.30 * s, th, 40.0, 330.0, col);
    cv.fill_poly(&[(cx + 0.14 * s, cy - 0.40 * s), (cx + 0.40 * s, cy - 0.18 * s), (cx + 0.08 * s, cy - 0.12 * s)], col);
}

pub fn moon_sleep(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    cv.crescent(cx, cy, 0.34 * s, 0.20 * s, -0.12 * s, 0.30 * s, col);
}

pub fn signout(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.09);
    cv.line(cx - 0.30 * s, cy - 0.32 * s, cx - 0.30 * s, cy + 0.32 * s, th, col);
    cv.line(cx - 0.30 * s, cy - 0.32 * s, cx + 0.04 * s, cy - 0.32 * s, th, col);
    cv.line(cx - 0.30 * s, cy + 0.32 * s, cx + 0.04 * s, cy + 0.32 * s, th, col);
    cv.line(cx - 0.02 * s, cy, cx + 0.34 * s, cy, th, col);
    cv.line(cx + 0.18 * s, cy - 0.14 * s, cx + 0.34 * s, cy, th, col);
    cv.line(cx + 0.18 * s, cy + 0.14 * s, cx + 0.34 * s, cy, th, col);
}

pub fn volume(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba, muted: bool, level: f32) {
    let th = stroke_w(s, 0.09);
    cv.fill_poly(&[(cx - 0.34 * s, cy - 0.12 * s), (cx - 0.18 * s, cy - 0.12 * s), (cx + 0.00 * s, cy - 0.30 * s), (cx + 0.00 * s, cy + 0.30 * s), (cx - 0.18 * s, cy + 0.12 * s), (cx - 0.34 * s, cy + 0.12 * s)], col);
    if muted {
        cv.line(cx + 0.10 * s, cy - 0.14 * s, cx + 0.34 * s, cy + 0.14 * s, th, col);
        cv.line(cx + 0.34 * s, cy - 0.14 * s, cx + 0.10 * s, cy + 0.14 * s, th, col);
    } else {
        if level > 0.05 {
            cv.arc_cap(cx + 0.02 * s, cy, 0.17 * s, th, 310.0, 360.0, col);
            cv.arc_cap(cx + 0.02 * s, cy, 0.17 * s, th, 0.0, 50.0, col);
        }
        if level > 0.5 {
            cv.arc_cap(cx + 0.02 * s, cy, 0.30 * s, th, 315.0, 360.0, col);
            cv.arc_cap(cx + 0.02 * s, cy, 0.30 * s, th, 0.0, 45.0, col);
        }
    }
}

pub fn bluetooth(cv: &mut Canvas, cx: f32, cy: f32, s: f32, col: Rgba) {
    let th = stroke_w(s, 0.09);
    cv.line(cx, cy - 0.36 * s, cx, cy + 0.36 * s, th, col);
    cv.line(cx, cy - 0.36 * s, cx + 0.22 * s, cy - 0.16 * s, th, col);
    cv.line(cx + 0.22 * s, cy - 0.16 * s, cx - 0.20 * s, cy + 0.20 * s, th, col);
    cv.line(cx, cy + 0.36 * s, cx + 0.22 * s, cy + 0.16 * s, th, col);
    cv.line(cx + 0.22 * s, cy + 0.16 * s, cx - 0.20 * s, cy - 0.20 * s, th, col);
}

// ------------------------------------------------------------------ cuaca

/// Kelas cuaca dari kode WMO (Open-Meteo): 0 cerah, 1 berawan sebagian, 2 berawan, 3 kabut, 4 gerimis/hujan, 5 hujan deras, 6 salju, 7 badai petir.
pub fn weather_class(code: i32) -> u8 {
    match code {
        0 => 0,
        1 | 2 => 1,
        3 => 2,
        45 | 48 => 3,
        51..=57 | 61 | 63 | 80 | 81 => 4,
        65 | 66 | 67 | 82 => 5,
        71..=77 | 85 | 86 => 6,
        95..=99 => 7,
        _ => 2,
    }
}

pub fn weather_text(code: i32) -> &'static str {
    match weather_class(code) {
        0 => "Cerah",
        1 => "Cerah berawan",
        2 => "Berawan",
        3 => "Berkabut",
        4 => "Hujan ringan",
        5 => "Hujan lebat",
        6 => "Salju",
        _ => "Badai petir",
    }
}

/// Awan dua lapis berwarna: bayangan biru di belakang, badan putih kebiruan di depan, sorot putih tipis di atas.
fn wx_cloud(cv: &mut Canvas, x: f32, y: f32, w: f32, tone: u8, light_bg: bool) {
    // tone: 0 cerah-berawan/berawan (putih kebiruan), 1 mendung (abu kebiruan, untuk hujan dan badai)
    let (back, front, shine) = match (tone, light_bg) {
        (0, false) => (Rgba(122, 170, 238, 255), Rgba(240, 247, 255, 255), Rgba(255, 255, 255, 255)),
        (0, true) => (Rgba(96, 150, 226, 255), Rgba(226, 238, 255, 255), Rgba(255, 255, 255, 255)),
        (_, false) => (Rgba(98, 124, 176, 255), Rgba(206, 218, 238, 255), Rgba(232, 240, 252, 255)),
        (_, true) => (Rgba(86, 110, 162, 255), Rgba(186, 202, 230, 255), Rgba(222, 232, 248, 255)),
    };
    let u = w / 10.0;
    cloud(cv, x + 0.25 * u, y + 0.55 * u, w, back);
    cloud(cv, x, y, w, front);
    cv.fill_rrect(x - 3.4 * u, y - 0.1 * u, 6.8 * u, 0.55 * u, 0.28 * u, shine.a(0.65), shine.a(0.0));
}

/// Glyph cuaca berwarna: matahari emas-oranye, awan biru-putih, tetes hujan biru, petir kuning, bulan emas.
pub fn weather_glyph(cv: &mut Canvas, class: u8, day: bool, cx: f32, cy: f32, s: f32, light_bg: bool) {
    let th = stroke_w(s, 0.075);
    let sun_a = Rgba(255, 224, 82, 255);
    let sun_b = Rgba(255, 148, 22, 255);
    let ray = Rgba(255, 176, 36, 255);
    let moon_c = if light_bg { Rgba(246, 188, 40, 255) } else { Rgba(255, 224, 120, 255) };
    let sun_or_moon = |cv: &mut Canvas, x: f32, y: f32, r: f32| {
        if day {
            cv.fill_circle_g(x, y, r, sun_a, sun_b);
            rays(cv, x, y, r * 1.45, r * 2.05, 0.0, 315.0, 8, th, ray);
        } else {
            cv.crescent(x, y, r * 1.35, r * 0.72, -r * 0.5, r * 1.15, moon_c);
            star(cv, x + r * 1.5, y - r * 1.2, r * 0.55, Rgba(255, 244, 190, 255));
        }
    };
    match class {
        0 => sun_or_moon(cv, cx, cy, 0.20 * s),
        1 => {
            sun_or_moon(cv, cx - 0.10 * s, cy - 0.13 * s, 0.14 * s);
            wx_cloud(cv, cx + 0.07 * s, cy + 0.14 * s, 0.66 * s, 0, light_bg);
        }
        2 => wx_cloud(cv, cx, cy + 0.02 * s, 0.84 * s, 0, light_bg),
        3 => {
            wx_cloud(cv, cx, cy - 0.08 * s, 0.74 * s, 0, light_bg);
            let fog = if light_bg { Rgba(104, 132, 190, 255) } else { Rgba(170, 196, 240, 255) };
            cv.line(cx - 0.30 * s, cy + 0.20 * s, cx + 0.30 * s, cy + 0.20 * s, th * 1.2, fog);
            cv.line(cx - 0.20 * s, cy + 0.33 * s, cx + 0.34 * s, cy + 0.33 * s, th * 1.2, fog.mul_a(0.75));
        }
        4 | 5 => {
            wx_cloud(cv, cx, cy - 0.10 * s, 0.78 * s, 1, light_bg);
            let drop_a = Rgba(96, 190, 255, 255);
            let drop_b = Rgba(36, 120, 255, 255);
            let n = if class == 5 { 4 } else { 3 };
            for i in 0..n {
                let x = cx - 0.27 * s + i as f32 * (0.54 * s / (n - 1) as f32);
                let y0 = cy + 0.15 * s + if i % 2 == 1 { 0.05 * s } else { 0.0 };
                cv.line(x, y0, x - 0.06 * s, y0 + 0.18 * s, th * 1.45, drop_a.lerp(drop_b, 0.5));
            }
        }
        6 => {
            wx_cloud(cv, cx, cy - 0.10 * s, 0.78 * s, 0, light_bg);
            for i in 0..3 {
                let x = cx - 0.24 * s + i as f32 * 0.24 * s;
                let y = cy + 0.25 * s + if i % 2 == 1 { 0.06 * s } else { 0.0 };
                star(cv, x, y, 0.075 * s, Rgba(120, 190, 255, 255));
            }
        }
        _ => {
            wx_cloud(cv, cx, cy - 0.12 * s, 0.78 * s, 1, light_bg);
            cv.fill_poly(&[(cx + 0.06 * s, cy + 0.04 * s), (cx - 0.12 * s, cy + 0.27 * s), (cx - 0.01 * s, cy + 0.27 * s), (cx - 0.08 * s, cy + 0.44 * s), (cx + 0.16 * s, cy + 0.16 * s), (cx + 0.04 * s, cy + 0.16 * s)], Rgba(255, 210, 40, 255));
        }
    }
}

/// Baterai garis tunggal (satu warna) untuk header panel.
pub fn battery_mono(cv: &mut Canvas, cx: f32, cy: f32, s: f32, pct: u8, charging: bool, col: Rgba) {
    let th = (s * 0.09).max(1.4);
    let (w, h) = (s, s * 0.56);
    let nub = (s * 0.08).max(1.6);
    let x = cx - (w + nub + 1.0) / 2.0;
    let y = cy - h / 2.0;
    cv.stroke_rrect(x, y, w, h, h * 0.3, th, col);
    cv.fill_rrect(x + w + 1.0, cy - h * 0.2, nub, h * 0.4, nub * 0.5, col, col);
    let inset = th + 1.2;
    let fw = ((w - 2.0 * inset) * pct as f32 / 100.0).max(1.5);
    cv.fill_rrect(x + inset, y + inset, fw, h - 2.0 * inset, (h * 0.15).max(1.0), col.mul_a(0.9), col.mul_a(0.9));
    if charging {
        bolt(cv, x + w * 0.5, cy, h * 0.9, Rgba(0, 0, 0, 200));
    }
}
