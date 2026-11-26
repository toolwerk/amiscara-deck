//! Amiscara Deck for Linux and Windows: an icon in the tray, a panel and a
//! main window.
//!
//! No logic of its own: the daemon holds the state, `deck-client` follows it.
//! This program passes every change to the panel (event `deck://state`) and
//! the panel's answers back to the daemon. Without a daemon it says so and
//! tries again; the hook program then never holds a request.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod notes;
mod system_notes;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use deck_client::Client;
use deck_proto::agent::{Agent, RequestState, Status};
use deck_proto::message::{AnswerVia, DaemonMessage, Decision};
use serde::Serialize;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};
use tokio::sync::mpsc;

const TRAY_IDLE: &[u8] = include_bytes!("../icons/tray.png");
const TRAY_WAITING: &[u8] = include_bytes!("../icons/tray-waiting.png");
const RETRY: Duration = Duration::from_secs(2);

/// What the panel shows.
#[derive(Debug, Clone, Serialize)]
struct View {
    connected: bool,
    /// Why there is no connection, for the panel.
    problem: Option<String>,
    agents: Vec<Agent>,
    /// What today cost on this machine, in millionths of a US dollar.
    today_micro_usd: Option<u64>,
}

impl View {
    fn offline(problem: String) -> Self {
        View {
            connected: false,
            problem: Some(problem),
            agents: Vec::new(),
            today_micro_usd: None,
        }
    }
}

enum Command {
    Answer {
        request_id: String,
        allow: bool,
        /// The user saw the full text (panel and window always show it
        /// before "Allow"; a notification only offers it for a short one).
        expanded: bool,
        via: AnswerVia,
    },
}

struct Link(mpsc::UnboundedSender<Command>);

/// The last view, for a panel that loads after it was sent.
#[derive(Default)]
struct Last(std::sync::Mutex<Option<View>>);

/// The current view; `None` before the first contact with the daemon.
#[tauri::command]
fn current(last: tauri::State<'_, Last>) -> Option<View> {
    last.0.lock().ok().and_then(|v| v.clone())
}

/// The texts of the panel in the language of the system (`locale` as the
/// webview reports it), msgid to text. Empty for English.
#[tauri::command]
fn catalog(locale: String) -> BTreeMap<String, String> {
    deck_i18n::catalog(deck_i18n::Language::from_locale(&locale))
}

/// Show the main window, for the link in the panel. The panel goes away: it
/// stays on top and would cover the window.
#[tauri::command]
fn open_main(app: AppHandle) {
    if let Some(panel) = window(&app, "panel") {
        let _ = panel.hide();
    }
    show(&app, "main");
}

/// Allow or deny a held request. Only a card shown in full may be allowed:
/// the panel expands a shortened one before it offers "Allow".
#[tauri::command]
fn answer(
    window: tauri::Window,
    link: tauri::State<'_, Link>,
    request_id: String,
    allow: bool,
) -> Result<(), String> {
    let via = if window.label() == "main" {
        AnswerVia::Window
    } else {
        AnswerVia::Panel
    };
    link.0
        .send(Command::Answer {
            request_id,
            allow,
            expanded: true,
            via,
        })
        .map_err(|_| "not connected".to_owned())
}

/// What a button of a notification does. "View" and a click on the
/// notification open the panel with the full text of the request.
fn on_note(
    app: &AppHandle,
    link: &mpsc::UnboundedSender<Command>,
    request_id: Option<String>,
    action: Option<notes::Action>,
) {
    let answer = |allow| {
        if let Some(id) = request_id.clone() {
            let _ = link.send(Command::Answer {
                request_id: id,
                allow,
                // Offered only for a request short enough to read there.
                expanded: false,
                via: AnswerVia::Notification,
            });
        }
    };
    match action {
        Some(notes::Action::Allow) => answer(true),
        Some(notes::Action::Deny) => answer(false),
        Some(notes::Action::View) | None => {
            show(app, "panel");
            if let Some(id) = request_id {
                let _ = app.emit("deck://view", id);
            }
        }
    }
}

/// Show the notification the daemon asked for, or take one back.
fn notify(
    app: &AppHandle,
    notifier: &system_notes::Notifier,
    link: &mpsc::UnboundedSender<Command>,
    client: &Client,
    msg: &DaemonMessage,
) {
    match msg {
        DaemonMessage::Notify {
            agent_id,
            request_id,
            kind,
        } => {
            let Some(agent) = client.state().agents.get(agent_id) else {
                return;
            };
            let Some(note) = notes::make(agent, *kind, request_id.as_deref()) else {
                return;
            };
            let app = app.clone();
            let link = link.clone();
            let request = note.request_id.clone();
            notifier.show(
                note,
                Arc::new(move |action| on_note(&app, &link, request.clone(), action)),
            );
        }
        DaemonMessage::Withdraw { request_id } => notifier.withdraw(request_id),
        _ => {}
    }
}

/// The agents that wait for an answer in Deck.
fn waiting(agents: &[Agent]) -> usize {
    agents
        .iter()
        .filter(|a| {
            a.request
                .as_ref()
                .is_some_and(|r| r.state == RequestState::Held)
        })
        .count()
}

/// Waiting first, then working, then the rest; newest change first.
fn sorted(agents: impl Iterator<Item = Agent>) -> Vec<Agent> {
    let rank = |a: &Agent| match (&a.request, a.status) {
        (Some(r), _) if r.state == RequestState::Held => 0,
        (_, Status::Blocked) => 1,
        (_, Status::Working) => 2,
        (_, Status::Error) => 3,
        (_, Status::Done) => 4,
        (_, Status::Idle) => 5,
        (_, Status::Exited) => 6,
    };
    let mut list: Vec<Agent> = agents.collect();
    list.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then(b.status_since_ms.cmp(&a.status_since_ms))
    });
    list
}

fn publish(app: &AppHandle, view: View) {
    let n = waiting(&view.agents);
    if let Some(tray) = app.tray_by_id("deck") {
        update_tray(&tray, n, view.connected);
    }
    if let Ok(mut last) = app.state::<Last>().0.lock() {
        *last = Some(view.clone());
    }
    let _ = app.emit("deck://state", view);
}

fn update_tray(tray: &TrayIcon, waiting: usize, connected: bool) {
    let bytes = if waiting > 0 { TRAY_WAITING } else { TRAY_IDLE };
    if let Ok(icon) = Image::from_bytes(bytes) {
        let _ = tray.set_icon(Some(icon));
    }
    let tip = if !connected {
        deck_i18n::t("No daemon reachable. Start it with amiscara-deckd.")
    } else if waiting == 0 {
        format!("Amiscara Deck · {}", deck_i18n::t("Nobody is waiting."))
    } else if waiting == 1 {
        deck_i18n::t("Amiscara Deck, one agent waiting")
    } else {
        deck_i18n::t_with(
            "Amiscara Deck, {n} agents waiting",
            &[("n", &waiting.to_string())],
        )
    };
    let _ = tray.set_tooltip(Some(&tip));
}

/// Follow the daemon for as long as the program runs.
async fn follow(
    app: AppHandle,
    link: mpsc::UnboundedSender<Command>,
    mut commands: mpsc::UnboundedReceiver<Command>,
) {
    let notifier = system_notes::Notifier::new(&app.config().identifier);
    loop {
        let Some(socket) = deck_proto::paths::default_socket() else {
            publish(&app, View::offline(deck_i18n::t("No data folder found.")));
            return;
        };
        match Client::connect(&socket, "Amiscara Deck (desktop)").await {
            Ok(mut client) => {
                let view = |c: &Client| View {
                    connected: true,
                    problem: None,
                    agents: sorted(c.state().agents.values().cloned()),
                    today_micro_usd: c.state().today_micro_usd,
                };
                publish(&app, view(&client));
                loop {
                    tokio::select! {
                        msg = client.next() => match msg {
                            Ok(msg) => {
                                publish(&app, view(&client));
                                notify(&app, &notifier, &link, &client, &msg);
                            }
                            Err(_) => break,
                        },
                        cmd = commands.recv() => match cmd {
                            Some(Command::Answer { request_id, allow, expanded, via }) => {
                                let decision = if allow {
                                    Decision::Allow { always: None, expanded }
                                } else {
                                    Decision::Deny { message: None }
                                };
                                if client.answer(&request_id, decision, via).await.is_err() {
                                    break;
                                }
                            }
                            None => return,
                        },
                    }
                }
                publish(
                    &app,
                    View::offline(deck_i18n::t("The daemon closed the connection.")),
                );
            }
            Err(e) => publish(
                &app,
                View::offline(match e {
                    deck_client::Error::NoDaemon(_) => {
                        deck_i18n::t("No daemon reachable. Start it with amiscara-deckd.")
                    }
                    other => other.to_string(),
                }),
            ),
        }
        tokio::time::sleep(RETRY).await;
    }
}

fn window(app: &AppHandle, label: &str) -> Option<WebviewWindow> {
    app.get_webview_window(label)
}

fn show(app: &AppHandle, label: &str) {
    if let Some(w) = window(app, label) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn toggle_panel(app: &AppHandle) {
    let Some(w) = window(app, "panel") else {
        return;
    };
    if w.is_visible().unwrap_or(false) {
        let _ = w.hide();
    } else {
        show(app, "panel");
    }
}

fn main() {
    let (tx, rx) = mpsc::unbounded_channel();
    let link = tx.clone();
    let result = tauri::Builder::default()
        .manage(Link(tx))
        .manage(Last::default())
        .invoke_handler(tauri::generate_handler![
            catalog, answer, current, open_main
        ])
        .setup(move |app| {
            let open =
                MenuItem::with_id(app, "open", deck_i18n::t("Open panel"), true, None::<&str>)?;
            let quit = MenuItem::with_id(
                app,
                "quit",
                deck_i18n::t("Quit Amiscara Deck"),
                true,
                None::<&str>,
            )?;
            let main_window = MenuItem::with_id(
                app,
                "main",
                deck_i18n::t("Open main window"),
                true,
                None::<&str>,
            )?;
            let menu = Menu::with_items(app, &[&open, &main_window, &quit])?;
            let tray = TrayIconBuilder::with_id("deck")
                .icon(Image::from_bytes(TRAY_IDLE)?)
                .menu(&menu)
                // Left click opens the panel where the system reports it
                // (Windows); the menu works everywhere, also on Linux.
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => toggle_panel(app),
                    "main" => show(app, "main"),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_panel(tray.app_handle());
                    }
                })
                .build(app)?;
            update_tray(&tray, 0, false);
            // Closing a window hides it; the program stays in the tray.
            for label in ["panel", "main"] {
                if let Some(w) = window(app.handle(), label) {
                    let hide = w.clone();
                    w.on_window_event(move |e| {
                        if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                            api.prevent_close();
                            let _ = hide.hide();
                        }
                    });
                }
            }
            // Started by the user: show the panel. A start at login passes
            // `--hidden` and stays in the tray.
            if !std::env::args().any(|a| a == "--hidden") {
                toggle_panel(app.handle());
            }
            tauri::async_runtime::spawn(follow(app.handle().clone(), link, rx));
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("amiscara-deck-desktop: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, status: Status, since: u64) -> Agent {
        serde_json::from_value(serde_json::json!({
            "id": id, "host": "h", "tool": "claude", "cwd": null, "project": null,
            "branch": null, "title": null, "status": status, "paused": false,
            "status_since_ms": since, "activity": null, "last_prompt": null,
            "context": null, "turns": null, "subagents": 0,
            "location": {"kind": "unknown"}, "request": null
        }))
        .expect("agent")
    }

    #[test]
    fn working_comes_before_idle_and_newer_before_older() {
        let list = sorted(
            [
                agent("idle", Status::Idle, 5),
                agent("old", Status::Working, 1),
                agent("new", Status::Working, 9),
            ]
            .into_iter(),
        );
        let ids: Vec<&str> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(ids, ["new", "old", "idle"]);
        assert_eq!(waiting(&list), 0);
    }
}
