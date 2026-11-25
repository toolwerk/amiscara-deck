#!/usr/bin/env python3
"""Tests for i18n.py: python3 scripts/i18n_test.py"""
import tempfile
import unittest
from pathlib import Path

import i18n

SWIFT = '''
// L10n.t("in a comment") is no text
struct A {
    var a: String { L10n.t("Allow") }
    var b: String { L10n.t("Hello {name}", ["name": n]) }
    var c: String { L10n.plural("{n} agent", "{n} agents", count) }
    static let d: LocalizedStringResource = /*i18n*/"Stay quiet"
    var e: String { L10n.t("say \\"hi\\"\\n") }
    var f: String { L10n.t("Allow") }
}
'''

RUST = '''
use deck_i18n::{t, t_with, plural};
pub fn t(x: &str) -> String { x.into() }
fn show() {
    let a = t("Settings");
    let b = t_with("running: {command}", &[("command", c)]);
    let c = deck_i18n::plural("{n} day", "{n} days", n, &[]);
    let d = "t(\\"not a call\\")";
    let e = text("command");
    let f = t("Usage:\\
  more");
}
'''

DE = '''msgid ""
msgstr ""
"Language: de\\n"

msgid "Allow"
msgstr "Erlauben"

msgid "Hello {name}"
msgstr "Hallo {name}"

msgid "{n} agent"
msgid_plural "{n} agents"
msgstr[0] "{n} Agent"
msgstr[1] "{n} Agenten"

msgid "Stay quiet"
msgstr "Still bleiben"

msgid "say \\"hi\\"\\n"
msgstr "sag \\"hallo\\"\\n"

msgid "Settings"
msgstr "Einstellungen"

msgid "running: {command}"
msgstr "führt aus: {command}"

msgid "{n} day"
msgid_plural "{n} days"
msgstr[0] "{n} Tag"
msgstr[1] "{n} Tage"

msgid "Usage:more"
msgstr "Aufruf:mehr"
'''


def tree(swift=SWIFT, rust=RUST, de=DE, template=True):
    d = Path(tempfile.mkdtemp())
    (d / "apps/AmiscaraDeck/AmiscaraDeck").mkdir(parents=True)
    (d / "apps/AmiscaraDeck/AmiscaraDeck/A.swift").write_text(swift)
    (d / "crates/deck-x/src").mkdir(parents=True)
    (d / "crates/deck-x/src/lib.rs").write_text(rust)
    (d / "crates/deck-i18n/src").mkdir(parents=True)
    (d / "crates/deck-i18n/src/lib.rs").write_text('fn x() { t("only a test of the catalog"); }')
    (d / "po").mkdir()
    (d / "po/de.po").write_text(de)
    if template:
        (d / "po/deck.pot").write_text(i18n.render_template(i18n.extract(d)))
    return d


class Extract(unittest.TestCase):
    def test_finds_every_marked_text_once(self):
        entries = i18n.extract(tree(template=False))
        got = [(e.msgid, e.plural) for e in entries]
        self.assertEqual(got, [
            ("Allow", None),
            ("Hello {name}", None),
            ("{n} agent", "{n} agents"),
            ("say \"hi\"\n", None),
            ("Stay quiet", None),
            ("Settings", None),
            ("running: {command}", None),
            ("{n} day", "{n} days"),
            ("Usage:more", None),
        ])

    def test_a_call_without_a_literal_fails(self):
        with self.assertRaises(i18n.Failure):
            i18n.extract(tree(swift='let x = L10n.t(key)', template=False))
        with self.assertRaises(i18n.Failure):
            i18n.extract(tree(rust='fn a() { t(&name); }', template=False))

    def test_a_call_inside_an_interpolation_fails(self):
        with self.assertRaises(i18n.Failure):
            i18n.extract(tree(swift='let x = "--- \\(L10n.t("further")) ---"', template=False))

    def test_interpolation_fails(self):
        with self.assertRaises(i18n.Failure):
            i18n.extract(tree(swift='let x = L10n.t("Hi \\(name)")', template=False))

    def test_a_plural_needs_two_literals(self):
        with self.assertRaises(i18n.Failure):
            i18n.extract(tree(swift='let x = L10n.plural("{n} a", other, 2)', template=False))

    def test_the_same_text_with_other_plurals_fails(self):
        swift = 'let a = L10n.t("{n} agent"); let b = L10n.plural("{n} agent", "{n} agents", 2)'
        with self.assertRaises(i18n.Failure):
            i18n.extract(tree(swift=swift, template=False))


class Po(unittest.TestCase):
    def test_quote_and_unquote_round_trip(self):
        for s in ['plain', 'a "b" \\ c', 'line\nnext\ttab', '⟦U+202E⟧ „x“']:
            self.assertEqual(i18n.unquote(i18n.quote(s), "t"), s)

    def test_continued_lines_and_fuzzy_are_read(self):
        entries = i18n.parse_po('msgid ""\n"a"\n"b"\n#, fuzzy\nmsgstr "x"\n"y"\n', "t")
        self.assertEqual((entries[0].msgid, entries[0].msgstr), ("ab", ["xy"]))
        self.assertTrue(entries[0].fuzzy)

    def test_the_template_reads_back(self):
        d = tree()
        back = i18n.parse_po((d / "po/deck.pot").read_text(), "t")
        self.assertEqual([e.msgid for e in back], [e.msgid for e in i18n.extract(d)])


class Check(unittest.TestCase):
    def test_a_complete_tree_passes(self):
        self.assertEqual(i18n.check(tree()), [])

    def test_a_stale_template_fails(self):
        d = tree()
        (d / "apps/AmiscaraDeck/AmiscaraDeck/B.swift").write_text('let x = L10n.t("New text")')
        found = i18n.check(d)
        self.assertTrue(any("does not match the code" in f for f in found), found)
        self.assertTrue(any("missing 'New text'" in f for f in found) is False, "the template decides")

    def test_a_missing_text_fails(self):
        found = i18n.check(tree(de=DE.replace('msgid "Allow"\nmsgstr "Erlauben"\n', "")))
        self.assertEqual(found, ["po/de.po: missing 'Allow'"])

    def test_an_empty_or_fuzzy_text_fails(self):
        found = i18n.check(tree(de=DE.replace('msgstr "Erlauben"', 'msgstr ""')))
        self.assertEqual(found, ["po/de.po: 'Allow' is not translated"])
        found = i18n.check(tree(de=DE.replace('msgid "Allow"', '#, fuzzy\nmsgid "Allow"')))
        self.assertEqual(found, ["po/de.po: 'Allow' is marked fuzzy"])
        found = i18n.check(tree(de=DE.replace('msgstr[1] "{n} Agenten"', 'msgstr[1] ""')))
        self.assertEqual(found, ["po/de.po: '{n} agent' is not translated"])

    def test_a_text_not_in_the_template_fails(self):
        found = i18n.check(tree(de=DE + '\nmsgid "Gone"\nmsgstr "Weg"\n'))
        self.assertEqual(found, ["po/de.po: 'Gone' is not in the template"])

    def test_other_placeholders_fail(self):
        found = i18n.check(tree(de=DE.replace('"Hallo {name}"', '"Hallo {nme}"')))
        self.assertEqual(len(found), 1)
        self.assertIn("placeholders", found[0])
        found = i18n.check(tree(de=DE.replace('msgstr[0] "{n} Agent"', 'msgstr[0] "ein Agent"')))
        self.assertEqual(len(found), 1, found)

    def test_a_missing_template_fails(self):
        self.assertTrue(i18n.check(tree(template=False)))


class Apple(unittest.TestCase):
    def test_writes_english_and_german_tables(self):
        d = tree()
        out = d / "out"
        i18n.apple(out, d)
        en = (out / "en.lproj/Localizable.strings").read_text()
        de = (out / "de.lproj/Localizable.strings").read_text()
        self.assertIn('"Allow" = "Allow";', en)
        self.assertIn('"Allow" = "Erlauben";', de)
        self.assertIn('"{n} agents" = "{n} Agenten";', de)
        self.assertIn('"{n} agent" = "{n} Agent";', de)
        self.assertIn('"say \\"hi\\"\\n" = "sag \\"hallo\\"\\n";', de)

    def test_a_missing_translation_stays_english(self):
        d = tree(de=DE.replace('msgstr "Erlauben"', 'msgstr ""'))
        i18n.apple(d / "out", d)
        self.assertIn('"Allow" = "Allow";', (d / "out/de.lproj/Localizable.strings").read_text())

    def test_one_key_with_two_translations_fails(self):
        swift = 'let a = L10n.t("{n} agents"); let b = L10n.plural("{n} agent", "{n} agents", 2)'
        de = ('msgid "{n} agents"\nmsgstr "{n} Leute"\n\n'
              'msgid "{n} agent"\nmsgid_plural "{n} agents"\nmsgstr[0] "{n} Agent"\nmsgstr[1] "{n} Agenten"\n')
        d = tree(swift=swift, rust="", de=de)
        found = i18n.check(d)
        self.assertTrue(any("two different translations" in f for f in found), found)


class Repository(unittest.TestCase):
    def test_this_repository_passes(self):
        self.assertEqual(i18n.check(), [])


if __name__ == "__main__":
    unittest.main()
