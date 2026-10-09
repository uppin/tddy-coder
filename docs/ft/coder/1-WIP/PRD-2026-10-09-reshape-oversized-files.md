# The engine's production-line count skips test-only items, and its own files fit the 500-line budget - PRD

**Date**: 2026-10-09
**PRD Type**: Bug fix (the measurement) + behaviour-preserving restructure (the engine's own oversized files)
**Stack**: `#reshape` 15/19 — `feature/reshape/oversized-files`

## Affected Features

- **Primary Feature**: [Rust code restructuring](../rust-code-restructuring.md) — the `check` row of the command table
  (what `--budget LINES` counts), a new `lines` row (print the production lines of files), and `## What a run executes`
  where it describes the budget report.

No other feature document changes. No plan field, operation or wire message changes. Splitting the engine's files changes
no behaviour.

## Summary

The engine's production-line count leaves out each test-only item wherever it sits in the file, instead of stopping at
the first `#[cfg(test)]` that looks like the test module. `check --budget` and a new `restructure lines` command report
that count, and the `/pr-wrap` file-length gate uses the command instead of its own `awk`. With the true count, three
engine files besides `backends/rust.rs` are over 500 lines. All three are split by `tddy-tools restructure` plans, along
their responsibilities.

## Background

The repository holds every non-test source file to 500 production lines. Two counters measure that, and both read
"production" as "everything before some `#[cfg(test)]`":

- `check --budget` stops at the first `#[cfg(test)]` followed by a `mod` line. An extracted test module declared out of
  line (`#[cfg(test)] mod wide_facade_tests;`) counts too. So `runner/tidy.rs` measures **36** lines when it has **540**.
- The `/pr-wrap` gate stops at the first `#[cfg(…test…)]` of any kind, including a `#[cfg(test)] use` and a
  `#[cfg(not(test))]`. `tddy-session-lifecycle/src/connection_service.rs` measured 43 for about 1,570, and five engine
  files are under-read by over 300 lines each.

Both fail silently. A file that puts test-only code near its top, which is good practice, stops being measured. The
backlog's list of oversized engine files (three) was taken with the first counter and missed `runner/tidy.rs`.

The three files over budget are each more than one idea in one module. `crate_move/test_binary.rs` (967) holds a test
binary move, a lexical masker that four other modules share, a reader of what a file names and binds, a facade walk and a
dev-dependency writer. `runner/tidy.rs` (540) holds the tidy's entry point and the rounds that remove, redo and undo
import removals. `backends/rust/item_move/assemble.rs` (507) holds the assembly of a move together with the visibility
decisions and the type every sibling module takes, and two module cycles run through it. The later split of
`tddy-code-restructuring` into engine crates needs these seams first. One example: the Rust backend reaches into
`crate_move` only for the lexical masker.

## Proposed Changes

### What's Changing

- **What counts as production.** A Rust file's production lines are all of its lines except those of a test-only
  item: an item marked `#[cfg(test)]` or `#[cfg(all(test, …))]`, from its first doc comment or attribute through its
  last line. This covers an out-of-line declaration (`mod x_tests;`), a `use`, a function, an inline module and a member
  of an `impl`, wherever each sits. Braces and semicolons inside strings, characters and comments do not end an item.
  `#[cfg(not(test))]`, `#[cfg(any(test, …))]` and `cfg_attr` count as production: they also build outside tests. A file
  in any other language counts every line, as today.
- **`check --budget LINES`** reports that count. The report's format is unchanged.
- **`tddy-tools restructure lines <file>...`** prints each file's production lines as `<lines>\t<path>`. It writes
  nothing, needs no language server or daemon, and refuses an unreadable file by name.
- **The `/pr-wrap` gate** measures with `restructure lines`, for the working tree and for the merge-base blob, instead of
  its `awk`. A missing `tddy-tools` aborts the gate loudly. The deferred-work record format and the code-restructuring
  skill state the same rule.
- **The engine's oversized files are split.** The list is re-taken with the corrected count at this node's base, and
  covers every production file in `packages/tddy-code-restructuring/src` except `backends/rust.rs` (node 17). Each file
  over 500 is split along its responsibilities (today's sketch):
  - `crate_move/test_binary.rs` → the lexical masker gets its own crate-level module, used by `test_binary`,
    `source_scan` and `early_return`; the reader of names and bindings gets a module of its own under `crate_move`; the
    facade walk becomes a child of `test_binary`; the dev-dependency writer joins `crate_move/manifest_edits.rs`.
    About 394 lines remain: the operation itself.
  - `runner/tidy.rs` → the import rounds (report, round, repair, undo, restore) become `runner/tidy/rounds.rs`. Choosing
    and applying rustc's fixes becomes `runner/tidy/fixes.rs`. About 196 lines remain: the entry, the check and its
    reports.
  - `backends/rust/item_move/assemble.rs` → `Moving` becomes `item_move/moving.rs` and the visibility decisions become
    `item_move/visibility.rs`, matching `module_reparent/visibility.rs`. About 356 lines remain: the assembly.
- **Moves are engine moves.** Every split is a `tddy-tools restructure` plan proved by `check --deep`, then applied. A
  hand edit is made only to fix the build after an engine move, and each one gets a backlog entry. If the engine refuses
  a move, work stops and the developer decides.
- **The result is pinned.** A shape test fails when any production file of the crate is over 500 lines, except
  `backends/rust.rs` until node 17. A second shape test fails when any module edge listed under must-not reappears.

### What's Staying the Same

- Every operation's behaviour, every refusal, the plan format, the CLI output of every existing command.
- The masker stays **one** definition of "code" for every pass that uses it. It is moved, never copied.
- `backends/rust.rs` (node 17) and the function-size lists (nodes 16 and 19). Moved functions keep their bodies and
  lengths.
- The other crates' own shape-test counters.

## Impact Analysis

### Technical Impact

- `tddy-code-restructuring`: `runner/budget.rs` (the count), a test-only-item scan beside `cfg_test_attribute` in
  `crate_move/source_scan.rs`, the `lines` subcommand (`restructure_args.rs`, `runner/options.rs`, an entry point), and
  the three splits, with their re-pointed callers.
- `tddy-tools` (`index_client.rs`) and `tddy-index-daemon` (`cli.rs`): one routing arm each for the new subcommand,
  answered without an index. The developer consented to this on 2026-10-09 (changeset F3; F1–F7 decided, recommendations taken).
- `.agents/commands/pr-wrap.md`, `.agents/skills/code-restructuring/SKILL.md`,
  `.agents/skills/deferred-work/references/code-issue-record.md`.
- Every code-issue record measured with the old rules may now read higher. Records are corrected only when touched.

### User Impact

- Files that hid behind a test-only line near their top are reported by `/pr-wrap` and `check --budget`, and a PR that
  grows one is told to decompose it.
- An agent restructuring the engine meets smaller files and no module cycle through `item_move/assemble.rs`.

## Implementation Plan

1. The test-only-item scan and the new count, at unit level.
2. `restructure lines` and its routing; the `/pr-wrap` gate and the two docs switch to it.
3. Re-measure at the base; write and prove the split plans with `check --deep`.
4. Apply plan by plan (one commit each): `test_binary.rs`, then `assemble.rs`, then `tidy.rs`.
5. The two shape tests go green; baseline re-run by failing-test name; `verify --against` the pre-plan ref.
6. Docs at wrap: feature doc, package docs, code-issue record and backlog entries closed or narrowed.

## Acceptance Criteria

- [ ] An out-of-line `#[cfg(test)] mod x_tests;` near the top of a file does not end the count. The lines after it count
      ([Rust code restructuring](../rust-code-restructuring.md))
- [ ] Every `#[cfg(test)]` / `#[cfg(all(test, …))]` item is left out wherever it sits, with its docs and attributes,
      including a member of a production `impl`. A brace in a string or comment of a test item does not end it early
- [ ] `#[cfg(not(test))]` and `#[cfg(any(test, …))]` items count as production
- [ ] `check --budget 500` on a plan naming `runner/tidy.rs` (before its split) reports it at 540
- [ ] `tddy-tools restructure lines <files>` prints each file's count and writes nothing; an unreadable file is refused by name
- [ ] The `/pr-wrap` gate measures with `restructure lines` and aborts loudly when it cannot run it
- [ ] Every production file in `packages/tddy-code-restructuring/src` except `backends/rust.rs` is at most 500
      production lines, pinned by a test
- [ ] The must-not module edges are absent, pinned by a test: the masker depends on nothing in `crate_move`, `backends`
      or `runner`; `early_return` does not reach `crate_move`; nothing outside `test_binary` imports from it;
      `item_move`'s siblings do not import from `assemble`
- [ ] Every split was applied by `tddy-tools restructure`. Each hand fix after a move has a backlog entry.
      `restructure verify --against <pre-plan ref>` holds. Comment lines are unchanged as a multiset
- [ ] Tests pass for `tddy-code-restructuring` (scoped; the failing set equals the baseline's by name; CI for the rest)

## References

### Affected Features (Complete List)

- [Rust code restructuring](../rust-code-restructuring.md)

### Related Documentation

- Changeset: `docs/dev/1-WIP/2026-10-09-reshape-oversized-files.md`
- Discovery: `docs/dev/1-WIP/2026-10-09-reshape-oversized-files-initial-discovery.md`
- Backlog entries this resolves:
  [the file-length gate stops at the first cfg(test) use](../../../dev/todo/2026-09-19-the-file-length-gate-stops-at-the-first-cfg-test-use.md),
  [test_binary.rs is 950 production lines](../../../dev/todo/2026-09-19-test-binary-rs-is-950-production-lines.md),
  [item_move/assemble.rs past 500](../../../dev/todo/2026-10-06-restructure-item-move-assemble-past-500.md),
  code issue [oversized-file-test-binary](../../../../packages/tddy-code-restructuring/docs/code-issues/oversized-file-test-binary.md),
  [leftovers of the live-plan carve](../../../dev/todo/2026-10-03-restructure-leftovers-of-the-live-plan-carve-and-tooling-pass.md) (item 3)
- How a split plan is written: `.agents/skills/code-restructuring/SKILL.md`, `references/restructure-changeset.md`
