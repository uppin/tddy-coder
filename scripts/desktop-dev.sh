#!/usr/bin/env bash
set -euo pipefail
# Background Vite is not in the foreground process group; Ctrl+C won't reach it unless we
# tear it down in a trap. Job control gives the background pipeline its own PGID so one
# kill can stop bun + vite.
set -m

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VITE_PORT="${VITE_PORT:-5173}"
export VITE_URL="${VITE_URL:-http://localhost:${VITE_PORT}}"

cd "$PROJECT_ROOT"

# Load .env if present (does not override already-set env vars) — same as ./web-dev.
if [[ -f .env ]]; then
  while IFS='=' read -r key value; do
    [[ -z "$key" || "$key" == \#* ]] && continue
    value="${value%\"}"
    value="${value#\"}"
    value="${value%\'}"
    value="${value#\'}"
    if [[ -z "${!key+x}" ]]; then
      export "$key=$value"
    fi
  done < .env
fi

# Embedded daemon resolves repo root from cwd / TDDY_WORKSPACE_ROOT; Electrobun's cwd is the repo when launched from here.
export TDDY_WORKSPACE_ROOT="${TDDY_WORKSPACE_ROOT:-$PROJECT_ROOT}"

# Default daemon config for Electrobun embedded tddy-daemon (dev.desktop.yaml at repo root).
if [[ -z "${TDDY_DAEMON_CONFIG:-}" ]] && [[ -f "$PROJECT_ROOT/dev.desktop.yaml" ]]; then
  export TDDY_DAEMON_CONFIG="$PROJECT_ROOT/dev.desktop.yaml"
fi

# Prefer an existing Cargo-built binary so Electrobun dev (wrong import.meta.dir) still finds tddy-daemon.
if [[ -z "${TDDY_DAEMON_BINARY:-}" ]]; then
  if [[ -f "$PROJECT_ROOT/target/debug/tddy-daemon" ]]; then
    export TDDY_DAEMON_BINARY="$PROJECT_ROOT/target/debug/tddy-daemon"
  elif [[ -f "$PROJECT_ROOT/target/release/tddy-daemon" ]]; then
    export TDDY_DAEMON_BINARY="$PROJECT_ROOT/target/release/tddy-daemon"
  fi
fi

# Codex OAuth: Electrobun must join LiveKit and bind /auth/callback on the port Codex uses, or the browser hits connection refused.
# Main process also reads dev.desktop.yaml when these are unset; .env LIVEKIT_* fills gaps here.
export TDDY_RPC_BASE="${TDDY_RPC_BASE:-http://127.0.0.1:8899/rpc}"
export TDDY_LIVEKIT_ROOM="${TDDY_LIVEKIT_ROOM:-tddy-lobby}"
export TDDY_LIVEKIT_URL="${TDDY_LIVEKIT_URL:-${LIVEKIT_URL:-${LIVEKIT_PUBLIC_URL:-}}}"

VITE_PID=""
CLEANED_UP=0

# Stop the background Vite job (bun + node/vite). Idempotent for INT/EXIT both firing.
stop_vite() {
  [[ "$CLEANED_UP" -eq 1 ]] && return 0
  CLEANED_UP=1
  if [[ -z "${VITE_PID:-}" ]]; then
    return 0
  fi
  if ! kill -0 "$VITE_PID" 2>/dev/null; then
    return 0
  fi
  # Negative PID: signal the whole process group (leader PID == PGID from `set -m` + `&`).
  kill -TERM -"$VITE_PID" 2>/dev/null || kill -TERM "$VITE_PID" 2>/dev/null || true
  local i
  for ((i = 0; i < 40; i++)); do
    kill -0 "$VITE_PID" 2>/dev/null || break
    sleep 0.05
  done
  if kill -0 "$VITE_PID" 2>/dev/null; then
    kill -KILL -"$VITE_PID" 2>/dev/null || kill -KILL "$VITE_PID" 2>/dev/null || true
  fi
  wait "$VITE_PID" 2>/dev/null || true
}

trap stop_vite EXIT
trap 'stop_vite; exit 130' INT
trap 'stop_vite; exit 143' TERM

echo "Starting tddy-web Vite dev server on port ${VITE_PORT} ..."
bun run --filter tddy-web dev &
VITE_PID=$!

echo "Waiting for ${VITE_URL} ..."
ready=0
for _ in $(seq 1 120); do
  if curl -s -o /dev/null -f --max-time 2 "${VITE_URL}/"; then
    ready=1
    break
  fi
  sleep 0.5
done

if [[ "$ready" -ne 1 ]]; then
  echo "Error: Vite did not become ready at ${VITE_URL}" >&2
  exit 1
fi

echo "Starting Electrobun desktop (VITE_URL=${VITE_URL}) ..."
bun run --filter tddy-desktop dev "$@"
