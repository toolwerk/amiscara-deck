use deck_proto::agent::{Location, Status, Tool};
use deck_proto::hook::HookInput;
use deck_session::{Change, EXITED_VISIBLE_MS, Incoming, Store};
use serde_json::{Value, json};

const S: &str = "s1";

fn ev(name: &str, extra: Value) -> HookInput {
    let mut v = json!({"session_id": S, "hook_event_name": name, "cwd": "/work/app"});
    v.as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    HookInput::parse(&serde_json::to_vec(&v).expect("json")).expect("parse")
}

fn inc(input: HookInput, now_ms: u64) -> Incoming {
    Incoming {
        input,
        tool: Tool::Claude,
        now_ms,
        location: Location::Unknown,
        branch: None,
        hold: false,
        wait_ms: 0,
    }
}

fn send(store: &mut Store, name: &str, extra: Value, now: u64) -> Vec<Change> {
    store.apply(inc(ev(name, extra), now))
}

fn status(store: &Store) -> Status {
    store.get(S).expect("agent").status
}

#[test]
fn a_session_walks_through_its_states() {
    let mut st = Store::new("work");
    send(&mut st, "SessionStart", json!({"source": "startup"}), 1000);
    assert_eq!(status(&st), Status::Idle);
    let a = st.get(S).expect("agent");
    assert_eq!(
        (a.project.as_deref(), a.host.as_str()),
        (Some("app"), "work")
    );

    send(
        &mut st,
        "UserPromptSubmit",
        json!({"prompt": "Fix it"}),
        2000,
    );
    assert_eq!(status(&st), Status::Working);
    assert_eq!(
        st.get(S).expect("agent").last_prompt.as_deref(),
        Some("Fix it")
    );
    assert_eq!(st.get(S).expect("agent").turns, Some(1));
    assert_eq!(st.get(S).expect("agent").status_since_ms, 2000);

    send(
        &mut st,
        "PreToolUse",
        json!({"tool_name": "Edit", "tool_input": {"file_path": "/work/app/src/import.rs"}}),
        3000,
    );
    assert_eq!(
        st.get(S).expect("agent").activity.as_deref(),
        Some("bearbeitet src/import.rs")
    );

    send(
        &mut st,
        "PermissionRequest",
        json!({"tool_name": "Bash", "tool_input": {"command": "ls"}}),
        4000,
    );
    assert_eq!(status(&st), Status::Blocked);
    assert_eq!(st.get(S).expect("agent").status_since_ms, 4000);

    send(
        &mut st,
        "PostToolUse",
        json!({"tool_name": "Bash", "tool_input": {}}),
        5000,
    );
    assert_eq!(status(&st), Status::Working);

    send(&mut st, "Stop", json!({"background_tasks": []}), 6000);
    assert_eq!(status(&st), Status::Done);
    assert_eq!(st.get(S).expect("agent").activity, None);

    send(
        &mut st,
        "Notification",
        json!({"notification_type": "idle_prompt"}),
        66_000,
    );
    assert_eq!(status(&st), Status::Idle);

    send(&mut st, "SessionEnd", json!({"reason": "other"}), 70_000);
    assert_eq!(status(&st), Status::Exited);
}

#[test]
fn status_since_only_moves_when_the_status_changes() {
    let mut st = Store::new("work");
    send(&mut st, "UserPromptSubmit", json!({}), 1000);
    send(
        &mut st,
        "PreToolUse",
        json!({"tool_name": "Read", "tool_input": {"file_path": "/work/app/a"}}),
        2000,
    );
    send(
        &mut st,
        "PreToolUse",
        json!({"tool_name": "Read", "tool_input": {"file_path": "/work/app/b"}}),
        3000,
    );
    assert_eq!(st.get(S).expect("agent").status_since_ms, 1000);
}

#[test]
fn an_event_that_changes_nothing_reports_no_change() {
    let mut st = Store::new("work");
    send(&mut st, "UserPromptSubmit", json!({"prompt": "x"}), 1000);
    let first = send(
        &mut st,
        "PostToolUse",
        json!({"tool_name": "Read", "tool_input": {}}),
        1500,
    );
    assert!(first.is_empty());
    let again = send(
        &mut st,
        "PostToolUse",
        json!({"tool_name": "Read", "tool_input": {}}),
        1600,
    );
    assert!(again.is_empty());
}

#[test]
fn an_event_for_an_unknown_session_creates_the_agent() {
    let mut st = Store::new("work");
    let changes = send(
        &mut st,
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_input": {"command": "cargo test"}}),
        500,
    );
    assert_eq!(changes.len(), 1);
    assert_eq!(status(&st), Status::Working);
    assert_eq!(
        st.get(S).expect("agent").activity.as_deref(),
        Some("führt aus: cargo test")
    );
}

#[test]
fn a_stop_with_background_tasks_pauses_the_agent() {
    let mut st = Store::new("work");
    send(
        &mut st,
        "Stop",
        json!({"background_tasks": [{"id": "x", "status": "running"}]}),
        1000,
    );
    let a = st.get(S).expect("agent");
    assert_eq!(a.status, Status::Done);
    assert!(a.paused);
    send(&mut st, "UserPromptSubmit", json!({}), 2000);
    assert!(!st.get(S).expect("agent").paused);
}

#[test]
fn a_subagent_stop_does_not_end_the_main_agent() {
    let mut st = Store::new("work");
    send(&mut st, "UserPromptSubmit", json!({}), 1000);
    send(&mut st, "SubagentStart", json!({"agent_id": "a1"}), 1100);
    assert_eq!(st.get(S).expect("agent").subagents, 1);
    send(&mut st, "Stop", json!({"agent_id": "a1"}), 1200);
    assert_eq!(status(&st), Status::Working);
    send(&mut st, "SubagentStop", json!({"agent_id": "a1"}), 1300);
    assert_eq!(st.get(S).expect("agent").subagents, 0);
    send(&mut st, "SubagentStop", json!({"agent_id": "a1"}), 1400);
    assert_eq!(st.get(S).expect("agent").subagents, 0, "never below zero");
}

#[test]
fn session_start_resets_a_known_session() {
    let mut st = Store::new("work");
    send(&mut st, "UserPromptSubmit", json!({"prompt": "old"}), 1000);
    send(&mut st, "SubagentStart", json!({"agent_id": "a1"}), 1100);
    send(&mut st, "SessionStart", json!({"source": "clear"}), 2000);
    let a = st.get(S).expect("agent");
    assert_eq!(
        (a.status, a.last_prompt.clone(), a.turns, a.subagents),
        (Status::Idle, None, None, 0)
    );
}

#[test]
fn permission_prompt_notification_blocks_and_idle_prompt_only_follows_done() {
    let mut st = Store::new("work");
    send(&mut st, "UserPromptSubmit", json!({}), 1000);
    send(
        &mut st,
        "Notification",
        json!({"notification_type": "idle_prompt"}),
        2000,
    );
    assert_eq!(
        status(&st),
        Status::Working,
        "idle_prompt does not touch a working agent"
    );
    send(
        &mut st,
        "Notification",
        json!({"notification_type": "permission_prompt"}),
        3000,
    );
    assert_eq!(status(&st), Status::Blocked);
}

#[test]
fn compaction_shows_as_activity() {
    let mut st = Store::new("work");
    send(&mut st, "PreCompact", json!({}), 1000);
    assert_eq!(
        st.get(S).expect("agent").activity.as_deref(),
        Some("komprimiert")
    );
    send(&mut st, "PostCompact", json!({}), 2000);
    assert_eq!(st.get(S).expect("agent").activity, None);
}

#[test]
fn stop_failure_is_an_error() {
    let mut st = Store::new("work");
    send(&mut st, "UserPromptSubmit", json!({}), 1000);
    send(&mut st, "StopFailure", json!({}), 2000);
    assert_eq!(status(&st), Status::Error);
}

#[test]
fn working_folder_and_branch_follow_the_events() {
    let mut st = Store::new("work");
    send(&mut st, "SessionStart", json!({}), 1000);
    let mut i = inc(ev("CwdChanged", json!({"cwd": "/work/other"})), 2000);
    i.branch = Some("feature/x".into());
    st.apply(i);
    let a = st.get(S).expect("agent");
    assert_eq!(
        (a.cwd.as_deref(), a.project.as_deref(), a.branch.as_deref()),
        (Some("/work/other"), Some("other"), Some("feature/x"))
    );
}

#[test]
fn location_is_kept_once_known() {
    let mut st = Store::new("work");
    let mut i = inc(ev("SessionStart", json!({})), 1000);
    i.location = Location::Herdr {
        pane_id: "w1:p1".into(),
    };
    st.apply(i);
    send(&mut st, "UserPromptSubmit", json!({}), 2000);
    assert_eq!(
        st.get(S).expect("agent").location,
        Location::Herdr {
            pane_id: "w1:p1".into()
        }
    );
}

#[test]
fn an_ended_agent_stays_visible_for_ten_minutes() {
    let mut st = Store::new("work");
    send(&mut st, "SessionEnd", json!({}), 1000);
    assert!(st.expire(1000 + EXITED_VISIBLE_MS - 1).is_empty());
    let gone = st.expire(1000 + EXITED_VISIBLE_MS);
    assert_eq!(gone, vec![Change::Removed { id: S.into() }]);
    assert!(st.get(S).is_none());
}

#[test]
fn unknown_events_are_ignored() {
    let mut st = Store::new("work");
    assert!(send(&mut st, "SomethingNew", json!({}), 1000).is_empty());
    assert!(st.agents().is_empty());
}

#[test]
fn long_commands_are_shortened_to_one_line() {
    let mut st = Store::new("work");
    let long = format!("echo {}\nsecond line", "x".repeat(200));
    send(
        &mut st,
        "PreToolUse",
        json!({"tool_name": "Bash", "tool_input": {"command": long}}),
        1000,
    );
    let act = st
        .get(S)
        .expect("agent")
        .activity
        .clone()
        .expect("activity");
    assert!(act.chars().count() <= 95 && !act.contains('\n'), "{act}");
    assert!(act.ends_with('…'));
}

#[test]
fn the_history_records_the_steps_and_collapses_repeats() {
    let mut st = Store::new("work");
    send(&mut st, "SessionStart", json!({}), 1000);
    send(&mut st, "UserPromptSubmit", json!({"prompt": "x"}), 2000);
    for (i, f) in ["a", "b"].iter().enumerate() {
        send(
            &mut st,
            "PreToolUse",
            json!({"tool_name": "Read", "tool_input": {"file_path": format!("/work/app/{f}")}}),
            3000 + i as u64,
        );
    }
    send(&mut st, "Stop", json!({}), 4000);
    let texts: Vec<String> = st
        .get(S)
        .expect("agent")
        .history
        .iter()
        .map(|e| e.text.clone())
        .collect();
    assert_eq!(texts.first().map(String::as_str), Some("Sitzung gestartet"));
    assert_eq!(texts.last().map(String::as_str), Some("Zug beendet"));
    assert_eq!(texts.len(), 5, "{texts:?}");
}

#[test]
fn history_is_capped_and_pruned_by_retention() {
    let mut st = Store::new("work");
    for i in 0..60u64 {
        send(
            &mut st,
            "PreToolUse",
            json!({"tool_name": "Read", "tool_input": {"file_path": format!("/work/app/f{i}")}}),
            1000 + i,
        );
    }
    assert_eq!(
        st.get(S).expect("agent").history.len(),
        deck_session::HISTORY_MAX
    );
    let day = 24 * 60 * 60 * 1000;
    let changes = st.set_retention(1, 1000 + 2 * day);
    assert_eq!(changes.len(), 1);
    assert!(st.get(S).expect("agent").history.is_empty());
}

#[test]
fn clearing_forgets_the_history() {
    let mut st = Store::new("work");
    send(&mut st, "SessionStart", json!({}), 1000);
    assert_eq!(st.clear_history().len(), 1);
    assert!(st.get(S).expect("agent").history.is_empty());
    assert!(st.clear_history().is_empty());
}
