//! Start the daemon at login on Windows: a value in the Run key of the user.
//!
//! This module only builds the command and reads the answer of `reg query`;
//! `reg.exe` is called by the command line. The value name is stable, so
//! install twice is the same entry. No administrator rights are needed.

use std::path::Path;

/// The key Windows reads at every login of this user.
pub const KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
/// The name of our value in [`KEY`].
pub const VALUE: &str = "Amiscara Deck";

/// The command line Windows runs at login.
pub fn command(program: &Path, data_dir: &Path) -> String {
    format!(
        "\"{}\" --data-dir \"{}\"",
        program.display(),
        data_dir.display()
    )
}

/// Our value from the output of `reg query <KEY> /v <VALUE>`, if it is there.
///
/// The line looks like `    Amiscara Deck    REG_SZ    "C:\...\amiscara-deckd.exe" --data-dir "..."`.
pub fn parse_query(output: &str) -> Option<String> {
    output.lines().find_map(|line| {
        let rest = line.trim_start().strip_prefix(VALUE)?;
        let rest = rest.trim_start().strip_prefix("REG_SZ")?;
        Some(rest.trim().to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_quotes_both_paths() {
        assert_eq!(
            command(
                Path::new(r"C:\Users\A B\AppData\Local\Programs\Amiscara Deck\amiscara-deckd.exe"),
                Path::new(r"C:\Users\A B\AppData\Local\Amiscara Deck"),
            ),
            r#""C:\Users\A B\AppData\Local\Programs\Amiscara Deck\amiscara-deckd.exe" --data-dir "C:\Users\A B\AppData\Local\Amiscara Deck""#
        );
    }

    #[test]
    fn the_value_is_read_from_reg_query() {
        let out = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\r\n    Amiscara Deck    REG_SZ    \"C:\\x\\amiscara-deckd.exe\" --data-dir \"C:\\d\"\r\n\r\n";
        assert_eq!(
            parse_query(out).as_deref(),
            Some(r#""C:\x\amiscara-deckd.exe" --data-dir "C:\d""#)
        );
        assert_eq!(
            parse_query(
                "ERROR: The system was unable to find the specified registry key or value."
            ),
            None
        );
        assert_eq!(
            parse_query("    Amiscara Deck Beta    REG_SZ    x"),
            None,
            "only our own name"
        );
    }
}
