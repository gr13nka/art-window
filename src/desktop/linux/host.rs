use anyhow::{anyhow, Context, Result};
use glib::variant::ToVariant;

pub(crate) const WATCHER: &str = "org.kde.StatusNotifierWatcher";

pub(super) fn appindicator_available() -> bool {
    // SAFETY: probing and immediately dropping a dynamic library runs its ordinary
    // loader initialisation only. No symbol is read and no pointer outlives it.
    unsafe {
        libloading::Library::new("libayatana-appindicator3.so.1")
            .or_else(|_| libloading::Library::new("libappindicator3.so.1"))
            .is_ok()
    }
}

pub(super) fn watcher_has_owner() -> Result<bool> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .context("connecting to the session bus")?;
    let parameters = (WATCHER,).to_variant();
    let reply = connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            3000,
            gio::Cancellable::NONE,
        )
        .context("asking whether a StatusNotifierWatcher is running")?;
    reply
        .get::<(bool,)>()
        .map(|answer| answer.0)
        .ok_or_else(|| anyhow!("the session bus returned an invalid NameHasOwner reply"))
}
