mod background;
mod host;
mod login;

use super::Pinned;
use crate::placement::{Hang, Mode};
use anyhow::{bail, Context, Result};
use gdk::prelude::MonitorExt;
use glib::variant::ToVariant;
use std::collections::HashMap;
use std::path::Path;

/// GNOME keeps one wallpaper and this backend reads it back before returning, so
/// there is no half-measure to report: it either took or it errored.
pub(super) fn pin(hang: &Hang) -> Result<Pinned> {
    match background::pin(hang) {
        Ok(()) => {
            crate::journal::note!("pin", "path {}, took and read back", hang.path.display());
            Ok(Pinned::Everywhere)
        }
        Err(error) => {
            crate::journal::fault("pin", &error);
            Err(error)
        }
    }
}

/// The primary monitor's size in device pixels, or `None` with no display.
pub(super) fn primary_screen() -> Option<(u32, u32)> {
    let display = gdk::Display::default()?;
    let monitor = display.primary_monitor().or_else(|| display.monitor(0))?;
    let area = monitor.geometry();
    let scale = monitor.scale_factor().max(1);
    Some((
        (area.width() * scale) as u32,
        (area.height() * scale) as u32,
    ))
}

/// Nothing to publish: GNOME Shell watches the settings this backend writes, so a
/// picture is visible everywhere the moment `pin` returns.
pub(super) fn catch_up(_reason: &str) {}

pub(super) fn browse(url: &str) {
    if let Err(e) = std::process::Command::new("xdg-open").arg(url).status() {
        crate::journal::note!("desktop", "could not open {url}: {e}");
    }
}

pub(super) fn starts_at_login() -> bool {
    login::is_enabled()
}

pub(super) fn set_start_at_login(enabled: bool) -> Result<()> {
    login::set(enabled)
}

pub(super) fn check(shown: Option<&Path>) -> Result<()> {
    let mut failures = Vec::new();

    let has_bus = std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some();
    println!(
        "session bus       {}",
        if has_bus { "available" } else { "missing" }
    );
    if !has_bus {
        failures.push("DBUS_SESSION_BUS_ADDRESS is not set".to_string());
    }

    match background::inspect() {
        Ok(found) => {
            println!("background schema  present");
            println!(
                "dark wallpaper key {}",
                if found.has_dark { "present" } else { "absent" }
            );
            println!("current picture    {}", found.picture_uri);
        }
        Err(error) => {
            println!("background schema  unavailable ({error:#})");
            failures.push(error.to_string());
        }
    }

    match host::watcher_has_owner() {
        Ok(true) => println!("tray host          running"),
        Ok(false) => println!("tray host          absent (window fallback will be used)"),
        Err(error) => println!("tray host          unknown ({error:#})"),
    }
    println!(
        "appindicator      {}",
        if host::appindicator_available() {
            "available"
        } else {
            "missing (window fallback will be used)"
        }
    );

    match shown {
        None => println!("wallpaper write    skipped (no shown artwork is recorded)"),
        Some(path) => match pin(&Hang {
            path: path.to_path_buf(),
            mode: Mode::Fit,
            colour: [0, 0, 0],
        }) {
            Ok(_) => println!("wallpaper write    accepted and read back"),
            Err(error) => {
                println!("wallpaper write    failed ({error:#})");
                failures.push(error.to_string());
            }
        },
    }

    if failures.is_empty() {
        Ok(())
    } else {
        bail!("GNOME check failed: {}", failures.join("; "))
    }
}

pub(crate) use host::TrayHostWatch;

pub(super) fn appindicator_available() -> bool {
    host::appindicator_available()
}

pub(super) fn watch_tray_host(on_changed: impl Fn(bool) + 'static) -> Result<TrayHostWatch> {
    TrayHostWatch::new(on_changed)
}

pub(super) fn quit_running() -> Result<()> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("connecting to the running Art Window instance")?;
    let parameters = (
        super::QUIT_ACTION,
        Vec::<glib::Variant>::new(),
        HashMap::<String, glib::Variant>::new(),
    )
        .to_variant();
    connection
        .call_sync(
            Some(super::APP_ID),
            "/dev/artwindow",
            "org.freedesktop.Application",
            "ActivateAction",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            3000,
            gio::Cancellable::NONE,
        )
        .context("asking the running Art Window instance to quit")?;
    Ok(())
}
