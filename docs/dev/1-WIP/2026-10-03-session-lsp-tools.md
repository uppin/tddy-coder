# Changeset: Session LSP tools answered by the warm index

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-session-lsp-tools-initial-discovery.md).

## Stack

`#live-plan` 11/15 — branch `feature/live-plan/session-lsp-tools`, base `master` (its parents #539, #574, #569 and #566 have merged; the PR was re-based onto `master` on 2026-10-04).
PR: [#570](https://github.com/uppin/tddy-coder/pull/570)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Host-side `Lsp*` execution against `IndexDaemonRegistry::connect` + the session's own worktree, bound on the host.
- `Symbols` / `Diagnostics` index RPCs.
- Unchanged tool names, schemas, results; unchanged behaviour without `index_daemon:`.

## Boundaries

- Does **not** add restructure tools (`session-restructure-tools`).
- Does **not** change the advertised tool set or the in-jail allowlist.

## Dependencies

What each parent PR delivers that this PR consumes. These surfaces are **theirs to create**;
implementing one here collides with the PR that owns it.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `code-navigation` (10) | `Definition/References/Hover` index RPCs; the daemon's `connect`-based forwarding helper | the tools call the same RPCs through the same helper | add navigation RPCs or a second forwarding path |

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

**Sequencing facts found while writing the contract:**

- The executor tests (`tddy-lsp-executor/tests/lsp_tools_via_index.rs`) inject a fake `code_index`
  server, so they go green on this node's own work alone — they do **not** wait for
  code-navigation's `navigation.rs`.
- What does wait for code-navigation: the end-to-end path (a real index answering `Definition` /
  `References` / `Hover` for a session) and, by reuse, the position/location translation
  code-navigation writes in the index daemon (`navigation.rs`'s zero-based ↔ one-based byte mapping
  and root-relative paths). `symbols.rs` here should reuse that mapping rather than restate it, so
  green this node's `Symbols` / `Diagnostics` after code-navigation is green.
- Adding `Symbols` / `Diagnostics` to `CodeIndexService` extended the trait, so code-navigation's
  fake index in `tddy-daemon/tests/code_navigation_acceptance.rs` gained two
  `Err(not_part_of_this_fake())` methods — the only edit to a parent's file.

## Draft PR contract

The first push after this commit publishes:

- `code_index.proto`: `Symbols(SymbolsRequest) → SymbolsResponse` and
  `Diagnostics(DiagnosticsRequest) → DiagnosticsResponse`, with `CodeSymbol` and `CodeDiagnostic`
  (one-based byte `SourceRange`, `CodeLocation` reused). Served by
  `tddy-index-daemon/src/symbols.rs` (`serve_symbols`, `serve_diagnostics`), answering
  `Unimplemented … TODO(session-lsp-tools)`.
- `tddy_lsp_executor::index_backed` (**not** `tddy_tool_engine::lsp_via_index` as first drafted —
  see Decisions): `IndexChannel` (async port: `connect() -> tonic Channel`), `IndexLspExecutor`
  implementing `LspExecutor` (every method `TODO(session-lsp-tools)`),
  `bind_to_session_worktree(worktree, file) -> Result<String, String>` (stub), and
  `select_lsp_executor(Option<Arc<dyn IndexChannel>>, existing) -> Arc<dyn LspExecutor>` (real: a
  four-line deployment switch).
- `tddy-daemon`: `impl IndexChannel for IndexDaemonRegistry` (`index_daemon/lsp_channel.rs`,
  delegating to `connect`); the selection hook in `runtime.rs` is a `TODO(session-lsp-tools)`
  comment, so today's behaviour is unchanged.
- Failing tests below.

## Green wave

**Wave:** 2 of 3
**Greenable independently:** no — its executor calls `code-navigation`'s RPCs; greenable once code-navigation is green.
**Concurrent with:** transactional-groups, indexing-indicators
**Blocks:** session-restructure-tools

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/indexing-indicators` — next in the line.

## Prerequisites

### ℹ REFERENCE — a jail can name another session's worktree over the host bridge — [`2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md`](../todo/2026-10-01-a-jail-can-name-another-sessions-conversation-worktree-over-the-host-bridge.md)

This node binds its own tools' worktree host-side and so does not widen that gap; it does not fix the conversation route the entry describes.

## Affected Packages

- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — `Symbols`, `Diagnostics` RPCs
- **tddy-tool-engine**: unchanged — its `Lsp*` dispatch already asks whatever executor the host registered
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — `IndexDaemonRegistry` as the executor's `IndexChannel`; selects the index-backed executor when `index_daemon:` is set
- **tddy-lsp-executor**: [docs](../../../packages/tddy-lsp-executor/docs/) — `index_backed::IndexLspExecutor`, the host-side worktree binding, the executor selection
- **tddy-session-lifecycle**: unchanged — the host already hands the session's worktree to the tool engine (see Decisions)

## Related Feature Documentation

- [PRD](../../ft/coder/1-WIP/PRD-2026-10-03-session-lsp-tools.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

`Lsp*` exec tools (gated by `TDDY_LSP_TOOLS`) run on the host through `tddy_lsp_executor` (its own rust-analyzer per `BUILD.yaml` target) via `tddy_tool_engine::execute_tool_with_env`; nothing in sessions uses the daemon-managed index; the host bridge trusts jail-supplied session ids (todo 2026-10-01).

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

### tddy-lsp-executor — `tests/lsp_tools_via_index.rs`

Fake `code_index` server on an AF_UNIX socket behind a test `IndexChannel`; a recording stand-in
for today's executor; every call through `select_lsp_executor` on a blocking thread, as
`tddy_tool_engine`'s `Lsp*` dispatch runs it.

- `lsp_definition_is_answered_by_the_index_for_the_session_worktree` — ❌ fails: `IndexLspExecutor::definition` stub
- `a_path_outside_the_session_worktree_is_refused_on_the_host` — ❌ fails: stub's message is not the refusal
- `lsp_symbols_and_diagnostics_are_answered_by_the_index` — ❌ fails: `symbols` / `diagnostics` stubs
- `without_an_index_daemon_the_existing_executor_answers` — ✅ passes by design (guard on `select_lsp_executor`)

### tddy-lsp-executor — `src/index_backed.rs` unit tests (`bind_to_session_worktree`)

- `a_relative_file_inside_the_worktree_is_bound_to_it` — ❌ stub
- `an_absolute_file_inside_the_worktree_is_made_relative_to_it` — ❌ stub
- `a_file_that_climbs_out_of_the_worktree_is_refused` — ❌ stub
- `an_absolute_file_in_another_worktree_is_refused` — ❌ stub

### tddy-index-daemon — `tests/code_index_service_acceptance.rs` (§ Symbols and diagnostics)

- `symbols_lists_the_symbols_of_a_file_relative_to_the_root` — ❌ `Unimplemented … TODO(session-lsp-tools)`
- `diagnostics_reports_what_the_server_finds_wrong_with_a_file` — ❌ same

### tddy-tools — `tests/mcp_tool_advertisement_audit.rs` (existing, must stay green)

- `advertises_forty_four_tool_names_on_the_daemon_path_which_serves_no_action_tool` — ✅
- `advertises_all_forty_seven_tool_names_where_the_host_serves_the_action_tools` — ✅
- `withholds_exactly_the_three_action_tools_where_the_host_does_not_claim_them` — ✅

(The planned name `the_lsp_tool_names_and_schemas_are_unchanged` does not exist; the three above are
what pins the advertised set.)

## Technical Debt & Production Readiness

**Stubs:** none left. `Symbols` / `Diagnostics` are served by `tddy-index-daemon/src/symbols.rs`, the
`IndexLspExecutor` methods and `bind_to_session_worktree` by `tddy-lsp-executor/src/index_backed.rs`,
and `tddy-daemon/src/runtime.rs` registers `select_lsp_executor` (with the registry as the channel when
`index_daemon:` is set) in place of today's `tddy_lsp_executor::register` call; the existing executor is
still built there, and its registry still drives the idle reaper.

**Open, not written as tests:**

- `ReadLints` (`workspace_diagnostics`) has no index RPC — `Diagnostics` is per file. **Decided: a
  refusal** naming `LspDiagnostics` as the alternative, not a fall back to the local executor (a second
  server would disagree with the index). Pinned by
  `read_lints_is_refused_through_the_index_and_not_answered_by_the_existing_executor`, which also
  asserts the existing executor is not asked. TODO: a workspace-wide `Diagnostics` (empty `file`)
  would lift it.
- `LspReferences` / `LspHover` through the index are not tested here: they are the same shape as
  `LspDefinition` over code-navigation's RPCs, and adding them would only grow the fake.
- `is_available` for the index-backed executor (it gates the tools' exposure) is not pinned: true when the worktree root holds a `Cargo.toml`.
- No daemon-level test proves the hook selects the index executor: the process-global
  `register_lsp_executor` is first-wins per process, and the runtime registers before any test can
  observe it. `select_lsp_executor` is the tested seam.
- The worktree binding (`bind_to_session_worktree`) and the index's `file_within` are both lexical
  (`..` and absolute paths are refused), so a symlink inside the worktree that points outside it is
  followed by neither. Not pinned and not closed here.
- UTF-16 ↔ byte column conversion is implemented by reading the line from disk (inputs and answers) but pinned only on ASCII lines.

**Dependency weight:** `tddy-lsp-executor` now depends on `tddy-index-daemon` (+ `tonic`,
`async-trait`), so `tddy-session-lifecycle`, `tddy-sandbox-app` and `tddy-tools` (already a
dependent) pull the index daemon's library — including `tddy-code-restructuring` — transitively. No
cycle. A thin `code_index` client crate would remove the weight; not done here.

## Decisions & Trade-offs

- **Executor home: `tddy-lsp-executor`, not `tddy-tool-engine`.** `tddy-tool-engine` is depended
  on by `tddy-sandbox-runner`, which runs inside every jail; putting a `tddy-index-daemon` client
  there would ship the restructure engine into the jail runner. `tddy-lsp-executor` already owns the
  concrete `LspExecutor`, is a dependency of the daemon (which registers it), and the index daemon
  does not depend on it.
- **`IndexChannel` port** instead of `IndexDaemonRegistry`: the registry lives in `tddy-daemon`,
  which depends on `tddy-lsp-executor`. The daemon implements the port by delegating to `connect`.
- **Host-side binding via `repo_dir`:** the worktree every `LspExecutor` method receives is
  already the host's — `execute_tool_with_env`'s `worktree_root`, resolved from the session (or the
  subagent conversation worktree under it). So `tddy-session-lifecycle` needs no change; the binding
  is that the queried `file` must lie inside it (`bind_to_session_worktree`).
- **`navigation.rs` (code-navigation's file) was edited, minimally.** `symbols.rs` reuses its path check
  and document sync rather than restating them, as the Dependencies section asks: `asked_document` is
  split into `served_source` (the path check) and `synced_document` (read, URI, `client_for`,
  `sync_document`), and `wire_position`, `code_location` and the two new helpers became `pub(crate)`.
  Check order and error messages are unchanged. 🆕 Not in the original plan, which named only the
  fake index in `code_navigation_acceptance.rs` as an edit to a parent's file.
- **`tddy_lsp_executor::register` is no longer called by the daemon.** It builds its own executor, so
  calling it after the selection would leave a second, unused one behind. The runtime builds one
  `TddyLspExecutor`, takes its registry for the idle reaper, and registers
  `select_lsp_executor(...)`'s result — the same three steps `register` performs.
- **Test file named `lsp_tools_via_index.rs`** (not `…_acceptance.rs`): fluent-tests naming —
  `acceptance` is not a scope marker.

## Refactoring Needed

### From @validate-changes (Change Validation)

- ✅ `ReadLints` refusal message carried ~24 stray spaces — fixed.
- ✅ The `ReadLints` refusal was untested — pinned (see Technical Debt).
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### /validate-changes — 2026-10-03

- **Stack gate:** base `feature/live-plan/transactional-groups`; current; `origin/<base>..HEAD` is this
  PR's commits only. No unplanned deletions; the diff holds only this PR's files.
- **Boundary:** nothing from `## Dependencies` implemented here (the Definition/References/Hover RPCs,
  `navigation.rs`'s mapping and the registry's `connect` are consumed). One edit to a parent's file,
  `navigation.rs` — a visibility change and an extract-function refactor, recorded under Decisions.
  Nothing from `## Boundaries` crept in; the advertised tool set is unchanged (audit green).
- **Responsibility:** delivered; no `TODO(session-lsp-tools)` stub remains. One deliberate refusal
  (`ReadLints`), with its own `TODO`.
- **Build:** `tddy-lsp-executor`, `tddy-index-daemon`, `tddy-daemon` build clean.
- **Findings:** 2 warnings (stray whitespace in a user-visible message; untested refusal) — both fixed.
  2 infos (lexical-only path binding; parent file edit) — recorded above.

### Scoped gates — 2026-10-03 (packages touched only: `tddy-lsp-executor`, `tddy-index-daemon`, `tddy-daemon`)

- `cargo fmt`: clean. `cargo clippy -p … --all-targets -- -D warnings`: clean.
- `./test -p tddy-lsp-executor -p tddy-index-daemon`: **146 passed, 0 failed** (incl. 5
  `lsp_tools_via_index`, 9 `index_backed` unit tests, 35 `code_index_service_acceptance`).
- `tddy-daemon` `code_navigation_acceptance` (7) and `tddy-tools` `mcp_tool_advertisement_audit` (3)
  passed in the implementer's run; not re-run here. Whole-workspace health is CI's.

### File length gate (step 3.5)

- 🔴 `packages/tddy-daemon/src/runtime.rs`: 1,636 → 1,650 production lines (+14). **Deferred with the
  developer's consent** — #571 and #573 touch the file. Recorded in
  `packages/tddy-daemon/docs/code-issues/oversized-file-runtime.md` and
  `docs/dev/todo/2026-10-03-session-lsp-tools-grew-runtime-rs.md`.
- Every other changed non-test file is under 500 (largest new: `index_backed.rs`, 385).

## TODO

- [x] Record initial discovery (`2026-10-03-session-lsp-tools-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [x] Update documentation with progress
- [x] Repeat Red→Green→Update cycle until feature complete
- [x] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
- [x] Validate changes (/validate-changes)
- [x] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [x] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-session-lsp-tools-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
