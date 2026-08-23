# Seeded agents (and the semantic index) on any codebase placement - PRD

**Date**: 2026-08-23
**PRD Type**: Requirement Update

## Affected Features

- **Primary Feature**: [Session agent roster](../session-agent-roster.md) — § Create-session picker
  (the withdrawal is deleted), § Remote agents (a seed at start is a first-class case), § Tool
  replacement (the spawn allowlist is derived from the roster, not from the request's names).
- **Related Feature**: [Remote managed worktree](../remote-managed-worktree.md) — § What a split
  session cannot also ask for loses two of its four rows.
- **Related Feature**: [Semantic index](../../coder/semantic-index.md) — the index is built where the
  worktree is, which for a split session is the codebase host.
- **Related Feature**: [Session worktree sync](../session-worktree-sync.md) — unchanged mechanism,
  but it is now also what a *seeded* agent's clone is kept current by.

## Summary

Selecting a specialized agent is never gated by where the codebase lives. What the placement decides
is **how the session is split across hosts**, not whether the selection is admissible:

- an agent on the same host as the codebase reads that codebase directly — no clone, no sync;
- an agent on any other host gets a clone kept current by the session worktree sync, reads it
  locally, and proxies writes to the authoritative worktree.

That is already exactly what `AttachSessionAgent` does after a session has started. This PRD makes
the **start-time seed** behave the same way, for every placement, and removes the two refusals and
the one UI withdrawal that exist only because it did not.

The semantic index is ungated on the same reasoning: it indexes a worktree, that worktree exists on
the codebase host, so it is built there.

## Background

The roster shipped with three start-time restrictions:

| Restriction | Where |
|---|---|
| A peer-owned agent cannot be named at start, on **any** session | `remote_agent_at_start_unsupported`, `packages/tddy-daemon/src/connection_service.rs:2453` |
| **No** agent can be named at start on a split session | `start_split_claude_cli_session`, `packages/tddy-daemon/src/connection_service.rs:7370` |
| The picker is withdrawn once a codebase host is chosen | `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx:1124` |

All three rest on one premise, stated in the refusal itself: *"that peer is admitted to the session's
room and given its clone only once the room exists"*, and the room is opened by the spawn.

**The premise is false.** `claim_agent_clone` opens the session's room itself, off the checkout the
roster holder has (`connection_service.rs:3383` — *"opened {} as {} so an owning daemon can be
admitted to it"*). Nothing about admitting a peer waits on the agent's spawn. And because a roster
call for a split session is routed to the codebase host *before* the session is looked up
(`connection_service.rs:8869`), "the local host" in the attach path already means the host holding
the authoritative worktree — so the co-located/remote split the attach path performs is already the
rule this PRD asks for, applied to the right host.

What was actually missing is an ordering: the seed has to be resolved into the roster **before** the
agent is spawned, because the spawn's tool withdrawal is fixed at launch.

## Proposed Changes

### What's Changing

1. **`specialized_agents` is accepted on every placement**, split included. The refusal at
   `connection_service.rs:7370` is deleted, and `remote-managed-worktree.md` § What a split session
   cannot also ask for drops the `specialized_agents` row.
2. **A qualified id naming a peer resolves from that peer.**
   `remote_agent_at_start_unsupported` is deleted; `resolve_specialized_agent_defs` stops comparing
   against the starting daemon's instance id and resolves each reference the way
   `roster_record_for_agent_id` does for an attach.
3. **The seed is a roster write, performed before the spawn**, routed to the daemon that holds the
   roster (the codebase host for a split placement). For every seeded agent not co-located with the
   authoritative worktree, a clone is claimed exactly as an attach claims one — one per
   (session, remote daemon), kept current by the session worktree sync.
4. **The spawn's tool withdrawal is derived from the persisted roster.** The runner computes its
   replaced set from `TDDY_SUBAGENTS_JSON` (`packages/tddy-sandbox-runner/src/runner.rs:1484`); the
   daemon fills that env from the roster it has just written rather than from the request's names, so
   a seeded remote agent withdraws its tools at launch and not only at the next resume.
5. **Atomicity is the split path's, extended to the roster.** A start that fails after clones were
   claimed tears them down, and a roster write that fails leaves no clone — the same
   "no roster entry, no half-built clone, no room membership" contract attach already keeps.
6. **Clone readiness gates prompts, not session start.** A seeded agent whose clone is still
   provisioning is refused *by prompt*, naming the state (this is the existing AC33). Session start
   does not block on a peer, and the model warm-up gate that today runs on the starting daemon runs
   on the daemon that owns the agent.
7. **`semantic_index` is accepted on a split placement**, built on the codebase host against the
   worktree that exists there, with the index-DB env pair exported to that host's exec-tool surface —
   which is where a split session's `mcp__tddy-tools__SemanticSearch` already lands.
8. **The web offers both controls regardless of placement.** `CreateSessionPane` drops the
   `!isSplitCodebase` guard on the agent picker and on the Semantic index toggle;
   `session-agent-roster.md` § Create-session picker loses its withdrawal paragraph.

### What's Staying the Same

- **Reads local, writes proxied.** There is exactly one worktree that counts. A seeded remote agent's
  `Write`/`StrReplace`/`Delete`/`Shell`/`Await` proxies to the host holding it, and `SHELL` running
  there rather than on the agent's own host remains the documented sharp edge.
- **One clone per (session, remote daemon)**, sync one-way, `.tddy-session-sync.json` marker written.
- **Withdrawal enforcement**, both layers, and the `workspace`-session back-pointer refusal
  (`agent_daemon_instance_id` / `agent_session_id`) that makes a split session's codebase half a
  shape a withdrawal is enforceable on.
- **`recipe` and `sandbox` stay refused on a split placement.** Both resolve a repository *on the
  daemon running the agent* for reasons that are not about where a read can be served: a recipe's
  tooling runs the agent's own host-side transition, and the sandboxed spawn jails that host's
  filesystem. Out of scope here.
- **Split placement remains `claude-cli` only**, and `codebase_daemon_instance_id` remains meaningful
  only with `managed_codebase = true`.
- **No new proto fields.** `specialized_agents` already carries qualified ids, and
  `semantic_index` already crosses the wire; the split forward already carries the back-pointer
  (`connection.proto:454`).

## Impact Analysis

### Technical Impact

| Area | Change |
|---|---|
| `packages/tddy-daemon/src/connection_service.rs` | Delete `remote_agent_at_start_unsupported` and the `specialized_agents` / `semantic_index` refusals in `start_split_claude_cli_session`; resolve seeds through the roster; write the roster (and claim clones) before the spawn; derive the subagent env from the roster; unwind on failure |
| `packages/tddy-daemon/src/split_session.rs` | The split forward asks the codebase host to build the index when the request carries `semantic_index` |
| `packages/tddy-daemon/src/session_agent_clone.rs`, `session_agent_roster.rs` | Reused as-is; the seed path is a second caller, not a second implementation |
| `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` | Two guards removed; `specializedAgents` / `semanticIndex` are no longer blanked for a split submit |
| `packages/tddy-daemon/tests/*` | New acceptance coverage per placement class |

**Performance**: a start that seeds a non-co-located agent now provisions a clone before the spawn,
so it inherits the clone's provisioning cost. Readiness is not waited on (change 6), so the added
latency is the peer's `StartSession` for a `workspace` session, not a checkout.

**API compatibility**: no wire change. A client that never sends a qualified id or a split placement
sees identical behaviour. Two `invalid_argument` / `unimplemented` refusals stop being returned,
which no client can depend on except to display.

### User Impact

- The agent picker no longer disappears when a codebase host is chosen, and the agents it offers
  from other hosts can now be selected at start rather than attached afterwards.
- The Semantic index toggle no longer disappears either.
- A seeded agent may appear in the roster as `provisioning` and refuse a prompt until its clone is
  ready — visible in the Agent roster pane, where clone state is already a column.
- No migration: existing sessions are unaffected, and an existing split session can still attach
  agents exactly as it does today.

⚠️ **Known limitation, unchanged by this PRD.** `SemanticSearch` cannot answer a query on *any*
session shape yet — `tool_semantic_search` returns *"index query not yet wired"*
(`packages/tddy-tool-engine/src/lib.rs:688`) because the query-side embedder is not wired into the
tool engine. Ungating the index on a split placement therefore delivers an index built on the correct
host and a tool that reports the same not-yet-wired error it reports on a co-located session. This is
deliberate — the placement refusal is what is being removed, not the query gap — and the query gap
stays tracked in [semantic-index.md](../../coder/semantic-index.md).

## Implementation Plan

1. **Seed resolution through the roster.** Replace `resolve_specialized_agent_defs`'s local-only
   resolution with the attach path's record resolution, keeping the "unknown reference is a request
   error" contract (a session is never started with a silently-dropped agent).
2. **Ordering.** In both the co-located and split start paths, write the roster and claim any clones
   before the spawn, then derive `TDDY_SUBAGENTS_JSON` from the persisted roster.
3. **Unwind.** Extend the split path's existing teardown to the roster and clones.
4. **Warm-up.** Move the readiness gate to the owning daemon and demote it from a start gate to the
   existing per-prompt clone-state refusal.
5. **Semantic index on a split.** Thread `semantic_index` into the codebase host's `workspace`
   session start, and export the index-DB env pair to that host's exec-tool surface.
6. **Web.** Remove both guards and both submit-time blanks.
7. **Docs.** Rewrite the two feature-doc sections named above once the behaviour lands.

## Acceptance Criteria

- [ ] AC1 — A session started with an agent owned by a **peer** succeeds, and the roster records that
      daemon and the clone serving it ([session agent roster](../session-agent-roster.md)).
- [ ] AC2 — A **split** session started with an agent owned by the **codebase host** succeeds with
      **no clone**: that agent reads the authoritative worktree directly.
- [ ] AC3 — A **split** session started with an agent owned by a **third host** succeeds with one
      clone on that host, carrying the `.tddy-session-sync.json` mirror marker.
- [ ] AC4 — A split session started with two agents owned by the same third host gets **one** clone.
- [ ] AC5 — A seeded agent's `replaces` is withdrawn from the main agent **at launch**, not at the
      first resume — the spawn's tool set is derived from the persisted roster.
- [ ] AC6 — A prompt to a seeded agent whose clone is still provisioning is refused naming the clone
      state; session start itself never blocks on the owning daemon's model.
- [ ] AC7 — A start that fails after clones were claimed leaves no clone, no roster entry and no room
      membership on any host.
- [ ] AC8 — An unresolvable agent reference still fails the start with `invalid_argument` naming the
      reference; no session is created.
- [ ] AC9 — A split session started with `semantic_index` succeeds, and the index is built on the
      **codebase host** against its worktree.
- [ ] AC10 — `recipe` and `sandbox` on a split placement are still refused, each naming its field.
- [ ] AC11 — `CreateSessionPane` keeps the agent picker and the Semantic index toggle visible when a
      codebase host is selected, and submits both.
- [ ] AC12 — Tests passing for all affected features.

## References

### Affected Features (Complete List)

- [Session agent roster](../session-agent-roster.md) — § Create-session picker (withdrawal deleted),
  § Remote agents (seed-at-start added), § Tool replacement (allowlist derived from the roster).
- [Remote managed worktree](../remote-managed-worktree.md) — § What a split session cannot also ask
  for (two rows removed), and the UI paragraph that mandated the withdrawal.
- [Semantic index](../../coder/semantic-index.md) — built where the worktree is.
- [Session worktree sync](../session-worktree-sync.md) — what a seeded remote agent's clone is kept
  current by.

### Related Documentation

- Refusals removed: `packages/tddy-daemon/src/connection_service.rs:2453`, `:7370`
- The room-opening the refusals assumed impossible: `packages/tddy-daemon/src/connection_service.rs:3383`
- Roster routing that already means "the codebase host": `packages/tddy-daemon/src/connection_service.rs:8869`
- Spawn-time replaced-tools source: `packages/tddy-sandbox-runner/src/runner.rs:1484`
- Unwired query path: `packages/tddy-tool-engine/src/lib.rs:688`
