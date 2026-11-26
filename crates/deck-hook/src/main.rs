//! `deck-hook`: see the library for the rule that every error ends silently.

fn main() {
    // A panic must not print anything: stay silent, exit 0.
    std::panic::set_hook(Box::new(|_| {}));
    let _ = std::panic::catch_unwind(|| {
        let tool = if std::env::args().any(|a| a == "codex" || a == "--codex") {
            deck_proto::agent::Tool::Codex
        } else {
            deck_proto::agent::Tool::Claude
        };
        let Some(opts) = deck_hook::Options::from_env(tool) else {
            drain();
            return;
        };
        let env = |k: &str| std::env::var(k).ok();
        if let Some(out) = deck_hook::run(std::io::stdin().lock(), &opts, &env) {
            use std::io::Write;
            let mut stdout = std::io::stdout().lock();
            let _ = stdout.write_all(out.as_bytes());
            let _ = stdout.write_all(b"\n");
            let _ = stdout.flush();
        }
        drain();
    });
}

/// Read what is left of the input: the caller must never write into a closed
/// pipe, whatever made this program do nothing.
fn drain() {
    let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
}
