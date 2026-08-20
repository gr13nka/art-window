//! Art Window — a daily painting on your desktop, always fit to the screen.
//!
//! Until the tray arrives this is a one-shot command. `--if-due` makes it safe to
//! run often: launchd can wake it every hour and the program decides whether a
//! new picture is actually owed. That keeps the schedule correct across sleep,
//! shutdown and missed windows without anything staying resident.

mod art;
mod config;
mod wallpaper;

use anyhow::{Context, Result};
use art::Source;
use config::{Config, Paths, State};

fn main() -> Result<()> {
    let mut only_if_due = false;
    let mut show_where = false;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "--if-due" => only_if_due = true,
            "--where" => show_where = true,
            "--help" | "-h" => {
                usage();
                return Ok(());
            }
            other => anyhow::bail!("unknown argument {other:?}; try --help"),
        }
    }

    let paths = Paths::locate()?;
    Config::write_default_if_absent(&paths.config)?;

    if show_where {
        println!("config  {}", paths.config.display());
        println!("state   {}", paths.state.display());
        println!("cache   {}", paths.cache.display());
        return Ok(());
    }

    let config = Config::load(&paths.config)?;
    let mut state = State::load(&paths.state);

    if only_if_due && !state.is_due(config.refresh_hours) {
        return Ok(());
    }

    let source = build_source(&config, &state);
    let artwork = source
        .fetch(&paths.cache)
        .with_context(|| format!("fetching from {}", source.label()))?;

    wallpaper::pin(&artwork.path)?;

    // Recorded only now: a fetch that failed above must not count as today's
    // picture, or a bad network moment would cost a whole day.
    state.last_success = Some(config::now_secs());
    state.last_path = Some(artwork.path.clone());
    state.last_met_id = met_id_of(&artwork.path).or(state.last_met_id);
    state.save(&paths.state)?;

    tidy_cache(&paths.cache, &artwork.path);

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

fn build_source(config: &Config, state: &State) -> Box<dyn Source> {
    if config.source == "met" {
        Box::new(art::met::Met::new(state.last_met_id))
    } else {
        Box::new(art::folder::Folder::new(
            expand_tilde(&config.source),
            state.last_path.clone(),
        ))
    }
}

/// Recovers the Met object id from a cached filename, so the next run knows what
/// to avoid without the source having to report it separately.
fn met_id_of(path: &std::path::Path) -> Option<u64> {
    path.file_stem()?
        .to_str()?
        .strip_prefix("met-")?
        .parse()
        .ok()
}

/// Keeps only the picture now on screen. Deleting it while it is the wallpaper
/// would leave a blank desktop, so the current file is always spared.
fn tidy_cache(cache: &std::path::Path, keep: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path != keep {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn expand_tilde(s: &str) -> std::path::PathBuf {
    match s.strip_prefix("~/") {
        Some(rest) => {
            std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(rest)
        }
        None => std::path::PathBuf::from(s),
    }
}

fn usage() {
    println!(
        "Art Window — a daily painting on your desktop, always fit to the screen.

  art-window            fetch a new picture now
  art-window --if-due   fetch only if the current one has had its time
  art-window --where    print where settings and state are kept

Settings live in config.toml; run --where to find it."
    );
}
