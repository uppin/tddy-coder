#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LOCAL_LOCK="$PROJECT_ROOT/local.bun.lock"
BUN_LOCK="$PROJECT_ROOT/bun.lock"
BUN_LOCK_BACKUP="$PROJECT_ROOT/bun.lock.orig"
REGISTRY="${LOCAL_REGISTRY_URL:-https://npm.dev.wixpress.com}"

if [ ! -f "$LOCAL_LOCK" ]; then
  echo "Error: local.bun.lock not found. Run resolve-local-lock.ts first:"
  echo "  ./dev bun run scripts/resolve-local-lock.ts"
  exit 1
fi

cleanup() {
  if [ -f "$BUN_LOCK_BACKUP" ]; then
    mv "$BUN_LOCK_BACKUP" "$BUN_LOCK"
    echo "Restored original bun.lock"
  fi
}
trap cleanup EXIT

cp "$BUN_LOCK" "$BUN_LOCK_BACKUP"
cp "$LOCAL_LOCK" "$BUN_LOCK"
echo "Swapped bun.lock with local.bun.lock"

echo "Installing from $REGISTRY ..."
bun install --registry "$REGISTRY" "$@"

echo "Done."
