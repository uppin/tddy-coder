# 2026-10-05 — The agent topic runs over an owned `AgentRoster` handle and an `AgentHostCallbacks` port

**Type:** Architecture

Every agent-clone and roster method of `DaemonSessionHost` (topic 3 in [module-layout.md](../module-layout.md))
is `impl AgentRoster`, an owned handle over the host's roster fields, in place. The topic's modules name
neither `DaemonSessionHost` nor a wiring module, so the topic can move into `tddy-session-agents` as a
plain module move. No behaviour change, no crate move, no public-surface change: no consumer crate
(`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`, `tddy-desktop`) was edited and no crate edge was
added. This is the second in-place conversion of the `#carve` stack, after the leaf topics
([`2026-10-04-carve-lifecycle-ports-leaf-topics`](./2026-10-04-carve-lifecycle-ports-leaf-topics.md)).

## State B

- `AgentRoster` (`connection_service/agent_host_callbacks.rs`) holds the host's twelve roster fields under
  the host's names, plus `host: Arc<dyn AgentHostCallbacks>`. It is `Clone`; `state()` lends the fields as
  `tddy_session_agents::AgentRosterState`. The host builds it per call (`agent_roster()`).
- 37 methods are `impl AgentRoster`: `svc_provision_agent_clone.rs` 12, `svc_start_hosted_agent_clone.rs` 9,
  `svc_ensure_session_room_for_agents.rs` 6, `svc_resolve_listed_worktree.rs` 6, `svc_turn_end_reporter.rs` 4.
  Moving a method changed its `impl` header and its receiver paths, nothing else; the five `self.clone()`
  hand-offs clone the handle.
- `AgentHostCallbacks` has five methods: `worktree_snapshot`, `run_exec_tool_locally`, `ensure_session_room`,
  `hosted_clone_for`, `run_hosted_clone_tool`. It is defined once and implemented once, on the host, in
  `svc_agent_host_ports.rs`.
- Wiring left on the host: `ensure_session_room` (`svc_agent_host_ports/session_room_opening.rs`, the
  terminal-bridge wiring), `DaemonSeedCloneClaimant` (holds the handle) and `impl RemoteSnapshotSource`
  (`svc_agent_roster_wiring.rs`), seven delegators (`svc_agent_roster_delegators.rs`), `agent_roster()` and
  `seed_clone_claimant()`. The five session-agent port adapters hold the handle.
- Modules in their final places: `first_admission_token`, `session_dir_lookup` (direct children of
  `connection_service`), `session_room_opening` (under `svc_agent_host_ports`), `split_forward_deadline`
  (in `agent_roster.rs`) and `resolve_worktree_root_in_session_dir` (in `peer_session_answer.rs`).
- Every converted file names foundations by their defining crate, not through a lifecycle facade.

## Decisions

- **D1, owned handle:** methods on a per-topic owned handle with the host's field names, built per call.
  Free functions over a borrowed state would have added `state` and `host` parameters to every function
  and made two code issues worse; the handle keeps parameter counts and every `self.clone()` textually
  identical. Cost: each `agent_roster()` clones `DaemonConfig` and the host once.
- **D2, five callbacks.** The trait began as `worktree_snapshot`, `run_exec_tool_locally`,
  `local_exec_tools`. `ensure_session_room` joined it because the host's room opening needs the host's
  terminal bridge (a CLI capability), which a handle method cannot reach. `local_exec_tools()` was then
  replaced by the two calls the topic makes on it, `hosted_clone_for` and `run_hosted_clone_tool`, because
  its return type was lifecycle's `LocalExecTools` and the trait could not name it and still move.
- **D3:** `DaemonSeedCloneClaimant` holds the handle, so the launch topic needs no seed-claimant callback.
- **D7:** `LocalExecTools` stays in lifecycle, reached only inside the host's impl of the trait.

## Before and after

| | Before | After |
|---|---|---|
| `tddy-session-lifecycle` suite (`--no-fail-fast`, `--test-threads=1`, macOS) | 575 passed, 22 failed, 1 ignored | 575 passed, 22 failed, 1 ignored; the failing set is identical by name |
| `tddy-session-agents` | not re-measured on the parent | 75 passed (the node added no test) |
| `AgentRosterState` fields | 10 | 12 |
| CI at the final head | | Rust 8,473 passed, Rust e2e 380 passed, web 2,800 passed |

The 22 failures are the macOS-only sandbox and LiveKit suites listed in
[test-suites.md](../test-suites.md); Linux CI runs them green. `cargo check --all-targets`, clippy
`-D warnings` and `fmt --check` are clean on lifecycle and `tddy-session-agents`; `check --all-targets` is
clean on `tddy-daemon-rpc`, `tddy-daemon` and `tddy-telegram-control`. `tddy-desktop` is CI's.

## Acceptance

| # | Criterion | Result |
|---|---|---|
| A1 | no converted file names `DaemonSessionHost` | pass over the converted set **excluding wiring files** (`svc_*_ports.rs` and what sits under them) and the other topics' ranges in the two mixed files; the wiring files name it by design |
| A2 | no converted file names a wiring module | pass |
| A3 | no upward topic edge | pass by scripted grep; `cargo modules` was not run |
| A4 | foundations named by defining crate | pass |
| A5 | no host clone in a converted file | pass over the same set as A1; the host's own `ensure_session_room`, in wiring, still clones the host |
| A6 | trait defined once, implemented once, in wiring, five methods | pass |
| A7 | no consumer edit, every facade resolves | pass |
| A8 | baseline held | pass (above) |
| A9 | `AgentRosterState` has twelve fields, `tddy-session-agents` changed nowhere else | pass |

`restructure verify`, run stage by stage, accounted for every statement of the engine moves. Over the whole
node it lists the hand edits the engine has no operation for: the `impl` retargets, the receiver re-points
and the callback swap.

## What the engine could not do

Engine moves: four adapter-field renames and the five module moves and re-parents (E1 to E5). By hand:
the 37 `impl` retargets and receiver re-points, 30 lines of import re-pointing and the trait swap. The gaps
are on the backlog as `2026-10-05-restructure-no-operation-retargets-an-impl-to-another-type`,
`2026-10-05-restructure-no-operation-re-points-a-calls-receiver-or-writes-a-delegator`,
`2026-10-05-restructure-cannot-re-point-an-import-through-a-facade-to-its-defining-crate`,
`2026-10-05-restructure-move-item-copies-a-moved-signature-s-facade-path`,
`2026-10-05-restructure-move-item-leaves-intra-doc-links-to-the-old-path`,
`2026-10-05-restructure-move-item-writes-a-caller-re-point-as-a-full-path`,
`2026-10-05-restructure-apply-did-not-return-after-a-clean-deep-check` and
`2026-10-05-restructure-snapshot-cannot-write-a-missing-plan-header`.
`2026-09-24-lifecycle-modules-to-re-parent-by-hand` lost two of its three rows to the moves.

## Code issues

Re-measured by function line to closing brace, `origin/master` against this head. None closed; none
deleted.

| Record | Before | After | Action |
|---|---:|---:|---|
| `complexity-svc-resolve-listed-worktree-ensure-project-available-for-start` | 99 lines | 99 | unchanged; row added |
| `complexity-svc-spawn-split-agent-spawn-split-agent` | 112 lines, 9 parameters | 112, 9 | unchanged (the file lost `split_forward_deadline`); row added |
| `complexity-svc-start-sandboxed-claude-cli-session-start-sandboxed-claude-cli-session` | 342 lines | 343 | +1 (the seed-clone claim goes through `.agent_roster()`); row added |
| `crap-svc-start-sandboxed-cursor-cli-session` | 415 lines | 416 | +1, same cause; still never executed; row added |
| `complexity-svc-start-session-core-start-session-core` | 373 lines | 374 | +1 (rustfmt wraps `.agent_roster().unwind_seeded_roster(`); row added |

The other lifecycle records name files this node did not touch. `tddy-session-agents`'s records name files
this node did not touch (`agent_roster_state.rs` is the only file it changed, and no record names it).

## Open items for the next nodes

- `AgentHostCallbacks::worktree_snapshot` has no caller yet; the split topic's `join_split_livekit_room`
  arrives with `#carve` 18/21, and the `#[allow(dead_code)]` carries a `TODO(#carve 18/21)`.
- The agent topic moves into `tddy-session-agents` in the move node, with the engine only; a refusal there
  means stop and ask.
- `FIXME(session-worktree-sync)` on a moved method still cites `docs/dev/TODO.md`, a file the backlog
  directory replaced.
