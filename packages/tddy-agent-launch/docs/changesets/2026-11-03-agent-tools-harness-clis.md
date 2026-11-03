# 2026-11-03 — OpenCode, Muse Code and Warp from their specs

**Type:** Feature

`#agent-tools` 8/12 ([#797](https://github.com/uppin/tddy-coder/pull/797)).

- `harness_from_spec`: template params, `require_spec_placement` (the sandboxed `warp-cli` refusal names the spec's placements), session type → spec id, `resolve_session_agent` (`FailedPrecondition` without coding agents).
- Host OpenCode / Muse / Warp start and resume through `launch_host_pty`; Warp's resume token through the spec's resume adapter as the `on_exit` hook.
- Jailed OpenCode / Muse start and resume through `launch_jailed`, argv from `run:`. A first prompt goes through the argv, a blank model is not refused, and the agent sees no `TDDY_*` (only the MCP server's env does).
- Route helpers (`cli_route_launch`, `start_cli_route`, `resume_cli_route`, `launch_model_for`) take the spec. Harness profiles resolve `harness:` with `profile_spec_id`.

See [OpenCode](../../../../docs/ft/daemon/opencode-cli-session.md), [Muse Code](../../../../docs/ft/daemon/muse-cli-session.md) and [Warp](../../../../docs/ft/daemon/warp-cli-session.md) CLI sessions.
