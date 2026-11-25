use std::io::Write;
use std::path::PathBuf;

use deck_hooks::{EventState, Status};
use deck_i18n::{t, t_with};

struct Opts {
    settings: PathBuf,
    program: PathBuf,
    force: bool,
}

fn parse(opts: &[&str], err: &mut dyn Write) -> Option<Opts> {
    let mut settings = None;
    let mut program = None;
    let mut force = false;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match *o {
            "--settings" => settings = it.next().map(PathBuf::from),
            "--program" => program = it.next().map(PathBuf::from),
            "--force" => force = true,
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
    let Some(settings) = settings.or_else(deck_hooks::default_claude_settings) else {
        let _ = writeln!(
            err,
            "{}",
            t("No home folder found: please give --settings.")
        );
        return None;
    };
    let Some(program) = program.or_else(sibling_program) else {
        let _ = writeln!(
            err,
            "{}",
            t("Hook program not found: please give --program.")
        );
        return None;
    };
    Some(Opts {
        settings,
        program,
        force,
    })
}

/// `deck-hook` next to this program: where the app puts both.
fn sibling_program() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = std::fs::canonicalize(exe).ok()?;
    Some(exe.parent()?.join("deck-hook"))
}

pub fn run(sub: &str, opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let Some(o) = parse(opts, err) else {
        return 2;
    };
    match sub {
        "install" => install(&o, out, err),
        "uninstall" => uninstall(&o, out, err),
        "status" => status(&o, out, err),
        other => {
            let _ = writeln!(
                err,
                "{}",
                t_with(
                    "Unknown command: {command}",
                    &[("command", &format!("hooks {other}"))]
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
    if !ready && !o.force {
        let _ = writeln!(
            err,
            "{}",
            t_with(
                "The hook program is missing or not executable: {path}\nNothing installed. Install anyway with --force.",
                &[("path", &o.program.display().to_string())]
            )
        );
        return 1;
    }
    match deck_hooks::install(&o.settings, &o.program) {
        Ok(r) if r.changed => {
            let _ = writeln!(
                out,
                "{}",
                t_with(
                    "Hooks installed in {path}",
                    &[("path", &o.settings.display().to_string())]
                )
            );
            if let Some(b) = r.backup {
                let _ = writeln!(
                    out,
                    "{}",
                    t_with("Backup: {path}", &[("path", &b.display().to_string())])
                );
            }
            let _ = writeln!(
                out,
                "{}",
                t(
                    "Running sessions with project settings pick them up at once; with the user settings, restart a session when in doubt."
                )
            );
            0
        }
        Ok(_) => {
            let _ = writeln!(out, "{}", t("Already installed, nothing changed."));
            0
        }
        Err(e) => fail(err, &e),
    }
}

fn uninstall(o: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match deck_hooks::uninstall(&o.settings, &o.program) {
        Ok(r) if r.changed => {
            let _ = writeln!(
                out,
                "{}",
                t_with(
                    "Hooks removed from {path}",
                    &[("path", &o.settings.display().to_string())]
                )
            );
            if let Some(b) = r.backup {
                let _ = writeln!(
                    out,
                    "{}",
                    t_with("Backup: {path}", &[("path", &b.display().to_string())])
                );
            }
            0
        }
        Ok(_) => {
            let _ = writeln!(out, "{}", t("Nothing of Deck found, nothing changed."));
            0
        }
        Err(e) => fail(err, &e),
    }
}

fn status(o: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let s = match deck_hooks::status(&o.settings, &o.program) {
        Ok(s) => s,
        Err(e) => return fail(err, &e),
    };
    let _ = write_status(&s, o, out);
    if s.fully_installed() { 0 } else { 3 }
}

fn write_status(s: &Status, o: &Opts, out: &mut dyn Write) -> std::io::Result<()> {
    let missing = if s.file_exists {
        String::new()
    } else {
        format!(" {}", t("(missing)"))
    };
    writeln!(
        out,
        "{}",
        t_with(
            "Settings: {path}{missing}",
            &[
                ("path", &o.settings.display().to_string()),
                ("missing", &missing)
            ]
        )
    )?;
    let program = match (s.program_exists, s.program_executable) {
        (true, true) => t("present"),
        (true, false) => t("present, but not executable"),
        _ => t("missing"),
    };
    writeln!(
        out,
        "{}",
        t_with(
            "Hook program: {path} ({state})",
            &[
                ("path", &o.program.display().to_string()),
                ("state", &program)
            ]
        )
    )?;
    for (event, state) in &s.events {
        let text = match state {
            EventState::Installed => t("installed"),
            EventState::Missing => t("missing"),
            EventState::Outdated => t("outdated (install fixes it)"),
        };
        writeln!(out, "  {event:<20} {text}")?;
    }
    writeln!(
        out,
        "{}",
        t_with(
            "Hooks of other programs (left alone): {count}",
            &[("count", &s.foreign_entries.to_string())]
        )
    )
}

fn fail(err: &mut dyn Write, e: &deck_hooks::Error) -> u8 {
    let _ = writeln!(err, "{}", crate::view::error(e));
    1
}
