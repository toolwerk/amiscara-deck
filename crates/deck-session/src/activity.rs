//! Short German texts for what an agent does right now.

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

/// "bearbeitet src/import.rs", "führt aus: cargo test", ...
pub fn describe_tool(tool: &str, input: &Value, cwd: Option<&str>) -> String {
    let text = |k: &str| input.get(k).and_then(Value::as_str);
    match tool {
        "Bash" => match text("command") {
            Some(c) => format!("führt aus: {}", shorten(c)),
            None => "führt einen Befehl aus".into(),
        },
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            match text("file_path").or(text("notebook_path")) {
                Some(p) => format!("bearbeitet {}", relative(p, cwd)),
                None => "bearbeitet eine Datei".into(),
            }
        }
        "Read" => match text("file_path") {
            Some(p) => format!("liest {}", relative(p, cwd)),
            None => "liest eine Datei".into(),
        },
        "Grep" | "Glob" => match text("pattern") {
            Some(p) => format!("sucht {}", shorten(p)),
            None => "sucht".into(),
        },
        "Agent" | "Task" => "startet einen Subagenten".into(),
        "WebFetch" | "WebSearch" => "ruft das Netz ab".into(),
        other => other.to_owned(),
    }
}
