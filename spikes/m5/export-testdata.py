#!/usr/bin/env python3
"""M3-07: turn the events of record-events.py into test data without personal
details: testdata/hooks-codex/<Event>[-<tool_name>].json, SessionStart as
SessionStart-<source>. A later event of the same name gets a number
(`PostToolUse-Bash-2`), so the files keep the whole recording in its order.

Paths are replaced (work folder, CODEX_HOME, home folder, login name). If a
login name, an e-mail address or another home folder is left, nothing is
written and the script ends with exit code 1.

Usage: export-testdata.py
"""
import getpass, glob, json, os, re, sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
OUT = os.path.join(HERE, "out")
EVENTS = os.path.join(OUT, "events")
TARGET = os.path.join(ROOT, "testdata", "hooks-codex")

USER = getpass.getuser()
HOME = os.path.expanduser("~")
EMAIL = re.compile(r"[\w.+-]+@[\w-]+\.[A-Za-z]{2,}")
OTHER_HOME = re.compile(r"/Users/(?!example\b)[^/\"\s]+")


def replacements():
    pairs = []
    for base, to in ((os.path.join(OUT, "work"), "/work/project"),
                     (os.path.join(OUT, "codex-home"), "/Users/example/.codex")):
        for p in {base, os.path.realpath(base)}:
            pairs.append((p, to))
    pairs.append((HOME, "/Users/example"))
    pairs.append((USER, "example"))
    return pairs


def clean(text):
    for old, new in replacements():
        text = text.replace(old, new)
    return text


def name_of(event, inp):
    if event == "SessionStart":
        return f"SessionStart-{inp.get('source', 'unknown')}"
    tool = inp.get("tool_name")
    return f"{event}-{tool}" if tool else event


def main():
    files = sorted(glob.glob(os.path.join(EVENTS, "*.json")))
    if not files:
        print("no events in", EVENTS, "- run record-events.py first", file=sys.stderr)
        return 1
    chosen = {}
    for f in files:
        with open(f) as fh:
            rec = json.load(fh)
        base = name_of(rec["event"], rec["input"])
        name, n = base, 1
        while name in chosen:
            n += 1
            name = f"{base}-{n}"
        sample = {
            "event": rec["event"],
            "input": rec["input"],
            # Variables of the machine that recorded, not of Codex: left out.
            "env": {k: v for k, v in rec.get("env", {}).items() if not k.startswith("CLAUDE")},
            "parents": [os.path.basename(p["comm"]) for p in rec.get("parents", [])],
        }
        chosen[name] = clean(json.dumps(sample, indent=1, ensure_ascii=False))

    bad = []
    for name, text in chosen.items():
        for what, pattern in (("login name", re.compile(re.escape(USER), re.I)),
                              ("e-mail address", EMAIL), ("home folder", OTHER_HOME)):
            m = pattern.search(text)
            if m:
                bad.append(f"{name}: {what} left: {m.group(0)}")
    if bad:
        print("\n".join(bad), file=sys.stderr)
        print("nothing written", file=sys.stderr)
        return 1

    os.makedirs(TARGET, exist_ok=True)
    for old in glob.glob(os.path.join(TARGET, "*.json")):
        os.remove(old)
    for name, text in sorted(chosen.items()):
        with open(os.path.join(TARGET, name + ".json"), "w") as fh:
            fh.write(text)
        print("wrote", os.path.relpath(os.path.join(TARGET, name + ".json"), ROOT))
    return 0


if __name__ == "__main__":
    sys.exit(main())
