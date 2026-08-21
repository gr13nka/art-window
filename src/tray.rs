//! The menu bar presence: an icon, a short menu, and a clock.
//!
//! The clock is the reason this exists as a resident program at all. It replaces a
//! launchd job that woke a one-shot command every hour, and it keeps that job's one
//! good idea: *wall-clock time decides whether a picture is owed*, never a countdown.
//! A countdown cannot survive a closed lid, and a machine that sleeps through the
//! moment a timer was set for is the normal case, not the edge one.
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
use crate::config::{Config, Paths, State};
use crate::rotation;
use crate::wallpaper;
use anyhow::{anyhow, Result};
use std::time::{Duration, Instant};
use tao::event::{Event, StartCause};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

#[cfg(target_os = "macos")]
use tao::platform::macos::{ActivationPolicy, EventLoopExtMacOS};

/// How often to look at the clock. Only has to be often enough that a picture owed
/// while the machine was asleep appears shortly after it wakes; the refresh interval
/// itself is measured in hours, so five minutes is already far finer than anyone can
/// notice, and rare enough to leave an idle laptop alone.
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
    /// The worker finished, for better or worse.
    Fetched(Result<Artwork>),
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

    let ui = Ui::new()?;
    ui.describe(&state);

    let mut tray: Option<TrayIcon> = None;
    let mut fetching = false;
    // Set while an attempt is cooling off; cleared by success.
    let mut hold_until: Option<Instant> = None;

    event_loop.run(move |event, _target, control_flow| {
        match event {
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
            }

            // The tick. Someone may have changed the login setting in System
            // Settings since the last one.
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                ui.login.set_checked(autostart::is_enabled());
            }

            Event::UserEvent(Wake::Menu(click)) => {
                if click.id == ui.quit.id() {
                    // Dropping the icon is what takes it out of the menu bar.
                    tray.take();
                    *control_flow = ControlFlow::Exit;
                    return;
                }
                ui.handle(&click, &state);
            }

            Event::UserEvent(Wake::Fetched(result)) => {
                fetching = false;
                match result.and_then(|artwork| {
                    rotation::show(&artwork, &paths, &mut state).map(|()| artwork)
                }) {
                    Ok(_) => {
                        hold_until = None;
                        ui.describe(&state);
                    }
                    Err(e) => {
                        report(&e);
                        hold_until = Some(Instant::now() + RETRY);
                        ui.set_status("Last attempt failed — will retry");
                    }
                }
            }

            _ => {}
        }

        // When to wake up next. Recomputed after every event rather than scheduled
        // once, so that a click, a finished download and a tick all leave the clock
        // in the same, correct place.
        *control_flow = if fetching {
            // The worker will wake us.
            ControlFlow::Wait
        } else if let Some(until) = hold_until.filter(|u| Instant::now() < *u) {
            ControlFlow::WaitUntil(until)
        } else if state.is_due(config.refresh_hours) {
            ui.set_status("Fetching…");
            spawn_fetch(&config, &state, &paths, proxy.clone());
            fetching = true;
            ControlFlow::Wait
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

/// The menu, and the handles needed to keep it telling the truth.
struct Ui {
    menu: Menu,
    /// The two greyed-out rows at the top. `byline` doubles as the status line:
    /// while a fetch is in flight there is no artist worth naming yet, and when one
    /// fails the user would rather know that than read yesterday's credit.
    title: MenuItem,
    byline: MenuItem,
    open: MenuItem,
    reapply: MenuItem,
    login: CheckMenuItem,
    quit: MenuItem,
}

impl Ui {
    fn new() -> Result<Self> {
        let ui = Self {
            menu: Menu::new(),
            title: MenuItem::new("", false, None),
            byline: MenuItem::new("", false, None),
            open: MenuItem::new("Open in browser", false, None),
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
                &ui.reapply,
                &ui.login,
                &PredefinedMenuItem::separator(),
                &ui.quit,
            ])
            .map_err(|e| anyhow!("building the menu: {e}"))?;
        Ok(ui)
    }

    /// Points the menu at whatever is on the desktop now.
    fn describe(&self, state: &State) {
        match &state.shown {
            Some(art) => {
                self.title.set_text(&art.title);
                self.byline.set_text(if art.byline.is_empty() {
                    &art.attribution
                } else {
                    &art.byline
                });
                self.open.set_enabled(art.details_url.is_some());
                self.reapply.set_enabled(true);
            }
            None => {
                self.title.set_text("No picture yet");
                self.byline.set_text("Waiting for the first one");
                self.open.set_enabled(false);
                self.reapply.set_enabled(false);
            }
        }
    }

    fn set_status(&self, text: &str) {
        self.byline.set_text(text);
    }

    /// Acts on a click. Quit is handled by the caller, which owns the icon.
    fn handle(&self, click: &MenuEvent, state: &State) {
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
    }
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
