//! The command line `amiscara-deck`.
//!
//! Exit codes: 0 done, 1 failed, 2 wrong usage, 3 (`hooks status` only) the
//! hooks are not all installed.
//!
//! Texts are English in the code and German from `po/de.po`, by the language
//! of the environment (`deck-i18n`).

mod answer;
mod daemon_cmd;
mod daemon_win;
mod hooks_cmd;
mod live;
mod sim_cmd;
mod view;

use std::io::Write;

use deck_i18n::t;

/// The help text, in the language of the user.
pub fn usage() -> String {
    t("\
Amiscara Deck

Usage:
  amiscara-deck answer <request> allow|deny [--always <no>] [--message <text>] [--seen] [--socket <file>]
  amiscara-deck sim [scenario] [--speed <factor>] [--wait <seconds>] [--list] [--socket <file>]
  amiscara-deck list  [--socket <file>] [--json]
  amiscara-deck watch [--socket <file>] [--json]
  amiscara-deck hooks install   [--settings <file>] [--program <path>] [--force]
  amiscara-deck hooks uninstall [--settings <file>] [--program <path>]
  amiscara-deck hooks status    [--settings <file>] [--program <path>]
  amiscara-deck daemon install|uninstall|status [--program <path>] [--data-dir <folder>]

  --socket    Socket of the daemon (default: in the data folder of Amiscara Deck)
  --speed     sim: playback speed (1 = real time)
  --wait      sim: how long a request waits for an answer (default 60)
  --json      Output as JSON (watch: one line per message)
  --settings  Settings file of Claude Code (default: ~/.claude/settings.json)
  --program   Path of the hook program (default: deck-hook next to this program)
  --force     Install even if the hook program is missing or not executable
  daemon      Start the daemon at login: LaunchAgent on macOS, systemd user unit on Linux,
              Run key on Windows

Exit codes: 0 done, 1 failed, 2 wrong usage,
3 (status) not all hooks installed.
")
}

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
            let _ = out.write_all(usage().as_bytes());
            0
        }
        _ => {
            let _ = err.write_all(usage().as_bytes());
            2
        }
    }
}
