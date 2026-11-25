#!/usr/bin/env bash
# Builds the daemon, the hook program and the command line and copies them
# into the app: scripts/embed-helpers.sh [debug|release] <Helpers folder>
set -euo pipefail

cd "$(dirname "$0")/.."

profile="${1:-release}"
dest="${2:?usage: embed-helpers.sh [debug|release] <Helpers folder>}"

# Homebrew's rustup (the Mac runner) keeps cargo in its own folder.
export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

cargo_profile=()
if [[ "$profile" == "release" ]]; then
    cargo_profile=(--release)
fi

# Same reason as in build-ffi.sh: Xcode's SDK and deployment variables must not
# reach cargo.
env -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET -u IPHONEOS_DEPLOYMENT_TARGET -u TVOS_DEPLOYMENT_TARGET \
    cargo build --target aarch64-apple-darwin \
    -p deck-daemon -p deck-hook -p deck-cli ${cargo_profile[@]+"${cargo_profile[@]}"}

# Sign like the app, before Xcode seals it: notarization wants every program
# signed with Developer ID, with the hardened runtime and a secure timestamp.
# Without an identity (CI builds unsigned) cargo's ad hoc signature stays.
sign() {
    local identity="${EXPANDED_CODE_SIGN_IDENTITY:-}"
    [[ "${CODE_SIGNING_ALLOWED:-YES}" == "YES" && -n "$identity" && "$identity" != "-" ]] || return 0
    local timestamp=--timestamp=none
    case "${CODE_SIGN_IDENTITY:-}${EXPANDED_CODE_SIGN_IDENTITY_NAME:-}" in
        *"Developer ID"*) timestamp=--timestamp ;;
    esac
    codesign --force --options runtime "$timestamp" --sign "$identity" \
        --identifier "net.staude.amiscara.deck.$(basename "$1")" "$1"
}

mkdir -p "$dest"
for bin in amiscara-deckd deck-hook amiscara-deck; do
    src="target/aarch64-apple-darwin/$profile/$bin"
    # Replace only when it changed: a running daemon keeps its old copy open.
    # A signed copy always differs from what cargo built, so it is replaced.
    if ! cmp -s "$src" "$dest/$bin" 2>/dev/null; then
        cp -f "$src" "$dest/$bin.new"
        mv -f "$dest/$bin.new" "$dest/$bin"
    fi
    sign "$dest/$bin"
done
echo "helpers ($profile) -> $dest"
