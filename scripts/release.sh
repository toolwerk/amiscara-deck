#!/usr/bin/env bash
# Builds, signs (Developer ID), notarizes and staples the Mac app.
#
#   scripts/release.sh                   build, notarize, verify; zip in target/release-app
#   scripts/release.sh --install         also copy the app to /Applications
#   scripts/release.sh --skip-notarize   build and sign only (a dry run, not for others)
#
# Besides the zip it writes appcast.xml, the update feed (scripts/sparkle-feed.sh).
# Nothing is published here: scripts/publish.sh does that.
#
# One-time setup, see docs/release.md:
#   - Sparkle signing key in the login keychain (account amiscara-deck), its
#     public key in project.yml as SUPublicEDKey
#   - "Developer ID Application" certificate in the login keychain
#   - xcrun notarytool store-credentials AmiscaraDeck --team-id 7N2W4T6UPV
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/apple-common.sh
source scripts/apple-common.sh

out="target/release-app"
install=false
notarize=true
for arg in "$@"; do
    case "$arg" in
        --install) install=true ;;
        --skip-notarize) notarize=false ;;
        *) die "unknown option $arg" ;;
    esac
done

profile="${NOTARY_PROFILE:-AmiscaraDeck}"
if has_api_key; then
    notary_auth=(--key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY_ID" --issuer "$APPLE_API_ISSUER_ID")
else
    notary_auth=(--keychain-profile "$profile")
fi

# Preconditions first, so nothing is built that cannot be shipped.
require_identity "Developer ID Application"
command -v xcodegen >/dev/null || die "xcodegen not found (brew install xcodegen)"
if $notarize; then
    xcrun notarytool history "${notary_auth[@]}" >/dev/null 2>&1 \
        || die "notarytool cannot sign in (profile '$profile' or API key missing); see docs/release.md"
    require_clean_tree
    sparkle_key=$(sed -n 's/^ *SUPublicEDKey: \(.*\)$/\1/p' "$spec")
    [[ -n "$sparkle_key" && "$sparkle_key" != __* ]] \
        || die "SUPublicEDKey in project.yml is still the placeholder; the release could not update itself"
fi
version=$(marketing_version)
build=$(build_number)
[[ -n "$version" && -n "$build" ]] || die "no version in $spec"
cargo_version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
[[ "$cargo_version" == "$version" ]] || die "version differs: Cargo.toml $cargo_version, $spec $version"
notes=$(python3 scripts/release-notes.py show "$version")

name="AmiscaraDeck-$version-$build"
step "Amiscara Deck $version ($build), $(git rev-parse --short HEAD)"

rm -rf "$out"
mkdir -p "$out"
printf '%s\n' "$notes" >"$out/$name.md"

step "Rust core and Xcode project"
scripts/build-ffi.sh release
(cd apps/AmiscaraDeck && xcodegen generate --quiet)

step "Archive"
xcodebuild -project apps/AmiscaraDeck/AmiscaraDeck.xcodeproj -scheme AmiscaraDeck \
    -configuration Release -destination 'generic/platform=macOS' ARCHS=arm64 \
    -derivedDataPath "$out/derived" -archivePath "$out/AmiscaraDeck.xcarchive" \
    -xcconfig scripts/Signing-DeveloperID.xcconfig archive -quiet

step "Export with Developer ID"
xcodebuild -exportArchive -archivePath "$out/AmiscaraDeck.xcarchive" \
    -exportOptionsPlist scripts/ExportOptions.plist -exportPath "$out/export" -quiet
app="$out/export/AmiscaraDeck.app"
codesign --verify --deep --strict --verbose=2 "$app"
# The helpers must be signed like the app, or notarization refuses them.
for bin in "$app"/Contents/Helpers/*; do
    info=$(codesign -dv --verbose=4 "$bin" 2>&1)
    grep -q "^Authority=Developer ID Application" <<<"$info" || die "$bin is not signed with Developer ID"
    grep -q "flags=.*runtime" <<<"$info" || die "$bin has no hardened runtime"
    grep -q "^Timestamp=" <<<"$info" || die "$bin has no secure timestamp"
done

if $notarize; then
    step "Notarize"
    ditto -c -k --keepParent "$app" "$out/notarize.zip"
    xcrun notarytool submit "$out/notarize.zip" "${notary_auth[@]}" --wait
    xcrun stapler staple "$app"
    rm -f "$out/notarize.zip"

    step "Verify"
    spctl -a -vvv -t exec "$app"
    xcrun stapler validate "$app"
else
    step "Not notarized: for this Mac only"
    name="$name-unnotarized"
fi

ditto -c -k --keepParent "$app" "$out/$name.zip"

# Only a notarized build may reach installed apps.
if $notarize; then
    step "Update feed"
    scripts/sparkle-feed.sh "$out/$name.zip" "$version" "$build"
fi

if $install; then
    step "Install to /Applications"
    ditto "$app" "/Applications/AmiscaraDeck.app"
fi

step "Done: $out/$name.zip"
