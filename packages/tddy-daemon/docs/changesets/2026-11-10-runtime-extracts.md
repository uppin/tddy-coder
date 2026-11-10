# 2026-11-10 — the daemon's runtime extracts in the agent-tools sweep

**Type:** Refactor

`#agent-tools` 33/33 ([#876](https://github.com/uppin/tddy-coder/pull/876)).

- `runtime/build_context.rs` `new`: 71 → 56 lines.
- `runtime/mount_host.rs` `build_session_host`: 90 → 74 lines (`build_connection_impl`,
  `adopt_and_build_connection`, `daemon_launcher_deps` extracted). Still over 60: deferred by the
  user, 2026-11-10, record `complexity-mount-host-build-session-host`.
- The daemon reads `users.yaml` through `tddy-users-file` via `tddy-daemon-kernel` (see that
  package's entry of the same date).
