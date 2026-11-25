//! Start the daemon at login on macOS: a LaunchAgent.
//!
//! This module only writes and reads the plist; `launchctl` is called by the
//! command line. The label and file name are stable, so install twice is the
//! same file.

use std::path::{Path, PathBuf};

pub const LABEL: &str = "net.staude.amiscara.deckd";

/// `~/Library/LaunchAgents/<label>.plist`
pub fn plist_path(home: &Path) -> PathBuf {
    home.join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// The plist for the daemon at `program`, logging into `data_dir`.
///
/// `KeepAlive` restarts it after a crash, but not after a clean stop
/// (`SuccessfulExit` false), so `launchctl bootout` and SIGTERM end it.
pub fn plist(program: &Path, data_dir: &Path) -> String {
    let log = data_dir.join("deckd.log");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{program}</string>
        <string>--data-dir</string>
        <string>{data}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>ProcessType</key>
    <string>Background</string>
    <key>StandardErrorPath</key>
    <string>{log}</string>
    <key>StandardOutPath</key>
    <string>{log}</string>
</dict>
</plist>
"#,
        label = LABEL,
        program = xml(&program.to_string_lossy()),
        data = xml(&data_dir.to_string_lossy()),
        log = xml(&log.to_string_lossy()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_plist_has_the_label_the_program_and_runs_at_login() {
        let p = plist(
            Path::new("/Applications/Amiscara Deck.app/Contents/MacOS/amiscara-deckd"),
            Path::new("/Users/x/Library/Application Support/Amiscara Deck"),
        );
        assert!(p.contains("<string>net.staude.amiscara.deckd</string>"));
        assert!(p.contains(
            "<string>/Applications/Amiscara Deck.app/Contents/MacOS/amiscara-deckd</string>"
        ));
        assert!(p.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(p.contains("<string>--data-dir</string>"));
        assert!(
            p.contains("<key>SuccessfulExit</key>\n        <false/>"),
            "a clean stop stays stopped"
        );
    }

    #[test]
    fn special_characters_in_paths_cannot_break_the_xml() {
        let p = plist(Path::new("/a&b/<x>\"y\"/'z'/deckd"), Path::new("/d"));
        assert!(
            p.contains("/a&amp;b/&lt;x&gt;&quot;y&quot;/&apos;z&apos;/deckd"),
            "{p}"
        );
        assert!(!p.contains("<x>"));
    }

    #[test]
    fn the_path_is_stable() {
        assert_eq!(
            plist_path(Path::new("/Users/x")),
            PathBuf::from("/Users/x/Library/LaunchAgents/net.staude.amiscara.deckd.plist")
        );
    }
}
