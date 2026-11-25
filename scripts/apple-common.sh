# shellcheck shell=bash
# Shared by the release scripts (sourced, not run).
#
# notarytool signs in with a keychain profile of this Mac (NOTARY_PROFILE,
# default AmiscaraDeck) or with an App Store Connect API key:
#   APPLE_API_KEY_PATH, APPLE_API_KEY_ID, APPLE_API_ISSUER_ID

# shellcheck disable=SC2034 # used by the scripts that source this
team="${APPLE_TEAM_ID:-7N2W4T6UPV}"
spec="apps/AmiscaraDeck/project.yml"

die() { echo "$(basename "$0"): $*" >&2; exit 1; }
step() { echo "==> $*"; }

has_api_key() {
    [[ -n "${APPLE_API_KEY_PATH:-}" && -n "${APPLE_API_KEY_ID:-}" && -n "${APPLE_API_ISSUER_ID:-}" ]]
}

marketing_version() { sed -n 's/^ *MARKETING_VERSION: "\(.*\)"$/\1/p' "$spec"; }
build_number() { sed -n 's/^ *CURRENT_PROJECT_VERSION: "\(.*\)"$/\1/p' "$spec"; }

# Stops unless the keychain holds a signing identity of the team, e.g.
# require_identity "Developer ID Application".
require_identity() {
    local identities
    identities=$(security find-identity -v -p codesigning)
    grep -q "$1: .*($team)" <<<"$identities" \
        || die "no '$1' certificate for team $team in the keychain"
}

require_clean_tree() {
    if [[ -n "$(git status --porcelain)" ]]; then
        die "working tree not clean; commit first so the build matches a revision"
    fi
}
