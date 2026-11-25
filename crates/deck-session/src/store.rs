use std::collections::BTreeMap;

use deck_proto::agent::{
    Activity, Agent, ContextUsage, HistoryEntry, HistoryEvent, Location, Request, RequestState,
    Status, Tool,
};
use deck_proto::hook::{HookEvent, HookInput, PermissionAnswer};
use deck_proto::message::Decision;
use deck_proto::text::show_deceptive;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::activity::describe_tool;
use crate::request::{
    QUESTION_TOOL, always_allow, answered_input, display_questions, never_held, parse_questions,
    render,
};

/// How many lines of history an agent keeps.
pub const HISTORY_MAX: usize = 30;
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// How long an ended agent stays visible: 10 minutes.
pub const EXITED_VISIBLE_MS: u64 = 10 * 60 * 1000;

/// Shown to the model when a request is denied in Deck.
pub const DENY_MESSAGE: &str = "Im Amiscara Deck abgelehnt";

/// One hook event plus what only the daemon can know.
#[derive(Debug, Clone)]
pub struct Incoming {
    pub input: HookInput,
    pub tool: Tool,
    /// Unix time in milliseconds.
    pub now_ms: u64,
    /// Where the agent runs, from the environment of the hook program.
    pub location: Location,
    /// Branch read from `.git/HEAD` by the daemon, if known.
    pub branch: Option<String>,
    /// The daemon wants to hold a PermissionRequest: a UI is connected and the
    /// switch is not on "always in the terminal" (decision E13). A Codex
    /// request is only held if its approvals are switched on (M5).
    pub hold: bool,
    /// How long to hold, in milliseconds.
    pub wait_ms: u64,
}

/// A change the daemon has to pass on.
#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    /// UIs learn the new state of an agent.
    Changed(Box<Agent>),
    Removed {
        id: String,
    },
    /// End the waiting `deck-hook` of this request without a decision. The
    /// dialog in the terminal stays or was already answered.
    Release {
        request_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerError {
    UnknownRequest,
    /// Already passed through, answered or closed.
    NotHeld,
    /// "Always allow" with an index the request does not offer.
    BadSuggestion,
    /// "Allow" for a shortened card whose full text was not shown (E4).
    NotExpanded,
    /// "Allow" for a question: it needs an answer.
    NeedsAnswer,
    /// An answer for something that is no question, or picks that do not fit.
    BadAnswer,
}

/// How a request was answered, for the log of decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerKind {
    Allow,
    /// A question was answered.
    Answer,
    /// Allowed, and a rule was added.
    AllowAlways,
    Deny,
}

/// What `answer` returns: the output for the waiting hook and the changes.
#[derive(Debug, Clone, PartialEq)]
pub struct Answered {
    pub request_id: String,
    pub agent_id: String,
    pub project: Option<String>,
    pub tool_name: String,
    /// The whole text the user decided on (escaped like on the card), with
    /// the extra fields after it.
    pub full: String,
    pub kind: AnswerKind,
    /// The rules of "always allow", as shown on the card.
    pub rules: Vec<String>,
    /// "Question = choice" lines of an answered question.
    pub answers: Vec<String>,
    pub at_ms: u64,
    /// What `deck-hook` prints.
    pub output: Value,
    pub changes: Vec<Change>,
}

/// What the store keeps of a request besides what UIs see.
#[derive(Debug, Clone)]
struct Pending {
    request_id: String,
    /// `agent_id` of the subagent that asked, `None` for the main agent.
    origin: Option<String>,
    /// The suggestions as Claude Code sent them, for `updatedPermissions`.
    raw_suggestions: Vec<Value>,
    /// The input of a question, to answer it with.
    question_input: Option<Value>,
}

/// What a daemon keeps across a restart. Open requests do not survive: the hook
/// programs that waited for them are gone, so on restore they wait in the terminal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Next number for a request id: ids are never used twice, so a UI that
    /// answers an old card cannot hit a new request.
    pub next_request: u64,
    pub agents: Vec<Agent>,
}

/// All agents of this machine.
#[derive(Debug, Clone)]
pub struct Store {
    host: String,
    agents: BTreeMap<String, Agent>,
    /// By agent id. At most one per agent.
    pending: BTreeMap<String, Pending>,
    next_request: u64,
    /// Time of the last event or tick: `answer` has no clock of its own.
    clock_ms: u64,
    /// How many days a line of history is kept.
    retention_days: u32,
    /// Requests of Codex may be held and answered (a setting, off by default).
    codex_approvals: bool,
}

fn project_of(cwd: &str) -> Option<String> {
    cwd.trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
}

/// Remove the request of `a`. A held one has a waiting hook: release it.
/// The prompt as a person reads it. A shell command typed into Claude Code
/// (`! cmd`) arrives wrapped in tags with the whole output and terminal
/// control codes: only the command is kept. Hidden characters are shown.
fn clean_prompt(raw: &str) -> String {
    let command = raw
        .strip_prefix("<bash-input>")
        .and_then(|rest| rest.split_once("</bash-input>"))
        .map(|(cmd, _)| format!("! {cmd}"));
    let text = command
        .as_deref()
        .unwrap_or(raw)
        .replace('\r', "")
        .replace('\t', " ");
    show_deceptive(&text)
}

/// Add a line to the history: consecutive duplicates collapse, old lines go.
fn push_history(h: &mut Vec<HistoryEntry>, at_ms: u64, event: HistoryEvent, retention_days: u32) {
    if h.last().is_some_and(|l| l.event == event) {
        return;
    }
    h.push(HistoryEntry { at_ms, event });
    prune_history(h, at_ms, retention_days);
}

fn prune_history(h: &mut Vec<HistoryEntry>, now_ms: u64, retention_days: u32) {
    let limit = now_ms.saturating_sub(u64::from(retention_days) * DAY_MS);
    h.retain(|e| e.at_ms >= limit);
    if h.len() > HISTORY_MAX {
        h.drain(..h.len() - HISTORY_MAX);
    }
}

fn end_request(a: &mut Agent, pending: &mut BTreeMap<String, Pending>, released: &mut Vec<String>) {
    if let Some(r) = a.request.take()
        && r.state == RequestState::Held
    {
        released.push(r.id);
    }
    pending.remove(&a.id);
}

impl Store {
    pub fn new(host: impl Into<String>) -> Self {
        Store {
            host: host.into(),
            agents: BTreeMap::new(),
            pending: BTreeMap::new(),
            next_request: 1,
            clock_ms: 0,
            retention_days: 7,
            codex_approvals: false,
        }
    }

    /// Set how long history is kept (days) and drop what is older.
    pub fn set_retention(&mut self, days: u32, now_ms: u64) -> Vec<Change> {
        self.retention_days = days;
        self.clock_ms = self.clock_ms.max(now_ms);
        self.prune(now_ms)
    }

    fn prune(&mut self, now_ms: u64) -> Vec<Change> {
        let days = self.retention_days;
        let mut changes = Vec::new();
        for a in self.agents.values_mut() {
            let before = a.history.len();
            prune_history(&mut a.history, now_ms, days);
            if a.history.len() != before {
                changes.push(Change::Changed(Box::new(a.clone())));
            }
        }
        changes
    }

    /// Whether requests of Codex may be held. Turning it off lets go of every
    /// Codex request that is held.
    pub fn set_codex_approvals(&mut self, on: bool) -> Vec<Change> {
        self.codex_approvals = on;
        if on {
            return Vec::new();
        }
        let held: Vec<String> = self
            .agents
            .values()
            .filter(|a| a.tool == Tool::Codex)
            .filter_map(|a| a.request.as_ref())
            .filter(|r| r.state == RequestState::Held)
            .map(|r| r.id.clone())
            .collect();
        held.iter()
            .flat_map(|id| self.release_request(id))
            .collect()
    }

    /// The context fill of an agent, read from its transcript by the daemon.
    pub fn set_context(&mut self, id: &str, usage: ContextUsage) -> Vec<Change> {
        let Some(a) = self.agents.get_mut(id) else {
            return Vec::new();
        };
        if a.context.as_ref() == Some(&usage) {
            return Vec::new();
        }
        a.context = Some(usage);
        vec![Change::Changed(Box::new(a.clone()))]
    }

    /// The cost of a session, read from its transcript by the daemon.
    pub fn set_cost(&mut self, id: &str, micro_usd: u64) -> Vec<Change> {
        let Some(a) = self.agents.get_mut(id) else {
            return Vec::new();
        };
        if a.cost_micro_usd == Some(micro_usd) {
            return Vec::new();
        }
        a.cost_micro_usd = Some(micro_usd);
        vec![Change::Changed(Box::new(a.clone()))]
    }

    /// Forget the history of every agent.
    pub fn clear_history(&mut self) -> Vec<Change> {
        let mut changes = Vec::new();
        for a in self.agents.values_mut() {
            if !a.history.is_empty() {
                a.history.clear();
                changes.push(Change::Changed(Box::new(a.clone())));
            }
        }
        changes
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            next_request: self.next_request,
            agents: self.agents(),
        }
    }

    /// A store from a snapshot. Held requests become "waits in the terminal",
    /// the host name is this machine's (it may have changed), and ended agents
    /// that were visible long enough are dropped.
    pub fn restore(host: impl Into<String>, snap: Snapshot, now_ms: u64) -> Self {
        let mut store = Store::new(host);
        store.next_request = snap.next_request.max(1);
        for mut a in snap.agents {
            a.host = store.host.clone();
            if let Some(r) = a.request.as_mut()
                && r.state == RequestState::Held
            {
                r.state = RequestState::PassedThrough;
                r.deadline_ms = None;
            }
            // Never reuse an id that a restored request already carries.
            if let Some(n) = a
                .request
                .as_ref()
                .and_then(|r| r.id.strip_prefix('r'))
                .and_then(|n| n.parse::<u64>().ok())
            {
                store.next_request = store.next_request.max(n + 1);
            }
            store.agents.insert(a.id.clone(), a);
        }
        let _ = store.expire(now_ms);
        store
    }

    pub fn get(&self, id: &str) -> Option<&Agent> {
        self.agents.get(id)
    }

    /// All agents, sorted by id.
    pub fn agents(&self) -> Vec<Agent> {
        self.agents.values().cloned().collect()
    }

    fn new_agent(&self, id: &str, tool: Tool, now_ms: u64) -> Agent {
        Agent {
            id: id.to_owned(),
            host: self.host.clone(),
            tool,
            cwd: None,
            project: None,
            branch: None,
            title: None,
            status: Status::Idle,
            paused: false,
            status_since_ms: now_ms,
            activity: None,
            last_prompt: None,
            context: None,
            turns: None,
            subagents: 0,
            location: Location::Unknown,
            request: None,
            history: Vec::new(),
            cost_micro_usd: None,
        }
    }

    /// Apply one event. Returns the changes, empty if nothing visible changed.
    /// An event for an unknown session creates the agent: hooks may be
    /// installed while a session is already running.
    pub fn apply(&mut self, inc: Incoming) -> Vec<Change> {
        let id = inc.input.common.session_id.clone();
        if matches!(inc.input.event, HookEvent::Unknown { .. }) {
            return Vec::new();
        }
        let before = self.agents.get(&id).cloned();
        let mut a = before
            .clone()
            .unwrap_or_else(|| self.new_agent(&id, inc.tool, inc.now_ms));
        let origin = inc.input.common.agent_id.clone();
        let from_subagent = origin.is_some();
        let mut released: Vec<String> = Vec::new();

        if let Some(cwd) = inc
            .input
            .common
            .cwd
            .as_ref()
            .filter(|c| a.cwd.as_ref() != Some(*c))
        {
            a.project = project_of(cwd);
            a.cwd = Some(cwd.clone());
        }
        if inc.branch.is_some() {
            a.branch = inc.branch.clone();
        }
        if inc.location != Location::Unknown {
            a.location = inc.location.clone();
        }

        let mut status = a.status;
        match &inc.input.event {
            HookEvent::SessionStart(_) => {
                // A fresh start, resume or /clear: a new run of the same session.
                a.activity = None;
                a.last_prompt = None;
                a.turns = None;
                a.subagents = 0;
                a.paused = false;
                end_request(&mut a, &mut self.pending, &mut released);
                status = Status::Idle;
            }
            HookEvent::UserPromptSubmit(p) => {
                a.last_prompt = p.prompt.as_deref().map(clean_prompt);
                a.turns = Some(a.turns.unwrap_or(0) + 1);
                a.activity = None;
                a.paused = false;
                end_request(&mut a, &mut self.pending, &mut released);
                status = Status::Working;
            }
            HookEvent::PreToolUse(t) => {
                a.activity = Some(describe_tool(&t.tool_name, &t.tool_input, a.cwd.as_deref()));
                a.paused = false;
                status = Status::Working;
            }
            HookEvent::PostToolUse(_) | HookEvent::PostToolUseFailure(_) => {
                a.paused = false;
                // The tool ran or failed: its dialog is over, whoever answered.
                // Only the event of the same (sub)agent counts. A request from
                // before a restart has no record of who asked: there a tool of
                // the main agent says that it moved on.
                let own = match self.pending.get(&id) {
                    Some(p) => p.origin == origin,
                    None => origin.is_none(),
                };
                if own {
                    end_request(&mut a, &mut self.pending, &mut released);
                }
                status = Status::Working;
            }
            HookEvent::PermissionRequest(req) => {
                // At most one request per agent: a new one replaces the old.
                end_request(&mut a, &mut self.pending, &mut released);
                let held = inc.hold
                    && (inc.tool == Tool::Claude || self.codex_approvals)
                    && !never_held(&req.tool_name);
                // A question is held only if it can be answered here.
                let questions = (req.tool_name == QUESTION_TOOL)
                    .then(|| parse_questions(&req.tool_input))
                    .flatten();
                let held = held && (req.tool_name != QUESTION_TOOL || questions.is_some());
                let shown = render(req);
                let request_id = format!("r{}", self.next_request);
                self.next_request += 1;
                a.request = Some(Request {
                    id: request_id.clone(),
                    // Shown in the card: escaped like the rest.
                    tool_name: deck_proto::text::show_deceptive(&req.tool_name),
                    summary: shown.summary,
                    truncated: shown.truncated,
                    full: shown.full,
                    headings: shown.headings,
                    extra: shown.extra,
                    always_allow: always_allow(&req.permission_suggestions),
                    state: if held {
                        RequestState::Held
                    } else {
                        RequestState::PassedThrough
                    },
                    deadline_ms: held.then(|| inc.now_ms + inc.wait_ms),
                    questions: questions
                        .as_deref()
                        .map(display_questions)
                        .unwrap_or_default(),
                });
                self.pending.insert(
                    id.clone(),
                    Pending {
                        request_id,
                        origin: origin.clone(),
                        raw_suggestions: req
                            .permission_suggestions
                            .iter()
                            .map(|s| s.raw.clone())
                            .collect(),
                        question_input: questions.is_some().then(|| req.tool_input.clone()),
                    },
                );
                status = Status::Blocked;
            }
            HookEvent::Notification(n) => match n.notification_type.as_deref() {
                // Not for a request Deck already answered: this notice may
                // come after the answer.
                Some("permission_prompt")
                    if a.request
                        .as_ref()
                        .is_none_or(|r| r.state != RequestState::Answered) =>
                {
                    status = Status::Blocked;
                }
                // 60 s after Stop: nobody came back, the agent has no task.
                Some("idle_prompt") if a.status == Status::Done => status = Status::Idle,
                _ => {}
            },
            HookEvent::SubagentStart(_) => {
                a.subagents += 1;
            }
            HookEvent::SubagentStop(_) => {
                a.subagents = a.subagents.saturating_sub(1);
            }
            HookEvent::PreCompact(_) => {
                a.activity = Some(Activity::Compacting);
                status = Status::Working;
            }
            HookEvent::PostCompact(_) => {
                a.activity = None;
            }
            HookEvent::CwdChanged(_) => {}
            HookEvent::Stop(s) => {
                // A stop of a subagent is not the end of the main agent.
                if !from_subagent {
                    a.paused = !s.background_tasks.is_empty();
                    a.activity = None;
                    end_request(&mut a, &mut self.pending, &mut released);
                    status = Status::Done;
                }
            }
            HookEvent::StopFailure(_) => {
                a.activity = None;
                end_request(&mut a, &mut self.pending, &mut released);
                status = Status::Error;
            }
            HookEvent::SessionEnd(_) => {
                a.activity = None;
                a.paused = false;
                end_request(&mut a, &mut self.pending, &mut released);
                status = Status::Exited;
            }
            HookEvent::Unknown { .. } => {}
        }
        if status != a.status {
            a.status = status;
            a.status_since_ms = inc.now_ms;
        }
        self.clock_ms = inc.now_ms;
        let note = match &inc.input.event {
            HookEvent::SessionStart(_) => Some(HistoryEvent::SessionStarted),
            HookEvent::UserPromptSubmit(_) => Some(HistoryEvent::PromptReceived),
            HookEvent::PreToolUse(_) => a.activity.clone().map(HistoryEvent::Activity),
            HookEvent::PermissionRequest(_) => Some(HistoryEvent::PermissionRequested),
            HookEvent::PreCompact(_) => Some(HistoryEvent::Compacting),
            HookEvent::Stop(_) if !from_subagent => Some(HistoryEvent::TurnEnded),
            HookEvent::StopFailure(_) => Some(HistoryEvent::TurnFailed),
            HookEvent::SessionEnd(_) => Some(HistoryEvent::SessionEnded),
            _ => None,
        };
        if let Some(event) = note {
            push_history(&mut a.history, inc.now_ms, event, self.retention_days);
        }

        let mut changes = Vec::new();
        if before.as_ref() != Some(&a) {
            self.agents.insert(id, a.clone());
            changes.push(Change::Changed(Box::new(a)));
        }
        changes.extend(
            released
                .into_iter()
                .map(|request_id| Change::Release { request_id }),
        );
        changes
    }

    /// The user decided in Deck. On success the hook gets `output` and the
    /// request stays visible as answered until the next event closes it.
    pub fn answer(
        &mut self,
        request_id: &str,
        decision: Decision,
    ) -> Result<Answered, AnswerError> {
        let (agent_id, pending) = self
            .pending
            .iter()
            .find(|(_, p)| p.request_id == request_id)
            .map(|(k, p)| (k.clone(), p.clone()))
            .ok_or(AnswerError::UnknownRequest)?;
        let agent = self
            .agents
            .get_mut(&agent_id)
            .ok_or(AnswerError::UnknownRequest)?;
        let req = agent.request.as_mut().ok_or(AnswerError::UnknownRequest)?;
        if req.state != RequestState::Held {
            return Err(AnswerError::NotHeld);
        }
        // "Allow" of a shortened card needs the full text first.
        if req.truncated
            && matches!(
                decision,
                Decision::Allow {
                    expanded: false,
                    ..
                }
            )
        {
            return Err(AnswerError::NotExpanded);
        }
        // A question and a permission take different answers.
        let is_question = pending.question_input.is_some();
        match &decision {
            Decision::Allow { .. } if is_question => return Err(AnswerError::NeedsAnswer),
            Decision::Answer { .. } if !is_question => return Err(AnswerError::BadAnswer),
            _ => {}
        }
        let mut said = Vec::new();
        let (kind, rules) = match &decision {
            Decision::Allow { always: None, .. } => (AnswerKind::Allow, Vec::new()),
            Decision::Allow {
                always: Some(i), ..
            } => (
                AnswerKind::AllowAlways,
                req.always_allow
                    .iter()
                    .find(|r| r.index == *i)
                    .map(|r| r.rules.clone())
                    .unwrap_or_default(),
            ),
            Decision::Deny { .. } => (AnswerKind::Deny, Vec::new()),
            Decision::Answer { .. } => (AnswerKind::Answer, Vec::new()),
        };
        let answer = match decision {
            Decision::Allow { always: None, .. } => PermissionAnswer::Allow {
                updated_permissions: Vec::new(),
            },
            Decision::Allow {
                always: Some(i), ..
            } => {
                // Only an offered rule may go back, unchanged.
                if !req.always_allow.iter().any(|r| r.index == i) {
                    return Err(AnswerError::BadSuggestion);
                }
                let raw = pending
                    .raw_suggestions
                    .get(i)
                    .cloned()
                    .ok_or(AnswerError::BadSuggestion)?;
                PermissionAnswer::Allow {
                    updated_permissions: vec![raw],
                }
            }
            Decision::Deny { message } => PermissionAnswer::Deny {
                message: message.unwrap_or_else(|| DENY_MESSAGE.to_owned()),
            },
            Decision::Answer { picks } => {
                let (updated_input, lines) = pending
                    .question_input
                    .as_ref()
                    .and_then(|input| answered_input(input, &picks))
                    .ok_or(AnswerError::BadAnswer)?;
                said = lines;
                PermissionAnswer::AllowWithInput { updated_input }
            }
        };
        req.state = RequestState::Answered;
        req.deadline_ms = None;
        let tool_name = req.tool_name.clone();
        // The log keeps everything that was decided on, the extra fields too.
        let full = if req.extra.is_empty() {
            req.full.clone()
        } else {
            format!(
                "{}\n--- further details ---\n{}",
                req.full,
                req.extra.join("\n")
            )
        };
        // The agent goes on. Its next event may be a while away (a long
        // command), and until then it must not look like it still waits.
        if agent.status == Status::Blocked {
            agent.status = Status::Working;
            agent.status_since_ms = self.clock_ms;
        }
        let event = match kind {
            AnswerKind::Deny => HistoryEvent::Denied,
            AnswerKind::Answer => HistoryEvent::Answered,
            _ => HistoryEvent::Allowed,
        };
        push_history(
            &mut agent.history,
            self.clock_ms,
            event,
            self.retention_days,
        );
        Ok(Answered {
            request_id: request_id.to_owned(),
            agent_id: agent.id.clone(),
            project: agent.project.clone(),
            tool_name,
            full,
            kind,
            rules,
            answers: said,
            at_ms: self.clock_ms,
            output: answer.to_json(),
            changes: vec![Change::Changed(Box::new(agent.clone()))],
        })
    }

    /// Stop holding: every held request becomes "waits in the terminal" and
    /// its hook is released. For the moment the last UI goes away (no UI, no
    /// holding) and for requests whose time ran out.
    fn release(&mut self, only_due: Option<u64>) -> Vec<Change> {
        let mut changes = Vec::new();
        for a in self.agents.values_mut() {
            let Some(r) = a.request.as_mut() else {
                continue;
            };
            let due = only_due.is_none_or(|now| r.deadline_ms.is_some_and(|d| now >= d));
            if r.state == RequestState::Held && due {
                r.state = RequestState::PassedThrough;
                r.deadline_ms = None;
                let id = r.id.clone();
                changes.push(Change::Changed(Box::new(a.clone())));
                changes.push(Change::Release { request_id: id });
            }
        }
        changes
    }

    /// The hook program of one request went away (killed because the terminal
    /// answered, or crashed): nobody waits any more. The card stays as "waits
    /// in the terminal" until the next event of the agent clears it.
    pub fn release_request(&mut self, request_id: &str) -> Vec<Change> {
        for a in self.agents.values_mut() {
            let Some(r) = a.request.as_mut() else {
                continue;
            };
            if r.id == request_id && r.state == RequestState::Held {
                r.state = RequestState::PassedThrough;
                r.deadline_ms = None;
                return vec![
                    Change::Changed(Box::new(a.clone())),
                    Change::Release {
                        request_id: request_id.to_owned(),
                    },
                ];
            }
        }
        Vec::new()
    }

    /// The last UI disconnected: nobody can answer, so nobody is kept waiting.
    pub fn release_held(&mut self) -> Vec<Change> {
        self.release(None)
    }

    /// Time passes: requests past their deadline go to the terminal, and
    /// ended agents that were visible long enough are removed.
    pub fn expire(&mut self, now_ms: u64) -> Vec<Change> {
        let mut changes = self.release(Some(now_ms));
        let gone: Vec<String> = self
            .agents
            .values()
            .filter(|a| {
                a.status == Status::Exited
                    && now_ms.saturating_sub(a.status_since_ms) >= EXITED_VISIBLE_MS
            })
            .map(|a| a.id.clone())
            .collect();
        for id in gone {
            self.agents.remove(&id);
            self.pending.remove(&id);
            changes.push(Change::Removed { id });
        }
        changes
    }
}
