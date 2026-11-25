//! Replays the real hook inputs recorded in M0 (`testdata/hooks`) through the
//! state machine, in order, repeated, shuffled and interleaved.

use deck_proto::agent::{Location, Status, Tool};
use deck_proto::hook::HookInput;
use deck_proto::message::Decision;
use deck_session::{Change, Incoming, Store};
use serde_json::Value;
use std::path::PathBuf;

fn sample(name: &str, session: &str) -> HookInput {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/hooks")
        .join(format!("{name}.json"));
    let raw: Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap_or_else(|e| panic!("{name}: {e}")))
            .expect("sample is JSON");
    let mut input = raw["input"].clone();
    // The samples come from different sessions: put them into one.
    input["session_id"] = Value::String(session.into());
    input["cwd"] = Value::String("/work/project".into());
    HookInput::parse(&serde_json::to_vec(&input).expect("json")).expect("parse")
}

struct Run {
    store: Store,
    now: u64,
    hold: bool,
    all: Vec<Change>,
}

impl Run {
    fn new(hold: bool) -> Self {
        Run {
            store: Store::new("work"),
            now: 1_000_000,
            hold,
            all: Vec::new(),
        }
    }

    fn feed(&mut self, name: &str, session: &str) -> Vec<Change> {
        self.now += 1000;
        let changes = self.store.apply(Incoming {
            input: sample(name, session),
            tool: Tool::Claude,
            now_ms: self.now,
            location: Location::Unknown,
            branch: None,
            hold: self.hold,
            wait_ms: 120_000,
        });
        self.all.extend(changes.clone());
        changes
    }

    fn status(&self, session: &str) -> Status {
        self.store.get(session).expect("agent").status
    }
}

const S: &str = "session-a";

#[test]
fn a_whole_session_with_an_approval_in_deck() {
    let mut r = Run::new(true);
    r.feed("SessionStart", S);
    assert_eq!(r.status(S), Status::Idle);
    r.feed("UserPromptSubmit", S);
    assert_eq!(r.status(S), Status::Working);
    r.feed("PreToolUse-Bash", S);
    assert!(
        r.store
            .get(S)
            .expect("agent")
            .activity
            .as_deref()
            .is_some_and(|a| a.starts_with("führt aus: "))
    );

    r.feed("PermissionRequest-Bash-addRules", S);
    let a = r.store.get(S).expect("agent");
    assert_eq!(a.status, Status::Blocked);
    let req = a.request.clone().expect("request");
    assert_eq!(req.summary, "curl --version");
    assert_eq!(req.always_allow.len(), 1);

    r.feed("Notification-permission_prompt", S);
    assert_eq!(r.status(S), Status::Blocked);

    let done = r
        .store
        .answer(
            &req.id,
            Decision::Allow {
                always: Some(0),
                expanded: false,
            },
        )
        .expect("answer");
    assert_eq!(
        done.output["hookSpecificOutput"]["decision"]["updatedPermissions"][0]["rules"][0]["ruleContent"],
        "curl --version"
    );

    r.feed("PostToolUse-Bash", S);
    let a = r.store.get(S).expect("agent");
    assert_eq!((a.status, a.request.is_none()), (Status::Working, true));

    r.feed("Stop", S);
    assert_eq!(r.status(S), Status::Done);
    r.feed("Notification-idle_prompt", S);
    assert_eq!(r.status(S), Status::Idle);
    r.feed("SessionEnd", S);
    assert_eq!(r.status(S), Status::Exited);
    assert_eq!(r.store.get(S).expect("agent").turns, Some(1));
}

#[test]
fn a_session_where_the_terminal_answers_first() {
    let mut r = Run::new(true);
    r.feed("UserPromptSubmit", S);
    r.feed("PermissionRequest-Bash-addRules", S);
    let id = r
        .store
        .get(S)
        .expect("agent")
        .request
        .clone()
        .expect("request")
        .id;
    let changes = r.feed("PostToolUse-Bash", S);
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, Change::Release { request_id } if *request_id == id))
    );
    assert!(r.store.get(S).expect("agent").request.is_none());
}

#[test]
fn a_failed_tool_keeps_the_agent_working() {
    let mut r = Run::new(false);
    r.feed("UserPromptSubmit", S);
    r.feed("PreToolUse-Read", S);
    r.feed("PostToolUseFailure-Read", S);
    assert_eq!(r.status(S), Status::Working);
}

#[test]
fn a_subagent_run_counts_and_does_not_end_the_main_agent() {
    let mut r = Run::new(false);
    r.feed("UserPromptSubmit", S);
    r.feed("PreToolUse-Agent", S);
    assert_eq!(
        r.store.get(S).expect("agent").activity.as_deref(),
        Some("startet einen Subagenten")
    );
    r.feed("SubagentStart", S);
    assert_eq!(r.store.get(S).expect("agent").subagents, 1);
    r.feed("SubagentStop", S);
    assert_eq!(r.store.get(S).expect("agent").subagents, 0);
    r.feed("PostToolUse-Agent", S);
    assert_eq!(r.status(S), Status::Working);
}

#[test]
fn the_same_event_twice_changes_nothing_the_second_time() {
    for name in [
        "SessionStart",
        "UserPromptSubmit",
        "PreToolUse-Read",
        "PostToolUse-Read",
        "Stop",
        "SessionEnd",
    ] {
        let mut r = Run::new(false);
        r.feed(name, S);
        let again = r.feed(name, S);
        // The status time may move only when the status changes, so a repeat is silent
        // except for events that count (a prompt is a turn).
        if name == "UserPromptSubmit" {
            assert_eq!(r.store.get(S).expect("agent").turns, Some(2), "{name}");
        } else {
            assert!(again.is_empty(), "{name}: {again:?}");
        }
    }
}

#[test]
fn events_in_a_wrong_order_never_break_the_state() {
    let mut r = Run::new(true);
    // Hooks installed mid-session: the first thing we see is the end of a tool.
    r.feed("PostToolUse-Bash", S);
    assert_eq!(r.status(S), Status::Working);
    r.feed("Stop", S);
    r.feed("PreToolUse-Bash", S);
    r.feed("SessionEnd", S);
    r.feed("PreToolUse-Read", S);
    // A late event after the end revives the agent: the session lives on.
    assert_eq!(r.status(S), Status::Working);
}

#[test]
fn an_aborted_session_without_session_end_stays_visible() {
    let mut r = Run::new(false);
    r.feed("UserPromptSubmit", S);
    r.feed("PreToolUse-Bash", S);
    assert!(
        r.store.expire(r.now + 24 * 3600 * 1000).is_empty(),
        "only ended agents expire"
    );
    assert!(r.store.get(S).is_some());
}

#[test]
fn sessions_do_not_mix() {
    let mut r = Run::new(true);
    r.feed("UserPromptSubmit", "a");
    r.feed("UserPromptSubmit", "b");
    r.feed("PermissionRequest-Bash-addRules", "a");
    r.feed("Stop", "b");
    assert_eq!(r.status("a"), Status::Blocked);
    assert_eq!(r.status("b"), Status::Done);
    assert!(r.store.get("b").expect("agent").request.is_none());
    assert_eq!(r.store.agents().len(), 2);
    // The request ids of two sessions differ.
    r.feed("PermissionRequest-Write-setMode", "b");
    let ia = r
        .store
        .get("a")
        .expect("agent")
        .request
        .clone()
        .expect("request")
        .id;
    let ib = r
        .store
        .get("b")
        .expect("agent")
        .request
        .clone()
        .expect("request")
        .id;
    assert_ne!(ia, ib);
}

const ALL: &[&str] = &[
    "Notification-idle_prompt",
    "Notification-permission_prompt",
    "PermissionRequest-Bash-addDirectories",
    "PermissionRequest-Bash-addRules",
    "PermissionRequest-Write-setMode",
    "PostToolUse-Agent",
    "PostToolUse-Bash",
    "PostToolUse-Read",
    "PostToolUse-Write",
    "PostToolUseFailure-Read",
    "PreToolUse-Agent",
    "PreToolUse-Bash",
    "PreToolUse-Read",
    "PreToolUse-Write",
    "SessionEnd",
    "SessionStart",
    "Stop",
    "SubagentStart",
    "SubagentStop",
    "UserPromptSubmit",
];

#[test]
fn every_sample_alone_gives_a_sane_agent() {
    for name in ALL {
        let mut r = Run::new(true);
        r.feed(name, S);
        let a = r.store.get(S).expect("agent");
        assert_eq!(a.id, S, "{name}");
        assert_eq!(a.project.as_deref(), Some("project"), "{name}");
        assert!(a.subagents <= 1, "{name}");
    }
}

/// A small deterministic generator, so the shuffles are the same on every run.
fn next(seed: &mut u64) -> usize {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*seed >> 33) as usize
}

#[test]
fn any_order_of_all_samples_keeps_the_invariants() {
    let mut seed = 42;
    for round in 0..200 {
        let mut r = Run::new(round % 2 == 0);
        for _ in 0..40 {
            let name = ALL[next(&mut seed) % ALL.len()];
            r.feed(name, S);
            let a = r.store.get(S).expect("agent");
            // A request exists only while the agent is blocked or just answered.
            if let Some(req) = &a.request {
                assert!(
                    req.deadline_ms.is_some()
                        == (req.state == deck_proto::agent::RequestState::Held),
                    "round {round} {name}"
                );
            }
            // The counter of subagents never underflows (u32) and the status time never goes back.
            assert!(a.status_since_ms <= r.now, "round {round}");
        }
        // Whatever happened: a final stop leaves nothing open.
        r.feed("SessionEnd", S);
        let a = r.store.get(S).expect("agent");
        assert!(
            a.request.is_none() && a.status == Status::Exited,
            "round {round}"
        );
    }
}

#[test]
fn every_released_request_was_held_before() {
    let mut seed = 7;
    for _ in 0..100 {
        let mut r = Run::new(true);
        let mut held: Vec<String> = Vec::new();
        for _ in 0..40 {
            let name = ALL[next(&mut seed) % ALL.len()];
            let changes = r.feed(name, S);
            for c in &changes {
                match c {
                    Change::Changed(a) => {
                        if let Some(req) = a.request.as_ref().filter(|r| {
                            r.state == deck_proto::agent::RequestState::Held
                                && !held.contains(&r.id)
                        }) {
                            held.push(req.id.clone());
                        }
                    }
                    Change::Release { request_id } => {
                        assert!(
                            held.contains(request_id),
                            "released {request_id} but never held"
                        );
                        held.retain(|h| h != request_id);
                    }
                    Change::Removed { .. } => {}
                }
            }
        }
    }
}
