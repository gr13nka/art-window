//! The resident presence: a compact menu where the desktop supports one, a window,
//! and a clock.
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
//! - AppKit and GTK own their UI on the main thread; AppKit also sets wallpaper
//!   there.
//! - A museum download blocks for up to two minutes, which on the main thread is a
//!   frozen interface.
//!
//! So the fetch goes to a throwaway worker and comes back as an [`Artwork`] through
//! the event loop's own queue, where the main thread hangs it. Nothing is shared
//! between the two; the worker gets copies and returns a value.

use crate::art::{Artwork, Selection};
use crate::backdrop::{App, Backdrop, Slot};
use crate::config::{now_secs, Config, Paths, State};
use crate::desktop::{self, Pinned};
use crate::favourites::Favourites;
use crate::gallery::{Control, Gallery, Pick, Tab};
use crate::rotation;
use crate::settings::Settings;
use crate::wake;
use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy, EventLoopWindowTarget};
use tao::window::WindowId;
use tray_icon::menu::{CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};
#[cfg(target_os = "linux")]
use {
    gio::prelude::{ActionMapExt, ApplicationExt},
    tao::platform::unix::{EventLoopBuilderExtUnix, EventLoopWindowTargetExtUnix},
};

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

/// How long to leave the desktop alone before offering it a picture it took only
/// in part, and how many times to offer it.
///
/// Measured against the Dock, which is what refuses: at login it spends the first
/// minute or so building the wallpaper store this program writes into, and a write
/// during that is refused for both sides at once. A minute apart, five times over,
/// outlasts that comfortably. It stops there because a store still refusing after
/// five minutes is not busy but broken — see the quarantined store in
/// `docs/macos-wallpaper.md` — and asking a broken one all day would restart the
/// Dock for nothing and fill the log with the saying so.
const RE_PIN: Duration = Duration::from_secs(60);
const PATIENCE: u32 = 5;

/// How long after the displays change before the picture is offered again.
///
/// The notification arrives while macOS is still moving Spaces between screens and
/// the Dock is still giving them its default picture. Asked at once, the painting
/// would go up and then be painted over, or be refused outright by a store the Dock
/// is holding. A few seconds is enough for the rearranging to finish, and short
/// enough that the default picture is only a glimpse.
const SETTLE: Duration = Duration::from_secs(5);

/// Something that needs the main thread's attention.
enum Notice {
    /// A menu item was clicked. Forwarded rather than acted on where it arrives,
    /// because that is one of AppKit's own callbacks and no place to do work.
    Menu(MenuEvent),
    /// A window callback already translated into the event loop's vocabulary.
    Chose(Wanted),
    #[cfg(target_os = "linux")]
    /// Whether a StatusNotifierWatcher is currently rendering tray items.
    TrayHost(bool),
    /// The worker finished, for better or worse.
    Fetched(Result<Artwork>),
    /// The machine came back from sleep. Carries nothing, and its arm asks for
    /// nothing: the clock at the tail of the loop is re-read after every event,
    /// which is the whole reason it lives there rather than in an arm of its own.
    /// All the arm does is note that the desktop is being redrawn anyway.
    Woke,
    /// A display was attached, detached or rearranged. On macOS the Spaces that
    /// move between screens arrive showing the Dock's default picture.
    Rearranged,
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
    /// Open the same window on its settings tab.
    Settings,
    /// Use these settings from now on.
    Apply(Settings),
    Browse,
    /// Open a page the window named — a painter's, from the artist browser.
    Read(String),
    Reapply,
    Login(bool),
    /// Keep a painting ready as the background of the next meeting in this app,
    /// or stop.
    Backdrop(App, bool),
    Quit,
}

impl From<Control> for Wanted {
    fn from(control: Control) -> Self {
        match control {
            Control::Browse => Self::Browse,
            Control::Next => Self::Next,
            Control::Keep => Self::Keep,
            Control::Today => Self::Today,
            Control::Reapply => Self::Reapply,
            Control::Login(enabled) => Self::Login(enabled),
            Control::Quit => Self::Quit,
        }
    }
}

impl From<Pick> for Wanted {
    fn from(pick: Pick) -> Self {
        match pick {
            Pick::Show(key) => Self::Show(key),
            Pick::Forget(key) => Self::Forget(key),
            Pick::Apply(settings) => Self::Apply(settings),
            Pick::Read(url) => Self::Read(url),
        }
    }
}

/// What the desktop still owes, and when it is worth interrupting the user to
/// collect it.
///
/// [`desktop::pin`] answers with two kinds of debt and this holds both, because
/// both are settled by the loop and by nothing else.
///
/// [`Pinned::InPart`] is the Dock's store refusing to be written — at login the
/// ordinary case, while the Dock is still building it. That is not a failed
/// rotation: the painting arrived and the day is spent, so no schedule will ever
/// come back to it and the desktop would keep what it had until tomorrow. So the
/// picture is offered again on a clock, and that clock is wall time like every
/// other deadline in this module: a machine that sleeps through the next asking
/// asks on waking rather than a minute of running time later.
///
/// [`Pinned::AfterRedraw`] is the opposite problem — the writing worked, and
/// showing it costs a Dock restart that blanks the desktop for half a minute. That
/// debt is not settled on a clock but at a moment: waking, or beginning a session,
/// when the desktop is going to be redrawn regardless and nobody is watching a
/// picture fail to appear.
struct Owed {
    /// The second at which to offer the picture again; `None` when nothing is owed.
    at: Option<u64>,
    /// How many more askings before the desktop is taken at its word.
    tries: u32,
    /// The picture written but not yet made visible. Naming it prevents a failed
    /// write for a newer picture from publishing an older one by mistake.
    unseen: Option<PathBuf>,
}

impl Owed {
    /// Nothing owed.
    const fn settled() -> Self {
        Self {
            at: None,
            tries: 0,
            unseen: None,
        }
    }

    /// Owes an asking on the next turn of the loop, whatever the desktop last
    /// said — what beginning a session owes, because a session that has just begun
    /// is one whose desktop nobody has seen yet.
    fn owe(&mut self) {
        self.owe_after(Duration::ZERO);
    }

    /// Owes an asking `delay` from now, whatever the desktop last said.
    fn owe_after(&mut self, delay: Duration) {
        self.at = Some(now_secs() + delay.as_secs());
        self.tries = PATIENCE;
    }

    /// Records what the desktop made of a picture just put up.
    fn took(&mut self, pinned: Pinned, path: &Path) {
        self.tries = PATIENCE;
        self.at = match pinned {
            Pinned::InPart => Some(now_secs() + RE_PIN.as_secs()),
            _ => None,
        };
        self.remember_visibility(pinned, path);
    }

    /// Offers the desktop `shown` again, if an asking is owed by now.
    ///
    /// `shown` because that is the picture the desktop is failing to show; a
    /// re-asserted placement is never a new one, and touches neither the day nor
    /// the state. Each asking spends one of [`PATIENCE`], and an error ends them:
    /// a picture that cannot be put up at all is not made puttable by asking twice
    /// more.
    fn press(
        &mut self,
        shown: Option<&Artwork>,
        settings: &Settings,
        scratch: &Path,
    ) -> Result<()> {
        match self.at {
            Some(at) if at <= now_secs() => self.at = None,
            _ => return Ok(()),
        }
        let Some(art) = shown else { return Ok(()) };

        self.tries = self.tries.saturating_sub(1);
        let pinned = desktop::pin(&art.path, &settings.style, &settings.framing, scratch)?;
        self.remember_visibility(pinned, &art.path);
        if pinned == Pinned::InPart && self.tries > 0 {
            self.at = Some(now_secs() + RE_PIN.as_secs());
        }
        Ok(())
    }

    /// Shows whatever has been written for the Spaces nobody is looking at.
    ///
    /// Only ever called where a blanked desktop costs nothing — see
    /// [`desktop::catch_up`] — and cheap to call anywhere, because a desktop that
    /// owes nothing is not disturbed.
    fn catch_up(&mut self) {
        if self.unseen.take().is_some() {
            desktop::catch_up();
        }
    }

    /// Records which picture a future Dock restart would publish. A later picture
    /// supersedes that debt unless the store says it already contains the same path.
    fn remember_visibility(&mut self, pinned: Pinned, path: &Path) {
        if pinned == Pinned::AfterRedraw {
            self.unseen = Some(path.to_path_buf());
        } else if self.unseen.as_deref() != Some(path) {
            self.unseen = None;
        }
    }

    /// Drops the redraw debt because the desktop has just been redrawn by other
    /// means than [`Owed::catch_up`].
    fn forget_unseen(&mut self) {
        self.unseen = None;
    }

    /// Seconds until the next asking, for the clock at the tail of the loop.
    fn left(&self) -> Option<u64> {
        self.at.map(|at| at.saturating_sub(now_secs()))
    }
}

/// When the next picture is fetched, and what becomes of one already on its way.
///
/// The loop tells this what happened — a row was clicked, the worker came back, a
/// picture went up by hand — and asks it one question, at the tail and nowhere
/// else: [`Schedule::step`]. Only that answer starts a download, which is what
/// keeps there from ever being two.
struct Schedule {
    /// A download is in the air.
    fetching: bool,
    /// Unix seconds until which a failed attempt is cooling off; cleared by success.
    /// Wall clock rather than an `Instant` for the reason in the module comment: a
    /// fifteen-minute countdown started before the lid closed still owes fifteen
    /// minutes of *waking* time the next morning, which is the very complaint the
    /// schedule exists to answer.
    cooling_off: Option<u64>,
    /// Set when a picture goes up by hand while a download is still in the air, so
    /// that the download does not land on top of a choice just made.
    superseded: bool,
    /// Set when the Next picture row is clicked. A click asks for a fetch rather
    /// than starting one, so the clock is wound the same way whoever did the asking.
    asked_for_next: bool,
}

/// What the tail of the loop should do about the next picture.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    /// A download is in the air, and its worker will wake the loop.
    Wait,
    /// Start a download now. The schedule already counts it as in the air.
    Fetch,
    /// Nothing to start. Carries the seconds a cooling-off period still has to
    /// run, when there is one.
    Idle(Option<u64>),
}

impl Schedule {
    const fn idle() -> Self {
        Self {
            fetching: false,
            cooling_off: None,
            superseded: false,
            asked_for_next: false,
        }
    }

    fn fetching(&self) -> bool {
        self.fetching
    }

    fn ask_for_next(&mut self) {
        self.asked_for_next = true;
    }

    /// The worker came back. Answers whether what it brought should be hung: not
    /// when a picture went up by hand while it was in the air, because hanging it
    /// now would undo a choice just made.
    fn landed(&mut self) -> bool {
        self.fetching = false;
        !std::mem::take(&mut self.superseded)
    }

    fn succeeded(&mut self) {
        self.cooling_off = None;
    }

    /// Every failure path has to come through here: the day is marked done only on
    /// success, so a failure with no cooling-off is retried at once and for ever.
    fn failed(&mut self, now: u64) {
        self.cooling_off = Some(now + RETRY.as_secs());
    }

    /// A picture went up by hand. Nothing is owed that this has not just answered,
    /// and a download already in the air would only undo it.
    fn chosen_by_hand(&mut self) {
        self.cooling_off = None;
        self.superseded = self.fetching;
    }

    /// Decides what happens next, given whether the day still owes a picture.
    ///
    /// Being asked jumps both queues: somebody looking at a picture they do not
    /// like is not waiting out a museum's bad afternoon, and is plainly not
    /// waiting for tomorrow. The request is spent whatever the answer, so that one
    /// made while a download was in the air cannot start a second the moment the
    /// first lands.
    fn step(&mut self, now: u64, is_due: impl FnOnce() -> bool) -> Step {
        let asked = std::mem::take(&mut self.asked_for_next);
        if self.fetching {
            return Step::Wait;
        }
        let cooling_off = self
            .cooling_off
            .and_then(|until| until.checked_sub(now))
            .filter(|left| *left > 0);
        if asked || (cooling_off.is_none() && is_due()) {
            self.fetching = true;
            Step::Fetch
        } else {
            Step::Idle(cooling_off)
        }
    }
}

/// Runs until the user picks Quit.
pub fn run(paths: Paths, config: Config, mut settings: Settings, mut state: State) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        glib::set_prgname(Some(desktop::APP_ID));
    }
    let mut builder = EventLoopBuilder::<Notice>::with_user_event();
    #[cfg(target_os = "linux")]
    builder.with_app_id(desktop::APP_ID);
    #[cfg_attr(not(target_os = "macos"), allow(unused_mut))]
    let mut event_loop = builder.build();

    #[cfg(target_os = "linux")]
    gdk::set_program_class(desktop::APP_ID);

    #[cfg(target_os = "linux")]
    if event_loop.gtk_app().is_remote() {
        event_loop.gtk_app().activate();
        return Ok(());
    }

    // No Dock icon, no application menu: this program lives in the menu bar. The
    // bundle's `LSUIElement` says the same thing earlier and without the momentary
    // bounce, but this is what makes an unbundled `cargo run` behave.
    #[cfg(target_os = "macos")]
    event_loop.set_activation_policy(ActivationPolicy::Accessory);

    let proxy = event_loop.create_proxy();

    // One tray icon per session: autostart and a Start-menu click would otherwise
    // make two. The claim is held until the process ends, which `run` never
    // survives, and it is also how a later `--quit` reaches this one.
    #[cfg(windows)]
    let _instance = {
        let quit_proxy = proxy.clone();
        match desktop::claim_instance(move || {
            let _ = quit_proxy.send_event(Notice::Chose(Wanted::Quit));
        })? {
            Some(instance) => instance,
            None => return Ok(()),
        }
    };

    let menu_proxy = proxy.clone();
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = menu_proxy.send_event(Notice::Menu(event));
    }));

    #[cfg(target_os = "linux")]
    let _tray_host_watch = {
        // The D-Bus subscription is delivered on this GTK main context. It still
        // goes through tao's queue so tray changes and window actions have one
        // ordering and one owner.
        let host_proxy = proxy.clone();
        match desktop::watch_tray_host(move |present| {
            let _ = host_proxy.send_event(Notice::TrayHost(present));
        }) {
            Ok(watch) => Some(watch),
            Err(error) => {
                report(&error);
                None
            }
        }
    };

    // Both callbacks do the same thing and for the same reason: they arrive on
    // somebody else's terms — AppKit's here, the workspace's below — and neither is
    // a place to touch the state the loop owns. They say what happened; the loop
    // decides what it means.
    //
    // Never dropped, because `run` below never returns. Dropping it would silence
    // the notifications, which is what the binding is holding them open against.
    let wake_proxy = proxy.clone();
    let _woken = match wake::watch(move || {
        let _ = wake_proxy.send_event(Notice::Woke);
    }) {
        Ok(watch) => Some(watch),
        Err(error) => {
            // The Linux GLib tick below preserves correctness; wake notification
            // only removes up to a minute of latency after opening a lid.
            report(&error);
            None
        }
    };
    // Held the same way and for the same reason as the wake watch above.
    let rearranged_proxy = proxy.clone();
    let _rearranged = wake::displays(move || {
        let _ = rearranged_proxy.send_event(Notice::Rearranged);
    })
    .unwrap_or_else(|error| {
        // Losing it costs only the rescue after an unplug; the next wake or login
        // still re-asserts the picture.
        report(&error);
        None
    });

    let mut favourites = Favourites::open(&paths.favourites)?;

    // The window answers a click the same way the menu does: by saying what
    // happened and letting the loop decide what it means.
    let pick_proxy = proxy.clone();
    let control_proxy = proxy.clone();
    let mut ui = Ui::new(
        move |pick| {
            let _ = pick_proxy.send_event(Notice::Chose(pick.into()));
        },
        move |control| {
            let _ = control_proxy.send_event(Notice::Chose(control.into()));
        },
        paths.cache.clone(),
        &state.backdrops,
    )?;

    // Meeting backgrounds keep their own clock on their own thread — see
    // [`backdrop`] — so nothing below schedules them. The loop holds the handle
    // only because dropping it is what stops them.
    let mut backdrops: Vec<(App, Backdrop)> = state
        .backdrops
        .iter()
        .filter(|slot| App::all().contains(&slot.app()))
        .map(|slot| {
            let running = Backdrop::begin(
                slot.clone(),
                false,
                &config,
                &paths.cache,
                &settings.filters,
            );
            (slot.app(), running)
        })
        .collect();
    // Only the museum catalogue knows a painting's region, subject, artist or
    // size; the window says so rather than offering filters that do nothing.
    let filters_apply = config.source.honours_filters();
    ui.gallery
        .set_settings(&settings, filters_apply, &favourites);
    ui.describe(&state, &favourites);

    let mut tray: Option<TrayIcon> = None;
    #[cfg(target_os = "linux")]
    let mut timer_started = false;
    // Whether the session has already begun. Linux emits `Init` again for every
    // later launcher click — that is how a second `art-window` asks this one to show
    // itself — so "the loop is starting" is a fact worth keeping rather than one to
    // read off the event.
    let mut initialized = false;
    #[cfg(target_os = "linux")]
    let mut quit_action_added = false;
    #[cfg(target_os = "linux")]
    let mut window_requested = false;
    #[cfg(target_os = "linux")]
    let indicator_available = desktop::appindicator_available();
    // When the next picture is fetched. A fetch is started at the tail of the loop
    // and nowhere else; everything above it only tells this what happened.
    let mut schedule = Schedule::idle();
    // What the desktop still owes, and when to ask it again.
    let mut owed = Owed::settled();

    event_loop.run(move |event, target, control_flow| {
        // Whether the desktop is being redrawn around this event anyway, which is
        // what makes it a free moment to show a picture the Spaces out of sight are
        // still waiting for. Two events say so and no state outlives them, so this
        // belongs to the pass rather than to the loop.
        let mut redrawing = false;

        // Every event is first read for what it asks of the loop, and only then
        // acted on. The two halves are separate because the same two things can be
        // asked from two places — a menu row and a picture in the window — and
        // answering them twice over is how the two would drift apart.
        let wanted = match event {
            Event::NewEvents(StartCause::Init) => {
                let starting = !initialized;
                initialized = true;

                #[cfg(target_os = "linux")]
                if !timer_started {
                    // tao's GTK backend does not register a source for WaitUntil.
                    // This source decides nothing; it merely lets the loop ask its
                    // wall-clock question at least once a minute.
                    glib::timeout_add_seconds_local(60, || glib::ControlFlow::Continue);
                    timer_started = true;
                }

                #[cfg(any(target_os = "macos", windows))]
                {
                    // tray-icon wants a run loop that is already turning — building
                    // here keeps it visible in front of full-screen applications.
                    match build_tray(&ui.menu) {
                        Ok(built) => tray = Some(built),
                        Err(e) => {
                            report(&e);
                            *control_flow = ControlFlow::Exit;
                            return;
                        }
                    }
                    nudge_run_loop();
                }

                #[cfg(target_os = "linux")]
                {
                    if !quit_action_added {
                        let quit = gio::SimpleAction::new(desktop::QUIT_ACTION, None);
                        let quit_proxy = proxy.clone();
                        quit.connect_activate(move |_, _| {
                            let _ = quit_proxy.send_event(Notice::Chose(Wanted::Quit));
                        });
                        target.gtk_app().add_action(&quit);
                        quit_action_added = true;
                    }

                    // A remote GApplication activation emits Init again. The first
                    // one establishes fallback presence; every later one is the
                    // explicit gesture to bring that window back.
                    if !starting {
                        window_requested = true;
                    }
                    if let Err(error) = ui.present(target, &favourites, Tab::Favourites) {
                        report(&error);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                    if tray.is_some() && !window_requested {
                        ui.minimize();
                    }
                }

                // A session that has just begun is the moment the desktop is least
                // likely to be showing what this program last put there: on macOS
                // the Dock rebuilds its wallpaper store as it starts and refuses
                // every write while it does, so the picture that went up at the end
                // of the last session can have been lost with it. Nothing else
                // would put that right — the day is settled the moment a picture
                // arrives, and no rotation is owed until tomorrow.
                if starting {
                    owed.owe();
                    redrawing = true;
                }
                Wanted::Nothing
            }

            // The tick. Someone may have changed the login setting in System
            // Settings since the last one.
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                ui.set_login(desktop::starts_at_login());
                Wanted::Nothing
            }

            Event::UserEvent(Notice::Menu(click)) => ui.handle(&click),

            Event::UserEvent(Notice::Chose(wanted)) => wanted,

            #[cfg(target_os = "linux")]
            Event::UserEvent(Notice::TrayHost(present)) => {
                if present && indicator_available {
                    if tray.is_none() {
                        match build_tray(&ui.menu) {
                            Ok(built) => {
                                tray = Some(built);
                                nudge_run_loop();
                            }
                            Err(error) => report(&error),
                        }
                    }
                    if tray.is_some() && !window_requested {
                        ui.minimize();
                    }
                } else {
                    tray.take();
                    if let Err(error) = ui.present(target, &favourites, Tab::Favourites) {
                        report(&error);
                        *control_flow = ControlFlow::Exit;
                        return;
                    }
                }
                Wanted::Nothing
            }

            // Shutting the window is not quitting: the program lives in the menu
            // bar and carries on there. Deliberately not returned from, so the
            // clock below is still wound.
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
                ..
            } => {
                if ui.owns_window(window_id) {
                    #[cfg(target_os = "linux")]
                    {
                        window_requested = false;
                    }
                    ui.close_window();
                }
                Wanted::Nothing
            }

            Event::UserEvent(Notice::Fetched(result)) => {
                ui.set_fetching(false);
                // Dropped on the floor when a kept picture went up while this was
                // in the air: hanging it now would undo a choice just made. The
                // file stays where it is, for the next rotation to overwrite or
                // sweep away. Skipped rather than returned from, because the clock
                // below still has to be wound.
                if schedule.landed() {
                    match result.and_then(|artwork| {
                        rotation::show(&artwork, &settings, &config, &paths, &mut state)
                            .map(|pinned| (artwork, pinned))
                    }) {
                        Ok((artwork, pinned)) => {
                            schedule.succeeded();
                            owed.took(pinned, &artwork.path);
                            // Where a favourite dropped while it was on the desktop
                            // finally goes.
                            favourites.discard_all_but(Some(&artwork.path));
                            ui.describe(&state, &favourites);
                        }
                        Err(e) => {
                            report(&e);
                            schedule.failed(now_secs());
                            ui.set_status("Last attempt failed — will retry");
                        }
                    }
                }
                Wanted::Nothing
            }

            // Nothing to do but arrive: the clock below is what this is for. The
            // screen is coming back with it, which makes this one of the two
            // moments a blanked desktop costs nothing.
            Event::UserEvent(Notice::Woke) => {
                redrawing = true;
                Wanted::Nothing
            }

            // A monitor unplugged leaves its Spaces on the screens that remain,
            // showing the Dock's default. What is owed is exactly what beginning a
            // session owes — the picture asked for again, whatever the desktop last
            // said — once the rearranging has settled. Not a redraw, though: the
            // user is looking straight at the screen, so the Spaces out of sight
            // wait for the next wake as they always do.
            Event::UserEvent(Notice::Rearranged) => {
                owed.owe_after(SETTLE);
                Wanted::Nothing
            }

            _ => Wanted::Nothing,
        };

        // Both ways of picking a picture by hand end in the same place, so the arms
        // only say which picture and the work happens once, below.
        let mut chosen: Option<Artwork> = None;
        match wanted {
            Wanted::Nothing => {}

            Wanted::Next => schedule.ask_for_next(),

            Wanted::Gallery | Wanted::Settings => {
                #[cfg(target_os = "linux")]
                {
                    window_requested = true;
                }
                let tab = match wanted {
                    Wanted::Settings => Tab::Settings,
                    _ => Tab::Favourites,
                };
                if let Err(e) = ui.present(target, &favourites, tab) {
                    report(&e);
                    ui.set_status("Could not open the window");
                }
            }

            // Filters wait for the next picture — the one on the desktop was chosen
            // under the old ones and is still a fair choice. A new style, or the
            // picture moved about under the same one, does not wait: the person pressing Apply is looking at the preview of exactly
            // this picture hung that way, and expects to see it. Only the Space in
            // front of them, though; the rest catch up at the next redraw, as every
            // other change does.
            Wanted::Apply(staged) => match staged.save(&paths.settings) {
                Ok(()) => {
                    let restyled =
                        staged.style != settings.style || staged.framing != settings.framing;
                    settings = staged;
                    ui.gallery
                        .set_settings(&settings, filters_apply, &favourites);
                    for (_, backdrop) in &backdrops {
                        backdrop.set_filters(&settings.filters);
                    }
                    if let Some(art) = state.shown.as_ref().filter(|_| restyled) {
                        match desktop::pin(
                            &art.path,
                            &settings.style,
                            &settings.framing,
                            &paths.cache,
                        ) {
                            Ok(pinned) => owed.took(pinned, &art.path),
                            Err(error) => {
                                report(&error);
                                ui.set_status("Could not hang the picture that way");
                            }
                        }
                    }
                }
                Err(error) => {
                    report(&error);
                    ui.set_status("Could not save the settings");
                }
            },

            Wanted::Browse => {
                if let Some(url) = state
                    .shown
                    .as_ref()
                    .and_then(|art| art.details_url.as_deref())
                {
                    desktop::browse(url);
                }
            }

            Wanted::Read(url) => desktop::browse(&url),

            // The one place the desktop is blanked while somebody is watching, and
            // rightly: this row exists to say *put it right now*, and waiting for
            // the next redraw is not what it means.
            Wanted::Reapply => {
                if let Some(art) = &state.shown {
                    match desktop::pin(&art.path, &settings.style, &settings.framing, &paths.cache)
                    {
                        Ok(pinned) => {
                            owed.took(pinned, &art.path);
                            if pinned != Pinned::InPart {
                                // Explicitly disruptive: even an unchanged store may
                                // be newer than the Dock's in-memory copy.
                                desktop::catch_up();
                                owed.forget_unseen();
                            }
                        }
                        Err(error) => {
                            report(&error);
                            ui.set_status("Could not re-apply the wallpaper");
                        }
                    }
                }
            }

            Wanted::Login(enabled) => {
                if let Err(error) = desktop::set_start_at_login(enabled) {
                    report(&error);
                    ui.set_login(!enabled);
                } else {
                    ui.set_login(enabled);
                }
            }

            // Adopting comes first and can fail in the ordinary way — nobody has
            // added a background in that app yet — so the box is unticked again and
            // the reason, which says what to do about it, goes where it is read.
            Wanted::Backdrop(app, true) => {
                let adopted = Slot::adopt(app).and_then(|slot| {
                    state.record_backdrop(app, Some(slot.clone()), &paths.state)?;
                    Ok(slot)
                });
                match adopted {
                    Ok(slot) => {
                        let running =
                            Backdrop::begin(slot, true, &config, &paths.cache, &settings.filters);
                        backdrops.retain(|(kept, _)| *kept != app);
                        backdrops.push((app, running));
                    }
                    Err(error) => {
                        report(&error);
                        ui.set_backdrop(app, false);
                        ui.set_status(&error.to_string());
                    }
                }
            }

            // The painting already in the slot stays there: it is the user's
            // background now, and taking it away would leave the app with nothing.
            Wanted::Backdrop(app, false) => {
                backdrops.retain(|(kept, _)| *kept != app);
                if let Err(error) = state.record_backdrop(app, None, &paths.state) {
                    report(&error);
                }
            }

            Wanted::Quit => {
                tray.take();
                ui.dismiss();
                *control_flow = ControlFlow::Exit;
                return;
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
                    let on_desktop = state.shown.as_ref().map(|art| art.path.as_path());
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
            match rotation::revisit(&art, &settings, &config, &paths, &mut state) {
                Ok(pinned) => {
                    schedule.chosen_by_hand();
                    owed.took(pinned, &art.path);
                    favourites.discard_all_but(Some(&art.path));
                    ui.describe(&state, &favourites);
                }
                Err(e) => {
                    report(&e);
                    ui.set_status("Could not put that picture up");
                }
            }
        }

        // Whatever the desktop still owes, offered to it again. Last of the three,
        // because a picture that has just gone up in one of them has already said
        // what it wants asking for and when. Not while a download is in the air,
        // though: that picture is about to be replaced, and the loop would be
        // pressing for a painting nobody will see.
        if !schedule.fetching() {
            if let Err(e) = owed.press(state.shown.as_ref(), &settings, &paths.cache) {
                report(&e);
                ui.set_status("Could not re-apply the wallpaper");
            }
        }

        // A desktop that is being redrawn anyway can be shown what was written for
        // the Spaces out of sight, at no cost anyone will notice. After the pressing
        // above and never before it: at the start of a session it is that asking
        // which leaves something written to show.
        if redrawing {
            owed.catch_up();
        }

        // When to wake up next. Recomputed after every event rather than scheduled
        // once, so that a click, a finished download and a tick all leave the clock
        // in the same, correct place.
        *control_flow = match schedule.step(now_secs(), || state.is_due()) {
            Step::Wait => ControlFlow::Wait,
            Step::Fetch => {
                ui.set_fetching(true);
                // Measured here because only the main thread may ask about screens.
                let selection = Selection {
                    filters: settings.filters.clone(),
                    screen_aspect: desktop::primary_aspect(),
                };
                spawn_fetch(&config, &state, &paths, selection, proxy.clone());
                ControlFlow::Wait
            }
            Step::Idle(cooling_off) => {
                // The nearer of the two standing deadlines. Either may be further
                // off than the tick — a cooling-off period is three of them — and
                // waiting the whole of it is right: the tick asks a question these
                // two have already answered.
                match [cooling_off, owed.left()].into_iter().flatten().min() {
                    Some(left) => {
                        ControlFlow::WaitUntil(Instant::now() + Duration::from_secs(left))
                    }
                    None => ControlFlow::WaitUntil(Instant::now() + TICK),
                }
            }
        };
    })
}

/// Hands the slow half of a rotation to a thread that is allowed to block.
fn spawn_fetch(
    config: &Config,
    state: &State,
    paths: &Paths,
    selection: Selection,
    proxy: EventLoopProxy<Notice>,
) {
    let config = config.clone();
    let state = state.clone();
    let cache = paths.cache.clone();
    std::thread::spawn(move || {
        let fetched = rotation::fetch(&config, &state, &cache, &selection);
        let _ = proxy.send_event(Notice::Fetched(fetched));
    });
}

fn build_tray(menu: &Menu) -> Result<TrayIcon> {
    let builder = TrayIconBuilder::new()
        .with_menu(Box::new(menu.clone()))
        .with_tooltip("Art Window")
        .with_icon(glyph()?);
    #[cfg(target_os = "macos")]
    let builder = builder.with_icon_as_template(true);
    builder
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
    favourites_row: MenuItem,
    /// Opens the same window on the filters and placement styles.
    settings: MenuItem,
    /// That window. Shut, until one of these rows is clicked.
    gallery: Gallery,
    /// The way back from a kept picture to the one the rotation brought in. Its
    /// text names that picture, so it says what it would return to.
    today: MenuItem,
    reapply: MenuItem,
    login: CheckMenuItem,
    /// Whether each meeting gets a painting behind the user, one row for every
    /// meeting app [`App::all`] says can be reached here.
    backdrops: Vec<(App, CheckMenuItem)>,
    quit: MenuItem,
}

impl Ui {
    fn new(
        on_pick: impl Fn(Pick) + 'static,
        on_control: impl Fn(Control) + 'static,
        artist_pictures: PathBuf,
        backdrops: &[Slot],
    ) -> Result<Self> {
        let ui = Self {
            menu: Menu::new(),
            title: MenuItem::new("", false, None),
            byline: MenuItem::new("", false, None),
            open: MenuItem::new("Open in browser", false, None),
            next: MenuItem::new("Next picture", true, None),
            keep: MenuItem::new("Add to favourites", false, None),
            favourites_row: MenuItem::new("Favourites…", false, None),
            settings: MenuItem::new("Settings…", true, None),
            gallery: Gallery::new(on_pick, on_control, artist_pictures),
            today: MenuItem::new(NO_WAY_BACK, false, None),
            reapply: MenuItem::new("Re-apply wallpaper", false, None),
            login: CheckMenuItem::new("Start at login", true, desktop::starts_at_login(), None),
            backdrops: App::all()
                .iter()
                .map(|&app| {
                    let on = backdrops.iter().any(|slot| slot.app() == app);
                    (app, CheckMenuItem::new(app.label(), true, on, None))
                })
                .collect(),
            quit: MenuItem::new("Quit Art Window", true, None),
        };
        let (rule_a, rule_b, rule_c) = (
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::separator(),
            PredefinedMenuItem::separator(),
        );
        let mut rows: Vec<&dyn IsMenuItem> = vec![
            &ui.title,
            &ui.byline,
            &ui.open,
            &rule_a,
            &ui.next,
            &ui.keep,
            &ui.favourites_row,
            &ui.today,
            &rule_b,
            &ui.settings,
            &ui.reapply,
            &ui.login,
        ];
        rows.extend(ui.backdrops.iter().map(|(_, row)| row as &dyn IsMenuItem));
        rows.extend([&rule_c as &dyn IsMenuItem, &ui.quit]);
        ui.menu
            .append_items(&rows)
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
        self.gallery.describe(state, favourites);
        self.favourites_row.set_enabled(!favourites.is_empty());
        self.offer_the_way_back(state);
    }

    /// Points the way-back row at the day's picture, when [`State::way_back`] says
    /// there is one to go back to.
    fn offer_the_way_back(&self, state: &State) {
        match state.way_back() {
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

    /// Opens the window, or brings it forward if it is already up.
    fn present<T>(
        &mut self,
        target: &EventLoopWindowTarget<T>,
        favourites: &Favourites,
        tab: Tab,
    ) -> Result<()> {
        self.gallery.present(target, favourites, tab)
    }

    /// Whether `id` names the window this program opened.
    fn owns_window(&self, id: WindowId) -> bool {
        self.gallery.owns(id)
    }

    /// Shuts the window, if it is open.
    fn dismiss(&mut self) {
        self.gallery.dismiss();
    }

    fn close_window(&mut self) {
        self.gallery.close();
    }

    #[cfg(target_os = "linux")]
    fn minimize(&self) {
        self.gallery.minimize();
    }

    fn set_status(&mut self, text: &str) {
        self.byline.set_text(text);
        self.gallery.set_status(text);
    }

    fn set_login(&mut self, enabled: bool) {
        self.login.set_checked(enabled);
        self.gallery.set_login(enabled);
    }

    fn set_backdrop(&mut self, app: App, enabled: bool) {
        if let Some((_, row)) = self.backdrops.iter().find(|(kept, _)| *kept == app) {
            row.set_checked(enabled);
        }
    }

    /// Says whether a download is in the air.
    ///
    /// Greying the row is the half that matters: a second worker started on top of
    /// the first would race it to the desktop, and the one that lost would still be
    /// writing a file into the cache the sweep had already been run for. The status
    /// line is only set on the way in, because whatever comes back replaces it —
    /// a new painting's byline, or the word that there is none.
    fn set_fetching(&mut self, fetching: bool) {
        self.next.set_enabled(!fetching);
        self.gallery.set_fetching(fetching);
        if fetching {
            self.set_status("Fetching…");
        }
    }

    /// Translates a click without acting on it. Both surfaces meet in the event
    /// loop's `Wanted` match, so side effects have exactly one owner.
    fn handle(&self, click: &MenuEvent) -> Wanted {
        if click.id == self.keep.id() {
            return Wanted::Keep;
        } else if click.id == self.favourites_row.id() {
            return Wanted::Gallery;
        } else if click.id == self.settings.id() {
            return Wanted::Settings;
        } else if click.id == self.today.id() {
            return Wanted::Today;
        } else if click.id == self.next.id() {
            return Wanted::Next;
        }

        if click.id == self.open.id() {
            Wanted::Browse
        } else if click.id == self.reapply.id() {
            Wanted::Reapply
        } else if click.id == self.login.id() {
            Wanted::Login(self.login.is_checked())
        } else if let Some((app, row)) = self.backdrops.iter().find(|(_, row)| click.id == row.id())
        {
            Wanted::Backdrop(*app, row.is_checked())
        } else if click.id == self.quit.id() {
            Wanted::Quit
        } else {
            Wanted::Nothing
        }
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
/// time. Eighteen points does not justify an image decoder. macOS tints the black
/// template itself; GNOME does not tint an absolute icon path, so Linux carries
/// white ink for its dark default panel.
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
    #[cfg(target_os = "macos")]
    let ink: [u8; 3] = [0, 0, 0];
    #[cfg(target_os = "linux")]
    let ink: [u8; 3] = [255, 255, 255];
    // Windows draws the icon as given, on a taskbar the user may have made light or
    // dark. Asked once: a theme changed mid-session keeps the old ink until the next.
    #[cfg(windows)]
    let ink: [u8; 3] = if desktop::light_taskbar() {
        [0, 0, 0]
    } else {
        [255, 255, 255]
    };

    let side = GLYPH.len();
    let mut rgba = Vec::with_capacity(side * side * SCALE * SCALE * 4);
    for row in GLYPH {
        for _ in 0..SCALE {
            for pixel in row.chars() {
                let alpha = if pixel == ' ' { 0 } else { 255 };
                for _ in 0..SCALE {
                    rgba.extend_from_slice(&[ink[0], ink[1], ink[2], alpha]);
                }
            }
        }
    }
    let side = (side * SCALE) as u32;
    Icon::from_rgba(rgba, side, side).map_err(|e| anyhow!("drawing the menu bar icon: {e}"))
}

/// Nudges the run loop so a status item created from inside it appears at once.
#[cfg(target_os = "macos")]
fn nudge_run_loop() {
    if let Some(main) = objc2_core_foundation::CFRunLoop::main() {
        main.wake_up();
    }
}

/// GTK's main context and the Win32 message loop are already turning; neither
/// needs an AppKit-style nudge.
#[cfg(not(target_os = "macos"))]
fn nudge_run_loop() {}

/// The menu carries the short version; this is where the whole chain goes. Under
/// the launchd agent it lands in `~/Library/Logs/ArtWindow.log`; on Windows,
/// `art-window.log` beside the state file — see `desktop::log_to`.
fn report(e: &anyhow::Error) {
    eprintln!("art-window: {e:#}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_controls_translate_without_side_effects() {
        assert!(matches!(Wanted::from(Control::Browse), Wanted::Browse));
        assert!(matches!(Wanted::from(Control::Next), Wanted::Next));
        assert!(matches!(Wanted::from(Control::Keep), Wanted::Keep));
        assert!(matches!(Wanted::from(Control::Today), Wanted::Today));
        assert!(matches!(Wanted::from(Control::Reapply), Wanted::Reapply));
        assert!(matches!(
            Wanted::from(Control::Login(true)),
            Wanted::Login(true)
        ));
        assert!(matches!(Wanted::from(Control::Quit), Wanted::Quit));
    }

    #[test]
    fn favourite_picks_keep_their_stable_keys() {
        assert!(matches!(
            Wanted::from(Pick::Show("one".into())),
            Wanted::Show(key) if key == "one"
        ));
        assert!(matches!(
            Wanted::from(Pick::Forget("two".into())),
            Wanted::Forget(key) if key == "two"
        ));
        assert!(matches!(
            Wanted::from(Pick::Apply(Settings::default())),
            Wanted::Apply(s) if s == Settings::default()
        ));
        assert!(matches!(
            Wanted::from(Pick::Read("https://example.org".into())),
            Wanted::Read(url) if url == "https://example.org"
        ));
    }

    #[test]
    fn a_day_that_is_owed_starts_one_download_and_then_waits() {
        let mut schedule = Schedule::idle();
        assert_eq!(schedule.step(100, || true), Step::Fetch);
        assert_eq!(schedule.step(101, || true), Step::Wait);

        assert!(schedule.landed());
        schedule.succeeded();
        assert_eq!(schedule.step(102, || false), Step::Idle(None));
    }

    #[test]
    fn a_failure_cools_off_before_the_day_is_asked_about_again() {
        let mut schedule = Schedule::idle();
        assert_eq!(schedule.step(100, || true), Step::Fetch);
        assert!(schedule.landed());
        schedule.failed(100);

        let left = RETRY.as_secs() - 10;
        assert_eq!(
            schedule.step(110, || panic!("not asked while cooling off")),
            Step::Idle(Some(left))
        );
        assert_eq!(schedule.step(100 + RETRY.as_secs(), || true), Step::Fetch);
    }

    #[test]
    fn being_asked_for_the_next_picture_jumps_the_cooling_off_and_the_day() {
        let mut schedule = Schedule::idle();
        schedule.failed(100);
        schedule.ask_for_next();
        assert_eq!(schedule.step(101, || false), Step::Fetch);
    }

    #[test]
    fn a_request_made_mid_download_does_not_start_a_second_one() {
        let mut schedule = Schedule::idle();
        assert_eq!(schedule.step(100, || true), Step::Fetch);
        schedule.ask_for_next();
        assert_eq!(schedule.step(101, || false), Step::Wait);

        assert!(schedule.landed());
        schedule.succeeded();
        assert_eq!(schedule.step(102, || false), Step::Idle(None));
    }

    #[test]
    fn a_picture_chosen_by_hand_drops_the_download_in_the_air() {
        let mut schedule = Schedule::idle();
        assert_eq!(schedule.step(100, || true), Step::Fetch);
        schedule.chosen_by_hand();
        assert!(!schedule.landed());

        // And only that one: the next download is hung as usual.
        assert_eq!(schedule.step(101, || true), Step::Fetch);
        assert!(schedule.landed());
    }

    #[test]
    fn a_picture_chosen_by_hand_ends_a_cooling_off() {
        let mut schedule = Schedule::idle();
        schedule.failed(100);
        schedule.chosen_by_hand();
        assert_eq!(schedule.step(101, || true), Step::Fetch);
        assert!(schedule.landed(), "nothing was in the air to supersede");
    }

    #[test]
    fn a_new_picture_supersedes_an_older_unseen_write() {
        let mut owed = Owed::settled();
        owed.took(Pinned::AfterRedraw, Path::new("old.jpg"));
        assert_eq!(owed.unseen.as_deref(), Some(Path::new("old.jpg")));

        owed.took(Pinned::InPart, Path::new("new.jpg"));
        assert!(owed.unseen.is_none());
        assert!(owed.at.is_some());
    }

    #[test]
    fn reasserting_the_same_picture_keeps_its_redraw_debt() {
        let mut owed = Owed::settled();
        owed.took(Pinned::AfterRedraw, Path::new("same.jpg"));
        owed.took(Pinned::Everywhere, Path::new("same.jpg"));

        assert_eq!(owed.unseen.as_deref(), Some(Path::new("same.jpg")));
        assert!(owed.at.is_none());
    }
}
