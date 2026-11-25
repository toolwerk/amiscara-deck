//! Text that comes from outside (commands, paths, branch names, activities) must
//! not be able to look like something else when a UI shows it.

/// Characters that can make text look different from what it is. By Unicode
/// category, not by a list: control characters (a lone carriage return, escape,
/// backspace, tab), format characters (bidi controls, zero-width characters,
/// the tag block), line and paragraph separators, private use, unassigned code
/// points and every space other than the plain one. Plus a few invisible
/// letters and marks that no category covers: Hangul fillers, the blank Braille
/// pattern, the combining grapheme joiner, variation selectors, Mongolian
/// free variation selectors. A line feed is not deceptive, it is a line break.
pub fn is_deceptive(c: char) -> bool {
    use unicode_general_category::{GeneralCategory as G, get_general_category};
    if c == '\n' {
        return false;
    }
    match get_general_category(c) {
        G::Control
        | G::Format
        | G::LineSeparator
        | G::ParagraphSeparator
        | G::PrivateUse
        | G::Surrogate
        | G::Unassigned => true,
        G::SpaceSeparator => c != ' ',
        _ => matches!(
            c,
            '\u{034F}'
                | '\u{0334}'..='\u{0338}' // combining overlays strike text through
                | '\u{115F}'
                | '\u{1160}'
                | '\u{17B4}'..='\u{17B5}'
                | '\u{180B}'..='\u{180D}'
                | '\u{180F}'
                | '\u{20D2}'..='\u{20D3}'
                | '\u{20D8}'..='\u{20DA}'
                | '\u{2800}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FFA0}'
                | '\u{E0100}'..='\u{E01EF}'
        ),
    }
}

/// Opening mark of an escaped character: `⟦U+202E⟧`. A literal `⟦` in the
/// input is shown as `⟦⟦`, so what the user types and what Deck escapes can
/// never look the same (the escaping is injective).
pub const MARK_OPEN: char = '⟦';

/// Make deceptive characters visible as `⟦U+202E⟧`.
pub fn show_deceptive(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if is_deceptive(c) {
            out.push_str(&format!("{MARK_OPEN}U+{:04X}⟧", c as u32));
        } else if c == MARK_OPEN {
            out.push(MARK_OPEN);
            out.push(MARK_OPEN);
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_text_is_not_deceptive() {
        assert!(
            !"echo Grüße aus Neuendettelsau ~/dev/app-1.0"
                .chars()
                .any(is_deceptive)
        );
        assert!(!is_deceptive('\n'), "a line break is a line break");
    }

    #[test]
    fn hidden_characters_are_escaped_and_a_typed_mark_is_doubled() {
        assert_eq!(show_deceptive("a\u{202e}b"), "a⟦U+202E⟧b");
        assert_eq!(show_deceptive("⟦U+202E⟧"), "⟦⟦U+202E⟧");
    }
}
