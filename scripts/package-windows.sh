#!/bin/sh
# Builds dist/amiscara-deck-<version>-windows-x86_64.zip with the daemon, the
# hook program and the command line. Cross-compiles in a Linux container
# (Docker or Podman) with MinGW, from macOS as well as from Linux.
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
engine=${CONTAINER_ENGINE:-$(command -v docker || command -v podman || true)}
[ -n "$engine" ] || { echo "Docker or Podman needed." >&2; exit 1; }
image=${RUST_IMAGE:-rust:1-bookworm}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
case $(uname -m) in
    arm64 | aarch64) platform=linux/arm64 ;;
    *) platform=linux/amd64 ;;
esac
target=x86_64-pc-windows-gnu
name=amiscara-deck-$version-windows-x86_64
mkdir -p dist

"$engine" run --rm --platform "$platform" \
    -v "$root":/src -w /src \
    -v deck-cargo-registry:/usr/local/cargo/registry \
    -v deck-target-windows:/src/target \
    -e CARGO_TERM_COLOR=never \
    "$image" sh -c "
        set -e
        command -v x86_64-w64-mingw32-gcc >/dev/null || { apt-get update -qq && apt-get install -y -qq gcc-mingw-w64-x86-64 >/dev/null; }
        rustup target add $target >/dev/null
        cargo build --release --locked --target $target -p deck-daemon -p deck-hook -p deck-cli >&2
        rm -rf /tmp/out && mkdir -p /tmp/out
        for p in amiscara-deckd deck-hook amiscara-deck; do cp target/$target/release/\$p.exe /tmp/out/; done
        tar -C /tmp/out -cf - .
    " > "dist/bin-windows.tar"

stage=$(mktemp -d)
mkdir "$stage/$name"
tar -C "$stage/$name" -xf dist/bin-windows.tar
rm dist/bin-windows.tar
cp scripts/windows/install.ps1 README.md README.en.md LICENSE-MIT LICENSE-APACHE "$stage/$name/"
rm -f "dist/$name.zip"
(cd "$stage" && zip -qr -X "$root/dist/$name.zip" "$name")
rm -rf "$stage"
echo "dist/$name.zip"
