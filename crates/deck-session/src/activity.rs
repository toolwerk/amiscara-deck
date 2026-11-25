//! What an agent does right now, as data for the UIs.

use deck_proto::agent::Activity;
use serde_json::Value;

const MAX: usize = 80;

fn shorten(s: &str) -> String {
    let one_line = s.lines().next().unwrap_or("").trim();
    if one_line.chars().count() > MAX {
        let cut: String = one_line.chars().take(MAX - 1).collect();
        format!("{cut}…")
    } else {
        one_line.to_owned()
    }
}

/// Path shown relative to the working folder when it lies inside.
fn relative(path: &str, cwd: Option<&str>) -> String {
    match cwd {
        Some(c) if !c.is_empty() => path
            .strip_prefix(c)
            .map(|p| p.trim_start_matches('/'))
            .filter(|p| !p.is_empty())
            .unwrap_or(path)
            .to_owned(),
        _ => path.to_owned(),
    }
}

/// The first file a patch of Codex (`apply_patch`) adds, changes or deletes.
fn patch_path(patch: &str) -> Option<&str> {
    patch.lines().find_map(|l| {
        ["*** Add File: ", "*** Update File: ", "*** Delete File: "]
            .iter()
            .find_map(|p| l.strip_prefix(p))
            .map(str::trim)
            .filter(|p| !p.is_empty())
    })
}

/// What a tool call does, as data: the UI words it in its own language.
pub fn describe_tool(tool: &str, input: &Value, cwd: Option<&str>) -> Activity {
    let text = |k: &str| input.get(k).and_then(Value::as_str);
    match tool {
        "Bash" => Activity::Run {
            command: text("command").map(shorten),
        },
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" | "apply_patch" => Activity::Edit {
            path: text("file_path")
                .or(text("notebook_path"))
                .or_else(|| text("command").and_then(patch_path))
                .map(|p| relative(p, cwd)),
        },
        "Read" => Activity::Read {
            path: text("file_path").map(|p| relative(p, cwd)),
        },
        "Grep" | "Glob" => Activity::Search {
            pattern: text("pattern").map(shorten),
        },
        "Agent" | "Task" => Activity::Subagent,
        "WebFetch" | "WebSearch" => Activity::Web,
        other => Activity::Tool {
            name: other.to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_codex_patch_is_an_edit_of_its_first_file() {
        let patch = "*** Begin Patch\n*** Update File: src/a.rs\n@@\n-x\n+y\n*** Add File: b.txt\n*** End Patch";
        assert_eq!(
            describe_tool("apply_patch", &json!({ "command": patch }), Some("/w")),
            Activity::Edit {
                path: Some("src/a.rs".into())
            }
        );
        assert_eq!(
            describe_tool(
                "apply_patch",
                &json!({ "command": "*** Begin Patch\n*** Delete File: /w/old.txt\n*** End Patch" }),
                Some("/w")
            ),
            Activity::Edit {
                path: Some("old.txt".into())
            }
        );
        assert_eq!(
            describe_tool("apply_patch", &json!({ "command": "no patch" }), None),
            Activity::Edit { path: None }
        );
    }
}
