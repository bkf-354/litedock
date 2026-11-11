//! Volume utama Windows lewat IAudioEndpointVolume (dipakai Pusat Kontrol).
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};

unsafe fn endpoint() -> Option<IAudioEndpointVolume> {
    let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
    let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia).ok()?;
    dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None).ok()
}

/// (level 0..1, bisu)
pub fn get() -> Option<(f32, bool)> {
    unsafe {
        let ep = endpoint()?;
        let v = ep.GetMasterVolumeLevelScalar().ok()?;
        let m = ep.GetMute().map(|b| b.as_bool()).unwrap_or(false);
        Some((v, m))
    }
}

pub fn set(level: f32) {
    unsafe {
        if let Some(ep) = endpoint() {
            let _ = ep.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null());
            if level > 0.0 {
                let _ = ep.SetMute(false, std::ptr::null());
            }
        }
    }
}

pub fn set_mute(m: bool) {
    unsafe {
        if let Some(ep) = endpoint() {
            let _ = ep.SetMute(m, std::ptr::null());
        }
    }
}
