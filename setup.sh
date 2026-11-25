#!/usr/bin/env bash
# Prepares a fresh checkout: toolchain check, build, tests and the Xcode project.
set -euo pipefail

cd "$(dirname "$0")"

if ! command -v cargo >/dev/null 2>&1; then
    echo "cargo not found. Install Rust from https://rustup.rs and run this again." >&2
    exit 1
fi
if command -v rustup >/dev/null 2>&1; then
    rustup component add rustfmt clippy
fi

if [[ -f Cargo.toml ]]; then
    cargo build --workspace
    cargo test --workspace
else
    echo "No Cargo workspace here."
fi

# Mac app: Rust library, Swift bindings, Xcode project from project.yml (from M2-04).
if [[ "$(uname)" == "Darwin" && -f apps/AmiscaraDeck/project.yml ]]; then
    if ! command -v xcodegen >/dev/null 2>&1; then
        echo "xcodegen not found: brew install xcodegen" >&2
        exit 1
    fi
    rustup target add aarch64-apple-darwin
    scripts/build-ffi.sh debug
    (cd apps/AmiscaraDeck && xcodegen generate)
fi
