#!/usr/bin/env python3
"""Tests for release-notes.py: python3 scripts/release_notes_test.py"""
import importlib.util
import unittest
from pathlib import Path

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


if __name__ == "__main__":
    unittest.main()
