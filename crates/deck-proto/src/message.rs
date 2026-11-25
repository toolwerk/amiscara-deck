//! Messages between hook program, daemon and UIs.
//!
//! One JSON object per line. The first message of every connection carries the
//! protocol number; a daemon and a UI that do not match must not talk (the app
//! restarts the daemon). No message holds a path that only works locally.

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{Error, PROTOCOL_VERSION, agent::Agent};

/// Process in the chain of parents of the hook program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parent {
    pub pid: u32,
    pub name: String,
}

/// What the hook program adds to the input of Claude Code or Codex.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookEnv {
    pub herdr_pane_id: Option<String>,
    pub tmux_pane: Option<String>,
    pub term_program: Option<String>,
    pub parents: Vec<Parent>,
}

/// Hook program to daemon. The daemon parses `input` with `hook::HookInput`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookMessage {
    pub protocol: u32,
    /// Which agent called: `claude` or `codex`.
    pub tool: crate::agent::Tool,
    pub env: HookEnv,
    /// The raw input as the agent sent it.
    pub input: Value,
}

/// Daemon to hook program. Only PermissionRequest waits for it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookReply {
    /// What to print on standard output. `None`: print nothing, exit 0 (the
    /// dialog in the terminal decides).
    pub output: Option<Value>,
}

/// What the user decided on a request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    /// `always`: index of an `AlwaysAllow` of the request.
    /// `expanded`: the UI showed the user the full text of the request. The
    /// daemon refuses "allow" for a shortened card without it (decision E4):
    /// nothing may be allowed that was not shown in full.
    Allow {
        always: Option<usize>,
        #[serde(default)]
        expanded: bool,
    },
    Deny {
        message: Option<String>,
    },
    /// The answer to a question: for each question, in order, the indexes of
    /// the picked options. A question is never answered by `Allow`.
    Answer {
        picks: Vec<Vec<usize>>,
    },
}

/// The three positions of the switch (Detailkonzept, section 7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// Always notify.
    Deck,
    Auto,
    /// Never hold.
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub mode: ApprovalMode,
    /// How long the daemon holds a request, in seconds (30 to 540).
    pub wait_secs: u32,
    /// How many days history and the log of decisions are kept (1 to 365).
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// Hold and answer requests of Codex as well (off by default). Codex shows
    /// its dialog only after the hook ends, so Deck holds its requests only
    /// when it would notify about them (docs/befunde-m5.md).
    #[serde(default)]
    pub codex_approvals: bool,
}

fn default_retention_days() -> u32 {
    7
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mode: ApprovalMode::Auto,
            wait_secs: 120,
            retention_days: 7,
            codex_approvals: false,
        }
    }
}

/// Measured by the menu bar app and reported to the daemon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Presence {
    pub screen_locked: bool,
    pub idle_secs: u64,
    /// Path of the bundle of the frontmost app (`/Applications/Ghostty.app`).
    /// The daemon compares it with the processes above an agent.
    pub frontmost_app: Option<String>,
    /// Focus mode such as "Do not disturb" is on.
    pub focus_mode: bool,
    /// Unix time in milliseconds of the last input on this UI's machine.
    pub last_input_ms: u64,
}

/// UI to daemon.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum UiMessage {
    Hello {
        protocol: u32,
        name: String,
    },
    Answer {
        request_id: String,
        decision: Decision,
        /// Where the user answered, for the log of decisions.
        #[serde(default)]
        via: AnswerVia,
    },
    Presence(Presence),
    SetSettings(Settings),
    /// Delete the history of all sessions and the log of decisions.
    ClearHistory,
}

/// Where a decision was made.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerVia {
    Panel,
    Notification,
    Window,
    Cli,
    #[default]
    Unknown,
}

/// Why the daemon wants a notification shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifyKind {
    /// With "allow" and "deny" buttons.
    Held,
    /// Without buttons: the agent waits in the terminal.
    InTerminal,
    Done,
}

/// Daemon to UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DaemonMessage {
    /// Answer to `Hello`: the full state.
    Welcome {
        protocol: u32,
        host: String,
        settings: Settings,
        agents: Vec<Agent>,
        /// What today cost so far, in millionths of a US dollar: an estimate
        /// of Claude Code. Absent from an older daemon.
        #[serde(default)]
        today_micro_usd: Option<u64>,
    },
    /// Protocol numbers differ; the daemon closes the connection.
    VersionMismatch {
        daemon: u32,
        ui: u32,
    },
    AgentChanged(Box<Agent>),
    AgentRemoved {
        id: String,
    },
    SettingsChanged(Settings),
    /// A notification shown earlier is no longer needed (the user came back to
    /// the terminal). Goes to the UI that got the `Notify`.
    Withdraw {
        request_id: String,
    },
    /// Only the UI where the user is present gets this.
    Notify {
        agent_id: String,
        request_id: Option<String>,
        kind: NotifyKind,
    },
    /// The cost of today changed.
    Costs {
        today_micro_usd: u64,
    },
    /// A message of a newer daemon this program does not know. It is ignored.
    #[serde(other)]
    Unknown,
}

/// Encode one message as a line of JSON ending in a newline.
pub fn encode_line<T: Serialize>(msg: &T) -> Result<String, Error> {
    let mut s = serde_json::to_string(msg)?;
    s.push('\n');
    Ok(s)
}

/// Decode one line. Unknown fields are ignored.
pub fn decode_line<T: DeserializeOwned>(line: &str) -> Result<T, Error> {
    Ok(serde_json::from_str(line.trim_end())?)
}

/// First check of a connection: do both sides speak the same protocol?
pub fn check_protocol(theirs: u32) -> Result<(), Error> {
    if theirs == PROTOCOL_VERSION {
        Ok(())
    } else {
        Err(Error::ProtocolMismatch {
            ours: PROTOCOL_VERSION,
            theirs,
        })
    }
}
