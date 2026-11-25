use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::*;
use deck_proto::message::*;

fn agent() -> Agent {
    Agent {
        id: "s1".into(),
        host: "work".into(),
        tool: Tool::Claude,
        cwd: Some("/Users/example/dev/app".into()),
        project: Some("app".into()),
        branch: Some("main".into()),
        title: None,
        status: Status::Blocked,
        paused: false,
        status_since_ms: 1_700_000_000_000,
        activity: Some(Activity::Edit {
            path: Some("src/import.rs".into()),
        }),
        last_prompt: Some("Fix the import".into()),
        context: Some(ContextUsage {
            used_tokens: 39_600,
            window_tokens: 1_000_000,
        }),
        turns: Some(3),
        subagents: 1,
        location: Location::Herdr {
            pane_id: "w1:p1".into(),
        },
        request: Some(Request {
            id: "r1".into(),
            tool_name: "Bash".into(),
            summary: "curl --version".into(),
            truncated: false,
            full: "curl --version".into(),
            headings: Vec::new(),
            extra: vec!["dangerouslyDisableSandbox=true".into()],
            always_allow: vec![AlwaysAllow {
                index: 0,
                rules: vec!["Bash(curl --version)".into()],
                destination: Some("localSettings".into()),
            }],
            state: RequestState::Held,
            deadline_ms: Some(1_700_000_120_000),
            questions: Vec::new(),
        }),
        cost_micro_usd: Some(1_250_000),
        history: vec![
            HistoryEntry {
                at_ms: 1_700_000_000_500,
                event: HistoryEvent::PromptReceived,
            },
            HistoryEntry {
                at_ms: 1_700_000_000_600,
                event: HistoryEvent::Activity(Activity::Subagent),
            },
        ],
    }
}

fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    v: T,
) {
    let line = encode_line(&v).expect("encode");
    assert!(
        line.ends_with('\n') && line.matches('\n').count() == 1,
        "one line: {line:?}"
    );
    let back: T = decode_line(&line).expect("decode");
    assert_eq!(back, v);
}

/// Activity and history are data the UIs word themselves (no sentences of
/// one language in the protocol). Every form survives the way over the socket.
#[test]
fn every_activity_and_history_event_survives_a_round_trip() {
    let activities = vec![
        Activity::Run {
            command: Some("cargo test".into()),
        },
        Activity::Run { command: None },
        Activity::Edit {
            path: Some("a.rs".into()),
        },
        Activity::Read { path: None },
        Activity::Search {
            pattern: Some("fn main".into()),
        },
        Activity::Subagent,
        Activity::Web,
        Activity::Compacting,
        Activity::Tool {
            name: "mcp__x".into(),
        },
    ];
    for a in &activities {
        roundtrip(a.clone());
        roundtrip(HistoryEvent::Activity(a.clone()));
    }
    for e in [
        HistoryEvent::SessionStarted,
        HistoryEvent::PromptReceived,
        HistoryEvent::PermissionRequested,
        HistoryEvent::Compacting,
        HistoryEvent::TurnEnded,
        HistoryEvent::TurnFailed,
        HistoryEvent::SessionEnded,
        HistoryEvent::Allowed,
        HistoryEvent::Denied,
        HistoryEvent::Answered,
    ] {
        roundtrip(e);
    }
    let json = serde_json::to_value(HistoryEvent::Activity(Activity::Run {
        command: Some("ls".into()),
    }))
    .expect("json");
    assert_eq!(
        json,
        serde_json::json!({"event": "activity", "kind": "run", "command": "ls"})
    );
}

#[test]
fn every_message_survives_a_round_trip() {
    roundtrip(UiMessage::Hello {
        protocol: PROTOCOL_VERSION,
        name: "menubar".into(),
    });
    roundtrip(UiMessage::Answer {
        request_id: "r1".into(),
        decision: Decision::Allow {
            always: Some(0),
            expanded: false,
        },
        via: AnswerVia::Notification,
    });
    roundtrip(UiMessage::Answer {
        request_id: "r1".into(),
        decision: Decision::Deny { message: None },
        via: AnswerVia::Cli,
    });
    roundtrip(UiMessage::ClearHistory);
    roundtrip(UiMessage::SetSettings(Settings::default()));
    roundtrip(UiMessage::Presence(Presence {
        screen_locked: false,
        idle_secs: 4,
        frontmost_app: Some("Terminal".into()),
        focus_mode: false,
        last_input_ms: 1,
    }));
    roundtrip(DaemonMessage::Welcome {
        protocol: PROTOCOL_VERSION,
        host: "work".into(),
        settings: Settings::default(),
        agents: vec![agent()],
        today_micro_usd: Some(7),
        build: Some("abc".into()),
    });
    roundtrip(DaemonMessage::VersionMismatch { daemon: 2, ui: 1 });
    roundtrip(DaemonMessage::AgentChanged(Box::new(agent())));
    roundtrip(DaemonMessage::AgentRemoved { id: "s1".into() });
    roundtrip(DaemonMessage::SettingsChanged(Settings {
        mode: ApprovalMode::Terminal,
        wait_secs: 30,
        retention_days: 30,
        codex_approvals: false,
    }));
    roundtrip(DaemonMessage::Notify {
        agent_id: "s1".into(),
        request_id: Some("r1".into()),
        kind: NotifyKind::Held,
    });
    roundtrip(HookMessage {
        protocol: PROTOCOL_VERSION,
        tool: Tool::Codex,
        env: HookEnv {
            herdr_pane_id: Some("w1:p1".into()),
            tmux_pane: None,
            term_program: None,
            parents: vec![Parent {
                pid: 1,
                name: "claude".into(),
            }],
        },
        input: serde_json::json!({"hook_event_name": "Stop", "session_id": "s"}),
    });
    roundtrip(HookReply { output: None });
    roundtrip(HookReply {
        output: Some(serde_json::json!({"a": 1})),
    });
}

#[test]
fn wire_format_uses_stable_names() {
    let line = encode_line(&UiMessage::Answer {
        request_id: "r1".into(),
        decision: Decision::Allow {
            always: None,
            expanded: false,
        },
        via: AnswerVia::Unknown,
    })
    .expect("encode");
    assert_eq!(
        line,
        "{\"type\":\"answer\",\"request_id\":\"r1\",\"decision\":{\"decision\":\"allow\",\"always\":null,\"expanded\":false},\"via\":\"unknown\"}\n"
    );
    assert_eq!(
        serde_json::to_string(&Status::Blocked).expect("json"),
        "\"blocked\""
    );
    assert_eq!(
        serde_json::to_string(&ApprovalMode::Auto).expect("json"),
        "\"auto\""
    );
    assert_eq!(
        serde_json::to_string(&Location::Unknown).expect("json"),
        "{\"kind\":\"unknown\"}"
    );
}

#[test]
fn unknown_fields_from_a_newer_peer_are_ignored() {
    let line = r#"{"type":"agent_removed","id":"s1","added_later":true}"#;
    let msg: DaemonMessage = decode_line(line).expect("decode");
    assert_eq!(msg, DaemonMessage::AgentRemoved { id: "s1".into() });
}

#[test]
fn protocol_numbers_must_match() {
    assert!(check_protocol(PROTOCOL_VERSION).is_ok());
    assert!(check_protocol(PROTOCOL_VERSION + 1).is_err());
}

#[test]
fn the_state_carries_no_machine_local_path_except_for_display() {
    // `cwd` is shown to the user, nothing else holds a path: the daemon never
    // asks a UI to open a local file. This test pins the field list.
    let v = serde_json::to_value(agent()).expect("json");
    let paths: Vec<_> = v
        .as_object()
        .expect("object")
        .keys()
        .filter(|k| k.contains("path") || k.contains("file"))
        .collect();
    assert!(paths.is_empty(), "{paths:?}");
}

#[test]
fn a_message_of_a_newer_daemon_is_ignored_not_an_error() {
    let m: DaemonMessage = decode_line("{\"type\":\"something_new\",\"x\":1}\n").expect("decode");
    assert_eq!(m, DaemonMessage::Unknown);
}

#[test]
fn an_older_welcome_has_no_cost() {
    let line = "{\"type\":\"welcome\",\"protocol\":1,\"host\":\"h\",\"settings\":{\"mode\":\"auto\",\"wait_secs\":120},\"agents\":[]}\n";
    let DaemonMessage::Welcome {
        today_micro_usd,
        settings,
        ..
    } = decode_line(line).expect("decode")
    else {
        panic!("welcome")
    };
    assert_eq!(today_micro_usd, None);
    assert!(!settings.codex_approvals);
}
