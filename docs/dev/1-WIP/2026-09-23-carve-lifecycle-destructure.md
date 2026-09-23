# Changeset: destructure tddy-session-lifecycle, close its code issues, remove its duplicate code

**Date**: 2026-09-23
**Status**: 🚧 In Progress — planned
**Type**: Refactor (in-crate restructure; no behaviour change)
**Stack**: `#carve` 14/15, on top of `restructure-engine-fixes` (#527), which sits on `core-split` (#522)

## Initial Discovery

[2026-09-23-carve-lifecycle-wiring-initial-discovery.md](./2026-09-23-carve-lifecycle-wiring-initial-discovery.md)
records:
- the topic breakdown;
- every file over 500 production lines, with its seams;
- the seven functions that no move can shrink;
- the duplication counts;
- the coupling.

State A below is distilled from it.

## Affected Packages

- **`tddy-session-lifecycle`**: restructured in place.
- **`tddy-worktree-service`**: gains `MpscResultStream::into_receiver`, so lifecycle's copy can go.
- **`tddy-pty`**: gains the resize-escape decoder, replacing three copies (see DRY).
- **`tddy-sandbox-recipes`**: exports `PERMISSION_PROMPT_TOOL`.
- **`tddy-coder`, `tddy-sandbox-runner`**: each drops its own copy of the resize decoder and uses the
  shared one. These are **consumer edits**, allowed as the DRY exception the developer accepted
  (2026-09-23).

## Related Feature Documentation

None. This is a behaviour-preserving restructure, so there is no PRD.

## Summary

`tddy-session-lifecycle` holds **21,264 production lines**:
- 11 files over 500 production lines;
- 7 functions of 243–857 lines;
- 16 open code-issue records;
- at least eleven measured duplications, three of them across crates.

This PR fixes all of that **in place**. Every file ends under 500 production lines, no function is
over 150 lines, every duplicate is replaced by one shared definition, and every code-issue record is
closed or narrowed.

**Nothing leaves the crate here** apart from the DRY targets named above. The public
`tddy_session_lifecycle::…` surface is unchanged. Moving the topics out is the next node's job.

## Background

This is the first of the two lifecycle nodes that close `#carve`. The developer's order (2026-09-23)
is to **destructure first, then split**. A crate move cannot cut an 857-line function, a 1,647-line
module file or three copies of the sandboxed-launch code. Moving them as they are would only relocate
the debt.

The developer also decided:
- the open code issues are resolved before the split;
- duplicate code is removed (DRY), not moved;
- these are **two PRs**, so this restructure merges and is reviewed on its own.

## Responsibility

- **Tests before seams.**
  - Characterisation tests for `start_sandboxed_cursor_cli_session`, as its CRAP record requires.
  - Coverage of all three callers before relaunch is switched onto the shared jail launch.
- **Split the 11 oversized files** along the seams in the discovery. Every file ends under 500
  production lines, with a target of ≤ 400.
- **Extract-method on the seven long functions:**
  - `start_session_core` (857)
  - `start_sandboxed_claude_cli_session` (615)
  - `start_sandboxed_cursor_cli_session` (465)
  - `spawn_claude_cli_session_inner` (408)
  - `spawn_cursor_cli_session_inner` (338)
  - `spawn_split_agent` (271)
  - `resume_session_at_session_coordinate` (243)
- **DRY:** replace every duplicate in the inventory below with one definition.
- **Relocate misplaced code** to the file of the topic it belongs to, so the next node's moves carry
  nothing from another topic (list below).
- **Close or narrow each of the 16 code-issue records** by re-measuring it.

## DRY inventory

| # | Duplicate | Copies | One definition |
|---:|---|---|---|
| 1 | Sandboxed launch-and-register: jail dir, context dir, `canonicalize_exec`, semantic-index env, warm-up, spawn/ready/bridge/state, metadata | Claude (`svc_start_sandboxed_claude_cli_session.rs:46-660`), Cursor (`svc_start_sandboxed_cursor_cli_session.rs:40-504`, which matches Claude line for line on 76% of its lines), relaunch (`svc_relaunch_sandboxed_runner.rs`, ~80 lines) | `svc_sandboxed_jail_launch.rs`, with the parts that differ (env, mounts, `session_type`, `hook_token`) as explicit parameters |
| 2 | `tddy-coder` spawn-backend match | `svc_start_session_core.rs:776-893` and `session_coordinate_handlers.rs:383-500` | `spawn_tddy_coder(ToolSpawnPlan)` |
| 3 | Non-sandboxed Claude and Cursor spawn: project lookup, initial changeset, local worktree, semantic index, trimmed prompt, `SessionMetadata`, push and response (~155 lines each side) | `spawn_claude_cli_session_inner` (`connection_service.rs:263-670`), `spawn_cursor_cli_session_inner` (`cursor_cli_spawn.rs:113-450`) | `cli_spawn/common.rs` plus `CliSpawnRequest` |
| 4 | CLI start prelude (sessions base, new id, attachments, initial prompt; ~25 lines) and the `managed_recipe` block | claude and cursor branches of `start_session_core`, `svc_spawn_split_agent.rs:68-88` | `cli_start_prelude`, `managed_recipe_for` |
| 5 | Semantic-index build plus env var | 5 files | one helper in `cli_spawn/common.rs`, used by all five |
| 6 | `resolve_branch_workflow` call sequence | 5 files | one helper |
| 7 | `setup_worktree_for_session_with_optional_chain_base` wrapping | 6 files | one helper |
| 8 | Trim, and treat empty as unset, into `Option<String>` | 6 sites (the [2026-08-13 entry](../todo/2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md)); a nested copy already exists inside `resume_agent_and_recipe` | the nested helper hoisted to module scope |
| 9 | Routed session-agent methods copy each request field by field into an identical literal | 7 methods in `svc_session_agent_ports.rs` | pass `req` |
| 10 | `MpscResultStream` | `connection_service.rs:64-100`, `tddy-worktree-service/src/stream.rs` | the `tddy-worktree-service` copy, once it gains `into_receiver` |
| 11 | `activity_delta_frames` (a stale copy of the live function) | `connection_service.rs:1382-1457`, `tddy-session-activity/src/service.rs:282` | the `tddy-session-activity` one. Its one caller, `tests/stream_agent_activity_delta_rpc_acceptance.rs:20`, is repointed, and the headroom `const _: assert!` is kept |
| 12 | Resize-escape decoder `strip_resize` | `cli_session_manager.rs:1126`, `tddy-coder/src/session_participant/terminal_manager.rs:355` (byte-identical), `tddy-sandbox-runner/src/runner.rs:1009` (`strip_resize_escape`; confirm it is the same function before merging) | `tddy-pty`. Lifecycle and `tddy-coder` already depend on it, and it pulls in only `bytes` and `tddy-task`. The encoder `encode_resize_osc` (`tddy-terminal-rpc/src/pty_relay.rs`) may join it, so the pair lives together |
| 13 | `PERMISSION_PROMPT_TOOL` | `split_session.rs:49`, `tddy-sandbox-recipes/src/claude_cli.rs:162` | exported from `tddy-sandbox-recipes`, which lifecycle already depends on |

**How to dedupe.**
- Use extract-method with explicit parameters, not a new abstraction (no `SandboxedAgentKind`).
- Merge a copy only once it is proven identical. When two copies differ, the difference becomes a
  parameter, and the difference is written down.
- **Each seam gets a sweep for further copies.** Any found are added to this table before they are
  merged, not silently folded in.

## Misplaced code, relocated here

| Code | Found in | Belongs to |
|---|---|---|
| `start_split_claude_cli_session` | `svc_materialize_staged_attachment.rs:270` | split sessions |
| `prepare_session_attachments`, `materialize_session_attachments`, `run_exec_tool_locally` | `svc_resolve_os_user.rs:117-188` | attachments, and exec tools |
| `session_dir_for`, `ensure_session_room` | `svc_resolve_listed_worktree.rs:365,385` | catalog, and rooms |
| jail and subagent env builders | `svc_turn_end_reporter.rs:176,211` | sandboxed launch |
| host builders, plus `record_rpc_activity`:420, `mint_first_admission_token`:218, `maybe_spawn_presenter_observer`:432 | `svc_resolve_tddy_tools_path.rs` | `svc_host_builders.rs`, routing, admission, activity |
| `ManagedWorkflow` (`session_toolcall`) | host core | agent launch |

## Restructure plans (proven 2026-09-23)

The plans are in [`2026-09-23-carve-lifecycle-wiring-plans/`](./2026-09-23-carve-lifecycle-wiring-plans/),
one JSONL per seam group, and nothing has been applied yet. Each was run through
`tddy-tools restructure check --deep` against a warm index. The cold index took 7m10s; after that,
clean checks took 2–14 s.

| Plan | Target | `--deep` |
|---|---|---|
| `01-connection-service-clusters` (10 × `extract_module`) | `connection_service.rs` | ⛔ all refused: import loop (defect E1) |
| `02-cli-session-manager-dir` (9 seams) | `cli_session_manager.rs` | ⛔ 5 of 9 refused (E3, and a grouped `use`) |
| `02a` clean subset (pty_handle, relaunch, control_lease, livekit_terminals) | same | ✅, leaving ~1,045 |
| `03-cursor-cli-spawn` (chat, resume) | `cursor_cli_spawn.rs` | ✅ → ~366 |
| `04-split-session` / `04a` (credentials only) | `split_session.rs` | ⛔ `agent_argv` (placeholder in a test module) / ✅, leaving ~551 |
| `05` / `05a` (teardown only) | `svc_spawn_split_agent.rs` | ⛔ extract-methods (E2) / ✅ → ~366 |
| `06-ports-files` | the two ports files | ✅ → ~118 and ~209 |
| `07-host-builders` | `svc_resolve_tddy_tools_path.rs` | ✅ → ~74 (new file ~401) |
| `08-session-coordinate-handlers` | `session_coordinate_handlers.rs` | ⛔ import loop (E1) |
| `09-sandboxed-claude-extract-methods` (8 ops) | `svc_start_sandboxed_claude_cli_session.rs` | ✅. The file stays long until the helpers are moved out |
| `10` / `10a` (`spawn_tddy_coder` only) | `svc_start_session_core.rs` | ⛔ 5 of 6 (E2) / ✅ |

**Engine defects that block the largest seams:**

- **E1 — the import pass loops forever** (`tddy-code-restructuring` `backends/rust.rs`, `next_import`).
  - It collects unresolved names from the whole file, not just the new module. Its alias and
    parent-binding branches return an import without checking it, and never mark the name as
    unimportable, so it writes the same `use` 512 times.
  - Likely trigger: `use start_session_event::Event as StartSessionEventKind`.
  - Blocks all of `01` and `08`, which is every seam in `connection_service.rs` and
    `session_coordinate_handlers.rs`.
- **E2 — extract-method signatures come out `req: _`.** rust-analyzer leaves the session proto types
  untyped (`StartSessionRequest` becomes `_` or `&_`). This happened 3 times against a warm index, so
  the engine's "retry" advice is wrong. The suspected cause, unconfirmed, is the generated `session.rs`
  included from `OUT_DIR`. It blocks every extract-method that reads `req` in `start_session_core` and
  `spawn_split_agent`, and probably `resume_session_at_session_coordinate`.
- **E3 — a seam cutting an `impl` is refused too eagerly.** Calls like `self.method()` in the same file
  count as "would resolve nowhere", although method calls resolve from anywhere. Workaround: split
  the `impl` by hand at the seam, adding only `}` / `impl X {` lines, then move whole `impl` blocks.

**What the engine cannot express**, so it is done by hand and recorded:

- **Folding code into an existing sibling file.** The name-collision check refuses it. Plan `01` writes
  new files instead (`stack_seed_validation`, `stack_child_spawn`, `conversation_spawn`,
  `roster_replacement`).
- **Moving code to a module under a different parent.** `cli_spawn/{claude,cursor}.rs` needs a
  `git mv` after `01` and `03`.
- **Seams that are not contiguous.** `WorktreeSource` becomes its own file; `split_session`'s
  constants and `ControlLeaseInfo` stay where they are.
- **All the DRY rows.** Every one is a hand edit.
- **Order.** Extract-methods compose only when a plan runs them **bottom-up**; top-down is refused.
  `09` and `10` are ordered that way.

**Plans that can only be finished after earlier applies:**
- resume and list, after `08`;
- moving the `09`/`10` helpers out, after the `impl` hand-split;
- the CLI spawn extract-methods;
- the relocations of misplaced code.

## Boundaries

- **No behaviour change.** The recorded baseline is re-run with zero regressions after every seam.
- **No moved or extracted code is written by hand** where `tddy-tools restructure` can do it. A hand
  move (`git mv` plus `use` fixes where the engine refuses) is recorded with the refusal that forced
  it. The deduplications are necessarily hand edits and are reviewed as such.
- **No topic leaves the crate.** The only code that leaves is the DRY targets above.
- **Consumer edits are limited to the DRY exception** (`tddy-coder`, `tddy-sandbox-runner`) and to any
  test that reads lifecycle source by path. Each one is listed.
- **The public `tddy_session_lifecycle::…` surface is unchanged.**
- **The `PeerRouted*` wrappers stay in this crate.** They are split into their own files, not moved.
- **Nothing in `tddy-core` or the nine #522 crates is touched.**

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `13` restructure-engine-fixes (#527) | the fixed `tddy-tools restructure`: E1, E2 and E3 no longer refuse correct plans | runs the refused plans against the engine in this branch's tree | touch `tddy-code-restructuring` |
| `12` core-split (#522) | `tddy-core` as facades over nine crates | lifecycle keeps naming `tddy_core::…`, and those paths resolve through the facades | touch `tddy-core` or the nine crates, or repoint lifecycle's `tddy_core::` imports |

## Draft PR contract

This is a mechanical restructure, one of the pr-stack skill's two named exceptions, so there is no
stub surface to publish. The draft is this plan together with the restructure plans in
`2026-09-23-carve-lifecycle-wiring-plans/`. The next commit is the recorded baseline plus the
characterisation tests.

## Green wave

**Wave:** after #522.
**Greenable independently:** **not until #522 and #527 (the engine fixes, directly below) are green.** It inherits #522's three open failures
(`git_plumbing_shape` ×2, `session_store_shape` ×1). The baseline is taken later, at the developer's
call (2026-09-23), and records them by name if they are still red.
**Concurrent with:** nothing.
**Blocks:** the wiring node above it, which moves topics out of the layout this PR creates.

## Prerequisites

| Item | Verdict | What this change does about it |
|---|---|---|
| #522 not yet green (3 known failures) | ⛔ **BLOCKING the baseline** | The baseline is taken later; it records the 3 by name if still red |
| Engine defects E1 (import loop), E2 (`req: _`), E3 (`impl`-seam refusal), in "Restructure plans" | ⛔ **BLOCKING** the seams they refuse | **Fixed in #527**, the node directly below this one. The refused plans (`01`, `02`, `05`, `08`, `10`) wait for #527's green, then this branch is rebased onto it and the plans are re-run with `check --deep` |
| `docs/code-issues/crap-svc-start-sandboxed-cursor-cli-session.md`: "**Restructure: no — tests first**" | ⛔ **BLOCKING** for that function | Characterisation tests land before its seams are cut |
| The 10 `complexity-*.md` records (`start_session_core`, `start_sandboxed_claude_cli_session`, `spawn_cursor_cli_session_inner`, `resume_session_at_session_coordinate`, `spawn_split_agent`, `delete_paired_codebase_session`, `start_split_claude_cli_session`, `ensure_project_available_for_start`, `resume_claude_cli_session`, `handle_rpc`) | ✅ **RESOLVED HERE** | Extract-method, then re-measure. Delete the record, or narrow it if the fix is partial |
| The 5 `oversized-file-*.md` records (`connection_service`, `session_coordinate_handlers`, `split_session`, `svc_spawn_split_agent`, `svc_start_session_core`) | ✅ **RESOLVED HERE** | Split below 500, then re-measure and delete. Their designed seams are the starting point |
| [2026-08-13 … trim-to-option string block](../todo/2026-08-13-tddy-daemon-connection-service-rs-repeats-a-trim-to-option-string-bloc.md) | ✅ **RESOLVED HERE** | DRY #8, plus `validate_stack_seed_base_session` → `stack_parent.rs` |
| [2026-09-19 file-length gate stops at the first `#[cfg(test)] use`](../todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md) | ⚠ **DURING** | Measure every seam with the inline-test-block rule. The gate itself is not fixed here |
| [2026-09-19 stack-child-spawn tests flake under concurrency](../todo/2026-09-19-stack-child-spawn-tests-flake-under-concurrency.md) | ⚠ **DURING** | Known baseline noise; record it as such |
| [2026-09-09 untested complexity hotspots](../todo/2026-09-09-tddy-daemon-untested-complexity-hotspots.md) | ⚠ **DURING** | `connection_service.rs` and `session_agent_clone.rs` are on it. Characterise before extract-method on untested code, and re-measure at wrap |
| [2026-07-01 tddy-daemon](../todo/2026-07-01-tddy-daemon.md) (stdio transport) | — unrelated | The launch path is deduplicated, but its transport does not change |

## Scope

- [ ] ⛔ Baseline recorded (`./test -p tddy-session-lifecycle`, per-suite counts, known-red names, known flake). **Taken later**
- [ ] ⛔ Characterisation tests: `start_sandboxed_cursor_cli_session`; the relaunch path
- [ ] Warm index (`./run-index-daemon`); every plan passes `restructure check --deep` before `apply`
- [ ] DRY #10, #11, #13 (the cross-crate one-liners)
- [ ] Misplaced code relocated (table above)
- [ ] `connection_service.rs` split (placement, split_start, attachment_progress, managed_launch, stack_parent, spawn handlers, agent_roster)
- [ ] `spawn_claude_cli_session_inner` → `cli_spawn/claude.rs`; `cursor_cli_spawn` → `cli_spawn/cursor.rs` + `chat.rs` + `resume.rs`
- [ ] DRY #3, #5, #6, #7, #8: `cli_spawn/common.rs` + `CliSpawnRequest`
- [ ] `cli_session_manager` → directory module (7 files); DRY #12 resize decoder → `tddy-pty`
- [ ] `split_session` → `agent_argv.rs` + `agent_credentials.rs`
- [ ] `start_session_core` extract-method; DRY #2 `spawn_tddy_coder`, DRY #4 prelude
- [ ] `session_coordinate_handlers` split; `session_entry_from_listing`
- [ ] DRY #1 `svc_sandboxed_jail_launch`: Claude, then Cursor, then relaunch
- [ ] Ports files split; DRY #9
- [ ] `svc_spawn_split_agent` teardown split + extract-method; `svc_host_builders.rs`
- [ ] Every non-test file < 500 production lines; no function > 150 lines
- [ ] All 16 code-issue records re-measured and deleted or narrowed

## Technical changes

### State A

- 21,264 production lines, 7,570 in-`src/` test lines, 59 integration suites.
- 11 files ≥ 500 lines and 7 functions of 243–857 lines.

The discovery has the per-file seams.

### State B (in-crate layout)

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

The deduplication should also shrink the crate's total, by an estimated 1–1.5k lines. It is
measured at wrap.

### Callers

- `tddy-daemon-rpc/tests/*` use the placement types; keep the `pub use`.
- `tests/stream_agent_activity_delta_rpc_acceptance.rs:20` is repointed to `tddy-session-activity`
  (DRY #11). It is this crate's own test.
- `tddy-coder` and `tddy-sandbox-runner` use the shared resize decoder (DRY #12). This is the accepted
  consumer-edit exception.

## Baseline

**Taken later**, at the developer's call. It records:
- `./test -p tddy-session-lifecycle` per-suite counts;
- known-red names and the known flake;
- `./test -p tddy-coder -p tddy-sandbox-runner -p tddy-pty -p tddy-worktree-service -p tddy-sandbox-recipes`
  for the DRY targets.

After every seam, the same numbers come back.

## Decisions & trade-offs

- **Two PRs** (developer, 2026-09-23): this restructure first, the final split above it.
- **Destructure before splitting, and resolve code issues first** (developer).
- **DRY, including across crates** (developer). The consumer edits this needs (`tddy-coder`,
  `tddy-sandbox-runner`) are accepted as the exception.
- **Dedupe by extract-method with explicit parameters, not by a new abstraction.** This is the lowest
  behaviour risk.
- **The resize decoder goes to `tddy-pty`, not `tddy-terminal-rpc`.** `tddy-sandbox-runner` runs inside
  every jail, so the lightest possible crate is the one it should depend on.

## Refactoring needed

## Validation results

## TODO

- [x] Record initial discovery
- [x] Cross-check `docs/code-issues/` and `docs/dev/todo/` (Step 2b)
- [x] Create changeset — this document
- [x] Restructure plans proven with `check --deep`: 8 of 14 clean, 3 engine defects
- [ ] USER REVIEW — layout and DRY inventory
- [ ] Baseline + characterisation tests
- [ ] Implementation
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`)

## Final Checklist

Executed at wrap:

- [ ] Every non-test `src/` file in lifecycle < 500 production lines (inline-test-block rule)
- [ ] No function in lifecycle > 150 lines
- [ ] Every DRY inventory row has one definition left; the per-seam sweep's additions are recorded
- [ ] All 16 code-issue records deleted or narrowed, with final measurements in the change-history entry
- [ ] The ✅ RESOLVED HERE backlog entry (2026-08-13 trim-to-option) deleted
- [ ] Public `tddy_session_lifecycle::…` surface unchanged; consumer edits limited to the listed exceptions
- [ ] Baseline numbers matched
