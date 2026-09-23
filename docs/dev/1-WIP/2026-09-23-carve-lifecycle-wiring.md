# Changeset: tddy-session-lifecycle becomes a wiring crate

**Date**: 2026-09-23
**Status**: 🚧 In Progress — planned
**Type**: Refactor (restructure, then crate extraction; no behaviour change)
**Stack**: `#carve` 13/13, on top of `core-split` (#522)

## Initial Discovery

[2026-09-23-carve-lifecycle-wiring-initial-discovery.md](./2026-09-23-carve-lifecycle-wiring-initial-discovery.md)
covers the topic breakdown, every file over 500 production lines with its seams, the seven functions
that no move can shrink, the duplication counts and the coupling. State A below is distilled from it.

## Affected Packages

- **`tddy-session-lifecycle`**. Phase 1 restructures it in place. Phase 2 leaves it holding only the
  `DaemonSessionHost` construction, the port impls that delegate, and the `pub use` facades.
- **Receivers in phase 2.** The crates listed under State B, both new and existing.
- **`tddy-worktree-service`**: gains `MpscResultStream::into_receiver`, so the duplicate can go.
- **`tddy-terminal-rpc`**: gains `strip_resize`, which three copies currently repeat.
- **Consumers** (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`): none is edited. They
  resolve through facades, as in #522.

## Related Feature Documentation

None. This is a behaviour-preserving restructure, so there is no PRD.

## Summary

`tddy-session-lifecycle` holds **21,264 production lines** in eleven topics. Eleven of its files are
over 500 production lines, and seven of its functions are 243 to 857 lines long. It carries 16 open
code-issue records.

This PR runs in two phases, and **phase 2 does not start until phase 1 is finished**:

1. **Destructure in place, and close the code issues.**
   - Bring every file in the crate under 500 production lines.
   - Extract-method on every function over 150 lines.
   - Remove the four duplications the discovery measured.
   - Close or narrow all 16 code-issue records.
   - Every move goes through `/code-restructuring` (`tddy-tools restructure`), and the code moves
     only within this crate.
2. **The final split.**
   - Move each topic out to a crate of its own, or into an existing crate below this one.
   - What remains of `tddy-session-lifecycle` is a wiring crate: host construction, port impls that
     delegate, and `pub use` facades.
   - Every public `tddy_session_lifecycle::…` path stays reachable.

## Background

This is the last `#carve` lifecycle node, so it carries the stack's acceptance criterion,
lifecycle ≤ 10k production lines (see memory `carve-size-target-10k`). The developer tightened that
on 2026-09-23 to **a wiring crate only**.

**Why phase 1 comes first.** A crate move cannot cut a 857-line function or a 1,647-line module file,
and it cannot separate the three copies of the sandboxed-launch code. Moving those as they are would
only relocate the debt. The developer decided (2026-09-23) that the >500-line files and the open code
issues are both resolved before the final split.

## Responsibility

- **Phase 1 — destructure (in-crate):**
  - Add the characterisation tests the CRAP record requires before `start_sandboxed_cursor_cli_session`
    is restructured.
  - Split the 11 oversized files along the seams in the discovery. Every file must end under 500
    production lines, with a target of ≤ 400.
  - Extract-method on the seven long functions:
    - `start_session_core` (857)
    - `start_sandboxed_claude_cli_session` (615)
    - `start_sandboxed_cursor_cli_session` (465)
    - `spawn_claude_cli_session_inner` (408)
    - `spawn_cursor_cli_session_inner` (338)
    - `spawn_split_agent` (271)
    - `resume_session_at_session_coordinate` (243)
  - Remove the measured duplications:
    - The sandboxed Claude, Cursor and relaunch launch path becomes `svc_sandboxed_jail_launch`.
    - The tool-spawn backend match becomes `spawn_tddy_coder`, used by both start and resume.
    - The Claude and Cursor spawn helpers go to `cli_spawn/common`, with `CliSpawnRequest`.
    - `MpscResultStream` becomes the `tddy-worktree-service` copy.
    - The stale `activity_delta_frames` is deleted.
    - The three `strip_resize` copies become one in `tddy-terminal-rpc`.
  - Close or narrow each of the 16 `docs/code-issues/` records by re-measuring it.
- **Phase 2 — final split:**
  - Introduce a per-topic state or port struct wherever a topic's code reaches `DaemonSessionHost`'s
    private fields, following #520's `handler_state` pattern.
  - Move each topic to its receiver (State B).
  - Leave a facade for every public `tddy_session_lifecycle::…` path.
  - Move the tests with their code.

## Boundaries

- **No behaviour change.** The recorded test baseline is re-run with zero regressions after every
  seam. Deduplication is extract-method, with the differing parts passed as explicit parameters, not
  a new abstraction such as `SandboxedAgentKind`.
- **No moved or extracted code is written by hand** where `tddy-tools restructure` can do the move.
  Any hand move (`git mv` plus `use` fixes, where the engine refuses) is recorded with the refusal
  that forced it.
- **Phase 2 does not start while any phase 1 item is open.**
- **No consumer crate is edited** except where a consumer reads lifecycle source by path. #522 found
  two such tests. Any case here is listed as an explicit exception.
- **The `PeerRouted*` wrappers stay in this crate** unless the developer approves moving routing down
  behind a forwarding port. The module doc (`svc_session_agent_ports.rs:10-15`) chose routing here on
  purpose.
- **No receiving crate goes over 10k production lines**, and no receiver may depend on
  `tddy-session-lifecycle`.
- **Nothing in `tddy-core` or the nine #522 crates is touched.**

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `12` core-split (#522) | `tddy-core` as facades over nine crates | lifecycle keeps naming `tddy_core::…`, and those paths resolve through #522's facades | touch `tddy-core` or the nine crates, or repoint lifecycle's `tddy_core::` imports |
| `11` rpc-handlers (#520, **merged**) | the four RPC families in `tddy-daemon-rpc`; `handler_state.rs`; `PrStackHandler` in `tddy-pr-stack` | phase 2 follows the `handler_state` pattern for every topic | move anything more into `tddy-daemon-rpc` without checking its 10k budget |

## Draft PR contract

This is a mechanical restructure plus extraction, which is one of the pr-stack skill's two named
exceptions, so there is no stub surface to publish. The draft PR is this plan (commit 1). Commit 2 is
the recorded baseline and the CRAP record's characterisation tests. Nothing is stacked above this
node.

## Green wave

**Wave:** after #522.
**Greenable independently:** **not until #522 is green.**
- The branch inherits #522's three open failures (`git_plumbing_shape` ×2, `session_store_shape` ×1),
  which the developer is still deciding on.
- The baseline must record those as known-red, or be taken after #522's fix lands and this branch is
  rebased.
**Concurrent with:** nothing. #522 is in wrap, and this node restructures a crate #522 does not touch.
**Blocks:** nothing.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| #522 not yet green (3 known failures) | ⛔ **BLOCKING the baseline** | Take the baseline after #522's fix is rebased in, or record the 3 by name as known-red |
| `docs/code-issues/crap-svc-start-sandboxed-cursor-cli-session.md`: "**Restructure: no — tests first**" | ⛔ **BLOCKING** for that function | Characterisation tests land before its seams are cut. Listed in `## Scope` |
| The 10 `complexity-*.md` records (`start_session_core`, `start_sandboxed_claude_cli_session`, `spawn_cursor_cli_session_inner`, `resume_session_at_session_coordinate`, `spawn_split_agent`, `delete_paired_codebase_session`, `start_split_claude_cli_session`, `ensure_project_available_for_start`, `resume_claude_cli_session`, `handle_rpc`) | ✅ **RESOLVED HERE** (phase 1) | Extract-method; re-measure each and delete the record, or narrow it if the fix is partial |
| The 5 `oversized-file-*.md` records (`connection_service`, `session_coordinate_handlers`, `split_session`, `svc_spawn_split_agent`, `svc_start_session_core`) | ✅ **RESOLVED HERE** (phase 1) | Split below 500; re-measure and delete. Their designed seams are the starting point |
| [2026-08-13 … trim-to-option string block](../todo/2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md) | ✅ **RESOLVED HERE** | The trimmed-prompt helper in `cli_spawn/common` collapses the repeated block; `validate_stack_seed_base_session` moves to `stack_parent.rs` |
| [2026-09-19 file-length gate stops at the first `#[cfg(test)] use`](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) | ⚠ **DURING** | This plan's counts use the inline-test-block rule (see the discovery). Measure every seam that way, not with the gate's heuristic. The gate itself is not fixed here |
| [2026-09-19 stack-child-spawn tests flake under concurrency](../todo/2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md) | ⚠ **DURING** | Known baseline noise. Record it in the baseline; a flake there is not a regression |
| [2026-09-09 untested complexity hotspots](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **DURING** | `connection_service.rs` and `session_agent_clone.rs` are on it. Extract-method on untested code is the risk the CRAP rule exists for; re-measure per owning crate at wrap |
| [2026-07-01 tddy-daemon](../todo/2026-07-01-tddy-daemon.md) (stdio transport for sandboxed sessions) | — unrelated | The sandboxed launch path moves but its transport does not change |

## Scope

**Phase 1 — destructure and close the code issues**
- [ ] ⛔ Baseline recorded (`./test -p tddy-session-lifecycle`, with counts and known-red names)
- [ ] ⛔ Characterisation tests for `start_sandboxed_cursor_cli_session`
- [ ] Warm index started (`./run-index-daemon`); every plan passes `restructure check --deep` before `apply`
- [ ] `MpscResultStream` deduplicated; `activity_delta_frames` removed
- [ ] `connection_service.rs` split (placement, split_start, attachment_progress, managed_launch, stack_parent, spawn handlers, agent_roster)
- [ ] `spawn_claude_cli_session_inner` → `cli_spawn/claude.rs`; `cursor_cli_spawn` → `cli_spawn/cursor.rs` + `chat.rs` + `resume.rs`
- [ ] `cli_spawn/common.rs` + `CliSpawnRequest` (the only behaviour-sensitive step in the CLI group)
- [ ] `cli_session_manager` → directory module (7 files); `strip_resize` → `tddy-terminal-rpc`
- [ ] `split_session` → `agent_argv.rs` + `agent_credentials.rs`
- [ ] `start_session_core` extract-method; `spawn_tddy_coder(ToolSpawnPlan)` shared with resume
- [ ] `session_coordinate_handlers` split; `session_entry_from_listing`
- [ ] `svc_sandboxed_jail_launch` for Claude, then Cursor, then relaunch (relaunch only after it is covered)
- [ ] Ports files: adapters and the `PeerRouted*` wrappers into their own files; routed methods pass `req`
- [ ] `svc_spawn_split_agent` teardown split + extract-method; `svc_resolve_tddy_tools_path` builders → `svc_host_builders.rs`
- [ ] Every non-test file < 500 production lines; no function > 150 lines
- [ ] All 16 code-issue records re-measured and deleted or narrowed

**Phase 2 — final split** (starts only when every phase 1 box is ticked)
- [ ] Per-topic state/port structs (the `handler_state` pattern)
- [ ] Each topic moved to its receiver (State B); receivers ≤ 10k, none depends on lifecycle
- [ ] `tddy-session-lifecycle` is wiring only; facades cover every public path; consumers unedited
- [ ] Tests moved with their code; baseline back to the recorded numbers

## Technical changes

### State A

21,264 production lines, 7,570 in-`src/` test lines, 59 integration suites. There are 11 files ≥ 500
and 7 functions of 243–857 lines. Topic sizes and per-file seams are in the discovery.

### State B — phase 1 (in-crate layout)

| Today | Becomes | Size after |
|---|---|---:|
| `connection_service.rs` 1,647 | + `placement.rs`, `split_start.rs`, `attachment_progress.rs`, `managed_launch.rs`; clusters folded into existing siblings | ~390 |
| `cli_session_manager.rs` 1,371 | directory: `pty_handle`, `control_lease`, `argv`, `launch`, `pty_spawn`, `terminals`, `livekit_bridge` | ~170 + ≤300 each |
| `cursor_cli_spawn.rs` 524 + claude spawn fn | `cli_spawn/{claude,cursor,chat,resume,common}.rs` | ≤ 400 each |
| `split_session.rs` 651 | + `agent_argv.rs`, `agent_credentials.rs` | ~350 |
| `svc_start_session_core.rs` 911 | + `svc_spawn_tddy_coder.rs`, `svc_start_tool_session.rs`, `svc_start_workspace_branch.rs`, `svc_start_cli_branches.rs` | ~230 |
| `session_coordinate_handlers.rs` 818 | + `session_list_entries.rs`, `svc_resume_session.rs`, `svc_signal_delete_session.rs` | ~320 |
| `svc_start_sandboxed_claude_cli_session.rs` 661 | + `svc_sandboxed_jail_launch.rs` (~330, shared), `svc_sandboxed_claude_worktree.rs` | ~230 |
| `svc_start_sandboxed_cursor_cli_session.rs` 505 | uses the shared jail launch | ~200 |
| `svc_session_agent_ports.rs` 644 | + `svc_session_agent_port_adapters.rs`, `svc_peer_routed_session_agents.rs` | ~115 |
| `svc_session_files_ports.rs` 503 | + `svc_peer_routed_session_files.rs` | ~210 |
| `svc_spawn_split_agent.rs` 520 | + `svc_paired_codebase_teardown.rs` | ~375 |
| `svc_resolve_tddy_tools_path.rs` 475 | + `svc_host_builders.rs` | ~120 |

### State B — phase 2 (proposed receivers; confirmed after phase 1)

| Topic | Production today | Proposed receiver | Receiver after |
|---|---:|---|---:|
| 1 Agent CLI start/resume (with `cli_spawn`, `cli_session_manager`, sandboxed starts) | 6,398 (less after dedup) | new `tddy-agent-launch` | ≤ ~6k |
| 4 Split and sandboxed-codebase | 1,981 | new `tddy-session-split` (the working name from memory) | ~2k |
| 3 Agent clones and roster | 2,746 | `tddy-session-agents` (3,592) | ~6.3k |
| 8 Attachments | 919 | `tddy-session-files` (3,639) | ~4.6k |
| 10 Activity ports | 625 | `tddy-session-activity` (1,573) | ~2.2k |
| 5 Session catalog | 1,397 | `tddy-session-catalog` (747) | ~2.1k |
| 6 Terminals / PTY runtime / tasks / actions | 1,314 | `tddy-terminal-rpc` (2,348), plus the tasks/actions RPCs to be decided | ~3.6k |
| 7 Routing / peers / admission | 930 | `tddy-daemon-kernel` (2,880) | ~3.8k |
| 9 Stack spawns | 814 | to be decided: `tddy-session-split` or its own crate, since `tddy-pr-stack` risks a cycle | — |
| 11 Demo VM | 302 | `tddy-daemon-rpc` (3,447) | ~3.7k |
| 2 RPC host core | 3,472 | **stays**, as the wiring: host struct, construction, port impls, facades | target ≤ ~1.5k |

**Wiring crate, defined:**
- no file ≥ 500 production lines;
- no function beyond construction, delegation and port impls;
- target ≤ ~1.5k production lines.

The exact number is the developer's to confirm.

### Callers

- `tddy-daemon-rpc/tests/*` use the placement types: keep `pub use`.
- `tests/stream_agent_activity_delta_rpc_acceptance.rs:20` uses `activity_delta_frames`: repoint it to
  `tddy-session-activity`. It is this crate's own test, so no consumer is edited.
- `tddy-coder/src/session_participant/terminal_manager.rs:355` has a `strip_resize` copy. Repointing it
  **edits a consumer**, so the developer must decide: leave that copy, or accept the exception.

## Baseline

To be recorded in commit 2:
- `./test -p tddy-session-lifecycle`, with pass/fail/ignored counts per suite;
- the names of known-red tests (see Prerequisites) and the known flake.

After every seam the same numbers come back.

## Decisions & trade-offs

- **One PR, two phases.** The developer's decision (2026-09-23).
  - Two nodes (restructure, then split) would let phase 1 merge on its own and keep each diff
    reviewable. It is still open to the developer as an alternative.
  - Phase 1 alone is roughly 12k lines of moves plus seven extract-methods.
- **Phase 1 fully before phase 2.** The developer's decision: the >500-line files and the code issues
  are resolved before the final split.
- **Dedupe by extract-method with explicit parameters, not by a new abstraction.** Lowest behaviour
  risk.
- **`PeerRouted*` stays** unless the developer approves the forwarding port.

## Refactoring needed

## Validation results

## TODO

- [x] Record initial discovery
- [x] Cross-check `docs/code-issues/` and `docs/dev/todo/` (Step 2b)
- [x] Create changeset — this document
- [ ] USER REVIEW — layout, receivers, wiring-crate definition
- [ ] Baseline + characterisation tests (commit 2)
- [ ] Phase 1
- [ ] Phase 2
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`)

## Final Checklist

Executed at wrap:

- [ ] Every non-test `src/` file in lifecycle and in each receiver < 500 production lines (inline-test-block rule)
- [ ] `tddy-session-lifecycle` meets the wiring-crate definition above
- [ ] Every receiver ≤ 10k; none depends on `tddy-session-lifecycle`
- [ ] All 16 code-issue records deleted or narrowed, final measurements in the change-history entry
- [ ] The ✅ RESOLVED HERE backlog entry (2026-08-13 trim-to-option) deleted
- [ ] Consumer crates unedited, apart from any listed exception
- [ ] `/analyze-code-issues` run on every new crate
