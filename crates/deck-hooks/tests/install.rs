use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use deck_hooks::{EVENTS, Error, EventState, PERMISSION_TIMEOUT_SECS, install, status, uninstall};
use serde_json::{Value, json};

const PROGRAM: &str = "/opt/deck/deck-hook";

struct Env {
    dir: tempfile::TempDir,
    settings: PathBuf,
}

fn env_with(content: Option<&str>) -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = dir.path().join("settings.json");
    if let Some(c) = content {
        std::fs::write(&settings, c).expect("write");
    }
    Env { dir, settings }
}

fn json_of(p: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).expect("read")).expect("json")
}

fn leftovers(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(dir)
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("deck-tmp"))
        .collect();
    v.sort();
    v
}

fn backups(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bak"))
        .collect();
    v.sort();
    v
}

/// A settings file like a real one: foreign hooks, other keys, own order.
const FOREIGN: &str = r#"{
  "model": "opus",
  "permissions": {
    "allow": [
      "Bash(ls *)"
    ]
  },
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "/Users/example/.config/status-line"
          }
        ]
      }
    ],
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "/usr/local/bin/audit",
            "timeout": 5
          }
        ]
      }
    ]
  },
  "note": "Grüße aus Neuendettelsau"
}
"#;

#[test]
fn installing_into_a_missing_file_adds_every_event() {
    let e = env_with(None);
    let out = install(&e.settings, Path::new(PROGRAM)).expect("install");
    assert!(out.changed);
    assert_eq!(out.backup, None, "there was nothing to back up");
    let v = json_of(&e.settings);
    for event in EVENTS {
        let groups = v["hooks"][event]
            .as_array()
            .unwrap_or_else(|| panic!("{event}"));
        assert_eq!(groups.len(), 1, "{event}");
        let hook = &groups[0]["hooks"][0];
        assert_eq!(hook["type"], "command");
        assert_eq!(hook["command"], PROGRAM);
        if *event == "PermissionRequest" {
            assert_eq!(hook["timeout"], PERMISSION_TIMEOUT_SECS);
            assert!(hook.get("async").is_none(), "the answer must be waited for");
        } else {
            assert_eq!(hook["async"], true, "{event}");
            assert!(hook.get("timeout").is_none());
        }
    }
    for event in [
        "PreToolUse",
        "PostToolUse",
        "PostToolUseFailure",
        "PermissionRequest",
    ] {
        assert_eq!(v["hooks"][event][0]["matcher"], "*");
    }
    assert!(v["hooks"]["Stop"][0].get("matcher").is_none());
    assert!(leftovers(e.dir.path()).is_empty());
}

#[test]
fn installing_twice_gives_the_same_file_and_no_second_backup() {
    let e = env_with(Some(FOREIGN));
    install(&e.settings, Path::new(PROGRAM)).expect("first");
    let after_first = std::fs::read(&e.settings).expect("read");
    let backups_after_first = backups(e.dir.path());
    let again = install(&e.settings, Path::new(PROGRAM)).expect("second");
    assert!(!again.changed && again.backup.is_none());
    assert_eq!(std::fs::read(&e.settings).expect("read"), after_first);
    assert_eq!(backups(e.dir.path()), backups_after_first);
}

#[test]
fn foreign_entries_other_keys_and_order_stay() {
    let e = env_with(Some(FOREIGN));
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    let text = std::fs::read_to_string(&e.settings).expect("read");
    let v: Value = serde_json::from_str(&text).expect("json");
    // Top-level keys keep their order, ours comes into the existing section.
    let keys: Vec<_> = v.as_object().expect("object").keys().cloned().collect();
    assert_eq!(keys, ["model", "permissions", "hooks", "note"]);
    assert_eq!(v["note"], "Grüße aus Neuendettelsau", "Unicode survives");
    assert!(text.contains("Grüße"), "not escaped");
    // The foreign hook stays first in its event, ours comes after it.
    assert_eq!(
        v["hooks"]["Stop"][0]["hooks"][0]["command"],
        "/Users/example/.config/status-line"
    );
    assert_eq!(v["hooks"]["Stop"][1]["hooks"][0]["command"], PROGRAM);
    assert_eq!(
        v["hooks"]["PreToolUse"][0]["hooks"][0]["command"],
        "/usr/local/bin/audit"
    );
    assert_eq!(v["hooks"]["PreToolUse"][0]["hooks"][0]["timeout"], 5);
    assert_eq!(v["hooks"]["PreToolUse"][1]["hooks"][0]["command"], PROGRAM);
    // Events that were there keep their place before the ones we added.
    let events: Vec<_> = v["hooks"]
        .as_object()
        .expect("hooks")
        .keys()
        .take(2)
        .cloned()
        .collect();
    assert_eq!(events, ["Stop", "PreToolUse"]);
}

#[test]
fn uninstall_after_install_gives_back_the_original_file_byte_for_byte() {
    let e = env_with(Some(FOREIGN));
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    let out = uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
    assert!(out.changed);
    assert_eq!(std::fs::read_to_string(&e.settings).expect("read"), FOREIGN);
}

#[test]
fn uninstall_of_a_file_that_was_only_ours_removes_the_empty_section() {
    let e = env_with(Some("{\n  \"model\": \"opus\"\n}\n"));
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
    assert_eq!(
        std::fs::read_to_string(&e.settings).expect("read"),
        "{\n  \"model\": \"opus\"\n}\n"
    );
}

#[test]
fn uninstall_without_anything_of_ours_touches_nothing() {
    let e = env_with(Some(FOREIGN));
    let out = uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
    assert!(!out.changed && out.backup.is_none());
    assert_eq!(std::fs::read_to_string(&e.settings).expect("read"), FOREIGN);
    let missing = env_with(None);
    assert!(
        !uninstall(&missing.settings, Path::new(PROGRAM))
            .expect("uninstall")
            .changed
    );
    assert!(
        !missing.settings.exists(),
        "no file is made just to remove nothing"
    );
}

#[test]
fn a_backup_holds_the_old_file_and_only_five_are_kept() {
    let e = env_with(Some(FOREIGN));
    let out = install(&e.settings, Path::new(PROGRAM)).expect("install");
    let backup = out.backup.expect("backup");
    assert_eq!(std::fs::read_to_string(backup).expect("read"), FOREIGN);
    for _ in 0..4 {
        uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
        install(&e.settings, Path::new(PROGRAM)).expect("install");
    }
    assert_eq!(
        backups(e.dir.path()).len(),
        5,
        "{:?}",
        backups(e.dir.path())
    );
}

#[test]
fn a_file_that_is_not_json_is_left_alone() {
    for (name, text) in [
        ("comment", "{\n  // my note\n  \"model\": \"opus\"\n}\n"),
        ("trailing comma", "{\"model\": \"opus\",}"),
        ("garbage", "this is not json"),
        ("cut off", "{\"hooks\": {"),
    ] {
        let e = env_with(Some(text));
        let err = install(&e.settings, Path::new(PROGRAM))
            .err()
            .unwrap_or_else(|| panic!("{name}"));
        assert!(matches!(err, Error::Invalid { .. }), "{name}: {err}");
        assert_eq!(
            std::fs::read_to_string(&e.settings).expect("read"),
            text,
            "{name}"
        );
        assert!(
            backups(e.dir.path()).is_empty() && leftovers(e.dir.path()).is_empty(),
            "{name}"
        );
        assert!(matches!(
            uninstall(&e.settings, Path::new(PROGRAM)),
            Err(Error::Invalid { .. })
        ));
        assert!(matches!(
            status(&e.settings, Path::new(PROGRAM)),
            Err(Error::Invalid { .. })
        ));
    }
}

#[test]
fn a_file_with_an_unexpected_shape_is_left_alone() {
    for (name, text) in [
        ("top level is a list", "[1, 2]"),
        ("hooks is a list", "{\"hooks\": []}"),
        ("event is an object", "{\"hooks\": {\"Stop\": {}}}"),
        ("group is a string", "{\"hooks\": {\"Stop\": [\"x\"]}}"),
        (
            "group hooks is an object",
            "{\"hooks\": {\"Stop\": [{\"hooks\": {}}]}}",
        ),
    ] {
        let e = env_with(Some(text));
        let err = install(&e.settings, Path::new(PROGRAM))
            .err()
            .unwrap_or_else(|| panic!("{name}"));
        assert!(matches!(err, Error::Shape { .. }), "{name}: {err}");
        assert_eq!(
            std::fs::read_to_string(&e.settings).expect("read"),
            text,
            "{name}"
        );
        assert!(backups(e.dir.path()).is_empty(), "{name}");
    }
}

#[test]
fn the_indentation_of_the_file_is_kept() {
    for indent in ["    ", "\t"] {
        let text = format!("{{\n{indent}\"model\": \"opus\"\n}}\n");
        let e = env_with(Some(&text));
        install(&e.settings, Path::new(PROGRAM)).expect("install");
        let out = std::fs::read_to_string(&e.settings).expect("read");
        assert!(
            out.contains(&format!("\n{indent}\"hooks\": {{")),
            "{indent:?}: {out}"
        );
        uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
        assert_eq!(std::fs::read_to_string(&e.settings).expect("read"), text);
    }
}

#[test]
fn an_empty_file_and_a_byte_order_mark_are_fine() {
    for text in ["", "  \n", "\u{feff}{\"model\": \"opus\"}"] {
        let e = env_with(Some(text));
        install(&e.settings, Path::new(PROGRAM)).expect("install");
        assert!(
            status(&e.settings, Path::new(PROGRAM))
                .expect("status")
                .fully_installed()
        );
    }
}

#[test]
fn an_outdated_entry_is_fixed_in_place() {
    let e = env_with(Some(FOREIGN));
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    // Someone changed our PermissionRequest timeout by hand.
    let mut v = json_of(&e.settings);
    v["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"] = json!(5);
    std::fs::write(
        &e.settings,
        serde_json::to_string_pretty(&v).expect("json") + "\n",
    )
    .expect("write");
    let s = status(&e.settings, Path::new(PROGRAM)).expect("status");
    let state = s
        .events
        .iter()
        .find(|(n, _)| *n == "PermissionRequest")
        .expect("event")
        .1;
    assert_eq!(state, EventState::Outdated);
    assert!(!s.fully_installed());
    let out = install(&e.settings, Path::new(PROGRAM)).expect("repair");
    assert!(out.changed);
    let v = json_of(&e.settings);
    assert_eq!(
        v["hooks"]["PermissionRequest"]
            .as_array()
            .expect("array")
            .len(),
        1,
        "no duplicate"
    );
    assert_eq!(
        v["hooks"]["PermissionRequest"][0]["hooks"][0]["timeout"],
        PERMISSION_TIMEOUT_SECS
    );
}

#[test]
fn our_entry_inside_a_group_with_foreign_hooks_is_split_off() {
    let text = format!(
        "{{\"hooks\": {{\"Stop\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"/x/other\"}}, \
         {{\"type\": \"command\", \"command\": \"{PROGRAM}\", \"async\": false}}]}}]}}}}"
    );
    let e = env_with(Some(&text));
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    let v = json_of(&e.settings);
    let stop = v["hooks"]["Stop"].as_array().expect("array");
    assert_eq!(
        stop[0]["hooks"].as_array().expect("hooks").len(),
        1,
        "the foreign hook stays alone"
    );
    assert_eq!(stop[0]["hooks"][0]["command"], "/x/other");
    assert_eq!(stop[1]["hooks"][0]["async"], true);
    uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
    let v = json_of(&e.settings);
    assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "/x/other");
    assert_eq!(v["hooks"]["Stop"].as_array().expect("array").len(), 1);
}

#[test]
fn duplicates_of_ours_collapse_to_one() {
    let one = format!("{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{PROGRAM}\"}}]}}");
    let e = env_with(Some(&format!(
        "{{\"hooks\": {{\"Stop\": [{one}, {one}]}}}}"
    )));
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    assert_eq!(
        json_of(&e.settings)["hooks"]["Stop"]
            .as_array()
            .expect("array")
            .len(),
        1
    );
}

#[test]
fn a_path_with_spaces_and_quotes_is_recognized_again() {
    let program = Path::new("/Applications/Amiscara Deck.app/Contents/Resources/it's/deck-hook");
    let e = env_with(None);
    install(&e.settings, program).expect("install");
    let command = json_of(&e.settings)["hooks"]["Stop"][0]["hooks"][0]["command"]
        .as_str()
        .expect("command")
        .to_owned();
    assert!(command.starts_with('\''), "quoted for the shell: {command}");
    assert!(
        status(&e.settings, program)
            .expect("status")
            .fully_installed()
    );
    assert!(!install(&e.settings, program).expect("again").changed);
    uninstall(&e.settings, program).expect("uninstall");
    assert_eq!(json_of(&e.settings), json!({}));
}

#[test]
fn entries_of_another_program_path_are_foreign() {
    let e = env_with(None);
    install(&e.settings, Path::new("/old/place/deck-hook")).expect("install old");
    let s = status(&e.settings, Path::new(PROGRAM)).expect("status");
    assert_eq!(s.foreign_entries, EVENTS.len());
    assert!(s.events.iter().all(|(_, st)| *st == EventState::Missing));
    uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
    assert_eq!(
        json_of(&e.settings)["hooks"]["Stop"][0]["hooks"][0]["command"],
        "/old/place/deck-hook",
        "what is not ours stays"
    );
}

#[test]
fn a_symlinked_settings_file_stays_a_symlink() {
    let dir = tempfile::tempdir().expect("tempdir");
    let real = dir.path().join("dotfiles/settings.json");
    std::fs::create_dir_all(real.parent().expect("parent")).expect("mkdir");
    std::fs::write(&real, FOREIGN).expect("write");
    let link = dir.path().join("settings.json");
    std::os::unix::fs::symlink(&real, &link).expect("symlink");
    install(&link, Path::new(PROGRAM)).expect("install");
    assert!(
        std::fs::symlink_metadata(&link)
            .expect("meta")
            .file_type()
            .is_symlink()
    );
    assert!(
        status(&real, Path::new(PROGRAM))
            .expect("status")
            .fully_installed()
    );
}

#[test]
fn the_rights_of_the_file_are_kept() {
    let e = env_with(Some(FOREIGN));
    std::fs::set_permissions(&e.settings, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    let out = install(&e.settings, Path::new(PROGRAM)).expect("install");
    let mode = |p: &Path| std::fs::metadata(p).expect("meta").permissions().mode() & 0o777;
    assert_eq!(mode(&e.settings), 0o600);
    assert_eq!(
        mode(&out.backup.expect("backup")),
        0o600,
        "the copy is as private as the file"
    );
}

#[test]
fn status_tells_about_the_program_and_the_foreign_entries() {
    let e = env_with(Some(FOREIGN));
    let s = status(&e.settings, Path::new(PROGRAM)).expect("status");
    assert!(s.file_exists && !s.program_exists && !s.program_executable);
    assert_eq!(s.foreign_entries, 2);
    assert!(s.events.iter().all(|(_, st)| *st == EventState::Missing));

    let prog = e.dir.path().join("deck-hook");
    std::fs::write(&prog, "#!/bin/sh\n").expect("write");
    let s = status(&e.settings, &prog).expect("status");
    assert!(s.program_exists && !s.program_executable);
    std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    assert!(
        status(&e.settings, &prog)
            .expect("status")
            .program_executable
    );

    let none = env_with(None);
    let s = status(&none.settings, &prog).expect("status");
    assert!(!s.file_exists);
    assert!(!none.settings.exists(), "status never creates the file");
}

#[test]
fn other_files_next_to_the_settings_are_not_touched() {
    let e = env_with(Some(FOREIGN));
    std::fs::write(e.dir.path().join("settings.local.json"), "{\"x\": 1}").expect("write");
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    assert_eq!(
        std::fs::read_to_string(e.dir.path().join("settings.local.json")).expect("read"),
        "{\"x\": 1}"
    );
}

#[test]
fn the_backup_is_never_readable_by_others_not_even_for_a_moment() {
    // A file with rights 0600: the copy is created with 0600 and not narrowed
    // later, so no copy of it can be wider.
    let e = env_with(Some(FOREIGN));
    std::fs::set_permissions(&e.settings, std::fs::Permissions::from_mode(0o600)).expect("chmod");
    for _ in 0..5 {
        install(&e.settings, Path::new(PROGRAM)).expect("install");
        uninstall(&e.settings, Path::new(PROGRAM)).expect("uninstall");
    }
    for b in backups(e.dir.path()) {
        assert_eq!(
            std::fs::metadata(&b).expect("meta").permissions().mode() & 0o777,
            0o600,
            "{b:?}"
        );
    }
    assert!(leftovers(e.dir.path()).is_empty());
}

#[test]
fn a_planted_link_at_the_temporary_name_is_not_followed() {
    let e = env_with(Some(FOREIGN));
    let outside = tempfile::tempdir().expect("tempdir");
    let victim = outside.path().join("victim");
    std::fs::write(&victim, "keep").expect("write");
    let tmp = e
        .dir
        .path()
        .join(format!(".settings.json.deck-tmp-{}", std::process::id()));
    std::os::unix::fs::symlink(&victim, &tmp).expect("symlink");
    install(&e.settings, Path::new(PROGRAM)).expect("install");
    assert_eq!(
        std::fs::read_to_string(&victim).expect("read"),
        "keep",
        "nothing outside is written"
    );
    assert!(
        status(&e.settings, Path::new(PROGRAM))
            .expect("status")
            .fully_installed()
    );
}
