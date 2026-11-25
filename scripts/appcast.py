#!/usr/bin/env python3
"""Writes the Sparkle feed (appcast.xml) for one release of the Mac app.

Called by scripts/sparkle-feed.sh; the values come as environment variables:
OUTPUT_PATH, VERSION, BUILD, ZIP_URL, ZIP_LENGTH, ZIP_ED_SIGNATURE,
MIN_SYSTEM, PUB_DATE, RELEASE_URL, NOTES_EN_PATH and NOTES_DE_PATH. The
last two name Markdown files with the release notes; the feed carries their
text. RELEASE_URL is the page of the release.
"""
import os
import sys
from pathlib import Path
from xml.etree import ElementTree as ET

SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
ET.register_namespace("sparkle", SPARKLE_NS)
XML_LANG = "{http://www.w3.org/XML/1998/namespace}lang"
# English first: Sparkle takes the first one if no language of the Mac matches.
NOTES = (("en", "NOTES_EN_PATH"), ("de", "NOTES_DE_PATH"))
FEED_URL = "https://github.com/toolwerk/amiscara-deck/releases/download/updates/appcast.xml"


def required(env, name: str) -> str:
    value = env.get(name)
    if not value:
        sys.exit(f"appcast: {name} is missing")
    return value


def read_notes(env, name: str) -> str:
    path = Path(required(env, name))
    if not path.is_file():
        sys.exit(f"appcast: {name}: no file at {path}")
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        sys.exit(f"appcast: {name}: {path} is empty")
    return text


def feed(env) -> ET.ElementTree:
    version = required(env, "VERSION")
    rss = ET.Element("rss", {"version": "2.0"})
    channel = ET.SubElement(rss, "channel")
    ET.SubElement(channel, "title").text = "Amiscara Deck"
    ET.SubElement(channel, "link").text = FEED_URL
    ET.SubElement(channel, "description").text = "Signierte Versionen der Mac-App"
    ET.SubElement(channel, "language").text = "de"
    item = ET.SubElement(channel, "item")
    ET.SubElement(item, "title").text = f"Version {version}"
    ET.SubElement(item, "pubDate").text = required(env, "PUB_DATE")
    ET.SubElement(item, f"{{{SPARKLE_NS}}}version").text = required(env, "BUILD")
    ET.SubElement(item, f"{{{SPARKLE_NS}}}shortVersionString").text = version
    ET.SubElement(item, f"{{{SPARKLE_NS}}}minimumSystemVersion").text = required(env, "MIN_SYSTEM")
    for lang, name in NOTES:
        ET.SubElement(item, "description", {
            XML_LANG: lang,
            f"{{{SPARKLE_NS}}}format": "markdown",
        }).text = read_notes(env, name)
    # Behind "Version History" in the dialog that says the app is up to date.
    ET.SubElement(item, f"{{{SPARKLE_NS}}}fullReleaseNotesLink").text = required(env, "RELEASE_URL")
    ET.SubElement(item, "enclosure", {
        "url": required(env, "ZIP_URL"),
        f"{{{SPARKLE_NS}}}edSignature": required(env, "ZIP_ED_SIGNATURE"),
        "length": required(env, "ZIP_LENGTH"),
        "type": "application/octet-stream",
    })
    tree = ET.ElementTree(rss)
    ET.indent(tree, space="  ")
    return tree


def main() -> None:
    output = Path(required(os.environ, "OUTPUT_PATH"))
    output.parent.mkdir(parents=True, exist_ok=True)
    feed(os.environ).write(output, encoding="UTF-8", xml_declaration=True)


if __name__ == "__main__":
    main()
