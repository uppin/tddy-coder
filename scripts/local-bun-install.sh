#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

LOCAL_LOCK="$PROJECT_ROOT/local.bun.lock"
LOCAL_PKG_DIR="$PROJECT_ROOT/.local-install"
BUN_LOCK="$PROJECT_ROOT/bun.lock"
BUN_LOCK_BACKUP="$PROJECT_ROOT/bun.lock.orig"
REGISTRY="${LOCAL_REGISTRY_URL:-https://npm.dev.wixpress.com}"

if [ ! -f "$LOCAL_LOCK" ]; then
  echo "Error: local.bun.lock not found. Run 'bun run resolve-local-lock' first."
  exit 1
fi

BACKED_UP_PKGJSONS=()

cleanup() {
  for bakfile in "${BACKED_UP_PKGJSONS[@]}"; do
    original="${bakfile%.orig}"
    if [ -f "$bakfile" ]; then
      mv "$bakfile" "$original"
    fi
  done
  if [ -f "$BUN_LOCK_BACKUP" ]; then
    mv "$BUN_LOCK_BACKUP" "$BUN_LOCK"
  fi
  echo "Restored original files"
}
trap cleanup EXIT

cp "$BUN_LOCK" "$BUN_LOCK_BACKUP"
cp "$LOCAL_LOCK" "$BUN_LOCK"

if [ -d "$LOCAL_PKG_DIR" ]; then
  while IFS= read -r -d '' patched; do
    rel="${patched#"$LOCAL_PKG_DIR/"}"
    target="$PROJECT_ROOT/$rel"
    if [ -f "$target" ]; then
      cp "$target" "$target.orig"
      BACKED_UP_PKGJSONS+=("$target.orig")
      cp "$patched" "$target"
      echo "Patched $rel"
    fi
  done < <(find "$LOCAL_PKG_DIR" -name "package.json" -print0)
fi

echo "Installing from $REGISTRY ..."
bun install --verbose --registry "$REGISTRY" "$@"

echo "Done."
