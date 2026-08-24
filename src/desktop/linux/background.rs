//! GNOME's fit-and-letterbox wallpaper settings.

use anyhow::{anyhow, bail, Context, Result};
use gio::prelude::SettingsExt;
use std::path::Path;

const SCHEMA: &str = "org.gnome.desktop.background";
const PICTURE: &str = "picture-uri";
const PICTURE_DARK: &str = "picture-uri-dark";

pub(super) struct Inspection {
    pub(super) has_dark: bool,
    pub(super) picture_uri: String,
}

/// Reports what GNOME offers without constructing settings against a missing
/// schema, which would make GLib abort rather than return an error.
pub(super) fn inspect() -> Result<Inspection> {
    let (settings, has_dark) = open()?;
    Ok(Inspection {
        has_dark,
        picture_uri: settings.string(PICTURE).to_string(),
    })
}

pub(super) fn pin(path: &Path) -> Result<()> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        bail!("DBUS_SESSION_BUS_ADDRESS is not set; refusing a wallpaper write that may vanish");
    }

    let uri = glib::filename_to_uri(path, None)
        .with_context(|| format!("turning {} into a file URI", path.display()))?;
    let (settings, has_dark) = open()?;

    // Placement is part of putting a picture up, never a setting callers have to
    // remember separately. Set it before clearing the URI so any brief bare frame
    // is already the intended black.
    set(&settings, "picture-options", "scaled")?;
    set(&settings, "primary-color", "#000000")?;
    set(&settings, "color-shading-type", "solid")?;

    // dconf drops same-value writes and emits no service-side notification. A
    // distinct empty value followed by the real URI guarantees Re-apply reaches
    // GNOME Shell instead of succeeding only in this process.
    set(&settings, PICTURE, "")?;
    if has_dark {
        set(&settings, PICTURE_DARK, "")?;
    }
    set(&settings, PICTURE, &uri)?;
    if has_dark {
        set(&settings, PICTURE_DARK, &uri)?;
    }

    gio::Settings::sync();
    let stored = settings.string(PICTURE);
    if stored.as_str() != uri.as_str() {
        bail!(
            "GNOME stored picture-uri as {:?}, expected {:?}",
            stored.as_str(),
            uri.as_str()
        );
    }
    Ok(())
}

fn open() -> Result<(gio::Settings, bool)> {
    let source = gio::SettingsSchemaSource::default()
        .ok_or_else(|| anyhow!("no GSettings schemas are installed"))?;
    let schema = source
        .lookup(SCHEMA, true)
        .ok_or_else(|| anyhow!("this desktop has no {SCHEMA} schema"))?;
    let has_dark = schema.has_key(PICTURE_DARK);
    let settings = gio::Settings::new_full(&schema, None::<&gio::SettingsBackend>, None);
    Ok((settings, has_dark))
}

fn set(settings: &gio::Settings, key: &str, value: &str) -> Result<()> {
    settings
        .set_string(key, value)
        .with_context(|| format!("setting GNOME background key {key}"))
}
