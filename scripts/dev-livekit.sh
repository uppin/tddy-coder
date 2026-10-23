#!/usr/bin/env bash
# The LiveKit server a ./web-dev run talks to. Sourced by ./web-dev; it declares two functions and
# runs nothing.
#
# Docker is the primary path. With no LiveKit credentials in the environment (.env included —
# ./web-dev loads it first), dev_livekit_up starts a container of the pinned image in --dev mode and
# exports LIVEKIT_URL / LIVEKIT_PUBLIC_URL / LIVEKIT_API_KEY / LIVEKIT_API_SECRET for the daemon,
# which reads them as overrides of its YAML (packages/tddy-daemon/src/runtime/env.rs).
#
# Only when the environment names a complete external server — a URL (LIVEKIT_URL or
# LIVEKIT_PUBLIC_URL) plus LIVEKIT_API_KEY and LIVEKIT_API_SECRET — is that server reused, and then
# no container is started. A partial set is an error: the daemon would mix .env values with the
# container's. An external server that does not answer is an error too, never a silent fall back
# to Docker: the daemon would otherwise retry a dead URL forever while the web UI shows nothing.
#
# Never touches a LiveKit it did not start. A port block already allocated by something else draws
# another ephemeral block; nothing pre-existing is stopped.
# The caller registers dev_livekit_down on EXIT, which removes this run's container on a normal
# exit and an error exit alike. A SIGKILLed ./web-dev cannot run its trap: its container is
# labelled tddy.web-dev.pid=<pid> for `docker ps --filter label=tddy.web-dev.pid`.
#
# Environment:
#   DEV_LIVEKIT_READY_TIMEOUT_S — seconds to wait for the container to answer (default: 30)
#   DEV_LIVEKIT_AVOID_PORTS     — space-separated ports no drawn block may cover. ./web-dev passes
#                                 its daemon and Vite ports: both are chosen before LiveKit starts but
#                                 bound only after, so Docker cannot report them as taken.
#
# Sets DEV_LIVEKIT_CONTAINER to the container this run started ("" when reusing a server).

DEV_LIVEKIT_CONTAINER=""

# --dev mode's fixed credentials (livekit-server --dev; same as tddy-livekit-testkit).
DEV_LIVEKIT_DEV_KEY="devkey"
DEV_LIVEKIT_DEV_SECRET="secret"
# Ports are ephemeral: each attempt draws a random block — signal, ICE/TCP (+1), ICE/UDP (+2) — from
# the IANA dynamic range, so runs in different worktrees and the testkit's container never collide
# on a fixed port. Docker cannot pick them for us: they must keep their number inside the container.
DEV_LIVEKIT_EPHEMERAL_FIRST=49152
DEV_LIVEKIT_EPHEMERAL_LAST=65535
DEV_LIVEKIT_PORT_ATTEMPTS=10

dev_livekit_up() {
  local url="${LIVEKIT_URL:-${LIVEKIT_PUBLIC_URL:-}}"
  local missing=()
  [ -n "$url" ] || missing+=("LIVEKIT_URL")
  [ -n "${LIVEKIT_API_KEY:-}" ] || missing+=("LIVEKIT_API_KEY")
  [ -n "${LIVEKIT_API_SECRET:-}" ] || missing+=("LIVEKIT_API_SECRET")

  if [ "${#missing[@]}" -eq 0 ]; then
    dev_livekit_check_external "$url"
    return
  fi
  if [ "${#missing[@]}" -lt 3 ]; then
    echo "Error: LiveKit is partly configured (.env or environment); missing: ${missing[*]}." >&2
    echo "  Set all of LIVEKIT_URL, LIVEKIT_API_KEY and LIVEKIT_API_SECRET to reuse a running server," >&2
    echo "  or none of them to have ./web-dev start its own LiveKit container." >&2
    return 1
  fi
  dev_livekit_start_container
}

# Reuse an external server, but only one that answers.
dev_livekit_check_external() {
  local url="$1" http_url code
  http_url="${url/#wss:/https:}"
  http_url="${http_url/#ws:/http:}"
  code=$(curl -s -o /dev/null -w '%{http_code}' --connect-timeout 5 --max-time 8 "$http_url" 2>/dev/null || true)
  if [ -z "$code" ] || [ "$code" = "000" ]; then
    echo "Error: LiveKit at ${url} (from .env / environment) is not reachable." >&2
    echo "  Bring that server up, or remove LIVEKIT_URL, LIVEKIT_PUBLIC_URL, LIVEKIT_API_KEY and" >&2
    echo "  LIVEKIT_API_SECRET from .env so ./web-dev starts its own LiveKit container." >&2
    return 1
  fi
  echo "=== Reusing LiveKit at ${url} (credentials from .env / environment) ===" >&2
}

# Set DEV_LIVEKIT_BLOCK to the signal port of a random block (signal, +1, +2) in the ephemeral range
# that covers none of the space-separated ports in $1. Drawn rather than probed: Docker reports a
# held port when it starts the container, and these are ports the caller binds later, which no probe
# can see yet. Sets a variable instead of printing: a $(...) subshell reseeds $RANDOM.
_dev_livekit_draw_block() {
  local avoid="$1" port taken tries
  local span=$((DEV_LIVEKIT_EPHEMERAL_LAST - DEV_LIVEKIT_EPHEMERAL_FIRST - 1))
  for ((tries = 0; tries < 100; tries++)); do
    # Two $RANDOMs: one alone tops out at 32767, short of the range's 16382-port span.
    port=$((DEV_LIVEKIT_EPHEMERAL_FIRST + (RANDOM * 32768 + RANDOM) % span))
    for taken in $avoid; do
      [ "$taken" -ge "$port" ] && [ "$taken" -le $((port + 2)) ] && continue 2
    done
    DEV_LIVEKIT_BLOCK="$port"
    return 0
  done
  echo "Error: no LiveKit port block clear of ${avoid} after 100 draws." >&2
  return 1
}

dev_livekit_start_container() {
  if ! command -v docker >/dev/null 2>&1; then
    echo "Error: docker not found — ./web-dev starts LiveKit in Docker unless .env names a running server" >&2
    echo "  (LIVEKIT_URL, LIVEKIT_API_KEY, LIVEKIT_API_SECRET)." >&2
    return 1
  fi
  if ! docker info >/dev/null 2>&1; then
    echo "Error: Docker is not running — start it, or set LIVEKIT_URL, LIVEKIT_API_KEY and" >&2
    echo "  LIVEKIT_API_SECRET in .env to reuse a running LiveKit server." >&2
    return 1
  fi

  local image name attempt port tcp_port udp_port err
  # Blocks found taken join the avoided ports, so a retry never redraws one of them.
  local avoid="${DEV_LIVEKIT_AVOID_PORTS:-}"
  image="$(tr -d '[:space:]' <"$(dirname "${BASH_SOURCE[0]}")/../.config/livekit-server.image")"
  name="tddy-web-dev-livekit-$$"

  for ((attempt = 0; attempt < DEV_LIVEKIT_PORT_ATTEMPTS; attempt++)); do
    _dev_livekit_draw_block "$avoid" || return 1
    port="$DEV_LIVEKIT_BLOCK"
    tcp_port=$((port + 1))
    udp_port=$((port + 2))
    # Each port is published on the same number inside and outside: LiveKit embeds its container
    # ports in ICE candidates, so a remapped port is unreachable for media.
    # TODO: --node-ip 127.0.0.1 keeps media on loopback, so a browser on another machine (VITE_URL
    # on a LAN address) reaches signalling but not media. Derive the node IP from VITE_URL.
    if err=$(docker run -d --rm --name "$name" --label "tddy.web-dev.pid=$$" \
      -p "${port}:${port}" \
      -p "${tcp_port}:${tcp_port}" \
      -p "${udp_port}:${udp_port}/udp" \
      -e "UDP_PORT=${udp_port}" \
      "$image" --dev --bind 0.0.0.0 --node-ip 127.0.0.1 \
      --config-body "$(printf 'port: %s\nrtc:\n  tcp_port: %s\n' "$port" "$tcp_port")" \
      2>&1 >/dev/null); then
      DEV_LIVEKIT_CONTAINER="$name"
      break
    fi
    # A failed start can leave a created container behind under our name; it is ours to remove.
    docker rm -f "$name" >/dev/null 2>&1 || true
    case "$err" in
      *"port is already allocated"* | *"address already in use"*)
        echo "=== LiveKit ports ${port}-${udp_port} are taken; drawing another block ===" >&2
        avoid="${avoid} ${port} ${tcp_port} ${udp_port}"
        ;;
      *)
        echo "Error: could not start the LiveKit container (${image}):" >&2
        echo "$err" >&2
        return 1
        ;;
    esac
  done
  if [ -z "$DEV_LIVEKIT_CONTAINER" ]; then
    echo "Error: no free LiveKit port block in ${DEV_LIVEKIT_EPHEMERAL_FIRST}-${DEV_LIVEKIT_EPHEMERAL_LAST} (${DEV_LIVEKIT_PORT_ATTEMPTS} tried)." >&2
    return 1
  fi

  echo "=== Started LiveKit container ${name} (${image}) on :${port} ===" >&2
  local timeout="${DEV_LIVEKIT_READY_TIMEOUT_S:-30}" deadline code
  deadline=$((SECONDS + timeout))
  while :; do
    code=$(curl -s -o /dev/null -w '%{http_code}' --connect-timeout 1 --max-time 2 "http://127.0.0.1:${port}/" 2>/dev/null || true)
    [ "$code" = "200" ] && break
    if [ "$SECONDS" -ge "$deadline" ]; then
      echo "Error: LiveKit container ${name} did not answer on :${port} within ${timeout}s. Its log:" >&2
      docker logs --tail 20 "$name" >&2 2>&1 || true
      return 1
    fi
    sleep 0.5
  done

  export LIVEKIT_URL="ws://127.0.0.1:${port}"
  export LIVEKIT_PUBLIC_URL="ws://127.0.0.1:${port}"
  export LIVEKIT_API_KEY="$DEV_LIVEKIT_DEV_KEY"
  export LIVEKIT_API_SECRET="$DEV_LIVEKIT_DEV_SECRET"
}

# Remove the container this run started, if any. Idempotent; safe in an EXIT trap.
dev_livekit_down() {
  [ -n "${DEV_LIVEKIT_CONTAINER:-}" ] || return 0
  echo "=== Removing LiveKit container ${DEV_LIVEKIT_CONTAINER} ===" >&2
  docker rm -f "$DEV_LIVEKIT_CONTAINER" >/dev/null 2>&1 || true
  DEV_LIVEKIT_CONTAINER=""
}
