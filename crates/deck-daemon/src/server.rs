use std::collections::HashMap;
use std::future::Future;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{Location, RequestState, Tool};
use deck_proto::hook::{HookEvent, HookInput};
use deck_proto::message::{AnswerVia, NotifyKind, Parent, Presence};
use deck_proto::message::{
    ApprovalMode, DaemonMessage, Decision, HookEnv, HookMessage, HookReply, Settings, UiMessage,
    check_protocol, decode_line, encode_line,
};
use deck_session::notify::{self, NotifyDecision, Report};
use deck_session::{Change, Incoming, Store};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot, watch};

use crate::costs;
use crate::decisions;
use crate::git::branch_of;
use crate::state;

/// A hook input with environment is a few kilobytes, a long Bash command or a
/// written file may be more. Anything past this is not ours.
const MAX_LINE: u64 = 4 * 1024 * 1024;
/// How long a connection may take to send its first line.
const FIRST_LINE_TIMEOUT: Duration = Duration::from_secs(5);
/// How often the daemon looks at deadlines and ended agents.
const TICK: Duration = Duration::from_secs(1);
/// The longest and shortest time a request may be held, in seconds. The hook
/// of Claude Code is installed with a timeout above the longest.
const MIN_WAIT_SECS: u32 = 1;
const MAX_WAIT_SECS: u32 = 540;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("another daemon is already listening on {0}")]
    AlreadyRunning(PathBuf),
    #[error("no data folder: set HOME or DECK_DATA_DIR")]
    NoDataDir,
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Error {
    let context = context.into();
    move |source| Error::Io { context, source }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub data_dir: PathBuf,
    /// Name of this machine, part of the state of every agent.
    pub host: String,
}

impl Config {
    /// Default folder and the name of this machine.
    pub fn from_env() -> Result<Self, Error> {
        Ok(Config {
            data_dir: crate::default_data_dir().ok_or(Error::NoDataDir)?,
            host: gethostname::gethostname().to_string_lossy().into_owned(),
        })
    }

    pub fn socket_path(&self) -> PathBuf {
        self.data_dir.join(deck_proto::paths::SOCKET_NAME)
    }
}

/// A connected UI. Lines are encoded once and queued; a UI that falls behind
/// is dropped and reconnects to get the full state again.
struct Subscriber {
    id: u64,
    tx: mpsc::Sender<String>,
}

/// How many lines may wait for a slow UI before it is dropped.
/// The further looks into a transcript after a turn ended, each after the
/// one before.
const LATE_READS: [Duration; 2] = [Duration::from_millis(700), Duration::from_secs(3)];
const LOG_PRUNE_EVERY_MS: u64 = 60 * 60 * 1000;
const QUEUE: usize = 256;

struct Inner {
    store: Store,
    settings: Settings,
    subscribers: Vec<Subscriber>,
    next_subscriber: u64,
    /// A hook program waits for each held request: its answer goes here.
    /// `None` means "print nothing, the terminal decides".
    waiters: HashMap<String, oneshot::Sender<Option<Value>>>,
    /// The state changed since it was last written.
    dirty: bool,
    /// What each UI last said about where its user is.
    reports: HashMap<u64, Report>,
    /// The processes above each agent, from the hook program. They say which
    /// app is the terminal of the agent.
    parents: HashMap<String, Vec<Parent>>,
    /// The notification shown for each request: what and where, so it is shown
    /// once, moved or taken back, and never repeated.
    notified: HashMap<String, (NotifyKind, u64)>,
    /// When the log of decisions was last pruned.
    log_pruned_ms: u64,
    /// What today cost, per session.
    costs: costs::Ledger,
}

struct Shared {
    inner: Mutex<Inner>,
    /// Set to true when the daemon stops: UIs are told by a closed connection.
    stop: watch::Sender<bool>,
    state_path: PathBuf,
    log_path: PathBuf,
    costs_path: PathBuf,
    windows_path: PathBuf,
}

/// Read the transcript of a session and pass on what it says.
async fn read_usage(shared: &Shared, session: &str, path: &str) {
    let windows = shared.windows_path.clone();
    let path = PathBuf::from(path);
    let usage = tokio::task::spawn_blocking(move || Usage {
        context: deck_usage::context_usage_with(&path, &deck_usage::load_windows(&windows)),
        cost: deck_usage::cost_state(&path),
    })
    .await;
    if let Ok(usage) = usage {
        shared.set_usage(session, usage);
    }
}

/// What was read from a transcript.
struct Usage {
    context: Option<deck_proto::agent::ContextUsage>,
    cost: Option<deck_usage::CostState>,
}

/// A hook program that waits for the answer to a request.
struct Waiting {
    request_id: String,
    answer: oneshot::Receiver<Option<Value>>,
}

impl Shared {
    /// A poisoned lock means a handler panicked; the state is still a valid
    /// store, so keep going.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// An event of an agent. If it is a PermissionRequest that the daemon
    /// holds, the caller gets the way to wait for the answer.
    fn apply(&self, mut incoming: Incoming, parents: Vec<Parent>) -> Option<Waiting> {
        let session = incoming.input.common.session_id.clone();
        let mut inner = self.lock();
        if !parents.is_empty() {
            inner.parents.insert(session.clone(), parents);
        }
        // Holding only makes sense while someone can answer and wants to.
        incoming.hold =
            !inner.subscribers.is_empty() && inner.settings.mode != ApprovalMode::Terminal;
        // Codex shows its dialog only after the hook ends: hold it only if
        // Deck would tell the user about it, else the terminal would just wait.
        if incoming.tool == Tool::Codex {
            incoming.hold = incoming.hold
                && inner.settings.codex_approvals
                && inner.would_notify(&session, &incoming.location);
        }
        incoming.wait_ms = u64::from(inner.settings.wait_secs) * 1000;
        let changes = inner.store.apply(incoming);
        let waiting = changes.iter().find_map(|c| match c {
            Change::Changed(a) if a.id == session => a
                .request
                .as_ref()
                .filter(|r| r.state == RequestState::Held && !inner.waiters.contains_key(&r.id))
                .map(|r| r.id.clone()),
            _ => None,
        });
        let waiting = waiting.map(|request_id| {
            let (tx, rx) = oneshot::channel();
            inner.waiters.insert(request_id.clone(), tx);
            Waiting {
                request_id,
                answer: rx,
            }
        });
        inner.finish(&changes);
        waiting
    }

    fn expire(&self, now_ms: u64) {
        let mut inner = self.lock();
        let changes = inner.store.expire(now_ms);
        inner.finish(&changes);
        // Midnight: today starts at nothing, and the UIs hear it.
        if inner.costs.roll(&costs::local_day(now_ms)) {
            self.costs_changed(&mut inner);
        }
        if now_ms.saturating_sub(inner.log_pruned_ms) >= LOG_PRUNE_EVERY_MS {
            inner.log_pruned_ms = now_ms;
            let _ = decisions::prune(&self.log_path, now_ms, inner.settings.retention_days);
        }
    }

    /// What the transcript of a session says: context fill and cost.
    fn set_usage(&self, session: &str, usage: Usage) {
        let mut inner = self.lock();
        if let Some(context) = usage.context {
            let changes = inner.store.set_context(session, context);
            inner.finish(&changes);
        }
        if let Some(cost) = usage.cost {
            let before = inner.costs.today();
            inner
                .costs
                .record(session, cost, &costs::local_day(now_ms()));
            let changes = inner.store.set_cost(session, cost.total_micro_usd);
            inner.finish(&changes);
            if inner.costs.today() != before {
                self.costs_changed(&mut inner);
            } else {
                let _ = costs::save(&self.costs_path, &inner.costs);
            }
        }
    }

    /// Save the ledger and tell every UI what today cost.
    fn costs_changed(&self, inner: &mut Inner) {
        let _ = costs::save(&self.costs_path, &inner.costs);
        let msg = DaemonMessage::Costs {
            today_micro_usd: inner.costs.today(),
        };
        inner.send_all(&msg);
    }

    /// The user wants the history and the log of decisions gone.
    fn clear_history(&self) {
        let mut inner = self.lock();
        let changes = inner.store.clear_history();
        inner.finish(&changes);
        let _ = decisions::clear(&self.log_path);
    }

    /// The full state and a queue for every later change, taken together: no
    /// change can fall between the two.
    fn subscribe(&self) -> (String, u64, mpsc::Receiver<String>) {
        let mut inner = self.lock();
        let (tx, rx) = mpsc::channel(QUEUE);
        let id = inner.next_subscriber;
        inner.next_subscriber += 1;
        inner.subscribers.push(Subscriber { id, tx });
        let welcome = DaemonMessage::Welcome {
            protocol: PROTOCOL_VERSION,
            host: inner.store.host().to_owned(),
            settings: inner.settings.clone(),
            agents: inner.store.agents(),
            today_micro_usd: Some(inner.costs.today()),
        };
        (encode_line(&welcome).unwrap_or_default(), id, rx)
    }

    /// A UI went away. With none left nobody can answer, so nothing is held.
    fn unsubscribe(&self, id: u64) {
        let mut inner = self.lock();
        inner.subscribers.retain(|s| s.id != id);
        inner.reports.remove(&id);
        inner.notified.retain(|_, (_, sub)| *sub != id);
        if inner.subscribers.is_empty() {
            let changes = inner.store.release_held();
            inner.finish(&changes);
        }
    }

    /// The user decided in a UI. Unknown, late or doubled answers do nothing:
    /// the state the UIs get shows what is true.
    fn answer(&self, request_id: &str, decision: Decision, via: AnswerVia) {
        let mut inner = self.lock();
        if let Ok(done) = inner.store.answer(request_id, decision) {
            let _ = decisions::append(&self.log_path, &done, via);
            inner.finish(&done.changes);
            if let Some(tx) = inner.waiters.remove(request_id) {
                let _ = tx.send(Some(done.output));
            }
        }
    }

    /// A UI tells where its user is. May make a held request worth a notification
    /// (the user left) or make one needless (the user is back at the terminal).
    fn set_presence(&self, subscriber: u64, presence: Presence) {
        let mut inner = self.lock();
        inner.reports.insert(
            subscriber,
            Report {
                presence,
                received_ms: now_ms(),
            },
        );
        inner.notify_pass();
    }

    fn set_settings(&self, new: Settings) {
        let mut inner = self.lock();
        let new = Settings {
            mode: new.mode,
            wait_secs: new.wait_secs.clamp(MIN_WAIT_SECS, MAX_WAIT_SECS),
            retention_days: new.retention_days.clamp(1, 365),
            codex_approvals: new.codex_approvals,
        };
        if inner.settings == new {
            return;
        }
        let now = now_ms();
        let pruned = inner.store.set_retention(new.retention_days, now);
        inner.finish(&pruned);
        let released = inner.store.set_codex_approvals(new.codex_approvals);
        inner.finish(&released);
        let _ = decisions::prune(&self.log_path, now, new.retention_days);
        let to_terminal =
            new.mode == ApprovalMode::Terminal && inner.settings.mode != ApprovalMode::Terminal;
        inner.settings = new.clone();
        inner.dirty = true;
        inner.notify_pass();
        let msg = DaemonMessage::SettingsChanged(new);
        inner.send_all(&msg);
        if to_terminal {
            // "Always in the terminal": what is held is let go.
            let changes = inner.store.release_held();
            inner.finish(&changes);
        }
    }

    /// The hook program of a request disappeared.
    fn hook_gone(&self, request_id: &str) {
        let mut inner = self.lock();
        inner.waiters.remove(request_id);
        let changes = inner.store.release_request(request_id);
        inner.finish(&changes);
    }

    /// Write the state if it changed. A failure keeps it dirty, so the next
    /// tick tries again.
    fn save_if_dirty(&self) {
        let mut inner = self.lock();
        if inner.dirty && state::save(&self.state_path, &inner.store, &inner.settings).is_ok() {
            inner.dirty = false;
        }
    }

    fn save_now(&self) {
        let inner = self.lock();
        let _ = state::save(&self.state_path, &inner.store, &inner.settings);
    }

    /// The daemon stops: nobody is kept waiting.
    fn release_all(&self) {
        let mut inner = self.lock();
        let changes = inner.store.release_held();
        inner.finish(&changes);
        inner.waiters.clear();
    }
}

impl Inner {
    /// Pass changes on: to the UIs, and the release of a request to its hook.
    fn finish(&mut self, changes: &[Change]) {
        if !changes.is_empty() {
            self.dirty = true;
        }
        for c in changes {
            if let Change::Release { request_id } = c
                && let Some(tx) = self.waiters.remove(request_id)
            {
                let _ = tx.send(None);
            }
        }
        let msgs: Vec<DaemonMessage> = changes
            .iter()
            .filter_map(|c| match c {
                Change::Changed(a) => Some(DaemonMessage::AgentChanged(a.clone())),
                Change::Removed { id } => Some(DaemonMessage::AgentRemoved { id: id.clone() }),
                Change::Release { .. } => None,
            })
            .collect();
        for m in &msgs {
            self.send_all(m);
        }
        self.notify_pass();
    }

    /// Queue one message for one UI.
    fn send_to(&mut self, subscriber: u64, msg: &DaemonMessage) {
        let Ok(line) = encode_line(msg) else { return };
        self.subscribers
            .retain(|sub| sub.id != subscriber || sub.tx.try_send(line.clone()).is_ok());
    }

    /// Show, move or take back the notification of every open request, from
    /// what the UIs reported (Detailkonzept, section 7). Only changes are sent.
    /// The UI where the user is. Without a fresh report anywhere the first
    /// UI gets it: Deck rather shows a notification once too often.
    fn present_ui(&self, now: u64) -> Option<u64> {
        notify::most_present(
            self.subscribers
                .iter()
                .enumerate()
                .map(|(i, s)| (i, self.reports.get(&s.id))),
            now,
        )
        .or(if self.subscribers.is_empty() {
            None
        } else {
            Some(0)
        })
        .map(|i| self.subscribers[i].id)
    }

    /// Would Deck notify about a request of this agent right now?
    fn would_notify(&self, agent_id: &str, location: &Location) -> bool {
        let now = now_ms();
        let report = self.present_ui(now).and_then(|id| self.reports.get(&id));
        let parents = self.parents.get(agent_id).map(Vec::as_slice).unwrap_or(&[]);
        let pane_visible = match location {
            Location::Tmux { pane } => crate::tmux::pane_visible(pane),
            _ => None,
        };
        notify::decide(self.settings.mode, report, parents, pane_visible, now)
            == NotifyDecision::Notify
    }

    fn notify_pass(&mut self) {
        let now = now_ms();
        let mode = self.settings.mode;
        let chosen = self.present_ui(now);
        let Some(chosen) = chosen else { return };
        let report = self.reports.get(&chosen).cloned();

        let mut open: Vec<String> = Vec::new();
        // Held Codex requests whose user came back to the terminal.
        let mut give_back: Vec<String> = Vec::new();
        let mut sends: Vec<(u64, DaemonMessage)> = Vec::new();
        for agent in self.store.agents() {
            let Some(req) = &agent.request else { continue };
            open.push(req.id.clone());
            let parents = self
                .parents
                .get(&agent.id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let wants = match req.state {
                RequestState::Answered => None,
                RequestState::Held | RequestState::PassedThrough => {
                    let pane_visible = match &agent.location {
                        Location::Tmux { pane } => crate::tmux::pane_visible(pane),
                        _ => None,
                    };
                    match notify::decide(mode, report.as_ref(), parents, pane_visible, now) {
                        NotifyDecision::Notify if req.state == RequestState::Held => {
                            Some(NotifyKind::Held)
                        }
                        NotifyDecision::Notify => Some(NotifyKind::InTerminal),
                        NotifyDecision::Quiet(_) => {
                            // Codex waits for the hook before it shows its
                            // dialog: the user is back, so let it show.
                            if agent.tool == Tool::Codex && req.state == RequestState::Held {
                                give_back.push(req.id.clone());
                            }
                            None
                        }
                    }
                }
            };
            match (wants, self.notified.get(&req.id).copied()) {
                (Some(kind), Some((k, sub))) if k == kind && sub == chosen => {}
                (Some(kind), prior) => {
                    if let Some((_, sub)) = prior
                        && sub != chosen
                    {
                        sends.push((
                            sub,
                            DaemonMessage::Withdraw {
                                request_id: req.id.clone(),
                            },
                        ));
                    }
                    sends.push((
                        chosen,
                        DaemonMessage::Notify {
                            agent_id: agent.id.clone(),
                            request_id: Some(req.id.clone()),
                            kind,
                        },
                    ));
                    self.notified.insert(req.id.clone(), (kind, chosen));
                }
                (None, Some((_, sub))) => {
                    sends.push((
                        sub,
                        DaemonMessage::Withdraw {
                            request_id: req.id.clone(),
                        },
                    ));
                    self.notified.remove(&req.id);
                }
                (None, None) => {}
            }
        }
        // Requests that are gone need no record. The UI takes its own
        // notification back when it sees the request end.
        self.notified.retain(|id, _| open.contains(id));
        for (sub, msg) in sends {
            self.send_to(sub, &msg);
        }
        for id in give_back {
            // A released request is no longer held, so this does not repeat.
            let released = self.store.release_request(&id);
            self.finish(&released);
        }
    }

    /// Queue one message for every UI. A UI that fell behind is dropped.
    fn send_all(&mut self, msg: &DaemonMessage) {
        let Ok(line) = encode_line(msg) else { return };
        self.subscribers
            .retain(|sub| sub.tx.try_send(line.clone()).is_ok());
    }
}

/// A daemon that has its socket and does not run yet.
pub struct Daemon {
    listener: UnixListener,
    socket: PathBuf,
    shared: Arc<Shared>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Daemon {
    /// Create the folder (0700), take over or refuse the socket, bind it (0600).
    pub async fn bind(config: Config) -> Result<Self, Error> {
        let dir = &config.data_dir;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(io(format!("create {}", dir.display())))?;
        // An existing folder may have been made with other rights.
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .map_err(io(format!("chmod {}", dir.display())))?;

        let socket = config.socket_path();
        if socket.exists() {
            if UnixStream::connect(&socket).await.is_ok() {
                return Err(Error::AlreadyRunning(socket));
            }
            // Left behind by a daemon that did not shut down.
            std::fs::remove_file(&socket).map_err(io(format!("remove {}", socket.display())))?;
        }
        let listener =
            UnixListener::bind(&socket).map_err(io(format!("bind {}", socket.display())))?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
            .map_err(io(format!("chmod {}", socket.display())))?;

        let state_path = dir.join(state::FILE_NAME);
        let costs_path = dir.join(costs::FILE_NAME);
        let windows_path = dir.join(deck_usage::WINDOWS_FILE);
        let mut ledger = costs::load(&costs_path);
        ledger.roll(&costs::local_day(now_ms()));
        let log_path = dir.join(decisions::FILE_NAME);
        let (mut store, mut settings) = state::load(&state_path, &config.host, now_ms())
            .unwrap_or_else(|| (Store::new(config.host.clone()), Settings::default()));
        settings.retention_days = settings.retention_days.clamp(1, 365);
        let _ = store.set_retention(settings.retention_days, now_ms());
        let _ = store.set_codex_approvals(settings.codex_approvals);
        let _ = decisions::prune(&log_path, now_ms(), settings.retention_days);

        Ok(Daemon {
            listener,
            socket,
            shared: Arc::new(Shared {
                state_path,
                log_path,
                costs_path,
                windows_path,
                inner: Mutex::new(Inner {
                    dirty: false,
                    reports: HashMap::new(),
                    parents: HashMap::new(),
                    notified: HashMap::new(),
                    log_pruned_ms: now_ms(),
                    costs: ledger,
                    store,
                    settings,
                    subscribers: Vec::new(),
                    next_subscriber: 1,
                    waiters: HashMap::new(),
                }),
                stop: watch::channel(false).0,
            }),
        })
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket
    }

    /// Serve until `shutdown` completes, then remove the socket.
    pub async fn run(self, shutdown: impl Future<Output = ()>) {
        tokio::pin!(shutdown);
        let mut tick = tokio::time::interval(TICK);
        loop {
            tokio::select! {
                _ = &mut shutdown => break,
                _ = tick.tick() => {
                    self.shared.expire(now_ms());
                    self.shared.save_if_dirty();
                }
                accepted = self.listener.accept() => {
                    let Ok((stream, _)) = accepted else { continue };
                    let shared = self.shared.clone();
                    tokio::spawn(async move { handle(stream, shared).await });
                }
            }
        }
        self.shared.release_all();
        self.shared.save_now();
        let _ = self.shared.stop.send(true);
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// Read one line of at most `MAX_LINE` bytes. `None` on timeout, close,
/// oversize or invalid UTF-8.
async fn read_line(reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> Option<String> {
    let mut buf = Vec::new();
    let mut limited = reader.take(MAX_LINE);
    let read = tokio::time::timeout(FIRST_LINE_TIMEOUT, limited.read_until(b'\n', &mut buf))
        .await
        .ok()?
        .ok()?;
    if read == 0 || buf.last() != Some(&b'\n') {
        return None;
    }
    String::from_utf8(buf).ok()
}

async fn handle(stream: UnixStream, shared: Arc<Shared>) {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read);
    let Some(line) = read_line(&mut reader).await else {
        return;
    };
    let Ok(value) = serde_json::from_str::<Value>(&line) else {
        return;
    };
    if value.get("type").and_then(Value::as_str) == Some("hello") {
        ui_session(&line, &shared, reader, write).await;
        return;
    }
    if let Some(reply) = hook(&line, &shared, &mut reader).await {
        let _ = write.write_all(reply.as_bytes()).await;
    }
    let _ = write.shutdown().await;
}

/// What a UI may tell the daemon.
fn ui_message(msg: UiMessage, shared: &Shared, subscriber: u64) {
    match msg {
        UiMessage::Answer {
            request_id,
            decision,
            via,
        } => shared.answer(&request_id, decision, via),
        UiMessage::ClearHistory => shared.clear_history(),
        UiMessage::SetSettings(s) => shared.set_settings(s),
        UiMessage::Presence(p) => shared.set_presence(subscriber, p),
        UiMessage::Hello { .. } => {}
    }
}

/// A UI: the full state first, then every change, until it goes away.
async fn ui_session(
    line: &str,
    shared: &Shared,
    mut reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    mut write: tokio::net::unix::OwnedWriteHalf,
) {
    let Ok(UiMessage::Hello { protocol, .. }) = decode_line(line) else {
        return;
    };
    if check_protocol(protocol).is_err() {
        let msg = DaemonMessage::VersionMismatch {
            daemon: PROTOCOL_VERSION,
            ui: protocol,
        };
        if let Ok(l) = encode_line(&msg) {
            let _ = write.write_all(l.as_bytes()).await;
        }
        let _ = write.shutdown().await;
        return;
    }
    let (welcome, id, mut rx) = shared.subscribe();
    if write.write_all(welcome.as_bytes()).await.is_err() {
        shared.unsubscribe(id);
        return;
    }
    let mut stop = shared.stop.subscribe();
    let mut incoming = Vec::new();
    loop {
        tokio::select! {
            _ = stop.changed() => break,
            queued = rx.recv() => {
                // `None`: the daemon dropped this UI because it fell behind.
                let Some(l) = queued else { break };
                if write.write_all(l.as_bytes()).await.is_err() {
                    break;
                }
            }
            // `read_until` keeps what it read in `incoming` when another branch
            // wins, so a line that arrives in pieces is not lost.
            read = reader.read_until(b'\n', &mut incoming) => {
                if read.map(|n| n == 0).unwrap_or(true) || incoming.len() as u64 > MAX_LINE {
                    break;
                }
                if incoming.last() == Some(&b'\n') {
                    if let Ok(text) = std::str::from_utf8(&incoming)
                        && let Ok(msg) = decode_line::<UiMessage>(text)
                    {
                        ui_message(msg, shared, id);
                    }
                    incoming.clear();
                }
            }
        }
    }
    shared.unsubscribe(id);
    let _ = write.shutdown().await;
}

fn location_of(env: &HookEnv) -> Location {
    if let Some(p) = &env.herdr_pane_id {
        Location::Herdr { pane_id: p.clone() }
    } else if let Some(p) = &env.tmux_pane {
        Location::Tmux { pane: p.clone() }
    } else if env.term_program.is_some() {
        Location::Terminal {
            app: env.term_program.clone(),
        }
    } else {
        Location::Unknown
    }
}

/// A message of the hook program. Only a held PermissionRequest waits; all
/// else is answered with "print nothing" at once.
async fn hook(
    line: &str,
    shared: &Arc<Shared>,
    reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>,
) -> Option<String> {
    let silent = encode_line(&HookReply { output: None }).ok();
    let Ok(msg) = decode_line::<HookMessage>(line) else {
        return silent;
    };
    if check_protocol(msg.protocol).is_err() {
        return silent;
    }
    let Ok(bytes) = serde_json::to_vec(&msg.input) else {
        return silent;
    };
    let Ok(input) = HookInput::parse(&bytes) else {
        return silent;
    };
    let branch = input
        .common
        .cwd
        .as_deref()
        .and_then(|c| branch_of(Path::new(c)));
    // The transcript is read after a turn ended or began, and when the session
    // ends: the usage of the last message and the cost are there by then.
    let usage_from = match (&input.event, &input.common.agent_id) {
        (HookEvent::Stop(_) | HookEvent::UserPromptSubmit(_) | HookEvent::SessionEnd(_), None) => {
            input.common.transcript_path.clone()
        }
        _ => None,
    };
    let after_turn = matches!(input.event, HookEvent::Stop(_) | HookEvent::SessionEnd(_));
    let session = input.common.session_id.clone();
    let tool = msg.tool;
    let incoming = Incoming {
        input,
        tool: msg.tool,
        now_ms: now_ms(),
        location: location_of(&msg.env),
        branch,
        hold: false,
        wait_ms: 0,
    };
    let waiting = shared.apply(incoming, msg.env.parents.clone());
    // Codex writes other transcripts: nothing to read there.
    if let Some(path) = usage_from.filter(|_| tool == Tool::Claude) {
        read_usage(shared, &session, &path).await;
        if after_turn {
            // Claude Code writes the last message of a turn after it called
            // the hook: look again a little later.
            let shared = shared.clone();
            let session = session.clone();
            tokio::spawn(async move {
                for pause in LATE_READS {
                    tokio::time::sleep(pause).await;
                    read_usage(&shared, &session, &path).await;
                }
            });
        }
    }
    let Some(waiting) = waiting else {
        return silent;
    };
    // Held: wait for an answer, the end of the time, a release, or the hook
    // program going away (then nobody needs an answer).
    let output = tokio::select! {
        answer = waiting.answer => answer.ok().flatten(),
        () = connection_closed(reader) => {
            shared.hook_gone(&waiting.request_id);
            None
        }
    };
    encode_line(&HookReply { output }).ok()
}

/// Resolves when the other side closes. The hook program sends nothing more
/// after its line, so anything that arrives is ignored.
async fn connection_closed(reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>) {
    let mut buf = [0u8; 256];
    loop {
        match reader.read(&mut buf).await {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
    }
}
