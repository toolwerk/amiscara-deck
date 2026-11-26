//! `daemon install|uninstall|status` on Windows: a value in the Run key of the
//! user (`deck_daemon::winrun`), set and read with `reg.exe`.
//!
//! Plain `Command` calls only, so this compiles everywhere; `daemon_cmd`
//! calls it on Windows.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use deck_daemon::winrun;
use deck_i18n::{t, t_with};
use deck_proto::message::UiMessage;

fn reg(args: &[&str]) -> Option<String> {
    let out = Command::new("reg").args(args).output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn registered() -> Option<String> {
    winrun::parse_query(&reg(&["query", winrun::KEY, "/v", winrun::VALUE])?)
}

fn runtime() -> Option<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
}

fn socket(data_dir: &Path) -> std::path::PathBuf {
    data_dir.join(deck_proto::paths::SOCKET_NAME)
}

fn running(data_dir: &Path) -> bool {
    let socket = socket(data_dir);
    runtime().is_some_and(|rt| rt.block_on(deck_ipc::connect(&socket)).is_ok())
}

/// Ask a running daemon to end, as the app does when it replaces one.
fn stop(data_dir: &Path) {
    let socket = socket(data_dir);
    let Some(rt) = runtime() else { return };
    rt.block_on(async {
        if let Ok(mut c) = deck_client::Client::connect(&socket, "amiscara-deck daemon").await {
            let _ = c.send(&UiMessage::Shutdown).await;
        }
    });
}

pub fn install(program: &Path, data_dir: &Path, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if !program.is_file() {
        let _ = writeln!(
            err,
            "{}",
            t_with(
                "The daemon is missing or not executable: {path}\nNothing set up.",
                &[("path", &program.display().to_string())]
            )
        );
        return 1;
    }
    let place = format!(r"{}\{}", winrun::KEY, winrun::VALUE);
    let command = winrun::command(program, data_dir);
    if registered().as_deref() == Some(command.as_str()) {
        let _ = writeln!(
            out,
            "{}",
            t_with("Already set up: {path}", &[("path", &place)])
        );
    } else if reg(&[
        "add",
        winrun::KEY,
        "/v",
        winrun::VALUE,
        "/t",
        "REG_SZ",
        "/d",
        &command,
        "/f",
    ])
    .is_some()
    {
        let _ = writeln!(out, "{}", t_with("Written: {path}", &[("path", &place)]));
    } else {
        let _ = writeln!(
            err,
            "{}",
            t("reg add failed. Nothing set up, start amiscara-deckd yourself.")
        );
        return 1;
    }
    if !running(data_dir) {
        let started = Command::new(program)
            .arg("--data-dir")
            .arg(data_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
            && (0..30).any(|_| {
                std::thread::sleep(Duration::from_millis(100));
                running(data_dir)
            });
        if !started {
            let _ = writeln!(
                err,
                "{}",
                t("The daemon did not start. It starts at the next login.")
            );
            return 1;
        }
    }
    let _ = writeln!(
        out,
        "{}",
        t("The daemon now starts at login and is already running.")
    );
    0
}

pub fn uninstall(data_dir: &Path, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    stop(data_dir);
    let place = format!(r"{}\{}", winrun::KEY, winrun::VALUE);
    if registered().is_none() {
        let _ = writeln!(out, "{}", t("Nothing set up, nothing removed."));
        return 0;
    }
    if reg(&["delete", winrun::KEY, "/v", winrun::VALUE, "/f"]).is_some() {
        let _ = writeln!(out, "{}", t_with("Removed: {path}", &[("path", &place)]));
        0
    } else {
        let _ = writeln!(err, "{}", t("reg delete failed."));
        1
    }
}

pub fn status(program: &Path, data_dir: &Path, out: &mut dyn Write) -> u8 {
    let place = format!(r"{}\{}", winrun::KEY, winrun::VALUE);
    let want = winrun::command(program, data_dir);
    let (ok, state) = match registered() {
        Some(have) if have == want => (true, t("set up")),
        Some(_) => (
            false,
            t("set up, but different (install brings it in line)"),
        ),
        None => (false, t("not set up")),
    };
    let _ = writeln!(
        out,
        "{}",
        t_with(
            "Start at login: {state} ({path})",
            &[("state", &state), ("path", &place)]
        )
    );
    let missing = if program.exists() {
        String::new()
    } else {
        format!(" {}", t("(missing)"))
    };
    let _ = writeln!(
        out,
        "{}",
        t_with(
            "Daemon: {path}{missing}",
            &[
                ("path", &program.display().to_string()),
                ("missing", &missing)
            ]
        )
    );
    if ok { 0 } else { 3 }
}
