#!/usr/bin/env python3
"""Copy one sanitized sample per event (and Notification type, PermissionRequest tool) from out/events to testdata/hooks."""
import glob, json, os, re
HERE = os.path.dirname(os.path.abspath(__file__))
DEST = os.path.join(HERE, "..", "..", "testdata", "hooks")
os.makedirs(DEST, exist_ok=True)
SCRATCH = re.compile(r"/private/tmp/claude-501/-Users-staude/[0-9a-f-]+/scratchpad/m0-work")
SCRATCH2 = re.compile(r"/private/tmp/claude-501/-Users-staude/[0-9a-f-]+/scratchpad")
def clean(o):
    s = json.dumps(o)
    s = SCRATCH.sub("/work/project", s)
    s = SCRATCH2.sub("/work/scratchpad", s)
    s = s.replace("-private-tmp-claude-501--Users-staude-" , "-work-project-").replace("/Users/staude", "/Users/example")
    s = re.sub(r"-[0-9a-f]{8}-[0-9a-f-]{27}-scratchpad-m0-work", "-scratchpad-m0-work", s)
    s = s.replace("staude", "example")
    return json.loads(s)
best = {}
for f in sorted(glob.glob(os.path.join(HERE, "out", "events", "*.json"))):
    d = json.load(open(f))
    i = d["input"]
    key = d["event"] + ("-" + i["notification_type"] if i.get("notification_type") else "") + ("-" + i["tool_name"] if d["event"] == "PermissionRequest" and i.get("permission_suggestions") else "")
    if d["event"] == "PermissionRequest":
        key += "-" + (i["permission_suggestions"][0]["type"] if i.get("permission_suggestions") else "nosugg")
    if d["event"] in ("PreToolUse", "PostToolUse", "PostToolUseFailure"):
        key += "-" + i.get("tool_name", "")
    best[key] = d
for key, d in best.items():
    out = {"event": d["event"], "input": clean(d["input"]), "env": clean({k: v for k, v in d["env"].items() if k in ("HERDR_PANE_ID", "TMUX_PANE", "TERM_PROGRAM", "CLAUDE_PROJECT_DIR")}),
           "parents": clean([p["comm"] for p in d["parents"]])}
    json.dump(out, open(os.path.join(DEST, key + ".json"), "w"), indent=1, ensure_ascii=False)
print(sorted(best))
