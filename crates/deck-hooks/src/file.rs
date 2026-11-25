//! Reading and replacing the settings file safely.

use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Serializer, Value, ser::PrettyFormatter};

use crate::Error;

/// How many backups are kept next to the file.
const KEEP_BACKUPS: usize = 5;
/// How often an edit starts over when the file changes under it.
const RETRIES: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub changed: bool,
    /// The copy of the old file made before the change.
    pub backup: Option<PathBuf>,
}

fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Error {
    let context = context.into();
    move |source| Error::Io { context, source }
}

pub fn is_executable(meta: &std::fs::Metadata) -> bool {
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
}

/// The file behind a symlink, so the link itself stays a link.
fn real_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// Parse the file. A missing or empty file is an empty object.
pub fn read(path: &Path) -> Result<(Value, bool), Error> {
    let (text, existed) = read_text(path)?;
    Ok((parse(path, &text)?, existed))
}

fn read_text(path: &Path) -> Result<(String, bool), Error> {
    match std::fs::read_to_string(real_path(path)) {
        Ok(t) => Ok((t, true)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((String::new(), false)),
        Err(e) => Err(io(format!("read {}", path.display()))(e)),
    }
}

fn parse(path: &Path, text: &str) -> Result<Value, Error> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    if text.trim().is_empty() {
        return Ok(Value::Object(Default::default()));
    }
    serde_json::from_str(text).map_err(|source| Error::Invalid {
        path: path.to_owned(),
        source,
    })
}

/// The indentation the file uses, so a change does not rewrite every line.
fn indent_of(text: &str) -> String {
    text.lines()
        .find_map(|l| {
            let trimmed = l.trim_start_matches([' ', '\t']);
            (trimmed.len() < l.len() && trimmed.starts_with('"'))
                .then(|| l[..l.len() - trimmed.len()].to_owned())
        })
        .unwrap_or_else(|| "  ".to_owned())
}

fn render(value: &Value, indent: &str) -> String {
    let mut buf = Vec::new();
    let fmt = PrettyFormatter::with_indent(indent.as_bytes());
    let mut ser = Serializer::with_formatter(&mut buf, fmt);
    // Serializing a `Value` cannot fail.
    serde::Serialize::serialize(value, &mut ser).expect("serialize Value");
    let mut s = String::from_utf8(buf).expect("JSON is UTF-8");
    s.push('\n');
    s
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Read, change with `f`, write back if it differs. If the file changes
/// between reading and replacing (Claude Code writes it too), start over.
pub fn edit(path: &Path, f: impl Fn(&mut Value) -> Result<(), Error>) -> Result<Outcome, Error> {
    let target = real_path(path);
    for _ in 0..RETRIES {
        let (text, existed) = read_text(path)?;
        let mut root = parse(path, &text)?;
        f(&mut root)?;
        let new = render(&root, &indent_of(&text));
        if existed && new == text {
            return Ok(Outcome {
                changed: false,
                backup: None,
            });
        }
        if !existed && root.as_object().is_some_and(|o| o.is_empty()) {
            return Ok(Outcome {
                changed: false,
                backup: None,
            });
        }
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(io(format!("create {}", dir.display())))?;
        }
        let backup = if existed {
            Some(backup(&target, &text)?)
        } else {
            None
        };
        let tmp = target.with_file_name(format!(
            ".{}.deck-tmp-{}",
            target
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("settings"),
            std::process::id()
        ));
        let mode = std::fs::metadata(&target)
            .map(|m| m.permissions().mode() & 0o777)
            .unwrap_or(0o644);
        let result = (|| -> Result<(), Error> {
            // Private from the first byte: the file may hold tokens. `create_new`
            // also refuses a file or link that someone put at this name.
            let _ = std::fs::remove_file(&tmp);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(mode)
                .open(&tmp)
                .map_err(io(format!("create {}", tmp.display())))?;
            // The mode given on creation is cut by the umask; set it exactly.
            file.set_permissions(std::fs::Permissions::from_mode(mode))
                .map_err(io(format!("chmod {}", tmp.display())))?;
            file.write_all(new.as_bytes()).map_err(io("write"))?;
            file.sync_all().map_err(io("sync"))?;
            // Someone else wrote in the meantime: do not overwrite that.
            let (now, _) = read_text(path)?;
            if now != text {
                return Err(Error::Changed {
                    path: path.to_owned(),
                });
            }
            std::fs::rename(&tmp, &target).map_err(io(format!("replace {}", target.display())))
        })();
        match result {
            Ok(()) => {
                return Ok(Outcome {
                    changed: true,
                    backup,
                });
            }
            Err(Error::Changed { .. }) => {
                let _ = std::fs::remove_file(&tmp);
                if let Some(b) = backup {
                    let _ = std::fs::remove_file(b);
                }
            }
            Err(e) => {
                let _ = std::fs::remove_file(&tmp);
                return Err(e);
            }
        }
    }
    Err(Error::Changed {
        path: path.to_owned(),
    })
}

fn backup(target: &Path, text: &str) -> Result<PathBuf, Error> {
    let name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json");
    let prefix = format!("{name}.deck-");
    let dir = target.parent().unwrap_or(Path::new("."));
    // Seconds are not enough when two edits come within one second.
    let mut n = 0;
    let path = loop {
        let p = dir.join(format!("{prefix}{}-{n}.bak", now_secs()));
        if !p.exists() {
            break p;
        }
        n += 1;
    };
    let mode = std::fs::metadata(target)
        .map(|m| m.permissions().mode() & 0o777)
        .unwrap_or(0o600);
    // Same rights as the file from the first byte (it may hold tokens), and
    // never over an existing file or link.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&path)
        .map_err(io(format!("create {}", path.display())))?;
    file.set_permissions(std::fs::Permissions::from_mode(mode))
        .map_err(io(format!("chmod {}", path.display())))?;
    file.write_all(text.as_bytes())
        .map_err(io(format!("write {}", path.display())))?;
    file.sync_all()
        .map_err(io(format!("sync {}", path.display())))?;
    prune(dir, &prefix);
    Ok(path)
}

/// Keep the newest backups. Names sort by time because the time comes first.
fn prune(dir: &Path, prefix: &str) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let mut old: Vec<(u64, u64, PathBuf)> = read
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let rest = name.strip_prefix(prefix)?.strip_suffix(".bak")?;
            let (secs, n) = rest.split_once('-')?;
            Some((secs.parse().ok()?, n.parse().ok()?, e.path()))
        })
        .collect();
    old.sort();
    let excess = old.len().saturating_sub(KEEP_BACKUPS);
    for (_, _, p) in old.into_iter().take(excess) {
        let _ = std::fs::remove_file(p);
    }
}
