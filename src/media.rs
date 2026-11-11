//! Pemutar musik: membaca sesi media Windows (SMTC) di thread latar dan mengirim perintah putar/jeda/lanjut/sebelumnya.
use std::sync::Mutex;
use std::time::Duration;
use windows::Media::Control::*;
use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Default, PartialEq)]
pub struct MediaSnap {
    pub title: String,
    pub artist: String,
    pub app: String,
    pub playing: bool,
}

static SNAP: Mutex<Option<MediaSnap>> = Mutex::new(None);
static THREAD: std::sync::OnceLock<std::thread::Thread> = std::sync::OnceLock::new();
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Pembacaan media hanya berjalan bila modul musik diaktifkan.
pub fn set_enabled(on: bool) {
    let was = ENABLED.swap(on, std::sync::atomic::Ordering::Relaxed);
    if on && !was {
        kick();
    }
}

pub fn snapshot() -> Option<MediaSnap> {
    SNAP.lock().ok().and_then(|g| g.clone())
}

fn session() -> Option<GlobalSystemMediaTransportControlsSession> {
    let mgr = GlobalSystemMediaTransportControlsSessionManager::RequestAsync().ok()?.join().ok()?;
    mgr.GetCurrentSession().ok()
}

fn read() -> Option<MediaSnap> {
    let s = session()?;
    let props = s.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = props.Title().ok()?.to_string();
    let artist = props.Artist().map(|a| a.to_string()).unwrap_or_default();
    let playing = s.GetPlaybackInfo().ok().and_then(|i| i.PlaybackStatus().ok()).map(|p| p == GlobalSystemMediaTransportControlsSessionPlaybackStatus::Playing).unwrap_or(false);
    let app = s.SourceAppUserModelId().map(|a| a.to_string()).unwrap_or_default();
    if title.is_empty() && artist.is_empty() {
        return None;
    }
    Some(MediaSnap { title, artist, app, playing })
}

/// Mulai thread pembaca media (polling ~1,2 detik, hanya mengirim pesan bila berubah).
pub fn start(hwnd: isize, msg: u32) {
    let h = std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        loop {
            if !ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::park_timeout(Duration::from_secs(5));
                continue;
            }
            let now = read();
            let changed = {
                let mut g = SNAP.lock().unwrap_or_else(|e| e.into_inner());
                let ch = *g != now;
                *g = now;
                ch
            };
            if changed {
                unsafe {
                    let _ = PostMessageW(Some(HWND(hwnd as *mut _)), msg, WPARAM(0), LPARAM(0));
                }
            }
            std::thread::park_timeout(Duration::from_millis(1200));
        }
    });
    let _ = THREAD.set(h.thread().clone());
}

pub fn kick() {
    if let Some(t) = THREAD.get() {
        t.unpark();
    }
}

#[derive(Clone, Copy)]
pub enum Cmd {
    PlayPause,
    Next,
    Prev,
}

/// Kirim perintah ke sesi media saat ini (di thread terpisah agar UI tidak menunggu).
pub fn command(c: Cmd) {
    std::thread::spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_MULTITHREADED);
        }
        if let Some(s) = session() {
            let _ = match c {
                Cmd::PlayPause => s.TryTogglePlayPauseAsync().and_then(|o| o.join()),
                Cmd::Next => s.TrySkipNextAsync().and_then(|o| o.join()),
                Cmd::Prev => s.TrySkipPreviousAsync().and_then(|o| o.join()),
            };
        }
        kick();
    });
}
