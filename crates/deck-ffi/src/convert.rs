//! The types the apps see. They mirror `deck-proto` one to one; the logic stays
//! in the core and these are only the face of it for Swift.

use deck_proto::agent::{
    Activity, Agent, AlwaysAllow, HistoryEntry, HistoryEvent, Location, Question, Request,
    RequestState, Status, Tool,
};
use deck_proto::message::{AnswerVia, ApprovalMode, NotifyKind, Presence, Settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ToolKind {
    Claude,
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AgentStatus {
    Working,
    Blocked,
    Done,
    Idle,
    Error,
    Exited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum RequestStateKind {
    /// Deck holds the hook; the dialog is also in the terminal.
    Held,
    /// Waits in the terminal.
    PassedThrough,
    Answered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ApprovalModeKind {
    Deck,
    Auto,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum NotifyKindView {
    /// With allow and deny buttons.
    Held,
    /// Without buttons: the agent waits in the terminal.
    InTerminal,
    Done,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum LocationView {
    Herdr { pane_id: String },
    Tmux { pane: String },
    Terminal { app: Option<String> },
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AlwaysAllowView {
    pub index: u32,
    /// Rules as text, for example `Bash(curl --version)`.
    pub rules: Vec<String>,
    /// `session`, `localSettings`, `projectSettings` or `userSettings`.
    pub destination: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RequestView {
    pub id: String,
    pub tool_name: String,
    pub summary: String,
    /// Shortened: "allow" only after the full text was shown.
    pub truncated: bool,
    pub full: String,
    /// Fields the card does not show by name, as `key=value` lines.
    #[uniffi(default = [])]
    pub extra: Vec<String>,
    pub always_allow: Vec<AlwaysAllowView>,
    pub state: RequestStateKind,
    /// Unix time in milliseconds when the wait ends.
    pub deadline_ms: Option<u64>,
    /// The questions of an agent that can be answered here; empty otherwise.
    #[uniffi(default = [])]
    pub questions: Vec<QuestionView>,
}

/// One question of an agent with its choices.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuestionView {
    pub question: String,
    pub header: Option<String>,
    pub multi_select: bool,
    pub options: Vec<QuestionOptionView>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct QuestionOptionView {
    pub label: String,
    pub description: Option<String>,
}

impl From<Question> for QuestionView {
    fn from(q: Question) -> Self {
        QuestionView {
            question: q.question,
            header: q.header,
            multi_select: q.multi_select,
            options: q
                .options
                .into_iter()
                .map(|o| QuestionOptionView {
                    label: o.label,
                    description: o.description,
                })
                .collect(),
        }
    }
}

/// Where an answer was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum AnswerViaKind {
    Panel,
    Notification,
    Window,
    Cli,
    Unknown,
}

impl From<AnswerViaKind> for AnswerVia {
    fn from(v: AnswerViaKind) -> Self {
        match v {
            AnswerViaKind::Panel => AnswerVia::Panel,
            AnswerViaKind::Notification => AnswerVia::Notification,
            AnswerViaKind::Window => AnswerVia::Window,
            AnswerViaKind::Cli => AnswerVia::Cli,
            AnswerViaKind::Unknown => AnswerVia::Unknown,
        }
    }
}

/// What an agent does right now. The app words it in its own language.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ActivityView {
    Run { command: Option<String> },
    Edit { path: Option<String> },
    Read { path: Option<String> },
    Search { pattern: Option<String> },
    Subagent,
    Web,
    Compacting,
    Tool { name: String },
}

impl From<Activity> for ActivityView {
    fn from(a: Activity) -> Self {
        match a {
            Activity::Run { command } => ActivityView::Run { command },
            Activity::Edit { path } => ActivityView::Edit { path },
            Activity::Read { path } => ActivityView::Read { path },
            Activity::Search { pattern } => ActivityView::Search { pattern },
            Activity::Subagent => ActivityView::Subagent,
            Activity::Web => ActivityView::Web,
            Activity::Compacting => ActivityView::Compacting,
            Activity::Tool { name } => ActivityView::Tool { name },
        }
    }
}

/// What happened in a session.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum HistoryEventView {
    SessionStarted,
    PromptReceived,
    Activity { activity: ActivityView },
    PermissionRequested,
    Compacting,
    TurnEnded,
    TurnFailed,
    SessionEnded,
    Allowed,
    Denied,
    Answered,
}

impl From<HistoryEvent> for HistoryEventView {
    fn from(e: HistoryEvent) -> Self {
        match e {
            HistoryEvent::SessionStarted => HistoryEventView::SessionStarted,
            HistoryEvent::PromptReceived => HistoryEventView::PromptReceived,
            HistoryEvent::Activity(a) => HistoryEventView::Activity { activity: a.into() },
            HistoryEvent::PermissionRequested => HistoryEventView::PermissionRequested,
            HistoryEvent::Compacting => HistoryEventView::Compacting,
            HistoryEvent::TurnEnded => HistoryEventView::TurnEnded,
            HistoryEvent::TurnFailed => HistoryEventView::TurnFailed,
            HistoryEvent::SessionEnded => HistoryEventView::SessionEnded,
            HistoryEvent::Allowed => HistoryEventView::Allowed,
            HistoryEvent::Denied => HistoryEventView::Denied,
            HistoryEvent::Answered => HistoryEventView::Answered,
        }
    }
}

/// One line of the history of an agent.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HistoryEntryView {
    pub at_ms: u64,
    pub event: HistoryEventView,
}

impl From<HistoryEntry> for HistoryEntryView {
    fn from(e: HistoryEntry) -> Self {
        HistoryEntryView {
            at_ms: e.at_ms,
            event: e.event.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AgentView {
    pub id: String,
    pub host: String,
    pub tool: ToolKind,
    pub cwd: Option<String>,
    pub project: Option<String>,
    pub branch: Option<String>,
    pub title: Option<String>,
    pub status: AgentStatus,
    pub paused: bool,
    pub status_since_ms: u64,
    pub activity: Option<ActivityView>,
    pub last_prompt: Option<String>,
    pub context_used: Option<u64>,
    pub context_window: Option<u64>,
    pub turns: Option<u32>,
    pub subagents: u32,
    pub location: LocationView,
    pub request: Option<RequestView>,
    pub history: Vec<HistoryEntryView>,
    /// What the session cost so far, in millionths of a US dollar.
    #[uniffi(default = None)]
    pub cost_micro_usd: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct SettingsView {
    pub mode: ApprovalModeKind,
    pub wait_secs: u32,
    pub retention_days: u32,
    /// Hold and answer requests of Codex as well.
    #[uniffi(default = false)]
    pub codex_approvals: bool,
}

/// What the menu bar app measures and reports.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct PresenceReport {
    pub screen_locked: bool,
    pub idle_secs: u64,
    pub frontmost_app: Option<String>,
    pub focus_mode: bool,
    pub last_input_ms: u64,
}

impl From<Tool> for ToolKind {
    fn from(t: Tool) -> Self {
        match t {
            Tool::Claude => ToolKind::Claude,
            Tool::Codex => ToolKind::Codex,
        }
    }
}

impl From<Status> for AgentStatus {
    fn from(s: Status) -> Self {
        match s {
            Status::Working => AgentStatus::Working,
            Status::Blocked => AgentStatus::Blocked,
            Status::Done => AgentStatus::Done,
            Status::Idle => AgentStatus::Idle,
            Status::Error => AgentStatus::Error,
            Status::Exited => AgentStatus::Exited,
        }
    }
}

impl From<RequestState> for RequestStateKind {
    fn from(s: RequestState) -> Self {
        match s {
            RequestState::Held => RequestStateKind::Held,
            RequestState::PassedThrough => RequestStateKind::PassedThrough,
            RequestState::Answered => RequestStateKind::Answered,
        }
    }
}

impl From<ApprovalMode> for ApprovalModeKind {
    fn from(m: ApprovalMode) -> Self {
        match m {
            ApprovalMode::Deck => ApprovalModeKind::Deck,
            ApprovalMode::Auto => ApprovalModeKind::Auto,
            ApprovalMode::Terminal => ApprovalModeKind::Terminal,
        }
    }
}

impl From<ApprovalModeKind> for ApprovalMode {
    fn from(m: ApprovalModeKind) -> Self {
        match m {
            ApprovalModeKind::Deck => ApprovalMode::Deck,
            ApprovalModeKind::Auto => ApprovalMode::Auto,
            ApprovalModeKind::Terminal => ApprovalMode::Terminal,
        }
    }
}

impl From<NotifyKind> for NotifyKindView {
    fn from(k: NotifyKind) -> Self {
        match k {
            NotifyKind::Held => NotifyKindView::Held,
            NotifyKind::InTerminal => NotifyKindView::InTerminal,
            NotifyKind::Done => NotifyKindView::Done,
        }
    }
}

impl From<Location> for LocationView {
    fn from(l: Location) -> Self {
        match l {
            Location::Herdr { pane_id } => LocationView::Herdr { pane_id },
            Location::Tmux { pane } => LocationView::Tmux { pane },
            Location::Terminal { app } => LocationView::Terminal { app },
            Location::Unknown => LocationView::Unknown,
        }
    }
}

impl From<AlwaysAllow> for AlwaysAllowView {
    fn from(a: AlwaysAllow) -> Self {
        AlwaysAllowView {
            index: u32::try_from(a.index).unwrap_or(u32::MAX),
            rules: a.rules,
            destination: a.destination,
        }
    }
}

impl From<Request> for RequestView {
    fn from(r: Request) -> Self {
        RequestView {
            id: r.id,
            tool_name: r.tool_name,
            summary: r.summary,
            truncated: r.truncated,
            full: r.full,
            extra: r.extra,
            always_allow: r.always_allow.into_iter().map(Into::into).collect(),
            state: r.state.into(),
            deadline_ms: r.deadline_ms,
            questions: r.questions.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<Agent> for AgentView {
    fn from(a: Agent) -> Self {
        AgentView {
            id: a.id,
            host: a.host,
            tool: a.tool.into(),
            cwd: a.cwd,
            project: a.project,
            branch: a.branch,
            title: a.title,
            status: a.status.into(),
            paused: a.paused,
            status_since_ms: a.status_since_ms,
            activity: a.activity.map(Into::into),
            last_prompt: a.last_prompt,
            context_used: a.context.as_ref().map(|c| c.used_tokens),
            context_window: a.context.as_ref().map(|c| c.window_tokens),
            turns: a.turns,
            subagents: a.subagents,
            location: a.location.into(),
            request: a.request.map(Into::into),
            history: a.history.into_iter().map(Into::into).collect(),
            cost_micro_usd: a.cost_micro_usd,
        }
    }
}

impl From<Settings> for SettingsView {
    fn from(s: Settings) -> Self {
        SettingsView {
            mode: s.mode.into(),
            wait_secs: s.wait_secs,
            retention_days: s.retention_days,
            codex_approvals: s.codex_approvals,
        }
    }
}

impl From<SettingsView> for Settings {
    fn from(s: SettingsView) -> Self {
        Settings {
            mode: s.mode.into(),
            wait_secs: s.wait_secs,
            retention_days: s.retention_days,
            codex_approvals: s.codex_approvals,
        }
    }
}

impl From<PresenceReport> for Presence {
    fn from(p: PresenceReport) -> Self {
        Presence {
            screen_locked: p.screen_locked,
            idle_secs: p.idle_secs,
            frontmost_app: p.frontmost_app,
            focus_mode: p.focus_mode,
            last_input_ms: p.last_input_ms,
        }
    }
}
