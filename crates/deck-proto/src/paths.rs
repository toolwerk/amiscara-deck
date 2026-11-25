//! Where daemon, hook program and UIs find each other. Std only.

use std::path::PathBuf;

/// File name of the Unix socket inside the data folder.
pub const SOCKET_NAME: &str = "deckd.sock";

/// Where the daemon keeps its socket and state.
///
/// macOS: `~/Library/Application Support/Amiscara Deck`.
/// Elsewhere: `$XDG_DATA_HOME/amiscara-deck`, else `~/.local/share/amiscara-deck`.
/// `DECK_DATA_DIR` overrides both (tests, several daemons side by side).
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("DECK_DATA_DIR").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
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
