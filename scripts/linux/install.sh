#!/bin/sh
# Installs Amiscara Deck for the current user: copies the programs to
# ~/.local/bin (or the folder given), installs the hooks in Claude Code and
# starts the daemon at login as a systemd user unit.
#
#   ./install.sh                 # ~/.local/bin
#   ./install.sh /opt/deck/bin   # another folder
#   ./install.sh --uninstall     # unit and hooks out, programs stay
set -eu

here=$(cd "$(dirname "$0")" && pwd)
if [ "${1:-}" = "--uninstall" ]; then
    bin=${2:-$HOME/.local/bin}
    "$bin/amiscara-deck" daemon uninstall || true
    "$bin/amiscara-deck" hooks uninstall || true
    echo "Programs remain in $bin."
    exit 0
fi
bin=${1:-$HOME/.local/bin}
mkdir -p "$bin"
for p in amiscara-deckd deck-hook amiscara-deck; do
    install -m 755 "$here/bin/$p" "$bin/$p"
done
echo "Programs in $bin"
"$bin/amiscara-deck" hooks install
"$bin/amiscara-deck" daemon install
"$bin/amiscara-deck" daemon status
