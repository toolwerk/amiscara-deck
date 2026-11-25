#!/usr/bin/env python3
"""Release notes from CHANGELOG.md (Keep a Changelog).

    release-notes.py show <version>          print the notes of a version
    release-notes.py cut <version> <date>    turn [Unreleased] into [version] - date

`show` takes the section "## [<version>]". Before a release is cut it falls
back to [Unreleased]. An empty section is an error: nothing ships without
notes.
"""
import re
import sys
from pathlib import Path

CHANGELOG = Path(__file__).resolve().parent.parent / "CHANGELOG.md"


def section(text: str, name: str):
    """The body under `## [name]` up to the next `## ` heading, or None."""
    m = re.search(rf"^## \[{re.escape(name)}\][^\n]*\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    return m.group(1).strip() if m else None


def notes(text: str, version: str) -> str:
    body = section(text, version)
    if body is None:
        body = section(text, "Unreleased")
    if not body or not re.search(r"^\s*- ", body, re.M):
        raise SystemExit(f"release-notes: no notes for {version} in CHANGELOG.md")
    return body + "\n"


def cut(text: str, version: str, date: str) -> str:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise SystemExit(f"release-notes: '{version}' is not a version like 1.2.3")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", date):
        raise SystemExit(f"release-notes: '{date}' is not a date like 2026-01-31")
    if section(text, version) is not None:
        raise SystemExit(f"release-notes: [{version}] is already in CHANGELOG.md")
    notes(text, version)  # there must be something to release
    return text.replace("## [Unreleased]", f"## [Unreleased]\n\n## [{version}] - {date}", 1)


def main(argv) -> None:
    text = CHANGELOG.read_text()
    if len(argv) == 2 and argv[0] == "show":
        sys.stdout.write(notes(text, argv[1]))
    elif len(argv) == 3 and argv[0] == "cut":
        CHANGELOG.write_text(cut(text, argv[1], argv[2]))
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
