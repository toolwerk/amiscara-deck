//! The state across restarts: one small file in the data folder.
//!
//! It holds what a person would miss after a restart: the agents with their
//! last prompt and activity, the settings, and the counter for request ids.
//! Open requests do not survive: the hook programs that waited for them are
//! gone, so they come back as "waits in the terminal" (see `Store::restore`).
//! The file holds prompts and commands, so it is private (0600) like the folder.

use deck_proto::fsperm;
use std::io::Write;
use std::path::Path;

use deck_proto::message::Settings;
use deck_session::{Snapshot, Store};
use serde::{Deserialize, Serialize};

/// Raised when the file can no longer be read by an older daemon.
/// 2: activity and history are data, no longer German sentences.
pub const VERSION: u32 = 2;

/// The version whose agents cannot be read any more, but whose settings can.
const SETTINGS_ONLY: u32 = 1;

pub const FILE_NAME: &str = "state.json";

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    settings: Settings,
    #[serde(flatten)]
    snapshot: Snapshot,
}

/// Only what every version has: the version and the settings.
#[derive(Deserialize)]
struct Head {
    version: u32,
    settings: Settings,
    #[serde(default)]
    next_request: u64,
}

/// Read the state. `None` for a missing, unreadable, garbled or newer file:
/// the daemon then starts empty and the next save replaces it. A file of
/// version 1 keeps its settings and its counter of request ids; its agents and
/// their history are in the old form and start empty, an agent comes back with
/// its next event.
pub fn load(path: &Path, host: &str, now_ms: u64) -> Option<(Store, Settings)> {
    let text = std::fs::read_to_string(path).ok()?;
    let head: Head = serde_json::from_str(&text).ok()?;
    match head.version {
        VERSION => {
            let file: File = serde_json::from_str(&text).ok()?;
            Some((Store::restore(host, file.snapshot, now_ms), file.settings))
        }
        // The counter goes on: an old notification or command must not hit a
        // new request with the same id.
        SETTINGS_ONLY => Some((
            Store::restore(
                host,
                Snapshot {
                    next_request: head.next_request,
                    agents: Vec::new(),
                },
                now_ms,
            ),
            head.settings,
        )),
        _ => None,
    }
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
    let mut f = fsperm::create_mode(
        std::fs::OpenOptions::new().write(true).create_new(true),
        fsperm::PRIVATE_FILE,
    )
    .open(&tmp)?;
    fsperm::set_mode(&f, fsperm::PRIVATE_FILE)?;
    f.write_all(&json)?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}
