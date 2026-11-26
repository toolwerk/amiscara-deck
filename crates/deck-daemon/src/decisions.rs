//! The log of decisions: one JSON line per answer made in Deck.
//!
//! It holds commands, so the file is private (0600) from its first byte.
//! Lines older than the retention are dropped; the user can clear it.

use deck_proto::fsperm;
use std::io::Write;
use std::path::Path;

use deck_proto::message::AnswerVia;
use deck_session::{AnswerKind, Answered};
use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "decisions.jsonl";
const DAY_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Serialize, Deserialize)]
struct Line {
    at_ms: u64,
    agent: String,
    project: Option<String>,
    tool_name: String,
    request_id: String,
    /// The whole text, escaped like on the card.
    command: String,
    decision: String,
    rules: Vec<String>,
    /// "Question = choice" for an answered question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    answers: Vec<String>,
    via: AnswerVia,
}

fn open_private(path: &Path, append: bool) -> std::io::Result<std::fs::File> {
    let mut o = std::fs::OpenOptions::new();
    fsperm::create_mode(o.write(true).create(true), fsperm::PRIVATE_FILE);
    if append {
        o.append(true);
    } else {
        o.truncate(true);
    }
    let f = o.open(path)?;
    fsperm::set_mode(&f, fsperm::PRIVATE_FILE)?;
    Ok(f)
}

/// Add one decision. Errors are not fatal for the answer, the caller ignores them.
pub fn append(path: &Path, a: &Answered, via: AnswerVia) -> std::io::Result<()> {
    let line = Line {
        at_ms: a.at_ms,
        agent: a.agent_id.clone(),
        project: a.project.clone(),
        tool_name: a.tool_name.clone(),
        request_id: a.request_id.clone(),
        command: a.full.clone(),
        decision: match a.kind {
            AnswerKind::Allow => "allow",
            AnswerKind::AllowAlways => "allow_always",
            AnswerKind::Deny => "deny",
            AnswerKind::Answer => "answer",
        }
        .to_owned(),
        rules: a.rules.clone(),
        answers: a.answers.clone(),
        via,
    };
    let mut json = serde_json::to_vec(&line).map_err(std::io::Error::other)?;
    json.push(b'\n');
    open_private(path, true)?.write_all(&json)
}

/// Drop lines older than `retention_days`. Unreadable lines are dropped too.
pub fn prune(path: &Path, now_ms: u64, retention_days: u32) -> std::io::Result<()> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    let limit = now_ms.saturating_sub(u64::from(retention_days) * DAY_MS);
    let keep: Vec<&str> = text
        .lines()
        .filter(|l| {
            serde_json::from_str::<serde_json::Value>(l)
                .ok()
                .and_then(|v| v.get("at_ms").and_then(serde_json::Value::as_u64))
                .is_some_and(|t| t >= limit)
        })
        .collect();
    if keep.len() == text.lines().count() {
        return Ok(());
    }
    let mut out = keep.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    let tmp = path.with_extension("jsonl.tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut f = fsperm::create_mode(
        std::fs::OpenOptions::new().write(true).create_new(true),
        fsperm::PRIVATE_FILE,
    )
    .open(&tmp)?;
    fsperm::set_mode(&f, fsperm::PRIVATE_FILE)?;
    f.write_all(out.as_bytes())?;
    f.sync_all()?;
    std::fs::rename(&tmp, path)
}

/// Remove every line.
pub fn clear(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        open_private(path, false)?;
    }
    Ok(())
}
