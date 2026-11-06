# 2026-11-05 — The model listing still matches agent ids, and the bespoke ACP backends remain

**Category:** Deferred from `spec-backend` (#822, `#agent-tools 16/19`) to the `#agent-tools` sweep node (n20)

**Source:** `docs/dev/1-WIP/2026-11-04-agent-tools-spec-backend.md`, § Decisions & Trade-offs

## What remains

When #822 is green, every workflow backend `tddy-coder --agent <id>` builds is a `SpecBackend` made
from the agent's spec. Two things it does not delete stay behind:

- `packages/tddy-agent-backend/src/backend/model_catalog.rs` `build_backend` still matches agent ids
  to pick the backend whose `list_models` answers `tddy-tools list-models`
  (`packages/tddy-tools/src/list_models.rs:55` → `resolve_agent_models`). #822 changes its return
  type from `AnyBackend` to `Box<dyn CodingBackend>` so that `AnyBackend` can be deleted, and marks
  it `TODO(spec-backend)`. `tddy-agent-backend` cannot read a spec (the specs crate depends on it),
  so the listing has to move to a crate that can, or take the registry from its caller.
- `ClaudeAcpBackend`, `CodexAcpBackend`, `OpenCodeAcpBackend` (`backend/acp.rs`, `codex_acp.rs`,
  `opencode_acp.rs`) and `tddy_coder::acp_from_spec`'s three builders. `--agent` builds every ACP
  agent as a `SpecAcpBackend` once #822 is green, so they are left to `model_catalog::build_backend`
  and to the suites in `tddy-integration-tests` (`acp_backend_acceptance.rs`,
  `codex_acp_backend.rs`, `opencode_acp_backend.rs`, `tool_session_provider_route.rs`) and
  `tddy-agent-backend/tests/claude_acp_on_driver_acceptance.rs`.

## Why it is deferred

Neither deletion is on #822's **Removed** list. Moving the model listing onto specs changes
`tddy-tools`, which #822 does not touch. The stack's rule sends a deletion beyond a node's Removed
list to the sweep with a TODO and this record.

## What would close it

`resolve_agent_models` takes a `SpecBackend` (or the registry) from a caller that can read specs,
and `build_backend` is deleted. The suites above move to `SpecAcpBackend`, and then the three
bespoke ACP backends and `acp_from_spec`'s builders are deleted.
