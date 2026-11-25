//! Is the tmux pane of an agent in front of the user? The pane is shown when
//! it is the active pane of the active window and a client is attached to the
//! session. The daemon asks the default tmux server; a custom socket, a missing
//! tmux or any odd answer gives `None` (unknown), never a wrong "hidden".

use std::process::{Command, Stdio};

/// `%12`: a pane id. Anything else is not passed to tmux.
fn valid_pane(pane: &str) -> bool {
    pane.strip_prefix('%')
        .is_some_and(|n| !n.is_empty() && n.len() <= 9 && n.bytes().all(|b| b.is_ascii_digit()))
}

/// "1 1 2" (pane active, window active, clients attached) to visible or not.
pub fn parse_visible(output: &str) -> Option<bool> {
    let mut f = output.split_whitespace();
    let pane = f.next()?;
    let window = f.next()?;
    let attached: u32 = f.next()?.parse().ok()?;
    if f.next().is_some() || !matches!(pane, "0" | "1") || !matches!(window, "0" | "1") {
        return None;
    }
    Some(pane == "1" && window == "1" && attached >= 1)
}

pub fn pane_visible(pane: &str) -> Option<bool> {
    if !valid_pane(pane) {
        return None;
    }
    let out = Command::new("tmux")
        .args([
            "display-message",
            "-p",
            "-t",
            pane,
            "#{pane_active} #{window_active} #{session_attached}",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    parse_visible(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_are_read_strictly() {
        assert_eq!(parse_visible("1 1 1\n"), Some(true));
        assert_eq!(parse_visible("1 1 3"), Some(true));
        assert_eq!(parse_visible("0 1 1"), Some(false));
        assert_eq!(parse_visible("1 0 1"), Some(false));
        assert_eq!(parse_visible("1 1 0"), Some(false));
        assert_eq!(parse_visible(""), None);
        assert_eq!(parse_visible("1 1"), None);
        assert_eq!(parse_visible("1 1 x"), None);
        assert_eq!(parse_visible("2 1 1"), None);
        assert_eq!(parse_visible("1 1 1 1"), None);
    }

    #[test]
    fn only_pane_ids_reach_tmux() {
        assert!(valid_pane("%0"));
        assert!(valid_pane("%123"));
        assert!(!valid_pane("%"));
        assert!(!valid_pane("%1;rm"));
        assert!(!valid_pane("-t"));
        assert!(!valid_pane("session:0.1"));
        assert_eq!(pane_visible("not a pane"), None);
    }
}
