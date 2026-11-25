//! The version of Claude Code. Deck was checked against 2.1.331 (M0, M1);
//! older versions may lack hook events or fields, so Deck warns.

use std::path::PathBuf;
use std::process::Command;

/// The oldest version Deck was tested with.
pub const MIN_TESTED: (u32, u32, u32) = (2, 1, 331);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VersionCheck {
    /// At least the tested version.
    Ok { version: String },
    /// Older than the tested version.
    TooOld { version: String },
    /// Claude Code was not found or its answer could not be read.
    Unknown,
}

/// "2.1.331 (Claude Code)" gives (2, 1, 331).
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let word = text.split_whitespace().next()?;
    let mut parts = word.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    Some((major, minor, patch))
}

pub fn check_text(text: &str) -> VersionCheck {
    match parse_version(text) {
        None => VersionCheck::Unknown,
        Some(v) => {
            let version = format!("{}.{}.{}", v.0, v.1, v.2);
            if v >= MIN_TESTED {
                VersionCheck::Ok { version }
            } else {
                VersionCheck::TooOld { version }
            }
        }
    }
}

/// Where `claude` usually lives. The app does not inherit the PATH of a shell.
fn candidates(home: Option<PathBuf>) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(h) = home {
        v.push(h.join(".local/bin/claude"));
        v.push(h.join(".claude/local/claude"));
    }
    v.push(PathBuf::from("/opt/homebrew/bin/claude"));
    v.push(PathBuf::from("/usr/local/bin/claude"));
    v
}

/// Ask Claude Code for its version: the usual places first, then the PATH.
pub fn check_installed(home: Option<PathBuf>) -> VersionCheck {
    let mut programs: Vec<PathBuf> = candidates(home)
        .into_iter()
        .filter(|p| p.is_file())
        .collect();
    programs.push(PathBuf::from("claude"));
    for program in programs {
        if let Ok(out) = Command::new(program).arg("--version").output()
            && out.status.success()
        {
            let r = check_text(&String::from_utf8_lossy(&out.stdout));
            if r != VersionCheck::Unknown {
                return r;
            }
        }
    }
    VersionCheck::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(parse_version("2.1.331 (Claude Code)"), Some((2, 1, 331)));
        assert_eq!(parse_version("garbage"), None);
        assert_eq!(parse_version(""), None);
        assert!(matches!(
            check_text("2.1.331 (Claude Code)"),
            VersionCheck::Ok { .. }
        ));
        assert!(matches!(check_text("2.2.0"), VersionCheck::Ok { .. }));
        assert!(matches!(check_text("3.0.0"), VersionCheck::Ok { .. }));
        assert_eq!(
            check_text("2.1.330 (Claude Code)"),
            VersionCheck::TooOld {
                version: "2.1.330".into()
            }
        );
        assert_eq!(
            check_text("1.9.9"),
            VersionCheck::TooOld {
                version: "1.9.9".into()
            }
        );
        assert_eq!(check_text("nope"), VersionCheck::Unknown);
    }
}
