# Changeset: Seeded agents (and the semantic index) on any codebase placement

**Date**: 2026-08-23
**Status**: 🚧 In Progress
**Type**: Architecture Change

## Affected Areas

- **Daemon** (`packages/tddy-daemon/src/`):
  - `connection_service.rs` — delete `remote_agent_at_start_unsupported` (:2453) and the
    `specialized_agents` / `semantic_index` refusals in `start_split_claude_cli_session` (:7345,
    :7370); resolve a seed through the roster instead of against this daemon's defs
    (`resolve_specialized_agent_defs`, :3286); write the roster and claim clones **before** the
    spawn; derive the spawn's subagent env from the persisted roster; unwind clones and roster on a
    failed start
  - `split_session.rs` — carry `semantic_index` into the codebase host's `workspace` session start
  - `session_agent_clone.rs`, `session_agent_roster.rs` — reused by the seed path; no new
    provisioning implementation
  - `semantic_index.rs` — the index-DB env pair is exported on the host that built the index
- **Web app** (`packages/tddy-web/src/components/sessions/`):
  - `CreateSessionPane.tsx` — remove the `!isSplitCodebase` guards on the agent picker (:1124) and
    the Semantic index toggle (:1127), and stop blanking `specializedAgents` / `semanticIndex` on a
    split submit (:572-573)
- **Sandbox runner** (`packages/tddy-sandbox-runner/src/`):
  - `runner.rs` — no change expected; `TDDY_SUBAGENTS_JSON` (:1484) stays the spawn's source of
    truth, and the daemon fills it from the roster
- **Documentation** (`docs/`):
  - `docs/ft/daemon/session-agent-roster.md` — § Create-session picker, § Remote agents,
    § Tool replacement
  - `docs/ft/daemon/remote-managed-worktree.md` — § What a split session cannot also ask for
  - `docs/ft/coder/semantic-index.md` — where the index is built

## Related Feature Documentation

- [PRD 2026-08-23 — Seeded agents on any codebase placement](../../ft/daemon/1-WIP/PRD-2026-08-23-seeded-agents-on-any-placement.md)
- [Session agent roster](../../ft/daemon/session-agent-roster.md)
- [Remote managed worktree](../../ft/daemon/remote-managed-worktree.md)
- [Session worktree sync](../../ft/daemon/session-worktree-sync.md)
- [Semantic index](../../ft/coder/semantic-index.md)

## Summary

`specialized_agents` and `semantic_index` become admissible on every codebase placement. A seeded
agent is resolved into the session's roster before the agent is spawned — on the daemon that holds
the authoritative worktree — and gets a clone only when it is not co-located with that worktree.

## Background

The roster's attach path already places an agent by comparing its owning daemon against the host
holding the worktree, and a roster call for a split session is routed to the codebase host before the
session is resolved. So the intended rule is implemented; only the **start-time** path refused to use
it, on the premise that a peer's clone cannot be provisioned until the spawn opens the session room.
`claim_agent_clone` opens that room itself (`connection_service.rs:3383`), so the premise is false
and the refusals are unnecessary. What is genuinely required is ordering: the roster must be written
before the spawn, because tool withdrawal is fixed at launch.

## Scope

**High-level deliverables tracking progress throughout development:**

- [ ] **Documentation**: PRD written; the three feature docs rewritten at wrap
- [ ] **Implementation**: Seed-through-roster, pre-spawn ordering, unwind, split semantic index, web ungating
- [ ] **Testing**: All acceptance tests passing
- [ ] **Integration**: Cross-host verified against the LiveKit testkit with two real daemons
- [ ] **Technical Debt**: Production readiness gaps addressed
- [ ] **Code Quality**: Builds clean, no warnings

## Technical Changes

### State A (Current)

- `resolve_specialized_agent_defs` (`connection_service.rs:3286`) resolves each reference against
  **this** daemon's `resolvable_agent_defs` and returns `remote_agent_at_start_unsupported`
  (`:2453`, `Code::Unimplemented`) for any qualified id naming a peer — on every session type.
- `start_split_claude_cli_session` (`:7331`) refuses four fields with `invalid_argument`: `recipe`,
  `semantic_index`, `sandbox`, `specialized_agents`.
- Seeded defs reach the spawn as `TDDY_SUBAGENTS_JSON`; the runner derives its replaced-tool set from
  that env (`runner.rs:1484`) and hard-disables the withdrawn tools' native aliases. The persisted
  roster is not consulted at spawn.
- `attach_session_agent` (`:8862`) routes by `daemon_instance_id` before session lookup, resolves the
  record (from the owning daemon when it is a peer), claims one clone per (session, remote daemon),
  and unwinds the clone if the roster write fails.
- `CreateSessionPane` withdraws the agent picker and the Semantic index toggle when
  `isSplitCodebase`, and blanks both fields on submit.
- `tool_semantic_search` (`packages/tddy-tool-engine/src/lib.rs:656`) returns
  `"index query not yet wired"` for every session shape.

### State B (Target)

- A seed is a list of qualified agent ids resolved the way an attach resolves them: from the owning
  daemon, with an unresolvable reference still failing the start with `invalid_argument`.
- Start order becomes: resolve placement → resolve seed records → **write roster (claiming clones for
  agents not co-located with the authoritative worktree)** → spawn with the subagent env derived from
  that roster → on any later failure, unwind roster and clones.
- For a split placement the roster write is routed to the codebase host, so "co-located" there means
  "owned by the codebase host" — an agent on that host reads the authoritative worktree with no clone.
- `semantic_index` on a split placement is built by the codebase host for its `workspace` session,
  and that host exports `TDDY_SEMANTIC_INDEX_DB` on the exec-tool surface a split session's
  `mcp__tddy-tools__SemanticSearch` already reaches.
- Session start never blocks on a peer's model: readiness is the clone's, and a prompt to an agent
  whose clone is provisioning is refused naming the state.
- `recipe` and `sandbox` remain refused on a split placement, each naming its field.
- The web offers both controls on every placement and submits them.

### Delta (What's Changing)

#### Daemon (`packages/tddy-daemon/`)

- **Architecture**: one seeding mechanism. The roster becomes the record a spawn's tool withdrawal is
  derived from, rather than a parallel structure attach maintains after the fact.
- **API**: `resolve_specialized_agent_defs` returns roster records rather than local defs;
  `remote_agent_at_start_unsupported` deleted; two refusals deleted from
  `start_split_claude_cli_session`.
- **Implementation**: the seed path calls the existing `claim_agent_clone` / roster `attach`; the
  split path's existing teardown is extended to cover them.

#### Web app (`packages/tddy-web/`)

- **Integration**: two guards and two submit-time blanks removed; no new state, no new RPC.

#### Documentation (`docs/`)

- § Create-session picker loses its withdrawal paragraph; § What a split session cannot also ask for
  loses two of four rows; § Remote agents gains the seed-at-start case; the semantic index doc states
  that the index is built where the worktree is.

## Implementation Milestones

- [x] M1: Seed resolution goes through the roster; `remote_agent_at_start_unsupported` deleted; an
      unknown reference still fails the start
- [x] M2: Roster written (and clones claimed) before the spawn, on the daemon holding the worktree
      — split placements only; see **Co-located starts** under Technical Debt
- [x] M3: Spawn's subagent env derived from the persisted roster — withdrawal correct at launch
- [x] M4: Unwind on a failed start leaves no clone, no entry, no room membership
- [x] M5: `specialized_agents` refusal deleted from the split start path
- [x] M6: Warm-up moved off the start gate onto clone readiness — reached structurally rather than
      by a move: `resolve_specialized_agent_defs` now skips a peer's reference, so the warm-up it
      feeds only ever dials this host's endpoints and a start can no longer block on a peer's model.
      The prompt-time half was already in place (`refuse_prompt_to_unready_clone`)
- [x] M7: `semantic_index` accepted on a split and built on the codebase host
- [x] M8: Web ungating
- [ ] M9: Feature docs rewritten

## Testing Plan

### Testing Strategy

**Determine Appropriate Test Level:**

**Integration / cross-host acceptance** is the primary level. Every claim in this changeset is about
*which host did something* — which host holds the roster, which host got a clone, which host built the
index. A single-daemon fixture would answer all of them from the wrong place while passing, which is
precisely the failure mode the existing `session_agent_remote_acceptance.rs` header calls out.

Two complementary levels are used deliberately:

1. **Two real daemons in a real common room** (LiveKit testkit, `#[serial]`) for placement outcomes:
   clone or no clone, one clone or two, index on which host, teardown on failure.
2. **Single daemon with a nameable but unconnected peer** for *admissibility*: a call that reaches the
   forwarding path is refused with `FailedPrecondition`, which is an observable only that path
   produces. This is how "the refusal is gone" is asserted without booting a peer — the technique
   `split_session_roster_routing_acceptance.rs` already uses.

Unit level covers reference resolution only (qualified vs bare, unknown name).

### Coverage Requirements

**Acceptance tests MUST cover (at appropriate test level):**

- [ ] **Happy path**: a session seeded with a co-located agent, a peer's agent, and — on a split — an
      agent on the codebase host and an agent on a third host
- [ ] **Error scenarios**: unknown agent reference; a peer unreachable at start; a start that fails
      after a clone was claimed; a prompt to an agent whose clone is still provisioning
- [ ] **Edge cases**: two agents on one remote host (one clone); a seed naming the session's own host
      explicitly; `recipe` / `sandbox` still refused on a split
- [ ] **Integration points**: the roster→spawn env derivation (withdrawal at launch), and the split
      forward carrying `semantic_index`

## Acceptance Tests

Written and run. `✗` = failing for the intended reason (missing production behaviour); `✓ guard` =
passing already, kept so the behaviour that stays cannot regress while the rest changes.

### Daemon — split placement admissibility (`packages/tddy-daemon/tests/remote_managed_worktree_acceptance.rs`)

Placed here rather than in a new `seeded_agent_placement_acceptance.rs`: this suite already holds the
exact fixture the tests need — a nameable-but-unconnected codebase peer, where `InvalidArgument` means
"refused the combination" and `FailedPrecondition` means "accepted it and went looking for the host".
A second binary would have duplicated that ~130-line harness.

- [x] ✗ `a_split_start_seeding_an_agent_of_this_host_reaches_the_codebase_host` (AC1/AC2) — :438
- [x] ✓ guard `a_split_start_seeding_an_agent_no_host_defines_is_refused_naming_that_agent` (AC8) — :465
- [x] ✗ `a_split_start_asking_for_a_semantic_index_reaches_the_codebase_host` (AC9) — :498
- [x] ✓ guard `a_split_start_carrying_a_workflow_recipe_is_still_refused_naming_the_field` (AC10) — :524
- [x] ✓ guard `a_split_start_asking_for_a_sandbox_is_still_refused_naming_the_field` (AC10) — :555
- [x] **Deleted**: `start_session_seeding_an_agent_on_a_split_placement_is_refused` — it asserted the
      refusal this changeset removes.

### Daemon — seed resolution (`packages/tddy-daemon/src/connection_service.rs`, in-file unit tests)

`mod seeded_roster_records_unit_tests`. These define the API the seed needs:
`seeded_roster_records(&[String]) -> Result<Vec<SessionAgentRecord>, Status>` — records, not defs,
because a def can only describe an agent on one host. All six fail to compile (the method does not
exist), which is the intended red for a new API.

- [x] ✗ `an_empty_seed_resolves_to_an_empty_roster`
- [x] ✗ `a_bare_seed_reference_resolves_to_this_daemons_own_agent`
- [x] ✗ `a_seeded_record_carries_the_tools_its_def_takes_from_the_main_agent` (AC5's input)
- [x] ✗ `a_seed_reference_that_resolves_to_no_def_is_a_request_error_naming_it` (AC8)
- [x] ✗ `a_seed_reference_naming_a_daemon_this_host_cannot_see_is_a_request_error_naming_it` (AC1)
- [x] ✗ `a_seed_reference_naming_two_daemons_is_a_request_error_naming_the_field`

### Web app (`packages/tddy-web/cypress/component/CreateSessionCodebaseHostAcceptance.cy.tsx`)

20 tests in the spec, 16 passing (the guards) and 4 failing:

- [x] ✗ `keeps the semantic index on offer once the codebase is placed on another host` (AC11) — :353
- [x] ✗ `sends the chosen semantic index for a split session, and no sandbox` (AC11/AC10) — :368
- [x] ✗ `keeps the specialized-agent picker once the codebase is placed on another host` (AC11) — :401
- [x] ✗ `sends the chosen specialized agents for a split session` (AC11) — :416
- [x] ✓ guard `stops offering sandbox once the codebase is placed on another host` (AC10) — :338
- [x] ✓ guard `stops offering a workflow recipe once the codebase is placed on another host` (AC10)
- [x] **Deleted**: `restores the specialized-agent picker when the codebase comes back to the session
      host` — vacuous once the picker is never withdrawn.

### Not written, and why

- **Two real daemons seeding at start** (`session_agent_remote_acceptance.rs`, AC1-AC4/AC7) — not
  writable at the RPC boundary: `StartSession` for `claude-cli` spawns a real agent process, and that
  suite's `Fleet` writes session metadata directly rather than calling `StartSession`. The placement
  outcomes themselves are already pinned there through the **attach** path — `serves_two_agents_of_one_daemon_from_a_single_clone`
  (AC4), `gives_each_owning_daemon_its_own_clone`, `reads_a_remote_agents_files_from_its_own_clone`,
  `refuses_a_prompt_while_the_clone_is_still_being_built` (AC6),
  `leaves_nothing_behind_when_the_owning_daemon_cannot_be_reached` (AC7) — and the seed reuses that
  path verbatim (`roster_record_for` + `claim_agent_clone`). What the seed adds on top is reference
  resolution, which the units above cover.
- **AC5 at launch** — already pinned by `launches_a_resumed_session_without_the_tools_its_roster_replaced`
  and `launches_a_split_session_without_the_tools_its_roster_replaced`: both build the allowlist from
  the *persisted* roster, which is exactly what the seed writes. A third test over the same pure
  functions would duplicate them. The uncovered half is `runner.rs:1484` deriving the replaced set from
  `TDDY_SUBAGENTS_JSON` instead of the roster — a green-phase item (M3) needing its own test in the
  sandbox-runner suite.
- **AC9's index placement** (`semantic_index_wiring.rs`) — needs the split forward to actually run.
  Green-phase plan: extract the workspace `StartSessionRequest` construction out of
  `start_split_claude_cli_session` into a pure builder, and unit-test that it carries `semantic_index`
  and the qualified seed. Recorded as a milestone rather than asserted here.

## Technical Debt & Production Readiness

- [ ] `SemanticSearch` cannot answer a query on any session shape — `tool_semantic_search`
      (`packages/tddy-tool-engine/src/lib.rs:688`) returns `"index query not yet wired"` because the
      query-side embedder is not wired into the tool engine. AC9 therefore asserts *where the index is
      built*, not that a search returns hits. Pre-existing; unchanged by this changeset.
- [ ] **Co-located starts still refuse a peer-owned seed** (`Status::unimplemented`,
      `connection_service.rs:3608`, `co_located_seeded_roster_records`). A co-located start writes its
      `.session.yaml` *after* the spawn (it needs the pid), and `provision_agent_clone` reads that file,
      so a clone claimed pre-spawn would race the metadata write. The status is the one master already
      returned for this combination, so it is not a regression — but **AC1 holds for split placements
      only**. Refused rather than dropped, and never substituted with a same-named local def. Lifting it
      means reordering the metadata write, which is a separate change.
- [ ] **A split session's `SemanticSearch` cannot reach the index this now builds**
      (`connection_service.rs:3642`): the daemon's `tool_engine::execute_tool` call sites pass no env
      pairs, so `TDDY_SEMANTIC_INDEX_DB` never reaches the exec-tool surface. Moot while the query side
      is unwired (next item), but it is the second half of the same gap.
- [ ] `SHELL` from a remote agent still executes on the host holding the authoritative worktree, not
      on the agent's own host. Write-back remains a non-goal (`docs/dev/TODO.md`).
- [ ] `recipe` and `sandbox` remain refused on a split placement — deliberately out of scope, and
      worth revisiting once a recipe's host-side `transition` can be resolved remotely.

## Implementation Status

Verified independently of the implementers' reports, on 2026-08-23.

**Production files changed (2)**

| File | Change |
|---|---|
| `packages/tddy-daemon/src/connection_service.rs` | +791 / −123 — `seeded_roster_records`, `seed_session_agent_roster`, `unwind_seeded_roster`, `index_workspace_worktree`, pure `workspace_start_request`; `remote_agent_at_start_unsupported` and `started_roster` deleted; the `semantic_index` and `specialized_agents` refusals removed from `start_split_claude_cli_session` |
| `packages/tddy-web/src/components/sessions/CreateSessionPane.tsx` | agent picker and Semantic index no longer gated on `isSplitCodebase`, and no longer blanked at submit |

`packages/tddy-web/src/buildId.ts` carries only a regenerated build timestamp.

**Test results**

| Suite | Result |
|---|---|
| `cargo test -p tddy-daemon --lib` | 543 passed / 0 failed (includes the 6 seed-resolution units and 9 new `workspace_start_request` units) |
| `--test remote_managed_worktree_acceptance` | 21 passed / 0 failed (was 19/2) |
| `CreateSessionCodebaseHostAcceptance.cy.tsx` | 20 passed / 0 failed (was 16/4) |
| 4 adjacent create-session specs | 17 passed / 0 failed |
| `cargo clippy -p tddy-daemon --all-targets -- -D warnings` | clean |
| whole package, `--no-fail-fast` | 1564 passed / 5 failed — every failure panics in `LiveKitTestkit::start()`, the pre-existing Docker-container class; the failing *set* varies run to run and none touches a changed path |

No test file was modified during implementation: both test diffs are the red phase's, assertions and
fixtures unchanged.

**Not yet reflected on this host.** `/usr/local/bin/tddy-daemon` and `tddy-tools` were installed at
18:00 from `d461774d`, so a session started now still meets the old gate — a split session started at
18:08 logged `spawn seed carries 0 specialized agent def(s)` and `roster rev 0 applied`, and tddy-tools
correctly withheld the subagent conversation tools. `sudo ./install --systemd --build` is what makes
the change observable.

## Decisions & Trade-offs

- **Seed through the roster rather than keeping the env-baked path for local agents.** Two mechanisms
  would mean two answers to "what has this session withdrawn", and the spawn would keep deriving
  withdrawal from the request instead of from the record an attach later mutates. One record, one
  derivation.
- **Roster written before the spawn, not after.** `--allowedTools` is fixed at launch, so a roster
  written afterwards would leave a seeded agent's `replaces` unenforced until the first resume — the
  silent-success shape the refusals were originally protecting against.
- **Clone readiness gates prompts, not session start** (chosen by the developer). Start never blocks
  on a peer's model endpoint; a provisioning clone is already a state the roster reports and a prompt
  already refuses (AC33). The trade-off is that a session can come up with an agent that is not yet
  answerable, which the Agent roster pane shows.
- **`semantic_index` ungated on a split; `recipe` and `sandbox` not.** The index targets a worktree,
  which exists — on the codebase host. A recipe's tooling runs the agent host's own `transition`, and
  the sandboxed spawn jails the agent host's filesystem; neither is a read that can be served
  elsewhere.
- **No proto change.** `specialized_agents` already carries qualified ids, `semantic_index` already
  crosses the wire, and the split forward already carries the `agent_daemon_instance_id` back-pointer
  the withdrawal refusal reads.

## TODO

- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [ ] USER REVIEW — acceptance tests
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [~] Run all tests — daemon lib 543/543, target acceptance suite 21/21, web 20/20 + 17/17 adjacent;
      5 pre-existing LiveKit-container failures remain (see Test Results)
- [ ] Validate changes
- [ ] USER REVIEW — development complete
- [ ] Linting and type checking
- [ ] Wrap documentation
- [ ] USER REVIEW — work complete, decide next steps

## References

- Related changesets: [2026-08-15 session worktree sync](2026-08-15-session-worktree-sync.md)
- Refusals removed: `packages/tddy-daemon/src/connection_service.rs:2453`, `:7345`, `:7370`
- Room opened by the roster holder: `packages/tddy-daemon/src/connection_service.rs:3383`
- Roster routing that already means "the codebase host": `packages/tddy-daemon/src/connection_service.rs:8869`
- Spawn-time replaced-tools source: `packages/tddy-sandbox-runner/src/runner.rs:1484`
