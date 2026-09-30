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

pub(super) fn catch_up() {
    wallpaper::catch_up();
}

pub(super) fn browse(url: &str) {
    let _ = std::process::Command::new("/usr/bin/open")
        .arg(url)
        .status();
}

pub(super) fn starts_at_login() -> bool {
    login::is_enabled()
}

pub(super) fn set_start_at_login(enabled: bool) -> Result<()> {
    login::set(enabled)
}
