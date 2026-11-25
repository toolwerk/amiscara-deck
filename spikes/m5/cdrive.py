#!/usr/bin/env python3
"""M5-01: drive an interactive Codex session through a pty with the spike home."""
import fcntl, os, pty, re, select, signal, struct, sys, termios, time

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out")
HOME = os.path.join(OUT, "codex-home")
WORK = os.path.join(OUT, "work")
ANSI = re.compile(r"\x1b\[[0-9;?<>=]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b[()][A-Z0-9]|\x1b[=>78]|\r")

class Codex:
    def __init__(self, name="probe", args=(), sandbox="read-only"):
        os.makedirs(WORK, exist_ok=True)
        self.buf, self.mark_pos = "", 0
        self.log = open(os.path.join(OUT, f"tui-{name}.log"), "wb")
        env = dict(os.environ)
        env.update({"TERM": "xterm-256color", "CODEX_HOME": HOME, "DECK_SPIKE_OUT": OUT, "DECK_SPIKE_TAG": name})
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.chdir(WORK)
            os.execvpe("codex", ["codex", "-a", "on-request", "-s", sandbox, "--dangerously-bypass-hook-trust", *args], env)
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
                self.log.write(d); self.log.flush()
                self.buf += ANSI.sub("", re.sub(r"\x1b\[\d*C", " ", d.decode("utf-8", "replace")))
        return True

    def wait(self, pattern, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            self.pump(0.3)
            m = re.search(pattern, re.sub(r"\s+", "", self.buf[self.mark_pos:]), re.S)
            if m:
                return m
        return None

    def ready(self):
        """Get past the folder trust dialog; return when the input prompt shows."""
        self.pump(6)
        if "Trustandcontinue" in re.sub(r"\s+", "", self.buf):
            self.send("\r")
            self.pump(3)
        self.wait(r"Askcodextodoanything", timeout=40)
        self.pump(3)

    def mark(self): self.mark_pos = len(self.buf)
    def send(self, s): os.write(self.fd, s.encode())
    def screen(self, n=2500): return self.buf[-n:]

    def close(self):
        # Codex may ignore SIGTERM while it works: ask, then insist.
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try: os.kill(self.pid, sig)
            except ProcessLookupError: break
            end = time.time() + 3
            while time.time() < end:
                try:
                    done, _ = os.waitpid(self.pid, os.WNOHANG)
                except ChildProcessError:
                    return
                if done:
                    return
                time.sleep(0.1)

if __name__ == "__main__":
    c = Codex("probe")
    c.pump(12)
    print(c.screen(3000))
    c.close()
