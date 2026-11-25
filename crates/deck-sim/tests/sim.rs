use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use deck_client::Client;
use deck_daemon::{Config, Daemon};
use deck_proto::agent::{RequestState, Status};
use deck_proto::hook::{HookEvent, HookInput};
use deck_proto::message::{DaemonMessage, Decision};
use deck_sim::{Event, Options, SCENARIOS, play, scenario};
use tokio::sync::oneshot;

#[test]
fn every_scenario_is_made_of_inputs_the_parser_accepts() {
    for name in SCENARIOS {
        let sessions = scenario(name).unwrap_or_else(|| panic!("{name}"));
        assert!(!sessions.is_empty(), "{name}");
        let mut ids = std::collections::BTreeSet::new();
        for s in &sessions {
            assert!(
                ids.insert(s.id.clone()),
                "{name}: duplicate session {}",
                s.id
            );
            assert!(!s.steps.is_empty());
            for (i, step) in s.steps.iter().enumerate() {
                let bytes = serde_json::to_vec(&step.input).expect("json");
                let input = HookInput::parse(&bytes)
                    .unwrap_or_else(|e| panic!("{name}/{} step {i}: {e}", s.id));
                assert_eq!(input.common.session_id, s.id);
                if i == 0 {
                    assert!(
                        matches!(input.event, HookEvent::SessionStart(_)),
                        "{name}/{}: starts with SessionStart",
                        s.id
                    );
                }
            }
        }
    }
}

#[test]
fn nothing_in_a_scenario_points_at_a_real_machine() {
    for name in SCENARIOS {
        for s in scenario(name).expect("scenario") {
            for step in s.steps {
                let text = step.input.to_string();
                assert!(
                    !text.contains("/Users/staude") && !text.contains("staude"),
                    "{name}: {text}"
                );
            }
        }
    }
}

#[test]
fn an_unknown_scenario_is_none() {
    assert!(scenario("nope").is_none());
}

struct Running {
    socket: std::path::PathBuf,
    _dir: tempfile::TempDir,
    stop: Option<oneshot::Sender<()>>,
    task: tokio::task::JoinHandle<()>,
}

async fn start() -> Running {
    let dir = tempfile::tempdir().expect("tempdir");
    let daemon = Daemon::bind(Config {
        data_dir: dir.path().join("deck"),
        host: "simhost".into(),
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
}

fn recorder() -> (
    Arc<Mutex<Vec<Event>>>,
    impl Fn(Event) + Send + Sync + Clone + 'static,
) {
    let log = Arc::new(Mutex::new(Vec::new()));
    let l = log.clone();
    (log, move |e| l.lock().expect("lock").push(e))
}

#[tokio::test]
async fn the_demo_plays_through_and_leaves_three_agents_in_three_states() {
    let d = start().await;
    let (log, report) = recorder();
    let opts = Options {
        speed: 1000.0,
        permission_wait: Duration::from_millis(300),
    };
    play(&d.socket, scenario("demo").expect("demo"), &opts, report).await;

    let c = Client::connect(&d.socket, "t").await.expect("connect");
    let agents = &c.state().agents;
    assert_eq!(agents["sim-app"].status, Status::Done);
    assert_eq!(agents["sim-api"].status, Status::Working);
    assert_eq!(agents["sim-docs"].status, Status::Exited);
    assert_eq!(agents["sim-app"].project.as_deref(), Some("shop-app"));
    assert!(
        log.lock()
            .expect("lock")
            .iter()
            .any(|e| matches!(e, Event::Answered { .. })),
        "the request was reported"
    );
    d.stop().await;
}

#[tokio::test]
async fn a_request_waits_for_the_user_and_the_answer_reaches_the_simulated_agent() {
    let d = start().await;
    let mut ui = Client::connect(&d.socket, "ui").await.expect("connect");
    let (log, report) = recorder();
    let socket = d.socket.clone();
    let player = tokio::spawn(async move {
        let opts = Options {
            speed: 1000.0,
            permission_wait: Duration::from_secs(30),
        };
        play(
            &socket,
            scenario("permission").expect("permission"),
            &opts,
            report,
        )
        .await;
    });
    // Answer every request that is held, like a user would.
    let mut answered = 0;
    let deadline = Instant::now() + Duration::from_secs(20);
    while answered < 2 && Instant::now() < deadline {
        let msg = tokio::time::timeout(Duration::from_secs(5), ui.next())
            .await
            .expect("a message")
            .expect("open");
        if let DaemonMessage::AgentChanged(a) = msg
            && let Some(r) = a.request
            && r.state == RequestState::Held
        {
            ui.answer(
                &r.id,
                Decision::Allow {
                    always: None,
                    expanded: true,
                },
                deck_proto::message::AnswerVia::Unknown,
            )
            .await
            .expect("answer");
            answered += 1;
        }
    }
    player.await.expect("player");
    assert_eq!(
        answered, 2,
        "both requests of the scenario were held and answered"
    );
    let outputs: Vec<_> = log
        .lock()
        .expect("lock")
        .iter()
        .filter_map(|e| match e {
            Event::Answered { output, .. } => Some(output.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(outputs.len(), 2);
    assert!(outputs.iter().all(|o| {
        o.as_ref()
            .is_some_and(|v| v["hookSpecificOutput"]["decision"]["behavior"] == "allow")
    }));
    d.stop().await;
}

#[tokio::test]
async fn speed_shortens_the_waits() {
    let d = start().await;
    let (_, report) = recorder();
    let start = Instant::now();
    play(
        &d.socket,
        scenario("busy").expect("busy"),
        &Options {
            speed: 2000.0,
            ..Options::default()
        },
        report,
    )
    .await;
    assert!(
        start.elapsed() < Duration::from_secs(3),
        "{:?}",
        start.elapsed()
    );
    let c = Client::connect(&d.socket, "t").await.expect("connect");
    assert_eq!(c.state().agents.len(), 8);
    d.stop().await;
}

#[tokio::test]
async fn without_a_daemon_the_player_says_so_and_returns() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (log, report) = recorder();
    let start = Instant::now();
    play(
        &dir.path().join("none.sock"),
        scenario("demo").expect("demo"),
        &Options {
            speed: 1000.0,
            ..Options::default()
        },
        report,
    )
    .await;
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(log.lock().expect("lock").contains(&Event::NoDaemon));
}
