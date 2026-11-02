# 2026-11-02 — Stream adapters and the headless renderer port

**Type:** Feature

`#agent-tools` 6/12 ([#795](https://github.com/uppin/tddy-coder/pull/795)). `adapters::{StreamAdapter, AdapterRegistry, ClaudeStreamJson, HeadlessRenderer, HeadlessTurn, HeadlessInvocation}`; `ClaudeCodeBackend::with_headless_renderer` and `HeadlessCommand.{renderer, scratch_dir}` render Claude from its declaration, files written 0600. `invoke_sync`'s body is now `run_child`, shared by both paths. See [architecture.md § Adapters](../architecture.md#adapters-adapters) and the [cross-package entry](../../../../docs/dev/changesets/2026-11-02-agent-tools-claude-headless.md).
