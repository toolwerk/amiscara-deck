//! Shows a [`Note`] as a notification of the system. Windows: a toast with
//! buttons. Linux: a notification over D-Bus, with buttons where the desktop
//! offers them, otherwise without (then a click or the panel answers).
//! Elsewhere nothing: the Mac has its own app.

use std::sync::Arc;

use crate::notes::{Action, Note};

/// Called with the button pressed, or `None` for a click on the notification
/// itself (which opens the panel).
pub type OnAction = Arc<dyn Fn(Option<Action>) + Send + Sync>;

#[cfg(windows)]
pub use windows::Notifier;

#[cfg(target_os = "linux")]
pub use linux::Notifier;

#[cfg(not(any(windows, target_os = "linux")))]
pub struct Notifier;

#[cfg(not(any(windows, target_os = "linux")))]
impl Notifier {
    pub fn new(_identifier: &str) -> Self {
        Notifier
    }
    pub fn show(&self, _note: Note, _on: OnAction) {}
    pub fn withdraw(&self, _id: &str) {}
}

#[cfg(windows)]
mod windows {
    use super::*;
    use tauri_winrt_notification::{Duration, Scenario, Toast};

    pub struct Notifier {
        app_id: String,
    }

    impl Notifier {
        /// The installer gives the Start menu entry the identifier of the app
        /// as its id; a program started from the build folder has no entry and
        /// borrows the one of PowerShell, so its toasts still show.
        pub fn new(identifier: &str) -> Self {
            let in_build = std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(|d| d.to_path_buf()))
                .is_some_and(|d| d.ends_with("target/release") || d.ends_with("target/debug"));
            Notifier {
                app_id: if in_build {
                    Toast::POWERSHELL_APP_ID.to_owned()
                } else {
                    identifier.to_owned()
                },
            }
        }

        pub fn show(&self, note: Note, on: OnAction) {
            let mut toast = Toast::new(&self.app_id)
                .title(&note.title)
                .text1(&note.body);
            if note.sticky && !note.actions.is_empty() {
                // Stays on screen until it is answered or dismissed.
                toast = toast.scenario(Scenario::Reminder);
            } else {
                toast = toast.duration(Duration::Short);
            }
            for a in &note.actions {
                toast = toast.add_button(&a.label(), a.key());
            }
            let toast = toast.on_activated(move |key| {
                on(key.as_deref().and_then(Action::from_key));
                Ok(())
            });
            if let Err(e) = toast.show() {
                eprintln!("amiscara-deck-desktop: notification: {e}");
            }
        }

        /// Toasts cannot be taken back here. An answer to a request that is
        /// gone does nothing in the daemon.
        pub fn withdraw(&self, _id: &str) {}
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use notify_rust::{Notification, Timeout, Urgency};

    #[derive(Default)]
    pub struct Notifier {
        /// Our note id to the id the notification server gave.
        shown: Arc<Mutex<HashMap<String, u32>>>,
    }

    /// The server shows `<b>` and friends when it supports markup: the text of
    /// an agent must stay text.
    fn escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }

    impl Notifier {
        pub fn new(_identifier: &str) -> Self {
            Notifier::default()
        }

        /// Waits for the answer on a thread of its own: the D-Bus call blocks.
        pub fn show(&self, note: Note, on: OnAction) {
            let shown = Arc::clone(&self.shown);
            std::thread::spawn(move || {
                let caps = notify_rust::get_capabilities().unwrap_or_default();
                let buttons = caps.iter().any(|c| c == "actions");
                let markup = caps.iter().any(|c| c == "body-markup");
                let mut n = Notification::new();
                n.appname("Amiscara Deck")
                    .icon("dialog-question")
                    .summary(&note.title)
                    .body(&if markup {
                        escape(&note.body)
                    } else {
                        note.body.clone()
                    });
                let earlier = shown.lock().ok().and_then(|m| m.get(&note.id).copied());
                if let Some(id) = earlier {
                    n.id(id);
                }
                if note.sticky {
                    n.urgency(Urgency::Critical).timeout(Timeout::Never);
                }
                if buttons {
                    // The click on the notification itself.
                    n.action("default", &deck_i18n::t("Open panel"));
                    for a in &note.actions {
                        n.action(a.key(), &a.label());
                    }
                }
                let handle = match n.show() {
                    Ok(h) => h,
                    Err(e) => {
                        eprintln!("amiscara-deck-desktop: notification: {e}");
                        return;
                    }
                };
                if let Ok(mut m) = shown.lock() {
                    m.insert(note.id.clone(), handle.id());
                }
                if !buttons {
                    return;
                }
                handle.wait_for_action(|key| match key {
                    "__closed" => {}
                    "default" => on(None),
                    other => {
                        if let Some(a) = Action::from_key(other) {
                            on(Some(a));
                        }
                    }
                });
                if let Ok(mut m) = shown.lock() {
                    m.remove(&note.id);
                }
            });
        }

        /// Replaces the notification by an empty one and closes that.
        pub fn withdraw(&self, id: &str) {
            let Some(server_id) = self.shown.lock().ok().and_then(|mut m| m.remove(id)) else {
                return;
            };
            std::thread::spawn(move || {
                if let Ok(h) = Notification::new()
                    .appname("Amiscara Deck")
                    .id(server_id)
                    .show()
                {
                    h.close();
                }
            });
        }
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn markup_of_an_agent_stays_text() {
            assert_eq!(
                super::escape("<b>rm</b> && x"),
                "&lt;b&gt;rm&lt;/b&gt; &amp;&amp; x"
            );
        }
    }
}
