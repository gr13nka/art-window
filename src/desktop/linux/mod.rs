mod background;
mod login;

use anyhow::Result;
use std::path::Path;

pub(super) fn pin(path: &Path) -> Result<()> {
    background::pin(path)
}

pub(super) fn browse(url: &str) {
    let _ = std::process::Command::new("xdg-open").arg(url).status();
}

pub(super) fn starts_at_login() -> bool {
    login::is_enabled()
}

pub(super) fn set_start_at_login(enabled: bool) -> Result<()> {
    login::set(enabled)
}
