//! Inti renderer perangkat lunak (tanpa Win32): piksel premultiplied BGRA (0xAARRGGBB), anti-alias via SDF.
//! Dipisah dari gfx.rs agar bisa diuji di platform mana pun.

#[derive(Clone, Copy, Debug)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

impl Rgba {
    pub fn a(self, a: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, (a.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
    pub fn mul_a(self, f: f32) -> Rgba {
        Rgba(self.0, self.1, self.2, ((self.3 as f32) * f.clamp(0.0, 1.0)).round() as u8)
    }
    pub fn lerp(self, o: Rgba, t: f32) -> Rgba {
        let l = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
        Rgba(l(self.0, o.0), l(self.1, o.1), l(self.2, o.2), l(self.3, o.3))
    }
}

#[inline(always)]
pub fn premul(c: Rgba, a: u32) -> u32 {
    // a: alfa efektif 0..255
    let r = (c.0 as u32 * a + 127) / 255;
    let g = (c.1 as u32 * a + 127) / 255;
    let b = (c.2 as u32 * a + 127) / 255;
    (a << 24) | (r << 16) | (g << 8) | b
}

#[inline(always)]
fn sat_add_lanes(a: u32, b: u32) -> u32 {
    let s = a + b;
    let o = (s >> 8) & 0x0001_0001;
    (s | (o * 255)) & 0x00FF_00FF
}

#[inline(always)]
pub fn blend_px(dst: u32, src: u32) -> u32 {
    let sa = src >> 24;
    if sa == 255 {
        return src;
    }
    if sa == 0 {
        return dst;
    }
    let ia = 255 - sa;
    let rb = (dst & 0x00FF_00FF) * ia + 0x0080_0080;
    let rb = ((rb + ((rb >> 8) & 0x00FF_00FF)) >> 8) & 0x00FF_00FF;
    let ag = ((dst >> 8) & 0x00FF_00FF) * ia + 0x0080_0080;
    let ag = ((ag + ((ag >> 8) & 0x00FF_00FF)) >> 8) & 0x00FF_00FF;
    let rb = sat_add_lanes(rb, src & 0x00FF_00FF);
    let ag = sat_add_lanes(ag, (src >> 8) & 0x00FF_00FF);
    (ag << 8) | rb
}

#[inline(always)]
pub fn scale_px(p: u32, a: u32) -> u32 {
    if a >= 255 {
        return p;
    }
    let rb = (p & 0x00FF_00FF) * a + 0x0080_0080;
    let rb = ((rb + ((rb >> 8) & 0x00FF_00FF)) >> 8) & 0x00FF_00FF;
    let ag = ((p >> 8) & 0x00FF_00FF) * a + 0x0080_0080;
    let ag = ((ag + ((ag >> 8) & 0x00FF_00FF)) >> 8) & 0x00FF_00FF;
    (ag << 8) | rb
}

#[inline(always)]
pub fn lerp_px(a: u32, b: u32, t: u32) -> u32 {
    let it = 256 - t;
    let rb = (((a & 0x00FF_00FF) * it + (b & 0x00FF_00FF) * t) >> 8) & 0x00FF_00FF;
    let ag = (((a >> 8) & 0x00FF_00FF) * it + ((b >> 8) & 0x00FF_00FF) * t) & 0xFF00_FF00;
    rb | ag
}

// ---------------------------------------------------------------- Image / Icon

#[derive(Clone)]
pub struct Image {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
}

impl Image {
    pub fn new(w: i32, h: i32) -> Image {
        Image { w, h, px: vec![0; (w.max(1) * h.max(1)) as usize] }
    }

    #[inline]
    fn at(&self, x: i32, y: i32) -> u32 {
        let x = x.clamp(0, self.w - 1);
        let y = y.clamp(0, self.h - 1);
        self.px[(y * self.w + x) as usize]
    }

    /// Sampel bilinear di koordinat piksel (titik pusat piksel = +0.5 sudah dikurangi pemanggil).
    #[inline]
    pub fn sample(&self, u: f32, v: f32) -> u32 {
        let x0 = u.floor();
        let y0 = v.floor();
        let fx = ((u - x0) * 256.0) as u32;
        let fy = ((v - y0) * 256.0) as u32;
        let (x0, y0) = (x0 as i32, y0 as i32);
        let top = lerp_px(self.at(x0, y0), self.at(x0 + 1, y0), fx);
        let bot = lerp_px(self.at(x0, y0 + 1), self.at(x0 + 1, y0 + 1), fx);
        lerp_px(top, bot, fy)
    }

    /// Ikon berupa glif transparan (tanpa latar): bungkus dengan ubin persegi membulat ala iOS/macOS dengan warna serasi.
    /// None bila ikon sudah berlatar/berbentuk utuh.
    pub fn glyph_tile(&self) -> Option<Image> {
        let (w, h) = (self.w, self.h);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1, -1);
        for y in 0..h {
            for x in 0..w {
                if self.px[(y * w + x) as usize] >> 24 > 40 {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 < x0 {
            return None;
        }
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        let (mut solid, mut sr, mut sg, mut sb, mut sa) = (0u32, 0f64, 0f64, 0f64, 0f64);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = self.px[(y * w + x) as usize];
                let a = p >> 24;
                if a >= 200 {
                    solid += 1;
                }
                if a > 120 {
                    let af = a as f64;
                    sr += ((p >> 16) & 255) as f64 * 255.0 / af * af;
                    sg += ((p >> 8) & 255) as f64 * 255.0 / af * af;
                    sb += (p & 255) as f64 * 255.0 / af * af;
                    sa += af;
                }
            }
        }
        let fill = solid as f32 / (bw * bh) as f32;
        if fill >= 0.60 || sa <= 0.0 {
            return None;
        }
        let (mr, mg, mb) = ((sr / sa) as f32, (sg / sa) as f32, (sb / sa) as f32);
        let lum = 0.2126 * mr + 0.7152 * mg + 0.0722 * mb;
        let mix = |a: f32, b: f32, t: f32| a + (b - a) * t;
        let (top, bot) = if lum > 178.0 {
            // glif terang: ubin gelap bernuansa warna glif
            (
                Rgba(mix(58.0, mr * 0.55, 0.35) as u8, mix(62.0, mg * 0.55, 0.35) as u8, mix(82.0, mb * 0.55, 0.35) as u8, 255),
                Rgba(mix(30.0, mr * 0.35, 0.30) as u8, mix(32.0, mg * 0.35, 0.30) as u8, mix(46.0, mb * 0.35, 0.30) as u8, 255),
            )
        } else {
            // glif gelap/berwarna: ubin terang dengan nuansa halus
            (
                Rgba(mix(252.0, mr, 0.10) as u8, mix(252.0, mg, 0.10) as u8, mix(254.0, mb, 0.10) as u8, 255),
                Rgba(mix(226.0, mr, 0.16) as u8, mix(230.0, mg, 0.16) as u8, mix(240.0, mb, 0.16) as u8, 255),
            )
        };
        // ubin selalu digambar vektor pada 256 px (bukan pada ukuran sumber yang bisa 32-48 px) -> tepi, gradien, dan garis tepi
        // tetap tajam setelah dikecilkan ke ukuran dock; hanya glifnya yang bergantung pada resolusi sumber
        let t = 256;
        let mut out = Image::new(t, t);
        {
            let mut cv = Canvas { w: t, h: t, px: &mut out.px };
            let tf = t as f32;
            cv.fill_rrect(tf * 0.03, tf * 0.03, tf * 0.94, tf * 0.94, tf * 0.215, top, bot);
            cv.stroke_rrect(tf * 0.03, tf * 0.03, tf * 0.94, tf * 0.94, tf * 0.215, (tf / 128.0).max(1.0), Rgba(0, 0, 0, 38));
            cv.fill_rrect(tf * 0.10, tf * 0.045, tf * 0.80, (tf / 128.0).max(1.0), 0.5, Rgba(255, 255, 255, 120), Rgba(255, 255, 255, 0));
        }
        // glif dipangkas ke kotak pembatas lalu diskalakan agar muat 66% ubin
        let mut crop = Image::new(bw, bh);
        for y in 0..bh {
            for x in 0..bw {
                crop.px[(y * bw + x) as usize] = self.px[((y0 + y) * w + x0 + x) as usize];
            }
        }
        let scale = (t as f32 * 0.62 / bw.max(bh) as f32).min(8.0);
        let (gw, gh) = (((bw as f32 * scale).round() as i32).max(1), ((bh as f32 * scale).round() as i32).max(1));
        let g = if gw != bw || gh != bh { crop.resized(gw, gh) } else { crop };
        {
            let mut cv = Canvas { w: t, h: t, px: &mut out.px };
            cv.blit_image(&g, (t - gw) / 2, (t - gh) / 2);
        }
        Some(out)
    }

    /// Perkecil ke (nw, nh) dengan rata-rata area (box filter) pada piksel premultiplied: tajam dan tanpa moire.
    pub fn box_resize(&self, nw: i32, nh: i32) -> Image {
        let (nw, nh) = (nw.max(1), nh.max(1));
        let mut out = Image::new(nw, nh);
        for y in 0..nh {
            let (sy0, sy1) = ((y * self.h) / nh, (((y + 1) * self.h) / nh).max((y * self.h) / nh + 1));
            for x in 0..nw {
                let (sx0, sx1) = ((x * self.w) / nw, (((x + 1) * self.w) / nw).max((x * self.w) / nw + 1));
                let (mut a, mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
                for yy in sy0..sy1.min(self.h) {
                    for xx in sx0..sx1.min(self.w) {
                        let p = self.px[(yy * self.w + xx) as usize];
                        a += p >> 24;
                        r += (p >> 16) & 255;
                        g += (p >> 8) & 255;
                        b += p & 255;
                        n += 1;
                    }
                }
                if n > 0 {
                    out.px[(y * nw + x) as usize] = ((a / n) << 24) | ((r / n) << 16) | ((g / n) << 8) | (b / n);
                }
            }
        }
        out
    }

    /// Perbesar bulat 2x dengan nearest-neighbor (tetap tajam, untuk ikon kecil di layar DPI tinggi).
    pub fn nearest2x(&self) -> Image {
        let mut out = Image::new(self.w * 2, self.h * 2);
        for y in 0..self.h * 2 {
            for x in 0..self.w * 2 {
                out.px[(y * out.w + x) as usize] = self.px[((y / 2) * self.w + x / 2) as usize];
            }
        }
        out
    }

    /// Perkecil 2x dengan rata-rata 4 piksel (cepat; dipakai untuk mip animasi).
    pub fn half(&self) -> Image {
        let nw = (self.w / 2).max(1);
        let nh = (self.h / 2).max(1);
        let mut out = Image::new(nw, nh);
        let avg = |a: u32, b: u32, c: u32, d: u32| -> u32 {
            let rb = (((a & 0x00FF_00FF) + (b & 0x00FF_00FF) + (c & 0x00FF_00FF) + (d & 0x00FF_00FF) + 0x0002_0002) >> 2) & 0x00FF_00FF;
            let ag = ((((a >> 8) & 0x00FF_00FF) + ((b >> 8) & 0x00FF_00FF) + ((c >> 8) & 0x00FF_00FF) + ((d >> 8) & 0x00FF_00FF) + 0x0002_0002) >> 2) & 0x00FF_00FF;
            rb | (ag << 8)
        };
        for y in 0..nh {
            let r0 = ((2 * y).min(self.h - 1) * self.w) as usize;
            let r1 = ((2 * y + 1).min(self.h - 1) * self.w) as usize;
            for x in 0..nw {
                let c0 = (2 * x).min(self.w - 1) as usize;
                let c1 = (2 * x + 1).min(self.w - 1) as usize;
                out.px[(y * nw + x) as usize] = avg(self.px[r0 + c0], self.px[r0 + c1], self.px[r1 + c0], self.px[r1 + c1]);
            }
        }
        out
    }

    /// Interpolasi bikubik (Catmull-Rom) pada piksel premultiplied; jauh lebih halus daripada bilinear saat memperbesar.
    pub fn sample_cubic(&self, u: f32, v: f32) -> u32 {
        #[inline]
        fn cr(t: f32) -> [f32; 4] {
            let t2 = t * t;
            let t3 = t2 * t;
            [-0.5 * t3 + t2 - 0.5 * t, 1.5 * t3 - 2.5 * t2 + 1.0, -1.5 * t3 + 2.0 * t2 + 0.5 * t, 0.5 * t3 - 0.5 * t2]
        }
        let (x0, y0) = (u.floor(), v.floor());
        let (wx, wy) = (cr(u - x0), cr(v - y0));
        let (xi, yi) = (x0 as i32, y0 as i32);
        let mut acc = [0f32; 4];
        for j in 0..4i32 {
            let mut row = [0f32; 4];
            for i in 0..4i32 {
                let p = self.at(xi - 1 + i, yi - 1 + j);
                let w = wx[i as usize];
                row[0] += w * (p >> 24) as f32;
                row[1] += w * ((p >> 16) & 255) as f32;
                row[2] += w * ((p >> 8) & 255) as f32;
                row[3] += w * (p & 255) as f32;
            }
            for c in 0..4 {
                acc[c] += wy[j as usize] * row[c];
            }
        }
        let a = acc[0].clamp(0.0, 255.0);
        let cl = |v: f32| v.clamp(0.0, a);
        ((a as u32) << 24) | ((cl(acc[1]) as u32) << 16) | ((cl(acc[2]) as u32) << 8) | cl(acc[3]) as u32
    }

    /// Deteksi gambar hasil pembesaran "nearest-neighbor" (blok seragam) dan kembalikan versi aslinya
    /// (g x g) agar bisa diperbesar ulang dengan halus. None bila gambar bukan hasil pembesaran blok.
    /// Image list shell kadang menaruh ikon kecil (32/48 px) apa adanya di tengah kanvas 256 px yang transparan.
    /// Bila isi (alfa > 8) hanya menempati <= 34% sisi kanvas, potong ke kotak isinya (+1 px) supaya diperbesar
    /// dengan filter yang bagus, bukan tampil sebagai ikon mungil. None bila isi cukup besar.
    pub fn crop_small_content(&self) -> Option<Image> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, -1i32, -1i32);
        for y in 0..self.h {
            for x in 0..self.w {
                if (self.px[(y * self.w + x) as usize] >> 24) > 8 {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 < x0 || y1 < y0 {
            return None;
        }
        let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
        if (bw.max(bh) as f32) > 0.34 * self.w.max(self.h) as f32 || bw < 8 || bh < 8 {
            return None;
        }
        let (cx0, cy0) = ((x0 - 1).max(0), (y0 - 1).max(0));
        let (cx1, cy1) = ((x1 + 1).min(self.w - 1), (y1 + 1).min(self.h - 1));
        let (cw, ch) = (cx1 - cx0 + 1, cy1 - cy0 + 1);
        let mut out = Image::new(cw, ch);
        for y in 0..ch {
            for x in 0..cw {
                out.px[(y * cw + x) as usize] = self.px[((cy0 + y) * self.w + cx0 + x) as usize];
            }
        }
        Some(out)
    }

    pub fn undo_nearest_upscale(&self) -> Option<Image> {
        let (w, h) = (self.w, self.h);
        if w < 96 || h < 96 {
            return None;
        }
        let mut cols = Vec::new();
        for x in 1..w {
            if (0..h).any(|y| self.px[(y * w + x) as usize] != self.px[(y * w + x - 1) as usize]) {
                cols.push(x);
            }
        }
        let mut rows = Vec::new();
        for y in 1..h {
            if (0..w).any(|x| self.px[(y * w + x) as usize] != self.px[((y - 1) * w + x) as usize]) {
                rows.push(y);
            }
        }
        for g in [16, 20, 24, 32, 40, 48, 64, 80] {
            if g * 2 > w.min(h) {
                continue;
            }
            let near = |pos: i32, size: i32| {
                let m = (pos * g) % size;
                m < 2 * g || m >= size - g
            };
            let (nc, nr) = (cols.len() as i32, rows.len() as i32);
            if nc >= g / 3 && nr >= g / 3 && nc <= g + 2 && nr <= g + 2 && cols.iter().all(|&x| near(x, w)) && rows.iter().all(|&y| near(y, h)) {
                let mut out = Image::new(g, g);
                for j in 0..g {
                    let sy = ((j * h) / g + ((j + 1) * h) / g) / 2;
                    for i in 0..g {
                        let sx = ((i * w) / g + ((i + 1) * w) / g) / 2;
                        out.px[(j * g + i) as usize] = self.px[(sy.min(h - 1) * w + sx.min(w - 1)) as usize];
                    }
                }
                return Some(out);
            }
        }
        None
    }

    #[inline]
    fn at_t(&self, x: i32, y: i32) -> u32 {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            0
        } else {
            self.px[(y * self.w + x) as usize]
        }
    }

    /// Bilinear dengan piksel di luar gambar dianggap transparan.
    #[inline]
    pub fn sample_t(&self, u: f32, v: f32) -> u32 {
        let x0 = u.floor();
        let y0 = v.floor();
        let fx = ((u - x0) * 256.0) as u32;
        let fy = ((v - y0) * 256.0) as u32;
        let (x0, y0) = (x0 as i32, y0 as i32);
        let top = lerp_px(self.at_t(x0, y0), self.at_t(x0 + 1, y0), fx);
        let bot = lerp_px(self.at_t(x0, y0 + 1), self.at_t(x0 + 1, y0 + 1), fx);
        lerp_px(top, bot, fy)
    }

    /// Ubah ukuran dengan filter Lanczos-3 yang dapat dipisah (horizontal lalu vertikal) pada piksel premultiplied.
    /// Memperkecil memakai jendela filter yang ikut diperlebar (anti-aliasing penuh, tanpa moire/tangga), memperbesar
    /// tetap tajam tanpa kotak-kotak. Hasil dijepit agar tetap premultiplied yang sah (warna <= alfa).
    pub fn resized(&self, nw: i32, nh: i32) -> Image {
        let (nw, nh) = (nw.max(1), nh.max(1));
        if nw == self.w && nh == self.h {
            return self.clone();
        }
        let tx = lanczos_taps(self.w, nw);
        let ty = lanczos_taps(self.h, nh);
        let (sw, sh) = (self.w as usize, self.h as usize);
        // lintasan horizontal: sh baris x nw kolom, 4 kanal float
        let mut mid = vec![0f32; nw as usize * sh * 4];
        for y in 0..sh {
            let row = &self.px[y * sw..(y + 1) * sw];
            for (x, (x0, wts)) in tx.iter().enumerate() {
                let (mut a, mut r, mut g, mut b) = (0f32, 0f32, 0f32, 0f32);
                for (i, w) in wts.iter().enumerate() {
                    let sx = (*x0 + i as i32).clamp(0, self.w - 1) as usize;
                    let p = row[sx];
                    a += w * (p >> 24) as f32;
                    r += w * ((p >> 16) & 255) as f32;
                    g += w * ((p >> 8) & 255) as f32;
                    b += w * (p & 255) as f32;
                }
                let o = (y * nw as usize + x) * 4;
                mid[o] = a;
                mid[o + 1] = r;
                mid[o + 2] = g;
                mid[o + 3] = b;
            }
        }
        // lintasan vertikal
        let mut out = Image::new(nw, nh);
        for (y, (y0, wts)) in ty.iter().enumerate() {
            for x in 0..nw as usize {
                let (mut a, mut r, mut g, mut b) = (0f32, 0f32, 0f32, 0f32);
                for (i, w) in wts.iter().enumerate() {
                    let sy = (*y0 + i as i32).clamp(0, self.h - 1) as usize;
                    let o = (sy * nw as usize + x) * 4;
                    a += w * mid[o];
                    r += w * mid[o + 1];
                    g += w * mid[o + 2];
                    b += w * mid[o + 3];
                }
                let a = a.round().clamp(0.0, 255.0);
                let c = |v: f32| v.round().clamp(0.0, a) as u32;
                out.px[y * nw as usize + x] = ((a as u32) << 24) | (c(r) << 16) | (c(g) << 8) | c(b);
            }
        }
        out
    }

    /// Penajaman unsharp-mask (gaussian `sigma` px, kekuatan `amount`) pada piksel premultiplied. Dipakai untuk ikon sumber
    /// kecil (32-64 px) yang diperbesar: mengembalikan tepi yang hilang karena interpolasi tanpa menimbulkan kotak-kotak.
    pub fn unsharp(&self, sigma: f32, amount: f32) -> Image {
        let rad = (sigma * 3.0).ceil().max(1.0) as i32;
        let mut k: Vec<f32> = (-rad..=rad).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
        let sum: f32 = k.iter().sum();
        for v in k.iter_mut() {
            *v /= sum;
        }
        let (w, h) = (self.w as usize, self.h as usize);
        let ch = |p: u32| [(p >> 24) as f32, ((p >> 16) & 255) as f32, ((p >> 8) & 255) as f32, (p & 255) as f32];
        let mut mid = vec![[0f32; 4]; w * h];
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0f32; 4];
                for (i, kv) in k.iter().enumerate() {
                    let sx = (x as i32 + i as i32 - rad).clamp(0, w as i32 - 1) as usize;
                    let c = ch(self.px[y * w + sx]);
                    for j in 0..4 {
                        acc[j] += kv * c[j];
                    }
                }
                mid[y * w + x] = acc;
            }
        }
        let mut out = Image::new(self.w, self.h);
        for y in 0..h {
            for x in 0..w {
                let mut acc = [0f32; 4];
                for (i, kv) in k.iter().enumerate() {
                    let sy = (y as i32 + i as i32 - rad).clamp(0, h as i32 - 1) as usize;
                    let c = mid[sy * w + x];
                    for j in 0..4 {
                        acc[j] += kv * c[j];
                    }
                }
                let o = ch(self.px[y * w + x]);
                let a = (o[0] + amount * (o[0] - acc[0])).round().clamp(0.0, 255.0);
                let c = |j: usize| (o[j] + amount * (o[j] - acc[j])).round().clamp(0.0, a) as u32;
                out.px[y * w + x] = ((a as u32) << 24) | (c(1) << 16) | (c(2) << 8) | c(3);
            }
        }
        out
    }
}

pub const UNSHARP_SIGMA: f32 = 0.36;
pub const UNSHARP_AMOUNT: f32 = 0.7;

#[inline]
fn lanczos3(x: f32) -> f32 {
    let x = x.abs();
    if x < 1e-5 {
        1.0
    } else if x >= 3.0 {
        0.0
    } else {
        let px = std::f32::consts::PI * x;
        3.0 * px.sin() * (px / 3.0).sin() / (px * px)
    }
}

/// Bobot Lanczos-3 untuk mengubah `src` piksel menjadi `dst` piksel: per piksel hasil, indeks sumber pertama + bobot (jumlah 1).
fn lanczos_taps(src: i32, dst: i32) -> Vec<(i32, Vec<f32>)> {
    let scale = src as f32 / dst as f32;
    let fs = scale.max(1.0);
    let support = 3.0 * fs;
    (0..dst)
        .map(|i| {
            let center = (i as f32 + 0.5) * scale;
            let lo = (center - support).floor() as i32;
            let hi = (center + support).ceil() as i32;
            let mut w: Vec<f32> = (lo..hi).map(|x| lanczos3(((x as f32 + 0.5) - center) / fs)).collect();
            let sum: f32 = w.iter().sum();
            if sum.abs() > 1e-6 {
                for v in w.iter_mut() {
                    *v /= sum;
                }
            }
            (lo, w)
        })
        .collect()
}

/// Ikon dengan rantai mip (128, 64, 32) agar zoom tetap tajam tanpa aliasing.
pub struct Icon {
    pub mips: Vec<Image>,
    /// Mip tepat pada ukuran diam (ukuran dock) dengan filter area -> tajam tanpa bergerigi
    rest: std::cell::RefCell<Option<Image>>,
}

impl Icon {
    pub fn from_image(img: Image) -> Icon {
        // dasar 256 px bila sumber besar (ikon jumbo), selain itu 128 px. Mip dibuat langsung dari dasar (bukan bertingkat)
        // dengan rasio rapat (x0,82) sehingga skala berapa pun tetap memakai mip yang hanya sedikit lebih besar -> tajam
        let bsz = if img.w.max(img.h) >= 200 { 256 } else { 128 };
        let src_max = img.w.max(img.h);
        let mut base = if img.w != bsz || img.h != bsz { img.resized(bsz, bsz) } else { img };
        // sumber kecil (ikon lama 32-64 px) diperbesar >= 1,5x: pertajam tepinya agar tidak tampak buram/pecah
        if (8..100).contains(&src_max) {
            let f = bsz as f32 / src_max as f32;
            if f >= 1.5 {
                base = base.unsharp(UNSHARP_SIGMA * f, UNSHARP_AMOUNT);
            }
        }
        let mut mips = vec![];
        let mut sz = bsz as f32 * 0.82;
        let mut sizes = Vec::new();
        while sz >= 26.0 {
            sizes.push(sz.round() as i32);
            sz *= 0.82;
        }
        for s in sizes {
            mips.push(base.resized(s, s));
        }
        mips.insert(0, base);
        Icon { mips, rest: std::cell::RefCell::new(None) }
    }

    /// Siapkan mip khusus untuk ukuran diam `size` px (dibuat sekali per ukuran).
    pub fn set_rest(&self, size: i32) {
        let need = {
            let r = self.rest.borrow();
            r.as_ref().map(|i| i.w != size).unwrap_or(true)
        };
        if need && size >= 8 {
            let img = self.mips[0].resized(size, size);
            *self.rest.borrow_mut() = Some(img);
        }
    }

    fn pick(&self, target: f32) -> &Image {
        let mut best = &self.mips[0];
        for m in &self.mips {
            if (m.w as f32) >= target * 0.98 {
                best = m;
            }
        }
        best
    }
}


pub struct Canvas<'a> {
    pub w: i32,
    pub h: i32,
    pub px: &'a mut [u32],
}

#[inline(always)]
fn rr_sdf(px: f32, py: f32, cx: f32, cy: f32, hx: f32, hy: f32, r: f32) -> f32 {
    let qx = (px - cx).abs() - (hx - r);
    let qy = (py - cy).abs() - (hy - r);
    let ox = qx.max(0.0);
    let oy = qy.max(0.0);
    (ox * ox + oy * oy).sqrt() + qx.max(qy).min(0.0) - r
}

impl<'a> Canvas<'a> {
    pub fn clear(&mut self) {
        self.px.fill(0);
    }

    #[inline(always)]
    fn put(&mut self, x: i32, y: i32, src: u32) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            let i = (y * self.w + x) as usize;
            self.px[i] = blend_px(self.px[i], src);
        }
    }

    /// Persegi panjang membulat terisi. grad = (warna atas, warna bawah) jika ingin gradien vertikal.
    pub fn fill_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, top: Rgba, bottom: Rgba) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (cx, cy, hx, hy) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
        let x0 = (x.floor() as i32).max(0);
        let x1 = ((x + w).ceil() as i32).min(self.w);
        let y0 = (y.floor() as i32).max(0);
        let y1 = ((y + h).ceil() as i32).min(self.h);
        for py in y0..y1 {
            let t = ((py as f32 + 0.5 - y) / h).clamp(0.0, 1.0);
            let col = top.lerp(bottom, t);
            let full = premul(col, col.3 as u32);
            let dy = (py as f32 + 0.5 - cy).abs() - (hy - r);
            let (inner0, inner1) = if dy < -1.0 { (((x + r + 1.0).ceil() as i32).max(x0), ((x + w - r - 1.0).floor() as i32).min(x1)) } else { (x1, x1) };
            let row = (py * self.w) as usize;
            let (i0, i1) = (inner0.clamp(x0, x1), inner1.clamp(x0, x1));
            let edge = |this: &mut Self, from: i32, to: i32| {
                for px_ in from..to {
                    let d = rr_sdf(px_ as f32 + 0.5, py as f32 + 0.5, cx, cy, hx, hy, r);
                    let cov = (0.5 - d).clamp(0.0, 1.0);
                    if cov > 0.0 {
                        let a = (col.3 as f32 * cov).round() as u32;
                        let i = row + px_ as usize;
                        this.px[i] = blend_px(this.px[i], premul(col, a));
                    }
                }
            };
            if i1 > i0 {
                edge(self, x0, i0);
                let seg = &mut self.px[row + i0 as usize..row + i1 as usize];
                if col.3 == 255 {
                    seg.fill(full);
                } else {
                    for d in seg.iter_mut() {
                        *d = blend_px(*d, full);
                    }
                }
                edge(self, i1, x1);
            } else {
                edge(self, x0, x1);
            }
        }
    }

    /// Garis tepi (di sisi dalam bentuk) setebal th.
    pub fn stroke_rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, th: f32, c: Rgba) {
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let r = r.min(w / 2.0).min(h / 2.0).max(0.0);
        let (cx, cy, hx, hy) = (x + w / 2.0, y + h / 2.0, w / 2.0, h / 2.0);
        let x0 = (x.floor() as i32).max(0);
        let x1 = ((x + w).ceil() as i32).min(self.w);
        let y0 = (y.floor() as i32).max(0);
        let y1 = ((y + h).ceil() as i32).min(self.h);
        let skip = th + 1.5;
        for py in y0..y1 {
            let dy = (py as f32 + 0.5 - cy).abs() - (hy - r);
            let in_band = dy < -skip; // baris tengah: hanya tepi kiri/kanan yang perlu dihitung
            let left_end = (x + skip).ceil() as i32;
            let right_start = (x + w - skip).floor() as i32;
            let mut px_ = x0;
            while px_ < x1 {
                if in_band && px_ >= left_end && px_ < right_start {
                    px_ = right_start.max(px_ + 1);
                    continue;
                }
                let cur = px_;
                px_ += 1;
                let px_ = cur;
                let d = rr_sdf(px_ as f32 + 0.5, py as f32 + 0.5, cx, cy, hx, hy, r);
                let ring = (d + th / 2.0).abs() - th / 2.0;
                let cov = (0.5 - ring).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (c.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, r: f32, c: Rgba) {
        let x0 = ((cx - r - 1.0).floor() as i32).max(0);
        let x1 = ((cx + r + 1.0).ceil() as i32).min(self.w);
        let y0 = ((cy - r - 1.0).floor() as i32).max(0);
        let y1 = ((cy + r + 1.0).ceil() as i32).min(self.h);
        for py in y0..y1 {
            for px_ in x0..x1 {
                let dx = px_ as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (c.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    /// Bulan sabit: lingkaran (cx,cy,r) dikurangi lingkaran (cx+ox, cy+oy, r2), tepi anti-alias.
    pub fn crescent(&mut self, cx: f32, cy: f32, r: f32, ox: f32, oy: f32, r2: f32, c: Rgba) {
        let x0 = ((cx - r - 1.0).floor() as i32).max(0);
        let x1 = ((cx + r + 1.0).ceil() as i32).min(self.w);
        let y0 = ((cy - r - 1.0).floor() as i32).max(0);
        let y1 = ((cy + r + 1.0).ceil() as i32).min(self.h);
        for py in y0..y1 {
            for px_ in x0..x1 {
                let (fx, fy) = (px_ as f32 + 0.5, py as f32 + 0.5);
                let da = ((fx - cx).powi(2) + (fy - cy).powi(2)).sqrt() - r;
                let db = ((fx - cx - ox).powi(2) + (fy - cy - oy).powi(2)).sqrt() - r2;
                let cov = (0.5 - da).clamp(0.0, 1.0) * (db + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (c.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    /// Garis dengan ujung membulat.
    pub fn line(&mut self, ax: f32, ay: f32, bx: f32, by: f32, th: f32, c: Rgba) {
        let pad = th / 2.0 + 1.0;
        let x0 = ((ax.min(bx) - pad).floor() as i32).max(0);
        let x1 = ((ax.max(bx) + pad).ceil() as i32).min(self.w);
        let y0 = ((ay.min(by) - pad).floor() as i32).max(0);
        let y1 = ((ay.max(by) + pad).ceil() as i32).min(self.h);
        let (vx, vy) = (bx - ax, by - ay);
        let len2 = (vx * vx + vy * vy).max(1e-6);
        for py in y0..y1 {
            for px_ in x0..x1 {
                let (qx, qy) = (px_ as f32 + 0.5 - ax, py as f32 + 0.5 - ay);
                let t = ((qx * vx + qy * vy) / len2).clamp(0.0, 1.0);
                let (dx, dy) = (qx - vx * t, qy - vy * t);
                let d = (dx * dx + dy * dy).sqrt() - th / 2.0;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (c.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    /// Busur lingkaran (derajat, 0 = kanan, searah jarum jam di layar).
    pub fn arc(&mut self, cx: f32, cy: f32, r: f32, th: f32, a0: f32, a1: f32, c: Rgba) {
        let pad = r + th;
        let x0 = ((cx - pad).floor() as i32).max(0);
        let x1 = ((cx + pad).ceil() as i32).min(self.w);
        let y0 = ((cy - pad).floor() as i32).max(0);
        let y1 = ((cy + pad).ceil() as i32).min(self.h);
        for py in y0..y1 {
            for px_ in x0..x1 {
                let dx = px_ as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let ang = dy.atan2(dx).to_degrees();
                let ang = if ang < 0.0 { ang + 360.0 } else { ang };
                if ang < a0 || ang > a1 {
                    continue;
                }
                let d = ((dx * dx + dy * dy).sqrt() - r).abs() - th / 2.0;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (c.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    /// Busur dengan ujung membulat (gaya ikon Fluent Windows 11). Sudut dalam derajat, 0 = kanan, searah jarum jam.
    pub fn arc_cap(&mut self, cx: f32, cy: f32, r: f32, th: f32, a0: f32, a1: f32, c: Rgba) {
        // ujung bulat dihitung dari posisi sudut; cakupan memakai maks (bukan jumlah) agar alfa tidak menumpuk
        let pad = r + th + 1.0;
        let x0 = ((cx - pad).floor() as i32).max(0);
        let x1 = ((cx + pad).ceil() as i32).min(self.w);
        let y0 = ((cy - pad).floor() as i32).max(0);
        let y1 = ((cy + pad).ceil() as i32).min(self.h);
        let (sa, ca) = (a0.to_radians().sin(), a0.to_radians().cos());
        let (sb, cb) = (a1.to_radians().sin(), a1.to_radians().cos());
        let (p0x, p0y) = (cx + r * ca, cy + r * sa);
        let (p1x, p1y) = (cx + r * cb, cy + r * sb);
        let half = th / 2.0;
        for py in y0..y1 {
            for px_ in x0..x1 {
                let fx = px_ as f32 + 0.5;
                let fy = py as f32 + 0.5;
                let (dx, dy) = (fx - cx, fy - cy);
                let mut ang = dy.atan2(dx).to_degrees();
                if ang < 0.0 {
                    ang += 360.0;
                }
                let dist_ring = ((dx * dx + dy * dy).sqrt() - r).abs() - half;
                let d_arc = if ang >= a0 && ang <= a1 { dist_ring } else { f32::MAX };
                let d0 = ((fx - p0x).powi(2) + (fy - p0y).powi(2)).sqrt() - half;
                let d1 = ((fx - p1x).powi(2) + (fy - p1y).powi(2)).sqrt() - half;
                let d = d_arc.min(d0).min(d1);
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (c.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    /// Poligon terisi (aturan genap-ganjil), anti-alias dengan 4x4 sampel per piksel.
    pub fn fill_poly(&mut self, pts: &[(f32, f32)], c: Rgba) {
        if pts.len() < 3 {
            return;
        }
        let (mut minx, mut miny, mut maxx, mut maxy) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            minx = minx.min(x);
            maxx = maxx.max(x);
            miny = miny.min(y);
            maxy = maxy.max(y);
        }
        let x0 = (minx.floor() as i32).max(0);
        let x1 = (maxx.ceil() as i32).min(self.w);
        let y0 = (miny.floor() as i32).max(0);
        let y1 = (maxy.ceil() as i32).min(self.h);
        let n = pts.len();
        const SS: i32 = 4;
        for py in y0..y1 {
            // persilangan tepi pada tiap sub-baris -> rentang x terisi
            let mut cover = vec![0u8; (x1 - x0).max(0) as usize];
            for sy in 0..SS {
                let y = py as f32 + (sy as f32 + 0.5) / SS as f32;
                let mut xs: Vec<f32> = Vec::with_capacity(8);
                for i in 0..n {
                    let (ax, ay) = pts[i];
                    let (bx, by) = pts[(i + 1) % n];
                    if (ay <= y && by > y) || (by <= y && ay > y) {
                        let t = (y - ay) / (by - ay);
                        xs.push(ax + t * (bx - ax));
                    }
                }
                xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                for pair in xs.chunks(2) {
                    if pair.len() < 2 {
                        break;
                    }
                    let (xa, xb) = (pair[0], pair[1]);
                    for px_ in x0..x1 {
                        // lebar tumpang tindih sub-baris ini dengan piksel [px_, px_+1)
                        let l = xa.max(px_ as f32);
                        let r = xb.min(px_ as f32 + 1.0);
                        if r > l {
                            let add = ((r - l) * 255.0 / SS as f32).round() as u32;
                            let idx = (px_ - x0) as usize;
                            cover[idx] = (cover[idx] as u32 + add).min(255) as u8;
                        }
                    }
                }
            }
            for px_ in x0..x1 {
                let cv_ = cover[(px_ - x0) as usize] as u32;
                if cv_ > 0 {
                    let a = (c.3 as u32 * cv_ + 127) / 255;
                    self.put(px_, py, premul(c, a));
                }
            }
        }
    }

    /// Lingkaran dengan gradien vertikal (atas -> bawah).
    pub fn fill_circle_g(&mut self, cx: f32, cy: f32, r: f32, top: Rgba, bottom: Rgba) {
        let x0 = ((cx - r - 1.0).floor() as i32).max(0);
        let x1 = ((cx + r + 1.0).ceil() as i32).min(self.w);
        let y0 = ((cy - r - 1.0).floor() as i32).max(0);
        let y1 = ((cy + r + 1.0).ceil() as i32).min(self.h);
        for py in y0..y1 {
            let t = ((py as f32 + 0.5 - (cy - r)) / (2.0 * r)).clamp(0.0, 1.0);
            let col = top.lerp(bottom, t);
            for px_ in x0..x1 {
                let dx = px_ as f32 + 0.5 - cx;
                let dy = py as f32 + 0.5 - cy;
                let d = (dx * dx + dy * dy).sqrt() - r;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                if cov > 0.0 {
                    let a = (col.3 as f32 * cov).round() as u32;
                    self.put(px_, py, premul(col, a));
                }
            }
        }
    }

    /// Poligon pendekatan setengah lingkaran / sektor (derajat, 0 = kanan, searah jarum jam).
    pub fn fill_pie(&mut self, cx: f32, cy: f32, r: f32, a0: f32, a1: f32, c: Rgba) {
        let steps = (((a1 - a0).abs() / 6.0).ceil() as usize).max(3);
        let mut pts = Vec::with_capacity(steps + 2);
        pts.push((cx, cy));
        for i in 0..=steps {
            let a = (a0 + (a1 - a0) * i as f32 / steps as f32).to_radians();
            pts.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        self.fill_poly(&pts, c);
    }

    /// Salin gambar 1:1 pada posisi bulat (tanpa penskalaan -> tajam), dengan pencampuran alfa premultiplied.
    pub fn blit_image(&mut self, img: &Image, x: i32, y: i32) {
        for ry in 0..img.h {
            let py = y + ry;
            if py < 0 || py >= self.h {
                continue;
            }
            for rx in 0..img.w {
                let px_ = x + rx;
                if px_ < 0 || px_ >= self.w {
                    continue;
                }
                let sp = img.px[(ry * img.w + rx) as usize];
                if sp >> 24 == 0 {
                    continue;
                }
                let i = (py * self.w + px_) as usize;
                self.px[i] = blend_px(self.px[i], sp);
            }
        }
    }

    /// Gambar ikon berukuran `size` (piksel), pojok kiri-atas (x,y), alfa global 0..1.
    /// Diam di ukuran/posisi bulat -> salin mip tepat (tajam). Selain itu: supersampling 2x2 saat memperkecil.
    pub fn draw_icon(&mut self, icon: &Icon, x: f32, y: f32, size: f32, alpha: f32) {
        if size < 1.0 || alpha <= 0.0 {
            return;
        }
        let ga = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
        {
            let r = icon.rest.borrow();
            if let Some(rest) = r.as_ref() {
                if (rest.w as f32 - size).abs() < 0.01 && (x - x.round()).abs() < 0.01 && (y - y.round()).abs() < 0.01 {
                    let (ox, oy) = (x.round() as i32, y.round() as i32);
                    for ry in 0..rest.h {
                        let py = oy + ry;
                        if py < 0 || py >= self.h {
                            continue;
                        }
                        for rx in 0..rest.w {
                            let px_ = ox + rx;
                            if px_ < 0 || px_ >= self.w {
                                continue;
                            }
                            let sp = rest.px[(ry * rest.w + rx) as usize];
                            if sp >> 24 == 0 {
                                continue;
                            }
                            let i = (py * self.w + px_) as usize;
                            self.px[i] = blend_px(self.px[i], scale_px(sp, ga));
                        }
                    }
                    return;
                }
            }
        }
        let img = icon.pick(size);
        let k = img.w as f32 / size;
        let ss = k > 1.08;
        let x0 = (x.floor() as i32).max(0);
        let x1 = ((x + size).ceil() as i32).min(self.w);
        let y0 = (y.floor() as i32).max(0);
        let y1 = ((y + size).ceil() as i32).min(self.h);
        let avg4 = |a: u32, b: u32, c: u32, d: u32| -> u32 {
            let rb = (((a & 0x00FF_00FF) + (b & 0x00FF_00FF) + (c & 0x00FF_00FF) + (d & 0x00FF_00FF) + 0x0002_0002) >> 2) & 0x00FF_00FF;
            let ag = ((((a >> 8) & 0x00FF_00FF) + ((b >> 8) & 0x00FF_00FF) + ((c >> 8) & 0x00FF_00FF) + ((d >> 8) & 0x00FF_00FF) + 0x0002_0002) >> 2) & 0x00FF_00FF;
            rb | (ag << 8)
        };
        for py in y0..y1 {
            for px_ in x0..x1 {
                let (fx, fy) = (px_ as f32 + 0.5 - x, py as f32 + 0.5 - y);
                let s = if ss {
                    let o = 0.25;
                    avg4(
                        img.sample_t((fx - o) * k - 0.5, (fy - o) * k - 0.5),
                        img.sample_t((fx + o) * k - 0.5, (fy - o) * k - 0.5),
                        img.sample_t((fx - o) * k - 0.5, (fy + o) * k - 0.5),
                        img.sample_t((fx + o) * k - 0.5, (fy + o) * k - 0.5),
                    )
                } else {
                    img.sample_t(fx * k - 0.5, fy * k - 0.5)
                };
                if s >> 24 == 0 {
                    continue;
                }
                let s = scale_px(s, ga);
                let i = (py * self.w + px_) as usize;
                self.px[i] = blend_px(self.px[i], s);
            }
        }
    }

    pub fn draw_mask(&mut self, m: &Mask, x: i32, y: i32, c: Rgba) {
        for my in 0..m.h {
            let py = y + my;
            if py < 0 || py >= self.h {
                continue;
            }
            for mx in 0..m.w {
                let cov = m.a[(my * m.w + mx) as usize] as u32;
                if cov == 0 {
                    continue;
                }
                let px_ = x + mx;
                if px_ < 0 || px_ >= self.w {
                    continue;
                }
                let a = (c.3 as u32 * cov + 127) / 255;
                let i = (py * self.w + px_) as usize;
                self.px[i] = blend_px(self.px[i], premul(c, a));
            }
        }
    }

    /// Isi alfa=1 (nyaris tak terlihat) agar area tetap menerima mouse (piksel alfa 0 tembus klik).
    pub fn fill_hit(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let x0 = x.max(0);
        let x1 = (x + w).min(self.w);
        let y0 = y.max(0);
        let y1 = (y + h).min(self.h);
        for py in y0..y1 {
            for px_ in x0..x1 {
                let i = (py * self.w + px_) as usize;
                if self.px[i] == 0 {
                    self.px[i] = 0x0100_0000;
                }
            }
        }
    }
}

// ---------------------------------------------------------------- Teks (GDI -> mask cakupan)

// ---------------------------------------------------------------- Mask cakupan teks

pub struct Mask {
    pub w: i32,
    pub h: i32,
    pub a: Vec<u8>,
}

impl Mask {
    /// Kotak pembatas tinta (piksel dengan cakupan > 12): (x0, y0, x1, y1) inklusif.
    pub fn ink(&self) -> Option<(i32, i32, i32, i32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, -1, -1);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.a[(y * self.w + x) as usize] > 12 {
                    x0 = x0.min(x);
                    x1 = x1.max(x);
                    y0 = y0.min(y);
                    y1 = y1.max(y);
                }
            }
        }
        if x1 < x0 { None } else { Some((x0, y0, x1, y1)) }
    }
}
