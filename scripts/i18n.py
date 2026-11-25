#!/usr/bin/env python3
"""Texts for people: the template po/deck.pot and its translations po/<lang>.po.

The source texts are English and stand in the code. A call marks a text:

  Swift  L10n.t("Allow")   L10n.t("Version {version} is ready.", ["version": v])
         L10n.plural("{n} agent", "{n} agents", count)
         /*i18n*/"Stay quiet"   (a literal the system reads, such as a Focus filter)
  Rust   t("Settings")   t_with("running: {command}", &[("command", c)])
         plural("{n} agent", "{n} agents", n, &[])

Placeholders are {name}. Any other form of these calls is an error, so no text
slips past the template.

  python3 scripts/i18n.py extract      write po/deck.pot from the code
  python3 scripts/i18n.py check        exit 1 if the template is stale, or a
                                       language misses, leaves empty, marks fuzzy
                                       or adds a text, or changes placeholders
  python3 scripts/i18n.py apple <dir>  write <dir>/<lang>.lproj/Localizable.strings
                                       for the Mac app (en from the template)

Only the standard library: it runs in Xcode build phases and in CI.
"""
import os
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PO_DIR = ROOT / "po"
TEMPLATE = PO_DIR / "deck.pot"
LANGUAGES = ["de"]

SWIFT_DIRS = ["apps/AmiscaraDeck/AmiscaraDeck"]
RUST_GLOB = "crates/*/src"
# The crate that reads the catalog: its own tests use texts that are no texts.
RUST_SKIP = {"deck-i18n"}

PLACEHOLDER = re.compile(r"\{([A-Za-z_][A-Za-z0-9_]*)\}")

SWIFT_CALL = re.compile(r"L10n\.(t|plural)\(")
SWIFT_MARK = re.compile(r"/\*i18n\*/")
RUST_CALL = re.compile(r"(?<![\w.])(t|t_with|plural)\(")


class Failure(Exception):
    pass


class Entry:
    def __init__(self, msgid, plural=None):
        self.msgid = msgid
        self.plural = plural
        self.files = set()
        self.msgstr = []
        self.fuzzy = False

    def key(self):
        return (self.msgid, self.plural)


# --- reading the code -------------------------------------------------------


def _line(text, pos):
    return text.count("\n", 0, pos) + 1


def read_literal(text, pos, where, rust):
    """The string literal at `pos` (after spaces), and the position after it."""
    while pos < len(text) and text[pos] in " \t\r\n":
        pos += 1
    if not text.startswith('"', pos) or (not rust and text.startswith('"""', pos)):
        raise Failure(f"{where}:{_line(text, pos)}: text is not a plain string literal")
    out = []
    i = pos + 1
    while i < len(text):
        c = text[i]
        if c == '"':
            return "".join(out), i + 1
        if c == "\n" and not rust:
            break
        if c == "\\":
            n = text[i + 1] if i + 1 < len(text) else ""
            simple = {"n": "\n", "t": "\t", '"': '"', "\\": "\\", "'": "'", "0": "\0", "r": "\r"}
            if n in simple:
                out.append(simple[n])
                i += 2
                continue
            if n == "u":
                m = re.match(r"\\u\{([0-9A-Fa-f]+)\}", text[i:])
                if not m:
                    break
                out.append(chr(int(m.group(1), 16)))
                i += len(m.group(0))
                continue
            if rust and n == "\n":
                # A line continuation: the break and the spaces after it go.
                i += 2
                while i < len(text) and text[i] in " \t\r\n":
                    i += 1
                continue
            if not rust and n == "(":
                raise Failure(f"{where}:{_line(text, i)}: interpolation in a text, use a {{placeholder}}")
            break
        out.append(c)
        i += 1
    raise Failure(f"{where}:{_line(text, pos)}: unreadable string literal")


def expect_comma(text, pos, where):
    while pos < len(text) and text[pos] in " \t\r\n":
        pos += 1
    if not text.startswith(",", pos):
        raise Failure(f"{where}:{_line(text, pos)}: a plural needs two string literals")
    return pos + 1


def _strip_comments(text, rust):
    """Line comments become spaces of the same length, so positions hold.
    `/*i18n*/` stays: it is the mark. Also returns where string literals are,
    so a call written inside a string is no call."""
    out = []
    strings = []
    i = 0
    def blank(a, b):
        out.append(re.sub(r"[^\n]", " ", text[a:b]))

    def literal(a, b):
        out.append(text[a:b])
        strings.append((a, b))

    while i < len(text):
        word_before = i > 0 and (text[i - 1].isalnum() or text[i - 1] == "_")
        raw = None if word_before else (
            re.match(r'b?r(#*)"', text[i:]) if rust else re.match(r'(#+)"', text[i:])
        )
        if text.startswith("//", i):
            j = text.find("\n", i)
            j = len(text) if j < 0 else j
            blank(i, j)
            i = j
        elif text.startswith("/*", i) and not text.startswith("/*i18n*/", i):
            j = text.find("*/", i + 2)
            j = len(text) if j < 0 else j + 2
            blank(i, j)
            i = j
        elif raw:
            # A raw string: it ends at a quote with as many hashes.
            end = '"' + raw.group(1)
            j = text.find(end, i + len(raw.group(0)))
            j = len(text) if j < 0 else j + len(end)
            literal(i, j)
            i = j
        elif not rust and text.startswith('"""', i):
            j = text.find('"""', i + 3)
            j = len(text) if j < 0 else j + 3
            literal(i, j)
            i = j
        elif text.startswith('"', i):
            j = i + 1
            while j < len(text) and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append(text[i : j + 1])
            strings.append((i, j + 1))
            i = j + 1
        elif text.startswith("'", i) and rust:
            # A char literal such as '"' or a lifetime: copied as is.
            m = re.match(r"'(\\.|[^\\'])'", text[i:])
            span = len(m.group(0)) if m else 1
            out.append(text[i : i + span])
            i += span
        else:
            out.append(text[i])
            i += 1
    return "".join(out), strings


def _inside(pos, strings):
    return any(a < pos < b for a, b in strings)


def scan_swift(path, rel, found):
    text, strings = _strip_comments(path.read_text(encoding="utf-8"), rust=False)
    for m in SWIFT_CALL.finditer(text):
        if _inside(m.start(), strings):
            # Inside an interpolation the literal cannot be read reliably.
            raise Failure(f"{rel}:{_line(text, m.start())}: a text call inside a string, take it out of the interpolation")
        msgid, pos = read_literal(text, m.end(), rel, rust=False)
        plural = None
        if m.group(1) == "plural":
            pos = expect_comma(text, pos, rel)
            plural, pos = read_literal(text, pos, rel, rust=False)
        found.append((msgid, plural, rel))
    for m in SWIFT_MARK.finditer(text):
        if _inside(m.start(), strings):
            continue
        msgid, _ = read_literal(text, m.end(), rel, rust=False)
        found.append((msgid, None, rel))


def scan_rust(path, rel, found):
    text, strings = _strip_comments(path.read_text(encoding="utf-8"), rust=True)
    for m in RUST_CALL.finditer(text):
        if _inside(m.start(), strings):
            continue
        before = text[: m.start()].rstrip()
        if before.endswith("fn"):
            continue
        msgid, pos = read_literal(text, m.end(), rel, rust=True)
        plural = None
        if m.group(1) == "plural":
            pos = expect_comma(text, pos, rel)
            plural, pos = read_literal(text, pos, rel, rust=True)
        found.append((msgid, plural, rel))


def source_files(root):
    files = []
    for d in SWIFT_DIRS:
        files += [(p, "swift") for p in sorted((root / d).rglob("*.swift"))]
    for src in sorted(root.glob(RUST_GLOB)):
        if src.parent.name in RUST_SKIP:
            continue
        files += [(p, "rust") for p in sorted(src.rglob("*.rs"))]
    return files


def extract(root=ROOT):
    """The entries of the template, in the order they first appear."""
    found = []
    for path, kind in source_files(root):
        rel = path.relative_to(root).as_posix()
        (scan_swift if kind == "swift" else scan_rust)(path, rel, found)
    entries = {}
    for msgid, plural, rel in found:
        if msgid == "":
            raise Failure(f"{rel}: empty text")
        e = entries.get(msgid)
        if e is None:
            e = entries[msgid] = Entry(msgid, plural)
        elif e.plural != plural:
            raise Failure(f"{rel}: {msgid!r} is used with different plural forms")
        e.files.add(rel)
    return list(entries.values())


# --- the po format ----------------------------------------------------------


def quote(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t") + '"'


def unquote(s, where):
    s = s.strip()
    if len(s) < 2 or not (s.startswith('"') and s.endswith('"')):
        raise Failure(f"{where}: expected a quoted string: {s}")
    body = s[1:-1]
    out = []
    i = 0
    while i < len(body):
        c = body[i]
        if c == "\\" and i + 1 < len(body):
            n = body[i + 1]
            out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(n, n))
            i += 2
        else:
            out.append(c)
            i += 1
    return "".join(out)


def render_template(entries):
    lines = [
        "# Amiscara Deck: the texts for people, in English.",
        "# Written by `python3 scripts/i18n.py extract`. Do not edit by hand.",
        'msgid ""',
        'msgstr ""',
        '"Content-Type: text/plain; charset=UTF-8\\n"',
        '"Plural-Forms: nplurals=2; plural=(n != 1);\\n"',
        "",
    ]
    for e in entries:
        lines.append("#: " + " ".join(sorted(e.files)))
        lines.append("msgid " + quote(e.msgid))
        if e.plural is None:
            lines.append('msgstr ""')
        else:
            lines.append("msgid_plural " + quote(e.plural))
            lines.append('msgstr[0] ""')
            lines.append('msgstr[1] ""')
        lines.append("")
    return "\n".join(lines)


def parse_po(text, where):
    """Entries of a po file, header left out."""
    entries = []
    blocks = re.split(r"\n\s*\n", text)
    for block in blocks:
        e = None
        field = None
        fuzzy = False
        msgid = plural = None
        msgstr = {}
        for n, raw in enumerate(block.splitlines(), 1):
            line = raw.strip()
            if not line:
                continue
            if line.startswith("#,"):
                fuzzy = fuzzy or "fuzzy" in line
                continue
            if line.startswith("#"):
                continue
            m = re.match(r"^(msgid_plural|msgid|msgstr(?:\[(\d)\])?)\s+(\".*\")$", line)
            if m:
                name, index, value = m.group(1), m.group(2), unquote(m.group(3), where)
                if name == "msgid":
                    field, msgid = "msgid", value
                elif name == "msgid_plural":
                    field, plural = "plural", value
                else:
                    field = int(index or 0)
                    msgstr[field] = value
                continue
            if line.startswith('"') and field is not None:
                value = unquote(line, where)
                if field == "msgid":
                    msgid += value
                elif field == "plural":
                    plural += value
                else:
                    msgstr[field] += value
                continue
            raise Failure(f"{where}: unreadable line: {raw}")
        if msgid is None:
            continue
        if msgid == "":
            continue
        e = Entry(msgid, plural)
        e.fuzzy = fuzzy
        e.msgstr = [msgstr.get(i, "") for i in range(2 if plural is not None else 1)]
        entries.append(e)
    return entries


def read_po(path):
    return parse_po(path.read_text(encoding="utf-8"), path.relative_to(path.parents[1]).as_posix())


# --- check ------------------------------------------------------------------


def placeholders(s):
    return sorted(set(PLACEHOLDER.findall(s)))


def problems(template_text, expected_text, template, languages):
    """Everything that is wrong, one line each. `languages` maps a name to its entries."""
    out = []
    if template_text != expected_text:
        out.append("po/deck.pot does not match the code: run `python3 scripts/i18n.py extract`")
    want = {e.msgid: e for e in template}
    for lang, entries in languages.items():
        have = {}
        for e in entries:
            if e.msgid in have:
                out.append(f"po/{lang}.po: {e.msgid!r} is there twice")
            have[e.msgid] = e
        for msgid, t in want.items():
            e = have.get(msgid)
            if e is None:
                out.append(f"po/{lang}.po: missing {msgid!r}")
                continue
            if e.plural != t.plural:
                out.append(f"po/{lang}.po: {msgid!r} has other plural forms than the template")
                continue
            if e.fuzzy:
                out.append(f"po/{lang}.po: {msgid!r} is marked fuzzy")
            if any(s == "" for s in e.msgstr):
                out.append(f"po/{lang}.po: {msgid!r} is not translated")
                continue
            sources = [t.msgid] if t.plural is None else [t.msgid, t.plural]
            for src, dst in zip(sources, e.msgstr):
                if placeholders(src) != placeholders(dst):
                    out.append(
                        f"po/{lang}.po: {msgid!r}: placeholders {placeholders(dst)} differ from {placeholders(src)}"
                    )
        for msgid in have:
            if msgid not in want:
                out.append(f"po/{lang}.po: {msgid!r} is not in the template")
    return out


def check(root=ROOT):
    template_path = root / "po" / "deck.pot"
    if not template_path.exists():
        return ["po/deck.pot is missing: run `python3 scripts/i18n.py extract`"]
    template_text = template_path.read_text(encoding="utf-8")
    expected = render_template(extract(root))
    template = parse_po(template_text, "po/deck.pot")
    languages = {}
    for lang in LANGUAGES:
        p = root / "po" / f"{lang}.po"
        if not p.exists():
            return [f"po/{lang}.po is missing"]
        languages[lang] = parse_po(p.read_text(encoding="utf-8"), f"po/{lang}.po")
    out = problems(template_text, expected, template, languages)
    for lang, entries in languages.items():
        try:
            strings_table(template, entries)
        except Failure as e:
            out.append(f"po/{lang}.po: {e}")
    return out


# --- Apple ------------------------------------------------------------------


def strings_table(template, entries):
    """Key to value, as the Mac app looks texts up. A plural is two keys, its
    singular and its plural text. `entries` None: English, the keys themselves."""
    have = {e.msgid: e for e in entries or []}
    table = {}

    def put(key, value):
        if key in table and table[key] != value:
            raise Failure(f"{key!r} needs two different translations")
        table[key] = value

    for t in template:
        e = have.get(t.msgid)
        ok = e is not None and not e.fuzzy and e.plural == t.plural and all(e.msgstr)
        if t.plural is None:
            put(t.msgid, e.msgstr[0] if ok else t.msgid)
        else:
            put(t.msgid, e.msgstr[0] if ok else t.msgid)
            put(t.plural, e.msgstr[1] if ok else t.plural)
    return table


def strings_quote(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t") + '"'


def render_strings(table):
    lines = ["/* Written by scripts/i18n.py from po/. Do not edit. */", ""]
    for key in sorted(table):
        lines.append(f"{strings_quote(key)} = {strings_quote(table[key])};")
    return "\n".join(lines) + "\n"


def write_if_changed(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() and path.read_text(encoding="utf-8") == text:
        return
    tmp = path.with_suffix(".tmp")
    tmp.write_text(text, encoding="utf-8")
    os.replace(tmp, path)


def apple(out_dir, root=ROOT):
    template = read_po(root / "po" / "deck.pot")
    write_if_changed(out_dir / "en.lproj" / "Localizable.strings", render_strings(strings_table(template, None)))
    for lang in LANGUAGES:
        entries = read_po(root / "po" / f"{lang}.po")
        write_if_changed(out_dir / f"{lang}.lproj" / "Localizable.strings", render_strings(strings_table(template, entries)))


def main(argv):
    try:
        if argv[:1] == ["extract"] and len(argv) == 1:
            write_if_changed(TEMPLATE, render_template(extract()))
            return 0
        if argv[:1] == ["check"] and len(argv) == 1:
            found = check()
            for line in found:
                print(line, file=sys.stderr)
            return 1 if found else 0
        if argv[:1] == ["apple"] and len(argv) == 2:
            apple(Path(argv[1]).resolve())
            return 0
    except Failure as e:
        print(f"i18n: {e}", file=sys.stderr)
        return 1
    print("usage: i18n.py extract | check | apple <dir>", file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
