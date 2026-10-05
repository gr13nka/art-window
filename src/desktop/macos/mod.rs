mod login;
mod wallpaper;

use super::Pinned;
use crate::placement::Hang;
use anyhow::Result;

pub(super) fn pin(hang: &Hang) -> Result<Pinned> {
    wallpaper::pin(hang)
}

pub(super) fn primary_screen() -> Option<(u32, u32)> {
    wallpaper::primary_screen()
}

pub(super) fn catch_up(reason: &str) {
    wallpaper::catch_up(reason);
}

pub(super) fn browse(url: &str) {
    if let Err(e) = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .status()
    {
        crate::journal::note!("desktop", "could not open {url}: {e}");
    }
}

pub(super) fn starts_at_login() -> bool {
    login::is_enabled()
}

pub(super) fn set_start_at_login(enabled: bool) -> Result<()> {
    login::set(enabled)
}
