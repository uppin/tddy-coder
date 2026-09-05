# 2026-08-18 — **session agent roster

**Type:** Feature

AC37 cross-daemon provisioning rides the remote-git transport** — `daemon_rpc.rs` carries the `RemoteGitService` calls a facilitating daemon makes to clone a project onto a peer that has never seen it; a missing `use std::error::Error` (needed by `err.source()`) is added. Feature [session-agent-roster.md](../../../../docs/ft/daemon/session-agent-roster.md). Cross-package [docs/dev/changesets/](../../../../docs/dev/changesets/). (tddy-remote-git-repo)
