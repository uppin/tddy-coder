# 2026-11-07 — A split agent reaches its remote through its tool server and a context-dir pointer

**Type:** Feature

`#agent-tools` 25/28 ([#833](https://github.com/uppin/tddy-coder/pull/833)), branch
`feature/agent-tools/split-remote-env`, based on `feature/agent-tools/dead-launch-code`.

Packages: `tddy-session-split`, `tddy-agent-launch`, `tddy-session-lifecycle`, `tddy-cli-sessions`,
`tddy-coding-agent-specs`; tests in `tddy-daemon` and `tddy-daemon-rpc`.

## What changed

- **No `TDDY_*` on the agent process.** A split session's start, its resume and the sandboxed-codebase
  resume put the `TDDY_REMOTE_*` transport and the roster seed on the registration of the agent's
  `tddy-tools --mcp` server (`split_claude_extra_args(spec, …, tool_server_env, …)`), not on the
  process. `SplitAgentWiring.env` is empty.
- **A pointer in the context dir.** The agent's context dir holds `.tddy/session.json` while it runs:
  the same transport by variable name, owned by this daemon, no socket, `0600`. The start places it
  through the `SplitSessionPointers` port (`HostSplitSessionPointers` in `tddy-agent-launch`, over
  #829's `place_session_pointer`) and spawns with the exit hook that removes it; a spawn that fails
  after the pointer is placed removes it. The resume places the pointer its wiring was re-derived with
  and refuses a split session whose wiring has none (no fallback to the generic wiring's pointer). The
  context dir is inside the session dir, so a delete removes it too.
- **The host's spec.** `split_agent_spec(&CodingAgentRegistry)` reads `SplitSessions.coding_agents`, so an
  operator's override of the split agent reaches argv, hooks, context globs and guidance files.
  `build_split_context_dir` lost its `unwrap_or_default` fallback; `tddy_tools_only_mcp_args` is deleted.
  `embedded_spec` has no production caller left.
- **`CliSessionManager::with_launch_flags` is public** (`tddy-cli-sessions`). The split start renders its
  argv itself and spawns with `spawn_rendered_main`, which takes an exit hook; `start_with_options`
  takes none, and splicing the launch flags before the trailing prompt is the one thing the start
  still needs from it. A narrower seam would be a second spawn entry point with the same body.
- `write_split_agent_metadata` moved to `svc_split_agent_metadata.rs` (`svc_spawn_split_agent.rs`
  497 → 493 production lines, after growing past 500).

## Deferred

[`2026-11-07-embedded-spec-test-fixtures-in-sandbox-app`](../todo/2026-11-07-embedded-spec-test-fixtures-in-sandbox-app.md):
four `#[cfg(test)]` fixtures still build their spec through `embedded_spec`; the sweep node decides.

Commit `227ad5dc7` briefly contained the generated `schema-manifest.json`; `0163b312f` restored it, so
the branch's net diff does not touch it.

See [session-split module layout](../../packages/tddy-session-split/docs/module-layout.md) and
[agent-launch module layout](../../packages/tddy-agent-launch/docs/module-layout.md).
