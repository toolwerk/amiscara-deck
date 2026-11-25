#!/usr/bin/env bash
# Signs the zip of the Mac app with the Sparkle key and writes appcast.xml next
# to it, the feed for the release `updates` (docs/release.md). release.sh runs
# it at the end.
#
#   scripts/sparkle-feed.sh <zip> <version> <build>
#
# The key comes from the login keychain (account amiscara-deck).
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/apple-common.sh
source scripts/apple-common.sh

zip="${1:?zip}" version="${2:?version}" build="${3:?build}"
[[ -f "$zip" ]] || die "no zip at $zip"
# Sparkle comes as a Swift package; its tools are in the derived data.
sign_update=$(find target/release-app/derived/SourcePackages/artifacts target/xcode/SourcePackages/artifacts \
    -path '*/old_dsa_scripts' -prune -o -type f -name sign_update -print 2>/dev/null | head -1 || true)
[[ -x "$sign_update" ]] || die "Sparkle's sign_update not found; build the app first"

signature=$("$sign_update" --account amiscara-deck "$zip")
ed_signature=$(sed -nE 's/.*sparkle:edSignature="([^"]+)".*/\1/p' <<<"$signature")
length=$(sed -nE 's/.*length="([0-9]+)".*/\1/p' <<<"$signature")
[[ -n "$ed_signature" && -n "$length" ]] || die "cannot read the output of sign_update"

tag="v$version"
out=$(dirname "$zip")
appcast="$out/appcast.xml"
# The feed carries the notes in both languages; the app shows its own.
for lang in en de; do
    python3 scripts/release-notes.py show "$version" --lang "$lang" >"$out/notes-$lang.md"
done
OUTPUT_PATH="$appcast" VERSION="$version" BUILD="$build" \
    ZIP_URL="https://github.com/toolwerk/amiscara-deck/releases/download/$tag/$(basename "$zip")" \
    ZIP_LENGTH="$length" ZIP_ED_SIGNATURE="$ed_signature" \
    MIN_SYSTEM="$(sed -n 's/^ *macOS: "\(.*\)"$/\1/p' "$spec" | head -1)" \
    NOTES_EN_PATH="$out/notes-en.md" NOTES_DE_PATH="$out/notes-de.md" \
    PUB_DATE="$(LC_ALL=C date -u '+%a, %d %b %Y %H:%M:%S +0000')" \
    python3 scripts/appcast.py
step "Feed: $appcast (for release $tag)"
