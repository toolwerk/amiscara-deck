//! Just enough shell to write a command with a path in it and to find the
//! program again in a command that was written earlier.

/// The path as one shell word: left alone if it is plain, else in single
/// quotes. On Windows in double quotes, which both Git Bash and `cmd` read.
pub fn quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_./-+=:@%".contains(c))
    {
        return s.to_owned();
    }
    if cfg!(windows) {
        // A Windows path holds no `"`; `$` and `` ` `` would expand in bash.
        // Claude Code runs hooks through Git Bash on Windows, so the escapes
        // are bash escapes; `cmd` would keep the backslash.
        let escaped: String = s
            .chars()
            .flat_map(|c| match c {
                '"' | '$' | '`' | '\\' => vec!['\\', c],
                c => vec![c],
            })
            .collect();
        return format!("\"{escaped}\"");
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// The first word of a command line with quotes resolved, as a shell would
/// read it. `None` if the line is empty or a quote is not closed.
pub fn first_word(command: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = command.trim_start().chars().peekable();
    let mut any = false;
    while let Some(&c) = chars.peek() {
        match c {
            c if c.is_whitespace() => break,
            '\'' => {
                chars.next();
                loop {
                    match chars.next()? {
                        '\'' => break,
                        c => out.push(c),
                    }
                }
                any = true;
            }
            '"' => {
                chars.next();
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => out.push(chars.next()?),
                        c => out.push(c),
                    }
                }
                any = true;
            }
            '\\' => {
                chars.next();
                out.push(chars.next()?);
                any = true;
            }
            c => {
                chars.next();
                out.push(c);
                any = true;
            }
        }
    }
    any.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn windows_paths_are_quoted_for_bash_and_cmd_alike() {
        assert_eq!(quote("C:/x/deck-hook.exe"), "C:/x/deck-hook.exe");
        assert_eq!(
            quote("C:/Users/A B/deck-hook.exe"),
            "\"C:/Users/A B/deck-hook.exe\""
        );
    }

    #[test]
    #[cfg(not(windows))]
    fn plain_paths_stay_plain_and_others_are_quoted() {
        assert_eq!(
            quote("/usr/local/bin/deck-hook"),
            "/usr/local/bin/deck-hook"
        );
        assert_eq!(
            quote("/Applications/Amiscara Deck.app/x"),
            "'/Applications/Amiscara Deck.app/x'"
        );
        assert_eq!(quote("/a/it's"), "'/a/it'\\''s'");
    }

    #[test]
    fn quote_and_first_word_are_inverse() {
        for p in [
            "/plain/deck-hook",
            "/with space/deck hook",
            "/it's/here",
            "/dollar$/`x`",
            "/tab\t/x",
            "/\"q\"/x",
        ] {
            assert_eq!(
                first_word(&format!("{} codex", quote(p))).as_deref(),
                Some(p),
                "{p}"
            );
            assert_eq!(first_word(&quote(p)).as_deref(), Some(p), "{p}");
        }
    }

    #[test]
    fn first_word_reads_what_a_person_would_write() {
        assert_eq!(first_word("  /a/b --flag").as_deref(), Some("/a/b"));
        assert_eq!(first_word("\"/a b/c\" x").as_deref(), Some("/a b/c"));
        assert_eq!(first_word("'/a b/c'x y").as_deref(), Some("/a b/cx"));
        assert_eq!(first_word("/a\\ b/c").as_deref(), Some("/a b/c"));
        assert_eq!(first_word(""), None);
        assert_eq!(first_word("   "), None);
        assert_eq!(first_word("'unclosed"), None);
    }
}
