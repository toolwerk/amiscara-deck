#!/usr/bin/env python3
"""M0 spike: drive an interactive Claude Code session through a pty.

Every scenario starts its own session in a throw-away folder with only the spike
hooks (--setting-sources project,local --settings out/settings.json), so the
user's own hooks and ~/.claude/settings.json are neither read nor changed.
"""
import fcntl, glob, json, os, pty, re, select, signal, struct, sys, termios, time

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out")
WORK = os.environ.get("DECK_SPIKE_WORK", os.path.join(OUT, "work"))
ANSI = re.compile(r"\x1b\[[0-9;?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[()][A-Z0-9]|\x1b[=>78]|\r")


class Session:
    def __init__(self, name, extra_args=(), cwd=None, settings=None, env_extra=None, model="haiku"):
        cwd = cwd or WORK
        os.makedirs(cwd, exist_ok=True)
        self.name, self.buf, self.mark_pos = name, "", 0
        self.log = open(os.path.join(OUT, f"tui-{name}.log"), "wb")
        env = {k: v for k, v in os.environ.items()
               if k != "CLAUDECODE" and not k.startswith("CLAUDE_CODE_") and not k.startswith("CLAUDE_AGENT")}
        env.update({"TERM": "xterm-256color", "DECK_SPIKE_TAG": name})
        env.update(env_extra or {})
        argv = ["claude", "--model", model, "--setting-sources", "project,local",
                "--settings", settings or os.path.join(OUT, "settings.json"), "--strict-mcp-config",
                "--permission-mode", "default", *extra_args]
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(cwd)
            os.execvpe("claude", argv, env)
        fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", 45, 140, 0, 0))

    def pump(self, secs=0.3):
        end = time.time() + secs
        while time.time() < end:
            r, _, _ = select.select([self.fd], [], [], 0.1)
            if r:
                try:
                    d = os.read(self.fd, 65536)
                except OSError:
                    return False
                if not d:
                    return False
                self.log.write(d)
                self.log.flush()
                self.buf += ANSI.sub("", re.sub(r"\x1b\[\d*C", " ", d.decode("utf-8", "replace")))
        return True

    def wait(self, pattern, timeout=60):
        """Wait until the regex matches in text received after the last mark()."""
        end = time.time() + timeout
        while time.time() < end:
            self.pump(0.3)
            m = re.search(pattern, re.sub(r"\s+", "", self.buf[self.mark_pos:]), re.S)
            if m:
                return m
        return None

    def mark(self):
        self.mark_pos = len(self.buf)

    def send(self, s):
        os.write(self.fd, s.encode())

    def screen(self, n=1800):
        return self.buf[-n:]

    def close(self):
        try:
            os.kill(self.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            os.waitpid(self.pid, 0)
        except ChildProcessError:
            pass

    def start(self):
        """Get past the trust dialog; return when the input prompt shows."""
        self.mark()
        for _ in range(40):
            self.pump(0.5)
            t = self.buf[self.mark_pos:]
            # Act only on the whole dialog (its footer is printed last).
            if re.search(r"trust", t, re.I) and "Entertoconfirm" in re.sub(r"\s+", "", t):
                # The cursor may start on "No, exit": move it until the latest
                # frame shows it on "Yes", then confirm.
                self.pump(1.5)
                for _ in range(4):
                    flat = re.sub(r"\s+", "", self.buf[self.mark_pos:])
                    if flat[flat.rfind("❯"):].startswith("❯Yes"):
                        break
                    self.send("\x1b[B")
                    self.pump(1.0)
                self.send("\r")
                self.pump(1)
                self.mark()
                continue
            if re.search(r"\? for shortcuts|accept edits|esc to interrupt", t):
                break
        self.pump(1.0)
        return self


def set_mode(m):
    open(os.path.join(OUT, "mode"), "w").write(m)


def events(since):
    res = []
    for f in glob.glob(os.path.join(OUT, "events", "*.json")):
        d = json.load(open(f))
        if d["ts"] >= since - 0.01:
            res.append((d["ts"], d["event"], d))
    return sorted(res, key=lambda x: x[0])


def summary(since):
    return [(round(t - since, 2), e, (d["input"].get("tool_name") or "") + " " + str(d["input"].get("notification_type") or ""))
            for t, e, d in events(since)]
