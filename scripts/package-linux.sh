#!/bin/sh
# Builds dist/amiscara-deck-<version>-linux-<arch>.tar.gz with the daemon, the
# hook program and the command line, linked statically against musl, so they
# run on any Linux. Runs the build in a container (Docker or Podman), from
# macOS as well as from Linux.
#
#   scripts/package-linux.sh                  # x86_64 and aarch64
#   scripts/package-linux.sh aarch64          # one architecture
#
# The container runs in the architecture of this machine and links both
# targets with rust-lld and the C library that Rust brings for musl, so no
# cross toolchain and no emulation are needed.
set -eu

cd "$(dirname "$0")/.."
root=$(pwd)
engine=${CONTAINER_ENGINE:-$(command -v docker || command -v podman || true)}
[ -n "$engine" ] || { echo "Docker or Podman needed." >&2; exit 1; }
image=${RUST_IMAGE:-rust:1-bookworm}
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
archs=${*:-x86_64 aarch64}
case $(uname -m) in
    arm64 | aarch64) platform=linux/arm64 ;;
    *) platform=linux/amd64 ;;
esac
mkdir -p dist

for arch in $archs; do
    case $arch in
        x86_64 | aarch64) ;;
        *) echo "Unknown architecture: $arch (x86_64 or aarch64)" >&2; exit 2 ;;
    esac
    target=$arch-unknown-linux-musl
    linker=CARGO_TARGET_$(echo "$target" | tr 'a-z-' 'A-Z_')_LINKER
    echo "== $target"
    "$engine" run --rm --platform "$platform" \
        -v "$root":/src -w /src \
        -v deck-cargo-registry:/usr/local/cargo/registry \
        -v deck-target-musl:/src/target \
        -e CARGO_TERM_COLOR=never \
        -e "$linker=rust-lld" \
        "$image" sh -c "
            set -e
            rustup target add $target >/dev/null
            cargo build --release --locked --target $target -p deck-daemon -p deck-hook -p deck-cli >&2
            rm -rf /tmp/out && mkdir -p /tmp/out/bin
            for p in amiscara-deckd deck-hook amiscara-deck; do cp target/$target/release/\$p /tmp/out/bin/; done
            tar -C /tmp/out -cf - bin
        " > "dist/bin-$arch.tar"

    name=amiscara-deck-$version-linux-$arch
    stage=$(mktemp -d)
    mkdir "$stage/$name"
    tar -C "$stage/$name" -xf "dist/bin-$arch.tar"
    rm "dist/bin-$arch.tar"
    cp scripts/linux/install.sh "$stage/$name/"
    cp README.md README.en.md LICENSE-MIT LICENSE-APACHE "$stage/$name/"
    COPYFILE_DISABLE=1 tar --no-xattrs -C "$stage" -czf "dist/$name.tar.gz" "$name"
    rm -rf "$stage"
    echo "dist/$name.tar.gz"
done
