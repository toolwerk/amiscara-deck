#!/usr/bin/env python3
"""M0-04 part 2: answer with updatedPermissions (first suggestion) and check the rule appears.
Then ask the same command again: no PermissionRequest should fire."""
import os, time
from drive import OUT, WORK, Session, events, set_mode, summary
set_mode("suggest:0")
t0 = time.time()
s = Session("apply").start(); s.pump(3)
for _ in range(2):
    s.mark(); s.send("Run exactly this bash command and nothing else: curl --version"); s.pump(1); s.send("\r")
    s.wait(r"for\d+s·done", timeout=45); s.pump(1)
print(summary(t0))
p = os.path.join(WORK, ".claude", "settings.local.json")
print("settings.local.json:", open(p).read() if os.path.exists(p) else "missing")
s.send("/exit\r"); s.pump(2); s.close()
