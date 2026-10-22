# 2026-10-22 — OpenCode terminal sessions and the CLI harness descriptor

**Type:** Feature

`#provider-route` 3/9 ([#664](https://github.com/uppin/tddy-coder/pull/664)), across `tddy-agent-backend` (the `cli_harness` registry with the claude-cli, cursor-cli and
opencode-cli descriptors; the OpenCode arm of `resolve_route` and `opencode_config_for`; the
`opencode` context-glob row; `resolve_agent_models` for a harness that lists its own models),
`tddy-cli-sessions` (argv from the registry, `launch_cli_harness`), `tddy-agent-launch`
(`opencode_cli_spawn`, `cli_route_launch`, `cli_session_worktree` shared with cursor-cli, the start and
resume branches), `tddy-daemon-kernel` (`cli_harnesses:` and `cli_harness_binary`), `tddy-daemon-rpc`
(the `--opencode-cli-path` probe), `tddy-tools` (`list-models --opencode-cli-path`),
`tddy-session-activity` (listing from the registry; worktree removal on delete), `tddy-session-files`
(context agent from the registry), `tddy-worktree-service` (`BranchIntentPolicy::opencode_cli`),
`tddy-testing-commons` (a no-argument stub invocation reads back as no arguments) and `tddy-web`
(the OpenCode toggle and request). Sandboxed OpenCode is refused (`TODO(provider-route-jail)`,
`#provider-route` 9/9); a managed codebase is refused; a start without a model leaves the choice to
OpenCode. OpenCode's CLI contract (`--model`, `--prompt`, `--continue`, `opencode models`,
`OPENCODE_CONFIG`) is taken from opencode.ai's documentation: no real `opencode` binary was available
to confirm it against, and every test runs a stub.

The daemon start/resume/listing/relay/catalog tests (`tddy-session-lifecycle/tests/opencode_cli_session.rs`,
`tddy-agent-backend/tests/opencode_model_catalog.rs`, `tddy-tools/tests/list_models_opencode_path.rs`,
`tddy-agent-launch/tests/opencode_cli_facilitation.rs`, the probe and route-launch unit tests) were
written during green for the planned scope, not reviewed at the acceptance-test review.

Docs: [OpenCode CLI session](../../ft/daemon/opencode-cli-session.md),
[provider accounts](../../ft/daemon/provider-accounts.md#running-a-harness-on-an-account),
[tool-session model selection](../../ft/web/tool-session-model-selection.md),
[tddy-agent-backend](../../../packages/tddy-agent-backend/docs/architecture.md#cli-harnesses-cli_harness),
[tddy-agent-launch](../../../packages/tddy-agent-launch/docs/module-layout.md).

Backlog: **resolved** `2026-10-08-cursor-cli-spawn-rs-in-tddy-agent-launch-is-563-production-lines`
("`cursor_cli_spawn.rs` in `tddy-agent-launch` is 563 production lines, over the 500 budget") —
562 → 418 production lines (count to the first `#[cfg(test)]`; the file has none), the worktree cut
and semantic index moved to `cli_session_worktree.rs`; entry deleted.

Code issues: **closed** `packages/tddy-agent-backend/docs/code-issues/panicking-api-provider-route-opencode-config-for.md`
(first detected in this PR) — 1 → 0 reachable panics: `opencode_config_for` returns
`Option<RouteFile>`, `None` for an account that serves neither OpenAI protocol; the three pinned call
sites changed only their call shape; record deleted. **Regressed, kept open**:
`oversized-file-backend-mod` 877 → 879 (stack-shared with #665 and #668, split deferred),
`complexity-svc-start-session-core-start-session-core` 386 → 391,
`complexity-svc-resume-session-resume-session-at-session-coordinate` 173 → 181. **Narrowed**:
`oversized-file-create-session-pane` 828 → 822 (still over). **Unchanged**:
`complexity-cursor-cli-spawn-spawn-cursor-cli-session-inner` (75; its sibling 267 → 207),
`complexity-branch-intent-resolve-branch-workflow` (86).
