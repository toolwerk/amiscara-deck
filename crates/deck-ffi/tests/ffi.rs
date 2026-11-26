//! The hull against a real daemon, with an observer written in Rust (the app
//! writes it in Swift; to the library both are the same trait).

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use deck_daemon::{Config, Daemon};
use deck_ffi::{
    ActivityView, AgentStatus, AgentView, AnswerViaKind, ApprovalModeKind, DeckConnection,
    DeckError, DeckObserver, NotifyKindView, PresenceReport, RemoteConfig, RemoteFailureKind,
    RequestStateKind, SettingsView, default_socket_path, hooks_status, install_hooks,
    protocol_version, uninstall_hooks,
};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::Tool;
use deck_proto::message::{HookEnv, HookMessage, encode_line};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq)]
enum Seen {
    State { host: String, agents: usize },
    Changed(Box<AgentView>),
    Removed(String),
    Settings(SettingsView),
    Notify(String, NotifyKindView),
    Disconnected(String),
}

#[derive(Default)]
struct Recorder {
    seen: Mutex<Vec<Seen>>,
}

impl Recorder {
    fn all(&self) -> Vec<Seen> {
        self.seen.lock().expect("lock").clone()
    }

    /// Wait until `pick` finds something in what was seen.
    fn wait<T>(&self, pick: impl Fn(&[Seen]) -> Option<T>) -> T {
        let end = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(v) = pick(&self.all()) {
                return v;
            }
            assert!(Instant::now() < end, "nothing came: {:?}", self.all());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn agent(&self, pred: impl Fn(&AgentView) -> bool) -> AgentView {
        self.wait(|s| {
            s.iter().rev().find_map(|e| match e {
                Seen::Changed(a) if pred(a) => Some((**a).clone()),
                _ => None,
            })
        })
    }
}

impl DeckObserver for Recorder {
    fn on_state(&self, host: String, _settings: SettingsView, agents: Vec<AgentView>) {
        self.seen.lock().expect("lock").push(Seen::State {
            host,
            agents: agents.len(),
        });
    }
    fn on_agent_changed(&self, agent: AgentView) {
        self.seen
            .lock()
            .expect("lock")
            .push(Seen::Changed(Box::new(agent)));
    }
    fn on_agent_removed(&self, id: String) {
        self.seen.lock().expect("lock").push(Seen::Removed(id));
    }
    fn on_settings_changed(&self, settings: SettingsView) {
        self.seen
            .lock()
            .expect("lock")
            .push(Seen::Settings(settings));
    }
    fn on_notify(&self, agent_id: String, _request_id: Option<String>, kind: NotifyKindView) {
        self.seen
            .lock()
            .expect("lock")
            .push(Seen::Notify(agent_id, kind));
    }
    fn on_costs(&self, _today_micro_usd: u64) {}
    fn on_withdraw(&self, _request_id: String) {}
    fn on_disconnected(&self, reason: String) {
        self.seen
            .lock()
            .expect("lock")
            .push(Seen::Disconnected(reason));
    }
}

struct Running {
    socket: String,
    _dir: tempfile::TempDir,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

async fn start() -> Running {
    let dir = tempfile::tempdir().expect("tempdir");
    let daemon = Daemon::bind(Config {
        data_dir: dir.path().join("deck"),
        host: "ffihost".into(),
    })
    .await
    .expect("bind");
    let socket = daemon.socket_path().to_string_lossy().into_owned();
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(daemon.run(async {
        let _ = rx.await;
    }));
    Running {
        socket,
        _dir: dir,
        stop: Some(tx),
        task,
    }
}

impl Running {
    async fn stop(mut self) {
        let _ = self.stop.take().expect("stop").send(());
        self.task.await.expect("task");
    }

    /// A hook program: send an event of a recorded sample as `session`.
    fn hook(&self, sample: &str, session: &str) -> tokio::task::JoinHandle<()> {
        let name = format!(
            "{}/../../testdata/hooks/{sample}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let mut input = raw["input"].clone();
        input["session_id"] = json!(session);
        let line = encode_line(&HookMessage {
            protocol: PROTOCOL_VERSION,
            tool: Tool::Claude,
            env: HookEnv::default(),
            input,
        })
        .expect("encode");
        let socket = self.socket.clone();
        tokio::spawn(async move {
            let mut s = UnixStream::connect(&socket).await.expect("connect");
            s.write_all(line.as_bytes()).await.expect("write");
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
        })
    }
}

/// `connect` blocks; the app calls it off the main thread, the test the same.
async fn connect(socket: &str, rec: &Arc<Recorder>) -> Result<Arc<DeckConnection>, DeckError> {
    let socket = socket.to_owned();
    let rec = rec.clone();
    tokio::task::spawn_blocking(move || DeckConnection::connect(socket, "ffi test".into(), rec))
        .await
        .expect("join")
}

#[tokio::test(flavor = "multi_thread")]
async fn connect_gives_the_full_state_first_and_then_every_change() {
    let d = start().await;
    d.hook("UserPromptSubmit", "s0").await.expect("hook");
    let rec = Arc::new(Recorder::default());
    let _c = connect(&d.socket, &rec).await.expect("connect");
    assert_eq!(
        rec.all()[0],
        Seen::State {
            host: "ffihost".into(),
            agents: 1
        },
        "the state comes before any change"
    );

    d.hook("PreToolUse-Bash", "s1");
    let a = rec.agent(|a| a.id == "s1");
    assert_eq!(a.status, AgentStatus::Working);
    assert_eq!(a.host, "ffihost");
    assert!(matches!(
        a.activity,
        Some(ActivityView::Run { command: Some(_) })
    ));
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_request_can_be_allowed_through_the_hull() {
    let d = start().await;
    let rec = Arc::new(Recorder::default());
    let c = connect(&d.socket, &rec).await.expect("connect");
    let hook = d.hook("PermissionRequest-Bash-addRules", "s1");
    let a = rec.agent(|a| {
        a.request
            .as_ref()
            .is_some_and(|r| r.state == RequestStateKind::Held)
    });
    let req = a.request.expect("request");
    assert_eq!(req.summary, "curl --version");
    assert_eq!(req.always_allow.len(), 1);
    assert!(req.deadline_ms.is_some());

    c.answer_allow(
        req.id.clone(),
        Some(req.always_allow[0].index),
        false,
        AnswerViaKind::Cli,
    );
    rec.agent(|a| {
        a.request
            .as_ref()
            .is_some_and(|r| r.state == RequestStateKind::Answered)
    });
    hook.await.expect("the hook got its answer");
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_shortened_request_needs_expanded_and_deny_works() {
    let d = start().await;
    let rec = Arc::new(Recorder::default());
    let c = connect(&d.socket, &rec).await.expect("connect");
    let hook = d.hook("PermissionRequest-Write-setMode", "s1");
    let req = rec
        .agent(|a| {
            a.request
                .as_ref()
                .is_some_and(|r| r.state == RequestStateKind::Held)
        })
        .request
        .expect("request");
    assert!(req.truncated);
    c.answer_allow(req.id.clone(), None, false, AnswerViaKind::Cli); // refused by the daemon
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !hook.is_finished(),
        "the hook still waits: nothing was allowed"
    );
    c.answer_deny(req.id, Some("Nein".into()), AnswerViaKind::Cli);
    hook.await.expect("answered");
    rec.agent(|a| {
        a.request
            .as_ref()
            .is_some_and(|r| r.state == RequestStateKind::Answered)
    });
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_and_presence_go_to_the_daemon() {
    let d = start().await;
    let rec = Arc::new(Recorder::default());
    let c = connect(&d.socket, &rec).await.expect("connect");
    c.set_settings(SettingsView {
        mode: ApprovalModeKind::Terminal,
        wait_secs: 90,
        retention_days: 7,
        codex_approvals: false,
    });
    let s = rec.wait(|s| {
        s.iter().find_map(|e| {
            if let Seen::Settings(s) = e {
                Some(s.clone())
            } else {
                None
            }
        })
    });
    assert_eq!((s.mode, s.wait_secs), (ApprovalModeKind::Terminal, 90));
    // Presence has no answer; it must at least not break the connection.
    c.report_presence(PresenceReport {
        screen_locked: false,
        idle_secs: 3,
        frontmost_app: Some("Terminal".into()),
        focus_mode: false,
        last_input_ms: 1,
    });
    d.hook("UserPromptSubmit", "s1");
    rec.agent(|a| a.id == "s1");
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_end_of_the_daemon_and_close_are_reported() {
    let d = start().await;
    let rec = Arc::new(Recorder::default());
    let _c = connect(&d.socket, &rec).await.expect("connect");
    d.stop().await;
    let reason = rec.wait(|s| {
        s.iter().find_map(|e| {
            if let Seen::Disconnected(r) = e {
                Some(r.clone())
            } else {
                None
            }
        })
    });
    assert!(reason.contains("closed"), "{reason}");

    let d = start().await;
    let rec = Arc::new(Recorder::default());
    let c = connect(&d.socket, &rec).await.expect("connect");
    c.close();
    rec.wait(|s| {
        s.iter()
            .any(|e| matches!(e, Seen::Disconnected(_)))
            .then_some(())
    });
    c.answer_deny("r1".into(), None, AnswerViaKind::Cli); // after close: ignored, no panic
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_daemon_is_a_clear_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let rec = Arc::new(Recorder::default());
    let Err(err) = connect(&dir.path().join("none.sock").to_string_lossy(), &rec).await else {
        panic!("no daemon, but it connected");
    };
    assert!(matches!(err, DeckError::NoDaemon { .. }), "{err}");
    assert!(rec.all().is_empty());
}

#[test]
fn small_facts_of_the_library() {
    assert_eq!(protocol_version(), PROTOCOL_VERSION);
    // Not asserting the path (it depends on HOME); it must be a path to a socket.
    if let Some(p) = default_socket_path() {
        assert!(p.ends_with("deckd.sock"), "{p}");
    }
}

#[test]
fn hooks_can_be_installed_and_removed_through_the_hull() {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = dir
        .path()
        .join("settings.json")
        .to_string_lossy()
        .into_owned();
    let program = dir.path().join("deck-hook");
    std::fs::write(&program, "#!/bin/sh\n").expect("write");
    std::fs::set_permissions(
        &program,
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .expect("chmod");
    let program = program.to_string_lossy().into_owned();

    let s = hooks_status(Some(settings.clone()), program.clone()).expect("status");
    assert_eq!(
        (s.installed, s.total, s.file_exists, s.program_executable),
        (0, 15, false, true)
    );
    let c = install_hooks(Some(settings.clone()), program.clone()).expect("install");
    assert!(c.changed);
    let s = hooks_status(Some(settings.clone()), program.clone()).expect("status");
    assert_eq!((s.installed, s.outdated, s.foreign_entries), (15, 0, 0));
    assert!(
        !install_hooks(Some(settings.clone()), program.clone())
            .expect("again")
            .changed
    );
    assert!(
        uninstall_hooks(Some(settings.clone()), program.clone())
            .expect("uninstall")
            .changed
    );
    assert_eq!(
        hooks_status(Some(settings), program)
            .expect("status")
            .installed,
        0
    );
}

#[test]
fn a_broken_settings_file_is_an_error_not_a_change() {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = dir.path().join("settings.json");
    std::fs::write(&settings, "{ // comment").expect("write");
    let err = install_hooks(
        Some(settings.to_string_lossy().into_owned()),
        "/x/deck-hook".into(),
    )
    .expect_err("an invalid file must be refused");
    assert!(matches!(err, DeckError::Failed { .. }));
    assert_eq!(
        std::fs::read_to_string(&settings).expect("read"),
        "{ // comment"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_machine_that_is_not_there_is_an_error_not_a_hang() {
    // Nothing listens on port 1 of the own machine.
    let rec = Arc::new(Recorder::default());
    let r = tokio::task::spawn_blocking(move || {
        DeckConnection::connect_remote(
            RemoteConfig {
                host: "127.0.0.1".into(),
                port: 1,
                user: "nobody".into(),
                key_file: None,
                socket: None,
            },
            "ffi test".into(),
            rec,
        )
    })
    .await
    .expect("join");
    assert!(
        matches!(
            r,
            Err(DeckError::Remote {
                kind: RemoteFailureKind::Refused,
                ..
            })
        ),
        "{:?}",
        r.err()
    );
}
