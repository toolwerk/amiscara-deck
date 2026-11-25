//! The state of one agent as daemon and UIs see it (Detailkonzept, section 5).
//!
//! Nothing here depends on the machine it was made on: the host name is part
//! of the state, so a UI can show agents from several machines (M4).

use serde::{Deserialize, Serialize};

/// Which coding agent runs the session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    Claude,
    Codex,
}

/// Same terms as herdr and Amiscara Agents, so the family stays uniform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Working,
    /// Waits for an answer, in Deck or in the terminal.
    Blocked,
    Done,
    /// No task.
    Idle,
    Error,
    Exited,
}

/// Where the agent runs, for the match between agent and terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Location {
    Herdr { pane_id: String },
    Tmux { pane: String },
    Terminal { app: Option<String> },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextUsage {
    pub used_tokens: u64,
    pub window_tokens: u64,
}

/// A rule Claude Code offers for "always allow" (only `addRules` suggestions).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlwaysAllow {
    /// Index into the suggestions the daemon keeps. The UI sends it back.
    pub index: usize,
    /// Rules as text, for example `Bash(curl --version)`.
    pub rules: Vec<String>,
    /// `session`, `localSettings`, `projectSettings` or `userSettings`.
    pub destination: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestState {
    /// Deck holds the hook; the dialog is also in the terminal.
    Held,
    /// Deck did not hold it, or the time ran out. Answer in the terminal.
    PassedThrough,
    /// Answered in Deck. The card goes away with the next event of the agent.
    Answered,
}

/// An open permission request. At most one per agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Request {
    /// Own id, because PermissionRequest has no `tool_use_id`.
    pub id: String,
    pub tool_name: String,
    /// What the card shows: the command, the path, or a cut-down input.
    pub summary: String,
    /// True if `summary` is shortened. Then "allow" only comes after expanding.
    pub truncated: bool,
    /// The full text, for the main window.
    pub full: String,
    /// The lines of `full` (counted from 0) that are section headings such as
    /// `--- old ---`. The UI translates only these: a line of the same look
    /// inside a file or command stays as it is.
    #[serde(default)]
    pub headings: Vec<u32>,
    /// Fields of the input the card does not show by name, as `key=value`
    /// lines. The UI shows them after the full text under a heading of its own
    /// language. Not empty: the card is shortened.
    #[serde(default)]
    pub extra: Vec<String>,
    pub always_allow: Vec<AlwaysAllow>,
    pub state: RequestState,
    /// Unix time in milliseconds when the wait ends.
    pub deadline_ms: Option<u64>,
    /// The questions of an `AskUserQuestion` that can be answered here,
    /// escaped for display. Empty for every other request.
    #[serde(default)]
    pub questions: Vec<Question>,
}

/// One question of an agent with its choices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub question: String,
    pub header: Option<String>,
    /// More than one choice may be picked.
    pub multi_select: bool,
    pub options: Vec<QuestionOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: Option<String>,
}

/// What an agent does right now. The daemon only says what and with what;
/// every UI words it in its own language, so an agent of another machine reads
/// like one of this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Activity {
    /// Runs a command (shortened to one line).
    Run {
        command: Option<String>,
    },
    /// Writes or edits a file (relative to the working folder when inside).
    Edit {
        path: Option<String>,
    },
    Read {
        path: Option<String>,
    },
    /// Searches for a pattern (shortened to one line).
    Search {
        pattern: Option<String>,
    },
    Subagent,
    /// Fetches or searches the web.
    Web,
    /// Compacts its context.
    Compacting,
    /// Any other tool, by its name.
    Tool {
        name: String,
    },
}

/// What happened in a session, for its history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum HistoryEvent {
    SessionStarted,
    PromptReceived,
    Activity(Activity),
    PermissionRequested,
    Compacting,
    TurnEnded,
    TurnFailed,
    SessionEnded,
    /// A request was allowed in Deck.
    Allowed,
    Denied,
    /// A question was answered in Deck.
    Answered,
}

/// One line of what an agent did, for the history of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix time in milliseconds.
    pub at_ms: u64,
    pub event: HistoryEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// `session_id`.
    pub id: String,
    /// Name of the machine the agent runs on.
    pub host: String,
    pub tool: Tool,
    pub cwd: Option<String>,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub title: Option<String>,
    pub status: Status,
    /// Done while background tasks still run.
    pub paused: bool,
    /// Unix time in milliseconds of the last status change.
    pub status_since_ms: u64,
    pub activity: Option<Activity>,
    pub last_prompt: Option<String>,
    pub context: Option<ContextUsage>,
    pub turns: Option<u32>,
    pub subagents: u32,
    pub location: Location,
    pub request: Option<Request>,
    /// The last things that happened in this session, oldest first, kept for
    /// the number of days in the settings.
    #[serde(default)]
    pub history: Vec<HistoryEntry>,
    /// What the session cost so far, in millionths of a US dollar: an estimate
    /// of Claude Code, read from the transcript.
    #[serde(default)]
    pub cost_micro_usd: Option<u64>,
}
