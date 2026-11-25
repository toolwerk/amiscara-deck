//! The command line `amiscara-deck`.
//!
//! Exit codes: 0 done, 1 failed, 2 wrong usage, 3 (`hooks status` only) the
//! hooks are not all installed.

mod answer;
mod daemon_cmd;
mod hooks_cmd;
mod live;
mod sim_cmd;
mod view;

use std::io::Write;

pub const USAGE: &str = "\
Amiscara Deck

Aufruf:
  amiscara-deck answer <Anfrage> allow|deny [--always <Nr>] [--message <Text>] [--seen] [--socket <Datei>]
  amiscara-deck sim [Szenario] [--speed <Faktor>] [--wait <Sekunden>] [--list] [--socket <Datei>]
  amiscara-deck list  [--socket <Datei>] [--json]
  amiscara-deck watch [--socket <Datei>] [--json]
  amiscara-deck hooks install   [--settings <Datei>] [--program <Pfad>] [--force]
  amiscara-deck hooks uninstall [--settings <Datei>] [--program <Pfad>]
  amiscara-deck hooks status    [--settings <Datei>] [--program <Pfad>]

  --socket    Socket des Dienstes (Vorgabe: im Datenordner von Amiscara Deck)
  --speed     sim: Abspielgeschwindigkeit (1 = Echtzeit)
  --wait      sim: wie lange eine Anfrage auf Antwort wartet (Vorgabe 60)
  --json      Ausgabe als JSON (watch: eine Zeile je Nachricht)
  --settings  Einstellungsdatei von Claude Code (Vorgabe: ~/.claude/settings.json)
  --program   Pfad des Hook-Programms (Vorgabe: deck-hook neben diesem Programm)
  --force     Auch einbauen, wenn das Hook-Programm fehlt oder nicht ausführbar ist

Beendigungscodes: 0 erledigt, 1 Fehler, 2 falscher Aufruf,
3 (status) nicht alle Hooks eingebaut.
";

/// Run the command line. `args` without the program name. Returns the exit code.
pub fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let rest: Vec<&str> = args.iter().map(String::as_str).collect();
    match rest.as_slice() {
        ["hooks", sub, opts @ ..] => hooks_cmd::run(sub, opts, out, err),
        ["daemon", sub, opts @ ..] => daemon_cmd::run(sub, opts, out, err),
        ["answer", opts @ ..] => answer::run(opts, out, err),
        ["sim", opts @ ..] => sim_cmd::run(opts, out, err),
        ["list", opts @ ..] => live::list(opts, out, err),
        ["watch", opts @ ..] => live::watch(opts, out, err),
        ["--help"] | ["-h"] | ["help"] => {
            let _ = out.write_all(USAGE.as_bytes());
            0
        }
        _ => {
            let _ = err.write_all(USAGE.as_bytes());
            2
        }
    }
}
