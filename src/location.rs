//! Deteksi lokasi otomatis: Lokasi Windows (WinRT Geolocator) lebih dulu, cadangan lewat IP (HTTPS, WinHTTP).
//! Dipanggil dari thread pekerja; tidak pernah memblokir UI.
use crate::config::CITIES;

#[derive(Clone, Debug)]
pub struct Detected {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
    pub source: &'static str,
}

/// Cari angka JSON sederhana: "key":-6.17
pub fn json_num(s: &str, key: &str) -> Option<f64> {
    let pat = format!("\"{}\"", key);
    let i = s.find(&pat)? + pat.len();
    let rest = s[i..].trim_start().strip_prefix(':')?.trim_start();
    let end = rest.find(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E')).unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Cari string JSON sederhana: "key":"Jakarta" (tanpa escape yang rumit)
pub fn json_str(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{}\"", key);
    let i = s.find(&pat)? + pat.len();
    let rest = s[i..].trim_start().strip_prefix(':')?.trim_start().strip_prefix('"')?;
    let end = rest.find('"')?;
    let v: String = rest[..end].chars().filter(|c| !c.is_control() && *c != '=').take(48).collect();
    if v.is_empty() { None } else { Some(v) }
}

fn haversine_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = 6371.0;
    let (la1, la2) = (a.0.to_radians(), b.0.to_radians());
    let dla = la2 - la1;
    let dlo = (b.1 - a.1).to_radians();
    let h = (dla / 2.0).sin().powi(2) + la1.cos() * la2.cos() * (dlo / 2.0).sin().powi(2);
    2.0 * r * h.sqrt().asin()
}

/// Nama kota preset terdekat bila dalam radius 45 km.
/// GET HTTPS sederhana (WinHTTP); dipakai juga oleh modul cuaca.
#[cfg(windows)]
pub fn http_get(host: &str, path: &str) -> Option<String> {
    win::http_get(host, path)
}

pub fn nearest_city(lat: f64, lon: f64) -> Option<&'static str> {
    CITIES[..CITIES.len() - 1]
        .iter()
        .map(|c| (c.0, haversine_km((lat, lon), (c.1, c.2))))
        .filter(|x| x.1 <= 45.0)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|x| x.0)
}

fn valid(lat: f64, lon: f64) -> bool {
    lat.is_finite() && lon.is_finite() && (-66.0..=66.0).contains(&lat) && (-180.0..=180.0).contains(&lon) && !(lat == 0.0 && lon == 0.0)
}

pub fn parse_ip_json(body: &str) -> Option<Detected> {
    // ipwho.is memakai "success":true; ipapi.co tidak punya field itu tetapi memakai "error":true saat gagal
    if body.contains("\"success\":false") || body.contains("\"success\": false") || body.contains("\"error\":true") || body.contains("\"error\": true") {
        return None;
    }
    let lat = json_num(body, "latitude")?;
    let lon = json_num(body, "longitude")?;
    if !valid(lat, lon) {
        return None;
    }
    let name = json_str(body, "city").or_else(|| nearest_city(lat, lon).map(|s| s.to_string())).unwrap_or_default();
    Some(Detected { lat, lon, name, source: "IP" })
}

#[cfg(windows)]
mod win {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Devices::Geolocation::{Geolocator, PositionAccuracy};
    use windows::Foundation::TimeSpan;
    use windows::Win32::Networking::WinHttp::*;
    use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn windows_location() -> Option<(f64, f64)> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let r = (|| {
            let g = Geolocator::new().ok()?;
            let _ = g.SetDesiredAccuracy(PositionAccuracy::Default);
            let op = g
                .GetGeopositionAsyncWithAgeAndTimeout(TimeSpan { Duration: 30 * 60 * 10_000_000 }, TimeSpan { Duration: 12 * 10_000_000 })
                .ok()?;
            let pos = op.join().ok()?;
            let bp = pos.Coordinate().ok()?.Point().ok()?.Position().ok()?;
            Some((bp.Latitude, bp.Longitude))
        })();
        unsafe {
            CoUninitialize();
        }
        r.filter(|(la, lo)| valid(*la, *lo))
    }

    /// GET https://host/path (TLS divalidasi oleh WinHTTP). Respons dibatasi 64 KB.
    pub fn http_get(host: &str, path: &str) -> Option<String> {
        unsafe {
            let agent = wide("LiteDock/1.1");
            let session = WinHttpOpen(PCWSTR(agent.as_ptr()), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, PCWSTR::null(), PCWSTR::null(), 0);
            if session.is_null() {
                return None;
            }
            let _ = WinHttpSetTimeouts(session, 5000, 5000, 8000, 8000);
            let hw = wide(host);
            let conn = WinHttpConnect(session, PCWSTR(hw.as_ptr()), INTERNET_DEFAULT_HTTPS_PORT, 0);
            let mut out: Option<String> = None;
            if !conn.is_null() {
                let verb = wide("GET");
                let pw = wide(path);
                let req = WinHttpOpenRequest(conn, PCWSTR(verb.as_ptr()), PCWSTR(pw.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), WINHTTP_FLAG_SECURE);
                if !req.is_null() {
                    if WinHttpSendRequest(req, None, None, 0, 0, 0).is_ok() && WinHttpReceiveResponse(req, std::ptr::null_mut()).is_ok() {
                        let mut body: Vec<u8> = Vec::new();
                        loop {
                            let mut avail = 0u32;
                            if WinHttpQueryDataAvailable(req, &mut avail).is_err() || avail == 0 {
                                break;
                            }
                            let mut buf = vec![0u8; avail.min(16384) as usize];
                            let mut read = 0u32;
                            if WinHttpReadData(req, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut read).is_err() || read == 0 {
                                break;
                            }
                            body.extend_from_slice(&buf[..read as usize]);
                            if body.len() > 65536 {
                                break;
                            }
                        }
                        out = String::from_utf8(body).ok();
                    }
                    let _ = WinHttpCloseHandle(req);
                }
                let _ = WinHttpCloseHandle(conn);
            }
            let _ = WinHttpCloseHandle(session);
            out
        }
    }

    pub fn detect() -> Option<Detected> {
        if let Some((lat, lon)) = windows_location() {
            let name = nearest_city(lat, lon).unwrap_or("Lokasi Windows").to_string();
            return Some(Detected { lat, lon, name, source: "Lokasi Windows" });
        }
        for (host, path) in [("ipwho.is", "/?fields=success,latitude,longitude,city"), ("ipapi.co", "/json/")] {
            if let Some(body) = http_get(host, path) {
                if let Some(d) = parse_ip_json(&body) {
                    return Some(d);
                }
            }
        }
        None
    }
}

#[cfg(windows)]
pub use win::detect;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_ipwho() {
        let b = r#"{"success":true,"latitude":-6.1753924,"longitude":106.8271528,"city":"Jakarta"}"#;
        let d = parse_ip_json(b).unwrap();
        assert!((d.lat + 6.1753924).abs() < 1e-6 && (d.lon - 106.8271528).abs() < 1e-6);
        assert_eq!(d.name, "Jakarta");
    }
    #[test]
    fn parse_ipapi_and_failures() {
        let b = "{\n \"city\": \"Bandung\",\n \"latitude\": -6.9175,\n \"longitude\": 107.6191\n}";
        assert_eq!(parse_ip_json(b).unwrap().name, "Bandung");
        assert!(parse_ip_json(r#"{"success":false,"message":"x"}"#).is_none());
        assert!(parse_ip_json(r#"{"error":true,"reason":"RateLimited"}"#).is_none());
        assert!(parse_ip_json(r#"{"latitude":0,"longitude":0}"#).is_none());
        assert!(parse_ip_json(r#"{"latitude":95,"longitude":10}"#).is_none());
    }
    #[test]
    fn nearest() {
        assert_eq!(nearest_city(-6.25, 106.85), Some("Jakarta"));
        assert_eq!(nearest_city(10.0, 10.0), None);
    }
}
