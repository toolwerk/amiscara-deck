#!/usr/bin/env python3
"""M0 spike: write every hook input to a file, answer PermissionRequest per mode.

Mode comes from the file spikes/m0/out/mode (read at call time, so it can change
while a session runs): allow | deny | none | garbage | exit1 | exit2 |
sleep:<seconds> | suggest | suggest:<index>
Default: none (no output, exit 0).
"""
import json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.environ.get("DECK_SPIKE_OUT", os.path.join(HERE, "out"))
os.makedirs(OUT, exist_ok=True)

raw = sys.stdin.read()
try:
    data = json.loads(raw)
except Exception:
    data = {"_unparsable": raw}

event = data.get("hook_event_name", "unknown")
ts = time.time()

def chain():
    res, pid = [], os.getppid()
    for _ in range(12):
        if pid <= 1:
            break
        r = subprocess.run(["ps", "-o", "ppid=,comm=", "-p", str(pid)], capture_output=True, text=True).stdout.strip()
        if not r:
            break
        pp, comm = r.split(None, 1)
        res.append({"pid": pid, "comm": comm})
        pid = int(pp)
    return res

env_keys = ["HERDR_PANE_ID", "HERDR_ENV", "TMUX_PANE", "TERM_PROGRAM", "TERM_SESSION_ID", "ITERM_SESSION_ID",
            "CLAUDE_PROJECT_DIR", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_ENV_FILE", "CLAUDECODE", "PWD"]
rec = {
    "ts": ts,
    "event": event,
    "input": data,
    "env": {k: os.environ.get(k) for k in env_keys if os.environ.get(k) is not None},
    "env_claude_keys": sorted(k for k in os.environ if k.startswith("CLAUDE")),
    "parents": chain(),
}
tag = os.environ.get("DECK_SPIKE_TAG", "")
name = f"{int(ts*1000)}-{event}{('-' + tag) if tag else ''}-{os.getpid()}.json"
with open(os.path.join(OUT, "events", name) if os.path.isdir(os.path.join(OUT, "events")) else os.path.join(OUT, name), "w") as f:
    json.dump(rec, f, indent=1)

if event != "PermissionRequest":
    try:
        n = float(open(os.path.join(OUT, "async-sleep")).read().strip())
        if event in ("PreToolUse", "Stop"):
            time.sleep(n)  # M0-05: does a slow hook hold Claude Code back?
    except (OSError, ValueError):
        pass
    sys.exit(0)

mode = "none"
try:
    mode = open(os.path.join(OUT, "mode")).read().strip() or "none"
except OSError:
    pass

def log(msg):
    with open(os.path.join(OUT, "decisions.log"), "a") as f:
        f.write(f"{time.time():.3f} {data.get('session_id','?')[:8]} {data.get('tool_name')} mode={mode} {msg}\n")

def out(decision, extra=None):
    d = dict(decision)
    if extra:
        d.update(extra)
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": d}}))

log("received")
if mode == "allow":
    out({"behavior": "allow"})
elif mode == "deny":
    out({"behavior": "deny", "message": "Im Amiscara Deck abgelehnt (M0 spike)"})
elif mode == "garbage":
    print("this is not json {")
elif mode == "exit1":
    sys.exit(1)
elif mode == "exit2":
    print("blocked by spike", file=sys.stderr)
    sys.exit(2)
elif mode.startswith("sleep:"):
    time.sleep(float(mode.split(":")[1]))
    log("woke up, answering allow")
    out({"behavior": "allow"})
elif mode.startswith("suggest"):
    sugg = data.get("permission_suggestions") or []
    idx = int(mode.split(":")[1]) if ":" in mode else 0
    if sugg:
        out({"behavior": "allow", "updatedPermissions": [sugg[idx]]})
        log(f"allow with suggestion {idx}: {json.dumps(sugg[idx])}")
    else:
        log("no suggestions, no output")
else:
    pass
