#![cfg(unix)]
//! Every failure ends the same way: nothing printed, exit code 0, so Claude Code
//! shows its normal dialog. Deck must never be the reason an agent hangs.

#![cfg(unix)]

use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use deck_hook::{Options, run};
use deck_proto::agent::Tool;
use serde_json::json;

fn opts(socket: PathBuf, reply_timeout: Duration) -> Options {
    Options {
        socket,
        tool: Tool::Claude,
        connect_timeout: Duration::from_millis(300),
        write_timeout: Duration::from_millis(500),
        reply_timeout,
    }
}

fn permission_request() -> Cursor<Vec<u8>> {
    let v = json!({"session_id": "s1", "hook_event_name": "PermissionRequest", "cwd": "/work/app",
                   "tool_name": "Bash", "tool_input": {"command": "ls"}});
    Cursor::new(serde_json::to_vec(&v).expect("json"))
}

fn no_env(_: &str) -> Option<String> {
    None
}

/// A daemon that takes one connection, reads the line and does `act` with the stream.
fn daemon_that(
    dir: &tempfile::TempDir,
    act: impl FnOnce(std::os::unix::net::UnixStream) + Send + 'static,
) -> PathBuf {
    let socket = dir.path().join("d.sock");
    let listener = UnixListener::bind(&socket).expect("bind");
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).expect("read");
        act(stream);
    });
    socket
}

#[test]
fn without_a_socket_nothing_is_printed_and_it_is_quick() {
    let dir = tempfile::tempdir().expect("tempdir");
    let start = Instant::now();
    let out = run(
        permission_request(),
        &opts(dir.path().join("none.sock"), Duration::from_secs(30)),
        &no_env,
    );
    assert_eq!(out, None);
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "{:?}",
        start.elapsed()
    );
}

#[test]
fn a_socket_file_nobody_listens_on_is_refused_quickly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = dir.path().join("stale.sock");
    drop(UnixListener::bind(&socket).expect("bind")); // the file stays, nobody listens
    assert!(socket.exists());
    let start = Instant::now();
    assert_eq!(
        run(
            permission_request(),
            &opts(socket, Duration::from_secs(30)),
            &no_env
        ),
        None
    );
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn a_connection_that_is_cut_before_the_answer_prints_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = daemon_that(&dir, drop); // closes at once
    let start = Instant::now();
    assert_eq!(
        run(
            permission_request(),
            &opts(socket, Duration::from_secs(30)),
            &no_env
        ),
        None
    );
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "does not wait for the timeout"
    );
}

#[test]
fn a_connection_that_is_cut_in_the_middle_of_the_answer_prints_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let socket = daemon_that(&dir, |mut s| {
        let _ = s.write_all(b"{\"output\":{\"hookSpecificOutput\":{\"hookEventN");
    });
    assert_eq!(
        run(
            permission_request(),
            &opts(socket, Duration::from_secs(30)),
            &no_env
        ),
        None
    );
}

#[test]
fn when_the_time_runs_out_nothing_is_printed() {
    let dir = tempfile::tempdir().expect("tempdir");
    // The daemon takes the request and never answers (holds the stream open).
    let socket = daemon_that(&dir, |s| {
        std::thread::sleep(Duration::from_secs(3));
        drop(s);
    });
    let start = Instant::now();
    let out = run(
        permission_request(),
        &opts(socket, Duration::from_millis(400)),
        &no_env,
    );
    assert_eq!(out, None);
    let took = start.elapsed();
    assert!(
        took >= Duration::from_millis(350) && took < Duration::from_secs(2),
        "{took:?}"
    );
}

#[test]
fn answers_that_are_not_valid_print_nothing() {
    for (name, reply) in [
        ("not json", "this is not json\n"),
        ("json without output", "{}\n"),
        ("output is a string", "{\"output\":\"allow\"}\n"),
        (
            "output without event name",
            "{\"output\":{\"hookSpecificOutput\":{}}}\n",
        ),
        ("empty line", "\n"),
        ("answer without newline", "{\"output\":null}"),
    ] {
        let dir = tempfile::tempdir().expect("tempdir");
        let reply = reply.to_owned();
        let socket = daemon_that(&dir, move |mut s| {
            let _ = s.write_all(reply.as_bytes());
        });
        assert_eq!(
            run(
                permission_request(),
                &opts(socket, Duration::from_secs(5)),
                &no_env
            ),
            None,
            "{name}"
        );
    }
}

#[test]
fn input_that_is_too_big_is_not_forwarded() {
    let dir = tempfile::tempdir().expect("tempdir");
    let o = opts(dir.path().join("none.sock"), Duration::from_secs(1));
    let big = format!(
        "{{\"hook_event_name\":\"Stop\",\"x\":\"{}\"}}",
        "a".repeat(5 * 1024 * 1024)
    );
    assert_eq!(run(Cursor::new(big.into_bytes()), &o, &no_env), None);
}

// The real program, as Claude Code starts it.

fn run_binary(
    stdin: &[u8],
    envs: &[(&str, &str)],
    data_dir: Option<&std::path::Path>,
) -> (Option<i32>, String, String) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_deck-hook"));
    cmd.env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = data_dir {
        cmd.env("DECK_DATA_DIR", d);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(stdin)
        .expect("write stdin");
    let mut out = String::new();
    let mut err = String::new();
    child
        .stdout
        .take()
        .expect("stdout")
        .read_to_string(&mut out)
        .expect("out");
    child
        .stderr
        .take()
        .expect("stderr")
        .read_to_string(&mut err)
        .expect("err");
    let status = child.wait().expect("wait");
    (status.code(), out, err)
}

fn permission_bytes() -> Vec<u8> {
    permission_request().into_inner()
}

#[test]
fn the_binary_ends_silently_with_code_0_when_no_daemon_runs() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (code, out, err) = run_binary(&permission_bytes(), &[], Some(dir.path()));
    assert_eq!((code, out.as_str(), err.as_str()), (Some(0), "", ""));
}

#[test]
fn the_binary_ends_silently_with_code_0_without_a_home_folder() {
    let (code, out, err) = run_binary(&permission_bytes(), &[], None);
    assert_eq!((code, out.as_str(), err.as_str()), (Some(0), "", ""));
}

#[test]
fn the_binary_ends_silently_with_code_0_on_garbage_and_empty_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    for bad in [
        &b""[..],
        b"not json",
        b"\xff\xfe\x00",
        b"{\"hook_event_name\":1}",
    ] {
        let (code, out, err) = run_binary(bad, &[], Some(dir.path()));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (Some(0), "", ""),
            "{bad:?}"
        );
    }
}

#[test]
fn the_binary_ends_silently_when_the_daemon_hangs_up() {
    let dir = tempfile::tempdir().expect("tempdir");
    let listener =
        UnixListener::bind(dir.path().join(deck_proto::paths::SOCKET_NAME)).expect("bind");
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).expect("read");
        // hang up without an answer
    });
    let start = Instant::now();
    let (code, out, err) = run_binary(&permission_bytes(), &[], Some(dir.path()));
    assert_eq!((code, out.as_str(), err.as_str()), (Some(0), "", ""));
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn the_binary_prints_the_answer_of_the_daemon() {
    let dir = tempfile::tempdir().expect("tempdir");
    let listener =
        UnixListener::bind(dir.path().join(deck_proto::paths::SOCKET_NAME)).expect("bind");
    let answer = json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "deny", "message": "no"}}});
    let reply = format!("{}\n", json!({"output": answer}));
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).expect("read");
        stream.write_all(reply.as_bytes()).expect("reply");
    });
    let (code, out, err) = run_binary(&permission_bytes(), &[], Some(dir.path()));
    assert_eq!((code, err.as_str()), (Some(0), ""));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&out).expect("JSON"),
        answer
    );
}
