# Changeset: agent-tools sweep — agent metadata lives only in YAML

**Date**: 2026-11-06
**Status**: 🚧 In Progress
**Type**: Refactor

## Initial Discovery

[2026-11-06-agent-tools-sweep-yaml-only-initial-discovery.md](./2026-11-06-agent-tools-sweep-yaml-only-initial-discovery.md)

## Affected Packages

- **tddy-agent-backend**: legacy `CliHarness` table, `claude_argv`, curated
  model lists and context-glob table deleted; model listing becomes spec-fed.
- **tddy-coding-agent-specs**: registry gains the cached builtin view and any
  CLI-arg schema fields a deleted descriptor field still needs.
- **tddy-agent-launch**, **tddy-cli-sessions**, **tddy-session-lifecycle**,
  **tddy-session-files**, **tddy-daemon-rpc**, **tddy-tool-engine**,
  **tddy-sandbox**, **tddy-tools**, **tddy-web**: consumers read specs.

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

Every coding agent's metadata — session type, label, binary and its env
override, CLI argv, model catalogue, context globs, route config env,
`requires_model` — is read from `tddy-coding-agent-specs/specs/*.yaml` and
nowhere else. The legacy Rust descriptor table and every hard-coded agent
branch it fed are deleted. Protocol adapters (stream parsers, hook-file
writers) stay as code, selected by name from YAML.

## Background

`#agent-tools` 1–19/19 moved launches onto specs but deferred the deletions
to this sweep (n20): `docs/dev/todo/2026-11-03-harness-clis-legacy-cli-harness-removal.md`,
`docs/dev/todo/2026-11-05-callerless-bespoke-launch-helpers.md`,
`docs/dev/todo/2026-11-05-model-listing-and-bespoke-acp-backends-remain.md`.
The developer approved the deletions on 2026-11-06 and directed that any
schema gap (e.g. CLI arg schema) be closed by extending the YAML.

## Scope

- [x] Legacy `CliHarness` table + `claude_argv` deleted; consumers on specs
- [x] Curated model lists + context-glob table deleted; listings spec-fed
- [x] Callerless bespoke launch helpers deleted
- [x] `tddy-sandbox` guidance files passed in, not named
- [x] `pty_relay` names no default agent; web names no agent
- [x] UI node: the web bundle hard-codes no agent — session types, labels,
      capabilities and CLI launch params reach it from the daemon: the
      serving daemon's `GET /api/config` payload for its own selection, and
      `CatalogService.ListAgents` (per target host) for a session started on
      another daemon. `AgentInfo` gains `session_type`, `capabilities`, the
      spec argv's bound params, `permission_modes`, `default_model` and
      `placements`; `SESSION_TYPE_OPTIONS`,
      `isClaudeCliSession`/`isCursorCliSession` and the per-agent
      Create-session field components are derived from that metadata
- [x] Baseline re-run at zero regressions (by failing-test name) — scoped
      batches green; the two failures seen are pre-existing on the base
      (`tddy-daemon-sandbox` seatbelt test does not compile at HEAD) or
      flaky (seatbelt jail provisioning, green on re-run)
- [ ] Deferred records closed/narrowed at wrap

## Technical Changes

### State A

See Initial Discovery. Headline: `cli_harness.rs` (208 lines) +
`cli_harness_muse.rs` + `cli_harness_warp.rs` + `claude_argv.rs`, ~14
production consumer sites, `curated_models_for_agent` / `claude_cli_models` /
`cursor_cli_models`, `context_globs_for_agent`, legacy jail helpers below the
`--agent-kind` line in `cli_harness_jail.rs`.

### State B

| Metadata | Only home after sweep |
|---|---|
| session type, label/product | spec `id`, `coding_agent.product` |
| binary, env override, route config env | tool `binary`, `coding_agent.route.config_env` |
| CLI argv (PTY + headless) | `run.argv`, `coding_agent.headless.argv` |
| models (curated / listed) | `coding_agent.models` |
| context globs, instructions files | `coding_agent.context_globs`, `instructions.files` |
| requires-model, placements, capabilities | `coding_agent.capabilities`, `placements` |

### Callers

Outside `tddy-agent-backend`: `tddy-agent-launch` (facilitation, jail,
start checks), `tddy-cli-sessions` (argv), `tddy-session-lifecycle`
(terminal ports), `tddy-session-files` (context files), `tddy-daemon-rpc`
(catalog), `tddy-tool-engine` (start schema), `tddy-tools` (`pty_relay`,
`list-models`), `tddy-web` (session fields). Each reads the registry/spec it
already holds, or the cached builtin registry where it held only the table.

## Prerequisites

Step 2b verdicts: no `Claimed by:` record in any touched package.
Oversized-file records (`backend-mod`, `cli-harness-jail`, `spec`) —
improved by deletion, not closed; narrowed at wrap.

## Testing Plan

Baseline: `./test -p tddy-agent-backend -p tddy-coding-agent-specs` recorded
before the first deletion; final gate re-runs every touched package and
compares failing tests by name. A durable source scan (extending
`tddy-session-lifecycle/tests/no_agent_name_gates.rs` /
`tddy-sandbox/tests/no_agent_vocabulary.rs`) pins the result.

## Decisions & Trade-offs

- Protocol adapters keep agent-protocol names in code (`claude-stream-json`
  is a YAML-selected adapter id, not metadata). Renaming them would hide the
  protocol they implement without moving any metadata.
- Where a consumer held only the static table and no host registry
  (facilitation, start-schema), it reads the cached builtin registry — the
  same agents the table offered; dropped-in agents were never in the table.
- No package gained a compile dependency on `tddy-coding-agent-specs`
  (developer direction, 2026-11-06): specs are injected as plain data by
  binary/composition-root modules — `AgentChoice` into `tddy-tool-engine`,
  the `default_models` map into `tddy-presenter`, `[AgentModels]` into
  `tddy-tools`, guidance files/globs as values into sandbox/session code.
  The one new specs dependency is `tddy-tools-cli`, a binary crate.

## Final Checklist

- [x] `rg 'cli_harness|CliHarness|CliLaunch'` in production `src` finds only
      the generic harness launch mechanism (`cli_harness_tmux`, `cli_harness_jail`
      — the tmux/jail launch of any spec-driven agent), no agent table or descriptor
- [x] No production line compares a session type with an agent name
- [x] Scoped tests match baseline by failing-test name
- [ ] Deferred records deleted or narrowed with measurements
