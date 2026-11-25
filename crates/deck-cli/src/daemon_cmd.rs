//! `daemon install|uninstall|status`: start the daemon at login (macOS).

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use deck_daemon::launchd;
use deck_i18n::{t, t_with};

struct Opts {
    program: PathBuf,
    data_dir: PathBuf,
    plist: PathBuf,
    /// Write or remove the file only, do not call `launchctl`.
    no_launchctl: bool,
}

fn parse(opts: &[&str], err: &mut dyn Write) -> Option<Opts> {
    let mut program = None;
    let mut data_dir = None;
    let mut plist = None;
    let mut no_launchctl = false;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match *o {
            "--program" => program = it.next().map(PathBuf::from),
            "--data-dir" => data_dir = it.next().map(PathBuf::from),
            "--plist" => plist = it.next().map(PathBuf::from),
            "--no-launchctl" => no_launchctl = true,
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
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let program = program.or_else(|| {
        let exe = std::fs::canonicalize(std::env::current_exe().ok()?).ok()?;
        Some(exe.parent()?.join("amiscara-deckd"))
    });
    let (Some(program), Some(data_dir), Some(plist)) = (
        program,
        data_dir.or_else(deck_proto::paths::default_data_dir),
        plist.or_else(|| home.as_deref().map(launchd::plist_path)),
    ) else {
        let _ = writeln!(
            err,
            "{}",
            t("Paths not found: please give --program, --data-dir and --plist.")
        );
        return None;
    };
    Some(Opts {
        program,
        data_dir,
        plist,
        no_launchctl,
    })
}

fn uid() -> Option<String> {
    let out = Command::new("id").arg("-u").output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

fn launchctl(args: &[&str]) -> bool {
    Command::new("launchctl")
        .args(args)
        .status()
        .is_ok_and(|s| s.success())
}

pub fn run(sub: &str, opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
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
    let ready = std::fs::metadata(&o.program).ok().is_some_and(|m| {
        m.is_file() && std::os::unix::fs::PermissionsExt::mode(&m.permissions()) & 0o111 != 0
    });
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
    let text = launchd::plist(&o.program, &o.data_dir);
    if std::fs::read_to_string(&o.plist).is_ok_and(|t| t == text) {
        let _ = writeln!(
            out,
            "{}",
            t_with(
                "Already set up: {path}",
                &[("path", &o.plist.display().to_string())]
            )
        );
    } else {
        if let Some(dir) = o.plist.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            let _ = writeln!(err, "{}", crate::view::error(&e));
            return 1;
        }
        if let Err(e) = std::fs::write(&o.plist, text) {
            let _ = writeln!(err, "{}", crate::view::error(&e));
            return 1;
        }
        let _ = writeln!(
            out,
            "{}",
            t_with(
                "Written: {path}",
                &[("path", &o.plist.display().to_string())]
            )
        );
    }
    if o.no_launchctl {
        return 0;
    }
    let Some(uid) = uid() else {
        let _ = writeln!(err, "{}", t("User id not found, launchctl not called."));
        return 1;
    };
    let domain = format!("gui/{uid}");
    // A loaded job must go before a changed one comes in; an error here only means it was not loaded.
    let _ = launchctl(&["bootout", &format!("{domain}/{}", launchd::LABEL)]);
    if launchctl(&["bootstrap", &domain, &o.plist.to_string_lossy()]) {
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

fn uninstall(o: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if !o.no_launchctl
        && let Some(uid) = uid()
    {
        let _ = launchctl(&["bootout", &format!("gui/{uid}/{}", launchd::LABEL)]);
    }
    match std::fs::remove_file(&o.plist) {
        Ok(()) => {
            let _ = writeln!(
                out,
                "{}",
                t_with(
                    "Removed: {path}",
                    &[("path", &o.plist.display().to_string())]
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
    let want = launchd::plist(&o.program, &o.data_dir);
    let (ok, state) = match std::fs::read_to_string(&o.plist) {
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
            &[("state", &state), ("path", &o.plist.display().to_string())]
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
