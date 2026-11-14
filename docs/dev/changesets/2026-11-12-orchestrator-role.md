# 2026-11-12 — Orchestrator role: admission, placement, reach

**Type:** Feature

`#orchestrators` 1/11, PR [#878](https://github.com/uppin/tddy-coder/pull/878), stack
[#889](https://github.com/uppin/tddy-coder/stack/889). Feature:
[project-concept.md § Orchestrator sessions](../../ft/daemon/project-concept.md#orchestrator-sessions).

## What changed

- **`tddy-changeset`**: `ORCHESTRATOR_ROLE` and `is_orchestrator_role` (true for `project-home` and
  `orchestrator`). Five exact-string gates call it: launch wiring, the read-only guard,
  `session_tool_role`, the cross-host owner check and `refuse_a_role_the_agent_cannot_hold`.
  Auto-stop still matches `project-home`.
- **`tddy-agent-launch`**: `orchestrator_start.rs` adds `admit_the_requested_role` (person or
  orchestrator-role agent), `place_an_orchestrator` (main's `on_home_worktree` and refusals, sandbox
  refused, home fast-forwarded) and a fast-forward on resume. The orchestrator link is written in the
  child's first changeset (`OrchestratorLink`, every start path), and `record_orchestrator` is
  removed.
- **`tddy-github`**: an agent token carries `session_id`, the session it was minted for. The field is
  optional and backward-compatible. `mint_agent(user, ttl, session_id)`.
- **`tddy-session-split`**: `mint_agent_session_token` / `agent_session_token_for` take the agent's
  own session at all four mint sites. `bearer_of(token) -> TokenBearer`. A split agent's first
  changeset records its orchestrator before spawn.
- **`tddy-tool-engine`**: exec `StartSession` offers `session_role` (one-value enum). Git tools refuse
  a target holding an orchestrator role (`ProjectHomePort::session_role_of`).
- **`tddy-daemon-rpc`**: the role guard and the owner check go through the predicate.
- **web**: the `CreateSessionOrchestratorField` switch and request builder, and the drawer badge.

## Decisions

- **An agent's role is proven by its token, not named by its request** (developer, 2026-11-12). Admission
  first read the requester's role from `orchestrator_session_id`. An ordinary agent calling the RPC
  directly could name main and be admitted. Agent tokens now name their session, and an agent token
  without one is granted no role.
- **A second role value, not a reuse of `project-home`**: auto-stop and `home_session_id` keep
  meaning main.
- **Sandbox is refused for orchestrators**: a jailed session gets no tool wiring
  ([todo](../todo/2026-10-24-sandboxed-main-needs-session-role.md)).

## Restructuring (file-length gate)

| File | Before → after (production lines) |
|---|---|
| `tddy-session-split/src/svc_spawn_split_agent.rs` | 618 → 422; the launch helpers moved to `svc_spawn_split_agent/launch.rs` (224) by engine moves (`move_impl_members`, `move_item`) |
| `tddy-session-lifecycle/src/test_util.rs` | 579 → 474; the signed-credential host moved to `test_util/signing.rs` (120), `reexport: outside` |

Both `oversized-file` records are closed and deleted. `CreateSessionPane.tsx` shrank from 595 to 593
lines and stays over budget (its record has a new row).

## Resolved backlog

- `2026-10-25-start-session-orchestrator-recorded-after-start.md`: deleted.
