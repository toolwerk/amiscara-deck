//! What a permission request shows, and the answer to it.

use deck_proto::agent::{AlwaysAllow, Question, QuestionOption};
use deck_proto::hook::{PermissionRequest, Suggestion, SuggestionKind};
use deck_proto::text::{is_deceptive, show_deceptive};
use serde_json::Value;

/// The card shows at most this many characters of the input, on one line.
/// More than that, or more than one line: the card is shortened and "allow"
/// only comes after expanding (decision E4).
pub const MAX_SUMMARY: usize = 120;

/// A plan waiting for approval is never held: its dialog offers more than a
/// yes, and a plain "allow" from a hook would pick for the user.
pub fn never_held(tool_name: &str) -> bool {
    tool_name == "ExitPlanMode"
}

/// The tool whose dialog is a question to the user.
pub const QUESTION_TOOL: &str = "AskUserQuestion";

/// More than this is not a card any more: such a question stays where the
/// agent runs.
const MAX_QUESTIONS: usize = 8;
const MAX_OPTIONS: usize = 20;

/// The questions of an `AskUserQuestion` input, as Claude Code sent them, if
/// every one can be answered by picking options: each has its own text and
/// choices with distinct labels. Otherwise `None`.
pub fn parse_questions(input: &Value) -> Option<Vec<Question>> {
    let list = input.get("questions")?.as_array()?;
    if list.is_empty() || list.len() > MAX_QUESTIONS {
        return None;
    }
    let mut seen = Vec::new();
    list.iter()
        .map(|q| {
            let question = q.get("question")?.as_str()?.to_owned();
            if question.is_empty() || seen.contains(&question) {
                return None;
            }
            seen.push(question.clone());
            let options = q.get("options")?.as_array()?;
            if options.is_empty() || options.len() > MAX_OPTIONS {
                return None;
            }
            let options: Option<Vec<QuestionOption>> = options
                .iter()
                .map(|o| {
                    Some(QuestionOption {
                        label: o
                            .get("label")?
                            .as_str()
                            .filter(|l| !l.is_empty())?
                            .to_owned(),
                        description: o
                            .get("description")
                            .and_then(Value::as_str)
                            .filter(|d| !d.is_empty())
                            .map(str::to_owned),
                    })
                })
                .collect();
            let options = options?;
            let distinct = options
                .iter()
                .enumerate()
                .all(|(i, o)| options[..i].iter().all(|p| p.label != o.label));
            distinct.then(|| Question {
                question,
                header: q.get("header").and_then(Value::as_str).map(str::to_owned),
                multi_select: q.get("multiSelect").and_then(Value::as_bool) == Some(true),
                options,
            })
        })
        .collect()
}

/// The questions for the card: hidden characters shown.
pub fn display_questions(questions: &[Question]) -> Vec<Question> {
    questions
        .iter()
        .map(|q| Question {
            question: show_deceptive(&q.question),
            header: q.header.as_deref().map(show_deceptive),
            multi_select: q.multi_select,
            options: q
                .options
                .iter()
                .map(|o| QuestionOption {
                    label: show_deceptive(&o.label),
                    description: o.description.as_deref().map(show_deceptive),
                })
                .collect(),
        })
        .collect()
}

/// The input that answers the questions: the raw input of Claude Code plus
/// `answers`, question text to chosen label (several joined with ", "), as the
/// terminal does it. `None` if the picks do not fit the questions.
pub fn answered_input(input: &Value, picks: &[Vec<usize>]) -> Option<(Value, Vec<String>)> {
    let questions = parse_questions(input)?;
    if picks.len() != questions.len() {
        return None;
    }
    let mut answers = serde_json::Map::new();
    let mut said = Vec::new();
    for (q, pick) in questions.iter().zip(picks) {
        let distinct = pick.iter().enumerate().all(|(i, p)| !pick[..i].contains(p));
        if pick.is_empty() || !distinct || (!q.multi_select && pick.len() != 1) {
            return None;
        }
        let labels: Option<Vec<&str>> = pick
            .iter()
            .map(|i| q.options.get(*i).map(|o| o.label.as_str()))
            .collect();
        let joined = labels?.join(", ");
        said.push(show_deceptive(&format!("{} = {joined}", q.question)));
        answers.insert(q.question.clone(), Value::String(joined));
    }
    let mut out = input.as_object()?.clone();
    out.insert("answers".into(), Value::Object(answers));
    Some((Value::Object(out), said))
}

/// The questions as readable text: each question, then its choices.
fn question_text(input: &Value) -> Option<String> {
    let blocks: Vec<String> = input
        .get("questions")?
        .as_array()?
        .iter()
        .filter_map(|q| {
            let mut out = q.get("question")?.as_str()?.to_owned();
            for o in q
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(label) = o.get("label").and_then(Value::as_str) else {
                    continue;
                };
                out.push_str("\n• ");
                out.push_str(label);
                if let Some(d) = o
                    .get("description")
                    .and_then(Value::as_str)
                    .filter(|d| !d.is_empty())
                {
                    out.push_str(": ");
                    out.push_str(d);
                }
            }
            Some(out)
        })
        .collect();
    (!blocks.is_empty()).then(|| blocks.join("\n\n"))
}

/// The text a human has to read before allowing: command, path, or for
/// tools that write, the path and what is written. Whatever is written must be
/// visible before "allow", so it goes into the full text on further lines and
/// the card counts as shortened.
fn raw_full_text(req: &PermissionRequest) -> String {
    let input = &req.tool_input;
    let text = |k: &str| input.get(k).and_then(Value::as_str);
    let with_body = |path: &str, parts: Vec<(&str, String)>| {
        let mut out = path.to_owned();
        for (label, body) in parts.into_iter().filter(|(_, b)| !b.is_empty()) {
            out.push_str(&format!("\n--- {label} ---\n{body}"));
        }
        out
    };
    let known = match req.tool_name.as_str() {
        "Bash" => text("command").map(str::to_owned),
        QUESTION_TOOL => question_text(input),
        "ExitPlanMode" => text("plan").map(str::to_owned),
        "Read" => text("file_path").map(str::to_owned),
        "Write" => text("file_path").map(|p| {
            with_body(
                p,
                vec![("Inhalt", text("content").unwrap_or("").to_owned())],
            )
        }),
        "Edit" => text("file_path").map(|p| {
            with_body(
                p,
                vec![
                    ("alt", text("old_string").unwrap_or("").to_owned()),
                    ("neu", text("new_string").unwrap_or("").to_owned()),
                    ("Optionen", options(input, &["replace_all"])),
                ],
            )
        }),
        "MultiEdit" => text("file_path").map(|p| {
            let edits = input
                .get("edits")
                .and_then(Value::as_array)
                .map(|e| {
                    e.iter()
                        .enumerate()
                        .flat_map(|(i, edit)| {
                            let get = |k: &str| {
                                edit.get(k).and_then(Value::as_str).unwrap_or("").to_owned()
                            };
                            [
                                (format!("{} alt", i + 1), get("old_string")),
                                (format!("{} neu", i + 1), get("new_string")),
                                (
                                    format!("{} Optionen", i + 1),
                                    options(edit, &["replace_all"]),
                                ),
                            ]
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut out = p.to_owned();
            for (label, body) in edits.into_iter().filter(|(_, b)| !b.is_empty()) {
                out.push_str(&format!("\n--- {label} ---\n{body}"));
            }
            let opts = options(input, &["replace_all"]);
            if !opts.is_empty() {
                out.push_str(&format!("\n--- Optionen ---\n{opts}"));
            }
            out
        }),
        "NotebookEdit" => text("notebook_path").map(|p| {
            with_body(
                p,
                vec![
                    ("neu", text("new_source").unwrap_or("").to_owned()),
                    (
                        "Optionen",
                        options(input, &["edit_mode", "cell_id", "cell_type"]),
                    ),
                ],
            )
        }),
        _ => None,
    };
    match known {
        // Whatever the card does not show by name is shown after it: nothing
        // may be allowed that was not seen (a Bash call can ask to run
        // without its sandbox, for example). The extra lines also make the
        // card a shortened one, so "allow" needs it expanded.
        Some(mut out) => {
            let extra = unshown(&req.tool_name, input);
            if !extra.is_empty() {
                out.push_str("\n--- Weitere Angaben ---\n");
                out.push_str(&extra.join("\n"));
            }
            out
        }
        None if input.is_null() => req.tool_name.clone(),
        None => format!("{}: {input}", req.tool_name),
    }
}

/// The fields of a known tool that its card already shows, or that change
/// nothing about what is done (a description, a time limit, which part of a
/// file is read).
fn shown_fields(tool_name: &str) -> &'static [&'static str] {
    match tool_name {
        "Bash" => &["command", "description", "timeout"],
        "Read" => &["file_path", "offset", "limit", "pages"],
        "Write" => &["file_path", "content"],
        "Edit" => &["file_path", "old_string", "new_string", "replace_all"],
        "MultiEdit" => &["file_path", "edits", "replace_all"],
        "NotebookEdit" => &[
            "notebook_path",
            "new_source",
            "edit_mode",
            "cell_id",
            "cell_type",
        ],
        QUESTION_TOOL => &["questions"],
        "ExitPlanMode" => &["plan"],
        _ => &[],
    }
}

/// Switches whose "off" is what happens without them.
const HARMLESS_WHEN_FALSE: &[&str] = &["dangerouslyDisableSandbox", "run_in_background"];

/// `key=value` for every field the card does not show, edits of a MultiEdit
/// included.
fn unshown(tool_name: &str, input: &Value) -> Vec<String> {
    let line = |prefix: &str, key: &str, value: &Value| match value.as_str() {
        Some(t) => format!("{prefix}{key}={t}"),
        None => format!("{prefix}{key}={value}"),
    };
    let matters = |key: &str, value: &Value| {
        !value.is_null() && !(value == &Value::Bool(false) && HARMLESS_WHEN_FALSE.contains(&key))
    };
    let shown = shown_fields(tool_name);
    let mut out: Vec<String> = input
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, v)| !shown.contains(&k.as_str()) && matters(k, v))
        .map(|(k, v)| line("", k, v))
        .collect();
    if tool_name == "MultiEdit" {
        let edits = input.get("edits").and_then(Value::as_array);
        for (i, edit) in edits.into_iter().flatten().enumerate() {
            let prefix = format!("{} ", i + 1);
            out.extend(
                edit.as_object()
                    .into_iter()
                    .flatten()
                    .filter(|(k, v)| {
                        !["old_string", "new_string", "replace_all"].contains(&k.as_str())
                            && matters(k, v)
                    })
                    .map(|(k, v)| line(&prefix, k, v)),
            );
        }
    }
    out
}

/// `key=value` for the given keys that are set, on one line.
fn options(input: &Value, keys: &[&str]) -> String {
    keys.iter()
        .filter_map(|k| {
            input.get(*k).map(|v| match v.as_str() {
                Some(t) => format!("{k}={t}"),
                None => format!("{k}={v}"),
            })
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Tools that write always need expanding, whatever the body: the card must
/// not suggest that a path alone is all there is.
pub fn must_expand(tool_name: &str) -> bool {
    matches!(tool_name, "Write" | "Edit" | "MultiEdit" | "NotebookEdit")
}

/// What a card shows of a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rendered {
    /// First line, cut, deceptive characters escaped.
    pub summary: String,
    /// All of it, deceptive characters escaped, line breaks kept.
    pub full: String,
    /// "Allow" only comes after expanding. True when the text is long, has
    /// more than one line, holds a character that can hide something, or the
    /// tool writes (decision E4 and its security review).
    pub truncated: bool,
}

/// Render the request. Detection runs on the raw text, escaping comes after:
/// no renderer ever gets a raw control or bidi character.
pub fn render(req: &PermissionRequest) -> Rendered {
    let raw = raw_full_text(req);
    let first = raw.lines().next().unwrap_or("");
    let multi = raw.lines().nth(1).is_some();
    let deceptive = raw.chars().any(is_deceptive);
    let shown = show_deceptive(first);
    let too_long = shown.chars().count() > MAX_SUMMARY;
    let truncated = too_long || multi || deceptive || must_expand(&req.tool_name);
    let summary = if too_long || multi {
        let cut: String = shown.chars().take(MAX_SUMMARY - 1).collect();
        format!("{cut}…")
    } else {
        shown
    };
    Rendered {
        summary,
        full: show_deceptive(&raw),
        truncated,
    }
}

/// The four targets Claude Code knows pass unchanged (the UI has words for
/// them). Anything else is shown escaped.
fn display_destination(d: &str) -> String {
    match d {
        "session" | "localSettings" | "projectSettings" | "userSettings" => d.to_owned(),
        other => show_deceptive(other),
    }
}

/// Only `addRules` suggestions become "always allow". `addDirectories` and
/// `setMode` are something else and are not offered.
pub fn always_allow(suggestions: &[Suggestion]) -> Vec<AlwaysAllow> {
    suggestions
        .iter()
        .enumerate()
        .filter_map(|(index, s)| match &s.kind {
            SuggestionKind::AddRules {
                rules,
                behavior,
                destination,
            } if behavior.as_deref().is_none_or(|b| b == "allow") && !rules.is_empty() => {
                Some(AlwaysAllow {
                    index,
                    // For display only: hidden characters in a rule (it comes from
                    // outside) are shown, the daemon keeps the raw suggestion and
                    // sends exactly that back.
                    rules: rules
                        .iter()
                        .map(|r| {
                            show_deceptive(&match &r.rule_content {
                                Some(c) => format!("{}({})", r.tool_name, c),
                                None => r.tool_name.clone(),
                            })
                        })
                        .collect(),
                    destination: destination.as_deref().map(display_destination),
                })
            }
            _ => None,
        })
        .collect()
}
