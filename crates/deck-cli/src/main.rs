fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = deck_cli::run(&args, &mut std::io::stdout(), &mut std::io::stderr());
    std::process::ExitCode::from(code)
}
