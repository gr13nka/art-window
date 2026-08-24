//! Art Window — a daily painting on your desktop, always fit to the screen.
//!
//! With no arguments it settles into the menu bar and looks after itself. The
//! one-shot forms are kept because a resident program is a bad place to find out
//! that the network, the museum or the Dock's database has changed its mind:
//! `--once` does exactly one rotation and says what happened, on a terminal where
//! the answer is visible.

mod art;
mod autostart;
mod config;
mod day;
mod favourites;
mod gallery;
mod rotation;
mod tray;
mod wake;
mod wallpaper;

use anyhow::Result;
use config::{Config, Paths, State};

fn main() -> Result<()> {
    let mut mode = Mode::Tray;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--once" => mode = Mode::Once { only_if_due: false },
            "--if-due" => mode = Mode::Once { only_if_due: true },
            "--where" => mode = Mode::Where,
            "--help" | "-h" => {
                usage();
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other:?}; try --help"),
        }
    }

    let paths = Paths::locate()?;
    Config::write_default_if_absent(&paths.config)?;

    if let Mode::Where = mode {
        println!("config  {}", paths.config.display());
        println!("state   {}", paths.state.display());
        println!("cache   {}", paths.cache.display());
        println!("kept    {}", paths.favourites.display());
        println!(
            "login   {}",
            if autostart::is_enabled() { "on" } else { "off" }
        );
        return Ok(());
    }

    let config = Config::load(&paths.config)?;
    let mut state = State::load(&paths.state);

    match mode {
        Mode::Where => unreachable!("handled above, before the config is read"),
        Mode::Tray => tray::run(paths, config, state),
        Mode::Once { only_if_due } => {
            if only_if_due && !state.is_due() {
                return Ok(());
            }
            let artwork = rotation::fetch(&config, &state, &paths.cache)?;
            rotation::show(&artwork, &config, &paths, &mut state)?;

            println!("{}", artwork.title);
            if !artwork.byline.is_empty() {
                println!("{}", artwork.byline);
            }
            println!("{}", artwork.attribution);
            if let Some(url) = &artwork.details_url {
                println!("{url}");
            }
            Ok(())
        }
    }
}

enum Mode {
    /// Live in the menu bar and rotate on a schedule.
    Tray,
    /// Rotate once and report, for a terminal or a script.
    Once { only_if_due: bool },
    /// Print where the files are and stop.
    Where,
}

fn usage() {
    println!("art-window — a daily painting on your desktop\n");
    println!("  art-window            live in the menu bar and rotate daily");
    println!("  art-window --once     fetch a picture now, then exit");
    println!("  art-window --if-due   the same, but only if one is due");
    println!("  art-window --where    print where settings and pictures live");
}
