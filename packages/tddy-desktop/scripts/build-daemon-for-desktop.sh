#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPO_ROOT="$(cd "$PKG_ROOT/../.." && pwd)"

cd "$REPO_ROOT"
cargo build --release -p tddy-daemon

mkdir -p "$PKG_ROOT/resources/bin"
cp "$REPO_ROOT/target/release/tddy-daemon" "$PKG_ROOT/resources/bin/tddy-daemon"
chmod +x "$PKG_ROOT/resources/bin/tddy-daemon"
echo "Copied tddy-daemon to $PKG_ROOT/resources/bin/tddy-daemon"
