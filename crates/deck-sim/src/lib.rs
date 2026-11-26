//! Simulated agents for development and demos: plays sessions built from the
//! recorded hook inputs (`testdata/hooks`) against a running daemon, the way
//! `deck-hook` would, without spending a single token.
//!
//! Every event is a recorded sample with the fields that make it a different
//! session patched in: session id, working folder, prompt, tool and so on.

use std::path::Path;
use std::time::Duration;

use deck_proto::PROTOCOL_VERSION;
use deck_proto::agent::Tool;
use deck_proto::message::{HookEnv, HookMessage, HookReply, decode_line, encode_line};
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// One recorded sample, as embedded in the program.
fn template(name: &str) -> Value {
    let text = match name {
        "SessionStart" => include_str!("../../../testdata/hooks/SessionStart.json"),
        "UserPromptSubmit" => include_str!("../../../testdata/hooks/UserPromptSubmit.json"),
        "PreToolUse" => include_str!("../../../testdata/hooks/PreToolUse-Bash.json"),
        "PostToolUse" => include_str!("../../../testdata/hooks/PostToolUse-Bash.json"),
        "PermissionRequest" => {
            include_str!("../../../testdata/hooks/PermissionRequest-Bash-addRules.json")
        }
        "Notification" => {
            include_str!("../../../testdata/hooks/Notification-permission_prompt.json")
        }
        "SubagentStart" => include_str!("../../../testdata/hooks/SubagentStart.json"),
        "SubagentStop" => include_str!("../../../testdata/hooks/SubagentStop.json"),
        "Stop" => include_str!("../../../testdata/hooks/Stop.json"),
        "SessionEnd" => include_str!("../../../testdata/hooks/SessionEnd.json"),
        other => panic!("no sample for {other}"),
    };
    let sample: Value = serde_json::from_str(text).expect("embedded sample is JSON");
    sample["input"].clone()
}

/// One event of a simulated session.
#[derive(Debug, Clone, PartialEq)]
pub struct Step {
    /// Wait this long before sending, in milliseconds at speed 1.
    pub after_ms: u64,
    /// The hook input, a complete one.
    pub input: Value,
}

/// A simulated agent.
#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: String,
    pub tool: Tool,
    pub env: HookEnv,
    pub steps: Vec<Step>,
}

struct Builder {
    id: String,
    cwd: String,
    steps: Vec<Step>,
}

impl Builder {
    fn new(id: &str, project: &str) -> Self {
        Builder {
            id: id.into(),
            cwd: format!("/Users/demo/dev/{project}"),
            steps: Vec::new(),
        }
    }

    fn event(mut self, after_ms: u64, name: &str, patch: Value) -> Self {
        let mut input = template(name);
        input["session_id"] = json!(self.id);
        input["cwd"] = json!(self.cwd);
        input["transcript_path"] = json!(format!("/Users/demo/.claude/projects/{}.jsonl", self.id));
        input["scratchpad_dir"] = json!(format!("/tmp/demo/{}", self.id));
        if let (Some(obj), Some(p)) = (input.as_object_mut(), patch.as_object()) {
            obj.extend(p.clone());
        }
        // A tool call has its own id: derive it from the position.
        if input.get("tool_use_id").is_some() {
            input["tool_use_id"] = json!(format!("toolu_{}_{}", self.id, self.steps.len()));
        }
        self.steps.push(Step { after_ms, input });
        self
    }

    fn start(self, after_ms: u64) -> Self {
        self.event(after_ms, "SessionStart", json!({"source": "startup"}))
    }

    fn prompt(self, after_ms: u64, text: &str) -> Self {
        self.event(after_ms, "UserPromptSubmit", json!({"prompt": text}))
    }

    fn bash(self, after_ms: u64, command: &str) -> Self {
        self.event(
            after_ms,
            "PreToolUse",
            json!({"tool_name": "Bash", "tool_input": {"command": command}}),
        )
    }

    fn done_bash(self, after_ms: u64, command: &str) -> Self {
        self.event(
            after_ms,
            "PostToolUse",
            json!({"tool_name": "Bash", "tool_input": {"command": command}}),
        )
    }

    fn ask(self, after_ms: u64, tool: &str, input: Value) -> Self {
        self.event(
            after_ms,
            "PermissionRequest",
            json!({"tool_name": tool, "tool_input": input, "permission_suggestions": []}),
        )
    }

    fn ask_bash(self, after_ms: u64, command: &str) -> Self {
        // Keep the recorded suggestion shape: an addRules entry for exactly this command.
        let sugg = json!([{"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": command}],
                           "behavior": "allow", "destination": "localSettings"}]);
        self.event(after_ms, "PermissionRequest", json!({"tool_name": "Bash", "tool_input": {"command": command}, "permission_suggestions": sugg}))
    }

    fn stop(self, after_ms: u64, message: &str) -> Self {
        self.event(
            after_ms,
            "Stop",
            json!({"last_assistant_message": message, "background_tasks": []}),
        )
    }

    fn end(self, after_ms: u64) -> Self {
        self.event(after_ms, "SessionEnd", json!({"reason": "other"}))
    }

    fn build(self, tool: Tool, env: HookEnv) -> Session {
        Session {
            id: self.id,
            tool,
            env,
            steps: self.steps,
        }
    }
}

fn herdr(pane: &str) -> HookEnv {
    HookEnv {
        herdr_pane_id: Some(pane.into()),
        ..HookEnv::default()
    }
}

fn terminal(app: &str) -> HookEnv {
    HookEnv {
        term_program: Some(app.into()),
        ..HookEnv::default()
    }
}

/// Names of the built-in scenarios.
pub const SCENARIOS: &[&str] = &["demo", "permission", "busy"];

/// A built-in scenario.
pub fn scenario(name: &str) -> Option<Vec<Session>> {
    match name {
        // Three agents: one works, one asks, one has finished.
        "demo" => Some(vec![
            Builder::new("sim-app", "shop-app")
                .start(0)
                .prompt(500, "Fix the import of the product feed")
                .bash(1500, "cargo test -p importer")
                .done_bash(2500, "cargo test -p importer")
                .bash(1000, "git status --short")
                .ask_bash(1500, "cargo fmt --all")
                .done_bash(500, "cargo fmt --all")
                .stop(2000, "The import works again, tests pass.")
                .build(Tool::Claude, herdr("w1:p1")),
            Builder::new("sim-api", "api-server")
                .start(300)
                .prompt(1000, "Add a health endpoint")
                .bash(2000, "npm run lint")
                .done_bash(3000, "npm run lint")
                .bash(1000, "npm test")
                .build(Tool::Claude, terminal("ghostty")),
            Builder::new("sim-docs", "docs-site")
                .start(100)
                .prompt(600, "Update the install guide")
                .bash(800, "ls docs")
                .done_bash(600, "ls docs")
                .stop(1200, "The guide is up to date.")
                .end(2000)
                .build(Tool::Claude, terminal("iTerm.app")),
        ]),
        // One request of each kind a card has to show.
        "permission" => Some(vec![Builder::new("sim-perm", "shop-app")
            .start(0)
            .prompt(300, "Clean up and publish")
            .ask_bash(800, "rm -rf node_modules && npm ci")
            .ask(
                300,
                "Write",
                json!({"file_path": "/Users/demo/dev/shop-app/deploy.sh", "content": "#!/bin/sh\nrsync -a dist/ server:/var/www/\n"}),
            )
            .stop(500, "Done.")
            .build(Tool::Claude, herdr("w1:p2"))]),
        // Many agents at once, for a full grid.
        "busy" => Some(
            (1..=8)
                .map(|n| {
                    Builder::new(&format!("sim-busy-{n}"), &format!("project-{n}"))
                        .start(n * 100)
                        .prompt(300, "Refactor the module")
                        .bash(500 + n * 150, "cargo build")
                        .done_bash(1500, "cargo build")
                        .build(Tool::Claude, terminal("ghostty"))
                })
                .collect(),
        ),
        _ => None,
    }
}

/// How the player behaves.
#[derive(Debug, Clone)]
pub struct Options {
    /// 1.0 is real time, 10.0 ten times faster.
    pub speed: f64,
    /// How long a PermissionRequest may wait for an answer before the
    /// simulated agent goes on, like a hook program that gave up.
    pub permission_wait: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            speed: 1.0,
            permission_wait: Duration::from_secs(60),
        }
    }
}

/// What the player reports while it plays.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Sent {
        session: String,
        event: String,
    },
    /// The answer to a PermissionRequest: what the hook program would print.
    Answered {
        session: String,
        output: Option<Value>,
    },
    NoDaemon,
}

/// Play the sessions in parallel, each in its own order. Returns when all are
/// done. `report` is called for every event.
pub async fn play(
    socket: &Path,
    sessions: Vec<Session>,
    opts: &Options,
    report: impl Fn(Event) + Send + Sync + Clone + 'static,
) {
    let mut tasks = Vec::new();
    for s in sessions {
        let socket = socket.to_owned();
        let opts = opts.clone();
        let report = report.clone();
        tasks.push(tokio::spawn(async move {
            play_one(&socket, s, &opts, report).await
        }));
    }
    for t in tasks {
        let _ = t.await;
    }
}

async fn play_one(socket: &Path, s: Session, opts: &Options, report: impl Fn(Event)) {
    for step in &s.steps {
        let wait = (step.after_ms as f64 / opts.speed.max(0.001)) as u64;
        tokio::time::sleep(Duration::from_millis(wait)).await;
        let name = step.input["hook_event_name"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        let msg = HookMessage {
            protocol: PROTOCOL_VERSION,
            tool: s.tool,
            env: s.env.clone(),
            input: step.input.clone(),
        };
        let Ok(line) = encode_line(&msg) else {
            continue;
        };
        let Ok(mut stream) = deck_ipc::connect(socket).await else {
            report(Event::NoDaemon);
            return;
        };
        if stream.write_all(line.as_bytes()).await.is_err() {
            continue;
        }
        report(Event::Sent {
            session: s.id.clone(),
            event: name.clone(),
        });
        if name != "PermissionRequest" {
            continue;
        }
        let mut reply = String::new();
        let read =
            tokio::time::timeout(opts.permission_wait, stream.read_to_string(&mut reply)).await;
        let output = match read {
            Ok(Ok(_)) => decode_line::<HookReply>(&reply).ok().and_then(|r| r.output),
            _ => None,
        };
        report(Event::Answered {
            session: s.id.clone(),
            output,
        });
    }
}
