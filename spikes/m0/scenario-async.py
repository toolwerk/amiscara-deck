#!/usr/bin/env python3
"""M0-05b: PreToolUse and Stop hooks sleep 8 s. async:true vs synchronous. Usage: scenario-async.py async|sync"""
import os, subprocess, sys, time
from drive import OUT, Session, summary
kind = sys.argv[1]
subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), "make-settings.py")] + (["--sync"] if kind == "sync" else []), capture_output=True)
open(os.path.join(OUT, "mode"), "w").write("none")
open(os.path.join(OUT, "async-sleep"), "w").write("8")
t0 = time.time()
s = Session("async-" + kind).start(); s.pump(3)
t1 = time.time()
s.mark(); s.send("Read the file probe.txt and reply with one word"); s.pump(1); s.send("\r")
m = s.wait(r"for\d+s·done|·done", timeout=60)
print(kind, "UI says done after", round(time.time() - t1, 1), "s")
s.pump(10)
print(summary(t1))
os.remove(os.path.join(OUT, "async-sleep"))
s.send("/exit\r"); s.pump(2); s.close()
subprocess.run([sys.executable, os.path.join(os.path.dirname(__file__), "make-settings.py")], capture_output=True)
