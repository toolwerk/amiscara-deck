//! Where daemon, hook program and UIs find each other. Std only.

use std::path::{Path, PathBuf};

/// File name of the Unix socket inside the data folder. On Windows a file of
/// that name holds the name of the daemon's pipe, see [`new_pipe_name`].
pub const SOCKET_NAME: &str = "deckd.sock";

/// The home folder of the user: `HOME`, on Windows else `USERPROFILE`.
pub fn home_dir() -> Option<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|p| !p.is_empty());
    let home = var("HOME");
    #[cfg(windows)]
    let home = home.or_else(|| var("USERPROFILE"));
    home.map(PathBuf::from)
}

/// `std::fs::canonicalize` without the `\\?\` prefix it puts before every
/// Windows path: shells and Claude Code do not take that form.
pub fn canonical(path: &Path) -> std::io::Result<PathBuf> {
    let p = std::fs::canonicalize(path)?;
    if cfg!(windows) {
        let text = p.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{rest}")));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(rest));
        }
    }
    Ok(p)
}

/// Every pipe name of the daemon starts with this.
pub const PIPE_PREFIX: &str = r"\\.\pipe\amiscara-deck-";

/// A new pipe name for a daemon on Windows, random each time.
///
/// A fixed or derived name could be taken first by another user of the
/// machine, who would then read the hook events and answer them. So the
/// daemon picks a name nobody can guess and writes it into the socket path in
/// its data folder, which only the user can read ([`read_pipe_name`]).
pub fn new_pipe_name() -> String {
    use std::hash::BuildHasher;
    // `RandomState` is keyed from the operating system's random source.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let a = std::collections::hash_map::RandomState::new().hash_one((std::process::id(), nanos));
    let b = std::collections::hash_map::RandomState::new().hash_one((nanos, a));
    format!("{PIPE_PREFIX}{a:016x}{b:016x}")
}

/// The pipe name a daemon on Windows wrote into the socket path `socket`.
/// `NotFound` if there is no such file (no daemon), `InvalidData` if it
/// holds anything but a pipe name of ours.
pub fn read_pipe_name(socket: &Path) -> std::io::Result<String> {
    let text = std::fs::read_to_string(socket)?;
    let name = text.trim();
    let ok = name.strip_prefix(PIPE_PREFIX).is_some_and(|rest| {
        !rest.is_empty() && rest.len() <= 64 && rest.bytes().all(|b| b.is_ascii_hexdigit())
    });
    if ok {
        Ok(name.to_owned())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("{} holds no pipe name", socket.display()),
        ))
    }
}

/// Where the daemon keeps its socket and state.
///
/// macOS: `~/Library/Application Support/Amiscara Deck`.
/// Windows: `%LOCALAPPDATA%\Amiscara Deck`.
/// Elsewhere: `$XDG_DATA_HOME/amiscara-deck`, else `~/.local/share/amiscara-deck`.
/// `DECK_DATA_DIR` overrides all (tests, several daemons side by side).
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DECK_DATA_DIR").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .or_else(|| home_dir().map(|h| h.join("AppData").join("Local")))?;
        return Some(local.join("Amiscara Deck"));
    }
    let home = home_dir()?;
    if cfg!(target_os = "macos") {
        return Some(home.join("Library/Application Support/Amiscara Deck"));
    }
    let base = std::env::var_os("XDG_DATA_HOME")
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    Some(base.join("amiscara-deck"))
}

/// The socket in the default data folder.
pub fn default_socket() -> Option<PathBuf> {
    default_data_dir().map(|d| d.join(SOCKET_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_names_are_new_each_time_and_read_back() {
        let a = new_pipe_name();
        let b = new_pipe_name();
        assert_ne!(a, b);
        assert!(
            a.starts_with(PIPE_PREFIX) && a.len() == PIPE_PREFIX.len() + 32,
            "{a}"
        );
        let dir = std::env::temp_dir().join(format!("deck-pipe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let file = dir.join(SOCKET_NAME);
        std::fs::write(&file, format!("{a}\n")).expect("write");
        assert_eq!(read_pipe_name(&file).expect("read"), a);
        for bad in [
            r"\\.\pipe\other",
            r"\\server\pipe\amiscara-deck-ab",
            "",
            "x",
        ] {
            std::fs::write(&file, bad).expect("write");
            assert!(read_pipe_name(&file).is_err(), "{bad}");
        }
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(
            read_pipe_name(&dir.join("none")).map_err(|e| e.kind()),
            Err(std::io::ErrorKind::NotFound)
        );
    }
}
