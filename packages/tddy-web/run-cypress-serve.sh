#!/usr/bin/env bash
# Serve Storybook on all interfaces so you can open it in Chrome on another host.
#
# Usage:
#   ./run-cypress-serve.sh
#   # On this machine: Storybook built and served at http://0.0.0.0:9786
#   # On another host: open http://<this-machine-ip>:9786 in Chrome
#
# To run Cypress against this remote server from another machine:
#   cd packages/tddy-web
#   CYPRESS_BASE_URL=http://<server-ip>:9786 cypress open --e2e
#
# Note: Cypress Test Runner itself binds to 127.0.0.1 and cannot be accessed
# from another host. Only the app under test (Storybook) can be served remotely.
# See: https://github.com/cypress-io/cypress/issues/6319

set -e
cd "$(dirname "$0")"

echo "Building Storybook..."
bun run build-storybook

# Get a displayable address (prefer hostname, fall back to common IP patterns)
DISPLAY_HOST="${CYPRESS_SERVE_HOST:-}"
if [[ -z "$DISPLAY_HOST" ]]; then
  if command -v hostname &>/dev/null; then
    DISPLAY_HOST=$(hostname -I 2>/dev/null | awk '{print $1}' || hostname -f 2>/dev/null || echo "localhost")
  else
    DISPLAY_HOST="localhost"
  fi
fi

echo ""
echo "=========================================="
echo "Storybook server (for remote Chrome access)"
echo "=========================================="
echo "Local:    http://localhost:9786"
echo "Remote:   http://${DISPLAY_HOST}:9786"
echo ""
echo "From another host, open in Chrome: http://${DISPLAY_HOST}:9786"
echo "To run Cypress against this server: CYPRESS_BASE_URL=http://${DISPLAY_HOST}:9786 cypress open --e2e"
echo "=========================================="
echo ""

exec npx http-server storybook-static -p 9786 -a 0.0.0.0 -c-1 --cors
