//! Install and remove the hook entries for Claude Code.
//!
//! Only the section `hooks` of `~/.claude/settings.json` is touched. Our entries
//! are recognized by the path of the hook program, foreign entries stay where
//! they are. Installing twice gives the same file. Before every change the old
//! file is copied, and the new one replaces it in one step (write next to it,
//! then rename). A file that is not valid JSON, or has an unexpected shape, is
//! never changed.

mod file;
mod shell;
pub mod version;

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

pub use file::Outcome;

/// The events Deck listens to (Detailkonzept, section 4.2).
pub const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "SubagentStart",
    "SubagentStop",
    "PreCompact",
    "PostCompact",
    "CwdChanged",
    "Stop",
    "StopFailure",
    "SessionEnd",
];

/// The events Deck listens to in Codex: read only (Detailkonzept, section 10).
/// `PermissionRequest` is only watched, Deck never answers for Codex.
pub const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PermissionRequest",
    "Stop",
    "SessionEnd",
];

/// Which tool the entries are for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Claude,
    Codex,
}

impl Target {
    pub fn events(self) -> &'static [&'static str] {
        match self {
            Target::Claude => EVENTS,
            Target::Codex => CODEX_EVENTS,
        }
    }
}

/// Events whose hooks Claude Code filters by tool name.
const TOOL_EVENTS: &[&str] = &[
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
];

/// Timeout of the PermissionRequest hook in seconds: a little above the
/// longest wait Deck allows (9 minutes), so Deck lets go before Claude Code
/// gives up on the hook.
pub const PERMISSION_TIMEOUT_SECS: u64 = 570;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: not valid JSON ({source}); left untouched")]
    Invalid {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("{path}: unexpected shape at {at}; left untouched")]
    Shape { path: PathBuf, at: String },
    #[error("{path}: changed while it was being edited; try again")]
    Changed { path: PathBuf },
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
}

/// `~/.claude/settings.json`, or in `$CLAUDE_CONFIG_DIR` if that is set.
pub fn default_claude_settings() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("settings.json"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/settings.json"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventState {
    /// Our entry is there and as we would write it.
    Installed,
    Missing,
    /// Our entry is there but differs (older version, edited by hand).
    Outdated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Status {
    pub file_exists: bool,
    pub events: Vec<(&'static str, EventState)>,
    /// The hook program exists at the path in the entries.
    pub program_exists: bool,
    pub program_executable: bool,
    /// Hook entries of other programs, left alone.
    pub foreign_entries: usize,
}

impl Status {
    pub fn fully_installed(&self) -> bool {
        self.events.iter().all(|(_, s)| *s == EventState::Installed)
    }
}

fn command_for(program: &Path, target: Target) -> String {
    let program = shell::quote(&program.to_string_lossy());
    match target {
        Target::Claude => program,
        // The program learns from the argument which tool calls it.
        Target::Codex => format!("{program} codex"),
    }
}

/// The entry as we want it for `event`.
fn desired_group(event: &str, command: &str, target: Target) -> Value {
    let mut hook = Map::new();
    hook.insert("type".into(), json!("command"));
    hook.insert("command".into(), json!(command));
    match target {
        Target::Claude if event == "PermissionRequest" => {
            hook.insert("timeout".into(), json!(PERMISSION_TIMEOUT_SECS));
        }
        Target::Claude => {
            hook.insert("async".into(), json!(true));
        }
        // Codex has no async. A PermissionRequest may be held for as long as
        // Deck waits, so its limit is above that from the start: changing it
        // later would cost the user's trust in the entry.
        Target::Codex if event == "PermissionRequest" => {
            hook.insert("timeout".into(), json!(PERMISSION_TIMEOUT_SECS));
        }
        Target::Codex => {
            hook.insert("timeout".into(), json!(10));
        }
    }
    let mut group = Map::new();
    if target == Target::Claude && TOOL_EVENTS.contains(&event) {
        group.insert("matcher".into(), json!("*"));
    }
    group.insert("hooks".into(), Value::Array(vec![Value::Object(hook)]));
    Value::Object(group)
}

fn is_ours(hook: &Value, program: &str) -> bool {
    hook.get("type").and_then(Value::as_str) == Some("command")
        && hook
            .get("command")
            .and_then(Value::as_str)
            .is_some_and(|c| shell::first_word(c).as_deref() == Some(program))
}

/// Where our entries are in the groups of one event: (group, hook) pairs.
fn find_ours(groups: &[Value], program: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    for (g, group) in groups.iter().enumerate() {
        if let Some(hooks) = group.get("hooks").and_then(Value::as_array) {
            for (h, hook) in hooks.iter().enumerate() {
                if is_ours(hook, program) {
                    found.push((g, h));
                }
            }
        }
    }
    found
}

fn hooks_section<'a>(
    root: &'a mut Value,
    path: &Path,
) -> Result<Option<&'a mut Map<String, Value>>, Error> {
    let shape = |at: &str| Error::Shape {
        path: path.to_owned(),
        at: at.to_owned(),
    };
    let obj = root.as_object_mut().ok_or_else(|| shape("top level"))?;
    match obj.get_mut("hooks") {
        None => Ok(None),
        Some(Value::Object(m)) => Ok(Some(m)),
        Some(_) => Err(shape("hooks")),
    }
}

/// Check the whole `hooks` section before anything is changed.
fn check_shape(root: &mut Value, path: &Path) -> Result<(), Error> {
    let shape = |at: String| Error::Shape {
        path: path.to_owned(),
        at,
    };
    if let Some(section) = hooks_section(root, path)? {
        for (event, groups) in section.iter() {
            let arr = groups
                .as_array()
                .ok_or_else(|| shape(format!("hooks.{event}")))?;
            for (i, group) in arr.iter().enumerate() {
                let g = group
                    .as_object()
                    .ok_or_else(|| shape(format!("hooks.{event}[{i}]")))?;
                if let Some(h) = g.get("hooks")
                    && !h.is_array()
                {
                    return Err(shape(format!("hooks.{event}[{i}].hooks")));
                }
            }
        }
    }
    Ok(())
}

fn install_into(
    root: &mut Value,
    path: &Path,
    program: &Path,
    target: Target,
) -> Result<(), Error> {
    check_shape(root, path)?;
    let prog = program.to_string_lossy().into_owned();
    let command = command_for(program, target);
    let obj = root.as_object_mut().expect("checked: object");
    let section = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("checked: object");
    for event in target.events() {
        let want = desired_group(event, &command, target);
        let groups = section
            .entry(*event)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .expect("checked: array");
        let ours = find_ours(groups, &prog);
        // Exactly one of ours, alone in its group: keep it, or fix it in place.
        if let [(g, _)] = ours[..] {
            let alone = groups[g]
                .get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|h| h.len() == 1);
            if alone {
                if groups[g] != want {
                    groups[g] = want;
                }
                continue;
            }
        }
        remove_ours(groups, &prog, target == Target::Codex);
        groups.push(want);
    }
    Ok(())
}

fn is_empty_group(g: &Value) -> bool {
    g.get("hooks")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
}

/// Remove our hooks from the groups. A group that only held ours goes too,
/// unless `keep_places` and another group follows: Codex keys its trust by
/// the position of a group, so an empty one stays in front of foreign entries.
fn remove_ours(groups: &mut Vec<Value>, program: &str, keep_places: bool) {
    let mut emptied = Vec::new();
    for (i, group) in groups.iter_mut().enumerate() {
        if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
            let before = hooks.len();
            hooks.retain(|h| !is_ours(h, program));
            if hooks.is_empty() && before > 0 {
                emptied.push(i);
            }
        }
    }
    if keep_places {
        // Only what stands at the end can go without moving anybody.
        while groups.last().is_some_and(is_empty_group) {
            groups.pop();
        }
        // A kept place holds nothing but the empty list.
        for i in emptied {
            if let Some(g) = groups.get_mut(i) {
                *g = json!({"hooks": []});
            }
        }
    } else {
        groups.retain(|g| !is_empty_group(g));
    }
}

fn uninstall_from(
    root: &mut Value,
    path: &Path,
    program: &Path,
    target: Target,
) -> Result<(), Error> {
    check_shape(root, path)?;
    let prog = program.to_string_lossy().into_owned();
    let Some(section) = hooks_section(root, path)? else {
        return Ok(());
    };
    let events: Vec<String> = section.keys().cloned().collect();
    for event in events {
        let Some(groups) = section.get_mut(&event).and_then(Value::as_array_mut) else {
            continue;
        };
        if find_ours(groups, &prog).is_empty() {
            continue;
        }
        remove_ours(groups, &prog, target == Target::Codex);
        if groups.is_empty() {
            section.shift_remove(&event);
        }
    }
    if section.is_empty() {
        root.as_object_mut()
            .expect("checked: object")
            .shift_remove("hooks");
    }
    Ok(())
}

/// Add our hooks for Claude Code. Idempotent. Returns whether the file changed.
pub fn install(settings: &Path, program: &Path) -> Result<Outcome, Error> {
    install_for(Target::Claude, settings, program)
}

/// Add our hooks for the given tool.
pub fn install_for(target: Target, settings: &Path, program: &Path) -> Result<Outcome, Error> {
    file::edit(settings, |root| {
        install_into(root, settings, program, target)
    })
}

/// `~/.codex/hooks.json`, or in `$CODEX_HOME` if that is set.
pub fn default_codex_hooks() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("CODEX_HOME").filter(|d| !d.is_empty()) {
        return Some(PathBuf::from(d).join("hooks.json"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".codex/hooks.json"))
}

/// Remove our hooks for Claude Code and nothing else.
pub fn uninstall(settings: &Path, program: &Path) -> Result<Outcome, Error> {
    uninstall_for(Target::Claude, settings, program)
}

/// Remove our hooks for the given tool and nothing else.
pub fn uninstall_for(target: Target, settings: &Path, program: &Path) -> Result<Outcome, Error> {
    file::edit(settings, |root| {
        uninstall_from(root, settings, program, target)
    })
}

/// What is installed for Claude Code. Reads only.
pub fn status(settings: &Path, program: &Path) -> Result<Status, Error> {
    status_for(Target::Claude, settings, program)
}

/// What is installed for the given tool. Reads only.
pub fn status_for(target: Target, settings: &Path, program: &Path) -> Result<Status, Error> {
    let (mut root, existed) = file::read(settings)?;
    check_shape(&mut root, settings)?;
    let prog = program.to_string_lossy().into_owned();
    let command = command_for(program, target);
    let section = root.get("hooks").and_then(Value::as_object);
    let mut foreign = 0;
    if let Some(section) = section {
        for groups in section.values().filter_map(Value::as_array) {
            for group in groups {
                for hook in group
                    .get("hooks")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if !is_ours(hook, &prog) {
                        foreign += 1;
                    }
                }
            }
        }
    }
    let events = target
        .events()
        .iter()
        .map(|event| {
            let groups = section
                .and_then(|s| s.get(*event))
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let ours = find_ours(groups, &prog);
            let want = desired_group(event, &command, target);
            let state = match ours[..] {
                [] => EventState::Missing,
                [(g, _)] if groups[g] == want => EventState::Installed,
                _ => EventState::Outdated,
            };
            (*event, state)
        })
        .collect();
    let meta = std::fs::metadata(program).ok();
    Ok(Status {
        file_exists: existed,
        events,
        program_exists: meta.is_some(),
        program_executable: meta.as_ref().is_some_and(file::is_executable),
        foreign_entries: foreign,
    })
}
