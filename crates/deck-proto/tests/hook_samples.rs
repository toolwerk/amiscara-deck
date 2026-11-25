//! Parses every sample from `testdata/hooks` (real inputs recorded in M0).

use deck_proto::hook::{HookEvent, HookInput, PermissionAnswer, SuggestionKind};
use std::path::PathBuf;

fn samples() -> Vec<(String, Vec<u8>)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/hooks");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("testdata/hooks exists") {
        let path = entry.expect("dir entry").path();
        let sample: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read sample"))
                .expect("sample is JSON");
        let name = path
            .file_stem()
            .expect("stem")
            .to_string_lossy()
            .into_owned();
        out.push((name, serde_json::to_vec(&sample["input"]).expect("input")));
    }
    out.sort();
    out
}

fn event_name(e: &HookEvent) -> &'static str {
    match e {
        HookEvent::SessionStart(_) => "SessionStart",
        HookEvent::SessionEnd(_) => "SessionEnd",
        HookEvent::UserPromptSubmit(_) => "UserPromptSubmit",
        HookEvent::PreToolUse(_) => "PreToolUse",
        HookEvent::PostToolUse(_) => "PostToolUse",
        HookEvent::PostToolUseFailure(_) => "PostToolUseFailure",
        HookEvent::PermissionRequest(_) => "PermissionRequest",
        HookEvent::Notification(_) => "Notification",
        HookEvent::SubagentStart(_) => "SubagentStart",
        HookEvent::SubagentStop(_) => "SubagentStop",
        HookEvent::Stop(_) => "Stop",
        _ => "other",
    }
}

#[test]
fn every_sample_parses_to_the_event_in_its_file_name() {
    let all = samples();
    assert!(
        all.len() >= 20,
        "expected the M0 samples, got {}",
        all.len()
    );
    for (name, bytes) in all {
        let input = HookInput::parse(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let prefix = name.split('-').next().expect("prefix");
        assert_eq!(event_name(&input.event), prefix, "{name}");
        assert!(!input.common.session_id.is_empty(), "{name}");
    }
}

#[test]
fn add_rules_suggestion_keeps_raw_value_for_updated_permissions() {
    let all = samples();
    let (_, bytes) = all
        .iter()
        .find(|(n, _)| n == "PermissionRequest-Bash-addRules")
        .expect("sample");
    let input = HookInput::parse(bytes).expect("parse");
    let HookEvent::PermissionRequest(req) = input.event else {
        panic!("not a PermissionRequest")
    };
    assert_eq!(req.tool_name, "Bash");
    let s = &req.permission_suggestions[0];
    match &s.kind {
        SuggestionKind::AddRules {
            rules,
            behavior,
            destination,
        } => {
            assert_eq!(rules[0].tool_name, "Bash");
            assert_eq!(rules[0].rule_content.as_deref(), Some("curl --version"));
            assert_eq!(behavior.as_deref(), Some("allow"));
            assert_eq!(destination.as_deref(), Some("localSettings"));
        }
        other => panic!("unexpected kind {other:?}"),
    }
    let answer = PermissionAnswer::Allow {
        updated_permissions: vec![s.raw.clone()],
    }
    .to_json();
    assert_eq!(
        answer["hookSpecificOutput"]["decision"]["updatedPermissions"][0],
        serde_json::json!({"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "curl --version"}], "behavior": "allow", "destination": "localSettings"})
    );
}

#[test]
fn suggestions_that_are_not_rules_are_not_add_rules() {
    let all = samples();
    let (_, bytes) = all
        .iter()
        .find(|(n, _)| n == "PermissionRequest-Bash-addDirectories")
        .expect("sample");
    let input = HookInput::parse(bytes).expect("parse");
    let HookEvent::PermissionRequest(req) = input.event else {
        panic!("not a PermissionRequest")
    };
    let kinds: Vec<_> = req.permission_suggestions.iter().map(|s| &s.kind).collect();
    assert!(matches!(kinds[0], SuggestionKind::AddDirectories { .. }));
    assert!(matches!(kinds[1], SuggestionKind::SetMode { .. }));
}

#[test]
fn permission_request_has_no_tool_use_id() {
    let all = samples();
    let (_, bytes) = all
        .iter()
        .find(|(n, _)| n == "PermissionRequest-Write-setMode")
        .expect("sample");
    let v: serde_json::Value = serde_json::from_slice(bytes).expect("json");
    assert!(v.get("tool_use_id").is_none());
}

#[test]
fn unknown_fields_and_events_do_not_fail() {
    let input =
        HookInput::parse(br#"{"session_id":"s","hook_event_name":"Stop","new_field":{"a":1}}"#)
            .expect("parse");
    assert!(matches!(input.event, HookEvent::Stop(_)));
    let input = HookInput::parse(br#"{"session_id":"s","hook_event_name":"FutureEvent","x":1}"#)
        .expect("parse");
    assert!(matches!(input.event, HookEvent::Unknown { ref name, .. } if name == "FutureEvent"));
}

#[test]
fn unknown_suggestion_type_is_kept_as_other() {
    let json = br#"{"session_id":"s","hook_event_name":"PermissionRequest","tool_name":"X","tool_input":{},
        "permission_suggestions":[{"type":"somethingNew","foo":1}]}"#;
    let input = HookInput::parse(json).expect("parse");
    let HookEvent::PermissionRequest(req) = input.event else {
        panic!()
    };
    assert_eq!(
        req.permission_suggestions[0].kind,
        SuggestionKind::Other("somethingNew".into())
    );
    assert_eq!(req.permission_suggestions[0].raw["foo"], 1);
}

#[test]
fn broken_input_is_an_error_not_a_panic() {
    assert!(HookInput::parse(b"not json").is_err());
    assert!(HookInput::parse(b"{}").is_err());
    assert!(HookInput::parse(br#"{"hook_event_name":"Stop"}"#).is_err()); // no session_id
    assert!(HookInput::parse(br#"{"session_id":"s","hook_event_name":"PreToolUse"}"#).is_err()); // no tool_name
}

#[test]
fn answers_have_the_documented_shape() {
    assert_eq!(
        PermissionAnswer::Allow {
            updated_permissions: vec![]
        }
        .to_json(),
        serde_json::json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}})
    );
    let deny = PermissionAnswer::Deny {
        message: "Im Amiscara Deck abgelehnt".into(),
    }
    .to_json();
    assert_eq!(deny["hookSpecificOutput"]["decision"]["behavior"], "deny");
    assert_eq!(
        deny["hookSpecificOutput"]["decision"]["message"],
        "Im Amiscara Deck abgelehnt"
    );
    assert!(
        deny["hookSpecificOutput"]["decision"]
            .get("interrupt")
            .is_none()
    );
}
