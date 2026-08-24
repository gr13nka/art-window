//! Being told the machine has come back from sleep.
//!
//! The scheduler's rule is that wall-clock time decides whether a picture is owed.
//! Keeping that rule needs two things and the event loop only had one of them: it
//! asks the right question, but it can only ask while it is running, and the timer
//! that wakes it is monotonic — it does not advance while the lid is shut. A laptop
//! asleep through midnight is the ordinary case, so without this the first painting
//! of a new day waits for whatever happens to poke the loop next.
//!
//! Small enough to be one file rather than the folder `wallpaper` needs, and the
//! Its implementation is another platform seam: a real macOS body, and elsewhere
//! an honest nothing until that desktop grows one.

/// Calls `on_wake` on the main thread each time the machine wakes from sleep.
///
/// The returned [`Watch`] owns the subscription. Hold it for as long as the calls
/// are wanted; dropping it stops them.
pub fn watch(on_wake: impl Fn() + 'static) -> anyhow::Result<Watch> {
    Watch::new(on_wake)
}

#[cfg(target_os = "macos")]
pub use platform::Watch;

#[cfg(target_os = "macos")]
mod platform {
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::{NSNotification, NSNotificationCenter, NSOperationQueue};
    use std::ptr::NonNull;

    /// A live subscription to `NSWorkspaceDidWakeNotification`.
    ///
    /// Sleep and wake are the workspace's business rather than the default centre's,
    /// which is why this goes through `NSWorkspace` and not `NSNotificationCenter`
    /// directly — the default centre never sees these.
    pub struct Watch {
        centre: Retained<NSNotificationCenter>,
        token: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    }

    impl Watch {
        pub(super) fn new(on_wake: impl Fn() + 'static) -> anyhow::Result<Self> {
            // The notification itself says nothing worth reading: that it arrived at
            // all is the entire message.
            let block = RcBlock::new(move |_: NonNull<NSNotification>| on_wake());
            let centre = NSWorkspace::sharedWorkspace().notificationCenter();
            // The main queue, because what this wakes goes on to touch AppKit and
            // the state the event loop owns.
            let queue = NSOperationQueue::mainQueue();

            // SAFETY: the name is AppKit's own static, the block outlives the
            // subscription by living in the token, and the queue is the main one,
            // which is where the block's only side effect belongs.
            let token = unsafe {
                centre.addObserverForName_object_queue_usingBlock(
                    Some(NSWorkspaceDidWakeNotification),
                    None,
                    Some(&queue),
                    &block,
                )
            };

            Ok(Self { centre, token })
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
