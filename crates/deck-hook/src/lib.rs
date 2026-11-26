//! The program Claude Code and Codex call for every hook event.
//!
//! No logic of its own. It reads the input, adds what only it can see (herdr
//! and tmux pane, terminal, chain of parent processes), hands everything to
//! the daemon, and for PermissionRequest waits for the answer and prints it.
//!
//! **Safety rule:** every error ends silently with exit code 0 and no output:
//! no socket, no daemon, a broken answer, a timeout, even a panic. Then Claude
//! Code shows its normal dialog in the terminal. Deck must never be the reason
//! that an agent hangs or fails.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::Tool;
use deck_proto::message::{HookEnv, HookMessage, HookReply, Parent, decode_line, encode_line};
use serde_json::Value;

/// Hook inputs past this size are not ours to forward.
const MAX_INPUT: u64 = 4 * 1024 * 1024;

/// Events that arrive in bursts and do not need the process chain: the
/// location of an agent is known from the first events.
const BURST_EVENTS: &[&str] = &["PreToolUse", "PostToolUse", "PostToolUseFailure"];

#[derive(Debug, Clone)]
pub struct Options {
    pub socket: PathBuf,
    pub tool: Tool,
    pub connect_timeout: Duration,
    pub write_timeout: Duration,
    /// How long to wait for the answer to a PermissionRequest. The daemon
    /// holds a request for the configured time and then lets go; this is the
    /// outer limit if the daemon does not answer at all.
    pub reply_timeout: Duration,
}

impl Options {
    /// Defaults for a real call. `None` if there is no home folder.
    pub fn from_env(tool: Tool) -> Option<Self> {
        Some(Options {
            socket: deck_proto::paths::default_socket()?,
            tool,
            connect_timeout: Duration::from_millis(500),
            write_timeout: Duration::from_secs(2),
            // Below the 10 minutes Claude Code allows a hook by default.
            reply_timeout: Duration::from_secs(9 * 60 + 30),
        })
    }
}

/// The environment the hook program reports.
pub fn env_from(vars: impl Fn(&str) -> Option<String>, parents: Vec<Parent>) -> HookEnv {
    HookEnv {
        herdr_pane_id: vars("HERDR_PANE_ID").filter(|v| !v.is_empty()),
        tmux_pane: vars("TMUX_PANE").filter(|v| !v.is_empty()),
        term_program: vars("TERM_PROGRAM").filter(|v| !v.is_empty()),
        parents,
    }
}

/// The chain of parent processes, nearest first, from one `ps` call.
/// Empty if `ps` is missing or fails.
pub fn parent_chain(start_pid: u32) -> Vec<Parent> {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,comm="])
        .output()
    else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut table = std::collections::HashMap::new();
    for line in text.lines() {
        let mut it = line.split_whitespace();
        let (Some(pid), Some(ppid)) = (it.next(), it.next()) else {
            continue;
        };
        let name = it.collect::<Vec<_>>().join(" ");
        if let (Ok(pid), Ok(ppid)) = (pid.parse::<u32>(), ppid.parse::<u32>()) {
            table.insert(pid, (ppid, name));
        }
    }
    let mut chain = Vec::new();
    let mut pid = start_pid;
    while chain.len() < 16 {
        let Some((ppid, name)) = table.get(&pid) else {
            break;
        };
        chain.push(Parent {
            pid,
            name: name.clone(),
        });
        if *ppid <= 1 {
            break;
        }
        pid = *ppid;
    }
    chain
}

/// Run once. Returns what to print on standard output, or `None` for nothing.
/// Never fails: every error is `None`.
pub fn run(
    input: impl Read,
    opts: &Options,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    let mut raw = Vec::new();
    input.take(MAX_INPUT + 1).read_to_end(&mut raw).ok()?;
    if raw.len() as u64 > MAX_INPUT {
        return None;
    }
    let value: Value = serde_json::from_slice(&raw).ok()?;
    let event = value
        .get("hook_event_name")
        .and_then(Value::as_str)?
        .to_owned();

    let parents = if BURST_EVENTS.contains(&event.as_str()) {
        Vec::new()
    } else {
        parent_chain(std::process::id())
    };
    let msg = HookMessage {
        protocol: PROTOCOL_VERSION,
        tool: opts.tool,
        env: env_from(env, parents),
        input: value,
    };
    let line = encode_line(&msg).ok()?;

    // Only a PermissionRequest can be answered; everything else is a message
    // and does not wait.
    let reply = exchange(opts, &line, event == "PermissionRequest")?;
    let reply: HookReply = decode_line(&reply).ok()?;
    let output = reply.output?;
    // Only a PermissionRequest answer in the documented shape goes out.
    (output
        .pointer("/hookSpecificOutput/hookEventName")
        .and_then(Value::as_str)
        == Some("PermissionRequest"))
    .then(|| output.to_string())
}

/// Send `line` to the daemon and, if `answer` is set, read its one-line
/// reply. `None` on any error or timeout.
#[cfg(unix)]
fn exchange(opts: &Options, line: &str, answer: bool) -> Option<String> {
    use std::os::unix::net::{SocketAddr, UnixStream};

    let addr = SocketAddr::from_pathname(&opts.socket).ok()?;
    // `UnixStream` has no connect timeout in std. A connect to a Unix socket
    // answers at once (accepted, refused or missing), so a thread bounds the rare hang.
    let mut stream = within(opts.connect_timeout, move || {
        UnixStream::connect_addr(&addr).ok()
    })?;
    stream.set_write_timeout(Some(opts.write_timeout)).ok()?;
    // Set before anything is sent: macOS refuses it once the daemon has
    // answered and hung up, and the answer would be lost.
    stream.set_read_timeout(Some(opts.reply_timeout)).ok()?;
    stream.write_all(line.as_bytes()).ok()?;
    stream.flush().ok()?;
    if !answer {
        return None;
    }
    let mut reply = String::new();
    std::io::BufRead::read_line(&mut std::io::BufReader::new(&stream), &mut reply).ok()?;
    Some(reply)
}

/// Windows: the daemon listens on a named pipe whose random name it wrote
/// into the socket path; the pipe is opened here like a file. A pipe handle
/// has no timeouts, so threads bound every step; when this program ends, a
/// thread still waiting ends with it.
#[cfg(windows)]
fn exchange(opts: &Options, line: &str, answer: bool) -> Option<String> {
    use std::os::windows::fs::OpenOptionsExt;
    /// `SECURITY_IDENTIFICATION`: the server may learn who connects, but not
    /// act as that user (std adds `SECURITY_SQOS_PRESENT`).
    const SECURITY_IDENTIFICATION: u32 = 1 << 16;

    /// All instances of the pipe are taken: the daemon is between accepting
    /// one client and offering the next instance.
    const ERROR_PIPE_BUSY: i32 = 231;

    let name = deck_proto::paths::read_pipe_name(&opts.socket).ok()?;
    let limit = opts.connect_timeout;
    let pipe = within(limit, move || {
        let start = std::time::Instant::now();
        loop {
            match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .security_qos_flags(SECURITY_IDENTIFICATION)
                .open(&name)
            {
                Ok(pipe) => return Some(pipe),
                Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY) && start.elapsed() < limit => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(_) => return None,
            }
        }
    })?;
    let pipe = std::sync::Arc::new(pipe);
    let writer = pipe.clone();
    let line = line.to_owned();
    within(opts.write_timeout, move || {
        let mut w: &std::fs::File = &writer;
        w.write_all(line.as_bytes()).ok()?;
        w.flush().ok()
    })?;
    if !answer {
        return None;
    }
    within(opts.reply_timeout, move || {
        let mut reply = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(&*pipe), &mut reply).ok()?;
        Some(reply)
    })
}

#[cfg(not(any(unix, windows)))]
fn exchange(_: &Options, _: &str, _: bool) -> Option<String> {
    None
}

/// Run `f` in a thread; `None` if it does not finish within `limit`.
fn within<T: Send + 'static>(
    limit: Duration,
    f: impl FnOnce() -> Option<T> + Send + 'static,
) -> Option<T> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    rx.recv_timeout(limit).ok().flatten()
}
