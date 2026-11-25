//! Which UI gets a notification, and when it is shown, moved or taken back.

#![cfg(unix)]

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use deck_daemon::{Config, Daemon};
use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::{RequestState, Tool};
use deck_proto::message::{
    ApprovalMode, DaemonMessage, HookEnv, HookMessage, NotifyKind, Parent, Presence, Settings,
    UiMessage, decode_line, encode_line,
};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const GHOSTTY: &str = "/Applications/Ghostty.app";
const SAFARI: &str = "/Applications/Safari.app";

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
        host: "h".into(),
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis() as u64
}

impl Running {
    async fn stop(mut self) {
        let _ = self.stop.take().expect("stop").send(());
        self.task.await.expect("task");
    }

    /// A PermissionRequest from an agent that runs under Ghostty.
    fn ask(&self, session: &str) -> JoinHandle<()> {
        self.ask_as(Tool::Claude, session)
    }

    fn ask_as(&self, tool: Tool, session: &str) -> JoinHandle<()> {
        let name = format!(
            "{}/../../testdata/hooks/PermissionRequest-Bash-addRules.json",
            env!("CARGO_MANIFEST_DIR")
        );
        let raw: Value =
            serde_json::from_slice(&std::fs::read(name).expect("sample")).expect("json");
        let mut input = raw["input"].clone();
        input["session_id"] = json!(session);
        let env = HookEnv {
            parents: vec![
                Parent {
                    pid: 10,
                    name: "claude".into(),
                },
                Parent {
                    pid: 8,
                    name: format!("{GHOSTTY}/Contents/MacOS/ghostty"),
                },
            ],
            ..HookEnv::default()
        };
        let line = encode_line(&HookMessage {
            protocol: PROTOCOL_VERSION,
            tool,
            env,
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
        self.next_within(Duration::from_secs(5))
            .await
            .expect("a message within 5 s")
    }

    async fn next_within(&mut self, t: Duration) -> Option<Option<DaemonMessage>> {
        let mut line = String::new();
        let n = tokio::time::timeout(t, self.reader.read_line(&mut line))
            .await
            .ok()?
            .expect("read");
        Some((n > 0).then(|| decode_line(&line).expect("decode")))
    }

    async fn send(&mut self, msg: UiMessage) {
        self.writer
            .write_all(encode_line(&msg).expect("encode").as_bytes())
            .await
            .expect("send");
    }

    async fn present(&mut self, app: &str, idle_ms: u64, locked: bool, focus: bool) {
        self.send(UiMessage::Presence(Presence {
            screen_locked: locked,
            idle_secs: idle_ms / 1000,
            frontmost_app: Some(app.to_owned()),
            focus_mode: focus,
            last_input_ms: now_ms() - idle_ms,
        }))
        .await;
    }

    /// Wait for the held card (so the request exists), then collect what comes in `quiet` time.
    async fn until_held(&mut self) {
        loop {
            if let DaemonMessage::AgentChanged(a) = self.next().await.expect("open")
                && a.request
                    .as_ref()
                    .is_some_and(|r| r.state == RequestState::Held)
            {
                return;
            }
        }
    }

    /// The next Notify or Withdraw, skipping state messages; None if nothing comes in `t`.
    async fn next_notice(&mut self, t: Duration) -> Option<DaemonMessage> {
        let end = tokio::time::Instant::now() + t;
        loop {
            let left = end.saturating_duration_since(tokio::time::Instant::now());
            match self.next_within(left).await {
                None => return None,
                Some(None) => panic!("connection ended"),
                Some(Some(m @ (DaemonMessage::Notify { .. } | DaemonMessage::Withdraw { .. }))) => {
                    return Some(m);
                }
                Some(Some(_)) => {}
            }
        }
    }
}

const QUIET: Duration = Duration::from_millis(500);
const LONG: Duration = Duration::from_secs(5);

fn notify(kind: NotifyKind) -> impl Fn(&Option<DaemonMessage>) -> bool {
    move |m| matches!(m, Some(DaemonMessage::Notify { kind: k, .. }) if *k == kind)
}

#[tokio::test]
async fn at_the_terminal_in_auto_nothing_is_notified() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.present(GHOSTTY, 2_000, false, false).await;
    d.ask("s1");
    ui.until_held().await;
    assert_eq!(
        ui.next_notice(QUIET).await,
        None,
        "the dialog in the terminal is in front of the user"
    );
    d.stop().await;
}

#[tokio::test]
async fn another_app_in_front_notifies_with_buttons() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.present(SAFARI, 2_000, false, false).await;
    d.ask("s1");
    let n = ui.next_notice(LONG).await;
    assert!(notify(NotifyKind::Held)(&n), "{n:?}");
    let Some(DaemonMessage::Notify {
        agent_id,
        request_id,
        ..
    }) = n
    else {
        panic!()
    };
    assert_eq!(agent_id, "s1");
    assert!(request_id.is_some());
    d.stop().await;
}

#[tokio::test]
async fn a_locked_screen_or_a_long_idle_time_notifies_with_the_terminal_in_front() {
    for (idle, locked) in [(1_000, true), (3 * 60_000, false)] {
        let d = start().await;
        let mut ui = Ui::connect(&d.socket).await;
        ui.present(GHOSTTY, idle, locked, false).await;
        d.ask("s1");
        let n = ui.next_notice(LONG).await;
        assert!(
            notify(NotifyKind::Held)(&n),
            "idle {idle} locked {locked}: {n:?}"
        );
        d.stop().await;
    }
}

#[tokio::test]
async fn a_focus_mode_is_never_broken_through_in_any_position_of_the_switch() {
    for mode in [ApprovalMode::Auto, ApprovalMode::Deck] {
        let d = start().await;
        let mut ui = Ui::connect(&d.socket).await;
        ui.send(UiMessage::SetSettings(Settings {
            mode,
            wait_secs: 120,
            retention_days: 7,
            codex_approvals: false,
        }))
        .await;
        ui.present(SAFARI, 2_000, false, true).await;
        d.ask("s1");
        ui.until_held().await;
        assert_eq!(
            ui.next_notice(QUIET).await,
            None,
            "{mode:?}: the card is there, the banner is not"
        );
        d.stop().await;
    }
}

#[tokio::test]
async fn deck_mode_notifies_even_at_the_terminal() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Deck,
        wait_secs: 120,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    ui.present(GHOSTTY, 1_000, false, false).await;
    d.ask("s1");
    let n = ui.next_notice(LONG).await;
    assert!(notify(NotifyKind::Held)(&n), "{n:?}");
    d.stop().await;
}

#[tokio::test]
async fn without_a_report_the_ui_is_notified() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    d.ask("s1");
    let n = ui.next_notice(LONG).await;
    assert!(
        notify(NotifyKind::Held)(&n),
        "no report: Deck rather notifies once too often: {n:?}"
    );
    d.stop().await;
}

#[tokio::test]
async fn the_terminal_mode_holds_nothing_and_notifies_nothing() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Terminal,
        wait_secs: 120,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    ui.present(SAFARI, 1_000, false, false).await;
    d.ask("s1");
    assert_eq!(ui.next_notice(QUIET).await, None);
    d.stop().await;
}

#[tokio::test]
async fn leaving_the_terminal_notifies_and_coming_back_takes_the_notification_back() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.present(GHOSTTY, 1_000, false, false).await;
    d.ask("s1");
    ui.until_held().await;
    assert_eq!(ui.next_notice(QUIET).await, None);

    ui.present(SAFARI, 500, false, false).await; // the user goes elsewhere
    let n = ui.next_notice(LONG).await;
    assert!(notify(NotifyKind::Held)(&n), "{n:?}");
    let Some(DaemonMessage::Notify {
        request_id: Some(id),
        ..
    }) = n
    else {
        panic!()
    };

    ui.present(GHOSTTY, 500, false, false).await; // and comes back
    let w = ui.next_notice(LONG).await;
    assert_eq!(w, Some(DaemonMessage::Withdraw { request_id: id }));
    assert_eq!(ui.next_notice(QUIET).await, None, "once is enough");
    d.stop().await;
}

#[tokio::test]
async fn the_same_picture_twice_does_not_notify_twice() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.present(SAFARI, 2_000, false, false).await;
    d.ask("s1");
    assert!(notify(NotifyKind::Held)(&ui.next_notice(LONG).await));
    for _ in 0..3 {
        ui.present(SAFARI, 2_000, false, false).await;
    }
    assert_eq!(ui.next_notice(QUIET).await, None);
    d.stop().await;
}

#[tokio::test]
async fn when_the_time_runs_out_the_waiting_in_the_terminal_is_notified_without_buttons() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    ui.send(UiMessage::SetSettings(Settings {
        mode: ApprovalMode::Auto,
        wait_secs: 1,
        retention_days: 7,
        codex_approvals: false,
    }))
    .await;
    ui.present(SAFARI, 2_000, false, false).await;
    d.ask("s1");
    assert!(notify(NotifyKind::Held)(&ui.next_notice(LONG).await));
    let n = ui.next_notice(LONG).await;
    assert!(
        notify(NotifyKind::InTerminal)(&n),
        "after the time: waits in the terminal: {n:?}"
    );
    d.stop().await;
}

#[tokio::test]
async fn of_two_uis_only_the_one_where_the_user_was_last_is_notified() {
    let d = start().await;
    let mut laptop = Ui::connect(&d.socket).await;
    let mut desk = Ui::connect(&d.socket).await;
    laptop.present(SAFARI, 120_000, false, false).await; // idle for two minutes
    desk.present(SAFARI, 1_000, false, false).await; // just typed here
    tokio::time::sleep(Duration::from_millis(200)).await;
    d.ask("s1");
    assert!(
        notify(NotifyKind::Held)(&desk.next_notice(LONG).await),
        "the desk gets it"
    );
    assert_eq!(laptop.next_notice(QUIET).await, None, "the laptop does not");
    d.stop().await;
}

#[tokio::test]
async fn when_the_user_moves_to_the_other_ui_the_notification_moves_along() {
    let d = start().await;
    let mut laptop = Ui::connect(&d.socket).await;
    let mut desk = Ui::connect(&d.socket).await;
    laptop.present(SAFARI, 120_000, false, false).await;
    desk.present(SAFARI, 1_000, false, false).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    d.ask("s1");
    assert!(notify(NotifyKind::Held)(&desk.next_notice(LONG).await));

    laptop.present(SAFARI, 100, false, false).await; // now the user types on the laptop
    desk.present(SAFARI, 120_000, false, false).await;
    assert!(
        notify(NotifyKind::Held)(&laptop.next_notice(LONG).await),
        "the laptop takes over"
    );
    assert!(
        matches!(
            desk.next_notice(LONG).await,
            Some(DaemonMessage::Withdraw { .. })
        ),
        "the desk lets go"
    );
    d.stop().await;
}

#[tokio::test]
async fn a_report_that_stops_coming_is_not_believed() {
    // Not a time-based test (the limit is 45 s): a UI that never reported counts as "unknown".
    let d = start().await;
    let mut a = Ui::connect(&d.socket).await;
    let mut b = Ui::connect(&d.socket).await;
    b.present(GHOSTTY, 1_000, false, false).await; // at the terminal, but nobody can tell the daemon about a
    tokio::time::sleep(Duration::from_millis(200)).await;
    d.ask("s1");
    // b is the present UI and is at the terminal: quiet for everybody.
    assert_eq!(b.next_notice(QUIET).await, None);
    assert_eq!(a.next_notice(QUIET).await, None);
    d.stop().await;
}

async fn codex_on(ui: &mut Ui, mode: ApprovalMode, on: bool) {
    ui.send(UiMessage::SetSettings(Settings {
        mode,
        wait_secs: 120,
        retention_days: 7,
        codex_approvals: on,
    }))
    .await;
}

/// The next state of the request of an agent that the UI hears about.
async fn request_state(ui: &mut Ui) -> RequestState {
    loop {
        if let DaemonMessage::AgentChanged(a) = ui.next().await.expect("open")
            && let Some(r) = a.request
        {
            return r.state;
        }
    }
}

#[tokio::test]
async fn codex_is_only_held_when_its_approvals_are_on_and_deck_would_notify() {
    // Off: never held, whatever the switch says.
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    codex_on(&mut ui, ApprovalMode::Deck, false).await;
    let hook = d.ask_as(Tool::Codex, "c1");
    assert_eq!(request_state(&mut ui).await, RequestState::PassedThrough);
    tokio::time::timeout(LONG, hook)
        .await
        .expect("released")
        .expect("task");
    d.stop().await;

    // On, "Automatik", the user at the terminal: the dialog must show at once.
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    codex_on(&mut ui, ApprovalMode::Auto, true).await;
    ui.present(GHOSTTY, 2_000, false, false).await;
    tokio::time::sleep(QUIET).await;
    let hook = d.ask_as(Tool::Codex, "c2");
    assert_eq!(request_state(&mut ui).await, RequestState::PassedThrough);
    tokio::time::timeout(LONG, hook)
        .await
        .expect("released")
        .expect("task");
    d.stop().await;

    // On, another app in front: held and announced.
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    codex_on(&mut ui, ApprovalMode::Auto, true).await;
    ui.present(SAFARI, 2_000, false, false).await;
    tokio::time::sleep(QUIET).await;
    let _hook = d.ask_as(Tool::Codex, "c3");
    ui.until_held().await;
    d.stop().await;
}

#[tokio::test]
async fn a_held_codex_request_is_given_back_when_the_user_returns() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    codex_on(&mut ui, ApprovalMode::Auto, true).await;
    ui.present(SAFARI, 2_000, false, false).await;
    tokio::time::sleep(QUIET).await;
    let hook = d.ask_as(Tool::Codex, "c4");
    ui.until_held().await;
    ui.present(GHOSTTY, 500, false, false).await;
    assert_eq!(request_state(&mut ui).await, RequestState::PassedThrough);
    tokio::time::timeout(LONG, hook)
        .await
        .expect("released")
        .expect("task");
    d.stop().await;
}

#[tokio::test]
async fn switching_codex_approvals_off_lets_go_of_what_is_held() {
    let d = start().await;
    let mut ui = Ui::connect(&d.socket).await;
    codex_on(&mut ui, ApprovalMode::Deck, true).await;
    tokio::time::sleep(QUIET).await;
    let hook = d.ask_as(Tool::Codex, "c5");
    ui.until_held().await;
    codex_on(&mut ui, ApprovalMode::Deck, false).await;
    assert_eq!(request_state(&mut ui).await, RequestState::PassedThrough);
    tokio::time::timeout(LONG, hook)
        .await
        .expect("released")
        .expect("task");
    d.stop().await;
}
