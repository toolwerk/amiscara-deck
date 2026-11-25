#!/usr/bin/env bash
# Builds deck-ffi for the Mac app: the static library wrapped as
# DeckFFI.xcframework plus the generated Swift bindings, both in
# apps/AmiscaraDeck/Generated (not checked in).
#
# Usage: scripts/build-ffi.sh [debug|release]
#
# Profile defaults to release. Mac only (Apple silicon) for now.
set -euo pipefail

cd "$(dirname "$0")/.."

profile="${1:-release}"
triple=aarch64-apple-darwin

# Xcode build phases run with a minimal PATH.
# Homebrew's rustup (the Mac runner) keeps cargo in its own folder.
# /usr/bin first: its `cc` is the shim that adds the SDK. The compiler inside
# the Xcode toolchain, which Xcode puts first in PATH, finds no system
# headers, and C code in the SSH crypto library would not build.
export PATH="/usr/bin:$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

cargo_profile=()
if [[ "$profile" == "release" ]]; then
    cargo_profile=(--release)
fi

# Xcode exports SDKROOT and deployment targets for its own platform. They must
# not leak into cargo: host proc-macros would be built against them and the
# host loader rejects what comes out (and never set MACOSX_DEPLOYMENT_TARGET
# for the Rust build).
clean_env() {
    env -u SDKROOT -u MACOSX_DEPLOYMENT_TARGET -u IPHONEOS_DEPLOYMENT_TARGET -u TVOS_DEPLOYMENT_TARGET "$@"
}

# ${a[@]+"${a[@]}"}: empty arrays count as unset under `set -u` in the bash 3.2
# that Xcode build phases run.
clean_env cargo build -p deck-ffi --target "$triple" ${cargo_profile[@]+"${cargo_profile[@]}"}

work="target/ffi"
out="apps/AmiscaraDeck/Generated"
rm -rf "$work"
mkdir -p "$work/swift" "$work/headers" "$out"

lib="target/$triple/$profile/libdeck_ffi.a"
clean_env cargo run -q -p uniffi-bindgen -- generate \
    --library "$lib" \
    --language swift \
    --out-dir "$work/swift"

cp "$work/swift/deck_ffiFFI.h" "$work/headers/"
cp "$work/swift/deck_ffiFFI.modulemap" "$work/headers/module.modulemap"

# Leave the framework alone when nothing changed: Xcode may already be
# reading it for the app target.
stamp="$(shasum "$lib" "$work/headers/deck_ffiFFI.h" | shasum | cut -d' ' -f1)"
if [[ -d "$out/DeckFFI.xcframework" && "$(cat "$out/.ffi-stamp" 2>/dev/null)" == "$stamp" ]] \
    && cmp -s "$work/swift/deck_ffi.swift" "$out/deck_ffi.swift"; then
    echo "deck-ffi ($profile) unchanged"
    exit 0
fi

# Build next to the old framework and swap, so Xcode never finds the path
# missing; touch the Swift file only when it changed (no needless recompiles).
xcodebuild -create-xcframework -library "$lib" -headers "$work/headers" \
    -output "$work/DeckFFI.xcframework" >/dev/null
rm -rf "$out/DeckFFI.xcframework.old"
if [[ -d "$out/DeckFFI.xcframework" ]]; then
    mv "$out/DeckFFI.xcframework" "$out/DeckFFI.xcframework.old"
fi
mv "$work/DeckFFI.xcframework" "$out/DeckFFI.xcframework"
rm -rf "$out/DeckFFI.xcframework.old"
if ! cmp -s "$work/swift/deck_ffi.swift" "$out/deck_ffi.swift"; then
    cp "$work/swift/deck_ffi.swift" "$out/"
fi
echo "$stamp" > "$out/.ffi-stamp"

echo "deck-ffi ($profile) -> $out"
