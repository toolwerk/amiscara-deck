//! How agents look on the command line. Pure functions, tested without a daemon.
//! The words are the same as in the app (`po/deck.pot`).

use deck_i18n::{t, t_with};
use deck_proto::agent::{Activity, Agent, Status, Tool};
use deck_proto::text::show_deceptive;

/// Label of a status. A finished agent with background tasks is "paused".
pub fn status_label(a: &Agent) -> String {
    match (a.status, a.paused) {
        (Status::Done, true) => t("paused"),
        (Status::Working, _) => t("working"),
        (Status::Blocked, _) => t("waiting"),
        (Status::Done, false) => t("done"),
        (Status::Idle, _) => t("no task"),
        (Status::Error, _) => t("error"),
        (Status::Exited, _) => t("ended"),
    }
}

/// What an agent does: "editing src/import.rs", "running: cargo test".
pub fn activity_text(a: &Activity) -> String {
    match a {
        Activity::Run { command: Some(c) } => t_with("running: {command}", &[("command", c)]),
        Activity::Run { command: None } => t("running a command"),
        Activity::Edit { path: Some(p) } => t_with("editing {path}", &[("path", p)]),
        Activity::Edit { path: None } => t("editing a file"),
        Activity::Read { path: Some(p) } => t_with("reading {path}", &[("path", p)]),
        Activity::Read { path: None } => t("reading a file"),
        Activity::Search { pattern: Some(p) } => t_with("searching {pattern}", &[("pattern", p)]),
        Activity::Search { pattern: None } => t("searching"),
        Activity::Subagent => t("starting a subagent"),
        Activity::Web => t("fetching from the web"),
        Activity::Compacting => t("compacting"),
        Activity::Tool { name } => name.clone(),
    }
}

/// The name of a section of the full text (`--- 2 old ---`), translated.
fn section(name: &str) -> Option<String> {
    let (number, word) = match name.split_once(' ') {
        Some((n, w)) if n.chars().all(|c| c.is_ascii_digit()) => (Some(n), w),
        _ => (None, name),
    };
    let word = match word {
        "content" => t("content"),
        "old" => t("old"),
        "new" => t("new"),
        "options" => t("options"),
        _ => return None,
    };
    Some(match number {
        Some(n) => format!("{n} {word}"),
        None => word,
    })
}

/// The whole text of a request: section names in the language of the user,
/// the fields the card does not name after it.
pub fn request_text(full: &str, extra: &[String]) -> String {
    let mut lines: Vec<String> = full
        .split('\n')
        .map(|line| {
            line.strip_prefix("--- ")
                .and_then(|l| l.strip_suffix(" ---"))
                .and_then(section)
                .map_or_else(|| line.to_owned(), |s| format!("--- {s} ---"))
        })
        .collect();
    if !extra.is_empty() {
        lines.push(format!("--- {} ---", t("further details")));
        lines.extend(extra.iter().cloned());
    }
    lines.join("\n")
}

/// "Error: ..." for a failure.
pub fn error(e: &dyn std::fmt::Display) -> String {
    t_with("Error: {error}", &[("error", &e.to_string())])
}

/// No daemon answers at this socket.
pub fn no_daemon(socket: &std::path::Path) -> String {
    t_with(
        "No daemon reachable at {socket}. Start it with amiscara-deckd.",
        &[("socket", &socket.display().to_string())],
    )
}

/// Waiting first, then working, error, done, idle, ended (Detailkonzept, section 8).
fn rank(s: Status) -> u8 {
    match s {
        Status::Blocked => 0,
        Status::Working => 1,
        Status::Error => 2,
        Status::Done => 3,
        Status::Idle => 4,
        Status::Exited => 5,
    }
}

pub fn sorted(mut agents: Vec<Agent>) -> Vec<Agent> {
    agents.sort_by(|a, b| {
        (rank(a.status), a.project.as_deref(), a.id.as_str()).cmp(&(
            rank(b.status),
            b.project.as_deref(),
            b.id.as_str(),
        ))
    });
    agents
}

/// "12 s", "3 min", "2 h", "4 d", the same as in the app.
pub fn since(now_ms: u64, then_ms: u64) -> String {
    let s = now_ms.saturating_sub(then_ms) / 1000;
    let n = |v: u64| v.to_string();
    match s {
        0..=59 => t_with("{n} s", &[("n", &n(s))]),
        60..=3599 => t_with("{n} min", &[("n", &n(s / 60))]),
        3600..=86399 => t_with("{n} h", &[("n", &n(s / 3600))]),
        _ => t_with("{n} d", &[("n", &n(s / 86400))]),
    }
}

/// One table cell: on one line, hidden characters (control, bidi, zero-width,
/// look-alike spaces, ...) shown as `⟦U+202E⟧`, a line break as `⏎`, cut to
/// `max` characters with an ellipsis. Nothing in a project name, branch or
/// activity can steer the terminal or pass for something else.
pub fn cell(s: &str, max: usize) -> String {
    let one_line = s.replace('\n', "⏎");
    let clean = show_deceptive(&one_line);
    if clean.chars().count() > max {
        let cut: String = clean.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        clean
    }
}

fn tool(t: Tool) -> &'static str {
    match t {
        Tool::Claude => "claude",
        Tool::Codex => "codex",
    }
}

/// The table `list` prints. Empty input gives the "no agents" line.
pub fn table(agents: &[Agent], now_ms: u64) -> String {
    if agents.is_empty() {
        return format!("{}\n", t("No agents."));
    }
    let head = [
        t("STATUS"),
        t("TOOL"),
        t("PROJECT"),
        t("BRANCH"),
        t("ACTIVITY"),
        t("SINCE"),
    ];
    let rows: Vec<[String; 6]> = sorted(agents.to_vec())
        .iter()
        .map(|a| {
            [
                status_label(a),
                tool(a.tool).to_owned(),
                cell(a.project.as_deref().unwrap_or("-"), 24),
                cell(a.branch.as_deref().unwrap_or("-"), 24),
                match &a.request {
                    Some(r) => cell(
                        &t_with(
                            "asks: {summary} [{id}]",
                            &[("summary", &r.summary), ("id", &r.id)],
                        ),
                        60,
                    ),
                    None => cell(
                        &a.activity
                            .as_ref()
                            .map_or_else(|| "-".to_owned(), activity_text),
                        60,
                    ),
                },
                since(now_ms, a.status_since_ms),
            ]
        })
        .collect();
    let mut width = head.each_ref().map(|h| h.chars().count());
    for r in &rows {
        for (w, c) in width.iter_mut().zip(r) {
            *w = (*w).max(c.chars().count());
        }
    }
    let line = |cols: [&str; 6]| {
        let mut s = String::new();
        for (i, c) in cols.iter().enumerate() {
            s.push_str(c);
            if i + 1 < cols.len() {
                s.push_str(&" ".repeat(width[i] - c.chars().count() + 2));
            }
        }
        s.push('\n');
        s
    };
    let mut out = line([&head[0], &head[1], &head[2], &head[3], &head[4], &head[5]]);
    for r in &rows {
        out.push_str(&line([&r[0], &r[1], &r[2], &r[3], &r[4], &r[5]]));
    }
    out
}

/// One line for `watch`: what changed for whom.
pub fn change_line(before: Option<&Agent>, now: &Agent) -> String {
    let name = cell(now.project.as_deref().unwrap_or(&now.id), 24);
    let label = status_label(now);
    let head = match before {
        None => t_with(
            "{name}: new, {status}",
            &[("name", &name), ("status", &label)],
        ),
        Some(b) if status_label(b) != label => format!("{name}: {} → {label}", status_label(b)),
        Some(_) => format!("{name}: {label}"),
    };
    match (&now.request, &now.activity) {
        (Some(r), _) => format!(
            "{head}  {}",
            t_with("asks: {summary}", &[("summary", &cell(&r.summary, 60))])
        ),
        (None, Some(act)) => format!("{head}  {}", cell(&activity_text(act), 60)),
        (None, None) => head,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deck_i18n::{Language, with_language};
    use deck_proto::agent::Location;

    fn de<R>(f: impl FnOnce() -> R) -> R {
        with_language(Language::De, f)
    }

    fn en<R>(f: impl FnOnce() -> R) -> R {
        with_language(Language::En, f)
    }

    fn agent(id: &str, project: &str, status: Status) -> Agent {
        Agent {
            id: id.into(),
            host: "h".into(),
            tool: Tool::Claude,
            cwd: None,
            project: Some(project.into()),
            branch: Some("main".into()),
            title: None,
            status,
            paused: false,
            status_since_ms: 0,
            activity: None,
            last_prompt: None,
            context: None,
            turns: None,
            subagents: 0,
            location: Location::Unknown,
            request: None,
            history: Vec::new(),
            cost_micro_usd: None,
        }
    }

    #[test]
    fn order_is_waiting_working_error_done_idle_ended() {
        let list = sorted(vec![
            agent("1", "a", Status::Exited),
            agent("2", "b", Status::Idle),
            agent("3", "c", Status::Done),
            agent("4", "d", Status::Error),
            agent("5", "e", Status::Working),
            agent("6", "f", Status::Blocked),
        ]);
        let order: Vec<_> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(order, ["6", "5", "4", "3", "2", "1"]);
    }

    #[test]
    fn same_status_sorts_by_project_then_id() {
        let list = sorted(vec![
            agent("2", "zeta", Status::Working),
            agent("3", "alpha", Status::Working),
            agent("1", "alpha", Status::Working),
        ]);
        let order: Vec<_> = list.iter().map(|a| a.id.as_str()).collect();
        assert_eq!(order, ["1", "3", "2"]);
    }

    #[test]
    fn time_since_is_short_in_both_languages() {
        de(|| {
            assert_eq!(since(10_000, 4_000), "6 s");
            assert_eq!(since(200_000, 20_000), "3 min");
            assert_eq!(since(8_000_000, 0), "2 Std.");
            assert_eq!(since(200_000_000, 0), "2 Tg.");
            assert_eq!(
                since(0, 5_000),
                "0 s",
                "a clock that went back does not underflow"
            );
        });
        en(|| {
            assert_eq!(since(8_000_000, 0), "2 h");
            assert_eq!(since(200_000_000, 0), "2 d");
        });
    }

    #[test]
    fn cells_are_cut_and_cannot_steer_the_terminal() {
        assert_eq!(cell("kurz", 10), "kurz");
        assert_eq!(cell("0123456789abc", 10), "012345678…");
        assert_eq!(cell("a\x1b[31mb\nc", 40), "a⟦U+001B⟧[31mb⏎c");
        assert_eq!(cell("rm\u{202e}gnp.exe", 40), "rm⟦U+202E⟧gnp.exe");
        assert_eq!(cell("a\r\x08b", 40), "a⟦U+000D⟧⟦U+0008⟧b");
        assert_eq!(cell("x\u{a0}y\u{200b}z", 40), "x⟦U+00A0⟧y⟦U+200B⟧z");
        assert_eq!(
            cell("⟦U+202E⟧", 40),
            "⟦⟦U+202E⟧",
            "a typed mark cannot pass for a real one"
        );
        assert_eq!(
            cell("Grüße aus Neuendettelsau", 40),
            "Grüße aus Neuendettelsau"
        );
    }

    #[test]
    fn a_paused_agent_is_called_paused() {
        let mut a = agent("1", "a", Status::Done);
        de(|| assert_eq!(status_label(&a), "fertig"));
        a.paused = true;
        de(|| assert_eq!(status_label(&a), "pausiert"));
        en(|| assert_eq!(status_label(&a), "paused"));
    }

    #[test]
    fn activities_read_in_the_language_of_the_user() {
        let run = Activity::Run {
            command: Some("cargo test".into()),
        };
        de(|| {
            assert_eq!(activity_text(&run), "führt aus: cargo test");
            assert_eq!(
                activity_text(&Activity::Edit { path: None }),
                "bearbeitet eine Datei"
            );
        });
        en(|| {
            assert_eq!(activity_text(&run), "running: cargo test");
            assert_eq!(activity_text(&Activity::Subagent), "starting a subagent");
        });
        assert_eq!(
            de(|| activity_text(&Activity::Tool {
                name: "mcp__x".into()
            })),
            "mcp__x",
            "a tool name stays as it is"
        );
    }

    #[test]
    fn the_full_text_names_its_sections_in_the_language_of_the_user() {
        let full = "a.rs\n--- 1 old ---\nx\n--- 1 new ---\ny\n--- not ours ---";
        let extra = vec!["dangerouslyDisableSandbox=true".to_owned()];
        assert_eq!(
            de(|| request_text(full, &extra)),
            "a.rs\n--- 1 alt ---\nx\n--- 1 neu ---\ny\n--- not ours ---\n--- Weitere Angaben ---\ndangerouslyDisableSandbox=true"
        );
        assert_eq!(
            en(|| request_text(full, &[])),
            full,
            "English is the text itself"
        );
    }

    #[test]
    fn the_table_has_a_head_and_aligned_columns() {
        let mut a = agent("1", "app", Status::Working);
        a.activity = Some(Activity::Edit {
            path: Some("src/import.rs".into()),
        });
        a.status_since_ms = 0;
        let list = [a, agent("2", "längerer-name", Status::Blocked)];
        let english = en(|| table(&list, 90_000));
        assert!(english.starts_with("STATUS") && english.contains("ACTIVITY"));
        assert!(english.contains("editing src/import.rs"), "{english}");
        let t = de(|| table(&list, 90_000));
        let lines: Vec<_> = t.lines().collect();
        assert!(lines[0].starts_with("STATUS") && lines[0].contains("TÄTIGKEIT"));
        assert!(lines[1].starts_with("wartet"), "waiting first: {t}");
        assert!(lines[2].starts_with("arbeitet"), "{t}");
        assert!(lines[2].contains("bearbeitet src/import.rs") && lines[2].contains("1 min"));
        let col = |l: &str| l.find("claude").or(l.find("WERKZEUG"));
        assert_eq!(col(lines[0]), col(lines[1]));
        assert_eq!(col(lines[1]), col(lines[2]));
    }

    #[test]
    fn no_agents_say_so() {
        assert_eq!(de(|| table(&[], 0)), "Keine Agenten.\n");
        assert_eq!(en(|| table(&[], 0)), "No agents.\n");
    }

    #[test]
    fn change_lines_tell_what_happened() {
        let before = agent("1", "app", Status::Working);
        let mut now = agent("1", "app", Status::Blocked);
        de(|| {
            assert_eq!(change_line(Some(&before), &now), "app: arbeitet → wartet");
            assert_eq!(change_line(None, &now), "app: neu, wartet");
        });
        en(|| assert_eq!(change_line(None, &now), "app: new, waiting"));
        now.status = Status::Working;
        now.activity = Some(Activity::Read {
            path: Some("a.rs".into()),
        });
        assert_eq!(
            de(|| change_line(Some(&before), &now)),
            "app: arbeitet  liest a.rs"
        );
    }
}
