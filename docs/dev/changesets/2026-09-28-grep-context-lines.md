# 2026-09-28 — Grep returns the lines before and after each match

**Type:** Feature

`#subagent-control` 2/5, [#554](https://github.com/uppin/tddy-coder/pull/554) (base:
`feature/subagent-control/tool-previews`, node 1). No node depends on this one; it takes its line
position for ordering only.

Packages: `tddy-tool-engine` (the rg flags, event folding, catalog schema), `tddy-discovery`
(the `before`/`after` surface end to end: argument reading and bounds, both codebase paths, the
model-facing schema).

## What was delivered

`Grep` accepts optional **`before`/`after`** — each 0–50 lines, either alone or together. Every
match entry then carries a bounded **`context`** list: `{lineNumber, text, relation}` each
(`relation` spelled `before`/`after`), in file order, so the signature-above/body-below question
costs one call instead of a `READ` of the whole file.

- The engine passes `-B`/`-A` to ripgrep and folds its `type:"context"` events into the adjacent
  match entries (`fold_context_events`): a line two matches' windows share attaches to exactly
  one entry — the next match's `before`, else the last match's `after` — never duplicated, never
  folded across files (`begin` resets the window).
- The Local path (`CodebaseAccess::Local`) computes the same windows from the file's own lines
  (`subagent/grep_context.rs`), clamped at file edges — fewer lines than asked, never padding,
  never an error. Both paths answer identically on the wire.
- Without the arguments the result is byte for byte the context-free shape: no `-B`/`-A` out, no
  context events back, no `context` key.
- The window semantics do not move: `truncated`/`total_matches` keep counting **matches**; context
  lines never consume it.
- `GrepContext::from_args` **rejects** a count past `GREP_CONTEXT_LINE_CEILING` (50) rather than
  clamping — the model re-asks within the bound; the engine clamps instead, because its own
  callers are the host. `validate_tool_arguments` faults a past-ceiling or negative count as
  `ArgumentProblem::OutOfRange { allowed: "0 to 50" }`, naming the argument, and the model-facing
  schema carries the bound formally (`minimum: 0, maximum: 50`).
- The two crates cannot import each other, so the engine repeats the ceiling as a local const
  tied to its discovery twin by comment, as with `REMOTE_ENGINE_DEFAULT_BLOCK_MS`.

## Tests

Six acceptance tests pin the shape (engine path: context per match, edge clamping, unchanged
no-context shape, the match-only window; local path: same shapes, clamping, unchanged shape) and
two argument-validation tests pin the bounds. One red-phase expectation was corrected against
ground truth: the match-window test asserted `total_matches == 4` where rg reports **3** for its
fixture — 4 would have required counting a context line as a match, the anti-pattern the test's
own name forbids.

## Code-issue measurements at wrap

- `packages/tddy-discovery/docs/code-issues/oversized-file-subagent.md` — 1,912 production lines
  (was 1,850 at the merge-base), +62 around the planned `grep_with_context` seam; the window
  computation itself went to the new sibling `subagent/grep_context.rs`. Open, unclaimed.
- `packages/tddy-tool-engine/docs/code-issues/oversized-file-lib.md` — 869 (was 789), +80: the
  in-place edit the changeset planned (argv, folding, mirrored ceiling). Open, unclaimed.

Restructuring both is deferred past the `subagent-control` stack — sibling nodes touch both files.

## Backlog

No `docs/dev/todo/` entry resolved: `glob-and-grep-cannot-be-paged` stays open (this PR adds
`before`/`after` precisely so the truncation asymmetry does not deepen — no `offset`), as do the
jail-reachability entry (closed by #549, not here) and the oversized-file records.
