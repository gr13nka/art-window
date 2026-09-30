//! Being told the machine has come back from sleep.
//!
//! The scheduler's rule is that wall-clock time decides whether a picture is owed.
//! Keeping that rule needs two things and the event loop only had one of them: it
//! asks the right question, but it can only ask while it is running, and the timer
//! that wakes it is monotonic — it does not advance while the lid is shut. A laptop
//! asleep through midnight is the ordinary case, so without this the first painting
//! of a new day waits for whatever happens to poke the loop next.
//!
//! Small enough to keep every platform body in this file: NSWorkspace on macOS,
//! logind on Linux and the power manager's suspend-resume callback on Windows.
//!
//! The displays changing is the same kind of news and lives here too. On macOS
//! unplugging a monitor moves its Spaces onto the screens that remain, and the Dock
//! greets them with its own default picture — so something has to say it happened
//! for the loop to put the painting back. See [`displays`].

/// Calls `on_wake` each time the machine wakes from sleep.
///
/// Which thread it is called on is the platform's choice — the main one on macOS
/// and Linux, one of the system's on Windows — so `on_wake` must be `Send` and
/// should do no more than say that it happened.
///
/// The returned [`Watch`] owns the subscription. Hold it for as long as the calls
/// are wanted; dropping it stops them.
pub fn watch(on_wake: impl Fn() + Send + Sync + 'static) -> anyhow::Result<Watch> {
    Watch::new(on_wake)
}

/// Calls `on_change` each time a display is attached, detached or rearranged.
///
/// Only macOS is watched. GNOME keeps one wallpaper URI whatever is plugged in,
/// and Windows has not been seen to lose its wallpaper this way, so elsewhere this
/// answers `None` and nothing is ever called.
#[cfg(target_os = "macos")]
pub fn displays(on_change: impl Fn() + Send + Sync + 'static) -> anyhow::Result<Option<Watch>> {
    Watch::displays(on_change).map(Some)
}

#[cfg(not(target_os = "macos"))]
pub fn displays(_on_change: impl Fn() + Send + Sync + 'static) -> anyhow::Result<Option<Watch>> {
    Ok(None)
}

#[cfg(target_os = "macos")]
pub use platform::Watch;

#[cfg(target_os = "macos")]
mod platform {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
    use objc2_app_kit::{
        NSApplicationDidChangeScreenParametersNotification, NSWorkspace,
        NSWorkspaceDidWakeNotification,
    };
    use objc2_foundation::{
        NSNotification, NSNotificationCenter, NSNotificationName, NSOperationQueue,
    };
    use std::ptr::NonNull;

    /// A live subscription to one notification.
    pub struct Watch {
        centre: Retained<NSNotificationCenter>,
        token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    }

    impl Watch {
        /// `NSWorkspaceDidWakeNotification`. Sleep and wake are the workspace's
        /// business rather than the default centre's, which is why this goes
        /// through `NSWorkspace` — the default centre never sees these.
        pub(super) fn new(on_wake: impl Fn() + 'static) -> anyhow::Result<Self> {
            let centre = NSWorkspace::sharedWorkspace().notificationCenter();
            // SAFETY: AppKit's own static.
            Ok(Self::observe(
                centre,
                unsafe { NSWorkspaceDidWakeNotification },
                on_wake,
            ))
        }

        /// `NSApplicationDidChangeScreenParametersNotification`. The reverse of
        /// waking: `NSApplication` posts this to the *default* centre, and the
        /// workspace's never sees it.
        pub(super) fn displays(on_change: impl Fn() + 'static) -> anyhow::Result<Self> {
            let centre = NSNotificationCenter::defaultCenter();
            // SAFETY: AppKit's own static.
            Ok(Self::observe(
                centre,
                unsafe { NSApplicationDidChangeScreenParametersNotification },
                on_change,
            ))
        }

        fn observe(
            centre: Retained<NSNotificationCenter>,
            name: &NSNotificationName,
            then: impl Fn() + 'static,
        ) -> Self {
            // The notification itself says nothing worth reading: that it arrived at
            // all is the entire message.
            let block = RcBlock::new(move |_: NonNull<NSNotification>| then());
            // The main queue, because what this wakes goes on to touch AppKit and
            // the state the event loop owns.
            let queue = NSOperationQueue::mainQueue();

            // SAFETY: the name is one of AppKit's statics, the block outlives the
            // subscription by living in the token, and the queue is the main one,
            // which is where the block's only side effect belongs.
            let token = unsafe {
                centre.addObserverForName_object_queue_usingBlock(
                    Some(name),
                    None,
                    Some(&queue),
                    &block,
                )
            };

            Self { centre, token }
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            // Named through `AsRef` rather than by method call: `Retained` has an
            // `as_ref` of its own that stops one deref short of what is wanted here.
            let observer: &AnyObject = AsRef::as_ref(&*self.token);
            // SAFETY: the token came from this centre and is removed exactly once.
            unsafe { self.centre.removeObserver(observer) };
        }
    }
}

#[cfg(target_os = "linux")]
pub use platform::Watch;

#[cfg(target_os = "linux")]
mod platform {
    use anyhow::{Context, Result};

    /// A live subscription to logind's system-bus sleep transition.
    pub struct Watch {
        connection: gio::DBusConnection,
        subscription: Option<gio::SignalSubscriptionId>,
    }

    impl Watch {
        pub(super) fn new(on_wake: impl Fn() + 'static) -> Result<Self> {
            let connection = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE)
                .context("watching logind for wake notifications")?;
            let subscription = connection.signal_subscribe(
                Some("org.freedesktop.login1"),
                Some("org.freedesktop.login1.Manager"),
                Some("PrepareForSleep"),
                Some("/org/freedesktop/login1"),
                None,
                gio::DBusSignalFlags::NONE,
                move |_, _, _, _, _, parameters| {
                    if parameters.get::<(bool,)>() == Some((false,)) {
                        on_wake();
                    }
                },
            );
            Ok(Self {
                connection,
                subscription: Some(subscription),
            })
        }
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            if let Some(subscription) = self.subscription.take() {
                self.connection.signal_unsubscribe(subscription);
            }
        }
    }
}

#[cfg(windows)]
pub use platform::Watch;

#[cfg(windows)]
mod platform {
    use anyhow::{bail, Result};
    use core::ffi::c_void;
    use windows::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
    use windows::Win32::System::Power::{
        PowerRegisterSuspendResumeNotification, PowerUnregisterSuspendResumeNotification,
        DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS, HPOWERNOTIFY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DEVICE_NOTIFY_CALLBACK, PBT_APMRESUMEAUTOMATIC};

    type OnWake = Box<dyn Fn() + Send + Sync>;

    /// A live subscription to the power manager's suspend-resume callback.
    pub struct Watch {
        registration: HPOWERNOTIFY,
        /// Double-boxed so the pointer handed to the system is thin. Freed only after
        /// the registration is gone, so no callback can find it dangling.
        on_wake: *mut OnWake,
        /// What the registration was handed. The documentation does not promise the
        /// system copies it, so it lives exactly as long as the registration does.
        _recipient: Box<DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS>,
    }

    // SAFETY: the registration is an opaque token, and `on_wake` is owned here alone
    // and only ever called through a `Send + Sync` closure.
    unsafe impl Send for Watch {}

    impl Watch {
        pub(super) fn new(on_wake: impl Fn() + Send + Sync + 'static) -> Result<Self> {
            let on_wake = Box::into_raw(Box::new(Box::new(on_wake) as OnWake));
            let recipient = Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
                Callback: Some(resumed),
                Context: on_wake.cast(),
            });
            let mut registration = std::ptr::null_mut();
            // SAFETY: `recipient` and `on_wake` both stay allocated until `Drop` has
            // unregistered.
            let status = unsafe {
                PowerRegisterSuspendResumeNotification(
                    DEVICE_NOTIFY_CALLBACK,
                    HANDLE(std::ptr::from_ref(&*recipient).cast_mut().cast()),
                    &mut registration,
                )
            };
            if status != ERROR_SUCCESS {
                // SAFETY: nothing registered, so nothing else holds the pointer.
                drop(unsafe { Box::from_raw(on_wake) });
                bail!("subscribing to power notifications failed: {status:?}");
            }
            Ok(Self {
                registration: HPOWERNOTIFY(registration as isize),
                on_wake,
                _recipient: recipient,
            })
        }
    }

    /// Only `PBT_APMRESUMEAUTOMATIC` is listened for: it is sent on every wake,
    /// whereas the suspend-resume variant arrives only when a person caused it.
    unsafe extern "system" fn resumed(
        context: *const c_void,
        kind: u32,
        _setting: *const c_void,
    ) -> u32 {
        if kind == PBT_APMRESUMEAUTOMATIC {
            // SAFETY: `context` is the pointer `new` registered, alive until unregistered.
            let on_wake = unsafe { &*context.cast::<OnWake>() };
            on_wake();
        }
        0
    }

    impl Drop for Watch {
        fn drop(&mut self) {
            // SAFETY: the registration came from `new` and is removed exactly once;
            // the closure is freed only after, so no callback can still be using it.
            unsafe {
                let _ = PowerUnregisterSuspendResumeNotification(self.registration);
                drop(Box::from_raw(self.on_wake));
            }
        }
    }
}
