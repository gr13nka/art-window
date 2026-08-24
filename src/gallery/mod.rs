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

use crate::art::Artwork;
use crate::config::State;
use crate::desktop;
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
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
use linux as platform;

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

/// An action from the Linux control strip. Kept separate from [`Pick`] because
/// looking at a favourite and controlling the resident application are different
/// vocabularies even when GNOME puts them in one window.
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub enum Control {
    Browse,
    Next,
    Keep,
    Today,
    Reapply,
    Login(bool),
    Quit,
}

#[derive(Default)]
struct Snapshot {
    shown: Option<Artwork>,
    today: Option<Artwork>,
    can_keep: bool,
    fetching: bool,
    status: Option<String>,
    starts_at_login: bool,
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
    /// Linux's combined window sends application controls by a separate route.
    on_control: Rc<dyn Fn(Control)>,
    snapshot: Snapshot,
    open: Option<Open>,
}

/// The window and what is inside it, which exist only together.
struct Open {
    window: Window,
    content: platform::Content,
}

#[cfg(target_os = "macos")]
const TITLE: &str = "Favourites";
#[cfg(target_os = "linux")]
const TITLE: &str = "Art Window";
const OPENS_AT: LogicalSize<f64> = LogicalSize::new(940.0, 640.0);
const NO_SMALLER_THAN: LogicalSize<f64> = LogicalSize::new(560.0, 400.0);

impl Gallery {
    pub fn new(on_pick: impl Fn(Pick) + 'static, on_control: impl Fn(Control) + 'static) -> Self {
        Self {
            on_pick: Rc::new(on_pick),
            on_control: Rc::new(on_control),
            snapshot: Snapshot {
                starts_at_login: desktop::starts_at_login(),
                ..Snapshot::default()
            },
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
            open.content.describe(&self.snapshot, favourites);
            platform::present(&open.window);
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
        let content =
            platform::Content::install(&window, self.on_pick.clone(), self.on_control.clone())?;
        content.describe(&self.snapshot, favourites);
        platform::present(&window);
        self.open = Some(Open { window, content });
        Ok(())
    }

    /// Updates everything shared by the tray and combined Linux window.
    pub fn describe(&mut self, state: &State, favourites: &Favourites) {
        self.snapshot.shown = state.shown.clone();
        self.snapshot.today = state
            .fetched
            .as_ref()
            .filter(|art| Some(&art.path) != state.shown.as_ref().map(|shown| &shown.path))
            .filter(|art| art.path.exists())
            .cloned();
        self.snapshot.can_keep = state
            .shown
            .as_ref()
            .is_some_and(|art| !favourites.holds(art));
        self.snapshot.status = None;
        self.snapshot.starts_at_login = desktop::starts_at_login();
        if let Some(open) = &self.open {
            open.content.describe(&self.snapshot, favourites);
        }
    }

    pub fn set_status(&mut self, text: &str) {
        self.snapshot.status = Some(text.to_string());
        if let Some(open) = &self.open {
            open.content.describe_status(&self.snapshot);
        }
    }

    pub fn set_fetching(&mut self, fetching: bool) {
        self.snapshot.fetching = fetching;
        if fetching {
            self.snapshot.status = Some("Fetching…".to_string());
        }
        if let Some(open) = &self.open {
            open.content.describe_status(&self.snapshot);
        }
    }

    pub fn set_login(&mut self, enabled: bool) {
        self.snapshot.starts_at_login = enabled;
        if let Some(open) = &self.open {
            open.content.set_login(enabled);
        }
    }

    /// Whether `id` names this window, which is how the loop knows a window event
    /// is about this one and not some other window the program has yet to grow.
    pub fn owns(&self, id: WindowId) -> bool {
        self.open
            .as_ref()
            .is_some_and(|open| open.window.id() == id)
    }

    /// Answers a close request. macOS dismisses its optional gallery; Linux keeps
    /// its only application window mapped and minimises it into GNOME's Dash.
    pub fn close(&mut self) {
        if let Some(open) = &self.open {
            if platform::close(&open.window) {
                return;
            }
        }
        self.open = None;
    }

    #[cfg(target_os = "linux")]
    pub fn minimize(&self) {
        if let Some(open) = &self.open {
            open.window.set_minimized(true);
        }
    }

    /// Takes the window down.
    ///
    /// Dropping it is what shuts it, exactly as dropping the `TrayIcon` is what
    /// clears the menu bar — so this is the whole of it, and Quit has to say it.
    pub fn dismiss(&mut self) {
        self.open = None;
    }
}
