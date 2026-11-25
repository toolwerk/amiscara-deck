//! `list` and `watch`: look at the daemon.

use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use deck_client::{Client, Error};
use deck_i18n::{t, t_with};
use deck_proto::message::DaemonMessage;

use crate::view;

struct Opts {
    socket: PathBuf,
    json: bool,
}

fn parse(opts: &[&str], err: &mut dyn Write) -> Option<Opts> {
    let mut socket = None;
    let mut json = false;
    let mut it = opts.iter();
    while let Some(o) = it.next() {
        match *o {
            "--socket" => socket = it.next().map(PathBuf::from),
            "--json" => json = true,
            other => {
                let _ = writeln!(
                    err,
                    "{}",
                    t_with("Unknown option: {option}", &[("option", other)])
                );
                return None;
            }
        }
    }
    let Some(socket) = socket.or_else(deck_proto::paths::default_socket) else {
        let _ = writeln!(err, "{}", t("No home folder found: please give --socket."));
        return None;
    };
    Some(Opts { socket, json })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}

fn explain(e: &Error, socket: &std::path::Path) -> String {
    match e {
        Error::NoDaemon(_) => view::no_daemon(socket),
        Error::VersionMismatch { daemon, ours } => t_with(
            "The daemon speaks protocol {daemon}, this program {ours}. Restart the daemon.",
            &[("daemon", &daemon.to_string()), ("ours", &ours.to_string())],
        ),
        other => view::error(other),
    }
}

pub fn list(opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let Some(o) = parse(opts, err) else { return 2 };
    let Ok(rt) = runtime() else {
        let _ = writeln!(err, "{}", t("Error: no runtime."));
        return 1;
    };
    match rt.block_on(deck_client::fetch(&o.socket, "amiscara-deck list")) {
        Ok(state) => {
            let agents: Vec<_> = view::sorted(state.agents.into_values().collect());
            if o.json {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string(&agents).unwrap_or_default()
                );
            } else {
                let _ = out.write_all(view::table(&agents, now_ms()).as_bytes());
            }
            0
        }
        Err(e) => {
            let _ = writeln!(err, "{}", explain(&e, &o.socket));
            1
        }
    }
}

pub fn watch(opts: &[&str], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let Some(o) = parse(opts, err) else { return 2 };
    let Ok(rt) = runtime() else {
        let _ = writeln!(err, "{}", t("Error: no runtime."));
        return 1;
    };
    rt.block_on(async {
        let mut client = match Client::connect(&o.socket, "amiscara-deck watch").await {
            Ok(c) => c,
            Err(e) => {
                let _ = writeln!(err, "{}", explain(&e, &o.socket));
                return 1;
            }
        };
        if o.json {
            let first: Vec<_> = client.state().agents.values().collect();
            let _ = writeln!(out, "{}", serde_json::to_string(&first).unwrap_or_default());
        } else {
            let agents: Vec<_> = client.state().agents.values().cloned().collect();
            let _ = out.write_all(view::table(&agents, now_ms()).as_bytes());
        }
        let _ = out.flush();
        loop {
            // Remember how the agent looked, to say what changed.
            let before = client.state().clone();
            let msg = match client.next().await {
                Ok(m) => m,
                Err(Error::Closed) => {
                    let _ = writeln!(err, "{}", t("The daemon closed the connection."));
                    return 1;
                }
                Err(e) => {
                    let _ = writeln!(err, "{}", explain(&e, &o.socket));
                    return 1;
                }
            };
            if o.json {
                let _ = writeln!(out, "{}", serde_json::to_string(&msg).unwrap_or_default());
            } else {
                match &msg {
                    DaemonMessage::AgentChanged(a) => {
                        let _ = writeln!(out, "{}", view::change_line(before.agents.get(&a.id), a));
                    }
                    DaemonMessage::AgentRemoved { id } => {
                        let name = before
                            .agents
                            .get(id)
                            .and_then(|a| a.project.clone())
                            .unwrap_or_else(|| id.clone());
                        let _ = writeln!(
                            out,
                            "{}",
                            t_with("{name}: removed", &[("name", &view::cell(&name, 24))])
                        );
                    }
                    _ => {}
                }
            }
            let _ = out.flush();
        }
    })
}
