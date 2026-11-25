//! The state across restarts.

#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use deck_daemon::{Config, Daemon};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{RequestState, Status, Tool};
use deck_proto::message::{
    ApprovalMode, DaemonMessage, HookEnv, HookMessage, Settings, UiMessage, decode_line,
    encode_line,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::oneshot;

struct Running {
    socket: PathBuf,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

async fn start_in(dir: &Path, host: &str) -> Running {
    let daemon = Daemon::bind(Config {
        data_dir: dir.to_owned(),
        host: host.into(),
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
        stop: Some(tx),
        task,
    }
}

impl Running {
    async fn stop(mut self) {
        let _ = self.stop.take().expect("stop").send(());
        self.task.await.expect("task");
    }

    /// Send a hook event as session `session`, with the prompt etc. patched in.
    async fn event(&self, sample: &str, session: &str, patch: Value) {
        let name = format!(
            "{}/../../testdata/hooks/{sample}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let mut input = raw["input"].clone();
        input["session_id"] = json!(session);
        input
            .as_object_mut()
            .expect("object")
            .extend(patch.as_object().expect("object").clone());
        let line = encode_line(&HookMessage {
            protocol: PROTOCOL_VERSION,
            tool: Tool::Claude,
            env: HookEnv::default(),
            input,
        })
        .expect("encode");
        let mut s = UnixStream::connect(&self.socket).await.expect("connect");
        s.write_all(line.as_bytes()).await.expect("write");
        let mut out = String::new();
        s.read_to_string(&mut out).await.expect("read");
    }

    async fn ui(
        &self,
    ) -> (
        BufReader<tokio::net::unix::OwnedReadHalf>,
        tokio::net::unix::OwnedWriteHalf,
        DaemonMessage,
    ) {
        let (r, mut w) = UnixStream::connect(&self.socket)
            .await
            .expect("connect")
            .into_split();
        let hello = encode_line(&UiMessage::Hello {
            protocol: PROTOCOL_VERSION,
            name: "t".into(),
        })
        .expect("encode");
        w.write_all(hello.as_bytes()).await.expect("hello");
        let mut r = BufReader::new(r);
        let mut line = String::new();
        r.read_line(&mut line).await.expect("read");
        let first = decode_line(&line).expect("decode");
        (r, w, first)
    }

    async fn welcome(&self) -> (String, Settings, Vec<deck_proto::agent::Agent>) {
        let (_, _, first) = self.ui().await;
        let DaemonMessage::Welcome {
            host,
            settings,
            agents,
            ..
        } = first
        else {
            panic!("expected Welcome")
        };
        (host, settings, agents)
    }
}

fn dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("tempdir")
}

#[tokio::test]
async fn agents_come_back_after_a_restart() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "work").await;
    a.event(
        "UserPromptSubmit",
        "s1",
        json!({"prompt": "Fix the import"}),
    )
    .await;
    a.event("PreToolUse-Bash", "s1", json!({})).await;
    a.stop().await;

    let b = start_in(&data, "work").await;
    let (_, _, agents) = b.welcome().await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, "s1");
    assert_eq!(agents[0].last_prompt.as_deref(), Some("Fix the import"));
    assert_eq!(agents[0].status, Status::Working);
    assert!(
        agents[0]
            .activity
            .as_deref()
            .is_some_and(|t| t.starts_with("führt aus"))
    );
    b.stop().await;
}

#[tokio::test]
async fn the_name_of_this_machine_is_the_current_one() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "old-name").await;
    a.event("UserPromptSubmit", "s1", json!({})).await;
    a.stop().await;
    let b = start_in(&data, "new-name").await;
    let (host, _, agents) = b.welcome().await;
    assert_eq!(
        (host.as_str(), agents[0].host.as_str()),
        ("new-name", "new-name")
    );
    b.stop().await;
}

#[tokio::test]
async fn an_open_request_waits_in_the_terminal_after_a_restart_and_ids_go_on() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "work").await;
    // A UI is connected, so the request is held; stopping the daemon releases it.
    let (mut r, _w, _) = a.ui().await;
    let hook = {
        let socket = a.socket.clone();
        let line = {
            let raw: Value = serde_json::from_slice(
                &std::fs::read(format!(
                    "{}/../../testdata/hooks/PermissionRequest-Bash-addRules.json",
                    env!("CARGO_MANIFEST_DIR")
                ))
                .expect("sample"),
            )
            .expect("json");
            let mut input = raw["input"].clone();
            input["session_id"] = json!("s1");
            encode_line(&HookMessage {
                protocol: PROTOCOL_VERSION,
                tool: Tool::Claude,
                env: HookEnv::default(),
                input,
            })
            .expect("encode")
        };
        tokio::spawn(async move {
            let mut s = UnixStream::connect(&socket).await.expect("connect");
            s.write_all(line.as_bytes()).await.expect("write");
            let mut out = String::new();
            let _ = s.read_to_string(&mut out).await;
        })
    };
    loop {
        let mut line = String::new();
        r.read_line(&mut line).await.expect("read");
        if let DaemonMessage::AgentChanged(a) = decode_line(&line).expect("decode")
            && a.request
                .as_ref()
                .is_some_and(|q| q.state == RequestState::Held)
        {
            break;
        }
    }
    a.stop().await;
    hook.await.expect("hook");

    let b = start_in(&data, "work").await;
    let (_, _, agents) = b.welcome().await;
    let req = agents[0].request.clone().expect("the card is back");
    assert_eq!(
        (req.id.as_str(), req.state, req.deadline_ms),
        ("r1", RequestState::PassedThrough, None)
    );

    // A new request gets a new id.
    b.event("PermissionRequest-Write-setMode", "s2", json!({}))
        .await;
    let (_, _, agents) = b.welcome().await;
    let s2 = agents.iter().find(|a| a.id == "s2").expect("s2");
    assert_eq!(s2.request.as_ref().expect("request").id, "r2");
    b.stop().await;
}

#[tokio::test]
async fn the_settings_survive_a_restart() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "work").await;
    let (mut r, mut w, _) = a.ui().await;
    let msg = encode_line(&UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Terminal,
        wait_secs: 77,
        retention_days: 7,
        codex_approvals: false,
    }))
    .expect("encode");
    w.write_all(msg.as_bytes()).await.expect("send");
    let mut line = String::new();
    r.read_line(&mut line).await.expect("read");
    assert!(matches!(
        decode_line(&line),
        Ok(DaemonMessage::SettingsChanged(_))
    ));
    a.stop().await;

    let b = start_in(&data, "work").await;
    let (_, settings, _) = b.welcome().await;
    assert_eq!(
        (settings.mode, settings.wait_secs),
        (ApprovalMode::Terminal, 77)
    );
    b.stop().await;
}

#[tokio::test]
async fn the_state_file_is_private_and_no_temporary_file_is_left() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "work").await;
    a.event("UserPromptSubmit", "s1", json!({})).await;
    a.stop().await;
    let file = data.join("state.json");
    assert_eq!(
        std::fs::metadata(&file).expect("meta").permissions().mode() & 0o777,
        0o600
    );
    let names: Vec<String> = std::fs::read_dir(&data)
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!names.iter().any(|n| n.ends_with(".tmp")), "{names:?}");
}

#[tokio::test]
async fn a_crash_loses_at_most_a_moment() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "work").await;
    a.event(
        "UserPromptSubmit",
        "s1",
        json!({"prompt": "work in progress"}),
    )
    .await;
    // No stop: the daemon "crashes". The periodic save must have written it.
    let file = data.join("state.json");
    let mut found = false;
    for _ in 0..40 {
        if std::fs::read_to_string(&file).is_ok_and(|t| t.contains("work in progress")) {
            found = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(found, "the state was not written within 4 s");
    a.stop().await;
}

#[tokio::test]
async fn a_broken_or_foreign_state_file_means_an_empty_start_not_a_crash() {
    for (name, content) in [
        ("empty", "".to_owned()),
        ("garbage", "this is not json".to_owned()),
        ("wrong shape", "{\"hello\": 1}".to_owned()),
        ("newer version", json!({"version": 99, "settings": {"mode": "auto", "wait_secs": 120}, "next_request": 1, "agents": []}).to_string()),
        ("cut off", "{\"version\": 1, \"agents\": [".to_owned()),
    ] {
        let d = dir();
        let data = d.path().join("deck");
        std::fs::create_dir_all(&data).expect("mkdir");
        std::fs::write(data.join("state.json"), content).expect("write");
        let a = start_in(&data, "work").await;
        let (_, settings, agents) = a.welcome().await;
        assert!(agents.is_empty(), "{name}");
        assert_eq!(settings, Settings::default(), "{name}");
        a.event("UserPromptSubmit", "s1", json!({})).await;
        a.stop().await;
        // The next start can read what the daemon wrote.
        let b = start_in(&data, "work").await;
        assert_eq!(b.welcome().await.2.len(), 1, "{name}");
        b.stop().await;
    }
}

#[tokio::test]
async fn ended_agents_that_were_visible_long_enough_are_not_restored() {
    let d = dir();
    let data = d.path().join("deck");
    let a = start_in(&data, "work").await;
    a.event("SessionEnd", "gone", json!({})).await;
    a.event("UserPromptSubmit", "alive", json!({})).await;
    a.stop().await;
    // Make the ended one old by editing the file, as if the daemon had been off for a day.
    let file = data.join("state.json");
    let mut v: Value =
        serde_json::from_str(&std::fs::read_to_string(&file).expect("read")).expect("json");
    for agent in v["agents"].as_array_mut().expect("agents") {
        if agent["id"] == "gone" {
            agent["status_since_ms"] = json!(1);
        }
    }
    std::fs::write(&file, v.to_string()).expect("write");

    let b = start_in(&data, "work").await;
    let ids: Vec<String> = b.welcome().await.2.into_iter().map(|a| a.id).collect();
    assert_eq!(ids, ["alive"]);
    b.stop().await;
}
