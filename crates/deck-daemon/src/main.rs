//! `amiscara-deckd`: the background service of Amiscara Deck.

#[cfg(unix)]
#[tokio::main]
async fn main() -> std::process::ExitCode {
    use deck_daemon::{Config, Daemon};
    use tokio::signal::unix::{SignalKind, signal};

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
    let (Ok(mut term), Ok(mut int)) = (
        signal(SignalKind::terminate()),
        signal(SignalKind::interrupt()),
    ) else {
        eprintln!("amiscara-deckd: cannot listen for signals");
        return std::process::ExitCode::FAILURE;
    };
    daemon
        .run(async {
            tokio::select! { _ = term.recv() => {}, _ = int.recv() => {} }
        })
        .await;
    std::process::ExitCode::SUCCESS
}

#[cfg(not(unix))]
fn main() {
    eprintln!("amiscara-deckd: this platform is not supported yet");
    std::process::exit(1);
}
