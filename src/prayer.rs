//! Perhitungan waktu salat offline (algoritma PrayTimes.org). Tanpa jaringan, tanpa dependensi.
use std::f64::consts::PI;

pub struct Method {
    pub name: &'static str,
    pub fajr: f64,
    pub isha: f64,
    /// >0: Isya = Maghrib + menit ini (mis. Umm al-Qura)
    pub isha_min: f64,
    /// Ihtiyat (menit) ditambahkan ke waktu salat, dikurangi dari Terbit.
    pub ihtiyat: f64,
}

pub const METHODS: [Method; 6] = [
    Method { name: "Kemenag RI (20\u{b0}/18\u{b0}, ihtiyat 2 mnt)", fajr: 20.0, isha: 18.0, isha_min: 0.0, ihtiyat: 2.0 },
    Method { name: "Muslim World League (18\u{b0}/17\u{b0})", fajr: 18.0, isha: 17.0, isha_min: 0.0, ihtiyat: 0.0 },
    Method { name: "ISNA (15\u{b0}/15\u{b0})", fajr: 15.0, isha: 15.0, isha_min: 0.0, ihtiyat: 0.0 },
    Method { name: "Mesir (19.5\u{b0}/17.5\u{b0})", fajr: 19.5, isha: 17.5, isha_min: 0.0, ihtiyat: 0.0 },
    Method { name: "Umm al-Qura (18.5\u{b0}, +90 mnt)", fajr: 18.5, isha: 0.0, isha_min: 90.0, ihtiyat: 0.0 },
    Method { name: "Karachi (18\u{b0}/18\u{b0})", fajr: 18.0, isha: 18.0, isha_min: 0.0, ihtiyat: 0.0 },
];

/// Urutan: Subuh, Terbit, Dzuhur, Ashar, Maghrib, Isya
pub const NAMES: [&str; 6] = ["Subuh", "Terbit", "Dzuhur", "Ashar", "Maghrib", "Isya"];

fn dtr(d: f64) -> f64 { d * PI / 180.0 }
fn rtd(r: f64) -> f64 { r * 180.0 / PI }
fn sin_d(d: f64) -> f64 { dtr(d).sin() }
fn cos_d(d: f64) -> f64 { dtr(d).cos() }
fn tan_d(d: f64) -> f64 { dtr(d).tan() }
fn arcsin_d(x: f64) -> f64 { rtd(x.asin()) }
fn arccos_d(x: f64) -> f64 { rtd(x.acos()) }
fn arccot_d(x: f64) -> f64 { rtd((1.0 / x).atan()) }
fn arctan2_d(y: f64, x: f64) -> f64 { rtd(y.atan2(x)) }
fn fix_angle(a: f64) -> f64 { a.rem_euclid(360.0) }
fn fix_hour(a: f64) -> f64 { a.rem_euclid(24.0) }

pub fn julian(y: i32, m: i32, d: i32) -> f64 {
    let (mut y, mut m) = (y, m);
    if m <= 2 {
        y -= 1;
        m += 12;
    }
    let a = (y as f64 / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    (365.25 * (y as f64 + 4716.0)).floor() + (30.6001 * (m as f64 + 1.0)).floor() + d as f64 + b - 1524.5
}

/// (deklinasi, equation of time dalam jam)
fn sun_position(jd: f64) -> (f64, f64) {
    let d = jd - 2451545.0;
    let g = fix_angle(357.529 + 0.98560028 * d);
    let q = fix_angle(280.459 + 0.98564736 * d);
    let l = fix_angle(q + 1.915 * sin_d(g) + 0.020 * sin_d(2.0 * g));
    let e = 23.439 - 0.00000036 * d;
    let ra = fix_hour(arctan2_d(cos_d(e) * sin_d(l), cos_d(l)) / 15.0);
    let decl = arcsin_d(sin_d(e) * sin_d(l));
    let eqt = (q / 15.0 - ra + 12.0).rem_euclid(24.0) - 12.0;
    (decl, eqt)
}

struct Ctx {
    jd: f64,
    lat: f64,
}

impl Ctx {
    fn midday(&self, t: f64) -> f64 {
        fix_hour(12.0 - sun_position(self.jd + t / 24.0).1)
    }
    fn angle_time(&self, angle: f64, t: f64, ccw: bool) -> f64 {
        let decl = sun_position(self.jd + t / 24.0).0;
        let noon = self.midday(t);
        let x = (-sin_d(angle) - sin_d(decl) * sin_d(self.lat)) / (cos_d(decl) * cos_d(self.lat));
        if !(-1.0..=1.0).contains(&x) {
            return f64::NAN;
        }
        let h = arccos_d(x) / 15.0;
        noon + if ccw { -h } else { h }
    }
    fn asr_time(&self, factor: f64, t: f64) -> f64 {
        let decl = sun_position(self.jd + t / 24.0).0;
        let angle = -arccot_d(factor + tan_d((self.lat - decl).abs()));
        self.angle_time(angle, t, false)
    }
}

/// Waktu salat (jam lokal, 0..24) untuk tanggal tertentu.
pub fn compute(y: i32, m: i32, d: i32, lat: f64, lon: f64, tz: f64, method: &Method, hanafi: bool) -> [f64; 6] {
    let ctx = Ctx { jd: julian(y, m, d) - lon / 360.0, lat };
    let mut t = [5.0, 6.0, 12.0, 13.0, 18.0, 18.0];
    let factor = if hanafi { 2.0 } else { 1.0 };
    for _ in 0..2 {
        let fajr = ctx.angle_time(method.fajr, t[0], true);
        let sunrise = ctx.angle_time(0.833, t[1], true);
        let dhuhr = ctx.midday(t[2]);
        let asr = ctx.asr_time(factor, t[3]);
        let maghrib = ctx.angle_time(0.833, t[4], false);
        let isha = if method.isha_min > 0.0 { maghrib + method.isha_min / 60.0 } else { ctx.angle_time(method.isha, t[5], false) };
        t = [fajr, sunrise, dhuhr, asr, maghrib, isha];
    }
    let adj = tz - lon / 15.0;
    let ih = method.ihtiyat / 60.0;
    let mut out = [0.0; 6];
    for i in 0..6 {
        let mut v = t[i] + adj;
        if i == 1 {
            v -= ih;
        } else {
            v += ih;
        }
        out[i] = v;
    }
    out
}

pub fn fmt_hm(h: f64) -> String {
    if h.is_nan() {
        return "--:--".into();
    }
    let total = (h * 60.0).round() as i64;
    let total = total.rem_euclid(24 * 60);
    format!("{:02}:{:02}", total / 60, total % 60)
}

// ---- aritmetika tanggal (algoritma Howard Hinnant) ----
pub fn days_from_civil(y: i32, m: i32, d: i32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i32, i32, i32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    ((if m <= 2 { y + 1 } else { y }) as i32, m as i32, d as i32)
}

pub struct Next {
    pub idx: usize,
    pub time: f64,
    /// menit tersisa
    pub minutes_left: i64,
    pub today: [f64; 6],
}

/// Salat wajib berikutnya dari (tahun, bulan, hari, jam-desimal sekarang).
pub fn next_prayer(y: i32, m: i32, d: i32, now_h: f64, lat: f64, lon: f64, tz: f64, method: &Method, hanafi: bool) -> Next {
    let today = compute(y, m, d, lat, lon, tz, method, hanafi);
    for &i in &[0usize, 2, 3, 4, 5] {
        if today[i].is_finite() && today[i] > now_h {
            return Next { idx: i, time: today[i], minutes_left: ((today[i] - now_h) * 60.0).ceil() as i64, today };
        }
    }
    let (ty, tm, td) = civil_from_days(days_from_civil(y, m, d) + 1);
    let tom = compute(ty, tm, td, lat, lon, tz, method, hanafi);
    let left = ((tom[0] + 24.0 - now_h) * 60.0).ceil() as i64;
    Next { idx: 0, time: tom[0], minutes_left: left, today }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jakarta_sanity() {
        // Jakarta, 2026-10-31, UTC+7: Dzuhur sekitar 11:4x-11:5x, Maghrib ~17:5x-18:0x, Subuh ~04:0x-04:2x
        let t = compute(2026, 10, 31, -6.2088, 106.8456, 7.0, &METHODS[0], false);
        for (i, v) in t.iter().enumerate() {
            println!("{} {}", NAMES[i], fmt_hm(*v));
        }
        assert!(t[2] > 11.5 && t[2] < 12.0, "dzuhur {}", t[2]);
        assert!(t[4] > 17.6 && t[4] < 18.3, "maghrib {}", t[4]);
        assert!(t[0] > 3.8 && t[0] < 4.6, "subuh {}", t[0]);
        assert!(t[3] > 14.5 && t[3] < 15.6, "ashar {}", t[3]);
    }
    #[test]
    fn dates() {
        assert_eq!(civil_from_days(days_from_civil(2026, 10, 31) + 1), (2026, 11, 1));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 28) + 1), (2024, 2, 29));
    }
}
