//! `amiscara-deckd`: the background service of Amiscara Deck.

#[tokio::main]
async fn main() -> std::process::ExitCode {
    use deck_daemon::{Config, Daemon};

    let mut config = match Config::from_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("amiscara-deckd: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match (a.as_str(), args.next()) {
            ("--data-dir", Some(p)) => config.data_dir = p.into(),
            _ => {
                eprintln!("usage: amiscara-deckd [--data-dir <folder>]");
                return std::process::ExitCode::from(2);
            }
        }
    }
    let daemon = match Daemon::bind(config).await {
        Ok(d) => d,
        Err(e) => {
            eprintln!("amiscara-deckd: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    eprintln!(
        "amiscara-deckd: listening on {}",
        daemon.socket_path().display()
    );
    let Some(stop) = stop_signal() else {
        eprintln!("amiscara-deckd: cannot listen for signals");
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

/// Ctrl+C, closing the console, logging off or shutting down.
#[cfg(windows)]
fn stop_signal() -> Option<impl Future<Output = ()>> {
    use tokio::signal::windows;
    let mut c = windows::ctrl_c().ok()?;
    let mut close = windows::ctrl_close().ok()?;
    let mut logoff = windows::ctrl_logoff().ok()?;
    let mut shutdown = windows::ctrl_shutdown().ok()?;
    Some(async move {
        tokio::select! {
            _ = c.recv() => {},
            _ = close.recv() => {},
            _ = logoff.recv() => {},
            _ = shutdown.recv() => {},
        }
    })
}
