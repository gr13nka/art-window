mod login;
mod wallpaper;

use super::Pinned;
use anyhow::Result;
use std::path::Path;

pub(super) fn pin(path: &Path) -> Result<Pinned> {
    wallpaper::pin(path)
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
