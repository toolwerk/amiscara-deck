#!/usr/bin/env python3
"""M0-04: permission_suggestions and updatedPermissions.
Run 1 (mode none): trigger several tools, report permission_suggestions per tool.
Run 2 (mode suggest:0): allow with the first suggestion, then check whether the rule exists."""
import json, os, sys, time
from drive import OUT, WORK, Session, events, set_mode, summary

mode = sys.argv[1]
set_mode(mode)
t0 = time.time()
s = Session("suggest-" + mode.replace(":", "-")).start()
s.pump(3)
prompts = ["Run exactly this bash command and nothing else: python3 --version",
           "Run exactly this bash command and nothing else: curl --version",
           "Run exactly this bash command and nothing else: mkdir m0-dir && rmdir m0-dir"]
for p in prompts:
    s.mark(); s.send(p); s.pump(1); s.send("\r")
    d = s.wait(r"Doyouwanttoproceed|for\d+s·done", timeout=45)
    s.pump(1)
    if d and "proceed" in d.group(0):
        s.send("\r")  # first option: plain yes
    s.wait(r"for\d+s·done", timeout=40)
    s.pump(1)
for t, e, d in events(t0):
    if e == "PermissionRequest":
        i = d["input"]
        print(i["tool_name"], json.dumps(i["tool_input"])[:120], "permission_mode:", i.get("permission_mode"))
        print("  keys:", sorted(i.keys()))
        print("  suggestions:", json.dumps(i.get("permission_suggestions")))
print(summary(t0))
s.send("/exit\r"); s.pump(2); s.close()
