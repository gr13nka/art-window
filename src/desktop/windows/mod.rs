mod instance;
mod login;
mod wallpaper;

use super::Pinned;
use anyhow::{Context, Result};
use std::os::windows::io::IntoRawHandle;
use std::path::Path;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Console::{
    AttachConsole, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNORMAL,
};

pub(crate) use instance::Instance;

pub(super) fn pin(hang: &crate::placement::Hang) -> Result<Pinned> {
    let pinned = wallpaper::pin(hang);
    match &pinned {
        Ok(result) => {
            crate::journal::note!("pin", "path {}, answered {result:?}", hang.path.display())
        }
        Err(error) => crate::journal::fault("pin", error),
    }
    pinned
}

/// The primary display's size in physical pixels; the process is PerMonitorV2 DPI
/// aware, so `GetSystemMetrics` does not answer in scaled units.
pub(super) fn primary_screen() -> Option<(u32, u32)> {
    // SAFETY: plain queries with no arguments to borrow.
    let (w, h) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    (w > 0 && h > 0).then_some((w as u32, h as u32))
}

/// Nothing to publish: the shell service `pin` writes through repaints as it goes.
/// If it turns out that other virtual desktops lag, this is where their redraw
/// would be made — see the note at the top of `wallpaper.rs`.
pub(super) fn catch_up(_reason: &str) {}

pub(super) fn browse(url: &str) {
    let url = windows::core::HSTRING::from(url);
    // SAFETY: every string outlives the call; the result is only a status code, and
    // a browser that fails to open is not worth interrupting anyone for.
    unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            &url,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

pub(super) fn starts_at_login() -> bool {
    login::is_enabled()
}

pub(super) fn set_start_at_login(enabled: bool) -> Result<()> {
    login::set(enabled)
}

pub(super) fn claim_instance(
    on_quit: impl Fn() + Send + Sync + 'static,
) -> Result<Option<Instance>> {
    instance::claim(on_quit)
}

pub(super) fn quit_running() -> Result<()> {
    instance::quit_running()
}

/// Whether the taskbar is drawn light. Windows 10 has no such setting and its
/// taskbar is dark, so a missing value reads as dark.
pub(super) fn light_taskbar() -> bool {
    let mut light = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: `light` is a DWORD-sized buffer and `size` says so.
    let read = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("SystemUsesLightTheme"),
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut light).cast()),
            Some(&mut size),
        )
    };
    read.is_ok() && light != 0
}

/// Rust's `println!` looks the standard handle up on every write, so attaching after
/// start-up is early enough. Failing means there is no parent console — launched
/// from Explorer — and then there is nobody to print to.
pub(super) fn attach_console() {
    // SAFETY: no arguments beyond a constant.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

pub(super) fn log_to(path: &Path) {
    let _ = redirect_stderr(path);
}

fn redirect_stderr(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("opening {}", path.display()))?;
    // Leaked on purpose: the process's stderr is this handle from now until it exits.
    let handle = HANDLE(file.into_raw_handle());
    // SAFETY: the handle is valid and never closed.
    unsafe { SetStdHandle(STD_ERROR_HANDLE, handle) }.context("redirecting stderr")
}
