//! What a notification says, for the `Notify` of the daemon. Worded as the
//! Mac app does it, as plain data so it is tested without a system.

use deck_proto::agent::{Agent, Request, RequestState};
use deck_proto::message::NotifyKind;

/// A banner shows only a few lines and cuts the rest, so it may show less than
/// the request says. "Allow" in a notification is only for a request short
/// enough to be read there in full; anything longer offers "View" and "Deny".
pub const MAX_ALLOWABLE: usize = 60;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Allow,
    Deny,
    /// Opens the panel with the full text.
    View,
}

// Only Windows and Linux show notifications from here.
#[cfg_attr(not(any(windows, target_os = "linux")), allow(dead_code))]
impl Action {
    /// The key the system hands back when the button was pressed.
    pub fn key(self) -> &'static str {
        match self {
            Action::Allow => "allow",
            Action::Deny => "deny",
            Action::View => "view",
        }
    }

    pub fn from_key(key: &str) -> Option<Action> {
        match key {
            "allow" => Some(Action::Allow),
            "deny" => Some(Action::Deny),
            "view" => Some(Action::View),
            _ => None,
        }
    }

    pub fn label(self) -> String {
        match self {
            Action::Allow => deck_i18n::t("Allow"),
            Action::Deny => deck_i18n::t("Deny"),
            Action::View => deck_i18n::t("View"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// The request id, or `done-<agent>`: a later note of the same id
    /// replaces the earlier one, a withdrawal names it.
    pub id: String,
    pub title: String,
    pub body: String,
    pub request_id: Option<String>,
    pub actions: Vec<Action>,
    /// Stays until it is answered (held requests).
    pub sticky: bool,
}

fn display_name(a: &Agent) -> &str {
    a.project
        .as_deref()
        .or(a.title.as_deref())
        .unwrap_or(a.id.as_str())
}

pub fn short_enough(r: &Request) -> bool {
    !r.truncated && r.summary.chars().count() <= MAX_ALLOWABLE && !r.summary.contains('\n')
}

/// The notification the daemon asked for, or `None` when there is nothing
/// sensible to say (the agent or its request is already gone).
pub fn make(agent: &Agent, kind: NotifyKind, request_id: Option<&str>) -> Option<Note> {
    let name = display_name(agent);
    match kind {
        NotifyKind::Held => {
            let r = agent.request.as_ref()?;
            if Some(r.id.as_str()) != request_id || r.state != RequestState::Held {
                return None;
            }
            if let Some(first) = r.questions.first() {
                return Some(Note {
                    id: r.id.clone(),
                    title: deck_i18n::t_with("{name} has a question", &[("name", name)]),
                    body: first.question.clone(),
                    request_id: Some(r.id.clone()),
                    actions: vec![Action::View],
                    sticky: true,
                });
            }
            let allowable = short_enough(r);
            let body = if allowable {
                r.summary.clone()
            } else {
                let start: String = r.summary.chars().take(MAX_ALLOWABLE).collect();
                deck_i18n::t_with(
                    "{start}…\nLong: view it first, then allow.",
                    &[("start", &start)],
                )
            };
            Some(Note {
                id: r.id.clone(),
                title: deck_i18n::t_with("{name} is waiting for approval", &[("name", name)]),
                body,
                request_id: Some(r.id.clone()),
                actions: if allowable {
                    vec![Action::Allow, Action::Deny]
                } else {
                    vec![Action::View, Action::Deny]
                },
                sticky: true,
            })
        }
        NotifyKind::InTerminal => {
            let r = agent.request.as_ref()?;
            if Some(r.id.as_str()) != request_id {
                return None;
            }
            Some(Note {
                id: r.id.clone(),
                title: deck_i18n::t_with("{name} is waiting in the terminal", &[("name", name)]),
                body: deck_i18n::t("Answer there. Deck does not hold this request."),
                request_id: Some(r.id.clone()),
                actions: Vec::new(),
                sticky: false,
            })
        }
        NotifyKind::Done => Some(Note {
            id: format!("done-{}", agent.id),
            title: deck_i18n::t_with("{name} is done", &[("name", name)]),
            body: match &agent.branch {
                Some(b) => deck_i18n::t_with("Branch {branch}", &[("branch", b)]),
                None => deck_i18n::t("The agent is waiting for a new task."),
            },
            request_id: None,
            actions: Vec::new(),
            sticky: false,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(request: Option<serde_json::Value>) -> Agent {
        serde_json::from_value(serde_json::json!({
            "id": "s1", "host": "h", "tool": "claude", "cwd": null, "project": "shop",
            "branch": "main", "title": null, "status": "blocked", "paused": false,
            "status_since_ms": 1, "activity": null, "last_prompt": null,
            "context": null, "turns": null, "subagents": 0,
            "location": {"kind": "unknown"}, "request": request
        }))
        .expect("agent")
    }

    fn request(summary: &str, truncated: bool, state: &str) -> serde_json::Value {
        serde_json::json!({
            "id": "r1", "tool_name": "Bash", "summary": summary, "truncated": truncated,
            "full": summary, "always_allow": [], "state": state, "deadline_ms": null
        })
    }

    #[test]
    fn a_short_request_can_be_allowed_from_the_notification() {
        let a = agent(Some(request("cargo fmt --all", false, "held")));
        let n = make(&a, NotifyKind::Held, Some("r1")).expect("note");
        assert_eq!(n.actions, [Action::Allow, Action::Deny]);
        assert_eq!(n.body, "cargo fmt --all");
        assert!(n.sticky);
    }

    #[test]
    fn a_long_or_shortened_request_must_be_viewed_first() {
        let long = "x".repeat(MAX_ALLOWABLE + 1);
        for (summary, truncated) in [
            (long.as_str(), false),
            ("short", true),
            ("two\nlines", false),
        ] {
            let a = agent(Some(request(summary, truncated, "held")));
            let n = make(&a, NotifyKind::Held, Some("r1")).expect("note");
            assert_eq!(n.actions, [Action::View, Action::Deny], "{summary:?}");
            assert!(!n.actions.contains(&Action::Allow));
        }
    }

    #[test]
    fn a_question_only_offers_view() {
        let mut r = request("q", false, "held");
        r["tool_name"] = "AskUserQuestion".into();
        r["questions"] = serde_json::json!([{
            "question": "Which one?", "header": null, "multi_select": false, "options": []
        }]);
        let n = make(&agent(Some(r)), NotifyKind::Held, Some("r1")).expect("note");
        assert_eq!(n.actions, [Action::View]);
        assert_eq!(n.body, "Which one?");
    }

    #[test]
    fn nothing_for_a_request_that_is_gone_or_another_one() {
        let a = agent(Some(request("ls", false, "answered")));
        assert_eq!(make(&a, NotifyKind::Held, Some("r1")), None);
        let a = agent(Some(request("ls", false, "held")));
        assert_eq!(make(&a, NotifyKind::Held, Some("r2")), None);
        assert_eq!(make(&agent(None), NotifyKind::Held, Some("r1")), None);
    }

    #[test]
    fn in_terminal_and_done_have_no_buttons() {
        let a = agent(Some(request("ls", false, "passed_through")));
        let n = make(&a, NotifyKind::InTerminal, Some("r1")).expect("note");
        assert!(n.actions.is_empty());
        let n = make(&a, NotifyKind::Done, None).expect("note");
        assert_eq!(n.id, "done-s1");
        assert!(n.actions.is_empty());
        assert!(!n.sticky);
    }

    #[test]
    fn keys_round_trip() {
        for a in [Action::Allow, Action::Deny, Action::View] {
            assert_eq!(Action::from_key(a.key()), Some(a));
        }
        assert_eq!(Action::from_key("default"), None);
    }
}
