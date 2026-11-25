#!/usr/bin/env python3
"""M3-07: record the hook events of a whole real Codex session.

Throw-away CODEX_HOME from setup-home.py (only the login file is copied and
removed again), capture hook from M0 in mode `none` (Deck never answers).
Work folder `out/work` is a Git repository on branch `probe-branch`.

Session 1 (workspace-write): read a file, change a line, touch a file outside
the work folder (Codex asks, the script allows with Enter), /exit.
Session 2: `codex resume --last`, one prompt, /exit.

Raw events land in out/events, export-testdata.py turns them into test data.
Usage: record-events.py
"""
import glob, json, os, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from cdrive import Codex, OUT, WORK  # noqa: E402
import importlib.util  # noqa: E402

spec = importlib.util.spec_from_file_location("setup_home", os.path.join(HERE, "setup-home.py"))
home = importlib.util.module_from_spec(spec)
spec.loader.exec_module(home)

EVENTS = os.path.join(OUT, "events")
DIALOG = r"(Wouldyouliketo|Yes,proceed|Yes,allow)"
T0 = time.time()


def events():
    out = []
    for f in sorted(glob.glob(os.path.join(EVENTS, "*.json"))):
        with open(f) as fh:
            out.append(json.load(fh))
    return out


def count(name):
    return sum(1 for e in events() if e["event"] == name)


def show_new(seen):
    """Print one line per event that came since the last call."""
    all_ = events()
    for e in all_[seen:]:
        i = e["input"]
        detail = i.get("tool_name") or i.get("source") or ""
        extra = json.dumps(i.get("tool_input"))[:90] if i.get("tool_input") else (i.get("prompt") or "")[:60]
        print(f"{e['ts'] - T0:6.1f} {e['event']:<18} {detail:<12} {extra}")
    return len(all_)


def git(*args):
    subprocess.run(["git", "-C", WORK, *args], check=True, capture_output=True)


def fresh_work():
    shutil.rmtree(WORK, ignore_errors=True)
    outside = os.path.join(OUT, "outside.txt")
    if os.path.exists(outside):
        os.remove(outside)
    os.makedirs(WORK)
    git("init", "-q", "-b", "probe-branch")
    with open(os.path.join(WORK, "notes.txt"), "w") as f:
        f.write("first line\nsecond line\nthird line\n")
    git("add", "notes.txt")
    git("-c", "user.name=Probe", "-c", "user.email=probe@example.invalid", "commit", "-q", "-m", "start")


def type_in(c, text):
    c.mark()
    for ch in text:
        c.send(ch)
        c.pump(0.02)
    c.pump(0.8)
    c.send("\r")


def until_stop(c, stops_before, timeout=180, allow=True):
    """Pump until one more Stop event arrived; allow every dialog with Enter."""
    end = time.time() + timeout
    while time.time() < end:
        c.pump(0.5)
        if allow and re.search(DIALOG, re.sub(r"\s+", "", c.buf[c.mark_pos:])):
            print(f"{time.time() - T0:6.1f} -- dialog in the terminal, allowed with Enter")
            c.pump(0.5)
            c.send("\r")
            c.mark()
        if count("Stop") > stops_before:
            return True
    return False


def leave(c, timeout=20):
    c.mark()
    for ch in "/exit":
        c.send(ch)
        c.pump(0.05)
    c.pump(0.5)
    c.send("\r")
    end = time.time() + timeout
    while time.time() < end:
        if not c.pump(0.3):
            break
    c.close()


def main():
    home.setup()
    with open(os.path.join(OUT, "mode"), "w") as f:
        f.write("none")
    for f in glob.glob(os.path.join(EVENTS, "*.json")):
        os.remove(f)
    fresh_work()
    seen = 0
    ok = True

    print("--- session 1 (workspace-write) ---")
    c = Codex("rec-1", sandbox="workspace-write")
    try:
        c.ready()
        seen = show_new(seen)
        type_in(c, "Show the content of notes.txt. Then replace the words 'second line' in notes.txt "
                   "with 'changed by codex'. Then run this shell command: touch ../outside.txt")
        ok &= until_stop(c, count("Stop"))
        seen = show_new(seen)
    finally:
        leave(c)
    time.sleep(1)
    seen = show_new(seen)

    print("--- session 2 (resume --last) ---")
    c = Codex("rec-2", args=("resume", "--last"))
    try:
        c.ready()
        seen = show_new(seen)
        type_in(c, "Reply with the single word ok")
        ok &= until_stop(c, count("Stop"), allow=False)
        seen = show_new(seen)
    finally:
        leave(c)
    time.sleep(1)
    show_new(seen)

    with open(os.path.join(WORK, "notes.txt")) as f:
        print("notes.txt changed:", "changed by codex" in f.read())
    print("outside.txt created:", os.path.exists(os.path.join(OUT, "outside.txt")))
    print("events:", ", ".join(sorted({e["event"] for e in events()})))
    return ok


if __name__ == "__main__":
    try:
        good = main()
    finally:
        home.cleanup()
    print("PASS" if good else "FAIL: a turn did not end")
    sys.exit(0 if good else 1)
