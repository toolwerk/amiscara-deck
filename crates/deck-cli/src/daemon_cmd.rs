//! `daemon install|uninstall|status`: start the daemon at login. A
//! LaunchAgent on macOS, a systemd user unit on Linux.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use deck_daemon::{launchd, systemd};
use deck_i18n::{t, t_with};

struct Opts {
    program: PathBuf,
    data_dir: PathBuf,
    /// The LaunchAgent plist (macOS) or the systemd unit (Linux).
    file: PathBuf,
    /// Write or remove the file only, do not call `launchctl` or `systemctl`.
    no_manager: bool,
}

/// What this system starts the daemon with.
fn service_text(o: &Opts) -> String {
    if cfg!(target_os = "macos") {
        launchd::plist(&o.program, &o.data_dir)
    } else {
        systemd::unit(&o.program, &o.data_dir)
    }
}

fn default_file(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        launchd::plist_path(home)
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from);
        systemd::unit_path(home, config.as_deref())
    }
}

fn parse(opts: &[&str], err: &mut dyn Write) -> Option<Opts> {
    let mut program = None;
    let mut data_dir = None;
    let mut file = None;
    let mut no_manager = false;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match *o {
            "--program" => program = it.next().map(PathBuf::from),
            "--data-dir" => data_dir = it.next().map(PathBuf::from),
            "--plist" | "--unit" => file = it.next().map(PathBuf::from),
            "--no-launchctl" | "--no-systemctl" => no_manager = true,
            other => {
                let _ = writeln!(
                    err,
                    "{}",
                    t_with("Unknown option: {option}", &[("option", other)])
                );
                return None;
            }
        }
    }
    let home = deck_proto::paths::home_dir();
    let program = program.or_else(|| {
        let exe = deck_proto::paths::canonical(&std::env::current_exe().ok()?).ok()?;
        Some(
            exe.parent()?
                .join(format!("amiscara-deckd{}", std::env::consts::EXE_SUFFIX)),
        )
    });
    let (Some(program), Some(data_dir), Some(file)) = (
        program,
        data_dir.or_else(deck_proto::paths::default_data_dir),
        file.or_else(|| home.as_deref().map(default_file)),
    ) else {
        let _ = writeln!(
            err,
            "{}",
            t("Paths not found: please give --program, --data-dir and --plist (or --unit).")
        );
        return None;
    };
    Some(Opts {
        program,
        data_dir,
        file,
        no_manager,
    })
}

fn uid() -> Option<String> {
    let out = Command::new("id").arg("-u").output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn systemctl(args: &[&str]) -> bool {
    Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

fn launchctl(args: &[&str]) -> bool {
    Command::new("launchctl")
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

pub fn run(sub: &str, opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if cfg!(windows) {
        let _ = writeln!(
            err,
            "{}",
            t("Start at login is not available on Windows yet. Start amiscara-deckd yourself.")
        );
        return 1;
    }
    let Some(o) = parse(opts, err) else { return 2 };
    match sub {
        "install" => install(&o, out, err),
        "uninstall" => uninstall(&o, out, err),
        "status" => status(&o, out),
        other => {
            let _ = writeln!(
                err,
                "{}",
                t_with(
                    "Unknown command: {command}",
                    &[("command", &format!("daemon {other}"))]
                )
            );
            2
        }
    }
}

fn install(o: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let ready = std::fs::metadata(&o.program)
        .ok()
        .is_some_and(|m| deck_proto::fsperm::is_executable(&m));
    if !ready {
        let _ = writeln!(
            err,
            "{}",
            t_with(
                "The daemon is missing or not executable: {path}\nNothing set up.",
                &[("path", &o.program.display().to_string())]
            )
        );
        return 1;
    }
    let text = service_text(o);
    let unchanged = std::fs::read_to_string(&o.file).is_ok_and(|t| t == text);
    if unchanged {
        let _ = writeln!(
            out,
            "{}",
            t_with(
                "Already set up: {path}",
                &[("path", &o.file.display().to_string())]
            )
        );
    } else {
        if let Some(dir) = o.file.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            let _ = writeln!(err, "{}", crate::view::error(&e));
            return 1;
        }
        if let Err(e) = std::fs::write(&o.file, text) {
            let _ = writeln!(err, "{}", crate::view::error(&e));
            return 1;
        }
        let _ = writeln!(
            out,
            "{}",
            t_with(
                "Written: {path}",
                &[("path", &o.file.display().to_string())]
            )
        );
    }
    if o.no_manager {
        return 0;
    }
    if !cfg!(target_os = "macos") {
        return start_systemd(unchanged, out, err);
    }
    let Some(uid) = uid() else {
        let _ = writeln!(err, "{}", t("User id not found, launchctl not called."));
        return 1;
    };
    let domain = format!("gui/{uid}");
    // A loaded job must go before a changed one comes in; an error here only means it was not loaded.
    let _ = launchctl(&["bootout", &format!("{domain}/{}", launchd::LABEL)]);
    if launchctl(&["bootstrap", &domain, &o.file.to_string_lossy()]) {
        let _ = writeln!(
            out,
            "{}",
            t("The daemon now starts at login and is already running.")
        );
        0
    } else {
        let _ = writeln!(
            err,
            "{}",
            t(
                "launchctl bootstrap failed. The file is in place, the daemon starts at the next login."
            )
        );
        1
    }
}

/// Enable the unit and start it; a changed unit is restarted so the new
/// file counts.
fn start_systemd(unchanged: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let started = systemctl(&["daemon-reload"])
        && systemctl(&["enable", systemd::UNIT])
        && systemctl(&[if unchanged { "start" } else { "restart" }, systemd::UNIT]);
    if started {
        let _ = writeln!(
            out,
            "{}",
            t("The daemon now starts at login and is already running.")
        );
        0
    } else {
        let _ = writeln!(
            err,
            "{}",
            t(
                "systemctl --user failed. The file is in place; start the daemon with: systemctl --user enable --now amiscara-deckd"
            )
        );
        1
    }
}

fn uninstall(o: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if !o.no_manager {
        if cfg!(target_os = "macos") {
            if let Some(uid) = uid() {
                let _ = launchctl(&["bootout", &format!("gui/{uid}/{}", launchd::LABEL)]);
            }
        } else {
            let _ = systemctl(&["disable", "--now", systemd::UNIT]);
        }
    }
    let removed = std::fs::remove_file(&o.file);
    if !o.no_manager && !cfg!(target_os = "macos") && removed.is_ok() {
        let _ = systemctl(&["daemon-reload"]);
    }
    match removed {
        Ok(()) => {
            let _ = writeln!(
                out,
                "{}",
                t_with(
                    "Removed: {path}",
                    &[("path", &o.file.display().to_string())]
                )
            );
            0
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let _ = writeln!(out, "{}", t("Nothing set up, nothing removed."));
            0
        }
        Err(e) => {
            let _ = writeln!(err, "{}", crate::view::error(&e));
            1
        }
    }
}

fn status(o: &Opts, out: &mut dyn Write) -> u8 {
    let want = service_text(o);
    let (ok, state) = match std::fs::read_to_string(&o.file) {
        Ok(text) if text == want => (true, t("set up")),
        Ok(_) => (
            false,
            t("set up, but different (install brings it in line)"),
        ),
        Err(_) => (false, t("not set up")),
    };
    let _ = writeln!(
        out,
        "{}",
        t_with(
            "Start at login: {state} ({path})",
            &[("state", &state), ("path", &o.file.display().to_string())]
        )
    );
    let missing = if Path::new(&o.program).exists() {
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
                ("path", &o.program.display().to_string()),
                ("missing", &missing)
            ]
        )
    );
    if ok { 0 } else { 3 }
}
