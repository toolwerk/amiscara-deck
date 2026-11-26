//! Where the user is: frontmost program, time without input, locked screen.
//! The interface measures, the daemon decides (E10), as on the Mac.
//!
//! What each system tells:
//! - Windows: all three. The lock screen counts as locked.
//! - Linux with X11, KDE or Hyprland: all three. GNOME with Wayland does not
//!   tell the frontmost program: it stays unknown, and "Automatic" then
//!   notifies, rather once too often.
//! - Lock on Linux from logind (`LockedHint`), otherwise from the screen saver.

use std::time::Duration;

use deck_proto::message::Presence;

/// Even without a change the daemon hears from us this often; it stops
/// believing a report after 45 seconds.
pub const HEARTBEAT: Duration = Duration::from_secs(15);
/// How often the system is asked.
pub const TICK: Duration = Duration::from_secs(1);

/// One look at the system. `None`: the system did not tell.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reading {
    pub frontmost: Option<String>,
    pub idle_ms: Option<u64>,
    pub locked: bool,
}

/// The report for the daemon. Unknown idle time counts as "just now": the
/// other signals then decide.
pub fn report(r: Reading, now_ms: u64) -> Presence {
    let idle = r.idle_ms.unwrap_or(0);
    Presence {
        screen_locked: r.locked,
        idle_secs: idle / 1000,
        frontmost_app: r.frontmost.filter(|f| !f.is_empty()),
        focus_mode: false,
        last_input_ms: now_ms.saturating_sub(idle),
    }
}

/// Worth sending? Idle time alone changes every second: it counts only when it
/// crosses "just now" (under 5 s); the daemon works out the rest from
/// `last_input_ms`.
pub fn differs(a: &Presence, b: Option<&Presence>) -> bool {
    let Some(b) = b else { return true };
    a.screen_locked != b.screen_locked
        || a.frontmost_app != b.frontmost_app
        || a.focus_mode != b.focus_mode
        || (a.idle_secs < 5) != (b.idle_secs < 5)
}

pub use imp::Sources;

#[cfg(windows)]
mod imp {
    use super::Reading;

    pub struct Sources;

    impl Sources {
        pub async fn new() -> Self {
            Sources
        }

        pub async fn read(&mut self) -> Reading {
            let path = active_win_pos_rs::get_active_window()
                .ok()
                .and_then(|w| w.process_path.to_str().map(str::to_owned))
                .filter(|p| !p.is_empty());
            // The lock screen is a program of its own.
            let locked = path.as_deref().is_some_and(|p| {
                p.rsplit('\\')
                    .next()
                    .is_some_and(|f| f.eq_ignore_ascii_case("LockApp.exe"))
            });
            let idle_ms = user_idle::UserIdle::get_time()
                .ok()
                .map(|t| u64::try_from(t.as_milliseconds()).unwrap_or(u64::MAX));
            Reading {
                frontmost: if locked { None } else { path },
                idle_ms,
                locked,
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use super::Reading;
    use x11rb::connection::Connection as _;
    use x11rb::protocol::screensaver::ConnectionExt as _;

    pub struct Sources {
        session: Option<zbus::Connection>,
        system: Option<zbus::Connection>,
        x11: Option<(x11rb::rust_connection::RustConnection, u32)>,
        /// The desktop can tell the frontmost window.
        frontmost: bool,
    }

    fn var(name: &str) -> String {
        std::env::var(name).unwrap_or_default()
    }

    impl Sources {
        pub async fn new() -> Self {
            let wayland =
                var("XDG_SESSION_TYPE") == "wayland" || !var("WAYLAND_DISPLAY").is_empty();
            let kde = var("XDG_CURRENT_DESKTOP").to_uppercase().contains("KDE");
            let hyprland = !var("HYPRLAND_INSTANCE_SIGNATURE").is_empty();
            // Under Wayland the X11 server only knows its own windows: what
            // it calls active may be long in the background.
            let x11 = if wayland {
                None
            } else {
                x11rb::connect(None).ok().and_then(|(conn, screen)| {
                    let root = conn.setup().roots.get(screen)?.root;
                    Some((conn, root))
                })
            };
            Sources {
                session: zbus::Connection::session().await.ok(),
                system: zbus::Connection::system().await.ok(),
                frontmost: x11.is_some() || kde || hyprland,
                x11,
            }
        }

        pub async fn read(&mut self) -> Reading {
            let frontmost = if self.frontmost {
                tokio::task::spawn_blocking(|| {
                    active_win_pos_rs::get_active_window()
                        .ok()
                        .and_then(|w| w.process_path.to_str().map(str::to_owned))
                })
                .await
                .ok()
                .flatten()
                .filter(|p| !p.is_empty())
            } else {
                None
            };
            Reading {
                frontmost,
                idle_ms: self.idle_ms().await,
                locked: self.locked().await,
            }
        }

        async fn idle_ms(&self) -> Option<u64> {
            if let Some(session) = &self.session {
                // GNOME, also with Wayland.
                if let Ok(m) = session
                    .call_method(
                        Some("org.gnome.Mutter.IdleMonitor"),
                        "/org/gnome/Mutter/IdleMonitor/Core",
                        Some("org.gnome.Mutter.IdleMonitor"),
                        "GetIdletime",
                        &(),
                    )
                    .await
                    && let Ok(ms) = m.body().deserialize::<u64>()
                {
                    return Some(ms);
                }
            }
            if let Some((conn, root)) = &self.x11
                && let Ok(cookie) = conn.screensaver_query_info(*root)
                && let Ok(info) = cookie.reply()
            {
                return Some(u64::from(info.ms_since_user_input));
            }
            if let Some(session) = &self.session {
                // KDE answers in milliseconds.
                if let Ok(m) = session
                    .call_method(
                        Some("org.freedesktop.ScreenSaver"),
                        "/org/freedesktop/ScreenSaver",
                        Some("org.freedesktop.ScreenSaver"),
                        "GetSessionIdleTime",
                        &(),
                    )
                    .await
                    && let Ok(ms) = m.body().deserialize::<u32>()
                {
                    return Some(u64::from(ms));
                }
            }
            None
        }

        async fn locked(&self) -> bool {
            if let Some(system) = &self.system
                && let Ok(proxy) = zbus::Proxy::new(
                    system,
                    "org.freedesktop.login1",
                    "/org/freedesktop/login1/session/auto",
                    "org.freedesktop.login1.Session",
                )
                .await
                && let Ok(locked) = proxy.get_property::<bool>("LockedHint").await
            {
                return locked;
            }
            if let Some(session) = &self.session
                && let Ok(m) = session
                    .call_method(
                        Some("org.freedesktop.ScreenSaver"),
                        "/org/freedesktop/ScreenSaver",
                        Some("org.freedesktop.ScreenSaver"),
                        "GetActive",
                        &(),
                    )
                    .await
                && let Ok(active) = m.body().deserialize::<bool>()
            {
                return active;
            }
            false
        }
    }
}

/// The Mac has its own app; here the program only runs for trying it out.
#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::Reading;

    pub struct Sources;

    impl Sources {
        pub async fn new() -> Self {
            Sources
        }

        pub async fn read(&mut self) -> Reading {
            Reading::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(app: Option<&str>, idle: u64, locked: bool) -> Presence {
        report(
            Reading {
                frontmost: app.map(str::to_owned),
                idle_ms: Some(idle),
                locked,
            },
            1_000_000,
        )
    }

    #[test]
    fn the_report_carries_the_time_of_the_last_input() {
        let r = p(Some("/usr/bin/foot"), 7_500, false);
        assert_eq!(r.idle_secs, 7);
        assert_eq!(r.last_input_ms, 992_500);
        assert_eq!(r.frontmost_app.as_deref(), Some("/usr/bin/foot"));
        let unknown = report(Reading::default(), 1_000_000);
        assert_eq!(unknown.last_input_ms, 1_000_000);
        assert_eq!(unknown.frontmost_app, None);
    }

    #[test]
    fn only_changes_that_matter_are_sent() {
        let a = p(Some("/a"), 1_000, false);
        assert!(differs(&a, None));
        assert!(!differs(&p(Some("/a"), 3_000, false), Some(&a)));
        assert!(differs(&p(Some("/a"), 6_000, false), Some(&a)));
        assert!(differs(&p(Some("/b"), 1_000, false), Some(&a)));
        assert!(differs(&p(Some("/a"), 1_000, true), Some(&a)));
        assert!(!differs(
            &p(Some("/a"), 60_000, false),
            Some(&p(Some("/a"), 9_000, false))
        ));
    }
}
