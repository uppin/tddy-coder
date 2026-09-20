# Changeset: sandboxed-codebase-managed-workflow

State A → State B amendment: [`docs/ft/daemon/amendments/PRD-2026-09-20-sandboxed-codebase-managed-workflow.md`](../../ft/daemon/amendments/PRD-2026-09-20-sandboxed-codebase-managed-workflow.md)
PRD (target): [`docs/ft/daemon/remote-managed-worktree.md`](../../ft/daemon/remote-managed-worktree.md)
Discovery: [`2026-09-20-sandboxed-codebase-managed-workflow-initial-discovery.md`](2026-09-20-sandboxed-codebase-managed-workflow-initial-discovery.md)
PR: [#521](https://github.com/uppin/tddy-coder/pull/521)

## Responsibility

- Remove the `sandboxed_codebase` × `managed_codebase` refusal in `connection_service.rs`; carry
  `managed_codebase` and `specialized_agents` through the sandboxed-codebase start path.
- Remove the `sandboxed_codebase` × `dangerously_skip_permissions` refusal, and state in the PRD
  what confinement guarantee that combination gives up.
- Hoist the specialized-agent picker and the Semantic-index toggle out of the Managed-codebase
  block so every placement offers them.
- Prove a specialized agent can be opened and prompted from a sandboxed-codebase session.

## Boundaries

- The four surviving refusals (`sandbox`, `codebase_daemon_instance_id`, non-`claude-cli`
  `session_type`, `recipe`) keep their behaviour and their tests.
- The Sandboxed-codebase toggle stays its own control; this changeset does not restructure the
  placement radio.
- No change to `TDDY_REPO_DIR` resolution — that is what keeps `recipe` refused.

## Dependencies

- `SessionToolTransport::DaemonUds` (commit `d4ef5a69`, this branch). A roster needs a full RPC
  client; the HTTP relay is not one. Already landed here — do not re-implement.

## Prerequisites

| Record | Verdict | Action |
|---|---|---|
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-spawn-split-agent-spawn-split-agent.md` | ⚠ during — 233 lines, nesting 5, 9 params; unclaimed; PR #518 grew it | Do **not** add parameters or nesting to `spawn_split_agent`. Managed wiring goes through the existing `SplitAgentWiring` struct. Re-measure at wrap |
| `packages/tddy-session-lifecycle/docs/code-issues/oversized-file-connection-service.md` | ⚠ during — ~1,930 lines, unclaimed | This change **removes** ~12 lines of refusal from it. Re-measure at wrap; narrow, do not delete |
| `packages/tddy-session-lifecycle/docs/code-issues/complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session.md` | ⚠ during — adjacent start path | Re-measure at wrap |
| [`docs/dev/todo/2026-08-17-session-agent-roster…md`](../todo/2026-08-17-session-agent-roster-the-sandbox-bridge-and-other-deliberate-gaps.md) | ℹ answered | In-jail subagents closed 2026-08-29; this placement is the inverted case and uses `DaemonUds` |

No 🚧 claimed issue is in this change's path, so there is no wait-or-proceed fork to put to the
developer.

## Scope

- `packages/tddy-session-lifecycle/src/connection_service.rs` — delete two refusal arms.
- `packages/tddy-session-lifecycle/src/connection_service/svc_start_sandboxed_codebase_session.rs`
  — carry `managed_codebase` / `specialized_agents` into the start.
- `packages/tddy-session-lifecycle/tests/sandboxed_codebase_placement_acceptance.rs` — invert two
  refusal tests into acceptance tests; keep the other four.
- `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` — hoist the picker.
- `packages/tddy-web/src/components/sessions/CreateSessionManagedCodebaseFields.tsx` — stop owning it.
- `docs/ft/daemon/remote-managed-worktree.md` — exclusion table loses a row (at wrap).

## Acceptance tests

| # | Test | Where |
|---|---|---|
| AC1 | a sandboxed-codebase session may be managed, and comes up with both its jail and its roster | `sandboxed_codebase_placement_acceptance.rs` |
| AC2 | a sandboxed-codebase session may skip permissions, and is still jailed | `sandboxed_codebase_placement_acceptance.rs` |
| AC3 | the four remaining exclusions are still refused, each naming both fields | `sandboxed_codebase_placement_acceptance.rs` |
| AC4 | the form offers the agent picker on a sandboxed-codebase session and submits its ids | `CreateSessionSandboxedCodebaseAcceptance.cy.tsx` |
| AC5 | a specialized agent on a sandboxed-codebase session can be opened and prompted | `sandboxed_codebase_placement_acceptance.rs` |

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset
- [ ] Write failing acceptance tests (AC1–AC5)
- [ ] Red phase — unit/integration tests
- [ ] Green phase
- [ ] Wrap: exclusion table row, re-measure the three code issues
