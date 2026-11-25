//! `answer`: allow or deny a held request, mostly to try the daemon by hand.

use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use deck_client::{Client, Error};
use deck_proto::agent::RequestState;
use deck_proto::message::{AnswerVia, DaemonMessage, Decision};

/// How long to wait for the daemon to show that the answer arrived.
const CONFIRM: Duration = Duration::from_secs(3);

pub fn run(opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut rest = opts.iter();
    let (Some(id), Some(verb)) = (rest.next(), rest.next()) else {
        let _ = writeln!(
            err,
            "Aufruf: amiscara-deck answer <Anfrage> allow|deny [--always <Nr>] [--message <Text>] [--socket <Datei>]"
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
                    let _ = writeln!(err, "--always braucht eine Zahl.");
                    return 2;
                }
            },
            "--seen" => seen = true,
            "--message" => message = rest.next().map(|m| m.to_string()),
            other => {
                let _ = writeln!(err, "Unbekannte Option: {other}");
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
            let _ = writeln!(err, "--message gibt es nur bei deny.");
            return 2;
        }
        ("deny", Some(_), _) => {
            let _ = writeln!(err, "--always gibt es nur bei allow.");
            return 2;
        }
        _ => {
            let _ = writeln!(err, "Erlaubt sind allow und deny.");
            return 2;
        }
    };
    let Some(socket) = socket.or_else(deck_proto::paths::default_socket) else {
        let _ = writeln!(err, "Kein Home-Ordner gefunden: bitte --socket angeben.");
        return 2;
    };
    let Ok(rt) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        let _ = writeln!(err, "Fehler: keine Laufzeit.");
        return 1;
    };
    rt.block_on(async {
        let mut client = match Client::connect(&socket, "amiscara-deck answer").await {
            Ok(c) => c,
            Err(Error::NoDaemon(_)) => {
                let _ = writeln!(
                    err,
                    "Kein Dienst unter {} erreichbar. Starten mit amiscara-deckd.",
                    socket.display()
                );
                return 1;
            }
            Err(e) => {
                let _ = writeln!(err, "Fehler: {e}");
                return 1;
            }
        };
        let Some(request) = client
            .state()
            .agents
            .values()
            .find_map(|a| a.request.clone().filter(|r| r.id == *id))
        else {
            let _ = writeln!(err, "Keine Anfrage mit der Kennung {id}.");
            return 1;
        };
        if request.state != RequestState::Held {
            let _ = writeln!(
                err,
                "Die Anfrage {id} wird nicht gehalten, sie wartet im Terminal."
            );
            return 1;
        }
        // A question takes picked options, which the command line cannot give.
        if !request.questions.is_empty() && matches!(decision, Decision::Allow { .. }) {
            let _ = writeln!(
                err,
                "Das ist eine Frage. Beantworten Sie sie im Deck oder dort, wo der Agent läuft."
            );
            return 1;
        }
        // Shortened card: show everything first, allow only when it was read.
        if request.truncated && matches!(decision, Decision::Allow { expanded: false, .. }) {
            let _ = writeln!(
                err,
                "Die Anfrage ist gekürzt. Der ganze Text:\n\n{}\n\nNichts erlaubt. Nach dem Lesen mit --seen erlauben.",
                request.full
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
            let _ = writeln!(
                err,
                "Diese Regel wird nicht angeboten. Angeboten: {}",
                if offered.is_empty() {
                    "keine".into()
                } else {
                    offered.join("; ")
                }
            );
            return 1;
        }
        if client.answer(id, decision.clone(), AnswerVia::Cli).await.is_err() {
            let _ = writeln!(err, "Fehler: Antwort konnte nicht gesendet werden.");
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
                    "Erlaubt."
                } else {
                    "Abgelehnt."
                }
            );
            0
        } else {
            let _ = writeln!(
                err,
                "Keine Bestätigung: Die Anfrage war schon beantwortet oder abgelaufen."
            );
            1
        }
    })
}
