# Changeset: split `connection_service.rs`

**Date**: 2026-09-09
**Status**: 🚧 In Progress
**Type**: Refactor

## Initial Discovery

Full codebase exploration that grounded this plan:
[2026-09-09-connection-service-split-initial-discovery.md](./2026-09-09-connection-service-split-initial-discovery.md).

State A below is distilled from that file. Do not duplicate grep traces or item dumps here.

## Affected Packages

- **tddy-daemon**: `packages/tddy-daemon/src/connection_service.rs` — the module a reader of the
  daemon docs is sent to for RPC handling becomes a facade over a `connection_service/` directory.
  - No dev-doc change expected: `reexport: "glob"` keeps every documented entry point resolving.
    Confirmed against the tree at wrap (see Final Checklist).

## Related Feature Documentation

None — behaviour-preserving restructure. No PRD.

## Summary

`connection_service.rs` (24,858 lines) becomes a thin facade over ~75 modules under
`packages/tddy-daemon/src/connection_service/`, grouped by what they serve: stream adapters, proto
message converters, seeding and stack-parent types, spawn helpers, framing, PR status, and one file
per test module.

Everything stays reachable at its current path. All 17 symbols the other 93 files reach are covered by
`reexport: "glob"` on each extracted module, so **no file outside the target is in the diff**.

Every resulting file lands under 500 lines except one: `impl ConnectionServiceTrait for
ConnectionServiceImpl`, which floors at ~680 lines and is a named refusal below.

## Background

24,858 lines in one file, 20× the next-largest hand-written source file in the workspace and 2.5×
the largest generated one. Four `impl` blocks account for 53% of it, two single methods exceed 500
lines on their own (`start_session_core` 830, `start_sandboxed_claude_cli_session` 609), and 29 inline
test modules account for another 25%.

The seam that already exists is the one the file's own layout admits: free converters and stream
adapters at the top, impl blocks in the middle, tests at the bottom. Discovery §3 established the two
constraints that shape everything: an `impl` block moves whole or not at all, and the trait impl has an
arithmetic floor.

## Scope

- [~] **Plan**: layout proposed, seam order fixed, snapshot taken
- [~] **Apply**: plan 1 of 6 applied (29/29 operations), moved-line diff clean
- [ ] **Baseline**: build and `-p tddy-daemon` suite back to the recorded numbers
- [ ] **Code Quality**: `cargo clippy -p tddy-daemon -- -D warnings` clean, `cargo fmt` clean
- [ ] **Documentation**: Final Checklist executed at wrap

**Status indicators**: `[ ]` not started · `[~]` in progress · `[x]` complete ✅

## Technical Changes

### State A

| File | Lines | Items |
|---|---:|---|
| `packages/tddy-daemon/src/connection_service.rs` | 24,858 | 197 top-level; 5 impl blocks (7,130 + 6,483); 29 test modules |

### State B

| Module | Contents | Projected lines |
|---|---|---:|
| `connection_service.rs` (facade) | `use` header, `mod` declarations, `pub use …::*` | ~250 |
| `connection_service/util.rs` | `spawn_blocking_with_timeout`, `await_supervised_with_timeout`, `push_new_branch_to_origin_if_requested`, terminal chunking, `resume_agent_and_recipe` | ~180 |
| `connection_service/streams/terminal.rs` | `TerminalOutputStream`, `MpscTerminalOutputStream`, `TerminalFrameIdentity`, input/output converters | ~165 |
| `connection_service/streams/acp_activity.rs` | `MpscResultStream`, control-event / activity / notification / ACP-replay streams and relays | ~345 |
| `connection_service/streams/host_worktree.rs` | poll intervals, LiveKit-rooms / host-stats / host-prompt / worktree-stats streams | ~135 |
| `connection_service/host_messages.rs` | proto converters for probe outcome, git identity, ssh agent, github cli, remote desktop; `unlock_and_add`; `now_unix_ms` | ~250 |
| `connection_service/activity_hub.rs` | `AgentActivityHub` + impl, `relay_agent_activity`, `DemoVmHandle` | ~115 |
| `connection_service/state.rs` | `ConnectionServiceImpl` struct, resolver type aliases | ~150 |
| `connection_service/stack_parent.rs` | `StackBaseLookup`, `StackNodeLink`, `StackParentHost`, `SpawnStackParent` + impls | ~240 |
| `connection_service/seed_clones.rs` | `SeedCodebase`, `SeededAgentClones`, `SeededAgent`, `SeededCloneGuard` + `Drop`, `AgentConversation`, `PromptRouting`, `ExecToolRoute` | ~255 |
| `connection_service/hooks_and_urls.rs` | hook settings, daemon URL resolution, binary resolution, `effective_spawn_branch`, `validate_repoint_target`, `project_entry_from` | ~300 |
| `connection_service/roster.rs` | `DaemonSeedCloneClaimant`, `subagent_info`, `roster_record`, agent-id helpers, `workspace_start_request`, withdrawal refusals | ~320 |
| `connection_service/claude_cli_spawn.rs` | `spawn_claude_cli_session_inner` (post-`extract_method`) | ~400 |
| `connection_service/managed_workflow.rs` | `ManagedLaunch`, `prepare_managed_workflow_inner`, `StackChildSpawnHandler` + impl | ~225 |
| `connection_service/conversation_spawn.rs` | `recipe_enables_conversation_spawn`, `conversation_branch_slug`, `GrillMeConversationSpawnHandler` + impl | ~130 |
| `connection_service/attachments.rs` | `roster_replacement_pairs`, attachment progress sink/reporter/materialization | ~125 |
| `connection_service/placement.rs` | `CodebasePlacement`, `classify_codebase_placement`, `SplitStartFailure`, split placement resolution | ~175 |
| `connection_service/peers.rs` | `merge_listed_projects_with_peers`, `DaemonRpcHandler` + `HostRpcHandler` impl | ~165 |
| `connection_service/framing.rs` | frame-size consts and asserts, exec-tool / conversation / document / worktree-file / activity-delta framing, path-traversal refusal | ~275 |
| `connection_service/pr_status.rs` | orchestrator requirement, stack-seed validation, base-sync views, worktree legs, error mapping | ~230 |
| `connection_service/agent_models.rs` | models cache, probe args, JSON parsing | ~65 |
| `connection_service/impl_state.rs` … `impl_*.rs` (~18 files) | the 4 inherent `impl ConnectionServiceImpl` blocks, cut into cohesive blocks by the prep commit | ~250–450 each |
| `connection_service/rpc_service_impl.rs` | `impl ConnectionServiceTrait for ConnectionServiceImpl` — 111 delegating members | **~680 ⚠ over budget** |
| `connection_service/handlers/*.rs` (~16 files) | the 90 trait-method bodies, grouped by RPC family | ~250–400 each |
| `connection_service/<29 test modules>.rs` | one file per inline `mod …tests`, the two largest split into nested submodules | 24–450 each |

### Delta — seams in dependency order

Definitions before their users; line order only breaks ties. Each numbered plan is applied, verified
and re-snapshotted before the next is written.

0. **Tooling.** Four defects on the transport `tddy-tools restructure` actually uses made every
   operation in this program impossible; they are fixed first and are the subject of their own
   changelog entry. See *Tooling* below.
1. **`plan-1-tests.jsonl`** — 29 × `extract_module_to_file` over the inline test modules. Leaves first:
   they reference everything and nothing references them. −~6,300 lines from the parent. Pure tool.
2. **`plan-2-test-split.jsonl`** — `extract_module` + `to_file` inside `host_add_key_handler_tests`
   (1,266) and `agent_activity_unit_tests` (1,236), 3 nested submodules each. A `mod` body may hold a
   `mod`, which is why this works where the impl equivalent does not. Pure tool.
3. **`plan-3-extract-method.jsonl`** — `extract_method` over the 8 oversized members, so no member
   exceeds ~150 lines: `start_session_core` (830), `start_sandboxed_claude_cli_session` (609),
   `start_sandboxed_cursor_cli_session` (460), `spawn_claude_cli_session_inner` (377),
   `relaunch_sandboxed_runner` (279), `spawn_split_agent` (228),
   `split_context_from_codebase_host` (213), `ensure_project_available_for_start` (168). Must precede
   any block split — a method cannot be split across files. Pure tool.
4. **`plan-4-trait-bodies.jsonl`** — `extract_method` over each of the 90 trait-method bodies. rust-analyzer
   writes the delegation itself and collects the bodies into a new inherent `impl` block
   (discovery §3.4), taking `rpc_service_impl.rs` to its ~680-line floor. Pure tool — this is the step
   the first draft of this changeset had as a hand-written commit.
5. **`plan-5-free-items.jsonl`** — `extract_module` with `to_file: true` and `reexport: "glob"` over
   the 16 free-item seams above, in definitions-first order. −~5,000 lines. Pure tool.
6. **prep commit** — hand-inserted `impl` block boundaries only: `}` + `impl ConnectionServiceImpl {`
   pairs cutting the large inherent impls — the three original ones and the block step 4 accumulates —
   into ~30 cohesive blocks. No body is touched; verified as a zero-moved-line diff. **The only
   hand-written lines in the restructure.**
7. **`plan-6-impl-blocks.jsonl`** — `extract_module` + `to_file` over each `impl` block. Moving a whole
   `impl` is free of caller churn. Pure tool.

## Tooling

This restructure was blocked outright until four defects in `tddy-tools restructure` were fixed. All
four sat on the **bridge** transport — the one the CLI uses, where the backend talks through a shared
`tddy-lsp` client instead of spawning rust-analyzer itself — and every one of them presented as the
same false message: *"rust-analyzer offers no \"extract into function\" assist for the given range"*.

| | Defect | Fix |
|---|---|---|
| D1 | `tddy-lsp`'s response dispatch read `result` and defaulted to null, discarding any JSON-RPC `error`, so every server error arrived as a successful empty answer — `ContentModified` included, which left `request_settled`'s retry loop dead on this path | `LspError::Server { code, message }`; the pending request is handed the error |
| D2 | `map_lsp_error` classified every failure as `MalformedPlan`, timeouts included — reporting a slow index as a defective plan, and skipping the only classification (`ServerCatchingUp`) the retry loops act on | `ContentModified` and `Timeout` map to `ServerCatchingUp`; other server errors keep their code and message |
| D3 | Every request was capped at a hardcoded 10s that `--indexing-budget` could not reach, so the documented remedy for a slow machine was inert | `LspClient::set_request_timeout`, driven from `--indexing-budget` |
| D4 | The bridge's client was initialized with `"capabilities": {}` — `client_capabilities()` is only sent on the self-spawned path, which `start()` skips when a bridge is present | `LaunchSpec::with_capabilities` / `with_initialization_options`, threaded to `initialize`; the CLI supplies the backend's own |

**D4 was the blocker and the most serious.** rust-analyzer returns *no code actions at all* to a
client that advertised no `codeAction` support. It also left the server on the LSP default of utf-16
positions while this client counts utf-8 bytes — so every column was silently wrong on any line
carrying a non-ASCII character, and this file's comments are full of em dashes. The self-spawned path
*refuses* that mismatch; the bridge never looked. The fix carries the refusal and the pinned import
granularity onto the bridge path as well, so the two transports now negotiate the same handshake.

A fifth change makes the failure legible rather than merely correct: an expired budget reports
`IndexingIncomplete` when the server never answered and an absent assist only when it did, and
**names the assist titles it was offered** — the evidence that separates a wrong title from an
unrefactorable range, which was previously not obtainable at all.

Tests: 8 added (2 unit + 3 acceptance in `tddy-lsp`, 3 unit + 2 unit in `tddy-code-restructuring`).
`tddy-lsp` 26/26, `tddy-code-restructuring` 218/218, clippy `-D warnings` clean across every
`tddy-lsp` consumer.

## Callers

What the restructure rewrites outside the target:

| | |
|---|---|
| External importers | 93 files — 12 in `packages/tddy-daemon/src/`, 81 in `packages/tddy-daemon/tests/`. All package-internal; no other crate reaches this module. |
| Symbols reached | 17. `ConnectionServiceImpl` alone accounts for 62 of the 93 references; `AgentActivityHub` 9; `now_unix_ms` and `HOST_DOCUMENT_FRAME_BYTES` 3 each; 13 symbols once each. |
| Facade | `reexport: "glob"` on every `extract_module` seam. A parent `foo.rs` may own a `foo/` directory in Rust 2018, so the facade survives `extract_module_to_file` with no rename to `mod.rs`. |
| Consequence | **No caller changes.** No file outside `connection_service.rs` and the new `connection_service/` directory appears in the diff. |

The facade is permanent, not scaffolding: `crate::connection_service::ConnectionServiceImpl` is the
daemon's RPC entry point and 62 call sites is the wrong number of edits to buy a shorter path.

## Applied so far

### Plan 1 — 29 test modules to their own files ✅

`applied 29 of 29 operations`, every one `ExtractModuleToFile -> 3 file(s)`.

| | Before | After |
|---|---:|---:|
| `connection_service.rs` | 24,858 | **18,300** |
| `connection_service/*.rs` | — | 29 files, 6,529 lines |
| Files over 500 lines among the new ones | — | 2 (`host_add_key_handler_tests` 1,264, `agent_activity_unit_tests` 1,234 — plan 2's targets) |

**Moved-line diff** — normalising whitespace and `pub(crate)` away, set-compared against the
pre-restructure ref, scoped to `connection_service.rs` + `connection_service/`:

| | Count | What |
|---|---:|---|
| Lost | 29 | `mod X {` — the inline declaration |
| Lost | 29 | `}` — their closing braces |
| Gained | 29 | `mod X;` — the file declaration |
| **Anything else** | **0** | — |

Distinct non-blank lines identical on both sides (12,249 → 12,249); 23,099 → 23,070 total, the
difference being exactly the 29 closing braces a file-level module does not need. Nothing else in
the file changed.

`cargo check -p tddy-daemon --all-targets`: clean, **and no unused-import warnings** — the
prediction in *Decisions* that every seam would strand imports in the parent does not hold for this
operation. `extract_module_to_file` moves a whole module and each test module reaches the code under
test through `use super::*`, which still resolves from a child file, so the parent's `use` block is
still fully used. The prediction stands for plan 5, where items *do* leave the parent's scope.

## Visibility

Filled in from what each run reports. Two widenings are expected by construction:

| Item | Was | Is | Why it had to widen | Doc comment fixed |
|---|---|---|---|---|
| _none — plan 1_ | | | **Zero widenings.** `extract_module_to_file` relocates a whole module, so nothing crossed a visibility boundary and the run reported no changes | n/a |
| _(relocated `impl` members, plans 4 and 7)_ | private | `pub(crate)` | The visibility pass does not descend into an `impl`, so a relocated member stays widened by design (`plan-schema.md`) | |

The moved-line diff is run alongside this table, never instead of it: normalise `pub(crate)` and
whitespace away, set-compare every moved line against the pre-restructure ref, and state how many
lines differ and why each one does.

## Baseline

Recorded before any change (step 1), and the acceptance criterion for step 7:

| Gate | Before | After |
|---|---|---|
| `cargo check -p tddy-daemon` | ✅ | |
| `./test -p tddy-daemon` | **1027 passed / 1 failed** (25 suites) | |
| `cargo clippy -p tddy-daemon -- -D warnings` | not yet recorded | |

**The one pre-existing failure**, recorded before any change so it cannot later read as a regression:

```
cursor_cli_session_acceptance::cursor_cli_sandbox_start_succeeds_when_sandbox_backend_available
  panicked at connection_service.rs:2176: ConnectionServiceImpl::self_arc called before set_self_handle
```

Same root cause as the five `sandbox_behavior_acceptance` failures already known to fail on master —
a missing `set_self_handle` in the test's own construction of `ConnectionServiceImpl`, not a
Seatbelt or sandbox regression. `self_arc` is in `IMPL-A`, which this restructure moves but does not
change; the failure is expected to survive unchanged and stay at exactly one.

Pre-existing failures are named here before any plan runs, so they cannot later read as regressions.
Per `tddy-coder-scoped-verification`, verification is scoped to `tddy-daemon`; the full workspace has
pre-existing noise unrelated to this change.

## Plan

`docs/dev/1-WIP/connection-service-split/plan-*.jsonl` — 6 plans, applied in the order above. Each
plan's header line carries the snapshot digest of the tree it was written against; a plan is written
only after its predecessor has been applied and verified.

## Decisions & Trade-offs

- **`impl ConnectionServiceTrait` cannot go under 500 lines, and this is arithmetic rather than a
  tooling limit.** 111 members (90 `async fn` + 21 `type …Stream`) occupy 385 lines of signatures
  alone; with every body reduced to one delegating line the block floors at ~680. Going lower means
  splitting `ConnectionService` into several gRPC services in the `.proto` — a protocol change, out of
  scope for a behaviour-preserving restructure. **Accepted as the one file over budget.**

- **`extract_method` *can* perform the trait-impl delegation, and the first draft of this document
  said it could not.** Asked to extract a body out of a trait impl, rust-analyzer writes a new
  inherent `impl` block after it and puts the function there, rewriting the original body into a
  `self.extracted(…)` call it composes itself — no `E0407`, no refusal (discovery §3.4). The earlier
  conclusion came from a probe that reported *"rust-analyzer offers no assist for the given range"*,
  which was one of the tooling defects below rather than an answer about Rust. **90 hand-written
  delegations are removed from the plan as a result**, and the only hand-written lines left in the
  whole restructure are `impl` block boundaries.

- **Successive extractions accumulate in one `impl` block, so step 4 does not remove the need for
  step 6.** Two extractions in one plan produced one new block holding both functions. The 90 bodies
  therefore land in a single ~5,800-line inherent impl that still has to be cut by hand-inserted
  boundaries — the block is movable for free, but not divisible.

- **There is no operation that splits an `impl` block.** `extract_module` is the only Rust splitting
  operation and an `impl` body cannot hold a `mod`; lifting a single member out is refused whenever a
  sibling in the same `impl` calls it, which is true throughout these blocks. Step 5 inserts the
  boundaries by hand — ~36 lines of `}` and `impl ConnectionServiceImpl {`, no body touched — so that
  step 6 can move each resulting block with the free whole-`impl` move.

- **Unused imports in the parent are a consequence of every seam.** `extract_module` restores imports
  in the module it writes but nothing prunes the parent's now-unused `use` declarations, and the
  file's header is 235 lines of them. `cargo clippy -- -D warnings` will fail until they are pruned;
  the pruning is deletion, not authoring, and is done per plan rather than at the end.

- **~75 files is the cost of a 500-line ceiling on a 24,858-line file**, and 50 is the arithmetic
  minimum. The grouping is by what a module serves rather than by size, so several files land well
  under the ceiling; splitting them further to even out line counts would trade cohesion for a
  number.

## Final Checklist

Tasks executed by `/wrap-context-docs` before this changeset is deleted. Derived at plan time and
re-triaged in step 8 against what actually landed.

- [ ] `packages/tddy-daemon/docs/changesets/2026-09-09-connection-service-split.md` — write the
      release-note file, carrying before/after line counts and the baseline numbers behind the
      no-behaviour-change claim
- [ ] Re-run the doc triage over every path and symbol the plans touched:
      `grep -rn -e 'connection_service' packages/tddy-daemon/README.md packages/tddy-daemon/docs docs/ft/daemon`
      — a hit is an item only where the restructure made it false; a glob facade leaves most examples
      running exactly as written
