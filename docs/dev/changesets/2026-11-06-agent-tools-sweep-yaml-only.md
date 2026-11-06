# 2026-11-06 — Agent metadata lives only in YAML specs; the UI reads it from the daemon

**Type:** Refactor (planned deletions) plus the UI node

`#agent-tools` 20/28 ([#828](https://github.com/uppin/tddy-coder/pull/828)), the sweep the stack's
deferred deletions were pointed at. Packages: `tddy-coding-agent-specs`, `tddy-agent-backend`,
`tddy-agent-launch`, `tddy-cli-sessions`, `tddy-session-lifecycle`, `tddy-session-files`,
`tddy-session-split`, `tddy-daemon-rpc`, `tddy-service` (proto), `tddy-tool-engine`,
`tddy-presenter`, `tddy-tools`, `tddy-tools-cli`, `tddy-sandbox`, `tddy-sandbox-app`, `tddy-web`,
`tddy-integration-tests` (dev-dependency only).

## What changed

Every coding agent's metadata — session type, label/product, binary and its env override, CLI
argv, model catalogue, context globs, guidance files, route config env, capabilities,
`requires_model`, permission modes, placements, `transcript_usage` — is read from
`tddy-coding-agent-specs/specs/*.yaml` and nowhere else. Schema gaps were closed in YAML instead
of keeping a Rust field: the argv templates double as the CLI-arg schema
(`CodingAgentSpec::argv_params`, `binds_argv_param`, `render_run_argv`, `model_listing_argv`),
curated models carry picker labels (`CuratedModel`), and the section gained `permission_modes`
and `transcript_usage`.

- **No package gained a compile dependency on `tddy-coding-agent-specs`** (developer direction,
  2026-11-06). Specs are injected as plain data by binary and composition-root modules:
  `AgentChoice` into `tddy-tool-engine`, a `default_models` map into `tddy-presenter`,
  `[AgentModels]` into `tddy-tools`, guidance files and context globs as values into sandbox and
  session code. The only new specs dependencies are `tddy-tools-cli` (a binary crate) and a
  dev-dependency of `tddy-integration-tests` (test fixtures). `tddy-tools list-models` takes the
  caller's resolved binary under the generic `--binary-path`; no agent has a flag of its own.
- **The web bundle hard-codes no agent.** The serving daemon's selection comes from the injected
  `GET /api/config` payload (`ClientConfig.allowed_agents`); a target daemon's metadata comes
  from `CatalogService.ListAgents`. `AgentInfo` gains `session_type`, `capabilities`,
  `run_params`, `permission_modes`, `default_model` and `placements` (proto fields 6–11), and
  the Create-session pane, request builder, placement gates, provider harness, terminal-tab
  kinds and the accounts "Default for" control are all derived from that served metadata.
- Protocol adapters (stream parsers, hook-file writers) stay as code, selected by the name a
  spec gives them; they own no metadata.

## Removed

- `CliHarness` / `CliLaunch` and the registry (`cli_harness.rs`, `cli_harness_muse.rs`,
  `cli_harness_warp.rs`), `claude_argv.rs`, and the legacy `--agent-kind` helpers in
  `cli_harness_jail.rs` with their pinned tests.
- `curated_models_for_agent` / `claude_cli_models` / `cursor_cli_models` and
  `context_globs_for_agent` / `SHARED_CONTEXT_GLOBS` (`backend/context_globs.rs`); listings and
  globs are spec-fed (`SpecHeadlessBackend::with_curated_models`, `curated_models()`).
- The bespoke ACP backends (`backend/acp.rs`, `codex_acp.rs`, `opencode_acp.rs`); every ACP
  agent runs through `SpecAcpBackend` with the hooks adapter its spec names.
- Callerless launch leftovers on the launch path: `launch_cli_harness` (cli-sessions),
  `refuse_an_unsupported_cli_harness_start`, `pty_relay`'s default agent, and `tddy-sandbox`'s
  `PREAMBLE_FILES` (callers thread the spec's `instructions.files`).

**Not removed:** the approval-gated helpers in
[callerless-bespoke-launch-helpers](../todo/2026-11-05-callerless-bespoke-launch-helpers.md)
(`warm_up_jail_agents`, the `jail_worktree` trio, `create_routed_session_dir`, the `chat::*`
notify helpers, the `relaunch_jail_*` trio). They still carry `#[allow(dead_code)]`; deleting
them was not in the sweep's approved scope.

## Code issues

Final measurements at the sweep's wrap (production lines, counted to the first `#[cfg(test)]`):

| Record | Final measurement |
|---|---|
| `tddy-agent-launch` `oversized-file-cli-harness-jail` | 591 → **388** — under the 500 budget; record deleted |
| `tddy-agent-backend` `oversized-file-backend-mod` | 851 → **537** — partially fixed; seams A–D remain |
| `tddy-coding-agent-specs` `oversized-file-spec` | 815 → **981** — regressed by the CLI-arg schema; split still to be scheduled |
| `tddy-agent-backend` `oversized-file-spec-headless` | 646 → **653** — worse by 7 (`with_curated_models`); split still to be scheduled |

## Backlog

- **Deleted (resolved here):** `2026-11-03-harness-clis-legacy-cli-harness-removal`,
  `2026-11-05-model-listing-and-bespoke-acp-backends-remain`.
- **Narrowed:** `2026-11-05-callerless-bespoke-launch-helpers` (the legacy jail helpers went;
  the approval-gated helpers remain), `2026-11-03-tddy-sandbox-still-names-the-guidance-files-of-two-agents`
  (functional threading done in the sweep; comment/fixture vocabulary remains).

## Tests

Behaviour preserved, pinned against the YAML that replaced the tables: spec-rendered argv
acceptance suites, `no_agent_name_gates` / `no_agent_vocabulary` source scans, and the Cypress
component suites whose fixtures now serve the daemon's wire
(`packages/tddy-web/cypress/support/servedAgents.ts`). CI on #828: Web tests 3338/3338, Rust
tests 12089/12089, Rust e2e 637/637, lint and both builds green.
