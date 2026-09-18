#!/usr/bin/env bash
# The cargo packages whose binaries a running dev stack can INVOKE — as opposed to the single
# process each launcher starts itself. Sourced by ./web-dev and ./desktop-dev for their --build
# flag; it declares names and one helper, and runs nothing.
#
# Why this needs its own list: every one of these is resolved at runtime as a sibling of
# current_exe() (see packages/tddy-daemon/src/index_daemon/spawn.rs,
# packages/tddy-daemon-sandbox/src/sandbox_session.rs, packages/tddy-projects/src/project_provision.rs)
# and then falls back to a bare name on PATH with NO existence check. So a sibling missing from
# target/debug is not a build error — it surfaces much later as a session that will not start, or
# as a dev run silently driving whatever ./install last put on PATH. Building the whole workspace
# instead is not the alternative: that is the tens-of-minutes local run CLAUDE.md forbids.
#
# Why these names: they are exactly install's DESKTOP_BINARIES — the production statement of "these
# binaries must sit beside the daemon". A dev run resolves the same way against target/debug, so
# the two lists are one contract, and tddy-e2e/tests/dev_runtime_binaries.rs fails if they drift.
#
# Deliberately absent: tddy-daemon and tddy-supervisor (a launcher names its own backend —
# ./web-dev builds tddy-daemon via BUILD_TARGETS, and Tddy Desktop's own process IS the daemon),
# and tddy-sandbox-app (an app a developer starts by hand, not something the stack invokes).

DEV_RUNTIME_PACKAGES=(
  tddy-coder           # the session worker the daemon spawns per session
  tddy-tools           # the tool/MCP binary a session's agent calls
  tddy-sandbox-runner  # runs inside every jail the daemon spawns
  tddy-index-daemon    # the warm rust-analyzer index, spawned per workspace root
  tddy-remote-git-repo # git's GIT_SSH_COMMAND shim
  tddy-session-sync    # mirrors a session's worktree
)

# Resolve the build set: DEV_RUNTIME_PACKAGES plus any extra package names passed as arguments,
# de-duplicated in first-seen order. Sets two globals rather than echoing, so a caller under
# `set -u` can pass the result to cargo without word-splitting:
#
#   DEV_RUNTIME_CARGO_PACKAGES — the names, for display
#   DEV_RUNTIME_CARGO_ARGS     — `-p <name>` pairs, for `cargo build "${DEV_RUNTIME_CARGO_ARGS[@]}"`
dev_runtime_cargo_args() {
  DEV_RUNTIME_CARGO_PACKAGES=()
  DEV_RUNTIME_CARGO_ARGS=()
  local name seen=""
  for name in "${DEV_RUNTIME_PACKAGES[@]}" "$@"; do
    [ -z "$name" ] && continue
    case " $seen " in
      *" $name "*) continue ;;
    esac
    seen="$seen $name"
    DEV_RUNTIME_CARGO_PACKAGES+=("$name")
    DEV_RUNTIME_CARGO_ARGS+=("-p" "$name")
  done
}
