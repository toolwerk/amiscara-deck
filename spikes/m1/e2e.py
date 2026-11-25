#!/usr/bin/env python3
"""M1 acceptance against the real Claude Code.

Starts our daemon in a throw-away data folder, installs the hooks into a
throw-away settings file with `amiscara-deck hooks install --settings`, and
drives an interactive Claude Code session (pty, only these settings, never
~/.claude/settings.json). Checks what the milestone promises:

  A  a session shows up in `amiscara-deck list`, with its state
  B  a held request is allowed from the command line, without touching the terminal
  C  the terminal answers first: the card goes away
  D  the daemon dies while a request is held: the agent is not stuck, the dialog shows

Usage: e2e.py A|B|C|D|all   (needs `cargo build` first; uses the haiku model)
"""
import json, os, re, signal, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "spikes", "m0"))
import drive  # noqa: E402  (pty driver from M0)

BIN = os.path.join(ROOT, "target", "debug")
DECKD, HOOK, CLI = (os.path.join(BIN, n) for n in ("amiscara-deckd", "deck-hook", "amiscara-deck"))

class Env:
    def __init__(self):
        self.tmp = tempfile.mkdtemp(prefix="deck-e2e-")
        self.data = os.path.join(self.tmp, "data")
        self.work = os.path.join(self.tmp, "work")
        os.makedirs(self.work)
        drive.OUT = os.path.join(self.tmp, "out")
        os.makedirs(os.path.join(drive.OUT, "events"))
        self.settings = os.path.join(self.tmp, "settings.json")
        self.env = dict(os.environ, DECK_DATA_DIR=self.data)
        r = subprocess.run([CLI, "hooks", "install", "--settings", self.settings, "--program", HOOK],
                           capture_output=True, text=True, env=self.env)
        assert r.returncode == 0, r.stderr
        self.daemon = None
        self.watch = None

    def start_daemon(self):
        self.daemon = subprocess.Popen([DECKD], env=self.env, stderr=subprocess.PIPE)
        for _ in range(50):
            if os.path.exists(os.path.join(self.data, "deckd.sock")):
                return
            time.sleep(0.1)
        raise RuntimeError("daemon did not start")

    def cli(self, *args):
        r = subprocess.run([CLI, *args], capture_output=True, text=True, env=self.env)
        return r.returncode, r.stdout, r.stderr

    def agents(self):
        code, out, _ = self.cli("list", "--json")
        return json.loads(out) if code == 0 and out.strip() else []

    def wait_agent(self, pred, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            for a in self.agents():
                if pred(a):
                    return a
            time.sleep(0.3)
        return None

    def session(self, name):
        s = drive.Session(name, cwd=self.work, settings=self.settings, env_extra={"DECK_DATA_DIR": self.data})
        s.start()
        s.pump(3)
        return s

    def ui_connected(self):
        """Keep a UI connected so requests are held."""
        self.watch = subprocess.Popen([CLI, "watch", "--json"], env=self.env, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
        self.watch.stdout.readline()

    def stop(self):
        for p in (self.watch, self.daemon):
            if p and p.poll() is None:
                p.terminate()
                try:
                    p.wait(5)
                except subprocess.TimeoutExpired:
                    p.kill()

def ask(s, text):
    s.mark(); s.send(text); s.pump(1); s.send("\r")

def dialog_shown(s):
    """The permission dialog was drawn since the last prompt."""
    s.pump(1.5)  # the terminal output is read only when asked for
    return "Doyouwanttoproceed" in re.sub(r"\s+", "", s.buf[s.mark_pos:])

def check(name, ok, detail=""):
    print(("PASS " if ok else "FAIL ") + name + (f"  [{detail}]" if detail else ""))
    return ok

def scenario_a():
    e = Env(); e.start_daemon(); ok = True
    try:
        s = e.session("a")
        a = e.wait_agent(lambda a: a["status"] in ("idle", "working", "done"), 30)
        ok &= check("A1 session appears after start", a is not None)
        ask(s, "Reply with the single word hello")
        a = e.wait_agent(lambda a: a["status"] == "done", 60)
        ok &= check("A2 finished session is done", a is not None)
        if a:
            ok &= check("A3 project and prompt are shown", a["project"] == "work" and "hello" in (a["last_prompt"] or ""), f'{a["project"]!r} {a["last_prompt"]!r}')
            ok &= check("A4 one turn counted", a["turns"] == 1, str(a["turns"]))
        s.send("/exit\r"); s.pump(2)
        a = e.wait_agent(lambda a: a["status"] == "exited", 20)
        ok &= check("A5 ended session shows as exited", a is not None)
        s.close()
    finally:
        e.stop()
    return ok

def scenario_b():
    e = Env(); e.start_daemon(); e.ui_connected(); ok = True
    try:
        s = e.session("b")
        marker = os.path.join(e.work, "m1-b.txt")
        ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
        a = e.wait_agent(lambda a: a["request"] and a["request"]["state"] == "held", 60)
        ok &= check("B1 the request is held", a is not None)
        if not a:
            return False
        rid = a["request"]["id"]
        time.sleep(1)
        ok &= check("B2 the file does not exist yet", not os.path.exists(marker))
        code, out, err = e.cli("answer", rid, "allow")
        ok &= check("B3 `answer allow` succeeds", code == 0, out.strip() + err.strip())
        for _ in range(100):
            if os.path.exists(marker):
                break
            s.pump(0.3)
        ok &= check("B4 the tool ran without any key in the terminal", os.path.exists(marker))
        a = e.wait_agent(lambda a: a["status"] == "done", 60)
        ok &= check("B5 the card is gone and the agent is done", a is not None and a["request"] is None)
        s.send("/exit\r"); s.pump(2); s.close()
    finally:
        e.stop()
    return ok

def scenario_c():
    e = Env(); e.start_daemon(); e.ui_connected(); ok = True
    try:
        s = e.session("c")
        marker = os.path.join(e.work, "m1-c.txt")
        ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
        a = e.wait_agent(lambda a: a["request"] and a["request"]["state"] == "held", 60)
        ok &= check("C1 the request is held and the dialog is in the terminal too", a is not None and dialog_shown(s))
        s.send("\r")  # answer in the terminal
        for _ in range(100):
            if os.path.exists(marker):
                break
            s.pump(0.3)
        ok &= check("C2 answering in the terminal works", os.path.exists(marker))
        a = e.wait_agent(lambda a: a["status"] == "done" and a["request"] is None, 60)
        ok &= check("C3 the card goes away", a is not None)
        s.send("/exit\r"); s.pump(2); s.close()
    finally:
        e.stop()
    return ok

def scenario_d():
    e = Env(); e.start_daemon(); e.ui_connected(); ok = True
    try:
        s = e.session("d")
        marker = os.path.join(e.work, "m1-d.txt")
        ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
        a = e.wait_agent(lambda a: a["request"] and a["request"]["state"] == "held", 60)
        ok &= check("D1 the request is held", a is not None)
        e.daemon.send_signal(signal.SIGKILL)  # no clean stop
        e.daemon.wait()
        time.sleep(1)
        ok &= check("D2 the dialog is still in the terminal", dialog_shown(s))
        s.send("\r")
        for _ in range(100):
            if os.path.exists(marker):
                break
            s.pump(0.3)
        ok &= check("D3 the agent is not stuck: the terminal answer works", os.path.exists(marker))
        ask(s, "Reply with the single word ok")
        s.wait(r"ok", 60)
        ok &= check("D4 and the agent goes on with the daemon gone", True)
        s.send("/exit\r"); s.pump(2); s.close()
    finally:
        e.stop()
    return ok

if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    runs = {"A": scenario_a, "B": scenario_b, "C": scenario_c, "D": scenario_d}
    keys = list(runs) if which == "all" else [which]
    results = {k: runs[k]() for k in keys}
    print(results)
    sys.exit(0 if all(results.values()) else 1)
