//! Whether Art Window comes back on its own when you log in.
//!
//! macOS offers three ways to arrange this and only one of them works everywhere
//! this program runs. `SMAppService` is the tidy modern answer but needs macOS 13.
//! Driving System Events with AppleScript needs an automation permission the user
//! has to grant by hand, and reports its state by string-matching a list. What is
//! left is the oldest and plainest: a launchd agent, which is a property list in
//! `~/Library/LaunchAgents` that launchd reads at login. No permission prompt, no
//! version floor, and the switch is literally whether a file is there.
//!
//! macOS-only, like the rest of the program. If Windows and Linux ever arrive this
//! grows the split that `wallpaper/` already has.
//!
//! Nothing here calls `launchctl`. Loading the agent by hand would start a *second*
//! copy of a program that is, by definition, already running; leaving it to the next
//! login costs nothing, because that is the only moment the setting has any effect.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Reverse-DNS name launchd files the agent under. Distinct from the `.daily`
/// label the pre-tray launchd job used, so upgrading does not leave two schedules
/// racing each other.
const LABEL: &str = "dev.artwindow.tray";

/// Whether the agent is installed.
///
/// Read fresh on every menu open rather than remembered: the user can throw this
/// switch from System Settings, and a remembered answer would quietly start lying.
pub fn is_enabled() -> bool {
    agent_path().map(|p| p.is_file()).unwrap_or(false)
}

/// Installs or removes the agent. Takes effect at the next login.
pub fn set(enabled: bool) -> Result<()> {
    let path = agent_path()?;
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other.with_context(|| format!("removing {}", path.display())),
        };
    }

    // Whichever binary is running claims the slot, so a copy in `~/.local/bin` and
    // one inside an `.app` bundle each register themselves without being told which
    // they are.
    let exe = std::env::current_exe().context("cannot find my own binary")?;
    let log = home()?.join("Library/Logs/ArtWindow.log");

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(
        &path,
        agent_plist(&exe.to_string_lossy(), &log.to_string_lossy()),
    )
    .with_context(|| format!("writing {}", path.display()))
}

fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var("HOME").context("HOME is not set")?,
    ))
}

fn agent_path() -> Result<PathBuf> {
    Ok(home()?
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist")))
}

/// `RunAtLoad` and nothing else. `KeepAlive` is the tempting neighbouring key and
/// it would be a bug: launchd would relaunch the program the moment Quit worked,
/// making the menu item useless.
fn agent_plist(program: &str, log: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{}</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
	<key>StandardOutPath</key>
	<string>{}</string>
	<key>StandardErrorPath</key>
	<string>{}</string>
</dict>
</plist>
"#,
        escape(program),
        escape(log),
        escape(log),
    )
}

/// A home directory may legally contain `&` or `<`, and this is XML.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}
