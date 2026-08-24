//! The menu bar presence: an icon, a short menu, and a clock.
//!
//! The clock is the reason this exists as a resident program at all. It replaces a
//! launchd job that woke a one-shot command every hour, and it keeps that job's one
//! good idea: *wall-clock time decides whether a picture is owed*, never a countdown.
//! A countdown cannot survive a closed lid, and a machine that sleeps through the
//! moment a timer was set for is the normal case, not the edge one.
//!
//! Which is why every deadline here is a wall-clock instant and `TICK` is the one
//! remaining countdown — it schedules a *question*, not an answer, and being late
//! with it costs nothing. What being awake to ask at all costs is [`wake`]: a
//! monotonic timer stops while the lid is shut, so without something to say the
//! machine is back, the first painting of a new day would wait for whatever poked
//! the loop next.
//!
//! Three threads' worth of constraints meet here and only two threads exist:
//!
//! - AppKit will build a status item, and set a wallpaper, on the main thread only.
//! - A museum download blocks for up to two minutes, which on the main thread is a
//!   frozen menu bar.
//!
//! So the fetch goes to a throwaway worker and comes back as an [`Artwork`] through
//! the event loop's own queue, where the main thread hangs it. Nothing is shared
//! between the two; the worker gets copies and returns a value.

use crate::art::Artwork;
use crate::autostart;
use crate::config::{now_secs, Config, Paths, State};
use crate::favourites::Favourites;
use crate::gallery::{Gallery, Pick};
use crate::rotation;
use crate::wake;
use crate::wallpaper;
use anyhow::{anyhow, Result};
use std::path::Path;
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::WindowId;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

/// How often to look at the clock while the machine is plainly awake. The waking
/// itself is announced — see [`wake`] — so this is only the backstop for a day that
/// turns over with nobody asleep and nothing else happening. A day is a day, so five
/// minutes is already far finer than anyone can notice, and rare enough to leave an
/// idle laptop alone.
const TICK: Duration = Duration::from_secs(5 * 60);

/// How long to leave a failed attempt alone. Without this a museum that is down, or
/// a wallpaper that will not take, would be retried the instant it failed and then
/// again forever: the day is only marked done on success, so nothing else would stop
/// the loop.
const RETRY: Duration = Duration::from_secs(15 * 60);

/// Something that needs the main thread's attention.
enum Wake {
    /// A menu item was clicked. Forwarded rather than acted on where it arrives,
    /// because that is one of AppKit's own callbacks and no place to do work.
    Menu(MenuEvent),
    /// A picture in the favourites window was picked. Forwarded for the same
    /// reason as a menu click, and by the same route: it arrives mid-click on
    /// AppKit's thread, which owns nothing this loop does.
    Picked(Pick),
    /// The worker finished, for better or worse.
    Fetched(Result<Artwork>),
    /// The machine came back from sleep. Carries nothing, and its arm does nothing:
    /// the clock at the tail of the loop is re-read after every event, which is the
    /// whole reason it lives there rather than in an arm of its own.
    Woke,
}

/// What an event asks of the event loop.
///
/// The menu can open a browser and tick a box by itself, but it owns neither the
/// state nor the settings, and hanging a picture needs both. Rather than borrow
/// them, it says what it wants and the loop does it. The window says the same two
/// things in its own words — see [`Pick`] — so that neither surface has to know
/// the other's vocabulary and neither is answered twice over.
enum Wanted {
    Nothing,
    /// Go back to the source for a different picture, now rather than tomorrow.
    Next,
    /// Keep the picture on the desktop.
    Keep,
    /// Put a kept picture up in place of whatever the clock had in mind.
    Show(String),
    /// Put the day's own picture back, after one of the above.
    Today,
    /// Drop a kept picture from the list.
    Forget(String),
    /// Open the window where the kept pictures can be looked at.
    Gallery,
}

impl From<Pick> for Wanted {
    fn from(pick: Pick) -> Self {
        match pick {
            Pick::Show(key) => Self::Show(key),
            Pick::Forget(key) => Self::Forget(key),
        }
    }
}

/// Runs until the user picks Quit.
pub fn run(paths: Paths, config: Config, mut state: State) -> Result<()> {
    let mut event_loop = EventLoopBuilder::<Wake>::with_user_event().build();

    // No Dock icon, no application menu: this program lives in the menu bar. The
    // bundle's `LSUIElement` says the same thing earlier and without the momentary
    // bounce, but this is what makes an unbundled `cargo run` behave.
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let proxy = event_loop.create_proxy();
    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(Wake::Menu(event));
    }));

    // Both callbacks do the same thing and for the same reason: they arrive on
    // somebody else's terms — AppKit's here, the workspace's below — and neither is
    // a place to touch the state the loop owns. They say what happened; the loop
    // decides what it means.
    //
    // Never dropped, because `run` below never returns. Dropping it would silence
    // the notifications, which is what the binding is holding them open against.
    let wake_proxy = proxy.clone();
    let _woken = wake::watch(move || {
        let _ = wake_proxy.send_event(Wake::Woke);
    });

    let mut favourites = Favourites::open(&paths.favourites)?;

    // The window answers a click the same way the menu does: by saying what
    // happened and letting the loop decide what it means.
    let pick_proxy = proxy.clone();
    let mut ui = Ui::new(move |pick| {
        let _ = pick_proxy.send_event(Wake::Picked(pick));
    })?;
    ui.describe(&state, &favourites);

    let mut tray: Option<TrayIcon> = None;
    let mut fetching = false;
    // Unix seconds until which a failed attempt is cooling off; cleared by success.
    // Wall clock rather than an `Instant` for the reason in the module comment: a
    // fifteen-minute countdown started before the lid closed still owes fifteen
    // minutes of *waking* time the next morning, which is the very complaint the
    // schedule exists to answer.
    let mut hold_until: Option<u64> = None;
    // Set when a kept picture goes up while a download is still in the air, so that
    // the download does not land on top of a choice just made.
    let mut superseded = false;
    // Set when the Next picture row is clicked. A fetch is started at the tail of
    // the loop and nowhere else, so a click asks for one rather than doing it: the
    // clock is then wound the same way whoever did the asking.
    let mut asked_for_next = false;

    event_loop.run(move |event, target, control_flow| {
        // Every event is first read for what it asks of the loop, and only then
        // acted on. The two halves are separate because the same two things can be
        // asked from two places — a menu row and a picture in the window — and
        // answering them twice over is how the two would drift apart.
        let wanted = match event {
            // The status item is built here and not before `run`, because tray-icon
            // wants a run loop that is already turning — otherwise it goes missing
            // in front of full-screen apps.
            Event::NewEvents(StartCause::Init) => {
                match build_tray(&ui.menu) {
                    Ok(built) => tray = Some(built),
                    Err(e) => {
                        // Without an icon there is no way to quit and no way to know
                        // why nothing appeared, so this one is fatal.
                        report(&e);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
                wake_run_loop();
                Wanted::Nothing
            }

            // The tick. Someone may have changed the login setting in System
            // Settings since the last one.
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                ui.login.set_checked(autostart::is_enabled());
                Wanted::Nothing
            }

            Event::UserEvent(Wake::Menu(click)) => {
                if click.id == ui.quit.id() {
                    // Dropping the icon is what takes it out of the menu bar, and
                    // dropping the window is what shuts it.
                    tray.take();
                    ui.dismiss();
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                ui.handle(&click, &state)
            }

            // The window asks for the same two things the submenu used to, in its
            // own two words.
            Event::UserEvent(Wake::Picked(pick)) => pick.into(),

            // Shutting the window is not quitting: the program lives in the menu
            // bar and carries on there. Deliberately not returned from, so the
            // clock below is still wound.
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                if ui.owns_window(window_id) {
                    ui.dismiss();
                }
                Wanted::Nothing
            }

            Event::UserEvent(Wake::Fetched(result)) => {
                fetching = false;
                ui.set_fetching(false);
                // Dropped on the floor when a kept picture went up while this was
                // in the air: hanging it now would undo a choice just made. The
                // file stays where it is, for the next rotation to overwrite or
                // sweep away. Skipped rather than returned from, because the clock
                // below still has to be wound.
                if !std::mem::take(&mut superseded) {
                    match result.and_then(|artwork| {
                        rotation::show(&artwork, &config, &paths, &mut state).map(|()| artwork)
                    }) {
                        Ok(artwork) => {
                            hold_until = None;
                            // Where a favourite dropped while it was on the desktop
                            // finally goes.
                            favourites.discard_all_but(&artwork.path);
                            ui.describe(&state, &favourites);
                        }
                        Err(e) => {
                            report(&e);
                            hold_until = Some(now_secs() + RETRY.as_secs());
                            ui.set_status("Last attempt failed — will retry");
                        }
                    }
                }
                Wanted::Nothing
            }

            // Nothing to do but arrive: the clock below is what this is for.
            Event::UserEvent(Wake::Woke) => Wanted::Nothing,

            _ => Wanted::Nothing,
        };

        // Both ways of picking a picture by hand end in the same place, so the arms
        // only say which picture and the work happens once, below.
        let mut chosen: Option<Artwork> = None;
        match wanted {
            Wanted::Nothing => {}

            Wanted::Next => asked_for_next = true,

            Wanted::Gallery => {
                if let Err(e) = ui.present(target, &favourites) {
                    report(&e);
                    ui.set_status("Could not open the favourites window");
                }
            }

            Wanted::Keep => {
                if let Some(art) = &state.shown {
                    match favourites.keep(art) {
                        Ok(()) => ui.describe(&state, &favourites),
                        Err(e) => {
                            report(&e);
                            ui.set_status("Could not keep that picture");
                        }
                    }
                }
            }

            // Cloned out of their owners because hanging one needs `state` mutably,
            // and it is about to become the picture `state` remembers.
            Wanted::Show(key) => chosen = favourites.get(&key).cloned(),
            Wanted::Today => chosen = state.fetched.clone(),

            Wanted::Forget(key) => match favourites.forget(&key) {
                Ok(()) => {
                    // An empty path names no file, which is the right thing to
                    // spare when nothing is on the desktop.
                    let on_desktop = state
                        .shown
                        .as_ref()
                        .map_or(Path::new(""), |art| art.path.as_path());
                    favourites.discard_all_but(on_desktop);
                    ui.describe(&state, &favourites);
                }
                Err(e) => {
                    report(&e);
                    ui.set_status("Could not drop that favourite");
                }
            },
        }

        if let Some(art) = chosen {
            match rotation::revisit(&art, &config, &paths, &mut state) {
                Ok(()) => {
                    // Nothing is owed that this has not just answered, and a
                    // download already in the air would only undo it.
                    hold_until = None;
                    superseded = fetching;
                    favourites.discard_all_but(&art.path);
                    ui.describe(&state, &favourites);
                }
                Err(e) => {
                    report(&e);
                    ui.set_status("Could not put that picture up");
                }
            }
        }

        // When to wake up next. Recomputed after every event rather than scheduled
        // once, so that a click, a finished download and a tick all leave the clock
        // in the same, correct place.
        let cooling_off = hold_until
            .and_then(|until| until.checked_sub(now_secs()))
            .filter(|left| *left > 0);
        *control_flow = if fetching {
            // The worker will wake us.
            ControlFlow::Wait
        } else if std::mem::take(&mut asked_for_next) || (cooling_off.is_none() && state.is_due()) {
            // Being asked jumps both queues. Somebody looking at a picture they do
            // not like is not waiting out a museum's bad afternoon, and is plainly
            // not waiting for tomorrow; the request is taken either way, so that one
            // made while a download was in the air cannot start a second later on.
            ui.set_fetching(true);
            spawn_fetch(&config, &state, &paths, proxy.clone());
            fetching = true;
            ControlFlow::Wait
        } else if let Some(left) = cooling_off {
            ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(left))
        } else {
            ControlFlow::WaitUntil(Instant::now() + TICK)
        };
    })
}

/// Hands the slow half of a rotation to a thread that is allowed to block.
fn spawn_fetch(config: &Config, state: &State, paths: &Paths, proxy: EventLoopProxy<Wake>) {
    let config = config.clone();
    let state = state.clone();
    let cache = paths.cache.clone();
    std::thread::spawn(move || {
        let _ = proxy.send_event(Wake::Fetched(rotation::fetch(&config, &state, &cache)));
    });
}

fn build_tray(menu: &Menu) -> Result<TrayIcon> {
    TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_tooltip("Art Window")
        .with_icon(glyph()?)
        .with_icon_as_template(true)
        .build()
        .map_err(|e| anyhow!("creating the menu bar icon: {e}"))
}

/// What the program shows, and the handles needed to keep it telling the truth.
///
/// Two surfaces, one list: the menu in the bar, and the window of kept pictures
/// when it is open. They are held together rather than side by side because every
/// one of the facts below is true of both at once, and a caller that had to
/// remember to tell the second would eventually forget.
struct Ui {
    menu: Menu,
    /// The two greyed-out rows at the top. `byline` doubles as the status line:
    /// while a fetch is in flight there is no artist worth naming yet, and when one
    /// fails the user would rather know that than read yesterday's credit.
    title: MenuItem,
    byline: MenuItem,
    open: MenuItem,
    /// Asks the source for a different picture without waiting for tomorrow. Greyed
    /// while one is already on its way.
    next: MenuItem,
    keep: MenuItem,
    /// Opens the window the kept pictures can be looked at in. Greyed while there
    /// is nothing kept, since an empty window says less than a greyed row does.
    favourites: MenuItem,
    /// That window. Shut, until this row is clicked.
    gallery: Gallery,
    /// The way back from a kept picture to the one the rotation brought in. Its
    /// text names that picture, so it says what it would return to.
    today: MenuItem,
    reapply: MenuItem,
    login: CheckMenuItem,
    quit: MenuItem,
}

impl Ui {
    fn new(on_pick: impl Fn(Pick) + 'static) -> Result<Self> {
        let ui = Self {
            menu: Menu::new(),
            title: MenuItem::new("", false, None),
            byline: MenuItem::new("", false, None),
            open: MenuItem::new("Open in browser", false, None),
            next: MenuItem::new("Next picture", true, None),
            keep: MenuItem::new("Add to favourites", false, None),
            favourites: MenuItem::new("Favourites…", false, None),
            gallery: Gallery::new(on_pick),
            today: MenuItem::new(NO_WAY_BACK, false, None),
            reapply: MenuItem::new("Re-apply wallpaper", false, None),
            login: CheckMenuItem::new("Start at login", true, autostart::is_enabled(), None),
            quit: MenuItem::new("Quit Art Window", true, None),
        };
        ui.menu
            .append_items(&[
                &ui.title,
                &ui.byline,
                &ui.open,
                &PredefinedMenuItem::separator(),
                &ui.next,
                &ui.keep,
                &ui.favourites,
                &ui.today,
                &PredefinedMenuItem::separator(),
                &ui.reapply,
                &ui.login,
                &PredefinedMenuItem::separator(),
                &ui.quit,
            ])
            .map_err(|e| anyhow!("building the menu: {e}"))?;
        Ok(ui)
    }

    /// Points everything the user can see at whatever is on the desktop now, and
    /// at what is kept.
    fn describe(&mut self, state: &State, favourites: &Favourites) {
        match &state.shown {
            Some(art) => {
                self.title.set_text(&art.title);
                self.byline.set_text(if art.byline.is_empty() {
                    &art.attribution
                } else {
                    &art.byline
                });
                self.open.set_enabled(art.details_url.is_some());
                self.keep.set_enabled(!favourites.holds(art));
                self.reapply.set_enabled(true);
            }
            None => {
                self.title.set_text("No picture yet");
                self.byline.set_text("Waiting for the first one");
                self.open.set_enabled(false);
                self.keep.set_enabled(false);
                self.reapply.set_enabled(false);
            }
        }
        self.relist(favourites);
        self.offer_the_way_back(state);
    }

    /// Points the way-back row at the day's picture, when there is one to go back
    /// to and the desktop is not already showing it.
    ///
    /// The file is checked for because a source can be changed, or a cache emptied,
    /// between the picture being fetched and anyone asking for it again; a row that
    /// only ever reports a failure is worse than a row that is plainly unavailable.
    fn offer_the_way_back(&self, state: &State) {
        let todays = state
            .fetched
            .as_ref()
            .filter(|art| Some(&art.path) != state.shown.as_ref().map(|s| &s.path))
            .filter(|art| art.path.exists());
        match todays {
            Some(art) => {
                self.today
                    .set_text(format!("Back to {}", shorten(&art.title)));
                self.today.set_enabled(true);
            }
            None => {
                self.today.set_text(NO_WAY_BACK);
                self.today.set_enabled(false);
            }
        }
    }

    /// Tells both surfaces what is kept.
    ///
    /// The window is told even though it is usually shut, because the alternative
    /// is for every caller to know whether it is open, and the one that got that
    /// wrong would leave a window showing a painting already thrown away.
    fn relist(&mut self, favourites: &Favourites) {
        self.favourites.set_enabled(!favourites.is_empty());
        self.gallery.relist(favourites);
    }

    /// Opens the window, or brings it forward if it is already up.
    fn present<T>(
        &mut self,
        target: &EventLoopWindowTarget<T>,
        favourites: &Favourites,
    ) -> Result<()> {
        self.gallery.present(target, favourites)
    }

    /// Whether `id` names the window this program opened.
    fn owns_window(&self, id: WindowId) -> bool {
        self.gallery.owns(id)
    }

    /// Shuts the window, if it is open.
    fn dismiss(&mut self) {
        self.gallery.dismiss();
    }

    fn set_status(&self, text: &str) {
        self.byline.set_text(text);
    }

    /// Says whether a download is in the air.
    ///
    /// Greying the row is the half that matters: a second worker started on top of
    /// the first would race it to the desktop, and the one that lost would still be
    /// writing a file into the cache the sweep had already been run for. The status
    /// line is only set on the way in, because whatever comes back replaces it —
    /// a new painting's byline, or the word that there is none.
    fn set_fetching(&self, fetching: bool) {
        self.next.set_enabled(!fetching);
        if fetching {
            self.set_status("Fetching…");
        }
    }

    /// Answers a click, doing what it can and asking for the rest.
    ///
    /// Quit is handled by the caller, which owns the icon.
    fn handle(&self, click: &MenuEvent, state: &State) -> Wanted {
        if click.id == self.keep.id() {
            return Wanted::Keep;
        } else if click.id == self.favourites.id() {
            return Wanted::Gallery;
        } else if click.id == self.today.id() {
            return Wanted::Today;
        } else if click.id == self.next.id() {
            return Wanted::Next;
        }

        if click.id == self.open.id() {
            if let Some(url) = state.shown.as_ref().and_then(|a| a.details_url.as_deref()) {
                let _ = std::process::Command::new("/usr/bin/open")
                    .arg(url)
                    .status();
            }
        } else if click.id == self.reapply.id() {
            if let Some(art) = &state.shown {
                if let Err(e) = wallpaper::pin(&art.path) {
                    report(&e);
                    self.set_status("Could not re-apply the wallpaper");
                }
            }
        } else if click.id == self.login.id() {
            // muda has already flipped the tick; the setting has to catch up with it,
            // and put it back if it cannot.
            let wanted = self.login.is_checked();
            if let Err(e) = autostart::set(wanted) {
                report(&e);
                self.login.set_checked(!wanted);
            }
        }
        Wanted::Nothing
    }
}

/// What the way-back row says when there is nowhere to go back to: either the
/// day's picture is already up, or none has been fetched since the program learnt
/// to remember which one it was.
const NO_WAY_BACK: &str = "Back to today's picture";

/// Menu rows are for recognising a picture, not reading a catalogue entry, and the
/// Met has titles a full line long.
const TITLE_LIMIT: usize = 44;

fn shorten(title: &str) -> String {
    if title.chars().count() <= TITLE_LIMIT {
        return title.to_string();
    }
    let kept: String = title.chars().take(TITLE_LIMIT - 1).collect();
    format!("{}…", kept.trim_end())
}

/// The menu bar glyph: a framed picture with a sun over a hill, drawn a pixel at a
/// time. Eighteen points of menu bar does not justify an image decoder, and macOS
/// tints template images itself for light and dark bars, so only the alpha channel
/// here carries any meaning — `#` is opaque, a space is not.
#[rustfmt::skip]
const GLYPH: [&str; 18] = [
    "                  ",
    "                  ",
    " ################ ",
    " #              # ",
    " #  ##          # ",
    " # ####         # ",
    " #  ##          # ",
    " #              # ",
    " #              # ",
    " #      ##      # ",
    " #     ####     # ",
    " #    ######    # ",
    " #   ########   # ",
    " #  ##########  # ",
    " # ############ # ",
    " ################ ",
    "                  ",
    "                  ",
];

/// tray-icon draws the icon eighteen points tall, and a Retina bar wants two pixels
/// for each of them.
const SCALE: usize = 2;

fn glyph() -> Result<Icon> {
    let side = GLYPH.len();
    let mut rgba = Vec::with_capacity(side * side * SCALE * SCALE * 4);
    for row in GLYPH {
        for _ in 0..SCALE {
            for pixel in row.chars() {
                let alpha = if pixel == ' ' { 0 } else { 255 };
                for _ in 0..SCALE {
                    rgba.extend_from_slice(&[0, 0, 0, alpha]);
                }
            }
        }
    }
    let side = (side * SCALE) as u32;
    Icon::from_rgba(rgba, side, side).map_err(|e| anyhow!("drawing the menu bar icon: {e}"))
}

/// Nudges the run loop so a status item created from inside it appears at once.
#[cfg(target_os = "macos")]
fn wake_run_loop() {
    if let Some(main) = objc2_core_foundation::CFRunLoop::main() {
        main.wake_up();
    }
}

/// The menu carries the short version; this is where the whole chain goes. Under
/// the launchd agent it lands in `~/Library/Logs/ArtWindow.log`.
fn report(e: &anyhow::Error) {
    eprintln!("art-window: {e:#}");
}
