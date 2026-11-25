use deck_proto::message::{ApprovalMode, Parent, Presence};
use deck_session::notify::{
    AWAY_AFTER_MS, NotifyDecision, Quiet, Report, STALE_AFTER_MS, decide, most_present,
    terminal_in_front,
};

const NOW: u64 = 10_000_000;
const GHOSTTY: &str = "/Applications/Ghostty.app";

fn parents() -> Vec<Parent> {
    vec![
        Parent {
            pid: 10,
            name: "claude".into(),
        },
        Parent {
            pid: 9,
            name: "/bin/zsh".into(),
        },
        Parent {
            pid: 8,
            name: format!("{GHOSTTY}/Contents/MacOS/ghostty"),
        },
        Parent {
            pid: 1,
            name: "/sbin/launchd".into(),
        },
    ]
}

fn report(app: Option<&str>, idle_ms: u64, locked: bool, focus: bool) -> Report {
    Report {
        presence: Presence {
            screen_locked: locked,
            idle_secs: idle_ms / 1000,
            frontmost_app: app.map(str::to_owned),
            focus_mode: focus,
            last_input_ms: NOW - idle_ms,
        },
        received_ms: NOW - 1000,
    }
}

#[test]
fn the_terminal_of_the_agent_is_found_by_the_path_of_its_bundle() {
    assert!(terminal_in_front(Some(GHOSTTY), &parents()));
    assert!(
        terminal_in_front(Some("/Applications/Ghostty.app/"), &parents()),
        "a trailing slash does not matter"
    );
    assert!(!terminal_in_front(
        Some("/Applications/Safari.app"),
        &parents()
    ));
    assert!(!terminal_in_front(None, &parents()));
    assert!(!terminal_in_front(Some(""), &parents()));
    assert!(
        !terminal_in_front(Some(GHOSTTY), &[]),
        "an agent whose parents are unknown is not matched"
    );
}

#[test]
fn a_bundle_that_only_starts_with_the_same_letters_is_not_the_terminal() {
    let p = vec![Parent {
        pid: 8,
        name: "/Applications/Ghostty.app.old/Contents/MacOS/x".into(),
    }];
    assert!(!terminal_in_front(Some(GHOSTTY), &p));
    let q = vec![Parent {
        pid: 8,
        name: "/Applications/GhosttyHelper.app/Contents/MacOS/x".into(),
    }];
    assert!(!terminal_in_front(Some(GHOSTTY), &q));
}

#[test]
fn the_terminal_inside_an_editor_matches_the_editor() {
    let code = "/Applications/Visual Studio Code.app";
    let p = vec![Parent {
        pid: 8,
        name: format!(
            "{code}/Contents/Frameworks/Code Helper (Plugin).app/Contents/MacOS/Code Helper (Plugin)"
        ),
    }];
    assert!(terminal_in_front(Some(code), &p));
}

#[test]
fn auto_is_quiet_only_when_the_user_is_at_the_terminal() {
    let at = report(Some(GHOSTTY), 3_000, false, false);
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&at), &parents(), None, NOW),
        NotifyDecision::Quiet(Quiet::AtTerminal)
    );
    let elsewhere = report(Some("/Applications/Safari.app"), 3_000, false, false);
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&elsewhere), &parents(), None, NOW),
        NotifyDecision::Notify
    );
}

#[test]
fn a_locked_screen_or_a_long_idle_time_notifies_even_with_the_terminal_in_front() {
    let locked = report(Some(GHOSTTY), 3_000, true, false);
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&locked), &parents(), None, NOW),
        NotifyDecision::Notify
    );
    let away = report(Some(GHOSTTY), AWAY_AFTER_MS + 1, false, false);
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&away), &parents(), None, NOW),
        NotifyDecision::Notify
    );
    let just_back = report(Some(GHOSTTY), AWAY_AFTER_MS, false, false);
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&just_back), &parents(), None, NOW),
        NotifyDecision::Quiet(Quiet::AtTerminal),
        "exactly 2 minutes is not yet away"
    );
}

#[test]
fn no_report_or_an_old_one_means_notify() {
    assert_eq!(
        decide(ApprovalMode::Auto, None, &parents(), None, NOW),
        NotifyDecision::Notify
    );
    let mut old = report(Some(GHOSTTY), 1_000, false, false);
    old.received_ms = NOW - STALE_AFTER_MS - 1;
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&old), &parents(), None, NOW),
        NotifyDecision::Notify,
        "the app may have stopped: do not trust it"
    );
}

#[test]
fn deck_mode_always_notifies_and_terminal_mode_never() {
    let at = report(Some(GHOSTTY), 1_000, false, false);
    assert_eq!(
        decide(ApprovalMode::Deck, Some(&at), &parents(), None, NOW),
        NotifyDecision::Notify
    );
    assert_eq!(
        decide(ApprovalMode::Deck, None, &parents(), None, NOW),
        NotifyDecision::Notify
    );
    let away = report(Some("/Applications/Safari.app"), 1_000, false, false);
    assert_eq!(
        decide(ApprovalMode::Terminal, Some(&away), &parents(), None, NOW),
        NotifyDecision::Quiet(Quiet::ModeTerminal)
    );
}

#[test]
fn a_focus_mode_wins_over_every_switch_position_but_the_report_must_be_fresh() {
    let focus = report(Some("/Applications/Safari.app"), 1_000, false, true);
    for mode in [
        ApprovalMode::Auto,
        ApprovalMode::Deck,
        ApprovalMode::Terminal,
    ] {
        assert_eq!(
            decide(mode, Some(&focus), &parents(), None, NOW),
            NotifyDecision::Quiet(Quiet::FocusMode),
            "{mode:?}"
        );
    }
    let mut stale = focus.clone();
    stale.received_ms = NOW - STALE_AFTER_MS - 1;
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&stale), &parents(), None, NOW),
        NotifyDecision::Notify,
        "an old Focus report is not believed"
    );
}

#[test]
fn the_ui_where_the_user_was_last_is_chosen() {
    let a = report(Some("x"), 60_000, false, false);
    let b = report(Some("y"), 2_000, false, false);
    let c = report(Some("z"), 30_000, false, false);
    assert_eq!(
        most_present([(0, Some(&a)), (1, Some(&b)), (2, Some(&c))], NOW),
        Some(1)
    );
    assert_eq!(most_present([(0, Some(&a)), (1, None)], NOW), Some(0));
    assert_eq!(
        most_present([(0, None), (1, None)], NOW),
        None,
        "the caller picks the first UI"
    );
}

#[test]
fn a_locked_or_stale_machine_is_not_where_the_user_is() {
    let at_home = report(Some("x"), 5_000, false, false);
    let locked_laptop = report(Some("y"), 1_000, true, false);
    assert_eq!(
        most_present([(0, Some(&at_home)), (1, Some(&locked_laptop))], NOW),
        Some(0)
    );
    let mut stale = report(Some("z"), 100, false, false);
    stale.received_ms = NOW - STALE_AFTER_MS - 5;
    assert_eq!(
        most_present([(0, Some(&at_home)), (1, Some(&stale))], NOW),
        Some(0)
    );
}

#[test]
fn a_tie_goes_to_the_first_ui() {
    let a = report(Some("x"), 5_000, false, false);
    let b = report(Some("y"), 5_000, false, false);
    assert_eq!(most_present([(3, Some(&a)), (1, Some(&b))], NOW), Some(1));
}

#[test]
fn a_hidden_tmux_pane_notifies_even_with_the_terminal_in_front() {
    let at = report(Some(GHOSTTY), 3_000, false, false);
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&at), &parents(), Some(false), NOW),
        NotifyDecision::Notify
    );
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&at), &parents(), Some(true), NOW),
        NotifyDecision::Quiet(Quiet::AtTerminal)
    );
    assert_eq!(
        decide(ApprovalMode::Auto, Some(&at), &parents(), None, NOW),
        NotifyDecision::Quiet(Quiet::AtTerminal)
    );
}
