//! Whether Deck shows a notification with buttons for a held request, and where
//! the user is (Detailkonzept, section 7). Pure functions: the daemon supplies
//! the time and what the UIs reported.
//!
//! The dialog in the terminal is always there. This only decides about the
//! notification.

use deck_proto::message::{ApprovalMode, Parent, Presence};

/// Away after this long without input: 2 minutes.
pub const AWAY_AFTER_MS: u64 = 2 * 60 * 1000;
/// A report older than this is not believed: the app may have stopped.
/// The app reports at least every 15 seconds.
pub const STALE_AFTER_MS: u64 = 45 * 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quiet {
    /// A Focus mode such as "Do not disturb" is on. Deck does not break through (E12).
    FocusMode,
    /// The switch is on "always in the terminal".
    ModeTerminal,
    /// The terminal of the agent is in front and the user is there.
    AtTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyDecision {
    Notify,
    Quiet(Quiet),
}

/// What a UI last told, and when it arrived.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub presence: Presence,
    pub received_ms: u64,
}

impl Report {
    /// A report that is too old says nothing.
    pub fn is_fresh(&self, now_ms: u64) -> bool {
        now_ms.saturating_sub(self.received_ms) <= STALE_AFTER_MS
    }

    /// How long the user has not touched keyboard or mouse, at `now_ms`.
    pub fn idle_ms(&self, now_ms: u64) -> u64 {
        now_ms.saturating_sub(self.presence.last_input_ms)
    }
}

/// Is the frontmost app the app of the agent's terminal? The reported frontmost
/// app is the path of its bundle (`/Applications/Ghostty.app`); a process above
/// the agent belongs to it when its path lies inside that bundle. Works for any
/// terminal and for the terminal inside an editor, without a list of names.
///
/// On Linux and Windows the frontmost app is the path of its program, which
/// equals the path of one parent. Windows paths (`C:\\...`) compare without
/// regard to case, as the file system does.
pub fn terminal_in_front(frontmost: Option<&str>, parents: &[Parent]) -> bool {
    let Some(app) = frontmost
        .map(|a| a.trim_end_matches(['/', '\\']))
        .filter(|a| !a.is_empty())
    else {
        return false;
    };
    let windows = app.contains('\\');
    let fold = |s: &str| {
        if windows {
            s.to_lowercase()
        } else {
            s.to_owned()
        }
    };
    let sep = if windows { '\\' } else { '/' };
    let app = fold(app);
    parents.iter().any(|p| {
        let name = fold(&p.name);
        name == app
            || name
                .strip_prefix(app.as_str())
                .is_some_and(|rest| rest.starts_with(sep))
    })
}

/// The decision of the switch and the steps of "Automatik". `report` is the
/// presence of the UI that would show the notification: none, or an old one,
/// counts as "not at the terminal", so Deck rather notifies once too often.
/// `pane_visible` is what tmux says about the pane of the agent: `Some(false)`
/// means the terminal is in front but the pane is not shown, `None` is unknown.
pub fn decide(
    mode: ApprovalMode,
    report: Option<&Report>,
    parents: &[Parent],
    pane_visible: Option<bool>,
    now_ms: u64,
) -> NotifyDecision {
    let fresh = report.filter(|r| r.is_fresh(now_ms));
    // E12: a Focus mode wins over everything, even "always in Deck".
    if fresh.is_some_and(|r| r.presence.focus_mode) {
        return NotifyDecision::Quiet(Quiet::FocusMode);
    }
    match mode {
        ApprovalMode::Terminal => NotifyDecision::Quiet(Quiet::ModeTerminal),
        ApprovalMode::Deck => NotifyDecision::Notify,
        ApprovalMode::Auto => {
            let Some(r) = fresh else {
                return NotifyDecision::Notify;
            };
            // 1. Screen locked or away for a while: notify.
            if r.presence.screen_locked || r.idle_ms(now_ms) > AWAY_AFTER_MS {
                return NotifyDecision::Notify;
            }
            // 2. Another app in front than the terminal of this agent: notify.
            if !terminal_in_front(r.presence.frontmost_app.as_deref(), parents) {
                return NotifyDecision::Notify;
            }
            // 3. The terminal is in front, but tmux shows another pane or
            //    window, or no client is attached: the dialog is out of sight.
            if pane_visible == Some(false) {
                return NotifyDecision::Notify;
            }
            // 4. Otherwise the user is at the terminal, which shows the dialog.
            NotifyDecision::Quiet(Quiet::AtTerminal)
        }
    }
}

/// Of several UIs, the one where the user was last: its notification is the
/// only one shown (E10). `None` if no UI has a fresh report; the caller then
/// uses the first UI. Ties go to the lower index.
pub fn most_present<'a>(
    reports: impl IntoIterator<Item = (usize, Option<&'a Report>)>,
    now_ms: u64,
) -> Option<usize> {
    reports
        .into_iter()
        .filter_map(|(i, r)| {
            r.filter(|r| r.is_fresh(now_ms) && !r.presence.screen_locked)
                .map(|r| (i, r.presence.last_input_ms))
        })
        .max_by_key(|(i, input)| (*input, std::cmp::Reverse(*i)))
        .map(|(i, _)| i)
}
