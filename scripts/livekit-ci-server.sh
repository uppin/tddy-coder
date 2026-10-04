#!/usr/bin/env bash
# One pinned LiveKit server for a CI e2e run.
#
#   scripts/livekit-ci-server.sh start   start it, wait for its API, export LIVEKIT_TESTKIT_WS_URL
#   scripts/livekit-ci-server.sh stop    remove it (safe to run twice)
#
# `start` appends `LIVEKIT_TESTKIT_WS_URL=ws://127.0.0.1:PORT` to $GITHUB_ENV, which is how the tests
# find it: `LiveKitTestkit::start()` reuses a server instead of launching its own when that variable
# is set. Each of the three ports is published on the SAME number inside and outside the container,
# because LiveKit embeds its container ports in ICE candidates.
#
# The image reference is read from .config/livekit-server.image, the single place it is pinned; the
# testkit compiles the same file in. LIVEKIT_CI_READY_TIMEOUT_SECS bounds the wait for the API.
set -euo pipefail

CONTAINER_NAME="tddy-livekit-ci"
PORT_WS=7880
PORT_ICE_TCP=7881
PORT_ICE_UDP=7882
READY_TIMEOUT_SECS="${LIVEKIT_CI_READY_TIMEOUT_SECS:-60}"

IMAGE_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.config/livekit-server.image"
IMAGE="$(tr -d '[:space:]' <"${IMAGE_FILE}")"

start() {
  : "${GITHUB_ENV:?GITHUB_ENV must name the file the job environment is appended to}"
  local ws_url="ws://127.0.0.1:${PORT_WS}"
  local http_url="http://127.0.0.1:${PORT_WS}"

  echo "LiveKit image: ${IMAGE}" >&2
  docker run -d --rm --name "${CONTAINER_NAME}" \
    -p "${PORT_WS}:${PORT_WS}" \
    -p "${PORT_ICE_TCP}:${PORT_ICE_TCP}" \
    -p "${PORT_ICE_UDP}:${PORT_ICE_UDP}/udp" \
    -e "UDP_PORT=${PORT_ICE_UDP}" \
    "${IMAGE}" \
    --dev --bind 0.0.0.0 --node-ip 127.0.0.1 \
    --config-body "$(printf 'port: %s\nrtc:\n  tcp_port: %s\n' "${PORT_WS}" "${PORT_ICE_TCP}")" >/dev/null
  echo "LiveKit digest: $(docker image inspect --format '{{index .RepoDigests 0}}' "${IMAGE}")" >&2

  # An unauthenticated Twirp call is answered (401) once the API is serving; a refused connection
  # makes curl fail. Any HTTP answer from the Twirp route proves the API stack is up.
  local deadline=$((SECONDS + READY_TIMEOUT_SECS))
  until curl -s -o /dev/null -X POST -H 'Content-Type: application/json' -d '{}' \
    "${http_url}/twirp/livekit.RoomService/ListRooms"; do
    if ((SECONDS >= deadline)); then
      echo "LiveKit API at ${http_url} did not answer within ${READY_TIMEOUT_SECS}s" >&2
      docker logs "${CONTAINER_NAME}" >&2 || true
      docker rm -f "${CONTAINER_NAME}" >/dev/null 2>&1 || true
      exit 1
    fi
    sleep 1
  done

  echo "LIVEKIT_TESTKIT_WS_URL=${ws_url}" >>"${GITHUB_ENV}"
  echo "LiveKit server ready at ${ws_url}" >&2
}

stop() {
  local output
  if ! output="$(docker rm -f "${CONTAINER_NAME}" 2>&1)"; then
    # Already gone is the goal state, not an error; anything else is.
    if [[ "${output}" != *"No such container"* ]]; then
      echo "${output}" >&2
      exit 1
    fi
  fi
}

case "${1:-}" in
  start) start ;;
  stop) stop ;;
  *)
    echo "usage: $0 start|stop" >&2
    exit 2
    ;;
esac
