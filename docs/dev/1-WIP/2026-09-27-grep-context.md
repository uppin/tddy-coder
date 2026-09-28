# Changeset — Grep context lines (`grep-context`)

**Date:** 2026-09-27
**Status:** 🚧 In Progress
**Type:** Feature
**Stack:** `subagent-control` node 2 of 5 (`(#subagent-control 2/5)`), wave 1. **PR:** [#554](https://github.com/uppin/tddy-coder/pull/554) (draft).
**Branch:** `feature/subagent-control/grep-context` · **Base:** `feature/subagent-control/tool-previews`

**PRD:** [`2026-09-27-grep-context-prd.md`](2026-09-27-grep-context-prd.md)
**Initial discovery:** [`2026-09-27-grep-context-initial-discovery.md`](2026-09-27-grep-context-initial-discovery.md)

## Prerequisites

- ⚠ **During** —
  [glob-and-grep-cannot-be-paged.md](../todo/2026-09-27-glob-and-grep-cannot-be-paged.md):
  `Grep` has no paging (`offset`), and context lines make each truncated result *bigger*. This PR
  adds only `before`/`after`; it does not add `offset`, and it keeps the window counting matches
  so the truncation semantics do not change. The entry stays open.
- ⚠ **During** —
  [grep-is-unreachable-inside-every-jail.md](../todo/2026-09-26-grep-is-unreachable-inside-every-jail.md):
  the jail PATH fix (#549) landed; this PR does not touch the jail environment, only the engine's
  argument handling.
- ⚠ **During** — open PR #548 (`sandbox-tool-specs`, requirements docs for a jail with a declared
  tool set): its follow-up will reshape the engine catalog this PR also edits. Overlap is the
  `ToolDef` schema entry only; expected to be additive on both sides.
- ⚠ **During** —
  [`oversized-file-lib.md`](../../../packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md)
  and
  [`oversized-file-subagent.md`](../../../packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md):
  the Local-path context handling goes in a new module
  (`packages/tddy-discovery/src/subagent/grep_context.rs` or similar), not as growth of the
  oversized files. The engine's `tool_grep` edit is in-place and small; `subagent.rs` takes only
  minimal argument-threading.

## Affected packages

- [`tddy-tool-engine`](../../../packages/tddy-tool-engine/README.md) — `tool_grep` flags + context folding, catalog schema
- [`tddy-discovery`](../../../packages/tddy-discovery/README.md) — `grep_limited` (Managed + Local paths), tool schema, argument validation

## Responsibility

- `before`/`after` on `Grep` end to end: engine (rg flags + context-event folding), catalog
  schema, discovery `grep_limited` Managed forwarding and Local implementation, subagent-facing
  schema, argument bounds.

## Boundaries

- No `offset`/paging for Grep.
- No `Glob`/`Read` changes.
- No result-summary changes (node 1 owns `resultSummary`).
- No jail/environment changes.

## Dependencies

None — every surface it touches exists on `master`. Its base position after `tool-previews` in
the line is order, not a behavioural dependency: it consumes nothing node 1 delivers.

| Parent node | What it delivers | How this PR consumes it | This PR does NOT |
|---|---|---|---|
| `n1` tool-previews | `resultSummary` extraction + `MessageDescriptor` widening | nothing — order only, no consumed surface | touch `result_summary.rs`, `MessageDescriptor`, or `prompt_outcome_json` |

## Draft PR contract

First push (wave 2, commit 2): the widened `Grep` argument/validation surface —
`validate_tool_arguments` bounds for `before`/`after`, the catalog `ToolDef` schema, and the
result-entry shape types (a new small `grep_context` module holding the context-entry types the
engine and the Local path both produce) — plus failing acceptance tests pinning the shapes.

## Green wave

**Wave:** 1 of 3
**Greenable independently:** yes — tests exercise the engine path and the Local path directly;
no other node's behaviour is needed.
**Concurrent with:** the `tool-previews` and `agent-usage-notes` nodes.
**Blocks:** nothing.

Real dependency edges, as opposed to the branch line:

    tool-previews → yield-conditions → resume-replacement      (grep-context: no edges)

## State A

`tool_grep` runs `rg --json -e <pattern> .` and keeps only `type:"match"` events; each match is
ripgrep's own JSON event. `grep_limited` forwards `{pattern, path, limit}` to the engine
(Managed) or runs `grep_file`/`grep_dir` locally; both produce
`{matches, truncated, total_matches}`.

## State B

`Grep` accepts optional `before`/`after` (0–50 each). The engine passes `-B`/`-A` to rg and folds
`type:"context"` events into their adjacent match entries; the Local path computes the same
context windows from the file's lines. Each match entry gains a bounded `context` list; the
window keeps counting matches.

## Delta

- `tddy-tool-engine`: `tool_grep` — flags, context-event folding, catalog `ToolDef` schema +
  description.
- `tddy-discovery`: `grep_limited` signature gains context args; new `subagent/grep_context.rs`
  for the Local path + shared entry types; `dispatch_tool_call`'s GREP arm threads the args;
  `validate_tool_arguments` bounds; subagent tool schema.

## Implementation milestones

- [ ] Shared context-entry types + Local-path context windows
- [ ] Engine `tool_grep` flags + event folding
- [ ] Catalog + subagent schemas, argument bounds
- [ ] Round-trip parity between Local and Managed shapes

## Testing plan

Unit + integration:

- `packages/tddy-tool-engine/tests/grep_context_acceptance.rs` — engine path over a fixture
  tree: context returned per match, edge clamping, no-context unchanged, caps count matches.
- `packages/tddy-discovery/tests/grep_context_local_acceptance.rs` — the Local path produces the
  same shapes; parity with the engine's.
- Argument-validation unit tests (bounds, negative rejection) beside the existing
  `tool_arguments` tests.

## Acceptance tests

1. `a_grep_with_before_and_after_returns_context_lines_per_match` —
   `packages/tddy-tool-engine/tests/grep_context_acceptance.rs`
2. `context_lines_are_clamped_at_file_edges` — same file
3. `a_grep_without_context_arguments_returns_its_unchanged_shape` — same file
4. `the_local_grep_path_returns_the_same_context_shapes` —
   `packages/tddy-discovery/tests/grep_context_local_acceptance.rs`
5. `the_match_window_counts_matches_not_context_lines` — either file, asserted on both paths

## Technical debt & production readiness

(populated during development)

## Decisions & trade-offs

- `before`/`after` as two args (not one `context`), so each side can be requested alone — matches
  the user's phrasing and ripgrep's own flag model.
- Context folded into the match entry rather than a flat line list — keeps the window counting
  matches and makes each entry self-contained.

## Refactoring needed

(none planned; new-module constraint above)

## Validation results

(to be filled by `/validate-changes`)

## TODO

- [x] Record initial discovery (`2026-09-27-grep-context-initial-discovery.md`)
- [x] Cross-check `packages/*/docs/code-issues/` and `docs/dev/todo/` for items this change touches (Step 2b)
- [x] Create/update PRD documentation
- [x] Create changeset (this document)
- [x] Create failing acceptance tests
- [x] Run acceptance tests (verify they fail) — 3 engine + 3 local failures, each the missing context folding/computation; node 1's 13 inherited red tests also ride this branch and are #553's to green
- [x] USER REVIEW — acceptance tests — waived by the developer ("finish the remaining ones without stopping", 2026-09-27)
- [x] TDD Red — write failing unit/integration tests
- [x] TDD Green — implement with quality code (scoped gates green: `./test -p tddy-discovery
  -p tddy-tool-engine`, 309 passed / 0 failed; scoped clippy clean). One red-phase expectation was
  corrected with ground truth: `the_match_window_counts_matches_not_context_lines` asserted
  `total_matches == 4` where rg reports 3 for its fixture — asserting 4 would have required
  counting a context line as a match, the anti-pattern the test's own name forbids. The two
  argument-bounds unit tests the testing plan called for were written green-side (the red phase
  omitted them).
- [ ] Update documentation with progress
- [ ] Repeat Red→Green→Update cycle until feature complete
- [ ] Run all tests (`./test`) — verify 100% pass
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
- [ ] Linting and formatting (`cargo clippy -- -D warnings`, `cargo fmt`)
- [ ] Wrap documentation (/wrap-context-docs) — when the PR is set ready for review; also deletes `2026-09-27-grep-context-initial-discovery.md`
- [ ] USER REVIEW — work complete, decide next steps
