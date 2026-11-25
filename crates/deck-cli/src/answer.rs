//! `answer`: allow or deny a held request, mostly to try the daemon by hand.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use deck_client::{Client, Error};
use deck_i18n::{t, t_with};
use deck_proto::agent::RequestState;
use deck_proto::message::{AnswerVia, DaemonMessage, Decision};

use crate::view;

/// How long to wait for the daemon to show that the answer arrived.
const CONFIRM: Duration = Duration::from_secs(3);

pub fn run(opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut rest = opts.iter();
    let (Some(id), Some(verb)) = (rest.next(), rest.next()) else {
        let _ = writeln!(
            err,
            "{}",
            t(
                "Usage: amiscara-deck answer <request> allow|deny [--always <no>] [--message <text>] [--socket <file>]"
            )
        );
        return 2;
    };
    let mut socket = None;
    let mut always = None;
    let mut message = None;
    let mut seen = false;
    while let Some(o) = rest.next() {
        match *o {
            "--socket" => socket = rest.next().map(PathBuf::from),
            "--always" => match rest.next().and_then(|n| n.parse::<usize>().ok()) {
                Some(n) => always = Some(n),
                None => {
                    let _ = writeln!(err, "{}", t("--always needs a number."));
                    return 2;
                }
            },
            "--seen" => seen = true,
            "--message" => message = rest.next().map(|m| m.to_string()),
            other => {
                let _ = writeln!(
                    err,
                    "{}",
                    t_with("Unknown option: {option}", &[("option", other)])
                );
                return 2;
            }
        }
    }
    let decision = match (*verb, always, &message) {
        ("allow", always, None) => Decision::Allow {
            always,
            expanded: seen,
        },
        ("deny", None, message) => Decision::Deny {
            message: message.clone(),
        },
        ("allow", _, Some(_)) => {
            let _ = writeln!(err, "{}", t("--message only goes with deny."));
            return 2;
        }
        ("deny", Some(_), _) => {
            let _ = writeln!(err, "{}", t("--always only goes with allow."));
            return 2;
        }
        _ => {
            let _ = writeln!(err, "{}", t("Use allow or deny."));
            return 2;
        }
    };
    let Some(socket) = socket.or_else(deck_proto::paths::default_socket) else {
        let _ = writeln!(err, "{}", t("No home folder found: please give --socket."));
        return 2;
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        let _ = writeln!(err, "{}", t("Error: no runtime."));
        return 1;
    };
    rt.block_on(async {
        let mut client = match Client::connect(&socket, "amiscara-deck answer").await {
            Ok(c) => c,
            Err(Error::NoDaemon(_)) => {
                let _ = writeln!(err, "{}", view::no_daemon(&socket));
                return 1;
            }
            Err(e) => {
                let _ = writeln!(err, "{}", view::error(&e));
                return 1;
            }
        };
        let Some(request) = client
            .state()
            .agents
            .values()
            .find_map(|a| a.request.clone().filter(|r| r.id == *id))
        else {
            let _ = writeln!(err, "{}", t_with("No request with the id {id}.", &[("id", id)]));
            return 1;
        };
        if request.state != RequestState::Held {
            let _ = writeln!(
                err,
                "{}",
                t_with(
                    "The request {id} is not held, it waits in the terminal.",
                    &[("id", id)]
                )
            );
            return 1;
        }
        // A question takes picked options, which the command line cannot give.
        if !request.questions.is_empty() && matches!(decision, Decision::Allow { .. }) {
            let _ = writeln!(
                err,
                "{}",
                t("This is a question. Answer it in Deck or where the agent runs.")
            );
            return 1;
        }
        // Shortened card: show everything first, allow only when it was read.
        if request.truncated && matches!(decision, Decision::Allow { expanded: false, .. }) {
            let full = view::request_text(&request.full, &request.headings, &request.extra);
            let _ = writeln!(
                err,
                "{}",
                t_with(
                    "The request is shortened. The whole text:\n\n{text}\n\nNothing allowed. After reading, allow with --seen.",
                    &[("text", &full)]
                )
            );
            return 1;
        }
        if let Decision::Allow { always: Some(n), .. } = &decision
            && !request.always_allow.iter().any(|r| r.index == *n)
        {
            let offered: Vec<String> = request
                .always_allow
                .iter()
                .map(|r| format!("{}: {}", r.index, r.rules.join(", ")))
                .collect();
            let offered = if offered.is_empty() {
                t("none")
            } else {
                offered.join("; ")
            };
            let _ = writeln!(
                err,
                "{}",
                t_with(
                    "This rule is not offered. Offered: {offered}",
                    &[("offered", &offered)]
                )
            );
            return 1;
        }
        if client.answer(id, decision.clone(), AnswerVia::Cli).await.is_err() {
            let _ = writeln!(err, "{}", t("Error: the answer could not be sent."));
            return 1;
        }
        let confirmed = tokio::time::timeout(CONFIRM, async {
            loop {
                match client.next().await {
                    Ok(DaemonMessage::AgentChanged(a))
                        if a.request
                            .as_ref()
                            .is_some_and(|r| r.id == *id && r.state == RequestState::Answered) =>
                    {
                        return true;
                    }
                    Ok(_) => {}
                    Err(_) => return false,
                }
            }
        })
        .await
        .unwrap_or(false);
        if confirmed {
            let _ = writeln!(
                out,
                "{}",
                if matches!(decision, Decision::Allow { .. }) {
                    t("Allowed.")
                } else {
                    t("Denied.")
                }
            );
            0
        } else {
            let _ = writeln!(
                err,
                "{}",
                t("No confirmation: the request was already answered or ran out.")
            );
            1
        }
    })
}
