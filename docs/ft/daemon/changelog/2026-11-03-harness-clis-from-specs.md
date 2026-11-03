# 2026-11-03 — OpenCode, Muse Code and Warp CLI sessions are launched from their tool definitions

`#agent-tools` 8/12 ([#797](https://github.com/uppin/tddy-coder/pull/797)).

- **`opencode-cli`, `muse-cli` and `warp-cli` render from `opencode.yaml`, `muse.yaml` and
  `warp.yaml`**, on the host and (OpenCode, Muse Code) in the jail. Argv, models, route files and
  Warp's resume token are unchanged.
- **A sandboxed OpenCode or Muse Code start may carry a first prompt**; it goes through the agent's
  argv. A blank model is no longer refused there.
- **A routed OpenCode jail reaches its provider's base URL**, which its route names only in
  `opencode.json`.
- **The jailed agent sees no `TDDY_*` variable**; only the `tddy-tools` MCP server gets them.
- **A sandboxed Warp start** is still refused, now by `warp.yaml`'s `placements: [host]`, and the
  message names the placements.
- A host without coding agents installed refuses these starts with `FAILED_PRECONDITION`.
- A harness profile's `harness:` resolves to the tool definition's id.

See [OpenCode](../opencode-cli-session.md), [Muse Code](../muse-cli-session.md) and
[Warp](../warp-cli-session.md) CLI sessions.
