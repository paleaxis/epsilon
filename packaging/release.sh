#!/bin/sh
# Builds the release profile and writes a relocatable tarball into dist/.
#
# Usage:
#   ./packaging/release.sh
#
# The archive unpacks to epsilon-<version>-linux-<arch>/ with the binary, the
# desktop entry, icon, MIME definitions, this packaging set, LICENSE and README.
# A matching .sha256 file is written next to the archive.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if [ -z "$version" ]; then
    printf 'error: could not read the workspace version from Cargo.toml\n' >&2
    exit 1
fi

arch=$(uname -m)
name="epsilon-$version-linux-$arch"
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT INT TERM

printf 'Building release binary...\n'
cargo build --release --locked -p epsilon-app

mkdir -p "$stage/$name"
cp target/release/epsilon "$stage/$name/epsilon"
cp packaging/epsilon.desktop \
    packaging/epsilon.svg \
    packaging/epsilon-mime.xml \
    packaging/install.sh \
    "$stage/$name/"
cp LICENSE README.md "$stage/$name/"

mkdir -p dist
archive="dist/$name.tar.gz"
tar -C "$stage" -czf "$archive" "$name"
(
    cd dist
    sha256sum "$name.tar.gz" > "$name.tar.gz.sha256"
)

printf '\nWrote %s\n' "$archive"
printf 'Wrote %s.sha256\n' "$archive"
printf 'Binary: %s bytes; archive: %s bytes\n' \
    "$(wc -c < "$stage/$name/epsilon")" \
    "$(wc -c < "$archive")"
