# PRD Amendment: sandboxed-codebase managed workflow

Amends [`docs/ft/daemon/remote-managed-worktree.md`](../remote-managed-worktree.md) (State A).
Dependency: the agent-facing tool socket (`SessionToolTransport::DaemonUds`, commit `d4ef5a69` on
this branch) — without a full RPC client to the daemon, a subagent on this placement cannot be
addressed at all.

## Problem

`StartSessionRequest.sandboxed_codebase = true` together with `managed_codebase = true` is refused
with `invalid_argument`:

> sandboxed_codebase is mutually exclusive with managed_codebase: sandboxed_codebase jails the
> codebase and leaves the agent on the host, managed_codebase jails the agent and leaves the
> codebase on it — a session has one placement, not two

**The justification describes the wrong flag.** "Jails the agent and leaves the codebase on it" is
`sandbox`. `managed_codebase` jails nothing: it is the orchestration axis — a `recipe`, a
`specialized_agents` roster, a seeded `changeset.yaml`, a `WorkflowController` and a per-session
toolcall listener ([`managed-codebase-workflow.md`](../../coder/managed-codebase-workflow.md)
§ Architecture). That document treats confinement as orthogonal throughout, and its own start-path
line — `start_(sandboxed_)claude_cli_session | start_sandboxed_cursor_cli_session (managed_recipe)`
— shows managed already composing with sandboxing on every other placement.

The practical cost is that **a sandboxed-codebase session cannot be given specialized agents**,
which is the one placement where they are most useful: the checkout is confined, so delegating work
to subagents carries the least risk.

A second, independent defect compounds it. The create-session form renders the specialized-agent
picker *inside* the Managed-codebase block (`CreateSessionPane.tsx:669`), so choosing this placement
removes the control from the page. Both this PRD (§ "What a split session cannot also ask for") and
the component's own comment state the picker is offered on **every** placement. The code has never
matched.

## State A (current)

- `connection_service.rs:1071` refuses `sandboxed_codebase` + `managed_codebase`.
- `connection_service.rs:1077` refuses `sandboxed_codebase` + `dangerously_skip_permissions`.
- The agent picker and the Semantic-index toggle render only when `managedCodebase` is true.
- `specialized_agents` is submitted only from inside that block.
- No subagent has ever been dispatched on a sandboxed-codebase session.

## State B (target)

### `managed_codebase` composes with `sandboxed_codebase`

The two are orthogonal and both are honoured: the checkout is jailed, the agent runs beside it with
its native tools withdrawn, **and** the session carries its recipe, its seeded changeset, its
toolcall listener and its specialized-agent roster.

The exclusion table in § "What a sandboxed-codebase session cannot also ask for" loses its
`managed_codebase` row. `sandbox`, `codebase_daemon_instance_id`, `session_type ≠ claude-cli` and
`recipe` keep theirs — each for a reason that survives scrutiny:

| Still refused | Because |
|---|---|
| `sandbox` | genuinely the opposite placement: it jails the agent this one leaves on the host |
| `codebase_daemon_instance_id` | the cross-host form of this same inversion |
| `session_type` ≠ `claude-cli` | no other agent's tool surface can be withdrawn |
| `recipe` | `TDDY_REPO_DIR` resolves where the *agent* is, not where the code is — a real technical gap, not a category error. A managed session **without** a recipe is now allowed, which is what carries the roster |

### `dangerously_skip_permissions` is permitted, and the cost is stated

The flag is no longer refused. **What is given up is stated here rather than left implicit:** this
placement's confinement claim has two layers — the Seatbelt jail around the checkout, and the
withdrawn-tool deny list that forces every codebase access through `mcp__tddy-tools__*`. The flag
bypasses the second. The jail still holds, so the filesystem outside the checkout stays confined on
macOS; what is lost is the guarantee that the agent *only* reaches the code through the routed tool
surface.

A session that sets both is therefore confined by the kernel and unconfined by policy. The operator
asked for it explicitly; the daemon serves it and records the combination in the session's wiring
log.

### The agent picker is offered on every placement

The specialized-agent picker and the Semantic-index toggle move out of the Managed-codebase block,
matching what this PRD already claims. Nothing about a placement decides whether an agent can be
attached — only where that agent reads the codebase from, which the roster half already answers
(`rosterHalfOf` → `codebase_session_id`).

### Subagents dispatch end to end

A specialized agent named at start is reachable from a sandboxed-codebase session: the roster
streams, a conversation opens, a prompt returns. The transport is `DaemonUds` on an embedded daemon
and the HTTP relay on a served one.

## Acceptance criteria

1. `StartSession` with `sandboxed_codebase = true` **and** `managed_codebase = true` is accepted,
   and the session comes up with both its jail and its roster.
2. `StartSession` with `sandboxed_codebase = true` and `dangerously_skip_permissions = true` is
   accepted; the jail is still provisioned.
3. `sandbox`, `codebase_daemon_instance_id`, a non-`claude-cli` `session_type` and `recipe` are
   still refused, each naming both fields.
4. The create-session form offers the specialized-agent picker and the Semantic-index toggle when
   Sandboxed codebase is selected, and submits `specialized_agents` with the request.
5. A specialized agent on a sandboxed-codebase session can be opened and prompted, and its answer
   reaches the calling agent.

## Non-goals

- **Lifting the `recipe` refusal.** It needs `TDDY_REPO_DIR` to resolve to the jailed checkout,
  which is its own change.
- **Lifting `sandbox`.** Jailing both halves is a confinement mode nobody has run; it deserves its
  own PRD rather than a line in this one.
- **Changing where the toggle sits in the form.** The placement stays its own control; only the
  picker moves.
