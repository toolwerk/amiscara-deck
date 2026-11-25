#!/usr/bin/env python3
"""M0-05: (a) hooks added to a running session via the --settings file and via
.claude/settings.local.json; (b) async hook that sleeps: is Claude Code held back?"""
import json, os, time
from drive import OUT, WORK, Session, events, summary
H = json.load(open(os.path.join(OUT, "settings.json")))["hooks"]
empty = os.path.join(OUT, "settings-empty.json")
json.dump({}, open(empty, "w"))
local = os.path.join(WORK, ".claude", "settings.local.json")
os.makedirs(os.path.dirname(local), exist_ok=True)
json.dump({}, open(local, "w"))
open(os.path.join(OUT, "mode"), "w").write("none")
def ask(s, t):
    s.mark(); s.send(f"Reply with the single word {t}"); s.pump(1); s.send("\r")
    s.wait(r"for\d+s·done|·done", timeout=40); s.pump(2)
t0 = time.time()
s = Session("late", settings=empty).start(); s.pump(3)
ask(s, "one"); t1 = time.time()
print("A0 no hooks yet:", summary(t0))
json.dump({"hooks": H}, open(empty, "w"))            # stage A: edit the --settings file
time.sleep(2); ask(s, "two"); t2 = time.time()
print("A1 after editing --settings file:", summary(t1))
json.dump({}, open(empty, "w"))
json.dump({"hooks": H}, open(local, "w"))            # stage B: project-local settings
time.sleep(2); ask(s, "three"); t3 = time.time()
print("B after editing .claude/settings.local.json:", summary(t2))
json.dump({}, open(local, "w"))
s.send("/exit\r"); s.pump(2); s.close()
