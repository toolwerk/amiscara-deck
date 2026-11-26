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

/// The chain of parent processes, nearest first, each by the path of its
/// program where the system tells it (the daemon compares it with the path of
/// the frontmost app). At most 16. Empty when the system does not tell.
pub fn parent_chain(start_pid: u32) -> Vec<Parent> {
    chain(start_pid, process_table(start_pid))
}

/// pid to parent pid and name.
type Table = std::collections::HashMap<u32, (u32, String)>;

fn chain(start_pid: u32, table: Table) -> Vec<Parent> {
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
        if *ppid <= 1 || *ppid == pid {
            break;
        }
        pid = *ppid;
    }
    chain
}

/// macOS and other Unix systems: one `ps` call; `comm` is the full path there.
#[cfg(all(unix, not(target_os = "linux")))]
fn process_table(start_pid: u32) -> Table {
    let _ = start_pid;
    let Ok(out) = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,comm="])
        .output()
    else {
        return Table::new();
    };
    parse_ps(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(any(test, all(unix, not(target_os = "linux"))))]
fn parse_ps(text: &str) -> Table {
    let mut table = Table::new();
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
    table
}

/// Linux: `ps` gives only 15 characters of the name. `/proc` gives the
/// parent and the path of the program; only the processes of the chain are
/// read. A program of another user has no readable path: its short name.
#[cfg(target_os = "linux")]
fn process_table(start_pid: u32) -> Table {
    let mut table = Table::new();
    let mut pid = start_pid;
    while table.len() < 16 {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            break;
        };
        let Some((comm, ppid)) = parse_stat(&stat) else {
            break;
        };
        let name = std::fs::read_link(format!("/proc/{pid}/exe"))
            .ok()
            .and_then(|p| p.to_str().map(str::to_owned))
            .unwrap_or(comm);
        table.insert(pid, (ppid, name));
        if ppid <= 1 {
            break;
        }
        pid = ppid;
    }
    table
}

/// `1234 (bash) S 1200 ...`: the name may hold spaces and brackets, so the
/// fields count from the last `)`.
#[cfg(any(test, target_os = "linux"))]
fn parse_stat(stat: &str) -> Option<(String, u32)> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let comm = stat.get(open + 1..close)?.to_owned();
    let ppid = stat
        .get(close + 1..)?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()?;
    Some((comm, ppid))
}

/// Windows: the process list of the system, then the program paths of the
/// chain only (opening every process would take long).
#[cfg(windows)]
fn process_table(start_pid: u32) -> Table {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let Ok(mut sys) = System::new() else {
        return Table::new();
    };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut pids = Vec::new();
    let mut pid = Pid::from_u32(start_pid);
    while pids.len() < 16 {
        let Some(process) = sys.process(pid) else {
            break;
        };
        pids.push(pid);
        match process.parent() {
            Some(parent) if parent != pid => pid = parent,
            _ => break,
        }
    }
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        false,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    let mut table = Table::new();
    for pid in pids {
        let Some(process) = sys.process(pid) else {
            continue;
        };
        let name = process
            .exe()
            .and_then(|p| p.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| process.name().to_string_lossy().into_owned());
        let ppid = process.parent().map_or(0, |p| p.as_u32());
        table.insert(pid.as_u32(), (ppid, name));
    }
    table
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_in_stat_may_hold_spaces_and_brackets() {
        assert_eq!(
            parse_stat("4711 (my (odd) prog) S 4700 4711 4700 0"),
            Some(("my (odd) prog".to_owned(), 4700))
        );
        assert_eq!(parse_stat("garbage"), None);
    }

    #[test]
    fn ps_lines_keep_paths_with_spaces() {
        let t =
            parse_ps("  10     1 /Applications/My Term.app/Contents/MacOS/term\n  20    10 -zsh\n");
        assert_eq!(
            t.get(&10).map(|(p, n)| (*p, n.as_str())),
            Some((1, "/Applications/My Term.app/Contents/MacOS/term"))
        );
        assert_eq!(t.get(&20).map(|(p, _)| *p), Some(10));
    }

    #[test]
    fn the_chain_goes_up_to_init_and_stops_at_a_loop() {
        let mut t = Table::new();
        t.insert(30, (20, "claude".into()));
        t.insert(20, (10, "zsh".into()));
        t.insert(10, (1, "term".into()));
        let names: Vec<String> = chain(30, t).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["claude", "zsh", "term"]);
        let mut t = Table::new();
        t.insert(5, (5, "self".into()));
        assert_eq!(chain(5, t).len(), 1);
    }
}
