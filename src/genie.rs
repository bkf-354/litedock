//! Animasi minimize/restore ala "genie" macOS memakai jendela overlay berlapis.
//! Dioptimalkan: render hanya pada kotak pembatas bentuk (mengecil seiring animasi), sampling
//! bilinear titik-tetap, dan baris dibagi ke beberapa thread agar tetap ~60 fps di layar besar.
use crate::gfx::*;
use std::time::Instant;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
use windows::Win32::UI::WindowsAndMessaging::*;

pub fn present_rect(hwnd: HWND, surf: &Surface, dst: (i32, i32), src: (i32, i32), size: (i32, i32)) {
    unsafe {
        let pt = POINT { x: dst.0, y: dst.1 };
        let sz = SIZE { cx: size.0.max(1), cy: size.1.max(1) };
        let sp = POINT { x: src.0, y: src.1 };
        let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let _ = UpdateLayeredWindow(hwnd, None, Some(&pt), Some(&sz), Some(surf.dc), Some(&sp), COLORREF(0), Some(&bf), ULW_ALPHA);
    }
}

/// Perbarui isi jendela berlapis tanpa mengubah posisinya (mis. jendela yang bisa diseret).
pub fn present_here(hwnd: HWND, surf: &Surface) {
    unsafe {
        let sz = SIZE { cx: surf.w, cy: surf.h };
        let sp = POINT { x: 0, y: 0 };
        let bf = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
        let _ = UpdateLayeredWindow(hwnd, None, None, Some(&sz), Some(surf.dc), Some(&sp), COLORREF(0), Some(&bf), ULW_ALPHA);
    }
}

pub fn present(hwnd: HWND, surf: &Surface, x: i32, y: i32) {
    present_rect(hwnd, surf, (x, y), (0, 0), (surf.w, surf.h));
}

/// Tangkap isi jendela (PrintWindow) sebagai gambar opak.
pub fn capture_window(hwnd: isize, rc: RECT) -> Option<Image> {
    let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
    if w < 40 || h < 30 || w * h > 12_000_000 {
        return None;
    }
    let mut s = Surface::new(w, h)?;
    let ok = unsafe { PrintWindow(HWND(hwnd as *mut _), s.dc, PRINT_WINDOW_FLAGS(2)) };
    if !ok.as_bool() {
        return None;
    }
    let mut px = s.px().to_vec();
    for p in px.iter_mut() {
        *p |= 0xFF00_0000;
    }
    Some(Image { w, h, px })
}

/// Gambar pengganti bila isi jendela tidak tersedia (mis. saat restore dari minimize lama).
pub fn make_ghost(w: i32, h: i32, icon: &Icon) -> Image {
    let mut img = Image::new(w.max(80), h.max(60));
    {
        let (iw, ih) = (img.w, img.h);
        let mut cv = Canvas { w: iw, h: ih, px: &mut img.px };
        let body = Rgba(248, 249, 252, 255);
        cv.fill_rrect(0.0, 0.0, iw as f32, ih as f32, 10.0, body, body);
        cv.fill_rrect(0.0, 0.0, iw as f32, 30.0, 10.0, Rgba(226, 229, 238, 255), Rgba(226, 229, 238, 255));
        cv.stroke_rrect(0.0, 0.0, iw as f32, ih as f32, 10.0, 1.5, Rgba(170, 176, 190, 255));
        let sz = (iw.min(ih) as f32 * 0.34).min(160.0);
        cv.draw_icon(icon, (iw as f32 - sz) / 2.0, (ih as f32 - sz) / 2.0 + 10.0, sz, 1.0);
    }
    img
}

pub struct Genie {
    pub hwnd: isize,
    pub minimize: bool,
    mips: Vec<Image>,
    bx: i32,
    by: i32,
    src: (f32, f32, f32, f32),
    tgt: (f32, f32, f32),
    surf: Surface,
    pub win: HWND,
    t0: Instant,
    dur: f32,
    threads: usize,
    ns_px: f32,
    pub fast_frames: u32,
}

fn ease(t: f32) -> f32 {
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

impl Genie {
    /// src: rect jendela (layar). tgt: (pusat-x, atas, lebar) ikon di dock (koordinat layar).
    pub fn start(win: HWND, hwnd: isize, minimize: bool, src: RECT, tgt: (f32, f32, f32), img: Image, speed: f32) -> Option<Genie> {
        let left = (src.left as f32).min(tgt.0 - tgt.2 / 2.0) as i32 - 4;
        let right = (src.right as f32).max(tgt.0 + tgt.2 / 2.0) as i32 + 4;
        let top = (src.top as f32).min(tgt.1) as i32 - 4;
        let bottom = (src.bottom as f32).max(tgt.1 + tgt.2) as i32 + 4;
        let (w, h) = (right - left, bottom - top);
        if w <= 0 || h <= 0 || (w as i64) * (h as i64) > 14_000_000 {
            return None;
        }
        let surf = Surface::new(w, h)?;
        let half = img.half();
        let quarter = half.half();
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 6);
        Some(Genie {
            hwnd,
            minimize,
            mips: vec![img, half, quarter],
            bx: left,
            by: top,
            src: ((src.left - left) as f32, (src.top - top) as f32, (src.right - src.left) as f32, (src.bottom - src.top) as f32),
            tgt: (tgt.0 - left as f32, tgt.1 - top as f32, tgt.2),
            surf,
            win,
            t0: Instant::now(),
            dur: 0.50 / speed.max(0.3),
            threads,
            ns_px: 14.0,
            fast_frames: 0,
        })
    }

    /// Hanya untuk uji: render frame pada progres q ke surface internal.
    pub fn debug_frame(&mut self, q: f32) -> &mut Surface {
        self.warp(q);
        &mut self.surf
    }

    /// Gambar satu frame; true bila animasi selesai.
    pub fn frame(&mut self) -> bool {
        let t = (self.t0.elapsed().as_secs_f32() / self.dur).min(1.0);
        let e = ease(t);
        let q = if self.minimize { e } else { 1.0 - e };
        let (x0, y0, x1, y1) = self.warp(q);
        present_rect(self.win, &self.surf, (self.bx + x0, self.by + y0), (x0, y0), (x1 - x0, y1 - y0));
        t >= 1.0
    }

    /// Render bentuk genie pada progres q; mengembalikan kotak pembatas (x0,y0,x1,y1) di surface.
    fn warp(&mut self, q: f32) -> (i32, i32, i32, i32) {
        const K: f32 = 0.55;
        let (sx, sy, sw, sh) = self.src;
        let (tx, ty, tw) = self.tgt;
        let h = self.mips[0].h as usize;
        let cx_src = sx + sw / 2.0;
        let mut dy = vec![0f32; h + 1];
        let mut cxs = vec![0f32; h + 1];
        let mut wds = vec![0f32; h + 1];
        let (mut xmin, mut xmax) = (f32::MAX, f32::MIN);
        for i in 0..=h {
            let v = i as f32 / h as f32;
            let ysrc = sy + v * sh;
            let rp = (q * (1.0 + K) - K * (1.0 - v)).clamp(0.0, 1.0);
            let mut y = ysrc + rp * (ty - ysrc);
            if i > 0 && y < dy[i - 1] {
                y = dy[i - 1];
            }
            dy[i] = y;
            let sm = rp * rp * (3.0 - 2.0 * rp);
            wds[i] = sw + (tw * 0.55 - sw) * sm;
            cxs[i] = cx_src + (tx - cx_src) * (1.0 - (std::f32::consts::PI * rp).cos()) / 2.0;
            xmin = xmin.min(cxs[i] - wds[i] / 2.0);
            xmax = xmax.max(cxs[i] + wds[i] / 2.0);
        }
        let fade = {
            let x = ((q - 0.88) / 0.12).clamp(0.0, 1.0);
            1.0 - x * x * (3.0 - 2.0 * x)
        };
        let ga = (fade * 255.0) as u32;
        let (cw, ch) = (self.surf.w, self.surf.h);
        let bx0 = (xmin.floor() as i32 - 1).clamp(0, cw);
        let bx1 = (xmax.ceil() as i32 + 1).clamp(0, cw);
        let by0 = (dy[0].floor() as i32 - 1).clamp(0, ch);
        let by1 = (dy[h].ceil() as i32 + 1).clamp(0, ch);
        if bx1 <= bx0 + 1 || by1 <= by0 + 1 {
            self.surf.px()[0] = 0;
            return (0, 0, 1, 1);
        }
        let rows = (by1 - by0) as usize;
        let nthreads = if ((bx1 - bx0) as usize) * rows < 160_000 { 1 } else { self.threads };
        let chunk_rows = rows.div_ceil(nthreads).max(1);
        let mips = &self.mips;
        let region = &mut self.surf.px()[(by0 * cw) as usize..(by1 * cw) as usize];
        let (dyr, cxr, wdr) = (&dy, &cxs, &wds);
        // kualitas adaptif: bila bilinear diperkirakan > ~10 ms/frame, pakai nearest (mip tetap menjaga aliasing)
        let area = ((bx1 - bx0) as f32) * rows as f32;
        let fast = area * self.ns_px * 1e-6 / nthreads as f32 > 10.0;
        let t0 = Instant::now();
        if nthreads == 1 {
            render_rows(region, cw, by0, bx0, bx1, dyr, cxr, wdr, mips, sw, ga, h, fast);
        } else {
            std::thread::scope(|sc| {
                for (ci, chunk) in region.chunks_mut(chunk_rows * cw as usize).enumerate() {
                    let row0 = by0 + (ci * chunk_rows) as i32;
                    sc.spawn(move || render_rows(chunk, cw, row0, bx0, bx1, dyr, cxr, wdr, mips, sw, ga, h, fast));
                }
            });
        }
        if fast {
            self.fast_frames += 1;
        } else {
            let ns = t0.elapsed().as_nanos() as f32 * nthreads as f32 / area.max(1.0);
            self.ns_px = self.ns_px * 0.6 + ns * 0.4;
        }
        (bx0, by0, bx1, by1)
    }
}

#[allow(clippy::too_many_arguments)]
fn render_rows(chunk: &mut [u32], cw: i32, row0: i32, bx0: i32, bx1: i32, dy: &[f32], cxs: &[f32], wds: &[f32], mips: &[Image], sw: f32, ga: u32, h: usize, fast: bool) {
    let nrows = chunk.len() / cw as usize;
    let base = &mips[0];
    for r in 0..nrows {
        let py = row0 + r as i32;
        let row = &mut chunk[r * cw as usize..(r + 1) * cw as usize];
        row[bx0 as usize..bx1 as usize].fill(0);
        let yc = py as f32 + 0.5;
        if yc < dy[0] || yc >= dy[h] {
            continue;
        }
        let i = dy.partition_point(|&d| d <= yc).saturating_sub(1).min(h - 1);
        let span = (dy[i + 1] - dy[i]).max(1e-4);
        let fr = ((yc - dy[i]) / span).clamp(0.0, 1.0);
        let v = i as f32 + fr;
        let cxv = cxs[i] + (cxs[i + 1] - cxs[i]) * fr;
        let wv = wds[i] + (wds[i + 1] - wds[i]) * fr;
        if wv < 0.5 {
            continue;
        }
        let ratio = sw / wv;
        let lvl = if ratio > 3.6 { 2 } else if ratio > 1.8 { 1 } else { 0 };
        let mip = &mips[lvl];
        let ky = mip.h as f32 / base.h as f32;
        let sv = v * ky - 0.5;
        let y0f = sv.floor();
        let fy = ((sv - y0f) * 256.0) as u32;
        let y0 = (y0f as i32).clamp(0, mip.h - 1) as usize;
        let y1 = (y0f as i32 + 1).clamp(0, mip.h - 1) as usize;
        let mw = mip.w as usize;
        let r0 = &mip.px[y0 * mw..(y0 + 1) * mw];
        let r1 = &mip.px[y1 * mw..(y1 + 1) * mw];
        let left = cxv - wv / 2.0;
        let right = cxv + wv / 2.0;
        let du = mip.w as f32 / wv;
        let x_start = (left.floor() as i32).max(bx0);
        let x_end = (right.ceil() as i32).min(bx1);
        let mut u_fp: i64 = ((((x_start as f32 + 0.5) - left) * du - 0.5) * 65536.0) as i64;
        let du_fp = (du * 65536.0) as i64;
        let maxx = mw as i64 - 1;
        let rn = if fy < 128 { r0 } else { r1 };
        for x in x_start..x_end {
            let s = if fast {
                rn[((u_fp + 32768) >> 16).clamp(0, maxx) as usize]
            } else {
                let xa = (u_fp >> 16).clamp(0, maxx) as usize;
                let xb = ((u_fp >> 16) + 1).clamp(0, maxx) as usize;
                let fx = ((u_fp >> 8) & 255) as u32;
                let top = lerp_px(r0[xa], r0[xb], fx);
                let bot = lerp_px(r1[xa], r1[xb], fx);
                lerp_px(top, bot, fy)
            };
            let a = if x == x_start || x + 1 >= x_end {
                let cov = ((right.min(x as f32 + 1.0)) - (left.max(x as f32))).clamp(0.0, 1.0);
                (ga as f32 * cov) as u32
            } else {
                ga
            };
            row[x as usize] = scale_px(s, a);
            u_fp += du_fp;
        }
    }
}
