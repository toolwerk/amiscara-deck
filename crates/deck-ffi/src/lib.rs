//! The Swift face of the Deck core, via UniFFI.
//!
//! No logic lives here: state, decisions and the protocol are in the other
//! crates. This crate only turns them into types and calls Swift can use.
//!
//! The app opens one [`DeckConnection`] to the daemon and gets every change
//! through a [`DeckObserver`]. Callbacks come on a background thread; the app
//! hops to its UI thread itself. `connect` waits for the daemon's answer, so
//! call it off the main thread.

mod convert;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use deck_client::Client;
use deck_proto::message::{DaemonMessage, Decision, UiMessage};
use tokio::runtime::Runtime;
use tokio::sync::mpsc;

pub use convert::{
    AgentStatus, AgentView, AlwaysAllowView, AnswerViaKind, ApprovalModeKind, HistoryEntryView,
    LocationView, NotifyKindView, PresenceReport, QuestionOptionView, QuestionView,
    RequestStateKind, RequestView, SettingsView, ToolKind,
};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum DeckError {
    /// No daemon at the socket: not started, or started elsewhere.
    #[error("no daemon at {path}")]
    NoDaemon { path: String },
    #[error("the daemon speaks protocol {daemon}, this app {ours}")]
    VersionMismatch { daemon: u32, ours: u32 },
    #[error("{message}")]
    Failed { message: String },
    /// The other machine presented a key nobody has trusted yet. Show the
    /// fingerprint, ask, and trust `key` with `trust_remote_host_key`.
    #[error("unknown host key for {host}: {fingerprint}")]
    UnknownHostKey {
        host: String,
        fingerprint: String,
        key: String,
    },
    /// The machine presented another key than the trusted one. Never trusted
    /// by the app: the user fixes `known_hosts` by hand.
    #[error("host key of {host} changed, it is now {fingerprint}")]
    HostKeyChanged { host: String, fingerprint: String },
}

impl From<deck_client::Error> for DeckError {
    fn from(e: deck_client::Error) -> Self {
        match e {
            deck_client::Error::NoDaemon(p) => DeckError::NoDaemon {
                path: p.to_string_lossy().into_owned(),
            },
            deck_client::Error::VersionMismatch { daemon, ours } => {
                DeckError::VersionMismatch { daemon, ours }
            }
            other => DeckError::Failed {
                message: other.to_string(),
            },
        }
    }
}

impl From<deck_remote::Error> for DeckError {
    fn from(e: deck_remote::Error) -> Self {
        use deck_remote::Error as E;
        match e {
            E::Client(c) => c.into(),
            E::Ssh(amiscara_ssh::SshError::UnknownHostKey {
                host,
                key,
                fingerprint,
            }) => DeckError::UnknownHostKey {
                host,
                fingerprint,
                key,
            },
            E::Ssh(amiscara_ssh::SshError::HostKeyChanged {
                host, fingerprint, ..
            }) => DeckError::HostKeyChanged { host, fingerprint },
            other => DeckError::Failed {
                message: other.to_string(),
            },
        }
    }
}

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("deck-ffi")
            .enable_all()
            .build()
            // Without a runtime nothing in this library works; there is no
            // way to recover and no caller to tell.
            .unwrap_or_else(|e| panic!("cannot start the runtime: {e}"))
    })
}

/// What the app implements to hear from the daemon.
#[uniffi::export(with_foreign)]
pub trait DeckObserver: Send + Sync {
    /// The full state, right after connecting.
    fn on_state(&self, host: String, settings: SettingsView, agents: Vec<AgentView>);
    fn on_agent_changed(&self, agent: AgentView);
    fn on_agent_removed(&self, id: String);
    fn on_settings_changed(&self, settings: SettingsView);
    /// The daemon wants a notification shown here.
    fn on_notify(&self, agent_id: String, request_id: Option<String>, kind: NotifyKindView);
    /// A notification shown earlier is no longer needed: take it back.
    fn on_withdraw(&self, request_id: String);
    /// What today cost, in millionths of a US dollar (an estimate).
    fn on_costs(&self, today_micro_usd: u64);
    /// The connection ended; `reason` is a short text for the log. The app
    /// reconnects, with a pause.
    fn on_disconnected(&self, reason: String);
}

enum Command {
    Send(UiMessage),
    Close,
}

/// One connection to the daemon.
#[derive(uniffi::Object)]
pub struct DeckConnection {
    commands: mpsc::UnboundedSender<Command>,
    /// The fingerprint the daemon told in its welcome.
    daemon_build: Option<String>,
}

#[uniffi::export]
impl DeckConnection {
    /// Connect, say hello and start following. Blocks until the daemon has
    /// answered (a moment), so call it off the main thread. The observer gets
    /// the full state first.
    #[uniffi::constructor]
    pub fn connect(
        socket_path: String,
        name: String,
        observer: Arc<dyn DeckObserver>,
    ) -> Result<Arc<Self>, DeckError> {
        let client = runtime().block_on(Client::connect(Path::new(&socket_path), &name))?;
        Ok(start(client, None, observer))
    }

    /// The same for the daemon of another machine, over SSH. Blocks until the
    /// daemon has answered, which takes longer: call it off the main thread.
    /// An unknown host key ends in `UnknownHostKey`; trust it with
    /// `trust_remote_host_key` once the user agreed and connect again.
    #[uniffi::constructor]
    pub fn connect_remote(
        config: RemoteConfig,
        name: String,
        observer: Arc<dyn DeckObserver>,
    ) -> Result<Arc<Self>, DeckError> {
        let target = config.target()?;
        let remote = runtime().block_on(deck_remote::connect(
            &target,
            config.socket.as_deref(),
            &name,
        ))?;
        let (client, keep) = remote.into_parts();
        Ok(start(client, Some(keep), observer))
    }

    /// Allow a held request. `always` is the index of an offered rule.
    /// `expanded` says the user was shown the full text; the daemon refuses
    /// to allow a shortened request without it.
    pub fn answer_allow(
        &self,
        request_id: String,
        always: Option<u32>,
        expanded: bool,
        via: AnswerViaKind,
    ) {
        self.send(UiMessage::Answer {
            request_id,
            decision: Decision::Allow {
                always: always.map(|i| i as usize),
                expanded,
            },
            via: via.into(),
        });
    }

    pub fn answer_deny(&self, request_id: String, message: Option<String>, via: AnswerViaKind) {
        self.send(UiMessage::Answer {
            request_id,
            decision: Decision::Deny { message },
            via: via.into(),
        });
    }

    /// Answer a question: for each question, in order, the indexes of the
    /// picked options.
    pub fn answer_question(&self, request_id: String, picks: Vec<Vec<u32>>, via: AnswerViaKind) {
        self.send(UiMessage::Answer {
            request_id,
            decision: Decision::Answer {
                picks: picks
                    .into_iter()
                    .map(|q| q.into_iter().map(|i| i as usize).collect())
                    .collect(),
            },
            via: via.into(),
        });
    }

    /// The fingerprint of the daemon this connection talks to.
    pub fn daemon_build(&self) -> Option<String> {
        self.daemon_build.clone()
    }

    /// Ask the daemon to end cleanly.
    pub fn shutdown_daemon(&self) {
        self.send(UiMessage::Shutdown);
    }

    /// Forget the history of every agent and the log of decisions.
    pub fn clear_history(&self) {
        self.send(UiMessage::ClearHistory);
    }

    pub fn set_settings(&self, settings: SettingsView) {
        self.send(UiMessage::SetSettings(settings.into()));
    }

    /// Tell the daemon where the user is (frontmost app, idle time, lock).
    pub fn report_presence(&self, presence: PresenceReport) {
        self.send(UiMessage::Presence(presence.into()));
    }

    /// End the connection. The observer gets `on_disconnected`.
    pub fn close(&self) {
        let _ = self.commands.send(Command::Close);
    }
}

impl DeckConnection {
    fn send(&self, msg: UiMessage) {
        // A closed connection has no task to take it: the app learns from
        // `on_disconnected`.
        let _ = self.commands.send(Command::Send(msg));
    }
}

/// Tell the observer the full state and start following the daemon.
fn start(
    client: Client,
    keep: Option<deck_remote::Keepalive>,
    observer: Arc<dyn DeckObserver>,
) -> Arc<DeckConnection> {
    let (tx, rx) = mpsc::unbounded_channel();
    let state = client.state().clone();
    observer.on_state(
        state.host.clone(),
        state.settings.clone().into(),
        state.agents.values().cloned().map(Into::into).collect(),
    );
    if let Some(today) = state.today_micro_usd {
        observer.on_costs(today);
    }
    let daemon_build = state.build.clone();
    runtime().spawn(follow(client, keep, rx, observer));
    Arc::new(DeckConnection {
        commands: tx,
        daemon_build,
    })
}

/// Another machine, as the user entered it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RemoteConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// A private key file, or `None` for the SSH agent. A key with a
    /// passphrase needs the agent.
    pub key_file: Option<String>,
    /// The socket of the daemon there, or `None` for the usual places.
    pub socket: Option<String>,
}

impl RemoteConfig {
    fn target(&self) -> Result<amiscara_ssh::SshTarget, DeckError> {
        let known = deck_remote::default_known_hosts().ok_or_else(|| DeckError::Failed {
            message: "no home folder".into(),
        })?;
        let identity = match &self.key_file {
            Some(p) => deck_remote::Identity::File {
                path: PathBuf::from(p),
                passphrase: None,
            },
            None => deck_remote::Identity::Agent,
        };
        Ok(amiscara_ssh::SshTarget {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            identity,
            // Never trust a new key on its own: the user is asked first.
            host_key: deck_remote::HostKeyPolicy::KnownHostsFile {
                path: known,
                accept_new: false,
            },
            agent_forward: false,
            keyboard_interactive: None,
        })
    }
}

/// Record the key the user agreed to in `~/.ssh/known_hosts`. Only for a key
/// that `connect_remote` reported as unknown.
#[uniffi::export]
pub fn trust_remote_host_key(host: String, port: u16, key: String) -> Result<(), DeckError> {
    let path = deck_remote::default_known_hosts().ok_or_else(|| DeckError::Failed {
        message: "no home folder".into(),
    })?;
    amiscara_ssh::trust_host_key(&path, &host, port, &key).map_err(|e| DeckError::Failed {
        message: e.to_string(),
    })
}

async fn follow(
    mut client: Client,
    _keep: Option<deck_remote::Keepalive>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    observer: Arc<dyn DeckObserver>,
) {
    let reason = loop {
        tokio::select! {
            msg = client.next() => match msg {
                Ok(m) => tell(&*observer, m),
                Err(deck_client::Error::Closed) => break "the daemon closed the connection".to_owned(),
                Err(e) => break e.to_string(),
            },
            cmd = commands.recv() => match cmd {
                Some(Command::Send(m)) => {
                    if let Err(e) = client.send(&m).await {
                        break e.to_string();
                    }
                }
                Some(Command::Close) | None => break "closed".to_owned(),
            },
        }
    };
    observer.on_disconnected(reason);
}

fn tell(observer: &dyn DeckObserver, msg: DaemonMessage) {
    match msg {
        DaemonMessage::AgentChanged(a) => observer.on_agent_changed((*a).into()),
        DaemonMessage::AgentRemoved { id } => observer.on_agent_removed(id),
        DaemonMessage::SettingsChanged(s) => observer.on_settings_changed(s.into()),
        DaemonMessage::Notify {
            agent_id,
            request_id,
            kind,
        } => observer.on_notify(agent_id, request_id, kind.into()),
        DaemonMessage::Withdraw { request_id } => observer.on_withdraw(request_id),
        DaemonMessage::Costs { today_micro_usd } => observer.on_costs(today_micro_usd),
        // Handled in `connect`; a message of a newer daemon is ignored.
        DaemonMessage::Welcome { .. }
        | DaemonMessage::VersionMismatch { .. }
        | DaemonMessage::Unknown => {}
    }
}

/// The fingerprint of a program file, to compare with `daemon_build`.
#[uniffi::export]
pub fn build_fingerprint(path: String) -> Option<String> {
    deck_proto::build::fingerprint(Path::new(&path))
}

/// The socket in the default data folder of the daemon.
#[uniffi::export]
pub fn default_socket_path() -> Option<String> {
    deck_proto::paths::default_socket().map(|p| p.to_string_lossy().into_owned())
}

/// The protocol number this library speaks.
#[uniffi::export]
pub fn protocol_version() -> u32 {
    deck_proto::PROTOCOL_VERSION
}

// Hooks in the settings of Claude Code, for the settings of the app.

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HooksStatus {
    pub settings_path: String,
    pub file_exists: bool,
    pub installed: u32,
    pub total: u32,
    pub outdated: u32,
    pub program_exists: bool,
    pub program_executable: bool,
    /// Hook entries of other programs, left alone.
    pub foreign_entries: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct HooksChange {
    pub changed: bool,
    pub backup_path: Option<String>,
}

fn settings_file(path: Option<String>) -> Result<PathBuf, DeckError> {
    path.map(PathBuf::from)
        .or_else(deck_hooks::default_claude_settings)
        .ok_or_else(|| DeckError::Failed {
            message: "no home folder".into(),
        })
}

fn hooks_error(e: deck_hooks::Error) -> DeckError {
    DeckError::Failed {
        message: e.to_string(),
    }
}

fn hooks_status_for(
    target: deck_hooks::Target,
    settings: PathBuf,
    program_path: &str,
) -> Result<HooksStatus, DeckError> {
    let s =
        deck_hooks::status_for(target, &settings, Path::new(program_path)).map_err(hooks_error)?;
    let count = |want: deck_hooks::EventState| {
        u32::try_from(s.events.iter().filter(|(_, st)| *st == want).count()).unwrap_or(0)
    };
    Ok(HooksStatus {
        settings_path: settings.to_string_lossy().into_owned(),
        file_exists: s.file_exists,
        installed: count(deck_hooks::EventState::Installed),
        total: u32::try_from(s.events.len()).unwrap_or(0),
        outdated: count(deck_hooks::EventState::Outdated),
        program_exists: s.program_exists,
        program_executable: s.program_executable,
        foreign_entries: u32::try_from(s.foreign_entries).unwrap_or(0),
    })
}

/// What is installed. `settings_path` `None`: the settings of Claude Code.
#[uniffi::export]
pub fn hooks_status(
    settings_path: Option<String>,
    program_path: String,
) -> Result<HooksStatus, DeckError> {
    hooks_status_for(
        deck_hooks::Target::Claude,
        settings_file(settings_path)?,
        &program_path,
    )
}

/// What the installed Claude Code says about its version.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ClaudeVersionCheck {
    Ok { version: String },
    TooOld { version: String },
    Unknown,
}

/// Runs `claude --version`, so call it off the main thread.
#[uniffi::export]
pub fn check_claude_version() -> ClaudeVersionCheck {
    use deck_hooks::version::{VersionCheck, check_installed};
    match check_installed(std::env::var_os("HOME").map(PathBuf::from)) {
        VersionCheck::Ok { version } => ClaudeVersionCheck::Ok { version },
        VersionCheck::TooOld { version } => ClaudeVersionCheck::TooOld { version },
        VersionCheck::Unknown => ClaudeVersionCheck::Unknown,
    }
}

fn codex_file() -> Result<PathBuf, DeckError> {
    deck_hooks::default_codex_hooks().ok_or_else(|| DeckError::Failed {
        message: "no home folder".into(),
    })
}

/// Is Codex there? Its folder exists (Detailkonzept, section 9).
#[uniffi::export]
pub fn codex_present() -> bool {
    codex_file().is_ok_and(|f| f.parent().is_some_and(Path::is_dir))
}

/// What is installed for Codex.
#[uniffi::export]
pub fn codex_hooks_status(program_path: String) -> Result<HooksStatus, DeckError> {
    hooks_status_for(deck_hooks::Target::Codex, codex_file()?, &program_path)
}

#[uniffi::export]
pub fn install_codex_hooks(program_path: String) -> Result<HooksChange, DeckError> {
    let file = codex_file()?;
    let o = deck_hooks::install_for(deck_hooks::Target::Codex, &file, Path::new(&program_path))
        .map_err(hooks_error)?;
    Ok(HooksChange {
        changed: o.changed,
        backup_path: o.backup.map(|b| b.to_string_lossy().into_owned()),
    })
}

#[uniffi::export]
pub fn uninstall_codex_hooks(program_path: String) -> Result<HooksChange, DeckError> {
    let file = codex_file()?;
    let o = deck_hooks::uninstall_for(deck_hooks::Target::Codex, &file, Path::new(&program_path))
        .map_err(hooks_error)?;
    Ok(HooksChange {
        changed: o.changed,
        backup_path: o.backup.map(|b| b.to_string_lossy().into_owned()),
    })
}

#[uniffi::export]
pub fn install_hooks(
    settings_path: Option<String>,
    program_path: String,
) -> Result<HooksChange, DeckError> {
    let settings = settings_file(settings_path)?;
    let o = deck_hooks::install(&settings, Path::new(&program_path)).map_err(hooks_error)?;
    Ok(HooksChange {
        changed: o.changed,
        backup_path: o.backup.map(|b| b.to_string_lossy().into_owned()),
    })
}

#[uniffi::export]
pub fn uninstall_hooks(
    settings_path: Option<String>,
    program_path: String,
) -> Result<HooksChange, DeckError> {
    let settings = settings_file(settings_path)?;
    let o = deck_hooks::uninstall(&settings, Path::new(&program_path)).map_err(hooks_error)?;
    Ok(HooksChange {
        changed: o.changed,
        backup_path: o.backup.map(|b| b.to_string_lossy().into_owned()),
    })
}
