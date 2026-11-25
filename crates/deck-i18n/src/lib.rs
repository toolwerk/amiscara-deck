//! Texts for people in the language of the user: English in the code, German
//! from `po/de.po`, which is built in. The template `po/deck.pot` and the check
//! that nothing is missing come from `scripts/i18n.py`.
//!
//! The language comes from the environment, like other command line programs:
//! the first value of `DECK_LANGUAGE`, `LC_ALL`, `LC_MESSAGES` and `LANG` that is
//! set and not `C` or `POSIX`. Starting with `de` means German, everything else
//! English. A text without translation stays English.

use std::cell::Cell;
use std::collections::HashMap;
use std::sync::OnceLock;

/// The German translation, read once.
const DE_PO: &str = include_str!("../../../po/de.po");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    De,
}

impl Language {
    /// The language a locale name such as `de_DE.UTF-8` stands for.
    pub fn from_locale(value: &str) -> Language {
        if value.to_ascii_lowercase().starts_with("de") {
            Language::De
        } else {
            Language::En
        }
    }
}

thread_local! {
    static OVERRIDE: Cell<Option<Language>> = const { Cell::new(None) };
}

/// The language from the given variables, in the order they count.
pub fn language_from(vars: &[Option<String>]) -> Language {
    vars.iter()
        .flatten()
        .map(|v| v.trim())
        .find(|v| {
            !v.is_empty()
                && !v.eq_ignore_ascii_case("C")
                && !v.eq_ignore_ascii_case("POSIX")
                && !v.starts_with("C.")
        })
        .map(Language::from_locale)
        .unwrap_or(Language::En)
}

/// The language of this program: an override of this thread (tests), else
/// the environment, read once.
pub fn language() -> Language {
    if let Some(l) = OVERRIDE.with(Cell::get) {
        return l;
    }
    static ENV: OnceLock<Language> = OnceLock::new();
    *ENV.get_or_init(|| {
        let vars: Vec<Option<String>> = ["DECK_LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .map(|k| std::env::var(k).ok())
            .collect();
        language_from(&vars)
    })
}

/// Run `f` with `lang` on this thread, whatever the environment says.
pub fn with_language<R>(lang: Language, f: impl FnOnce() -> R) -> R {
    let before = OVERRIDE.with(|o| o.replace(Some(lang)));
    let out = f();
    OVERRIDE.with(|o| o.set(before));
    out
}

/// One translated text: the singular, and for a plural also the plural.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Translation {
    one: String,
    other: Option<String>,
}

fn german() -> &'static HashMap<String, Translation> {
    static DE: OnceLock<HashMap<String, Translation>> = OnceLock::new();
    DE.get_or_init(|| parse_po(DE_PO))
}

/// Read a quoted po string: `"a\"b\n"`. `None` if it is not one.
fn unquote(s: &str) -> Option<String> {
    let body = s.trim().strip_prefix('"')?.strip_suffix('"')?;
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

#[derive(Clone, Copy)]
enum Field {
    Id,
    Plural,
    Str(usize),
}

/// One entry of a po file as written: maybe empty, maybe fuzzy.
struct Entry {
    id: String,
    plural: Option<String>,
    strs: Vec<String>,
    fuzzy: bool,
}

/// Every entry of a po file, header included. Entries end at a line that is
/// empty or holds only spaces or a carriage return, as `scripts/i18n.py` reads
/// them, so a file that passes its check reads the same here.
fn entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim).chain(std::iter::once("")) {
        if !line.is_empty() {
            block.push(line);
            continue;
        }
        if block.is_empty() {
            continue;
        }
        let mut fuzzy = false;
        let mut id: Option<String> = None;
        let mut plural: Option<String> = None;
        let mut strs: Vec<String> = Vec::new();
        let mut field: Option<Field> = None;
        for line in block.drain(..) {
            if line.starts_with("#,") && line.contains("fuzzy") {
                fuzzy = true;
                continue;
            }
            if line.starts_with('#') {
                continue;
            }
            let (target, rest) = if let Some(r) = line.strip_prefix("msgid_plural ") {
                (Some(Field::Plural), r)
            } else if let Some(r) = line.strip_prefix("msgid ") {
                (Some(Field::Id), r)
            } else if let Some(r) = line.strip_prefix("msgstr[") {
                let Some((n, r)) = r.split_once("] ") else {
                    continue;
                };
                (n.parse().ok().map(Field::Str), r)
            } else if let Some(r) = line.strip_prefix("msgstr ") {
                (Some(Field::Str(0)), r)
            } else {
                (None, line)
            };
            let Some(value) = unquote(rest) else {
                continue;
            };
            if target.is_some() {
                field = target;
            }
            match field {
                Some(Field::Id) => id.get_or_insert_with(String::new).push_str(&value),
                Some(Field::Plural) => plural.get_or_insert_with(String::new).push_str(&value),
                Some(Field::Str(n)) => {
                    if strs.len() <= n {
                        strs.resize(n + 1, String::new());
                    }
                    strs[n].push_str(&value);
                }
                None => {}
            }
        }
        if let Some(id) = id {
            out.push(Entry {
                id,
                plural,
                strs,
                fuzzy,
            });
        }
    }
    out
}

/// The entries of a po file that are translated and not fuzzy. Anything it
/// cannot read is left out: the text then stays English.
fn parse_po(text: &str) -> HashMap<String, Translation> {
    let mut out = HashMap::new();
    for Entry {
        id,
        plural,
        mut strs,
        fuzzy,
    } in entries(text)
    {
        if id.is_empty() || fuzzy || strs.is_empty() || strs.iter().any(String::is_empty) {
            continue;
        }
        let translation = match plural {
            None => Translation {
                one: strs.swap_remove(0),
                other: None,
            },
            Some(_) if strs.len() >= 2 => Translation {
                other: Some(strs.swap_remove(1)),
                one: strs.swap_remove(0),
            },
            Some(_) => continue,
        };
        out.insert(id, translation);
    }
    out
}

/// `{name}` replaced by its value. An unknown placeholder stays as it is.
fn fill(text: &str, args: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('}') {
            Some(end) => {
                let name = &after[..end];
                match args.iter().find(|(k, _)| *k == name) {
                    Some((_, v)) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(name);
                        out.push('}');
                    }
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

fn lookup(lang: Language, msgid: &str) -> Option<&'static Translation> {
    match lang {
        Language::En => None,
        Language::De => german().get(msgid),
    }
}

/// The text in the language of the user.
pub fn t(msgid: &str) -> String {
    t_with(msgid, &[])
}

/// The text with its placeholders filled: `t_with("running: {command}", &[("command", c)])`.
pub fn t_with(msgid: &str, args: &[(&str, &str)]) -> String {
    let text = lookup(language(), msgid).map_or(msgid, |t| t.one.as_str());
    fill(text, args)
}

/// The singular for `n == 1`, else the plural; `{n}` is the number.
pub fn plural(one: &str, other: &str, n: u64, args: &[(&str, &str)]) -> String {
    let found = lookup(language(), one);
    let text = if n == 1 {
        found.map_or(one, |t| t.one.as_str())
    } else {
        found.and_then(|t| t.other.as_deref()).unwrap_or(other)
    };
    let count = n.to_string();
    let mut all: Vec<(&str, &str)> = vec![("n", &count)];
    all.extend_from_slice(args);
    fill(text, &all)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(v: &[Option<&str>]) -> Vec<Option<String>> {
        v.iter().map(|o| o.map(str::to_owned)).collect()
    }

    #[test]
    fn the_first_set_variable_decides() {
        assert_eq!(
            language_from(&vars(&[None, None, None, Some("de_DE.UTF-8")])),
            Language::De
        );
        assert_eq!(
            language_from(&vars(&[Some("en"), Some("de_DE.UTF-8")])),
            Language::En
        );
        assert_eq!(
            language_from(&vars(&[Some(""), Some("C"), Some("POSIX"), Some("de_AT")])),
            Language::De,
            "empty, C and POSIX say nothing"
        );
        assert_eq!(
            language_from(&vars(&[Some("C.UTF-8"), Some("de")])),
            Language::De
        );
        assert_eq!(language_from(&vars(&[Some("fr_FR.UTF-8")])), Language::En);
        assert_eq!(language_from(&vars(&[None, None])), Language::En);
    }

    const SAMPLE: &str = r#"
msgid ""
msgstr ""
"Content-Type: text/plain; charset=UTF-8\n"

#: a.rs
msgid "Settings"
msgstr "Einstellungen"

msgid "say \"hi\"\n"
"again"
msgstr "sag \"hallo\"\n"
"nochmal"

msgid "{n} agent"
msgid_plural "{n} agents"
msgstr[0] "{n} Agent"
msgstr[1] "{n} Agenten"

#, fuzzy
msgid "Quit"
msgstr "Beenden"

msgid "Open"
msgstr ""
"#;

    #[test]
    fn po_entries_are_read_with_escapes_and_plurals() {
        let po = parse_po(SAMPLE);
        assert_eq!(po["Settings"].one, "Einstellungen");
        assert_eq!(po["say \"hi\"\nagain"].one, "sag \"hallo\"\nnochmal");
        assert_eq!(po["{n} agent"].other.as_deref(), Some("{n} Agenten"));
        assert!(!po.contains_key("Quit"), "fuzzy counts as missing");
        assert!(!po.contains_key("Open"), "empty counts as missing");
        assert!(!po.contains_key(""), "the header is no text");
    }

    #[test]
    fn entries_end_at_lines_with_only_spaces_or_a_carriage_return() {
        let po = parse_po(
            "msgid \"A\"\r\nmsgstr \"Ä\"\r\n \r\nmsgid \"B\"\nmsgstr \"Bé\"\n\t\nmsgid \"C\"\nmsgstr \"Cé\"",
        );
        assert_eq!(po["A"].one, "Ä");
        assert_eq!(po["B"].one, "Bé");
        assert_eq!(po["C"].one, "Cé");
    }

    /// What `scripts/i18n.py check` passes, this reader finds as well: every
    /// text of the template has its German translation here.
    #[test]
    fn every_text_of_the_template_is_translated_here() {
        let template = entries(include_str!("../../../po/deck.pot"));
        let ids: Vec<String> = template
            .into_iter()
            .map(|e| e.id)
            .filter(|i| !i.is_empty())
            .collect();
        assert!(ids.len() > 200, "{}", ids.len());
        let missing: Vec<&String> = ids.iter().filter(|i| !german().contains_key(*i)).collect();
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_stay() {
        assert_eq!(
            fill("running: {command}", &[("command", "ls")]),
            "running: ls"
        );
        assert_eq!(fill("{a} {b}", &[("a", "1")]), "1 {b}");
        assert_eq!(fill("open { brace", &[]), "open { brace");
        assert_eq!(
            fill("{a}", &[("a", "{b}"), ("b", "x")]),
            "{b}",
            "a value is not filled again"
        );
    }

    #[test]
    fn the_built_in_german_is_read() {
        assert!(!german().is_empty());
        with_language(Language::De, || {
            assert_eq!(t("Settings"), "Einstellungen");
            assert_eq!(
                t_with("running: {command}", &[("command", "make pdf")]),
                "führt aus: make pdf"
            );
            assert_eq!(plural("{n} agent", "{n} agents", 1, &[]), "1 Agent");
            assert_eq!(plural("{n} agent", "{n} agents", 2, &[]), "2 Agenten");
        });
    }

    #[test]
    fn english_is_the_text_itself_and_unknown_texts_stay() {
        with_language(Language::En, || {
            assert_eq!(t("Settings"), "Settings");
            assert_eq!(plural("{n} agent", "{n} agents", 1, &[]), "1 agent");
            assert_eq!(plural("{n} agent", "{n} agents", 3, &[]), "3 agents");
        });
        with_language(Language::De, || {
            assert_eq!(t("no such text here"), "no such text here");
            assert_eq!(
                plural("{n} no such", "{n} no such things", 2, &[]),
                "2 no such things"
            );
        });
    }

    #[test]
    fn the_override_is_undone() {
        let outer = language();
        with_language(Language::De, || {
            assert_eq!(language(), Language::De);
            with_language(Language::En, || assert_eq!(language(), Language::En));
            assert_eq!(language(), Language::De);
        });
        assert_eq!(language(), outer);
    }
}
