//! Hook inputs from Claude Code and Codex, and the answer to PermissionRequest.
//!
//! The parser is tolerant: unknown fields are ignored, unknown events become
//! [`HookEvent::Unknown`], and everything except `session_id` and
//! `hook_event_name` is optional. The interface of Claude Code can change with
//! every version, and a parse error must never be the reason for a hung agent.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Error;

/// Fields every hook input carries.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Common {
    pub session_id: String,
    #[serde(default)]
    pub transcript_path: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub prompt_id: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
    #[serde(default)]
    pub effort: Option<Effort>,
    /// Set when the event comes from a subagent.
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Effort {
    #[serde(default)]
    pub level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionStart {
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SessionEnd {
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UserPromptSubmit {
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PreToolUse {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PostToolUse {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub tool_response: Value,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PostToolUseFailure {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub tool_use_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub is_interrupt: Option<bool>,
    #[serde(default)]
    pub duration_ms: Option<u64>,
}

/// PermissionRequest carries no `tool_use_id` (checked in M0).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PermissionRequest {
    pub tool_name: String,
    #[serde(default)]
    pub tool_input: Value,
    #[serde(default)]
    pub permission_suggestions: Vec<Suggestion>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Notification {
    #[serde(default)]
    pub notification_type: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SubagentStart {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SubagentStop {
    #[serde(default)]
    pub agent_id: Option<String>,
    #[serde(default)]
    pub agent_type: Option<String>,
    #[serde(default)]
    pub last_assistant_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Stop {
    #[serde(default)]
    pub last_assistant_message: Option<String>,
    /// Background tasks that still run. Not empty: the agent is paused.
    #[serde(default)]
    pub background_tasks: Vec<Value>,
    #[serde(default)]
    pub stop_hook_active: Option<bool>,
}

/// One entry of `permission_suggestions`. `raw` is what Claude Code sent and
/// goes back unchanged in `updatedPermissions`; `kind` is only for display.
#[derive(Debug, Clone, PartialEq)]
pub struct Suggestion {
    pub raw: Value,
    pub kind: SuggestionKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SuggestionKind {
    /// A permission rule ("always allow"). The only kind the button offers.
    AddRules {
        rules: Vec<Rule>,
        behavior: Option<String>,
        destination: Option<String>,
    },
    AddDirectories {
        directories: Vec<String>,
        destination: Option<String>,
    },
    SetMode {
        mode: String,
        destination: Option<String>,
    },
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Rule {
    #[serde(rename = "toolName")]
    pub tool_name: String,
    #[serde(default, rename = "ruleContent")]
    pub rule_content: Option<String>,
}

impl<'de> Deserialize<'de> for Suggestion {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Value::deserialize(d)?;
        let kind = SuggestionKind::from_value(&raw);
        Ok(Suggestion { raw, kind })
    }
}

impl SuggestionKind {
    fn from_value(v: &Value) -> Self {
        let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_owned);
        let ty = text("type").unwrap_or_default();
        match ty.as_str() {
            "addRules" => SuggestionKind::AddRules {
                rules: v
                    .get("rules")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default(),
                behavior: text("behavior"),
                destination: text("destination"),
            },
            "addDirectories" => SuggestionKind::AddDirectories {
                directories: v
                    .get("directories")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default(),
                destination: text("destination"),
            },
            "setMode" => SuggestionKind::SetMode {
                mode: text("mode").unwrap_or_default(),
                destination: text("destination"),
            },
            _ => SuggestionKind::Other(ty),
        }
    }
}

/// The event with its payload.
#[derive(Debug, Clone, PartialEq)]
pub enum HookEvent {
    SessionStart(SessionStart),
    SessionEnd(SessionEnd),
    UserPromptSubmit(UserPromptSubmit),
    PreToolUse(PreToolUse),
    PostToolUse(PostToolUse),
    PostToolUseFailure(PostToolUseFailure),
    PermissionRequest(PermissionRequest),
    Notification(Notification),
    SubagentStart(SubagentStart),
    SubagentStop(SubagentStop),
    Stop(Stop),
    // Not seen in M0 (could not be triggered), so the payload stays raw.
    StopFailure(Value),
    PreCompact(Value),
    PostCompact(Value),
    CwdChanged(Value),
    /// An event this version does not know. Name and raw input are kept.
    Unknown {
        name: String,
        raw: Value,
    },
}

/// A parsed hook input.
#[derive(Debug, Clone, PartialEq)]
pub struct HookInput {
    pub common: Common,
    pub event: HookEvent,
}

impl HookInput {
    /// Parse the JSON a hook receives on standard input.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let raw: Value = serde_json::from_slice(bytes)?;
        let name = raw
            .get("hook_event_name")
            .and_then(Value::as_str)
            .ok_or(Error::MissingEventName)?
            .to_owned();
        let common: Common = serde_json::from_value(raw.clone())?;
        macro_rules! payload {
            ($variant:ident) => {
                HookEvent::$variant(serde_json::from_value(raw.clone())?)
            };
        }
        let event = match name.as_str() {
            "SessionStart" => payload!(SessionStart),
            "SessionEnd" => payload!(SessionEnd),
            "UserPromptSubmit" => payload!(UserPromptSubmit),
            "PreToolUse" => payload!(PreToolUse),
            "PostToolUse" => payload!(PostToolUse),
            "PostToolUseFailure" => payload!(PostToolUseFailure),
            "PermissionRequest" => payload!(PermissionRequest),
            "Notification" => payload!(Notification),
            "SubagentStart" => payload!(SubagentStart),
            "SubagentStop" => payload!(SubagentStop),
            "Stop" => payload!(Stop),
            "StopFailure" => HookEvent::StopFailure(raw),
            "PreCompact" => HookEvent::PreCompact(raw),
            "PostCompact" => HookEvent::PostCompact(raw),
            "CwdChanged" => HookEvent::CwdChanged(raw),
            _ => HookEvent::Unknown { name, raw },
        };
        Ok(HookInput { common, event })
    }
}

/// What the hook program prints for PermissionRequest.
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionAnswer {
    /// `updated_permissions` are the suggestions from Claude Code, unchanged.
    Allow { updated_permissions: Vec<Value> },
    /// `interrupt` stays off, so the agent keeps working.
    Deny { message: String },
    /// Allow with changed input: how a question is answered.
    AllowWithInput { updated_input: Value },
}

impl PermissionAnswer {
    pub fn to_json(&self) -> Value {
        let decision = match self {
            PermissionAnswer::Allow {
                updated_permissions,
            } if updated_permissions.is_empty() => {
                serde_json::json!({"behavior": "allow"})
            }
            PermissionAnswer::Allow {
                updated_permissions,
            } => {
                serde_json::json!({"behavior": "allow", "updatedPermissions": updated_permissions})
            }
            PermissionAnswer::Deny { message } => {
                serde_json::json!({"behavior": "deny", "message": message})
            }
            PermissionAnswer::AllowWithInput { updated_input } => {
                serde_json::json!({"behavior": "allow", "updatedInput": updated_input})
            }
        };
        serde_json::json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": decision}})
    }
}

impl Serialize for PermissionAnswer {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.to_json().serialize(s)
    }
}
