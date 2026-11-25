//! How agents look on the command line. Pure functions, tested without a daemon.

use deck_proto::agent::{Agent, Status, Tool};
use deck_proto::text::show_deceptive;

/// German label of a status. A finished agent with background tasks is "pausiert".
pub fn status_label(a: &Agent) -> &'static str {
    match (a.status, a.paused) {
        (Status::Done, true) => "pausiert",
        (Status::Working, _) => "arbeitet",
        (Status::Blocked, _) => "wartet",
        (Status::Done, false) => "fertig",
        (Status::Idle, _) => "ohne Auftrag",
        (Status::Error, _) => "Fehler",
        (Status::Exited, _) => "beendet",
    }
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

/// "12 s", "3 Min.", "2 Std.", "4 Tg."
pub fn since(now_ms: u64, then_ms: u64) -> String {
    let s = now_ms.saturating_sub(then_ms) / 1000;
    match s {
        0..=59 => format!("{s} s"),
        60..=3599 => format!("{} Min.", s / 60),
        3600..=86399 => format!("{} Std.", s / 3600),
        _ => format!("{} Tg.", s / 86400),
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
        return "Keine Agenten.\n".to_owned();
    }
    let head = [
        "STATUS",
        "WERKZEUG",
        "PROJEKT",
        "BRANCH",
        "TÄTIGKEIT",
        "SEIT",
    ];
    let rows: Vec<[String; 6]> = sorted(agents.to_vec())
        .iter()
        .map(|a| {
            [
                status_label(a).to_owned(),
                tool(a.tool).to_owned(),
                cell(a.project.as_deref().unwrap_or("-"), 24),
                cell(a.branch.as_deref().unwrap_or("-"), 24),
                match &a.request {
                    Some(r) => cell(&format!("fragt: {} [{}]", r.summary, r.id), 60),
                    None => cell(a.activity.as_deref().unwrap_or("-"), 60),
                },
                since(now_ms, a.status_since_ms),
            ]
        })
        .collect();
    let mut width = head.map(|h| h.chars().count());
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
    let mut out = line(head);
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
        None => format!("{name}: neu, {label}"),
        Some(b) if status_label(b) != label => format!("{name}: {} → {label}", status_label(b)),
        Some(_) => format!("{name}: {label}"),
    };
    match (&now.request, &now.activity) {
        (Some(r), _) => format!("{head}  fragt: {}", cell(&r.summary, 60)),
        (None, Some(act)) => format!("{head}  {}", cell(act, 60)),
        (None, None) => head,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use deck_proto::agent::Location;

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
    fn time_since_is_short_and_german() {
        assert_eq!(since(10_000, 4_000), "6 s");
        assert_eq!(since(200_000, 20_000), "3 Min.");
        assert_eq!(since(8_000_000, 0), "2 Std.");
        assert_eq!(since(200_000_000, 0), "2 Tg.");
        assert_eq!(
            since(0, 5_000),
            "0 s",
            "a clock that went back does not underflow"
        );
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
        assert_eq!(status_label(&a), "fertig");
        a.paused = true;
        assert_eq!(status_label(&a), "pausiert");
    }

    #[test]
    fn the_table_has_a_head_and_aligned_columns() {
        let mut a = agent("1", "app", Status::Working);
        a.activity = Some("bearbeitet src/import.rs".into());
        a.status_since_ms = 0;
        let t = table(&[a, agent("2", "längerer-name", Status::Blocked)], 90_000);
        let lines: Vec<_> = t.lines().collect();
        assert!(lines[0].starts_with("STATUS") && lines[0].contains("TÄTIGKEIT"));
        assert!(lines[1].starts_with("wartet"), "waiting first: {t}");
        assert!(lines[2].starts_with("arbeitet"), "{t}");
        assert!(lines[2].contains("bearbeitet src/import.rs") && lines[2].contains("1 Min."));
        let col = |l: &str| l.find("claude").or(l.find("WERKZEUG"));
        assert_eq!(col(lines[0]), col(lines[1]));
        assert_eq!(col(lines[1]), col(lines[2]));
    }

    #[test]
    fn no_agents_say_so() {
        assert_eq!(table(&[], 0), "Keine Agenten.\n");
    }

    #[test]
    fn change_lines_tell_what_happened() {
        let before = agent("1", "app", Status::Working);
        let mut now = agent("1", "app", Status::Blocked);
        assert_eq!(change_line(Some(&before), &now), "app: arbeitet → wartet");
        assert_eq!(change_line(None, &now), "app: neu, wartet");
        now.status = Status::Working;
        now.activity = Some("liest a.rs".into());
        assert_eq!(
            change_line(Some(&before), &now),
            "app: arbeitet  liest a.rs"
        );
    }
}
