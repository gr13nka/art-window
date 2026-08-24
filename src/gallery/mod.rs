//! A window in which the kept pictures can be looked at, and not merely named.
//!
//! The menu bar can list favourites but it cannot show them: a row of forty-four
//! characters of Met catalogue prose is no way to recognise a painting you liked.
//! So there is a window — a column of thumbnails to pick from, and whichever is
//! picked shown large enough to judge.
//!
//! What is in the window, how it is laid out and how a thumbnail is made are the
//! platform half's business. This half owns the window itself and the vocabulary a
//! click comes back in, which is deliberately only two words wide: everything else
//! a person does in there — scrolling, selecting, looking — changes nothing outside
//! the window and so never leaves it.

use crate::favourites::Favourites;
use anyhow::{anyhow, Result};
use std::rc::Rc;
use tao::dpi::LogicalSize;
use tao::event_loop::EventLoopWindowTarget;
use tao::window::{Window, WindowBuilder, WindowId};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
use macos as platform;

/// What somebody asked of a picture in the window.
///
/// Both name a picture by its [`Favourites`] key rather than by where it sat, for
/// the same reason the menu ids did: the list can be rebuilt between the click and
/// the answer, and a position would by then mean a different painting.
pub enum Pick {
    /// Hang this one on the desktop.
    Show(String),
    /// Drop this one from the list.
    Forget(String),
}

/// The window of kept pictures — shut most of the time, and then not there at all.
///
/// Left standing once opened rather than rebuilt on every visit, because the
/// thumbnails inside it are slow to make and worth having ready for the second
/// look. Shutting it really does throw them away; that is what the memory going
/// back is bought with.
pub struct Gallery {
    /// Called when a picture is picked. It runs on AppKit's thread, in the middle
    /// of a click, so it says what happened rather than doing anything about it —
    /// the same rule, and the same reason, as [`crate::wake::watch`].
    on_pick: Rc<dyn Fn(Pick)>,
    open: Option<Open>,
}

/// The window and what is inside it, which exist only together.
struct Open {
    window: Window,
    content: platform::Content,
}

const TITLE: &str = "Favourites";
const OPENS_AT: LogicalSize<f64> = LogicalSize::new(940.0, 640.0);
const NO_SMALLER_THAN: LogicalSize<f64> = LogicalSize::new(560.0, 400.0);

impl Gallery {
    pub fn new(on_pick: impl Fn(Pick) + 'static) -> Self {
        Self {
            on_pick: Rc::new(on_pick),
            open: None,
        }
    }

    /// Opens the window on `favourites`, or brings it forward if it is already up.
    ///
    /// Bringing it forward is [`Window::set_focus`] and not merely raising it: this
    /// program is an `Accessory` — no Dock tile, never the active application — so
    /// a window it opens is behind whatever the person was actually doing until it
    /// asks to be in front.
    pub fn present<T>(
        &mut self,
        target: &EventLoopWindowTarget<T>,
        favourites: &Favourites,
    ) -> Result<()> {
        if let Some(open) = &self.open {
            open.content.relist(favourites);
            open.window.set_visible(true);
            open.window.set_focus();
            return Ok(());
        }

        // Built hidden and shown once it has something in it, so that nobody
        // watches an empty window fill up a thumbnail at a time.
        let window = WindowBuilder::new()
            .with_title(TITLE)
            .with_inner_size(OPENS_AT)
            .with_min_inner_size(NO_SMALLER_THAN)
            .with_visible(false)
            .build(target)
            .map_err(|e| anyhow!("opening the favourites window: {e}"))?;
        let content = platform::Content::install(&window, self.on_pick.clone())?;
        content.relist(favourites);
        window.set_visible(true);
        window.set_focus();
        self.open = Some(Open { window, content });
        Ok(())
    }

    /// Shows a changed list in an open window, and does nothing when it is shut.
    ///
    /// Called wherever the menu is pointed at the truth again, because the two are
    /// views of the same list and there is no moment at which one of them may be
    /// wrong.
    pub fn relist(&mut self, favourites: &Favourites) {
        if let Some(open) = &self.open {
            open.content.relist(favourites);
        }
    }

    /// Whether `id` names this window, which is how the loop knows a window event
    /// is about this one and not some other window the program has yet to grow.
    pub fn owns(&self, id: WindowId) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.window.id() == id)
    }

    /// Takes the window down.
    ///
    /// Dropping it is what shuts it, exactly as dropping the `TrayIcon` is what
    /// clears the menu bar — so this is the whole of it, and Quit has to say it.
    pub fn dismiss(&mut self) {
        self.open = None;
    }
}
