#!/usr/bin/env python3
"""Tests for appcast.py: python3 scripts/appcast_test.py"""
import tempfile
import unittest
from pathlib import Path
from xml.etree import ElementTree as ET

import appcast

ENV = {
    "VERSION": "0.2.0", "BUILD": "7", "ZIP_URL": "https://example.org/A.zip", "ZIP_LENGTH": "42",
    "ZIP_ED_SIGNATURE": "sig==", "MIN_SYSTEM": "27.0", "PUB_DATE": "Wed, 25 Nov 2026 12:00:00 +0000",
    "RELEASE_URL": "https://example.org/v0.2.0",
}


class Feed(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name)
        en, de = self.folder / "notes-en.md", self.folder / "notes-de.md"
        en.write_text("### Added\n\n- A <new> thing & more\n", encoding="utf-8")
        de.write_text("### Added\n\n- Etwas Neues für alle\n", encoding="utf-8")
        self.env = {**ENV, "NOTES_EN_PATH": str(en), "NOTES_DE_PATH": str(de)}

    def test_the_item_carries_build_signature_and_address(self):
        item = appcast.feed(self.env).getroot().find("channel/item")
        ns = "{%s}" % appcast.SPARKLE_NS
        self.assertEqual(item.find(ns + "version").text, "7")
        self.assertEqual(item.find(ns + "shortVersionString").text, "0.2.0")
        self.assertEqual(item.find(ns + "minimumSystemVersion").text, "27.0")
        self.assertEqual(item.find(ns + "fullReleaseNotesLink").text, "https://example.org/v0.2.0")
        enclosure = item.find("enclosure")
        self.assertEqual(enclosure.get("url"), "https://example.org/A.zip")
        self.assertEqual(enclosure.get(ns + "edSignature"), "sig==")
        self.assertEqual(enclosure.get("length"), "42")

    def test_the_notes_come_in_english_then_german_as_markdown(self):
        written = ET.tostring(appcast.feed(self.env).getroot(), encoding="unicode")
        self.assertIn('xml:lang="en" sparkle:format="markdown"', written)
        item = ET.fromstring(written).find("channel/item")
        ns = "{%s}" % appcast.SPARKLE_NS
        found = [(d.get(appcast.XML_LANG), d.get(ns + "format"), d.text)
                 for d in item.findall("description")]
        self.assertEqual(found, [
            ("en", "markdown", "### Added\n\n- A <new> thing & more"),
            ("de", "markdown", "### Added\n\n- Etwas Neues für alle"),
        ])
        self.assertIsNone(item.find(ns + "releaseNotesLink"))

    def test_a_missing_value_stops(self):
        for name in self.env:
            with self.assertRaises(SystemExit):
                appcast.feed({**self.env, name: ""})

    def test_missing_or_empty_notes_stop(self):
        empty = self.folder / "empty.md"
        empty.write_text(" \n", encoding="utf-8")
        for name in ("NOTES_EN_PATH", "NOTES_DE_PATH"):
            for path in (self.folder / "nowhere.md", empty):
                with self.assertRaises(SystemExit) as stop:
                    appcast.feed({**self.env, name: str(path)})
                self.assertIn(name, str(stop.exception))


if __name__ == "__main__":
    unittest.main()
