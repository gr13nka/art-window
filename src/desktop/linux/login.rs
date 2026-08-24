use anyhow::{bail, Result};

pub(super) fn is_enabled() -> bool {
    false
}

pub(super) fn set(_enabled: bool) -> Result<()> {
    bail!("the Linux login backend is not installed yet")
}
