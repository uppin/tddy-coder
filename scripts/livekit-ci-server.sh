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
set -euo pipefail

# TODO(shared-livekit-ci): implement start and stop.
echo "TODO(shared-livekit-ci): scripts/livekit-ci-server.sh $* is not implemented" >&2
exit 1
