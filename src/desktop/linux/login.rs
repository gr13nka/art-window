//! XDG autostart is deliberately a file-exists setting, like the launchd backend.

use anyhow::{Context, Result};
use std::path::PathBuf;

const FILE: &str = "dev.artwindow.desktop";

pub(super) fn is_enabled() -> bool {
    entry_path().map(|path| path.is_file()).unwrap_or(false)
}

pub(super) fn set(enabled: bool) -> Result<()> {
    let path = entry_path()?;
    if !enabled {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other.with_context(|| format!("removing {}", path.display())),
        };
    }

    let executable = std::env::current_exe().context("cannot find my own binary")?;
    let executable = executable
        .to_str()
        .context("the Art Window executable path is not valid UTF-8")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, desktop_entry(executable))
        .with_context(|| format!("writing {}", path.display()))
}

fn entry_path() -> Result<PathBuf> {
    let dirs = directories::BaseDirs::new().context("cannot find the XDG config directory")?;
    Ok(dirs.config_dir().join("autostart").join(FILE))
}

fn desktop_entry(executable: &str) -> String {
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Art Window\n\
         Comment=A daily painting on your desktop\n\
         Exec={}\n\
         Terminal=false\n",
        escape_exec(executable)
    )
}

/// Quotes one desktop-entry `Exec` argument. The first pass escapes for the Exec
/// grammar; the second escapes those backslashes for the desktop-entry value.
fn escape_exec(value: &str) -> String {
    let mut exec = String::with_capacity(value.len() + 2);
    exec.push('"');
    for character in value.chars() {
        if matches!(character, '"' | '`' | '$' | '\\') {
            exec.push('\\');
        }
        exec.push(character);
    }
    exec.push('"');
    exec.replace('\\', "\\\\")
}

#[cfg(test)]
mod tests {
    use super::{desktop_entry, escape_exec};

    #[test]
    fn exec_path_is_quoted_and_escaped_twice() {
        assert_eq!(escape_exec("/tmp/art window"), r#""/tmp/art window""#);
        assert_eq!(escape_exec("/tmp/a\\b\"$`"), r#""/tmp/a\\\\b\\"\\$\\`""#);
    }

    #[test]
    fn autostart_entry_has_no_session_specific_directives() {
        let entry = desktop_entry("/tmp/art-window");
        assert!(entry.contains("Exec=\"/tmp/art-window\"\n"));
        assert!(!entry.contains("Autostart-Phase"));
        assert!(!entry.contains("Autostart-Delay"));
    }
}
