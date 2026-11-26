//! `amiscara-deckd`: the background service of Amiscara Deck.
//!
//! On Windows it is built without a console, so starting it at login opens
//! no window; its messages go to `deckd.log` in the data folder, as the
//! LaunchAgent does it on macOS.

#![cfg_attr(windows, windows_subsystem = "windows")]

/// Print a message; on Windows, where there is no console, append it to
/// `deckd.log` in `data_dir` as well.
fn say(data_dir: Option<&std::path::Path>, msg: &str) {
    eprintln!("amiscara-deckd: {msg}");
    #[cfg(windows)]
    if let Some(dir) = data_dir {
        use std::io::Write;
        let _ = deck_proto::fsperm::create_dir(dir, deck_proto::fsperm::PRIVATE_DIR);
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("deckd.log"))
        {
            let _ = writeln!(f, "amiscara-deckd: {msg}");
        }
    }
    #[cfg(not(windows))]
    let _ = data_dir;
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    use deck_daemon::{Config, Daemon};

    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            say(None, &e.to_string());
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--data-dir", Some(p)) => config.data_dir = p.into(),
            _ => {
                say(None, "usage: amiscara-deckd [--data-dir <folder>]");
                return std::process::ExitCode::from(2);
            }
        }
    }
    let data_dir = config.data_dir.clone();
    let daemon = match Daemon::bind(config).await {
        Ok(d) => d,
        Err(e) => {
            say(Some(&data_dir), &e.to_string());
            return std::process::ExitCode::FAILURE;
        }
    };
    say(
        Some(&data_dir),
        &format!("listening on {}", daemon.socket_path().display()),
    );
    let Some(stop) = stop_signal() else {
        say(Some(&data_dir), "cannot listen for signals");
        return std::process::ExitCode::FAILURE;
    };
    daemon.run(stop).await;
    std::process::ExitCode::SUCCESS
}

/// SIGTERM or SIGINT.
#[cfg(unix)]
fn stop_signal() -> Option<impl Future<Output = ()>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut term = signal(SignalKind::terminate()).ok()?;
    let mut int = signal(SignalKind::interrupt()).ok()?;
    Some(async move {
        tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
    })
}

/// Ctrl+C, closing the console, logging off or shutting down. Without a
/// console some of these cannot be registered; the others still count, and
/// an `amiscara-deck daemon uninstall` ends the daemon over the pipe.
#[cfg(windows)]
fn stop_signal() -> Option<impl Future<Output = ()>> {
    use tokio::signal::windows;
    let mut c = windows::ctrl_c().ok();
    let mut close = windows::ctrl_close().ok();
    let mut logoff = windows::ctrl_logoff().ok();
    let mut shutdown = windows::ctrl_shutdown().ok();
    Some(async move {
        tokio::select! {
            Some(_) = async { c.as_mut()?.recv().await } => {},
            Some(_) = async { close.as_mut()?.recv().await } => {},
            Some(_) = async { logoff.as_mut()?.recv().await } => {},
            Some(_) = async { shutdown.as_mut()?.recv().await } => {},
            else => std::future::pending::<()>().await,
        }
    })
}
