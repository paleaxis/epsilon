#!/bin/sh
# Installs Epsilon for the current user: binary, desktop launcher, scalable icon
# and the .epsilon/.excalidraw MIME types.
#
# Usage:
#   cargo build --release
#   ./packaging/install.sh
#
# PREFIX defaults to ~/.local. Use PREFIX=/usr/local with sufficient rights for
# a system-wide install, and set EPSILON_BIN to install a binary from elsewhere.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
root=$(CDPATH= cd -- "$here/.." && pwd)
prefix=${PREFIX:-$HOME/.local}
datadir=${DATADIR:-$prefix/share}

# This script ships both in the source tree (assets in packaging/, binary in
# target/release) and inside the release tarball (everything next to it).
if [ -f "$here/epsilon.desktop" ]; then
    assets=$here
else
    assets=$root/packaging
fi

binary=${EPSILON_BIN:-}
if [ -z "$binary" ]; then
    if [ -f "$here/epsilon" ]; then
        binary=$here/epsilon
    else
        binary=$root/target/release/epsilon
    fi
fi

if [ ! -f "$binary" ]; then
    printf 'error: binary not found: %s\n' "$binary" >&2
    printf 'build it first (cargo build --release) or set EPSILON_BIN\n' >&2
    exit 1
fi

install -Dm755 "$binary" "$prefix/bin/epsilon"
install -Dm644 "$assets/epsilon.desktop" \
    "$datadir/applications/epsilon.desktop"
install -Dm644 "$assets/epsilon.svg" \
    "$datadir/icons/hicolor/scalable/apps/epsilon.svg"
install -Dm644 "$assets/epsilon-mime.xml" \
    "$datadir/mime/packages/epsilon.xml"

# Indexes are optional: a missing tool must not fail an otherwise good install.
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$datadir/applications" >/dev/null 2>&1 || true
fi
if command -v update-mime-database >/dev/null 2>&1; then
    update-mime-database "$datadir/mime" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t "$datadir/icons/hicolor" >/dev/null 2>&1 || true
fi

printf 'Installed Epsilon into %s\n' "$prefix"
case ":$PATH:" in
    *":$prefix/bin:"*) ;;
    *) printf 'Note: add %s/bin to PATH so the launcher can find epsilon.\n' "$prefix" ;;
esac
