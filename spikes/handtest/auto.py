#!/usr/bin/env python3
"""The parts of docs/handtest.md that a script can check against the real
Claude Code: throw-away data folder and settings file, a Python user interface
that speaks the protocol (answers, settings, a made up presence), pty driver
from M0. ~/.claude/settings.json is neither read nor changed.

Usage: auto.py deny|always|timeout|presence|figures|question|all  (cargo build first)
"""
import glob, json, os, re, socket, stat, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(ROOT, "spikes", "m1"))
import e2e  # noqa: E402
from e2e import Env, ask, check, dialog_shown  # noqa: E402

def protocol():
    """The protocol number of this source tree, so the UI matches its daemon."""
    src = open(os.path.join(ROOT, "crates", "deck-proto", "src", "lib.rs")).read()
    return int(re.search(r"PROTOCOL_VERSION: u32 = (\d+);", src).group(1))

class Ui:
    def __init__(self, env):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(os.path.join(env.data, "deckd.sock"))
        self.buf = b""
        self.agents, self.seen = {}, []
        self.send({"type": "hello", "protocol": protocol(), "name": "handtest"})
        w = self.read(5)
        assert w and w["type"] == "welcome", w
        self.welcome = w

    def send(self, m):
        self.s.sendall((json.dumps(m) + "\n").encode())

    def read(self, timeout):
        end = time.time() + timeout
        while b"\n" not in self.buf:
            left = end - time.time()
            if left <= 0:
                return None
            self.s.settimeout(left)
            try:
                d = self.s.recv(65536)
            except socket.timeout:
                return None
            if not d:
                return None
            self.buf += d
        line, self.buf = self.buf.split(b"\n", 1)
        m = json.loads(line)
        self.seen.append(m)
        if m["type"] == "agent_changed":
            self.agents[m["id"]] = m
        return m

    def wait(self, pred, timeout=60, term=None):
        end = time.time() + timeout
        while time.time() < end:
            if term:
                term.pump(0.2)
            m = self.read(0.3)
            if m and pred(m):
                return m
        return None

    def held(self, term, timeout=60):
        return self.wait(lambda m: m["type"] == "agent_changed" and (m.get("request") or {}).get("state") == "held", timeout, term)

    def settings(self, mode="deck", wait=120):
        self.send({"type": "set_settings", "mode": mode, "wait_secs": wait, "retention_days": 7, "codex_approvals": False})
        time.sleep(0.5)

    def answer(self, rid, decision):
        self.send({"type": "answer", "request_id": rid, "decision": decision, "via": "cli"})

    def presence(self, app, idle=1, locked=False, focus=False):
        self.send({"type": "presence", "screen_locked": locked, "idle_secs": idle, "frontmost_app": app,
                   "focus_mode": focus, "last_input_ms": int(time.time() * 1000) - idle * 1000})
        time.sleep(0.5)

def setup(name, mode="deck", wait=120):
    e = Env(); e.start_daemon()
    ui = Ui(e); ui.settings(mode, wait)
    s = e.session(name)
    return e, ui, s

def done(e, s):
    try:
        s.send("/exit\r"); s.pump(2); s.close()
    finally:
        e.stop()

def wait_file(s, path, secs=30):
    end = time.time() + secs
    while time.time() < end and not os.path.exists(path):
        s.pump(0.3)
    return os.path.exists(path)

def transcript(e):
    # Claude Code stores the transcript under a folder named after the work folder.
    real = os.path.realpath(e.work)
    d = os.path.expanduser("~/.claude/projects/" + re.sub(r"[^A-Za-z0-9]", "-", real))
    files = sorted(glob.glob(os.path.join(d, "*.jsonl")), key=os.path.getmtime)
    return files[-1] if files else None

def deny():
    e, ui, s = setup("h-deny"); ok = True
    try:
        marker = os.path.join(e.work, "h-deny.txt")
        ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
        a = ui.held(s)
        ok &= check("F3a the request is held", a is not None)
        if not a:
            return False
        ui.answer(a["request"]["id"], {"decision": "deny", "message": "Handtest: bitte nicht, sage das Wort Ananas"})
        d = ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done", 60, s)
        s.pump(2)
        ok &= check("F3b denied: the tool did not run", not os.path.exists(marker))
        ok &= check("F3c the agent goes on to the end of its turn", d is not None)
        t = transcript(e)
        text = open(t).read() if t else ""
        ok &= check("F3d the reason reaches the model", "Handtest: bitte nicht" in text, t or "no transcript")
    finally:
        done(e, s)
    return ok

def always():
    e, ui, s = setup("h-always"); ok = True
    try:
        ask(s, "Run exactly this bash command and nothing else: curl --version")
        a = ui.held(s)
        offers = (a or {}).get("request", {}).get("always_allow") or []
        ok &= check("F13a 'always allow' is offered with a rule", bool(offers), json.dumps(offers))
        if not offers:
            return False
        ui.answer(a["request"]["id"], {"decision": "allow", "always": offers[0]["index"], "expanded": True})
        ok &= check("F13b the turn ends", ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done", 60, s) is not None)
        local = os.path.join(e.work, ".claude", "settings.local.json")
        rules = open(local).read() if os.path.exists(local) else ""
        ok &= check("F13c the rule is in .claude/settings.local.json", "curl --version" in rules, rules.replace("\n", " "))
        mark = len(ui.seen)
        ask(s, "Run exactly this bash command again and nothing else: curl --version")
        ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done" and len(ui.seen) > mark + 1, 60, s)
        asked = [m for m in ui.seen[mark:] if m["type"] == "agent_changed" and m.get("request")]
        ok &= check("F13d the second time nothing is asked", not asked)
    finally:
        done(e, s)
    return ok

def timeout():
    e, ui, s = setup("h-timeout", wait=30); ok = True
    try:
        marker = os.path.join(e.work, "h-timeout.txt")
        ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
        a = ui.held(s)
        ok &= check("F11a held with a wait of 30 s", a is not None)
        t0 = time.time()
        p = ui.wait(lambda m: m["type"] == "agent_changed" and (m.get("request") or {}).get("state") == "passed_through", 45, s)
        ok &= check("F11b after the time it waits in the terminal", p is not None and 25 < time.time() - t0 < 40, f"{time.time() - t0:.0f} s")
        ok &= check("F11c the dialog is in the terminal", dialog_shown(s))
        s.send("\r")
        ok &= check("F11d answering there works", wait_file(s, marker))
    finally:
        done(e, s)
    return ok

def presence():
    ok = True
    # A process above the agent of this run stands for the terminal app: the
    # bundle of this script's own interpreter, as `ps` names it.
    comm = subprocess.run(["ps", "-o", "comm=", "-p", str(os.getpid())], capture_output=True, text=True).stdout.strip()
    terminal = comm.split(".app/")[0] + ".app" if ".app/" in comm else comm
    print("stands for the terminal:", terminal)
    cases = [
        ("F5 Automatik, terminal in front", "auto", dict(app=terminal), True, False),
        ("F6 Automatik, another app in front", "auto", dict(app="/Applications/Safari.app"), True, True),
        ("F7 Automatik, screen locked", "auto", dict(app=terminal, locked=True), True, True),
        ("F7 Automatik, idle for 3 minutes", "auto", dict(app=terminal, idle=180), True, True),
        ("F9 Focus mode in 'Im Deck'", "deck", dict(app="/Applications/Safari.app", focus=True), True, False),
        ("F10 'Im Terminal'", "terminal", dict(app="/Applications/Safari.app"), False, False),
    ]
    e, ui, s = setup("h-presence")
    try:
        for i, (name, mode, p, want_held, want_notify) in enumerate(cases):
            ui.settings(mode, 120)
            ui.presence(**p)
            marker = os.path.join(e.work, f"h-p{i}.txt")
            mark = len(ui.seen)
            ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
            req = ui.wait(lambda m: m["type"] == "agent_changed" and m.get("request"), 60, s)
            ui.presence(**p)
            ui.wait(lambda m: False, 3, s)
            held = bool(req) and req["request"]["state"] == "held"
            notified = any(m["type"] == "notify" and m["kind"] == "held" for m in ui.seen[mark:])
            ok &= check(name, bool(req) and held == want_held and notified == want_notify, f"held={held} notify={notified}")
            if dialog_shown(s) or True:
                s.send("\r")
            wait_file(s, marker)
            ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done", 60, s)
    finally:
        done(e, s)
    return ok

def figures():
    e, ui, s = setup("h-figures"); ok = True
    try:
        marker = os.path.join(e.work, "h-fig.txt")
        ask(s, f"Run exactly this bash command and nothing else: touch {marker}")
        a = ui.held(s)
        ui.answer(a["request"]["id"], {"decision": "allow", "always": None, "expanded": True})
        ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done", 60, s)
        ui.wait(lambda m: False, 3, s)
        agent = ui.agents[a["id"]]
        t = transcript(e)
        usage = None
        for line in open(t):
            d = json.loads(line)
            if d.get("type") == "assistant" and not d.get("isSidechain"):
                u = d["message"]["usage"]
                usage = u["input_tokens"] + u.get("cache_creation_input_tokens", 0) + u.get("cache_read_input_tokens", 0)
        ctx = agent.get("context") or {}
        ok &= check("D4 context fill equals the transcript", ctx.get("used_tokens") == usage, f"deck={ctx} transcript={usage}")
        log = os.path.join(e.data, "decisions.jsonl")
        mode = stat.S_IMODE(os.stat(log).st_mode)
        lines = [json.loads(l) for l in open(log)]
        ok &= check("D5a decision log is private and has the answer", mode == 0o600 and lines[-1]["decision"] == "allow" and marker in lines[-1]["command"], f"{oct(mode)} {lines[-1]['decision']}")
        ok &= check("D5b history of the session", len(agent["history"]) >= 4, " | ".join(h["text"] for h in agent["history"]))
        ui.send({"type": "clear_history"}); time.sleep(1)
        ok &= check("D5c clearing empties the log", open(log).read() == "")
        # Cost: Claude Code writes cost-state now and then, at the latest on exit.
        s.send("/exit\r"); s.pump(5)
        ui.wait(lambda m: False, 4, s)
        total = None
        for line in open(t):
            if '"type":"cost-state"' in line:
                total = json.loads(line)["totalCostUSD"]
        agent = ui.agents[a["id"]]
        today = [m["today_micro_usd"] for m in ui.seen if m["type"] == "costs"]
        got = agent.get("cost_micro_usd")
        if total is None:
            ok &= check("D7 cost: no cost-state line in this transcript", False, "nothing to compare")
        else:
            ok &= check("D7a cost of the session equals the transcript", got == round(total * 1e6), f"deck={got} transcript={total}")
            ok &= check("D7b today's cost", bool(today) and today[-1] == got, f"today={today[-1:]} session={got}")
        s.close()
    finally:
        e.stop()
    return ok

def question():
    e, ui, s = setup("h-question"); ok = True
    try:
        ask(s, "Use the AskUserQuestion tool once with two questions: (1) which fruits I like, multiSelect true, options Apfel, Birne, Kirsche; (2) which drink, options Tee and Kaffee. Then list my answers.")
        a = ui.held(s)
        qs = ((a or {}).get("request") or {}).get("questions") or []
        ok &= check("F14a the question is held with its choices", len(qs) == 2, json.dumps(qs, ensure_ascii=False)[:160])
        if len(qs) != 2:
            return False
        s.pump(2)
        shown = "Apfel" in s.buf[s.mark_pos:]
        ok &= check("F14b the question stands in the terminal too", shown)
        rid = a["request"]["id"]
        ui.answer(rid, {"decision": "allow", "always": None, "expanded": True})
        ok &= check("F14c 'allow' does not answer it", ui.wait(lambda m: (m.get("request") or {}).get("state") == "answered", 3, s) is None)
        multi = 0 if qs[0]["multi_select"] else 1
        picks = [[0, 2], [1]] if multi == 0 else [[1], [0, 2]]
        ui.answer(rid, {"decision": "answer", "picks": picks})
        ok &= check("F14d the turn ends after the answer from Deck", ui.wait(lambda m: m["type"] == "agent_changed" and m["status"] == "done", 60, s) is not None)
        text = open(transcript(e)).read()
        expected = [", ".join(q["options"][i]["label"] for i in p) for q, p in zip(qs, picks)]
        ok &= check("F14e the model got exactly these answers", all(x in text for x in expected) and "have been answered" in text, " / ".join(expected))
    finally:
        done(e, s)
    return ok

if __name__ == "__main__":
    runs = {"deny": deny, "always": always, "timeout": timeout, "presence": presence, "figures": figures, "question": question}
    which = sys.argv[1] if len(sys.argv) > 1 else "all"
    keys = list(runs) if which == "all" else [which]
    results = {k: runs[k]() for k in keys}
    print(results)
    sys.exit(0 if all(results.values()) else 1)
