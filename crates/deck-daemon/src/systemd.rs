//! Start the daemon at login on Linux: a systemd user unit.
//!
//! This module only writes the unit text; `systemctl --user` is called by the
//! command line. The unit name and file name are stable, so install twice is
//! the same file.

use std::path::{Path, PathBuf};

pub const UNIT: &str = "amiscara-deckd.service";

/// `$XDG_CONFIG_HOME/systemd/user/<unit>`, else `~/.config/systemd/user/<unit>`.
pub fn unit_path(home: &Path, config_home: Option<&Path>) -> PathBuf {
    config_home
        .map(Path::to_path_buf)
        .unwrap_or_else(|| home.join(".config"))
        .join("systemd/user")
        .join(UNIT)
}

/// One word of `ExecStart`: in double quotes, with what systemd would expand
/// (`%` specifiers, `$` variables) and the quote syntax escaped.
fn word(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '%' => out.push_str("%%"),
            '$' => out.push_str("$$"),
            '\n' | '\r' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The unit for the daemon at `program`, with its data in `data_dir`.
///
/// `Restart=on-failure` restarts it after a crash, but not after a clean stop,
/// so `systemctl --user stop` and SIGTERM end it. The log goes to the journal
/// (`journalctl --user -u amiscara-deckd`).
pub fn unit(program: &Path, data_dir: &Path) -> String {
    format!(
        "[Unit]
Description=Amiscara Deck daemon
Documentation=https://github.com/toolwerk/amiscara-deck

[Service]
Type=simple
ExecStart={program} --data-dir {data}
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
",
        program = word(&program.to_string_lossy()),
        data = word(&data_dir.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_starts_the_program_with_its_data_folder_at_login() {
        let u = unit(
            Path::new("/opt/amiscara deck/amiscara-deckd"),
            Path::new("/home/x/.local/share/amiscara-deck"),
        );
        assert!(u.contains(
            "ExecStart=\"/opt/amiscara deck/amiscara-deckd\" --data-dir \"/home/x/.local/share/amiscara-deck\""
        ));
        assert!(u.contains("WantedBy=default.target"));
        assert!(
            u.contains("Restart=on-failure"),
            "a clean stop stays stopped"
        );
    }

    #[test]
    fn special_characters_in_paths_are_not_expanded() {
        let u = unit(Path::new("/a\"b/%h/$HOME/c\\d"), Path::new("/d"));
        assert!(u.contains(r#"ExecStart="/a\"b/%%h/$$HOME/c\\d""#), "{u}");
        let u = unit(Path::new("/a\nExecStartPre=/bin/evil"), Path::new("/d"));
        assert!(!u.contains("\nExecStartPre"), "{u}");
    }

    #[test]
    fn the_path_follows_xdg_config_home() {
        assert_eq!(
            unit_path(Path::new("/home/x"), None),
            PathBuf::from("/home/x/.config/systemd/user/amiscara-deckd.service")
        );
        assert_eq!(
            unit_path(Path::new("/home/x"), Some(Path::new("/cfg"))),
            PathBuf::from("/cfg/systemd/user/amiscara-deckd.service")
        );
    }
}
