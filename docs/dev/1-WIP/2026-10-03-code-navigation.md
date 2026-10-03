# Changeset: Code navigation in the session code explorer

**Date**: 2026-10-03
**Status**: 🚧 In Progress
**Type**: Feature

## Initial Discovery

Full codebase exploration that grounded this plan:
[initial-discovery.md](./2026-10-03-code-navigation-initial-discovery.md).

## Stack

`#live-plan` 8/15 — branch `feature/live-plan/code-navigation`, base `feature/live-plan/live-plans`.
PR: [#574](https://github.com/uppin/tddy-coder/pull/574)

**Position.** Appended after #539, in green-wave order: wave 1 #539, code-navigation, signature-assists · wave 2 transactional-groups, session-lsp-tools, indexing-indicators · wave 3 plan-dialog, session-restructure-tools, signature-rewrites; inside each wave the node with the most transitive dependents leads.

## Responsibility

- Index-daemon navigation RPCs over `WorkspaceIndex::client_for` + `LspClient` (1-based byte coordinates on the wire).
- `code_navigation.CodeNavigationService` in tddy-daemon: authorise like `WorktreeService`, map to the session worktree root, forward through `IndexDaemonRegistry::connect`; `FailedPrecondition` without `index_daemon:`.
- Web: position-aware `CodeBlock`, ctrl/cmd-click to definition, hover popover, references list.

## Boundaries

- Does **not** touch the agent `Lsp*` tools (`session-lsp-tools`).
- Does **not** add plan or warm-progress calls to the service (`plan-dialog`, `indexing-indicators`).
- No fallback to `tddy_lsp_executor` when the index is unavailable.

## Dependencies

None in behaviour. It sits after the nodes below it only because a registered stack is a line;
nothing they deliver is consumed, and its tests use no surface of theirs.

Every node below it in the line that is not in the table is **not consumed** — do not touch its surfaces.

## Draft PR contract

The first push after this commit publishes: `code_index.proto` `Definition/References/Hover` + messages; `tddy-service/proto/code_navigation.proto` service + messages and its web codegen; daemon `CodeNavigationServiceImpl` answering `unimplemented` with `TODO(code-navigation)`; web `CodeBlock` `onNavigate`/`onHover` props; failing tests below.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — index-daemon tests use the fake language server; daemon tests inject a fake index channel; Cypress mounts the pane over an in-memory RPC backend.
**Concurrent with:** #539 live-plans, signature-assists
**Blocks:** session-lsp-tools, indexing-indicators, plan-dialog, session-restructure-tools

Real dependency edges, as opposed to the branch line:

    live-plans (#539) → transactional-groups → signature-rewrites
    live-plans, transactional-groups → plan-dialog, session-restructure-tools
    code-navigation → session-lsp-tools → session-restructure-tools
    code-navigation → indexing-indicators, plan-dialog
    signature-assists: none            (item-anchors, plan-store and the move/extraction fixes have merged)

## Successor PRs

- `feature/live-plan/signature-assists` — next in the line.

## Prerequisites

### ✅ RESOLVED HERE — `IndexDaemonRegistry::connect` has no caller — [`2026-09-16-indexdaemonregistry-connect-has-no-caller.md`](../todo/2026-09-16-indexdaemonregistry-connect-has-no-caller.md)

The web is the first consumer; this node's acceptance test covers `connect`'s success leg. Its wrap deletes the entry.

## Affected Packages

- **tddy-index-daemon**: [README.md](../../../packages/tddy-index-daemon/README.md) — `Definition`, `References`, `Hover` RPCs
- **tddy-service**: [README.md](../../../packages/tddy-service/README.md) — `code_navigation.proto` (web-facing)
- **tddy-daemon**: [README.md](../../../packages/tddy-daemon/README.md) — `CodeNavigationService` handler, authorisation, first `IndexDaemonRegistry::connect` caller
- **tddy-worktree-service**: [README.md](../../../packages/tddy-worktree-service/README.md) — the listed-worktree authorisation reused
- **tddy-web**: [README.md](../../../packages/tddy-web/README.md) — navigable `CodeBlock`, hover popover, references list

## Related Feature Documentation

- [PRD](../../ft/web/1-WIP/PRD-2026-10-03-code-navigation.md)

## Summary

See the PRD's Summary; this changeset carries the technical delta and the stack contract.

## Technical Changes

### State A (Current)

`WorktreeCodePane` = `WorktreeFileTree` + read-only `CodeBlock` (PrismLight, no positions) over `worktree.WorktreeService`, authorised by `resolve_listed_worktree` in `tddy-worktree-service`; `CodeIndexService` has no navigation RPCs; `LspClient::{definition,references,hover}` exist in `tddy-lsp`; `IndexDaemonRegistry::connect` has no production caller; no `code_index` TS bindings; nothing proxies the index to the web.

### State B (Target)

As in `## Responsibility` and the PRD's Proposed Changes.

## Acceptance Tests

Written and run in the draft-PR contract commit; every one fails on a `TODO(code-navigation)` stub.

### tddy-index-daemon — `packages/tddy-index-daemon/tests/code_index_service_acceptance.rs` (§ Navigation)

Dispatched at the registered coordinate over `fake_lsp` started with the new
`--answers-in-its-workspace` flag (`packages/tddy-lsp/tests/bin/fake_lsp.rs`), which reports its
canned definition/reference locations under the client's `rootUri` instead of `file:///workspace`.

- `definition_returns_the_callee_location_in_the_worktree` — `src/lib.rs` 11:1–11:4, root-relative, one-based bytes
- `references_returns_every_reference` — `src/lib.rs` 11:1–11:4 and `src/main.rs` 21:5–21:8
- `hover_returns_the_type_text` — `markdown: Some("fn foo() -> u32")`

Each fails today with `Unimplemented: "<Method> is not served yet — TODO(code-navigation)"`
(`src/navigation.rs`).

### tddy-daemon — `packages/tddy-daemon/tests/code_navigation_acceptance.rs`

The registry is the production `IndexDaemonRegistry` over a stand-in program (the
`index_daemon_lifecycle_acceptance.rs` pattern) that symlinks the socket it is told to bind to a
fake `code_index` server the test hosts over tonic; the fake records every `DefinitionRequest`.

- `a_definition_request_is_forwarded_to_the_index_daemon_for_the_session_worktree` — fake asked once with `workspace_root` = listed worktree, `file` = `rel_path`; answer mapped to `CodeLocation{rel_path, range, outside_worktree}`
- `a_worktree_not_listed_for_the_project_is_refused` — `FailedPrecondition` "worktree_path is not a worktree of this project", fake never asked, registry never started
- `without_an_index_daemon_section_the_service_answers_failed_precondition` — `FailedPrecondition` naming `index_daemon`
- `the_first_request_starts_the_index_daemon` — stand-in argv `--grpc-uds <socket>`, `registry.running()` holds that socket

Each fails today on `Unimplemented: "Definition is not served yet — TODO(code-navigation)"`
(`src/code_navigation.rs`). `tests/test_placement.rs` `BELONGS_HERE` gained the suite.

### tddy-web — `packages/tddy-web/cypress/component/WorktreeCodePaneNavigation.cy.tsx` (`mountWithRpc` + in-memory backend)

Mounted through `SessionsDrawerScreen` → Code pane → `src/main.rs`; navigation answered by
`CodeNavigationService` handlers on the in-memory backend.

- "ctrl-click on an identifier opens the definition scrolled to its line" — `src/geometry.rs` line 120 visible and `data-navigation-target="true"`; recorded `Definition` request carries token, project, worktree, `src/main.rs`, position 4:26
- "hovering an identifier shows its type" — hover card shows `fn area(width: u32, height: u32) -> u32`
- "the references list navigates to a reference" — hover card → References action → list → `src/geometry.rs:120` opens at that line

All three fail today at `worktree-code-identifier-4-26` never rendering (`CodeBlock` TODO).

## Technical Debt & Production Readiness

Every `TODO(code-navigation)` stub the contract commit left is resolved: the index-daemon handlers
(`navigation.rs`), `CodeNavigationServiceImpl` (`code_navigation.rs`, `tddy-index-daemon` now a
`[dependencies]` entry), `createCodeNavigationApi`, `CodeBlock` positions and the pane / drawer
wiring. Still open:

- **Navigation is offered on every highlighted language**, but the index daemon announces every
  document to its one language server as `rust` (`navigation.rs` `LANGUAGE_ID`). Either the web gates
  `onNavigate`/`onHover` on `.rs` or the index daemon refuses other extensions — a decision for the
  developer; no fallback has been added.
- **The outgoing `file://` URI is not percent-encoded** (`navigation.rs` `asked_document`), while
  answered URIs are decoded — a worktree path with a space or `#` is announced wrongly.
- The hover card is a fixed card at the bottom of the preview, not anchored to the identifier.
- Hover markdown renders as a pre-wrapped `<pre>`: `renderSimpleMarkdown` has no code-fence support,
  so rust-analyzer's fences show as raw ```.
- No web unit test for the meta-click handler (the spec pins ctrl-click).

Contract deviations from the plan:

- `WorktreeServiceImpl::resolve_listed_worktree` was private; it is now `pub` so the daemon reuses the exact authorisation instead of copying it.
- The web-facing `CodeLocation` names its fields `rel_path` / `outside_worktree` (worktree vocabulary); the index's `CodeLocation` keeps `file` / `outside_root`.
- `code_navigation.proto` is built with an RPC-server pass only (no tonic adapter), like `session_files.proto`: nothing reaches it over the local gRPC socket.
- The index daemon does no UTF-16 conversion: the LSP client negotiates `positionEncoding: utf-8` and refuses a server that does not, so the wire column is `character + 1`.
- `WorktreeCodePaneNavigation.cy.tsx` `beforeEach` sets the token inside `cy.then(...)`: the queued `cy.clearLocalStorage()` otherwise ran after the synchronous `setItem` and emptied it. No assertion changed.

## Decisions & Trade-offs

_(populated during development)_

## Refactoring Needed

### From @validate-changes (Change Validation)
### From @validate-tests (Test Quality)
### From @prod-ready (Production Readiness)
### From @analyze-clean-code (Code Quality)

## Validation Results

### /validate-changes (2026-10-03)

Stack gate: base `feature/live-plan/live-plans`, already current, `origin/<base>..HEAD` is this PR's five commits only. Critical 0 · Warning 2 · Info 3.

- `cargo build -p tddy-index-daemon -p tddy-daemon -p tddy-service -p tddy-worktree-service`: ✅ clean, no warnings.
- Responsibility delivered (index RPCs, daemon service, web navigation); `## Dependencies` none; `## Boundaries` respected (no `Lsp*` agent tools, no plan/warm-progress calls, no `tddy_lsp_executor` fallback); no deletions.
- ⚠️ `navigation.rs` announces every document as `rust`; the web offers navigation for every highlighted language (see Technical Debt).
- ⚠️ `navigation.rs` builds the outgoing `file://` URI without percent-encoding.
- ℹ️ `CodeBlock` wraps every identifier in a span even when no handler is given (DOM weight on large plain previews).
- ℹ️ `code_navigation.rs` `root.display().to_string()` is lossy for non-UTF-8 worktree paths.
- ℹ️ Pre-existing failures in `tddy-index-daemon`, reproduced with `navigation.rs` reverted: `warming_a_root_forwards…`, `exits_non_zero_when_the_tree_no_longer_holds_against_the_ref`, `a_test_binary_move_in_plan_a_moves_plan_bs_file_hint`.

## TODO

- [x] Record initial discovery (`2026-10-03-code-navigation-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail)
- [x] USER REVIEW — acceptance tests (developer asked for the red phase across the whole stack without per-node stops; reviewed with the stack summary)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run scoped tests (`./test -p <pkg>` per affected package); CI for the rest
- [ ] Validate changes (/validate-changes)
- [ ] Refactor issues from change validation
- [ ] USER REVIEW — development complete
- [ ] Validate tests (/validate-tests)
- [ ] Refactor test issues
- [ ] Validate production readiness (/validate-prod-ready)
- [ ] Refactor production readiness issues
- [ ] Analyze code quality (/analyze-clean-code)
- [ ] Refactor code quality issues
- [ ] Final validation (/validate-changes)
- [ ] Linting and formatting (`cargo clippy -p <pkg> -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-10-03-code-navigation-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
