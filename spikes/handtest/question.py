#!/usr/bin/env python3
"""Spike: does an answer from a hook reach Claude Code like one typed in the
terminal? Throw-away folder and settings, ~/.claude/settings.json untouched."""
import glob, json, os, re, sys, tempfile, time
HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "..", "m0"))
import drive

def run(name, prompt, pick, sleep):
    tmp = tempfile.mkdtemp(prefix="deck-q-")
    out, work = os.path.join(tmp, "out"), os.path.join(tmp, "work")
    os.makedirs(os.path.join(out, "events")); os.makedirs(work)
    drive.OUT = out
    json.dump({"sleep": sleep, "pick": pick}, open(os.path.join(out, "question-answer.json"), "w"))
    hook = os.path.join(HERE, "question-hook.py")
    settings = os.path.join(tmp, "settings.json")
    json.dump({"hooks": {"PermissionRequest": [{"matcher": "*", "hooks": [{"type": "command", "command": f"DECK_Q_OUT={out} {sys.executable} {hook}", "timeout": 60}]}]}}, open(settings, "w"))
    s = drive.Session(name, cwd=work, settings=settings)
    s.start(); s.pump(3)
    s.mark(); s.send(prompt); s.pump(1); s.send("\r")
    shown = s.wait(r"Apfel", 60)
    s.pump(2)
    t_dialog = time.time()
    end = time.time() + 60
    while time.time() < end and not os.path.exists(os.path.join(out, "answered")):
        s.pump(0.5)
    s.pump(12)
    screen = s.screen(1200)
    real = os.path.realpath(work)
    d = os.path.expanduser("~/.claude/projects/" + re.sub(r"[^A-Za-z0-9]", "-", real))
    results = []
    for f in glob.glob(os.path.join(d, "*.jsonl")):
        for line in open(f):
            j = json.loads(line)
            c = (j.get("message") or {}).get("content")
            if j.get("type") == "user" and isinstance(c, list):
                results += [str(x.get("content"))[:300] for x in c if x.get("type") == "tool_result"]
            if j.get("type") == "assistant" and isinstance(c, list):
                results += ["assistant: " + x["text"][:200] for x in c if x.get("type") == "text"]
    hooks = [json.load(open(f)) for f in glob.glob(os.path.join(out, "q-*.json"))]
    print(f"== {name}: dialog_text_seen={bool(shown)} hook_called={bool(hooks)} answered={os.path.exists(os.path.join(out, 'answered'))}")
    for r in results:
        print("  ", r)
    print("--- screen tail ---")
    print(screen[-600:])
    s.send("\x1b"); s.pump(0.5); s.send("/exit\r"); s.pump(2); s.close()

if __name__ == "__main__":
    which = sys.argv[1] if len(sys.argv) > 1 else "single"
    if which == "single":
        run("single", "Use the AskUserQuestion tool to ask me one question: which fruit I prefer, options Apfel and Birne. Then reply with exactly the option I chose.", [[1]], 5)
    else:
        run("multi", "Use the AskUserQuestion tool once with two questions: (1) which fruits I like, multiSelect true, options Apfel, Birne, Kirsche; (2) which drink, options Tee and Kaffee. Then list my answers.", [[0, 2], [1]], 3)
