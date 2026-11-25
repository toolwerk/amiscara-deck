#!/usr/bin/env python3
"""M0-03: PermissionRequest answers. Usage: scenario-perm.py <mode>
Modes: none allow deny garbage exit1 exit2 sleep:N (see hook-capture.py).
Sends a prompt that needs a Bash permission, reports what the TUI shows and which events fired."""
import os
import sys
import time

from drive import OUT, Session, set_mode, summary

mode = sys.argv[1]
word = "m0-" + mode.replace(":", "-")
set_mode(mode)
t0 = time.time()
s = Session("perm-" + mode.replace(":", "-")).start()
s.pump(3)
s.mark()
s.send(f"Run exactly this bash command and nothing else: touch {word}.txt")
s.pump(1)
s.send("\r")
dialog = s.wait(r"Doyouwanttoproceed", timeout=45)
print(f"mode={mode} dialog_visible={bool(dialog)} at={time.time() - t0:.2f}s (abs {time.time():.2f})")
if dialog:
    print("--- screen at dialog ---")
    print(s.screen(1500))
    if "--no-answer" not in sys.argv:
        s.send("\r")  # answer in the terminal: first option
done = s.wait(r"for\d+s·done|Denied|denied|interrupted", timeout=40)
s.pump(2)
print("tool result seen:", bool(done), "file created:", os.path.exists(os.path.join(os.environ.get("DECK_SPIKE_WORK", ""), word + ".txt")))
print("--- tail ---")
print(s.screen(1200))
print("--- events ---")
for x in summary(t0):
    print(x)
os.system(f"tail -4 {OUT}/decisions.log")
s.send("/exit\r")
s.pump(2)
s.close()
