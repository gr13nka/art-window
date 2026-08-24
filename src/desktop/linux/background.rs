use anyhow::{bail, Result};
use std::path::Path;

pub(super) fn pin(_path: &Path) -> Result<()> {
    bail!("the GNOME wallpaper backend is not installed yet")
}
