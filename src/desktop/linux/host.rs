use anyhow::{anyhow, Context, Result};
use glib::variant::ToVariant;
use std::rc::Rc;

pub(crate) const WATCHER: &str = "org.kde.StatusNotifierWatcher";

pub(super) fn appindicator_available() -> bool {
    // SAFETY: probing and immediately dropping a dynamic library runs its ordinary
    // loader initialisation only. No symbol is read and no pointer outlives it.
    unsafe {
        libloading::Library::new("libayatana-appindicator3.so.1")
            .or_else(|_| libloading::Library::new("libappindicator3.so.1"))
            .or_else(|_| libloading::Library::new("libayatana-appindicator3.so"))
            .or_else(|_| libloading::Library::new("libappindicator3.so"))
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

pub(crate) struct TrayHostWatch {
    connection: gio::DBusConnection,
    subscription: Option<gio::SignalSubscriptionId>,
}

impl TrayHostWatch {
    pub(super) fn new(on_changed: impl Fn(bool) + 'static) -> Result<Self> {
        let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
            .context("watching for a desktop tray host")?;
        let on_changed = Rc::new(on_changed);
        let signal_changed = on_changed.clone();
        let subscription = connection.signal_subscribe(
            Some("org.freedesktop.DBus"),
            Some("org.freedesktop.DBus"),
            Some("NameOwnerChanged"),
            Some("/org/freedesktop/DBus"),
            Some(WATCHER),
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, parameters| {
                if let Some((_, _, new_owner)) = parameters.get::<(String, String, String)>() {
                    signal_changed(!new_owner.is_empty());
                }
            },
        );
        match watcher_has_owner() {
            Ok(present) => on_changed(present),
            Err(error) => {
                connection.signal_unsubscribe(subscription);
                return Err(error);
            }
        }
        Ok(Self {
            connection,
            subscription: Some(subscription),
        })
    }
}

impl Drop for TrayHostWatch {
    fn drop(&mut self) {
        if let Some(subscription) = self.subscription.take() {
            self.connection.signal_unsubscribe(subscription);
        }
    }
}
