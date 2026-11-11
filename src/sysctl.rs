//! Kontrol sistem untuk panel Wi-Fi/baterai: jaringan tersedia, Wi-Fi/mode pesawat/hotspot, penghemat baterai, mode daya.
//! Semua fungsi memblokir sebentar -> dijalankan dari thread pekerja, hasilnya dikirim ke jendela panel.
use windows::core::PCWSTR;
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Networking::Connectivity::NetworkInformation;
use windows::Networking::NetworkOperators::{NetworkOperatorTetheringManager, TetheringCapability, TetheringOperationStatus, TetheringOperationalState};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::NetworkManagement::WiFi::*;
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

#[derive(Clone, Default, Debug)]
pub struct AvNet {
    pub ssid: String,
    pub signal: u32,
    pub secured: bool,
    pub profile: String,
    pub connected: bool,
}

#[derive(Clone, Default, Debug)]
pub struct NetState {
    pub wifi_on: Option<bool>,
    pub bt_on: Option<bool>,
    pub airplane: Option<bool>,
    pub hotspot: Option<bool>,
    pub hotspot_ok: bool,
    pub nets: Vec<AvNet>,
}

struct Com;
impl Com {
    fn new() -> Com {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        Com
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        unsafe {
            CoUninitialize();
        }
    }
}

fn radios() -> Vec<Radio> {
    let _ = Radio::RequestAccessAsync().and_then(|op| op.join());
    match Radio::GetRadiosAsync().and_then(|op| op.join()) {
        Ok(list) => list.into_iter().collect(),
        Err(_) => Vec::new(),
    }
}

fn utf16_to_string(buf: &[u16]) -> String {
    let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..n])
}

fn wlan_networks(do_scan: bool) -> Vec<AvNet> {
    let mut out: Vec<AvNet> = Vec::new();
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return out;
        }
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let n = (*list).dwNumberOfItems as usize;
            let base = (*list).InterfaceInfo.as_ptr();
            if do_scan {
                for i in 0..n {
                    let it = &*base.add(i);
                    let _ = WlanScan(h, &it.InterfaceGuid, None, None, None);
                }
                std::thread::sleep(std::time::Duration::from_millis(1300));
            }
            for i in 0..n {
                let it = &*base.add(i);
                let mut nl: *mut WLAN_AVAILABLE_NETWORK_LIST = std::ptr::null_mut();
                if WlanGetAvailableNetworkList(h, &it.InterfaceGuid, 0, None, &mut nl) == 0 && !nl.is_null() {
                    let cnt = (*nl).dwNumberOfItems as usize;
                    let nb = (*nl).Network.as_ptr();
                    for j in 0..cnt {
                        let a = &*nb.add(j);
                        let len = (a.dot11Ssid.uSSIDLength as usize).min(32);
                        let ssid = String::from_utf8_lossy(&a.dot11Ssid.ucSSID[..len]).to_string();
                        if ssid.is_empty() {
                            continue;
                        }
                        let net = AvNet {
                            ssid: ssid.clone(),
                            signal: a.wlanSignalQuality,
                            secured: a.bSecurityEnabled.as_bool(),
                            profile: if a.dwFlags & 2 != 0 { utf16_to_string(&a.strProfileName) } else { String::new() },
                            connected: a.dwFlags & 1 != 0,
                        };
                        if let Some(e) = out.iter_mut().find(|x| x.ssid == ssid) {
                            if net.connected || net.signal > e.signal {
                                let keep_profile = if net.profile.is_empty() { e.profile.clone() } else { net.profile.clone() };
                                *e = net;
                                e.profile = keep_profile;
                            }
                        } else {
                            out.push(net);
                        }
                    }
                    WlanFreeMemory(nl as *const _);
                }
            }
            WlanFreeMemory(list as *const _);
        }
        let _ = WlanCloseHandle(h, None);
    }
    out.sort_by(|a, b| b.connected.cmp(&a.connected).then(b.signal.cmp(&a.signal)));
    out
}

/// Keadaan radio + hotspot + jaringan tersedia. `do_scan` memicu pemindaian Wi-Fi baru (~1,3 dtk).
pub fn scan_state(do_scan: bool) -> NetState {
    let _com = Com::new();
    let mut st = NetState::default();
    let rs = radios();
    if !rs.is_empty() {
        let mut wifi = None;
        let mut bt = None;
        let mut all_off = true;
        for r in &rs {
            let on = r.State().map(|s| s == RadioState::On).unwrap_or(false);
            if on {
                all_off = false;
            }
            if r.Kind().map(|k| k == RadioKind::WiFi).unwrap_or(false) {
                wifi = Some(wifi.unwrap_or(false) || on);
            }
            if r.Kind().map(|k| k == RadioKind::Bluetooth).unwrap_or(false) {
                bt = Some(bt.unwrap_or(false) || on);
            }
        }
        st.wifi_on = wifi;
        st.bt_on = bt;
        st.airplane = Some(all_off);
    }
    if let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() {
        if let Ok(cap) = NetworkOperatorTetheringManager::GetTetheringCapabilityFromConnectionProfile(&profile) {
            st.hotspot_ok = cap == TetheringCapability::Enabled;
            if st.hotspot_ok {
                if let Ok(m) = NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile) {
                    st.hotspot = m.TetheringOperationalState().ok().map(|s| s == TetheringOperationalState::On);
                }
            }
        }
    }
    if st.wifi_on != Some(false) {
        st.nets = wlan_networks(do_scan);
    }
    st
}

/// Hidupkan/matikan semua radio (mode pesawat) atau hanya Wi-Fi.
pub fn set_radios(only_wifi: bool, on: bool) -> bool {
    let _com = Com::new();
    let mut ok = false;
    for r in radios() {
        if only_wifi && !r.Kind().map(|k| k == RadioKind::WiFi).unwrap_or(false) {
            continue;
        }
        let want = if on { RadioState::On } else { RadioState::Off };
        if let Ok(op) = r.SetStateAsync(want) {
            if op.join().map(|s| s == RadioAccessStatus::Allowed).unwrap_or(false) {
                ok = true;
            }
        }
    }
    ok
}

/// Hidupkan/matikan radio Bluetooth.
pub fn set_bluetooth(on: bool) -> bool {
    let _com = Com::new();
    let mut ok = false;
    for r in radios() {
        if !r.Kind().map(|k| k == RadioKind::Bluetooth).unwrap_or(false) {
            continue;
        }
        let want = if on { RadioState::On } else { RadioState::Off };
        if let Ok(op) = r.SetStateAsync(want) {
            if op.join().map(|s| s == RadioAccessStatus::Allowed).unwrap_or(false) {
                ok = true;
            }
        }
    }
    ok
}

pub fn set_hotspot(on: bool) -> bool {
    let _com = Com::new();
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else { return false };
    let Ok(m) = NetworkOperatorTetheringManager::CreateFromConnectionProfile(&profile) else { return false };
    let op = if on { m.StartTetheringAsync() } else { m.StopTetheringAsync() };
    match op.and_then(|o| o.join()) {
        Ok(r) => r.Status().map(|s| s == TetheringOperationStatus::Success).unwrap_or(false),
        Err(_) => false,
    }
}

pub fn wlan_connect(profile: &str) -> bool {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return false;
        }
        let mut ok = false;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let n = (*list).dwNumberOfItems as usize;
            let base = (*list).InterfaceInfo.as_ptr();
            let wp: Vec<u16> = profile.encode_utf16().chain(std::iter::once(0)).collect();
            for i in 0..n {
                let it = &*base.add(i);
                let params = WLAN_CONNECTION_PARAMETERS {
                    wlanConnectionMode: wlan_connection_mode_profile,
                    strProfile: PCWSTR(wp.as_ptr()),
                    pDot11Ssid: std::ptr::null_mut(),
                    pDesiredBssidList: std::ptr::null_mut(),
                    dot11BssType: dot11_BSS_type_any,
                    dwFlags: 0,
                };
                if WlanConnect(h, &it.InterfaceGuid, &params, None) == 0 {
                    ok = true;
                }
            }
            WlanFreeMemory(list as *const _);
        }
        let _ = WlanCloseHandle(h, None);
        ok
    }
}

pub fn wlan_disconnect() -> bool {
    unsafe {
        let mut ver = 0u32;
        let mut h = HANDLE::default();
        if WlanOpenHandle(2, None, &mut ver, &mut h) != 0 {
            return false;
        }
        let mut ok = false;
        let mut list: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
        if WlanEnumInterfaces(h, None, &mut list) == 0 && !list.is_null() {
            let n = (*list).dwNumberOfItems as usize;
            let base = (*list).InterfaceInfo.as_ptr();
            for i in 0..n {
                let it = &*base.add(i);
                if WlanDisconnect(h, &it.InterfaceGuid, None) == 0 {
                    ok = true;
                }
            }
            WlanFreeMemory(list as *const _);
        }
        let _ = WlanCloseHandle(h, None);
        ok
    }
}

// ------------------------------------------------------------------ daya

fn run_hidden(exe: &str, args: &[&str]) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new(exe).args(args).creation_flags(0x0800_0000).status().map(|s| s.success()).unwrap_or(false)
}

/// Penghemat baterai (Energy Saver): atur ambang aktif otomatis. 100 = aktif selama memakai baterai, 0 = nonaktif.
pub fn energy_saver_set(on: bool) -> bool {
    let v = if on { "100" } else { "0" };
    let a = run_hidden("powercfg.exe", &["/setdcvalueindex", "SCHEME_CURRENT", "SUB_ENERGYSAVER", "ESBATTTHRESHOLD", v]);
    let b = run_hidden("powercfg.exe", &["/setactive", "SCHEME_CURRENT"]);
    a && b
}

fn power_fn(name: &[u8]) -> Option<usize> {
    use windows::core::PCSTR;
    use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    unsafe {
        let lib = LoadLibraryW(windows::core::w!("powrprof.dll")).ok()?;
        GetProcAddress(lib, PCSTR(name.as_ptr())).map(|f| f as usize)
    }
}

const G_EFFICIENCY: u128 = 0x961cc777_2547_4f9d_8174_7d86181b8a7a;
const G_BALANCED: u128 = 0x00000000_0000_0000_0000_000000000000;
const G_PERF: u128 = 0xded574b5_45a0_4f42_8737_46345c09c238;
const G_BETTER: u128 = 0x3af9b8d9_7c97_431d_ad78_34a8bfea439f;

/// Mode daya aktif: 0 hemat daya, 1 seimbang, 2 performa.
pub fn power_mode_get() -> Option<u8> {
    use windows::core::GUID;
    let f = power_fn(b"PowerGetActualOverlayScheme\0")?;
    let func: extern "system" fn(*mut GUID) -> u32 = unsafe { std::mem::transmute(f) };
    let mut g = GUID::zeroed();
    if func(&mut g) != 0 {
        return None;
    }
    let v = g.to_u128();
    Some(if v == G_EFFICIENCY { 0 } else if v == G_PERF { 2 } else if v == G_BETTER { 1 } else { 1 })
}

pub fn power_mode_set(mode: u8) -> bool {
    use windows::core::GUID;
    let Some(f) = power_fn(b"PowerSetActiveOverlayScheme\0") else { return false };
    let func: extern "system" fn(*const GUID) -> u32 = unsafe { std::mem::transmute(f) };
    let g = GUID::from_u128(match mode { 0 => G_EFFICIENCY, 2 => G_PERF, _ => G_BALANCED });
    func(&g) == 0
}
