#![cfg(unix)]
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use deck_daemon::{Config, Daemon};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::Tool;
use deck_proto::message::{HookEnv, HookMessage, encode_line};
use serde_json::Value;
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
    async fn stop(&mut self) {
        let _ = self.stop.take().expect("stop").send(());
        // `run` is consumed by the task; wait for it to end.
        let task = std::mem::replace(&mut self.task, tokio::spawn(async {}));
        task.await.expect("task");
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

fn cli(args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_amiscara-deck"));
    // The texts of these tests are German.
    c.args(args).env_clear().env("DECK_LANGUAGE", "de");
    c
}

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let o = cli(args).output().expect("run");
    (
        o.status.code(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

#[test]
fn without_a_daemon_list_and_watch_say_so_and_fail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("none.sock");
    for sub in ["list", "watch"] {
        let (code, out, err) = run(&[sub, "--socket", socket.to_str().expect("utf8")]);
        assert_eq!((code, out.as_str()), (Some(1), ""), "{sub}");
        assert!(
            err.contains("Kein Dienst") && err.contains("amiscara-deckd"),
            "{sub}: {err}"
        );
    }
}

#[test]
fn unknown_options_give_code_2() {
    assert_eq!(run(&["list", "--bogus"]).0, Some(2));
    assert_eq!(run(&["watch", "--bogus"]).0, Some(2));
    assert_eq!(run(&["list"]).0, Some(2), "no home folder and no --socket");
}

#[tokio::test(flavor = "multi_thread")]
async fn list_shows_the_agents_the_daemon_knows() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let (code, out, _) = run(&["list", "--socket", &socket]);
    assert_eq!((code, out.as_str()), (Some(0), "Keine Agenten.\n"));

    d.hook("UserPromptSubmit").await;
    d.hook("PreToolUse-Bash").await;
    let (code, out, _) = run(&["list", "--socket", &socket]);
    assert_eq!(code, Some(0));
    assert!(out.starts_with("STATUS"), "{out}");
    assert!(
        out.contains("arbeitet") && out.contains("project") && out.contains("führt aus: curl"),
        "{out}"
    );

    let (code, out, _) = run(&["list", "--socket", &socket, "--json"]);
    assert_eq!(code, Some(0));
    let v: Value = serde_json::from_str(out.trim()).expect("JSON");
    // The two samples come from different recorded sessions.
    let agents = v.as_array().expect("array");
    assert_eq!(agents.len(), 2);
    assert!(
        agents
            .iter()
            .all(|a| a["status"] == "working" && a["host"] == "testhost")
    );
    d.stop().await;
}

/// Lines of a child's stdout, as they come.
fn lines_of(child: &mut Child) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    let out = child.stdout.take().expect("stdout");
    std::thread::spawn(move || {
        for l in BufReader::new(out).lines().map_while(Result::ok) {
            if tx.send(l).is_err() {
                break;
            }
        }
    });
    rx
}

fn next_line(rx: &mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(10))
        .expect("a line within 10 s")
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_prints_the_state_and_then_every_change() {
    let mut d = start().await;
    d.hook("UserPromptSubmit").await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut child = cli(&["watch", "--socket", &socket])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    let lines = lines_of(&mut child);
    assert!(next_line(&lines).starts_with("STATUS"));
    assert!(next_line(&lines).contains("arbeitet"));

    d.hook("Stop").await;
    let l = next_line(&lines);
    assert!(l.starts_with("project: arbeitet → fertig"), "{l}");

    d.stop().await;
    let status = child.wait().expect("wait");
    assert_eq!(status.code(), Some(1), "the daemon is gone");
    let mut err = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().expect("stderr"), &mut err)
        .expect("stderr");
    assert!(err.contains("Verbindung beendet"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn watch_json_prints_one_json_line_per_message() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut child = cli(&["watch", "--socket", &socket, "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    let lines = lines_of(&mut child);
    let first: Value = serde_json::from_str(&next_line(&lines)).expect("JSON");
    assert_eq!(first.as_array().map(Vec::len), Some(0));

    d.hook("UserPromptSubmit").await;
    let msg: Value = serde_json::from_str(&next_line(&lines)).expect("JSON");
    assert_eq!(msg["type"], "agent_changed");
    assert_eq!(msg["status"], "working");
    d.stop().await;
    let _ = child.wait();
}

// answer

impl Running {
    /// A hook program waiting for the answer, in a task.
    fn hook_task(
        &self,
        sample: &str,
        session: &str,
    ) -> tokio::task::JoinHandle<deck_proto::message::HookReply> {
        let name = format!(
            "{}/../../testdata/hooks/{sample}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let mut input = raw["input"].clone();
        input["session_id"] = Value::String(session.into());
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
            s.read_to_string(&mut out).await.expect("read");
            deck_proto::message::decode_line(&out).expect("reply")
        })
    }
}

/// The id of the held request, as `list --json` shows it.
fn held_request(socket: &str) -> Option<String> {
    let (_, out, _) = run(&["list", "--socket", socket, "--json"]);
    let v: Value = serde_json::from_str(out.trim()).ok()?;
    v.as_array()?
        .iter()
        .find_map(|a| a["request"]["id"].as_str().map(str::to_owned))
}

async fn wait_for_held(socket: &str) -> String {
    for _ in 0..50 {
        if let Some(id) = held_request(socket) {
            return id;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("no held request appeared");
}

/// A UI that stays connected, so requests are held.
async fn stay_connected(socket: &str) -> Child {
    let mut child = cli(&["watch", "--socket", socket, "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    let lines = lines_of(&mut child);
    next_line(&lines); // the state: the UI is connected
    std::mem::forget(lines);
    child
}

#[tokio::test(flavor = "multi_thread")]
async fn answer_allows_a_held_request_and_the_hook_gets_it() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut ui = stay_connected(&socket).await;
    let hook = d.hook_task("PermissionRequest-Bash-addRules", "s-answer");
    let id = wait_for_held(&socket).await;

    let (_, listing, _) = run(&["list", "--socket", &socket]);
    assert!(
        listing.contains("wartet") && listing.contains(&format!("fragt: curl --version [{id}]")),
        "{listing}"
    );

    let (code, out, err) = run(&["answer", &id, "allow", "--always", "0", "--socket", &socket]);
    assert_eq!(
        (code, out.as_str(), err.as_str()),
        (Some(0), "Erlaubt.\n", "")
    );
    let reply = hook.await.expect("hook");
    let decision = &reply.output.expect("output")["hookSpecificOutput"]["decision"];
    assert_eq!(decision["behavior"], "allow");
    assert_eq!(
        decision["updatedPermissions"][0]["rules"][0]["ruleContent"],
        "curl --version"
    );

    // Answered: a second answer is refused.
    let (code, _, err) = run(&["answer", &id, "allow", "--socket", &socket]);
    assert_eq!(code, Some(1));
    assert!(err.contains("nicht gehalten"), "{err}");
    let _ = ui.kill();
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn answer_denies_with_a_message() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut ui = stay_connected(&socket).await;
    let hook = d.hook_task("PermissionRequest-Bash-addRules", "s-deny");
    let id = wait_for_held(&socket).await;
    let (code, out, _) = run(&[
        "answer",
        &id,
        "deny",
        "--message",
        "Lieber nicht",
        "--socket",
        &socket,
    ]);
    assert_eq!((code, out.as_str()), (Some(0), "Abgelehnt.\n"));
    let decision =
        hook.await.expect("hook").output.expect("output")["hookSpecificOutput"]["decision"].clone();
    assert_eq!(
        (decision["behavior"].as_str(), decision["message"].as_str()),
        (Some("deny"), Some("Lieber nicht"))
    );
    let _ = ui.kill();
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn answer_refuses_what_makes_no_sense() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut ui = stay_connected(&socket).await;
    let _hook = d.hook_task("PermissionRequest-Write-setMode", "s-bad");
    let id = wait_for_held(&socket).await;

    let (code, _, err) = run(&["answer", "r999", "allow", "--socket", &socket]);
    assert_eq!(code, Some(1));
    assert!(err.contains("Keine Anfrage"), "{err}");

    let (code, _, err) = run(&[
        "answer", &id, "allow", "--always", "0", "--seen", "--socket", &socket,
    ]);
    assert_eq!(code, Some(1));
    assert!(
        err.contains("nicht angeboten") && err.contains("keine"),
        "{err}"
    );

    for args in [
        vec!["answer"],
        vec!["answer", "r1"],
        vec!["answer", "r1", "maybe", "--socket", &socket],
        vec![
            "answer",
            "r1",
            "allow",
            "--message",
            "x",
            "--socket",
            &socket,
        ],
        vec!["answer", "r1", "deny", "--always", "1", "--socket", &socket],
        vec![
            "answer", "r1", "allow", "--always", "x", "--socket", &socket,
        ],
    ] {
        assert_eq!(run(&args).0, Some(2), "{args:?}");
    }
    let _ = ui.kill();
    d.stop().await;
}

#[test]
fn answer_without_a_daemon_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("none.sock");
    let (code, _, err) = run(&[
        "answer",
        "r1",
        "allow",
        "--socket",
        socket.to_str().expect("utf8"),
    ]);
    assert_eq!(code, Some(1));
    assert!(err.contains("Kein Dienst"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn answer_shows_the_whole_text_of_a_shortened_card_and_allows_only_with_seen() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut ui = stay_connected(&socket).await;
    let hook = d.hook_task("PermissionRequest-Write-setMode", "s-seen");
    let id = wait_for_held(&socket).await;

    let (code, out, err) = run(&["answer", &id, "allow", "--socket", &socket]);
    assert_eq!((code, out.as_str()), (Some(1), ""));
    assert!(
        err.contains("gekürzt") && err.contains("m0-write.txt") && err.contains("--seen"),
        "{err}"
    );
    assert!(
        !hook.is_finished(),
        "nothing was sent: the hook still waits"
    );

    // After reading it, with --seen, it goes through.
    let (code, out, _) = run(&["answer", &id, "allow", "--seen", "--socket", &socket]);
    assert_eq!((code, out.as_str()), (Some(0), "Erlaubt.\n"));
    assert!(hook.await.expect("hook").output.is_some());
    let _ = ui.kill();
    d.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_never_needs_seen() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let mut ui = stay_connected(&socket).await;
    let hook = d.hook_task("PermissionRequest-Write-setMode", "s-deny-short");
    let id = wait_for_held(&socket).await;
    let (code, out, _) = run(&["answer", &id, "deny", "--socket", &socket]);
    assert_eq!((code, out.as_str()), (Some(0), "Abgelehnt.\n"));
    assert!(hook.await.expect("hook").output.is_some());
    let _ = ui.kill();
    d.stop().await;
}

// sim

#[tokio::test(flavor = "multi_thread")]
async fn sim_plays_a_scenario_against_the_daemon() {
    let mut d = start().await;
    let socket = d.socket.to_str().expect("utf8").to_owned();
    let (code, out, err) = run(&[
        "sim", "demo", "--speed", "1000", "--wait", "1", "--socket", &socket,
    ]);
    assert_eq!((code, err.as_str()), (Some(0), ""), "{out}");
    assert!(
        out.contains("sim-app: SessionStart") && out.contains("sim-api: UserPromptSubmit"),
        "{out}"
    );
    assert!(
        out.contains("sim-app: Anfrage weiter ohne Antwort"),
        "no UI answered: {out}"
    );

    let (_, listing, _) = run(&["list", "--socket", &socket]);
    assert!(
        listing.contains("shop-app")
            && listing.contains("api-server")
            && listing.contains("beendet"),
        "{listing}"
    );
    d.stop().await;
}

#[test]
fn sim_lists_its_scenarios_and_refuses_unknown_ones() {
    let (code, out, _) = run(&["sim", "--list"]);
    assert_eq!(code, Some(0));
    assert!(
        out.lines().any(|l| l == "demo") && out.lines().any(|l| l == "permission"),
        "{out}"
    );
    let (code, _, err) = run(&["sim", "nope", "--socket", "/x"]);
    assert_eq!(code, Some(2));
    assert!(
        err.contains("Unbekanntes Szenario") && err.contains("demo"),
        "{err}"
    );
    assert_eq!(run(&["sim", "--speed", "0"]).0, Some(2));
    assert_eq!(run(&["sim", "--speed", "x"]).0, Some(2));
    assert_eq!(run(&["sim", "--bogus"]).0, Some(2));
}

#[test]
fn sim_without_a_daemon_says_so() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("none.sock");
    let (code, _, err) = run(&[
        "sim",
        "busy",
        "--speed",
        "1000",
        "--socket",
        socket.to_str().expect("utf8"),
    ]);
    assert_eq!(code, Some(1));
    assert!(err.contains("Kein Dienst"), "{err}");
}
