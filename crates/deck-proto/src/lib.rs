//! Types shared by hook program, daemon and UIs: hook inputs, agent, request, messages.

pub mod agent;
pub mod build;
pub mod hook;
pub mod message;
pub mod paths;
pub mod text;

/// Version of the protocol between hook program, daemon and UIs.
/// Raised on every incompatible change; the app restarts a daemon that does not match.
pub const PROTOCOL_VERSION: u32 = 2;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("input has no hook_event_name")]
    MissingEventName,
    #[error("protocol mismatch: ours {ours}, theirs {theirs}")]
    ProtocolMismatch { ours: u32, theirs: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_version_is_set() {
        const { assert!(PROTOCOL_VERSION >= 1) };
    }
}
