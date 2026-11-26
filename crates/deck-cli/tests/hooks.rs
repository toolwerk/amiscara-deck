use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

struct Env {
    dir: tempfile::TempDir,
    settings: PathBuf,
    program: PathBuf,
}

fn env() -> Env {
    let dir = tempfile::tempdir().expect("tempdir");
    let program = dir.path().join("deck-hook");
    std::fs::write(&program, "#!/bin/sh\n").expect("write");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    Env {
        settings: dir.path().join("settings.json"),
        program,
        dir,
    }
}

fn run(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_amiscara-deck"))
        .args(args)
        .env_clear()
        // The texts of these tests are German.
        .env("DECK_LANGUAGE", "de")
        .output()
        .expect("run");
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn hooks(e: &Env, sub: &str, extra: &[&str]) -> (Option<i32>, String, String) {
    let mut args = vec![
        "hooks",
        sub,
        "--settings",
        e.settings.to_str().expect("utf8"),
        "--program",
        e.program.to_str().expect("utf8"),
    ];
    args.extend_from_slice(extra);
    run(&args)
}

#[test]
fn install_status_uninstall_walk_through() {
    let e = env();
    let (code, out, _) = hooks(&e, "status", &[]);
    assert_eq!(code, Some(3), "nothing installed yet");
    assert!(
        out.contains("(fehlt)") && out.contains("PermissionRequest"),
        "{out}"
    );

    let (code, out, err) = hooks(&e, "install", &[]);
    assert_eq!((code, err.as_str()), (Some(0), ""), "{out}");
    assert!(out.contains("Hooks eingebaut"), "{out}");

    let (code, out, _) = hooks(&e, "status", &[]);
    assert_eq!(code, Some(0), "{out}");
    assert!(
        out.contains("vorhanden") && !out.contains("fehlt "),
        "{out}"
    );

    let (code, out, _) = hooks(&e, "install", &[]);
    assert_eq!(code, Some(0));
    assert!(out.contains("Schon eingebaut"), "{out}");

    let (code, out, _) = hooks(&e, "uninstall", &[]);
    assert_eq!(code, Some(0));
    assert!(out.contains("ausgebaut"), "{out}");
    assert_eq!(hooks(&e, "status", &[]).0, Some(3));
    let (code, out, _) = hooks(&e, "uninstall", &[]);
    assert_eq!(code, Some(0));
    assert!(out.contains("Nichts von Deck"), "{out}");
}

#[test]
fn a_missing_or_unrunnable_program_is_not_installed_without_force() {
    let e = env();
    std::fs::set_permissions(&e.program, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let (code, _, err) = hooks(&e, "install", &[]);
    assert_eq!(code, Some(1));
    assert!(
        err.contains("nicht ausführbar") && err.contains("--force"),
        "{err}"
    );
    assert!(!e.settings.exists(), "nothing written");

    let (code, _, _) = hooks(&e, "install", &["--force"]);
    assert_eq!(code, Some(0));
    assert!(e.settings.exists());

    let gone = Env {
        program: e.dir.path().join("nowhere"),
        ..env()
    };
    assert_eq!(hooks(&gone, "install", &[]).0, Some(1));
}

#[test]
fn a_broken_settings_file_is_reported_and_left_alone() {
    let e = env();
    std::fs::write(&e.settings, "{ // comment\n}").expect("write");
    for sub in ["install", "uninstall", "status"] {
        let (code, _, err) = hooks(&e, sub, &[]);
        assert_eq!(code, Some(1), "{sub}");
        assert!(
            err.contains("not valid JSON") && err.contains("untouched"),
            "{sub}: {err}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(&e.settings).expect("read"),
        "{ // comment\n}"
    );
}

#[test]
fn wrong_usage_gives_code_2_and_the_usage_text() {
    for args in [
        &[][..],
        &["hooks"],
        &["hooks", "frobnicate", "--settings", "x", "--program", "y"],
        &["nonsense"],
        &["hooks", "status", "--bogus"],
    ] {
        let (code, out, err) = run(args);
        assert_eq!(code, Some(2), "{args:?}");
        assert!(out.is_empty(), "{args:?}");
        assert!(
            err.contains("Aufruf") || err.contains("Unbekannt"),
            "{args:?}: {err}"
        );
    }
    let (code, out, _) = run(&["--help"]);
    assert_eq!(code, Some(0));
    assert!(out.contains("hooks install"));
}

#[test]
fn without_a_home_folder_the_settings_must_be_given() {
    let (code, _, err) = run(&["hooks", "status", "--program", "/x/deck-hook"]);
    assert_eq!(code, Some(2));
    assert!(err.contains("--settings"), "{err}");
}

#[test]
fn the_program_defaults_to_deck_hook_next_to_the_cli() {
    // Not built next to the test binary, so it is reported as missing, with its path.
    let dir = tempfile::tempdir().expect("tempdir");
    let settings = dir.path().join("s.json");
    let (code, out, _) = run(&[
        "hooks",
        "status",
        "--settings",
        settings.to_str().expect("utf8"),
    ]);
    assert_eq!(code, Some(3));
    let line = out
        .lines()
        .find(|l| l.starts_with("Hook-Programm:"))
        .expect("program line");
    assert!(line.contains("deck-hook"), "{line}");
    let _ = Path::new("x");
}

// daemon install

fn daemon_args<'a>(e: &'a Env, sub: &'a str, plist: &'a str, extra: &[&'a str]) -> Vec<&'a str> {
    let mut a = vec![
        "daemon",
        sub,
        "--program",
        e.program.to_str().expect("utf8"),
        "--data-dir",
        "/x/data",
        "--plist",
        plist,
        "--no-launchctl",
    ];
    a.extend_from_slice(extra);
    a
}

#[test]
fn daemon_install_writes_the_service_file_and_uninstall_removes_it() {
    let e = env();
    let plist = e
        .dir
        .path()
        .join("LaunchAgents/net.staude.amiscara.deckd.plist");
    let plist_s = plist.to_str().expect("utf8");
    let (code, out, _) = run(&daemon_args(&e, "status", plist_s, &[]));
    assert_eq!(code, Some(3), "{out}");
    assert!(out.contains("nicht eingerichtet"));

    let (code, out, err) = run(&daemon_args(&e, "install", plist_s, &[]));
    assert_eq!((code, err.as_str()), (Some(0), ""), "{out}");
    let text = std::fs::read_to_string(&plist).expect("plist");
    let marker = if cfg!(target_os = "macos") {
        "net.staude.amiscara.deckd"
    } else {
        "WantedBy=default.target"
    };
    assert!(
        text.contains(marker) && text.contains("--data-dir") && text.contains("/x/data"),
        "{text}"
    );
    assert!(text.contains(e.program.to_str().expect("utf8")));

    let (code, out, _) = run(&daemon_args(&e, "install", plist_s, &[]));
    assert_eq!(code, Some(0));
    assert!(out.contains("Schon eingerichtet"), "{out}");
    assert_eq!(run(&daemon_args(&e, "status", plist_s, &[])).0, Some(0));

    let (code, out, _) = run(&daemon_args(&e, "uninstall", plist_s, &[]));
    assert_eq!(code, Some(0));
    assert!(out.contains("Entfernt") && !plist.exists(), "{out}");
    let (code, out, _) = run(&daemon_args(&e, "uninstall", plist_s, &[]));
    assert_eq!(code, Some(0));
    assert!(out.contains("Nichts eingerichtet"), "{out}");
}

#[test]
fn daemon_install_refuses_a_missing_service_and_unknown_options() {
    let e = env();
    std::fs::set_permissions(&e.program, std::fs::Permissions::from_mode(0o644)).expect("chmod");
    let plist = e.dir.path().join("a.plist");
    let (code, _, err) = run(&daemon_args(
        &e,
        "install",
        plist.to_str().expect("utf8"),
        &[],
    ));
    assert_eq!(code, Some(1));
    assert!(err.contains("nicht ausführbar") && !plist.exists(), "{err}");
    assert_eq!(run(&["daemon", "install", "--bogus"]).0, Some(2));
    assert_eq!(
        run(&[
            "daemon",
            "frobnicate",
            "--program",
            "/x",
            "--data-dir",
            "/d",
            "--plist",
            "/p"
        ])
        .0,
        Some(2)
    );
}

#[test]
fn a_changed_launch_agent_is_reported_and_install_repairs_it() {
    let e = env();
    let plist = e.dir.path().join("a.plist");
    let plist_s = plist.to_str().expect("utf8");
    run(&daemon_args(&e, "install", plist_s, &[]));
    std::fs::write(&plist, "something else").expect("write");
    let (code, out, _) = run(&daemon_args(&e, "status", plist_s, &[]));
    assert_eq!(code, Some(3));
    assert!(out.contains("anders"), "{out}");
    assert_eq!(run(&daemon_args(&e, "install", plist_s, &[])).0, Some(0));
    assert_eq!(run(&daemon_args(&e, "status", plist_s, &[])).0, Some(0));
}

/// The help in the language of the environment, as `amiscara-deck` prints it.
fn help_with(vars: &[(&str, &str)]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_amiscara-deck"))
        .arg("help")
        .env_clear()
        .envs(vars.iter().copied())
        .output()
        .expect("run");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_language_follows_the_environment() {
    let english = help_with(&[("LANG", "en_US.UTF-8")]);
    assert!(
        english.contains("Usage:") && english.contains("Exit codes"),
        "{english}"
    );
    let german = help_with(&[("LANG", "de_DE.UTF-8")]);
    assert!(
        german.contains("Aufruf:") && german.contains("Beendigungscodes"),
        "{german}"
    );
    assert!(
        help_with(&[("LC_ALL", "C"), ("LANG", "de_DE.UTF-8")]).contains("Aufruf:"),
        "C says nothing about the language"
    );
    assert!(
        help_with(&[("DECK_LANGUAGE", "en"), ("LANG", "de_DE.UTF-8")]).contains("Usage:"),
        "DECK_LANGUAGE comes first"
    );
    assert!(
        help_with(&[("LANG", "fr_FR.UTF-8")]).contains("Usage:"),
        "neither German nor English: English"
    );
    assert!(help_with(&[]).contains("Usage:"));
}

#[test]
fn hook_status_in_english() {
    let e = env();
    let out = Command::new(env!("CARGO_BIN_EXE_amiscara-deck"))
        .args([
            "hooks",
            "status",
            "--settings",
            e.settings.to_str().expect("utf8"),
            "--program",
            e.program.to_str().expect("utf8"),
        ])
        .env_clear()
        .env("LANG", "en_GB.UTF-8")
        .output()
        .expect("run");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("Settings: ")
            && text.contains("(missing)")
            && text.contains("Hook program: "),
        "{text}"
    );
    assert!(
        !text.contains("fehlt") && !text.contains("Einstellungen"),
        "{text}"
    );
}
