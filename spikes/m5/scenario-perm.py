#!/usr/bin/env python3
"""M5-01: what Codex does with a PermissionRequest hook. Usage: scenario-perm.py <mode> [--no-answer]
Modes as in spikes/m0/hook-capture.py: none allow deny garbage exit1 exit2 sleep:N.
Asks Codex to write a file (the read-only sandbox makes it ask), then reports
whether the dialog showed, what the hook saw and whether the file appeared."""
import glob, json, os, sys, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from cdrive import Codex, OUT, WORK

mode = sys.argv[1]
word = "m5-" + mode.replace(":", "-")
open(os.path.join(OUT, "mode"), "w").write(mode)
for f in glob.glob(os.path.join(OUT, "events", "*.json")):
    os.remove(f)
t0 = time.time()
c = Codex("perm-" + mode.replace(":", "-"))
c.ready()
c.mark()
for ch in f"Please run this shell command and nothing else: touch {word}.txt":
    c.send(ch)
    c.pump(0.03)
c.pump(1)
print("--- typed ---")
print(c.screen(400))
c.send("\r")
dialog = c.wait(r"(Wouldyouliketorun|Allow|approve|Yes,proceed|Yes,)", timeout=60)
print(f"mode={mode} dialog_visible={bool(dialog)} after={time.time() - t0:.1f}s")
print("--- screen ---")
print(c.screen(1800))
if dialog and "--no-answer" not in sys.argv:
    c.send("\r")
c.pump(8)
print("file created:", os.path.exists(os.path.join(WORK, word + ".txt")))
print("--- events ---")
for f in sorted(glob.glob(os.path.join(OUT, "events", "*.json"))):
    d = json.load(open(f))
    print(round(d["ts"] - t0, 1), d["event"], d["input"].get("tool_name"), json.dumps(d["input"].get("tool_input"))[:100])
os.system(f"tail -3 {OUT}/decisions.log 2>/dev/null")
c.close()
