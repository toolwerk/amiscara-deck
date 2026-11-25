#!/usr/bin/env python3
"""Tests for appcast.py: python3 scripts/appcast_test.py"""
import unittest

import appcast

ENV = {
    "VERSION": "0.2.0", "BUILD": "7", "ZIP_URL": "https://example.org/A.zip", "ZIP_LENGTH": "42",
    "ZIP_ED_SIGNATURE": "sig==", "MIN_SYSTEM": "27.0", "PUB_DATE": "Wed, 25 Nov 2026 12:00:00 +0000",
    "NOTES_URL": "https://example.org/v0.2.0",
}


class Feed(unittest.TestCase):
    def test_the_item_carries_build_signature_and_address(self):
        item = appcast.feed(ENV).getroot().find("channel/item")
        ns = "{%s}" % appcast.SPARKLE_NS
        self.assertEqual(item.find(ns + "version").text, "7")
        self.assertEqual(item.find(ns + "shortVersionString").text, "0.2.0")
        self.assertEqual(item.find(ns + "minimumSystemVersion").text, "27.0")
        enclosure = item.find("enclosure")
        self.assertEqual(enclosure.get("url"), "https://example.org/A.zip")
        self.assertEqual(enclosure.get(ns + "edSignature"), "sig==")
        self.assertEqual(enclosure.get("length"), "42")

    def test_a_missing_value_stops(self):
        for name in ENV:
            with self.assertRaises(SystemExit):
                appcast.feed({**ENV, name: ""})


if __name__ == "__main__":
    unittest.main()
