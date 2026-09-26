# 2026-09-26 — tddy-session-lifecycle's leaf topics move to their receivers; eight restructure-engine fixes

**Type:** Refactor

`#carve` 15/21, [#526](https://github.com/uppin/tddy-coder/pull/526), on top of #524 (merged). The
successors are [#531](https://github.com/uppin/tddy-coder/pull/531) (`#carve` 16) through
[#536](https://github.com/uppin/tddy-coder/pull/536) (`#carve` 21). Behaviour-preserving extraction,
so there is no PRD and no product changelog.

Packages: `tddy-session-lifecycle`, `tddy-daemon-kernel`, `tddy-terminal-rpc`,
`tddy-daemon-sandbox`, `tddy-session-activity`, `tddy-daemon-livekit`, `tddy-session-files`,
`tddy-session-agents` (the moves); `tddy-code-restructuring`, `tddy-lsp`, `tddy-lsp-executor`,
`tddy-index-daemon` (the engine fixes); `.agents/skills/code-restructuring/references/plan-schema.md`
and `.config/nextest.toml`.

## What was delivered

Every host-free topic of `tddy-session-lifecycle` moved to the crate below it that owns its subject,
by `tddy-tools restructure` (engine moves only; hand edits were post-move build corrections, each new
cause filed as a backlog entry). Lifecycle keeps a facade for every moved public path, so no consumer
crate (`tddy-daemon-rpc`, `tddy-daemon`, `tddy-telegram-control`) was edited.

| Move | Receiver | Commit |
|---|---|---|
| `task_service`, `action_service`, with `action_service_acceptance` and `action_sandbox_acceptance` | `tddy-daemon-sandbox` | `6b1e8235` |
| `relay_idle`, `local_token_tonic_adapter` | `tddy-daemon-kernel` (new edges `tddy-task`, `tddy-github`, `tonic`, approved) | `ba2eec55` |
| `pty_runtime`, `tddy_user_config` (8 inline tests with them) | `tddy-terminal-rpc` (new edge `tddy-daemon-kernel`; dev `tempfile`) | `a21c1dcc` |
| `session_reader`, `user_sessions_path`, `session_deletion`, `session_list_enrichment`, with `worktree_removal_eligibility` | `tddy-session-activity` (new edges `tddy-session-files`, `tddy-projects`, `chrono`, `libc`, `tddy-daemon-sandbox`, `tddy-daemon-livekit`; dev `tddy-workflow`, `tempfile`) | `15374089` |
| `peer_routing`, `session_admission_service` | `tddy-daemon-livekit` (no new edges) | `96531fcc` |
| `connection_service::attachment_progress` | `tddy-session-files` (no new edges) | `3b3237bd` |
| agent roster and clones, host-free parts: `clone_readiness`, `exec_tool_caller`, `agent_records`, and the `self`-free tails of eleven host methods, with the hand-written `AgentRosterState<'a>` view and `DaemonSessionHost::agent_roster_state()` | `tddy-session-agents` (new edges `tddy-model-registry`, `tddy-projects`, approved) | `205c0162` … `63fcd0ca` |

The T3 tails were moved by a three- or four-plan chain per method: an `extract_variable` hoisting a
host read ahead of the range where needed, `extract_method` over the tail, `extract_module` with
`to_file`, then `move_module_to_crate`. Six gray-zone substitutions (`self.<field>` →
`state.<field>`, token for token) across three methods; no callback trait was written, because no
body that moved calls one. The comment multiset of both crates was unchanged.

### Production lines

| Crate | Before | After |
|---|---:|---:|
| `tddy-session-lifecycle` | ~22.8k | **20,041** |
| `tddy-daemon-kernel` | 3,290 | 3,409 |
| `tddy-terminal-rpc` | 2,348 | 2,526 |
| `tddy-daemon-sandbox` | 2,536 | 3,239 |
| `tddy-session-activity` | 1,573 | 2,541 |
| `tddy-daemon-livekit` | 5,409 | 5,863 |
| `tddy-session-files` | 4,600 | 4,716 |
| `tddy-session-agents` | 3,592 | 4,132 |

Lifecycle's figures come from the counters the move runs used (the first read the base as 22,939,
later ones as 22,116; each run's before and after share one counter). By the inline-test-block rule
`module-layout.md` states, lifecycle went from 22,393 to 19,618 production lines and from 122 to 109
non-test files. Every receiver is under 10k, no moved file is at or over 500 production lines, and
no receiver depends on lifecycle, normal or dev.

### Baseline held by name

`cargo test -p tddy-session-lifecycle --no-fail-fast -- --test-threads=1 --skip
sandboxed_bash_pty_action_streams_output` (macOS; the skipped test does not finish there, and moved
to `tddy-daemon-sandbox` with its suite), after every move:
the same 22 failures by name throughout — 5 `sandbox_behavior_acceptance`, 5
`sandboxed_claude_cli_acceptance`, 4 `sandboxed_cursor_cli_acceptance`, 2
`sandboxed_session_lifecycle_acceptance` (the sandbox RPC bridge is never installed), 6
`session_sync_livekit_acceptance` (`tddy-remote-git-repo` not built). The passed count fell only by
the tests that moved with their code: 622 → 615 (7 to `tddy-daemon-sandbox`) → 607 (8 to
`tddy-terminal-rpc`) → **562 passed, 22 failed, 1 ignored** (45 to `tddy-session-activity`). Each
receiver's own tests passed after its move: kernel 122, terminal-rpc 63, activity 45, livekit 174,
session-files 160, session-agents 72, and `tddy-daemon-sandbox` 32 with 1 pre-existing failure (below)
and 1 ignored. `restructure verify --against HEAD` accounted for every
statement after every move. `cargo check --all-targets` on lifecycle, the receiver and the three
consumers, clippy `-D warnings` and `cargo fmt` were clean after every move.

Observed and left as they are, all pre-existing: on macOS, `tddy-daemon-sandbox`'s
`sandbox_stdio_seatbelt_acceptance` does not compile (3 × `E0425`, `SandboxHandle` not imported) and
`sandbox_session_stdio_acceptance::real_daemon_session_drives_a_seatbelt_jailed_sandbox_runner_entirely_over_stdio`
times out; `session_room_acceptance::the_first_connect_makes_the_sessions_terminal_drivable_over_livekit`
is flaky (it passes alone); `pty_runtime`'s unconditional re-export of two `#[cfg(unix)]` kernel items
means neither lifecycle before nor `tddy-terminal-rpc` now builds on a non-unix target.

## Eight engine fixes

Each was found by a move, fixed test-first, in its own commit; `tddy-code-restructuring`'s tests went
from 646 to 694 passed, 0 failed.

| Commit | Fix |
|---|---|
| `543125a6` | `tddy_lsp::registry::workspace_root_for` roots a path at the nearest Cargo workspace root, bounded by `.git`, instead of the outermost ancestor manifest — so the warm index daemon serves a nested worktree its own tree. An unreadable manifest is an error. Also `tddy-lsp-executor`, `tddy-index-daemon` |
| `3c8323e6` | `check`'s partial-cluster finding reports a moved module whose header names a module staying behind (the cycle `apply` refuses), not a caller left behind that a facade or re-point already serves |
| `5446cec6` | The readiness wait ends on code rust-analyzer reports `inactive-code`, instead of polling a hover that never answers; a caller survey takes the empty answer, an operation at such code is refused |
| `ebeb8282` | "Staying behind" is read at each operation's point in the plan, so `check` predicts the cycle `apply` refuses for a mutual set spread over separate moves |
| `cb367ac6` | An elided lifetime `'_` in an extracted signature is read as a type, not an untyped placeholder |
| `cc19d3a4` | `extract_variable` renames the binding rust-analyzer actually introduced (`backends/rust/introduced.rs`), not a `let var_name` it no longer writes |
| `ff73fcb6` | A `return` is allowed in an `extract_method` range that runs to the end of a named `fn`, ending with its tail expression |
| `841545dd` | A warm `tddy-index-daemon` sends its server `workspace/didChangeWatchedFiles` for what changed on disk between requests (`tree_changes.rs`); a file in no module tree (`unlinked-file`) is refused instead of waited on |

The plan-level rules are in `.agents/skills/code-restructuring/references/plan-schema.md`; the
package docs are `tddy-code-restructuring`'s `readiness-and-gates.md` and
`assist-output-repairs.md`, `tddy-index-daemon`'s `code-index-service.md` and `tddy-lsp`'s
`workspace-root.md`.

**Backlog entries these fixes closed**, each filed and deleted on this branch (the fix commit's
message is its record): `2026-09-25-restructure-warm-index-serves-the-main-checkout-for-a-nested-worktree`
(`543125a6`), `2026-09-25-restructure-extract-method-refuses-an-elided-lifetime-as-an-untyped-placeholder`
(`cb367ac6`), `2026-09-25-restructure-extract-variable-expects-a-var-name-placeholder` (`cc19d3a4`),
`2026-09-25-restructure-warm-check-hangs-on-a-module-an-earlier-apply-created` (`841545dd`).

**Backlog entries filed and still open** (15, in `docs/dev/todo/`, each with a code example):
`2026-09-25-restructure-check-misses-a-body-path-to-a-module-staying-behind`,
`…-check-misses-a-module-name-the-destination-already-has`,
`…-extract-method-accepts-a-return-before-a-unit-if-tail`,
`…-extract-method-leaves-a-function-local-use-behind`,
`…-extract-variable-hoists-a-borrowed-field-by-value`,
`…-extract-variable-waits-forever-on-a-range-opening-with-a-borrow`,
`…-glob-facade-re-exports-a-name-the-origin-shadows`,
`…-has-no-operation-to-read-a-methods-fields-through-a-state-parameter`,
`…-move-to-crate-follows-a-facade-back-to-the-destination`,
`…-move-to-crate-leaves-a-nested-modules-parent-glob-dangling`,
`…-move-to-crate-leaves-the-destinations-own-extern-name`,
`…-move-to-crate-misses-a-crate-named-only-in-a-body-path`,
`…-move-to-crate-reads-an-import-reaching-the-destination-as-an-edge`,
`…-move-to-crate-skips-the-use-lines-of-the-moved-files-test-module`,
`…-test-binary-move-cannot-see-through-a-glob-facade`. Their `plan` references point at the plans as
committed at `22787218`. The 2026-09-09 untested-complexity-hotspots entry was a ⚠ DURING
prerequisite and is unchanged.

## The rescope, and why

This node was planned to leave lifecycle a wiring crate (~3.4k production lines, the developer's
chosen target: the wiring floor of ~3.9k less `test_util` gated or moved to a testkit and
`service_util` moved down, with the `PeerRouted*` wrappers staying). On 2026-09-26 the developer
rescoped it to what it delivered:

- An inherent `impl DaemonSessionHost` cannot leave lifecycle (`E0116`), and the engine moves only
  what a host method does without `self`. Two T3 pilot runs moved eleven methods' tails for a net
  −185 lifecycle lines; the heads, and every method that calls another host method or hands
  `self.clone()` to a task, stayed. T4 (split) and T1 (launch) are almost entirely that orchestration.
- Turning host methods into receiver-shaped functions over per-topic state and callback ports is a
  **restructure**, which this node's boundaries excluded. It became its own nodes, converted in place
  and guarded by the baseline, followed by plain engine moves.

Deferred to #531–#536: the demo VM, presenter observation (T10), the host-bound remainders of T3, T7
and T8, split sessions (T4), the launch paths (T1), the eight cross-topic cycle cuts the code turned
out to need, the per-topic state structs and callback traits, the `test_util`/`service_util` extras
and the wiring size target. Neither `tddy-agent-launch` nor `tddy-session-split` was created here, so
`/analyze-code-issues` on them does not apply.

Plan premises that did not hold, for the successors: `DemoVmState`, `AdmissionState`/`OsUserResolver`
and `AttachmentState` did not exist (and activity already exports an unrelated `OsUserResolver`);
T10 is not a leaf (`SessionNotificationPublishing` lives only in lifecycle's `session_notifications`);
T5a belongs in `tddy-session-activity`, not `tddy-session-catalog` (85 packages added to
`tddy-bsp`'s graph otherwise); `svc_resolve_os_user.rs` mixes T3, T7 and T8; `session_dir_for` and
`ensure_session_room` were already split; `worktree_snapshot` is already a port; the
`AgentHostCallbacks` list lacks seven host methods T3's bodies call. The 64 committed restructure
plans are in the history: `git ls-tree -r --name-only 22787218 docs/dev/1-WIP/` lists them.

## Code issues reconciled

- **Deleted, closed:** `tddy-code-restructuring/docs/code-issues/refusal-move-module-to-crate-any-caller-left-behind.md`
  (`move_module_to_crate` refused whenever any file left behind named the moved module; measured on
  `#carve` 5/11 at 19 findings over four modules). Closed by `3c8323e6`: a module staying behind that
  names the moved one is no longer a finding. Final measurement on this branch: plan `01a`'s plain
  `check` went from 3 findings (`connection_service` ×3 "stays behind … and names `relay_idle`") to
  none, and plan `02a`'s from 2 (`pty_handle`, `pty_spawn` naming `pty_runtime`) to none; both then
  applied.
- **Re-measured unchanged** (fn line to closing brace, identical at `2688227f` and `22787218`):
  lifecycle's `spawn_cursor_cli_session_inner` 244, `handle_rpc` 147, `start_split_claude_cli_session`
  146, `delete_paired_codebase_session` 95, `ensure_project_available_for_start` 99 (its file touched,
  the function not), `resume_claude_cli_session` 119, `resume_session_at_session_coordinate` 137,
  `spawn_split_agent` 110, `start_sandboxed_claude_cli_session` 342, `start_session_core` 358,
  `start_sandboxed_cursor_cli_session` 414 lines; `tddy-code-restructuring`'s `facade_lines` 47.
- **Updated:** `tddy-code-restructuring`'s `oversized-file-backends-rust`, regressed 4,342 → 4,360
  production lines on the record's scale (+5 `5446cec6`, +13 `cb367ac6`; this wrap's
  re-implementation of the inline-test-block rule reads 4,340 → 4,358); `tddy-daemon-kernel`'s `misplaced-tests-privilege-drop` and
  `missing-tests-privilege-drop-resolve-pty-os-user` (the tests and the call site moved with
  `pty_runtime` to `tddy-terminal-rpc`) and `heavy-dependency-livekit-peer-forwarding` (1 SDK module
  of 15, 17 dependents, `tddy-terminal-rpc` now among them).
- **Not recorded, over budget:** `tddy-code-restructuring/src/crate_move/cluster.rs` went from 493 to
  611 production lines (+56 `3c8323e6`, +62 `ebeb8282`).
