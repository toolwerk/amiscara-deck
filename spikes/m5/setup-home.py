#!/usr/bin/env python3
"""M5-01: a throw-away CODEX_HOME for the spike. ~/.codex is neither read for
settings nor changed: only the login file is copied (private, 0600) and the
copy is removed by `cleanup`.

Usage: setup-home.py [setup|cleanup]
"""
import json, os, shutil, stat, sys

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out")
HOME = os.path.join(OUT, "codex-home")
CAPTURE = os.path.join(HERE, "..", "m0", "hook-capture.py")
EVENTS = ["SessionStart", "UserPromptSubmit", "PreToolUse", "PostToolUse", "PermissionRequest", "Stop", "SessionEnd"]

def setup():
    os.makedirs(os.path.join(OUT, "events"), exist_ok=True)
    os.makedirs(HOME, exist_ok=True)
    os.chmod(HOME, 0o700)
    src = os.path.expanduser("~/.codex/auth.json")
    dst = os.path.join(HOME, "auth.json")
    fd = os.open(dst, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "wb") as f, open(src, "rb") as s:
        f.write(s.read())
    # No secrets in this file. the driver starts Codex with -a on-request -s read-only.
    open(os.path.join(HOME, "config.toml"), "w").write(
        '[features]\nhooks = true\n'
    )
    hooks = {}
    for e in EVENTS:
        h = {"type": "command", "command": f"{os.path.abspath(CAPTURE)}", "timeout": int(os.environ.get("DECK_SPIKE_TIMEOUT", "600"))}
        hooks[e] = [{"hooks": [h]}]
    json.dump({"hooks": hooks}, open(os.path.join(HOME, "hooks.json"), "w"), indent=2)

def cleanup():
    p = os.path.join(HOME, "auth.json")
    if os.path.exists(p):
        os.remove(p)
    shutil.rmtree(HOME, ignore_errors=True)

if __name__ == "__main__":
    {"setup": setup, "cleanup": cleanup}[sys.argv[1] if len(sys.argv) > 1 else "setup"]()
    print("ok", HOME)
