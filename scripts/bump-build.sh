#!/usr/bin/env bash
# Raises CURRENT_PROJECT_VERSION in the app's project.yml by one and
# regenerates the Xcode project. Every notarized build gets its own number;
# numbers are never reused or lowered.
set -euo pipefail

cd "$(dirname "$0")/.."
# shellcheck source=scripts/apple-common.sh
source scripts/apple-common.sh

current=$(build_number)
[[ "$current" =~ ^[0-9]+$ ]] || die "CURRENT_PROJECT_VERSION not found in $spec"
next=$((current + 1))
sed -i '' "s/^\( *CURRENT_PROJECT_VERSION: \)\"$current\"$/\1\"$next\"/" "$spec"
if command -v xcodegen >/dev/null; then
    (cd apps/AmiscaraDeck && xcodegen generate --quiet)
fi
echo "build $current -> $next"
