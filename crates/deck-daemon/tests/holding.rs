//! Holding a PermissionRequest while a UI can answer, and every way it ends.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use deck_daemon::{Config, Daemon};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{Agent, HistoryEvent, RequestState, Status, Tool};
use deck_proto::message::{
    AnswerVia, ApprovalMode, DaemonMessage, Decision, HookEnv, HookMessage, HookReply, Settings,
    UiMessage, decode_line, encode_line,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

struct Running {
    socket: PathBuf,
    _dir: tempfile::TempDir,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
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

    fn hook_line(sample: &str, session: &str, tool: Tool) -> String {
        let name = format!(
            "{}/../../testdata/hooks/{sample}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let mut input = raw["input"].clone();
        input["session_id"] = json!(session);
        encode_line(&HookMessage {
            protocol: PROTOCOL_VERSION,
            tool,
            env: HookEnv::default(),
            input,
        })
        .expect("encode")
    }

    /// A hook program: sends the event and waits for the reply, in a task.
    fn hook(&self, sample: &str, session: &str, tool: Tool) -> JoinHandle<HookReply> {
        self.send_hook(Self::hook_line(sample, session, tool))
    }

    fn send_hook(&self, line: String) -> JoinHandle<HookReply> {
        let socket = self.socket.clone();
        tokio::spawn(async move {
            let mut s = UnixStream::connect(&socket).await.expect("connect");
            s.write_all(line.as_bytes()).await.expect("write");
            let mut out = String::new();
            s.read_to_string(&mut out).await.expect("read");
            decode_line(&out).expect("reply")
        })
    }

    /// An event that is not waited for.
    async fn event(&self, sample: &str, session: &str) {
        self.hook(sample, session, Tool::Claude)
            .await
            .expect("reply");
    }
}

struct Ui {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl Ui {
    async fn connect(socket: &PathBuf) -> Ui {
        let (r, mut w) = UnixStream::connect(socket)
            .await
            .expect("connect")
            .into_split();
        let hello = encode_line(&UiMessage::Hello {
            protocol: PROTOCOL_VERSION,
            name: "t".into(),
        })
        .expect("encode");
        w.write_all(hello.as_bytes()).await.expect("hello");
        let mut ui = Ui {
            reader: BufReader::new(r),
            writer: w,
        };
        assert!(matches!(
            ui.next().await,
            Some(DaemonMessage::Welcome { .. })
        ));
        ui
    }

    async fn next(&mut self) -> Option<DaemonMessage> {
        let mut line = String::new();
        let n = tokio::time::timeout(Duration::from_secs(5), self.reader.read_line(&mut line))
            .await
            .expect("a message within 5 s")
            .expect("read");
        (n > 0).then(|| decode_line(&line).expect("decode"))
    }

    /// The next change of an agent.
    async fn next_agent(&mut self) -> Agent {
        loop {
            match self.next().await {
                Some(DaemonMessage::AgentChanged(a)) => return *a,
                Some(_) => {}
                None => panic!("connection ended"),
            }
        }
    }

    /// Wait for the card of a request in the given state.
    async fn card(&mut self, state: RequestState) -> Agent {
        loop {
            let a = self.next_agent().await;
            if a.request.as_ref().is_some_and(|r| r.state == state) {
                return a;
            }
        }
    }

    async fn send(&mut self, msg: UiMessage) {
        self.writer
            .write_all(encode_line(&msg).expect("encode").as_bytes())
            .await
            .expect("send");
    }

    async fn answer(&mut self, request_id: &str, decision: Decision) {
        self.send(UiMessage::Answer {
            request_id: request_id.into(),
            decision,
            via: AnswerVia::Unknown,
        })
        .await;
    }
}

async fn finished(h: &mut JoinHandle<HookReply>, within: Duration) -> Option<HookReply> {
    tokio::time::timeout(within, h)
        .await
        .ok()
        .map(|r| r.expect("task"))
}

const S: &str = "session-a";

#[tokio::test]
async fn a_request_is_held_until_the_ui_allows_it() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);

    let a = ui.card(RequestState::Held).await;
    let req = a.request.expect("request");
    assert_eq!(a.status, Status::Blocked);
    assert!(req.deadline_ms.is_some(), "a held request has a deadline");
    assert!(
        finished(&mut hook, Duration::from_millis(300))
            .await
            .is_none(),
        "the hook waits"
    );

    ui.answer(
        &req.id,
        Decision::Allow {
            always: None,
            expanded: false,
        },
    )
    .await;
    let reply = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("answered");
    assert_eq!(
        reply.output,
        Some(
            json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}})
        )
    );
    let a = ui.card(RequestState::Answered).await;
    assert_eq!(a.request.expect("request").deadline_ms, None);
    d.stop().await;
}

#[tokio::test]
async fn a_question_and_an_approval_go_back_to_their_own_claude_sessions() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut approval = d.hook(
        "PermissionRequest-Bash-addRules",
        "first-profile",
        Tool::Claude,
    );
    let first = ui.card(RequestState::Held).await;
    assert_eq!(first.id, "first-profile");
    let first_id = first.request.expect("approval").id;

    let line = Running::hook_line(
        "PermissionRequest-Bash-addRules",
        "second-profile",
        Tool::Claude,
    );
    let mut event: HookMessage = decode_line(&line).expect("hook message");
    event.input["tool_name"] = json!("AskUserQuestion");
    event.input["tool_input"] = json!({
        "questions": [{
            "question": "Which option?",
            "options": [{"label": "First"}, {"label": "Second"}]
        }]
    });
    let mut question = d.send_hook(encode_line(&event).expect("question event"));
    let second = ui.card(RequestState::Held).await;
    assert_eq!(second.id, "second-profile");
    let second_id = second.request.expect("question").id;
    assert_ne!(first_id, second_id);

    ui.answer(
        &second_id,
        Decision::Answer {
            picks: vec![vec![1]],
        },
    )
    .await;
    let reply = finished(&mut question, Duration::from_secs(5))
        .await
        .expect("question answered")
        .output
        .expect("answer output");
    assert_eq!(
        reply.pointer("/hookSpecificOutput/decision/updatedInput/answers/Which option?"),
        Some(&json!("Second"))
    );
    assert!(
        finished(&mut approval, Duration::from_millis(150))
            .await
            .is_none()
    );

    ui.answer(
        &first_id,
        Decision::Allow {
            always: None,
            expanded: true,
        },
    )
    .await;
    let reply = finished(&mut approval, Duration::from_secs(5))
        .await
        .expect("approval answered")
        .output
        .expect("approval output");
    assert_eq!(
        reply.pointer("/hookSpecificOutput/decision/behavior"),
        Some(&json!("allow"))
    );
    d.stop().await;
}

#[tokio::test]
async fn deny_gives_the_message_and_never_interrupts() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let id = ui
        .card(RequestState::Held)
        .await
        .request
        .expect("request")
        .id;
    ui.answer(
        &id,
        Decision::Deny {
            message: Some("Nein, danke".into()),
        },
    )
    .await;
    let out = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("answered")
        .output
        .expect("output");
    let decision = &out["hookSpecificOutput"]["decision"];
    assert_eq!(
        (decision["behavior"].as_str(), decision["message"].as_str()),
        (Some("deny"), Some("Nein, danke"))
    );
    assert!(decision.get("interrupt").is_none());
    d.stop().await;
}

#[tokio::test]
async fn always_allow_returns_the_suggestion_of_claude_code_unchanged() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let req = ui.card(RequestState::Held).await.request.expect("request");
    assert_eq!(req.always_allow.len(), 1);
    ui.answer(
        &req.id,
        Decision::Allow {
            always: Some(req.always_allow[0].index),
            expanded: false,
        },
    )
    .await;
    let out = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("answered")
        .output
        .expect("output");
    assert_eq!(
        out["hookSpecificOutput"]["decision"]["updatedPermissions"],
        json!([{"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "curl --version"}],
                "behavior": "allow", "destination": "localSettings"}])
    );
    d.stop().await;
}

#[tokio::test]
async fn a_rule_that_was_not_offered_is_refused_and_the_request_stays_held() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Write-setMode", S, Tool::Claude);
    let id = ui
        .card(RequestState::Held)
        .await
        .request
        .expect("request")
        .id;
    ui.answer(
        &id,
        Decision::Allow {
            always: Some(0),
            expanded: true,
        },
    )
    .await; // setMode is no rule
    assert!(
        finished(&mut hook, Duration::from_millis(400))
            .await
            .is_none(),
        "still waiting"
    );
    ui.answer(
        &id,
        Decision::Allow {
            always: None,
            expanded: true,
        },
    )
    .await;
    assert!(
        finished(&mut hook, Duration::from_secs(5))
            .await
            .expect("answered")
            .output
            .is_some()
    );
    d.stop().await;
}

#[tokio::test]
async fn when_the_time_runs_out_the_hook_is_released_with_nothing() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Auto,
        wait_secs: 1,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    ui.card(RequestState::Held).await;
    let started = Instant::now();
    let reply = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("released");
    assert_eq!(reply.output, None, "the dialog in the terminal stays");
    assert!(
        started.elapsed() >= Duration::from_millis(500),
        "{:?}",
        started.elapsed()
    );
    let a = ui.card(RequestState::PassedThrough).await;
    assert_eq!(a.status, Status::Blocked);
    d.stop().await;
}

#[tokio::test]
async fn when_the_terminal_answers_first_the_hook_is_released_and_the_card_goes() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let id = ui
        .card(RequestState::Held)
        .await
        .request
        .expect("request")
        .id;

    d.event("PostToolUse-Bash", S).await;
    let reply = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("released");
    assert_eq!(reply.output, None);
    loop {
        let a = ui.next_agent().await;
        if a.request.is_none() {
            assert_eq!(a.status, Status::Working);
            break;
        }
    }
    // Too late for Deck: nothing happens.
    ui.answer(
        &id,
        Decision::Allow {
            always: None,
            expanded: false,
        },
    )
    .await;
    d.stop().await;
}

#[tokio::test]
async fn a_hook_program_that_disappears_leaves_the_card_waiting_in_the_terminal() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let line = Running::hook_line("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let mut s = UnixStream::connect(&d.socket).await.expect("connect");
    s.write_all(line.as_bytes()).await.expect("write");
    ui.card(RequestState::Held).await;
    drop(s); // killed, for example because the terminal answered
    let a = ui.card(RequestState::PassedThrough).await;
    assert_eq!(a.request.expect("request").deadline_ms, None);
    d.stop().await;
}

#[tokio::test]
async fn without_a_ui_nothing_is_held_and_when_the_last_ui_leaves_the_hook_is_released() {
    let d = start().await;
    // No UI: answered at once.
    let mut hook = d.hook("PermissionRequest-Bash-addRules", "no-ui", Tool::Claude);
    assert_eq!(
        finished(&mut hook, Duration::from_secs(2))
            .await
            .expect("at once")
            .output,
        None
    );

    let mut ui = Ui::connect(&d.socket).await;
    let mut held = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    ui.card(RequestState::Held).await;
    drop(ui);
    let reply = finished(&mut held, Duration::from_secs(5))
        .await
        .expect("released");
    assert_eq!(reply.output, None);
    d.stop().await;
}

#[tokio::test]
async fn one_of_two_uis_leaving_does_not_release_anything() {
    let d = start().await;
    let mut a = Ui::connect(&d.socket).await;
    let b = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let id = a
        .card(RequestState::Held)
        .await
        .request
        .expect("request")
        .id;
    drop(b);
    assert!(
        finished(&mut hook, Duration::from_millis(400))
            .await
            .is_none(),
        "still held"
    );
    a.answer(
        &id,
        Decision::Allow {
            always: None,
            expanded: false,
        },
    )
    .await;
    assert!(
        finished(&mut hook, Duration::from_secs(5))
            .await
            .expect("answered")
            .output
            .is_some()
    );
    d.stop().await;
}

#[tokio::test]
async fn either_ui_may_answer_and_the_first_answer_counts() {
    let d = start().await;
    let mut a = Ui::connect(&d.socket).await;
    let mut b = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let id = a
        .card(RequestState::Held)
        .await
        .request
        .expect("request")
        .id;
    b.card(RequestState::Held).await;
    b.answer(&id, Decision::Deny { message: None }).await;
    let out = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("answered")
        .output
        .expect("output");
    assert_eq!(out["hookSpecificOutput"]["decision"]["behavior"], "deny");
    a.answer(
        &id,
        Decision::Allow {
            always: None,
            expanded: false,
        },
    )
    .await; // too late, ignored
    let a_state = a.card(RequestState::Answered).await;
    assert_eq!(a_state.request.expect("request").id, id);
    d.stop().await;
}

#[tokio::test]
async fn the_mode_in_the_terminal_holds_nothing_and_lets_go_of_what_is_held() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut held = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    ui.card(RequestState::Held).await;
    ui.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Terminal,
        wait_secs: 120,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    assert_eq!(
        finished(&mut held, Duration::from_secs(5))
            .await
            .expect("released")
            .output,
        None
    );

    let mut next = d.hook("PermissionRequest-Write-setMode", "session-b", Tool::Claude);
    assert_eq!(
        finished(&mut next, Duration::from_secs(2))
            .await
            .expect("at once")
            .output,
        None
    );
    let a = loop {
        let a = ui.next_agent().await;
        if a.id == "session-b" {
            break a;
        }
    };
    assert_eq!(
        a.request.expect("request").state,
        RequestState::PassedThrough
    );
    d.stop().await;
}

#[tokio::test]
async fn settings_are_clamped_and_every_ui_hears_about_them() {
    let d = start().await;
    let mut a = Ui::connect(&d.socket).await;
    let mut b = Ui::connect(&d.socket).await;
    a.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Deck,
        wait_secs: 100_000,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    for ui in [&mut a, &mut b] {
        let Some(DaemonMessage::SettingsChanged(s)) = ui.next().await else {
            panic!("expected SettingsChanged")
        };
        assert_eq!((s.mode, s.wait_secs), (ApprovalMode::Deck, 540));
    }
    a.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Deck,
        wait_secs: 0,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    let Some(DaemonMessage::SettingsChanged(s)) = a.next().await else {
        panic!("expected SettingsChanged")
    };
    assert_eq!(s.wait_secs, 1);
    d.stop().await;
}

#[tokio::test]
async fn codex_is_never_held() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook(
        "PermissionRequest-Bash-addRules",
        "codex-session",
        Tool::Codex,
    );
    assert_eq!(
        finished(&mut hook, Duration::from_secs(2))
            .await
            .expect("at once")
            .output,
        None
    );
    let a = ui.card(RequestState::PassedThrough).await;
    assert_eq!(a.tool, Tool::Codex);
    d.stop().await;
}

#[tokio::test]
async fn stopping_the_daemon_releases_a_waiting_hook() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    ui.card(RequestState::Held).await;
    d.stop().await;
    let reply = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("released");
    assert_eq!(reply.output, None);
}

#[tokio::test]
async fn a_second_request_of_the_same_agent_releases_the_first() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut first = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let id1 = ui
        .card(RequestState::Held)
        .await
        .request
        .expect("request")
        .id;
    let mut second = d.hook("PermissionRequest-Write-setMode", S, Tool::Claude);
    assert_eq!(
        finished(&mut first, Duration::from_secs(5))
            .await
            .expect("released")
            .output,
        None
    );
    let id2 = loop {
        let r = ui.card(RequestState::Held).await.request.expect("request");
        if r.id != id1 {
            break r.id;
        }
    };
    ui.answer(
        &id2,
        Decision::Allow {
            always: None,
            expanded: true,
        },
    )
    .await;
    assert!(
        finished(&mut second, Duration::from_secs(5))
            .await
            .expect("answered")
            .output
            .is_some()
    );
    d.stop().await;
}

#[tokio::test]
async fn every_answer_is_logged_privately_and_clearing_empties_the_log() {
    use std::os::unix::fs::PermissionsExt;
    let d = start().await;
    let log = d._dir.path().join("deck/decisions.jsonl");
    let mut ui = Ui::connect(&d.socket).await;
    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    let req = ui.card(RequestState::Held).await.request.expect("request");
    ui.send(UiMessage::Answer {
        request_id: req.id.clone(),
        decision: Decision::Deny { message: None },
        via: AnswerVia::Notification,
    })
    .await;
    finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("answered");
    let a = ui.card(RequestState::Answered).await;
    assert!(a.history.iter().any(|h| h.event == HistoryEvent::Denied));

    let text = std::fs::read_to_string(&log).expect("log");
    let line: Value = serde_json::from_str(text.lines().next().expect("line")).expect("json");
    assert_eq!(line["decision"], "deny");
    assert_eq!(line["via"], "notification");
    assert_eq!(line["request_id"], req.id.as_str());
    assert_eq!(
        std::fs::metadata(&log).expect("meta").permissions().mode() & 0o777,
        0o600
    );

    ui.send(UiMessage::ClearHistory).await;
    for _ in 0..50 {
        if std::fs::read_to_string(&log).is_ok_and(|t| t.is_empty()) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(std::fs::read_to_string(&log).expect("log"), "");
    d.stop().await;
}

#[tokio::test]
async fn the_retention_is_clamped() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    for (asked, got) in [(0, 1), (100_000, 365)] {
        ui.send(UiMessage::SetSettings(Settings {
            mode: ApprovalMode::Deck,
            wait_secs: 120,
            retention_days: asked,
            codex_approvals: false,
        }))
        .await;
        let Some(DaemonMessage::SettingsChanged(s)) = ui.next().await else {
            panic!("expected SettingsChanged")
        };
        assert_eq!(s.retention_days, got);
    }
    d.stop().await;
}

#[tokio::test]
async fn the_context_fill_comes_from_the_transcript_after_a_turn() {
    use std::io::Write;
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let mut t = tempfile::NamedTempFile::new().expect("tmp");
    writeln!(
        t,
        r#"{{"type":"assistant","message":{{"model":"claude-opus-5-5","usage":{{"input_tokens":5,"cache_creation_input_tokens":95,"cache_read_input_tokens":900}}}}}}"#
    )
    .expect("write");
    let name = format!(
        "{}/../../testdata/hooks/Stop.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw: Value = serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
    let mut input = raw["input"].clone();
    input["session_id"] = json!(S);
    input["transcript_path"] = json!(t.path().to_string_lossy());
    let line = encode_line(&HookMessage {
        protocol: PROTOCOL_VERSION,
        tool: Tool::Claude,
        env: HookEnv::default(),
        input,
    })
    .expect("encode");
    let mut s = UnixStream::connect(&d.socket).await.expect("connect");
    s.write_all(line.as_bytes()).await.expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).await.expect("read");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ui.next())
            .await
            .expect("a message")
            .expect("open");
        if let DaemonMessage::AgentChanged(a) = msg
            && let Some(c) = a.context
        {
            assert_eq!((c.used_tokens, c.window_tokens), (1000, 1_000_000));
            break;
        }
        assert!(Instant::now() < deadline, "no context seen");
    }
    d.stop().await;
}

#[tokio::test]
async fn the_cost_of_a_session_and_of_today_come_from_the_transcript() {
    use std::io::Write;
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let mut t = tempfile::NamedTempFile::new().expect("tmp");
    writeln!(
        t,
        r#"{{"type":"cost-state","sessionId":"x","totalCostUSD":0.25,"startTime":{started}}}"#
    )
    .expect("write");
    let name = format!(
        "{}/../../testdata/hooks/Stop.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw: Value = serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
    let mut input = raw["input"].clone();
    input["session_id"] = json!(S);
    input["transcript_path"] = json!(t.path().to_string_lossy());
    let line = encode_line(&HookMessage {
        protocol: PROTOCOL_VERSION,
        tool: Tool::Claude,
        env: HookEnv::default(),
        input,
    })
    .expect("encode");
    let mut s = UnixStream::connect(&d.socket).await.expect("connect");
    s.write_all(line.as_bytes()).await.expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).await.expect("read");

    let (mut agent_cost, mut today) = (None, None);
    while agent_cost.is_none() || today.is_none() {
        let msg = tokio::time::timeout(Duration::from_secs(5), ui.next())
            .await
            .expect("a message")
            .expect("open");
        match msg {
            DaemonMessage::AgentChanged(a) => agent_cost = a.cost_micro_usd.or(agent_cost),
            DaemonMessage::Costs { today_micro_usd } => today = Some(today_micro_usd),
            _ => {}
        }
    }
    assert_eq!((agent_cost, today), (Some(250_000), Some(250_000)));
    // A UI that comes later hears it in the welcome.
    let late = UnixStream::connect(&d.socket).await.expect("connect");
    let (r, mut w) = late.into_split();
    let hello = encode_line(&UiMessage::Hello {
        protocol: PROTOCOL_VERSION,
        name: "late".into(),
    })
    .expect("encode");
    w.write_all(hello.as_bytes()).await.expect("hello");
    let mut reader = BufReader::new(r);
    let mut first = String::new();
    reader.read_line(&mut first).await.expect("welcome");
    assert!(matches!(
        decode_line(&first).expect("decode"),
        DaemonMessage::Welcome {
            today_micro_usd: Some(250_000),
            ..
        }
    ));
    d.stop().await;
}

#[tokio::test]
async fn a_message_written_after_the_stop_hook_still_counts() {
    use std::io::Write;
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    let line_with = |tokens: u64| {
        format!(
            r#"{{"type":"assistant","message":{{"model":"claude-haiku-5-5","usage":{{"input_tokens":{tokens}}}}}}}"#
        )
    };
    let mut t = tempfile::NamedTempFile::new().expect("tmp");
    writeln!(t, "{}", line_with(100)).expect("write");
    let name = format!(
        "{}/../../testdata/hooks/Stop.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let raw: Value = serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
    let mut input = raw["input"].clone();
    input["session_id"] = json!(S);
    input["transcript_path"] = json!(t.path().to_string_lossy());
    let line = encode_line(&HookMessage {
        protocol: PROTOCOL_VERSION,
        tool: Tool::Claude,
        env: HookEnv::default(),
        input,
    })
    .expect("encode");
    let mut s = UnixStream::connect(&d.socket).await.expect("connect");
    s.write_all(line.as_bytes()).await.expect("write");
    let mut out = String::new();
    s.read_to_string(&mut out).await.expect("read");
    // The last message of the turn arrives only now, as Claude Code does it.
    writeln!(t, "{}", line_with(200)).expect("write");
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(6), ui.next())
            .await
            .expect("a message")
            .expect("open");
        if let DaemonMessage::AgentChanged(a) = msg
            && a.context.as_ref().is_some_and(|c| c.used_tokens == 200)
        {
            break;
        }
        assert!(Instant::now() < deadline, "the late message was missed");
    }
    d.stop().await;
}

#[tokio::test]
async fn the_welcome_names_the_build_and_a_ui_can_end_the_daemon() {
    let d = start().await;
    let s = UnixStream::connect(&d.socket).await.expect("connect");
    let (r, mut w) = s.into_split();
    let hello = encode_line(&UiMessage::Hello {
        protocol: PROTOCOL_VERSION,
        name: "t".into(),
    })
    .expect("encode");
    w.write_all(hello.as_bytes()).await.expect("hello");
    let mut reader = BufReader::new(r);
    let mut first = String::new();
    reader.read_line(&mut first).await.expect("welcome");
    let DaemonMessage::Welcome { build, .. } = decode_line(&first).expect("decode") else {
        panic!("welcome")
    };
    // The daemon runs inside this test program.
    let me = std::env::current_exe().expect("exe");
    assert_eq!(build, deck_proto::build::fingerprint(&me));
    assert!(build.is_some());

    let mut hook = d.hook("PermissionRequest-Bash-addRules", S, Tool::Claude);
    tokio::time::sleep(Duration::from_millis(200)).await;
    w.write_all(
        encode_line(&UiMessage::Shutdown)
            .expect("encode")
            .as_bytes(),
    )
    .await
    .expect("shutdown");
    let reply = finished(&mut hook, Duration::from_secs(5))
        .await
        .expect("let go");
    assert_eq!(reply.output, None, "a held request goes to the terminal");
    tokio::time::timeout(Duration::from_secs(5), d.task)
        .await
        .expect("the daemon ends")
        .expect("task");
    assert!(!d.socket.exists());
}
