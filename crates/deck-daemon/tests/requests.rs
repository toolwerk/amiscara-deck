#![cfg(unix)]
//! PermissionRequest at the daemon while nobody can answer: nothing is held,
//! the request is shown as "waits in the terminal". Holding is in `holding.rs`.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::Duration;

use deck_daemon::{Config, Daemon};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{RequestState, Tool};
use deck_proto::message::{
    DaemonMessage, HookEnv, HookMessage, HookReply, UiMessage, decode_line, encode_line,
};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::oneshot;

struct Running {
    socket: PathBuf,
    _dir: tempfile::TempDir,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

async fn start() -> Running {
    let dir = tempfile::tempdir().expect("tempdir");
    let daemon = Daemon::bind(Config {
        data_dir: dir.path().join("deck"),
        host: "testhost".into(),
    })
    .await
    .expect("bind");
    let socket = daemon.socket_path().to_owned();
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

    /// Send a recorded hook input and return the daemon's reply.
    async fn hook(&self, sample: &str, tool: Tool) -> HookReply {
        let name = format!(
            "{}/../../testdata/hooks/{sample}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let line = encode_line(&HookMessage {
            protocol: PROTOCOL_VERSION,
            tool,
            env: HookEnv::default(),
            input: raw["input"].clone(),
        })
        .expect("encode");
        let mut s = UnixStream::connect(&self.socket).await.expect("connect");
        s.write_all(line.as_bytes()).await.expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).await.expect("read");
        decode_line(&out).expect("reply")
    }
}

#[tokio::test]
async fn without_a_ui_a_permission_request_is_answered_at_once_with_nothing() {
    let d = start().await;
    let start = std::time::Instant::now();
    let reply = d
        .hook("PermissionRequest-Bash-addRules", Tool::Claude)
        .await;
    assert_eq!(reply.output, None, "the dialog in the terminal decides");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "nothing is held yet"
    );
    d.stop().await;
}

#[tokio::test]
async fn a_request_without_a_ui_is_just_passed_through_and_kept_for_later_uis() {
    let d = start().await;
    d.hook("PermissionRequest-Write-setMode", Tool::Claude)
        .await;
    let (r, mut w) = UnixStream::connect(&d.socket)
        .await
        .expect("connect")
        .into_split();
    let hello = encode_line(&UiMessage::Hello {
        protocol: PROTOCOL_VERSION,
        name: "late".into(),
    })
    .expect("encode");
    w.write_all(hello.as_bytes()).await.expect("hello");
    let mut line = String::new();
    BufReader::new(r).read_line(&mut line).await.expect("read");
    let DaemonMessage::Welcome { agents, .. } = decode_line(&line).expect("decode") else {
        panic!("expected Welcome");
    };
    let req = agents[0].request.clone().expect("the card is there");
    assert_eq!(req.state, RequestState::PassedThrough);
    assert!(req.truncated, "a Write needs expanding");
    assert!(req.always_allow.is_empty(), "setMode is no rule");
    d.stop().await;
}
