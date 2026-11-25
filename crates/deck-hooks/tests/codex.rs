use std::path::Path;

use deck_hooks::{
    CODEX_EVENTS, EventState, PERMISSION_TIMEOUT_SECS, Target, install_for, status_for,
    uninstall_for,
};
use serde_json::{Value, json};

const PROGRAM: &str = "/opt/deck/deck-hook";

fn read(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).expect("read")).expect("json")
}

/// Like the real file of a user: foreign hooks of other programs.
const FOREIGN: &str = r#"{
  "hooks": {
    "PermissionRequest": [
      {"hooks": [{"command": "/usr/bin/other --answer", "type": "command", "timeout": 40}], "matcher": ""}
    ]
  }
}"#;

#[test]
fn codex_gets_read_only_entries_that_name_the_tool_and_keep_foreign_ones() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("hooks.json");
    std::fs::write(&file, FOREIGN).expect("write");
    let out = install_for(Target::Codex, &file, Path::new(PROGRAM)).expect("install");
    assert!(out.changed);
    let v = read(&file);
    for event in CODEX_EVENTS {
        let groups = v["hooks"][*event].as_array().expect("event");
        let ours: Vec<&Value> = groups
            .iter()
            .flat_map(|g| g["hooks"].as_array().expect("hooks"))
            .filter(|h| h["command"] == "/opt/deck/deck-hook codex")
            .collect();
        assert_eq!(ours.len(), 1, "{event}");
        let limit = if *event == "PermissionRequest" {
            PERMISSION_TIMEOUT_SECS
        } else {
            10
        };
        assert_eq!(ours[0]["timeout"], limit, "{event}");
        assert!(ours[0].get("async").is_none(), "no async for Codex");
    }
    assert_eq!(
        v["hooks"]["PermissionRequest"][0],
        serde_json::from_str::<Value>(FOREIGN).expect("json")["hooks"]["PermissionRequest"][0]
    );
    assert!(
        v["hooks"].get("PreCompact").is_none(),
        "only the Codex events"
    );
}

#[test]
fn codex_status_install_again_and_uninstall() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("hooks.json");
    let before = status_for(Target::Codex, &file, Path::new(PROGRAM)).expect("status");
    assert!(!before.file_exists);
    assert!(before.events.iter().all(|(_, s)| *s == EventState::Missing));

    install_for(Target::Codex, &file, Path::new(PROGRAM)).expect("install");
    let after = status_for(Target::Codex, &file, Path::new(PROGRAM)).expect("status");
    assert!(after.fully_installed());
    let again = install_for(Target::Codex, &file, Path::new(PROGRAM)).expect("again");
    assert!(!again.changed, "idempotent");

    uninstall_for(Target::Codex, &file, Path::new(PROGRAM)).expect("uninstall");
    let gone = status_for(Target::Codex, &file, Path::new(PROGRAM)).expect("status");
    assert!(gone.events.iter().all(|(_, s)| *s == EventState::Missing));
    assert_eq!(read(&file), json!({}));
}

#[test]
fn removing_ours_keeps_the_places_of_the_entries_behind() {
    let dir = tempfile::tempdir().expect("tmp");
    let file = dir.path().join("hooks.json");
    install_for(Target::Codex, &file, Path::new(PROGRAM)).expect("install");
    // Another program appends its entry after ours.
    let mut v = read(&file);
    let foreign = json!({"hooks": [{"type": "command", "command": "/usr/bin/other"}]});
    v["hooks"]["Stop"]
        .as_array_mut()
        .expect("array")
        .push(foreign.clone());
    std::fs::write(&file, v.to_string()).expect("write");

    uninstall_for(Target::Codex, &file, Path::new(PROGRAM)).expect("uninstall");
    let after = read(&file);
    assert_eq!(
        after["hooks"]["Stop"],
        json!([{"hooks": []}, foreign]),
        "the foreign entry keeps its place and so its trust"
    );
    assert!(
        after["hooks"].get("SessionStart").is_none(),
        "nothing behind ours: the event goes"
    );
    let status = status_for(Target::Codex, &file, Path::new(PROGRAM)).expect("status");
    assert!(status.events.iter().all(|(_, s)| *s == EventState::Missing));
    assert_eq!(status.foreign_entries, 1);
}
