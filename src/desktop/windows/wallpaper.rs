//! Putting a picture on the Windows desktop.
//!
//! `IDesktopWallpaper` is the shell's own answer to this, and unlike the older
//! `SystemParametersInfo` route it takes placement and background colour as
//! separate settings, so fit-with-black-margins is stated rather than inferred.
//!
//! Whether one call reaches every Windows 11 virtual desktop is **unverified**. If
//! it turns out that a desktop other than the one in front keeps its old picture,
//! this backend is in the same position as macOS with its Spaces — see
//! `docs/macos-wallpaper.md` — and the honest answer becomes `Pinned::InPart` or
//! `Pinned::AfterRedraw`, not `Everywhere`.

use crate::desktop::Pinned;
use anyhow::{bail, Context, Result};
use std::path::Path;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{COLORREF, RPC_E_CHANGED_MODE};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{DesktopWallpaper, IDesktopWallpaper, DWPOS_FIT};

pub fn pin(path: &Path) -> Result<Pinned> {
    // Any thread may be asked to pin, and COM has to be entered on the one doing it.
    // A thread that already chose the other apartment model is left as it is — the
    // desktop object is usable from either.
    // SAFETY: plain initialisation of the calling thread; nothing is borrowed.
    let entered = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if entered != RPC_E_CHANGED_MODE {
        entered.ok().context("initialising COM")?;
    }

    // SAFETY: `DesktopWallpaper` is the shell's registered class for this interface.
    let desktop: IDesktopWallpaper =
        unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }
            .context("reaching the shell's wallpaper service")?;

    let wanted = shell_path(&path.to_string_lossy());
    let wanted_h = HSTRING::from(&wanted);

    // SAFETY: every argument outlives its call; a null monitor id means all of them.
    unsafe {
        desktop
            .SetBackgroundColor(COLORREF(0))
            .context("setting the margins to black")?;
        desktop
            .SetPosition(DWPOS_FIT)
            .context("setting the wallpaper to fit")?;
        desktop
            .SetWallpaper(PCWSTR::null(), &wanted_h)
            .context("setting the wallpaper")?;
    }

    // The call answers success for a picture it may not have kept, so ask each
    // monitor what it is showing now.
    // SAFETY: the strings the shell returns are ours to free, and each is freed once.
    unsafe {
        let count = desktop
            .GetMonitorDevicePathCount()
            .context("counting monitors")?;
        for index in 0..count {
            let monitor = take(desktop.GetMonitorDevicePathAt(index)?);
            let monitor = HSTRING::from(&monitor);
            let shown = take(desktop.GetWallpaper(&monitor)?);
            if !shown.eq_ignore_ascii_case(&wanted) {
                bail!("a monitor kept {shown} instead of {wanted}");
            }
        }
    }
    Ok(Pinned::Everywhere)
}

/// Reads a shell-allocated wide string and hands its memory back.
///
/// # Safety
/// `string` must come from a shell call that allocates with `CoTaskMemAlloc`.
unsafe fn take(string: windows::core::PWSTR) -> String {
    let text = string.to_string().unwrap_or_default();
    CoTaskMemFree(Some(string.as_ptr().cast()));
    text
}

/// `Path::canonicalize` answers `\\?\C:\…`, the verbatim form, and the shell's
/// wallpaper service refuses it without saying why. The plain form names the same
/// file, and a network share comes back as `\\server\share` rather than
/// `\\?\UNC\server\share`.
fn shell_path(canonical: &str) -> String {
    if let Some(share) = canonical.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{share}")
    } else {
        canonical
            .strip_prefix(r"\\?\")
            .unwrap_or(canonical)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::shell_path;

    #[test]
    fn drops_the_verbatim_prefix() {
        assert_eq!(shell_path(r"\\?\C:\Art\met-1.jpg"), r"C:\Art\met-1.jpg");
        assert_eq!(
            shell_path(r"\\?\UNC\server\share\met-1.jpg"),
            r"\\server\share\met-1.jpg"
        );
        assert_eq!(shell_path(r"C:\Art\met-1.jpg"), r"C:\Art\met-1.jpg");
    }
}
