//! Whether Art Window comes back on its own when you log in.
//!
//! The `Run` key under the user's own hive: no elevation, no scheduled task, and
//! like the launchd agent and the autostart file elsewhere, the setting *is*
//! whether the value exists. The installer writes the same value under the same
//! name, so the menu and the installer agree about a single setting.
//!
//! Nothing here starts anything; the value only means something at the next login.

use anyhow::{Context, Result};
use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const VALUE: PCWSTR = w!("ArtWindow");

/// Read fresh on every menu open: the user can also switch this off from Task
/// Manager, and a remembered answer would quietly start lying.
pub fn is_enabled() -> bool {
    // SAFETY: no output buffer is requested, so this only asks whether the value exists.
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    found.is_ok()
}

pub fn set(enabled: bool) -> Result<()> {
    if !enabled {
        // SAFETY: both names are static, NUL-terminated wide strings.
        let deleted = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE) };
        return if deleted == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            deleted.ok().context("removing the start-at-login entry")
        };
    }

    // Whichever binary is running claims the slot. Quoted, because the path may
    // hold spaces and an unquoted one is split at the first of them.
    let exe = std::env::current_exe().context("cannot find my own binary")?;
    let command = HSTRING::from(format!("\"{}\"", exe.display()));
    // REG_SZ is counted in bytes, terminator included; `HSTRING` keeps one past its
    // end, so the slice below is the string plus that terminator.
    let bytes = (command.len() + 1) * 2;
    // SAFETY: `command` outlives the call and holds `bytes` readable bytes.
    unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE,
            REG_SZ.0,
            Some(command.as_ptr().cast()),
            bytes as u32,
        )
    }
    .ok()
    .context("registering to start at login")
}
