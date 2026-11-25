#!/usr/bin/env python3
"""Release notes from CHANGELOG.md (German) and CHANGELOG.en.md (English),
both in the form of Keep a Changelog.

    release-notes.py show <version>              print both notes, German first
    release-notes.py show <version> --lang de|en print the notes in one language
    release-notes.py cut <version> <date>        turn [Unreleased] into [version] - date
    release-notes.py check                       compare the two files

`show` takes the section "## [<version>]". Before a release is cut it falls
back to [Unreleased], in both files alike: the German file decides. A missing
or empty section in either file is an error: nothing ships without notes.

`cut` changes both files or none.

`check` wants the same headings, "###" sections and number of entries in both
files, for [Unreleased] and every version after 0.1.0.
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CHANGELOG = ROOT / "CHANGELOG.md"
CHANGELOG_EN = ROOT / "CHANGELOG.en.md"
# This version and the older ones exist in German only.
GERMAN_ONLY = "0.1.0"


def section(text: str, name: str):
    """The body under `## [name]` up to the next `## ` heading, or None."""
    m = re.search(rf"^## \[{re.escape(name)}\][^\n]*\n(.*?)(?=^## |\Z)", text, re.M | re.S)
    return m.group(1).strip() if m else None


def body(text: str, name: str, version: str, file: str) -> str:
    """The section `name` as the notes of `version`; it must list something."""
    found = section(text, name)
    if not found or not re.search(r"^\s*- ", found, re.M):
        raise SystemExit(f"release-notes: no notes for {version} in {file}")
    return found + "\n"


def source(text: str, version: str) -> str:
    """The section that holds the notes of a version."""
    return version if section(text, version) is not None else "Unreleased"


def notes(text: str, version: str, file: str = CHANGELOG.name) -> str:
    return body(text, source(text, version), version, file)


def both(de: str, en: str, version: str):
    """The German and the English notes, from the same section."""
    name = source(de, version)
    return (body(de, name, version, CHANGELOG.name),
            body(en, name, version, CHANGELOG_EN.name))


def cut(text: str, version: str, date: str, file: str = CHANGELOG.name) -> str:
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise SystemExit(f"release-notes: '{version}' is not a version like 1.2.3")
    if not re.fullmatch(r"\d{4}-\d{2}-\d{2}", date):
        raise SystemExit(f"release-notes: '{date}' is not a date like 2026-01-31")
    if section(text, version) is not None:
        raise SystemExit(f"release-notes: [{version}] is already in {file}")
    body(text, "Unreleased", version, file)  # there must be something to release
    return text.replace("## [Unreleased]", f"## [Unreleased]\n\n## [{version}] - {date}", 1)


def releases(text: str):
    """name -> (heading, {"###" section: number of entries}), above GERMAN_ONLY."""
    found = {}
    entries = None
    for line in text.splitlines():
        if line.startswith("## "):
            m = re.match(r"## \[([^\]]+)\]", line)
            if m and m.group(1) == GERMAN_ONLY:
                break
            entries = {}
            if m:
                found[m.group(1)] = (line.rstrip(), entries)
            part = ""
        elif entries is None:
            continue
        elif line.startswith("### "):
            part = line[4:].strip()
            entries.setdefault(part, 0)
        elif line.startswith("- "):
            entries[part] = entries.get(part, 0) + 1
    return found


def check(de: str, en: str) -> list:
    """What differs between the two files, as one message each."""
    german, english = releases(de), releases(en)
    problems = [
        f"{file.name}: [Unreleased] is missing"
        for file, found in ((CHANGELOG, german), (CHANGELOG_EN, english))
        if "Unreleased" not in found
    ]
    for name in english:
        if name not in german:
            problems.append(f"{CHANGELOG.name}: [{name}] is missing")
    for name, (heading, entries) in german.items():
        if name not in english:
            problems.append(f"{CHANGELOG_EN.name}: [{name}] is missing")
            continue
        other_heading, other = english[name]
        if other_heading != heading:
            problems.append(f"{CHANGELOG_EN.name}: heading '{other_heading}' instead of '{heading}'")
        for part in {**entries, **other}:
            where = f"[{name}] {part}".rstrip()
            if part not in other:
                problems.append(f"{CHANGELOG_EN.name}: {where} is missing")
            elif part not in entries:
                problems.append(f"{CHANGELOG.name}: {where} is missing")
            elif entries[part] != other[part]:
                problems.append(f"{where}: {entries[part]} entries in {CHANGELOG.name}, "
                                f"{other[part]} in {CHANGELOG_EN.name}")
    return problems


def read(path: Path) -> str:
    if not path.is_file():
        raise SystemExit(f"release-notes: {path.name} is missing")
    return path.read_text(encoding="utf-8")


def main(argv) -> None:
    show = len(argv) >= 2 and argv[0] == "show"
    if show and len(argv) == 2:
        de, en = both(read(CHANGELOG), read(CHANGELOG_EN), argv[1])
        sys.stdout.write(f"{de}\n---\n\n## English\n\n{en}")
    elif show and len(argv) == 4 and argv[2] == "--lang" and argv[3] in ("de", "en"):
        de, en = both(read(CHANGELOG), read(CHANGELOG_EN), argv[1])
        sys.stdout.write(de if argv[3] == "de" else en)
    elif len(argv) == 3 and argv[0] == "cut":
        # Both are cut in memory first, so a failure leaves both files alone.
        cuts = [(path, cut(read(path), argv[1], argv[2], path.name))
                for path in (CHANGELOG, CHANGELOG_EN)]
        for path, text in cuts:
            path.write_text(text, encoding="utf-8")
    elif argv == ["check"]:
        problems = check(read(CHANGELOG), read(CHANGELOG_EN))
        for problem in problems:
            print(f"release-notes: {problem}", file=sys.stderr)
        if problems:
            raise SystemExit(1)
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    main(sys.argv[1:])
