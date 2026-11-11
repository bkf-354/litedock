//! Aksi sistem untuk modul dock: Recycle Bin, File Explorer, tombol daya.
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows::core::PCWSTR;
use windows::Win32::UI::Shell::*;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// (jumlah item, ada isi?) di Recycle Bin.
pub fn bin_status() -> (i64, bool) {
    unsafe {
        let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
        if SHQueryRecycleBinW(PCWSTR::null(), &mut info).is_ok() {
            return (info.i64NumItems, info.i64NumItems > 0);
        }
    }
    (0, false)
}

pub fn open_shell_folder(arg: &str) {
    let _ = Command::new("explorer.exe").arg(arg).spawn();
}

/// Buka File Explorer (Beranda).
pub fn open_explorer() {
    let _ = Command::new("explorer.exe").spawn();
}

pub fn bin_open() {
    open_shell_folder("shell:RecycleBinFolder");
}

/// Kosongkan Recycle Bin (Windows menampilkan konfirmasinya sendiri).
pub fn bin_empty() {
    unsafe {
        let _ = SHEmptyRecycleBinW(None, PCWSTR::null(), 0);
    }
}

fn hidden(exe: &str, args: &[&str]) {
    let _ = Command::new(exe).args(args).creation_flags(CREATE_NO_WINDOW).spawn();
}

pub fn lock() {
    unsafe {
        let _ = windows::Win32::System::Shutdown::LockWorkStation();
    }
}

pub fn sleep() {
    // tidur lewat powrprof; bila hibernasi aktif Windows memakai hibernasi (perilaku bawaan SetSuspendState)
    hidden("rundll32.exe", &["powrprof.dll,SetSuspendState", "0,1,0"]);
}

pub fn restart() {
    hidden("shutdown.exe", &["/r", "/t", "0"]);
}

pub fn shutdown() {
    hidden("shutdown.exe", &["/s", "/t", "0"]);
}

pub fn signout() {
    hidden("shutdown.exe", &["/l"]);
}
