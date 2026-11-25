use std::io::Write;
use std::path::PathBuf;

use deck_hooks::{EventState, Status};

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
                let _ = writeln!(err, "Unbekannte Option: {other}");
                return None;
            }
        }
    }
    let Some(settings) = settings.or_else(deck_hooks::default_claude_settings) else {
        let _ = writeln!(err, "Kein Home-Ordner gefunden: bitte --settings angeben.");
        return None;
    };
    let Some(program) = program.or_else(sibling_program) else {
        let _ = writeln!(
            err,
            "Hook-Programm nicht gefunden: bitte --program angeben."
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
            let _ = writeln!(err, "Unbekannter Befehl: hooks {other}");
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
            "Das Hook-Programm fehlt oder ist nicht ausführbar: {}\n\
             Nichts eingebaut. Mit --force trotzdem einbauen.",
            o.program.display()
        );
        return 1;
    }
    match deck_hooks::install(&o.settings, &o.program) {
        Ok(r) if r.changed => {
            let _ = writeln!(out, "Hooks eingebaut in {}", o.settings.display());
            if let Some(b) = r.backup {
                let _ = writeln!(out, "Sicherung: {}", b.display());
            }
            let _ = writeln!(
                out,
                "Laufende Sitzungen mit Projekt-Einstellungen greifen sie sofort, \
                 bei den Benutzer-Einstellungen im Zweifel eine Sitzung neu starten."
            );
            0
        }
        Ok(_) => {
            let _ = writeln!(out, "Schon eingebaut, nichts geändert.");
            0
        }
        Err(e) => fail(err, &e),
    }
}

fn uninstall(o: &Opts, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match deck_hooks::uninstall(&o.settings, &o.program) {
        Ok(r) if r.changed => {
            let _ = writeln!(out, "Hooks ausgebaut aus {}", o.settings.display());
            if let Some(b) = r.backup {
                let _ = writeln!(out, "Sicherung: {}", b.display());
            }
            0
        }
        Ok(_) => {
            let _ = writeln!(out, "Nichts von Deck gefunden, nichts geändert.");
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
    writeln!(
        out,
        "Einstellungen: {}{}",
        o.settings.display(),
        if s.file_exists { "" } else { " (fehlt)" }
    )?;
    let program = match (s.program_exists, s.program_executable) {
        (true, true) => "vorhanden",
        (true, false) => "vorhanden, aber nicht ausführbar",
        _ => "fehlt",
    };
    writeln!(out, "Hook-Programm: {} ({program})", o.program.display())?;
    for (event, state) in &s.events {
        let text = match state {
            EventState::Installed => "eingebaut",
            EventState::Missing => "fehlt",
            EventState::Outdated => "veraltet (install behebt es)",
        };
        writeln!(out, "  {event:<20} {text}")?;
    }
    writeln!(
        out,
        "Hooks anderer Programme (unberührt): {}",
        s.foreign_entries
    )
}

fn fail(err: &mut dyn Write, e: &deck_hooks::Error) -> u8 {
    let _ = writeln!(err, "Fehler: {e}");
    1
}
