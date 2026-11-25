#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use deck_daemon::{Config, Daemon, Error};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{Location, Status, Tool};
use deck_proto::message::{
    DaemonMessage, HookEnv, HookMessage, HookReply, UiMessage, decode_line, encode_line,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::oneshot;

struct Running {
    socket: PathBuf,
    dir: tempfile::TempDir,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

impl Running {
    async fn stop(mut self) -> tempfile::TempDir {
        let _ = self.stop.take().expect("stop").send(());
        self.task.await.expect("daemon task");
        self.dir
    }
}

fn config(dir: &tempfile::TempDir) -> Config {
    Config {
        data_dir: dir.path().join("deck"),
        host: "testhost".into(),
    }
}

async fn start() -> Running {
    let dir = tempfile::tempdir().expect("tempdir");
    let daemon = Daemon::bind(config(&dir)).await.expect("bind");
    let socket = daemon.socket_path().to_owned();
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(daemon.run(async {
        let _ = rx.await;
    }));
    Running {
        socket,
        dir,
        stop: Some(tx),
        task,
    }
}

async fn exchange(socket: &PathBuf, line: &str) -> String {
    let mut s = UnixStream::connect(socket).await.expect("connect");
    s.write_all(line.as_bytes()).await.expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).await.expect("read");
    out
}

fn hook_line(input: Value, env: HookEnv) -> String {
    encode_line(&HookMessage {
        protocol: PROTOCOL_VERSION,
        tool: Tool::Claude,
        env,
        input,
    })
    .expect("encode")
}

fn sample(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/hooks")
        .join(format!("{name}.json"));
    let raw: Value = serde_json::from_slice(&std::fs::read(path).expect("sample")).expect("json");
    raw["input"].clone()
}

/// Connect as a UI, say hello, read the first answer line, hang up.
async fn welcome(socket: &PathBuf) -> DaemonMessage {
    let (mut ui, first) = ui_connect(socket, PROTOCOL_VERSION).await;
    let _ = ui.writer.shutdown().await;
    first
}

struct Ui {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl Ui {
    async fn next(&mut self) -> Option<DaemonMessage> {
        let mut line = String::new();
        let n = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.reader.read_line(&mut line),
        )
        .await
        .expect("a message within 5 s")
        .expect("read");
        (n > 0).then(|| decode_line(&line).expect("decode"))
    }
}

async fn ui_connect(socket: &PathBuf, protocol: u32) -> (Ui, DaemonMessage) {
    let stream = UnixStream::connect(socket).await.expect("connect");
    let (r, mut w) = stream.into_split();
    let hello = encode_line(&UiMessage::Hello {
        protocol,
        name: "test".into(),
    })
    .expect("encode");
    w.write_all(hello.as_bytes()).await.expect("hello");
    let mut ui = Ui {
        reader: BufReader::new(r),
        writer: w,
    };
    let first = ui.next().await.expect("an answer to hello");
    (ui, first)
}

#[tokio::test]
async fn socket_and_folder_are_private() {
    let d = start().await;
    let mode =
        |p: &std::path::Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode(&d.socket), 0o600);
    assert_eq!(mode(d.socket.parent().expect("parent")), 0o700);
    d.stop().await;
}

#[tokio::test]
async fn an_existing_folder_with_open_rights_is_tightened() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config(&dir);
    std::fs::create_dir_all(&cfg.data_dir).expect("mkdir");
    std::fs::set_permissions(&cfg.data_dir, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let daemon = Daemon::bind(cfg.clone()).await.expect("bind");
    let mode = std::fs::metadata(&cfg.data_dir)
        .expect("meta")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    drop(daemon);
}

#[tokio::test]
async fn a_hook_message_shows_up_as_an_agent() {
    let d = start().await;
    let env = HookEnv {
        herdr_pane_id: Some("w1:p1".into()),
        ..HookEnv::default()
    };
    let reply = exchange(&d.socket, &hook_line(sample("UserPromptSubmit"), env)).await;
    let reply: HookReply = decode_line(&reply).expect("reply");
    assert_eq!(reply.output, None, "nothing is held yet");

    let DaemonMessage::Welcome { host, agents, .. } = welcome(&d.socket).await else {
        panic!("expected Welcome");
    };
    assert_eq!(host, "testhost");
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].status, Status::Working);
    assert_eq!(agents[0].host, "testhost");
    assert_eq!(
        agents[0].location,
        Location::Herdr {
            pane_id: "w1:p1".into()
        }
    );
    d.stop().await;
}

#[tokio::test]
async fn the_branch_comes_from_the_working_folder() {
    let d = start().await;
    let repo = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(repo.path().join(".git")).expect("mkdir");
    std::fs::write(repo.path().join(".git/HEAD"), "ref: refs/heads/feature/x\n").expect("write");
    let mut input = sample("SessionStart");
    input["cwd"] = json!(repo.path().to_string_lossy());
    exchange(&d.socket, &hook_line(input, HookEnv::default())).await;
    let DaemonMessage::Welcome { agents, .. } = welcome(&d.socket).await else {
        panic!("expected Welcome");
    };
    assert_eq!(agents[0].branch.as_deref(), Some("feature/x"));
    d.stop().await;
}

#[tokio::test]
async fn several_events_build_up_the_state() {
    let d = start().await;
    for name in ["SessionStart", "UserPromptSubmit", "PreToolUse-Bash"] {
        exchange(&d.socket, &hook_line(sample(name), HookEnv::default())).await;
    }
    let DaemonMessage::Welcome { agents, .. } = welcome(&d.socket).await else {
        panic!("expected Welcome");
    };
    let sessions: std::collections::BTreeSet<_> = agents.iter().map(|a| a.id.clone()).collect();
    assert!(!sessions.is_empty());
    assert!(agents.iter().any(|a| {
        a.activity
            .as_deref()
            .is_some_and(|t| t.starts_with("führt aus"))
    }));
    d.stop().await;
}

#[tokio::test]
async fn bad_input_gets_the_silent_reply_and_never_stops_the_daemon() {
    let d = start().await;
    let silent = encode_line(&HookReply { output: None }).expect("encode");
    for line in [
        "not json\n".to_owned(),
        "{}\n".to_owned(),
        "{\"protocol\":1}\n".to_owned(),
        format!(
            "{{\"protocol\":{},\"tool\":\"claude\",\"env\":{{\"parents\":[]}},\"input\":{{\"foo\":1}}}}\n",
            PROTOCOL_VERSION
        ),
        hook_line(json!({"session_id": "s"}), HookEnv::default()),
    ] {
        let reply = exchange(&d.socket, &line).await;
        // JSON without a hook shape gets no answer or the silent one, never output.
        assert!(reply.is_empty() || reply == silent, "{line:?} -> {reply:?}");
    }
    // Still alive and nothing was invented.
    let DaemonMessage::Welcome { agents, .. } = welcome(&d.socket).await else {
        panic!("expected Welcome");
    };
    assert!(agents.is_empty());
    d.stop().await;
}

#[tokio::test]
async fn a_message_with_another_protocol_number_is_ignored() {
    let d = start().await;
    let line = encode_line(&HookMessage {
        protocol: PROTOCOL_VERSION + 1,
        tool: Tool::Claude,
        env: HookEnv::default(),
        input: sample("UserPromptSubmit"),
    })
    .expect("encode");
    exchange(&d.socket, &line).await;
    let DaemonMessage::Welcome { agents, .. } = welcome(&d.socket).await else {
        panic!("expected Welcome");
    };
    assert!(agents.is_empty());
    d.stop().await;
}

#[tokio::test]
async fn a_ui_with_another_protocol_number_hears_about_the_mismatch() {
    let d = start().await;
    let (mut ui, first) = ui_connect(&d.socket, PROTOCOL_VERSION + 1).await;
    assert_eq!(
        first,
        DaemonMessage::VersionMismatch {
            daemon: PROTOCOL_VERSION,
            ui: PROTOCOL_VERSION + 1
        }
    );
    assert!(
        ui.next().await.is_none(),
        "the daemon closes the connection"
    );
    d.stop().await;
}

#[tokio::test]
async fn an_oversized_line_is_dropped() {
    let d = start().await;
    let big = format!("{}\n", "x".repeat(5 * 1024 * 1024));
    let mut s = UnixStream::connect(&d.socket).await.expect("connect");
    // The daemon may close early; a failed write is fine.
    let _ = s.write_all(big.as_bytes()).await;
    let mut out = String::new();
    let _ = s.read_to_string(&mut out).await;
    assert!(out.is_empty());
    let DaemonMessage::Welcome { .. } = welcome(&d.socket).await else {
        panic!("daemon still answers");
    };
    d.stop().await;
}

#[tokio::test]
async fn a_second_daemon_on_the_same_folder_is_refused() {
    let d = start().await;
    let cfg = Config {
        data_dir: d.socket.parent().expect("parent").to_owned(),
        host: "other".into(),
    };
    let err = Daemon::bind(cfg).await.err().expect("must fail");
    assert!(matches!(err, Error::AlreadyRunning(_)), "{err}");
    d.stop().await;
}

#[tokio::test]
async fn a_stale_socket_is_taken_over_and_the_socket_goes_with_the_daemon() {
    let dir = tempfile::tempdir().expect("tempdir");
    let cfg = config(&dir);
    // First daemon dies without cleaning up: leave its socket file behind.
    {
        let first = Daemon::bind(cfg.clone()).await.expect("bind");
        drop(first);
    }
    assert!(cfg.socket_path().exists(), "stale file stays after a drop");
    let daemon = Daemon::bind(cfg.clone()).await.expect("takes over");
    let (tx, rx) = oneshot::channel::<()>();
    let task = tokio::spawn(daemon.run(async {
        let _ = rx.await;
    }));
    assert!(matches!(
        welcome(&cfg.socket_path()).await,
        DaemonMessage::Welcome { .. }
    ));
    tx.send(()).expect("stop");
    task.await.expect("task");
    assert!(!cfg.socket_path().exists(), "socket is removed on shutdown");
}

#[tokio::test]
async fn a_ui_gets_every_change_after_the_welcome() {
    let d = start().await;
    let (mut ui, first) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    let DaemonMessage::Welcome { agents, .. } = first else {
        panic!("expected Welcome");
    };
    assert!(agents.is_empty());

    exchange(
        &d.socket,
        &hook_line(sample("UserPromptSubmit"), HookEnv::default()),
    )
    .await;
    let Some(DaemonMessage::AgentChanged(a)) = ui.next().await else {
        panic!("expected AgentChanged");
    };
    assert_eq!(a.status, Status::Working);

    exchange(&d.socket, &hook_line(sample("Stop"), HookEnv::default())).await;
    let Some(DaemonMessage::AgentChanged(a)) = ui.next().await else {
        panic!("expected AgentChanged");
    };
    assert_eq!(a.status, Status::Done);
    d.stop().await;
}

#[tokio::test]
async fn an_event_that_changes_nothing_sends_nothing() {
    let d = start().await;
    exchange(
        &d.socket,
        &hook_line(sample("SessionStart"), HookEnv::default()),
    )
    .await;
    let (mut ui, _) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    exchange(
        &d.socket,
        &hook_line(sample("SessionStart"), HookEnv::default()),
    )
    .await;
    let quiet = tokio::time::timeout(std::time::Duration::from_millis(300), ui.next()).await;
    assert!(quiet.is_err(), "no change, no message");
    d.stop().await;
}

#[tokio::test]
async fn several_uis_all_get_the_changes_and_one_leaving_hurts_nobody() {
    let d = start().await;
    let (mut a, _) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    let (mut b, _) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    let (c, _) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    drop(c); // goes away without a word
    exchange(
        &d.socket,
        &hook_line(sample("UserPromptSubmit"), HookEnv::default()),
    )
    .await;
    assert!(matches!(
        a.next().await,
        Some(DaemonMessage::AgentChanged(_))
    ));
    assert!(matches!(
        b.next().await,
        Some(DaemonMessage::AgentChanged(_))
    ));
    exchange(&d.socket, &hook_line(sample("Stop"), HookEnv::default())).await;
    assert!(matches!(
        a.next().await,
        Some(DaemonMessage::AgentChanged(_))
    ));
    assert!(matches!(
        b.next().await,
        Some(DaemonMessage::AgentChanged(_))
    ));
    d.stop().await;
}

#[tokio::test]
async fn a_late_ui_gets_the_state_in_the_welcome_and_each_change_once() {
    let d = start().await;
    exchange(
        &d.socket,
        &hook_line(sample("UserPromptSubmit"), HookEnv::default()),
    )
    .await;
    let (mut ui, first) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    let DaemonMessage::Welcome { agents, .. } = first else {
        panic!("expected Welcome");
    };
    assert_eq!(agents.len(), 1);
    exchange(&d.socket, &hook_line(sample("Stop"), HookEnv::default())).await;
    let Some(DaemonMessage::AgentChanged(a)) = ui.next().await else {
        panic!("expected AgentChanged");
    };
    assert_eq!(a.status, Status::Done);
    let more = tokio::time::timeout(std::time::Duration::from_millis(300), ui.next()).await;
    assert!(more.is_err(), "the earlier change is not sent again");
    d.stop().await;
}

#[tokio::test]
async fn a_ui_that_sends_nonsense_stays_connected() {
    let d = start().await;
    let (mut ui, _) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    ui.writer
        .write_all(b"this is not json\n{\"type\":\"unknown\"}\n")
        .await
        .expect("write");
    exchange(
        &d.socket,
        &hook_line(sample("UserPromptSubmit"), HookEnv::default()),
    )
    .await;
    assert!(matches!(
        ui.next().await,
        Some(DaemonMessage::AgentChanged(_))
    ));
    d.stop().await;
}

#[tokio::test]
async fn shutting_down_closes_the_connections_of_the_uis() {
    let d = start().await;
    let (mut ui, _) = ui_connect(&d.socket, PROTOCOL_VERSION).await;
    d.stop().await;
    assert!(ui.next().await.is_none());
}
