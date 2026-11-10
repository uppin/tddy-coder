# 2026-11-06 — The descriptor table, curated lists and bespoke ACP backends are deleted

**Type:** Refactor (planned deletions)

`#agent-tools` 20/28 ([#828](https://github.com/uppin/tddy-coder/pull/828)).

- `cli_harness.rs` / `cli_harness_muse.rs` / `cli_harness_warp.rs` (`CliHarness`, `CliLaunch`,
  the registry) and `claude_argv.rs` are deleted. Per-agent answers — argv, binary, route config
  env, model source, context agent — come from the agent's spec, supplied by callers.
- `curated_models_for_agent`, `claude_cli_models`, `cursor_cli_models` and
  `backend/context_globs.rs` are deleted; `SpecHeadlessBackend::with_curated_models` and
  `CodingAgentSpec::curated_models()` / `context_globs` answer from YAML.
- `backend/acp.rs`, `codex_acp.rs`, `opencode_acp.rs` are deleted; `SpecAcpBackend` runs every
  ACP agent from its spec's rendered command and named hooks adapter.
- `tddy_core` no longer re-exports a `claude_argv` path. See
  [architecture.md § CLI agents — no descriptor table](../architecture.md#cli-agents--no-descriptor-table)
  and the [cross-package entry](../../../../docs/dev/changesets/2026-11-06-agent-tools-sweep-yaml-only.md).

Code issues: `oversized-file-backend-mod` 851 → 537 production lines (partially fixed);
`oversized-file-spec-headless` 646 → 653 (worse by 7).
