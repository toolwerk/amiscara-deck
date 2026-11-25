#!/usr/bin/env python3
"""Write spikes/m0/out/settings.json: all events pointing at hook-capture.py.

PermissionRequest is the only synchronous hook. Others run with async: true
(argument --sync makes them synchronous too, for comparison in M0-05).
Optional: --timeout N sets the PermissionRequest timeout.
"""
import json, os, sys
HERE = os.path.dirname(os.path.abspath(__file__))
out = os.path.join(HERE, "out")
os.makedirs(os.path.join(out, "events"), exist_ok=True)
cmd = os.path.join(HERE, "hook-capture.py")
sync = "--sync" in sys.argv
timeout = int(sys.argv[sys.argv.index("--timeout") + 1]) if "--timeout" in sys.argv else 600
events = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest",
          "Notification", "SubagentStart", "SubagentStop", "PreCompact", "PostCompact", "CwdChanged", "Stop",
          "StopFailure", "SessionEnd"]
hooks = {}
for e in events:
    h = {"type": "command", "command": cmd}
    if e == "PermissionRequest":
        h["timeout"] = timeout
    elif not sync:
        h["async"] = True
    entry = {"hooks": [h]}
    if e in ("PreToolUse", "PostToolUse", "PostToolUseFailure", "PermissionRequest"):
        entry["matcher"] = "*"
    hooks[e] = [entry]
path = os.path.join(out, "settings.json")
json.dump({"hooks": hooks}, open(path, "w"), indent=2)
print(path)
