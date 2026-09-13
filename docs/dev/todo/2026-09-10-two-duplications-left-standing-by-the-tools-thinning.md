# 2026-09-10 — Two duplications left standing by the tools thinning

**Category:** Future enhancement
**Source:** `unbundle-tools-thinning` changeset (#unbundle node 5, PR #474), M5 / M7

Node 5 deleted four duplications outright (`build_cli::plugin_registry`, `server::
exec_tool_catalog`, `review_persist`, and `pty_relay`'s `RawMode`) and collapsed a fifth
(`env_non_empty`). Two it recorded rather than collapsed, and they will outlive the changeset
unless they are recorded here:

- **`MAX_MANIFEST_BYTES` is declared twice**, `64 * 1024` in both
  `tddy_core::session_actions::authoring` (where M5 gave it a home) and
  `tddy_sandbox_app::host_actions` (the authoritative host-side bound on the same value). One line
  to collapse. Not taken at M5 because `tddy-sandbox-app` was outside that milestone's verified
  package set.
- **"Unset or blank" has a third spelling.** M7 collapsed `mcp_primitives::env_non_empty` and
  `session_tool_client`'s private `non_empty_env` into `tddy_core::spawn_env::env_non_empty`, which
  settled a real disagreement — one trimmed before testing for empty and the other did not, so a
  LiveKit variable exported as `" "` used to configure a transport whose URL was one space.
  `tddy_daemon_kernel::config::non_empty_env` is still its own copy, and it is the one an operator's
  daemon config goes through.

Deferred because each crosses into a package the milestone that found it had not verified, and node
5's rule was to move code rather than reach into crates it was not testing.
