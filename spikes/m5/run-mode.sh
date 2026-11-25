#!/usr/bin/env bash
# Runs one scenario and prints what matters (no TUI noise).
cd "$(dirname "$0")"
python3 scenario-perm.py "$1" 2>&1 | python3 -c "
import sys, re
t = re.sub(r'[⠀-⣿]+', '', sys.stdin.read())
m = re.search(r'mode=\S+ dialog_visible=\S+ after=\S+', t); print(m.group(0) if m else 'no header')
print(t[t.find('file created'):])"
