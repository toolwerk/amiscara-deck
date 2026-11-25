use deck_client::{Client, Error, fetch};
use deck_daemon::{Config, Daemon};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{Status, Tool};
use deck_proto::message::{DaemonMessage, HookEnv, HookMessage, encode_line};
use serde_json::Value;
use std::path::PathBuf;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

    async fn hook(&self, event: &str) {
        let name = format!(
            "{}/../../testdata/hooks/{event}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let line = encode_line(&HookMessage {
            protocol: PROTOCOL_VERSION,
            tool: Tool::Claude,
            env: HookEnv::default(),
            input: raw["input"].clone(),
        })
        .expect("encode");
        let mut s = UnixStream::connect(&self.socket).await.expect("connect");
        s.write_all(line.as_bytes()).await.expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).await.expect("read");
    }
}

#[tokio::test]
async fn the_client_holds_the_full_state_and_follows_every_change() {
    let d = start().await;
    d.hook("UserPromptSubmit").await;
    let mut c = Client::connect(&d.socket, "test").await.expect("connect");
    assert_eq!(c.state().host, "testhost");
    assert_eq!(c.state().agents.len(), 1);
    let id = c.state().agents.keys().next().expect("agent").clone();
    assert_eq!(c.state().agents[&id].status, Status::Working);

    d.hook("Stop").await;
    let msg = c.next().await.expect("change");
    assert!(matches!(msg, DaemonMessage::AgentChanged(_)));
    assert_eq!(
        c.state().agents[&id].status,
        Status::Done,
        "applied to the state"
    );
    d.stop().await;
}

#[tokio::test]
async fn fetch_gives_the_state_once() {
    let d = start().await;
    d.hook("UserPromptSubmit").await;
    let s = fetch(&d.socket, "test").await.expect("fetch");
    assert_eq!(s.agents.len(), 1);
    d.stop().await;
}

#[tokio::test]
async fn no_daemon_is_a_clear_error() {
    let dir = tempfile::tempdir().expect("tempdir");
    let err = Client::connect(&dir.path().join("none.sock"), "t")
        .await
        .err()
        .expect("error");
    assert!(matches!(err, Error::NoDaemon(_)), "{err}");
    // A socket file nobody listens on is the same.
    let stale = dir.path().join("stale.sock");
    drop(std::os::unix::net::UnixListener::bind(&stale).expect("bind"));
    let err = Client::connect(&stale, "t").await.err().expect("error");
    assert!(matches!(err, Error::NoDaemon(_)), "{err}");
}

#[tokio::test]
async fn the_end_of_the_daemon_is_reported_as_closed() {
    let d = start().await;
    let mut c = Client::connect(&d.socket, "test").await.expect("connect");
    d.stop().await;
    assert!(matches!(c.next().await, Err(Error::Closed)));
}

#[tokio::test]
async fn a_daemon_that_speaks_another_protocol_is_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("fake.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 256];
        let _ = s.read(&mut buf).await;
        let _ = s
            .write_all(b"{\"type\":\"version_mismatch\",\"daemon\":99,\"ui\":1}\n")
            .await;
    });
    let err = Client::connect(&socket, "t").await.err().expect("error");
    assert!(
        matches!(err, Error::VersionMismatch { daemon: 99, .. }),
        "{err}"
    );
}

#[tokio::test]
async fn nonsense_from_the_other_side_is_an_error_not_a_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("fake.sock");
    let listener = tokio::net::UnixListener::bind(&socket).expect("bind");
    tokio::spawn(async move {
        let (mut s, _) = listener.accept().await.expect("accept");
        let mut buf = [0u8; 256];
        let _ = s.read(&mut buf).await;
        let _ = s.write_all(b"this is not json\n").await;
    });
    assert!(matches!(
        Client::connect(&socket, "t").await,
        Err(Error::Unexpected(_))
    ));
}
