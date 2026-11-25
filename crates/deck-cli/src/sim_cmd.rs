//! `sim`: play simulated agents against the daemon.

use std::io::Write;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use deck_i18n::{t, t_with};
use deck_sim::{Event, Options, SCENARIOS};

pub fn run(opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let mut name = None;
    let mut speed = 1.0_f64;
    let mut wait = 60_u64;
    let mut socket = None;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match *o {
            "--list" => {
                for s in SCENARIOS {
                    let _ = writeln!(out, "{s}");
                }
                return 0;
            }
            "--speed" => match it
                .next()
                .and_then(|v| v.parse::<f64>().ok())
                .filter(|v| *v > 0.0)
            {
                Some(v) => speed = v,
                None => {
                    let _ = writeln!(err, "{}", t("--speed needs a number above 0."));
                    return 2;
                }
            },
            "--wait" => match it.next().and_then(|v| v.parse::<u64>().ok()) {
                Some(v) => wait = v,
                None => {
                    let _ = writeln!(err, "{}", t("--wait needs a number of seconds."));
                    return 2;
                }
            },
            "--socket" => socket = it.next().map(PathBuf::from),
            other if !other.starts_with("--") && name.is_none() => name = Some(other),
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
    let name = name.unwrap_or("demo");
    let Some(sessions) = deck_sim::scenario(name) else {
        let _ = writeln!(
            err,
            "{}",
            t_with(
                "Unknown scenario: {name}. Available: {list}",
                &[("name", name), ("list", &SCENARIOS.join(", "))]
            )
        );
        return 2;
    };
    let Some(socket) = socket.or_else(deck_proto::paths::default_socket) else {
        let _ = writeln!(err, "{}", t("No home folder found: please give --socket."));
        return 2;
    };
    let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    else {
        let _ = writeln!(err, "{}", t("Error: no runtime."));
        return 1;
    };
    let (tx, rx) = mpsc::channel::<Event>();
    let options = Options {
        speed,
        permission_wait: Duration::from_secs(wait),
    };
    let player = rt.spawn(async move {
        deck_sim::play(&socket, sessions, &options, move |e| {
            let _ = tx.send(e);
        })
        .await;
    });
    let mut code = 0;
    // The channel closes when the player is done and its callbacks are dropped.
    for e in rx {
        match e {
            Event::Sent { session, event } => {
                let _ = writeln!(out, "{session}: {event}");
            }
            Event::Answered { session, output } => {
                let line = match output
                    .as_ref()
                    .and_then(|o| o.pointer("/hookSpecificOutput/decision/behavior"))
                    .and_then(|b| b.as_str())
                {
                    Some("allow") => t_with("{session}: request allowed", &[("session", &session)]),
                    Some("deny") => t_with("{session}: request denied", &[("session", &session)]),
                    _ => t_with(
                        "{session}: request goes on without an answer (terminal)",
                        &[("session", &session)],
                    ),
                };
                let _ = writeln!(out, "{line}");
            }
            Event::NoDaemon => {
                let _ = writeln!(
                    err,
                    "{}",
                    t("No daemon reachable. Start it with amiscara-deckd.")
                );
                code = 1;
            }
        }
        let _ = out.flush();
    }
    let _ = rt.block_on(player);
    code
}
