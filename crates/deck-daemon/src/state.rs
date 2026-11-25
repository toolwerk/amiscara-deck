//! The state across restarts: one small file in the data folder.
//!
//! It holds what a person would miss after a restart: the agents with their
//! last prompt and activity, the settings, and the counter for request ids.
//! Open requests do not survive: the hook programs that waited for them are
//! gone, so they come back as "waits in the terminal" (see `Store::restore`).
//! The file holds prompts and commands, so it is private (0600) like the folder.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use deck_proto::message::Settings;
use deck_session::{Snapshot, Store};
use serde::{Deserialize, Serialize};

/// Raised when the file can no longer be read by an older daemon.
pub const VERSION: u32 = 1;

pub const FILE_NAME: &str = "state.json";

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    settings: Settings,
    #[serde(flatten)]
    snapshot: Snapshot,
}

/// Read the state. `None` for a missing, unreadable, garbled or newer file:
/// the daemon then starts empty and the next save replaces it.
pub fn load(path: &Path, host: &str, now_ms: u64) -> Option<(Store, Settings)> {
    let text = std::fs::read_to_string(path).ok()?;
    let file: File = serde_json::from_str(&text).ok()?;
    if file.version != VERSION {
        return None;
    }
    Some((Store::restore(host, file.snapshot, now_ms), file.settings))
}

/// Write the state in one step: next to the file, then rename. The new file is
/// private from its first byte.
pub fn save(path: &Path, store: &Store, settings: &Settings) -> std::io::Result<()> {
    let file = File {
        version: VERSION,
        settings: settings.clone(),
        snapshot: store.snapshot(),
    };
    let json = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(&json)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}
