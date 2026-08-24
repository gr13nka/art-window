mod background;
mod host;
mod login;

use anyhow::{bail, Context, Result};
use glib::variant::ToVariant;
use std::collections::HashMap;
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
        Some(path) => match super::pin(path) {
            Ok(()) => println!("wallpaper write    accepted and read back"),
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
