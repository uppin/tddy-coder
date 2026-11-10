# 2026-11-03 — OpenCode, Muse Code and Warp CLI sessions launch from their specs

**Type:** Feature

`#agent-tools` 8/12 ([#797](https://github.com/uppin/tddy-coder/pull/797)). Packages:
`tddy-coding-agent-specs`, `tddy-agent-backend`, `tddy-agent-launch`, `tddy-cli-sessions`,
`tddy-daemon-sandbox`, `tddy-session-lifecycle`.

- `specs/{opencode,muse,warp}.yaml`, with new `coding_agent` sections: `route`, `models.list_command`
  / `models.adapter`, `resume` and `placements`. `warp.yaml` is `placements: [host]`, so a sandboxed
  Warp start is refused `FAILED_PRECONDITION` naming the placements.
- A launch-side adapter registry (`LaunchAdapters`) with `opencode-models` and `warp-resume-token`.
- Host starts and resumes go through `launch_host_pty`, which (with
  `CliSessionManager::spawn_rendered_main`) takes an additive `on_exit` hook for the resume adapter.
  Jailed OpenCode / Muse go through `launch_jailed`.
- **Behaviour changes (developer decisions, 2026-11-03):** a jailed first prompt goes through the spec
  argv (no longer refused); no blank-model refusal for jailed OpenCode / Muse; the jailed agent sees no
  `TDDY_*` (only the MCP server's env); `route_hosts` admits the hosts in a route's files, so a routed
  OpenCode jail reaches its provider's base URL; CLI fixtures install coding agents.
- **Deferred:** deleting `cli_harness.rs`, `cli_harness_muse.rs`, `cli_harness_warp.rs` and the jail
  `--agent-kind` helpers ([todo](../todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md)).
  Restructuring is deferred to the `#agent-tools` sweep node (developer, 2026-11-02). Code issues:
  `start_sandboxed_cli_harness_session` narrowed 250 → 214 (claim released); new records for
  `start_cli_harness_session` (191), `resume_cli_harness_session` (74),
  `start_cli_harness_from_request` (61) and `slots::render` (65); `cursor_cli_spawn.rs` 586 → 587.
- [2026-10-24-warp-cli-sandbox.md](../todo/2026-10-24-warp-cli-sandbox.md) narrowed to spec terms.

Product docs: [OpenCode](../../ft/daemon/opencode-cli-session.md),
[Muse Code](../../ft/daemon/muse-cli-session.md), [Warp](../../ft/daemon/warp-cli-session.md) CLI
sessions; [Harness profiles](../../ft/daemon/harness-profiles.md).
