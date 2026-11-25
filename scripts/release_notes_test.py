#!/usr/bin/env python3
"""Tests for release-notes.py: python3 scripts/release_notes_test.py"""
import contextlib
import importlib.util
import io
import tempfile
import unittest
from pathlib import Path
from unittest import mock

spec = importlib.util.spec_from_file_location("rn", Path(__file__).with_name("release-notes.py"))
rn = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rn)

LOG = """# Changelog

## [Unreleased]

### Added

- New thing

## [0.1.0] - 2026-01-01

- First
"""

LOG_EN = """# Changelog

Older versions are German only.

## [Unreleased]

### Added

- New thing, in English
"""


class Notes(unittest.TestCase):
    def test_a_version_has_its_own_section(self):
        self.assertEqual(rn.notes(LOG, "0.1.0"), "- First\n")

    def test_before_the_cut_the_unreleased_section_counts(self):
        self.assertIn("- New thing", rn.notes(LOG, "0.2.0"))
        self.assertNotIn("First", rn.notes(LOG, "0.2.0"))

    def test_nothing_ships_without_notes(self):
        with self.assertRaises(SystemExit):
            rn.notes("## [Unreleased]\n\n## [0.1.0]\n- x\n", "0.2.0")

    def test_cut_moves_the_notes_under_the_version(self):
        out = rn.cut(LOG, "0.2.0", "2026-02-02")
        self.assertIn("## [Unreleased]\n\n## [0.2.0] - 2026-02-02\n\n### Added", out)
        self.assertEqual(rn.notes(out, "0.2.0"), "### Added\n\n- New thing\n")

    def test_cut_refuses_nonsense_and_repeats(self):
        for version, date in [("0.2", "2026-02-02"), ("0.2.0", "today"), ("0.1.0", "2026-02-02")]:
            with self.assertRaises(SystemExit):
                rn.cut(LOG, version, date)


class Both(unittest.TestCase):
    def test_both_come_from_unreleased_before_the_cut(self):
        de, en = rn.both(LOG, LOG_EN, "0.2.0")
        self.assertEqual(de, "### Added\n\n- New thing\n")
        self.assertEqual(en, "### Added\n\n- New thing, in English\n")

    def test_a_missing_english_section_does_not_fall_back(self):
        de = rn.cut(LOG, "0.2.0", "2026-02-02")
        with self.assertRaises(SystemExit) as stop:
            rn.both(de, LOG_EN, "0.2.0")
        self.assertIn("CHANGELOG.en.md", str(stop.exception))

    def test_an_empty_version_names_its_file(self):
        empty = "## [Unreleased]\n"
        for de, en, file in [(empty, LOG_EN, "CHANGELOG.md"), (LOG, empty, "CHANGELOG.en.md")]:
            with self.assertRaises(SystemExit) as stop:
                rn.both(de, en, "0.2.0")
            self.assertTrue(str(stop.exception).endswith(file), stop.exception)


class Check(unittest.TestCase):
    def test_matching_files_pass(self):
        self.assertEqual(rn.check(LOG, LOG_EN), [])

    def test_empty_on_both_sides_is_fine(self):
        self.assertEqual(rn.check("## [Unreleased]\n", "## [Unreleased]\n"), [])

    def test_cut_files_still_match(self):
        de = rn.cut(LOG, "0.2.0", "2026-02-02")
        en = rn.cut(LOG_EN, "0.2.0", "2026-02-02")
        self.assertEqual(rn.check(de, en), [])

    def test_a_missing_entry_is_found(self):
        de = LOG.replace("- New thing\n", "- New thing\n- Another\n")
        self.assertEqual(rn.check(de, LOG_EN),
                         ["[Unreleased] Added: 2 entries in CHANGELOG.md, 1 in CHANGELOG.en.md"])

    def test_missing_and_extra_sections_are_found(self):
        more = "- New thing\n\n### Fixed\n\n- A bug\n"
        self.assertEqual(rn.check(LOG.replace("- New thing\n", more), LOG_EN),
                         ["CHANGELOG.en.md: [Unreleased] Fixed is missing"])
        self.assertEqual(rn.check(LOG, LOG_EN.replace("- New thing, in English\n", more)),
                         ["CHANGELOG.md: [Unreleased] Fixed is missing"])

    def test_versions_and_their_headings_must_match(self):
        de = rn.cut(LOG, "0.2.0", "2026-02-02")
        self.assertIn("CHANGELOG.en.md: [0.2.0] is missing", rn.check(de, LOG_EN))
        en = rn.cut(LOG_EN, "0.2.0", "2026-02-03")
        self.assertEqual(len(rn.check(de, en)), 1)
        self.assertIn("CHANGELOG.md: [0.2.0] is missing", rn.check(LOG, en))

    def test_unreleased_must_exist(self):
        self.assertIn("CHANGELOG.en.md: [Unreleased] is missing", rn.check(LOG, "# Changelog\n"))


class Command(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.de = Path(folder.name) / "CHANGELOG.md"
        self.en = Path(folder.name) / "CHANGELOG.en.md"
        self.de.write_text(LOG, encoding="utf-8")
        self.en.write_text(LOG_EN, encoding="utf-8")
        for name, path in [("CHANGELOG", self.de), ("CHANGELOG_EN", self.en)]:
            patch = mock.patch.object(rn, name, path)
            patch.start()
            self.addCleanup(patch.stop)

    def run_main(self, *argv) -> str:
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            rn.main(list(argv))
        return out.getvalue()

    def test_show_prints_german_then_english(self):
        self.assertEqual(self.run_main("show", "0.2.0"),
                         "### Added\n\n- New thing\n\n---\n\n## English\n\n"
                         "### Added\n\n- New thing, in English\n")

    def test_show_prints_one_language(self):
        self.assertEqual(self.run_main("show", "0.2.0", "--lang", "de"),
                         "### Added\n\n- New thing\n")
        self.assertEqual(self.run_main("show", "0.2.0", "--lang", "en"),
                         "### Added\n\n- New thing, in English\n")
        with self.assertRaises(SystemExit) as stop:
            self.run_main("show", "0.2.0", "--lang", "fr")
        self.assertEqual(stop.exception.code, rn.__doc__)

    def test_show_stops_without_the_english_file(self):
        self.en.unlink()
        with self.assertRaises(SystemExit):
            self.run_main("show", "0.2.0")

    def test_cut_changes_both_files(self):
        self.run_main("cut", "0.2.0", "2026-02-02")
        for path in (self.de, self.en):
            self.assertIn("## [0.2.0] - 2026-02-02", path.read_text(encoding="utf-8"))

    def test_cut_changes_neither_file_if_one_fails(self):
        self.en.write_text("## [Unreleased]\n", encoding="utf-8")
        with self.assertRaises(SystemExit):
            self.run_main("cut", "0.2.0", "2026-02-02")
        self.assertEqual(self.de.read_text(encoding="utf-8"), LOG)
        self.assertEqual(self.en.read_text(encoding="utf-8"), "## [Unreleased]\n")

    def test_check_exits_with_1_and_says_why(self):
        self.run_main("check")
        self.en.write_text("## [Unreleased]\n", encoding="utf-8")
        err = io.StringIO()
        with contextlib.redirect_stderr(err), self.assertRaises(SystemExit) as stop:
            self.run_main("check")
        self.assertEqual(stop.exception.code, 1)
        self.assertIn("CHANGELOG.en.md: [Unreleased] Added is missing", err.getvalue())


if __name__ == "__main__":
    unittest.main()
