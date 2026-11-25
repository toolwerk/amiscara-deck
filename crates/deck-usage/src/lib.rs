//! Context fill and costs from the transcripts of Claude Code.
//!
//! The fill is the sum of the three input fields of the last message of the
//! main agent (M0-06). The window is not in the transcript or in any hook, so
//! it comes from a table by model. Every read error gives `None`: the app then
//! shows an empty field, never a wrong number.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use deck_proto::agent::ContextUsage;
use serde_json::Value;

/// How much of the end of the file is read. A line is some KiB, so this holds
/// many messages and the last one with usage data.
const TAIL_BYTES: u64 = 512 * 1024;

/// Window when the model is unknown: the smaller, so a fill is not shown too low.
pub const DEFAULT_WINDOW: u64 = 200_000;

/// The user's own windows: part of a model name and its tokens. They go before
/// the built in table, for models Deck does not know yet.
pub type Windows = Vec<(String, u64)>;

/// The file in the data folder of the daemon with the user's own windows.
pub const WINDOWS_FILE: &str = "context-windows.json";

/// Read `{"model-part": tokens, ...}`. A missing or broken file gives none.
pub fn load_windows(path: &Path) -> Windows {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
        return Vec::new();
    };
    map.into_iter()
        .filter_map(|(k, v)| {
            let tokens = v.as_u64().filter(|t| *t > 0)?;
            (!k.is_empty()).then(|| (k.to_ascii_lowercase(), tokens))
        })
        .collect()
}

/// The context window of a model, in tokens: the user's table first, then the
/// built in one, kept here in one place (M6-01).
pub fn window_for_with(model: &str, own: &[(String, u64)]) -> u64 {
    let m = model.to_ascii_lowercase();
    // The longest matching part wins.
    if let Some((_, tokens)) = own
        .iter()
        .filter(|(part, _)| m.contains(part.as_str()))
        .max_by_key(|(part, _)| part.len())
    {
        return *tokens;
    }
    window_for(model)
}

/// The built in table.
pub fn window_for(model: &str) -> u64 {
    let m = model.to_ascii_lowercase();
    if m.contains("[1m]") {
        return 1_000_000;
    }
    // Haiku 5.5, Sonnet 5.5 and Opus 5.5 report 1,000,000 (M0-06).
    if ["haiku-5", "sonnet-5", "opus-5", "fable-5"]
        .iter()
        .any(|f| m.contains(f))
    {
        return 1_000_000;
    }
    DEFAULT_WINDOW
}

/// The context fill of a transcript, or `None` if it cannot be read.
pub fn context_usage(path: &Path) -> Option<ContextUsage> {
    context_usage_with(path, &[])
}

/// Like [`context_usage`], with the user's own windows.
pub fn context_usage_with(path: &Path, own: &[(String, u64)]) -> Option<ContextUsage> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.take(TAIL_BYTES).read_to_end(&mut bytes).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if start > 0 {
        // The first line is probably cut.
        lines.next();
    }
    let lines: Vec<&str> = lines.collect();
    lines.into_iter().rev().find_map(|l| parse_line(l, own))
}

/// The running cost of a session, as Claude Code estimates it (E15).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CostState {
    /// Total of the session so far, in millionths of a US dollar.
    pub total_micro_usd: u64,
    /// When the session began, Unix time in milliseconds, if told.
    pub started_ms: Option<u64>,
}

/// How far back a `cost-state` line is searched: it is written now and then,
/// not after every message.
const COST_SEARCH_BYTES: u64 = 16 * 1024 * 1024;

/// The last `cost-state` line of a transcript, or `None`.
pub fn cost_state(path: &Path) -> Option<CostState> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let floor = len.saturating_sub(COST_SEARCH_BYTES);
    let mut end = len;
    // The start of a line cut by the previous (later) block.
    let mut rest: Vec<u8> = Vec::new();
    while end > floor {
        let start = end.saturating_sub(TAIL_BYTES).max(floor);
        file.seek(SeekFrom::Start(start)).ok()?;
        let mut block = Vec::new();
        (&mut file).take(end - start).read_to_end(&mut block).ok()?;
        block.extend_from_slice(&rest);
        let mut parts: Vec<&[u8]> = block.split(|b| *b == b'\n').collect();
        // Unless the block begins the file, its first part is incomplete.
        let first = if start > 0 && !parts.is_empty() {
            parts.remove(0).to_vec()
        } else {
            Vec::new()
        };
        if let Some(found) = parts.iter().rev().find_map(|l| parse_cost(l)) {
            return Some(found);
        }
        rest = first;
        // A single line longer than the whole search is not a cost line.
        if rest.len() as u64 > TAIL_BYTES * 4 {
            rest.clear();
        }
        end = start;
    }
    None
}

fn parse_cost(line: &[u8]) -> Option<CostState> {
    // Cheap check first: most lines are long messages.
    if !line.windows(12).any(|w| w == b"\"cost-state\"") {
        return None;
    }
    let v: Value = serde_json::from_slice(line).ok()?;
    if v.get("type")?.as_str()? != "cost-state" {
        return None;
    }
    let usd = v.get("totalCostUSD")?.as_f64()?;
    if !usd.is_finite() || !(0.0..1e9).contains(&usd) {
        return None;
    }
    Some(CostState {
        total_micro_usd: (usd * 1_000_000.0).round() as u64,
        started_ms: v.get("startTime").and_then(Value::as_u64),
    })
}

fn parse_line(line: &str, own: &[(String, u64)]) -> Option<ContextUsage> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "assistant" {
        return None;
    }
    // Lines of subagents say nothing about the main context.
    if v.get("isSidechain").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let message = v.get("message")?;
    let usage = message.get("usage")?;
    let field = |name: &str| usage.get(name).and_then(Value::as_u64).unwrap_or(0);
    let used = field("input_tokens")
        + field("cache_creation_input_tokens")
        + field("cache_read_input_tokens");
    if used == 0 {
        return None;
    }
    let model = message.get("model").and_then(Value::as_str).unwrap_or("");
    Some(ContextUsage {
        used_tokens: used,
        window_tokens: window_for_with(model, own),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().expect("tmp");
        for l in lines {
            writeln!(f, "{l}").expect("write");
        }
        f
    }

    const A1: &str = r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","usage":{"input_tokens":10,"cache_creation_input_tokens":100,"cache_read_input_tokens":1000,"output_tokens":5}}}"#;
    const A2: &str = r#"{"type":"assistant","message":{"model":"claude-haiku-5-5","usage":{"input_tokens":20,"cache_creation_input_tokens":200,"cache_read_input_tokens":39400,"output_tokens":9}}}"#;

    #[test]
    fn the_last_message_counts_and_sums_the_three_input_fields() {
        let f = write(&[
            A1,
            r#"{"type":"user","message":{}}"#,
            A2,
            r#"{"type":"cost-state"}"#,
        ]);
        let u = context_usage(f.path()).expect("usage");
        assert_eq!(u.used_tokens, 39_620);
        assert_eq!(u.window_tokens, 1_000_000);
    }

    #[test]
    fn subagent_lines_and_broken_lines_are_skipped() {
        let side = r#"{"type":"assistant","isSidechain":true,"message":{"usage":{"input_tokens":999999}}}"#;
        let f = write(&[A1, side, "{not json"]);
        assert_eq!(context_usage(f.path()).expect("usage").used_tokens, 1110);
    }

    #[test]
    fn missing_or_empty_files_give_nothing() {
        assert!(context_usage(Path::new("/nonexistent/transcript.jsonl")).is_none());
        assert!(context_usage(write(&[]).path()).is_none());
        assert!(context_usage(write(&[r#"{"type":"user"}"#]).path()).is_none());
    }

    #[test]
    fn a_long_file_is_read_from_the_end() {
        let filler = format!(
            r#"{{"type":"user","message":{{"content":"{}"}}}}"#,
            "x".repeat(2000)
        );
        let mut lines = vec![A1];
        for _ in 0..400 {
            lines.push(&filler);
        }
        lines.push(A2);
        let f = write(&lines);
        assert_eq!(context_usage(f.path()).expect("usage").used_tokens, 39_620);
    }

    fn cost(usd: &str, start: u64) -> String {
        format!(
            r#"{{"type":"cost-state","sessionId":"s","totalCostUSD":{usd},"startTime":{start}}}"#
        )
    }

    #[test]
    fn the_last_cost_line_counts_even_far_from_the_end() {
        let filler = format!(
            r#"{{"type":"user","message":{{"content":"{}"}}}}"#,
            "x".repeat(3000)
        );
        let (old, new) = (cost("1.5", 10), cost("12.3456789", 20));
        let mut lines: Vec<&str> = vec![&old, &new];
        for _ in 0..600 {
            lines.push(&filler);
        }
        let f = write(&lines);
        assert!(f.as_file().metadata().expect("meta").len() > 3 * TAIL_BYTES);
        let c = cost_state(f.path()).expect("cost");
        assert_eq!(c.total_micro_usd, 12_345_679);
        assert_eq!(c.started_ms, Some(20));
    }

    #[test]
    fn a_cost_line_across_two_blocks_is_found() {
        let line = cost("2", 5);
        // Place the line so that a block boundary falls inside it.
        let tail_len = TAIL_BYTES as usize - line.len() / 2;
        let filler = "y".repeat(tail_len);
        let f = write(&["{}", &line, &filler]);
        assert_eq!(
            cost_state(f.path()).expect("cost").total_micro_usd,
            2_000_000
        );
    }

    #[test]
    fn broken_or_missing_cost_lines_give_nothing() {
        assert!(cost_state(Path::new("/nonexistent")).is_none());
        assert!(cost_state(write(&[A1]).path()).is_none());
        let bad = [
            r#"{"type":"cost-state","totalCostUSD":-1}"#,
            r#"{"type":"cost-state","totalCostUSD":"free"}"#,
            r#"{"type":"user","text":"\"cost-state\""}"#,
        ];
        assert!(cost_state(write(&bad).path()).is_none());
    }

    #[test]
    fn the_users_own_windows_go_first() {
        let mut f = tempfile::NamedTempFile::new().expect("tmp");
        write!(
            f,
            r#"{{"New-Model": 500000, "new-model-x": 600000, "bad": "x", "": 1}}"#
        )
        .expect("write");
        let own = load_windows(f.path());
        assert_eq!(own.len(), 2);
        assert_eq!(window_for_with("claude-new-model-x", &own), 600_000);
        assert_eq!(window_for_with("claude-new-model", &own), 500_000);
        assert_eq!(window_for_with("claude-opus-5-5", &own), 1_000_000);
        assert!(load_windows(Path::new("/nonexistent")).is_empty());
    }

    #[test]
    fn windows_by_model() {
        assert_eq!(window_for("claude-opus-5-5"), 1_000_000);
        assert_eq!(window_for("claude-sonnet-4-5[1m]"), 1_000_000);
        assert_eq!(window_for("claude-sonnet-4-5"), DEFAULT_WINDOW);
        assert_eq!(window_for(""), DEFAULT_WINDOW);
    }
}
