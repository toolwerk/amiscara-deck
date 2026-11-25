#!/usr/bin/env python3
"""The Codex rows C3 to C7 of docs/handtest.md, against the real Codex.

Own data folder (DECK_DATA_DIR), daemon from target/debug, a throw-away
CODEX_HOME (setup-home.py: only the login file is copied and removed again)
whose hooks.json calls `<deck-hook> codex` like the app installs it. The user
interface is the Python one of spikes/handtest/auto.py. "At the terminal"
means: the frontmost app is the bundle of the Python that drives Codex.

Usage: handtest-codex.py c3|c4|c5|c6|c7|all   (cargo build first)
Prints PASS or FAIL with a reason per check, exit code 1 on any FAIL.
"""
import importlib.util, json, os, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(ROOT, "spikes", "handtest"))
from cdrive import Codex, HOME as CODEX_HOME, OUT, WORK  # noqa: E402
from auto import Ui  # noqa: E402
from e2e import CLI, DECKD, HOOK, check  # noqa: E402


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, file))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


home = load("setup_home", "setup-home.py")
record = load("record_events", "record-events.py")

DIALOG = record.DIALOG
EVENTS = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop", "SessionEnd"]
OTHER_APP = "/Applications/Safari.app"


def terminal_app():
    """The bundle of this script's Python, as `ps` names it: a parent of Codex."""
    comm = subprocess.run(["ps", "-o", "comm=", "-p", str(os.getpid())], capture_output=True, text=True).stdout.strip()
    return comm.split(".app/")[0] + ".app" if ".app/" in comm else comm


class Deck:
    def __init__(self):
        self.tmp = tempfile.mkdtemp(prefix="deck-codex-")
        self.data = os.path.join(self.tmp, "data")
        self.env = dict(os.environ, DECK_DATA_DIR=self.data)
        # Codex hands its environment on to the hooks: they find this daemon.
        os.environ["DECK_DATA_DIR"] = self.data
        self.daemon = subprocess.Popen([DECKD], env=self.env, stderr=subprocess.DEVNULL)
        for _ in range(50):
            if os.path.exists(os.path.join(self.data, "deckd.sock")):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError("daemon did not start")
        self.ui = Ui(self)

    def settings(self, mode="auto", codex=False, wait=120):
        self.ui.send({"type": "set_settings", "mode": mode, "wait_secs": wait, "retention_days": 7,
                      "codex_approvals": codex})
        time.sleep(0.5)

    def presence(self, app):
        self.ui.presence(app)

    def agents(self):
        r = subprocess.run([CLI, "list", "--json"], capture_output=True, text=True, env=self.env)
        return json.loads(r.stdout) if r.returncode == 0 and r.stdout.strip() else []

    def stop(self):
        if self.daemon.poll() is None:
            self.daemon.terminate()
            try:
                self.daemon.wait(5)
            except subprocess.TimeoutExpired:
                self.daemon.kill()
        shutil.rmtree(self.tmp, ignore_errors=True)


def codex_home():
    home.setup()
    hooks = {}
    for e in EVENTS:
        hooks[e] = [{"hooks": [{"type": "command", "command": f"{HOOK} codex",
                                "timeout": 570 if e == "PermissionRequest" else 10}]}]
    with open(os.path.join(CODEX_HOME, "hooks.json"), "w") as f:
        json.dump({"hooks": hooks}, f, indent=2)


def ask(c, text):
    c.mark()
    for ch in text:
        c.send(ch)
        c.pump(0.02)
    c.pump(0.8)
    c.send("\r")


def dialog(c):
    return re.search(DIALOG, re.sub(r"\s+", "", c.buf[c.mark_pos:])) is not None


def until(c, pred, secs):
    end = time.time() + secs
    while time.time() < end:
        if pred():
            return True
        c.pump(0.2)
    return pred()


def request(d, c, timeout=90, keep_app=None):
    """The next agent change with a request; keeps the presence fresh."""
    end = time.time() + timeout
    while time.time() < end:
        if keep_app:
            d.presence(keep_app)
        m = d.ui.wait(lambda m: m["type"] == "agent_changed" and m.get("request"), 5, c)
        if m:
            return m
    return None


def turn_done(d, c, timeout=120, allow_dialogs=False):
    end = time.time() + timeout
    while time.time() < end:
        if allow_dialogs and dialog(c):
            c.pump(0.5)
            c.send("\r")
            c.mark()
        m = d.ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done", 1, c)
        if m:
            return m
    return None


def touch_case(d, c, word):
    marker = os.path.join(WORK, word + ".txt")
    ask(c, f"Please run this shell command and nothing else: touch {word}.txt")
    return marker


# --- the rows -----------------------------------------------------------------

def c3(d, c):
    """Approvals for Codex off: the dialog is there at once, Deck never decides."""
    d.settings("auto", codex=False)
    d.presence(OTHER_APP)
    mark = len(d.ui.seen)
    marker = touch_case(d, c, "c3")
    m = request(d, c, keep_app=OTHER_APP)
    ok = check("C3a the request shows in Deck", m is not None)
    if not m:
        return False
    t0 = time.time()
    shown = until(c, lambda: dialog(c), 5)
    ok &= check("C3b the dialog is in the terminal at once", shown, f"{time.time() - t0:.1f} s")
    ok &= check("C3c Deck waits in the terminal (passed_through)", m["request"]["state"] == "passed_through", m["request"]["state"])
    c.send("\r")
    ok &= check("C3d answering in the terminal works", until(c, lambda: os.path.exists(marker), 30))
    turn_done(d, c)
    held = [x for x in d.ui.seen[mark:] if x["type"] == "agent_changed" and (x.get("request") or {}).get("state") in ("held", "answered")]
    ok &= check("C3e Deck never held or answered", not held)
    return ok


def c5(d, c):
    """Approvals on, Automatik, another app in front: held, allow in Deck, no dialog."""
    d.settings("auto", codex=True)
    d.presence(OTHER_APP)
    mark = len(d.ui.seen)
    marker = touch_case(d, c, "c5")
    m = request(d, c, keep_app=OTHER_APP)
    ok = check("C5a the request is held", m is not None and m["request"]["state"] == "held", (m or {}).get("request", {}).get("state", "none"))
    if not m or m["request"]["state"] != "held":
        return False
    d.ui.wait(lambda x: x["type"] == "notify", 3, c)
    ok &= check("C5b Deck notifies", any(x["type"] == "notify" and x["kind"] == "held" for x in d.ui.seen[mark:]))
    ok &= check("C5c no dialog in Codex while held", not dialog(c))
    d.ui.answer(m["request"]["id"], {"decision": "allow", "always": None, "expanded": True})
    ok &= check("C5d allow in Deck runs the command", until(c, lambda: os.path.exists(marker), 30))
    ok &= check("C5e no dialog in Codex", not dialog(c))
    turn_done(d, c)
    return ok


def c6(d, c):
    """Held, then the user comes back to the terminal: the dialog shows soon."""
    d.settings("auto", codex=True)
    d.presence(OTHER_APP)
    marker = touch_case(d, c, "c6")
    m = request(d, c, keep_app=OTHER_APP)
    ok = check("C6a the request is held", m is not None and m["request"]["state"] == "held", (m or {}).get("request", {}).get("state", "none"))
    if not m or m["request"]["state"] != "held":
        return False
    c.pump(2)
    t0 = time.time()
    d.presence(terminal_app())
    shown = until(c, lambda: dialog(c), 10)
    ok &= check("C6b back at the terminal: the dialog shows within 10 s", shown, f"{time.time() - t0:.1f} s")
    c.send("\r")
    ok &= check("C6c answering in the terminal works", until(c, lambda: os.path.exists(marker), 30))
    turn_done(d, c)
    return ok


def c7(d, c):
    """Approvals on, the user sits at the terminal: no wait, the dialog is there."""
    d.settings("auto", codex=True)
    app = terminal_app()
    d.presence(app)
    marker = touch_case(d, c, "c7")
    m = request(d, c, keep_app=app)
    ok = check("C7a the request is not held", m is not None and m["request"]["state"] == "passed_through", (m or {}).get("request", {}).get("state", "none"))
    if not m:
        return False
    t0 = time.time()
    shown = until(c, lambda: dialog(c), 5)
    ok &= check("C7b the dialog is there at once", shown, f"{time.time() - t0:.1f} s")
    c.send("\r")
    ok &= check("C7c answering in the terminal works", until(c, lambda: os.path.exists(marker), 30))
    turn_done(d, c)
    return ok


def c4(d):
    """A whole session as in record-events.py, compared with `list --json`."""
    d.settings("auto", codex=False)
    record.fresh_work()
    ok = True
    c = Codex("hc-c4", sandbox="workspace-write")
    try:
        c.ready()
        mark = len(d.ui.seen)
        ask(c, "Show the content of notes.txt. Then replace the words 'second line' in notes.txt "
               "with 'changed by codex'. Then run this shell command: touch ../outside.txt")
        done = turn_done(d, c, 180, allow_dialogs=True)
        ok &= check("C4a the turn ends (done)", done is not None)
        if not done:
            return False
        sid = done["id"]
        a = next((a for a in d.agents() if a["id"] == sid), None) or {}
        ok &= check("C4b tool, project and branch", (a.get("tool"), a.get("project"), a.get("branch")) == ("codex", "work", "probe-branch"),
                    f'{a.get("tool")} {a.get("project")} {a.get("branch")}')
        ok &= check("C4c prompt", (a.get("last_prompt") or "").startswith("Show the content of notes.txt"), repr(a.get("last_prompt"))[:60])
        ok &= check("C4d one turn", a.get("turns") == 1, str(a.get("turns")))
        mine = [x for x in d.ui.seen[mark:] if x["type"] == "agent_changed" and x["id"] == sid]
        acts = [x.get("activity") or {} for x in mine]
        ok &= check("C4e activity: a command", any(x.get("kind") == "run" and x.get("command") for x in acts))
        ok &= check("C4f activity: the edit of notes.txt", any(x.get("kind") == "edit" and x.get("path") == "notes.txt" for x in acts),
                    json.dumps([x for x in acts if x])[:200])
        statuses = [x["status"] for x in mine]
        seq = [s for i, s in enumerate(statuses) if i == 0 or statuses[i - 1] != s]
        ok &= check("C4g status working, blocked, working, done", seq[-4:] == ["working", "blocked", "working", "done"], " ".join(seq))
    finally:
        record.leave(c)
    ok &= check("C4h after /exit: exited", d.ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "exited", 20) is not None)

    c = Codex("hc-c4r", args=("resume", "--last"))
    try:
        c.ready()
        mark = len(d.ui.seen)
        ask(c, "Reply with the single word ok")
        ok &= check("C4j resume: the turn ends", turn_done(d, c) is not None)
        # Codex sends SessionStart only with the first prompt, so the idle
        # state between both is seen in the stream, not in `list`.
        mine = [x for x in d.ui.seen[mark:] if x["type"] == "agent_changed" and x["id"] == sid]
        first = mine[0] if mine else {}
        ok &= check("C4i resume: the same agent, first idle without prompt", (first.get("status"), first.get("last_prompt")) == ("idle", None),
                    f'{first.get("status")} {first.get("last_prompt")!r}')
        a = next((a for a in d.agents() if a["id"] == sid), None) or {}
        ok &= check("C4k resume: prompt and one agent", a.get("last_prompt") == "Reply with the single word ok" and len(d.agents()) == 1,
                    f'{a.get("last_prompt")!r} agents={len(d.agents())}')
    finally:
        record.leave(c)
    ok &= check("C4l after /exit: exited", d.ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "exited", 20) is not None)
    return ok


ROWS = {"c3": c3, "c5": c5, "c6": c6, "c7": c7}


def main(which):
    keys = ["c3", "c4", "c5", "c6", "c7"] if which == "all" else [which]
    results = {}
    codex_home()
    d = Deck()
    try:
        if "c4" in keys:
            results["c4"] = c4(d)
        rows = [k for k in keys if k in ROWS]
        if rows:
            record.fresh_work()
            c = Codex("hc-perm")
            try:
                c.ready()
                for k in rows:
                    print(f"--- {k} ---")
                    results[k] = ROWS[k](d, c)
            finally:
                record.leave(c)
    finally:
        d.stop()
        home.cleanup()
    print(results)
    return all(results.values())


if __name__ == "__main__":
    arg = sys.argv[1] if len(sys.argv) > 1 else "all"
    if arg not in ("c3", "c4", "c5", "c6", "c7", "all"):
        print(__doc__)
        sys.exit(2)
    sys.exit(0 if main(arg) else 1)
