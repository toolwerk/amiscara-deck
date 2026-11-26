//! The programs that come with the package (daemon, hook program, command
//! line) and what the interface sets up with them: the daemon at login, and on
//! request the hooks in Claude Code. Everything goes through the command line
//! `amiscara-deck`, which knows each system.
//!
//! Where they lie: next to this program (Windows installer, deb in
//! `/usr/bin`). An AppImage is mounted at a new place on every start, so its
//! programs are first copied to `~/.local/share/amiscara-deck/bin`: the hooks
//! and the login entry need a path that stays.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

const CLI: &str = "amiscara-deck";
const HELPERS: [&str; 3] = ["amiscara-deckd", "deck-hook", "amiscara-deck"];

fn exe(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

/// The folder with the three programs, or `None` when this program was not
/// installed from a package (built by hand, for example).
///
/// Only on Windows and Linux (the Mac has its own app), and never with a data
/// folder of its own (`DECK_DATA_DIR`): a test must not set up the real
/// daemon or the real hooks.
pub fn helpers() -> Option<PathBuf> {
    if !cfg!(any(windows, target_os = "linux")) || std::env::var_os("DECK_DATA_DIR").is_some() {
        return None;
    }
    let here = std::env::current_exe().ok()?.parent()?.to_path_buf();
    if !HELPERS.iter().all(|n| here.join(exe(n)).is_file()) {
        return None;
    }
    if std::env::var_os("APPIMAGE").is_some() {
        let home = deck_proto::paths::home_dir()?;
        let lasting = home.join(".local/share/amiscara-deck/bin");
        copy_helpers(&here, &lasting).ok()?;
        return Some(lasting);
    }
    Some(here)
}

/// Copies the programs whose content differs, so an update of the AppImage
/// brings the new ones.
fn copy_helpers(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for name in HELPERS {
        let src = from.join(exe(name));
        let dst = to.join(exe(name));
        if std::fs::read(&src).ok() == std::fs::read(&dst).ok() {
            continue;
        }
        // A new file and a rename: a running daemon keeps its old program.
        let tmp = to.join(format!(".{}.new", exe(name)));
        std::fs::copy(&src, &tmp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
        }
        std::fs::rename(&tmp, &dst)?;
    }
    Ok(())
}

/// Runs the command line without a window; its exit code, `None` if it did
/// not run.
fn cli(dir: &Path, args: &[&str]) -> Option<i32> {
    let mut cmd = Command::new(dir.join(exe(CLI)));
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.status().ok()?.code()
}

/// The daemon at login, started right away. For a package only: by hand the
/// daemon is the business of whoever built it.
pub fn install_daemon() -> bool {
    helpers().is_some_and(|dir| cli(&dir, &["daemon", "install"]) == Some(0))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Hooks {
    Installed,
    Missing,
    /// No package programs, or the check failed: the panel says nothing.
    Unknown,
}

/// Does Claude Code report to Deck? `hooks status` ends with 3 when not.
pub fn hooks() -> Hooks {
    match helpers().and_then(|dir| cli(&dir, &["hooks", "status"])) {
        Some(0) => Hooks::Installed,
        Some(3) => Hooks::Missing,
        _ => Hooks::Unknown,
    }
}

/// Only on a click in the panel: it changes `~/.claude/settings.json`.
pub fn install_hooks() -> bool {
    helpers().is_some_and(|dir| cli(&dir, &["hooks", "install"]) == Some(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_only_what_changed() {
        let from = tempdir("from");
        let to = tempdir("to");
        for n in HELPERS {
            std::fs::write(from.join(exe(n)), n).expect("write");
        }
        copy_helpers(&from, &to).expect("copy");
        assert_eq!(
            std::fs::read_to_string(to.join(exe("deck-hook"))).expect("read"),
            "deck-hook"
        );
        std::fs::write(from.join(exe("deck-hook")), "new").expect("write");
        copy_helpers(&from, &to).expect("copy");
        assert_eq!(
            std::fs::read_to_string(to.join(exe("deck-hook"))).expect("read"),
            "new"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(to.join(exe("deck-hook")))
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        let _ = std::fs::remove_dir_all(from.parent().expect("parent"));
    }

    fn tempdir(name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("deck-setup-test-{}", std::process::id()));
        let dir = base.join(name);
        std::fs::create_dir_all(&dir).expect("dir");
        dir
    }
}
