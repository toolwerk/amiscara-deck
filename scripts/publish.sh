#!/usr/bin/env bash
# Publishes what release.sh built: the release v<version> with zip and notes,
# then the feed in the release `updates`, which every installed app reads.
# Everybody sees both at once, so it asks before it does anything.
#
#   scripts/publish.sh
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/apple-common.sh
source scripts/apple-common.sh

repo=toolwerk/amiscara-deck
out="target/release-app"
version=$(marketing_version)
build=$(build_number)
name="AmiscaraDeck-$version-$build"
zip="$out/$name.zip" notes="$out/$name.md" feed="$out/appcast.xml"
for f in "$zip" "$notes" "$feed"; do
    [[ -f "$f" ]] || die "missing $f; run scripts/release.sh first"
done
require_clean_tree
git fetch -q origin main
[[ "$(git rev-parse HEAD)" == "$(git rev-parse origin/main)" ]] || die "not on the published main"
xcrun stapler validate "$out/export/AmiscaraDeck.app" >/dev/null || die "the app is not notarized"
gh release view "v$version" --repo "$repo" >/dev/null 2>&1 && die "release v$version exists already"

echo "Publish Amiscara Deck $version ($build) from $(git rev-parse --short HEAD) to $repo?"
read -r -p "Type the version to publish: " answer
[[ "$answer" == "$version" ]] || die "nothing published"

step "Release v$version"
gh release create "v$version" "$zip" --repo "$repo" --target "$(git rev-parse HEAD)" \
    --title "Amiscara Deck $version" --notes-file "$notes"

step "Feed"
if ! gh release view updates --repo "$repo" >/dev/null 2>&1; then
    gh release create updates --repo "$repo" --title "Update-Feed" --latest=false \
        --notes "Der Feed, den installierte Apps lesen. Versionen liegen in den anderen Releases."
fi
gh release upload updates "$feed" --repo "$repo" --clobber
step "Done"
