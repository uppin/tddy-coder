# Changeset: destructure tddy-session-lifecycle, close its code issues, remove its duplicate code

**Date**: 2026-09-23
**Status**: 🚧 In Progress — plans `03`–`01`, then `10b`, `09c`, `11`–`18`, `20`, `21` applied, and DRY #2–#13 done. No file is at or over 500 production lines. What is left needs the developer: see "Consent list"
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
- **`tddy-code-restructuring` is touched once, for the plan `05` refusal** (developer, 2026-09-24).
  This relaxes the original "does not touch `tddy-code-restructuring`" boundary. The change is
  `51211cd8`: the Rust backend closes every document an operation opens, with its live-server test.
  No other engine change lands here. The open gaps G–Q stay in their TODOs.

## Dependencies

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `13` restructure-engine-fixes (#527) | the fixed `tddy-tools restructure`: E1, E2 and E3 no longer refuse correct plans | runs the refused plans against the engine in this branch's tree. **One engine fix lands here instead of on #527** (developer, 2026-09-24): `51211cd8`, which closes documents so a shared server reads the tree again (see "Apply run 2026-09-24") | touch `tddy-code-restructuring` beyond that one fix |
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

- [x] Baseline recorded (`./test -p tddy-session-lifecycle`, per-suite counts, known-red names, known hang). Taken 2026-09-24; the DRY-target packages' too (58 / 481 / 12 / 1, at `3a96ca22`)
- [ ] Characterisation tests: `start_sandboxed_cursor_cli_session`; the relaunch path. **Only if a seam needs one** (developer, 2026-09-24); none of the plans applied so far did
- [ ] Warm index (`./run-index-daemon`); every plan passes `restructure check --deep` before `apply`. Every applied plan did, `05` and `01` included, once `51211cd8` fixed the refusal
- [x] DRY #10, #11, #13 (the cross-crate one-liners): `b6be2b73`, `4cd52230` (kept as a delegate: the path is public), `3616c80c`
- [ ] Misplaced code relocated (table above). ✅ 9 seams into modules of their own (`11`, `93a95054`); re-parenting them, and `ManagedWorkflow`, is a hand move (consent list)
- [x] `connection_service.rs` split (placement, split_start, attachment_progress, managed_launch, stack_parent, spawn handlers, agent_roster). ✅ 10 topic files (`01`, `23b73d0b`); 434 production lines now. Folding them into existing siblings is a hand move (consent list)
- [ ] `spawn_claude_cli_session_inner` → `cli_spawn/claude.rs`; `cursor_cli_spawn` → `cli_spawn/cursor.rs` + `chat.rs` + `resume.rs`. ✅ `chat.rs` and `resume.rs` (`03`); ✅ `connection_service/claude_cli_spawn.rs` (`01`); the `git mv` into `cli_spawn/` is open
- [x] DRY #3, #5, #6, #7, #8, as `service_util` helpers rather than `cli_spawn/common.rs` + `CliSpawnRequest` (see "DRY rows")
- [x] `cli_session_manager` → directory module (7 files); DRY #12 resize decoder → `tddy-pty`. ✅ directory module, 9 files (`02`, `834a76b2`); DRY #12 (`0ce85fe8`)
- [x] `split_session` → `agent_argv.rs` + `agent_credentials.rs` (`04`)
- [ ] `start_session_core` extract-method; DRY #2 `spawn_tddy_coder`, DRY #4 prelude. ✅ `10a`, `10b` (857 → 358), file split (`12`), DRY #2 (`d4ef468d`) and #4 (`be55575c`); the guards that remain are refused (E4, consent list)
- [ ] `session_coordinate_handlers` split; `session_entry_from_listing`. ✅ split (`08`); `session_entry_from_listing` open
- [ ] DRY #1 `svc_sandboxed_jail_launch`: Claude, then Cursor, then relaunch. ✅ Claude's steps (`09b`, `09c`, `13`, `18`) and relaunch's (`16`, `21`) are in modules; the merge waits on coverage (consent list)
- [x] Ports files split; DRY #9. ✅ split (`06`, `86670241`); DRY #9 (`4b2c2726`)
- [x] `svc_spawn_split_agent` teardown split + extract-method; `svc_host_builders.rs`. ✅ `svc_host_builders.rs` (`07`, `1a9a72d4`); ✅ `svc_paired_codebase_teardown.rs` and 4 extract-methods (`05`, `8b55523e`)
- [ ] Every non-test file < 500 production lines; no function > 150 lines. ✅ files: 0 at or over 500 (was 11); functions: 5 over 150 (was 10), each on the consent list
- [x] All 16 code-issue records re-measured and deleted or narrowed (`3e05735c`)

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

**Taken 2026-09-24** on `aef355a4`, which is origin/master (`e72a0e97`, with #522 and #527) plus
this PR's 6 docs commits and the 4 applies `03`, `10a`, `04` and `08`. The DRY-target packages'
baseline is still to take, before the first DRY row lands.

**Command.** The run is `./test -p tddy-session-lifecycle` with `--no-fail-fast` and one `--skip`,
in the same dev shell, with the same fixture source and prebuild:

```bash
cargo test -p tddy-session-lifecycle --no-fail-fast -- --test-threads=1 \
  --skip sandboxed_bash_pty_action_streams_output
```

`./test` itself cannot express this. It passes its arguments *before* its own
`-- --test-threads=1`, so a libtest `--skip` would become a filter. Without `--no-fail-fast`, cargo
stops at the first red suite.

**Result:** 61 targets (60 test binaries plus doctests), **622 passed, 22 failed, 1 ignored, 1
skipped**.

| Known red | Count | Signature | Cause |
|---|---:|---|---|
| `sandbox_behavior_acceptance` (all 5) | 5 | `sandbox RPC bridge not installed — runtime must call install_sandbox_rpc_bridge` (`svc_resolve_tddy_tools_path.rs:171`) | the harness never installs the bridge. Every sandboxed-start path panics on macOS |
| `sandboxed_claude_cli_acceptance` (all 5) | 5 | same | same |
| `sandboxed_cursor_cli_acceptance` (all 4) | 4 | same | same |
| `sandboxed_session_lifecycle_acceptance`: `delete_sandbox_session_stops_child_and_removes_directory`, `resume_sandbox_session_respawns_and_updates_pid` | 2 | same | same |
| `session_sync_livekit_acceptance` (all 6) | 6 | `tddy-remote-git-repo is not built at …/target/debug`, then `Once instance has previously been poisoned` | `./test`'s prebuild list does not build `tddy-remote-git-repo` |

- **Known hang:** `action_sandbox_acceptance::sandboxed_bash_pty_action_streams_output` did not
  finish in over 20 minutes. It is skipped. Another worktree's run skips it the same way.
- **None of the 22 comes from this branch.** `git diff e72a0e97 aef355a4 -- packages/` touches no
  line naming the sandbox RPC bridge or the git transport. The files the panics come from, and every
  test file, are byte-identical to master.
- **The three #522 failures named in the Green-wave section are not in this crate.**
  `git_plumbing_shape` ×2 and `session_store_shape` ×1 belong to #522's crates, so this baseline
  cannot show them.

After every plan, the same 61 / 622 / 22 / 1 came back, with the same 22 failures by name (see
"Apply run 2026-09-24").

## Apply run 2026-09-24

**Setup:**
- #527's engine (`target/debug/tddy-tools`), run against the warm `./run-index-daemon`;
- plans from the scratch set: `real-06-ports-files`, `real-07`, `real-05`, `real-02-…`, `real-09b`,
  `real-01`;
- every snapshot hash matched the tree, so none needed `snapshot` or `reanchor.py`;
- **no characterisation tests were added** (developer, 2026-09-24: "guarding tests only if needed").
  No seam in this run needed one; every one was a pure move.

**The developer's rules for the run** (2026-09-24):
- a refusal stops that plan, with no hand move around it;
- hand edits are only corrections that make an engine move build;
- every non-building move gets a TODO.

The gap letters G–M are
[the gaps TODO](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md).
N1–N4 are [the lint-gate TODO](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md).
P and Q are [the extract TODO](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md).

| Plan | Engine | Hand fixes (gap) | Tests vs baseline | Commit |
|---|---|---|---|---|
| `06-ports-files` | 3/3 moved; the compile gate failed | `async_trait` ×3 (G). `prost::Message as _` into the adapters (I). A stray `use prost::Message;` removed from the peer-routed file (N3). Parent imports and 2 emptied `use …::{ }` groups (N1). The unused adapters glob (N2). fmt (N4) | identical | `86670241` |
| lint of `03`/`10a`/`04`/`08` | — | unused imports (N1), 2 unused globs (N2), fmt (N4), and `#[allow(clippy::too_many_arguments)]` on `spawn_tddy_coder` with a TODO for DRY #2 | — | `1d7fded9` |
| `07-host-builders` | 1/1 moved; the compile gate failed | `super::DaemonRpcHandler` → `super::super::` (H). 13 parent imports (N1). The glob (N2). A stray `use tddy_rpc::Status;` (N3). fmt | identical | `1a9a72d4` |
| `02-cli-session-manager-dir` | 9/9 moved; the compile gate failed. The engine widened 4 methods to `pub(crate)` | `strip_resize` → `pub(super)` in `livekit_bridge.rs` (J; the E0282 went with it). `async_trait` (G) and `prost::Message as _` (I) into `livekit_bridge.rs`. Parent imports (N1). 7 unused globs (N2). fmt | identical, and all 6 dependent crates `check --all-targets` clean | `834a76b2` |
| `05` spawn_split_agent | first run: ⛔ **refused** at `check --deep` and at `apply`, 3 of 3, for op 0 and for the full plan. After the engine fix `51211cd8`: 5/5 moved, and the compile gate passed. **L did not recur**, so the plan was not split | 6 parent imports (N1). The glob (N2). 3 × `&PathBuf` → `&Path`, with one `.clone()` → `.to_path_buf()`, and a new Q shape: `Ok(if … { … })` (`unit_arg`) → the `if`, then `Ok(())` (Q). fmt (N4). No comment lost | identical | `8b55523e` |
| `01` connection_service | first run: ⏸ **held** behind `05`. After the fix: 10/10 moved; the library built, and the test build failed on M | M: `Path`, `Changeset`, `SessionAttachment`, `SplitAgentPlacement` into the parent's `#[cfg(test)]` block, plus the dropped `pub use … spawn_blocking_with_timeout` restored (see below). 10 parent imports, 2 of them emptied groups (N1). 2 stray `PathBuf` imports (N3). fmt (N4). No comment lost | identical, and all 6 dependent crates `check --all-targets` clean | `23b73d0b` |
| `09b` sandboxed claude | 7/7 moved; the compile gate failed on K. It builds after the K fix | `WorkflowRecipe` qualified as `tddy_core::workflow::recipe::WorkflowRecipe` at the two new signatures (K). With the developer's consent: 14 comment lines restored (P) and the signatures reshaped (Q) | identical | `09b` commit, "split start_sandboxed_claude_cli_session into its jail steps" |

After every commit, `cargo clippy -p tddy-session-lifecycle --all-targets -- -D warnings` and
`cargo fmt -p tddy-session-lifecycle --check` are clean.

**The `05` refusal, verbatim:**

> 0: this seam cannot be cut here: the moved code names `SplitStartFailure`, and writing the parent's
> own `use super::super::SplitStartFailure;` into the module left 4 unresolved occurrence(s) of it,
> where there were 3: rust-analyzer cannot resolve what that declaration names. A type a build script
> generates reads this way until the server has loaded the script's output. Cut the seam where the
> moved code does not name `SplitStartFailure`, or make its path resolve first.

The path is correct. `svc_spawn_split_agent.rs` imports it as `use super::SplitStartFailure;`, and
it is a plain `pub(crate) enum` in `connection_service.rs:1179`, not build-script output. The
gaps TODO records this plan applying 5 of 5 on the same engine before this branch's applies, so the
refusal is new.

**The developer's decision** (2026-09-24): reproduce and fix it on this PR, relaxing the
"does not touch `tddy-code-restructuring`" boundary (see Boundaries and Dependencies), then land `05`
and `01` through the fixed engine.

**Root cause: a document left open on a shared server.** The Rust backend sent `didOpen` for every
document it worked on and never sent `didClose`. rust-analyzer treats an open document as the
authority on its file. `tddy-index-daemon` keeps one server per root and serves a new backend per
request, so a document outlived the run that opened it. It went on answering for its file with the
last text that run sent. The evidence comes from a temporary trace, removed before commit:

- **A/B on one warm daemon.** `05` op 0 gave no findings. Then a `check --deep` of plan `01`, which
  writes nothing, gave no findings. Then `05` op 0 again was refused with the exact message above. A
  fresh daemon never refuses it.
- **What `01` left behind.** Plan `01`'s check left `connection_service.rs` open at a **999-line**
  rehearsed text of a 1,652-line file. That was op 10's text, after nine seams had moved into
  `mod placement;`, `mod split_start;` and others. Their files existed only in the check's overlay, so
  for the server `connection_service::SplitStartFailure` was declared nowhere.
- **What the refused run saw.** Before anything was cut, the untouched file already reported 16
  unresolved names (the passing run: 0). Among them was every `use super::X` naming an item of
  `connection_service.rs`: `SplitStartFailure`, `peer_has_no_such_session`,
  `AttachmentMaterialization`, `AttachmentProgressSink`.
- **The hypothesis in the brief was wrong on two counts.** The assist never wrote the `use` (528
  lines against 532), so `prune_assist_imports` dropped nothing. `codeAction` offered no import at
  all. The reconstruction then wrote `use super::super::SplitStartFailure;`, and its own token read
  unresolved: 3 → 4.
- **Not a missing re-analysis wait.** `serverStatus` said `quiescent: true`, `loading: false` at every
  read.
- **Why a cold run or a restarted daemon passes.** Only vacuously: a server that has only just
  started reports no unresolved names at all.

**The fix** (`51211cd8`): `backends/rust/documents.rs` owns `did_open`, records each document it
opens, and closes them all when `resolve`, `anchor_for` or `outside_references` ends, whatever it
ended with. The test runs a real `check --deep` of two seams in the parent on a settled server, then
resolves a seam that names the parent's type through `super` on a fresh backend on the same server:
`import_pass_acceptance::imports_the_parent_s_type_on_a_server_an_earlier_check_rehearsed_its_parent_on`.
It failed with the production message ("left 4 unresolved occurrence(s) of it, where there were 3")
and passes after the fix. `./test -p tddy-code-restructuring`: 488 passed, 0 failed, 1 ignored.
After the fix, the same A/B on one daemon (`05` → `01` → `05`, then the full `05`) gives no findings
throughout.

**What the fix does not change.** Within one `check --deep`, an operation after the first is still
resolved against overlay text whose new files the server cannot see. The server now forgets that
text when the operation ends, so it no longer reaches the next run.

**`01`'s M, as it actually happens.** The shape the gaps TODO guessed was a moved `mod x;`, and no
seam of plan `01` carries one. Instead, the assist removed names from the parent's `use` groups when
a seam held their last non-test use. The test children reach those names through `use super::*`,
and nothing counted them. It also removed `spawn_blocking_with_timeout` from a **`pub use`**. The
build stayed green through the `pub(crate)` glob, but the crate's public path narrowed. It was
restored, because the public surface is a Boundary here. The gaps TODO's M section now has the
before/after.

**`09b` was held** after passing its tests:
- **P:** the assist dropped 14 comment lines from the extracted ranges. They are the readiness-gate
  and Seatbelt canonical-path rationale.
- **Q:** the new signatures leave 18 clippy findings (`ptr_arg` ×14, `too_many_arguments` ×3,
  `type_complexity`, an unneeded `mut`).

Restoring the comments and reshaping the signatures are both beyond a build correction, so the
applied, K-fixed and formatted state was saved as a patch outside the tree.

**The developer's decision** (2026-09-24): "Restore comments and sigs, file TODO for the engine
fix." The saved patch applied cleanly to `4a99f0e8`. Then, by hand:
- **P:** the 14 comment lines went back unreworded, each beside its statement in the function it
  moved into: 2 + 4 in `warm_up_jail_agents` and 6 + 2 in `prepare_jail_dirs`. The comment-line
  multiset is HEAD's plus the 4 new doc comments below.
- **Q:** the signatures were reshaped with no new `#[allow]`:
  - `&PathBuf`/`&String`/`&Vec<_>` → `&Path`/`&str`/`&[_]`, and `sessions_base` borrowed;
  - the 3 over-arity functions take file-local structs: `JailSession<'a>` (id, project, session and
    worktree dirs), `JailDirs` (was `prepare_jail_dirs`'s 5-tuple) and `JailLaunch` (what the
    runner is spawned with);
  - `type ManagedJailEnv` for `managed_jail_env`'s return;
  - the `mut` dropped.
- **Tests:** the same 61 / 622 / 22 / 1 as the baseline, with the same 22 by name. clippy and fmt
  are clean.

The engine fix is [the extract TODO](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md).
It now also records the hand-fixed shape.

**Plan `02` also lost a comment** (P): the 3-line `LiveKit bridge` section banner. It is already
pushed. The module's name now says what the banner said, so it was left as is.

## Apply run 2026-09-24, second half

Work items 1–5 of the developer's brief, in order: the refused extract-methods, the DRY rows, the
relocations, the oversized files and functions, and the code-issue records. The same rules held: a
refusal stops that op with no hand move around it, and hand edits are only corrections that make an
engine move build and lint (use lines, paths, visibility, clippy signature shapes, restoring dropped
comments). The DRY rows are hand edits by design, behaviour-preserving only. Letters G–M and W–X are
[the gaps TODO](../todo/2026-09-24-restructure-apply-gaps-from-the-lifecycle-destructure-run.md);
P–V are [the extract TODO](../todo/2026-09-24-restructure-extract-drops-comments-and-writes-clippy-failing-signatures.md).

**Setup.** This checkout's engine (`target/debug/tddy-tools`, with `51211cd8`) against the warm
`./run-index-daemon`. **The daemon was restarted whenever a hand edit touched code the next plan's
files depend on** (V): it answers from the tree as it was when it started, and a stale answer reads
as a refusal (`project: _`). N1–N4 are [the lint-gate TODO](../todo/2026-09-24-restructure-apply-leaves-the-lint-gate-red.md).

### Plans, in the order applied

| Plan | Target | Engine | Hand fixes | Tests | Commit |
|---|---|---|---|---|---|
| `10b` | `start_session_core`, 15 ranges between its early returns (bodies between exits, and call expressions after `return`) | 15/15; compile gate failed. The first cut refused one range (S: an `if … else` expression) and hung the daemon on another (R: a bare block `{`); both re-cut | K ×5; three breaks the gate never reached (a stray `*` before `log::warn!`, a value moved while borrowed, by-reference params the callee consumes); Q (`CliStart`; `repo_path` derived from the project); P ×22 | baseline | `5d9d0486` |
| `09c` | plan `09`'s op 6 (the worktree `match`), 4 ranges between its returns | 4/4; gate failed | K; Q (`JailBranch<'a>`, and a new `redundant_field_names` shape); P ×2 | baseline | `3a96ca22` |
| `11` | misplaced code (table above), 9 × `extract_module` | 9/9; gate passed | N1/N2 with `cargo fix` (imports only) | baseline | `93a95054` |
| `12` | `svc_start_session_core.rs` → 5 modules | first apply refused after a cold start (W); after a warming check, 5/5; gate failed | H ×3; X (`pub(super)` → `pub(in crate::connection_service)`); 16 engine-widened methods back to `pub(super)`; N1/N2 | baseline | `f6e9414b` |
| `13` | the sandboxed claude start's jail helpers → 3 modules | 3/3; gate passed | 10 widened items → `pub(super)`; N1/N2 | baseline | `cb7e387c` |
| `14` | `spawn_claude_cli_session_inner`, 6 extract-methods | 3 more ops refused (T, `SpawnStackParent<'_>`) and dropped; 6/6; gate failed | K ×3; Q (`ClaudeCliWorktreeCut`, `ManagedClaudeCliLaunch`, `ClaudeCliProcess`, each destructured on the first line so the body is the engine's); ptr_arg ×6; P ×7 | baseline | `7106b8cf` |
| `15` | `spawn_cursor_cli_session_inner`, 6 extract-methods | 6/6; gate failed | K; Q (`CursorCliSessionRecord`); ptr_arg ×8 | baseline | `3b6227c7` |
| `16` | `relaunch_sandboxed_runner`, 7 extract-methods | op on a comment line hung the daemon (R); argv op panicked rust-analyzer (U) and was dropped; 7/7; gate failed | K; Q (`RelaunchJailEnv`, `RelaunchedRunnerSpawn`, `RelaunchedJailBridge`, `type RelaunchManagedEnv`); ptr_arg; P ×2 | baseline | `c2ad27cd` |
| `17` | `claude_cli_spawn.rs`'s new steps → 1 module | 1/1; gate passed | widened items and fields → `pub(super)`; N1/N2 | baseline | `334d0d6f` |
| `18` | the sandboxed claude handler, 2 extract-methods | argv op panicked rust-analyzer again (U), dropped; 2/2; gate failed | one E0505 (`session_dir` moved while borrowed); Q (`JailRunnerEnv`) | baseline | `c21667c4` |
| `19` | `start_session_core`'s four branch bodies and its agent-allowlist check | ⛔ **refused, 4 of 4** (E4), nothing written | — | — | — |
| `20` | `spawn_split_agent`, `ensure_project_available_for_start`, `delete_session_directory` | 4/4; gate passed | Q (`SplitAgentProcess`, `ProjectClone`); P ×6 | baseline | `767d2333` |
| `21` | the relaunch's new steps → 2 modules | 2/2; gate passed | widened items → `pub(super)`; N1/N2 | baseline | `64912abc` |

"Baseline" means the plan's command gave 61 targets, **622 passed, 22 failed, 1 ignored**, with the
same 22 by name. The suite was run after `10b`, after `09c`, after the DRY rows, after `11`, after
`20` and after `21`, so each row's tests are those of the next run at or after its commit.
The run after `21` had one extra failure, `session_room_acceptance::the_terminal_bridge_publishes_the_block_the_session_was_started_with`:
Docker could not start the LiveKit testkit container ("failed to bind host port … address already in
use"). The suite re-run alone passed 21 of 21, so the count matches the baseline. The plan JSONLs are saved as `10b-…` to `21-…` in the plans directory, each as it was
applied (re-cut ops included, refused ops removed).

### DRY rows

| # | Result | Commit | Net lines |
|---:|---|---|---:|
| 1 | ⛔ **not done**, needs coverage: see the consent list | — | — |
| 2 | ✅ `spawn_tddy_coder(ToolSpawnPlan)`. What differed is `ToolSpawnPurpose` (labels; the start-only debug line) | `d4ef468d` | −111 |
| 3 | ✅ partly: one `starting_session_metadata` for 7 literals, and `find_registered_project` + `project_repo_root` for 9 lookups. The empty-LiveKit response literals (5 start, 4 resume) are left: they are field lists, not logic. No `CliSpawnRequest` struct: the shared pieces are `service_util` helpers, and the two spawn functions keep their signatures | `4f1d738f`, `1eca6b90` | −141 |
| 4 | ✅ `cli_start_prelude`, `managed_recipe_for`, and `attached_initial_prompt`, which the split agent shares | `be55575c` | −30 |
| 5 | ✅ `index_session_worktree` for all five | `682e824c` | −3 |
| 6 | ✅ `write_initial_changeset` for the four CLI starts (it replaces `09c`'s `write_jail_changeset`). workspace_session keeps its own call: it resolves before the session directory exists | `27e7fb3a` | −49 |
| 7 | ✅ `create_session_worktree` for four copies. workspace_session differs (own timeout mapping, no log) and is left | `141eb0c6` | −7 |
| 8 | ✅ `tddy_daemon_kernel::trim_to_option` (already hoisted), 7 sites plus 3 the sweep found | `ea9bc2a3`, `c6f756d0` | −68 |
| 9 | ✅ `req` for 8 literals (the inventory said 7), plus 2 the sweep found in `svc_activity_ports.rs` | `4b2c2726` | −65 |
| 10 | ✅ `tddy-worktree-service` gains `into_receiver`; lifecycle's path is a `pub use` | `b6be2b73` | −24 |
| 11 | ✅ **with a deviation**: the lifecycle function is public, so it stays as a one-line delegate (through the `measured_delta` conversion both callers now share) rather than being deleted, and its test is untouched | `4cd52230` | −13 |
| 12 | ✅ `tddy_pty::strip_resize`; the runner imports it as `strip_resize_escape`, so its call sites and tests are untouched | `0ce85fe8` | −74 |
| 13 | ✅ exported from `tddy-sandbox-recipes` | `3616c80c` | 0 |

**Net: +568 / −1,153 = −585 lines across the repo, of which −563 in `tddy-session-lifecycle`.**

Sweep findings, recorded and **not** merged: `tddy-tui`'s `parse_resize_from_buf` is a different
decoder (anchored at the buffer start, returns the bytes consumed). The resize *encoder* has three
copies (`tddy-terminal-rpc`, and two in `tddy-sandbox-app`); `tddy-sandbox-app` is not one of the
consumer-edit exceptions.

**The DRY-target baseline**, taken before the first row at `3a96ca22`: 58 targets, 481 passed,
12 failed, 1 ignored. The 12 are all of `tddy-worktree-service`'s `remote_git_livekit_acceptance`
(`tddy-remote-git-repo is not built`, the same cause as lifecycle's `session_sync` six). After the
rows: the same, plus `a_cached_size_is_served_after_reload_without_recomputing`, which fails 2 of 4
runs at `3a96ca22` itself. It is a race in the test, not a regression:
[its TODO](../todo/2026-09-24-worktree-size-reload-test-races-the-persist-it-reads.md). Every crate
that depends on lifecycle (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-model-registry`,
`tddy-telegram-control`, `tddy-tool-engine`, `tddy-worktree-service`) passes `check --all-targets`.

### Code-issue records (`3e05735c`)

- **Clean, removed**, final numbers (production lines by the rule below): `connection_service.rs`
  1,634 → **434**; `session_coordinate_handlers.rs` 818 → **414**; `split_session.rs` 647 → **409**;
  `svc_spawn_split_agent.rs` 502 → **445**; `svc_start_session_core.rs` 911 → **455**.
- **Moved and renamed**: `resume_session_at_session_coordinate` (also 242 → 137),
  `start_split_claude_cli_session` (146, unchanged), `delete_paired_codebase_session` (unchanged).
- **Narrowed**: `start_session_core` 857 → 358, `start_sandboxed_claude_cli_session` 615 → 342,
  `spawn_cursor_cli_session_inner` 337 → 244, `spawn_split_agent` 254 → 110,
  `ensure_project_available_for_start` 158 → 99. All are still over their records' 60-line budget.
- **Touched, still open**: the sandboxed cursor start's CRAP record (465 → 414, mechanical DRY
  merges only; still never executed).
- **Unchanged, untouched**: `handle_rpc` (147), `resume_claude_cli_session` (119).

### Consent list — refusals, and the hand moves not made

Each of these needs the developer's decision. None was worked around.

1. **E4 refuses every range in `start_session_core` that holds an early return** (plan `19`,
   verbatim, one of four):
   > 2: this seam cannot be cut here: the range returns early from the function around it, on line
   > 221 (`return Err(Status {`) and line 284 (`return Err(status);`) and line 306
   > (`return Err(status);`) and line 309 (`return Ok(started);`) and line 321
   > (`return Ok(started);`). An extracted function cannot carry an early exit of its caller: the
   > assist copies the `return` verbatim, so it returns from the new function instead — whose return
   > type differs, which is `E0308` at best and a silently skipped exit at worst. Cut the range so it
   > holds no `return`, or end it before the first one.

   What is left of the function (358 lines) is its 19 early exits and the code around them. Nine are
   `return Err(…)` guards, which *would* extract soundly into a `Result`-returning helper and a `?`.
   The other ten are the dispatch's own exits (six `return self.start_…().await`, four
   `return Ok(…)`). The engine cannot tell the two kinds apart. A hand extract of the `Err`-only
   guards is the option.
2. **T refuses every range naming `SpawnStackParent<'_>`** (plan `14`, 3 ops, verbatim in the
   extract TODO). That is why both CLI spawns stop at 255 and 244 lines. An engine fix is the option,
   or a hand extract.
3. **U: rust-analyzer panics on the runner-argv ranges** (plans `16` and `18`). They stay inline.
4. **DRY #1, the shared jail launch.** The Cursor start's CRAP record says "tests first", and its
   suite, the relaunch path's and the Claude start's are all in the known-red set on this host (the
   harness never installs the sandbox RPC bridge). So a hand merge would land with nothing exercising
   it. The Claude steps now sit in `jail_*` modules and the relaunch's in `relaunch_jail_*`, ready
   for it. It needs either the harness fixed or characterisation tests first.
   `start_sandboxed_cursor_cli_session` (414) and the Claude handler (342) stay over 150 until then.
5. **Re-parenting**: every module plan `11` created, and `cli_spawn/{claude,cursor}.rs`, need a
   `git mv` to their topic's parent. The engine cannot move code under a different parent.
   `ManagedWorkflow` (`session_toolcall.rs`) is the same case.
6. **Folding** plan `01`'s topic files into existing siblings: refused by the name-collision check,
   so it is a hand move.
7. **Not done, not refused**: `session_entry_from_listing` (the State B row); the ≤ 400 target for
   the 14 files that end between 400 and 493 production lines; `run_exec_tool_locally` is moved, but
   its exec-tool siblings in `svc_resolve_os_user.rs` are not contiguous with it.
8. **Deleting the 2026-08-13 trim-to-option backlog entry**, which DRY #8 resolved. The Final
   Checklist deletes it at wrap; CLAUDE.md asks before deleting files, so it stays until then.

## LoC assessment

`origin/master` merge-base (`e72a0e97`) against `64912abc`, for `packages/tddy-session-lifecycle`.

| Measure | Before | After |
|---|---:|---:|
| Files (whole package, including tests and docs) | 175 | 220 |
| Lines (whole package) | 55,142 | 56,403 |
| Non-test `src/*.rs` files | 72 | 122 |
| Their production lines | 20,899 | 22,387 |
| Files ≥ 500 production lines | 11 | **0** |
| Files ≥ 1,000 production lines | 2 | **0** |
| Production functions over 150 lines | 10 | 5 |

**Production lines went up by 1,488.** The DRY rows removed 563. The moves added more:

- `use` lines +286, because each new module file imports what it names;
- blank lines +365;
- `mod` declarations +50;
- code +755. Every extract-method writes a signature, a call and a return, every module wraps its
  methods in its own `impl` block, and 13 parameter structs were added (12 by Q fixes, plus
  `ToolSpawnPlan` for DRY #2).

Per commit, the engine applies since the merge-base added ≈ +2,050 `src` lines and the DRY rows
−563. The restructure trades lines for size: no file is over 500, and no function over 150 except
the five on the consent list.

**The 15 largest files, before → after** (same path; the rest of a split file lives in its new
modules):

| File | Before | After |
|---|---:|---:|
| `connection_service.rs` | 1,601 | 434 |
| `cli_session_manager.rs` | 1,371 | 173 |
| `connection_service/svc_start_session_core.rs` | 911 | 455 |
| `connection_service/session_coordinate_handlers.rs` | 818 | 414 |
| `split_session.rs` | 674 | 409 |
| `connection_service/svc_start_sandboxed_claude_cli_session.rs` | 661 | 493 |
| `connection_service/svc_session_agent_ports.rs` | 644 | 113 |
| `cursor_cli_spawn.rs` | 524 | 445 |
| `connection_service/svc_spawn_split_agent.rs` | 505 | 445 |
| `connection_service/svc_start_sandboxed_cursor_cli_session.rs` | 505 | 445 |
| `connection_service/svc_session_files_ports.rs` | 503 | 200 |
| `connection_service/svc_resolve_tddy_tools_path.rs` | 498 | 51 |
| `connection_service/svc_split_context_from_codebase_host.rs` | 471 | 471 |
| `session_deletion.rs` | 450 | 459 |
| `connection_service/svc_resolve_listed_worktree.rs` | 444 | 443 |

After, the largest is `svc_start_sandboxed_claude_cli_session.rs` at 493.

**The 10 longest functions, before → after:**

| Function | Before | After |
|---|---:|---:|
| `start_session_core` | 857 | 358 |
| `start_sandboxed_claude_cli_session` | 615 | 342 |
| `start_sandboxed_cursor_cli_session` | 465 | 414 |
| `spawn_claude_cli_session_inner` | 407 | 255 |
| `spawn_cursor_cli_session_inner` | 337 | 244 |
| `relaunch_sandboxed_runner` | 282 | 149 |
| `spawn_split_agent` | 254 | 110 |
| `resume_session_at_session_coordinate` | 242 | 137 |
| `delete_session_directory` | 168 | 98 |
| `ensure_project_available_for_start` | 158 | 99 |

**The rule, and how to re-run it.** Production lines are the lines outside every `#[cfg(test)]`
item (a `mod`, a `use`, an inline test block), with test-only files (`#[cfg(test)] mod x;`,
`*_tests.rs`, `tests.rs`, `test_util.rs`) excluded. This is the inline-test-block rule. The naive
count to the first `#[cfg(test)]` is shown too, because `connection_service.rs` reads 18 by it.
Function length runs from the `fn` line to its closing brace, and braces in strings, chars and
comments are skipped. No script for this exists in `scripts/`, so it is inline:

```bash
git archive e72a0e97 packages/tddy-session-lifecycle | tar -x -C /tmp/before
python3 loc.py /tmp/before/packages/tddy-session-lifecycle      # before
python3 loc.py packages/tddy-session-lifecycle                   # after
```

<details><summary><code>loc.py</code></summary>

```python
import os, re, sys
# Usage: python3 loc.py DIR  — DIR is packages/tddy-session-lifecycle of some tree.
# Rust production lines = lines before a file's first `#[cfg(test)]` line (the plan's rule).
FN = re.compile(r'^\s*(pub(\([^)]*\))?\s+)?(const\s+)?(async\s+)?(unsafe\s+)?fn\s+(\w+)')
def code_of(line, st):
    out = []; i = 0
    while i < len(line):
        if st == "block":
            if line.startswith("*/", i): st = None; i += 2
            else: i += 1
            continue
        if st == "str":
            if line[i] == "\\": i += 2; continue
            if line[i] == '"': st = None
            i += 1; continue
        if isinstance(st, str) and st.startswith("raw"):
            end = line.find('"' + st[3:], i)
            if end < 0: return "".join(out), st
            i = end + 1 + len(st) - 3; st = None; continue
        if line.startswith("//", i): break
        if line.startswith("/*", i): st = "block"; i += 2; continue
        m = re.match(r'b?r(#*)"', line[i:])
        if m and (i == 0 or not (line[i-1].isalnum() or line[i-1] == "_")):
            st = "raw" + m.group(1); i += len(m.group(0)); continue
        if line[i] == '"': st = "str"; i += 1; continue
        m = re.match(r"b?'(\\.|\\u\{[0-9a-fA-F]+\}|[^\\'])'", line[i:])
        if m: i += len(m.group(0)); continue
        out.append(line[i]); i += 1
    return "".join(out), st
def fns(lines, stop):
    res = []
    for i in range(stop):
        m = FN.match(lines[i])
        if not m: continue
        depth = 0; opened = False; st = None; j = i; done = False
        while j < len(lines) and not done:
            code, st = code_of(lines[j], st)
            for c in code:
                if c == "{": depth += 1; opened = True
                elif c == "}":
                    depth -= 1
                    if opened and depth == 0: done = True; break
                elif c == ";" and not opened: done = True; break
            if not done: j += 1
        res.append((j - i + 1, m.group(6), i + 1))
    return res
def item_end(lines, i):
    """Last line of the item starting at or after line i (skipping attributes and doc comments)."""
    j = i
    while j < len(lines) and (lines[j].strip().startswith("#[") or lines[j].strip().startswith("///") or not lines[j].strip()):
        j += 1
    depth = 0; opened = False; st = None
    while j < len(lines):
        code, st = code_of(lines[j], st)
        for c in code:
            if c == "{": depth += 1; opened = True
            elif c == "}":
                depth -= 1
                if opened and depth == 0: return j
            elif c == ";" and not opened and depth == 0: return j
        j += 1
    return len(lines) - 1
def test_mask(lines):
    mask = [False] * len(lines); test_mods = []
    i = 0
    while i < len(lines):
        if lines[i].strip().startswith("#[cfg(test)]"):
            end = item_end(lines, i + 1)
            for k in range(i, end + 1): mask[k] = True
            for k in range(i + 1, end + 1):
                m = re.match(r"^\s*(pub(\([^)]*\))?\s+)?mod\s+(\w+)\s*;", lines[k])
                if m: test_mods.append(m.group(3))
            i = end + 1
        else: i += 1
    return mask, test_mods
root = sys.argv[1]
texts = {}
for d, _, fs in os.walk(root):
    for f in fs:
        p = os.path.join(d, f)
        try: lines = open(p).read().split("\n")
        except UnicodeDecodeError: continue
        if lines and lines[-1] == "": lines = lines[:-1]
        texts[os.path.relpath(p, root)] = lines
testfiles = set()
for rel, lines in texts.items():
    if not (rel.startswith("src/") and rel.endswith(".rs")): continue
    _, mods = test_mask(lines)
    base = os.path.dirname(rel) if os.path.basename(rel) in ("lib.rs", "mod.rs") else rel[:-3]
    for m in mods:
        testfiles.add(os.path.join(base, m + ".rs")); testfiles.add(os.path.join(base, m, "mod.rs"))
src = []; allfns = []
for rel, lines in texts.items():
    if not (rel.startswith("src/") and rel.endswith(".rs")): continue
    name = os.path.basename(rel)
    if rel in testfiles or name.endswith("_tests.rs") or name in ("tests.rs", "test_util.rs"): continue
    mask, _ = test_mask(lines)
    naive = next((k for k, l in enumerate(lines) if l.strip().startswith("#[cfg(test)]")), len(lines))
    prod = sum(1 for k in range(len(lines)) if not mask[k])
    src.append((rel, len(lines), prod, naive))
    for n, fname, ln in fns([("" if mask[k] else l) for k, l in enumerate(lines)], len(lines)):
        allfns.append((n, fname, rel, ln))
tot_lines = sum(len(l) for l in texts.values())
print(f"files (whole package): {len(texts)}   lines (whole package): {tot_lines}")
print(f"src non-test .rs files: {len(src)}   their lines: {sum(f[1] for f in src)}   production lines: {sum(f[2] for f in src)}   (naive, to first #[cfg(test)]: {sum(f[3] for f in src)})")
print(f"test-only src files excluded: {sum(1 for r in texts if r.startswith('src/') and r.endswith('.rs')) - len(src)}")
print(f"src files >= 500 production lines: {sum(1 for f in src if f[2] >= 500)}   >= 1000: {sum(1 for f in src if f[2] >= 1000)}")
print("largest 15 src files by production lines (naive count in brackets where it differs):")
for rel, n, p, nv in sorted(src, key=lambda f: -f[2])[:15]:
    print(f"  {p:6d}  {rel}" + (f"  [{nv}]" if nv != p else ""))
print("longest 10 production fns (fn line to closing brace):")
for n, fname, rel, ln in sorted(allfns, key=lambda t: -t[0])[:10]: print(f"  {n:6d}  {fname}  {rel}:{ln}")
print(f"production fns over 150 lines: {sum(1 for t in allfns if t[0] > 150)}")
```

</details>

## Decisions & trade-offs

- **Two PRs** (developer, 2026-09-23): this restructure first, the final split above it.
- **Destructure before splitting, and resolve code issues first** (developer).
- **DRY, including across crates** (developer). The consumer edits this needs (`tddy-coder`,
  `tddy-sandbox-runner`) are accepted as the exception.
- **Dedupe by extract-method with explicit parameters, not by a new abstraction.** This is the lowest
  behaviour risk.
- **The resize decoder goes to `tddy-pty`, not `tddy-terminal-rpc`.** `tddy-sandbox-runner` runs inside
  every jail, so the lightest possible crate is the one it should depend on.
- **Fix the `05` refusal here, not on #527 or #529** (developer, 2026-09-24). The engine defect that
  refused `05` is fixed on this PR, relaxing the boundary against touching
  `tddy-code-restructuring`. It lands test-first: a live-harness fixture reproduces the mechanism
  with the production message. #529's fixture did not reproduce it, and the developer closes that
  PR.
- **Restore a `pub use` the engine narrows** (this PR, under the public-surface Boundary). It is a
  `use`-line edit, but its purpose is keeping the surface unchanged, not making the tree build. It
  is listed as such in `23b73d0b`.

## Refactoring needed

## Validation results

## TODO

- [x] Record initial discovery
- [x] Cross-check `docs/code-issues/` and `docs/dev/todo/` (Step 2b)
- [x] Create changeset — this document
- [x] Restructure plans proven with `check --deep`: 8 of 14 clean, 3 engine defects
- [ ] USER REVIEW — layout and DRY inventory
- [x] Baseline (characterisation tests only where a seam needs one)
- [ ] Implementation: plans `03`, `10a`, `04`, `08`, `06`, `07`, `02`, `09b`, `05`, `01`, `10b`, `09c`, `11`–`18`, `20` and `21` applied; DRY #2–#13 done; the consent list is open
- [ ] `/validate-changes`
- [ ] `/pr-wrap`
- [ ] Wrap documentation (`/wrap-context-docs`)

## Final Checklist

Executed at wrap:

- [x] Every non-test `src/` file in lifecycle < 500 production lines (inline-test-block rule): largest 493
- [ ] No function in lifecycle > 150 lines
- [ ] Every DRY inventory row has one definition left; the per-seam sweep's additions are recorded
- [ ] All 16 code-issue records deleted or narrowed, with final measurements in the change-history entry. ✅ done (`3e05735c`); the change-history entry is written at wrap
- [ ] The ✅ RESOLVED HERE backlog entry (2026-08-13 trim-to-option) deleted
- [ ] Public `tddy_session_lifecycle::…` surface unchanged; consumer edits limited to the listed exceptions
- [x] Baseline numbers matched: 61 / 622 / 22 / 1 after every milestone, the same 22
